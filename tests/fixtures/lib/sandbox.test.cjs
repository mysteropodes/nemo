'use strict';
// The fixture VM (sandbox.cjs) must install production modules in the order
// src/index.html does. R08 (#949) moved Motion's easing kernel to
// src/js/animation/curve.js and motion.js now binds `evalCurvePoints` to the
// installed `SMAnimationCurve` as its IIFE runs; a sandbox that ran motion.js
// alone died on that line (ReferenceError: SMAnimationCurve is not defined),
// which took 12 of 17 fixture checks and both bench evaluation tests with it
// on the combined #946+#949 candidate. These tests pin the loader contract on
// BOTH sides of the extraction, so the R03 harness holds whichever PR lands
// first: the prelude is installed when the tree has it, the facade member is
// the extracted kernel (not a copy), a keyed track really invokes it, and the
// values it produces match the independent expectations of reference.cjs.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');

const sandbox = require('./sandbox.cjs');
const ref = require('./reference.cjs');
const corpus = require('./corpus.cjs');

const SRC = path.join(sandbox.ROOT, 'src', 'js');
const CURVE_MODULE = path.join(SRC, 'animation', 'curve.js');
const extracted = fs.existsSync(CURVE_MODULE);
const motionSource = fs.readFileSync(path.join(SRC, 'motion.js'), 'utf8');
const clone = (x) => JSON.parse(JSON.stringify(x));
function near(actual, expect, tol, msg) {
  assert.equal(actual.length, expect.length, msg + ': dimension');
  for (let i = 0; i < expect.length; i++) assert.ok(Math.abs(actual[i] - expect[i]) <= tol, `${msg}: [${i}] got ${actual[i]}, expected ${expect[i]} (±${tol})`);
}

// Position keyed with the default ease (no curvePoints on the key): frames 2,
// 4 and 6 sit on the default curve's on-curve waypoints (t = 0.25, 0.5, 0.75),
// where the eased fraction is known without evaluating any spline.
const KEYS = [{ frame: 0, v: [0, 0] }, { frame: 8, v: [80, 40] }];
function keyedLayer() {
  return corpus.helpers.layer('ly_sandbox_keyed', 'Sandbox keyed', corpus.helpers.frames(24), { motion: { position: corpus.helpers.keys(KEYS) } });
}
// Manual tangents (1, 0) at both ends make the segment the smoothstep
// polynomial 3t² − 2t³: an independent closed form for any evaluator.
const SMOOTH = [{ x: 0, y: 0, tx: 1, ty: 0 }, { x: 1, y: 1, tx: 1, ty: 0 }];

test('sandbox: the production prelude precedes motion.js exactly as src/index.html loads it', () => {
  const html = fs.readFileSync(path.join(sandbox.ROOT, 'src', 'index.html'), 'utf8');
  const scripts = Array.from(html.matchAll(/<script src="js\/([^"]+)"><\/script>/g), (m) => m[1]);
  const motionAt = scripts.indexOf('motion.js');
  assert.ok(motionAt >= 0, 'index.html loads js/motion.js');
  for (const file of sandbox.MOTION_PRELUDE) {
    const onDisk = fs.existsSync(path.join(SRC, file));
    const at = scripts.indexOf(file);
    assert.equal(at >= 0, onDisk, `${file}: script tag present iff the module exists`);
    if (onDisk) assert.ok(at < motionAt, `${file} is loaded before motion.js (index.html position ${at} vs ${motionAt})`);
  }
  const m = sandbox.loadMotion();
  assert.equal(typeof m.SMMotion.evalCurvePoints, 'function');
  assert.equal(m.modules[m.modules.length - 1], 'motion.js');
  if (extracted) {
    assert.deepEqual(m.modules, ['animation/curve.js', 'motion.js']);
    assert.equal(typeof m.SMAnimationCurve.evalCurvePoints, 'function', 'the kernel is installed in the VM');
    assert.equal(m.SMMotion.evalCurvePoints, m.SMAnimationCurve.evalCurvePoints, 'the facade member is the extracted kernel itself, not a copy');
    assert.doesNotMatch(motionSource, /function evalCurvePoints\(/, 'after the extraction motion.js no longer declares the evaluator');
  } else {
    assert.deepEqual(m.modules, ['motion.js']);
    assert.equal(m.SMAnimationCurve, null);
    assert.match(motionSource, /function evalCurvePoints\(/, 'before the extraction motion.js declares the evaluator itself');
  }
});

test('sandbox: the evaluator the facade exposes matches the independent expectations, before or after the extraction', () => {
  const { SMMotion } = sandbox.loadMotion();
  const curve = SMMotion.DEFAULT_CURVE();
  for (const t of Object.keys(ref.DEFAULT_EASE_WAYPOINTS).map(Number)) assert.ok(Math.abs(SMMotion.evalCurvePoints(curve, t) - ref.defaultEaseAt(t)) <= 1e-9, `default ease at ${t}`);
  for (let i = 0; i <= 64; i++) { const t = i / 64; assert.ok(Math.abs(SMMotion.evalCurvePoints(SMOOTH, t) - t * t * (3 - 2 * t)) <= 1e-9, `smoothstep at ${t}`); }
  // Two waypoints without tangents still go through the Newton solve: equal to 1e-9, not bit-exact.
  for (const t of [0.1, 0.3, 0.5, 0.9]) assert.ok(Math.abs(SMMotion.evalCurvePoints(ref.LINEAR_CURVE, t) - ref.linearEaseAt(t)) <= 1e-9, `linear at ${t}`);
});

test('sandbox: the kernel installed in the VM is the same production file Node imports', { skip: !extracted && 'src/js/animation/curve.js is absent (before the R08 extraction)' }, () => {
  const direct = require(CURVE_MODULE).evalCurvePoints;
  const { SMAnimationCurve } = sandbox.loadMotion();
  const curves = { default: corpus.DEFAULT_CURVE, smooth: SMOOTH, peak: [{ x: 0, y: 0 }, { x: 0.5, y: 1 }, { x: 1, y: 0 }] };
  for (const [name, pts] of Object.entries(curves)) {
    for (let i = 0; i <= 64; i++) { const x = i / 64; assert.equal(SMAnimationCurve.evalCurvePoints(clone(pts), x), direct(pts, x), `${name} at ${x}`); }
  }
});

test('sandbox: valueAtFrame on a keyed track invokes the extracted evaluator with the key curve and the segment fraction', { skip: !extracted && 'src/js/animation/curve.js is absent (before the R08 extraction)' }, () => {
  const calls = [];
  const m = sandbox.loadMotion(undefined, {
    beforeMotion(sb) {
      // Wrap the kernel BEFORE motion.js binds to it: whatever motion.js
      // captures at load is what its track evaluation calls at run time.
      const kernel = sb.SMAnimationCurve.evalCurvePoints;
      sb.SMAnimationCurve.evalCurvePoints = function (pts, x) { calls.push({ pts: clone(pts), x }); return kernel(pts, x); };
    },
  });
  const ld = keyedLayer();
  m.state.layers.push(ld);
  near(Array.from(m.SMMotion.valueAtFrame(ld, 'position', 0)), KEYS[0].v, 0, 'on the first key');
  near(Array.from(m.SMMotion.valueAtFrame(ld, 'position', 8)), KEYS[1].v, 0, 'on the last key');
  assert.equal(calls.length, 0, 'a frame on a key does not consult the curve');
  for (const frame of [2, 4, 6]) {
    const got = Array.from(m.SMMotion.valueAtFrame(ld, 'position', frame));
    near(got, ref.trackValueAt(KEYS, frame, ref.defaultEaseAt), 1e-9, `position@${frame} (independent: default-ease waypoint)`);
  }
  assert.deepEqual(calls.map((c) => c.x), [0.25, 0.5, 0.75], 'one kernel call per evaluated frame, at the segment fraction');
  for (const c of calls) assert.deepEqual(c.pts, corpus.DEFAULT_CURVE, 'a key without curvePoints evaluates DEFAULT_CURVE');
  ld.motion.position.keys[0].curvePoints = clone(SMOOTH);
  calls.length = 0;
  const got = Array.from(m.SMMotion.valueAtFrame(ld, 'position', 2));
  near(got, ref.lerpVec(KEYS[0].v, KEYS[1].v, 0.25 * 0.25 * (3 - 2 * 0.25)), 1e-9, 'position@2 with manual tangents (independent: smoothstep)');
  assert.deepEqual(calls, [{ pts: SMOOTH, x: 0.25 }], 'the edited key curve reaches the kernel unchanged');
});

test('sandbox: motion.js run alone on an extracted tree (the original failure) fails loudly with the loader-order explanation', { skip: !extracted && 'src/js/animation/curve.js is absent (before the R08 extraction)' }, () => {
  assert.throws(() => sandbox.loadMotion(undefined, { prelude: [] }), (e) => /animation\/curve\.js/.test(e.message) && /SMAnimationCurve is not defined/.test(e.message) && /src\/index\.html loader order/.test(e.message));
  assert.deepEqual(sandbox.loadMotion(undefined, { prelude: sandbox.MOTION_PRELUDE }).modules, ['animation/curve.js', 'motion.js']);
});
