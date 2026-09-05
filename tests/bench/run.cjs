#!/usr/bin/env node
'use strict';
// Nemo bench workloads — R03 initial measurements.
//
// Records what a workload costs on THIS machine for THIS source revision, in
// a receipt that names the workload, hardware, backend and revision. It sets
// no budget: thresholds are R19's job, and a number without its context is
// worthless (CLAUDE.md §5bis on why rAF-based probes mislead).
//
//   evaluation.*   the real Motion evaluator (tests/bench/motion-eval.cjs)
//   copy.*         undo/serialize clones of generated scale documents
//   memory.*       heap held by a parsed scale document (needs --expose-gc for
//                  stable numbers; recorded as gcAvailable:false otherwise)
//   render.* / export.*  declared with their fixture, recorded `not-run`: they
//                  need the WebGPU engine in a browser or the packaged app
//                  (test:browser / test:desktop, R21).
//
// Usage: node --expose-gc tests/bench/run.cjs [--json] [--out file]
//        [--iterations N] [--quick]
const fs = require('node:fs');
const path = require('node:path');
const { performance } = require('node:perf_hooks');

const ROOT = path.resolve(__dirname, '..', '..');
const { sourceIdentity, platformIdentity } = require(path.join(ROOT, 'scripts', 'nemo', 'lib', 'identity.cjs'));
const lib = require('../fixtures/lib.cjs');
const gen = require('../fixtures/generate.cjs');
const motionEval = require('./motion-eval.cjs');

const SCHEMA = 'nemo.bench/1';
const RENDER_FIXTURE = 'export-12f-320x240';

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
function gcNow() { if (typeof global.gc === 'function') { global.gc(); return true; } return false; }

function runBench(opts) {
  opts = opts || {};
  const quick = !!opts.quick;
  const iterations = opts.iterations || (quick ? 2 : 5);
  const sizes = opts.sizes || (quick ? [200] : [500, 2000, 4000]);
  const workloads = [];
  const api = motionEval.load();

  // ---- evaluation ---------------------------------------------------------
  {
    const fx = gen.buildFixture('keyed-position-default-ease');
    const ld = fx.layers[0];
    const synthetic = { motion: { position: lib.track(Array.from({ length: 200 }, (_, i) => ({ frame: i * 3, v: [i * 7, (i % 5) * 11] }))) } };
    const framesPer = quick ? 48 : 600;
    let evaluations = 0;
    const samples = timed(() => {
      for (let f = 0; f < framesPer; f++) {
        api.rawValueAtFrame(ld, 'position', f); api.rawValueAtFrame(ld, 'rotation', f); api.rawValueAtFrame(ld, 'scale', f); api.rawValueAtFrame(ld, 'opacity', f);
        api.rawValueAtFrame(synthetic, 'position', f);
        evaluations += 5;
      }
    }, iterations);
    const perEval = samples.map((ms) => (ms * 1e6) / (framesPer * 5));
    workloads.push({ id: 'evaluation.rawValueAtFrame', kind: 'evaluation', status: 'ran', fixture: 'keyed-position-default-ease', backend: 'node:motion.js(extracted)', workload: { framesPerIteration: framesPer, propertiesPerFrame: 5, keysInSyntheticTrack: 200, evaluations }, stats: stats(perEval, 'ns/evaluation') });
  }
  {
    const n = quick ? 5000 : 100000;
    const samples = timed(() => { let acc = 0; for (let i = 0; i < n; i++) acc += api.evalCurvePoints(api.DEFAULT_CURVE, i / n); if (acc < 0) throw new Error('unreachable'); }, iterations);
    workloads.push({ id: 'evaluation.evalCurvePoints', kind: 'evaluation', status: 'ran', fixture: null, backend: 'node:motion.js(extracted)', workload: { evaluationsPerIteration: n, curve: 'DEFAULT_CURVE' }, stats: stats(samples.map((ms) => (ms * 1e6) / n), 'ns/evaluation') });
  }

  // ---- copy / memory over generated scale documents ------------------------
  for (const n of sizes) {
    const entry = gen.GENERATED_SCALE.find((g) => g.strokes === n);
    const seed = entry ? entry.seed : 9000 + n;
    const d = lib.scaleDocument(n, seed);
    const json = JSON.stringify(d);
    const sha = lib.sha256(json);
    if (entry && entry.sha256 && entry.sha256 !== sha) throw new Error('scale document ' + n + ' is not deterministic: expected ' + entry.sha256 + ' got ' + sha);
    const undo = timed(() => { JSON.parse(JSON.stringify(d)); }, iterations);
    workloads.push({ id: 'copy.undoClone.' + n, kind: 'copy', status: 'ran', fixture: 'scale-' + n, backend: 'node:JSON', workload: { strokes: n, bytes: json.length, seed, sha256: sha, operation: 'JSON.parse(JSON.stringify(doc))' }, stats: stats(undo, 'ms') });
    const ser = timed(() => { JSON.stringify(d); }, iterations);
    workloads.push({ id: 'copy.serialize.' + n, kind: 'copy', status: 'ran', fixture: 'scale-' + n, backend: 'node:JSON', workload: { strokes: n, bytes: json.length, operation: 'JSON.stringify(doc)' }, stats: stats(ser, 'ms') });
    const gcAvailable = gcNow();
    const before = process.memoryUsage().heapUsed;
    const held = JSON.parse(json);
    gcNow();
    const after = process.memoryUsage().heapUsed;
    const delta = Math.max(0, after - before);
    if (!held.layers) throw new Error('unreachable');
    workloads.push({ id: 'memory.parsedDocument.' + n, kind: 'memory', status: 'ran', fixture: 'scale-' + n, backend: 'node:V8 heap', workload: { strokes: n, bytes: json.length, gcAvailable }, stats: { n: 1, unit: 'bytes', median: delta, p90: delta, min: delta, max: delta, mean: delta, bytesPerStroke: Math.round(delta / n) } });
  }

  // ---- render / export: declared, not runnable here --------------------------
  const reason = 'requires the WebGPU engine in a browser or the packaged desktop app (test:browser / test:desktop, R21); the fixture and its expectations are in tests/fixtures/manifest.json';
  workloads.push({ id: 'render.engine.' + RENDER_FIXTURE, kind: 'render', status: 'not-run', fixture: RENDER_FIXTURE, backend: null, reason, workload: { frames: 12, width: 320, height: 240 } });
  workloads.push({ id: 'export.mp4.' + RENDER_FIXTURE, kind: 'export', status: 'not-run', fixture: RENDER_FIXTURE, backend: null, reason, workload: { encoder: 'h264_videotoolbox (export.js)', frames: 12, width: 320, height: 240, fps: 12 } });

  return {
    schema: SCHEMA, generatedAt: new Date().toISOString(), quick, iterations,
    source: sourceIdentity(), platform: platformIdentity(), node: process.version, v8: process.versions.v8,
    backends: { evaluation: 'node:motion.js(extracted via tests/bench/motion-eval.cjs)', copy: 'node:JSON', memory: 'node:V8 heap', render: null, export: null },
    budgets: null, // deliberately none: R19 sets budgets from these receipts, not the other way round
    workloads,
  };
}

function main() {
  const args = process.argv.slice(2);
  const arg = (k) => { const i = args.indexOf(k); return i >= 0 ? args[i + 1] : null; };
  const receipt = runBench({ quick: args.includes('--quick'), iterations: arg('--iterations') ? parseInt(arg('--iterations'), 10) : undefined });
  const out = arg('--out');
  if (out) { fs.mkdirSync(path.dirname(out), { recursive: true }); fs.writeFileSync(out, JSON.stringify(receipt, null, 1) + '\n'); }
  if (args.includes('--json')) { process.stdout.write(JSON.stringify(receipt, null, 1) + '\n'); return; }
  const ran = receipt.workloads.filter((w) => w.status === 'ran');
  for (const w of receipt.workloads) console.log(w.status === 'ran' ? `  ${w.id.padEnd(36)} median ${w.stats.median} ${w.stats.unit}` : `  ${w.id.padEnd(36)} ${w.status}: ${w.reason}`);
  console.log(`bench: ${ran.length} measured, ${receipt.workloads.length - ran.length} not-run; ${receipt.source.head.slice(0, 12)} on ${receipt.platform.cpuModel || receipt.platform.arch}` + (out ? `; receipt ${path.relative(ROOT, out)}` : ''));
}

if (require.main === module) main();
module.exports = { runBench, SCHEMA };
