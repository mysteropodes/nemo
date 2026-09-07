'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

// Load complete production scripts, including the existing history authority.
// Only Paper reconstruction and display ports are inert for this stored-layer fixture.
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

test('API edit and ordinary UI undo/redo use the production history stack', () => {
  const ctx = application();
  handle(ctx, 'edit', 'property.set', opacity(42));
  assert.equal(value(ctx), 42);
  assert.equal(ctx.state.undoStack.length, 1);
  const stale = command(ctx, 'stale-after-undo', 'property.set', opacity(10));
  const before = ctx.NemoOpacityApplication.meta().revision;
  ctx.undo();
  assert.equal(value(ctx), 100);
  assert.equal(ctx.state.undoStack.length, 0);
  assert.equal(ctx.state.redoStack.length, 1);
  assert.equal(ctx.NemoOpacityApplication.meta().revision, before + 1);
  assert.equal(ctx.NemoApplication.handle(stale).error.code, 'stale_revision');
  ctx.redo();
  assert.equal(value(ctx), 42);
  assert.equal(ctx.state.undoStack.length, 1);
});

test('ordinary Motion edits invalidate stale commands and API history restores them once', () => {
  const ctx = application();
  ctx.pushUndo();
  const stale = command(ctx, 'before-ui', 'property.set', opacity(10));
  ctx.SMMotion.setValue(ctx.state.layers[0], 'opacity', [25]);
  assert.equal(value(ctx), 25);
  assert.equal(ctx.state.undoStack.length, 1, 'UI owns one gesture checkpoint');
  assert.equal(ctx.NemoApplication.handle(stale).error.code, 'stale_revision');
  const undo = command(ctx, 'api-undo', 'history.undo');
  const result = ctx.NemoApplication.handle(undo);
  assert.equal(result.ok, true);
  assert.equal(value(ctx), 100);
  assert.equal(ctx.state.redoStack.length, 1);
  assert.deepEqual(ctx.NemoApplication.handle(undo), result);
  assert.equal(ctx.state.redoStack.length, 1, 'retry must not consume another history entry');
  handle(ctx, 'api-redo', 'history.redo');
  assert.equal(value(ctx), 25);
  assert.equal(ctx.state.undoStack.length, 1);
});

test('bounded UI history detects changed top entries even when stack length is unchanged', () => {
  const ctx = application();
  ctx.state.maxUndo = 1;
  ctx.pushUndo();
  ctx.SMMotion.setValue(ctx.state.layers[0], 'opacity', [60]);
  const stale = command(ctx, 'before-ui-checkpoint', 'property.set', opacity(10));
  const before = ctx.NemoOpacityApplication.meta().revision;
  ctx.pushUndo();
  assert.equal(ctx.state.undoStack.length, 1);
  assert.equal(ctx.NemoOpacityApplication.meta().revision, before + 1);
  assert.equal(ctx.NemoApplication.handle(stale).error.code, 'stale_revision');
});

test('empty and cross-context UI history do not consume entries or advance revision', () => {
  const ctx = application();
  const initial = ctx.NemoOpacityApplication.meta().revision;
  ctx.undo(); ctx.redo();
  assert.equal(ctx.NemoOpacityApplication.meta().revision, initial);
  ctx.state.symbols.other = { name: 'Other' };
  ctx.state.undoStack.push({ type: 'layers', symbolId: 'other', layers: [] });
  ctx.state.undoLabels.push('Other edit');
  const refused = ctx.NemoApplication.handle(command(ctx, 'wrong-context', 'history.undo'));
  assert.equal(refused.error.code, 'history_unavailable');
  assert.equal(ctx.state.undoStack.length, 1);
  assert.equal(ctx.state.redoStack.length, 0);
  assert.equal(ctx.NemoOpacityApplication.meta().revision, initial);
});

test('key edits use the production evaluator and UI history retains keyed storage', () => {
  const ctx = application();
  handle(ctx, 'key-zero', 'property.key.set', { ...opacity(80), frame: 0 });
  handle(ctx, 'key-ten', 'property.key.set', { ...opacity(20), frame: 10 });
  assert.equal(value(ctx, 0), 80);
  assert.equal(value(ctx, 10), 20);
  const stored = JSON.stringify(ctx.state.layers);
  ctx.undo();
  assert.equal(value(ctx, 10), 80);
  ctx.redo();
  assert.equal(JSON.stringify(ctx.state.layers), stored);
  assert.equal(value(ctx, 10), 20);
});

test('document replacement rejects old retries and assigns stable ids to newly loaded layers', () => {
  const ctx = application();
  const old = command(ctx, 'old-document-write', 'property.set', opacity(30));
  assert.equal(ctx.NemoApplication.handle(old).ok, true);
  handle(ctx, 'second-old-write', 'property.set', opacity(45));
  handle(ctx, 'old-undo', 'history.undo');
  for (const field of ['undoStack', 'undoLabels', 'redoStack', 'redoLabels']) {
    assert.equal(ctx.state[field].length, 1, field + ' must contain prior-document history');
  }
  let historyRefreshes = 0;
  ctx.renderHistoryPanelIfOpen = () => historyRefreshes++;
  ctx.state.layers = [{ name: 'Imported', motionStatic: { opacity: [75] }, frames: { 0: { strokes: [] } } }];
  ctx.NemoOpacityApplication.documentChanged();
  for (const field of ['undoStack', 'undoLabels', 'redoStack', 'redoLabels']) {
    assert.equal(ctx.state[field].length, 0, field + ' must not cross the document boundary');
  }
  assert.equal(historyRefreshes, 1);
  const loaded = handle(ctx, 'new-snapshot', 'snapshot');
  assert.notEqual(loaded.documentId, old.documentId);
  assert.ok(loaded.result.layers[0].id);
  assert.equal(loaded.result.layers[0].opacity, 75);
  assert.equal(ctx.NemoApplication.handle(old).error.code, 'wrong_document');
  for (const operation of ['history.undo', 'history.redo']) {
    const refused = ctx.NemoApplication.handle(command(ctx, 'new-' + operation, operation));
    assert.equal(refused.ok, false);
    assert.equal(refused.error.code, 'history_unavailable');
  }
  assert.equal(value(ctx), 75);
  assert.equal(ctx.state.undoStack.length, 0);
  assert.equal(handle(ctx, 'again', 'snapshot').result.layers[0].id, loaded.result.layers[0].id);
});

test('ordinary context changes retain history for returning to the edited context', () => {
  const ctx = application();
  handle(ctx, 'scene-write', 'property.set', opacity(30));
  const stacks = JSON.stringify([ctx.state.undoStack, ctx.state.undoLabels, ctx.state.redoStack, ctx.state.redoLabels]);
  ctx.state.symbols.other = { name: 'Other' };
  ctx.state.activeSymbolId = 'other';
  assert.equal(ctx.NemoApplication.handle({ apiVersion: 1, requestId: 'symbol-snapshot', operation: 'snapshot', payload: {} }).ok, true);
  assert.equal(JSON.stringify([ctx.state.undoStack, ctx.state.undoLabels, ctx.state.redoStack, ctx.state.redoLabels]), stacks);
  const refused = ctx.NemoApplication.handle(command(ctx, 'symbol-undo', 'history.undo'));
  assert.equal(refused.error.code, 'history_unavailable');
  ctx.state.activeSymbolId = null;
  assert.equal(ctx.NemoApplication.handle({ apiVersion: 1, requestId: 'scene-snapshot', operation: 'snapshot', payload: {} }).ok, true);
  handle(ctx, 'scene-undo', 'history.undo');
  assert.equal(value(ctx), 100);
  handle(ctx, 'scene-redo', 'history.redo');
  assert.equal(value(ctx), 30);
});

test('a live UI gesture rejects API writes before checkpointing or changing stored opacity', () => {
  const ctx = application();
  const pending = command(ctx, 'gesture-write', 'property.set', opacity(12));
  ctx._scrubLiveActive = true;
  const result = ctx.NemoApplication.handle(pending);
  assert.equal(result.error.code, 'unavailable');
  assert.equal(ctx.state.undoStack.length, 0);
  assert.equal(value(ctx), 100);
  ctx._scrubLiveActive = false;
  assert.equal(ctx.NemoApplication.handle(pending).ok, true);
  assert.equal(ctx.state.undoStack.length, 1);
  assert.equal(value(ctx), 12);
});
