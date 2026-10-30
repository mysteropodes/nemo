'use strict';
// #2358: on a native object project the Animation 2D Tween UI sends the native tween commands. The REAL tweens.js entry points
// (generateTweens: the panel Tween button, the Regenerate Tween menu item and the cmdTween shortcut target; the Reassign flow;
// harmonizeAfterEdit) and the REAL timeline.js onTweenPairCurveChanged run against a fake native session and an owner double;
// the tests assert the exact payloads and that no legacy writer, history checkpoint or Paper rebuild is touched. A refusal
// surfaces as a typed unavailable notice. Without a native session the baseline path is unchanged.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const { createFixture } = require('./native-object-rig-support.cjs');

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

function frames(strokesAt) {
  return Array.from({ length: 12 }, (_, i) => (strokesAt[i] ? { isKeyframe: true, strokes: strokesAt[i] } : { isKeyframe: false, isInterpolated: i > 0, strokes: [] }));
}
// A native object project on layer index 1 with drawn keys at 0, 4 and 8.
function harness(options = {}) {
  const fx = createFixture(options.fixture || {});
  const live = fx.session.info;
  fx.session.info = () => Object.assign(live(), { layers: UID.map((layerUid) => ({ layerUid })) });
  const toasts = [], legacy = [];
  const poison = (name) => () => { legacy.push(name); throw new Error('legacy writer reached: ' + name); };
  const state = { activeLayerIdx: 1, currentFrame: 0, totalFrames: 12, resamplePts: 50, tweenStep: 1, tweenSkipManual: true, tweenHarmonizeEdits: true,
    tweenOverrides: {}, tweenEasing: {}, easingCurve: { points: CURVE.map((p) => Object.assign({}, p)) }, tool: 'select',
    layers: [{ layerUid: UID[0], frames: frames({}) },
      { layerUid: UID[1], frames: frames({ 0: [{ strokeId: 'a1' }, { strokeId: 'a2' }], 4: [{ strokeId: 'b1' }], 8: [{ strokeId: 'c1' }] }) }] };
  class PathStub { constructor(strokeId) { this.data = { strokeId }; } }
  let hitTarget = null;
  const root = { state, console, Math, JSON, Date, Promise, _sel: { frames: [] }, SM: { t: (k) => k }, showToast: (m) => toasts.push(m),
    document: { addEventListener() {}, getElementById: () => null }, Path: PathStub, view: { zoom: 1 }, selectedPaths: [],
    userLayers: [null, { hitTest: () => (hitTarget ? { item: hitTarget } : null) }],
    resolveBrushAnchor: (item) => item, goToFrame: (f) => { state.currentFrame = f; }, selClear() {}, selAdd() {},
    n20AllowLegacyWrite: poison('n20AllowLegacyWrite'), saveAllLayerFrames: poison('saveAllLayerFrames'), saveActiveLayerFrame: poison('saveActiveLayerFrame'),
    ensureStrokeId: poison('ensureStrokeId'), loadFrame: poison('loadFrame'), renderOS: poison('renderOS'), updateUI: poison('updateUI') };
  root.window = root;
  if (!options.noSession) root.NemoNativeObjectProject = { desktop: () => (options.inactive ? null : fx.session) };
  if (options.cutover) root.NemoNativeOpacityCutover = { blocksLegacy: () => true };
  root.NemoNativeObjectTween = Tween;
  root.NemoNativeTweenUI = TweenUI;
  vm.createContext(root);
  for (const file of ['domain/tween/assignment.js', 'application/history/frame-entry.js', 'tweens.js']) vm.runInContext(read(file), root, { filename: file });
  // The legacy gate and the baseline writers the page also defines are poisoned AFTER tweens.js declared its own gate.
  root.n20AllowLegacyWrite = options.gate ? (kind) => { legacy.push('gate:' + kind); return false; } : poison('n20AllowLegacyWrite');
  const timeline = read('timeline.js');
  const from = timeline.indexOf('function onTweenPairCurveChanged(li,fA){');
  vm.runInContext(timeline.slice(from, timeline.indexOf('\n}\n', from) + 3), root, { filename: 'timeline.js#onTweenPairCurveChanged' });
  return { root, fx, state, toasts, legacy, hit: (p) => { hitTarget = p; }, sent: () => fx.owner.log.received.map((r) => ({ operation: r.operation, payload: r.payload, expectedRevision: r.expectedRevision })) };
}
const settle = () => new Promise((resolve) => setImmediate(resolve));

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

async function reassignFlow(h, aIds, bId) {
  h.root.startReassign();
  h.hit(new h.root.Path(aIds[0])); assert.equal(h.root.reassignHandleClick({}), true);
  for (const id of aIds.slice(1)) { h.hit(new h.root.Path(id)); h.root.reassignHandleClick({}); }
  h.root.confirmReassignStep1();
  h.hit(new h.root.Path(bId));
  assert.equal(h.root.reassignHandleClick({}), true);
  await settle(); await settle();
}

test('Reassign: the clicked pair becomes ONE native reassign of its span; no id is stamped and no frame is saved', async () => {
  const h = harness();
  await reassignFlow(h, ['a1'], 'b1');
  assert.deepEqual(plain(h.sent()), [{ operation: REASSIGN, expectedRevision: 7,
    payload: { command: 'object.tween.reassign', layerUid: UID[1], span: { from: 0, to: 4, overrides: [{ aId: 'a1', bId: 'b1' }] }, options: OPTIONS } }]);
  assert.deepEqual(h.legacy, []);
  assert.deepEqual(h.toasts.slice(-1), ['toastInbetweenReassigned']);
});

test('Reassign of a split stroke sends aIds; a second reassign of the span keeps the first; the span curve and manual strokes ride along', async () => {
  const h = harness();
  h.state.tweenEasing[UID[1] + ':0-4'] = { points: [{ x: 0, y: 0 }, { x: 1, y: 1 }] };
  h.state.layers[1].frames[0].tweenManualMode = true;
  h.state.layers[1].frames[0].strokes[1].tweenOn = true;
  await reassignFlow(h, ['a1', 'a2'], 'b1');
  assert.deepEqual(plain(h.sent()[0].payload.span), { from: 0, to: 4, overrides: [{ aIds: ['a1', 'a2'], bId: 'b1' }], easing: [{ x: 0, y: 0 }, { x: 1, y: 1 }], manualStrokes: ['a2'] });
  h.state.currentFrame = 0;
  await reassignFlow(h, ['a1'], 'b2');
  assert.deepEqual(plain(h.sent()[1].payload.span.overrides), [{ aId: 'a1', bId: 'b2' }]);
  assert.deepEqual(h.legacy, []);
});

test('an item without a projected strokeId cannot be reassigned: no id is invented', async () => {
  const h = harness();
  h.root.startReassign();
  h.hit(new h.root.Path(undefined));
  assert.equal(h.root.reassignHandleClick({}), true);
  assert.deepEqual(plain(h.root._reassign.aIds), []);
  assert.deepEqual(h.toasts.slice(-1), ['toastNoElementHere']);
  assert.deepEqual([h.sent(), h.legacy], [[], []]);
});

test('the per-span easing edit (real timeline.js onTweenPairCurveChanged) is a native reassign of that span with its curve', async () => {
  const h = harness();
  h.state.tweenEasing[UID[1] + ':4-8'] = { points: [{ x: 0, y: 0 }, { x: 0.5, y: 0.9 }, { x: 1, y: 1 }] };
  await h.root.onTweenPairCurveChanged(1, 4);
  assert.deepEqual(plain(h.sent()), [{ operation: REASSIGN, expectedRevision: 7, payload: { command: 'object.tween.reassign', layerUid: UID[1],
    span: { from: 4, to: 8, overrides: [], easing: [{ x: 0, y: 0 }, { x: 0.5, y: 0.9 }, { x: 1, y: 1 }] }, options: OPTIONS } }]);
  const last = await h.root.onTweenPairCurveChanged(1, 8);
  assert.deepEqual([last.status, last.code], ['unavailable', 'tween_span_unavailable']);
  assert.equal(h.sent().length, 1);
  assert.deepEqual(h.legacy, []);
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
