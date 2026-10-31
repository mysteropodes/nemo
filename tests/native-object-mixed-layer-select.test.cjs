'use strict';
// #2372: a layer that holds a vector together with text or an image is read by Select (hit-test listing + one geometry page per
// kept vector) and by Direct Select (editable listing + geometry) without any "complete native vector layer" refusal.
const test = require('node:test');
const assert = require('node:assert/strict');
const Reads = require('../src/js/adapters/native-object-project-reads.js');

const seg = (x, y) => ({ point: [x, y], handleIn: [0, 0], handleOut: [0, 0] });
const target = (id) => ({ contextId: 'scene-root', layerUid: 'L1', frameScope: { kind: 'authored', frame: 0 }, strokeId: id });
const I = [1, 0, 0, 1, 0, 0];
const VECTOR = { stableTarget: target('v'), closed: false, count: 2, role: 'plain', filled: false, bounds: [10, 10, 90, 10], matrix: I };
const TEXT = { stableTarget: target('t'), closed: true, count: 0, role: 'text', filled: true, bounds: [10, 20, 60, 40], matrix: I };
const IMAGE = { stableTarget: target('i'), closed: true, count: 0, role: 'raster', filled: true, bounds: [0, 0, 100, 100], matrix: I };
const GEOMETRY = [seg(10, 10), seg(90, 10)];

// A native fake that behaves as the engine does for a mixed layer: the hit-test listing carries every painted item in paint
// order, the editable listing only the vector records, and a geometry page is served for a vector record only.
function reader(mixed = true, listed = 2) {
  const pin = { instanceId: 'i', documentId: 'd', lifecycleGeneration: 1, contentRevision: 0, documentSnapshotId: 'native-object:d:0' };
  const presentation = { ...pin, owner: 'native', status: 'presented', contextId: 'scene-root', frame: 0, workId: 'work-1', viewGeneration: 1 };
  const state = { active: true, opening: false, pending: 0, synchronizing: false, projectionReady: true, ticket: 1, info: pin, presented: { ...pin, available: true, presentation } };
  const sent = [];
  const reads = Reads.create({ state: () => state, settled: async () => {}, requestId: () => `q-${sent.length}`, dispatch: async (q) => {
    sent.push(q);
    const p = q.payload;
    const base = { atRevision: 0, documentSnapshotId: pin.documentSnapshotId };
    let result;
    if (p.view) {
      const items = p.view.hitTest ? (mixed ? [{ ...VECTOR, count: listed }, TEXT, IMAGE] : [VECTOR]) : [{ stableTarget: VECTOR.stableTarget, closed: false, count: 2, matrix: I }];
      result = { ...p.view, ...base, total: items.length, nextCursor: null, preview: false, items: items.map((i) => ({ ...i })) };
      delete result.limit;
    } else if (p.stableTarget.strokeId !== 'v') {
      return { apiVersion: 2, requestId: q.requestId, instanceId: 'i', documentId: 'd', contentRevision: 0, ok: false, error: { code: 'unavailable', message: 'Subselect target is absent.' } };
    } else {
      result = { ...base, subselect: { stableTarget: p.stableTarget, closed: false, total: 2, cursor: 0, nextCursor: null, preview: false, ...(p.subselect.hitTest ? { hitTest: true } : {}), segments: GEOMETRY } };
    }
    return { apiVersion: 2, requestId: q.requestId, instanceId: 'i', documentId: 'd', contentRevision: 0, ok: true, result };
  } });
  const selector = { contextId: 'scene-root', layerUid: 'L1', frame: 0 };
  return { sent, presented: (wanted) => reads.readPresentedStrokes(selector, pin, wanted), subselect: () => reads.readSubselect(selector, pin) };
}

test('Select reads a vector beside a text block and an image: one hit-test geometry page for the vector, boxes for the others', async () => {
  const f = reader(), all = await f.presented();
  assert.deepEqual(all.items.map((i) => [i.stableTarget.strokeId, i.role, i.segments.length]), [['v', 'plain', 2], ['t', 'text', 4], ['i', 'raster', 4]]);
  const pages = f.sent.filter((q) => !q.payload.view);
  assert.deepEqual(pages.map((q) => [q.payload.stableTarget.strokeId, q.payload.subselect.hitTest]), [['v', true]], 'only the vector is paged, as a hit-test page');
  assert.equal(pages[0].payload.subselect.preview, undefined);
});

test('Select refuses a vector whose geometry page disagrees with the listing, and a wanted filter pages only what it keeps', async () => {
  await assert.rejects(() => reader(true, 3).presented(), /Presented geometry differs from its listing/);
  const kept = await reader().presented((item) => item.role === 'plain');
  assert.deepEqual(kept.items.map((i) => Array.isArray(i.segments)), [true, false, false]);
});

test('Direct Select reads the vector of a mixed layer: the editable listing carries it alone and its geometry is paged without hitTest', async () => {
  const f = reader(), data = await f.subselect();
  assert.deepEqual(data.items.map((i) => [i.stableTarget.strokeId, i.segments.length]), [['v', 2]]);
  const pages = f.sent.filter((q) => !q.payload.view);
  assert.deepEqual(pages.map((q) => [q.payload.stableTarget.strokeId, q.payload.subselect.hitTest]), [['v', undefined]]);
  const views = f.sent.filter((q) => q.payload.view).map((q) => q.payload.view.hitTest);
  assert.deepEqual(views, [undefined], 'the editable listing is not a hit-test listing');
});
