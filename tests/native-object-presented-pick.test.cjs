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
  // A filled closed plain record (a drawn rectangle with fill paint) and an unfilled closed outline beside it.
  { stableTarget: target('filled'), closed: true, count: 4, role: 'plain', filled: true, bounds: [800, 0, 900, 100], matrix: [1, 0, 0, 1, 0, 0],
    segments: [seg(800, 0), seg(900, 0), seg(900, 100), seg(800, 100)] },
  { stableTarget: target('outline'), closed: true, count: 4, role: 'plain', filled: false, bounds: [1000, 0, 1100, 100], matrix: [1, 0, 0, 1, 0, 0],
    segments: [seg(1000, 0), seg(1100, 0), seg(1100, 100), seg(1000, 100)] },
  // A ribbon's linked fill below everything else, far from the line: a click on it selects its ribbon.
  { stableTarget: target('fill'), closed: true, count: 4, role: 'fill', owner: 'ribbon', bounds: [600, 0, 700, 100], matrix: [1, 0, 0, 1, 0, 0],
    segments: [seg(600, 0), seg(700, 0), seg(700, 100), seg(600, 100)] },
  { stableTarget: target('line'), closed: false, count: 2, role: 'plain', filled: false, bounds: [100, 100, 500, 100], segments: [seg(100, 100), seg(500, 100)], matrix: [1, 0, 0, 1, 0, 0] },
  // A ribbon is listed with its painted outline (hit inside); this one is moved 300 down by its presented transform.
  { stableTarget: target('ribbon'), closed: true, count: 4, role: 'ribbon', bounds: [100, -20, 500, 20], matrix: [1, 0, 0, 1, 0, 300],
    segments: [seg(100, -20), seg(500, -20), seg(500, 20), seg(100, 20)] },
];
function root(options = {}) {
  const reads = [], fetched = [];
  let pin = { ...PIN };
  const session = {
    isActive: () => true, blocksLegacy: () => options.legacy !== true, info: () => pin, readPresentedObjectSession: () => ({ presentation: { ...pin, frame: 0 } }),
    async readPresentedStrokes(selector, at, wanted) {
      reads.push(selector);
      if (options.stale && reads.length <= options.stale) throw Object.assign(new Error('Native image-mesh view changed'), { code: 'stale_revision' });
      if (options.changeDuringRead) pin = { ...pin, contentRevision: 5 };
      // As the reader does: every record listed, geometry only for the ones `wanted` keeps.
      return { items: ITEMS.map(({ segments, ...item }) => { const kept = wanted(item); fetched.push([item.stableTarget.strokeId, kept]); return kept ? { ...item, segments } : item; }) };
    },
  };
  return { reads, fetched, selectedPaths: [], state: { activeLayerIdx: 0, selectedStrokeIndices: [1] }, Path: paper.Path, Matrix: paper.Matrix, Point: paper.Point,
    NemoNativeObjectProject: { desktop: () => session },
    document: { getElementById: () => ({ getBoundingClientRect: () => ({ left: 0, top: 0, width: 960, height: 540 }) }) },
    // The underlay maps a client point at half scale.
    NemoNativeViewportUnderlay: { canvasMapping: { point: (e) => ({ x: e.clientX * 2, y: e.clientY * 2 }) } },
    updated: 0, updateUI() { this.updated++; } };
}

test('a click picks the top-most presented stroke of the active layer in world space', async () => {
  const r = root();
  assert.equal((await Intent.pick(r, 150, 50)).data.strokeId, 'line', 'world (300,100) lies on the line');
  assert.deepEqual(r.fetched, [['filled', false], ['outline', false], ['fill', false], ['line', true], ['ribbon', false]], 'only geometry whose world bounds reach the click is read');
  assert.deepEqual(r.reads[0], { contextId: 'scene-root', layerUid: 'L1', frame: 0 });
  const ribbon = await Intent.pick(r, 150, 155);
  assert.deepEqual([ribbon.data.strokeId, ribbon.data.nativeRole, ribbon.data.nativeStableTarget], ['ribbon', 'ribbon', target('ribbon')],
    'world (300,310) is inside the transformed outline');
  assert.equal(await Intent.pick(r, 150, 100), null, 'empty canvas picks nothing');
  const fill = await Intent.pick(r, 325, 25);
  assert.deepEqual([fill.data.strokeId, fill.data.nativeRole, fill.data.nativeStableTarget.strokeId], ['ribbon', 'ribbon', 'ribbon'], 'a linked fill selects its ribbon');
  assert.equal(fill.data.linkedFill.data.strokeId, 'fill', 'and the ribbon carries its fill, as a baseline Paper item did');
  assert.equal((await Intent.pick(r, 150, 155)).data.linkedFill.data.strokeId, 'fill', 'a ribbon hit directly carries it too');
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

test('a read refused as stale by a repaint is retried on the fresh presentation, a bounded number of times', async () => {
  const twice = root({ stale: 2 });
  assert.equal((await Intent.pick(twice, 150, 50)).data.strokeId, 'line');
  assert.equal(twice.reads.length, 3);
  const always = root({ stale: 99 });
  await assert.rejects(Intent.pick(always, 150, 50), (e) => e.code === 'stale_revision');
  assert.equal(always.reads.length, 4);
});

test('a pick stands across a repaint but not across a frame, tool or revision change', () => {
  const r = root(), before = Intent.pickContext(r);
  assert.equal(Intent.pickContext(r), before);
  r.state.currentFrame = 2;
  assert.notEqual(Intent.pickContext(r), before);
});

test('an interior click hits a filled closed record but not an unfilled outline, whose line is still hit within 8 px (#2372)', async () => {
  const r = root();
  assert.equal((await Intent.pick(r, 425, 25)).data.strokeId, 'filled', 'world (850,50) is inside the filled quad');
  assert.equal(await Intent.pick(r, 525, 25), null, 'world (1050,50) is inside the unfilled outline only');
  assert.equal((await Intent.pick(r, 500, 25)).data.strokeId, 'outline', 'world (1000,50) is on the outline');
  assert.equal((await Intent.pick(r, 508, 25)).data.strokeId, 'outline', 'world (1016,50) is within 8 screen pixels of the outline');
  assert.equal(await Intent.pick(r, 512, 25), null, 'world (1024,50) is beyond 8 screen pixels (16 world) of it');
});
