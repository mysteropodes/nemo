'use strict';
// R-lane-a-fixes: the Motion pose a pointer is mapped back through before Draw, Pen, shapes and Fill Brush store a record (baseline toLocalPoint,
// feedback #135). The pose itself is the engine's (`layerMatrix` of the hit-test listing, tested in native-engine against the real projection and in
// native-subselect-contract.test.cjs for the wire); here: the inverse, the mapping of every kind of pointer data, and the read's refusals.
const test = require('node:test'), assert = require('node:assert/strict');
const Pose = require('../src/js/adapters/native-layer-motion-pose.js');
const near = (a, b) => assert.ok(Math.abs(a - b) < 1e-9, `${a} != ${b}`);
const PIN = { instanceId: 'i', documentId: 'd', lifecycleGeneration: 1, contentRevision: 4, documentSnapshotId: 's' };

test('the inverse of a rotated, scaled, moved pose maps a world point and a handle back into the layer\'s own space', () => {
  // local (10, 5): scale (2, 3), turn 90 degrees, move by (100, 50) -> world.
  const m = [0, 2, -3, 0, 100, 50]; // x' = -3 y + 100, y' = 2 x + 50
  const world = [m[0] * 10 + m[2] * 5 + m[4], m[1] * 10 + m[3] * 5 + m[5]];
  const inv = Pose.inverse(m);
  const back = inv.point(world);
  near(back[0], 10); near(back[1], 5);
  const handle = inv.vector([m[0] * 4 + m[2] * 1, m[1] * 4 + m[3] * 1]); // a relative handle (4, 1)
  near(handle[0], 4); near(handle[1], 1);
});

test('a pose that is not six finite numbers or is singular is refused', () => {
  for (const bad of [null, [1, 0, 0, 1, 0], [1, 0, 0, 1, 0, NaN], [1, 2, 2, 4, 0, 0], [0, 0, 0, 0, 0, 0]]) assert.throws(() => Pose.inverse(bad), /Motion transform/, JSON.stringify(bad));
});

test('a Pen path and a freehand stroke keep their widths and curve handles consistent in the layer\'s own space', () => {
  const local = Pose.inverse([1, 0, 0, 1, 30, -10]);
  const t = { start: [40, 0], anchors: [{ point: [40, 0], client: [5, 5], handleOut: [4, 6] }, { point: [70, 20], handleIn: [-4, -6], handleOut: [0, 0] }],
    samples: [[40, 0, 1.5], [55, 10, 2.5]] };
  const { t: u, end } = Pose.localize(t, [70, 20], local);
  assert.deepEqual(u.start, [10, 10]); assert.deepEqual(end, [40, 30]);
  assert.deepEqual(u.anchors.map((a) => a.point), [[10, 10], [40, 30]]);
  assert.deepEqual(u.anchors.map((a) => a.handleOut), [[4, 6], [0, 0]], 'a handle is a vector: the move does not touch it');
  assert.deepEqual(u.anchors[1].handleIn, [-4, -6]);
  assert.deepEqual(u.anchors[0].client, [5, 5], 'the screen position of an anchor is not a document coordinate');
  assert.deepEqual(u.samples, [[10, 10, 1.5], [25, 20, 2.5]], 'x and y map, the width stays');
  assert.deepEqual(t.start, [40, 0], 'the gesture itself is untouched: the preview keeps following the cursor');
  assert.deepEqual(Pose.localize(t, [1, 2], null), { t, end: [1, 2] }, 'no Motion: the gesture is returned as it is');
  assert.equal(Pose.localize(t, null, local).end, null, 'a Pen path has no end point');
});

test('only a layer whose row says it carries Motion needs a pose', () => {
  assert.equal(Pose.needs({ layerUid: 'a', hasMotion: true }), true);
  for (const row of [null, { layerUid: 'a' }, { layerUid: 'a', hasMotion: false }, { hasMotion: 'yes' }]) assert.equal(Pose.needs(row), false);
});

test('the read asks for no geometry, correlates the pin and frame it began on, and refuses anything else', async () => {
  const seen = [];
  const listing = (extra) => ({ pin: { ...PIN }, frame: 3, layerUid: 'a', layerMatrix: [1, 0, 0, 1, 7, -3], ...extra });
  const root = (answer) => ({ NemoSelectCanvasIntent: { async layerPaths(r, wanted, centres, idx) { seen.push([wanted({ role: 'shape' }, [0, 0, 1, 1]), centres, idx]); return answer(); } } });
  const inv = await Pose.read(root(() => listing()), 2, PIN, 3);
  assert.deepEqual(seen, [[false, false, 2]]);
  assert.deepEqual(inv.point([17, 7]), [10, 10]);
  for (const answer of [() => null, () => listing({ frame: 4 }), () => listing({ pin: { ...PIN, contentRevision: 5 } }), () => listing({ layerMatrix: null }), () => listing({ layerMatrix: [1, 0, 0, 1, 0, 0, 0] }),
    () => { throw new Error('stale'); }]) await assert.rejects(Pose.read(root(answer), 2, PIN, 3), /Motion transform|stale/);
  await assert.rejects(Pose.read({}, 2, PIN, 3), /Motion transform/, 'no listing reader at all');
});
