'use strict';
// P25 / #1027: the native rotate-selection gesture lifecycle on a native object project. begin (press on the ring) captures stable targets
// (layerUid / strokeId, never Paper refs), the document pin, the frame and the box; update previews without a write; commit sends ONE
// object.vector.batch (one revision, one undo entry) with a record-space rotation matrix about the oriented box centre; Escape, a lost
// capture, or a selection / frame / layer / tool / document change before the write sends nothing; scale and Direct Select rotation do not
// enter this lifecycle; the legacy select-bridge rotate writer cannot run while the native cutover owns the canvas.
//
// Coordinate convention under test: the matrix is read in WORLD (presented document canvas) pixels, y down, a positive angle turning
// clockwise on screen; the angle is atan2(pointer - centre) - atan2(press - centre); the pivot is the centre of the oriented selection box
// (the union box for a multi-item selection). The engine receives each item's matrix conjugated into its own record frame.
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const Intent = require('../src/js/adapters/select-canvas-intent.js');
const Overlay = require('../src/js/adapters/native-select-overlay.js');
const Gesture = require('../src/js/adapters/native-select-gesture.js');
const Paper = require('./helpers/native-import-paper.cjs');

const src = (file) => fs.readFileSync(path.join(__dirname, '..', file), 'utf8');
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
const round = (v) => Math.round(v * 1e6) / 1e6 + 0;
const M = (step) => step.reshape.matrix.map(round);

function harness(items) {
  const paper = Paper.load(), body = el('body'), listeners = [], toasts = [], batches = [], frames = [];
  let pin = { instanceId: 'i', documentId: 'd', lifecycleGeneration: 1, contentRevision: 1, documentSnapshotId: 's1', canvasW: 200, canvasH: 200, layers: [{ layerUid: 'L' }] };
  let frame = 0;
  const store = items.map((i) => JSON.parse(JSON.stringify(i)));
  const session = { info: () => pin, isActive: () => true, blocksLegacy: () => true, readPresentedObjectSession: () => ({ presentation: { ...pin, frame } }),
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
    NemoNativeObjectLabs: { desktop: () => ({ batchStrokes: async (layerUid, frameIdx, steps) => {
      steps.forEach((s) => { s.before = store.find((i) => i.stableTarget.strokeId === s.reshape.stableTarget.strokeId).segments.map((q) => q.point); });
      batches.push({ layerUid, frame: frameIdx, steps });
      steps.forEach((s) => { const it = store.find((i) => i.stableTarget.strokeId === s.reshape.stableTarget.strokeId), m = s.reshape.matrix;
        if (Math.abs(m[0] - m[3]) < 1e-9 && Math.abs(m[1] + m[2]) < 1e-9 && Math.abs(Math.hypot(m[0], m[1]) - 1) < 1e-6 && Math.abs(m[1]) > 1e-9) it.boxAngle = ((it.boxAngle || 0) + Math.atan2(m[1], m[0]) * 180 / Math.PI) % 360;
        const at = (q) => [m[0] * q[0] + m[2] * q[1] + m[4], m[1] * q[0] + m[3] * q[1] + m[5]];
        it.segments = it.segments.map((q) => ({ point: at(q.point), handleIn: q.handleIn, handleOut: q.handleOut }));
        const [x0, y0, x1, y1] = it.bounds, c = [at([x0, y0]), at([x1, y1])]; it.bounds = [Math.min(c[0][0], c[1][0]), Math.min(c[0][1], c[1][1]), Math.max(c[0][0], c[1][0]), Math.max(c[0][1], c[1][1])]; });
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
  const ev = (x, y, extra) => ({ button: 0, pointerId: 1, buttons: 1, clientX: x, clientY: y, shiftKey: false, altKey: false, currentTarget: target, stopImmediatePropagation() {}, preventDefault() {}, ...extra });
  const h = { root, intent, toasts, batches, store, body, listeners, session,
    setPin: (n) => { pin = { ...pin, ...n }; }, setFrame: (n) => { frame = n; root.state.currentFrame = n; },
    press: (x, y, extra) => intent.down(ev(x, y, extra)), drag: (x, y, extra) => intent.move(ev(x, y, extra)), release: (x, y, extra) => intent.up(ev(x, y, { buttons: 0, ...extra })),
    key: (key) => listeners.filter((l) => l[0] === 'keydown').forEach((l) => l[1]({ key, stopImmediatePropagation() {}, preventDefault() {} })),
    svg: () => body.children.filter((c) => c.tag === 'svg').slice(-1)[0],
    outline: () => h.svg() ? ((h.svg().children.find((c) => c.tag === 'path') || { attrs: {} }).attrs.d || '') : '',
    async select(x, y, shift) { h.press(x, y, { shiftKey: !!shift }); await h.release(x, y, { shiftKey: !!shift }); await settle(); },
    ids: () => root.selectedPaths.map((p) => p.data.strokeId) };
  return h;
}
const BIG = quad('A', 20, 20, 120, 120); // box centre (70,70); the ring spot is 36 px from it: (106,70)
const RING = [100, 70]; // 30 px right of the centre, inside the 7 px ring band (36 * wpp = 30 = min(36, 0.3 * 100))
const QUARTER = [70, 100]; // a quarter turn clockwise from RING
// Begins a rotate drag of the selected BIG and returns after the first move has been processed.
async function startTurn(h, to = QUARTER) { h.press(RING[0], RING[1]); h.drag(to[0], to[1]); await settle(); }

test('begin: a press on the ring captures stable targets and the document snapshot; the commit names them as plain data', async () => {
  const h = harness([BIG]); await h.select(35, 35);
  assert.ok(h.root.selectedPaths.every((p) => p.data.nativeStableTarget), 'the selection carries stable targets');
  await startTurn(h);
  await h.release(...QUARTER); await settle();
  const step = h.batches[0].steps[0];
  assert.deepEqual(step.reshape.stableTarget, BIG.stableTarget, 'layerUid + strokeId + frameScope, nothing live');
  assert.equal(h.batches[0].layerUid, 'L');
  assert.equal(h.batches[0].frame, 0);
  assert.deepEqual(JSON.parse(JSON.stringify(h.batches[0].steps.map((s) => s.reshape))), h.batches[0].steps.map((s) => s.reshape), 'the payload is pure JSON (no Paper item or function reference)');
  assert.equal(step.reshape.geometry, undefined, 'a rotation is a matrix, not re-sent geometry');
});

test('begin: the snapshot is the gesture\'s, not whatever root.selectedPaths holds at release', async () => {
  const h = harness([BIG, quad('B', 150, 150, 190, 190)]); await h.select(35, 35);
  await startTurn(h);
  const kept = h.root.selectedPaths;
  h.root.selectedPaths = kept.slice(); // same stroke ids, fresh array and fresh instances of the list: the snapshot is by id
  await h.release(...QUARTER); await settle();
  assert.equal(h.batches.length, 1);
  assert.equal(h.batches[0].steps[0].reshape.stableTarget.strokeId, 'A');
});

test('update: the drag previews the turned outline and writes nothing, however many moves', async () => {
  const h = harness([BIG]); await h.select(35, 35);
  const before = JSON.stringify(h.store), outlineBefore = h.outline();
  h.press(...RING); h.drag(95, 85); h.drag(85, 95); h.drag(...QUARTER); await settle();
  assert.deepEqual(h.batches, [], 'no native write while the pointer moves');
  assert.equal(JSON.stringify(h.store), before, 'the document is untouched');
  assert.notEqual(h.outline(), outlineBefore, 'the preview moved');
  assert.match(h.outline(), /^M 120 (19\.9999|20)/, 'the first anchor (20,20) turned a quarter about (70,70) is at (120,20)');
});

test('commit: exactly ONE batch of one step (one revision, one undo entry) with the rotation matrix about the box centre, world px, clockwise', async () => {
  const h = harness([BIG]); await h.select(35, 35);
  await startTurn(h); await h.release(...QUARTER); await settle();
  assert.equal(h.batches.length, 1, 'one object.vector.batch');
  assert.equal(h.batches[0].steps.length, 1);
  assert.deepEqual(M(h.batches[0].steps[0]), [0, 1, -1, 0, 140, 0], 'R(90deg) about (70,70): [cos sin -sin cos cx-cos*cx+sin*cy cy-sin*cx-cos*cy]');
  assert.ok(Math.abs(h.store[0].boxAngle - 90) < 1e-9, 'the engine stored the turn with the same batch (box angle)');
  assert.deepEqual(h.ids(), ['A'], 'the selection is re-read at the new geometry');
  assert.equal(h.listeners.length, 0, 'the key listener is gone');
  // The counter-clockwise direction and a non-right angle: the sign follows atan2 of the pointer about the centre.
  const g = harness([BIG]); await g.select(35, 35);
  g.press(...RING); g.drag(70, 40); await settle(); await g.release(70, 40); await settle();
  assert.deepEqual(M(g.batches[0].steps[0]), [0, -1, 1, 0, 0, 140], 'a quarter turn counter-clockwise (up) is R(-90deg)');
});

test('commit: a multi-item selection turns about the shared box centre and is still ONE batch', async () => {
  const h = harness([quad('A', 20, 20, 60, 60), quad('B', 100, 100, 140, 140)]);
  await h.select(30, 30); await h.select(120, 120, true);
  assert.deepEqual(h.ids(), ['A', 'B']);
  const box = Overlay.boxOf(h.root, h.root.selectedPaths); // union (20,20)-(140,140): centre (80,80)
  assert.deepEqual([box.center.x, box.center.y], [80, 80]);
  const ring = Math.min(36, 120 * 0.3); // 36 px right of the centre
  h.press(80 + ring, 80); h.drag(80, 80 + ring); await settle(); await h.release(80, 80 + ring); await settle();
  assert.equal(h.batches.length, 1, 'one history entry for the whole selection');
  assert.deepEqual(h.batches[0].steps.map((s) => s.reshape.stableTarget.strokeId), ['A', 'B']);
  h.batches[0].steps.forEach((s) => assert.deepEqual(M(s), [0, 1, -1, 0, 160, 0]));
});

test('commit: a mixed vector + image + text selection turns as ONE batch of three matrices', async () => {
  const ribbon = quad('R', 20, 150, 120, 170, { role: 'ribbon', center: [{ point: [20, 160], handleIn: [0, 0], handleOut: [10, 0], width: 8 }, { point: [120, 160], handleIn: [-10, 0], handleOut: [0, 0], width: 8 }] });
  const img = quad('IMG', 20, 20, 120, 120, { role: 'raster' }), txt = quad('T', 140, 20, 190, 60, { role: 'text' });
  const h = harness([ribbon, img, txt]);
  await h.select(70, 160); await h.select(70, 70, true); await h.select(165, 40, true);
  assert.deepEqual(h.ids(), ['R', 'IMG', 'T']);
  const box = Overlay.boxOf(h.root, h.root.selectedPaths), ring = Math.min(36, Math.max(box.width, box.height) * 0.3);
  h.press(box.center.x + ring, box.center.y); h.drag(box.center.x, box.center.y + ring); await settle();
  await h.release(box.center.x, box.center.y + ring); await settle();
  assert.deepEqual(h.toasts, [], 'no refusal: image and text take a turn');
  assert.equal(h.batches.length, 1, 'one history entry for the mixed turn');
  assert.deepEqual(h.batches[0].steps.map((s) => s.reshape.stableTarget.strokeId), ['R', 'IMG', 'T']);
  h.batches[0].steps.forEach((s) => assert.equal(round(Math.hypot(s.reshape.matrix[0], s.reshape.matrix[1])), 1, 'a pure rotation'));
});

test('cancel: Escape mid-drag sends nothing, restores the unturned preview and keeps the selection', async () => {
  const h = harness([BIG]); await h.select(35, 35);
  const before = h.outline();
  await startTurn(h);
  assert.notEqual(h.outline(), before);
  h.key('Escape');
  await h.release(...QUARTER); await settle();
  assert.deepEqual(h.batches, []);
  assert.equal(h.outline(), before, 'the overlay shows the initial selection again');
  assert.deepEqual(h.ids(), ['A']);
  assert.equal(h.listeners.length, 0);
  assert.equal(h.store[0].boxAngle, undefined);
  // The next gesture is unaffected by the cancelled one.
  await startTurn(h); await h.release(...QUARTER); await settle();
  assert.equal(h.batches.length, 1);
});

test('cancel: a lost capture mid-drag sends nothing', async () => {
  const h = harness([BIG]); await h.select(35, 35);
  await startTurn(h); h.intent.cancel(); await h.release(...QUARTER); await settle();
  assert.deepEqual(h.batches, []);
});

test('cancel: Escape pressed between the release and the send still cancels (the write is not yet sent)', async () => {
  const h = harness([BIG]); await h.select(35, 35);
  await startTurn(h);
  const done = h.release(...QUARTER); // the commit is now awaiting its re-read of the targets
  h.key('Escape');
  await done; await settle();
  assert.deepEqual(h.batches, [], 'the cancel won: nothing was sent');
  assert.equal(h.listeners.length, 0);
  assert.deepEqual(h.ids(), ['A']);
});

test('cancel: Escape after the write is sent is not a cancel (its key listener is already removed)', async () => {
  const h = harness([BIG]); await h.select(35, 35);
  await startTurn(h); await h.release(...QUARTER); await settle();
  assert.equal(h.batches.length, 1);
  assert.equal(h.listeners.length, 0);
  h.key('Escape'); assert.equal(h.batches.length, 1);
});

test('context: a frame change mid-drag ends the turn at once with a notice, clears the preview and writes nothing', async () => {
  const h = harness([BIG]); await h.select(35, 35);
  const before = h.outline();
  await startTurn(h);
  h.setFrame(1); h.drag(60, 90); await settle();
  assert.doesNotMatch(h.outline(), /^M 120 /, 'the turned preview was dropped as soon as the snapshot went stale');
  assert.ok(h.toasts.some((t) => /document changed/.test(t)), h.toasts.join('|'));
  await h.release(60, 90); await settle();
  assert.deepEqual(h.batches, []);
});

test('context: a selection change mid-drag (another item picked from elsewhere) writes nothing', async () => {
  const h = harness([BIG, quad('B', 150, 150, 190, 190)]); await h.select(35, 35);
  await startTurn(h);
  h.root.selectedPaths = []; h.drag(60, 90); await settle();
  await h.release(60, 90); await settle();
  assert.deepEqual(h.batches, []);
  assert.ok(h.toasts.some((t) => /document changed/.test(t)));
});

test('context: a layer, tool or document-revision change mid-drag writes nothing', async () => {
  for (const change of [(h) => { h.root.state.activeLayerIdx = 1; }, (h) => { h.root.state.tool = 'subselect'; }, (h) => h.setPin({ contentRevision: 9, documentSnapshotId: 's9' })]) {
    const h = harness([BIG]); await h.select(35, 35);
    await startTurn(h); change(h); h.drag(60, 90); await settle(); await h.release(60, 90); await settle();
    assert.deepEqual(h.batches, []);
  }
});

test('context: a change only noticed at the release is refused the same way', async () => {
  const h = harness([BIG]); await h.select(35, 35);
  await startTurn(h); h.setPin({ contentRevision: 9, documentSnapshotId: 's9' });
  await h.release(...QUARTER); await settle();
  assert.deepEqual(h.batches, []);
  assert.ok(h.toasts.some((t) => /document changed/.test(t)));
});

test('scale is not the rotate lifecycle: a handle drag sends a scale matrix, stores no turn, and its box angle stays unset', async () => {
  const h = harness([BIG]); await h.select(35, 35);
  h.press(120, 120); h.drag(170, 170); await settle(); await h.release(170, 170); await settle();
  assert.equal(h.batches.length, 1);
  const m = M(h.batches[0].steps[0]);
  assert.deepEqual(m, [1.5, 0, 0, 1.5, -10, -10], 'a uniform-direction corner scale about the opposite corner (20,20)');
  assert.equal(h.store[0].boxAngle, undefined, 'no rotation was recorded');
  assert.equal(Gesture.matrixOf({ kind: 'scale', index: 4, dir: 'se', axis: 0, box: { center: { x: 70, y: 70 }, handles: [[20, 20], [70, 20], [120, 20], [120, 70], [120, 120], [70, 120], [20, 120], [20, 70]] } }, { x: 170, y: 170 }, false, false).length, 6);
});

test('Direct Select rotation is untouched: the subselect bridge never enters the Select gesture, and the Select gesture never reads subselect', () => {
  const sub = src('src/js/subselect-bridge.js'), gesture = src('src/js/adapters/native-select-gesture.js');
  assert.doesNotMatch(sub, /NemoNativeSelectGesture|native-select-gesture|NemoSelectCanvasIntent/);
  assert.doesNotMatch(gesture, /NemoNativeSubselect|subselect-bridge|SubselectController/);
  const bridge = src('src/js/select-bridge.js');
  assert.match(bridge, /if \(state\.tool === 'subselect'\) return true;/, 'a Direct Select gesture is never consumed by the Select intent');
});

test('the legacy select-bridge rotate writer cannot run on a native project: every pointer route goes through nativePointerEvent first', () => {
  const bridge = src('src/js/select-bridge.js');
  for (const [type, legacy] of [['pointerdown', 'onDown'], ['pointermove', 'onMove'], ['pointerup', 'onUp'], ['pointercancel', 'onUp']]) {
    assert.ok(bridge.includes(`addEventListener('${type}', function (e) { if (!nativePointerEvent(`) && bridge.includes(`) ${legacy}(e); }, { capture: true })`), `${type} -> ${legacy} only when native declines`);
  }
  // xform-rotate (the legacy ring drag) is assigned only inside onDown, and onDown / onMove / onUp each begin with the native edit guard.
  const modes = [...bridge.matchAll(/mode = 'xform-rotate'/g)].length;
  assert.equal(modes, 1, 'one legacy rotate entry point');
  assert.match(bridge, /if \(mode && !allowLegacySelectionEdit\(e, 'select'\)\) return;/);
  assert.match(bridge, /if \(\(mode \|\| draggingArc \|\| motionGesture\) && !allowLegacySelectionEdit\(e, 'select'\)\) return;/);
  assert.match(bridge, /if \(shouldIntercept\(\) && !allowLegacySelectionEdit\(e, 'select'\)\) return;/);
});

function bootSelect(cutover) {
  const handlers = {}, target = { addEventListener(type, fn) { handlers[type] = fn; } }, calls = { world: 0 };
  const context = { console, Object, state: { tool: 'select', playing: false, appMode: 'anim2d', layers: [{ locked: false }], activeLayerIdx: 0, selectedStrokeIndices: [] },
    document: { readyState: 'complete', addEventListener() {}, getElementById: () => target }, Point: function Point(x, y) { this.x = x; this.y = y; }, window: null };
  context.window = context; context.addEventListener = () => {};
  context.SMEngineBridge = { isEnabled: () => true, screenToWorld: () => { calls.world++; return [0, 0]; } };
  if (cutover !== undefined) context.NemoNativeOpacityCutover = cutover;
  vm.runInNewContext(src('src/js/select-bridge.js'), context, { filename: 'select-bridge.js' });
  return { handlers, calls };
}
const pointer = (type) => { const e = { type, button: 0, pointerId: 1, buttons: 1, clientX: 106, clientY: 70, stopped: 0, stopImmediatePropagation() { this.stopped++; }, preventDefault() {} }; return e; };

test('runtime: with the native cutover owning the canvas (or throwing) a Select press / drag / release is consumed and the legacy writer never reads the canvas', () => {
  for (const cutover of [{ blocksLegacy: () => true }, { blocksLegacy() { throw new Error('boom'); } }, null]) {
    const { handlers, calls } = bootSelect(cutover);
    for (const type of ['pointerdown', 'pointermove', 'pointerup']) { const e = pointer(type); try { handlers[type](e); } catch (_) { /* a missing native intent port is a consumed gesture */ } assert.ok(e.stopped >= 1, `${type} consumed`); }
    assert.equal(calls.world, 0, 'screenToWorld (the first thing the legacy writer does) was never called');
  }
  const { handlers, calls } = bootSelect(undefined); // no cutover at all: the legacy path is the owner, so the control is live
  try { handlers.pointerdown(pointer('pointerdown')); } catch (_) {}
  assert.ok(calls.world > 0, 'control: without the native cutover the legacy writer does run');
});
