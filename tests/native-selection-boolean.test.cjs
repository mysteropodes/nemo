'use strict';
// #2372 insp-boolean-ops: on a native object project the Unite / Subtract / Intersect / Exclude buttons and the Alt-click combine send ONE
// native request through the existing group intent (object.boolean.apply / object.group.combine) instead of the legacy booleanOp whose
// history checkpoint a native project refuses (that threw). The real timeline.js wireBoolBtn and tools.js booleanOp run in a sandbox; the
// real group intent, component-intent ports, edit contract and select-canvas-intent run against a stub session client.
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const Intent = require('../src/js/adapters/select-canvas-intent.js');
const Overlay = require('../src/js/adapters/native-select-overlay.js');
const Gesture = require('../src/js/adapters/native-select-gesture.js');
const Boolean_ = require('../src/js/adapters/native-selection-boolean.js');
const Paper = require('./helpers/native-import-paper.cjs');

const read = (f) => fs.readFileSync(path.join(__dirname, '..', 'src', 'js', f), 'utf8');
const TIMELINE = read('timeline.js'), TOOLS = read('tools.js'), GROUP = read('group-bridge.js');
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
const SAME_LAYER = [quad('L1', 'a', 10, 10, 40, 40), quad('L1', 'b', 30, 30, 70, 70)];
const TWO_LAYERS = [quad('L1', 'a', 10, 10, 40, 40), quad('L2', 'b', 30, 30, 70, 70)];

// opts: native (blocksLegacy), route (the session offers the resource route), adapter (the module is loaded), answer(request) -> the response to give.
function harness(items = SAME_LAYER, opts = {}) {
  const { native = true, route = true, adapter = true } = opts;
  const paper = Paper.load(), body = el('body'), toasts = [], sent = [], legacy = [], calls = [], gate = { hold: null };
  let pin = { instanceId: 'i', documentId: 'd', lifecycleGeneration: 1, contentRevision: 7, documentSnapshotId: 's7', canvasW: 200, canvasH: 200, layers: LAYERS };
  const store = items.map((i) => JSON.parse(JSON.stringify(i)));
  let ids = 0;
  const session = { info: () => pin, isActive: () => true, blocksLegacy: () => native, requestId: (label) => label + '-' + (++ids),
    readPresentedObjectSession: () => ({ presentation: { ...pin, frame: 0 } }),
    async readPresentedStrokes(selector, _pin, wanted) {
      return { items: store.filter((i) => i.stableTarget.layerUid === selector.layerUid).map((item) => { const { segments, ...rest } = item; return wanted(rest) ? { ...rest, segments } : rest; }) };
    } };
  if (route) {
    session.runResourceCommand = async (action) => action({
      async dispatch(request) {
        sent.push(JSON.parse(JSON.stringify(request)));
        if (gate.hold) await gate.hold;
        if (opts.answer) return opts.answer(request, { pin: () => pin, bump: () => { pin = { ...pin, contentRevision: pin.contentRevision + 1, documentSnapshotId: 's' + (pin.contentRevision + 1) }; return pin.contentRevision; } });
        // the owner: a boolean replaces its operands by the result in the style source's (last operand's) id; a combine only groups.
        if (request.operation.endsWith('boolean.apply')) {
          const { strokeIds } = request.payload, last = strokeIds[strokeIds.length - 1];
          const parts = strokeIds.map((id) => store.find((i) => i.stableTarget.strokeId === id));
          const x0 = Math.min(...parts.map((i) => i.bounds[0])), y0 = Math.min(...parts.map((i) => i.bounds[1])), x1 = Math.max(...parts.map((i) => i.bounds[2])), y1 = Math.max(...parts.map((i) => i.bounds[3]));
          strokeIds.forEach((id) => store.splice(store.findIndex((i) => i.stableTarget.strokeId === id), 1));
          store.push(quad(request.payload.layerUid, last, x0, y0, x1, y1));
        }
        pin = { ...pin, contentRevision: pin.contentRevision + 1, documentSnapshotId: 's' + (pin.contentRevision + 1) };
        return { apiVersion: 2, requestId: request.requestId, instanceId: request.instanceId, documentId: request.documentId, contentRevision: pin.contentRevision, ok: true, result: { applied: true, layerUid: request.payload.layerUid } };
      },
      async reconcile() { calls.push('reconcile'); },
    });
  }
  const root = { paper, Path: paper.Path, Point: paper.Point, Matrix: paper.Matrix, state: { tool: 'select', appMode: 'anim2d', currentFrame: 0, activeLayerIdx: 0, layers: LAYERS, selectedStrokeIndices: [] },
    selectedPaths: [], updateUI() {}, SMEngineBridge: { renderNow() {} }, showToast: (t) => toasts.push(t),
    SM: { t: (key) => ({ toastSelectAtLeast2Shapes: 'Select at least 2 shapes', toastSelectAtLeast2ShapesToCombine: 'Select at least 2 shapes to combine', toastBooleanOpApplied: 'Boolean operation applied', toastCombined: 'Combined' }[key] || key) },
    setTimeout: (fn) => fn, clearTimeout() {},
    document: { body, getElementById: () => ({ getBoundingClientRect: () => ({ left: 0, top: 0, width: 200, height: 200 }) }), createElementNS: (_n, tag) => el(tag), addEventListener() {}, removeEventListener() {} },
    requestAnimationFrame: () => {}, pushUndo() { legacy.push('pushUndo'); throw new Error('native authority denied legacy history checkpoint'); },
    NemoNativeObjectProject: { desktop: () => session, selectLayerUI: () => true },
    NemoNativeViewportUnderlay: { canvasMapping: { point: (e) => ({ x: e.clientX, y: e.clientY }), transform: () => [1, 0, 0, 1, 0, 0] } },
    NemoSelectCanvasIntent: Intent, NemoNativeSelectOverlay: Overlay, NemoNativeSelectGesture: Gesture };
  if (adapter) root.NemoNativeSelectionBoolean = Boolean_;
  const intent = Intent.create({
    context: () => JSON.stringify([root.state.tool, root.state.activeLayerIdx, (root.selectedPaths || []).map((p) => p.data.strokeId)]),
    probe: (e) => Object.freeze({ mode: 'pick', x: e.clientX, y: e.clientY }),
    select: (d, shift) => Intent.resolvePick(root, Intent, d, shift, Intent.pickContext(root), false),
    gesture: Gesture.bind(root) });
  const target = { setPointerCapture() {}, releasePointerCapture() {}, hasPointerCapture: () => true };
  const ev = (x, y) => ({ button: 0, pointerId: 1, buttons: 1, clientX: x, clientY: y, shiftKey: false, altKey: false, currentTarget: target, stopImmediatePropagation() {}, preventDefault() {} });
  const h = { root, toasts, sent, legacy, calls, store, gate, session,
    async select() { intent.down(ev(2, 2)); intent.move(ev(190, 190)); await settle(); await intent.up({ ...ev(190, 190), buttons: 0 }); await settle(); },
    ids: () => root.selectedPaths.map((p) => p.data.strokeId) };
  // the real tools.js booleanOp and timeline.js wireBoolBtn, wired to fake buttons and legacy stand-ins that record being reached
  const listeners = {}, legacyCalls = [];
  const document = { getElementById: (id) => ({ addEventListener: (type, fn) => { listeners[id + ':' + type] = fn; } }) };
  root.SM.booleanOp = (mode) => legacyCalls.push('SM.booleanOp:' + mode);
  root.SMGroup = { combineSelection: (mode) => legacyCalls.push('SMGroup.combineSelection:' + mode) };
  const sandbox = { window: root, document, Object, Array, console, selectedPaths: [], SM: root.SM, SMGroup: root.SMGroup, showToast: (t) => toasts.push(t),
    pushUndo: () => { legacy.push('pushUndo'); throw new Error('legacy history checkpoint refused'); } };
  Object.defineProperty(sandbox, 'selectedPaths', { get: () => root.selectedPaths });
  vm.createContext(sandbox);
  vm.runInContext(extract(TIMELINE, 'wireBoolBtn') + "\n;['unite','subtract','intersect','exclude'].forEach(function(m){wireBoolBtn('btn-bool-'+m,m);});\n" + extract(TOOLS, 'booleanOp') + '\nthis.booleanOp = booleanOp;', sandbox);
  vm.runInContext(extract(GROUP, 'combineSelection').replace('function combineSelection', 'function combineSelectionNative') + '\nthis.combineSelection = combineSelectionNative;', sandbox);
  h.legacyCalls = legacyCalls; h.sandbox = sandbox;
  h.click = async (mode, altKey = false) => { listeners['btn-bool-' + mode + ':click']({ altKey }); await settle(); };
  return h;
}

for (const mode of ['unite', 'subtract', 'intersect', 'exclude']) {
  test(`${mode}: the button is ONE object.boolean.apply request in selection order, never the legacy writer`, async () => {
    const h = harness();
    await h.select();
    assert.deepEqual(h.ids(), ['a', 'b']);
    await h.click(mode);
    assert.equal(h.sent.length, 1, 'one request is one revision and one history entry');
    const [r] = h.sent;
    assert.equal(r.operation, 'command.document.object.boolean.apply');
    assert.equal(r.expectedRevision, 7, 'pinned to the presented revision');
    assert.deepEqual(r.payload, { command: 'object.boolean.apply', mode, layerUid: 'L1', contextId: 'scene-root', frame: 0, strokeIds: ['a', 'b'] });
    assert.deepEqual(h.legacy, [], 'no legacy history checkpoint');
    assert.deepEqual(h.legacyCalls, [], 'no legacy booleanOp / combine');
    assert.deepEqual(h.toasts, ['Boolean operation applied']);
    assert.deepEqual(h.ids(), ['b'], 'the result is selected again: the first island keeps the style source (last operand) id');
    assert.deepEqual(h.root.selectedPaths[0].bounds && [h.root.selectedPaths[0].bounds.x, h.root.selectedPaths[0].bounds.y, h.root.selectedPaths[0].bounds.width, h.root.selectedPaths[0].bounds.height], [10, 10, 60, 60]);
  });
}

test('the operands keep the selection order: the LAST selected is the style source', async () => {
  const h = harness();
  await h.select();
  h.root.selectedPaths.reverse();
  await h.click('subtract');
  assert.deepEqual(h.sent[0].payload.strokeIds, ['b', 'a']);
  assert.deepEqual(h.ids(), ['a'], 'a is the style source, so the result keeps its id');
});

test('Alt-click is ONE object.group.combine request: a fresh group id, the mode, the members; the selection stays', async () => {
  for (const mode of ['unite', 'subtract', 'intersect', 'exclude']) {
    const h = harness();
    await h.select();
    await h.click(mode, true);
    assert.equal(h.sent.length, 1, mode);
    const [r] = h.sent;
    assert.equal(r.operation, 'command.document.object.group.combine');
    assert.match(r.payload.groupId, /^grp_[a-z0-9]+_\d+$/);
    assert.deepEqual({ ...r.payload, groupId: 'G' }, { command: 'object.group.combine', layerUid: 'L1', contextId: 'scene-root', groupId: 'G', mode, strokeIds: ['a', 'b'] });
    assert.deepEqual(h.legacy, []);
    assert.deepEqual(h.legacyCalls, []);
    assert.deepEqual(h.toasts, ['Combined']);
    assert.deepEqual(h.ids(), ['a', 'b']);
  }
});

test('the other callers of the legacy writers (SMGroup.combineSelection, SM.booleanOp) route to the owner too and never throw', async () => {
  const h = harness();
  await h.select();
  h.sandbox.combineSelection('exclude');
  await settle();
  assert.equal(h.sent.length, 1);
  assert.equal(h.sent[0].operation, 'command.document.object.group.combine');
  h.sandbox.booleanOp('unite');
  await settle();
  assert.equal(h.sent.length, 2);
  assert.equal(h.sent[1].operation, 'command.document.object.boolean.apply');
  assert.deepEqual(h.legacy, []);
});

test('two quick clicks are two serialized requests: the second reads the selection the first left', async () => {
  const h = harness([quad('L1', 'a', 10, 10, 40, 40), quad('L1', 'b', 30, 30, 70, 70)]);
  await h.select();
  let release; h.gate.hold = new Promise((done) => { release = done; });
  const first = h.click('unite');
  await settle();
  const second = h.click('subtract');
  await settle();
  assert.equal(h.sent.length, 1, 'the second waits for the first');
  h.gate.hold = null; release();
  await first; await second; await settle();
  assert.equal(h.sent.length, 1, 'after the first one only one shape is left, so the second has nothing to combine');
  assert.deepEqual(h.toasts.slice(-1), ['Select at least 2 shapes']);
});

test('a refusal by the owner (curved, unfilled, locked, animated) is a notice, not a throw, and changes nothing', async () => {
  const h = harness(SAME_LAYER, { answer: (request) => ({ apiVersion: 2, requestId: request.requestId, instanceId: request.instanceId, documentId: request.documentId, contentRevision: 7, ok: false, error: { code: 'unavailable', message: 'curved operand' } }) });
  await h.select();
  await h.click('unite');
  assert.equal(h.sent.length, 1);
  assert.match(h.toasts.join('|'), /not available for these shapes/);
  assert.deepEqual(h.ids(), ['a', 'b'], 'the selection is untouched');
  assert.deepEqual(h.legacy, []);
});

test('a stale revision is reported, not retried', async () => {
  const h = harness(SAME_LAYER, { answer: (request) => ({ apiVersion: 2, requestId: request.requestId, instanceId: request.instanceId, documentId: request.documentId, contentRevision: 8, ok: false, error: { code: 'stale_revision', message: 'moved' } }) });
  await h.select();
  await h.click('intersect');
  assert.equal(h.sent.length, 1);
  assert.deepEqual(h.toasts, ['The document changed — nothing was changed.']);
});

test('without the native owner route the click is a notice and a return - it never throws and never reaches the legacy writer', async () => {
  const noRoute = harness(SAME_LAYER, { route: false });
  await noRoute.select();
  await noRoute.click('unite');
  await noRoute.click('unite', true);
  assert.equal(noRoute.sent.length, 0);
  assert.equal(noRoute.toasts.length, 2);
  assert.match(noRoute.toasts[0], /not available/);
  assert.deepEqual(noRoute.legacy, []);
  assert.deepEqual(noRoute.legacyCalls, []);
  // the adapter script absent altogether: booleanOp itself answers with the notice
  const absent = harness(SAME_LAYER, { adapter: false });
  await absent.select();
  absent.sandbox.booleanOp('unite');
  assert.deepEqual(absent.toasts, ['Boolean operations are not available on native projects right now.']);
  assert.deepEqual(absent.legacy, []);
});

test('fewer than two shapes, shapes of two layers, images and a selection that moved on send nothing', async () => {
  const one = harness();
  await one.click('unite');
  assert.deepEqual(one.toasts, ['Select at least 2 shapes']);
  await one.click('unite', true);
  assert.equal(one.toasts[1], 'Select at least 2 shapes to combine');
  const layers = harness(TWO_LAYERS);
  await layers.select();
  assert.deepEqual(layers.ids().sort(), ['a', 'b']);
  await layers.click('unite');
  assert.deepEqual(layers.toasts, ['Select shapes on one layer to combine them.']);
  const image = harness();
  await image.select();
  image.root.selectedPaths[0].data.nativeRole = 'raster';
  await image.click('unite');
  assert.match(image.toasts[0], /need vector shapes/);
  const gone = harness();
  await gone.select();
  gone.store.splice(0, 1);
  await gone.click('unite');
  assert.deepEqual(gone.toasts, ['The selection changed — nothing was changed.']);
  for (const h of [one, layers, image, gone]) { assert.equal(h.sent.length, 0); assert.deepEqual(h.legacy, []); }
});

test('a legacy project keeps the legacy writers byte for byte', async () => {
  const h = harness(SAME_LAYER, { native: false });
  await h.click('unite');
  await h.click('exclude', true);
  assert.deepEqual(h.legacyCalls, ['SM.booleanOp:unite', 'SMGroup.combineSelection:exclude']);
  assert.equal(h.sent.length, 0);
  assert.equal(Boolean_.native(h.root), false);
});

test('the module stays inside the adapter size contract and writes no Paper / WASM boolean', () => {
  const src = read('adapters/native-selection-boolean.js');
  assert.ok(src.split('\n').filter((l) => l.trim()).length <= 500);
  assert.doesNotMatch(src.replace(/\/\*[\s\S]*?\*\//, ''), /foldBooleanOp|booleanOpWasm|insertBooleanResult|pushUndo|GeometryWasm/);
});
