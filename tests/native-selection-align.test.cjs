'use strict';
// #2372 insp-align-distribute: on a native object project the align toolbar's six align buttons and two distribute buttons move the Select selection as
// ONE native object.vector.batch (one revision, one history entry), through the real tools.js alignSelection / distributeSelection entry points, instead of
// the legacy Paper writers (whose history checkpoint a native project refuses) that returned without a word. The expected translations are the ones the
// baseline (1065e9d8 tools.js alignSelection / distributeSelection) produces for the same three rectangles; they were taken by running that code.
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const Intent = require('../src/js/adapters/select-canvas-intent.js');
const Overlay = require('../src/js/adapters/native-select-overlay.js');
const Gesture = require('../src/js/adapters/native-select-gesture.js');
const Align = require('../src/js/adapters/native-selection-align.js');
const Steps = require('../src/js/adapters/native-selection-steps.js');
const Paper = require('./helpers/native-import-paper.cjs');

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
// a: 20 x 20, b: 40 x 40 (another layer), c: 10 x 5 - the union is x 10..140, y 10..60
const ITEMS = [quad('L1', 'a', 10, 10, 30, 30), quad('L2', 'b', 50, 20, 90, 60), quad('L1', 'c', 130, 40, 140, 45)];
// per item (a, b, c): [dx, dy] of the baseline
const BASELINE = {
  left: [[0, 0], [-40, 0], [-120, 0]], right: [[110, 0], [50, 0], [0, 0]], centerH: [[55, 0], [5, 0], [-60, 0]],
  top: [[0, 0], [0, -10], [0, -30]], bottom: [[0, 30], [0, 0], [0, 15]], centerV: [[0, 15], [0, -5], [0, -7.5]],
  horizontal: [[0, 0], [10, 0], [0, 0]], vertical: [[0, 0], [0, -5], [0, 0]] };

function harness(items = ITEMS, { native = true, tool = 'select' } = {}) {
  const paper = Paper.load(), body = el('body'), toasts = [], batches = [], legacy = [];
  let pin = { instanceId: 'i', documentId: 'd', lifecycleGeneration: 1, contentRevision: 1, documentSnapshotId: 's1', canvasW: 200, canvasH: 200, layers: LAYERS };
  const store = items.map((i) => JSON.parse(JSON.stringify(i)));
  const session = { info: () => pin, isActive: () => true, blocksLegacy: () => native, readPresentedObjectSession: () => ({ presentation: { ...pin, frame: 0 } }),
    async readPresentedStrokes(selector, _pin, wanted) {
      return { items: store.filter((i) => i.stableTarget.layerUid === selector.layerUid).map((item) => { const { segments, ...rest } = item; return wanted(rest) ? { ...rest, segments } : rest; }) };
    } };
  const root = { paper, Path: paper.Path, Point: paper.Point, Matrix: paper.Matrix, state: { tool, appMode: 'anim2d', currentFrame: 0, activeLayerIdx: 0, layers: LAYERS, selectedStrokeIndices: [], selRotAccum: 0, xformAnchorKey: 'mc' },
    selectedPaths: [], updateUI() {}, SMEngineBridge: { renderNow() {} }, showToast: (t) => toasts.push(t),
    setTimeout: (fn) => { fn(); return 1; }, clearTimeout() {},
    document: { body, getElementById: () => ({ getBoundingClientRect: () => ({ left: 0, top: 0, width: 200, height: 200 }) }), createElementNS: (_n, tag) => el(tag),
      addEventListener() {}, removeEventListener() {} },
    requestAnimationFrame: () => {}, pushUndo() { legacy.push('pushUndo'); throw new Error('native authority denied legacy history checkpoint'); },
    NemoNativeObjectProject: { desktop: () => session, selectLayerUI: () => true },
    NemoNativeViewportUnderlay: { canvasMapping: { point: (e) => ({ x: e.clientX, y: e.clientY }), transform: () => [1, 0, 0, 1, 0, 0] } },
    NemoNativeObjectLabs: { desktop: () => ({ batchStrokes: async (layerUid, frame, steps) => {
      batches.push({ layerUid, frame, steps: JSON.parse(JSON.stringify(steps)) });
      steps.forEach((s) => {
        const it = store.find((i) => i.stableTarget.strokeId === s.reshape.stableTarget.strokeId), m = s.reshape.matrix, at = (q) => [m[0] * q[0] + m[2] * q[1] + m[4], m[1] * q[0] + m[3] * q[1] + m[5]];
        it.segments = it.segments.map((q) => ({ point: at(q.point), handleIn: q.handleIn, handleOut: q.handleOut }));
        const xs = it.segments.map((q) => q.point[0]), ys = it.segments.map((q) => q.point[1]);
        it.bounds = [Math.min(...xs), Math.min(...ys), Math.max(...xs), Math.max(...ys)];
      });
      pin = { ...pin, contentRevision: pin.contentRevision + 1, documentSnapshotId: 's' + (pin.contentRevision + 1) };
      return { status: 'applied' };
    } }) },
    NemoSelectCanvasIntent: Intent, NemoNativeSelectOverlay: Overlay, NemoNativeSelectionSteps: Steps, NemoNativeSelectGesture: Gesture, NemoNativeSelectionAlign: Align };
  const intent = Intent.create({
    context: () => JSON.stringify([root.state.tool, root.state.activeLayerIdx, (root.selectedPaths || []).map((p) => p.data.strokeId)]),
    probe: (e) => Object.freeze({ mode: 'pick', x: e.clientX, y: e.clientY }),
    select: (d, shift) => Intent.resolvePick(root, Intent, d, shift, Intent.pickContext(root), false),
    gesture: Gesture.bind(root) });
  const target = { setPointerCapture() {}, releasePointerCapture() {}, hasPointerCapture: () => true };
  const ev = (x, y) => ({ button: 0, pointerId: 1, buttons: 1, clientX: x, clientY: y, shiftKey: false, altKey: false, currentTarget: target, stopImmediatePropagation() {}, preventDefault() {} });
  const h = { root, toasts, batches, store, legacy,
    async select() { intent.down(ev(2, 2)); intent.move(ev(190, 190)); await settle(); await intent.up({ ...ev(190, 190), buttons: 0 }); await settle(); },
    ids: () => root.selectedPaths.map((p) => p.data.strokeId) };
  // the real tools.js entry points, in a sandbox that stands in for the globals they read
  const sandbox = { window: root, Object, Math, Number, console, state: root.state, Point: paper.Point,
    pushUndo: root.pushUndo, fillRegenerateLinked() { legacy.push('fillRegenerateLinked'); }, saveActiveLayerFrameOrPromote() { legacy.push('save'); }, renderArcs() {}, updateUI() {},
    transformFillGradient() {}, userLayers: [{}] };
  Object.defineProperty(sandbox, 'selectedPaths', { get: () => root.selectedPaths });
  vm.createContext(sandbox);
  vm.runInContext(['allowLegacySelectionEdit', 'xformSelBounds', 'alignSelection', 'distributeSelection'].map((n) => extract(TOOLS, n)).join('\n'), sandbox);
  h.align = async (mode) => { vm.runInContext('alignSelection(' + JSON.stringify(mode) + ')', sandbox); await settle(); };
  h.distribute = async (mode) => { vm.runInContext('distributeSelection(' + JSON.stringify(mode) + ')', sandbox); await settle(); };
  return h;
}
const matrixOf = (batch, id) => { const s = batch.steps.find((x) => x.reshape.stableTarget.strokeId === id); return s ? s.reshape.matrix : null; };
const translations = (h) => ['a', 'b', 'c'].map((id) => { const m = matrixOf(h.batches[0], id); return m ? [m[4], m[5]] : [0, 0]; });

for (const mode of ['left', 'right', 'centerH', 'top', 'bottom', 'centerV']) {
  test('align ' + mode + ' moves each of three items on two layers by the baseline translation, as ONE native batch', async () => {
    const h = harness();
    await h.select();
    assert.deepEqual(h.ids().sort(), ['a', 'b', 'c']);
    await h.align(mode);
    assert.equal(h.batches.length, 1, 'one batch, one history entry');
    assert.deepEqual(translations(h), BASELINE[mode]);
    for (const s of h.batches[0].steps) assert.deepEqual(s.reshape.matrix.slice(0, 4), [1, 0, 0, 1], 'a pure translation');
    const moved = BASELINE[mode].filter((d) => d[0] || d[1]).length;
    assert.equal(h.batches[0].steps.length, moved, 'an item that stays is not written');
    assert.deepEqual(h.legacy, [], 'the legacy history checkpoint and writers were never reached');
    assert.deepEqual(h.toasts, []);
    assert.deepEqual(h.ids().sort(), ['a', 'b', 'c'], 'the selection is read again, whole');
  });
}

for (const mode of ['horizontal', 'vertical']) {
  test('distribute ' + mode + ' spaces the middle item with equal gaps like the baseline, in ONE batch, first and last untouched', async () => {
    const h = harness();
    await h.select();
    await h.distribute(mode);
    assert.equal(h.batches.length, 1);
    assert.deepEqual(translations(h), BASELINE[mode]);
    assert.deepEqual(h.batches[0].steps.map((s) => s.reshape.stableTarget.strokeId), ['b']);
    assert.deepEqual(h.legacy, []);
    assert.deepEqual(h.toasts, []);
  });
}

test('the pure translations equal the baseline for every mode, with the items in any selection order', () => {
  const boxes = ITEMS.map((i) => ({ left: i.bounds[0], top: i.bounds[1], right: i.bounds[2], bottom: i.bounds[3] }));
  for (const m of ['left', 'right', 'centerH', 'top', 'bottom', 'centerV']) assert.deepEqual(Align.alignDeltas(boxes, m).map((d) => d || [0, 0]), BASELINE[m], m);
  const shuffled = [boxes[2], boxes[0], boxes[1]];
  assert.deepEqual(Align.distributeDeltas(shuffled, 'horizontal').map((d) => d || [0, 0]), [[0, 0], [0, 0], [10, 0]], 'sorted by left edge, not selection order');
});

test('fewer than two (align) or three (distribute) items does nothing, silently, as in the baseline', async () => {
  const h = harness(ITEMS.slice(0, 2));
  await h.select();
  await h.distribute('horizontal');
  assert.equal(h.batches.length, 0);
  const one = harness(ITEMS.slice(0, 1));
  await one.select();
  await one.align('left');
  assert.equal(one.batches.length, 0);
  assert.deepEqual([...h.toasts, ...one.toasts, ...h.legacy, ...one.legacy], []);
});

test('a stroke id repeated on two layers keeps each item its own translation', async () => {
  // stroke ids are unique within a layer only; both items are 's' here
  const h = harness([quad('L1', 's', 10, 10, 30, 30), quad('L2', 's', 50, 20, 90, 60)]);
  await h.select();
  await h.align('left');
  assert.equal(h.batches.length, 1);
  const byLayer = Object.fromEntries(h.batches[0].steps.map((s) => [s.reshape.stableTarget.layerUid, s.reshape.matrix.slice(4)]));
  assert.deepEqual(byLayer, { L2: [-40, 0] }, 'only the L2 item moves, by its own delta; the L1 item already sits at the left edge');
});

test('an already aligned selection writes nothing', async () => {
  const h = harness([quad('L1', 'a', 10, 10, 30, 30), quad('L2', 'b', 10, 50, 40, 60)]);
  await h.select();
  await h.align('left');
  assert.equal(h.batches.length, 0);
  assert.deepEqual(h.toasts, []);
});

test('an item with no native transform refuses the whole gesture with the Select notice and writes nothing', async () => {
  const h = harness([...ITEMS.slice(0, 2), quad('L1', 'c', 130, 40, 140, 45, { role: 'brush', brush: 'bitmap', group: 'g' })]);
  await h.select();
  assert.equal(h.ids().length, 3);
  const before = JSON.stringify(h.store);
  await h.align('left');
  assert.equal(h.batches.length, 0);
  assert.equal(JSON.stringify(h.store), before, 'nothing moved');
  assert.equal(h.toasts.length, 1);
  assert.match(h.toasts[0], /bitmap brushes.*not available on native projects/);
  assert.deepEqual(h.legacy, []);
});

test('another tool, Motion mode or a missing route toasts and never throws, and never reaches the legacy writer', async () => {
  const h = harness();
  await h.select();
  h.root.state.tool = 'subselect';
  await h.align('left');
  h.root.state.tool = 'select'; h.root.state.appMode = 'motion';
  await h.distribute('horizontal');
  h.root.state.appMode = 'anim2d'; delete h.root.NemoNativeObjectLabs;
  await h.align('top');
  assert.equal(h.batches.length, 0);
  assert.equal(h.toasts.length, 3);
  assert.deepEqual(h.legacy, []);
});

test('a legacy project keeps the legacy writer (the adapter steps aside)', async () => {
  const h = harness(ITEMS, { native: false });
  await h.select().catch(() => {});
  assert.equal(Align.native(h.root), false);
  assert.equal(h.batches.length, 0);
});
