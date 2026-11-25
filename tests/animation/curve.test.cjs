'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const { evalCurvePoints: evaluate } = require('../../src/js/animation/curve.js');

function close(actual, expected, label = '') {
  assert.ok(Math.abs(actual - expected) < 1e-9, `${label}: ${actual} != ${expected}`);
}
const linear = [{ x: 0, y: 0 }, { x: 1, y: 1 }];
// x(t) = t and y(t) = 3t^2 - 2t^3: independent closed-form oracle.
const smooth = [{ x: 0, y: 0, tx: 1, ty: 0 }, { x: 1, y: 1, tx: 1, ty: 0 }];

test('absent or short curves return the input, including outside the unit interval', () => {
  for (const pts of [null, undefined, [], [{ x: 0, y: 0 }]]) {
    for (const x of [-2, 0, 0.3, 1, 2]) assert.equal(evaluate(pts, x), x);
  }
});

test('two on-curve waypoints give linear timing, with input clamping', () => {
  for (const x of [-1, 0, 0.1, 0.25, 0.5, 0.9, 1, 2]) {
    close(evaluate(linear, x), Math.max(0, Math.min(1, x)));
  }
});

test('manual tangents reproduce the smoothstep polynomial across the segment', () => {
  for (let i = 0; i <= 100; i++) {
    const x = i / 100;
    close(evaluate(smooth, x), x * x * (3 - 2 * x), `x=${x}`);
  }
});

test('a numeric tx with omitted ty means a horizontal manual tangent', () => {
  close(evaluate([{ x: 0, y: 0, tx: 1 }, { x: 1, y: 1, tx: 1 }], 0.25), 0.15625);
});

test('waypoints are on the curve, not CSS control handles', () => {
  const pts = [{ x: 0, y: 0 }, { x: 0.25, y: 0.156 }, { x: 0.5, y: 0.5 }, { x: 0.75, y: 0.844 }, { x: 1, y: 1 }];
  for (const p of pts) close(evaluate(pts, p.x), p.y);
});

test('automatic tangents keep flat plateaus and a turning point', () => {
  const plateau = [{ x: 0, y: 0 }, { x: 0.25, y: 0 }, { x: 0.75, y: 1 }, { x: 1, y: 1 }];
  close(evaluate(plateau, 0.125), 0);
  close(evaluate(plateau, 0.5), 0.5);
  close(evaluate(plateau, 0.875), 1);
  const peak = [{ x: 0, y: 0 }, { x: 0.5, y: 1 }, { x: 1, y: 0 }];
  close(evaluate(peak, 0.5), 1);
  for (let i = 0; i <= 100; i++) {
    const y = evaluate(peak, i / 100);
    assert.ok(y >= 0 && y <= 1);
    close(y, evaluate(peak, 1 - i / 100));
  }
});

test('automatic nonuniform collinear points preserve the line and unbounded output', () => {
  const pts = [0, 0.1, 0.6, 1].map(x => ({ x, y: 2 * x + 3 }));
  for (let i = 0; i <= 100; i++) close(evaluate(pts, i / 100), 3 + 2 * i / 100);
});

test('the automatic slope limiter prevents a steep next interval from pulling the first below zero', () => {
  // At Bezier parameter 1/2 the first segment has x=7/32, y=3/320.
  close(evaluate([{ x: 0, y: 0 }, { x: 0.5, y: 0.05 }, { x: 1, y: 1 }], 7 / 32), 3 / 320);
});

test('manual tangents preserve overshoot beyond both output endpoints', () => {
  const pts = [{ x: 0, y: 0, tx: 1, ty: -6 }, { x: 1, y: 1, tx: 1, ty: -6 }];
  close(evaluate(pts, 0.25), -0.40625);
  close(evaluate(pts, 0.75), 1.40625);
});

test('equal-x points and a zero derivative retain the legacy first-point result', () => {
  close(evaluate([{ x: 0, y: 0.2 }, { x: 0, y: 0.8 }], 0), 0.2);
  close(evaluate([{ x: 0, y: 0.2, tx: 0 }, { x: 0, y: 0.8, tx: 0 }], 0.5), 0.2);
  const duplicate = [{ x: 0, y: 0 }, { x: 0, y: 1 }, { x: 1, y: 1 }];
  close(evaluate(duplicate, 0), 0);
  close(evaluate(duplicate, 0.25), 1);
});

test('evaluation does not mutate frozen input and observes later in-place edits', () => {
  const frozen = Object.freeze(smooth.map(p => Object.freeze({ ...p })));
  close(evaluate(frozen, 0.25), 0.15625);
  const edited = smooth.map(p => ({ ...p }));
  close(evaluate(edited, 0.25), 0.15625);
  edited[0].ty = edited[1].ty = 1;
  close(evaluate(edited, 0.25), 0.25);
});

// ---- continuity of the solve (handles that used to fold x(t) back) ----------
// Shared with native-engine/tests/evaluation.rs: same curves, same samples.
const folded = {
  lateWaypoint: [{ x: 0, y: 0 }, { x: 0.9, y: 0.2 }, { x: 1, y: 1 }],
  earlyWaypoint: [{ x: 0, y: 0 }, { x: 0.05, y: 0.6 }, { x: 1, y: 1 }],
  crowdedEnds: [{ x: 0, y: 0 }, { x: 0.02, y: 0.3 }, { x: 0.5, y: 0.5 }, { x: 0.98, y: 0.7 }, { x: 1, y: 1 }],
  longManual: [{ x: 0, y: 0, tx: 4, ty: 0 }, { x: 1, y: 1, tx: 4, ty: 0 }],
  negativeManual: [{ x: 0, y: 0, tx: -1, ty: 1 }, { x: 1, y: 1, tx: 1, ty: 0 }],
};

test('unevenly spaced waypoints no longer step: the value is continuous in x', () => {
  // Before the handle limit, lateWaypoint moved by 0.8 of the range in one
  // 5e-5 step near x = 0.995 (the last frames of the segment popped).
  for (const [name, pts] of Object.entries(folded)) {
    let previous = evaluate(pts, 0), worst = 0;
    for (let i = 1; i <= 20000; i++) {
      const y = evaluate(pts, i / 20000);
      worst = Math.max(worst, Math.abs(y - previous));
      previous = y;
    }
    assert.ok(worst < 0.05, `${name}: largest step ${worst}`);
    for (const p of pts) close(evaluate(pts, p.x), p.y, `${name} waypoint x=${p.x}`);
  }
});

test('monotone waypoints give monotone timing after the handle limit', () => {
  for (const name of ['lateWaypoint', 'earlyWaypoint', 'crowdedEnds']) {
    let previous = -Infinity;
    for (let i = 0; i <= 2000; i++) {
      const y = evaluate(folded[name], i / 2000);
      assert.ok(y >= previous - 1e-12, `${name} runs backward at x=${i / 2000}`);
      assert.ok(y >= -1e-12 && y <= 1 + 1e-12, `${name} leaves [0,1] at x=${i / 2000}`);
      previous = y;
    }
  }
});

test('pinned samples of the limited curves (mirrored by the native kernel)', () => {
  const pinned = [
    ['lateWaypoint', 0.5, 0.08083535766687819], ['lateWaypoint', 0.95, 0.30785132460126435],
    ['lateWaypoint', 0.99, 0.7825917922680397], ['earlyWaypoint', 0.01, 0.3287537342245592],
    ['crowdedEnds', 0.99, 0.7404442467254743], ['longManual', 0.25, 0.029724605511925205],
    ['negativeManual', 0.1, 0.28148538221488],
  ];
  for (const [name, x, y] of pinned) close(evaluate(folded[name], x), y, `${name} x=${x}`);
});

test('a handle that stays inside its span is evaluated exactly as before', () => {
  // Reference: the unmodified eight-step Newton solve, inlined.
  const cubic = (t, a, b, c, d) => { const u = 1 - t; return u * u * u * a + 3 * u * u * t * b + 3 * u * t * t * c + t * t * t * d; };
  const deriv = (t, a, b, c, d) => { const u = 1 - t; return 3 * u * u * (b - a) + 6 * u * t * (c - b) + 3 * t * t * (d - c); };
  const pts = [{ x: 0, y: 0, tx: 1.26, ty: 0 }, { x: 1, y: 1, tx: 1.26, ty: 0 }]; // CSS ease-in-out
  const c1 = 1.26 / 3, c2 = 1 - 1.26 / 3;
  for (let i = 0; i <= 1000; i++) {
    const x = i / 1000;
    let t = x;
    for (let k = 0; k < 8; k++) {
      const dx = deriv(t, 0, c1, c2, 1);
      if (Math.abs(dx) < 1e-6) break;
      t = Math.max(0, Math.min(1, t - (cubic(t, 0, c1, c2, 1) - x) / dx));
    }
    assert.equal(evaluate(pts, x), cubic(t, 0, 0, 1, 1), `x=${x}`);
  }
});
