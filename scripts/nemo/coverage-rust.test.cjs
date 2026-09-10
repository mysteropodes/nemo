'use strict';
// Receipt tests for the T04 Rust coverage ratchet.
//
// The evaluator is a hand-rolled comparator, so most of these are mutation
// tests: take a report that legitimately passes, change exactly one thing that
// SHOULD be caught, and assert it is. A gate is only worth its receipt if it
// can be shown to fail.

const test = require('node:test');
const assert = require('node:assert');
const fs = require('node:fs');
const path = require('node:path');

const {
  SCHEMA, METRICS, floorPct, readReport, validateBaseline, evaluateRustCoverage, buildBaseline,
} = require('./lib/coverage-rust.cjs');
const { listSourceFiles, SOURCE_ROOT, BASELINE } = require('./lib/coverage-rust-job.cjs');

const ROOT = path.resolve(__dirname, '..', '..');
const SRC = 'nemo-mcp/src';

function summary(metrics) {
  const out = { branches: { count: 0, covered: 0, percent: 0 } };
  for (const metric of METRICS) {
    out[metric] = { count: 100, covered: Math.round(metrics[metric]), percent: metrics[metric] };
  }
  return out;
}

// An llvm-cov JSON export shaped like the real one: absolute filenames under
// `root`, a per-file summary, and a crate totals block.
function makeReport(files, opts = {}) {
  const root = opts.root || '/checkout';
  const totals = opts.totals || { lines: 90, regions: 90, functions: 90 };
  return {
    type: 'llvm.coverage.json.export',
    version: '3.0.1',
    data: [{
      files: files.map((f) => ({ filename: `${root}/${f.path}`, summary: summary(f) })),
      totals: summary(totals),
    }],
  };
}

const TWO_FILES = [
  { path: `${SRC}/wire.rs`, lines: 96.55, regions: 87.06, functions: 87.5 },
  { path: `${SRC}/server.rs`, lines: 93.55, regions: 92.2, functions: 100 },
];

function baselineFor(files, extra = {}) {
  return Object.assign({
    schema: SCHEMA,
    metrics: METRICS.slice(),
    sourceRoot: SRC,
    totals: { lines: 90, regions: 90, functions: 90 },
    files: Object.fromEntries(files.map((f) => [f.path, {
      lines: floorPct(f.lines), regions: floorPct(f.regions), functions: floorPct(f.functions),
    }])),
    exclusions: [],
  }, extra);
}

function evaluate(report, baseline, sourceFiles, root = '/checkout') {
  return evaluateRustCoverage({ report, baseline, sourceFiles, root });
}

const PATHS = TWO_FILES.map((f) => f.path);

// ---- rounding -----------------------------------------------------------

test('floorPct floors and never rounds up', () => {
  // 93.5483…% rounds to 93.55. A 93.55 floor would fail against the very run
  // that produced it, so the floor must be 93.54.
  assert.strictEqual(floorPct(93.54838709677419), 93.54);
  assert.strictEqual(floorPct(94.44444444444444), 94.44);
  assert.strictEqual(floorPct(87.05882352941177), 87.05);
  assert.strictEqual(floorPct(100), 100);
  assert.strictEqual(floorPct(0), 0);
});

test('a baseline built from a report passes against that same report', () => {
  const report = makeReport(TWO_FILES);
  const built = buildBaseline({ report, root: '/checkout', sourceRoot: SRC, sourceFiles: PATHS, meta: {} });
  const verdict = evaluate(report, built, PATHS);
  assert.deepStrictEqual(verdict.violations, []);
  assert.ok(verdict.ok);
});

// ---- regression detection (mutation) ------------------------------------

test('any metric falling 0.01 below its floor is a violation', () => {
  for (const target of TWO_FILES) {
    for (const metric of METRICS) {
      const mutated = TWO_FILES.map((f) => (f.path === target.path
        ? Object.assign({}, f, { [metric]: floorPct(f[metric]) - 0.01 })
        : f));
      const verdict = evaluate(makeReport(mutated), baselineFor(TWO_FILES), PATHS);
      const hit = verdict.violations.filter((v) => v.rule === 'coverage-baseline-regression'
        && v.file === target.path && v.detail.metric === metric);
      assert.strictEqual(hit.length, 1, `${target.path} ${metric} regression was not caught`);
      assert.strictEqual(verdict.ok, false);
    }
  }
});

test('coverage above the floor is an improvement, not a violation', () => {
  const better = TWO_FILES.map((f) => Object.assign({}, f, { lines: 99.99 }));
  const verdict = evaluate(makeReport(better), baselineFor(TWO_FILES), PATHS);
  assert.deepStrictEqual(verdict.violations, []);
  assert.ok(verdict.improvements.some((i) => i.metric === 'lines'));
});

test('a candidate equal to its floor is neither a violation nor an improvement', () => {
  const verdict = evaluate(makeReport(TWO_FILES), baselineFor(TWO_FILES), PATHS);
  assert.deepStrictEqual(verdict.violations, []);
  assert.deepStrictEqual(verdict.improvements, []);
});

test('crate totals falling below their floor is a violation', () => {
  const report = makeReport(TWO_FILES, { totals: { lines: 89.99, regions: 90, functions: 90 } });
  const verdict = evaluate(report, baselineFor(TWO_FILES), PATHS);
  assert.ok(verdict.violations.some((v) => v.rule === 'coverage-baseline-totals-regression'));
});

// ---- the silence failure modes ------------------------------------------

test('a baselined file still on disk but absent from the report fails', () => {
  // The failure this whole leaf exists to prevent: source stops being
  // instrumented and the run looks clean because nothing reports on it.
  const report = makeReport([TWO_FILES[0]]);
  const verdict = evaluate(report, baselineFor(TWO_FILES), PATHS);
  const hit = verdict.violations.filter((v) => v.rule === 'coverage-baseline-unmeasured');
  assert.strictEqual(hit.length, 1);
  assert.strictEqual(hit[0].file, `${SRC}/server.rs`);
  assert.strictEqual(verdict.ok, false);
});

test('a baselined file deleted from disk is a removal, not a violation', () => {
  const report = makeReport([TWO_FILES[0]]);
  const verdict = evaluate(report, baselineFor(TWO_FILES), [TWO_FILES[0].path]);
  assert.deepStrictEqual(verdict.violations, []);
  assert.deepStrictEqual(verdict.removals.map((r) => r.file), [`${SRC}/server.rs`]);
});

test('a measured file with no baseline entry fails', () => {
  const extra = TWO_FILES.concat([{ path: `${SRC}/new.rs`, lines: 10, regions: 10, functions: 10 }]);
  const verdict = evaluate(makeReport(extra), baselineFor(TWO_FILES), extra.map((f) => f.path));
  assert.ok(verdict.violations.some((v) => v.rule === 'coverage-baseline-unbaselined' && v.file === `${SRC}/new.rs`));
});

test('a source file on disk that is neither baselined nor measured fails', () => {
  // Produces no coverage record at all, so a report-only comparison would
  // never see it. Only the on-disk source list catches this.
  const verdict = evaluate(makeReport(TWO_FILES), baselineFor(TWO_FILES), PATHS.concat([`${SRC}/ghost.rs`]));
  const hit = verdict.violations.filter((v) => v.rule === 'coverage-baseline-unlisted-source');
  assert.strictEqual(hit.length, 1);
  assert.strictEqual(hit[0].file, `${SRC}/ghost.rs`);
});

test('a reviewed exclusion suppresses the unlisted-source failure', () => {
  const baseline = baselineFor(TWO_FILES, { exclusions: [{ path: `${SRC}/ghost.rs`, reason: 'reviewed' }] });
  const verdict = evaluate(makeReport(TWO_FILES), baseline, PATHS.concat([`${SRC}/ghost.rs`]));
  assert.deepStrictEqual(verdict.violations, []);
});

test('an unlisted source file is reported once, not twice', () => {
  const extra = TWO_FILES.concat([{ path: `${SRC}/new.rs`, lines: 10, regions: 10, functions: 10 }]);
  const verdict = evaluate(makeReport(extra), baselineFor(TWO_FILES), extra.map((f) => f.path));
  assert.strictEqual(verdict.violations.filter((v) => v.file === `${SRC}/new.rs`).length, 1);
});

// ---- no-regions files ---------------------------------------------------

test('a no-regions file that gains coverage demands a re-baseline', () => {
  const baseline = baselineFor(TWO_FILES);
  baseline.files[`${SRC}/lib.rs`] = { noRegions: true, reason: 'declarations only' };
  const withLib = TWO_FILES.concat([{ path: `${SRC}/lib.rs`, lines: 50, regions: 50, functions: 50 }]);
  const verdict = evaluate(makeReport(withLib), baseline, withLib.map((f) => f.path));
  assert.ok(verdict.violations.some((v) => v.rule === 'coverage-baseline-gained-regions' && v.file === `${SRC}/lib.rs`));
});

test('a no-regions file that stays empty passes', () => {
  const baseline = baselineFor(TWO_FILES);
  baseline.files[`${SRC}/lib.rs`] = { noRegions: true, reason: 'declarations only' };
  const verdict = evaluate(makeReport(TWO_FILES), baseline, PATHS.concat([`${SRC}/lib.rs`]));
  assert.deepStrictEqual(verdict.violations, []);
  assert.ok(verdict.perFile.some((r) => r.path === `${SRC}/lib.rs` && r.noRegions));
});

// ---- worktree independence ----------------------------------------------

test('the same coverage evaluates identically from a different checkout path', () => {
  // llvm-cov writes absolute paths. Two worktrees of this repo must not
  // disagree about whether the baseline holds.
  const a = evaluate(makeReport(TWO_FILES, { root: '/checkout' }), baselineFor(TWO_FILES), PATHS, '/checkout');
  const b = evaluate(makeReport(TWO_FILES, { root: '/somewhere/else/nemo-wt' }), baselineFor(TWO_FILES), PATHS, '/somewhere/else/nemo-wt');
  assert.deepStrictEqual(a.violations, b.violations);
  assert.deepStrictEqual(a.perFile, b.perFile);
  assert.ok(a.ok && b.ok);
});

test('files outside the crate source root are ignored, not compared', () => {
  const report = makeReport(TWO_FILES.concat([{ path: 'vendor/dep/lib.rs', lines: 1, regions: 1, functions: 1 }]));
  const verdict = evaluate(report, baselineFor(TWO_FILES), PATHS);
  assert.deepStrictEqual(verdict.violations, []);
  assert.ok(verdict.foreign.includes('vendor/dep/lib.rs'));
});

// ---- refusing a meaningless gate ----------------------------------------

test('a branch floor is refused outright', () => {
  // Branch counts are 0 on the stable toolchain, so a branch floor would be a
  // gate that can never fail while looking like coverage.
  const viaMetrics = baselineFor(TWO_FILES, { metrics: METRICS.concat(['branches']) });
  assert.throws(() => validateBaseline(viaMetrics), /metrics must be exactly|branch floor/);

  const viaFile = baselineFor(TWO_FILES);
  viaFile.files[PATHS[0]].branches = 0;
  assert.throws(() => validateBaseline(viaFile), /branch floor/);
});

// ---- malformed input never passes ---------------------------------------

test('a malformed or foreign report throws instead of passing', () => {
  const baseline = baselineFor(TWO_FILES);
  const cases = [
    [null, /report type/],
    [{ type: 'something-else', data: [] }, /report type/],
    [{ type: 'llvm.coverage.json.export' }, /data\[0\]\.files/],
    [{ type: 'llvm.coverage.json.export', data: [{ files: 'nope' }] }, /data\[0\]\.files/],
    [{ type: 'llvm.coverage.json.export', data: [{ files: [{ filename: `/checkout/${SRC}/wire.rs` }], totals: summary({ lines: 1, regions: 1, functions: 1 }) }] }, /no summary/],
  ];
  for (const [report, pattern] of cases) {
    assert.throws(() => evaluate(report, baseline, PATHS), pattern);
  }
});

test('a report entry missing one metric throws rather than scoring it', () => {
  const broken = makeReport(TWO_FILES);
  delete broken.data[0].files[0].summary.regions;
  assert.throws(() => evaluate(broken, baselineFor(TWO_FILES), PATHS), /numeric regions\.percent/);
});

test('a baseline with the wrong schema or no source root is refused', () => {
  assert.throws(() => validateBaseline(baselineFor(TWO_FILES, { schema: 'other/1' })), /baseline schema/);
  assert.throws(() => validateBaseline(baselineFor(TWO_FILES, { sourceRoot: '' })), /sourceRoot/);
  assert.throws(() => evaluateRustCoverage({ report: makeReport(TWO_FILES), baseline: baselineFor(TWO_FILES), root: '/checkout' }),
    /sourceFiles/);
});

test('a baseline entry missing a metric floor throws', () => {
  const baseline = baselineFor(TWO_FILES);
  delete baseline.files[PATHS[0]].functions;
  assert.throws(() => evaluate(makeReport(TWO_FILES), baseline, PATHS), /no numeric functions floor/);
});

// ---- the committed artifact --------------------------------------------

test('the committed baseline is valid and describes files that exist', () => {
  const baseline = JSON.parse(fs.readFileSync(path.join(ROOT, ...BASELINE.split('/')), 'utf8'));
  validateBaseline(baseline);
  assert.strictEqual(baseline.sourceRoot, SOURCE_ROOT);
  const onDisk = listSourceFiles(ROOT, SOURCE_ROOT);
  assert.ok(onDisk.length > 0, 'no Rust source discovered under ' + SOURCE_ROOT);
  for (const rel of Object.keys(baseline.files)) {
    assert.ok(onDisk.includes(rel), `baseline names ${rel}, which is not on disk`);
  }
  for (const rel of onDisk) {
    assert.ok(Object.hasOwn(baseline.files, rel), `${rel} exists but is not baselined`);
  }
});

test('the committed baseline records its tool pin and test result', () => {
  const baseline = JSON.parse(fs.readFileSync(path.join(ROOT, ...BASELINE.split('/')), 'utf8'));
  assert.strictEqual(baseline.tool.name, 'cargo-llvm-cov');
  assert.match(baseline.tool.version, /^\d+\.\d+\.\d+$/);
  assert.ok(baseline.recorded.sha, 'baseline records no source SHA');
  // Floors measured against a dirty crate would not be reproducible.
  assert.strictEqual(baseline.recorded.crateDirty, false);
  assert.strictEqual(baseline.tests.failed, 0);
  assert.ok(baseline.branchesExcluded.length > 0);
});

test('listSourceFiles finds the crate source and nothing else', () => {
  const found = listSourceFiles(ROOT, SOURCE_ROOT);
  assert.ok(found.every((f) => f.startsWith(SOURCE_ROOT + '/') && f.endsWith('.rs')));
  assert.ok(found.includes('nemo-mcp/src/lib.rs'));
  // build.rs sits beside the crate, not under src/, and is not instrumented.
  assert.ok(!found.includes('nemo-mcp/build.rs'));
});
