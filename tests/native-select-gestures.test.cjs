'use strict';
// #2372 SEL-F6: Select gestures on a native object project. A drag over the presented layer is a marquee, a move, a handle scale or
// a ring rotate: previewed in the select overlay while it runs (no document write), committed on release as ONE native
// object.vector.batch of reshapes against the document the drag began on, and ended with no write by Escape or a stale document.
const test = require('node:test'), assert = require('node:assert/strict');
const Intent = require('../src/js/adapters/select-canvas-intent.js');
const Overlay = require('../src/js/adapters/native-select-overlay.js');
const Gesture = require('../src/js/adapters/native-select-gesture.js');
const AltDuplicate = require('../src/js/adapters/native-select-alt-duplicate.js');
const Skew = require('../src/js/adapters/native-select-skew.js');
const Pivot = require('../src/js/adapters/native-select-pivot.js');
const Lasso = require('../src/js/adapters/native-select-lasso.js');
const Group = require('../src/js/adapters/native-select-group.js');
const Menu = require('../src/js/adapters/native-select-menu.js');
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
  let pin = { instanceId: 'i', documentId: 'd', lifecycleGeneration: 1, contentRevision: 1, documentSnapshotId: 's1', canvasW: 200, canvasH: 200, layers: [{ layerUid: 'L' }] };
  const store = items.map((i) => JSON.parse(JSON.stringify(i)));
  const session = { info: () => pin, isActive: () => true, blocksLegacy: () => true, readPresentedObjectSession: () => (busy ? null : { presentation: { ...pin, frame: 0 } }),
    async readPresentedStrokes(_selector, _pin, wanted, centres) {
      return { items: store.map((item) => { const { segments, center, ...rest } = item; return wanted(rest) ? { ...rest, segments, ...(centres && center ? { center } : {}) } : rest; }) };
    } };
  const root = { NemoNativePresentationWait: Wait, paper, Path: paper.Path, Point: paper.Point, Matrix: paper.Matrix, state: { tool: 'select', appMode: 'anim2d', currentFrame: 0, activeLayerIdx: 0, layers: [{ layerUid: 'L' }], selectedStrokeIndices: [] },
    selectedPaths: [], updateUI() { if (options.busyOnRepaint) busy = true; }, SMEngineBridge: { renderNow() {} }, showToast: (t) => toasts.push(t),
    document: { body, getElementById: () => ({ getBoundingClientRect: () => ({ left: 0, top: 0, width: 200, height: 200 }) }), createElementNS: (_n, tag) => el(tag),
      addEventListener: (type, fn) => listeners.push([type, fn]), removeEventListener: (type, fn) => { const at = listeners.findIndex((l) => l[0] === type && l[1] === fn); if (at >= 0) listeners.splice(at, 1); } },
    requestAnimationFrame: (fn) => frames.push(fn),
    NemoNativeObjectProject: { desktop: () => session, selectLayerUI: () => true },
    NemoNativeViewportUnderlay: { canvasMapping: { point: (e) => ({ x: e.clientX, y: e.clientY }), transform: () => [1, 0, 0, 1, 0, 0] } },
    NemoNativeObjectLabs: { desktop: () => ({ batchStrokes: async (layerUid, frame, steps) => {
      if (steps.every((s) => s.restyle)) { // a pivot write (A-select-skew-anchor-lasso): the engine stores the members on the listed record
        batches.push({ layerUid, frame, steps });
        steps.forEach((s) => { const it = store.find((i) => i.stableTarget.strokeId === s.restyle.stableTarget.strokeId), st = s.restyle.style;
          for (const k of ['xformAnchorKey', 'xformAnchorCustom']) if (k in st) { if (st[k] === null) delete it[k]; else it[k] = st[k]; } });
        pin = { ...pin, contentRevision: pin.contentRevision + 1, documentSnapshotId: 's' + (pin.contentRevision + 1) };
        return { status: 'applied' };
      }
      steps.forEach((s) => { const it = store.find((i) => i.stableTarget.strokeId === s.reshape.stableTarget.strokeId); s.before = it.segments.map((q) => q.point); });
      batches.push({ layerUid, frame, steps });
      if (options.refuse) return { status: 'invalid', reason: 'refused' };
      steps.forEach((s) => { const it = store.find((i) => i.stableTarget.strokeId === s.reshape.stableTarget.strokeId);
        if (s.reshape.geometry) it.segments = s.reshape.geometry.segments.map((q) => ({ point: [q.point.x, q.point.y], handleIn: [0, 0], handleOut: [0, 0] }));
        if (s.reshape.matrix) { const m = s.reshape.matrix; if (Math.abs(m[0] - m[3]) < 1e-9 && Math.abs(m[1] + m[2]) < 1e-9 && Math.abs(Math.hypot(m[0], m[1]) - 1) < 1e-6 && Math.abs(m[1]) > 1e-9) it.boxAngle = ((it.boxAngle || 0) + Math.atan2(m[1], m[0]) * 180 / Math.PI) % 360; // the engine stores a turn as boxAngle
          const at = (q) => [m[0] * q[0] + m[2] * q[1] + m[4], m[1] * q[0] + m[3] * q[1] + m[5]];
          it.segments = it.segments.map((q) => ({ point: at(q.point), handleIn: q.handleIn, handleOut: q.handleOut }));
          const [x0, y0, x1, y1] = it.bounds, c = [at([x0, y0]), at([x1, y1])]; it.bounds = [Math.min(c[0][0], c[1][0]), Math.min(c[0][1], c[1][1]), Math.max(c[0][0], c[1][0]), Math.max(c[0][1], c[1][1])]; }
        if (s.reshape.center) it.center = s.reshape.center.map((c) => ({ point: [c.point.x, c.point.y], handleIn: [c.handleIn.x, c.handleIn.y], handleOut: [c.handleOut.x, c.handleOut.y], width: c.width })); });
      pin = { ...pin, contentRevision: pin.contentRevision + 1, documentSnapshotId: 's' + (pin.contentRevision + 1) };
      return { status: 'applied' };
    } }) },
    NemoSelectCanvasIntent: Intent, NemoNativeSelectSkew: Skew, NemoNativeSelectPivot: Pivot, NemoNativeSelectLasso: Lasso, NemoNativeSelectGroup: Group,
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
  const h = { root, intent, toasts, batches, store, body, listeners, frames,
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

test('after the frame changes, pressing where a handle was does not start a transform, drops the stale selection and writes nothing', async () => {
  const h = harness([quad('A', 20, 20, 120, 120, { filled: false }), FAR]);
  await h.select(20, 70); assert.deepEqual(h.ids(), ['A']);
  const session = h.root.NemoNativeObjectProject.desktop(); session.readPresentedObjectSession = () => ({ presentation: { ...session.info(), frame: 1 } });
  h.root.state.currentFrame = 1; // the frame changed; nothing on a native project clears root.selectedPaths
  const stale = h.root.selectedPaths;
  h.press(106, 70); h.drag(100, 100); await settle(); await h.release(100, 100); await settle(); // the rotate ring's spot (36 px from the centre), over the hollow shape
  assert.deepEqual(h.batches, [], 'no transform was started on the new frame');
  assert.notEqual(h.root.selectedPaths, stale, 'the stale selection was dropped (the press became a marquee, not a grab of its ring)');
});

test('a bitmap brush and a dab with no anchor left are moved by the engine like any record: one batch with the record-space matrix, no refusal', async () => {
  for (const extra of [{ role: 'brush', brush: 'bitmap', group: 'g' }, { role: 'brush', brush: 'copy', group: 'g' }]) {
    const x = harness([quad('X', 20, 20, 120, 120, extra)]); await x.select(35, 35);
    x.press(35, 35); x.drag(60, 60); await settle(); await x.release(60, 60); await settle();
    assert.equal(x.batches.length, 1, `${extra.brush}: exactly one commit`);
    assert.deepEqual(x.batches[0].steps.map((st) => st.reshape.stableTarget.strokeId), ['X']);
    assert.deepEqual(x.batches[0].steps[0].reshape.matrix, [1, 0, 0, 1, 25, 25], 'the exact delta; the engine re-bakes the bitmap from the moved anchor');
    assert.deepEqual(x.toasts, [], 'no refusal notice');
  }
});

// #2372 SEL-F6C: an image and a text block are transformed by the same record-space matrix batch step as every other painted family
// (the engine maps the stored rect / anchor), so one gesture over any selection is one commit.
const IMG = quad('IMG', 20, 20, 120, 120, { role: 'raster', filled: true }), TXT = quad('T', 140, 20, 190, 60, { role: 'text', filled: true });
const gesture = async (h, from, to, mods) => { h.press(from[0], from[1]); h.drag(to[0], to[1], mods); await settle(); await h.release(to[0], to[1], mods); await settle(); };

test('dragging an image is ONE batch with the exact delta as a record-space matrix, and the selection is re-read', async () => {
  const h = harness([IMG, FAR]);
  await h.select(70, 70); assert.deepEqual(h.ids(), ['IMG']);
  h.press(70, 70); h.drag(95, 85); await settle(); assert.equal(h.batches.length, 0, 'no write while dragging');
  assert.ok(h.svg().children.some((c) => c.tag === 'path' && /^M 45 35/.test(c.attrs.d)), 'the live preview is the rect moved by (25,15)');
  await h.release(95, 85); await settle();
  assert.equal(h.batches.length, 1); assert.equal(h.batches[0].steps.length, 1);
  assert.deepEqual(h.batches[0].steps[0].reshape.stableTarget, IMG.stableTarget);
  assert.deepEqual(M(h.batches[0].steps[0]), [1, 0, 0, 1, 25, 15]);
  assert.equal(h.batches[0].steps[0].reshape.geometry, undefined);
  assert.deepEqual(h.ids(), ['IMG']);
  assert.deepEqual(h.toasts, []);
});

test('a text block moves and scales in one commit; a mirror of text or of an image is ONE batch with the mirror matrix (A-image-text-flip-skew)', async () => {
  let h = harness([TXT]); await h.select(165, 40);
  await gesture(h, [165, 40], [185, 60]);
  assert.equal(h.batches.length, 1); assert.deepEqual(M(h.batches[0].steps[0]), [1, 0, 0, 1, 20, 20]); assert.deepEqual(h.toasts, []);
  h = harness([TXT]); await h.select(165, 40);
  await gesture(h, [190, 60], [240, 100]);
  assert.equal(h.batches.length, 1, 'a corner handle scales text about the opposite corner');
  assert.deepEqual(M(h.batches[0].steps[0]), [2, 0, 0, 2, -140, -20]);
  const mirrored = async (items, press, to, pick = [165, 40]) => { const x = harness(items); await x.select(pick[0], pick[1]);
    await gesture(x, press, to); assert.equal(x.batches.length, 1, 'one commit'); assert.equal(x.batches[0].steps.length, 1); assert.deepEqual(x.toasts, [], 'no refusal'); return M(x.batches[0].steps[0]); };
  // The corner dragged across the opposite corner is a negative scale factor: stored by the engine as a mirror.
  assert.deepEqual(await mirrored([TXT], [190, 60], [110, 100]), [-0.6, 0, 0, 2, 224, -20]);
  assert.deepEqual(await mirrored([IMG], [120, 70], [-40, 70], [70, 70]), [-0.6, 0, 0, 1, 32, 0]);
});

// #2267: the text block has a stored rotation, so the ring turns it exactly as it turns an image: ONE batch, the record-space matrix.
test('the rotate ring turns a text block: one commit with the turn matrix, no refusal', async () => {
  const h = harness([TXT]); await h.select(165, 40);
  await gesture(h, [165, 25], [180, 40]); // the ring spot above the box centre (165,40), dragged a quarter turn clockwise
  assert.equal(h.batches.length, 1, 'one commit'); assert.equal(h.batches[0].steps.length, 1);
  assert.deepEqual(h.batches[0].steps[0].reshape.stableTarget, TXT.stableTarget);
  assert.deepEqual(M(h.batches[0].steps[0]), [0, 1, -1, 0, 205, -125], 'a quarter turn about the centre');
  assert.deepEqual(h.toasts, [], 'no notice');
  assert.deepEqual(h.ids(), ['T'], 'the selection is re-read');
});

test('a turned text block takes the box of its stored turn and scales along its own axes', async () => {
  const c = Math.SQRT1_2, turned = quad('T', 140, 20, 190, 60, { role: 'text', filled: true, matrix: [c, c, -c, c, 165 - c * 165 + c * 40, 40 - c * 165 - c * 40] });
  const h = harness([turned]); await h.select(165, 40);
  assert.deepEqual(h.ids(), ['T']);
  assert.ok(Math.abs(h.root.selectedPaths[0].data.boxAngle - 45) < 1e-9, 'the listed map carries the stored turn into the box');
  const box = Overlay.boxOf(h.root, h.root.selectedPaths), e = box.handles[3];
  await gesture(h, e, [e[0] + 20 * c, e[1] + 20 * c]);
  assert.equal(h.batches.length, 1, 'a stretch along the text own axis is one commit'); assert.deepEqual(h.toasts, []);
  const m = M(h.batches[0].steps[0]);
  assert.ok(Math.abs(m[0] - 1.4) < 1e-4 && m[1] === 0 && m[2] === 0 && m[3] === 1, m.join());
});

test('a world-axis stretch of a turned image is a shear in its own frame: ONE batch (stored as skew), no refusal', async () => {
  const c = Math.SQRT1_2, turned = quad('IMG', 20, 20, 120, 120, { role: 'raster', filled: true, matrix: [c, c, -c, c, 70 - c * 70 + c * 70, 70 - c * 70 - c * 70] });
  const h = harness([turned]); await h.select(70, 70);
  assert.deepEqual(h.ids(), ['IMG']);
  const box = h.root.selectedPaths[0].bounds;
  await gesture(h, [box.x + box.width, box.y + box.height], [box.x + box.width + 40, box.y + box.height]);
  assert.equal(h.batches.length, 1, 'one commit');
  assert.deepEqual(h.toasts, [], 'no refusal');
});

test('a mixed selection of a ribbon, an image and a text block moves as ONE batch of three matrices', async () => {
  const ribbon = quad('R', 20, 150, 120, 170, { role: 'ribbon', center: centre });
  const h = harness([ribbon, IMG, TXT]);
  await h.select(70, 160);
  await h.select(70, 70, true); await h.select(165, 40, true);
  assert.deepEqual(h.ids(), ['R', 'IMG', 'T']);
  await gesture(h, [165, 40], [175, 55]);
  assert.equal(h.batches.length, 1, 'one history entry for the mixed gesture');
  assert.deepEqual(h.batches[0].steps.map((s) => s.reshape.stableTarget.strokeId), ['R', 'IMG', 'T']);
  h.batches[0].steps.forEach((s) => assert.deepEqual(M(s), [1, 0, 0, 1, 10, 15]));
});

test('the image matrix is conjugated into its own frame (the listing matrix of a posed image)', async () => {
  const posed = quad('IMG', 20, 20, 60, 60, { role: 'raster', filled: true, matrix: [2, 0, 0, 2, 10, 0] });
  const h = harness([posed]); await h.select(90, 80);
  await gesture(h, [90, 80], [110, 90]);
  assert.deepEqual(M(h.batches[0].steps[0]), [1, 0, 0, 1, 10, 5], 'a world move of (20,10) is (10,5) in a frame posed at scale 2');
});

test('Escape mid-drag of an image or text writes nothing', async () => {
  for (const item of [IMG, TXT]) {
    const h = harness([item]); await h.select(item.bounds[0] + 10, item.bounds[1] + 10);
    h.press(item.bounds[0] + 10, item.bounds[1] + 10); h.drag(item.bounds[0] + 40, item.bounds[1] + 30); await settle();
    h.key('Escape'); await h.release(item.bounds[0] + 40, item.bounds[1] + 30); await settle();
    assert.deepEqual(h.batches, [], item.role); assert.deepEqual(h.listeners, []); assert.deepEqual(h.toasts, []);
  }
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

test('a drag that ends where it began commits nothing', async () => {
  const n = harness([BIG]); await n.select(35, 35);
  n.press(35, 35); n.drag(60, 60); n.drag(35, 35); await settle(); await n.release(35, 35); await settle();
  assert.deepEqual(n.batches, []);
});

const centre = [{ point: [20, 60], handleIn: [0, 0], handleOut: [10, 0], width: 8 }, { point: [120, 60], handleIn: [-10, 0], handleOut: [0, 0], width: 8 }];
const FAMILIES = [['a ribbon', { role: 'ribbon', center: centre }], ['a texture anchor', { role: 'brush', brush: 'anchor', group: 'g' }],
  ['a parametric or fill shape', { role: 'shape' }], ['a standalone fill', { role: 'fill' }], ['a ribbon with a gradient', { role: 'ribbon', filled: true }],
  ['a mask (MASK-NATIVE-REST)', { role: 'mask', filled: true, style: { maskMode: 'subtract', maskFeather: 6 } }]];
const M = (step) => step.reshape.matrix.map((v) => Math.round(v * 1e6) / 1e6 + 0);

test('every painted family moves by the exact delta as ONE batch of record-space matrices (the engine carries its fill, dabs and gradient)', async () => {
  for (const [name, extra] of FAMILIES) {
    const h = harness([quad('R', 20, 50, 120, 70, extra), quad('P', 150, 150, 190, 190)]);
    await h.select(70, 60);
    h.press(70, 60); h.drag(90, 80); await settle(); assert.equal(h.batches.length, 0, name + ': no write while dragging');
    await h.release(90, 80); await settle();
    assert.equal(h.batches.length, 1, name + ': exactly one commit');
    assert.equal(h.batches[0].steps.length, 1);
    assert.deepEqual(h.batches[0].steps[0].reshape.stableTarget, h.store[0].stableTarget);
    assert.deepEqual(M(h.batches[0].steps[0]), [1, 0, 0, 1, 20, 20], name);
    assert.equal(h.batches[0].steps[0].reshape.geometry, undefined, name + ': not a client-side geometry');
    assert.deepEqual(h.ids(), ['R'], name + ': the selection is re-read');
    assert.deepEqual([h.root.selectedPaths[0].bounds.x, h.root.selectedPaths[0].bounds.y], [40, 70]);
  }
});

test('the matrix is the gesture conjugated into the record space of a posed record, and a plain stroke beside it stays one step of the same batch', async () => {
  const posed = quad('S', 20, 50, 60, 70, { role: 'shape', matrix: [2, 0, 0, 2, 10, 0] });
  const h = harness([posed, quad('P', 100, 20, 140, 60)]);
  await h.select(100, 130); await h.select(110, 30, true);
  assert.deepEqual(h.ids(), ['S', 'P']);
  h.press(100, 130); h.drag(120, 140); await settle(); await h.release(120, 140); await settle();
  assert.equal(h.batches.length, 1, 'one history entry for the whole gesture');
  const [shape, plain] = h.batches[0].steps;
  assert.deepEqual(M(shape), [1, 0, 0, 1, 10, 5], 'a world move of (20,10) is (10,5) in a record posed at scale 2');
  assert.ok(plain.reshape.matrix && !plain.reshape.geometry, 'a plain stroke goes by the matrix too');
  assert.deepEqual(M(plain), [1, 0, 0, 1, 20, 10]);
});

test('a handle scale of a shape uses the baseline pivot (the opposite corner; Alt the centre) and a ring turn rotates about the box centre', async () => {
  const run = async (extra, press, to, mods) => { const h = harness([quad('R', 20, 20, 120, 120, extra)]); await h.select(70, 70);
    h.press(press[0], press[1]); h.drag(to[0], to[1], mods); await settle(); await h.release(to[0], to[1], mods); await settle(); return h; };
  let h = await run({ role: 'shape' }, [120, 120], [170, 170]);
  assert.deepEqual(M(h.batches[0].steps[0]), [1.5, 0, 0, 1.5, -10, -10], 'factor 1.5 about the north-west corner (20,20)');
  h = await run({ role: 'brush', brush: 'anchor', group: 'g' }, [120, 120], [170, 120], { altKey: true });
  assert.deepEqual(M(h.batches[0].steps[0]), [2, 0, 0, 1, -70, 0], 'Alt scales from the centre (70,70)');
  h = await run({ role: 'ribbon', center: centre }, [100, 70], [70, 100]);
  assert.deepEqual(M(h.batches[0].steps[0]), [0, 1, -1, 0, 140, 0], 'a quarter turn about (70,70)');
});

test('a dab is selected as its anchor, a marquee never selects a dab, and moving the anchor sends the anchor only', async () => {
  const anchor = quad('A', 20, 50, 120, 70, { role: 'brush', brush: 'anchor', group: 'g' });
  const dab = quad('D', 200, 200, 210, 210, { role: 'brush', brush: 'copy', group: 'g' });
  const near = { ...dab, stableTarget: { ...dab.stableTarget, strokeId: 'D2' }, bounds: [60, 55, 70, 65], segments: [seg(60, 55), seg(70, 55), seg(70, 65), seg(60, 65)] };
  const h = harness([anchor, near, dab]);
  await h.select(65, 60);
  assert.deepEqual(h.ids(), ['A'], 'a click on a dab selects its anchor');
  const m = harness([anchor, near, dab]);
  m.press(5, 5); m.drag(300, 300); await settle(); await m.release(300, 300); await settle();
  assert.deepEqual(m.ids(), ['A'], 'the marquee selects the anchor and none of its dabs');
  m.press(70, 60); m.drag(80, 60); await settle(); await m.release(80, 60); await settle();
  assert.equal(m.batches[0].steps.length, 1, 'one step; the engine carries the copies');
  assert.deepEqual(m.batches[0].steps[0].reshape.stableTarget, anchor.stableTarget);
});

test('Escape mid-drag of a ribbon writes nothing', async () => {
  const h = harness([quad('R', 20, 50, 120, 70, { role: 'ribbon', center: centre })]); await h.select(70, 60);
  h.press(70, 60); h.drag(90, 80); await settle(); h.key('Escape'); await h.release(90, 80); await settle();
  assert.deepEqual(h.batches, []);
});

test('a ribbon keeps its linked fill: the fill is selected as its ribbon and the one step names the ribbon (the engine re-syncs the fill)', async () => {
  const fill = quad('F', 20, 40, 120, 80, { role: 'fill', owner: 'R' }), ribbon = quad('R', 20, 50, 120, 70, { role: 'ribbon', center: centre });
  const h = harness([fill, ribbon]);
  await h.select(50, 60);
  assert.deepEqual(h.ids(), ['R'], 'a click on the fill selects its ribbon');
  h.press(50, 60); h.drag(50, 100); await settle(); await h.release(50, 100); await settle();
  assert.equal(h.batches.length, 1);
  assert.deepEqual(h.batches[0].steps.map((s) => s.reshape.stableTarget.strokeId), ['R']);
  assert.deepEqual(M(h.batches[0].steps[0]), [1, 0, 0, 1, 0, 40]);
});

test('a plain stroke moves by the matrix route: its payload has no geometry and does not grow with its anchor count', async () => {
  const dense = quad('D', 20, 20, 120, 120, { segments: Array.from({ length: 400 }, (_, i) => seg(20 + (i % 100), 20 + Math.floor(i / 4))), count: 400 });
  const h = harness([dense, FAR]);
  await h.select(35, 35);
  h.press(35, 35); h.drag(65, 50); await settle(); await h.release(65, 50); await settle();
  assert.equal(h.batches.length, 1);
  const step = h.batches[0].steps[0];
  assert.deepEqual(Object.keys(step.reshape).sort(), ['matrix', 'stableTarget']);
  assert.deepEqual(M(step), [1, 0, 0, 1, 30, 15]);
  assert.ok(JSON.stringify(h.batches[0].steps.map((x) => x.reshape)).length < 400, 'six numbers, not 400 anchors');
});

test('thirty plain strokes of 400 anchors each move as ONE batch well under the 65536-byte request cap', async () => {
  const items = Array.from({ length: 30 }, (_, i) => quad('P' + i, 20, 20, 120, 120, { segments: Array.from({ length: 400 }, (_, k) => seg(20 + (k % 100), 20 + Math.floor(k / 4))), count: 400 }));
  const h = harness(items);
  h.root.selectedPaths = [];
  await h.select(35, 35);
  h.root.selectedPaths = (await Intent.layerPaths(h.root, () => true, false, 0)).paths; Overlay.show(h.root);
  h.press(35, 35); h.drag(65, 50); await settle(); await h.release(65, 50); await settle();
  assert.equal(h.toasts.length, 0, h.toasts.join('|'));
  assert.equal(h.batches.length, 1, 'one batch');
  assert.equal(h.batches[0].steps.length, 30);
  assert.ok(h.batches[0].steps.every((st) => st.reshape.matrix && !st.reshape.geometry));
  assert.ok(JSON.stringify(h.batches[0].steps.map((x) => x.reshape)).length < 65536 / 4, 'far under the cap');
});

test('a batch whose payload would pass the byte cap is refused with a notice and writes nothing', async () => {
  const h = harness([BIG, FAR]);
  await h.select(35, 35);
  h.root.NemoSelectCanvasIntent = { ...Intent, transformSteps: () => [{ reshape: { stableTarget: BIG.stableTarget, matrix: [1, 0, 0, 1, 1, 1], pad: 'x'.repeat(70000) } }] };
  h.press(35, 35); h.drag(65, 50); await settle(); await h.release(65, 50); await settle();
  assert.equal(h.batches.length, 0);
  assert.ok(h.toasts.some((t) => /Too much/.test(t)), h.toasts.join('|'));
});

// #2372 REV-13: the box angle is stored natively with the turn and read back from the listing; a handle scale follows the box's own axes.
const turnedQuad = (id, deg) => { const r = deg * Math.PI / 180, c = Math.cos(r), n = Math.sin(r), mid = [70, 45], at = (x, y) => [mid[0] + (x - mid[0]) * c - (y - mid[1]) * n, mid[1] + (x - mid[0]) * n + (y - mid[1]) * c];
  const q = quad(id, 20, 30, 120, 60); q.segments = [at(20, 30), at(120, 30), at(120, 60), at(20, 60)].map((p) => seg(p[0], p[1]));
  q.bounds = [Math.min(...q.segments.map((s) => s.point[0])), Math.min(...q.segments.map((s) => s.point[1])), Math.max(...q.segments.map((s) => s.point[0])), Math.max(...q.segments.map((s) => s.point[1]))];
  q.boxAngle = deg; return q; };

test('a turn is committed with the batch and the re-read selection draws the turned box; a later pick (frame change, undo, reload) reads it back from the listing', async () => {
  const h = harness([BIG]); await h.select(35, 35);
  h.press(100, 70); h.drag(70 + 30 * Math.cos(Math.PI / 6), 70 + 30 * Math.sin(Math.PI / 6), { shiftKey: true }); await settle();
  await h.release(70 + 30 * Math.cos(Math.PI / 6), 70 + 30 * Math.sin(Math.PI / 6), { shiftKey: true }); await settle();
  assert.equal(h.batches.length, 1);
  assert.ok(Math.abs(h.store[0].boxAngle - 30) < 1e-9, 'the engine (harness) stored the 30 degree turn with the same batch');
  const box = Overlay.boxOf(h.root, h.root.selectedPaths);
  assert.ok(Math.abs(box.angle - 30) < 1e-9, 'the re-read selection is oriented from the listing: ' + box.angle);
  // A fresh context: deselect, then pick the item again. Nothing of the session carries over, only the listing does.
  h.root.selectedPaths = []; await h.select(300, 300); await settle();
  const w = h.store[0].bounds; await h.select((w[0] + w[2]) / 2, (w[1] + w[3]) / 2); await settle();
  assert.equal(h.ids().join(), 'A');
  assert.ok(Math.abs(h.root.selectedPaths[0].data.boxAngle - 30) < 1e-9, 'the picked path carries the stored angle');
  assert.ok(Math.abs(Overlay.boxOf(h.root, h.root.selectedPaths).angle - 30) < 1e-9, 'and its box is still turned');
});

test('a listing angle is turned by a rotated presented matrix, and an unturned record has none', async () => {
  const q = turnedQuad('T', 20); q.matrix = [0, 1, -1, 0, 0, 0]; // a quarter-turned layer: the world angle is the stored 20 plus 90
  const h = harness([q, quad('P', 150, 150, 190, 190)]); await h.select(160, 160);
  const read = await Intent.layerPaths(h.root, () => true, false, 0), by = (id) => read.paths.find((p) => p.data.strokeId === id);
  assert.ok(Math.abs(by('T').data.boxAngle - 110) < 1e-9, String(by('T').data.boxAngle));
  assert.equal(by('P').data.boxAngle, undefined, 'an unturned record has no box angle');
});

test('a handle scale of a turned box follows the box axes: a right-edge drag stretches along the box, leaving the perpendicular axis alone', async () => {
  const h = harness([turnedQuad('T', 30)]); const r = 30 * Math.PI / 180, u = [Math.cos(r), Math.sin(r)], v = [-Math.sin(r), Math.cos(r)];
  const c = [70, 45], e = [c[0] + 50 * u[0], c[1] + 50 * u[1]];
  await h.select(c[0], c[1]);
  const box = Overlay.boxOf(h.root, h.root.selectedPaths);
  assert.ok(Math.abs(box.angle - 30) < 1e-9);
  h.press(e[0], e[1]); const to = [e[0] + 30 * u[0], e[1] + 30 * u[1]]; h.drag(to[0], to[1]); await settle(); await h.release(to[0], to[1]); await settle();
  assert.equal(h.batches.length, 1, h.toasts.join());
  const m = h.batches[0].steps[0].reshape.matrix, lin = (p) => [m[0] * p[0] + m[2] * p[1], m[1] * p[0] + m[3] * p[1]];
  const lu = lin(u), lv = lin(v);
  assert.ok(Math.hypot(lu[0] - 1.3 * u[0], lu[1] - 1.3 * u[1]) < 1e-9, 'the box x axis grows from 100 to 130 (factor 1.3), anchored at the far edge: ' + lu);
  assert.ok(Math.hypot(lv[0] - v[0], lv[1] - v[1]) < 1e-9, 'the perpendicular box axis is untouched: ' + lv);
  const far = [c[0] - 50 * u[0], c[1] - 50 * u[1]], at = (p) => [m[0] * p[0] + m[2] * p[1] + m[4], m[1] * p[0] + m[3] * p[1] + m[5]];
  assert.ok(Math.hypot(at(far)[0] - far[0], at(far)[1] - far[1]) < 1e-9, 'the opposite edge midpoint stays fixed');
  assert.ok(Math.abs(h.store[0].boxAngle - 30) < 1e-9, 'a scale leaves the stored orientation alone');
});

test('a dynamic shape keeps the world-axis resize of its engine kernel even under a turned box', async () => {
  const q = turnedQuad('S', 30); q.role = 'shape';
  const h = harness([q]); const r = 30 * Math.PI / 180, c = [70, 45], e = [c[0] + 50 * Math.cos(r), c[1] + 50 * Math.sin(r)];
  await h.select(c[0], c[1]);
  h.press(e[0], e[1]); h.drag(e[0] + 30, e[1]); await settle(); await h.release(e[0] + 30, e[1]); await settle();
  const m = h.batches[0].steps[0].reshape.matrix;
  assert.ok(Math.abs(m[1]) < 1e-9 && Math.abs(m[2]) < 1e-9, 'axis-aligned, so the shape kernel accepts it');
});

test('MASK-NATIVE-REST: a mask scales like a shape, and moves with the content it masks in the one batch', async () => {
  const mask = quad('M', 20, 20, 120, 120, { role: 'mask', filled: true, style: { maskMode: 'add', maskFeather: 0 } });
  const h = harness([mask, FAR]);
  await h.select(70, 70);
  assert.deepEqual(h.ids(), ['M']);
  const handle = h.root.NemoNativeSelectOverlay.boxOf(h.root, h.root.selectedPaths).handles[4]; // the south-east corner
  h.press(handle[0], handle[1]); h.drag(handle[0] + 20, handle[1] + 20); await settle(); await h.release(handle[0] + 20, handle[1] + 20); await settle();
  assert.equal(h.toasts.length, 0, 'no refusal notice');
  assert.equal(h.batches.length, 1, 'one commit');
  const m = M(h.batches[0].steps[0]);
  assert.deepEqual([m[0] > 1, m[3] > 1, m[1], m[2]], [true, true, 0, 0], 'a scale about the opposite corner');
  assert.deepEqual(h.batches[0].steps[0].reshape.stableTarget, mask.stableTarget);
  const both = harness([mask, FAR]);
  await both.select(70, 70); await both.select(170, 170, true);
  assert.deepEqual(both.ids(), ['M', 'B']);
  both.press(70, 70); both.drag(80, 80); await settle(); await both.release(80, 80); await settle();
  assert.equal(both.batches.length, 1);
  assert.deepEqual(both.batches[0].steps.map((s) => s.reshape.stableTarget.strokeId), ['M', 'B']);
});

// The native queue reports no presentation while a command is pending - and the pick's own repaint (updateUI) is one. That window is
// "unreadable", not "changed": the installed build abandoned every drag and dropped the selection box inside it.
test('a drag on an unselected shape survives the queue being busy with the pick\'s own repaint, and sends one batch', async () => {
  const h = harness([BIG, FAR], { busyOnRepaint: true });
  h.press(35, 35); await settle(); // the pick resolves, applyPick repaints, the queue is busy
  h.drag(45, 35); h.drag(55, 35); await settle();
  assert.deepEqual(h.ids(), ['A'], 'the press selected the shape');
  assert.ok(!h.toasts.some((t) => /document changed/.test(t)), h.toasts.join('|'));
  h.busy(false);
  h.drag(65, 35); await h.release(65, 35); await settle();
  assert.equal(h.batches.length, 1, 'one batch');
  assert.deepEqual(pts(h.batches[0].steps[0])[0], [50, 20]);
  assert.deepEqual(h.toasts, []);
});

test('a click then a drag on the SELECTED shape sends one batch although the queue is busy during the drag', async () => {
  const h = harness([BIG, FAR]);
  await h.select(35, 35);
  assert.deepEqual(h.ids(), ['A']);
  h.press(35, 35); await settle(); h.busy(true); // the press's own pick is read before the queue gets busy
  h.drag(45, 40); h.drag(65, 50); await settle();
  assert.ok(!h.toasts.length, h.toasts.join('|'));
  h.busy(false);
  await h.release(65, 50); await settle();
  assert.equal(h.batches.length, 1);
  assert.deepEqual(pts(h.batches[0].steps[0])[0], [50, 35]);
  assert.deepEqual(h.toasts, []);
});

test('a release while the presentation is still unreadable waits for it instead of reporting a changed document', async () => {
  const h = harness([BIG, FAR]);
  await h.select(35, 35);
  h.press(35, 35); h.drag(65, 50); await settle();
  h.busy(true);
  const done = h.release(65, 50);
  setTimeout(() => h.busy(false), 60);
  await done; await new Promise((r) => setTimeout(r, 150)); await settle();
  assert.equal(h.batches.length, 1);
  assert.deepEqual(h.toasts, []);
});

test('a genuinely foreign change mid-drag still abandons, busy or not', async () => {
  for (const busy of [false, true]) {
    const frame = harness([BIG, FAR]); await frame.select(35, 35);
    frame.press(35, 35); frame.drag(45, 35); await settle(); frame.busy(busy); frame.root.state.currentFrame = 3; frame.drag(55, 35); await frame.release(55, 35); await settle();
    assert.equal(frame.batches.length, 0, `frame change (busy ${busy})`);
    assert.ok(frame.toasts.some((t) => /document changed/.test(t)));
    const layer = harness([BIG, FAR]); await layer.select(35, 35);
    layer.press(35, 35); layer.drag(45, 35); await settle(); layer.busy(busy); layer.root.state.activeLayerIdx = 1; layer.drag(55, 35); await layer.release(55, 35); await settle();
    assert.equal(layer.batches.length, 0, `layer change (busy ${busy})`);
    const sel = harness([BIG, FAR]); await sel.select(35, 35);
    sel.press(35, 35); sel.drag(45, 35); await settle(); sel.busy(busy); sel.root.selectedPaths = []; sel.drag(55, 35); await sel.release(55, 35); await settle();
    assert.equal(sel.batches.length, 0, `selection change (busy ${busy})`);
    const doc = harness([BIG, FAR]); await doc.select(35, 35);
    doc.press(35, 35); doc.drag(45, 35); await settle(); doc.busy(busy); doc.setPin({ contentRevision: 9, documentSnapshotId: 's9' }); doc.drag(55, 35); await doc.release(55, 35); await settle();
    assert.equal(doc.batches.length, 0, `revision change (busy ${busy})`);
    assert.ok(doc.toasts.some((t) => /document changed/.test(t)));
  }
});

test('a click shows the selection box even when the queue is busy with the click\'s repaint, and draws it once readable', async () => {
  const h = harness([BIG, FAR], { busyAtShow: true });
  h.press(35, 35); await h.release(35, 35); await settle();
  assert.deepEqual(h.ids(), ['A']);
  assert.ok(Overlay.selectionContext(h.root), 'the pick was recorded although the presentation was unreadable at that instant');
  h.runFrames(); // the repaint's one-shot redraw runs while the presentation is unreadable
  assert.deepEqual(h.ids(), ['A'], 'an unreadable presentation does not end the selection');
  assert.ok(Overlay.selectionContext(h.root), 'the overlay still owns the selection');
  h.busy(false); h.runFrames();
  const svg = h.svg();
  assert.ok(svg && svg.children.some((c) => c.tag === 'polygon'), 'the box is drawn');
  assert.equal(svg.children.filter((c) => c.tag === 'rect').length, 8, 'with its 8 handles');
});

test('the overlay selection still ends when its context really changes while the presentation is unreadable', async () => {
  const h = harness([BIG, FAR]);
  await h.select(35, 35);
  h.busy(true); h.root.state.currentFrame = 4; Overlay.notify(h.root); h.runFrames();
  assert.equal(Overlay.selectionContext(h.root), null);
});

// #2372 click feedback. The native queue reports no presentation while any command is pending (the press's own read, a hover read, the
// previous pick's repaint) and that window outlasts one short retry: the click's pick was dropped (first click showed no box, an empty
// click did not clear the selection). A click now waits the window out.
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const hasBox = (h) => { const s = h.svg(); return !!s && s.parentNode === h.body && s.children.some((c) => c.tag === 'polygon'); };

test('the FIRST click on an unselected shape shows its box although the presentation is unreadable for a while after the press', async () => {
  const h = harness([BIG, FAR]);
  h.busy(true); // the queue is busy right after the press (presentedState null)
  h.press(35, 35); await settle();
  const done = h.release(35, 35);
  setTimeout(() => h.busy(false), 300); // longer than the single 120 ms retry the click used to get
  await done; await sleep(500); await settle();
  assert.deepEqual(h.ids(), ['A'], 'the click selected the shape');
  assert.deepEqual(h.toasts, []);
  h.runFrames();
  assert.ok(hasBox(h), 'the selection box is drawn');
  assert.equal(h.svg().children.filter((c) => c.tag === 'rect').length, 8, 'with its 8 handles');
});

test('a click on empty canvas clears the selection and hides the box although the presentation is unreadable for a while', async () => {
  const h = harness([BIG, FAR]);
  await h.select(35, 35); h.runFrames();
  assert.deepEqual(h.ids(), ['A']); assert.ok(hasBox(h));
  h.busy(true);
  h.press(5, 195); await settle();
  const done = h.release(5, 195);
  setTimeout(() => h.busy(false), 300);
  await done; await sleep(500); await settle(); h.runFrames();
  assert.deepEqual(h.ids(), [], 'the selection is cleared');
  assert.ok(!h.svg() || h.svg().parentNode !== h.body, 'the box is hidden');
  assert.equal(h.root.state.selectedStrokeIndices.length, 0);
});

test('a Shift click on empty canvas keeps the selection and its box, busy or not', async () => {
  for (const busy of [false, true]) {
    const h = harness([BIG, FAR]);
    await h.select(35, 35); h.runFrames();
    h.busy(busy);
    h.press(5, 195, { shiftKey: true }); await settle();
    const done = h.release(5, 195, { shiftKey: true });
    setTimeout(() => h.busy(false), 300);
    await done; await sleep(500); await settle(); h.runFrames();
    assert.deepEqual(h.ids(), ['A'], `kept (busy ${busy})`);
    assert.ok(hasBox(h), `box kept (busy ${busy})`);
  }
});

test('a presentation that never becomes readable ends the click with ONE notice and leaves the selection alone', async () => {
  const h = harness([BIG, FAR]);
  await h.select(35, 35);
  h.root.setTimeout = (fn) => setTimeout(fn, 1);
  h.busy(true);
  h.press(5, 195); await settle();
  await h.release(5, 195); await sleep(200); await settle();
  assert.deepEqual(h.ids(), ['A'], 'an unprovable pick is not an empty click');
  assert.equal(h.toasts.length, 1);
  assert.match(h.toasts[0], /Could not read the canvas/);
});

test('a click whose context changes while it waits for the presentation selects nothing', async () => {
  const h = harness([BIG, FAR]);
  h.busy(true);
  h.press(35, 35); await settle();
  const done = h.release(35, 35);
  setTimeout(() => { h.root.state.currentFrame = 2; h.busy(false); }, 150);
  await done; await sleep(400); await settle();
  assert.deepEqual(h.ids(), []);
  assert.deepEqual(h.toasts, []);
});

// #2372 presentation-busy-audit: every Select-family action (Delete, Align, Transform fields, restyle) starts from presentedSettled.
test('presentedSettled waits out the queue-pending window, and gives null when the pick context changes meanwhile', async () => {
  const h = harness([BIG, FAR]);
  h.root.setTimeout = (fn) => setTimeout(fn, 5);
  h.busy(true);
  const waiting = Intent.presentedSettled(h.root);
  setTimeout(() => h.busy(false), 40);
  const shown = await waiting;
  assert.ok(shown && shown.pin, 'readable again: the state of the presented document');
  h.busy(true);
  const gone = Intent.presentedSettled(h.root);
  setTimeout(() => { h.root.state.currentFrame = 2; h.busy(false); }, 40);
  assert.equal(await gone, null, 'a changed pick context refuses');
  assert.deepEqual(h.toasts, []);
});

// A-select-alt-drag-duplicate: Alt+press inside the selection drags a COPY. The gesture previews the moved copy and on release makes ONE duplicate at the
// drag distance through the clipboard module (faked here: what it is asked to do is the contract); the originals and the batch route are never touched.
function altHarness(items, options) {
  const h = harness(items, options), calls = [];
  h.root.NemoNativeSelectAltDuplicate = AltDuplicate;
  h.root.NemoNativeSelectionClipboard = { native: () => true, duplicateAt: async (_root, offset, guard) => {
    calls.push({ offset, guard });
    if (guard.cancelled()) return { failed: true, status: 'cancelled' };
    guard.release();
    return options && options.reply ? options.reply : { status: 'applied' };
  } };
  h.calls = calls;
  return h;
}

test('Alt+press inside the selection and a drag sends ONE duplicate at the drag distance, previews the copy, writes no batch and leaves the original alone', async () => {
  const h = altHarness([BIG, FAR]), before = JSON.stringify(h.store), outlines = () => h.svg().children.filter((c) => c.tag === 'path').length;
  await h.select(35, 35);
  const resting = outlines();
  h.press(60, 60, { altKey: true }); h.drag(70, 66, { altKey: true }); h.drag(90, 75, { altKey: true }); await settle();
  const outline = h.svg().children.find((c) => c.tag === 'path');
  assert.match(outline.attrs.d, /^M 50 35 C/, 'the live preview is the copy, moved by (30,15)');
  assert.equal(h.calls.length, 0, 'no document write while dragging');
  await h.release(90, 75, { altKey: true }); await settle();
  assert.equal(h.calls.length, 1, 'exactly one duplicate request');
  assert.deepEqual(h.calls[0].offset, [30, 15]);
  assert.deepEqual(h.calls[0].guard.pin, h.root.NemoNativeObjectProject.desktop().info(), 'the pin read when the drag began travels with it');
  assert.deepEqual(h.batches, [], 'the reshape batch route is not used');
  assert.equal(JSON.stringify(h.store), before, 'the original is untouched');
  assert.equal(outlines(), resting, 'the preview ends with the drag');
  assert.deepEqual(h.toasts, []);
});

test('the Alt press only copies INSIDE the selection box: outside it (even on an unselected shape) it moves the pivot, and without Alt a drag of the selection is a plain move', async () => {
  const h = altHarness([BIG, FAR]);
  await h.select(35, 35);
  h.press(170, 170, { altKey: true }); h.drag(150, 150, { altKey: true }); await settle(); await h.release(150, 150, { altKey: true }); await settle();
  assert.equal(h.calls.length, 0, 'an unselected item under Alt is not copied');
  assert.ok(h.batches.every((b) => b.steps.every((st) => st.restyle && !st.reshape)), 'and not moved: the baseline places the pivot there, which is written as a pivot restyle only (A-select-skew-anchor-lasso)');
  assert.deepEqual(h.root.state.xformAnchorCustom, [150, 150]);
  const g = altHarness([BIG, FAR]);
  await g.select(35, 35);
  g.press(35, 35); g.drag(60, 35); await settle(); await g.release(60, 35); await settle();
  assert.equal(g.calls.length, 0);
  assert.equal(g.batches.length, 1, 'the ordinary move is unchanged');
});

test('Alt+press on empty space inside the selection box still copies the selection (the box is the target, not the stroke under the pointer)', async () => {
  const ring = quad('A', 20, 20, 120, 120, { filled: false });
  const h = altHarness([ring]);
  await h.select(20, 70);
  assert.deepEqual(h.ids(), ['A']);
  h.press(55, 55, { altKey: true }); h.drag(65, 55, { altKey: true }); h.drag(85, 75, { altKey: true }); await settle(); await h.release(85, 75, { altKey: true }); await settle();
  assert.equal(h.calls.length, 1, 'away from the pivot crosshair (Alt over the crosshair grabs the pivot)');
  assert.deepEqual(h.calls[0].offset, [30, 20]);
});

test('Escape mid-drag sends no duplicate; an Alt+click without a drag, and a drag that ends where it began, leave the copy where the original is (baseline: the clone is made at the press)', async () => {
  const h = altHarness([BIG, FAR]), outlines = () => h.svg().children.filter((c) => c.tag === 'path').length;
  await h.select(35, 35);
  const resting = outlines();
  h.press(60, 60, { altKey: true }); h.drag(90, 90, { altKey: true }); await settle();
  assert.match(h.svg().children.find((c) => c.tag === 'path').attrs.d, /^M 50 50 C/, 'the copy is previewed, moved by (30,30)');
  h.key('Escape'); await settle(); await h.release(90, 90, { altKey: true }); await settle();
  assert.equal(h.calls.length, 0);
  assert.equal(outlines(), resting, 'the preview is cleared');
  h.press(60, 60, { altKey: true }); await h.release(60, 60, { altKey: true }); await settle();
  assert.equal(h.calls.length, 1, 'a plain Alt-click leaves an in-place copy');
  assert.deepEqual(h.calls[0].offset, [0, 0]);
  assert.equal(outlines(), resting);
  h.press(60, 60, { altKey: true }); h.press(61, 60, { altKey: true }); await h.release(62, 61, { altKey: true }); await settle();
  assert.equal(h.calls.length, 2, 'a press that moves under the drag threshold is still a click');
  assert.deepEqual(h.calls[1].offset, [0, 0], 'the copy sits exactly on the original');
  h.press(60, 60, { altKey: true }); h.drag(90, 90, { altKey: true }); h.drag(60, 60, { altKey: true }); await settle(); await h.release(60, 60, { altKey: true }); await settle();
  assert.equal(h.calls.length, 3, 'back at the start is a copy in place, not a move');
  assert.deepEqual(h.calls[2].offset, [0, 0]);
  assert.deepEqual(h.batches, []);
  // Without Alt a click is only a selection; an Alt press outside the selection box never copies.
  const plain = altHarness([BIG, FAR]);
  await plain.select(35, 35);
  plain.press(60, 60); await plain.release(60, 60); await settle();
  plain.press(170, 170, { altKey: true }); await plain.release(170, 170, { altKey: true }); await settle();
  assert.equal(plain.calls.length, 0);
});

test('Escape between the release and the send cancels; a document that moved on during the drag sends nothing and says so', async () => {
  const h = altHarness([BIG, FAR]);
  await h.select(35, 35);
  h.press(60, 60, { altKey: true }); h.drag(90, 90, { altKey: true }); await settle();
  h.setPin({ contentRevision: 9, documentSnapshotId: 's9' });
  h.drag(95, 95, { altKey: true }); await settle(); await h.release(95, 95, { altKey: true }); await settle();
  assert.equal(h.calls.length, 0);
  assert.ok(h.toasts.some((t) => /nothing was changed/.test(t)), 'stale pin: ' + h.toasts.join('|'));
});

test('a refused or failed duplicate leaves the notice to the clipboard module and no batch; a posed record gets the distance in its own space', async () => {
  const posed = quad('A', 20, 20, 120, 120, { matrix: [2, 0, 0, 2, 0, 0] });
  const h = altHarness([posed]);
  await h.select(35, 35);
  h.press(60, 60, { altKey: true }); h.drag(80, 70, { altKey: true }); h.drag(100, 80, { altKey: true }); await settle(); await h.release(100, 80, { altKey: true }); await settle();
  assert.equal(h.calls.length, 1);
  assert.deepEqual(h.calls[0].offset, [20, 10], 'a world move of (40,20) is (20,10) in a record drawn at 2x');
  assert.deepEqual(h.batches, []);
  const mixed = altHarness([quad('A', 20, 20, 60, 60), quad('B', 70, 70, 110, 110, { matrix: [2, 0, 0, 2, 0, 0] })]);
  mixed.press(5, 5); mixed.drag(230, 230); await settle(); await mixed.release(230, 230); await settle();
  assert.deepEqual(mixed.ids(), ['A', 'B']);
  mixed.press(60, 60, { altKey: true }); mixed.drag(80, 80, { altKey: true }); mixed.drag(100, 100, { altKey: true }); await settle(); await mixed.release(100, 100, { altKey: true }); await settle();
  assert.equal(mixed.calls.length, 0, 'records that disagree on the distance are refused, not guessed');
  assert.ok(mixed.toasts.some((t) => /differently/.test(t)), mixed.toasts.join('|'));
});

// ---- A-select-skew-anchor-lasso: skew zones, the pivot and the Alt lasso ------------------------------------------------------------------------
// BIG is the box (20,20)-(120,120): the skew zones flank each edge midpoint 9 to 20 screen px along the edge (zone centres 14.5 px from it).
const zone = { n: [84.5, 20], s: [84.5, 120], w: [20, 84.5], e: [120, 84.5] };

test('a skew zone drag shears the box about the opposite edge: ONE batch with the exact affine, previewed with its ticks, nothing written meanwhile', async () => {
  const h = harness([BIG, FAR]); await h.select(35, 35);
  assert.ok(h.svg().children.filter((c) => c.tag === 'line' && c.attrs.stroke === 'rgb(74,158,255)' && c.attrs['stroke-width'] === '1.6').length >= 8, 'two ticks flank each of the four edge midpoints');
  h.press(...zone.n); h.drag(80, 20); h.drag(90, 20); await settle();
  assert.equal(h.batches.length, 0);
  const outline = h.svg().children.find((c) => c.tag === 'path');
  assert.match(outline.attrs.d, /^M 40 20 C/, 'the live preview is the outline with the top edge slid right by 20');
  await h.release(90, 20); await settle();
  assert.equal(h.batches.length, 1, 'exactly one commit'); assert.equal(h.batches[0].steps.length, 1);
  assert.deepEqual(M(h.batches[0].steps[0]), [1, 0, -0.2, 1, 24, 0], 'top edge: shx = -dx/height, dx from the edge midpoint (70,20), about the bottom edge midpoint (70,120)');
  assert.deepEqual(pts(h.batches[0].steps[0]), [[40, 20], [140, 20], [120, 120], [20, 120]]);
  assert.deepEqual(h.toasts, []);
});

test('the four edges shear along their own axis, the bottom and right ones with the opposite sign, and Ctrl lets the other axis shear too', async () => {
  const run = async (edge, to, mods) => { const h = harness([BIG]); await h.select(35, 35); await gesture(h, zone[edge], to, mods); assert.equal(h.batches.length, 1, edge); return M(h.batches[0].steps[0]); };
  assert.deepEqual(await run('s', [90, 120]), [1, 0, 0.2, 1, -4, 0], 'bottom edge right: shx = +dx/height about the top midpoint (70,20)');
  assert.deepEqual(await run('w', [20, 90]), [1, -0.2, 0, 1, 0, 24], 'left edge down: shy = -dy/width about the right midpoint (120,70)');
  assert.deepEqual(await run('e', [120, 90]), [1, 0.2, 0, 1, 0, -4]);
  const both = await run('n', [90, 40], { ctrlKey: true });
  assert.deepEqual(both, [1, -0.2, -0.2, 1, 24, 14], 'Ctrl: dy shears the other axis too');
  assert.deepEqual(await run('n', [90, 40]), [1, 0, -0.2, 1, 24, 0], 'without Ctrl the other axis stays');
});

test('a resize handle wins a tie with a skew zone, a zone on an edge shorter than 48 px does not exist, and an image or a text block shears like a stroke', async () => {
  const h = harness([BIG]); await h.select(35, 35);
  await gesture(h, [70, 20], [70, 0]); // the n handle itself: a scale, not a skew
  assert.equal(h.batches.length, 1); assert.deepEqual(M(h.batches[0].steps[0]).slice(0, 4), [1, 0, 0, 1.2]);
  const tiny = harness([quad('S', 20, 20, 60, 60)]); await tiny.select(35, 35);
  assert.equal(Skew.hit(Overlay.boxOf(tiny.root, tiny.root.selectedPaths), { x: 34.5, y: 20 }, 1), null, '40 px edge: no zone');
  for (const [item, inside] of [[IMG, [70, 70]], [TXT, [165, 40]]]) {
    const x = harness([item]); await x.select(inside[0], inside[1]);
    const b = x.root.selectedPaths[0].bounds, mid = (b.left + b.right) / 2;
    await gesture(x, [mid + 14.5, b.top], [mid + 24.5, b.top]);
    assert.equal(x.batches.length, 1, item.stableTarget.strokeId); assert.deepEqual(x.toasts, [], 'no refusal');
    assert.ok(Math.abs(M(x.batches[0].steps[0])[2]) > 0.05, 'a shear: the record-space matrix has a c term');
  }
});

test('the idle pointer over a handle sets its cursor (resize, turn, skew), and a skew edge hovered turns its ticks orange; leaving restores the default', async () => {
  const h = harness([BIG]); await h.select(35, 35);
  const canvas = { style: {}, dataset: {}, getBoundingClientRect: () => ({ left: 0, top: 0, width: 200, height: 200 }) };
  h.root.document.getElementById = () => canvas;
  h.root.NemoNativeSelectGesture = Gesture.bind(h.root);
  const over = (x, y, extra) => Skew.hover(h.root, { clientX: x, clientY: y, buttons: 0, altKey: false, ...extra });
  over(...zone.n); assert.equal(canvas.style.cursor, 'ew-resize'); assert.equal(Skew.hoveredEdge(h.root), 'n');
  over(...zone.w); assert.equal(canvas.style.cursor, 'ns-resize'); assert.equal(Skew.hoveredEdge(h.root), 'w');
  over(120, 120); assert.equal(canvas.style.cursor, 'nwse-resize', 'the se handle'); assert.equal(Skew.hoveredEdge(h.root), null);
  over(120, 20); assert.equal(canvas.style.cursor, 'nesw-resize');
  over(70, 40); assert.match(canvas.style.cursor, /^url\(/, 'the rotate ring'); // the ring is 30 px around the centre (70,70)
  over(70, 70); assert.equal(canvas.style.cursor, 'default');
  over(...zone.n, { altKey: true }); assert.equal(canvas.style.cursor, 'default', 'Alt shows no handle cursor');
});

test('Alt+press outside the selection box places the pivot (a click moves nothing and keeps the selection); Shift snaps to the nearest preset; no write at all', async () => {
  const h = harness([BIG, FAR]); await h.select(35, 35);
  h.press(140, 30, { altKey: true }); await h.release(140, 30, { altKey: true }); await settle();
  assert.deepEqual(h.root.state.xformAnchorCustom, [140, 30]);
  assert.deepEqual(h.ids(), ['A'], 'the click keeps the selection');
  assert.deepEqual(h.batches.map((b) => b.steps.map((st) => st.restyle.style)), [[{ xformAnchorCustom: [140, 30] }]], 'one pivot restyle, stored on the record (serP xformAnchorCustom)');
  assert.deepEqual(h.store[0].xformAnchorCustom, [140, 30]);
  assert.ok(h.svg().children.some((c) => c.tag === 'circle' && c.attrs.cx === '140' && c.attrs.cy === '30'), 'the crosshair is drawn at the pivot');
  h.press(140, 10, { altKey: true, shiftKey: true }); await h.release(140, 10, { altKey: true, shiftKey: true }); await settle();
  assert.equal(h.root.state.xformAnchorKey, 'tr'); assert.equal(h.root.state.xformAnchorCustom, null, 'the nearest preset of the box wins, the custom point goes');
  assert.deepEqual(Pivot.pointOf(h.root, Overlay.boxOf(h.root, h.root.selectedPaths)), { x: 120, y: 20 });
  assert.deepEqual(h.batches.map((b) => b.steps.map((st) => st.restyle.style)), [[{ xformAnchorCustom: [140, 30] }], [{ xformAnchorKey: 'tr', xformAnchorCustom: null }]], 'the preset replaces the stored custom point');
  assert.equal(h.store[0].xformAnchorKey, 'tr'); assert.equal('xformAnchorCustom' in h.store[0], false);
});

test('a stored pivot survives a deselect, a reselect and a reload of the listing; "Centrer le point d\'ancrage" writes the centre back and clears it', async () => {
  const h = harness([{ ...BIG, xformAnchorCustom: [140, 30] }, FAR]);
  await h.select(35, 35);
  assert.deepEqual(h.root.state.xformAnchorCustom, [140, 30], 'the listing carried the stored pivot (a reopened project)');
  assert.deepEqual(Pivot.pointOf(h.root, Overlay.boxOf(h.root, h.root.selectedPaths)), { x: 140, y: 30 });
  await h.select(170, 170);
  assert.equal(h.root.state.xformAnchorCustom, null, 'another stroke never given a pivot reads the centre');
  await h.select(35, 35);
  assert.deepEqual(h.root.state.xformAnchorCustom, [140, 30]);
  assert.deepEqual(h.batches, [], 'reading a stored pivot writes nothing');
  h.root.NemoNativeSelectionClipboard = { native: () => true, hasClip: () => false, copy() {}, cut() {}, paste() {}, duplicate() {} };
  const rows = await Menu.rows(h.root, h.root.selectedPaths);
  rows[rows.length - 1].action(); await settle();
  assert.deepEqual(h.batches.map((b) => b.steps.map((st) => st.restyle.style)), [[{ xformAnchorKey: 'mc', xformAnchorCustom: null }]]);
  assert.equal('xformAnchorCustom' in h.store[0], false); assert.equal(h.store[0].xformAnchorKey, 'mc');
  assert.deepEqual(Pivot.pointOf(h.root, Overlay.boxOf(h.root, h.root.selectedPaths)), { x: 70, y: 70 });
});

test('Alt+drag moves the pivot with the pointer, and the rotate ring then turns the selection about it', async () => {
  const h = harness([BIG]); await h.select(35, 35);
  h.press(140, 30, { altKey: true }); h.drag(150, 30, { altKey: true }); h.drag(160, 40, { altKey: true }); await settle();
  await h.release(160, 40, { altKey: true }); await settle();
  assert.deepEqual(h.root.state.xformAnchorCustom, [160, 40]);
  assert.deepEqual(h.batches.map((b) => b.steps.map((st) => st.restyle.style)), [[{ xformAnchorCustom: [160, 40] }]], 'the drag writes the pivot once, when it is released');
  // The ring is centred on the pivot now: 30 px right of it, dragged a quarter turn clockwise about (160,40).
  await gesture(h, [190, 40], [160, 70]);
  assert.equal(h.batches.length, 2);
  assert.deepEqual(M(h.batches[1].steps[0]), [0, 1, -1, 0, 200, -120], 'a quarter turn about the pivot');
});

test('a preset pivot follows the box and the inspector\'s numeric rotation reads the same point; the pivot comes back for the same stroke and resets for another', async () => {
  const h = harness([BIG, FAR]); await h.select(35, 35);
  h.root.state.xformAnchorKey = 'br'; Pivot.remember(h.root);
  assert.deepEqual(Pivot.pointOf(h.root, Overlay.boxOf(h.root, h.root.selectedPaths)), { x: 120, y: 120 });
  await h.select(170, 170);
  assert.deepEqual(h.ids(), ['B']); assert.equal(h.root.state.xformAnchorCustom, null);
  h.root.state.xformAnchorCustom = [1, 2]; Pivot.remember(h.root);
  await h.select(35, 35);
  assert.deepEqual(h.ids(), ['A']); assert.equal(h.root.state.xformAnchorKey, 'br', 'A is back with its own pivot');
  await h.select(170, 170);
  assert.deepEqual(h.root.state.xformAnchorCustom, [1, 2], 'and so is B');
});

test('Alt+drag on empty canvas with nothing selected is a freehand lasso: bounds first, then the centre inside or the path crossed; images by their centre only', async () => {
  const h = harness([BIG, FAR]);
  h.press(10, 10, { altKey: true }); h.drag(130, 10, { altKey: true }); h.drag(130, 130, { altKey: true }); h.drag(10, 130, { altKey: true }); await settle();
  const lasso = h.svg().children.filter((c) => c.tag === 'polygon').find((c) => /rgba\(74,158,255,0.08\)/.test(c.attrs.fill));
  assert.ok(lasso, 'the freehand path is drawn while dragging'); assert.equal(lasso.attrs.points, '10,10 130,10 130,130 10,130');
  assert.deepEqual(h.batches, []);
  await h.release(10, 130, { altKey: true }); await settle();
  assert.deepEqual(h.ids(), ['A'], 'A centre is inside, B is outside'); assert.deepEqual(h.batches, []);
  assert.ok(!h.svg().children.some((c) => c.attrs && c.attrs.fill === 'rgba(74,158,255,0.08)'), 'the preview ends with the drag');
  // Crossing without containing the centre: a small lasso over the corner of A takes the stroke, but not an image.
  for (const [item, want] of [[BIG, ['A']], [IMG, []]]) {
    const x = harness([item]);
    x.press(130, 130, { altKey: true }); x.drag(130, 100, { altKey: true }); x.drag(100, 100, { altKey: true }); x.drag(100, 130, { altKey: true }); await settle();
    await x.release(100, 130, { altKey: true }); await settle();
    assert.deepEqual(x.ids(), want, item.stableTarget.strokeId);
  }
});

test('a plain drag on empty canvas is still the rectangle marquee, and Alt+press with a selection never lassos', async () => {
  const h = harness([BIG, FAR]);
  h.press(2, 2); h.drag(60, 50); await settle();
  assert.ok(h.svg().children.some((c) => c.tag === 'polygon' && /rgba\(74,158,255,0.08\)/.test(c.attrs.fill)));
  await h.release(60, 50); await settle(); assert.deepEqual(h.ids(), ['A']);
  h.press(150, 20, { altKey: true }); h.drag(170, 40, { altKey: true }); await settle(); await h.release(170, 40, { altKey: true }); await settle();
  assert.deepEqual(h.ids(), ['A'], 'the selection is untouched: Alt moved the pivot');
  assert.deepEqual(h.root.state.xformAnchorCustom, [170, 40]);
});

// ---- A-select-rest (3): a click on any member of a group selects the whole group ----------------------------------------------------------------
const G1 = [{ id: 'g1' }];
const grouped = () => [quad('A', 10, 10, 50, 50, { groupPath: G1 }), quad('B', 110, 10, 150, 50, { groupPath: G1 }), quad('C', 10, 70, 50, 110), quad('D', 110, 70, 150, 110, { groupPath: [{ id: 'outer' }, { id: 'inner' }] }), quad('E', 110, 130, 150, 170, { groupPath: [{ id: 'outer' }] })];

test('a click on one member of a group selects every member of it (the outermost group), an ungrouped item alone, and Shift on a selected member takes the whole group away', async () => {
  const h = harness(grouped());
  await h.select(20, 20); await settle();
  assert.deepEqual(h.ids(), ['A', 'B'], 'the siblings of the clicked member come with it');
  await h.select(30, 90);
  assert.deepEqual(h.ids(), ['C'], 'an ungrouped item is just itself, and replaces the group');
  await h.select(120, 80); await settle();
  assert.deepEqual(h.ids(), ['D', 'E'], 'a nested group widens to its outermost level (baseline rootGroupOfItem)');
  await h.select(30, 90, true);
  assert.deepEqual(h.ids(), ['D', 'E', 'C'], 'Shift adds the picked item');
  await h.select(30, 90, true);
  assert.deepEqual(h.ids(), ['D', 'E'], 'and Shift on it again removes it');
  await h.select(120, 80, true); await settle();
  assert.deepEqual(h.ids(), [], 'Shift on a selected member removes the whole group');
  await h.select(120, 20, true); await settle();
  assert.deepEqual(h.ids(), ['A', 'B'], 'Shift on an unselected member adds the whole group');
  assert.deepEqual(h.batches, [], 'selecting writes nothing');
});

test('a press that becomes a drag on an unselected member moves the whole group in ONE batch', async () => {
  const h = harness(grouped());
  h.press(20, 20); h.drag(40, 30); await settle(); await h.release(40, 30); await settle();
  assert.equal(h.batches.length, 1);
  assert.deepEqual(h.batches[0].steps.map((st) => st.reshape.stableTarget.strokeId), ['A', 'B'], 'both members, in paint order');
  h.batches[0].steps.forEach((st) => assert.deepEqual(M(st), [1, 0, 0, 1, 20, 10]));
  assert.deepEqual(h.ids(), ['A', 'B']);
});

test('the Select menu ends with "Centrer le point d\'ancrage", which puts the pivot back at the centre and writes nothing', async () => {
  const h = harness([BIG, FAR]); await h.select(35, 35);
  h.root.NemoNativeSelectionClipboard = { native: () => true, hasClip: () => false, copy() {}, cut() {}, paste() {}, duplicate() {} };
  h.root.state.xformAnchorCustom = [150, 30]; Pivot.remember(h.root);
  const rows = await Menu.rows(h.root, h.root.selectedPaths);
  assert.deepEqual(rows.slice(-2).map((r) => (r.sep ? '---' : r.label)), ['---', "Centrer le point d'ancrage"]);
  rows[rows.length - 1].action(); await settle();
  assert.equal(h.root.state.xformAnchorCustom, null); assert.equal(h.root.state.xformAnchorKey, 'mc');
  assert.deepEqual(Pivot.pointOf(h.root, Overlay.boxOf(h.root, h.root.selectedPaths)), { x: 70, y: 70 });
  await h.select(170, 170); await h.select(35, 35);
  assert.equal(h.root.state.xformAnchorCustom, null, 'and the remembered pivot of that stroke is the centre too');
  assert.deepEqual(h.batches, []);
});
