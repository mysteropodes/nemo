'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ProjectDocument = require('../src/js/project-document.js');
const { extractFunction } = require('./fixtures/lib/sandbox.cjs');

const root = path.resolve(__dirname, '..');
const noop = () => {};

function browserApplication() {
  let sequence = 0;
  const ctx = {
    console, crypto: { randomUUID: () => `browser-${++sequence}` },
    document: { readyState: 'loading', addEventListener: noop,
      getElementById: () => null, querySelector: () => null, querySelectorAll: () => [] },
    localStorage: { getItem: () => null },
    state: { currentFrame: 0, totalFrames: 24, fps: 24, appMode: 'motion',
      activeLayerIdx: 0, waOut: 23, maxUndo: 50, symbols: {},
      layers: [{ name: 'A', layerUid: 'a', frames: { 0: { strokes: [] } },
        motionStatic: { opacity: [25] } }],
      undoStack: [], redoStack: [], undoLabels: [], redoLabels: [] },
    userLayers: [], _layerSel: [],
    SM: { t: value => value, setActiveLayer: noop,
      exportJSON() { return JSON.stringify({ layers: ctx.state.layers }); },
      importJSON(json) {
        if (!ctx.n20AllowLegacyWrite('document-import') ||
            !ctx.n20AllowLegacyWrite('create-layer')) return false;
        if (json === 'throw') throw new Error('invalid fixture');
        ctx.state.layers = JSON.parse(json).layers;
        ctx.NemoOpacityApplication.documentChanged();
        return true;
      } },
    saveAllLayerFrames: noop, activateUL: noop, loadFrame: noop,
    renderOS: noop, renderArcs: noop, updateUI: noop, showToast: noop,
    renderLayerList: noop, renderTimeline: noop,
    createUserLayer(name) { ctx.state.layers.push({ name }); return ctx.state.layers.length - 1; },
  };
  ctx.window = ctx;
  vm.createContext(ctx);
  for (const file of ['animation/curve.js', 'domain/animation/opacity.js',
    'adapters/native-opacity-legacy-surface.js', 'motion.js',
    'domain/document/folder-codec.js', 'domain/tween/assignment.js',
    'application/history/frame-entry.js', 'tweens.js',
    'domain/diagnostics/opacity-diagnostics.js', 'application/opacity-application.js',
    'application/capability-registry.js', 'application/opacity-capability.js',
    'application/export-job.js', 'adapters/export-svg-sequence.js',
    'bootstrap/opacity-application.js']) {
    const filename = path.join(root, 'src/js', file);
    vm.runInContext(fs.readFileSync(filename, 'utf8'), ctx, { filename });
    if (file === 'adapters/native-opacity-legacy-surface.js') {
      assert.equal(ctx.n20AllowLegacyWrite('create-layer'), true, 'boot may create the initial layer');
    }
    if (file === 'tweens.js') { ctx.renderOS = noop; ctx.renderArcs = noop; }
  }
  return ctx;
}

function command(ctx, requestId, operation, payload = {}) {
  const meta = ctx.NemoOpacityApplication.meta();
  return { apiVersion: 1, requestId, ...meta, expectedRevision: meta.revision,
    operation, payload };
}

function snapshot(ctx) {
  return { serialized: ctx.SM.exportJSON(), meta: ctx.NemoOpacityApplication.meta(),
    undo: JSON.stringify(ctx.state.undoStack), redo: JSON.stringify(ctx.state.redoStack) };
}

test('browser v1 opacity and history writes are unavailable before any legacy mutation', () => {
  const ctx = browserApplication();
  const before = snapshot(ctx);
  const descriptor = ctx.NemoApplication.capabilities().find(entry => entry.id === 'opacity').descriptor;
  assert.equal(descriptor.availability.state, 'unavailable');
  const discovery = ctx.NemoApplication.handle(command(ctx, 'discover', 'capabilities'));
  assert.equal(discovery.ok, true);
  assert.equal(discovery.result.descriptors.find(entry => entry.id === 'opacity').availability.state,
    'unavailable');
  assert.ok(discovery.result.operations.includes('property.set'), 'recognized v1 writes remain explicit');
  const payload = { layerId: 'a', property: 'opacity', value: 26 };
  const requests = [
    ['property.set', payload],
    ['property.key.set', { ...payload, frame: 0 }],
    ['property.key.remove', { layerId: 'a', property: 'opacity', frame: 0 }],
    ['property.animation.set', { layerId: 'a', property: 'opacity', animated: true }],
    ['history.undo', {}], ['history.redo', {}],
    ['diagnostics.replay', { request: { apiVersion: 1, requestId: 'recorded',
      operation: 'property.set', payload } }],
  ];
  requests.forEach(([operation, body], index) => {
    const result = ctx.NemoApplication.handle(command(ctx, `denied-${index}`, operation, body));
    assert.equal(result.ok, false, operation);
    assert.equal(result.error.code, 'unavailable', operation);
    assert.deepEqual(snapshot(ctx), before, operation);
  });
  const read = ctx.NemoApplication.handle(command(ctx, 'read', 'property.get',
    { layerId: 'a', property: 'opacity' }));
  assert.equal(read.ok, true);
  assert.equal(read.result.value, 25);
});

test('browser direct opacity UI entries deny while fixture import remains readable', () => {
  const ctx = browserApplication();
  assert.equal(ctx.n20AllowLegacyWrite('create-layer'), false);
  assert.equal(ctx.n20AllowLegacyWrite('document-import'), true);
  assert.equal(ctx.SM.importJSON(JSON.stringify({ layers: [{ name: 'Imported', layerUid: 'a',
    frames: { 0: { strokes: [] } }, motionStatic: { opacity: [25] } }] })), true);
  assert.equal(ctx.n20AllowLegacyWrite('create-layer'), false);
  assert.throws(() => ctx.SM.importJSON('throw'), /invalid fixture/);
  assert.equal(ctx.n20AllowLegacyWrite('create-layer'), false);
  const layer = ctx.state.layers[0];
  const before = snapshot(ctx);
  for (const kind of ['set', 'key-current', 'key-frame', 'remove', 'animated']) {
    const result = ctx.NemoOpacityApplication.legacy(kind, layer, [26], 0);
    assert.equal(result, false, kind);
    assert.deepEqual(snapshot(ctx), before, kind);
  }
  assert.equal(ctx.SMMotion.setValue(layer, 'opacity', [26]), false);
  assert.deepEqual(snapshot(ctx), before);
  for (const kind of ['motion-set-value', 'motion-key-current', 'history-checkpoint']) {
    assert.equal(ctx.n20AllowLegacyWrite(kind), false, kind);
  }
});

test('production browser importer rejects coercible objects before import grants create-layer', () => {
  const app = fs.readFileSync(path.join(root, 'src/js/app.js'), 'utf8');
  const timeline = fs.readFileSync(path.join(root, 'src/js/timeline.js'), 'utf8');
  const start = timeline.indexOf('  importJSON:function(json,silent){');
  const end = timeline.indexOf('\n  getState:function()', start);
  assert.ok(start >= 0 && end > start);
  const importSource = timeline.slice(start, end).replace('  importJSON:', '').replace(/},\s*$/, '}');
  let paperCreated = 0, revision = 0;
  const ctx = {
    state: { totalFrames: 2, layers: [{ name: 'Original', layerUid: 'a' }],
      undoStack: [], redoStack: [], undoLabels: [], redoLabels: [] },
    userLayers: [], arcLayer: { activate: noop }, showToast: noop,
    LAYER_COLOR_PALETTE: ['#ffffff'], SMProjectDocument: ProjectDocument,
    NemoApplication: {}, NemoOpacityApplication: {
      meta: () => ({ documentId: 'original', revision }), documentChanged: () => { revision++; },
    },
    Layer: function (options) { paperCreated++; this.name = options.name; this.insertBelow = noop; },
  };
  ctx.window = ctx;
  vm.createContext(ctx);
  vm.runInContext(fs.readFileSync(path.join(root, 'src/js/adapters/native-opacity-legacy-surface.js'), 'utf8'), ctx);
  vm.runInContext(`${extractFunction(app, 'nextLayerColor')}\n${extractFunction(app, 'createUserLayer')}\n` +
    `this.SM = { importJSON: (${importSource}) }; this.productionCreateUserLayer = createUserLayer;`, ctx);
  ctx.NemoNativeOpacityLegacySurface.installBrowserReadOnlyImport(ctx);
  assert.equal(ctx.n20AllowLegacyWrite('create-layer'), false);
  const before = { document: JSON.stringify(ctx.state), paperCreated, paperLayers: ctx.userLayers.length,
    metadata: ctx.NemoOpacityApplication.meta() };
  let coercions = 0;
  const malicious = { toString() {
    coercions++;
    ctx.productionCreateUserLayer('Reentrant mutation');
    ctx.state.undoStack.push({ type: 'reentrant' });
    ctx.NemoOpacityApplication.documentChanged();
    return '{';
  } };
  assert.equal(ctx.SM.importJSON(malicious), false);
  assert.equal(coercions, 0, 'untrusted conversion must not run inside the import allowance');
  assert.deepEqual({ document: JSON.stringify(ctx.state), paperCreated, paperLayers: ctx.userLayers.length,
    metadata: ctx.NemoOpacityApplication.meta() }, before);
  assert.equal(ctx.n20AllowLegacyWrite('create-layer'), false);
});
