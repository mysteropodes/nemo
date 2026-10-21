'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const { fixture } = require('./helpers/native-guide-ui.cjs');
const xy = point => point && [point.x, point.y];

test('ordinary Guide caller authors fixed 20 then 60 using actual native presentation, without Paper', async () => {
  const f = fixture({ rect: { left: 66, top: 102, width: 1038, height: 504 },
    affine: [1, 0, 0, 1, 359, 162], guides: { h: [], v: [] }, paperPoison: true });
  await f.ready();
  assert.deepEqual(xy(f.controller.coordinates().project({ x: 160, y: 90 })), [519, 252]);
  f.document.fire('pointerdown', { target: f.h, clientX: 500, clientY: 110 });
  f.document.fire('pointerup', { clientX: 500, clientY: 284 }); await f.settled();
  assert.deepEqual(f.edits()[0].payload.action, { kind: 'add', axis: 'h', position: 20 });
  assert.deepEqual(f.metadata(), { h: [20], v: [] });
  const hit = f.hits()[0];
  f.document.fire('pointerdown', { target: hit, clientX: 500, clientY: 284 });
  f.document.fire('pointerup', { clientX: 500, clientY: 324 }); await f.settled();
  assert.deepEqual(f.edits()[1].payload.action, { kind: 'move', axis: 'h', ordinal: 1, position: 60 });
  assert.deepEqual(f.metadata(), { h: [60], v: [] });
});

test('native Guide proof uses full client coordinates and subtracts the area offset exactly once', async () => {
  const f = fixture({ rect: { left: 20, top: 30 }, paperPoison: true }); await f.ready();
  const p = f.controller.coordinates(); assert(p && Object.isFrozen(p));
  assert.deepEqual(xy(p.project({ x: 10, y: 20 })), [120, 140]);
  assert.deepEqual(xy(p.unproject({ clientX: 140, clientY: 170 })), [10, 20]);
  assert.equal(p.current(), true);
});

test('native view changes redraw real ruler labels and guide hits with an unchanged Paper view', async () => {
  const f = fixture({ paperPoison: true }); await f.ready();
  const before = f.controller.coordinates(), labels = JSON.stringify(f.h.labels), hit = f.hits()[0];
  const style = JSON.stringify(hit.style);
  f.changeView([1, 0, 0, 1, 50, 70]); f.tick();
  assert.equal(before.current(), false);
  assert.notEqual(f.controller.coordinates().signature, before.signature);
  assert.notEqual(JSON.stringify(f.h.labels), labels);
  assert.notEqual(JSON.stringify(f.hits()[0].style), style);
  assert.equal(f.edits().length, 0);
});

test('view, frame, bounds and subscription changes reject an already captured native Guide gesture', async () => {
  for (const change of [f => f.changeView([1, 0, 0, 1, 0, 0]), f => f.changeFrame(1),
    f => f.changeBounds({ left: 1 }), f => f.resubscribe()]) {
    const f = fixture({ paperPoison: true }); await f.ready(); const before = f.metadata();
    f.document.fire('pointerdown', { target: f.h }); change(f);
    f.document.fire('pointerup', { clientY: 48 }); await f.settled();
    assert.equal(f.edits().length, 0); assert.deepEqual(f.metadata(), before); assert.equal(f.ghosts().length, 0);
  }
});

test('missing or wrong native presentation refuses both Guide edits and coordinate publication', async () => {
  for (const invalid of [{ status: 'failed-validation' }, { frame: 9 }, { owner: 'foreign' }, { documentId: 'foreign' }]) {
    const f = fixture({ paperPoison: true }); await f.ready(); const before = f.metadata();
    f.invalidatePresentation(invalid); f.tick();
    assert.equal(f.controller.coordinates(), null);
    f.document.fire('pointerdown', { target: f.h }); f.document.fire('pointerup', { clientY: 48 }); await f.settled();
    assert.equal(f.edits().length, 0); assert.deepEqual(f.metadata(), before); assert.equal(f.hits().length, 0);
  }
});

test('rotated native Camera mapping clips the actual world Guide line and authors its world position', async () => {
  const f = fixture({ affine: [0, 2, -2, 0, 300, 20], cameraView: true, guides: { h: [20], v: [] }, paperPoison: true });
  await f.ready(); const p = f.controller.coordinates(), line = p.segment('h', 20);
  assert.deepEqual([xy(line.from), xy(line.to)].sort((a, b) => a[1] - b[1]), [[260, 0], [260, 240]]);
  assert.deepEqual(xy(p.unproject({ clientX: 260, clientY: 100 })), [40, 20]);
  assert.match(f.hits()[0].style.transform, /rotate\(/);
  assert(f.h.labels.length < 100 && f.v.labels.length < 100, 'finite edge ticks even for a perpendicular world axis');
  f.document.fire('pointerdown', { target: f.hits()[0], clientX: 260, clientY: 100 });
  f.document.fire('pointerup', { clientX: 180, clientY: 100 }); await f.settled();
  assert.deepEqual(f.edits()[0].payload.action, { kind: 'move', axis: 'h', ordinal: 1, position: 60 });
});

test('native Guide snapping measures perpendicular CSS distance with nonuniform scale and Camera roll', async () => {
  const f = fixture({ affine: [2, 0, 0, 4, 100, 100], guides: { h: [20], v: [60] }, paperPoison: true });
  await f.ready();
  assert.deepEqual(xy(f.root.SMRulers.snapPoint({ x: 63, y: 21.5 }, 8)), [60, 20]);
  assert.deepEqual(xy(f.root.SMRulers.snapPoint({ x: 65, y: 22.5 }, 8)), [65, 22.5]);
  const rotated = fixture({ affine: [0, 2, -2, 0, 300, 20], cameraView: true, guides: { h: [20], v: [] }, paperPoison: true });
  await rotated.ready();
  assert.deepEqual(xy(rotated.root.SMRulers.snapPoint({ x: 40, y: 23 }, 8)), [40, 20]);
  rotated.invalidatePresentation({ status: 'failed-validation' });
  assert.throws(() => rotated.root.SMRulers.snapPoint({ x: 40, y: 23 }, 8), e => e.code === 'unavailable');
});
