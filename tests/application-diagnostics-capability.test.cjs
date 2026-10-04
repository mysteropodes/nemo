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

// Native panel behavior and T12 counter oracles: native-diagnostics-query.test.cjs.

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
  assert.match(code, /esc\(entry\.requestIdRedacted/);
  assert.match(code, /esc\(entry\.operation\)/);
});

// ---- T12/#1405 — request ids are unique by construction --------------------
//
// Not a repair of an observed collision: nothing was seen in the wild. This is
// correctness of construction. requestId is the application's idempotency key,
// and for these two READ paths a collision costs a spurious hard failure, not
// silent staleness — `remember()` stores WRITES only (opacity-application.js:61)
// while the lookup is unconditional (:136), so a read id equal to one of the
// up-to-256 retained write ids fails `invalid_request`. Probed both ways
// against the real core; an earlier revision of this comment claimed the
// memoised-stale-trace mechanism, which is wrong for a read.
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

// The id is `<prefix>:<identity hint>:<counter>`; the hint may contain ':',
// so only the final segment is the counter. Asserted as a run of
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
    'the counter must make every minted id distinct -- reads are never retained, so this pins the construction rather than guarding a reachable failure');
  const instanceId = win.NemoOpacityApplication.meta().instanceId;
  assert.ok(minted.every((id) => id.startsWith(`diagnostics-inspect:${instanceId.slice(0, 64)}:`)),
    'the id must be derived from the identity the application itself uses, not from a fresh random source');
  assert.ok(minted.every((id) => id.length <= 128),
    'validate() rejects a requestId longer than 128 characters');
  assertConsecutiveCounters(minted, 'inspect');
});

// Pins the claim the two comments above rest on, so it cannot rot into folklore:
// The v1 capability retains its wider identity contract; native panel coverage
// now lives in native-diagnostics-query.test.cjs with native's 128-byte IDs.
test('T12 capability preserves full long identities, clipped-hint uniqueness and exhaustion', () => {
  function load(counter) {
    const context = vm.createContext({});
    const source = fs.readFileSync(path.join(ROOT, 'src/js/application/diagnostics-capability.js'), 'utf8');
    vm.runInContext(source.replace('var minted = 0;', `var minted = ${counter};`), context);
    return context.NemoDiagnosticsCapability;
  }
  const module = load(0), seen = [];
  for (const length of [36, 120, 128, 512]) {
    const win = opacityWindow(), identity = 'identity:'.padEnd(length, 'x');
    assert.equal(win.NemoOpacityApplication.setInstanceId(identity).ok, true);
    const inner = win.NemoApplication.handle;
    win.NemoApplication.handle = (request) => {
      assert.equal(request.instanceId, identity);
      if (request.operation === 'diagnostics.trace') seen.push(request.requestId);
      return inner(request);
    };
    assert.equal(module.handlerFor(win)({ operation: 'inspect', payload: {} }).ok, true);
  }
  assert.equal(new Set(seen).size, 4); assertConsecutiveCounters(seen, 'capability');
  assert.ok(seen.every((id) => id.length <= 128));
  const exhausted = load(Number.MAX_SAFE_INTEGER - 1), win = opacityWindow();
  assert.equal(exhausted.handlerFor(win)({ operation: 'inspect', payload: {} }).ok, true);
  win.NemoApplication.handle = () => { throw new Error('exhausted counter dispatched'); };
  for (let i = 0; i < 2; i++) assert.match(exhausted.handlerFor(win)({ operation: 'inspect', payload: {} }).error.message, /counter exhausted/);
});

// what a requestId collision costs on a READ path. Probed rather than reasoned.
test('T12: a read is never retained, so a collision costs a hard failure and not a stale trace', () => {
  const win = opacityWindow();
  const layerId = win._f.state.layers[0].layerUid;
  const call = (id, operation, payload) => {
    const identity = win.NemoOpacityApplication.meta();
    return win.NemoApplication.handle({ apiVersion: 1, requestId: id, ...identity,
      expectedRevision: identity.revision, operation, payload });
  };

  // One id, two DIFFERENT read bodies: both execute. If reads were retained,
  // the second would fail 'requestId was reused with a changed body'.
  assert.equal(call('same-read-id', 'diagnostics.trace', {}).ok, true);
  assert.equal(call('same-read-id', 'snapshot', {}).ok, true,
    'reads are not stored in the retained map, so repeating a read id is not itself a hazard');

  // A read whose id collides with a retained WRITE id is what actually breaks:
  // the lookup is unconditional, the bodies differ, and it fails.
  assert.equal(call('W', 'property.set', { layerId, property: 'opacity', value: 12 }).ok, true);
  const clash = call('W', 'diagnostics.trace', {});
  assert.equal(clash.ok, false, 'a read reusing a retained write id must not be served');
  assert.equal(clash.error.code, 'invalid_request');

  // And a read does not grow the trace, so nothing here is revision-driven.
  const before = call('t-a', 'diagnostics.trace', {}).result.entries.length;
  call('t-b', 'snapshot', {});
  assert.equal(call('t-c', 'diagnostics.trace', {}).result.entries.length, before,
    'reads are not recorded, so a revision-keyed read id would not go stale either');
});
