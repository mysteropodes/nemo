'use strict';
// #2372 insp-transform-fields: on a native object project the inspector's X / Y / W / H / rotation fields and the arrow-key nudge commit the
// Select selection as ONE native object.vector.batch of record-space reshapes (one revision, one history entry), through the real
// timeline.js listeners, instead of the legacy selPropsApply* whose history checkpoint a native project refuses (that threw silently).
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const Intent = require('../src/js/adapters/select-canvas-intent.js');
const Overlay = require('../src/js/adapters/native-select-overlay.js');
const Gesture = require('../src/js/adapters/native-select-gesture.js');
const Transform = require('../src/js/adapters/native-selection-transform.js');
const Steps = require('../src/js/adapters/native-selection-steps.js');
const Paper = require('./helpers/native-import-paper.cjs');

const TIMELINE = fs.readFileSync(path.join(__dirname, '..', 'src', 'js', 'timeline.js'), 'utf8');
const TOOLS = fs.readFileSync(path.join(__dirname, '..', 'src', 'js', 'tools.js'), 'utf8');
function extract(src, name) {
  const from = src.indexOf('function ' + name + '(');
  assert.ok(from >= 0, name);
  let depth = 0;
  for (let i = src.indexOf('{', from); i < src.length; i++) { if (src[i] === '{') depth++; else if (src[i] === '}' && --depth === 0) return src.slice(from, i + 1); }
  throw new Error('unterminated ' + name);
}
function el(tag) {
  return { tag, attrs: {}, style: {}, children: [], setAttribute(k, v) { this.attrs[k] = String(v); },
    appendChild(c) { this.children.push(c); c.parentNode = this; return c; },
    remove() { if (this.parentNode) this.parentNode.children = this.parentNode.children.filter((c) => c !== this); } };
}
const seg = (x, y) => ({ point: [x, y], handleIn: [0, 0], handleOut: [0, 0] });
const quad = (layer, id, x0, y0, x1, y1, extra) => ({ stableTarget: { contextId: 'scene-root', layerUid: layer, frameScope: { kind: 'authored', frame: 0 }, strokeId: id },
  closed: true, count: 4, role: 'plain', filled: true, bounds: [x0, y0, x1, y1], matrix: [1, 0, 0, 1, 0, 0], segments: [seg(x0, y0), seg(x1, y0), seg(x1, y1), seg(x0, y1)],
  style: { fill: '#112233', stroke: '#445566', width: 4, opacity: 1 }, ...extra });
const tick = () => new Promise((done) => setTimeout(done, 0));
const settle = async () => { for (let i = 0; i < 12; i++) await tick(); };
const LAYERS = [{ layerUid: 'L1' }, { layerUid: 'L2' }];
const ITEMS = [quad('L1', 'a', 10, 10, 30, 30), quad('L2', 'b', 50, 10, 70, 30)];

function harness(items = ITEMS, { native = true } = {}) {
  const paper = Paper.load(), body = el('body'), toasts = [], batches = [], timers = [], legacy = [];
  let pin = { instanceId: 'i', documentId: 'd', lifecycleGeneration: 1, contentRevision: 1, documentSnapshotId: 's1', canvasW: 200, canvasH: 200, layers: LAYERS };
  const store = items.map((i) => JSON.parse(JSON.stringify(i))), gate = { hold: null };
  const session = { info: () => pin, isActive: () => true, blocksLegacy: () => native, readPresentedObjectSession: () => ({ presentation: { ...pin, frame: 0 } }),
    async readPresentedStrokes(selector, _pin, wanted) {
      return { items: store.filter((i) => i.stableTarget.layerUid === selector.layerUid).map((item) => { const { segments, ...rest } = item; return wanted(rest) ? { ...rest, segments } : rest; }) };
    } };
  const root = { paper, Path: paper.Path, Point: paper.Point, Matrix: paper.Matrix, state: { tool: 'select', appMode: 'anim2d', currentFrame: 0, activeLayerIdx: 0, layers: LAYERS, selectedStrokeIndices: [], selRotAccum: 0, xformAnchorKey: 'mc' },
    selectedPaths: [], updateUI() {}, SMEngineBridge: { renderNow() {} }, showToast: (t) => toasts.push(t),
    setTimeout: (fn) => { timers.push(fn); return timers.length; }, clearTimeout: (id) => { timers[id - 1] = null; },
    document: { body, getElementById: () => ({ getBoundingClientRect: () => ({ left: 0, top: 0, width: 200, height: 200 }) }), createElementNS: (_n, tag) => el(tag),
      addEventListener() {}, removeEventListener() {} },
    requestAnimationFrame: () => {}, pushUndo() { legacy.push('pushUndo'); throw new Error('native authority denied legacy history checkpoint'); },
    NemoNativeObjectProject: { desktop: () => session, selectLayerUI: () => true },
    NemoNativeViewportUnderlay: { canvasMapping: { point: (e) => ({ x: e.clientX, y: e.clientY }), transform: () => [1, 0, 0, 1, 0, 0] } },
    NemoNativeObjectLabs: { desktop: () => ({ batchStrokes: async (layerUid, frame, steps) => {
      batches.push({ layerUid, frame, steps: JSON.parse(JSON.stringify(steps)) });
      if (gate.hold) await gate.hold;
      steps.forEach((s) => {
        const it = store.find((i) => i.stableTarget.strokeId === s.reshape.stableTarget.strokeId), m = s.reshape.matrix, at = (q) => [m[0] * q[0] + m[2] * q[1] + m[4], m[1] * q[0] + m[3] * q[1] + m[5]];
        it.segments = it.segments.map((q) => ({ point: at(q.point), handleIn: q.handleIn, handleOut: q.handleOut }));
        const xs = it.segments.map((q) => q.point[0]), ys = it.segments.map((q) => q.point[1]);
        it.bounds = [Math.min(...xs), Math.min(...ys), Math.max(...xs), Math.max(...ys)];
      });
      pin = { ...pin, contentRevision: pin.contentRevision + 1, documentSnapshotId: 's' + (pin.contentRevision + 1) };
      return { status: 'applied' };
    } }) },
    NemoSelectCanvasIntent: Intent, NemoNativeSelectOverlay: Overlay, NemoNativeSelectionSteps: Steps, NemoNativeSelectGesture: Gesture, NemoNativeSelectionTransform: Transform };
  const intent = Intent.create({
    context: () => JSON.stringify([root.state.tool, root.state.activeLayerIdx, (root.selectedPaths || []).map((p) => p.data.strokeId)]),
    probe: (e) => Object.freeze({ mode: 'pick', x: e.clientX, y: e.clientY }),
    select: (d, shift) => Intent.resolvePick(root, Intent, d, shift, Intent.pickContext(root), false),
    gesture: Gesture.bind(root) });
  const target = { setPointerCapture() {}, releasePointerCapture() {}, hasPointerCapture: () => true };
  const ev = (x, y) => ({ button: 0, pointerId: 1, buttons: 1, clientX: x, clientY: y, shiftKey: false, altKey: false, currentTarget: target, stopImmediatePropagation() {}, preventDefault() {} });
  const h = { root, toasts, batches, store, timers, legacy, gate,
    async select() { intent.down(ev(2, 2)); intent.move(ev(190, 190)); await settle(); await intent.up({ ...ev(190, 190), buttons: 0 }); await settle(); },
    ids: () => root.selectedPaths.map((p) => p.data.strokeId),
    fire: async () => { timers.splice(0).filter(Boolean).forEach((fn) => fn()); await settle(); } };
  // the real timeline.js code, in a sandbox that stands in for the globals it reads
  const fields = {}, listeners = {};
  ['sp-x', 'sp-y', 'sp-w', 'sp-h', 'sp-rot'].forEach((id) => { fields[id] = { id, value: '', dataset: {}, addEventListener(type, fn) { (listeners[id + ':' + type] = listeners[id + ':' + type] || []).push(fn); } }; });
  const document = { getElementById: (id) => fields[id] || null };
  const sandbox = { window: root, document, Object, Array, Math, Number, parseFloat, console, state: root.state, Point: paper.Point, legacy: [], vertexMode: false,
    activeXformVertexMode: () => sandbox.vertexMode, _spSizeLockOn: false, _nativeTyped: {} };
  Object.defineProperty(sandbox, 'selectedPaths', { get: () => root.selectedPaths });
  vm.createContext(sandbox);
  const code = [extract(TOOLS, 'xformSelBounds'), extract(TOOLS, 'xformAnchorPoint'), TOOLS.match(/var XFORM_ANCHOR_PROP=\{[^}]*\};/)[0],
    TIMELINE.match(/var NATIVE_XFORM_FIELDS=\{[^}]*\};/)[0],
    ...['nativeTypedInput', 'nativeScrubChange', 'wireLiveXformField', 'markNativeXformFields', 'nativeXformOwned', 'nativeXformRead', 'nativeXformEdit'].map((n) => extract(TIMELINE, n)),
    "['sp-x','sp-y','sp-w','sp-h','sp-rot'].forEach(function(id){wireLiveXformField(id,function(){legacy.push(id);});});"].join('\n');
  vm.runInContext(code, sandbox);
  h.fields = fields; h.sandbox = sandbox; h.legacyFields = sandbox.legacy;
  // typing: an input event per keystroke; blur: a change
  h.type = (id, value) => { fields[id].value = String(value); (listeners[id + ':input'] || []).forEach((fn) => fn.call(fields[id], { type: 'input' })); };
  h.blur = (id) => (listeners[id + ':change'] || []).forEach((fn) => fn.call(fields[id], { type: 'change' }));
  h.mark = () => vm.runInContext('markNativeXformFields()', sandbox);
  // the arrow-key branch of onKeyDown, as written in timeline.js
  const src = TIMELINE.slice(TIMELINE.indexOf("else if(selectedPaths.length&&(k==='ArrowLeft'"), TIMELINE.indexOf("selPropsApplyMove(ndx,ndy);\n  }", TIMELINE.indexOf("else if(selectedPaths.length&&(k==='ArrowLeft'")) + "selPropsApplyMove(ndx,ndy);\n  }".length);
  const key = vm.runInNewContext('(function(event,k,selectedPaths,state,window,SMMotion,pushUndo,selPropsApplyMove,legacy){' + src.replace(/^else /, '') + '})', { Object, NemoNativeSelectionTransform: Transform });
  h.key = (k, extra = {}) => {
    const ev2 = { shiftKey: false, repeat: false, preventDefault() {}, ...extra };
    key(ev2, k, root.selectedPaths, root.state, root, undefined, () => { throw new Error('legacy pushUndo'); }, () => h.legacy.push('selPropsApplyMove'), []);
  };
  return h;
}
const matrices = (h, at = 0) => h.batches[at].steps.map((s) => s.reshape.matrix.map((n) => Math.round(n * 1e6) / 1e6));

test('typing a value into X is ONE native batch over a two-layer selection, however many keystrokes it took', async () => {
  const h = harness();
  await h.select();
  assert.deepEqual(h.ids(), ['a', 'b']);
  h.type('sp-x', '4'); h.type('sp-x', '40');
  await h.fire();
  assert.equal(h.batches.length, 1, 'one batch, one history entry');
  assert.deepEqual(h.batches[0].steps.map((s) => s.reshape.stableTarget.layerUid), ['L1', 'L2']);
  assert.deepEqual(matrices(h), [[1, 0, 0, 1, 30, 0], [1, 0, 0, 1, 30, 0]], 'union bounds x = 10, so the selection moves by 30');
  assert.deepEqual(h.root.selectedPaths.map((p) => [p.bounds.x, p.bounds.y]), [[40, 10], [80, 10]], 'the selection is read again at the new geometry');
  assert.deepEqual(h.legacy, [], 'the legacy history checkpoint was never reached');
  assert.deepEqual(h.legacyFields, [], 'nor the legacy field writers');
  assert.deepEqual(h.toasts, []);
  h.blur('sp-x'); await h.fire();
  assert.equal(h.batches.length, 1, 'the blur of a typed edit does not commit it twice');
});

test('Y, W and H follow the baseline matrices: translate by the difference, scale about the top-left corner, lock = same factor on both axes', async () => {
  const h = harness();
  await h.select();
  h.type('sp-y', '25'); await h.fire();
  assert.deepEqual(matrices(h, 0)[0], [1, 0, 0, 1, 0, 15], 'union top = 10');
  // now the union is x 10..70, y 25..45 (60 x 20)
  h.type('sp-w', '120'); await h.fire();
  assert.deepEqual(matrices(h, 1)[0], [2, 0, 0, 1, -10, 0], 'width x2 about the top-left (10, 25) = x*2 - 10, height kept');
  h.type('sp-h', '40'); await h.fire();
  assert.deepEqual(matrices(h, 2)[0], [1, 0, 0, 2, 0, -25], 'height x2 about the top-left');
  h.sandbox._spSizeLockOn = true;
  h.type('sp-w', '60'); await h.fire();
  const m = matrices(h, 3)[0];
  assert.equal(m[0], m[3], 'locked: both axes take the same factor');
  assert.equal(Number(h.fields['sp-h'].value), 20, 'and the partner field follows at once (120 x 40 halved)');
  assert.equal(h.batches.length, 4);
});

test('the rotation field turns about the anchor point by the difference to the accumulated angle, and keeps that angle for the next edit', async () => {
  const h = harness();
  await h.select();
  h.type('sp-rot', '90'); await h.fire();
  assert.equal(h.batches.length, 1);
  const [a, b, c, d, e, f] = matrices(h)[0];
  assert.deepEqual([a, b, c, d], [0, 1, -1, 0]);
  assert.deepEqual([e, f], [60, -20], 'about the centre (40, 20): e = cx + cy, f = cy - cx');
  assert.equal(h.root.state.selRotAccum, 90);
  h.type('sp-rot', '100'); await h.fire();
  const turn = Math.atan2(matrices(h, 1)[0][1], matrices(h, 1)[0][0]) * 180 / Math.PI;
  assert.equal(Math.round(turn), 10, 'only the 10 degrees that were added');
  assert.equal(h.root.state.selRotAccum, 100);
});

test('a move, resize or nudge after a turn keeps the accumulated angle, so the same angle typed again turns nothing', async () => {
  const h = harness();
  await h.select();
  // the real panel (updateSelPropsPanel) resets the angle when the selected items' ids change, as every native commit re-reads them
  let sig = h.root.selectedPaths.map((p) => p.id).join();
  h.root.updateUI = () => { const now = h.root.selectedPaths.map((p) => p.id).join(); if (now !== sig) { sig = now; h.root.state.selRotAccum = 0; } };
  h.type('sp-rot', '30'); await h.fire();
  assert.equal(h.root.state.selRotAccum, 30);
  h.key('ArrowRight'); await settle();
  assert.equal(h.batches.length, 2);
  assert.equal(h.root.state.selRotAccum, 30, 'the nudge keeps the angle');
  h.type('sp-x', '25'); await h.fire();
  assert.equal(h.root.state.selRotAccum, 30, 'so does a typed move');
  h.type('sp-rot', '30'); await h.fire();
  assert.equal(h.batches.length, 3, 'the same angle again is no turn');
});

test('an edit of another field commits the pending one first; the batches are written in order', async () => {
  const h = harness();
  await h.select();
  h.type('sp-x', '20'); h.type('sp-y', '30');
  await h.fire(); await h.fire();
  assert.equal(h.batches.length, 2);
  assert.deepEqual(matrices(h, 0)[0], [1, 0, 0, 1, 10, 0]);
  assert.deepEqual(matrices(h, 1)[0], [1, 0, 0, 1, 0, 20]);
});

test('a scrub of a field is ONE change at the release: the fields are marked static, and a typed edit is not applied again by its change', async () => {
  const h = harness();
  h.mark();
  assert.equal(h.fields['sp-x'].dataset.nativeOpacityScrub, undefined, 'nothing selected: left to the legacy scrub');
  await h.select();
  h.mark();
  assert.equal(['sp-x', 'sp-y', 'sp-w', 'sp-h', 'sp-rot'].every((id) => h.fields[id].dataset.nativeOpacityScrub === 'static'), true);
  h.fields['sp-x'].value = '45'; h.blur('sp-x'); await h.fire(); // the one change a static scrub ends with
  assert.equal(h.batches.length, 1);
  assert.deepEqual(matrices(h)[0], [1, 0, 0, 1, 35, 0]);
  h.root.selectedPaths = []; h.mark();
  assert.equal(h.fields['sp-x'].dataset.nativeOpacityScrub, undefined, 'released when the selection is gone');
});

test('the arrow keys are one batch per press: 1 unit, 10 with Shift; a held key sums the repeats that arrive while a write is in flight', async () => {
  const h = harness();
  await h.select();
  h.key('ArrowRight'); await settle();
  h.key('ArrowDown', { shiftKey: true }); await settle();
  h.key('ArrowLeft'); await settle();
  h.key('ArrowUp'); await settle();
  assert.equal(h.batches.length, 4, 'one batch per press');
  assert.deepEqual([0, 1, 2, 3].map((n) => matrices(h, n)[0].slice(4)), [[1, 0], [0, 10], [-1, 0], [0, -1]]);
  assert.deepEqual(h.batches[0].steps.map((s) => s.reshape.stableTarget.layerUid), ['L1', 'L2'], 'both layers in the one batch');
  assert.deepEqual(h.legacy, []);
  let release; h.gate.hold = new Promise((done) => { release = done; });
  h.key('ArrowRight'); await settle();
  h.key('ArrowRight', { repeat: true }); h.key('ArrowRight', { repeat: true }); h.key('ArrowRight', { repeat: true });
  await settle();
  assert.equal(h.batches.length, 5, 'the repeats wait for the write in flight');
  h.gate.hold = null; release(); await settle(); await settle();
  assert.equal(h.batches.length, 6, 'and are written as one batch');
  assert.deepEqual(matrices(h, 5)[0].slice(4), [3, 0]);
  assert.deepEqual(h.toasts, []);
});

test('a refusal is a notice and never a throw: Direct Select, Motion, picked points, an unsupported item, no route', async () => {
  const h = harness();
  await h.select();
  h.root.state.tool = 'subselect';
  assert.doesNotThrow(() => h.type('sp-x', '5'));
  assert.doesNotThrow(() => h.key('ArrowRight'));
  await h.fire();
  assert.match(h.toasts.join('|'), /Select tool/);
  h.root.state.tool = 'select'; h.root.state.appMode = 'motion';
  assert.doesNotThrow(() => h.key('ArrowRight'));
  assert.match(h.toasts.join('|'), /Motion/);
  h.root.state.appMode = 'anim2d'; h.sandbox.vertexMode = true;
  assert.doesNotThrow(() => h.type('sp-w', '50'));
  assert.match(h.toasts.join('|'), /points/);
  assert.equal(h.batches.length, 0, 'nothing was written');
  assert.deepEqual(h.legacyFields.concat(h.legacy), [], 'and no legacy writer ran (it would have thrown at the history checkpoint)');
});

test('an item with no native transform is refused with the gesture notice and nothing is written', async () => {
  const h = harness([quad('L1', 'a', 10, 10, 30, 30, { role: 'brush', brush: 'bitmap', group: 'g' }), ITEMS[1]]);
  await h.select();
  assert.ok(h.root.selectedPaths.length >= 1);
  h.key('ArrowRight'); await settle();
  assert.equal(h.batches.length, 0);
  assert.match(h.toasts.join('|'), /bitmap brushes is not available on native projects yet/);
});

test('a selection that outlived a context change is dropped with a notice, never moved', async () => {
  const h = harness();
  await h.select();
  h.root.state.currentFrame = 3; // the frame moved on under the stale selection
  h.key('ArrowRight'); await settle();
  assert.equal(h.batches.length, 0);
  assert.match(h.toasts.join('|'), /no longer applies/);
  assert.deepEqual(h.root.selectedPaths, []);
});

test('a document that changed between the typing and the commit writes nothing', async () => {
  const h = harness();
  await h.select();
  h.type('sp-x', '40');
  h.root.state.currentFrame = 2;
  await h.fire();
  assert.equal(h.batches.length, 0);
  assert.match(h.toasts.join('|'), /changed/);
});

test('on a project that is not native the fields and the nudge keep the legacy writers', async () => {
  const h = harness(ITEMS, { native: false });
  h.root.selectedPaths = [{ data: {}, bounds: { x: 0 } }];
  h.type('sp-x', '5');
  assert.deepEqual(h.legacyFields, ['sp-x']);
  h.key('ArrowRight');
  assert.deepEqual(h.legacy, ['selPropsApplyMove']);
  assert.deepEqual(h.toasts, []);
});
