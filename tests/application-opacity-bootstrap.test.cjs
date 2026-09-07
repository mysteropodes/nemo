'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

function application() {
  let sequence = 0;
  const noop = () => {};
  const ctx = {
    console, crypto: { randomUUID: () => `test-${++sequence}` },
    document: { readyState: 'loading', addEventListener: noop,
      getElementById: () => null, querySelector: () => null, querySelectorAll: () => [] },
    localStorage: { getItem: () => null },
    state: { currentFrame: 0, totalFrames: 24, fps: 24, appMode: 'motion',
      activeLayerIdx: 0, waOut: 23, maxUndo: 50, symbols: {},
      layers: [{ name: 'A', layerUid: 'a', frames: { 0: { strokes: [] } }, motionStatic: { opacity: [100] } }],
      undoStack: [], redoStack: [], undoLabels: [], redoLabels: [] },
    userLayers: [], _layerSel: [], SM: { t: value => value, setActiveLayer: noop },
    saveAllLayerFrames: noop, activateUL: noop, loadFrame: noop,
    renderOS: noop, renderArcs: noop, updateUI: noop, showToast: noop,
    renderLayerList: noop, renderTimeline: noop,
    createUserLayer(name) { ctx.state.layers.push({ name }); return ctx.state.layers.length - 1; },
  };
  ctx.window = ctx;
  vm.createContext(ctx);
  for (const file of ['animation/curve.js', 'domain/animation/opacity.js', 'motion.js',
    'tweens.js', 'application/opacity-application.js', 'bootstrap/opacity-application.js']) {
    const filename = path.resolve(__dirname, '../src/js', file);
    vm.runInContext(fs.readFileSync(filename, 'utf8'), ctx, { filename });
    if (file === 'tweens.js') { ctx.renderOS = noop; ctx.renderArcs = noop; }
  }
  return ctx;
}
function command(ctx, requestId, operation, payload = {}) {
  const identity = ctx.NemoOpacityApplication.meta();
  return { apiVersion: 1, requestId, operation, payload, ...identity, expectedRevision: identity.revision };
}
function opacity(value) { return { layerId: 'a', property: 'opacity', value }; }
function value(ctx, frame = 0) { return ctx.SMMotion.valueAtFrame(ctx.state.layers[0], 'opacity', frame)[0]; }
function handle(ctx, id, op, payload) {
  const result = ctx.NemoApplication.handle(command(ctx, id, op, payload));
  assert.equal(result.ok, true, JSON.stringify(result));
  return result;
}

test('document replacement clears retained history and labels at the boundary', () => {
  const ctx = application();
  handle(ctx, 'old-document-write', 'property.set', opacity(30));
  ctx.state.redoStack.push({ type: 'layers', symbolId: null, layers: [] });
  ctx.state.redoLabels.push('Redo old document');
  ctx.state.undoLabels[0] = 'Undo old document';
  ctx.state.layers = [{ name: 'Imported', motionStatic: { opacity: [75] }, frames: { 0: { strokes: [] } } }];
  assert.equal(ctx.state.undoStack.length, 1);
  assert.equal(ctx.state.redoStack.length, 1);
  ctx.NemoOpacityApplication.documentChanged();
  assert.deepEqual(ctx.state.undoStack, []);
  assert.deepEqual(ctx.state.redoStack, []);
  assert.deepEqual(ctx.state.undoLabels, []);
  assert.deepEqual(ctx.state.redoLabels, []);
  const loaded = handle(ctx, 'new-snapshot', 'snapshot');
  assert.notEqual(loaded.documentId, command(ctx, 'old', 'snapshot').documentId);
  assert.ok(loaded.result.layers[0].id);
  assert.equal(loaded.result.layers[0].opacity, 75);
  assert.equal(ctx.NemoApplication.handle(command(ctx, 'old-document-write', 'property.set', opacity(30))).error.code, 'wrong_document');
  assert.equal(value(ctx), 75);
  assert.equal(handle(ctx, 'again', 'snapshot').result.layers[0].id, loaded.result.layers[0].id);
});

test('ordinary symbol/context history remains available across non-document changes', () => {
  const ctx = application();
  ctx.state.symbols.other = { name: 'Other' };
  ctx.state.undoStack.push({ type: 'layers', symbolId: 'other', layers: [] });
  ctx.state.undoLabels.push('Other edit');
  const before = ctx.state.undoStack.slice();
  ctx.NemoOpacityApplication.meta();
  assert.deepEqual(ctx.state.undoStack, before);
  assert.deepEqual(ctx.state.undoLabels, ['Other edit']);
});
