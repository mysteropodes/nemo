// Unit tests for the bake-expression-to-keyframes curve reduction
// (src/js/expr-bake.js). Only the pure, dependency-free pieces are
// exercised here — bakeExpressionToKeyframes itself reaches into
// window.SMMotion/pushUndo and is verified live instead (see the PR
// description for the exact live numbers) — same split text-selector.js's
// own tests already use: the math is unit-tested, the DOM/state wiring is
// checked by driving the app.
const assert = require('node:assert/strict');
const test = require('node:test');
const path = require('node:path');

const bake = require(path.resolve(__dirname, '../src/js/expr-bake.js'));

function seriesFromFn(fn, frameStart, frameEnd) {
  var pts = [];
  for (var f = frameStart; f <= frameEnd; f++) pts.push({ x: f, y: fn(f) });
  return pts;
}

test('a flat run collapses to its two endpoints', () => {
  const pts = seriesFromFn(() => 42, 0, 100);
  const reduced = bake._rdp(pts, 0.01);
  assert.equal(reduced.length, 2);
  assert.equal(reduced[0].x, 0);
  assert.equal(reduced[1].x, 100);
  assert.equal(reduced[0].y, 42);
  assert.equal(reduced[1].y, 42);
});

test('a straight linear ramp collapses to its two endpoints', () => {
  const pts = seriesFromFn((f) => f * 3.5 + 10, 0, 48);
  const reduced = bake._rdp(pts, 0.001);
  assert.equal(reduced.length, 2);
});

test('a triangular pulse (one direction change) forces exactly one interior point', () => {
  // A clean rise-then-fall, unlike a one-frame impulse: each half is
  // perfectly linear on its own, so RDP needs only the shared peak, not a
  // cluster of points walling it off (a true single-frame spike IS a
  // legitimate worst case for this class of algorithm — every point
  // adjacent to it looks far from whatever chord spans across it — but
  // real expression output is smooth, not impulse-shaped, so that's not
  // representative of what this is actually reducing).
  const pts = seriesFromFn((f) => (f <= 12 ? (f / 12) * 100 : 100 - ((f - 12) / 12) * 100), 0, 24);
  const reduced = bake._rdp(pts, 1);
  assert.equal(reduced.length, 3);
  assert.equal(reduced[1].x, 12);
  assert.ok(Math.abs(reduced[1].y - 100) < 1e-6);
});

test('tolerance controls the tradeoff: looser epsilon keeps fewer points', () => {
  const pts = seriesFromFn((f) => Math.sin(f / 5) * 50, 0, 120);
  const loose = bake._rdp(pts, 5);
  const tight = bake._rdp(pts, 0.1);
  assert.ok(loose.length < tight.length, `loose ${loose.length} should be < tight ${tight.length}`);
  // Both must still bound every original sample within their own epsilon —
  // the actual CORRECTNESS guarantee RDP exists for, not just "fewer points".
  [[loose, 5], [tight, 0.1]].forEach(([reduced, eps]) => {
    for (const p of pts) {
      const i = reduced.findIndex((r) => r.x >= p.x);
      if (i <= 0 || i >= reduced.length) continue;
      const a = reduced[i - 1], b = reduced[i];
      const t = (p.x - a.x) / (b.x - a.x || 1);
      const interp = a.y + (b.y - a.y) * t;
      assert.ok(Math.abs(interp - p.y) <= eps + 1e-6, `frame ${p.x}: |${interp - p.y}| > ${eps}`);
    }
  });
});

test('fewer than 3 points pass through unchanged', () => {
  assert.deepEqual(bake._rdp([], 1), []);
  assert.deepEqual(bake._rdp([{ x: 0, y: 1 }], 1), [{ x: 0, y: 1 }]);
  const two = [{ x: 0, y: 1 }, { x: 5, y: 9 }];
  assert.deepEqual(bake._rdp(two, 1), two);
});

test('unionKeyframeTimes: each axis contributes its OWN extrema', () => {
  // X is a straight ramp (needs only its 2 endpoints); Y is a triangular
  // pulse peaking at frame 30 (needs an interior point X never would). The
  // union must carry both. A one-frame impulse instead of a triangle would
  // force extra points on EITHER side of the peak too (see the rdp test
  // above) — not because unionKeyframeTimes is wrong, but because RDP
  // legitimately needs more anchors to wall off a true single-frame spike;
  // a triangle is the realistic "one bump" shape this is meant to catch.
  const xSamples = seriesFromFn((f) => f, 0, 60).map((p) => ({ frame: p.x, value: p.y }));
  const ySamples = seriesFromFn((f) => (f <= 30 ? (f / 30) * 500 : 500 - ((f - 30) / 30) * 500), 0, 60).map((p) => ({ frame: p.x, value: p.y }));
  const times = bake._unionKeyframeTimes([xSamples, ySamples], [0.01, 1]);
  assert.deepEqual(times, [0, 30, 60]);
});

test('fitSegmentCurve returns null (plain linear) for an already-linear driver axis', () => {
  const samples = seriesFromFn((f) => f * 2, 0, 24).map((p) => ({ frame: p.x, value: p.y }));
  const curve = bake._fitSegmentCurve([samples], [0], [48], 0.02);
  assert.equal(curve, null);
});

test('fitSegmentCurve fits a real ease shape with a curvePoints array anchored at (0,0)/(1,1)', () => {
  // A smoothstep-shaped ramp from 0 to 100 — NOT linear, should need a curve.
  const samples = [];
  for (let f = 0; f <= 24; f++) {
    const t = f / 24;
    const eased = t * t * (3 - 2 * t); // smoothstep
    samples.push({ frame: f, value: eased * 100 });
  }
  const curve = bake._fitSegmentCurve([samples], [0], [100], 0.01);
  assert.ok(curve, 'expected a fitted curve, got null (linear)');
  assert.ok(curve.length >= 3, `expected >=3 points to describe an S-curve, got ${curve.length}`);
  assert.equal(curve[0].x, 0); assert.equal(curve[0].y, 0);
  assert.equal(curve[curve.length - 1].x, 1);
  assert.ok(Math.abs(curve[curve.length - 1].y - 1) < 1e-9);
});

test('fitSegmentCurve can express an OVERSHOOT (y outside 0..1) in one segment', () => {
  // Value rises past the end value, then settles back — an inertial-bounce
  // shape between two keyframes at 0 and 100. y must exceed 1 somewhere for
  // this to be representable without an extra keyframe at the peak.
  const samples = [];
  for (let f = 0; f <= 24; f++) {
    const t = f / 24;
    const overshoot = t < 1 ? 1 + 0.3 * Math.sin(t * Math.PI) * (1 - t) : 1;
    samples.push({ frame: f, value: overshoot * 100 });
  }
  const curve = bake._fitSegmentCurve([samples], [0], [100], 0.01);
  assert.ok(curve, 'expected a fitted curve');
  const maxY = Math.max.apply(null, curve.map((p) => p.y));
  assert.ok(maxY > 1, `expected an overshoot point with y > 1, max was ${maxY}`);
});

test('fitSegmentCurve picks the larger-delta axis as driver and ignores a flat companion', () => {
  // Axis 0 (driver) eases; axis 1 stays essentially flat across the segment.
  const driverSamples = [];
  const flatSamples = [];
  for (let f = 0; f <= 24; f++) {
    const t = f / 24;
    driverSamples.push({ frame: f, value: (t * t * (3 - 2 * t)) * 200 });
    flatSamples.push({ frame: f, value: 50 + (f % 2 === 0 ? 0 : 1e-6) }); // ~flat, tiny float noise
  }
  const curve = bake._fitSegmentCurve([driverSamples, flatSamples], [0, 50], [200, 50.0000005], 0.01);
  assert.ok(curve, 'expected the driver axis to still produce a fitted curve');
});
