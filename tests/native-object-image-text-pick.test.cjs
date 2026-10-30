'use strict';
// #2372: an image and a text block are listed by the native hit-test read (role raster / text, painted bounds, count 0) at their
// paint-order position, and Select picks them as their painted rect (rotation-aware through the item matrix).
const test = require('node:test');
const assert = require('node:assert/strict');
const paper = require('./helpers/native-import-paper.cjs').load();
const Intent = require('../src/js/adapters/select-canvas-intent.js');
const Reads = require('../src/js/adapters/native-object-project-reads.js');
const C = require('../src/js/adapters/native-subselect-contract.js');

const PIN = { instanceId: 'i', documentId: 'd', lifecycleGeneration: 1, contentRevision: 4, documentSnapshotId: 's4', canvasW: 1920, canvasH: 1080,
  layers: [{ layerUid: 'L1' }, { layerUid: 'L2' }] };
const seg = (x, y) => ({ point: [x, y], handleIn: [0, 0], handleOut: [0, 0] });
const target = (id) => ({ contextId: 'scene-root', layerUid: 'L1', frameScope: { kind: 'authored', frame: 0 }, strokeId: id });
const I = [1, 0, 0, 1, 0, 0];
const box = (b) => [seg(b[0], b[1]), seg(b[2], b[1]), seg(b[2], b[3]), seg(b[0], b[3])];
// Rotated 90 degrees about its centre (500,500): the unrotated rect [400,450,600,550] becomes the upright [450,400,550,600].
const TURN = [0, 1, -1, 0, 1000, 0];
const ITEMS = [
  { stableTarget: target('photo'), closed: true, count: 0, role: 'raster', filled: true, bounds: [100, 100, 700, 500], matrix: I },
  { stableTarget: target('line'), closed: false, count: 2, role: 'plain', filled: false, bounds: [650, 300, 900, 300], matrix: I, segments: [seg(650, 300), seg(900, 300)] },
  { stableTarget: target('caption'), closed: true, count: 0, role: 'text', filled: true, bounds: [120, 120, 320, 180], matrix: I },
  { stableTarget: target('turned'), closed: true, count: 0, role: 'raster', filled: true, bounds: [400, 450, 600, 550], matrix: TURN },
];
function root(options = {}) {
  const fetched = [], selected = [];
  const session = {
    isActive: () => true, blocksLegacy: () => true, info: () => ({ ...PIN }), readPresentedObjectSession: () => ({ presentation: { ...PIN, frame: 0 } }),
    async readPresentedStrokes(selector, at, wanted) {
      // As the reader does: a raster or text item is a box, never a paged geometry; the stroke is paged only when kept.
      return { items: ITEMS.map((item) => { const { segments, ...listed } = item, kept = wanted(listed); fetched.push([listed.stableTarget.strokeId, kept]);
        if (!kept) return listed;
        return { ...listed, segments: listed.role === 'raster' || listed.role === 'text' ? box(listed.bounds) : segments }; }) };
    },
  };
  return { fetched, selected, selectedPaths: [], state: { activeLayerIdx: options.active || 0, selectedStrokeIndices: [1] }, Path: paper.Path, Matrix: paper.Matrix, Point: paper.Point,
    NemoNativeObjectProject: { desktop: () => session, selectLayerUI(r, index, preserve, uid) { selected.push([index, preserve, uid]); r.state.activeLayerIdx = index; r._layerSel = [index]; r.selectedPaths = []; return true; } },
    document: { getElementById: () => ({ getBoundingClientRect: () => ({ left: 0, top: 0, width: 960, height: 540 }) }) },
    NemoNativeViewportUnderlay: { canvasMapping: { point: (e) => ({ x: e.clientX * 2, y: e.clientY * 2 }) } },
    updateUI() {} };
}

test('a click on an image selects it by its painted rect, and the stroke above it wins where they overlap', async () => {
  const r = root();
  const photo = await Intent.pick(r, 150, 150);
  assert.deepEqual([photo.data.strokeId, photo.data.nativeRole, photo.data.nativeStableTarget], ['photo', 'raster', target('photo')], 'world (300,300) is inside the image only');
  assert.equal(photo.data.nativeLayerIndex, 0);
  assert.equal((await Intent.pick(r, 330, 150)).data.strokeId, 'line', 'world (660,300): the line is above the image');
  assert.equal(await Intent.pick(r, 500, 250), null, 'world (1000,500) is empty');
});

test('a click on a text block selects it, above the image it lies on', async () => {
  const r = root();
  const text = await Intent.pick(r, 100, 70);
  assert.deepEqual([text.data.strokeId, text.data.nativeRole, text.data.nativeStableTarget], ['caption', 'text', target('caption')], 'world (200,140): text above image');
});

test('a rotated image is hit through its item matrix, not its unrotated rect', async () => {
  const r = root();
  assert.equal((await Intent.pick(r, 250, 295)).data.strokeId, 'turned', 'world (500,590) is inside the turned rect only');
  assert.equal(await Intent.pick(r, 290, 260), null, 'world (580,520) is inside the unrotated rect only');
});

test('picking an image makes its layer the selected layer row; a stroke or Shift pick leaves the layer selection alone', () => {
  const r = root();
  r._layerSel = [0, 1];
  const item = (id, role) => ({ data: { strokeId: id, nativeRole: role, nativeLayerIndex: 0, nativeLayerUid: 'L1' } });
  Intent.applyPick(r, item('s', 'plain'), false);
  assert.deepEqual([r.selected.length, r._layerSel], [0, [0, 1]], 'a stroke of the active layer does not');
  Intent.applyPick(r, item('photo', 'raster'), true);
  assert.deepEqual([r.selected.length, r._layerSel], [0, [0, 1]], 'a Shift pick extends the selection and keeps the layer rows');
  Intent.applyPick(r, item('caption', 'text'), false);
  assert.deepEqual([r.selected.length, r._layerSel], [0, [0, 1]], 'a text pick does not feed the Mesh row either');
  Intent.applyPick(r, item('photo', 'raster'), false);
  assert.deepEqual([r.selected, r._layerSel, r.selectedPaths.map((p) => p.data.strokeId)], [[[0, false, 'L1']], [0], ['photo']]);
});

function reader(items) {
  const pin = { instanceId: 'i', documentId: 'd', lifecycleGeneration: 1, contentRevision: 0, documentSnapshotId: 'native-object:d:0' };
  const presentation = { ...pin, owner: 'native', status: 'presented', contextId: 'scene-root', frame: 0, workId: 'work-1', viewGeneration: 1 };
  const state = { active: true, opening: false, pending: 0, synchronizing: false, projectionReady: true, ticket: 1, info: pin, presented: { ...pin, available: true, presentation } };
  const sent = [];
  const reads = Reads.create({ state: () => state, settled: async () => {}, requestId: () => `q-${sent.length}`, dispatch: async (q) => {
    sent.push(q);
    const p = q.payload;
    const result = p.view ? { ...p.view, limit: undefined, atRevision: 0, documentSnapshotId: pin.documentSnapshotId, total: items.length, nextCursor: null, preview: false, items: items.map((i) => ({ ...i })) }
      : { atRevision: 0, documentSnapshotId: pin.documentSnapshotId, subselect: { stableTarget: p.stableTarget, closed: false, total: 2, cursor: 0, nextCursor: null, preview: false, hitTest: true, segments: [seg(650, 300), seg(900, 300)] } };
    delete result.limit;
    return { apiVersion: 2, requestId: q.requestId, instanceId: 'i', documentId: 'd', contentRevision: 0, ok: true, result };
  } });
  return { read: (wanted) => reads.readPresentedStrokes({ contextId: 'scene-root', layerUid: 'L1', frame: 0 }, pin, wanted), sent };
}
test('the reader keeps a kept raster or text item as its painted box without paging any geometry for it', async () => {
  const listed = ITEMS.map(({ segments, ...i }) => i);
  const f = reader(listed), all = await f.read();
  assert.deepEqual(all.items.map((i) => [i.stableTarget.strokeId, Array.isArray(i.segments)]), [['photo', true], ['line', true], ['caption', true], ['turned', true]]);
  assert.deepEqual(all.items[0].segments.map((a) => a.point), [[100, 100], [700, 100], [700, 500], [100, 500]]);
  assert.deepEqual(f.sent.filter((q) => !q.payload.view).map((q) => q.payload.stableTarget.strokeId), ['line'], 'only the stroke is paged');
  const some = await reader(listed).read((item) => item.role === 'text');
  assert.deepEqual(some.items.map((i) => [i.stableTarget.strokeId, Array.isArray(i.segments)]), [['photo', false], ['line', false], ['caption', true], ['turned', false]]);
});

test('the hit-test listing accepts raster and text items and still refuses an unknown role', () => {
  const view = { kind: 'subselect', contextId: 'scene-root', layerUid: 'L1', frame: 0, workId: 'native-work-1', viewGeneration: 1, cursor: 0, hitTest: true };
  const r = { ...view, atRevision: 7, documentSnapshotId: 'native-object:d:7', total: 2, nextCursor: null, preview: false, items: ITEMS.filter((i) => i.role !== 'plain').slice(0, 2).map(({ segments, ...i }) => i) };
  C.viewResult(r);
  assert.throws(() => C.viewResult({ ...r, items: [{ ...r.items[0], role: 'image' }] }));
});
