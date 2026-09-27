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

// ---- review fixes (Pollen, 2026-09-22) -------------------------------------

test('a hostile requestId cannot inject markup into the panel', () => {
  // requestId is caller-supplied and only length-checked by validate(), so it
  // arrives here verbatim from whatever drove the command -- an MCP client
  // included. The panel builds rows as an innerHTML string, so this is the
  // regression guard for that escaping.
  const HOSTILE = '"><img src=x onerror=alert(1)>';
  const { ctx, layerA } = panelWindow();
  const meta = () => ctx.NemoOpacityApplication.meta();
  const written = ctx.NemoApplication.handle({ apiVersion: 1, requestId: HOSTILE, ...meta(),
    expectedRevision: meta().revision, operation: 'property.set',
    payload: { layerId: layerA, property: 'opacity', value: 42 } });
  assert.equal(written.ok, true, 'the core still accepts it as an ordinary requestId; the panel is what must be safe');

  ctx.SMLabs.enable('diagnostics-panel');
  const html = ctx.document._lastCreated.innerHTML;
  assert.doesNotMatch(html, /<img/, 'no tag may survive into the rendered markup');
  assert.ok(!html.includes(HOSTILE), 'the payload must not appear verbatim');
  assert.match(html, /&quot;&gt;&lt;img src=x onerror=alert\(1\)&gt;/, 'it must appear escaped instead');
});

test('the panel escapes every interpolated trace field, not only requestId', () => {
  const { ctx, layerA } = panelWindow();
  const meta = () => ctx.NemoOpacityApplication.meta();
  ctx.NemoApplication.handle({ apiVersion: 1, requestId: 'a<b>&c"d', ...meta(),
    expectedRevision: meta().revision, operation: 'property.set',
    payload: { layerId: layerA, property: 'opacity', value: 7 } });
  ctx.SMLabs.enable('diagnostics-panel');
  const html = ctx.document._lastCreated.innerHTML;
  assert.match(html, /a&lt;b&gt;&amp;c&quot;d/);
});

test('retentionLimit is read from the application, not restated as a literal', () => {
  // A bound change made coherently (ring buffer + capabilitySummary) used to
  // leave this capability advertising a stale 32 that no test could see.
  const win = opacityWindow();
  const realHandle = win.NemoApplication.handle;
  win.NemoApplication.handle = function (request) {
    const response = realHandle(request);
    if (request.operation === 'capabilities' && response.ok) response.result.traceRetention = 16;
    return response;
  };
  const inspected = capability.handlerFor(win)({ operation: 'inspect', payload: {} });
  assert.equal(inspected.ok, true);
  assert.equal(inspected.result.retentionLimit, 16, 'it must follow the application, not a hardcoded 32');
});

test('inspect fails honestly when the application reports no retention bound', () => {
  const win = opacityWindow();
  const realHandle = win.NemoApplication.handle;
  win.NemoApplication.handle = function (request) {
    const response = realHandle(request);
    if (request.operation === 'capabilities' && response.ok) delete response.result.traceRetention;
    return response;
  };
  const inspected = capability.handlerFor(win)({ operation: 'inspect', payload: {} });
  assert.equal(inspected.ok, false);
  assert.equal(inspected.error.code, 'unavailable', 'a wrong bound would be worse than an error');
});

test('a limit outside the declared bounds is ignored, never inverted', () => {
  const win = opacityWindow();
  const layerId = win._f.state.layers[0].layerUid;
  ['g1', 'g2', 'g3', 'g4'].forEach((id, i) => send(win, id, 'property.set', { layerId, property: 'opacity', value: 10 + i }));
  const handler = capability.handlerFor(win);
  const ids = (payload) => handler({ operation: 'inspect', payload }).result.entries.map((e) => e.requestId);

  assert.deepEqual(ids({ limit: 2 }), ['g3', 'g4'], 'a valid limit keeps the most recent');
  // slice(-0) is slice(0): asking for none used to return the WHOLE buffer.
  assert.deepEqual(ids({ limit: 0 }), ids({}), 'limit 0 is below the declared minimum: treated as absent');
  // A negative limit used to drop the OLDEST entries instead of keeping the newest.
  assert.deepEqual(ids({ limit: -3 }), ids({}), 'a negative limit is invalid: treated as absent, never inverted');
  assert.deepEqual(ids({ limit: 1.5 }), ids({}), 'a non-integer is treated as absent');
});

test('routing guard: no trace field is interpolated into the panel markup without esc()', () => {
  // The behavioral tests above can only drive a hostile requestId: `operation`
  // is constrained to opacity-application.js's closed READS/WRITES list, so no
  // test can reach the panel with a hostile one. This guard is what keeps that
  // field's escaping from being silently dropped.
  const code = withoutLineComments(fs.readFileSync(path.join(ROOT, 'src/js/labs/diagnostics-panel.js'), 'utf8'));
  assert.doesNotMatch(code, /\+\s*(req|entry)\.[A-Za-z]/,
    'every req./entry. field must reach the markup through esc(), never by direct interpolation');
  assert.match(code, /esc\(req\.requestId\)/);
  assert.match(code, /esc\(req\.operation\)/);
});

// ---- T12/#1405 — request ids are unique by construction --------------------
//
// Not a repair of an observed collision: nothing was seen in the wild. This is
// correctness of construction. requestId is the application's idempotency key
// (`retained.set(request.requestId, …)`, bounded at 256), and a collision does
// not error — with an identical body it returns the MEMOISED earlier result,
// which for these two read paths means quietly serving a stale trace.
//
// Both tests assert uniqueness across N calls rather than "the string changed",
// because a generator that varies but repeats is exactly the defect: a single
// inequality passes with Math.random() still in place.
//
// Honey's review of #1413 sharpened this, and it is worth stating so nobody
// mistakes which assertion carries the weight: a Set-of-N check does NOT have
// teeth against Math.random() -- among 200 draws a collision is vanishingly
// improbable, so that assertion would pass with the defect in place. Uniqueness
// by CONSTRUCTION is a structural property, so it is asserted structurally: the
// trailing component must be consecutive integers. A random suffix fails that
// deterministically, on every run, independently of the identity prefix.
// The Set check stays as the statement of what the counter is FOR.

const MINT_CALLS = 200;

// The id is `<prefix>:<instanceId>:<counter>`; instanceId is a UUID and carries
// no ':' of its own, so the counter is the last segment. Asserted as a run of
// consecutive integers rather than "all different": the starting value depends
// on how many times the module-scoped counter was already used in this process.
function assertConsecutiveCounters(ids, label) {
  const counters = ids.map((id) => id.slice(id.lastIndexOf(':') + 1));
  assert.ok(counters.every((value) => /^[0-9]+$/.test(value)),
    `${label}: every id must end in an integer counter, not a random component`);
  const numbers = counters.map(Number);
  const expected = numbers.map((_, index) => numbers[0] + index);
  assert.deepEqual(numbers, expected,
    `${label}: the counter must advance by exactly one per mint -- that is what makes the id unique by construction rather than unlikely to repeat`);
}

test('T12: every inspect mints a distinct request id, derived from the application identity', () => {
  const win = opacityWindow();
  const seen = [];
  const inner = win.NemoApplication.handle;
  win.NemoApplication.handle = (request) => { seen.push(request.requestId); return inner(request); };

  const handler = capability.handlerFor(win);
  for (let i = 0; i < MINT_CALLS; i++) {
    assert.equal(handler({ operation: 'inspect', payload: {} }).ok, true, `inspect #${i} failed`);
  }

  const minted = seen.filter((id) => id.startsWith('diagnostics-inspect:'));
  assert.equal(minted.length, MINT_CALLS, 'each inspect reaches diagnostics.trace exactly once');
  assert.equal(new Set(minted).size, MINT_CALLS,
    'a repeated requestId would hit the retained map and return a memoised trace');
  const instanceId = win.NemoOpacityApplication.meta().instanceId;
  assert.ok(minted.every((id) => id.startsWith(`diagnostics-inspect:${instanceId}:`)),
    'the id must be derived from the identity the application itself uses, not from a fresh random source');
  assert.ok(minted.every((id) => id.length <= 128),
    'validate() rejects a requestId longer than 128 characters');
  assertConsecutiveCounters(minted, 'inspect');
});

test('T12: every panel fetch mints a distinct request id — its body is identical each time', () => {
  const { ctx, layerA } = panelWindow();
  const response = ctx.NemoApplication.handle({ apiVersion: 1, requestId: 'seed', ...ctx.NemoOpacityApplication.meta(),
    expectedRevision: ctx.NemoOpacityApplication.meta().revision,
    operation: 'property.set', payload: { layerId: layerA, property: 'opacity', value: 37 } });
  assert.equal(response.ok, true, JSON.stringify(response));

  const seen = [];
  const inner = ctx.NemoApplication.handle;
  ctx.NemoApplication.handle = (request) => { seen.push(request.requestId); return inner(request); };

  // Each enable runs the shipped panel's own fetch; the module-scoped counter
  // survives the cycle, which is the property under test.
  for (let i = 0; i < MINT_CALLS; i++) {
    ctx.SMLabs.enable('diagnostics-panel');
    ctx.SMLabs.disable('diagnostics-panel');
  }

  const minted = seen.filter((id) => id.startsWith('diagnostics-panel:'));
  assert.equal(minted.length, MINT_CALLS, 'each open fetches the trace exactly once');
  assert.equal(new Set(minted).size, MINT_CALLS,
    'a repeated requestId would make the panel render the memoised, stale trace');
  const instanceId = ctx.NemoOpacityApplication.meta().instanceId;
  assert.ok(minted.every((id) => id.startsWith(`diagnostics-panel:${instanceId}:`)),
    'the panel keys off the same application identity as the capability');
  assert.ok(minted.every((id) => id.length <= 128));
  assertConsecutiveCounters(minted, 'panel fetch');
});
