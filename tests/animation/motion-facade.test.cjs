'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const loadMotion = require('./load-motion.cjs');
const linear = () => [{ x: 0, y: 0 }, { x: 1, y: 1 }];
const smooth = () => [{ x: 0, y: 0, tx: 1, ty: 0 }, { x: 1, y: 1, tx: 1, ty: 0 }];
const values = value => Array.from(value);

test('public scalar evaluation respects fallback, endpoint clamp and outgoing holds', () => {
  const motion = loadMotion();
  assert.equal(motion.evalTrack(null, 5, 17), 17);
  assert.equal(motion.evalTrack({ keys: [] }, 5, 17), 17);
  const track = { keys: [{ frame: 10, v: [40], curvePoints: linear() }, { frame: 20, v: [120] }, { frame: 30, v: [200] }] };
  assert.equal(motion.evalTrack(track, 0, 17), 40);
  assert.equal(motion.evalTrack(track, 15, 17), 80);
  track.keys[0].hold = true;
  assert.equal(motion.evalTrack(track, 19.999, 17), 40);
  assert.equal(motion.evalTrack(track, 20, 17), 120);
  assert.equal(motion.evalTrack(track, 40, 17), 200);
});

test('key insertion, value replacement and curve edits affect the existing facade immediately', () => {
  const motion = loadMotion(), holder = {};
  motion.setKeyAtFrame(holder, 'position', 20, [120, 0], linear());
  const start = motion.setKeyAtFrame(holder, 'position', 0, [40, 80], smooth());
  assert.deepEqual(values(motion.rawValueAtFrame(holder, 'position', 5)), [52.5, 67.5]);
  assert.equal(motion.keyAt(holder.motion.position, 0), start);
  assert.equal(motion.setKeyAtFrame(holder, 'position', 0, [0, 80]), start);
  assert.equal(holder.motion.position.keys.length, 2);
  start.curvePoints = linear();
  assert.deepEqual(values(motion.rawValueAtFrame(holder, 'position', 5)), [30, 60]);
  start.hold = true;
  assert.deepEqual(values(motion.rawValueAtFrame(holder, 'position', 19)), [0, 80]);
  assert.deepEqual(values(motion.rawValueAtFrame(holder, 'position', 20)), [120, 0]);
});

test('default timing and output ownership survive holder serialization and restoration', () => {
  const motion = loadMotion(), holder = {};
  motion.setKeyAtFrame(holder, 'rotation', 0, [40]);
  motion.setKeyAtFrame(holder, 'rotation', 12, [120]);
  assert.ok(Math.abs(motion.rawValueAtFrame(holder, 'rotation', 3)[0] - 52.48) < 1e-9);
  const restored = JSON.parse(JSON.stringify(holder));
  assert.ok(Math.abs(motion.rawValueAtFrame(restored, 'rotation', 3)[0] - 52.48) < 1e-9);
  const result = motion.rawValueAtFrame(restored, 'rotation', 0);
  result[0] = -999;
  assert.equal(motion.rawValueAtFrame(restored, 'rotation', 0)[0], 40);
  assert.equal(motion.rawValueAtFrame({}, 'opacity', 0)[0], 100);
});

test('missing and empty curves remain distinct, and default curves are independent clones', () => {
  const motion = loadMotion();
  const track = { keys: [{ frame: 0, v: [10] }, { frame: 20, v: [30] }] };
  assert.ok(Math.abs(motion.evalTrack(track, 5, 0) - 13.12) < 1e-9);
  track.keys[0].curvePoints = [];
  assert.equal(motion.evalTrack(track, 5, 0), 15);
  const a = motion.DEFAULT_CURVE(), b = motion.DEFAULT_CURVE();
  assert.equal(JSON.stringify(a), JSON.stringify(b));
  assert.notEqual(a, b);
  assert.notEqual(a[1], b[1]);
  a[1].y = -1;
  assert.equal(motion.DEFAULT_CURVE()[1].y, 0.156);
});

test('spatial position handles use eased time without modifying key geometry', () => {
  const motion = loadMotion(), holder = {};
  const a = motion.setKeyAtFrame(holder, 'position', 0, [0, 0], linear());
  const b = motion.setKeyAtFrame(holder, 'position', 10, [100, 0], linear());
  a.hOut = [0, 100]; b.hIn = [0, 100];
  const saved = JSON.stringify(holder);
  assert.deepEqual(values(motion.rawValueAtFrame(holder, 'position', 5)), [50, 75]);
  assert.equal(JSON.stringify(holder), saved);
});
