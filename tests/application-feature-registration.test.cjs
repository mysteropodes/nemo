'use strict';
// P08 — feature registration, accepted from a fresh session.
//
// The title of the leaf carries the whole difficulty: *fresh*. A session whose
// registry is already warm proves nothing about registration, so every test
// below builds a new realm and loads the real production scripts into it,
// bootstrap included. Nothing is pre-registered by the harness.
//
// Deliberately NOT retested here, because it is already pinned elsewhere and a
// second copy would be a second thing to keep in step:
//   application-capability-registry.test.cjs  — an "available" claim with no
//     handler is refused; discovery is order-independent.
//   application-export-job-binding.test.cjs   — D1 runtime descriptor equals
//     the declaration file; D2 the handler is what makes "available" true;
//     C1 the UI and MCP paths report the same typed reason; C3 every emittable
//     reason is in the capability-v1 enum.
//
// What is new here is the three things P08 actually asks: that a cold
// bootstrap registers the feature, that dispatch is *derived* from the
// declaration rather than switched in source, and that the declaration is held
// by a standard gate.

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const vm = require('node:vm');

const ROOT = path.resolve(__dirname, '..');
const drift = require('../scripts/nemo/lib/capability-drift.cjs');

const DECLARATION = 'engineering/application/capabilities/export-job.json';

// A cold session: a brand-new realm, the real scripts, the real bootstrap.
// Same load order the production page uses and the same list the existing
// bootstrap test loads, so a load-order regression shows up here too.
function coldSession() {
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
    userLayers: [], _layerSel: [], SM: { t: (v) => v, setActiveLayer: noop },
    saveAllLayerFrames: noop, activateUL: noop, loadFrame: noop,
    renderOS: noop, renderArcs: noop, updateUI: noop, showToast: noop,
    renderLayerList: noop, renderTimeline: noop,
    createUserLayer(name) { ctx.state.layers.push({ name }); return ctx.state.layers.length - 1; },
  };
  ctx.window = ctx;
  vm.createContext(ctx);
  for (const file of ['animation/curve.js', 'domain/animation/opacity.js', 'motion.js',
    'domain/document/folder-codec.js', 'domain/tween/assignment.js',
    'application/history/frame-entry.js', 'tweens.js', 'domain/diagnostics/opacity-diagnostics.js', 'application/opacity-application.js',
    'application/capability-registry.js', 'application/opacity-capability.js',
    'application/export-job.js', 'adapters/export-svg-sequence.js',
    'bootstrap/opacity-application.js']) {
    const filename = path.resolve(ROOT, 'src/js', file);
    vm.runInContext(fs.readFileSync(filename, 'utf8'), ctx, { filename });
  }
  return ctx;
}

function entry(ctx, id) {
  return ctx.NemoCapabilities.list().find((e) => e.id === id) || null;
}

// deepStrictEqual compares prototypes, and anything built inside a vm realm
// carries different ones — structurally identical values still fail. Everything
// crossing the realm boundary here is plain JSON, so compare it by value, with
// keys sorted so a difference in property order is not reported as a mismatch.
function canonical(value) {
  if (Array.isArray(value)) return value.map(canonical);
  if (value && typeof value === 'object') {
    return Object.keys(value).sort().reduce((out, key) => { out[key] = canonical(value[key]); return out; }, {});
  }
  return value;
}
function assertSame(actual, expected, message) {
  assert.equal(JSON.stringify(canonical(actual)), JSON.stringify(canonical(expected)), message);
}

// A descriptor the production tree knows nothing about. The point of the
// synthetic capability is that registration alone must be enough: if dispatch
// needed a source edit, nothing here could reach its handler.
function synthetic(id, lifecycle, handlerKey) {
  return {
    schemaVersion: 1, id, version: 1,
    input: { type: 'object' }, output: { type: 'object' },
    effects: { kind: 'query', scope: 'none', lifecycle },
    availability: { state: 'available', reason: null },
    handlerKey: handlerKey || ('test.' + id),
  };
}

// ---- check 1: a cold bootstrap registers the feature -----------------------

test('a cold session registers the export feature, bound, without anything pre-warming it', () => {
  const ctx = coldSession();
  const found = entry(ctx, 'export.svg.frame');
  assert.ok(found, 'the export capability is absent from a freshly bootstrapped registry');
  assert.equal(found.bound, true, 'registered but unbound would advertise a handler that cannot run');
  assert.equal(found.descriptor.availability.state, 'available');
  assert.equal(found.handlerKey, 'application.export.svgFrame');
  assert.equal(typeof ctx.NemoCapabilities.handlerFor('export.svg.frame'), 'function');
});

test('registration is a property of the bootstrap, not of one lucky session', () => {
  // Two independent realms must agree. If registration leaked through shared
  // module state rather than the bootstrap, the second would differ.
  const a = coldSession();
  const b = coldSession();
  const ids = (ctx) => ctx.NemoCapabilities.ids().slice().sort();
  assertSame(ids(a), ids(b));
  assert.ok(ids(a).includes('export.svg.frame'));
  assert.equal(entry(a, 'export.svg.frame').bound, entry(b, 'export.svg.frame').bound);
});

// ---- check 1: dispatch is derived, not a central switch --------------------

test('dispatch follows a newly declared lifecycle with no source edit', () => {
  const ctx = coldSession();
  let reached = null;
  // "export.probe" exists nowhere in the tree. Registering it is the ONLY
  // action taken; if some switch in source decided routing, this could not work.
  ctx.NemoCapabilities.register(
    synthetic('export.probe', ['probe.run']),
    (request) => { reached = request.operation; return { ok: true, probed: true }; },
  );
  const result = ctx.NemoApplication.handle({ apiVersion: 1, requestId: 'p', operation: 'probe.run', payload: {} });
  assert.equal(reached, 'probe.run', 'the request never reached the newly registered handler');
  assert.equal(result.probed, true);
});

test('an operation no capability claims falls back to the default capability', () => {
  // Service-wide operations (snapshot, history.*, diagnostics.*) belong to no
  // single feature, so the fallback is load-bearing, not an accident.
  const ctx = coldSession();
  const meta = ctx.NemoOpacityApplication.meta();
  const result = ctx.NemoApplication.handle({
    apiVersion: 1, requestId: 'snap', operation: 'snapshot', payload: {},
    ...meta, expectedRevision: meta.revision,
  });
  assert.equal(result.ok, true, JSON.stringify(result));
  assert.ok(result.result && Array.isArray(result.result.layers));
});

test('two capabilities claiming one operation fail loudly rather than mis-route', () => {
  const ctx = coldSession();
  // "start" is already claimed by the export capability's declared lifecycle.
  ctx.NemoCapabilities.register(synthetic('export.rival', ['start']), () => ({ ok: true }));
  assert.throws(
    () => ctx.NemoApplication.handle({ apiVersion: 1, requestId: 'x', operation: 'start', payload: {} }),
    /both claim the operation "start"/,
    'a collision must be loud; silently picking one would route half the calls to the wrong feature',
  );
});

// ---- check 2: one schema across the direct API and MCP ---------------------

test('the bootstrapped descriptor, the declaration file and the MCP transport copy are one schema', () => {
  const runtime = entry(coldSession(), 'export.svg.frame').descriptor;
  const declared = JSON.parse(fs.readFileSync(path.join(ROOT, DECLARATION), 'utf8'));
  const transport = JSON.parse(fs.readFileSync(
    path.join(ROOT, 'engineering/application/transport-v1.schema.json'), 'utf8'));

  const embedded = transport.request.properties.payload['x-nemo-registeredCapabilities']
    .find((c) => c.id === 'export.svg.frame');
  assert.ok(embedded, 'MCP advertises no export capability, so the two surfaces cannot agree');

  // Direct API === declaration, and declaration === what MCP advertises.
  // Either link breaking lets one surface describe a contract the other does
  // not honour, which is the failure this check exists for.
  assertSame(runtime, declared, 'the running registry and the declaration file disagree');
  assertSame(embedded, declared, 'the MCP transport copy and the declaration file disagree');
});

test('the MCP binary compiles the same declaration file, so it cannot advertise a different one', () => {
  // Source-level, so this holds without a Rust build: the include_str! path is
  // the declaration itself, not a hand-maintained duplicate.
  const rust = fs.readFileSync(path.join(ROOT, 'nemo-mcp/src/capabilities.rs'), 'utf8');
  assert.ok(drift.declaredSources(ROOT).includes(DECLARATION),
    'CAPABILITY_SOURCES does not declare the export descriptor');
  assert.match(rust, /include_str!\("\.\.\/\.\.\/engineering\/application\/capabilities\/export-job\.json"\)/);
});

test('the availability the direct API reports is the availability MCP advertises', () => {
  const runtime = entry(coldSession(), 'export.svg.frame').descriptor.availability;
  const declared = JSON.parse(fs.readFileSync(path.join(ROOT, DECLARATION), 'utf8')).availability;
  assertSame(runtime, declared);
  // P19 flipped this from unavailable/missing-dependency; the registry refuses
  // an "available" claim with no handler, so the two cannot drift apart.
  assert.equal(runtime.state, 'available');
  assert.equal(entry(coldSession(), 'export.svg.frame').bound, true);
});

// ---- check 3: a standard gate holds the declaration ------------------------

// Built against a throwaway root rather than deleting a tracked file, so the
// test cannot leave the working tree damaged if it fails midway.
function fixtureRoot(omit) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'p08-caps-'));
  for (const rel of ['engineering/application/capabilities/opacity.json',
    'engineering/application/capabilities/timelapse.json',
    'engineering/application/capabilities/diagnostics.json',
    DECLARATION,
    'engineering/application/capability-v1.schema.json',
    'engineering/application/transport-v1.schema.json',
    'nemo-mcp/src/capabilities.rs']) {
    if (rel === omit) continue;
    const dest = path.join(dir, rel);
    fs.mkdirSync(path.dirname(dest), { recursive: true });
    fs.copyFileSync(path.join(ROOT, rel), dest);
  }
  return dir;
}

test('removing the declaration fails the standard completeness gate; restoring it passes', () => {
  const whole = fixtureRoot(null);
  assert.equal(drift.checkSurface(whole).status, 'pass');
  assert.equal(drift.checkTransportCopies(whole).status, 'pass');

  const without = fixtureRoot(DECLARATION);
  const surface = drift.checkSurface(without);
  const copies = drift.checkTransportCopies(without);
  assert.equal(surface.status, 'fail', 'a missing declaration must not pass the surface gate');
  assert.match(surface.reason, /declares missing file\(s\).*export-job\.json/);
  assert.equal(copies.status, 'fail', 'MCP would still advertise a capability with no descriptor');
  assert.match(copies.reason, /export\.svg\.frame.*no descriptor file/);

  fs.rmSync(whole, { recursive: true, force: true });
  fs.rmSync(without, { recursive: true, force: true });
});

test('the catalog count is NOT what holds completeness, so do not rely on it', () => {
  // Recorded because it is counter-intuitive and a future simplification could
  // drop the two gates above and keep this one, losing the property silently:
  // the catalog happily reports a pass over the reduced set.
  const without = fixtureRoot(DECLARATION);
  const reduced = drift.catalog(without);
  assert.equal(reduced.sources.length, 3);
  assert.deepEqual(reduced.sources.map((s) => s.id), ['diagnostics', 'opacity', 'timelapse']);
  fs.rmSync(without, { recursive: true, force: true });
});
