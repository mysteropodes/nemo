'use strict';
// #2372: the native hit-test listing carries a plain stroke's authored width, so Select's reach and hit test cover the painted edge
// of a thick stroke (the baseline Paper hit-test used the real stroke width), not a 1px line plus the 8px tolerance.
const test = require('node:test');
const assert = require('node:assert/strict');
const paper = require('./helpers/native-import-paper.cjs').load();
const Intent = require('../src/js/adapters/select-canvas-intent.js');

const PIN = { instanceId: 'i', documentId: 'd', lifecycleGeneration: 1, contentRevision: 4, documentSnapshotId: 's4', canvasW: 1920, canvasH: 1080,
  layers: [{ layerUid: 'L1' }] };
const seg = (x, y) => ({ point: [x, y], handleIn: [0, 0], handleOut: [0, 0] });
const target = (id) => ({ contextId: 'scene-root', layerUid: 'L1', frameScope: { kind: 'authored', frame: 0 }, strokeId: id });
function root(items) {
  const fetched = [];
  const session = {
    isActive: () => true, blocksLegacy: () => true, info: () => PIN, readPresentedObjectSession: () => ({ presentation: { ...PIN, frame: 0 } }),
    async readPresentedStrokes(_selector, _at, wanted) {
      return { items: items.map(({ segments, ...item }) => { const kept = wanted(item); fetched.push([item.stableTarget.strokeId, kept]); return kept ? { ...item, segments } : item; }) };
    },
  };
  return { fetched, selectedPaths: [], state: { activeLayerIdx: 0, selectedStrokeIndices: [] }, Path: paper.Path, Matrix: paper.Matrix, Point: paper.Point,
    NemoNativeObjectProject: { desktop: () => session },
    document: { getElementById: () => ({ getBoundingClientRect: () => ({ left: 0, top: 0, width: 960, height: 540 }) }) },
    // Half scale: a client point (x, y) is world (2x, 2y), and 8 screen pixels are 16 world units.
    NemoNativeViewportUnderlay: { canvasMapping: { point: (e) => ({ x: e.clientX * 2, y: e.clientY * 2 }) } } };
}
const line = (strokeWidth, extra) => ({ stableTarget: target('thick'), closed: false, count: 2, role: 'plain', filled: false, bounds: [100, 100, 500, 100],
  matrix: [1, 0, 0, 1, 0, 0], segments: [seg(100, 100), seg(500, 100)], ...(strokeWidth === undefined ? {} : { strokeWidth }), ...extra });

test('a 40px plain stroke is hit on its painted outer edge, and not beyond it plus the tolerance', async () => {
  const r = root([line(40)]);
  assert.equal((await Intent.pick(r, 150, 59)).data.strokeId, 'thick', 'world y 118 is 18 from the centreline: the edge (20) is inside');
  assert.equal((await Intent.pick(r, 150, 68)).data.strokeId, 'thick', 'world y 136 is the edge (20) plus the 16 world tolerance');
  assert.equal(await Intent.pick(r, 150, 69), null, 'world y 138 is past the edge plus the tolerance');
  assert.deepEqual(r.fetched.filter(([, kept]) => kept).length, 2, 'the geometry is only read where the padded bounds reach the click');
});

test('without a listed width the stroke is still a 1px line (the pre-width behaviour)', async () => {
  const r = root([line(undefined)]);
  assert.equal(await Intent.pick(r, 150, 59), null, 'world y 118 is past the 16 world tolerance of a thin line');
  assert.equal((await Intent.pick(r, 150, 57)).data.strokeId, 'thick', 'world y 114 is inside it');
  assert.equal(await Intent.pick(root([line(0)]), 150, 59), null, 'an unstroked record (width 0) keeps the thin line');
});

test('the listed width is carried to world space by the item matrix', async () => {
  const doubled = line(20, { matrix: [2, 0, 0, 2, 0, 0], bounds: [50, 50, 250, 50], segments: [seg(50, 50), seg(250, 50)] });
  const r = root([doubled]);
  assert.equal((await Intent.pick(r, 150, 59)).data.strokeId, 'thick', 'a 20-wide record at scale 2 paints 40 wide in world space');
  assert.equal(await Intent.pick(r, 150, 69), null);
});
