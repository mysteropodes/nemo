'use strict';
// #2372 A-select-order: on a native object project the Select menu's 'Premier plan' and 'Arriere-plan' send ONE native object.vector.batch of `order`
// steps (one revision, one history entry) through NemoNativeSelectionOrder instead of the legacy pushUndo + bringToFront / sendToBack over Paper items a
// native project does not have. The expected stacks are the baseline's (1065e9d8 select-bridge.js onContext): bringToFront over the selection in selection
// order, sendToBack over it in reverse order, so the selected items keep the order they were selected in. The real select-canvas-intent reads the selection
// at the presented revision; the batch route is a stub whose owner restacks its layer listing the way the engine's order step does.
const test = require('node:test'), assert = require('node:assert/strict');
const Intent = require('../src/js/adapters/select-canvas-intent.js');
const Overlay = require('../src/js/adapters/native-select-overlay.js');
const Gesture = require('../src/js/adapters/native-select-gesture.js');
const Order = require('../src/js/adapters/native-selection-order.js');
const Menu = require('../src/js/adapters/native-select-menu.js');
const Contract = require('../src/js/adapters/native-object-vector-contract.js');
const Paper = require('./helpers/native-import-paper.cjs');

function el(tag) {
  return { tag, attrs: {}, style: {}, children: [], setAttribute(k, v) { this.attrs[k] = String(v); },
    appendChild(c) { this.children.push(c); c.parentNode = this; return c; },
    remove() { if (this.parentNode) this.parentNode.children = this.parentNode.children.filter((c) => c !== this); } };
}
const seg = (x, y) => ({ point: [x, y], handleIn: [0, 0], handleOut: [0, 0] });
const quad = (layer, id, x0, y0, x1, y1) => ({ stableTarget: { contextId: 'scene-root', layerUid: layer, frameScope: { kind: 'authored', frame: 0 }, strokeId: id },
  closed: true, count: 4, role: 'plain', filled: true, bounds: [x0, y0, x1, y1], matrix: [1, 0, 0, 1, 0, 0], segments: [seg(x0, y0), seg(x1, y0), seg(x1, y1), seg(x0, y1)],
  style: { fill: '#112233', stroke: '#445566', width: 4, opacity: 1 } });
const tick = () => new Promise((done) => setTimeout(done, 0));
const settle = async () => { for (let i = 0; i < 14; i++) await tick(); };
const LAYERS = [{ layerUid: 'L1' }, { layerUid: 'L2' }];
// L1 stack, bottom to top: a, b, c; L2: z. a and b overlap the marquee (2..95 x 2..65); c is far right.
const ITEMS = () => [quad('L1', 'a', 10, 10, 30, 30), quad('L1', 'b', 50, 20, 90, 60), quad('L1', 'c', 130, 40, 140, 45), quad('L2', 'z', 100, 100, 120, 120)];

// opts: native, answer (status the batch route answers), motion
function harness(opts = {}) {
  const { native = true, answer = 'applied' } = opts;
  const paper = Paper.load(), body = el('body'), toasts = [], batches = [], legacy = [], shown = [];
  let pin = { instanceId: 'i', documentId: 'd', lifecycleGeneration: 1, contentRevision: 1, documentSnapshotId: 's1', canvasW: 200, canvasH: 200, layers: LAYERS };
  const store = ITEMS();
  const stack = (layer) => store.filter((i) => i.stableTarget.layerUid === layer).map((i) => i.stableTarget.strokeId);
  const session = { info: () => pin, isActive: () => true, blocksLegacy: () => native, readPresentedObjectSession: () => ({ presentation: { ...pin, frame: 0 } }),
    async readPresentedStrokes(selector, _pin, wanted) {
      return { items: store.filter((i) => i.stableTarget.layerUid === selector.layerUid).map((item) => { const { segments, ...rest } = item; return wanted(rest) ? { ...rest, segments } : rest; }) };
    } };
  const root = { paper, Path: paper.Path, Point: paper.Point, Matrix: paper.Matrix, state: { tool: 'select', appMode: 'anim2d', currentFrame: 0, activeLayerIdx: 0, layers: LAYERS, selectedStrokeIndices: [], selRotAccum: 0, xformAnchorKey: 'mc' },
    selectedPaths: [], updateUI() {}, SMEngineBridge: { renderNow() {}, isEnabled: () => true }, showToast: (t) => toasts.push(t),
    setTimeout: (fn) => { fn(); return 1; }, clearTimeout() {}, showContextMenu: (x, y, rows) => shown.push({ x, y, rows }),
    document: { body, getElementById: () => ({ getBoundingClientRect: () => ({ left: 0, top: 0, width: 200, height: 200 }) }), createElementNS: (_n, tag) => el(tag),
      addEventListener() {}, removeEventListener() {} },
    requestAnimationFrame: () => {}, pushUndo() { legacy.push('pushUndo'); throw new Error('native authority denied legacy history checkpoint'); },
    NemoNativeObjectProject: { desktop: () => session, selectLayerUI: () => true },
    NemoNativeViewportUnderlay: { canvasMapping: { point: (e) => ({ x: e.clientX, y: e.clientY }), transform: () => [1, 0, 0, 1, 0, 0] } },
    NemoNativeObjectLabs: { desktop: () => ({ batchStrokes: async (layerUid, frame, steps, options) => {
      const body = JSON.parse(JSON.stringify(steps));
      batches.push({ layerUid, frame, steps: body, options: { ...options } });
      // the engine contract admits exactly the ops this adapter sends
      Contract.compose(null).payload(Contract.VECTOR, { command: 'object.vector.batch', ops: body.map((s) => ({ op: 'order', stableTarget: s.order.stableTarget, placement: s.order.mode, frame })) });
      if (answer !== 'applied') return { status: answer };
      body.forEach((s) => {
        const at = store.findIndex((i) => i.stableTarget.strokeId === s.order.stableTarget.strokeId), [item] = store.splice(at, 1);
        if (s.order.mode === 'front') store.push(item);
        else store.splice(store.findIndex((i) => i.stableTarget.layerUid === item.stableTarget.layerUid), 0, item);
      });
      pin = { ...pin, contentRevision: pin.contentRevision + 1, documentSnapshotId: 's' + (pin.contentRevision + 1) };
      return { status: 'applied' };
    } }) },
    NemoSelectCanvasIntent: Intent, NemoNativeSelectOverlay: Overlay, NemoNativeSelectGesture: Gesture, NemoNativeSelectionOrder: Order, NemoNativeSelectMenu: Menu };
  const intent = Intent.create({
    context: () => JSON.stringify([root.state.tool, root.state.activeLayerIdx, (root.selectedPaths || []).map((p) => p.data.strokeId)]),
    probe: (e) => Object.freeze({ mode: 'pick', x: e.clientX, y: e.clientY }),
    select: (d, shift) => Intent.resolvePick(root, Intent, d, shift, Intent.pickContext(root), false),
    gesture: Gesture.bind(root) });
  const target = { setPointerCapture() {}, releasePointerCapture() {}, hasPointerCapture: () => true };
  const ev = (x, y) => ({ button: 0, pointerId: 1, buttons: 1, clientX: x, clientY: y, shiftKey: false, altKey: false, currentTarget: target, stopImmediatePropagation() {}, preventDefault() {} });
  return { root, toasts, batches, store, legacy, shown, stack, bump() { pin = { ...pin, contentRevision: pin.contentRevision + 1, documentSnapshotId: 's' + (pin.contentRevision + 1) }; },
    async marquee(x0, y0, x1, y1) { intent.down(ev(x0, y0)); intent.move(ev(x1, y1)); await settle(); await intent.up({ ...ev(x1, y1), buttons: 0 }); await settle(); },
    ids: () => root.selectedPaths.map((p) => p.data.strokeId) };
}

test('Premier plan sends one front step per selected item in selection order, as ONE batch; the selection stays and the stack is restacked', async () => {
  const h = harness();
  await h.marquee(2, 2, 95, 65);
  assert.deepEqual(h.ids(), ['a', 'b']);
  assert.equal(await Order.front(h.root), 'applied');
  assert.equal(h.batches.length, 1, 'one gesture, one batch (one revision, one history entry)');
  assert.deepEqual(h.batches[0].steps.map((s) => [s.order.stableTarget.strokeId, s.order.mode]), [['a', 'front'], ['b', 'front']]);
  assert.equal(h.batches[0].layerUid, 'L1');
  assert.equal(h.batches[0].options.select, true, 'the Select batch route (components included)');
  assert.deepEqual(h.stack('L1'), ['c', 'a', 'b'], 'bringToFront over [a, b] in order: the last selected ends on top');
  assert.deepEqual(h.ids(), ['a', 'b'], 'the same items stay selected, read again at the new revision');
  assert.deepEqual(h.legacy, [], 'no legacy history checkpoint, no Paper z-order move');
});

test('Arriere-plan sends the steps in REVERSE selection order so the selected items keep their order at the bottom', async () => {
  const h = harness();
  await h.marquee(2, 2, 95, 65);
  assert.equal(await Order.back(h.root), 'applied');
  assert.deepEqual(h.batches[0].steps.map((s) => [s.order.stableTarget.strokeId, s.order.mode]), [['b', 'back'], ['a', 'back']]);
  assert.deepEqual(h.stack('L1'), ['a', 'b', 'c'], 'sendToBack over [b, a]: the first selected ends lowest');
  // the order of a selection of b then a: Premier plan then Arriere-plan restores nothing out of order
  assert.deepEqual(h.stack('L2'), ['z'], 'another layer is untouched');
});

test('a stack already in that order answers noop and shows no failure notice', async () => {
  const h = harness({ answer: 'noop' });
  await h.marquee(2, 2, 95, 65);
  assert.equal(await Order.front(h.root), 'noop');
  assert.deepEqual(h.toasts, []);
  assert.equal(h.batches.length, 1);
});

test('the engine refusing the batch (a locked layer, a path effect) is one notice and nothing else', async () => {
  const h = harness({ answer: 'unavailable' });
  await h.marquee(2, 2, 95, 65);
  assert.equal(await Order.back(h.root), 'failed');
  assert.deepEqual(h.toasts, ['The selection could not be reordered (locked, hidden or unsupported items are refused).']);
  assert.deepEqual(h.stack('L1'), ['a', 'b', 'c'], 'nothing was reordered');
});

test('a document that moved on between the click and the read writes nothing', async () => {
  const h = harness();
  await h.marquee(2, 2, 95, 65);
  const intent = h.root.NemoSelectCanvasIntent, real = intent.presentedState;
  let calls = 0;
  // the presented revision differs between the first and second read of the same request
  const moving = Object.assign({}, intent, { presentedState: (r) => { const s = real(r); if (s && ++calls === 2) return { ...s, pin: { ...s.pin, contentRevision: s.pin.contentRevision + 1 } }; return s; } });
  h.root.NemoSelectCanvasIntent = moving;
  assert.equal(await Order.front(h.root), 'stale');
  assert.equal(h.batches.length, 0);
  assert.deepEqual(h.toasts, ['The document changed — nothing was reordered.']);
});

test('nothing selected resolves to null and writes nothing; a request in flight is not doubled', async () => {
  const h = harness();
  assert.equal(await Order.front(h.root), null);
  await h.marquee(2, 2, 95, 65);
  const first = Order.front(h.root), second = Order.back(h.root);
  assert.equal(await second, 'busy');
  assert.equal(await first, 'applied');
  assert.equal(h.batches.length, 1);
  assert.equal(await Order.order(h.root, 'sideways'), 'refused');
});

// R-lane-a-fixes: the baseline rows run in any mode. Motion edits the key a held frame inherits (Animation 2D keys the held cell with a copy first).
test('Motion reorders too, asking the route for the inherited key; Animation 2D keeps the copy key', async () => {
  const h = harness();
  await h.marquee(2, 2, 95, 65);
  assert.equal(await Order.front(h.root), 'applied');
  assert.equal(h.batches[0].options.inherit, false, 'Animation 2D: the held cell is keyed with a copy');
  h.root.state.appMode = 'motion';
  await h.marquee(2, 2, 95, 65);
  assert.equal(await Order.back(h.root), 'applied');
  assert.equal(h.batches.length, 2);
  assert.equal(h.batches[1].options.inherit, true, 'Motion: no copy key');
  assert.equal(h.batches[1].options.select, true);
  assert.deepEqual(h.toasts, []);
});

test('the Labs batch route in Motion sends no copy key and addresses each step to the key its record lives in', async () => {
  const Labs = require('../src/js/adapters/native-object-labs.js');
  const sent = [];
  const host = { NemoNativeObjectVector: { create: () => ({ batchStrokes: async (layerUid, frame, steps, options) => { sent.push({ layerUid, frame, steps, options }); return { status: 'applied' }; } }) } };
  const route = Labs.create({ info: () => ({ layers: [] }) }, host);
  const target = (frame, id) => ({ contextId: 'scene-root', layerUid: 'L1', frameScope: { kind: 'authored', frame }, strokeId: id });
  const steps = [{ order: { stableTarget: target(2, 'a'), mode: 'front' } }, { reshape: { stableTarget: target(2, 'b'), matrix: [-1, 0, 0, 1, 10, 0] } }, { delete: 'c' }];
  await route.batchStrokes('L1', 7, steps, { select: true });
  assert.deepEqual(sent[0].options, { keyframe: 'copy' }, 'Animation 2D: the held frame 7 is keyed with a copy');
  assert.equal(sent[0].steps[0].frame, undefined);
  await route.batchStrokes('L1', 7, steps, { select: true, inherit: true });
  assert.deepEqual(sent[1].options, {}, 'Motion: no copy key');
  assert.deepEqual(sent[1].steps.map((x) => x.frame), [2, 2, undefined], 'the steps that name a presented record go to its own key; the others keep the batch frame');
  assert.deepEqual(steps.map((x) => x.frame), [undefined, undefined, undefined], 'the caller\'s steps are not touched');
});

test('the steps are pure: front keeps the order given, back reverses it', () => {
  const path = (id) => ({ data: { nativeStableTarget: { strokeId: id }, strokeId: id, nativeLayerIndex: 0 } });
  assert.deepEqual(Order.stepsOf([path('x'), path('y'), path('z')], 'front').map((s) => s.order.stableTarget.strokeId), ['x', 'y', 'z']);
  assert.deepEqual(Order.stepsOf([path('x'), path('y'), path('z')], 'back').map((s) => s.order.stableTarget.strokeId), ['z', 'y', 'x']);
});

test('the Select right-click menu offers Premier plan and Arriere-plan after the group rows and routes each to the order adapter', async () => {
  const h = harness();
  await h.marquee(2, 2, 95, 65);
  const calls = [];
  h.root.NemoNativeSelectionClipboard = { native: () => true, hasClip: () => false, copy() {}, cut() {}, paste() {}, duplicate() {} };
  h.root.NemoNativeSelectionOrder = { front: (host) => { calls.push(['front', host]); }, back: (host) => { calls.push(['back', host]); } };
  const rows = await Menu.rows(h.root, h.root.selectedPaths);
  const labels = rows.map((r) => (r.sep ? '---' : r.label));
  assert.deepEqual(labels.slice(-3), ['---', 'Premier plan', 'Arrière-plan']);
  rows.find((r) => r.label === 'Premier plan').action();
  rows.find((r) => r.label === 'Arrière-plan').action();
  assert.deepEqual(calls, [['front', h.root], ['back', h.root]]);
  // without the adapter the rows are not offered (no dead entry)
  delete h.root.NemoNativeSelectionOrder;
  assert.equal((await Menu.rows(h.root, h.root.selectedPaths)).some((r) => /plan/.test(r.label || '')), false);
});

test('the vector contract admits an order step by a target, front or back and an optional frame, and nothing else', () => {
  const t = { contextId: 'scene-root', frameScope: { kind: 'authored', frame: 0 }, layerUid: 'L1', strokeId: 'a' };
  const admit = (op) => Contract.compose(null).payload(Contract.VECTOR, { command: 'object.vector.batch', ops: [op] });
  assert.doesNotThrow(() => admit({ op: 'order', stableTarget: t, placement: 'front' }));
  assert.doesNotThrow(() => admit({ op: 'order', stableTarget: t, placement: 'back', frame: 3 }));
  for (const bad of [{ op: 'order', stableTarget: t }, { op: 'order', stableTarget: t, placement: 'behind' }, { op: 'order', stableTarget: t, placement: 'front', frame: -1 },
    { op: 'order', stableTarget: t, placement: 'front', keyframe: 'copy' }, { op: 'order', placement: 'front' }]) {
    assert.throws(() => admit(bad), undefined, JSON.stringify(bad));
  }
});

test('the vector adapter turns an order step into the engine op in the cell of the playhead frame, keyed with a copy', async () => {
  const Vector = require('../src/js/adapters/native-object-vector.js');
  const { createFixture } = require('./native-object-vector-support.cjs');
  const fixture = createFixture(), vector = Vector.create(fixture.session);
  const t = { contextId: 'scene-root', frameScope: { kind: 'authored', frame: 2 }, layerUid: 'L1', strokeId: 'a' };
  await vector.batchStrokes('L1', 5, [{ order: { stableTarget: t, mode: 'back' } }, { order: { stableTarget: t, mode: 'front' } }], { keyframe: 'copy', select: true });
  assert.equal(fixture.owner.log.received.length, 1);
  assert.deepEqual(fixture.owner.log.received[0].payload, { command: 'object.vector.batch', keyframe: 'copy',
    ops: [{ op: 'order', stableTarget: t, placement: 'back', frame: 5 }, { op: 'order', stableTarget: t, placement: 'front', frame: 5 }] });
});
