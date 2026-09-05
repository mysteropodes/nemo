'use strict';
// The R03 bench runner: a receipt with identity, measured and declared
// workloads, deterministic scale documents, and no budgets.
const assert = require('node:assert/strict');
const test = require('node:test');
const fs = require('node:fs');
const path = require('node:path');
const { spawnSync } = require('node:child_process');

const ROOT = path.resolve(__dirname, '..');
const bench = require('./bench/run.cjs');
const gen = require('./fixtures/generate.cjs');
const lib = require('./fixtures/lib.cjs');
const manifest = JSON.parse(fs.readFileSync(path.join(ROOT, 'tests', 'fixtures', 'manifest.json'), 'utf8'));

test('a quick run yields a receipt bound to source and hardware, with measured and declared workloads and no budgets', () => {
  const r = bench.runBench({ quick: true, iterations: 1 });
  assert.equal(r.schema, 'nemo.bench/1');
  assert.match(r.source.head, /^[0-9a-f]{40}$/);
  assert.ok(r.platform.cpuCount > 0 && r.platform.memoryBytes > 0 && r.platform.os && r.platform.arch);
  assert.equal(r.budgets, null);
  const ids = r.workloads.map((w) => w.id);
  for (const id of ['evaluation.rawValueAtFrame', 'evaluation.evalCurvePoints', 'copy.undoClone.200', 'copy.serialize.200', 'memory.parsedDocument.200', 'render.engine.export-12f-320x240', 'export.mp4.export-12f-320x240']) assert.ok(ids.includes(id), id);
  for (const w of r.workloads) {
    if (w.status === 'ran') { assert.ok(Number.isFinite(w.stats.median) && w.stats.median >= 0 && w.stats.unit, w.id); assert.ok(w.backend, w.id); }
    else { assert.equal(w.status, 'not-run'); assert.match(w.reason, /WebGPU/); assert.equal(w.fixture, 'export-12f-320x240'); }
  }
});

test('scale documents are deterministic and match the manifest hashes', () => {
  for (const g of manifest.generated) {
    const a = JSON.stringify(lib.scaleDocument(g.strokes, g.generation.seed));
    assert.equal(lib.sha256(a), g.sha256, g.id);
    assert.equal(a.length, g.bytes, g.id + ' bytes');
    assert.equal(JSON.parse(a).layers[0].frames[0].strokes.length, g.strokes);
  }
  assert.deepEqual(gen.GENERATED_SCALE.map((g) => g.id), manifest.generated.map((g) => g.id));
});

test('the CLI writes the receipt it prints', () => {
  const out = path.join(fs.mkdtempSync(path.join(require('node:os').tmpdir(), 'nemo-bench-test-')), 'bench.json');
  const r = spawnSync(process.execPath, ['--expose-gc', path.join(ROOT, 'tests', 'bench', 'run.cjs'), '--quick', '--iterations', '1', '--out', out], { encoding: 'utf8' });
  assert.equal(r.status, 0, r.stderr);
  assert.match(r.stdout, /bench: \d+ measured, 2 not-run/);
  const receipt = JSON.parse(fs.readFileSync(out, 'utf8'));
  assert.equal(receipt.quick, true);
  assert.ok(receipt.workloads.find((w) => w.id === 'memory.parsedDocument.200').workload.gcAvailable);
  fs.rmSync(path.dirname(out), { recursive: true, force: true });
});
