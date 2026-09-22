'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const path = require('node:path');
const { loadMotion } = require('./fixtures/lib/sandbox.cjs');
const MotionCanvasIntent = require('../src/js/adapters/motion-canvas-intent.js');
const SelectCanvasIntent = require('../src/js/adapters/select-canvas-intent.js');
const GUARD = require.resolve('../src/js/application/native-edit-guard.js');
const SELECT = path.join(__dirname, '../src/js/select-bridge.js');

class Point {
  constructor(x, y) { this.x = x; this.y = y; }
  clone() { return new Point(this.x, this.y); }
  add(p) { return new Point(this.x + p.x, this.y + p.y); }
  subtract(p) { return new Point(this.x - p.x, this.y - p.y); }
  multiply(n) { return new Point(this.x * n, this.y * n); }
  dot(p) { return this.x * p.x + this.y * p.y; }
  get length() { return Math.hypot(this.x, this.y); }
  normalize() { return this.multiply(1 / (this.length || 1)); }
  getDistance(p) { return Math.hypot(this.x - p.x, this.y - p.y); }
  rotate() { return this.clone(); }
}
function rectangle() {
  return { x: 0, y: 0, left: 0, top: 0, right: 200, bottom: 120, width: 200, height: 120,
    center: new Point(100, 60), contains(p) { return p.x >= 0 && p.x <= 200 && p.y >= 0 && p.y <= 120; },
    clone() { return rectangle(); } };
}
function fixture(options = {}) {
  const effects = [], events = {}, globalEvents = {}, captureEvents = [], projections = [];
  const owner = { native: options.native !== false, revision: 0, documentId: 'doc-1' };
  let settle;
  delete require.cache[GUARD];
  const guard = require(GUARD).install({
    getNativeIdentity: () => owner.native ? { documentId: owner.documentId, generation: 1 } : null,
    requestRelease(request) {
      effects.push(['release', request]);
      const receipt = { documentId: request.documentId, generation: request.generation, owner: 'legacy', status: 'released' };
      if (options.release === 'delayed') return new Promise((resolve) => { settle = () => { owner.native = false; resolve(receipt); }; });
      if (options.release === 'rejected') return Promise.reject(new Error('release rejected'));
      if (options.release === 'wrong') return { ...receipt, generation: 2 };
      if (options.release === 'sync') owner.native = false;
      return receipt;
    },
  });
  class Path {
    constructor() { effects.push(['paper']); this.data = {}; this.bounds = rectangle(); this.strokeBounds = this.bounds; }
  }
  Path.Rectangle = function () { return new Path(); };
  const item = Object.create(Path.prototype);
  Object.assign(item, { data: { strokeId: 'shape-1' }, bounds: rectangle(), strokeBounds: rectangle(), segments: [], removed: false });
  const layer = { children: [item], bounds: rectangle(), hitTest(p) { return item.bounds.contains(p) ? { item } : null; }, activate() {} };
  item.parent = item.layer = layer;
  const ld = { layerUid: 'layer-1', visible: true, locked: false, frames: [{ isKeyframe: true, strokes: [] }] };
  const state = { tool: 'select', appMode: options.mode || 'motion', currentFrame: 0, activeLayerIdx: 0,
    layers: [ld], selectedStrokeIndices: [0], canvasW: 320, canvasH: 180, totalFrames: 1, fps: 24, symbols: {} };
  const target = { addEventListener(type, callback) { events[type] = callback; },
    setPointerCapture(id) { this.captured = id; captureEvents.push(['capture', id]); },
    hasPointerCapture(id) { return this.captured === id; },
    releasePointerCapture(id) { delete this.captured; captureEvents.push(['release', id]); } };
  const { sandbox: scope, SMMotion: motion } = loadMotion(state, { beforeMotion(sb) {
    Object.assign(sb, { NemoMotionCanvasIntent: MotionCanvasIntent, NemoSelectCanvasIntent: SelectCanvasIntent, Point, Path, Raster: class Raster {}, view: { zoom: 1 }, selectedPaths: [item], userLayers: [layer],
      project: { layers: [layer], activeLayer: layer }, canvasEl: { style: {}, dataset: {} }, _layerSel: [0], _layerSelAnchor: 0, _compClick: {}, _marquee: {},
      clearSel() { sb.selectedPaths = []; state.selectedStrokeIndices = []; }, getSI(p) { return layer.children.indexOf(p); },
      isSelectablePathChild: () => true, resolveBrushAnchor: (p) => p, combineHitConfirm: () => true,
      arcHandles: [],
      orientedSelBox: () => ({ b: rectangle(), angle: 0, pivot: new Point(100, 60) }),
      selBoxPt: (x, y) => new Point(x, y), xformAnchorPoint: (b) => b.center,
      hitTestPosed: (_li, p) => layer.hitTest(p), hitTestComponentLayers: () => null, activateUL() {},
      renderArcs() {}, updateUI() {}, renderOS() {},
      pushUndo() { effects.push(['undo']); }, pushUndoLayers() { effects.push(['undo']); },
      saveAllLayerFrames() { effects.push(['save']); }, saveActiveLayerFrame() { effects.push(['save']); },
      ensureKeyframe() { effects.push(['promote']); }, loadFrame() { effects.push(['load']); },
      SMEngineBridge: { isEnabled: () => true, screenToWorld: (x, y) => [x, y], nativeEditGuard: guard,
        suspend() { effects.push(['suspend']); }, resume() { effects.push(['resume']); }, renderNow() {} },
      NemoNativeOpacityLegacyAdmission: guard,
      NemoNativeOpacityCutover: { blocksLegacy: () => owner.native, identity: () => owner.native
        ? { instanceId: 'instance-1', documentId: owner.documentId, contentRevision: owner.revision } : null,
      prepared: () => ({ layerUid: 'layer-1', opacityMode: 'static' }),
      projectSelection(descriptor, frame) {
        projections.push([descriptor, frame]);
        return { documentId: owner.documentId, contentRevision: owner.revision,
          selected: descriptor.selected.map((entry) => ({ stableTarget: { layerUid: entry.layerUid } })) };
      } },
      NemoNativeOpacityMotionSurface: { publishWriters: (api) => api, owns: () => false, read: () => ({ handled: false }), nativeKeyInteractionPlan: () => ({ handled: owner.native }),
        detachedElementView: (_item, seed) => seed(_item) },
    });
    sb.addEventListener = (type, callback) => { globalEvents[type] = callback; };
    sb.document.readyState = 'complete';
    sb.document.getElementById = () => target;
  } });
  // Motion's boot has no canvas dependency; Select installs the real capture callbacks.
  vm.runInNewContext(fs.readFileSync(SELECT, 'utf8').replace(/\}\)\(\);\s*$/, 'window.__pending = function () { return nativePointer().snapshot(); };\n})();'), scope, { filename: 'select-bridge.js' });
  function send(type, x = 100, y = 60, extra = {}) {
    const e = { type, currentTarget: target, pointerId: 1, button: 0, buttons: type === 'pointerup' ? 0 : 1, clientX: x, clientY: y,
      stopped: 0, prevented: 0, stopImmediatePropagation() { this.stopped++; }, preventDefault() { this.prevented++; }, ...extra };
    (events[type] || globalEvents[type])(e);
    return e;
  }
  return { scope, motion, state, item, layer, owner, effects, captureEvents, projections, send, settle: () => settle() };
}
const writes = (f) => f.effects.filter(([name]) => !['resume'].includes(name));

test('native click, reselect, deselect and Shift selection preserve identity and use the native selection query', () => {
  const f = fixture();
  const original = { ...f.owner };
  f.send('pointerdown', 300, 150); f.send('pointerup', 300, 150);
  assert.equal(f.scope.selectedPaths.length, 0);
  assert.deepEqual(Array.from(f.scope._layerSel), []);
  f.send('pointerdown', 100, 60); f.send('pointerup', 100, 60);
  assert.equal(f.scope.selectedPaths[0], f.item);
  assert.deepEqual(Array.from(f.scope._layerSel), [0]);
  f.send('pointerdown', 100, 60, { shiftKey: true }); f.send('pointerup', 100, 60, { shiftKey: true });
  assert.equal(f.scope.selectedPaths.length, 0);
  assert.deepEqual(Array.from(f.scope._layerSel), []);
  f.send('pointerdown', 100, 60, { shiftKey: true }); f.send('pointerup', 100, 60, { shiftKey: true });
  assert.equal(f.scope.selectedPaths[0], f.item);
  assert.ok(f.projections.length >= 3);
  assert.deepEqual(f.owner, original);
  assert.deepEqual(writes(f), []);
});

test('native pointer intent is scalar, frozen and has no gesture or writer effects', () => {
  const f = fixture();
  const down = f.send('pointerdown', 100, 60);
  assert.ok(down.stopped && down.prevented);
  const pending = f.scope.__pending();
  assert.equal(Object.isFrozen(pending), true);
  assert.equal(Object.isFrozen(pending.decision), true);
  assert.equal(pending.decision.strokeId, 'shape-1');
  assert.ok(!Object.values(pending).includes(f.item));
  assert.equal(f.motion.debugMotionDrag(), null);
  assert.deepEqual(writes(f), []);
  f.send('pointermove', 102, 61);
  f.send('pointerup', 102, 61);
  assert.deepEqual(writes(f), []);
  assert.deepEqual(f.captureEvents, [['capture', 1], ['release', 1]]);
});

test('unmigrated handle, body, marquee and lasso drags deny at threshold without release or replay', () => {
  for (const [x, y, altKey] of [[0, 0, false], [100, 60, false], [300, 150, false], [300, 150, true]]) {
    const f = fixture();
    f.send('pointerdown', x, y, { altKey });
    f.send('pointermove', x + 3, y, { altKey });
    assert.equal(f.scope.__pending().blocked, false);
    f.send('pointermove', x + 3.01, y, { altKey });
    assert.equal(f.scope.__pending().blocked, true);
    f.send('pointerup', x + 3.01, y, { altKey });
    assert.equal(f.scope.__pending(), null);
    assert.equal(f.motion.debugMotionDrag(), null);
    assert.deepEqual(writes(f), []);
    assert.equal(f.projections.length, 0);
  }
});

test('cancel, capture loss, blur, stale tool/frame/document and button loss discard the press', () => {
  const invalidations = [
    f => f.send('pointercancel'), f => f.send('lostpointercapture'), f => f.send('blur'),
    f => { f.state.currentFrame++; }, f => { f.state.tool = 'draw'; },
    f => { f.state.appMode = 'animation'; }, f => { f.state.layers = f.state.layers.slice(); },
    f => { f.owner.revision++; }, f => { f.owner.documentId = 'another'; },
    f => f.send('pointermove', 100, 60, { buttons: 0 }),
  ];
  for (const invalidate of invalidations) {
    const f = fixture();
    f.send('pointerdown', 100, 60); invalidate(f);
    f.send('pointermove', 100, 60);
    f.send('pointerup', 100, 60);
    assert.deepEqual(writes(f), []);
    assert.equal(f.projections.length, 0);
  }
});

test('unrelated pointer cannot commit or cancel a native press', () => {
  const f = fixture();
  f.send('pointerdown', 100, 60);
  f.send('pointermove', 110, 60, { pointerId: 2 });
  f.send('pointerup', 100, 60, { pointerId: 2 });
  assert.ok(f.scope.__pending());
  f.send('pointerup', 100, 60);
  assert.equal(f.scope.selectedPaths[0], f.item);
  assert.deepEqual(writes(f), []);
});

test('a denied drag remains denied after ownership changes and a fresh legacy press is separate', () => {
  const f = fixture();
  f.send('pointerdown', 0, 0); f.send('pointermove', 20, 0);
  f.owner.native = false;
  f.send('pointermove', 30, 0); f.send('pointerup', 30, 0);
  assert.deepEqual(writes(f), []);
  assert.equal(f.motion.debugMotionDrag(), null);
  f.send('pointerdown', 0, 0);
  assert.equal(f.motion.debugMotionDrag().mode, 'motionScale');
  assert.deepEqual(writes(f).map(([name]) => name), ['undo']);
  f.send('pointerup', 0, 0);
});

test('missing or malformed canvas adapters fail closed for native input', () => {
  for (const name of ['NemoMotionCanvasIntent', 'NemoSelectCanvasIntent']) {
    for (const value of [undefined, {}, { create() { throw new Error('broken'); } }]) {
      const f = fixture();
      f.scope[name] = value;
      const press = f.send('pointerdown', 0, 0);
      f.send('pointermove', 20, 0); f.send('pointerup', 20, 0);
      assert.ok(press.stopped);
      assert.equal(f.motion.debugMotionDrag(), null);
      assert.deepEqual(writes(f), []);
      assert.equal(f.projections.length, 0);
    }
  }
});

test('retained Motion callback and menu action deny without requesting release', () => {
  const f = fixture();
  assert.equal(f.motion.onDown({ point: new Point(0, 0) }), false);
  assert.equal(f.motion.onDrag({ point: new Point(20, 0) }), false);
  assert.equal(f.motion.onUp(), false);
  assert.ok(f.send('contextmenu', 100, 60).stopped);
  assert.deepEqual(writes(f), []);
});

test('malformed native selection projection cannot change UI selection', () => {
  const f = fixture();
  f.scope.NemoNativeOpacityCutover.projectSelection = () => ({ documentId: f.owner.documentId,
    contentRevision: f.owner.revision, selected: [{ stableTarget: { layerUid: 'wrong' } }] });
  f.send('pointerdown', 300, 150); f.send('pointerup', 300, 150);
  assert.equal(f.scope.selectedPaths[0], f.item);
  assert.deepEqual(writes(f), []);
});

test('Motion hit probe is detached, frozen and returns before gesture baselines', () => {
  const f = fixture(), before = JSON.stringify(f.state);
  let baselineReads = 0;
  f.scope.NemoMotionCanvasIntent = { create(readers, ...args) {
    const read = readers.valueAtFrame;
    readers.valueAtFrame = function (...valueArgs) { baselineReads++; return read(...valueArgs); };
    return MotionCanvasIntent.create(readers, ...args);
  } };
  const intent = f.motion.probeCanvasIntent({ point: new Point(0, 0) });
  assert.equal(intent.mode, 'motionScale');
  assert.equal(Object.isFrozen(intent), true);
  assert.equal(Object.isFrozen(intent.targets[0]), true);
  assert.equal(baselineReads, 0);
  assert.equal(JSON.stringify(f.state), before);
  assert.equal(f.motion.debugMotionDrag(), null);
  assert.deepEqual(writes(f), []);
});

test('classic adapter registrations precede their consumers and publish frozen APIs', () => {
  const html = fs.readFileSync(path.join(__dirname, '../src/index.html'), 'utf8');
  for (const [name, globalName, api, consumer] of [
    ['motion', 'NemoMotionCanvasIntent', MotionCanvasIntent, 'motion'],
    ['select', 'NemoSelectCanvasIntent', SelectCanvasIntent, 'select-bridge'],
  ]) {
    const scope = {};
    vm.runInNewContext(fs.readFileSync(path.join(__dirname, `../src/js/adapters/${name}-canvas-intent.js`), 'utf8'), scope);
    assert.equal(Object.isFrozen(scope[globalName]), true);
    assert.equal(Object.isFrozen(api), true);
    assert.ok(html.indexOf(`js/adapters/${name}-canvas-intent.js`) < html.indexOf(`js/${consumer}.js`));
    assert.throws(() => api.create({}), /Missing/);
  }
});

test('document restore and marquee cancel release capture without selecting', () => {
  for (const finish of [f => f.scope.SMSelectBridge.refreshAfterDocumentRestore(),
    f => f.scope.SMSelectBridge.cancelMarquee()]) {
    const f = fixture(); f.send('pointerdown', 100, 60); finish(f);
    assert.deepEqual(f.captureEvents, [['capture', 1], ['release', 1]]);
    f.send('pointerup', 100, 60);
    assert.deepEqual(writes(f), []);
    assert.equal(f.projections.length, 0);
  }
});
