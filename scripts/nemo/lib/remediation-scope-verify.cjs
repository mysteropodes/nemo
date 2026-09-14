'use strict';

const crypto = require('node:crypto');
const { CENSUS_PATH, censusPackets } = require('./remediation-scope-census.cjs');

const SCHEMA = 'nemo.remediation-scope/2';
const CLASSIFICATIONS = new Set([
  'handwritten-runtime', 'handwritten-tooling', 'test', 'configuration',
  'documentation', 'vendor', 'generated', 'static-data',
]);
const EXCLUSIONS = new Set(['documentation', 'vendor', 'generated', 'static-data']);
const ADMISSIONS = new Set(['pending', 'covered', 'admitted', 'deferred']);
const GAP_DISPOSITIONS = new Set(['needs-reconciliation', 'covered-by-packet', 'covered-by-leaf',
  'resolved', 'boundary', 'human-decision']);
const COVERAGE_KINDS = new Set(['leaf', 'oracle', 'deferred']);
const SPAN_KINDS = new Set(['admitted', 'covered', 'boundary']);
const LEAF_ID = /^[A-Z]\d{2}[A-Za-z0-9]*$/;
const SHA = /^[0-9a-f]{40}$/;

function digest(entries) {
  const rows = entries.map(({ mode, blob, path: name }) => `${mode} ${blob} ${name}\n`);
  return crypto.createHash('sha256').update(rows.join('')).digest('hex');
}

const text = (value) => typeof value === 'string' && value.trim().length > 0;
const count = (value) => Number.isInteger(value) && value > 0;
const sortedUnique = (list) => Array.isArray(list)
  && list.every((item, i) => typeof item === 'string' && (i === 0 || item > list[i - 1]));

// Evidence shape for anything that claims a delivered leaf: leaf id, issue and merged PR.
function leafEvidence(entry) {
  return LEAF_ID.test(entry.leaf || '') && count(entry.issue) && count(entry.pullRequest) && text(entry.evidence);
}

function checkAmendments(scope, source, check, expected) {
  const amendments = scope.amendments || [];
  check(amendments.length > 0, 'missing amendment history');
  amendments.forEach((entry, index) => {
    check(text(entry.id) && count(entry.issue) && SHA.test(entry.commit || '') && count(entry.fileCount)
      && /^[0-9a-f]{64}$/.test(entry.sha256 || '') && text(entry.reason), `malformed amendment: ${entry.id}`);
    if (index === 0) return;
    check(entry.from === amendments[index - 1].commit, `amendment chain broken: ${entry.id}`);
    check(sortedUnique(entry.added) && sortedUnique(entry.removed) && sortedUnique(entry.modified),
      `amendment delta not sorted/unique: ${entry.id}`);
  });
  const last = amendments[amendments.length - 1] || {};
  check(last.commit === scope.source?.commit && last.fileCount === source.length
    && last.sha256 === digest(source), 'last amendment does not describe the frozen source');
  if (!expected.previous || amendments.length < 2) return;
  const before = new Map(expected.previous.map((entry) => [entry.path, entry]));
  const after = new Map(source.map((entry) => [entry.path, entry]));
  const added = source.filter((entry) => !before.has(entry.path)).map((entry) => entry.path);
  const removed = expected.previous.filter((entry) => !after.has(entry.path)).map((entry) => entry.path);
  const modified = source.filter((entry) => before.has(entry.path)
    && (before.get(entry.path).blob !== entry.blob || before.get(entry.path).mode !== entry.mode)).map((entry) => entry.path);
  check(JSON.stringify([added, removed, modified]) === JSON.stringify([last.added, last.removed, last.modified]),
    'last amendment delta does not match the two Git trees');
}

function checkPackets(scope, packets, check) {
  const listed = scope.packets || [];
  check(listed.length === packets.size, 'packet count mismatch');
  const ids = new Set();
  const states = { pending: 0, covered: 0, admitted: 0, deferred: 0 };
  for (const packet of listed) {
    check(!ids.has(packet.id), `duplicate indexed packet: ${packet.id}`);
    ids.add(packet.id);
    check(packets.get(packet.id) === packet.census, `packet provenance mismatch: ${packet.id}`);
    if (!ADMISSIONS.has(packet.admission)) { check(false, `unsupported admission claim: ${packet.id}`); continue; }
    states[packet.admission] += 1;
    if (packet.admission === 'covered') check(leafEvidence(packet), `covered packet lacks leaf evidence: ${packet.id}`);
    if (packet.admission === 'admitted') check(count(packet.issue), `admitted packet lacks an issue: ${packet.id}`);
    if (packet.admission === 'deferred') check(text(packet.reason), `deferred packet lacks a reason: ${packet.id}`);
  }
  return states;
}

function checkGaps(scope, expectedGaps, packets, check) {
  const gaps = scope.uncoveredResponsibilities || [];
  check(gaps.length === expectedGaps.size, 'uncovered responsibility count mismatch');
  const ids = new Set();
  let unreconciled = 0;
  for (const gap of gaps) {
    check(!ids.has(gap.id), `duplicate uncovered responsibility: ${gap.id}`);
    ids.add(gap.id);
    check(expectedGaps.get(gap.id) === gap.note, `uncovered responsibility changed: ${gap.id}`);
    if (!GAP_DISPOSITIONS.has(gap.disposition)) { check(false, `unsupported gap disposition: ${gap.id}`); continue; }
    if (gap.disposition === 'needs-reconciliation') { unreconciled += 1; continue; }
    check(text(gap.evidence), `gap disposition lacks evidence: ${gap.id}`);
    if (gap.disposition === 'covered-by-packet') {
      check(Array.isArray(gap.packets) && gap.packets.length > 0
        && gap.packets.every((id) => packets.has(id)), `gap cites unknown packets: ${gap.id}`);
    }
    if (gap.disposition === 'covered-by-leaf') check(leafEvidence(gap), `gap lacks leaf evidence: ${gap.id}`);
  }
  return unreconciled;
}

function checkCoverage(entry, packets, check) {
  const coverage = entry.coverage;
  if (!coverage) return false;
  check(COVERAGE_KINDS.has(coverage.kind), `unsupported coverage kind: ${entry.path}`);
  if (coverage.kind === 'leaf') check(leafEvidence(coverage), `coverage lacks leaf evidence: ${entry.path}`);
  if (coverage.kind === 'oracle') {
    check(Array.isArray(coverage.packets) && coverage.packets.length > 0
      && coverage.packets.every((id) => packets.has(id)), `oracle coverage cites unknown packets: ${entry.path}`);
  }
  if (coverage.kind === 'deferred') check(text(coverage.reason), `deferred coverage lacks a reason: ${entry.path}`);
  return true;
}

function checkRangeCoverage(scope, expected, check) {
  const listed = scope.rangeCoverage || [];
  const strip = (entry) => JSON.stringify({ path: entry.path, lines: entry.lines,
    uncovered: entry.uncovered, overlap: entry.overlap, spans: entry.spans });
  check(JSON.stringify(listed.map(strip)) === JSON.stringify((expected || []).map(strip)),
    'range coverage does not match the census declarations at the frozen source');
  let undispositioned = 0;
  const summary = [];
  for (const entry of listed) {
    const spans = new Set((entry.spans || []).map(String));
    const seen = new Set();
    for (const disposition of entry.dispositions || []) {
      const key = String(disposition.span);
      check(spans.has(key) && !seen.has(key), `span disposition does not name one uncovered span once: ${entry.path}`);
      seen.add(key);
      check(SPAN_KINDS.has(disposition.kind) && text(disposition.evidence), `malformed span disposition: ${entry.path}`);
      if (disposition.kind === 'admitted') check(count(disposition.issue), `admitted span lacks an issue: ${entry.path}`);
    }
    const open = spans.size - seen.size;
    undispositioned += open;
    summary.push({ path: entry.path, lines: entry.lines, uncovered: entry.uncovered, spans: spans.size, open });
  }
  return { undispositioned, summary };
}

// expected: { previous?: source entries of the prior freeze, rangeCoverage?: computed report }
function verifyScope(scope, source, censuses, expected = {}) {
  const errors = [];
  const check = (ok, message) => { if (!ok) errors.push(message); };
  check(scope.schema === SCHEMA, 'unsupported scope schema');
  check(scope.source?.fileCount === source.length, 'source count mismatch');
  check(scope.source?.sha256 === digest(source), 'source digest mismatch');
  checkAmendments(scope, source, check, expected);
  const actual = new Map(source.map((entry) => [entry.path, entry]));
  const packets = new Map();
  const censusIds = new Set();
  const expectedGaps = new Map();
  const pinned = new Set();
  for (const census of censuses) {
    const { document, path: filename, blob } = census;
    const id = document.census_id;
    check(!censusIds.has(id), `duplicate census ${id}`);
    censusIds.add(id);
    const pin = scope.censuses?.find((entry) => entry.id === id);
    check(pin?.path === filename && pin?.blob === blob
      && pin?.sourceCommit === document.source_sha, `census pin mismatch: ${id}`);
    pinned.add(filename);
    for (const packet of censusPackets(document)) {
      check(!packets.has(packet.id), `duplicate packet ${packet.id}`);
      packets.set(packet.id, id);
    }
    for (const [index, note] of (document.uncovered_responsibilities || []).entries()) {
      expectedGaps.set(`${id}:${index + 1}`, note);
    }
  }
  check(scope.censuses?.length === censusIds.size, 'census set mismatch');
  for (const name of actual.keys()) {
    if (CENSUS_PATH.test(name)) check(pinned.has(name), `census file not pinned: ${name}`);
  }
  const seen = new Set();
  const counts = {};
  let previous = '';
  const unmapped = [];
  const dispositioned = [];
  for (const entry of scope.files || []) {
    check(typeof entry.path === 'string' && entry.path > previous, `paths not unique/sorted: ${entry.path}`);
    previous = entry.path;
    check(!seen.has(entry.path), `duplicate source path: ${entry.path}`);
    seen.add(entry.path);
    const original = actual.get(entry.path);
    check(original?.blob === entry.blob && original?.mode === entry.mode, `source identity mismatch: ${entry.path}`);
    check(CLASSIFICATIONS.has(entry.classification), `invalid classification: ${entry.path}`);
    check(text(entry.reason), `missing classification reason: ${entry.path}`);
    check(Array.isArray(entry.censusRefs), `missing census references: ${entry.path}`);
    for (const ref of entry.censusRefs || []) check(packets.has(ref), `unknown census packet: ${ref}`);
    if (!EXCLUSIONS.has(entry.classification) && !entry.censusRefs?.length) {
      (checkCoverage(entry, packets, check) ? dispositioned : unmapped).push(entry.path);
    }
    counts[entry.classification] = (counts[entry.classification] || 0) + 1;
  }
  for (const name of actual.keys()) check(seen.has(name), `missing source path: ${name}`);
  check(seen.size === actual.size, 'indexed source count mismatch');
  check(JSON.stringify(Object.entries(counts).sort()) === JSON.stringify(Object.entries(scope.counts || {}).sort()), 'classification counts mismatch');
  const states = checkPackets(scope, packets, check);
  const unreconciled = checkGaps(scope, expectedGaps, packets, check);
  const ranges = checkRangeCoverage(scope, expected.rangeCoverage, check);
  const remaining = { pendingPackets: states.pending, unreconciledResponsibilities: unreconciled,
    unmappedPaths: unmapped.length, undispositionedSpans: ranges.undispositioned };
  const exhaustive = Object.values(remaining).every((value) => value === 0);
  check(scope.complete === false || (scope.complete === true && exhaustive),
    'completeness claimed while packets, responsibilities, paths or spans remain open');
  return {
    integrity: errors.length ? 'fail' : 'pass', complete: errors.length === 0 && scope.complete === true, errors,
    sourceFiles: source.length, indexedFiles: seen.size, counts, packets: states,
    pendingPackets: states.pending, unreconciledResponsibilities: unreconciled,
    unmapped, dispositionedPaths: dispositioned.length, rangeCoverage: ranges.summary, remaining,
  };
}

module.exports = { SCHEMA, CLASSIFICATIONS, EXCLUSIONS, digest, verifyScope };
