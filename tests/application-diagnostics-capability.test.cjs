'use strict';
// T08 -- the shared diagnostics inspector's capability registration.
// Descriptor + contract tests, mirroring application-timelapse-capability.test.cjs
// (P31), that leaf's closest sibling: not wired into a script tag or the live
// NemoApplication.handle dispatch (bootstrap-layer work, out of scope).

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const ROOT = path.resolve(__dirname, '..');
const capability = require('../src/js/application/diagnostics-capability.js');
const NemoCapabilityRegistry = require('../src/js/application/capability-registry.js');
const fixture = require('./fixtures/lib/opacity-application.cjs');

// ---- the descriptor still agrees with the committed contract ------------

test('the runtime descriptor matches engineering/application/capabilities/diagnostics.json', () => {
  const committed = JSON.parse(fs.readFileSync(
    path.join(ROOT, 'engineering', 'application', 'capabilities', 'diagnostics.json'), 'utf8'));
  assert.deepEqual(capability.DESCRIPTOR, committed,
    'the synchronous runtime projection drifted from the canonical descriptor');
});

test('the descriptor is read-only: "inspect" is its only lifecycle operation', () => {
  assert.deepEqual(capability.DESCRIPTOR.effects.lifecycle, ['inspect']);
  assert.equal(capability.DESCRIPTOR.effects.kind, 'query');
});

// ---- computeAvailability ---------------------------------------------------

test('computeAvailability: no window is missing-dependency', () => {
  assert.deepEqual(capability.computeAvailability(null), { state: 'unavailable', reason: 'missing-dependency' });
});

test('computeAvailability: NemoApplication present but NemoOpacityApplication missing is missing-dependency', () => {
  assert.deepEqual(capability.computeAvailability({ NemoApplication: { handle: () => {} } }),
    { state: 'unavailable', reason: 'missing-dependency' });
});

test('computeAvailability: both loaded is available', () => {
  assert.deepEqual(capability.computeAvailability({ NemoApplication: { handle: () => {} }, NemoOpacityApplication: { meta: () => {} } }),
    { state: 'available', reason: null });
});

// ---- registration: the P05 registry contract -----------------------------

test('claiming available without a bound handler is refused, same guard as opacity/timelapse', () => {
  const registry = NemoCapabilityRegistry.create();
  let code = null;
  try { capability.register(registry, undefined, { state: 'available', reason: null }); }
  catch (error) { code = error.code; }
  assert.equal(code, 'availability_unbacked');
});

test('claiming available with a bound handler registers', () => {
  const registry = NemoCapabilityRegistry.create();
  const id = capability.register(registry, () => ({ ok: true }), { state: 'available', reason: null });
  assert.equal(id, 'diagnostics');
  assert.equal(registry.get('diagnostics').availability.state, 'available');
});

// ---- handlerFor: dispatch against the real opacity-application chain -----

function opacityWindow() {
  const f = fixture.build('diag-cap', 55);
  return { NemoApplication: { handle: (req) => f.app.handle(req) }, NemoOpacityApplication: f.app, _f: f };
}
function send(win, id, operation, payload) {
  const identity = win.NemoOpacityApplication.meta();
  const response = win.NemoApplication.handle({ apiVersion: 1, requestId: id, ...identity, expectedRevision: identity.revision, operation, payload });
  assert.equal(response.ok, true, JSON.stringify(response));
  return response;
}

test('handlerFor rejects an unknown operation', () => {
  const win = opacityWindow();
  const handler = capability.handlerFor(win);
  const result = handler({ operation: 'replay', payload: {} });
  assert.equal(result.ok, false);
  assert.equal(result.error.code, 'unknown_operation');
});

test('handlerFor "inspect" reaches the SAME trace diagnostics.trace already returns, reshaped and bounded', () => {
  const win = opacityWindow();
  const layerId = win._f.state.layers[0].layerUid;
  send(win, 'c1', 'property.set', { layerId, property: 'opacity', value: 11 });
  send(win, 'c2', 'property.set', { layerId, property: 'opacity', value: 22 });
  send(win, 'c3', 'property.set', { layerId, property: 'opacity', value: 33 });

  const handler = capability.handlerFor(win);
  const full = handler({ operation: 'inspect', payload: {} });
  assert.equal(full.ok, true);
  assert.equal(full.result.retentionLimit, 32);
  assert.deepEqual(full.result.entries.map((e) => e.requestId), ['c1', 'c2', 'c3']);
  full.result.entries.forEach((e) => assert.equal(e.operation, 'property.set'));

  const limited = handler({ operation: 'inspect', payload: { limit: 2 } });
  assert.deepEqual(limited.result.entries.map((e) => e.requestId), ['c2', 'c3'], 'limit keeps the most RECENT entries');

  // Cross-check against the underlying diagnostics.trace directly: same data, not a second copy.
  const identity = win.NemoOpacityApplication.meta();
  const raw = win.NemoApplication.handle({ apiVersion: 1, requestId: 'raw-trace', ...identity, expectedRevision: identity.revision, operation: 'diagnostics.trace', payload: {} });
  assert.deepEqual(full.result.entries.map((e) => e.requestId), raw.result.entries.map((e) => e.request.requestId));
});

test('handlerFor never calls diagnostics.replay -- inspect is read-only', () => {
  const win = opacityWindow();
  let sawReplay = false;
  const realHandle = win.NemoApplication.handle;
  win.NemoApplication.handle = function (req) { if (req.operation === 'diagnostics.replay') sawReplay = true; return realHandle(req); };
  const handler = capability.handlerFor(win);
  handler({ operation: 'inspect', payload: {} });
  assert.equal(sawReplay, false);
});

// ---- routing guards: source-level, not just behavioral --------------------

// Strips '//' line comments before matching, so a doc comment that merely
// TALKS ABOUT diagnostics.replay ("nothing here ever calls it") does not
// trip its own guard -- only actual code use of the string should.
function withoutLineComments(source) {
  return source.split('\n').map((line) => line.replace(/\/\/.*/, '')).join('\n');
}

test('routing guard: diagnostics-capability.js never calls diagnostics.replay or reads NemoOpacityDiagnostics state directly', () => {
  const code = withoutLineComments(fs.readFileSync(path.join(ROOT, 'src/js/application/diagnostics-capability.js'), 'utf8'));
  assert.doesNotMatch(code, /diagnostics\.replay/, 'the capability must stay read-only');
  assert.doesNotMatch(code, /NemoOpacityDiagnostics/, 'it must reach the trace through NemoApplication.handle, never the private module directly');
});

test('routing guard: diagnostics-panel.js never calls diagnostics.replay or a property-writing operation', () => {
  const code = withoutLineComments(fs.readFileSync(path.join(ROOT, 'src/js/labs/diagnostics-panel.js'), 'utf8'));
  assert.doesNotMatch(code, /diagnostics\.replay/);
  assert.doesNotMatch(code, /property\.(set|key\.set|key\.remove|animation\.set)/, 'the panel must never mutate a document property');
});

// ---- panel and capability observe the exact same synthetic trace ----------
//
// Loads the real shipped sources (opacity-application.js chain +
// diagnostics-panel.js) into one shared vm context, the same pattern
// application-opacity-bootstrap.test.cjs already uses, plus a minimal DOM
// fake covering exactly what the panel needs (createElement/appendChild/
// innerHTML/style). Not a reimplementation of the panel: the actual shipped
// diagnostics-panel.js source runs unmodified.

function fakeElement(tag) {
  return {
    tagName: tag, style: {}, children: [], _html: '',
    get innerHTML() { return this._html; }, set innerHTML(v) { this._html = v; },
    setAttribute() {}, getAttribute() { return null; },
    appendChild(child) { this.children.push(child); return child; },
    remove() {}, addEventListener() {}, querySelector() { return null; }, querySelectorAll() { return []; },
  };
}

function panelWindow() {
  let sequence = 0;
  const flags = {};
  const domain = require('../src/js/domain/animation/opacity.js');
  const undo = [], redo = [];
  const layerA = 'diag-panel-layer-a';
  const state = { currentFrame: 0, totalFrames: 24, fps: 24, appMode: 'motion',
    activeLayerIdx: 0, waOut: 23, maxUndo: 50, symbols: {},
    layers: [{ name: 'A', layerUid: layerA, frames: { 0: { strokes: [] } }, motionStatic: { opacity: [100] } }],
    undoStack: [], redoStack: [], undoLabels: [], redoLabels: [] };
  const ctx = {
    console,
    document: {
      _lastCreated: null,
      createElement(tag) { const el = fakeElement(tag); this._lastCreated = el; return el; },
      body: { appendChild: () => {} }, readyState: 'complete', addEventListener: () => {},
      querySelector: () => null, querySelectorAll: () => [], getElementById: () => null,
    },
    localStorage: { getItem: (k) => (flags[k] ? '1' : null), setItem: (k, v) => { flags[k] = v === '1'; } },
    crypto: { randomUUID: () => `panel-${++sequence}` },
    state, userLayers: [], _layerSel: [],
    SM: { t: (k) => k, setActiveLayer: () => {} },
    saveAllLayerFrames: () => {}, activateUL: () => {}, loadFrame: () => {},
    renderOS: () => {}, renderArcs: () => {}, updateUI: () => {}, showToast: () => {},
    renderLayerList: () => {}, renderTimeline: () => {},
    createUserLayer() { return 0; },
  };
  ctx.window = ctx;
  vm.createContext(ctx);
  for (const file of ['animation/curve.js', 'domain/animation/opacity.js', 'motion.js',
    'domain/document/folder-codec.js', 'domain/tween/assignment.js', 'application/history/frame-entry.js',
    'tweens.js', 'domain/diagnostics/opacity-diagnostics.js', 'application/opacity-application.js',
    'application/capability-registry.js', 'application/opacity-capability.js',
    'application/export-job.js', 'adapters/export-svg-sequence.js',
    'bootstrap/opacity-application.js',
    'application/diagnostics-capability.js', 'labs/labs-core.js', 'labs/diagnostics-panel.js']) {
    const filename = path.resolve(ROOT, 'src/js', file);
    vm.runInContext(fs.readFileSync(filename, 'utf8'), ctx, { filename });
    if (file === 'tweens.js') { ctx.renderOS = () => {}; ctx.renderArcs = () => {}; }
  }
  return { ctx, layerA };
}

test('the diagnostics capability and the panel observe the exact same synthetic trace', () => {
  const { ctx, layerA } = panelWindow();
  function send(id, value) {
    const response = ctx.NemoApplication.handle({ apiVersion: 1, requestId: id, ...ctx.NemoOpacityApplication.meta(),
      expectedRevision: ctx.NemoOpacityApplication.meta().revision, operation: 'property.set', payload: { layerId: layerA, property: 'opacity', value } });
    assert.equal(response.ok, true, JSON.stringify(response));
  }
  send('panel-c1', 41);
  send('panel-c2', 52);

  // The capability's own view.
  const registry = ctx.NemoCapabilityRegistry.create();
  const handler = ctx.NemoDiagnosticsCapability.handlerFor(ctx.window);
  ctx.NemoDiagnosticsCapability.register(registry, handler, { state: 'available', reason: null });
  const inspected = handler({ operation: 'inspect', payload: {} });
  assert.equal(inspected.ok, true);
  // Spread out of the vm realm: its Array.prototype differs from this
  // realm's, so a structurally identical array still fails deepEqual by
  // prototype alone (same trap documented in
  // application-opacity-capability.test.cjs).
  assert.deepEqual([...inspected.result.entries].map((e) => e.requestId), ['panel-c1', 'panel-c2']);

  // The panel's own view: enabling the labs flag renders through the real
  // shipped diagnostics-panel.js, which fetches through the same
  // NemoApplication.handle('diagnostics.trace', ...) path.
  ctx.SMLabs.enable('diagnostics-panel');
  const panelEl = ctx.document._lastCreated; // see note below
  assert.ok(panelEl, 'the panel must have created its container element');
  assert.match(panelEl.innerHTML, /panel-c1/);
  assert.match(panelEl.innerHTML, /panel-c2/);
  assert.match(panelEl.innerHTML, /property\.set/);
});
