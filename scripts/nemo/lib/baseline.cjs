'use strict';
// P02 — the current-state comparison manifest (schema `nemo.baseline-manifest/1`).
//
// A receipt (`nemo.receipt/1`) records what ONE run observed. It cannot say
// whether a failure is new. This module adds the missing half: it adopts a
// selected current state as the comparison baseline, and classifies a later
// receipt against it.
//
// Two rules drive every decision here:
//
//   1. A result is quoted only together with the bytes it was measured on.
//      Each entry carries its own immutable source/fixture/artifact references
//      plus the sha256 of the receipt it came from. If a reference is missing
//      or no longer matches, the entry is REPORTED as stale — never silently
//      reused as if it still described the current tree.
//   2. `blocked` and `not-run` are never rounded up to `pass`. A native,
//      packaged-desktop or browser check that could not genuinely run keeps
//      its own disposition and its exact reason.
//
// The verdict/severity model and the exact-case helpers live in
// ./baseline-verdicts.cjs and are re-exported here, so `require('./baseline')`
// stays the single entry point for callers.
const path = require('node:path');
const { ROOT, sha256File, exists, readJson, nowIso, fileInfo } = require('./util.cjs');
const identity = require('./identity.cjs');
const verdicts = require('./baseline-verdicts.cjs');
const { VERDICT, BLOCKING, INCONCLUSIVE, REQUIRED_BLOCKING, REQUIRED_INCONCLUSIVE, EXIT,
  severity, exactCase, caseSignature, classify } = verdicts;

const SCHEMA = 'nemo.baseline-manifest/1';
const MANIFEST_PATH = path.join(ROOT, 'engineering', 'inventory', 'baseline-manifest.json');
const FIXTURE_MANIFEST = path.join(ROOT, 'tests', 'fixtures', 'manifest.json');

// ---------------------------------------------------------------------------
// References: the immutable bytes an entry is allowed to be quoted against.
// ---------------------------------------------------------------------------

// `ignoreWorktree` records the committed tree only. It exists for adopting a
// baseline in the same change that introduces it: the manifest must describe
// the commit it ships in, not the working copy that produced it. It is never
// silent — `worktreeIgnored` stays in the manifest, so a reader can see that
// uncommitted changes were present and excluded on purpose.
function currentReferences(opts = {}) {
  const build = identity.buildIdentity();
  return {
    source: (() => {
      const s = identity.sourceIdentity();
      if (opts.ignoreWorktree) {
        return { head: s.head, branch: s.branch, commitDate: s.commitDate, dirty: false, dirtyDigest: null, worktreeIgnored: true, worktreeWasDirty: s.dirty, worktreeDigest: s.dirtyDigest };
      }
      return { head: s.head, branch: s.branch, commitDate: s.commitDate, dirty: s.dirty, dirtyDigest: s.dirtyDigest };
    })(),
    fixtures: exists(FIXTURE_MANIFEST)
      ? { path: path.relative(ROOT, FIXTURE_MANIFEST), present: true, sha256: sha256File(FIXTURE_MANIFEST), formatVersion: readJson(FIXTURE_MANIFEST).formatVersion ?? null }
      : { path: path.relative(ROOT, FIXTURE_MANIFEST), present: false, sha256: null, formatVersion: null },
    artifacts: {
      geometryWasm: build.artifacts.geometryWasm,
      vectorizeWasm: build.artifacts.vectorizeWasm,
      ffmpegSidecar: build.artifacts.ffmpegSidecar,
    },
    build: {
      packageVersion: build.packageVersion,
      tauriVersion: build.tauriVersion,
      indexHtmlTitleVersion: build.indexHtmlTitleVersion,
      hostTriple: build.hostTriple,
    },
  };
}

// Compare a recorded reference block against a current one. Anything that
// cannot be proven identical is returned as a finding; nothing is dropped.
function diffReferences(recorded, current) {
  const findings = [];
  const add = (kind, ref, expected, actual, note) => findings.push({ kind, ref, expected: expected ?? null, actual: actual ?? null, note: note || null });

  if (recorded.source.head !== current.source.head) {
    add('reference-mismatch', 'source.head', recorded.source.head, current.source.head,
      'Results adopted at a different commit do not describe this tree.');
  }
  if (recorded.source.dirty || current.source.dirty) {
    add(recorded.source.dirtyDigest === current.source.dirtyDigest ? 'reference-dirty' : 'reference-mismatch',
      'source.dirtyDigest', recorded.source.dirtyDigest, current.source.dirtyDigest,
      'Uncommitted changes are part of what was measured.');
  }

  if (!current.fixtures.present) add('reference-missing', 'fixtures.manifest', recorded.fixtures.sha256, null, 'Fixture manifest absent from this checkout.');
  else if (recorded.fixtures.sha256 !== current.fixtures.sha256) {
    add('reference-mismatch', 'fixtures.manifest', recorded.fixtures.sha256, current.fixtures.sha256,
      'Fixture corpus changed; fixture-dependent results are not transferable.');
  }

  for (const key of Object.keys(recorded.artifacts || {})) {
    const r = recorded.artifacts[key] || {};
    const c = (current.artifacts || {})[key] || {};
    if (r.present && !c.present) add('reference-missing', 'artifacts.' + key, r.sha256, null, 'Artifact recorded in the baseline is absent here.');
    else if (r.present && c.present && r.sha256 !== c.sha256) add('reference-mismatch', 'artifacts.' + key, r.sha256, c.sha256, 'Artifact bytes differ.');
    else if (!r.present && c.present) add('reference-added', 'artifacts.' + key, null, c.sha256, 'Artifact absent when the baseline was adopted, present now.');
  }
  return findings;
}

// Environment differences do not invalidate a result by themselves, but they
// decide whether a status change is a code regression or a host difference.
function diffEnvironment(recorded, current) {
  const findings = [];
  for (const key of ['os', 'arch', 'node']) {
    if (recorded[key] !== current[key]) findings.push({ kind: 'environment-difference', ref: 'platform.' + key, expected: recorded[key], actual: current[key], note: null });
  }
  return findings;
}

// ---------------------------------------------------------------------------
// Adoption
// ---------------------------------------------------------------------------

function receiptRef(receipt, receiptPath) {
  return {
    runId: receipt.runId,
    command: receipt.command,
    profile: receipt.profile || null,
    finishedAt: receipt.finishedAt || null,
    path: receiptPath ? path.relative(ROOT, receiptPath) : null,
    sha256: receiptPath && exists(receiptPath) ? sha256File(receiptPath) : null,
    source: receipt.source ? receipt.source.head : null,
    sourceDirty: receipt.source ? !!receipt.source.dirty : null,
  };
}

// Build the manifest from one or more receipts. Later receipts win for a job
// they both cover, so a targeted re-run can supersede an entry without
// re-running everything (P02: reuse equivalent prior receipts).
//
// A receipt taken at a different source than the adopted one is NOT rejected —
// it is recorded with `carriedOver: true` and its own mismatch findings, so the
// entry stays usable while never pretending to have been measured here.
function adopt(inputs, opts = {}) {
  const references = opts.references || currentReferences();
  const platform = opts.platform || identity.platformIdentity();
  const entries = new Map();
  const sources = [];

  for (const { receipt, receiptPath } of inputs) {
    if (!receipt || !Array.isArray(receipt.jobs)) continue;
    const ref = receiptRef(receipt, receiptPath);
    sources.push(ref);
    // With `ignoreWorktree` the adopted reference deliberately describes the
    // commit, so a receipt measured on the same commit with uncommitted work
    // present is still "here" — but the entry keeps `measuredDirty` so the
    // reader can see it.
    const refDirty = references.source.worktreeIgnored ? null : !!references.source.dirty;
    const sameSource = !!receipt.source && receipt.source.head === references.source.head
      && (refDirty === null || !!receipt.source.dirty === refDirty);
    for (const job of receipt.jobs) {
      const c = exactCase(job);
      entries.set(job.name, {
        job: job.name,
        required: !!job.required,
        status: job.status,
        case: c,
        caseSignature: job.status === 'pass' ? null : caseSignature(c),
        evidence: ref,
        measuredDirty: !!(receipt.source && receipt.source.dirty),
        carriedOver: !sameSource,
        carriedOverNote: sameSource ? null
          : `Measured at ${(ref.source || 'unknown').slice(0, 12)}${ref.sourceDirty ? ' (dirty)' : ''}, adopted at ${(references.source.head || 'unknown').slice(0, 12)}. Not re-measured here.`,
      });
    }
  }

  const ordered = Array.from(entries.values()).sort((a, b) => a.job.localeCompare(b.job));
  const counts = {};
  for (const e of ordered) counts[e.status] = (counts[e.status] || 0) + 1;

  return {
    schema: SCHEMA,
    adoptedAt: opts.now || nowIso(),
    task: opts.task || 'P02 / #1004',
    note: opts.note || null,
    references,
    platform: { os: platform.os, osRelease: platform.osRelease, arch: platform.arch, cpuModel: platform.cpuModel, cpuCount: platform.cpuCount, node: platform.node },
    receipts: sources,
    entries: ordered,
    summary: {
      total: ordered.length,
      counts,
      carriedOver: ordered.filter((e) => e.carriedOver).map((e) => e.job),
      knownNonPass: ordered.filter((e) => e.status !== 'pass').map((e) => ({ job: e.job, status: e.status, reason: e.case.reason })),
    },
  };
}

// ---------------------------------------------------------------------------
// Comparison
// ---------------------------------------------------------------------------

function compare(manifest, receipt, opts = {}) {
  const current = opts.references || currentReferences();
  const platform = opts.platform || identity.platformIdentity();

  const referenceFindings = diffReferences(manifest.references, current).concat(diffEnvironment(manifest.platform, platform));
  const stale = referenceFindings.some((f) => f.kind === 'reference-mismatch' || f.kind === 'reference-missing');

  const byName = new Map(manifest.entries.map((e) => [e.job, e]));
  const jobs = new Map((receipt.jobs || []).map((j) => [j.name, j]));
  const results = [];

  for (const job of receipt.jobs || []) {
    const entry = byName.get(job.name);
    const { verdict, note } = classify(entry, job);
    // Required in EITHER the baseline or the run. The two can legitimately
    // disagree (a job's `required` flag changed between them); taking the
    // stricter side is the only direction that cannot silently downgrade a
    // case that used to have to answer for itself.
    const required = !!job.required || !!(entry && entry.required);
    results.push({
      job: job.name,
      required,
      baseline: entry ? entry.status : null,
      current: job.status,
      verdict,
      severity: severity(verdict, required),
      note,
      baselineCarriedOver: entry ? !!entry.carriedOver : null,
      baselineEvidence: entry ? entry.evidence : null,
      currentReason: job.reason || null,
    });
  }
  // `expect` is the set of jobs this run CLAIMED to cover. A targeted run of
  // one job has not failed to answer for the other eleven, so a required entry
  // outside that claim is reported as uncovered without blocking. Default null
  // = the run is expected to answer for the whole baseline, which is what a
  // standalone `--check` against a receipt means.
  const expect = opts.expect ? new Set(opts.expect) : null;
  for (const entry of manifest.entries) {
    if (jobs.has(entry.job)) continue;
    const inScope = !expect || expect.has(entry.job);
    results.push({
      job: entry.job, required: !!entry.required, baseline: entry.status, current: null,
      verdict: VERDICT.MISSING_ENTRY,
      // Out of scope is reported but not judged — neither blocking nor
      // inconclusive, or every targeted single-job run would read as unable to
      // answer for the whole baseline.
      severity: inScope ? severity(VERDICT.MISSING_ENTRY, !!entry.required) : 'ok',
      note: inScope
        ? (entry.required ? 'Required baseline entry not covered by this run.' : 'Not covered by this run.')
        : 'Outside this run\'s declared scope; not judged.',
      baselineCarriedOver: !!entry.carriedOver, baselineEvidence: entry.evidence, currentReason: null,
    });
  }
  results.sort((a, b) => a.job.localeCompare(b.job));

  const counts = {};
  for (const r of results) counts[r.verdict] = (counts[r.verdict] || 0) + 1;
  const blocking = results.filter((r) => r.severity === 'blocking');
  const inconclusive = results.filter((r) => r.severity === 'inconclusive');

  let overall = 'ok';
  if (blocking.length) overall = 'blocking';
  else if (stale || inconclusive.length) overall = 'inconclusive';

  return {
    schema: 'nemo.baseline-comparison/1',
    comparedAt: opts.now || nowIso(),
    manifest: { adoptedAt: manifest.adoptedAt, source: manifest.references.source.head, task: manifest.task || null },
    receipt: receiptRef(receipt, opts.receiptPath || null),
    references: { recorded: manifest.references, current, findings: referenceFindings, stale },
    results,
    summary: {
      overall,
      exitCode: EXIT[overall === 'ok' ? 'ok' : overall === 'blocking' ? 'blocking' : 'inconclusive'],
      counts,
      blocking: blocking.map((r) => ({ job: r.job, required: r.required, verdict: r.verdict, note: r.note })),
      inconclusive: inconclusive.map((r) => ({ job: r.job, required: r.required, verdict: r.verdict, note: r.note })),
      // Required jobs this run produced no result for at all. Split out from
      // `inconclusive` because it is the one an operator has to act on: the
      // runtime has to be made available, not the comparison re-read.
      noEvidence: inconclusive.filter((r) => r.required && REQUIRED_INCONCLUSIVE.has(r.verdict))
        .map((r) => ({ job: r.job, status: r.current, reason: r.currentReason || r.note })),
      staleReferences: referenceFindings.filter((f) => f.kind === 'reference-mismatch' || f.kind === 'reference-missing').map((f) => f.ref),
    },
  };
}

function renderComparison(cmp) {
  const lines = [];
  const short = (s) => (s ? String(s).slice(0, 12) : 'n/a');
  lines.push(`baseline ${short(cmp.manifest.source)} (adopted ${cmp.manifest.adoptedAt})  vs  receipt ${cmp.receipt.runId}`);
  lines.push('');
  for (const r of cmp.results) {
    const sev = r.severity && r.severity !== 'ok' ? `[${r.severity}] ` : '';
    lines.push(`  ${String(r.verdict).padEnd(24)} ${(r.required ? '*' : ' ')}${String(r.job).padEnd(18)} ${String(r.baseline ?? '—').padEnd(8)} -> ${String(r.current ?? '—').padEnd(8)} ${sev}${r.note || ''}`);
  }
  if (cmp.summary.noEvidence && cmp.summary.noEvidence.length) {
    lines.push('');
    lines.push('required jobs with no evidence in this run (not a pass):');
    for (const n of cmp.summary.noEvidence) lines.push(`  ${String(n.job).padEnd(18)} ${String(n.status ?? '—').padEnd(8)} ${n.reason || ''}`);
  }
  if (cmp.references.findings.length) {
    lines.push('');
    lines.push('references:');
    for (const f of cmp.references.findings) lines.push(`  ${f.kind.padEnd(22)} ${f.ref} expected ${short(f.expected)} actual ${short(f.actual)}${f.note ? ' — ' + f.note : ''}`);
  }
  lines.push('');
  lines.push(`overall ${cmp.summary.overall.toUpperCase()} (exit ${cmp.summary.exitCode})  ` + Object.entries(cmp.summary.counts).map(([k, v]) => `${k} ${v}`).join(', '));
  return lines.join('\n') + '\n';
}

function loadManifest(file) {
  const f = file || MANIFEST_PATH;
  if (!exists(f)) throw new Error(`no baseline manifest at ${path.relative(ROOT, f)} — adopt one first`);
  const m = readJson(f);
  if (m.schema !== SCHEMA) throw new Error(`unexpected manifest schema "${m.schema}" (want ${SCHEMA})`);
  return m;
}

module.exports = {
  SCHEMA, MANIFEST_PATH, VERDICT, BLOCKING, INCONCLUSIVE, REQUIRED_BLOCKING, REQUIRED_INCONCLUSIVE, EXIT,
  currentReferences, diffReferences, diffEnvironment,
  adopt, classify, severity, compare, caseSignature, exactCase, renderComparison, loadManifest, receiptRef,
};
