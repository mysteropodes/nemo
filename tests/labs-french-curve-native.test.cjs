'use strict';
// U33 (#2362): under native authority the Labs French curve commits its guided stroke through the ordinary native
// vector create with the copy key (the baseline ensureKeyframe()), never through the legacy writers; the snapped,
// simplified geometry and the baseline paint reach the dictionary. The registry reports a prototype's draw hooks as
// unmigrated unless it declares a native route.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const paper = require('./helpers/native-import-paper.cjs').load();
const Registry = require('../src/js/adapters/labs-registry-model.js');
const poisoned = () => { throw Error('legacy writer reached'); };

function harness() {
  const listeners = {}, created = [], store = new Map();
  const area = { contains: (node) => node === target };
  const target = { tagName: 'CANVAS' };
  const state = { layers: [{ locked: false }], activeLayerIdx: 0, currentFrame: 3, strokeEnabled: true, strokeColor: '#112233',
    brushSize: 4, strokeCap: 'round', strokeJoin: 'miter', fillEnabled: false, fillColor: '#ffffff', opacity: 50, smoothing: 1 };
  const root = { state, console, Math, JSON, Path: paper.Path, Point: paper.Point, Segment: paper.Segment, SM: { t: (k) => k }, showToast() {},
    localStorage: { getItem: (k) => (store.has(k) ? store.get(k) : null), setItem: (k, v) => store.set(k, String(v)) },
    document: { getElementById: (id) => (id === 'canvas-area' ? area : null),
      addEventListener(kind, fn) { (listeners[kind] = listeners[kind] || []).push(fn); } },
    addEventListener() {}, SMEngineBridge: { screenToWorld: (x, y) => [x, y], renderNow() {}, renderWithOverlayItem() {} },
    SMLabs: { isOn: (n) => n === 'french-curve', register() {} },
    NemoNativeObjectLabs: { desktop: () => ({ layerUid: (i) => ['native-layer:1:0'][i] || null,
      createStroke: (...args) => { created.push(args); return Promise.resolve({ status: 'applied' }); } }) },
    pushUndo: poisoned, ensureKeyframe: poisoned, saveActiveLayerFrame: poisoned, userLayers: new Proxy({}, { get: poisoned }) };
  root.window = root;
  vm.createContext(root);
  vm.runInContext(fs.readFileSync(path.join(__dirname, '..', 'src/js/labs/french-curve.js'), 'utf8'), root, { filename: 'french-curve.js' });
  const fire = (kind, event) => { for (const fn of listeners[kind] || []) fn(Object.assign({ target, stopPropagation() {}, stopImmediatePropagation() {}, preventDefault() {} }, event)); };
  return { root, fire, created };
}

test('a guided French-curve stroke commits once through the native create with the copy key and the baseline paint', async () => {
  const h = harness();
  h.root.SMLabs.setCurveGuide('line', { x1: 0, y1: 0, x2: 200, y2: 0 });
  h.fire('keydown', { key: 'f' });
  h.fire('pointerdown', { clientX: 10, clientY: 5 });
  for (const [x, y] of [[40, 8], [80, -3], [120, 6], [160, -2]]) h.fire('pointermove', { clientX: x, clientY: y });
  h.fire('pointerup', { clientX: 160, clientY: -2 });
  await new Promise(setImmediate);
  assert.equal(h.created.length, 1);
  const [layerUid, frame, dict, opacity] = h.created[0];
  assert.deepEqual([layerUid, frame, opacity], ['native-layer:1:0', 3, 0.5]);
  assert.deepEqual([dict.strokeColor, dict.strokeWidth, dict.strokeCap, dict.strokeJoin, dict.fillColor, dict.closed], ['#112233', 4, 'round', 'miter', null, false]);
  assert.ok(dict.segments.length >= 2);
  for (const s of dict.segments) assert.ok(Math.abs(s.point[1]) < 1e-9, 'every anchor lies on the guide line');
  assert.deepEqual([dict.segments[0].point[0], dict.segments.at(-1).point[0]].map(Math.round), [10, 160]);
});

test('the registry reports enabled draw hooks without a native route, and only those', () => {
  const flags = new Map();
  const model = Registry.create({ getItem: (k) => (flags.has(k) ? flags.get(k) : null), setItem: (k, v) => flags.set(k, v), warn() {}, toast() {}, describe: (k) => k });
  model.register('predictive-stroke', { flag: 'p', onStroke() {}, nativeDraw: true });
  model.register('multiframe-draw', { flag: 'm', onStroke() {} });
  model.register('preview-aid', { flag: 'v', onPreview() {} });
  model.register('canvas-grid', { flag: 'g' });
  assert.deepEqual(model.unmigratedDrawHooks(), []);
  for (const name of ['predictive-stroke', 'canvas-grid']) model.enable(name);
  assert.deepEqual(model.unmigratedDrawHooks(), [], 'a migrated hook and a view aid leave native Drawing available');
  model.enable('multiframe-draw'); model.enable('preview-aid');
  assert.deepEqual(model.unmigratedDrawHooks(), ['multiframe-draw', 'preview-aid']);
});
