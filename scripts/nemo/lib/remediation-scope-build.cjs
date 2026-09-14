'use strict';

const { execFileSync } = require('node:child_process');
const census = require('./remediation-scope-census.cjs');
const { SCHEMA, EXCLUSIONS, digest, verifyScope } = require('./remediation-scope-verify.cjs');

const git = (root, args, binary = false) => execFileSync('git', args, { cwd: root, encoding: binary ? 'buffer' : 'utf8' });

function trackedSource(root, commit) {
  if (!/^[0-9a-f]{40}$/.test(commit)) throw new Error('source commit must be a full Git SHA');
  const output = git(root, ['ls-tree', '-rz', '--full-tree', commit]);
  return output.split('\0').filter(Boolean).map((record) => {
    const match = /^(\d{6}) (\w+) ([0-9a-f]{40})\t(.+)$/.exec(record);
    if (!match || match[2] !== 'blob') throw new Error('unsupported source tree entry');
    return { mode: match[1], blob: match[3], path: match[4] };
  }).sort((a, b) => a.path < b.path ? -1 : a.path > b.path ? 1 : 0);
}

function readAt(root, commit, file) {
  try { return git(root, ['show', `${commit}:${file}`]); } catch { return null; }
}

function loadCensuses(root, commit, source) {
  return source.filter((entry) => census.CENSUS_PATH.test(entry.path))
    .map((entry) => ({ ...entry, document: JSON.parse(readAt(root, commit, entry.path)) }));
}

// Line-mapping tools bound to one frozen tree; diff output is memoised per (path, commit).
function coverageTools(root, commit) {
  const cache = new Map();
  return {
    lines(file) {
      const body = readAt(root, commit, file);
      return body === null ? null : body.replace(/\n$/, '').split('\n');
    },
    hunks(file, fromCommit) {
      if (!fromCommit || fromCommit === commit) return [];
      const key = `${fromCommit}:${file}`;
      if (!cache.has(key)) {
        cache.set(key, census.parseHunks(git(root, ['diff', '--unified=0', fromCommit, commit, '--', file])));
      }
      return cache.get(key);
    },
  };
}

function expectedCoverage(root, commit, censuses) {
  return census.rangeCoverage(census.declarations(censuses), coverageTools(root, commit));
}

function inspect(root, scope, sourceCommit = scope.source?.commit) {
  const source = trackedSource(root, sourceCommit);
  const frozen = scope.source?.commit;
  const pins = frozen === sourceCommit ? source : trackedSource(root, frozen);
  const censuses = loadCensuses(root, frozen, pins);
  const amendments = scope.amendments || [];
  const last = amendments[amendments.length - 1];
  const previous = last?.from ? trackedSource(root, last.from) : undefined;
  return verifyScope(scope, source, censuses, { previous, rangeCoverage: expectedCoverage(root, frozen, censuses) });
}

const REASONS = {
  'handwritten-runtime': 'Tracked handwritten runtime/presentation/shader source; inclusion does not establish a complete responsibility map.',
  'handwritten-tooling': 'Tracked executable tool; requires explicit boundary/admission coverage.',
  test: 'Test code or fixture supporting named consumer checks; not proof that those checks passed.',
  configuration: 'Tracked configuration, schema, lock or metadata consumer; retained explicitly rather than excluded as documentation.',
  documentation: 'Tracked prose/license reference; not executable runtime or tooling.',
  'static-data': 'Recorded evidence or census data; identity frozen with the source set, no handwritten control logic.',
};

// Only paths new since the previous freeze are classified here; existing entries keep
// their reviewed classification. Anything outside these shapes must be classified by hand.
function classifyNew(file) {
  if (/(^|\/)[^/]+\.test\.cjs$/.test(file) || /^tests\//.test(file)) return 'test';
  if (/\.(min|vendor)\.[cm]?js$/.test(file) || /^src\/(wasm|wasm-vectorize)\//.test(file) || /^src-tauri\/(gen|binaries)\//.test(file)) {
    throw new Error(`vendor/generated path needs a reviewed classification: ${file}`);
  }
  if (/^src\/.*\.[cm]?js$/.test(file) || /^(src-tauri|nemo-mcp|geometry-wasm|vectorize-core|vectorize-wasm)\/src\/.*\.rs$/.test(file)) return 'handwritten-runtime';
  if (/^scripts\/.*\.cjs$/.test(file)) return 'handwritten-tooling';
  if (/^engineering\/inventory\/.*\.json$/.test(file)) return 'static-data';
  if (/\.(schema\.json|json|toml|yml|yaml)$/.test(file)) return 'configuration';
  if (/\.md$/.test(file)) return 'documentation';
  throw new Error(`no classification rule for new path: ${file}`);
}

// Rebuild the index at `commit`, keeping every reviewed field from `previous` (classification,
// reason, coverage, packet admissions, gap dispositions, span dispositions) and recomputing
// the mechanical ones (source identities, pins, packets, census references, range coverage).
function refreeze(root, previous, { commit, id, issue, reason }) {
  const source = trackedSource(root, commit);
  const before = new Map(trackedSource(root, previous.source.commit).map((entry) => [entry.path, entry]));
  const censuses = loadCensuses(root, commit, source);
  const rows = census.declarations(censuses);
  const oldFiles = new Map(previous.files.map((entry) => [entry.path, entry]));
  // Reviewed references survive; censuses pinned for the first time or re-pinned with a
  // different blob add their mechanical references.
  const newCensusIds = new Set(censuses.filter((entry) => !(previous.censuses || [])
    .some((pin) => pin.id === entry.document.census_id && pin.blob === entry.blob))
    .map((entry) => entry.document.census_id));
  const mechanicalRefs = (file, old) => {
    const refs = new Set(old?.censusRefs || []);
    for (const row of rows) if (row.path === file && (!old || newCensusIds.has(row.census))) refs.add(row.packet);
    return [...refs].sort();
  };
  const oldPackets = new Map(previous.packets.map((entry) => [entry.id, entry]));
  const oldGaps = new Map(previous.uncoveredResponsibilities.map((entry) => [entry.id, entry]));
  const oldSpans = new Map((previous.rangeCoverage || []).map((entry) => [entry.path, entry.dispositions || []]));
  const counts = {};
  const files = source.map((entry) => {
    const old = oldFiles.get(entry.path);
    const classification = old?.classification || classifyNew(entry.path);
    counts[classification] = (counts[classification] || 0) + 1;
    const record = { path: entry.path, mode: entry.mode, blob: entry.blob, classification,
      censusRefs: mechanicalRefs(entry.path, old), reason: old?.reason || REASONS[classification] };
    if (old?.coverage && !record.censusRefs.length && !EXCLUSIONS.has(classification)) record.coverage = old.coverage;
    return record;
  });
  const packets = [];
  for (const { document } of censuses) {
    for (const packet of census.censusPackets(document)) {
      packets.push(oldPackets.get(packet.id) || { id: packet.id, census: document.census_id, admission: 'pending' });
    }
  }
  const gaps = [];
  for (const { document } of censuses) {
    (document.uncovered_responsibilities || []).forEach((note, index) => {
      const gapId = `${document.census_id}:${index + 1}`;
      const old = oldGaps.get(gapId);
      gaps.push(old && old.note === note ? old : { id: gapId, note, disposition: 'needs-reconciliation' });
    });
  }
  const rangeCoverage = expectedCoverage(root, commit, censuses).map((entry) => {
    const kept = (oldSpans.get(entry.path) || []).filter((d) => entry.spans.some((span) => String(span) === String(d.span)));
    return kept.length ? { ...entry, dispositions: kept } : entry;
  });
  const added = source.filter((entry) => !before.has(entry.path)).map((entry) => entry.path);
  const removed = [...before.keys()].filter((name) => !source.some((entry) => entry.path === name)).sort();
  const modified = source.filter((entry) => before.has(entry.path) && (before.get(entry.path).blob !== entry.blob
    || before.get(entry.path).mode !== entry.mode)).map((entry) => entry.path);
  const amendment = { id, issue, commit, from: previous.source.commit, fileCount: source.length,
    sha256: digest(source), added, removed, modified, reason };
  // A schema-1 index (P03A/#1116) recorded only its own freeze; it becomes the first amendment.
  const history = previous.amendments || [{ id: 'P03A', issue: 1116, commit: previous.source.commit,
    fileCount: previous.source.fileCount, sha256: previous.source.sha256, reason: 'Initial frozen source index (schema 1).' }];
  return {
    schema: SCHEMA, issue: `https://github.com/mysteropodes/nemo/issues/${issue}`, complete: false,
    source: { commit, fileCount: source.length, sha256: digest(source) },
    amendments: [...history, amendment], counts: Object.fromEntries(Object.entries(counts).sort()),
    censuses: censuses.map((entry) => ({ id: entry.document.census_id, path: entry.path, blob: entry.blob,
      sourceCommit: entry.document.source_sha })),
    packets, uncoveredResponsibilities: gaps, rangeCoverage, files,
  };
}

module.exports = { trackedSource, readAt, loadCensuses, coverageTools, expectedCoverage, inspect, classifyNew, refreeze };
