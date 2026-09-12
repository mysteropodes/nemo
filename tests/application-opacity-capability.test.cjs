'use strict';
// P06 — opacity registers itself, and the application entry point dispatches
// through the registry rather than a captured handler reference.
//
// The behavioural guarantees (identity/revision/retry/history) are already
// owned by tests/application-opacity-bootstrap.test.cjs, which now boots
// through the registry: those 8 tests passing IS the "existing independent
// controls still pass through the registry" evidence. This file covers what
// that one cannot see — that dispatch really goes through the registry, and
// that the runtime descriptor still agrees with the committed contract.

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const ROOT = path.resolve(__dirname, '..');
const capability = require('../src/js/application/opacity-capability.js');

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
    'domain/document/folder-codec.js', 'tweens.js', 'application/opacity-application.js',
    'application/capability-registry.js', 'application/opacity-capability.js',
    'bootstrap/opacity-application.js']) {
    const filename = path.resolve(ROOT, 'src/js', file);
    vm.runInContext(fs.readFileSync(filename, 'utf8'), ctx, { filename });
    if (file === 'tweens.js') { ctx.renderOS = noop; ctx.renderArcs = noop; }
  }
  return ctx;
}

function command(ctx, requestId, operation, payload = {}) {
  const identity = ctx.NemoOpacityApplication.meta();
  return { apiVersion: 1, requestId, operation, payload, ...identity, expectedRevision: identity.revision };
}

// ---- the descriptor still agrees with the committed contract ------------

test('the runtime descriptor matches engineering/application/capabilities/opacity.json', () => {
  const committed = JSON.parse(fs.readFileSync(
    path.join(ROOT, 'engineering', 'application', 'capabilities', 'opacity.json'), 'utf8'));
  const runtime = capability.DESCRIPTOR;
  assert.deepEqual(runtime, committed,
    'the synchronous runtime projection drifted from the canonical descriptor');
});

test('the descriptor claims "available", which registration only accepts when backed', () => {
  assert.equal(capability.DESCRIPTOR.availability.state, 'available');
  const registry = require('../src/js/application/capability-registry.js').create();
  // Unbacked, the claim is refused — the P04 defect, still guarded here.
  let code = null;
  try { registry.register(capability.DESCRIPTOR); } catch (error) { code = error.code; }
  assert.equal(code, 'availability_unbacked');
  // Backed, it registers.
  assert.equal(capability.register(registry, () => ({ ok: true })), 'opacity');
});

// ---- dispatch really goes through the registry --------------------------

test('opacity is registered at boot and discoverable', () => {
  const ctx = application();
  // Array.from: the vm realm has its own Array.prototype, so a strict
  // deepEqual against a literal from this realm fails on prototype identity
  // alone even when the contents match (same trap as P20's `{}` default).
  assert.deepEqual(Array.from(ctx.NemoCapabilities.ids()), ['opacity']);
  const listed = ctx.NemoApplication.capabilities();
  assert.equal(listed.length, 1);
  assert.equal(listed[0].id, 'opacity');
  assert.equal(listed[0].handlerKey, 'application.opacity.property');
  assert.equal(listed[0].bound, true);
  assert.deepEqual(JSON.parse(JSON.stringify(listed[0].descriptor)), capability.DESCRIPTOR);
});

test('the production capabilities operation derives its legacy projection and full contract from registration', () => {
  const ctx = application();
  const result = ctx.NemoApplication.handle(command(ctx, 'discover', 'capabilities')).result;
  assert.deepEqual(JSON.parse(JSON.stringify(result.properties)), [
    { id: 'opacity', min: 0, max: 100, unit: 'percent', animated: true },
  ]);
  assert.deepEqual(JSON.parse(JSON.stringify(result.descriptors)), [capability.DESCRIPTOR]);

  // Prove the public response performs a fresh registry read and is not the old
  // hard-coded opacity metadata: changing discovery changes the returned data.
  const original = ctx.NemoCapabilities.list;
  ctx.NemoCapabilities.list = function () {
    const entries = original.call(this);
    entries[0].descriptor = Object.assign({}, entries[0].descriptor,
      { id: 'opacity-from-registry', units: { value: 'ratio' } });
    return entries;
  };
  const changed = ctx.NemoApplication.handle(command(ctx, 'discover-again', 'capabilities')).result;
  assert.equal(changed.descriptors[0].id, 'opacity-from-registry');
  assert.equal(changed.properties[0].id, 'opacity-from-registry');
  assert.equal(changed.properties[0].unit, 'ratio');
});

test('the application entry looks the handler up per call, not once at boot', () => {
  const ctx = application();
  const original = ctx.NemoCapabilities.handlerFor;
  let lookups = 0;
  ctx.NemoCapabilities.handlerFor = function (id) { lookups += 1; return original.call(this, id); };

  const before = ctx.SMMotion.valueAtFrame(ctx.state.layers[0], 'opacity', 0)[0];
  const result = ctx.NemoApplication.handle(
    command(ctx, 'r1', 'property.set', { layerId: 'a', property: 'opacity', value: 40 }));

  assert.equal(result.ok, true, JSON.stringify(result));
  assert.equal(lookups, 1, 'dispatch did not go through registry.handlerFor');
  assert.equal(before, 100);
  assert.equal(ctx.SMMotion.valueAtFrame(ctx.state.layers[0], 'opacity', 0)[0], 40);
});

test('replacing the registered handler changes what the entry point dispatches to', () => {
  // The strongest available proof that no captured reference survives: swap
  // what the registry returns and the public entry follows it.
  const ctx = application();
  ctx.NemoCapabilities.handlerFor = function () {
    return function () { return { ok: true, substituted: true }; };
  };
  const result = ctx.NemoApplication.handle(
    command(ctx, 'r2', 'property.set', { layerId: 'a', property: 'opacity', value: 10 }));
  assert.equal(result.substituted, true, 'the entry point kept its own handler reference');
  // ...and the real write never happened, because the real handler was bypassed.
  assert.equal(ctx.SMMotion.valueAtFrame(ctx.state.layers[0], 'opacity', 0)[0], 100);
});

test('an unregistered capability id fails explicitly rather than silently', () => {
  const ctx = application();
  let code = null;
  try { ctx.NemoCapabilities.handlerFor('not-a-capability'); } catch (error) { code = error.code; }
  assert.equal(code, 'unknown_capability');
});

// ---- baseline preserved -------------------------------------------------

test('the legacy UI gesture path is unchanged and still writes through the domain', () => {
  // Recorded, not silently claimed: NemoOpacityApplication.legacy is the
  // existing UI-gesture surface and it is NOT routed through the registry by
  // this leaf. It calls the same domain module and reports to the same history
  // authority, so it is not a second writer of opacity STATE, but it is a
  // second entry point. Changing that is a UI-surface change, outside P06.
  const ctx = application();
  assert.equal(typeof ctx.NemoOpacityApplication.legacy, 'function');
  ctx.NemoOpacityApplication.legacy('set', ctx.state.layers[0], [25]);
  assert.equal(ctx.SMMotion.valueAtFrame(ctx.state.layers[0], 'opacity', 0)[0], 25);
});
