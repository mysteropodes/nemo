'use strict';
// B-draw-perspective-snap: the baseline numbers of perspective-bridge.js snapToVP / findVPRayByAngle / projectOnRay and draw-bridge.js guideConstrain,
// over the vanishing points the native perspective read presents. Expected values are derived by hand from the baseline constants
// (SNAP_DEG 6, GUIDE_PULL_MAX_DEG 30, GUIDE_PULL_STRENGTH 0.55, 4 px floor), not from this module.
const test = require('node:test'), assert = require('node:assert/strict');
const Snap = require('../src/js/adapters/native-object-perspective-snap.js');
const on = (...items) => ({ perspective: { settings: { perspectiveEnabled: true }, items: items.map((v, i) => ({ ordinal: i + 1, x: v[0], y: v[1], locked: false })) } });
const close = (a, b, eps = 1e-9) => assert.ok(Math.abs(a - b) < eps, a + ' vs ' + b);
const VP = on([1000, 0]);

test('the guide off, absent or without points changes nothing', () => {
  assert.deepEqual(Snap.line(null, [0, 0], [100, 3]), [100, 3]);
  assert.deepEqual(Snap.line({ perspective: { settings: { perspectiveEnabled: false }, items: [{ ordinal: 1, x: 1000, y: 0 }] } }, [0, 0], [100, 3]), [100, 3]);
  assert.deepEqual(Snap.line({ perspective: { settings: {}, items: [{ ordinal: 1, x: 1000, y: 0 }] } }, [0, 0], [100, 3]), [100, 3]);
  assert.deepEqual(Snap.pull(on(), [0, 0], [100, 20]), [100, 20]);
});
test('Line: an end within 6 degrees of the VP axis lands on it at the drawn length', () => {
  const [x, y] = Snap.line(VP, [0, 0], [100, 3]); // atan2(3,100) = 1.72 degrees
  close(x, Math.hypot(100, 3)); close(y, 0);
  assert.deepEqual(Snap.line(VP, [0, 0], [100, 20]), [100, 20]); // 11.31 degrees: outside the catch angle
  const edge = Snap.line(VP, [0, 0], [100, 10.5]); // atan(.105) = 5.99 degrees: inside
  close(edge[1], 0);
  assert.deepEqual(Snap.line(VP, [0, 0], [100, 10.6]), [100, 10.6]); // atan(.106) = 6.05 degrees: outside
});
test('Line: the vertical axis is always a candidate; under 4 px of drag there is no direction yet', () => {
  const [x, y] = Snap.line(VP, [0, 0], [3, 100]);
  close(x, 0); close(y, Math.hypot(3, 100));
  assert.deepEqual(Snap.line(VP, [0, 0], [3, 1]), [3, 1]); // 3.16 px
  assert.deepEqual(Snap.line(VP, [50, 50], [50, 50]), [50, 50]);
});
test('the axis set is one direction per VP (none when sitting on it) then the vertical, in that order', () => {
  const a = Snap.axes([{ x: 100, y: 0 }, { x: 0, y: 0.4 }], [0, 0]);
  assert.equal(a.length, 2); // the second VP is under 1 px away: no meaningful direction
  assert.deepEqual(a, [{ x: 1, y: 0 }, { x: 0, y: 1 }]);
  const b = Snap.axes([{ x: 0, y: 100 }], [100, 100]);
  close(b[0].x, -1); close(b[0].y, 0);
});
test('the closest axis in angle wins among several VPs', () => {
  const two = on([1000, 0], [-1000, 0], [0, 5000]);
  const [x, y] = Snap.line(two, [0, 0], [-100, 4]); // toward the left VP, 2.29 degrees off
  close(y, 0); assert.ok(x < 0);
});
test('Draw pull: a sample is blended part-way toward its projection on the axis, easing from 0.55 to 0 across 30 degrees', () => {
  const diff = Math.atan(0.2) * 180 / Math.PI, t = (1 - diff / 30) * 0.55;
  const [x, y] = Snap.pull(VP, [0, 0], [100, 20]);
  close(x, 100); close(y, 20 - 20 * t); close(y, 13.14698, 1e-4);
  const [vx, vy] = Snap.pull(VP, [0, 0], [20, 100]); // the vertical axis, 11.31 degrees away
  close(vy, 100); close(vx, 20 - 20 * t);
  const perfect = Snap.pull(VP, [0, 0], [100, 0]); // aimed exactly: already on the axis
  assert.deepEqual(perfect, [100, 0]);
});
test('Draw pull: never a hard lock, nothing past 30 degrees and nothing under 4 px of drag', () => {
  assert.deepEqual(Snap.pull(VP, [0, 0], [100, 80]), [100, 80]); // 38.7 degrees from the VP axis, 51.3 from the vertical
  assert.deepEqual(Snap.pull(VP, [0, 0], [2, 1]), [2, 1]);
  const [, y] = Snap.pull(VP, [0, 0], [100, 40]); // 21.8 degrees: pulled, but not onto the axis
  assert.ok(y > 0 && y < 40);
  const [, y2] = Snap.pull(VP, [0, 0], [100, 29]); // 16.2 degrees: pulled more than at 21.8
  assert.ok(40 - y < 29 - y2 || y2 / 29 < y / 40);
});
test('the pull is relative to the press point: the VP axis from there is the ray, not the axis from the origin', () => {
  // From (0,500) the VP (1000,0) lies along (1000,-500), -26.57 degrees. A sample at (100,430) heads -35 degrees.
  const ux = 1000 / Math.hypot(1000, 500), uy = -500 / Math.hypot(1000, 500);
  const diff = Math.abs(Math.atan2(-70, 100) - Math.atan2(uy, ux)) * 180 / Math.PI, t = (1 - diff / 30) * 0.55;
  const along = 100 * ux + (-70) * uy, proj = [along * ux, 500 + along * uy];
  const [x, y] = Snap.pull(VP, [0, 500], [100, 430]);
  close(x, 100 + (proj[0] - 100) * t); close(y, 430 + (proj[1] - 430) * t);
  assert.ok(y > 430); // toward the ray
  const aimed = Snap.pull(VP, [0, 500], [100, 450]);
  close(aimed[0], 100); close(aimed[1], 450);
});
