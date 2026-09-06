// Easing solvers vs an independent cubic-bezier reference.
//
// Nemo evaluates "ease" in THREE separate places, each written for its own
// context and none of them tested until now:
//   - camera.js  bezierEase()      — Newton, then bisection as a fallback
//   - text-selector.js bezierEaser()— pure bisection (24 steps)
//   - animation/curve.js evalCurvePoints() — the ease-curve editor's multi-point
//                                    spline, which reduces to a plain cubic
//                                    bezier when the two ends carry explicit
//                                    tangents
// They must all agree with the CSS `cubic-bezier()` definition, and with each
// other: a camera move, a text animator and a keyframe pair set to the same
// curve are supposed to accelerate identically. This is the cheap guard for
// that (the idea comes from MotionStudio's own timeline spec, which pins its
// solver against CSS with a stated tolerance rather than eyeballing curves).
//
// The reference below is deliberately dumb and slow — 200 bisection steps on
// the exact polynomial — so it owes nothing to the implementations it judges.
const assert = require('node:assert/strict');
const test = require('node:test');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const root = path.join(__dirname, '..');

// ---- production solvers (legacy extraction until their modules export) -----
function extract(file, startMarker, exportName, extraMarkers) {
  const source = fs.readFileSync(path.join(root, file), 'utf8');
  const pieces = [];
  for (const marker of (extraMarkers || []).concat([startMarker])) {
    const start = source.indexOf(marker);
    if (start === -1) throw new Error(`${marker} not found in ${file}`);
    // Functions here are indented inside a module IIFE; the closing brace sits
    // at the same indentation as the `function` keyword.
    const indent = source.slice(source.lastIndexOf('\n', start) + 1, start);
    const end = source.indexOf(`\n${indent}}`, start) + indent.length + 2;
    pieces.push(source.slice(start, end));
  }
  const sandbox = {};
  vm.runInNewContext(`${pieces.join('\n')}\nthis.out = ${exportName};`, sandbox);
  return sandbox.out;
}

const bezierEase = extract('src/js/camera.js', 'function bezierEase(', 'bezierEase');
const bezierEaser = extract('src/js/text-selector.js', 'function bezierEaser(', 'bezierEaser');
const { evalCurvePoints } = require('../src/js/animation/curve.js');

// ---- independent reference ------------------------------------------------
// CSS cubic-bezier(x1,y1,x2,y2): P0=(0,0), P3=(1,1), solve x(t)=input, return y(t).
function reference(x1, y1, x2, y2) {
  const at = (t, a, b) => {
    const u = 1 - t;
    return 3 * u * u * t * a + 3 * u * t * t * b + t * t * t;
  };
  return (x) => {
    if (x <= 0) return 0;
    if (x >= 1) return 1;
    let lo = 0, hi = 1, t = x;
    for (let i = 0; i < 200; i++) {
      t = (lo + hi) / 2;
      if (at(t, x1, x2) < x) lo = t; else hi = t;
    }
    return at(t, y1, y2);
  };
}

// The ease-curve editor stores points with explicit tangents; these are the
// tangents whose thirds land exactly on the CSS control points, so the same
// curve can be handed to evalCurvePoints.
function curvePointsFor(x1, y1, x2, y2) {
  return [
    { x: 0, y: 0, tx: 3 * x1, ty: 3 * y1 },
    { x: 1, y: 1, tx: 3 * (1 - x2), ty: 3 * (1 - y2) },
  ];
}

const CASES = [
  { name: 'linear', h: [0, 0, 1, 1] },
  { name: 'ease (CSS default)', h: [0.25, 0.1, 0.25, 1] },
  { name: 'ease-in', h: [0.42, 0, 1, 1] },
  { name: 'ease-out', h: [0, 0, 0.58, 1] },
  { name: 'ease-in-out', h: [0.42, 0, 0.58, 1] },
  { name: 'near-vertical handles', h: [0.9, 0, 0.1, 1] },
  { name: 'overshoot (back)', h: [0.68, -0.55, 0.27, 1.55] },
  { name: 'flat middle', h: [0.99, 0.01, 0.01, 0.99] },
];

const SAMPLES = Array.from({ length: 101 }, (_, i) => i / 100);
const TOL = 1e-5; // MotionStudio's own stated tolerance against CSS

function worstError(fn, ref) {
  let worst = 0, at = null;
  for (const x of SAMPLES) {
    const d = Math.abs(fn(x) - ref(x));
    if (d > worst) { worst = d; at = x; }
  }
  return { worst, at };
}

test('camera.js bezierEase matches the CSS cubic-bezier reference', () => {
  for (const c of CASES) {
    const ref = reference(...c.h);
    const { worst, at } = worstError((x) => bezierEase(x, ...c.h), ref);
    assert.ok(worst <= TOL, `${c.name}: off by ${worst} at x=${at}`);
  }
});

test('text-selector.js bezierEaser matches the CSS cubic-bezier reference', () => {
  for (const c of CASES) {
    const ref = reference(...c.h);
    const { worst, at } = worstError(bezierEaser(...c.h), ref);
    assert.ok(worst <= TOL, `${c.name}: off by ${worst} at x=${at}`);
  }
});

test('animation/curve.js evalCurvePoints matches the reference on a single-segment curve', () => {
  for (const c of CASES) {
    const ref = reference(...c.h);
    const pts = curvePointsFor(...c.h);
    const { worst, at } = worstError((x) => evalCurvePoints(pts, x), ref);
    assert.ok(worst <= TOL, `${c.name}: off by ${worst} at x=${at}`);
  }
});

test('the three solvers agree with each other', () => {
  for (const c of CASES) {
    const pts = curvePointsFor(...c.h);
    const easer = bezierEaser(...c.h);
    for (const x of SAMPLES) {
      const a = bezierEase(x, ...c.h), b = easer(x), d = evalCurvePoints(pts, x);
      assert.ok(Math.abs(a - b) <= 2 * TOL, `${c.name} @${x}: camera ${a} vs selector ${b}`);
      assert.ok(Math.abs(a - d) <= 2 * TOL, `${c.name} @${x}: camera ${a} vs curve editor ${d}`);
    }
  }
});

test('every solver pins the endpoints and stays finite', () => {
  for (const c of CASES) {
    const pts = curvePointsFor(...c.h);
    const easer = bezierEaser(...c.h);
    for (const [label, v0, v1] of [
      ['camera', bezierEase(0, ...c.h), bezierEase(1, ...c.h)],
      ['selector', easer(0), easer(1)],
      ['curve editor', evalCurvePoints(pts, 0), evalCurvePoints(pts, 1)],
    ]) {
      assert.ok(Math.abs(v0) <= TOL, `${c.name} ${label}: f(0) = ${v0}`);
      assert.ok(Math.abs(v1 - 1) <= TOL, `${c.name} ${label}: f(1) = ${v1}`);
    }
    for (const x of SAMPLES) {
      assert.ok(Number.isFinite(bezierEase(x, ...c.h)), `${c.name} camera @${x}`);
      assert.ok(Number.isFinite(easer(x)), `${c.name} selector @${x}`);
      assert.ok(Number.isFinite(evalCurvePoints(pts, x)), `${c.name} curve editor @${x}`);
    }
  }
});
