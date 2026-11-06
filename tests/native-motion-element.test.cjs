'use strict';
// #2520/#2525 C-motion-canvas-4: Select in Motion on a native object project. The pick, the hover and the overlay work in Motion; a double-click
// enters the group of the active layer and targets the shape under it; a drag of the targeted shape is the baseline elementMove, ONE
// object.property.edit of the element's Position (the world delta pulled back through the layer pose), never a vector.batch of its geometry.
const test = require('node:test'), assert = require('node:assert/strict');
const Intent = require('../src/js/adapters/select-canvas-intent.js');
const Overlay = require('../src/js/adapters/native-select-overlay.js');
const Gesture = require('../src/js/adapters/native-select-gesture.js');
const Pose = require('../src/js/adapters/native-layer-motion-pose.js');
const Element = require('../src/js/adapters/native-motion-element.js');
const Wait = require('../src/js/adapters/native-presentation-wait.js');
const Paper = require('./helpers/native-import-paper.cjs');

function el(tag) {
  return { tag, attrs: {}, style: {}, children: [], setAttribute(k, v) { this.attrs[k] = String(v); },
    appendChild(c) { this.children.push(c); c.parentNode = this; return c; },
    remove() { if (this.parentNode) this.parentNode.children = this.parentNode.children.filter((c) => c !== this); } };
}
const seg = (x, y) => ({ point: [x, y], handleIn: [0, 0], handleOut: [0, 0] });
const quad = (id, x0, y0, x1, y1, extra) => ({ stableTarget: { contextId: 'scene-root', layerUid: 'L', frameScope: { kind: 'authored', frame: 0 }, strokeId: id },
  closed: true, count: 4, role: 'plain', filled: true, bounds: [x0, y0, x1, y1], matrix: [1, 0, 0, 1, 0, 0], segments: [seg(x0, y0), seg(x1, y0), seg(x1, y1), seg(x0, y1)], ...extra });
const tick = () => new Promise((done) => setTimeout(done, 0));
const settle = async () => { for (let i = 0; i < 6; i++) await tick(); };

function harness(items, options = {}) {
  const paper = Paper.load(), body = el('body'), listeners = [], toasts = [], batches = [], frames = [];
  let busy = false; // the native queue has a command pending: the public presentation getter reports none (queue-pending)
  let pin = { instanceId: 'i', documentId: 'd', lifecycleGeneration: 1, contentRevision: 1, documentSnapshotId: 's1', canvasW: 200, canvasH: 200, layers: [{ layerUid: 'L', hasMotion: !!options.layerMatrix }] };
  const reads = [], edits = [], props = { position: [100, 50] };
  const propSession = { readProperty: async (selector) => { reads.push(selector); return { value: props.position.slice() }; },
    editProperty: async (payload) => { edits.push(payload); if (options.refuseEdit) return { ok: true, result: { applied: false } }; props.position = payload.action.values.slice(); pin = { ...pin, contentRevision: pin.contentRevision + 1, documentSnapshotId: 's' + (pin.contentRevision + 1) }; return { ok: true, result: { applied: true } }; } };
  const store = items.map((i) => JSON.parse(JSON.stringify(i)));
  const session = { info: () => pin, isActive: () => true, blocksLegacy: () => true, readPresentedObjectSession: () => (busy ? null : { presentation: { ...pin, frame: 0 } }),
    async readPresentedStrokes(_selector, _pin, wanted, centres) {
      return { layerMatrix: options.layerMatrix || [1, 0, 0, 1, 0, 0], items: store.map((item) => { const { segments, center, ...rest } = item; return wanted(rest) ? { ...rest, segments, ...(centres && center ? { center } : {}) } : rest; }) };
    } };
  const root = { NemoNativePresentationWait: Wait, paper, Path: paper.Path, Point: paper.Point, Matrix: paper.Matrix, state: { tool: 'select', appMode: 'motion', currentFrame: 0, activeLayerIdx: 0, layers: [{ layerUid: 'L' }], selectedStrokeIndices: [] },
    selectedPaths: [], updateUI() { if (options.busyOnRepaint) busy = true; }, SMEngineBridge: { renderNow() {} }, showToast: (t) => toasts.push(t),
    document: { body, getElementById: () => ({ getBoundingClientRect: () => ({ left: 0, top: 0, width: 200, height: 200 }) }), createElementNS: (_n, tag) => el(tag),
      addEventListener: (type, fn) => listeners.push([type, fn]), removeEventListener: (type, fn) => { const at = listeners.findIndex((l) => l[0] === type && l[1] === fn); if (at >= 0) listeners.splice(at, 1); } },
    requestAnimationFrame: (fn) => frames.push(fn),
    NemoNativeObjectProject: { desktop: () => session, selectLayerUI: () => true },
    NemoNativeViewportUnderlay: { canvasMapping: { point: (e) => ({ x: e.clientX, y: e.clientY }), transform: () => [1, 0, 0, 1, 0, 0] } },
    NemoNativeObjectLabs: { desktop: () => ({ batchStrokes: async (layerUid, frame, steps) => { batches.push({ layerUid, frame, steps }); return { status: 'applied' }; } }) },
    NemoNativeObjectWidgetUI: { session: () => propSession, proofNow: () => ({ session: propSession, info: pin, selector: { contextId: 'scene-root', frame: 0 }, current: () => true }) },
    NemoNativeMotionElement: Element, NemoNativeLayerMotionPose: Pose,
    NemoSelectCanvasIntent: Intent,
    // busyAtShow: the pick's read has resolved but its queue entry is released a tick later, so applyPick draws inside that lag
    NemoNativeSelectOverlay: options.busyAtShow ? { ...Overlay, show: (r) => { busy = true; return Overlay.show(r); } } : Overlay };
  root.NemoNativeSelectGesture = Gesture.bind(root);
  const intent = Intent.create({
    context: () => JSON.stringify([root.state.tool, root.state.activeLayerIdx, (root.selectedPaths || []).map((p) => p.data.strokeId)]),
    probe: (e) => Object.freeze({ mode: 'pick', x: e.clientX, y: e.clientY }),
    select: (d, shift) => Intent.resolvePick(root, Intent, d, shift, Intent.pickContext(root), false),
    gesture: root.NemoNativeSelectGesture });
  const target = { setPointerCapture() {}, releasePointerCapture() {}, hasPointerCapture: () => true };
  const ev = (x, y, extra) => ({ button: 0, pointerId: 1, buttons: 1, clientX: x, clientY: y, shiftKey: false, altKey: false, currentTarget: target,
    stopImmediatePropagation() {}, preventDefault() {}, ...extra });
  const h = { root, intent, toasts, batches, reads, edits, props, store, body, listeners, frames,
    busy: (on) => { busy = on; }, runFrames: () => { frames.splice(0).forEach((f) => f()); }, // one animation frame: callbacks queued meanwhile wait for the next
    setPin: (n) => { pin = { ...pin, ...n }; },
    press: (x, y, extra) => intent.down(ev(x, y, extra)), drag: (x, y, extra) => intent.move(ev(x, y, extra)), release: (x, y, extra) => intent.up(ev(x, y, { buttons: 0, ...extra })),
    key: (key) => listeners.filter((l) => l[0] === 'keydown').forEach((l) => l[1]({ key, stopImmediatePropagation() {}, preventDefault() {} })),
    svg: () => body.children.filter((c) => c.tag === 'svg').slice(-1)[0],
    async select(x, y, shift) { h.press(x, y, { shiftKey: !!shift }); await h.release(x, y, { shiftKey: !!shift }); await settle(); },
    ids: () => root.selectedPaths.map((p) => p.data.strokeId) };
  return h;
}
// The anchors a plain stroke ends at: its pre-batch anchors (recorded by the harness) mapped by the step's record-space matrix.
const pts = (step) => { const m = step.reshape.matrix; return step.before.map((q) => [m[0] * q[0] + m[2] * q[1] + m[4], m[1] * q[0] + m[3] * q[1] + m[5]].map((v) => Math.round(v * 1e6) / 1e6 + 0)); };

const BIG = quad('A', 20, 20, 120, 120), OTHER = quad('B', 140, 140, 190, 190);
const dbl = async (h, x, y) => { const taken = await Element.enter(h.root, { clientX: x, clientY: y }); await settle(); return taken; };

test('Motion: the pick and the hover work, and the overlay draws the shape box without scale handles or a rotate ring', async () => {
  const h = harness([BIG, OTHER]);
  await h.select(40, 40);
  assert.deepEqual(h.ids(), ['A'], 'a click picks in Motion');
  const svg = h.svg();
  assert.ok(svg.children.some((c) => c.tag === 'polygon'), 'the box is drawn');
  assert.ok(!svg.children.some((c) => c.tag === 'rect'), 'no scale handles in Motion');
  assert.ok(!svg.children.some((c) => c.tag === 'circle'), 'no rotate ring in Motion');
  Overlay.hoverAt(h.root, { buttons: 0, clientX: 160, clientY: 160 }); await settle();
  assert.equal(Overlay.selectionContext(h.root), Intent.pickContext(h.root), 'the drawn selection and the pick context agree in Motion');
});

test('Motion: a drag of a shape that is not isolated selects it and writes nothing', async () => {
  const h = harness([BIG, OTHER]);
  h.press(40, 40); h.drag(60, 70); await settle(); await h.release(60, 70); await settle();
  assert.deepEqual(h.ids(), ['A']);
  assert.deepEqual(h.edits, []);
  assert.deepEqual(h.batches, []);
});

test('a double-click enters the group: it targets the shape under the pointer, and a drag of it is ONE Position edit', async () => {
  const h = harness([BIG, OTHER]);
  assert.equal(Element.isolated(h.root), false);
  assert.equal(await dbl(h, 40, 40), true);
  assert.deepEqual(h.ids(), ['A']);
  assert.equal(Element.isolated(h.root), true, 'the layer gizmo gives way to the shape box');
  h.press(40, 40); h.drag(50, 45); h.drag(70, 55); await settle();
  assert.deepEqual(h.edits, [], 'no write while dragging');
  await h.release(70, 55); await settle();
  assert.deepEqual(h.batches, [], 'never a geometry batch');
  assert.equal(h.edits.length, 1, 'one gesture, one edit');
  assert.deepEqual(h.edits[0], { command: 'object.property.edit', target: { contextId: 'scene-root', layerUid: 'L', strokeId: 'A' }, property: 'position',
    action: { kind: 'setValue', frame: 0, values: [130, 65] } }, 'the stored Position (100,50) plus the drag (30,15)');
  assert.deepEqual(h.reads[0].view, { kind: 'rawSample', target: { contextId: 'scene-root', layerUid: 'L', strokeId: 'A' }, property: 'position', frame: 0 });
  assert.deepEqual(h.ids(), ['A'], 'the element stays targeted');
  assert.equal(Element.isolated(h.root), true);
});

test('Shift locks the element move onto the nearer axis of the total displacement', async () => {
  const h = harness([BIG]);
  await dbl(h, 40, 40);
  h.press(40, 40); h.drag(80, 50, { shiftKey: true }); await settle(); await h.release(80, 50, { shiftKey: true }); await settle();
  assert.deepEqual(h.edits.map((e) => e.action.values), [[140, 50]]);
});

test('the drag delta is pulled back through the layer pose (a layer turned by 90 degrees)', async () => {
  const h = harness([BIG], { layerMatrix: [0, 1, -1, 0, 0, 0] });
  await dbl(h, 40, 40);
  h.press(40, 40); h.drag(70, 55); await settle(); await h.release(70, 55); await settle();
  assert.deepEqual(h.edits.map((e) => e.action.values), [[115, 20]], 'world (30,15) is local (15,-30)');
});

test('Escape ends the drag with no write, and a refused edit leaves the targeted shape in place', async () => {
  const h = harness([BIG]);
  await dbl(h, 40, 40);
  h.press(40, 40); h.drag(70, 55); await settle(); h.key('Escape'); await h.release(70, 55); await settle();
  assert.deepEqual(h.edits, []);
  const h2 = harness([BIG], { refuseEdit: true });
  await dbl(h2, 40, 40);
  h2.press(40, 40); h2.drag(70, 55); await settle(); await h2.release(70, 55); await settle();
  assert.equal(h2.edits.length, 1);
  assert.deepEqual(h2.props.position, [100, 50]);
});

test('Alt does not copy in Motion, a second shape cannot be dragged with the first, and a click elsewhere leaves the group', async () => {
  const h = harness([BIG, OTHER]);
  await dbl(h, 40, 40);
  h.press(40, 40, { altKey: true }); h.drag(70, 55, { altKey: true }); await settle(); await h.release(70, 55, { altKey: true }); await settle();
  assert.equal(h.edits.length, 1, 'a plain element move, no duplicate');
  await h.select(10, 190); // empty canvas
  assert.deepEqual(h.ids(), []);
  assert.equal(Element.isolated(h.root), false, 'nothing targeted: the layer gizmo returns');
});

test('outside Motion nothing is isolated and the double-click does nothing', async () => {
  const h = harness([BIG]);
  h.root.state.appMode = 'anim2d';
  assert.equal(await dbl(h, 40, 40), false);
  assert.equal(Element.isolated(h.root), false);
});
