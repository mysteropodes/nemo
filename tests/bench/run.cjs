#!/usr/bin/env node
'use strict';
// Nemo bench workloads — R03 initial measurements.
//
// Records what a workload costs on THIS machine for THIS source revision, in
// a receipt that names the workload, hardware, backend and revision. It sets
// no budget: thresholds are R19's job, and a number without its context is
// worthless (CLAUDE.md §5bis on why rAF-based probes mislead).
//
//   evaluation.*   the real motion.js, loaded whole in a vm sandbox
//                  (tests/fixtures/lib/sandbox.cjs, which first installs the
//                  production modules src/index.html loads before it — the
//                  R08 easing kernel src/js/animation/curve.js when the tree
//                  has it): valueAtFrame over the keyed-props fixture,
//                  evalCurvePoints, expression evaluation over the
//                  expression-props fixture. The backend string names every
//                  module that ran, so receipts from before and after the
//                  extraction are distinguishable.
//   copy.undoClone the PRODUCTION undo snapshot (tweens.js _cloneLayersForUndo,
//                  lifted) of a workload document's layers; copy.jsonClone is
//                  the plain JSON round trip it is built on, copy.serialize the
//                  stringify alone
//   memory.*       heap held by a parsed workload document, measured in an
//                  isolated child process (own heap, --expose-gc, nothing else
//                  alive) so the number does not depend on what ran before
//   render.* / export.*  declared with the export fixture, recorded `not-run`:
//                  they need the WebGPU engine in a browser or the packaged app
//                  (test:browser / test:desktop, R21).
//
// Workload documents come from the fixture corpus (tests/fixtures/lib/corpus.cjs
// benchDocument with the parameters of tests/fixtures/generate.cjs); the run
// refuses a document whose SHA-256 is not the one tests/fixtures/manifest.json
// pins, so every number is bound to byte-identical input.
//
// Usage: node --expose-gc tests/bench/run.cjs [--json] [--out file]
//        [--iterations N] [--quick]
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { spawnSync } = require('node:child_process');
const { performance } = require('node:perf_hooks');

const ROOT = path.resolve(__dirname, '..', '..');
const { sourceIdentity, platformIdentity } = require(path.join(ROOT, 'scripts', 'nemo', 'lib', 'identity.cjs'));
const corpus = require('../fixtures/lib/corpus.cjs');
const gen = require('../fixtures/generate.cjs');
const sandbox = require('../fixtures/lib/sandbox.cjs');

const SCHEMA = 'nemo.bench/1';
const FIXTURES = path.join(ROOT, 'tests', 'fixtures');
const RENDER_FIXTURE = 'export';
const QUICK_WORKLOAD = 'bench-vectors-8x24';

const sha256 = (s) => crypto.createHash('sha256').update(s).digest('hex');
const readJson = (p) => JSON.parse(fs.readFileSync(p, 'utf8'));
const manifest = () => readJson(path.join(FIXTURES, 'manifest.json'));

function stats(samples, unit) {
  const s = samples.slice().sort((a, b) => a - b);
  const at = (q) => s[Math.min(s.length - 1, Math.floor(q * (s.length - 1)))];
  const mean = s.reduce((a, b) => a + b, 0) / s.length;
  const round = (v) => Math.round(v * 1000) / 1000;
  return { n: s.length, unit, median: round(at(0.5)), p90: round(at(0.9)), min: round(s[0]), max: round(s[s.length - 1]), mean: round(mean) };
}
function timed(fn, iterations) {
  const out = [];
  for (let i = 0; i < iterations; i++) { const t0 = performance.now(); fn(); out.push(performance.now() - t0); }
  return out;
}
function gcNow() { if (typeof global.gc === 'function') { global.gc(); global.gc(); return true; } return false; }
function strokeCount(doc) { let n = 0; for (const ld of doc.layers) for (const fr of ld.frames) n += fr.strokes.length; return n; }

// The manifest is the only authority on what a workload document is: a run
// against a document it does not pin, or whose bytes differ, is not a receipt.
function verifyWorkloadJson(id, json) {
  const entry = manifest().workloads.find((w) => w.id === id);
  if (!entry) throw new Error('workload document ' + id + ' is not pinned in tests/fixtures/manifest.json');
  const sha = sha256(json);
  if (sha !== entry.sha256) throw new Error('workload document ' + id + ' differs from the manifest: expected sha256 ' + entry.sha256 + ', got ' + sha + ' (regenerate with npm run fixtures if the generator changed)');
  return { entry, sha };
}
function buildWorkload(id) {
  const params = gen.WORKLOAD_DOCS[id];
  if (!params) throw new Error('unknown workload document ' + id);
  const doc = corpus.benchDocument(params);
  const json = JSON.stringify(doc);
  const { sha } = verifyWorkloadJson(id, json);
  return { doc, json, sha, params, identity: { document: id, layers: doc.layers.length, totalFrames: doc.totalFrames, strokes: strokeCount(doc), bytes: json.length, seed: params.seed, sha256: sha } };
}

// Heap held by the parsed document, in a fresh process: only the JSON string
// is alive at the baseline; the parsed tree is the only allocation between
// the two readings. Repeated and reordered runs give the same number.
function measureMemory(id) {
  const r = spawnSync(process.execPath, ['--expose-gc', __filename, '--measure-memory', id], { encoding: 'utf8', timeout: 5 * 60 * 1000 });
  if (r.status !== 0) throw new Error('memory measurement for ' + id + ' failed: ' + (r.stderr || 'exit ' + r.status));
  return JSON.parse(r.stdout);
}
function measureMemoryHere(id) {
  const built = buildWorkload(id);
  const json = built.json;
  built.doc = null;
  const gcAvailable = gcNow();
  const before = process.memoryUsage().heapUsed;
  const held = JSON.parse(json);
  gcNow();
  const after = process.memoryUsage().heapUsed;
  if (!held.layers) throw new Error('unreachable');
  return { id, gcAvailable, before, after, delta: Math.max(0, after - before), identity: built.identity };
}

// The evaluation backend names every production module the sandbox ran, in
// load order (sandbox.cjs MOTION_PRELUDE + motion.js), so a receipt taken
// before the R08 extraction ('src/js/motion.js') and one taken after
// ('src/js/animation/curve.js + src/js/motion.js') are never confused.
function evaluationBackend(motion) {
  return 'node:' + motion.modules.map((f) => 'src/js/' + f).join(' + ') + ' (whole modules in a vm sandbox, tests/fixtures/lib/sandbox.cjs)';
}

function loadProject(id) {
  const project = readJson(path.join(FIXTURES, id, 'project.json'));
  const state = sandbox.defaultState();
  state.layers = project.layers; state.fps = project.fps; state.totalFrames = project.totalFrames; state.canvasW = project.canvasW; state.canvasH = project.canvasH;
  return { project, state, motion: sandbox.loadMotion(state) };
}

function runBench(opts) {
  opts = opts || {};
  const quick = !!opts.quick;
  const iterations = opts.iterations || (quick ? 2 : 5);
  const docIds = opts.workloads || (quick ? [QUICK_WORKLOAD] : Object.keys(gen.WORKLOAD_DOCS));
  const workloads = [];
  let EVAL_BACKEND = null;
  const UNDO_BACKEND = 'node:tweens.js _cloneLayersForUndo (lifted, tests/fixtures/lib/sandbox.cjs)';

  // ---- evaluation ---------------------------------------------------------
  {
    const { state, motion } = loadProject('keyed-props');
    const { SMMotion } = motion;
    EVAL_BACKEND = evaluationBackend(motion);
    const evaluator = motion.SMAnimationCurve ? 'SMAnimationCurve.evalCurvePoints (src/js/animation/curve.js)' : 'evalCurvePoints declared in src/js/motion.js';
    const ld = state.layers[0]; // "Default ease": position, rotation, scale and opacity keyed
    const synthetic = corpus.helpers.layer('ly_bench_synthetic', 'Synthetic 200 keys', corpus.helpers.frames(state.totalFrames), {
      motion: { position: corpus.helpers.keys(Array.from({ length: 200 }, (_, i) => ({ frame: i * 3, v: [i * 7, (i % 5) * 11] }))) },
    });
    state.layers.push(synthetic);
    const framesPer = quick ? 48 : 600;
    let evaluations = 0;
    const samples = timed(() => {
      for (let f = 0; f < framesPer; f++) {
        SMMotion.valueAtFrame(ld, 'position', f); SMMotion.valueAtFrame(ld, 'rotation', f); SMMotion.valueAtFrame(ld, 'scale', f); SMMotion.valueAtFrame(ld, 'opacity', f);
        SMMotion.valueAtFrame(synthetic, 'position', f);
        evaluations += 5;
      }
    }, iterations);
    workloads.push({ id: 'evaluation.valueAtFrame', kind: 'evaluation', status: 'ran', fixture: 'keyed-props', backend: EVAL_BACKEND, workload: { framesPerIteration: framesPer, propertiesPerFrame: 5, keysInSyntheticTrack: 200, evaluations, evaluator }, stats: stats(samples.map((ms) => (ms * 1e6) / (framesPer * 5)), 'ns/evaluation') });

    const curve = SMMotion.DEFAULT_CURVE();
    const n = quick ? 5000 : 100000;
    const curveSamples = timed(() => { let acc = 0; for (let i = 0; i < n; i++) acc += SMMotion.evalCurvePoints(curve, i / n); if (acc < 0) throw new Error('unreachable'); }, iterations);
    workloads.push({ id: 'evaluation.evalCurvePoints', kind: 'evaluation', status: 'ran', fixture: null, backend: EVAL_BACKEND, workload: { evaluationsPerIteration: n, curve: 'DEFAULT_CURVE', evaluator }, stats: stats(curveSamples.map((ms) => (ms * 1e6) / n), 'ns/evaluation') });
  }
  {
    const { project, state, motion } = loadProject('expression-props');
    if (evaluationBackend(motion) !== EVAL_BACKEND) throw new Error('the sandbox loaded different modules for two projects: ' + evaluationBackend(motion) + ' vs ' + EVAL_BACKEND);
    const le = state.layers[project.layers.findIndex((l) => l.layerUid === 'ly_expr_e')];
    const framesPer = quick ? 48 : 600;
    const samples = timed(() => { for (let f = 0; f < framesPer; f++) { motion.SMMotion.valueAtFrame(le, 'rotation', f); motion.SMMotion.valueAtFrame(le, 'position', f); } }, iterations);
    workloads.push({ id: 'evaluation.expression', kind: 'evaluation', status: 'ran', fixture: 'expression-props', backend: EVAL_BACKEND, workload: { framesPerIteration: framesPer, expressionsPerFrame: 2, expressions: ['frame * 3', '[value[0] + frame * 2, value[1]]'] }, stats: stats(samples.map((ms) => (ms * 1e6) / (framesPer * 2)), 'ns/evaluation') });
  }

  // ---- copy / memory over the workload documents ----------------------------
  const undo = sandbox.loadUndoClone();
  for (const id of docIds) {
    const { doc, identity } = buildWorkload(id);
    const undoSamples = timed(() => { undo.cloneLayersForUndo(doc.layers); }, iterations);
    workloads.push({ id: 'copy.undoClone.' + id, kind: 'copy', status: 'ran', fixture: id, backend: UNDO_BACKEND, workload: Object.assign({ operation: '_cloneLayersForUndo(doc.layers)', heavyFieldsDetached: undo.heavyFields }, identity), stats: stats(undoSamples, 'ms') });
    const jsonSamples = timed(() => { JSON.parse(JSON.stringify(doc)); }, iterations);
    workloads.push({ id: 'copy.jsonClone.' + id, kind: 'copy', status: 'ran', fixture: id, backend: 'node:JSON', workload: Object.assign({ operation: 'JSON.parse(JSON.stringify(doc))' }, identity), stats: stats(jsonSamples, 'ms') });
    const ser = timed(() => { JSON.stringify(doc); }, iterations);
    workloads.push({ id: 'copy.serialize.' + id, kind: 'copy', status: 'ran', fixture: id, backend: 'node:JSON', workload: Object.assign({ operation: 'JSON.stringify(doc)' }, identity), stats: stats(ser, 'ms') });
    const mem = measureMemory(id);
    workloads.push({ id: 'memory.parsedDocument.' + id, kind: 'memory', status: 'ran', fixture: id, backend: 'node:V8 heap (isolated child process)', workload: Object.assign({ gcAvailable: mem.gcAvailable, heapBefore: mem.before, heapAfter: mem.after }, identity), stats: { n: 1, unit: 'bytes', median: mem.delta, p90: mem.delta, min: mem.delta, max: mem.delta, mean: mem.delta, bytesPerStroke: Math.round(mem.delta / identity.strokes) } });
  }

  // ---- render / export: declared, not runnable here --------------------------
  const exportFx = readJson(path.join(FIXTURES, RENDER_FIXTURE, 'project.json'));
  const reason = 'requires the WebGPU engine in a browser or the packaged desktop app (test:browser / test:desktop, R21); the fixture and its expectations are in tests/fixtures/' + RENDER_FIXTURE + '/';
  workloads.push({ id: 'render.engine.' + RENDER_FIXTURE, kind: 'render', status: 'not-run', fixture: RENDER_FIXTURE, backend: null, reason, workload: { frames: exportFx.totalFrames, width: exportFx.canvasW, height: exportFx.canvasH } });
  workloads.push({ id: 'export.mp4.' + RENDER_FIXTURE, kind: 'export', status: 'not-run', fixture: RENDER_FIXTURE, backend: null, reason, workload: { encoder: 'h264_videotoolbox (export.js)', frames: exportFx.totalFrames, width: exportFx.canvasW, height: exportFx.canvasH, fps: exportFx.fps } });

  return {
    schema: SCHEMA, generatedAt: new Date().toISOString(), quick, iterations,
    source: sourceIdentity(), platform: platformIdentity(), node: process.version, v8: process.versions.v8,
    backends: { evaluation: EVAL_BACKEND, undoClone: UNDO_BACKEND, copy: 'node:JSON', memory: 'node:V8 heap (isolated child process)', render: null, export: null },
    budgets: null, // deliberately none: R19 sets budgets from these receipts, not the other way round
    workloads,
  };
}

function main() {
  const args = process.argv.slice(2);
  const arg = (k) => { const i = args.indexOf(k); return i >= 0 ? args[i + 1] : null; };
  if (arg('--measure-memory')) { process.stdout.write(JSON.stringify(measureMemoryHere(arg('--measure-memory'))) + '\n'); return; }
  const receipt = runBench({ quick: args.includes('--quick'), iterations: arg('--iterations') ? parseInt(arg('--iterations'), 10) : undefined });
  const out = arg('--out');
  if (out) { fs.mkdirSync(path.dirname(out), { recursive: true }); fs.writeFileSync(out, JSON.stringify(receipt, null, 1) + '\n'); }
  if (args.includes('--json')) { process.stdout.write(JSON.stringify(receipt, null, 1) + '\n'); return; }
  const ran = receipt.workloads.filter((w) => w.status === 'ran');
  for (const w of receipt.workloads) console.log(w.status === 'ran' ? `  ${w.id.padEnd(44)} median ${w.stats.median} ${w.stats.unit}` : `  ${w.id.padEnd(44)} ${w.status}: ${w.reason}`);
  console.log(`bench: ${ran.length} measured, ${receipt.workloads.length - ran.length} not-run; ${receipt.source.head.slice(0, 12)} on ${receipt.platform.cpuModel || receipt.platform.arch}` + (out ? `; receipt ${path.relative(ROOT, out)}` : ''));
}

if (require.main === module) main();
module.exports = { runBench, measureMemory, verifyWorkloadJson, buildWorkload, evaluationBackend, SCHEMA, QUICK_WORKLOAD };
