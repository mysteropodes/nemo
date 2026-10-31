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

test('bitmap brushes and an unresolved dab have no native transform here: a drag is refused with a notice naming them, and nothing is written', async () => {
  const refused = async (extra, pattern) => { const x = harness([quad('X', 20, 20, 120, 120, extra)]); await x.select(35, 35);
    x.press(35, 35); x.drag(60, 60); await settle(); await x.release(60, 60); await settle();
    assert.deepEqual(x.batches, [], pattern); assert.ok(x.toasts.some((t) => pattern.test(t)), x.toasts.join('|')); };
  await refused({ role: 'brush', brush: 'bitmap', group: 'g' }, /bitmap brushes/);
  await refused({ role: 'brush', brush: 'copy', group: 'g' }, /brush dabs/);
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

test('a text block moves and scales in one commit; a mirror of text or of an image is refused with a notice and writes nothing', async () => {
  let h = harness([TXT]); await h.select(165, 40);
  await gesture(h, [165, 40], [185, 60]);
  assert.equal(h.batches.length, 1); assert.deepEqual(M(h.batches[0].steps[0]), [1, 0, 0, 1, 20, 20]); assert.deepEqual(h.toasts, []);
  h = harness([TXT]); await h.select(165, 40);
  await gesture(h, [190, 60], [240, 100]);
  assert.equal(h.batches.length, 1, 'a corner handle scales text about the opposite corner');
  assert.deepEqual(M(h.batches[0].steps[0]), [2, 0, 0, 2, -140, -20]);
  const none = async (items, press, to, pattern, pick = [165, 40]) => { const x = harness(items); await x.select(pick[0], pick[1]);
    await gesture(x, press, to); assert.deepEqual(x.batches, [], String(pattern)); assert.ok(x.toasts.some((t) => pattern.test(t) && /not available/.test(t)), x.toasts.join('|')); };
  await none([TXT], [190, 60], [110, 100], /Flipping text/);
  await none([IMG], [120, 70], [-40, 70], /Flipping images/, [70, 70]);
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

test('a turned image scales along its own axes, not along the world axes (a shear has no stored form)', async () => {
  const c = Math.SQRT1_2, turned = quad('IMG', 20, 20, 120, 120, { role: 'raster', filled: true, matrix: [c, c, -c, c, 70 - c * 70 + c * 70, 70 - c * 70 - c * 70] });
  const h = harness([turned]); await h.select(70, 70);
  assert.deepEqual(h.ids(), ['IMG']);
  const box = h.root.selectedPaths[0].bounds;
  await gesture(h, [box.x + box.width, box.y + box.height], [box.x + box.width + 40, box.y + box.height]);
  assert.deepEqual(h.batches, []);
  assert.ok(h.toasts.some((t) => /Skewing images/.test(t) && /not available/.test(t)), h.toasts.join('|'));
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
  const h = harness([posed]); await h.select(100, 40);
  await gesture(h, [100, 40], [120, 50]);
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
  await h.select(90, 50);
  assert.deepEqual(h.ids(), ['R'], 'a click on the fill selects its ribbon');
  h.press(90, 50); h.drag(90, 90); await settle(); await h.release(90, 90); await settle();
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
