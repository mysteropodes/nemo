'use strict';
// #2362: a native object project builds no Paper items, so Select and the Labs tools hit-test the presented layer's
// geometry read through the native subselect view: transient world-space paths carrying only their native identity.
const test = require('node:test');
const assert = require('node:assert/strict');
const paper = require('./helpers/native-import-paper.cjs').load();
const Intent = require('../src/js/adapters/select-canvas-intent.js');

const PIN = { instanceId: 'i', documentId: 'd', lifecycleGeneration: 1, contentRevision: 4, documentSnapshotId: 's4', canvasW: 1920, canvasH: 1080,
  layers: [{ layerUid: 'L1' }, { layerUid: 'L2' }] };
const seg = (x, y) => ({ point: [x, y], handleIn: [0, 0], handleOut: [0, 0] });
const target = (id) => ({ contextId: 'scene-root', layerUid: 'L1', frameScope: { kind: 'authored', frame: 0 }, strokeId: id });
const ITEMS = [
  { stableTarget: target('line'), closed: false, count: 2, segments: [seg(100, 100), seg(500, 100)], matrix: [1, 0, 0, 1, 0, 0] },
  // A ribbon is hit on its painted outline; this one is moved 300 down by its presented transform.
  { stableTarget: target('ribbon'), closed: false, count: 2, segments: [seg(100, 0), seg(500, 0)], matrix: [1, 0, 0, 1, 0, 300],
    paint: { closed: true, segments: [seg(100, -20), seg(500, -20), seg(500, 20), seg(100, 20)] } },
];
function root(options = {}) {
  const reads = [];
  let pin = { ...PIN };
  const session = {
    isActive: () => true, info: () => pin, readPresentedObjectSession: () => ({ presentation: { ...pin, frame: 0 } }),
    async readSubselect(selector, at) { reads.push(selector); if (options.changeDuringRead) pin = { ...pin, contentRevision: 5 }; return { items: ITEMS }; },
  };
  return { reads, selectedPaths: [], state: { activeLayerIdx: 0, selectedStrokeIndices: [1] }, Path: paper.Path, Matrix: paper.Matrix, Point: paper.Point,
    SMProject: { nativeObjectProject: () => ({ blocksLegacy: () => options.legacy !== true }) }, NemoNativeObjectProject: { desktop: () => session },
    document: { getElementById: () => ({ getBoundingClientRect: () => ({ left: 0, top: 0, width: 960, height: 540 }) }) },
    // The underlay maps a client point at half scale.
    NemoNativeViewportUnderlay: { canvasMapping: { point: (e) => ({ x: e.clientX * 2, y: e.clientY * 2 }) } },
    updated: 0, updateUI() { this.updated++; } };
}

test('a click picks the top-most presented stroke of the active layer in world space', async () => {
  const r = root();
  assert.equal((await Intent.pick(r, 150, 50)).data.strokeId, 'line', 'world (300,100) lies on the line');
  assert.deepEqual(r.reads[0], { contextId: 'scene-root', layerUid: 'L1', frame: 0 });
  const ribbon = await Intent.pick(r, 150, 155);
  assert.deepEqual([ribbon.data.strokeId, ribbon.data.nativeRibbon, ribbon.data.nativeStableTarget], ['ribbon', true, target('ribbon')],
    'world (300,310) is inside the transformed outline');
  assert.equal(await Intent.pick(r, 150, 100), null, 'empty canvas picks nothing');
  assert.ok(await Intent.pick(r, 150, 53), 'within 8 screen pixels (16 world) still hits');
});

test('no object project, or a presentation that changed during the read, picks nothing', async () => {
  assert.equal(await Intent.pick(root({ legacy: true }), 150, 50), undefined);
  assert.equal(await Intent.pick(root({ changeDuringRead: true }), 150, 50), undefined);
});

test('applyPick selects, Shift toggles by native stroke id, and an empty click clears', async () => {
  const r = root();
  Intent.applyPick(r, await Intent.pick(r, 150, 50), false);
  assert.deepEqual(r.selectedPaths.map((p) => p.data.strokeId), ['line']);
  assert.deepEqual(r.state.selectedStrokeIndices, []);
  Intent.applyPick(r, await Intent.pick(r, 150, 155), true);
  assert.deepEqual(r.selectedPaths.map((p) => p.data.strokeId), ['line', 'ribbon']);
  Intent.applyPick(r, await Intent.pick(r, 150, 50), true);
  assert.deepEqual(r.selectedPaths.map((p) => p.data.strokeId), ['ribbon'], 'a fresh pick of the same stroke toggles it off');
  Intent.applyPick(r, null, true);
  assert.equal(r.selectedPaths.length, 1, 'Shift on empty canvas keeps the selection');
  Intent.applyPick(r, null, false);
  assert.equal(r.selectedPaths.length, 0);
  assert.equal(r.updated, 5);
});
