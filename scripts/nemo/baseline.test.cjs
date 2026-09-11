'use strict';
// Regressions for the P02 current-state comparison manifest
// (scripts/nemo/lib/baseline.cjs + scripts/nemo/baseline.cjs).
// Included in `npm test` / `verify` through tests/nemo-baseline.test.cjs.
// Run directly:  node --test scripts/nemo/baseline.test.cjs
//
// The property under test is the one P02 exists for: a later run must be
// classifiable as a NEW regression, an UNCHANGED known failure, or an
// environment mismatch — and a result whose immutable references no longer
// match the tree must be reported rather than silently reused.
//
// Everything here is synthetic. Real receipts are host- and clock-dependent,
// so the classification table is exercised against constructed receipts; only
// the CLI round trip touches the filesystem, in a temp directory.
const assert = require('node:assert/strict');
const test = require('node:test');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');

const baseline = require('./lib/baseline.cjs');
const { VERDICT } = baseline;
const ROOT = path.resolve(__dirname, '..', '..');

// --- synthetic fixtures ----------------------------------------------------

const REFS = {
  source: { head: 'a'.repeat(40), branch: 'main', commitDate: '2026-09-09T00:00:00Z', dirty: false, dirtyDigest: null },
  fixtures: { path: 'tests/fixtures/manifest.json', present: true, sha256: 'f'.repeat(64), formatVersion: 13 },
  artifacts: {
    geometryWasm: { path: 'src/wasm/geometry_wasm_bg.wasm', present: true, bytes: 10, sha256: '1'.repeat(64) },
    ffmpegSidecar: { path: 'src-tauri/binaries/ffmpeg-x', present: true, bytes: 20, sha256: '2'.repeat(64) },
  },
  build: { packageVersion: '0.7.0', tauriVersion: '0.7.0', indexHtmlTitleVersion: '0.7.0', hostTriple: 'aarch64-apple-darwin' },
};
const PLATFORM = { os: 'darwin', osRelease: '25.6.0', arch: 'arm64', cpuModel: 'Apple M3 Ultra', cpuCount: 32, node: 'v22.17.1' };

function job(name, status, extra = {}) {
  return Object.assign({ name, required: true, status, reason: null, exitCode: null, artifacts: [], limitations: [], details: null }, extra);
}

function receipt(jobs, over = {}) {
  return Object.assign({
    schema: 'nemo.receipt/1', runId: 'run-' + jobs.map((j) => j.status).join(''), command: 'verify', profile: 'quick',
    startedAt: '2026-09-09T00:00:00Z', finishedAt: '2026-09-09T00:01:00Z',
    source: { head: REFS.source.head, dirty: false }, jobs,
  }, over);
}

function adoptOf(jobs, over = {}) {
  return baseline.adopt([{ receipt: receipt(jobs, over), receiptPath: null }], { references: REFS, platform: PLATFORM, now: '2026-09-09T00:02:00Z' });
}

function compareOf(manifest, jobs, opts = {}) {
  return baseline.compare(manifest, receipt(jobs), Object.assign({ references: REFS, platform: PLATFORM, now: '2026-09-09T00:03:00Z' }, opts));
}

// --- the three classifications P02 names -----------------------------------

test('pass -> fail is a new regression, and it blocks', () => {
  const m = adoptOf([job('test:unit', 'pass')]);
  const c = compareOf(m, [job('test:unit', 'fail', { reason: '3 tests failed' })]);
  assert.equal(c.results[0].verdict, VERDICT.NEW_REGRESSION);
  assert.equal(c.summary.overall, 'blocking');
  assert.equal(c.summary.exitCode, 1);
});

test('the same failing case twice is an unchanged known failure, and does not block', () => {
  const failing = job('test:rust', 'fail', { reason: 'indexed_random_seek_cost_is_flat_and_small failed at p95 36.2 ms' });
  const m = adoptOf([failing]);
  const c = compareOf(m, [failing]);
  assert.equal(c.results[0].verdict, VERDICT.UNCHANGED_KNOWN_FAILURE);
  assert.equal(c.summary.overall, 'ok');
  assert.equal(c.summary.exitCode, 0);
});

test('a different failure in the same job does not inherit the known-failure acceptance', () => {
  const m = adoptOf([job('test:rust', 'fail', { reason: 'seek timing assertion' })]);
  const c = compareOf(m, [job('test:rust', 'fail', { reason: 'segmentation fault in decoder' })]);
  assert.equal(c.results[0].verdict, VERDICT.CHANGED_KNOWN_FAILURE);
  assert.equal(c.summary.overall, 'blocking');
});

test('same reason but different details is still a changed failure', () => {
  const m = adoptOf([job('test:coverage', 'fail', { reason: 'coverage below threshold', details: { file: 'a.js' } })]);
  const c = compareOf(m, [job('test:coverage', 'fail', { reason: 'coverage below threshold', details: { file: 'b.js' } })]);
  assert.equal(c.results[0].verdict, VERDICT.CHANGED_KNOWN_FAILURE);
});

test('pass -> blocked is an environment mismatch, not a pass and not a failure', () => {
  const m = adoptOf([job('test:browser', 'pass')]);
  const c = compareOf(m, [job('test:browser', 'blocked', { reason: 'playwright not installed' })]);
  assert.equal(c.results[0].verdict, VERDICT.ENVIRONMENT_MISMATCH);
  assert.equal(c.summary.overall, 'inconclusive');
  assert.equal(c.summary.exitCode, 2);
});

// --- blocked / not-run are never rounded up --------------------------------

// T02: the verdict was already right here in P02 — `unchanged-blocked`, and
// the comment even said "still not a pass". The SEVERITY was wrong: the
// verdict sat in neither the blocking nor the inconclusive set, so a required
// runtime that produced no evidence at all exited 0, i.e. read as a pass to
// anything reading the exit code. Required and optional now part ways.
test('a required runtime blocked in both runs is never rounded up to a pass', () => {
  const b = job('test:desktop', 'blocked', { reason: 'no packaged app found' });
  const c = compareOf(adoptOf([b]), [b]);
  assert.equal(c.results[0].verdict, VERDICT.UNCHANGED_BLOCKED);
  assert.notEqual(c.results[0].verdict, VERDICT.PASS);
  assert.equal(c.results[0].current, 'blocked');
  assert.equal(c.results[0].severity, 'inconclusive');
  assert.equal(c.summary.overall, 'inconclusive');
  assert.equal(c.summary.exitCode, 2, 'no evidence for a required runtime is not exit 0');
  assert.deepEqual(c.summary.noEvidence.map((n) => n.job), ['test:desktop']);
});

test('the same block on an OPTIONAL job is genuinely nothing to answer for', () => {
  const b = job('test:rust-tauri', 'blocked', { required: false, reason: 'native fixture sidecar unavailable' });
  const c = compareOf(adoptOf([b]), [b]);
  assert.equal(c.results[0].verdict, VERDICT.UNCHANGED_BLOCKED);
  assert.equal(c.results[0].severity, 'ok');
  assert.equal(c.summary.overall, 'ok');
  assert.deepEqual(c.summary.noEvidence, []);
});

test('a required job that stays not-run is no evidence either', () => {
  const b = job('test:desktop', 'not-run', { reason: 'no suite defined yet' });
  const c = compareOf(adoptOf([b]), [b]);
  assert.equal(c.results[0].verdict, VERDICT.UNCHANGED_NOT_RUN);
  assert.equal(c.summary.exitCode, 2);
});

test('required in the baseline is enough, even if the run stopped saying so', () => {
  // The two can disagree; taking the stricter side is the only direction that
  // cannot silently downgrade a case that used to have to answer for itself.
  const m = adoptOf([job('test:desktop', 'blocked', { required: true, reason: 'no packaged app found' })]);
  const c = compareOf(m, [job('test:desktop', 'blocked', { required: false, reason: 'no packaged app found' })]);
  assert.equal(c.results[0].required, true);
  assert.equal(c.summary.exitCode, 2);
});

test('a failure first observed while blocked is not called a regression against a pass it never had', () => {
  const m = adoptOf([job('build:desktop', 'blocked', { reason: 'no rust toolchain' })]);
  const c = compareOf(m, [job('build:desktop', 'fail', { reason: 'bundle-ffmpeg-dylibs.py failed (1)' })]);
  assert.equal(c.results[0].verdict, VERDICT.NEWLY_OBSERVED_FAILURE);
  assert.equal(c.summary.overall, 'blocking');
});

test('not-run -> pass is reported as resolved rather than hidden', () => {
  const c = compareOf(adoptOf([job('test:integration', 'not-run', { reason: 'no suite in this candidate' })]), [job('test:integration', 'pass')]);
  assert.equal(c.results[0].verdict, VERDICT.RESOLVED);
  assert.equal(c.summary.overall, 'ok');
});

// --- references: reported, never silently reused ---------------------------

test('a changed source commit is reported as a stale reference', () => {
  const m = adoptOf([job('test:unit', 'pass')]);
  const moved = JSON.parse(JSON.stringify(REFS));
  moved.source.head = 'b'.repeat(40);
  const c = baseline.compare(m, receipt([job('test:unit', 'pass')]), { references: moved, platform: PLATFORM });
  assert.equal(c.references.stale, true);
  assert.ok(c.summary.staleReferences.includes('source.head'));
  assert.equal(c.summary.overall, 'inconclusive'); // every job passed; the reference is still reported
});

test('a changed fixture corpus is reported, so fixture-dependent results are not transferable', () => {
  const m = adoptOf([job('test:unit', 'pass')]);
  const moved = JSON.parse(JSON.stringify(REFS));
  moved.fixtures.sha256 = 'e'.repeat(64);
  const c = baseline.compare(m, receipt([job('test:unit', 'pass')]), { references: moved, platform: PLATFORM });
  assert.ok(c.summary.staleReferences.includes('fixtures.manifest'));
  assert.equal(c.summary.overall, 'inconclusive');
});

test('a missing artifact is reported as missing, not as an unchanged reference', () => {
  const m = adoptOf([job('test:unit', 'pass')]);
  const gone = JSON.parse(JSON.stringify(REFS));
  gone.artifacts.ffmpegSidecar = { path: 'src-tauri/binaries/ffmpeg-x', present: false };
  const c = baseline.compare(m, receipt([job('test:unit', 'pass')]), { references: gone, platform: PLATFORM });
  const f = c.references.findings.find((x) => x.ref === 'artifacts.ffmpegSidecar');
  assert.equal(f.kind, 'reference-missing');
  assert.equal(c.references.stale, true);
});

test('a differing host is recorded as an environment difference', () => {
  const m = adoptOf([job('test:unit', 'pass')]);
  const c = baseline.compare(m, receipt([job('test:unit', 'pass')]), { references: REFS, platform: Object.assign({}, PLATFORM, { arch: 'x64' }) });
  const f = c.references.findings.find((x) => x.ref === 'platform.arch');
  assert.equal(f.kind, 'environment-difference');
  assert.equal(f.expected, 'arm64');
  assert.equal(f.actual, 'x64');
});

// --- coverage of the comparison itself -------------------------------------

test('a job with no baseline entry is uncomparable rather than assumed fine', () => {
  const c = compareOf(adoptOf([job('test:unit', 'pass')]), [job('test:unit', 'pass'), job('test:brandnew', 'pass')]);
  const r = c.results.find((x) => x.job === 'test:brandnew');
  assert.equal(r.verdict, VERDICT.UNKNOWN_ENTRY);
  assert.equal(c.summary.overall, 'inconclusive');
});

test('a baseline entry the run did not cover is reported as missing, not as passing', () => {
  const c = compareOf(adoptOf([job('test:unit', 'pass'), job('test:desktop', 'blocked')]), [job('test:unit', 'pass')]);
  const r = c.results.find((x) => x.job === 'test:desktop');
  assert.equal(r.verdict, VERDICT.MISSING_ENTRY);
  assert.equal(r.current, null);
});

// T02: "a missing required case fails". P02 filed every missing entry as
// inconclusive, so a required entry silently dropped from a profile exited 2
// alongside genuine can't-tell cases instead of failing.
test('a required baseline entry the run was expected to cover is a failure', () => {
  const m = adoptOf([job('test:unit', 'pass'), job('test:desktop', 'blocked', { required: true })]);
  const c = compareOf(m, [job('test:unit', 'pass')], { expect: ['test:unit', 'test:desktop'] });
  const r = c.results.find((x) => x.job === 'test:desktop');
  assert.equal(r.verdict, VERDICT.MISSING_ENTRY);
  assert.equal(r.severity, 'blocking');
  assert.equal(c.summary.exitCode, 1);
  assert.ok(c.summary.blocking.some((b) => b.job === 'test:desktop' && b.required));
});

test('an optional entry the run did not cover is uncomparable, not a failure', () => {
  const m = adoptOf([job('test:unit', 'pass'), job('bench', 'pass', { required: false })]);
  const c = compareOf(m, [job('test:unit', 'pass')], { expect: ['test:unit', 'bench'] });
  assert.equal(c.results.find((x) => x.job === 'bench').severity, 'inconclusive');
  assert.equal(c.summary.exitCode, 2);
});

test('a targeted run is not judged for entries it never claimed to cover', () => {
  // `npm run test:rust` has not failed to answer for test:desktop.
  const m = adoptOf([job('test:rust', 'pass'), job('test:desktop', 'blocked', { required: true })]);
  const c = compareOf(m, [job('test:rust', 'pass')], { expect: ['test:rust'] });
  const r = c.results.find((x) => x.job === 'test:desktop');
  assert.equal(r.verdict, VERDICT.MISSING_ENTRY, 'still reported, not hidden');
  assert.equal(r.severity, 'ok');
  assert.equal(c.summary.overall, 'ok');
});

// --- adoption --------------------------------------------------------------

test('adoption keeps the exact case and marks entries measured elsewhere as carried over', () => {
  const here = receipt([job('test:unit', 'pass')]);
  const elsewhere = receipt([job('build:desktop', 'fail', { reason: 'bundle-ffmpeg-dylibs.py failed (1)' })], {
    runId: 'other', source: { head: 'c'.repeat(40), dirty: false },
  });
  const m = baseline.adopt([{ receipt: here, receiptPath: null }, { receipt: elsewhere, receiptPath: null }], { references: REFS, platform: PLATFORM });
  const unit = m.entries.find((e) => e.job === 'test:unit');
  const desktop = m.entries.find((e) => e.job === 'build:desktop');
  assert.equal(unit.carriedOver, false);
  assert.equal(desktop.carriedOver, true);
  assert.match(desktop.carriedOverNote, /Not re-measured here/);
  assert.equal(desktop.case.reason, 'bundle-ffmpeg-dylibs.py failed (1)');
  assert.deepEqual(m.summary.carriedOver, ['build:desktop']);
  assert.deepEqual(m.summary.knownNonPass.map((e) => e.job), ['build:desktop']);
});

test('a later receipt supersedes an earlier entry for the same job', () => {
  const first = receipt([job('test:rust', 'fail', { reason: 'old' })], { runId: 'first' });
  const second = receipt([job('test:rust', 'pass')], { runId: 'second' });
  const m = baseline.adopt([{ receipt: first, receiptPath: null }, { receipt: second, receiptPath: null }], { references: REFS, platform: PLATFORM });
  assert.equal(m.entries.find((e) => e.job === 'test:rust').status, 'pass');
  assert.equal(m.entries.length, 1);
});

test('a pass entry carries no case signature, so it can never match a failure', () => {
  const m = adoptOf([job('test:unit', 'pass')]);
  assert.equal(m.entries[0].caseSignature, null);
});

// --- the committed manifest and the CLI ------------------------------------

test('the committed manifest is valid, current and self-consistent', () => {
  const m = baseline.loadManifest();
  assert.equal(m.schema, baseline.SCHEMA);
  assert.ok(m.entries.length > 0, 'manifest has entries');
  assert.equal(m.entries.length, m.summary.total);

  // Every entry names a real job, and every non-pass entry states its case.
  const { JOBS } = require('./lib/jobs.cjs');
  for (const e of m.entries) {
    assert.ok(JOBS[e.job], `manifest entry "${e.job}" is a real job`);
    assert.ok(['pass', 'fail', 'blocked', 'not-run'].includes(e.status), `${e.job} has a receipt status`);
    if (e.status !== 'pass') assert.ok(e.case && e.case.reason, `${e.job} is ${e.status} and must state its exact case`);
    assert.ok(e.evidence && e.evidence.runId, `${e.job} names the receipt it came from`);
  }

  // The manifest describes THIS tree: its recorded source must be an ancestor
  // of, or equal to, what is checked out. A stale committed manifest is the
  // failure mode this whole task exists to prevent.
  const head = spawnSync('git', ['rev-parse', 'HEAD'], { cwd: ROOT, encoding: 'utf8' });
  if (head.status === 0) {
    const merge = spawnSync('git', ['merge-base', '--is-ancestor', m.references.source.head, head.stdout.trim()], { cwd: ROOT, encoding: 'utf8' });
    assert.equal(merge.status, 0, `manifest source ${m.references.source.head.slice(0, 12)} must be an ancestor of HEAD; re-adopt after moving the baseline`);
  }
});

test('the committed manifest quotes no absolute machine path', () => {
  // Receipts are gitignored and may hold absolute paths; the manifest is
  // committed source. A checkout path in here would leak a private machine
  // path and make the case signature machine-specific.
  const raw = fs.readFileSync(baseline.MANIFEST_PATH, 'utf8');
  assert.ok(!raw.includes(os.homedir()), 'manifest must not contain the home directory');
  assert.ok(!raw.includes(ROOT), 'manifest must not contain the checkout path');
  for (const m of raw.matchAll(/"(?:\/Users|\/home|\/private\/var\/folders)\/[^"]*"/g)) {
    assert.fail(`manifest contains an absolute machine path: ${m[0].slice(0, 80)}`);
  }
});

test('redaction survives a round trip and keeps the case comparable', () => {
  const abs = path.join(ROOT, 'src-tauri', 'binaries', 'ffmpeg-x');
  const c = baseline.exactCase(job('test:rust-tauri', 'blocked', { reason: `sidecar unavailable: ${abs}`, details: { path: abs } }));
  assert.equal(c.reason, 'sidecar unavailable: <repo>/src-tauri/binaries/ffmpeg-x');
  assert.equal(c.details.path, '<repo>/src-tauri/binaries/ffmpeg-x');
  // The same job observed from a different checkout must produce the same
  // signature, otherwise every machine would report a changed known failure.
  assert.equal(baseline.caseSignature(c), baseline.caseSignature(baseline.exactCase(
    job('test:rust-tauri', 'blocked', { reason: `sidecar unavailable: ${abs}`, details: { path: abs } }))));
});

test('the manifest covers every job the full verify profile runs', () => {
  const { PROFILES } = require('./lib/jobs.cjs');
  const m = baseline.loadManifest();
  const covered = new Set(m.entries.map((e) => e.job));
  const missing = PROFILES.full.filter((j) => !covered.has(j));
  assert.deepEqual(missing, [], 'every full-profile job has a baseline entry');
});

test('--check round trips through the CLI and reports a synthetic regression', (t) => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-baseline-'));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));

  const reports = path.join(dir, 'reports');
  const runDir = path.join(reports, '20260909T000000Z-test');
  fs.mkdirSync(runDir, { recursive: true });

  // A receipt that fails a job the committed manifest records as passing.
  const m = baseline.loadManifest();
  const passing = m.entries.find((e) => e.status === 'pass');
  assert.ok(passing, 'committed manifest has at least one passing entry to regress');
  fs.writeFileSync(path.join(runDir, 'receipt.json'), JSON.stringify(receipt([
    job(passing.job, 'fail', { reason: 'synthetic failure injected by the P02 regression test' }),
  ]), null, 2));

  const r = spawnSync(process.execPath, [path.join(__dirname, 'baseline.cjs'), '--check', '--from', runDir, '--json'], {
    cwd: ROOT, encoding: 'utf8', env: Object.assign({}, process.env, { NEMO_REPORT_DIR: reports }),
  });
  assert.equal(r.status, 1, 'a new regression exits 1');
  const cmp = JSON.parse(r.stdout);
  const hit = cmp.results.find((x) => x.job === passing.job);
  assert.equal(hit.verdict, VERDICT.NEW_REGRESSION);
  assert.ok(cmp.summary.blocking.some((b) => b.job === passing.job));
});

test('--check exits 0 when the run reproduces the adopted state', (t) => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-baseline-ok-'));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  const reports = path.join(dir, 'reports');
  const runDir = path.join(reports, '20260909T000001Z-test');
  fs.mkdirSync(runDir, { recursive: true });

  // Replay the manifest's own entries as a receipt: same statuses, same cases.
  const m = baseline.loadManifest();
  const jobs = m.entries.map((e) => job(e.job, e.status, {
    required: e.required, reason: e.case.reason, exitCode: e.case.exitCode, limitations: e.case.limitations, details: e.case.details,
  }));
  fs.writeFileSync(path.join(runDir, 'receipt.json'), JSON.stringify(receipt(jobs), null, 2));

  const r = spawnSync(process.execPath, [path.join(__dirname, 'baseline.cjs'), '--check', '--from', runDir, '--json'], {
    cwd: ROOT, encoding: 'utf8', env: Object.assign({}, process.env, { NEMO_REPORT_DIR: reports }),
  });
  const cmp = JSON.parse(r.stdout);
  assert.deepEqual(cmp.summary.blocking, [], 'replaying the adopted state produces no blocking verdict');
  assert.notEqual(r.status, 1, 'replaying the adopted state is never a failure');

  // Every inconclusive entry must be one of the required jobs the baseline
  // itself records as having produced no result (T02): replaying the adopted
  // state re-lists them, because replaying "still no evidence" is still no
  // evidence. Nothing else may be inconclusive.
  const noEvidenceJobs = m.entries.filter((e) => e.required && (e.status === 'blocked' || e.status === 'not-run')).map((e) => e.job);
  assert.deepEqual(cmp.summary.inconclusive.map((i) => i.job).sort(), noEvidenceJobs.slice().sort(),
    'the only uncomparable entries are the required ones with no evidence');
  assert.deepEqual(cmp.summary.noEvidence.map((n) => n.job).sort(), noEvidenceJobs.slice().sort());

  // Exit is then 2 when the tree moved OR a required runtime still has no
  // evidence, 0 only when neither applies. Which one holds depends on the
  // checkout the suite runs in, so assert the implication, not a fixed code.
  const expectTwo = cmp.references.stale || noEvidenceJobs.length > 0;
  assert.equal(r.status, expectTwo ? 2 : 0);
  if (cmp.references.stale) {
    assert.ok(cmp.summary.staleReferences.length > 0, 'a stale result names which reference moved');
  }
});

// --- T02: a separate result, never a rewritten one -------------------------

test('comparison does not touch the receipt it classifies', () => {
  const failing = job('build:desktop', 'fail', { reason: 'bundle-ffmpeg-dylibs.py failed (1)', exitCode: 1 });
  const m = adoptOf([failing]);
  const run = receipt([job('build:desktop', 'fail', { reason: 'bundle-ffmpeg-dylibs.py failed (1)', exitCode: 1 })]);
  const before = JSON.stringify(run);
  const c = baseline.compare(m, run, { references: REFS, platform: PLATFORM, now: '2026-09-09T00:03:00Z' });

  assert.equal(JSON.stringify(run), before, 'the receipt is read, never rewritten');
  assert.equal(c.results[0].verdict, VERDICT.UNCHANGED_KNOWN_FAILURE);
  assert.equal(c.results[0].current, 'fail', 'the raw failure stays a failure');
  assert.equal(c.results[0].currentReason, 'bundle-ffmpeg-dylibs.py failed (1)', 'the raw reason is carried verbatim');
  // "Known" is a label on the comparison, not permission for the run to pass:
  // the comparison's own overall may be ok while the job is still failing.
  assert.equal(c.summary.overall, 'ok');
  assert.equal(run.jobs[0].status, 'fail');
});

test('runJobs writes a comparison beside the receipt without changing the run', (t) => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-baseline-wire-'));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  const reports = path.join(dir, 'reports');

  const r = spawnSync(process.execPath, [path.join(__dirname, 'job.cjs'), 'check', '--quiet'], {
    cwd: ROOT, encoding: 'utf8', env: Object.assign({}, process.env, { NEMO_REPORT_DIR: reports }),
  });
  const runs = fs.readdirSync(reports);
  assert.equal(runs.length, 1, 'one report directory');
  const runDir = path.join(reports, runs[0]);

  const receiptJson = JSON.parse(fs.readFileSync(path.join(runDir, 'receipt.json'), 'utf8'));
  const cmpPath = path.join(runDir, 'comparison.json');
  assert.ok(fs.existsSync(cmpPath), 'the run wrote comparison.json next to the receipt');
  const cmp = JSON.parse(fs.readFileSync(cmpPath, 'utf8'));
  assert.equal(cmp.schema, 'nemo.baseline-comparison/1');

  // The run's exit code is the receipt's own, never the comparison's.
  assert.equal(r.status, receiptJson.summary.exitCode);
  // A targeted single-job run is not judged for the rest of the baseline.
  assert.deepEqual(cmp.summary.blocking, [], 'a targeted run does not block on entries it never claimed');
  const outOfScope = cmp.results.filter((x) => x.job !== 'check' && x.job !== 'doctor');
  assert.ok(outOfScope.length > 0 && outOfScope.every((x) => x.severity === 'ok'),
    'entries outside the run are reported but not judged');
});

test('--no-baseline runs without comparing', (t) => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-baseline-off-'));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  const reports = path.join(dir, 'reports');
  spawnSync(process.execPath, [path.join(__dirname, 'job.cjs'), 'check', '--quiet', '--no-baseline'], {
    cwd: ROOT, encoding: 'utf8', env: Object.assign({}, process.env, { NEMO_REPORT_DIR: reports }),
  });
  const runDir = path.join(reports, fs.readdirSync(reports)[0]);
  assert.ok(fs.existsSync(path.join(runDir, 'receipt.json')));
  assert.equal(fs.existsSync(path.join(runDir, 'comparison.json')), false);
});
