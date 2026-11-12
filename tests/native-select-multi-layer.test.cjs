'use strict';
// #2372 MULTI-LAYER-BATCH: a Select over items of SEVERAL layers (a marquee reads every visible unlocked layer, a Shift pick adds from another)
// restyles, moves, scales and turns them as ONE native object.vector.batch (every step names its own layer), as the baseline did in one undo
// step; the overlay draws one combined box; a locked or hidden layer is never read.
const test = require('node:test'), assert = require('node:assert/strict');
const Intent = require('../src/js/adapters/select-canvas-intent.js');
const Overlay = require('../src/js/adapters/native-select-overlay.js');
const Gesture = require('../src/js/adapters/native-select-gesture.js');
const Style = require('../src/js/adapters/native-selection-style.js');
const Steps = require('../src/js/adapters/native-selection-steps.js');
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
const settle = async () => { for (let i = 0; i < 8; i++) await tick(); };
const LAYERS = [{ layerUid: 'L1' }, { layerUid: 'L2' }, { layerUid: 'L3', locked: true }, { layerUid: 'L4', visible: false }];
const ITEMS = [quad('L1', 'a', 10, 10, 30, 30), quad('L2', 'b', 50, 10, 70, 30), quad('L3', 'c', 10, 50, 30, 70), quad('L4', 'd', 50, 50, 70, 70)];

function harness(items = ITEMS) {
  const paper = Paper.load(), body = el('body'), listeners = [], toasts = [], batches = [], timers = [];
  let pin = { instanceId: 'i', documentId: 'd', lifecycleGeneration: 1, contentRevision: 1, documentSnapshotId: 's1', canvasW: 200, canvasH: 200, layers: LAYERS };
  const store = items.map((i) => JSON.parse(JSON.stringify(i))), reads = [];
  const session = { info: () => pin, isActive: () => true, blocksLegacy: () => true, readPresentedObjectSession: () => ({ presentation: { ...pin, frame: 0 } }),
    async readPresentedStrokes(selector, _pin, wanted) {
      reads.push(selector.layerUid);
      return { items: store.filter((i) => i.stableTarget.layerUid === selector.layerUid).map((item) => { const { segments, ...rest } = item; return wanted(rest) ? { ...rest, segments } : rest; }) };
    } };
  const root = { paper, Path: paper.Path, Point: paper.Point, Matrix: paper.Matrix, state: { tool: 'select', appMode: 'anim2d', currentFrame: 0, activeLayerIdx: 0, layers: LAYERS, selectedStrokeIndices: [] },
    selectedPaths: [], updateUI() {}, SMEngineBridge: { renderNow() {} }, showToast: (t) => toasts.push(t),
    setTimeout: (fn) => { timers.push(fn); return timers.length; }, clearTimeout: (id) => { timers[id - 1] = null; },
    document: { body, getElementById: () => ({ getBoundingClientRect: () => ({ left: 0, top: 0, width: 200, height: 200 }) }), createElementNS: (_n, tag) => el(tag),
      addEventListener: (type, fn) => listeners.push([type, fn]), removeEventListener: (type, fn) => { const at = listeners.findIndex((l) => l[0] === type && l[1] === fn); if (at >= 0) listeners.splice(at, 1); } },
    requestAnimationFrame: () => {},
    NemoNativeObjectProject: { desktop: () => session, selectLayerUI: (_r, at) => { root.state.activeLayerIdx = at; return true; } },
    NemoNativeViewportUnderlay: { canvasMapping: { point: (e) => ({ x: e.clientX, y: e.clientY }), transform: () => [1, 0, 0, 1, 0, 0] } },
    NemoNativeObjectLabs: { desktop: () => ({ batchStrokes: async (layerUid, frame, steps) => {
      batches.push({ layerUid, frame, steps: JSON.parse(JSON.stringify(steps)) });
      steps.forEach((s) => {
        if (s.restyle) { const it = store.find((i) => i.stableTarget.strokeId === s.restyle.stableTarget.strokeId); it.style = { ...it.style, ...s.restyle.style }; return; }
        const it = store.find((i) => i.stableTarget.strokeId === s.reshape.stableTarget.strokeId), m = s.reshape.matrix, at = (q) => [m[0] * q[0] + m[2] * q[1] + m[4], m[1] * q[0] + m[3] * q[1] + m[5]];
        it.segments = it.segments.map((q) => ({ point: at(q.point), handleIn: q.handleIn, handleOut: q.handleOut }));
        const xs = it.segments.map((q) => q.point[0]), ys = it.segments.map((q) => q.point[1]);
        it.bounds = [Math.min(...xs), Math.min(...ys), Math.max(...xs), Math.max(...ys)];
      });
      pin = { ...pin, contentRevision: pin.contentRevision + 1, documentSnapshotId: 's' + (pin.contentRevision + 1) };
      return { status: 'applied' };
    } }) },
    NemoSelectCanvasIntent: Intent, NemoNativeSelectOverlay: Overlay, NemoNativeSelectionSteps: Steps, NemoNativeSelectionStyle: Style };
  root.NemoNativeSelectGesture = Gesture.bind(root);
  const intent = Intent.create({
    context: () => JSON.stringify([root.state.tool, root.state.activeLayerIdx, (root.selectedPaths || []).map((p) => p.data.strokeId)]),
    probe: (e) => Object.freeze({ mode: 'pick', x: e.clientX, y: e.clientY }),
    select: (d, shift) => Intent.resolvePick(root, Intent, d, shift, Intent.pickContext(root), false),
    gesture: root.NemoNativeSelectGesture });
  const target = { setPointerCapture() {}, releasePointerCapture() {}, hasPointerCapture: () => true };
  const ev = (x, y, extra) => ({ button: 0, pointerId: 1, buttons: 1, clientX: x, clientY: y, shiftKey: false, altKey: false, currentTarget: target,
    stopImmediatePropagation() {}, preventDefault() {}, ...extra });
  return { root, toasts, batches, store, body, reads, timers,
    press: (x, y, extra) => intent.down(ev(x, y, extra)), drag: (x, y, extra) => intent.move(ev(x, y, extra)), release: (x, y, extra) => intent.up(ev(x, y, { buttons: 0, ...extra })),
    svg: () => body.children.filter((c) => c.tag === 'svg').slice(-1)[0], ids: () => root.selectedPaths.map((p) => p.data.strokeId),
    async marquee(x0, y0, x1, y1, extra) { const h = this; h.press(x0, y0, extra); h.drag(x1, y1, extra); await settle(); await h.release(x1, y1, extra); await settle(); },
    fire: async () => { timers.splice(0).filter(Boolean).forEach((fn) => fn()); await settle(); } };
}

test('a marquee across two layers selects both, skips the locked and hidden layers and never reads them', async () => {
  const h = harness();
  await h.marquee(2, 2, 190, 190);
  assert.deepEqual(h.ids(), ['a', 'b'], 'items of the visible unlocked layers, bottom layer first');
  assert.deepEqual(h.root.selectedPaths.map((p) => p.data.nativeLayerUid), ['L1', 'L2']);
  assert.equal(h.root.state.activeLayerIdx, 0, 'the active layer stays where it was');
  assert.deepEqual([...new Set(h.reads)], ['L1', 'L2'], 'a locked or hidden layer is not even read');
  assert.equal(h.batches.length, 0);
});

test('a Shift pick on another layer adds to the selection, which then spans layers', async () => {
  const h = harness();
  h.press(20, 20); await h.release(20, 20); await settle();
  assert.deepEqual(h.ids(), ['a']);
  h.press(60, 20, { shiftKey: true }); await h.release(60, 20, { shiftKey: true }); await settle();
  assert.deepEqual(h.ids(), ['a', 'b']);
  assert.deepEqual(h.root.selectedPaths.map((p) => p.data.nativeLayerIndex), [0, 1]);
});

test('the overlay draws ONE combined box, ring and 8 handles for a selection across layers', async () => {
  const h = harness();
  await h.marquee(2, 2, 190, 100);
  const svg = h.svg(), box = svg.children.filter((c) => c.tag === 'polygon' && !/0\.08/.test(c.attrs.fill || ''));
  assert.equal(box.length, 1, 'one oriented box');
  assert.equal(box[0].attrs.points, '10,10 70,10 70,30 10,30', 'the union of the items of both layers');
  assert.equal(svg.children.filter((c) => c.tag === 'circle').length, 1, 'one rotate ring');
  assert.equal(svg.children.filter((c) => c.tag === 'rect').length, 8, 'one set of handles');
  assert.equal(svg.children.filter((c) => c.tag === 'path').length, 2, 'an outline per item');
});

test('moving a two-layer selection is ONE batch of reshape steps naming both layers, and the selection is re-read on both', async () => {
  const h = harness();
  await h.marquee(2, 2, 190, 100);
  h.press(30, 20); h.drag(40, 25); h.drag(50, 30); await settle();
  assert.equal(h.batches.length, 0, 'nothing is written while dragging');
  await h.release(50, 30); await settle();
  assert.equal(h.batches.length, 1, 'one commit for both layers');
  assert.deepEqual(h.batches[0].steps.map((s) => s.reshape.stableTarget.layerUid), ['L1', 'L2']);
  assert.deepEqual(h.batches[0].steps.map((s) => s.reshape.matrix), [[1, 0, 0, 1, 20, 10], [1, 0, 0, 1, 20, 10]]);
  assert.equal(h.toasts.length, 0, h.toasts.join('|'));
  Contract.payload(Contract.VECTOR, { command: 'object.vector.batch', ops: h.batches[0].steps.map((s) => ({ op: 'reshape', stableTarget: s.reshape.stableTarget, matrix: s.reshape.matrix, frame: 0 })) });
  assert.deepEqual(h.ids(), ['a', 'b']);
  assert.deepEqual(h.root.selectedPaths.map((p) => [p.bounds.x, p.bounds.y]), [[30, 20], [70, 20]], 'both re-read at the new geometry');
});

test('scaling a two-layer selection by its corner handle is one batch about the combined box', async () => {
  const h = harness();
  await h.marquee(2, 2, 190, 100);
  h.press(70, 30); h.drag(130, 50); await settle(); await h.release(130, 50); await settle();
  assert.equal(h.batches.length, 1);
  assert.deepEqual(h.batches[0].steps.map((s) => s.reshape.stableTarget.layerUid), ['L1', 'L2']);
  assert.deepEqual(h.batches[0].steps[0].reshape.matrix, h.batches[0].steps[1].reshape.matrix, 'one transform of the union box');
  assert.equal(h.batches[0].steps[0].reshape.matrix[0], 2);
});

test('a document that changed during the drag writes nothing for a two-layer selection', async () => {
  const h = harness();
  await h.marquee(2, 2, 190, 100);
  h.press(20, 20); h.drag(40, 30); await settle();
  h.store.splice(1, 1); // the second layer's item is gone from the listing
  await h.release(40, 30); await settle();
  assert.equal(h.batches.length, 0);
  assert.match(h.toasts.join('|'), /changed/);
});

test('a restyle of a two-layer selection is ONE batch of restyle steps (one history entry)', async () => {
  const h = harness();
  await h.marquee(2, 2, 190, 100);
  assert.equal(Style.handles(h.root), true);
  const done = Style.apply(h.root, { opacity: 0.5 });
  await h.fire();
  assert.deepEqual(await done, { status: 'applied' });
  assert.equal(h.batches.length, 1, 'one batch for both layers');
  assert.deepEqual(h.batches[0].steps.map((s) => [s.restyle.stableTarget.layerUid, s.restyle.stableTarget.strokeId]), [['L1', 'a'], ['L2', 'b']]);
  assert.doesNotMatch(h.toasts.join('|'), /one layer at a time/);
  Contract.payload(Contract.VECTOR, { command: 'object.vector.batch', ops: h.batches[0].steps.map((s) => ({ op: 'restyle', stableTarget: s.restyle.stableTarget, style: s.restyle.style, frame: 0 })) });
  assert.deepEqual(h.ids(), ['a', 'b'], 'the selection is re-read on both layers');
  assert.equal(h.root.selectedPaths[1].data.nativeStyle.opacity, 0.5);
});

test('the vector contract admits a batch across scene-root layers and still refuses another context', () => {
  const target = (layerUid, contextId = 'scene-root') => ({ contextId, frameScope: { kind: 'authored', frame: 0 }, layerUid, strokeId: 's' });
  const ops = (a, b) => [{ op: 'restyle', stableTarget: a, style: { width: 3 } }, { op: 'reshape', stableTarget: b, matrix: [1, 0, 0, 1, 2, 2] }];
  Contract.payload(Contract.VECTOR, { command: 'object.vector.batch', ops: ops(target('L1'), target('L2')) });
  assert.throws(() => Contract.payload(Contract.VECTOR, { command: 'object.vector.batch', ops: ops(target('L1'), target('L2', 'component:c')) }), /not admitted/);
});
