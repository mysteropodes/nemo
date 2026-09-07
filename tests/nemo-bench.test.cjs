'use strict';
// The R03 bench runner: a receipt with identity, measured and declared
// workloads, deterministic workload documents pinned by the fixture manifest,
// isolated memory numbers, and no budgets.
const assert = require('node:assert/strict');
const test = require('node:test');
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { spawnSync } = require('node:child_process');

const ROOT = path.resolve(__dirname, '..');
const bench = require('./bench/run.cjs');
const gen = require('./fixtures/generate.cjs');
const corpus = require('./fixtures/lib/corpus.cjs');
const manifest = JSON.parse(fs.readFileSync(path.join(ROOT, 'tests', 'fixtures', 'manifest.json'), 'utf8'));
const sha256 = (s) => crypto.createHash('sha256').update(s).digest('hex');
const Q = bench.QUICK_WORKLOAD;

test('a quick run yields a receipt bound to source and hardware, with measured and declared workloads and no budgets', () => {
  const r = bench.runBench({ quick: true, iterations: 1 });
  assert.equal(r.schema, 'nemo.bench/1');
  assert.match(r.source.head, /^[0-9a-f]{40}$/);
  assert.ok(r.platform.cpuCount > 0 && r.platform.memoryBytes > 0 && r.platform.os && r.platform.arch);
  assert.equal(r.budgets, null);
  const ids = r.workloads.map((w) => w.id);
  for (const id of ['evaluation.valueAtFrame', 'evaluation.evalCurvePoints', 'evaluation.expression', 'copy.undoClone.' + Q, 'copy.jsonClone.' + Q, 'copy.serialize.' + Q, 'memory.parsedDocument.' + Q, 'render.engine.export', 'export.mp4.export']) assert.ok(ids.includes(id), id);
  for (const w of r.workloads) {
    if (w.status === 'ran') { assert.ok(Number.isFinite(w.stats.median) && w.stats.median >= 0 && w.stats.unit, w.id); assert.ok(w.backend, w.id); }
    else { assert.equal(w.status, 'not-run'); assert.match(w.reason, /WebGPU/); assert.equal(w.fixture, 'export'); }
  }
  // The evaluation backend names the production modules the sandbox ran, in
  // src/index.html order: easing and opacity dependencies precede Motion.
  const kernel = fs.existsSync(path.join(ROOT, 'src', 'js', 'animation', 'curve.js'));
  const prelude = ['animation/curve.js', 'domain/animation/opacity.js']
    .filter(file => fs.existsSync(path.join(ROOT, 'src', 'js', file)));
  const expectedBackend = bench.evaluationBackend({ modules: [...prelude, 'motion.js'] });
  assert.equal(r.backends.evaluation, expectedBackend);
  for (const w of r.workloads.filter((x) => x.kind === 'evaluation')) assert.equal(w.backend, expectedBackend, w.id);
  for (const id of ['evaluation.valueAtFrame', 'evaluation.evalCurvePoints']) {
    const w = r.workloads.find((x) => x.id === id);
    assert.match(w.workload.evaluator, kernel ? /^SMAnimationCurve\.evalCurvePoints \(src\/js\/animation\/curve\.js\)$/ : /^evalCurvePoints declared in src\/js\/motion\.js$/, id);
  }
  const undo = r.workloads.find((w) => w.id === 'copy.undoClone.' + Q);
  assert.match(undo.backend, /_cloneLayersForUndo/, 'the undo clone is the production function, not a JSON round trip');
  assert.deepEqual(undo.workload.heavyFieldsDetached, ['src', 'bitmapPressureProfile']);
  assert.equal(undo.workload.sha256, manifest.workloads.find((w) => w.id === Q).sha256, 'the receipt names the exact document it measured');
  const mem = r.workloads.find((w) => w.id === 'memory.parsedDocument.' + Q);
  assert.ok(mem.workload.gcAvailable, 'the isolated child always runs with --expose-gc');
  assert.ok(mem.stats.median > 0 && mem.stats.bytesPerStroke > 0);
});

test('workload documents are deterministic and match the manifest hashes; a tampered document is rejected', () => {
  assert.deepEqual(manifest.workloads.map((w) => w.id), Object.keys(gen.WORKLOAD_DOCS));
  for (const w of manifest.workloads) {
    const json = JSON.stringify(corpus.benchDocument(gen.WORKLOAD_DOCS[w.id]));
    assert.equal(sha256(json), w.sha256, w.id);
    assert.equal(json.length, w.bytes, w.id + ' bytes');
    assert.equal(w.committed, false);
    assert.deepEqual(bench.verifyWorkloadJson(w.id, json).sha, w.sha256);
  }
  const tampered = corpus.benchDocument(gen.WORKLOAD_DOCS[Q]);
  tampered.canvasW = 1;
  assert.throws(() => bench.verifyWorkloadJson(Q, JSON.stringify(tampered)), /differs from the manifest/);
  assert.throws(() => bench.verifyWorkloadJson('not-a-workload', '{}'), /not pinned/);
});

test('memory measurements are isolated: repeating a workload, or measuring another one first, gives the same number', () => {
  const a = bench.measureMemory(Q);
  const other = bench.measureMemory('bench-images-20x24');
  const b = bench.measureMemory(Q);
  assert.ok(a.delta > 0 && b.delta > 0 && other.delta > 0);
  const spread = Math.abs(a.delta - b.delta) / Math.max(a.delta, b.delta);
  assert.ok(spread < 0.05, `repeat spread ${(spread * 100).toFixed(1)}% (${a.delta} vs ${b.delta} bytes)`);
  assert.equal(a.identity.sha256, b.identity.sha256);
});

test('the CLI writes the receipt it prints', () => {
  const out = path.join(fs.mkdtempSync(path.join(require('node:os').tmpdir(), 'nemo-bench-test-')), 'bench.json');
  const r = spawnSync(process.execPath, ['--expose-gc', path.join(ROOT, 'tests', 'bench', 'run.cjs'), '--quick', '--iterations', '1', '--out', out], { encoding: 'utf8' });
  assert.equal(r.status, 0, r.stderr);
  assert.match(r.stdout, /bench: \d+ measured, 2 not-run/);
  const receipt = JSON.parse(fs.readFileSync(out, 'utf8'));
  assert.equal(receipt.quick, true);
  assert.ok(receipt.workloads.find((w) => w.id === 'memory.parsedDocument.' + Q).workload.gcAvailable);
  fs.rmSync(path.dirname(out), { recursive: true, force: true });
});
