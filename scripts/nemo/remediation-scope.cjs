'use strict';

const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { execFileSync } = require('node:child_process');

const INDEX = 'engineering/inventory/remediation-scope.json';
const CLASSIFICATIONS = new Set([
  'handwritten-runtime', 'handwritten-tooling', 'test', 'configuration',
  'documentation', 'vendor', 'generated', 'static-data',
]);
const EXCLUSIONS = new Set(['documentation', 'vendor', 'generated', 'static-data']);

function digest(entries) {
  const rows = entries.map(({ mode, blob, path: name }) => `${mode} ${blob} ${name}\n`);
  return crypto.createHash('sha256').update(rows.join('')).digest('hex');
}

function trackedSource(root, commit) {
  if (!/^[0-9a-f]{40}$/.test(commit)) throw new Error('source commit must be a full Git SHA');
  const output = execFileSync('git', ['ls-tree', '-rz', '--full-tree', commit], { cwd: root });
  return output.toString('utf8').split('\0').filter(Boolean).map((record) => {
    const match = /^(\d{6}) (\w+) ([0-9a-f]{40})\t(.+)$/.exec(record);
    if (!match || match[2] !== 'blob') throw new Error('unsupported source tree entry');
    return { mode: match[1], blob: match[3], path: match[4] };
  }).sort((a, b) => a.path < b.path ? -1 : a.path > b.path ? 1 : 0);
}

function verifyScope(scope, source, censuses) {
  const errors = [];
  const check = (ok, message) => { if (!ok) errors.push(message); };
  check(scope.schema === 'nemo.remediation-scope/1', 'unsupported scope schema');
  check(scope.complete === false, 'completeness must remain false until P03 admission is implemented');
  check(scope.source?.fileCount === source.length, 'source count mismatch');
  check(scope.source?.sha256 === digest(source), 'source digest mismatch');
  const actual = new Map(source.map((entry) => [entry.path, entry]));
  const packets = new Map();
  const censusIds = new Set();
  const expectedGaps = new Map();
  for (const census of censuses) {
    const { document, path: filename, blob } = census;
    const id = document.census_id;
    check(!censusIds.has(id), `duplicate census ${id}`);
    censusIds.add(id);
    const pin = scope.censuses?.find((entry) => entry.id === id);
    check(pin?.path === filename && pin?.blob === blob
      && pin?.sourceCommit === document.source_sha, `census pin mismatch: ${id}`);
    for (const area of document.areas || []) {
      // Supplements replace C08's preliminary packets; its documentation map remains.
      if (id === 'C08' && area.area !== 'documentation-and-process') continue;
      for (const packet of area.packets || []) {
        check(!packets.has(packet.id), `duplicate packet ${packet.id}`);
        packets.set(packet.id, id);
      }
    }
    for (const [index, note] of (document.uncovered_responsibilities || []).entries()) {
      expectedGaps.set(`${id}:${index + 1}`, note);
    }
  }
  check(scope.censuses?.length === censusIds.size, 'census set mismatch');
  const seen = new Set();
  const counts = {};
  let previous = '';
  const unmapped = [];
  for (const entry of scope.files || []) {
    check(typeof entry.path === 'string' && entry.path > previous, `paths not unique/sorted: ${entry.path}`);
    previous = entry.path;
    check(!seen.has(entry.path), `duplicate source path: ${entry.path}`);
    seen.add(entry.path);
    const original = actual.get(entry.path);
    check(original?.blob === entry.blob && original?.mode === entry.mode, `source identity mismatch: ${entry.path}`);
    check(CLASSIFICATIONS.has(entry.classification), `invalid classification: ${entry.path}`);
    check(typeof entry.reason === 'string' && entry.reason.trim().length > 0, `missing classification reason: ${entry.path}`);
    check(Array.isArray(entry.censusRefs), `missing census references: ${entry.path}`);
    for (const ref of entry.censusRefs || []) check(packets.has(ref), `unknown census packet: ${ref}`);
    if (!EXCLUSIONS.has(entry.classification) && !entry.censusRefs?.length) unmapped.push(entry.path);
    counts[entry.classification] = (counts[entry.classification] || 0) + 1;
  }
  for (const name of actual.keys()) check(seen.has(name), `missing source path: ${name}`);
  check(seen.size === actual.size, 'indexed source count mismatch');
  check(JSON.stringify(Object.entries(counts).sort()) === JSON.stringify(Object.entries(scope.counts || {}).sort()), 'classification counts mismatch');
  const listedPackets = scope.packets || [];
  check(listedPackets.length === packets.size, 'packet count mismatch');
  const packetIds = new Set();
  for (const packet of listedPackets) {
    check(!packetIds.has(packet.id), `duplicate indexed packet: ${packet.id}`);
    packetIds.add(packet.id);
    check(packets.get(packet.id) === packet.census, `packet provenance mismatch: ${packet.id}`);
    check(packet.admission === 'pending', `unsupported admission claim: ${packet.id}`);
  }
  const gaps = scope.uncoveredResponsibilities || [];
  check(gaps.length === expectedGaps.size, 'uncovered responsibility count mismatch');
  const gapIds = new Set();
  for (const gap of gaps) {
    check(!gapIds.has(gap.id), `duplicate uncovered responsibility: ${gap.id}`);
    gapIds.add(gap.id);
    check(expectedGaps.get(gap.id) === gap.note, `uncovered responsibility changed: ${gap.id}`);
    check(gap.disposition === 'needs-reconciliation', `unsupported gap disposition: ${gap.id}`);
  }
  return {
    integrity: errors.length ? 'fail' : 'pass', complete: false, errors,
    sourceFiles: source.length, indexedFiles: seen.size, counts,
    pendingPackets: packets.size, unreconciledResponsibilities: expectedGaps.size, unmapped,
  };
}

function inspect(root, scope, sourceCommit = scope.source?.commit) {
  const source = trackedSource(root, sourceCommit);
  const pins = trackedSource(root, scope.source?.commit);
  const censuses = pins.filter((entry) => /^engineering\/inventory\/census\/[^/]+\.json$/.test(entry.path))
    .map((entry) => ({ ...entry, document: JSON.parse(execFileSync('git',
      ['show', `${scope.source.commit}:${entry.path}`], { cwd: root, encoding: 'utf8' })) }));
  return verifyScope(scope, source, censuses);
}

function main(args = process.argv.slice(2), root = path.resolve(__dirname, '../..')) {
  try {
    const integrityOnly = args.includes('--integrity-only');
    const sourceIndex = args.indexOf('--source');
    const sourceCommit = sourceIndex < 0 ? undefined : args[sourceIndex + 1];
    const allowed = integrityOnly ? ['--integrity-only'] : [];
    if (sourceIndex >= 0) allowed.push('--source', sourceCommit);
    if (args.length !== allowed.length || args.some((arg) => !allowed.includes(arg))) {
      throw new Error('usage: remediation-scope.cjs [--integrity-only] [--source FULL_SHA]');
    }
    const scope = JSON.parse(fs.readFileSync(path.join(root, INDEX), 'utf8'));
    const result = inspect(root, scope, sourceCommit);
    process.stdout.write(`${JSON.stringify(result, null, 2)}\n`);
    return result.integrity === 'pass' && (integrityOnly || result.complete) ? 0 : 1;
  } catch (error) {
    process.stderr.write(`remediation scope check failed: ${error.message}\n`);
    return 1;
  }
}

if (require.main === module) process.exitCode = main();
module.exports = { digest, trackedSource, verifyScope, inspect, main };
