'use strict';
// #2372 insp-recolour: the Selected Colors panel (color-manager.js) on a native object project lists the colours the engine listed for the selected items
// and recolours them as ONE native restyle batch (one history entry), alpha kept, only the fills/strokes that equal the edited colour touched, never through
// pushUndo or a Paper item. Baseline: 1065e9d8 color-manager.js recolorColor (selection scope; a layer-wide recolour has no native equivalent yet).
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const Intent = require('../src/js/adapters/select-canvas-intent.js');
const Style = require('../src/js/adapters/native-selection-style.js');
const Steps = require('../src/js/adapters/native-selection-steps.js');
const Paper = require('./helpers/native-import-paper.cjs');
const overlayStub = require('./helpers/native-selection-overlay-stub.cjs');

const seg = (x, y) => ({ point: [x, y], handleIn: [0, 0], handleOut: [0, 0] });
const quad = (id, x0, y0, x1, y1, style, extra) => ({ stableTarget: { contextId: 'scene-root', layerUid: 'L', frameScope: { kind: 'authored', frame: 0 }, strokeId: id },
  closed: true, count: 4, role: 'plain', filled: true, bounds: [x0, y0, x1, y1], matrix: [1, 0, 0, 1, 0, 0], segments: [seg(x0, y0), seg(x1, y0), seg(x1, y1), seg(x0, y1)],
  style: { width: 4, opacity: 1, ...style }, ...extra });
const tick = () => new Promise((done) => setTimeout(done, 0));
const settle = async () => { for (let i = 0; i < 6; i++) await tick(); };
const COLOR_MANAGER = fs.readFileSync(path.join(__dirname, '..', 'src', 'js', 'color-manager.js'), 'utf8');

function harness(items, { native = true } = {}) {
  const paper = Paper.load(), toasts = [], batches = [], sent = [], timers = [], legacy = [], canvasBg = [];
  let pin = { instanceId: 'i', documentId: 'd', lifecycleGeneration: 1, contentRevision: 1, documentSnapshotId: 's1', canvasW: 200, canvasH: 200, layers: [{ layerUid: 'L' }] };
  const store = items.map((i) => JSON.parse(JSON.stringify(i)));
  const session = { info: () => pin, isActive: () => true, blocksLegacy: () => true, readPresentedObjectSession: () => ({ presentation: { ...pin, frame: 0 } }),
    async readPresentedStrokes(_s, _p, wanted) { return { items: store.map((item) => { const { segments, ...rest } = item; return wanted(rest) ? { ...rest, segments } : rest; }) }; } };
  const root = { paper, Path: paper.Path, Point: paper.Point, Matrix: paper.Matrix, state: { tool: 'select', appMode: 'anim2d', currentFrame: 0, activeLayerIdx: 0, layers: [{ layerUid: 'L' }], selectedStrokeIndices: [], canvasBg: '#222222' },
    selectedPaths: [], updateUI() {}, SMEngineBridge: { renderNow() {} }, showToast: (t) => toasts.push(t),
    setTimeout: (fn) => { timers.push(fn); return timers.length; }, clearTimeout: (id) => { timers[id - 1] = null; },
    pushUndo() { legacy.push('pushUndo'); throw new Error('native authority denied legacy history checkpoint'); },
    colorHex8: () => { throw new Error('a Paper colour was read'); },
    SM: { setCanvasBg: (c) => canvasBg.push(c) },
    document: { readyState: 'complete', getElementById: () => null },
    NemoNativeObjectProject: { desktop: () => session, selectLayerUI: () => true },
    NemoNativeObjectLabs: { desktop: () => native ? ({ batchStrokes: async (layerUid, frame, steps, options) => {
      sent.push(options);
      batches.push({ layerUid, frame, steps: JSON.parse(JSON.stringify(steps)) });
      steps.forEach((s) => { const it = store.find((i) => i.stableTarget.strokeId === s.restyle.stableTarget.strokeId); it.style = { ...it.style, ...s.restyle.style }; });
      pin = { ...pin, contentRevision: pin.contentRevision + 1, documentSnapshotId: 's' + (pin.contentRevision + 1) };
      return { status: 'applied' };
    } }) : null },
    NemoSelectCanvasIntent: Intent, NemoNativeSelectionSteps: Steps, NemoNativeSelectionStyle: Style, NemoNativeSelectOverlay: overlayStub(Intent) };
  root.window = root;
  vm.createContext(root);
  vm.runInContext(COLOR_MANAGER, root);
  return { root, toasts, batches, sent, store, legacy, canvasBg, panel: root.SMColorManager,
    async select(ids) { const layers = await Intent.layerPaths(root, (item) => ids.includes(item.stableTarget.strokeId), false, 0); root.selectedPaths = layers.paths; root.NemoNativeSelectOverlay.show(root); },
    fire: async () => { timers.splice(0).filter(Boolean).forEach((fn) => fn()); await settle(); },
    setPin: (n) => { pin = { ...pin, ...n }; } };
}

const SCENE = () => [
  quad('A', 10, 10, 30, 30, { fill: '#112233', stroke: '#445566' }),
  quad('B', 40, 40, 60, 60, { fill: '#112233' }),
  quad('C', 70, 70, 90, 90, { fill: '#778899', stroke: '#112233' }),
  quad('D', 100, 100, 120, 120, { fill: '#778899cc', stroke: '#445566' }),
  quad('R', 130, 130, 150, 150, { fill: '#112233' }, { role: 'ribbon' }),
  quad('T', 10, 150, 60, 170, { fill: '#112233' }, { role: 'text' }),
  quad('I', 70, 150, 120, 170, { opacity: 1 }, { role: 'raster' }),
];

test('the panel lists the fill and stroke colours the engine listed for the selected native items, most used first', async () => {
  const h = harness(SCENE());
  await h.select(['A', 'B', 'C', 'D', 'R', 'T', 'I']);
  assert.deepEqual(h.panel.computeUsedColors(), [
    { hex: '#112233', count: 5 }, // A fill, B fill, C stroke, the ribbon's ink, the text colour
    { hex: '#445566', count: 2 }, { hex: '#778899', count: 1 }, { hex: '#778899cc', count: 1 }]);
  await h.select(['B']);
  assert.deepEqual(h.panel.computeUsedColors(), [{ hex: '#112233', count: 1 }]);
});

test('a recolour is ONE native restyle batch: only the matching fills and strokes change, the alpha byte is kept, nothing reaches pushUndo or Paper', async () => {
  const h = harness(SCENE());
  await h.select(['A', 'B', 'C', 'D', 'R', 'T', 'I']);
  h.panel.recolorColor('#112233', '#ff000080');
  await settle();
  assert.equal(h.batches.length, 0, 'nothing is written while the colour is still being picked');
  await h.fire();
  assert.equal(h.batches.length, 1, 'one batch, one history entry, for the whole selection');
  assert.deepEqual(h.legacy, []);
  assert.deepEqual(h.sent[0], { skipUnavailable: true, select: true });
  assert.deepEqual(h.batches[0].steps.map((s) => [s.restyle.stableTarget.strokeId, s.restyle.style]), [
    ['A', { fill: '#ff000080' }], ['B', { fill: '#ff000080' }], ['C', { stroke: '#ff000080' }], ['R', { fill: '#ff000080' }], ['T', { fill: '#ff000080' }]]);
  assert.equal(h.store.find((i) => i.stableTarget.strokeId === 'A').style.stroke, '#445566', 'the other colours of a matching item are untouched');
  assert.deepEqual(h.panel.computeUsedColors().map((c) => c.hex).sort(), ['#445566', '#778899', '#778899cc', '#ff000080'], 'the panel shows the stored colours after the re-read');
});

test('a picker drag or an opacity scrub coalesces into one batch carrying the last colour; an opaque target drops no alpha it was given', async () => {
  const h = harness(SCENE());
  await h.select(['A', 'D']);
  h.panel.recolorColor('#778899cc', '#00ff0040');
  h.panel.recolorColor('#778899cc', '#00ff0099');
  h.panel.recolorColor('#778899cc', '#00ff00');
  await h.fire();
  assert.equal(h.batches.length, 1);
  assert.deepEqual(h.batches[0].steps.map((s) => [s.restyle.stableTarget.strokeId, s.restyle.style]), [['D', { fill: '#00ff00' }]]);
  h.panel.recolorColor('#00ff00', '#00ff00cc');
  await h.fire();
  assert.deepEqual(h.batches[1].steps.map((s) => s.restyle.style), [{ fill: '#00ff00cc' }]);
});

test('a selection that changed before the commit recolours nothing and says so', async () => {
  const h = harness(SCENE());
  await h.select(['A', 'B']);
  h.panel.recolorColor('#112233', '#ff0000');
  h.setPin({ contentRevision: 9, documentSnapshotId: 'other' });
  await h.fire();
  assert.equal(h.batches.length, 0);
  assert.match(h.toasts.join('|'), /selection changed/);
});

test('with nothing selected a layer-wide recolour says unavailable and writes nothing; the canvas background row still goes through the project setting', async () => {
  const h = harness(SCENE());
  assert.deepEqual(JSON.parse(JSON.stringify(h.panel.computeUsedColors())), [{ hex: '#222222', count: 1 }], 'only the colour a native project can still change is listed');
  h.panel.recolorColor('#112233', '#ff0000');
  await h.fire();
  assert.equal(h.batches.length, 0);
  assert.deepEqual(h.legacy, []);
  assert.match(h.toasts.join('|'), /recolour.*not available on native projects/);
  h.panel.recolorColor('#222222', '#333333');
  assert.deepEqual(h.canvasBg, ['#333333']);
});

test('a recolour that matches nothing sends no batch, and a legacy (non-native) project keeps the Paper path', async () => {
  const h = harness(SCENE());
  await h.select(['D']);
  h.panel.recolorColor('#112233', '#ff0000');
  await h.fire();
  assert.equal(h.batches.length, 0);
  const legacy = harness(SCENE(), { native: false });
  assert.throws(() => legacy.panel.recolorColor('#112233', '#ff0000'), /legacy history checkpoint/, 'the legacy writer is still the legacy path off native');
});
