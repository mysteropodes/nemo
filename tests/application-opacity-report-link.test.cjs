'use strict';
// T10 source-stage safety. Native opt-in reproduction is T10A/T08B; a legacy
// trace or user-state hash is not a recoverable synthetic fixture.
const test = require('node:test');
const assert = require('node:assert/strict');
const fixture = require('./fixtures/lib/opacity-application.cjs');
const diagnostics = require('../src/js/domain/diagnostics/opacity-diagnostics.js');

function drive(f, id, value) {
  const identity = f.app.meta();
  return f.app.handle({ apiVersion: 1, requestId: id, ...identity, expectedRevision: identity.revision,
    operation: 'property.set', payload: { layerId: f.state.layers[0].layerUid, property: 'opacity', value } });
}
function traceOf(f) {
  return f.app.handle({ apiVersion: 1, requestId: 'read-trace', ...f.app.meta(),
    operation: 'diagnostics.trace', payload: {} }).result.entries;
}

test('ordinary writes never retain the 50 KB private document sentinel', () => {
  const f = fixture.build('t10-private', 55);
  const sentinel = '/private/artist/assets/DO-NOT-EXPORT-' + 'x'.repeat(50000);
  f.state.privateAsset = sentinel;
  assert.equal(drive(f, 'c1', 11).ok, true);
  const entries = traceOf(f);
  assert.equal(Object.hasOwn(entries[0], 'stateBefore'), false);
  assert.equal(JSON.stringify(entries).includes('DO-NOT-EXPORT'), false);
  assert.ok(JSON.stringify(entries).length < 2048, 'trace size must not scale with document data');
  assert.equal(f.state.privateAsset, sentinel);
});

test('cyclic document state does not wedge the edit guard or a following valid write', () => {
  const f = fixture.build('t10-cycle', 7);
  f.state.privateCycle = f.state;
  let first, thrown;
  try { first = drive(f, 'c1', 11); } catch (error) { thrown = error; }
  delete f.state.privateCycle;
  const next = drive(f, 'c2', 22);
  assert.equal(next.ok, true, 'a failed diagnostic serialization must never retain edit ownership');
  assert.equal(thrown, undefined, 'a write must not serialize arbitrary root state');
  assert.equal(first.ok, true);
  assert.equal(f.state.layers[0].motionStatic.opacity[0], 22);
});

test('diagnostics never traverses an unrelated document getter', () => {
  const f = fixture.build('t10-getter', 3);
  Object.defineProperty(f.state, 'privateAsset', { enumerable: true, get() { throw new Error('private getter read'); } });
  assert.equal(drive(f, 'c1', 11).ok, true);
  assert.equal(drive(f, 'c2', 22).ok, true);
});

test('the diagnostics boundary ignores arbitrary state arguments and preserves detached entries', () => {
  const recorded = diagnostics.create(['property.set']);
  const privateState = { privateAsset: '/private/sentinel' };
  privateState.self = privateState;
  const request = { requestId: 'a', operation: 'property.set', payload: { value: 25 } };
  assert.doesNotThrow(() => recorded.remember(request, { revision: 1, ok: true }, privateState));
  const entries = recorded.entries();
  assert.deepEqual(Object.keys(entries[0]).sort(), ['ok', 'request', 'revision']);
  entries[0].request.payload.value = 99;
  assert.equal(recorded.entries()[0].request.payload.value, 25);
});

// Panel Report and lifecycle fences now exercise the native service in native-diagnostics-query.test.cjs.
