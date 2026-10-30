'use strict';
// #2358: on a native object project the Animation 2D Tween UI sends the native tween commands. The REAL tweens.js entry points
// (generateTweens: the panel Tween button, the Regenerate Tween menu item and the cmdTween shortcut target; the Reassign flow;
// harmonizeAfterEdit) and the REAL timeline.js onTweenPairCurveChanged run against a fake native session and an owner double;
// the tests assert the exact payloads and that no legacy writer, history checkpoint or Paper rebuild is touched. A refusal
// surfaces as a typed unavailable notice. Without a native session the baseline path is unchanged. A native object project
// projects every layer row with frames: [] and builds no Paper item: the Reassign flow and the per-span curve edit take their key
// list from the native whole-row cells read and the clicked element identities from the native hit-test listing (#2358 REV-5).
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const { createFixture } = require('./native-object-rig-support.cjs');
const paper = require('./helpers/native-import-paper.cjs').load();
const Intent = require('../src/js/adapters/select-canvas-intent.js');

const SRC = path.join(__dirname, '..', 'src', 'js');
const read = (file) => fs.readFileSync(path.join(SRC, file), 'utf8');
const Tween = require('../src/js/adapters/native-object-tween.js');
const TweenUI = require('../src/js/adapters/native-object-tween-ui.js');
const UID = ['native-layer:1:0', 'native-layer:2:0'];
const CURVE = [{ x: 0, y: 0 }, { x: 0.3, y: 0.05 }, { x: 0.7, y: 0.95 }, { x: 1, y: 1 }];
const OPTIONS = { resamplePoints: 50, step: 1, skipManual: true, easing: CURVE };
const GEN = 'command.document.object.tween.generate', REGEN = 'command.document.object.tween.regenerate',
  SPLIT = 'command.document.object.tween.split', REASSIGN = 'command.document.object.tween.reassign';
const plain = (v) => JSON.parse(JSON.stringify(v));

const seg = (x, y) => ({ point: [x, y], handleIn: [0, 0], handleOut: [0, 0] });
// A closed 100 x 100 box at (x, y) as the native hit-test listing reports a plain filled record.
const box = (layerUid, frame, strokeId, x, y, extra) => Object.assign({ stableTarget: { contextId: 'scene-root', layerUid, frameScope: { kind: 'authored', frame }, strokeId },
  closed: true, count: 4, role: 'plain', filled: true, bounds: [x, y, x + 100, y + 100], matrix: [1, 0, 0, 1, 0, 0],
  segments: [seg(x, y), seg(x + 100, y), seg(x + 100, y + 100), seg(x, y + 100)] }, extra || {});
// frame -> listing records; a click at (50,50) hits the first box of a frame, (250,50) the second.
const LISTING = { 'native-layer:2:0': { 0: [box(UID[1], 0, 'a1', 0, 0), box(UID[1], 0, 'a2', 200, 0)], 4: [box(UID[1], 4, 'b1', 0, 0), box(UID[1], 4, 'b2', 200, 0)] },
  'native-layer:1:0': { 0: [box(UID[0], 0, 'z1', 0, 200)], 4: [box(UID[0], 4, 'z2', 0, 200)] } };
const AT = { a1: [50, 50], a2: [250, 50], b1: [50, 50], b2: [250, 50] };
// Whole-row cells of a layer with drawn (or empty) keys: [startFrame, count, kind, sourceFrame, hasContent, manualEdit].
function cellsOf(keys, total = 12) {
  const runs = [], at = Object.keys(keys).map(Number).sort((a, b) => a - b);
  at.forEach((f, i) => { const end = i + 1 < at.length ? at[i + 1] : total; runs.push([f, 1, 'key', f, keys[f], false]); if (end > f + 1) runs.push([f + 1, end - f - 1, 'held', f, keys[f], false]); });
  return at.length && at[0] > 0 ? [[0, at[0], 'unexposed', null, false, false]].concat(runs) : runs;
}
// A native object project on layer index 1 with drawn keys at 0, 4 and 8. Every layer row is projected with frames: [] and no
// Paper item exists: the key list comes from the session's cells read and the clickable elements from its presented listing.
function harness(options = {}) {
  const fx = createFixture(options.fixture || {});
  const live = fx.session.info;
  const info = () => Object.assign(live(), { canvasW: 1000, canvasH: 600, layers: UID.map((layerUid) => ({ layerUid })) });
  fx.session.info = info;
  const listeners = [], toasts = [], legacy = [], cellReads = [], listingReads = [], sleeps = [];
  const poison = (name) => () => { legacy.push(name); throw new Error('legacy writer reached: ' + name); };
  const rows = options.keys || { 0: true, 4: true, 8: true };
  let shown = 0, refuseCells = options.refuseCells || 0;
  fx.session.readLayerCells = async (layerUid) => {
    cellReads.push(layerUid);
    if (refuseCells > 0) { refuseCells--; return null; }
    return layerUid === UID[1] ? cellsOf(rows) : [[0, 12, 'unexposed', null, false, false]];
  };
  fx.session.readPresentedObjectSession = () => ({ presentation: Object.assign({}, info(), { frame: shown }) });
  fx.session.readPresentedStrokes = async (selector, pin, wanted) => {
    listingReads.push(selector);
    return { items: ((LISTING[selector.layerUid] || {})[selector.frame] || []).map(({ segments, ...item }) => (wanted(item) ? { ...item, segments } : item)) };
  };
  const state = { activeLayerIdx: 1, currentFrame: 0, totalFrames: 12, resamplePts: 50, tweenStep: 1, tweenSkipManual: true, tweenHarmonizeEdits: true,
    tweenOverrides: {}, tweenEasing: {}, easingCurve: { points: CURVE.map((p) => Object.assign({}, p)) }, tool: 'select',
    layers: [{ layerUid: UID[0], frames: [] }, { layerUid: UID[1], frames: [] }] };
  const root = { state, console, Math, JSON, Date, Promise, _sel: { frames: [] }, SM: { t: (k) => k }, showToast: (m) => toasts.push(m),
    document: { addEventListener: (type, fn) => listeners.push([type, fn]), getElementById: (id) => (id === 'canvas-area' ? { contains: () => true } : id === 'drawing-canvas' ? { getBoundingClientRect: () => ({ left: 0, top: 0, width: 1000, height: 600 }) } : null) },
    Path: paper.Path, Matrix: paper.Matrix, Point: paper.Point, view: { zoom: 1 }, selectedPaths: [],
    NemoNativeViewportUnderlay: { canvasMapping: { point: (e) => ({ x: e.clientX, y: e.clientY }) } },
    setTimeout: (fn) => { sleeps.push(shown); if (options.onSleep) options.onSleep(sleeps.length); setImmediate(fn); },
    goToFrame: (f) => { state.currentFrame = f; if (!options.lagging) shown = f; }, selClear() {}, selAdd() {},
    n20AllowLegacyWrite: poison('n20AllowLegacyWrite'), saveAllLayerFrames: poison('saveAllLayerFrames'), saveActiveLayerFrame: poison('saveActiveLayerFrame'),
    ensureStrokeId: poison('ensureStrokeId'), loadFrame: poison('loadFrame'), renderOS: poison('renderOS'), updateUI: poison('updateUI') };
  root.window = root;
  if (!options.noSession) root.NemoNativeObjectProject = { desktop: () => (options.inactive ? null : fx.session) };
  if (options.cutover) root.NemoNativeOpacityCutover = { blocksLegacy: () => true };
  root.NemoNativeObjectTween = Tween;
  root.NemoNativeTweenUI = TweenUI;
  root.NemoSelectCanvasIntent = Intent;
  vm.createContext(root);
  for (const file of ['domain/tween/assignment.js', 'application/history/frame-entry.js', 'tweens.js']) vm.runInContext(read(file), root, { filename: file });
  // The legacy gate and the baseline writers the page also defines are poisoned AFTER tweens.js declared its own gate.
  root.n20AllowLegacyWrite = options.gate ? (kind) => { legacy.push('gate:' + kind); return false; } : poison('n20AllowLegacyWrite');
  const timeline = read('timeline.js');
  const from = timeline.indexOf('function onTweenPairCurveChanged(li,fA){');
  vm.runInContext(timeline.slice(from, timeline.indexOf('\n}\n', from) + 3), root, { filename: 'timeline.js#onTweenPairCurveChanged' });
  return { root, fx, state, toasts, legacy, listeners, cellReads, listingReads, sleeps, present: (f) => { shown = f; },
    sent: () => fx.owner.log.received.map((r) => ({ operation: r.operation, payload: r.payload, expectedRevision: r.expectedRevision })) };
}

test('Tween button / menu / shortcut target: the whole layer is ONE native generate with the panel options', async () => {
  const h = harness();
  const result = await h.root.generateTweens();
  assert.deepEqual(plain(h.sent()), [{ operation: GEN, expectedRevision: 7, payload: { command: 'object.tween.generate', layerUid: UID[1], options: OPTIONS } }]);
  assert.equal(result.status, 'applied');
  assert.deepEqual(plain(result.receipt), { requestId: 'project-tween-1', contentRevision: 8, applied: true, historyEntriesAdded: 1 });
  assert.deepEqual(h.legacy, []);
  assert.deepEqual(h.toasts, []);
});

test('a frame selection narrows the generate to those frames, from the active layer only, as selection', async () => {
  const h = harness();
  h.root._sel.frames = [{ layer: 1, frame: 6 }, { layer: 0, frame: 3 }, { layer: 1, frame: 2 }, { layer: 1, frame: 6 }];
  await h.root.generateTweens();
  assert.deepEqual(plain(h.sent()[0].payload), { command: 'object.tween.generate', layerUid: UID[1], selection: [2, 6], options: OPTIONS });
  assert.deepEqual(h.legacy, []);
});

test('Resample, Step, Skip-manual and the easing curve of the panel travel as the four options', async () => {
  const h = harness();
  Object.assign(h.state, { resamplePts: 77, tweenStep: 3, tweenSkipManual: false });
  h.root._curveEditor = { getState: () => ({ points: [{ x: 0, y: 0 }, { x: 0.5, y: 0.2, tx: 1, ty: 0 }, { x: 1, y: 1 }] }) };
  await h.root.generateTweens();
  assert.deepEqual(plain(h.sent()[0].payload.options),
    { resamplePoints: 77, step: 3, skipManual: false, easing: [{ x: 0, y: 0 }, { x: 0.5, y: 0.2, tx: 1, ty: 0 }, { x: 1, y: 1 }] });
});

test('an explicit start-key map (harmonize, regenerate after an edit) is a native regenerate with starts', async () => {
  const h = harness();
  await h.root.generateTweens({ 0: true, 4: true, 7: false });
  assert.deepEqual(plain(h.sent()), [{ operation: REGEN, expectedRevision: 7, payload: { command: 'object.tween.regenerate', layerUid: UID[1], starts: [0, 4], options: OPTIONS } }]);
});

test('a motion-arc regenerate (skipUndo) has no native operation: typed unavailable, nothing sent', async () => {
  const h = harness();
  const result = await h.root.generateTweens(undefined, true);
  assert.deepEqual(plain(result), { status: 'unavailable', code: 'tween_motion_arc_unavailable' });
  assert.deepEqual(h.sent(), []);
  assert.deepEqual(h.toasts, ['Motion-arc tweens are unavailable in the native document.']);
});

test('Harmonize re-tweens an edited interpolated frame as ONE native split at that frame', async () => {
  const h = harness();
  await h.root.harmonizeAfterEdit(2);
  assert.deepEqual(plain(h.sent()), [{ operation: SPLIT, expectedRevision: 7, payload: { command: 'object.tween.split', layerUid: UID[1], frame: 2, options: OPTIONS } }]);
  const off = harness();
  off.state.tweenHarmonizeEdits = false;
  assert.equal((await off.root.harmonizeAfterEdit(2)).status, 'unchanged');
  assert.deepEqual(off.sent(), []);
  assert.deepEqual([h.legacy, off.legacy], [[], []]);
});

const settle = async () => { for (let i = 0; i < 12; i++) await new Promise((resolve) => setImmediate(resolve)); };
const click = (h, id) => h.root.reassignHandleClick({}, { clientX: AT[id][0], clientY: AT[id][1] });
// The whole user flow: the Reassign button, the start element(s), Enter, then the matching element on the next key.
async function reassignFlow(h, aIds, bId) {
  h.root.startReassign(); await settle();
  assert.equal(h.root._reassign.active, true);
  for (const id of aIds) { assert.equal(click(h, id), true); await settle(); }
  h.root.confirmReassignStep1(); await settle();
  assert.equal(h.root._reassign.step, 2);
  assert.equal(click(h, bId), true); await settle();
}

test('Reassign start: the key span comes from the native cells read, with every layer row projected frames: []', async () => {
  const h = harness();
  h.state.currentFrame = 2;
  h.root.startReassign(); await settle();
  assert.deepEqual(h.cellReads, [UID[1]]);
  assert.deepEqual([h.root._reassign.active, h.root._reassign.layer, h.root._reassign.frameA, h.root._reassign.frameB], [true, 1, 0, 4]);
  assert.deepEqual(h.toasts, ['toastClickStartElement']);
  // On the key itself the span is that key's own; on the last key there is no next key to reassign to.
  h.root.cancelReassign(true); h.state.currentFrame = 4; h.root.startReassign(); await settle();
  assert.deepEqual([h.root._reassign.frameA, h.root._reassign.frameB], [4, 8]);
  h.root.cancelReassign(true); h.state.currentFrame = 10; h.root.startReassign(); await settle();
  assert.equal(h.root._reassign.active, false);
  assert.deepEqual(h.toasts.slice(-1), ['No next keyframe to reassign to.']);
  assert.deepEqual([h.legacy, h.sent()], [[], []]);
  const early = harness({ keys: { 3: true, 8: true } });
  early.state.currentFrame = 1; early.root.startReassign(); await settle();
  assert.equal(early.root._reassign.active, false);
  assert.deepEqual(early.toasts, ['Put the playhead on or after a keyframe.']);
  assert.deepEqual([early.legacy, early.sent()], [[], []]);
});

test('Reassign start: a cells read refused while the presentation catches up is retried; a row that stays unavailable is a typed notice', async () => {
  const once = harness({ refuseCells: 2 });
  once.root.startReassign(); await settle();
  assert.deepEqual([once.cellReads.length, once.root._reassign.active, once.root._reassign.frameB], [3, true, 4]);
  const never = harness({ refuseCells: 9 });
  never.root.startReassign(); await settle();
  assert.deepEqual([never.cellReads.length, never.root._reassign.active], [3, false]);
  assert.deepEqual(never.toasts, ['Native tween is unavailable.']);
  const moved = harness();
  moved.root.startReassign(); moved.state.currentFrame = 6; await settle();
  assert.equal(moved.root._reassign.active, false, 'the playhead moved while the keys were read: nothing is armed');
});

test('Reassign: the clicked pair becomes ONE native reassign of its span; ids come from the native listing, none is stamped, no frame is saved', async () => {
  const h = harness();
  await reassignFlow(h, ['a1'], 'b1');
  assert.deepEqual(plain(h.sent()), [{ operation: REASSIGN, expectedRevision: 7,
    payload: { command: 'object.tween.reassign', layerUid: UID[1], span: { from: 0, to: 4, overrides: [{ aId: 'a1', bId: 'b1' }] }, options: OPTIONS } }]);
  assert.deepEqual(h.legacy, []);
  assert.deepEqual(h.toasts.slice(-1), ['toastInbetweenReassigned']);
  assert.deepEqual(plain(h.listingReads), [{ contextId: 'scene-root', layerUid: UID[1], frame: 0 }, { contextId: 'scene-root', layerUid: UID[1], frame: 4 }]);
  assert.equal(h.root._reassign.active, false);
});

test('Reassign of a split stroke sends aIds; a second reassign of the span keeps the first; the span curve rides along (frames: [] has no manual strokes)', async () => {
  const h = harness();
  h.state.tweenEasing[UID[1] + ':0-4'] = { points: [{ x: 0, y: 0 }, { x: 1, y: 1 }] };
  await reassignFlow(h, ['a1', 'a2'], 'b1');
  assert.deepEqual(plain(h.sent()[0].payload.span), { from: 0, to: 4, overrides: [{ aIds: ['a1', 'a2'], bId: 'b1' }], easing: [{ x: 0, y: 0 }, { x: 1, y: 1 }] });
  h.state.currentFrame = 0; h.present(0);
  await reassignFlow(h, ['a1'], 'b2');
  assert.deepEqual(plain(h.sent()[1].payload.span.overrides), [{ aId: 'a1', bId: 'b2' }]);
  assert.deepEqual(h.legacy, []);
});

test('the real canvas pointerdown listener swallows the gesture at once and resolves the element from the listing, with no engine bridge', async () => {
  const h = harness();
  h.root.startReassign(); await settle();
  const down = h.listeners.filter(([type]) => type === 'pointerdown').pop()[1], stopped = [];
  down({ target: {}, clientX: 250, clientY: 50, stopImmediatePropagation: () => stopped.push('stop'), preventDefault: () => stopped.push('prevent') });
  assert.deepEqual(stopped, ['stop', 'prevent']);
  await settle();
  assert.deepEqual(plain(h.root._reassign.aIds), ['a2']);
});

test('Enter right after the clicks waits for the element reads still in flight', async () => {
  const h = harness();
  h.root.startReassign(); await settle();
  click(h, 'a1'); click(h, 'a2');
  h.root.confirmReassignStep1();
  await settle();
  assert.deepEqual([h.root._reassign.step, plain(h.root._reassign.aIds)], [2, ['a1', 'a2']]);
  assert.deepEqual(h.toasts.filter((m) => m === 'toastClickAtLeastOneElement'), []);
});

test('a click on empty canvas, or on another layer, names no element: no id is invented and nothing is sent', async () => {
  const h = harness();
  h.root.startReassign(); await settle();
  assert.equal(h.root.reassignHandleClick({}, { clientX: 500, clientY: 500 }), true);
  await settle();
  // The other layer has an element at (50,250); only the layer being reassigned is tested.
  assert.equal(h.root.reassignHandleClick({}, { clientX: 50, clientY: 250 }), true);
  await settle();
  assert.deepEqual(plain(h.root._reassign.aIds), []);
  assert.deepEqual(h.toasts.slice(-2), ['toastNoElementHere', 'toastNoElementHere']);
  assert.deepEqual([h.sent(), h.legacy], [[], []]);
});

test('a linked fill answers its ribbon and a texture dab its anchor, as the baseline resolveBrushAnchor did', async () => {
  const h = harness();
  LISTING[UID[1]][0] = [box(UID[1], 0, 'fill1', 0, 0, { role: 'fill', owner: 'rib1' }), box(UID[1], 0, 'rib1', 400, 0, { role: 'ribbon' }),
    box(UID[1], 0, 'dab1', 200, 0, { brush: 'copy', group: 'g' }), box(UID[1], 0, 'anchor1', 600, 0, { brush: 'anchor', group: 'g' })];
  try {
    h.root.startReassign(); await settle();
    click(h, 'a1'); click(h, 'a2'); await settle();
    assert.deepEqual(plain(h.root._reassign.aIds), ['rib1', 'anchor1']);
  } finally { LISTING[UID[1]][0] = [box(UID[1], 0, 'a1', 0, 0), box(UID[1], 0, 'a2', 200, 0)]; }
});

test('step 2 waits until the canvas presents the next key, then reads THAT frame; a presentation that never arrives is a typed notice', async () => {
  const lag = harness({ lagging: true, onSleep: (n) => { if (n === 2) lag.present(4); } });
  lag.root.startReassign(); await settle();
  click(lag, 'a1'); lag.root.confirmReassignStep1(); await settle();
  assert.equal(lag.root._reassign.step, 2);
  click(lag, 'b1'); await settle();
  assert.ok(lag.sleeps.length >= 2);
  assert.deepEqual(plain(lag.sent()[0].payload.span.overrides), [{ aId: 'a1', bId: 'b1' }], 'b1 is of frame 4, not of the still-presented frame 0');
  const stuck = harness({ lagging: true });
  stuck.root.startReassign(); await settle();
  click(stuck, 'a1'); stuck.root.confirmReassignStep1(); await settle();
  click(stuck, 'b1'); await settle();
  assert.deepEqual([stuck.sent(), stuck.root._reassign.active, stuck.toasts.slice(-1)], [[], true, ['Native tween is unavailable.']]);
});

test('a playhead or layer change mid-flow cancels it before any read', async () => {
  const h = harness();
  h.root.startReassign(); await settle();
  h.state.currentFrame = 3;
  assert.equal(h.root.reassignHandleClick({}, { clientX: 50, clientY: 50 }), false);
  assert.equal(h.root._reassign.active, false);
  assert.deepEqual([h.listingReads, h.sent(), h.legacy], [[], [], []]);
});

test('an item without a projected strokeId cannot be reassigned: no id is invented', async () => {
  const h = harness();
  LISTING[UID[1]][0] = [box(UID[1], 0, undefined, 0, 0)];
  try {
    h.root.startReassign(); await settle();
    click(h, 'a1'); await settle();
    assert.deepEqual(plain(h.root._reassign.aIds), []);
    assert.deepEqual(h.toasts.slice(-1), ['toastNoElementHere']);
    assert.deepEqual([h.sent(), h.legacy], [[], []]);
  } finally { LISTING[UID[1]][0] = [box(UID[1], 0, 'a1', 0, 0), box(UID[1], 0, 'a2', 200, 0)]; }
});

test('the per-span easing edit (real timeline.js onTweenPairCurveChanged) is a native reassign of the span up to the next DRAWN key, from the cells read', async () => {
  const h = harness();
  h.state.tweenEasing[UID[1] + ':4-8'] = { points: [{ x: 0, y: 0 }, { x: 0.5, y: 0.9 }, { x: 1, y: 1 }] };
  await h.root.onTweenPairCurveChanged(1, 4);
  assert.deepEqual(plain(h.sent()), [{ operation: REASSIGN, expectedRevision: 7, payload: { command: 'object.tween.reassign', layerUid: UID[1],
    span: { from: 4, to: 8, overrides: [], easing: [{ x: 0, y: 0 }, { x: 0.5, y: 0.9 }, { x: 1, y: 1 }] }, options: OPTIONS } }]);
  const last = await h.root.onTweenPairCurveChanged(1, 8);
  assert.deepEqual([last.status, last.code], ['unavailable', 'tween_span_unavailable']);
  assert.equal(h.sent().length, 1);
  assert.deepEqual(h.legacy, []);
  // An empty key (no content) between two drawn keys is not a span end.
  const gap = harness({ keys: { 0: true, 4: false, 8: true } });
  await gap.root.onTweenPairCurveChanged(1, 0);
  assert.deepEqual(plain(gap.sent()[0].payload.span), { from: 0, to: 8, overrides: [] });
  // A cells read that stays unavailable leaves no drawn key to end the span: typed unavailable, nothing sent.
  const dark = harness({ refuseCells: 9 });
  assert.deepEqual(plain(await dark.root.onTweenPairCurveChanged(1, 0)), { status: 'unavailable', code: 'tween_span_unavailable' });
  assert.deepEqual([dark.sent(), dark.legacy], [[], []]);
});

test('the largest declared request (256 easing points, override list) is within the tween budget, not the 4096-byte lane limit', async () => {
  const h = harness();
  const big = Array.from({ length: 256 }, (_, i) => ({ x: i / 255, y: i / 255 }));
  h.state.easingCurve.points = big;
  assert.equal((await h.root.generateTweens()).status, 'applied');
  assert.ok(JSON.stringify(h.fx.owner.log.received[0]).length > 4096);
});

test('refusals and unavailability are typed notices; nothing falls back to the legacy writer', async () => {
  const locked = harness({ fixture: { refuse: () => ({ code: 'unavailable', reason: 'layer_locked' }) } });
  assert.deepEqual(plain(await locked.root.generateTweens()), { status: 'unavailable', code: 'layer_locked' });
  const bad = harness({ fixture: { refuse: () => ({ code: 'invalid_request', reason: 'span_not_adjacent' }) } });
  assert.deepEqual(plain(await bad.root.onTweenPairCurveChanged(1, 4)), { status: 'refused', code: 'invalid_request', reason: 'span_not_adjacent' });
  const miss = harness({ fixture: { applied: () => false } });
  assert.equal((await miss.root.generateTweens()).status, 'unchanged');
  const away = harness({ cutover: true, noSession: true });
  assert.deepEqual(plain(await away.root.generateTweens()), { status: 'unavailable', code: 'tween_session_unavailable' });
  const easing = harness();
  easing.state.easingCurve.points = [{ x: 0, y: 0 }, { x: 0, y: 1 }];
  assert.deepEqual(plain(await easing.root.generateTweens()), { status: 'unavailable', code: 'tween_easing_invalid' });
  assert.deepEqual([locked.toasts, bad.toasts, miss.toasts, away.toasts, easing.toasts], [
    ['Native tween is unavailable.'], ['Native tween was refused (span_not_adjacent).'], ['Need at least 2 drawn keyframes.'],
    ['Native tween is unavailable.'], ['Native tween is unavailable.']]);
  assert.deepEqual([locked, bad, miss, away, easing].map((h) => h.legacy), [[], [], [], [], []]);
});

test('an unlocated layer is refused before a request id is minted', async () => {
  const h = harness();
  h.state.layers[1].layerUid = 'stale';
  assert.deepEqual(plain(await h.root.generateTweens()), { status: 'unavailable', code: 'tween_layer_unresolved' });
  assert.deepEqual([h.sent(), h.fx.events.ids], [[], []]);
});

test('outside native mode the baseline path is unchanged: the legacy gate decides and no native command is sent', () => {
  const h = harness({ inactive: true, gate: true });
  assert.equal(h.root.generateTweens(), false);
  assert.deepEqual(h.legacy, ['gate:timeline-generate-tweens']);
  assert.deepEqual(h.sent(), []);
});

test('every Tween UI entry point calls the routed functions', () => {
  const timeline = read('timeline.js');
  assert.match(timeline, /generateTweens:generateTweens/);
  assert.match(timeline, /getElementById\('btn-tw'\)\.addEventListener\('click',function\(\)\{window\.SM\.generateTweens\(\);\}\)/);
  assert.match(timeline, /ctxRegenerateTween'\),shortcut:'T',action:function\(\)\{window\.SM\.generateTweens\(\);\}/);
  assert.match(timeline, /case'cmdTween':window\.SM\.generateTweens\(\)/);
  assert.match(timeline, /function onTweenPairCurveChanged\(li,fA\)\{\n[^\n]*\n  var nativeCurve=window\.NemoNativeTweenUI&&window\.NemoNativeTweenUI\.pairCurve\(window,li,fA\);\n  if\(nativeCurve\)return nativeCurve;/);
  const tweens = read('tweens.js');
  assert.match(tweens, /function generateTweens\(explicitRestrictTo,skipUndo\)\{\n[^\n]*\n  var nativeTween=window\.NemoNativeTweenUI&&window\.NemoNativeTweenUI\.generate\(window,explicitRestrictTo,skipUndo\);\n  if\(nativeTween\)return nativeTween;\n  if\(!n20AllowLegacyWrite/);
});
