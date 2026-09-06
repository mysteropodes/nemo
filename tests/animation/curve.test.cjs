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
