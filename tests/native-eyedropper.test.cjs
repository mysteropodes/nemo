'use strict';
// #2372 EYEDROPPER: on a native object project the Eyedropper reads the engine's hit-test listing (the Select gesture's read) instead of the empty Paper layer,
// feeds the listed paint to the baseline controls, writes nothing and never falls back to Paper. Baseline: 1065e9d8 viewtools-bridge.js pick().
const test = require('node:test'), assert = require('node:assert/strict'), fs = require('node:fs'), vm = require('node:vm');
const Eye = require('../src/js/adapters/native-eyedropper.js');
const Intent = require('../src/js/adapters/select-canvas-intent.js');
const Wait = require('../src/js/adapters/native-presentation-wait.js');
const Paper = require('./helpers/native-import-paper.cjs');

const hit = (role, style, extra) => ({ data: Object.assign({ nativeRole: role, nativeStyle: style }, extra) });

test('a flat path gives its stroke and fill colours and its stroke width', () => {
  assert.deepEqual(Eye.describe(hit('plain', { fill: '#ff0000', stroke: '#00ff0080', width: 6, opacity: 1 })),
    { stroke: '#00ff0080', fill: '#ff0000', width: 6, preset: 'none' }, 'alpha is kept');
  assert.deepEqual(Eye.describe(hit('shape', { fill: '#112233', width: 4 })), { stroke: null, fill: '#112233', width: null, preset: 'none' }, 'an unstroked record has no width to pick');
  assert.deepEqual(Eye.describe(hit('plain', { stroke: '#445566', width: 3, strokeEnabled: true, fillEnabled: false, fill: '#abcdef' })),
    { stroke: '#445566', fill: null, width: 3, preset: 'none' }, 'a disabled fill is not picked');
});

test('a ribbon gives its fill as the stroke colour and its listed width as the size', () => {
  const ribbon = Eye.describe(hit('ribbon', { fill: '#336699cc', width: 12, opacity: 1 }, { nativeRibbon: true }));
  assert.deepEqual(ribbon, { stroke: '#336699cc', fill: null, width: 12, preset: 'none' });
  assert.deepEqual(Eye.describe(hit('plain', { fill: '#336699', width: 9 }, { isVectorBrush: true })), { stroke: '#336699', fill: null, width: 9, preset: 'none' });
});

test('text gives its colours; a bitmap brush without a width keeps the size; a listed preset is carried', () => {
  assert.deepEqual(Eye.describe(hit('text', { fill: '#101010' })), { stroke: null, fill: '#101010', width: null, preset: 'none' });
  assert.deepEqual(Eye.describe(hit('brush', { stroke: '#fa0000', brushPreset: 'chalk' })), { stroke: '#fa0000', fill: null, width: null, preset: 'chalk' });
  assert.deepEqual(Eye.describe(hit('plain', { stroke: '#fa0000', width: 0 })), { stroke: '#fa0000', fill: null, width: null, preset: 'none' });
});

test('an item that lists no paint, an image, or junk colours has nothing to pick', () => {
  assert.equal(Eye.describe(hit('raster', undefined)), null);
  assert.equal(Eye.describe(hit('plain', { fill: 'red', stroke: '#12' })), null);
  assert.equal(Eye.describe(null), null);
});

// Driven: the real viewtools-bridge.js, the real hit-test read (select-canvas-intent.pick) and wait, on a fake native session.
const seg = (x, y) => ({ point: [x, y], handleIn: [0, 0], handleOut: [0, 0] });
const quad = (layer, id, x0, y0, x1, y1, style) => ({ stableTarget: { contextId: 'scene-root', layerUid: layer, frameScope: { kind: 'authored', frame: 0 }, strokeId: id },
  closed: true, count: 4, role: 'plain', filled: true, bounds: [x0, y0, x1, y1], matrix: [1, 0, 0, 1, 0, 0], segments: [seg(x0, y0), seg(x1, y0), seg(x1, y1), seg(x0, y1)], style });
const tick = () => new Promise((done) => setTimeout(done, 0));
const settle = async () => { for (let i = 0; i < 12; i++) await tick(); };
function fixture({ items, native = true, busy = 0, layers = [{ layerUid: 'L1' }] }) {
  const paper = Paper.load(), handlers = {}, toasts = [], dom = {}, calls = { writes: 0, reads: 0, renders: 0, presets: [] };
  const pin = { instanceId: 'i', documentId: 'd', lifecycleGeneration: 1, contentRevision: 7, documentSnapshotId: 's7', canvasW: 200, canvasH: 200, layers };
  let pending = busy;
  const session = { info: () => pin, isActive: () => true, blocksLegacy: () => native,
    readPresentedObjectSession: (refuse) => { if (pending > 0) { pending--; if (refuse) refuse('queue-pending'); return null; } return { presentation: { ...pin, frame: 0 } }; },
    async readPresentedStrokes(selector, _pin, wanted) { calls.reads++;
      return { items: items.filter((i) => i.stableTarget.layerUid === selector.layerUid).map((item) => { const { segments, ...rest } = item; return wanted(rest) ? { ...rest, segments } : rest; }) }; },
    dispatch() { calls.writes++; }, run() { calls.writes++; } };
  const el = (id) => dom[id] || (dom[id] = { id, value: '', dataset: {}, style: {}, classList: { remove() {} }, checked: false });
  const area = { addEventListener(type, fn) { handlers[type] = fn; } };
  const layerStub = { hitTest() { throw new Error('the Paper layer must never be consulted'); } };
  const root = { paper, Path: paper.Path, Point: paper.Point, Matrix: paper.Matrix, readyState: 'complete',
    state: { tool: 'eyedropper', appMode: 'anim2d', currentFrame: 0, activeLayerIdx: 0, layers, playing: false, spaceDown: false, altDown: false, brushSize: 3, strokeColor: '#000000', fillColor: '#ffffff', fillEnabled: false },
    userLayers: [layerStub], view: { zoom: 1 }, showToast: (t) => toasts.push(t), setHex8Input: (e, h) => { if (e) { e.value = h.length > 7 ? h.slice(0, 7) : h; e.dataset.hex8 = h; } },
    SMEngineBridge: { isEnabled: () => false, renderNow() { calls.renders++; }, suspend() {}, resume() {} }, addEventListener() {},
    BrushPresetPicker: { selectPreset: (k) => calls.presets.push(k) },
    NemoNativeObjectProject: { desktop: () => session }, NemoSelectCanvasIntent: Intent, NemoNativePresentationWait: Wait, NemoNativeEyedropper: Eye,
    NemoNativeViewportUnderlay: { canvasMapping: { point: (e) => ({ x: e.clientX, y: e.clientY }) } },
    document: { readyState: 'complete', addEventListener() {}, getElementById: (id) => id === 'canvas-area' ? area : id === 'drawing-canvas' ? { getBoundingClientRect: () => ({ left: 0, top: 0, width: 200, height: 200 }) } : el(id) } };
  root.window = root; const context = vm.createContext(root);
  vm.runInContext('var state=window.state,view=window.view,userLayers=window.userLayers,Point=window.Point,showToast=window.showToast,setHex8Input=window.setHex8Input;', context);
  vm.runInContext(fs.readFileSync(require.resolve('../src/js/viewtools-bridge.js'), 'utf8'), context, { filename: 'viewtools-bridge.js' });
  const down = (x, y, extra) => { const e = { type: 'pointerdown', clientX: x, clientY: y, button: 0, pointerId: 1, stopped: false, prevented: false,
    stopImmediatePropagation() { this.stopped = true; }, preventDefault() { this.prevented = true; }, ...extra }; handlers.pointerdown(e); return e; };
  return { root, toasts, dom, calls, down, pin, handlers };
}
const RED = quad('L1', 'a', 10, 10, 50, 50, { fill: '#ff0000', stroke: '#0000ff80', width: 5, opacity: 1 });

test('driven: a click on a red-filled shape fills the controls, consumes the press and writes nothing', async () => {
  const f = fixture({ items: [RED] });
  const e = f.down(30, 30); await settle();
  assert(e.stopped && e.prevented, 'the press is consumed');
  assert.equal(f.root.state.fillColor, '#ff0000'); assert.equal(f.dom['color-fill'].dataset.hex8, '#ff0000'); assert.equal(f.root.state.fillEnabled, true);
  assert.equal(f.root.state.strokeColor, '#0000ff80', 'alpha kept'); assert.equal(f.dom['color-stroke'].dataset.hex8, '#0000ff80'); assert.equal(f.dom['color-stroke'].value, '#0000ff');
  assert.equal(f.root.state.brushSize, 5); assert.equal(f.dom['p-sw'].value, 5);
  assert.deepEqual(f.calls.presets, ['none']); assert.deepEqual(f.toasts, ['Color picked']);
  assert.equal(f.calls.writes, 0, 'no command'); assert.equal(f.pin.contentRevision, 7, 'no revision bump');
});

test('driven: the queue being busy is waited out, not reported as nothing', async () => {
  const f = fixture({ items: [RED], busy: 3 });
  f.down(30, 30); for (let i = 0; i < 40 && !f.toasts.length; i++) await new Promise((d) => setTimeout(d, 20));
  assert.deepEqual(f.toasts, ['Color picked']); assert.equal(f.root.state.fillColor, '#ff0000');
});

test('driven: a miss says so, changes nothing and never consults the Paper layer', async () => {
  const f = fixture({ items: [RED] });
  f.down(150, 150); await settle();
  assert.deepEqual(f.toasts, ['Nothing to pick here']); assert.equal(f.root.state.fillColor, '#ffffff'); assert.equal(f.root.state.brushSize, 3); assert.deepEqual(f.calls.presets, []);
});

test('driven: an unready session says so (no presentation, never a Paper fallback)', async () => {
  const f = fixture({ items: [RED], busy: 1e9 });
  f.down(30, 30); for (let i = 0; i < 80 && !f.toasts.length; i++) await new Promise((d) => setTimeout(d, 20));
  assert.deepEqual(f.toasts, ['Nothing to pick here']); assert.equal(f.root.state.fillColor, '#ffffff');
});

test('driven: moving the pointer over a native project does not hit-test Paper', () => {
  const f = fixture({ items: [RED] }); f.root.SMEngineBridge.isEnabled = () => true; // engine on: the legacy hover would reach hitPickable (Paper layer)
  f.handlers.pointermove({ type: 'pointermove', clientX: 30, clientY: 30, stopImmediatePropagation() {}, preventDefault() {} });
});

test('legacy (non-native) projects keep the Paper hit-test path, which this adapter never serves', () => {
  const f = fixture({ items: [RED], native: false });
  f.root.SMEngineBridge.screenToWorld = (x, y) => [x, y]; f.root.SMEngineBridge.isEnabled = () => true;
  assert.throws(() => f.down(30, 30), /Paper layer must never/, 'the legacy branch still reads the Paper layer');
});
