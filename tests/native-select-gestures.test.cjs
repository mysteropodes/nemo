'use strict';
// #2372 SEL-F6: Select gestures on a native object project. A drag over the presented layer is a marquee, a move, a handle scale or
// a ring rotate: previewed in the select overlay while it runs (no document write), committed on release as ONE native
// object.vector.batch of reshapes against the document the drag began on, and ended with no write by Escape or a stale document.
const test = require('node:test'), assert = require('node:assert/strict');
const Intent = require('../src/js/adapters/select-canvas-intent.js');
const Overlay = require('../src/js/adapters/native-select-overlay.js');
const Gesture = require('../src/js/adapters/native-select-gesture.js');
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
  let pin = { instanceId: 'i', documentId: 'd', lifecycleGeneration: 1, contentRevision: 1, documentSnapshotId: 's1', canvasW: 200, canvasH: 200, layers: [{ layerUid: 'L' }] };
  const store = items.map((i) => JSON.parse(JSON.stringify(i)));
  const session = { info: () => pin, isActive: () => true, blocksLegacy: () => true, readPresentedObjectSession: () => ({ presentation: { ...pin, frame: 0 } }),
    async readPresentedStrokes(_selector, _pin, wanted, centres) {
      return { items: store.map((item) => { const { segments, center, ...rest } = item; return wanted(rest) ? { ...rest, segments, ...(centres && center ? { center } : {}) } : rest; }) };
    } };
  const root = { paper, Path: paper.Path, Point: paper.Point, Matrix: paper.Matrix, state: { tool: 'select', appMode: 'anim2d', currentFrame: 0, activeLayerIdx: 0, layers: [{ layerUid: 'L' }], selectedStrokeIndices: [] },
    selectedPaths: [], updateUI() {}, SMEngineBridge: { renderNow() {} }, showToast: (t) => toasts.push(t),
    document: { body, getElementById: () => ({ getBoundingClientRect: () => ({ left: 0, top: 0, width: 200, height: 200 }) }), createElementNS: (_n, tag) => el(tag),
      addEventListener: (type, fn) => listeners.push([type, fn]), removeEventListener: (type, fn) => { const at = listeners.findIndex((l) => l[0] === type && l[1] === fn); if (at >= 0) listeners.splice(at, 1); } },
    requestAnimationFrame: (fn) => frames.push(fn),
    NemoNativeObjectProject: { desktop: () => session, selectLayerUI: () => true },
    NemoNativeViewportUnderlay: { canvasMapping: { point: (e) => ({ x: e.clientX, y: e.clientY }), transform: () => [1, 0, 0, 1, 0, 0] } },
    NemoNativeObjectLabs: { desktop: () => ({ batchStrokes: async (layerUid, frame, steps) => {
      batches.push({ layerUid, frame, steps });
      if (options.refuse) return { status: 'invalid', reason: 'refused' };
      steps.forEach((s) => { const it = store.find((i) => i.stableTarget.strokeId === s.reshape.stableTarget.strokeId);
        if (s.reshape.geometry) it.segments = s.reshape.geometry.segments.map((q) => ({ point: [q.point.x, q.point.y], handleIn: [0, 0], handleOut: [0, 0] }));
        if (s.reshape.center) it.center = s.reshape.center.map((c) => ({ point: [c.point.x, c.point.y], handleIn: [c.handleIn.x, c.handleIn.y], handleOut: [c.handleOut.x, c.handleOut.y], width: c.width })); });
      pin = { ...pin, contentRevision: pin.contentRevision + 1, documentSnapshotId: 's' + (pin.contentRevision + 1) };
      return { status: 'applied' };
    } }) },
    NemoSelectCanvasIntent: Intent, NemoNativeSelectOverlay: Overlay };
  root.NemoNativeSelectGesture = Gesture.bind(root);
  const intent = Intent.create({
    context: () => JSON.stringify([root.state.tool, root.state.activeLayerIdx, (root.selectedPaths || []).map((p) => p.data.strokeId)]),
    probe: (e) => Object.freeze({ mode: 'pick', x: e.clientX, y: e.clientY }),
    select: (d, shift) => Intent.resolvePick(root, Intent, d, shift, Intent.pickContext(root), false),
    gesture: root.NemoNativeSelectGesture });
  const target = { setPointerCapture() {}, releasePointerCapture() {}, hasPointerCapture: () => true };
  const ev = (x, y, extra) => ({ button: 0, pointerId: 1, buttons: 1, clientX: x, clientY: y, shiftKey: false, altKey: false, currentTarget: target,
    stopImmediatePropagation() {}, preventDefault() {}, ...extra });
  const h = { root, intent, toasts, batches, store, body, listeners,
    setPin: (n) => { pin = { ...pin, ...n }; },
    press: (x, y, extra) => intent.down(ev(x, y, extra)), drag: (x, y, extra) => intent.move(ev(x, y, extra)), release: (x, y, extra) => intent.up(ev(x, y, { buttons: 0, ...extra })),
    key: (key) => listeners.filter((l) => l[0] === 'keydown').forEach((l) => l[1]({ key, stopImmediatePropagation() {}, preventDefault() {} })),
    svg: () => body.children.filter((c) => c.tag === 'svg').slice(-1)[0],
    async select(x, y, shift) { h.press(x, y, { shiftKey: !!shift }); await h.release(x, y, { shiftKey: !!shift }); await settle(); },
    ids: () => root.selectedPaths.map((p) => p.data.strokeId) };
  return h;
}
const pts = (step) => JSON.parse(JSON.stringify(step.reshape.geometry.segments.map((s) => [s.point.x, s.point.y].map((v) => Math.round(v * 1e6) / 1e6 + 0))));
const BIG = quad('A', 20, 20, 120, 120), FAR = quad('B', 150, 150, 190, 190);

test('a marquee selects the items whose bounds meet the rectangle, previews it live, and writes nothing', async () => {
  const h = harness([quad('A', 10, 10, 30, 30), quad('B', 100, 100, 130, 130), quad('C', 40, 40, 60, 60, { role: 'fill', owner: 'A' }), quad('D', 28, 5, 32, 8)]);
  h.press(2, 2); h.drag(60, 50); await settle();
  const marquee = h.svg().children.filter((c) => c.tag === 'polygon').find((c) => /rgba\(74,158,255,0.08\)/.test(c.attrs.fill));
  assert.ok(marquee, 'the marquee rectangle is drawn while dragging');
  assert.equal(marquee.attrs.points, '2,2 60,2 60,50 2,50');
  assert.equal(h.batches.length, 0);
  await h.release(60, 50); await settle();
  assert.deepEqual(h.ids(), ['A', 'D'], 'bounds intersection, a linked fill never its own selection, B outside');
  assert.deepEqual(h.batches, []);
  assert.ok(!h.svg().children.some((c) => c.attrs && c.attrs.fill === 'rgba(74,158,255,0.08)'), 'the preview ends with the drag');
});

test('Shift extends the selection with a marquee; an unshifted marquee replaces it', async () => {
  const h = harness([quad('A', 10, 10, 30, 30), quad('B', 100, 100, 130, 130)]);
  await h.select(20, 20);
  assert.deepEqual(h.ids(), ['A']);
  h.press(90, 90, { shiftKey: true }); h.drag(140, 140, { shiftKey: true }); await settle(); await h.release(140, 140, { shiftKey: true }); await settle();
  assert.deepEqual(h.ids(), ['A', 'B']);
  h.press(150, 5); h.drag(180, 30); await settle(); await h.release(180, 30); await settle();
  assert.deepEqual(h.ids(), []);
});

test('dragging a selected shape previews its outline and sends one batch with the exact delta on release', async () => {
  const h = harness([BIG, FAR]);
  await h.select(35, 35);
  assert.deepEqual(h.ids(), ['A']);
  h.press(35, 35); h.drag(45, 40); h.drag(65, 50); await settle();
  const outline = h.svg().children.find((c) => c.tag === 'path');
  assert.match(outline.attrs.d, /^M 50 35 C/, 'the live preview is the outline moved by (30,15)');
  assert.equal(h.batches.length, 0, 'no document write while dragging');
  await h.release(65, 50); await settle();
  assert.equal(h.batches.length, 1, 'exactly one commit');
  assert.equal(h.batches[0].layerUid, 'L');
  assert.equal(h.batches[0].steps.length, 1);
  assert.deepEqual(pts(h.batches[0].steps[0]), [[50, 35], [150, 35], [150, 135], [50, 135]]);
  assert.deepEqual(h.batches[0].steps[0].reshape.stableTarget, BIG.stableTarget);
  assert.deepEqual(h.ids(), ['A'], 'the selection is re-read at the new geometry');
  assert.deepEqual([h.root.selectedPaths[0].bounds.x, h.root.selectedPaths[0].bounds.y], [50, 35]);
});

test('pressing an unselected shape selects it and drags it; a Shift press adds it to the selection', async () => {
  const h = harness([BIG, FAR]);
  h.press(35, 35); h.drag(55, 35); await settle(); await h.release(55, 35); await settle();
  assert.equal(h.batches.length, 1);
  assert.deepEqual(pts(h.batches[0].steps[0])[0], [40, 20]);
  const g = harness([BIG, FAR]);
  await g.select(35, 35);
  g.press(160, 160, { shiftKey: true }); g.drag(170, 160, { shiftKey: true }); await settle(); await g.release(170, 160, { shiftKey: true }); await settle();
  assert.equal(g.batches[0].steps.length, 2, 'both selected items move together');
  assert.deepEqual(g.batches[0].steps.map((s) => pts(s)[0]), [[30, 20], [160, 150]]);
});

test('a corner handle scales about the opposite corner in one commit; Alt scales from the centre, Shift keeps the proportion', async () => {
  const run = async (to, extra) => { const h = harness([BIG]); await h.select(35, 35);
    h.press(120, 120); h.drag(to[0], to[1], extra); await settle(); assert.equal(h.batches.length, 0); await h.release(to[0], to[1], extra); await settle(); return h; };
  let h = await run([170, 170]);
  assert.equal(h.batches.length, 1);
  assert.deepEqual(pts(h.batches[0].steps[0]), [[20, 20], [170, 20], [170, 170], [20, 170]], 'pivot is the opposite (nw) corner, factor 1.5');
  h = await run([170, 120], { altKey: true });
  assert.deepEqual(pts(h.batches[0].steps[0]), [[-30, 20], [170, 20], [170, 120], [-30, 120]], 'Alt: pivot is the box centre (70,70), x factor 2');
  h = await run([170, 130], { shiftKey: true });
  const w = pts(h.batches[0].steps[0]), side = (a, b) => Math.hypot(w[b][0] - w[a][0], w[b][1] - w[a][1]);
  assert.ok(Math.abs(side(0, 1) - side(1, 2)) < 1e-6, 'a proportional scale of a square stays square');
});

test('an edge handle scales one axis only', async () => {
  const h = harness([BIG]); await h.select(35, 35);
  h.press(70, 120); h.drag(70, 170); await settle(); await h.release(70, 170); await settle();
  assert.equal(h.batches.length, 1);
  assert.deepEqual(pts(h.batches[0].steps[0]), [[20, 20], [120, 20], [120, 170], [20, 170]]);
});

test('the rotate ring turns the selection about the box centre; Shift snaps to 15 degrees', async () => {
  const h = harness([BIG]); await h.select(35, 35);
  h.press(100, 70); h.drag(70, 100); await settle(); await h.release(70, 100); await settle();
  assert.equal(h.batches.length, 1);
  assert.deepEqual(pts(h.batches[0].steps[0]), [[120, 20], [120, 120], [20, 120], [20, 20]], 'a quarter turn about (70,70)');
  const s = harness([BIG]); await s.select(35, 35);
  s.press(100, 70); s.drag(70 + 30 * Math.cos(0.3), 70 + 30 * Math.sin(0.3), { shiftKey: true }); await settle();
  await s.release(70 + 30 * Math.cos(0.3), 70 + 30 * Math.sin(0.3), { shiftKey: true }); await settle();
  assert.equal(s.batches.length, 1);
  const r = 15 * Math.PI / 180, first = pts(s.batches[0].steps[0])[0];
  assert.deepEqual(first, [Math.round((70 + (-50) * Math.cos(r) - (-50) * Math.sin(r)) * 1e6) / 1e6, Math.round((70 + (-50) * Math.sin(r) + (-50) * Math.cos(r)) * 1e6) / 1e6], '17 degrees snaps to exactly 15');
});

test('Escape mid-drag sends nothing and clears the preview', async () => {
  const h = harness([BIG]); await h.select(35, 35);
  h.press(35, 35); h.drag(80, 80); await settle();
  assert.ok(h.svg().children.some((c) => c.tag === 'path' && /^M 65 65/.test(c.attrs.d)));
  h.key('Escape');
  await h.release(80, 80); await settle();
  assert.deepEqual(h.batches, []);
  assert.deepEqual(h.listeners, [], 'the key listener is gone');
  assert.ok(h.svg().children.some((c) => c.tag === 'path' && /^M 20 20/.test(c.attrs.d)), 'the overlay shows the unmoved selection again');
  assert.deepEqual(h.ids(), ['A']);
  h.press(35, 35); h.drag(80, 80); await settle(); h.intent.cancel(); await h.release(80, 80); await settle();
  assert.deepEqual(h.batches, [], 'a lost capture ends the drag the same way');
});

test('a click that does not drag still selects; a press on a handle that does not drag changes nothing', async () => {
  const h = harness([BIG, FAR]);
  await h.select(35, 35); assert.deepEqual(h.ids(), ['A']);
  await h.select(160, 160); assert.deepEqual(h.ids(), ['B']);
  await h.select(160, 160); await h.select(190, 190);
  assert.deepEqual(h.ids(), ['B'], 'the click on the se handle selects nothing else');
  assert.deepEqual(h.batches, []);
});

test('images, text and shapes have no native transform here: a drag is refused with a notice naming them, and nothing is written', async () => {
  const h = harness([quad('IMG', 20, 20, 120, 120, { role: 'raster' }), quad('T', 140, 20, 190, 60, { role: 'text' })]);
  await h.select(35, 35);
  h.press(35, 35); h.drag(60, 60); await settle(); await h.release(60, 60); await settle();
  assert.deepEqual(h.batches, []);
  assert.ok(h.toasts.some((t) => /images/.test(t) && /not available/.test(t)), h.toasts.join('|'));
  const t = harness([quad('T', 140, 20, 190, 60, { role: 'text' })]); await t.select(150, 30);
  t.press(150, 30); t.drag(160, 40); await settle(); await t.release(160, 40); await settle();
  assert.ok(t.toasts.some((x) => /text/.test(x)));
  assert.deepEqual(t.batches, []);
});

test('a document that changed during the drag is not written over, and a refused batch says so', async () => {
  const h = harness([BIG]); await h.select(35, 35);
  h.press(35, 35); h.drag(80, 80); await settle();
  h.setPin({ contentRevision: 9, documentSnapshotId: 's9' });
  await h.release(80, 80); await settle();
  assert.deepEqual(h.batches, []);
  assert.ok(h.toasts.some((t) => /document changed/.test(t)));
  const r = harness([BIG], { refuse: true }); await r.select(35, 35);
  r.press(35, 35); r.drag(80, 80); await settle(); await r.release(80, 80); await settle();
  assert.equal(r.batches.length, 1);
  assert.ok(r.toasts.some((t) => /could not be applied/.test(t)));
  assert.deepEqual(r.ids(), ['A']);
});

test('a ribbon moves by its centreline, and a drag that ends where it began commits nothing', async () => {
  const centre = [{ point: [20, 60], handleIn: [0, 0], handleOut: [10, 0], width: 8 }, { point: [120, 60], handleIn: [-10, 0], handleOut: [0, 0], width: 8 }];
  const h = harness([quad('R', 20, 50, 120, 70, { role: 'ribbon', center: centre })]);
  await h.select(70, 60);
  h.press(70, 60); h.drag(90, 80); await settle(); await h.release(90, 80); await settle();
  assert.equal(h.batches.length, 1);
  const step = h.batches[0].steps[0].reshape;
  assert.deepEqual(step.center.map((c) => [c.point.x, c.point.y, c.width]), [[40, 80, 8], [140, 80, 8]]);
  const n = harness([BIG]); await n.select(35, 35);
  n.press(35, 35); n.drag(60, 60); n.drag(35, 35); await settle(); await n.release(35, 35); await settle();
  assert.deepEqual(n.batches, []);
});
