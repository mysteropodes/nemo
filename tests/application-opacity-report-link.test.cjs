'use strict';
// T10/#1399 -- the report link: the recorded pre-state that makes a trace
// window into a REPRODUCIBLE bundle.
//
// The property under test is not "a bundle is produced" but "the bundle says
// what it actually replays from". T07's buildBundle identifies a bundle by
// {id, hash of its STARTING state}; before T10 the trace kept only
// {request, revision, ok}, so no honest hash existed and the only way to
// produce one would have been to hash the current state and mislabel it.

const test = require('node:test');
const assert = require('node:assert/strict');
const crypto = require('node:crypto');
const fixture = require('./fixtures/lib/opacity-application.cjs');
const diagnostics = require('../src/js/domain/diagnostics/opacity-diagnostics.js');
const bundleCodec = require('../src/js/domain/diagnostics/opacity-reproduction-bundle.js');

const sha = (value) => crypto.createHash('sha256').update(JSON.stringify(value)).digest('hex');

function drive(f, id, value) {
  const identity = f.app.meta();
  return f.app.handle({
    apiVersion: 1, requestId: id, ...identity, expectedRevision: identity.revision,
    operation: 'property.set', payload: { layerId: f.state.layers[0].layerUid, property: 'opacity', value },
  });
}
function traceOf(f) {
  const identity = f.app.meta();
  return f.app.handle({ apiVersion: 1, requestId: 'read-trace', ...identity,
    expectedRevision: identity.revision, operation: 'diagnostics.trace', payload: {} }).result.entries;
}

// ---- the fixture equivalence that makes a report verifiable --------------

test('the oldest retained pre-state hashes to exactly the fixture hash it started from', () => {
  const f = fixture.build('t10-equiv', 55);
  drive(f, 'c1', 11);
  drive(f, 'c2', 22);
  const entries = traceOf(f);
  assert.equal(entries.length, 2);
  assert.equal(sha(entries[0].stateBefore), f.hash,
    'a report built from this window must carry the same fixture hash a synthetic fixture would');
  assert.notEqual(sha(entries[1].stateBefore), f.hash, 'the second command starts from a different state');
});

// ---- the reason pre-state is per entry rather than per buffer ------------

test('after eviction the window reports the state its OLDEST SURVIVING command assumed', () => {
  const f = fixture.build('t10-evict', 7);
  const layerId = f.state.layers[0].layerUid;
  for (let n = 0; n < 40; n++) drive(f, `w${n}`, n % 101);
  const entries = traceOf(f);
  assert.equal(entries.length, diagnostics.LIMIT, 'the ring still bounds the trace');
  assert.equal(entries[0].request.requestId, 'w8', 'w0..w7 were evicted');
  assert.notEqual(sha(entries[0].stateBefore), f.hash,
    'a buffer-wide starting state would still claim the original fixture here -- that is the bug this design avoids');
  const opacityIn = (s) => s.layers.find((l) => l.layerUid === layerId).motionStatic.opacity[0];
  assert.equal(opacityIn(entries[0].stateBefore), 7,
    'the surviving window starts from what the evicted predecessor w7 left behind');
});

// ---- startingState() is the window's start, and never a guess ------------

test('startingState() tracks the surviving window and is undefined when nothing was recorded', () => {
  const recorded = diagnostics.create(['property.set']);
  assert.equal(recorded.startingState(), undefined, 'an empty trace has no starting state');

  recorded.remember({ requestId: 'a', operation: 'property.set', payload: {} }, { revision: 1, ok: true }, { at: 'before-a' });
  recorded.remember({ requestId: 'b', operation: 'property.set', payload: {} }, { revision: 2, ok: true }, { at: 'before-b' });
  assert.deepEqual(recorded.startingState(), { at: 'before-a' });

  const legacy = diagnostics.create(['property.set']);
  legacy.remember({ requestId: 'a', operation: 'property.set', payload: {} }, { revision: 1, ok: true });
  assert.equal(legacy.startingState(), undefined,
    'a caller that records no pre-state must get undefined, never a plausible-looking wrong answer');
  assert.equal(legacy.entries()[0].stateBefore, undefined, 'and the entry carries no invented field');
});

test('remember() copies the pre-state it is handed, so a live caller object cannot rewrite history', () => {
  // The application happens to pass a fresh clone today, which is why storing
  // the argument by reference still looks correct end-to-end. It is not: any
  // other caller handing over live state would see its recorded history
  // change underneath it. Guard the copy where the copy is made.
  const recorded = diagnostics.create(['property.set']);
  const live = { layers: [{ opacity: 1 }] };
  recorded.remember({ requestId: 'a', operation: 'property.set', payload: {} }, { revision: 1, ok: true }, live);
  live.layers[0].opacity = 999;
  assert.equal(recorded.startingState().layers[0].opacity, 1, 'the trace kept what was recorded, not a live alias');
  assert.equal(recorded.entries()[0].stateBefore.layers[0].opacity, 1);
});

test('the recorded pre-state is a copy: mutating the trace never reaches the document', () => {
  const f = fixture.build('t10-iso', 3);
  drive(f, 'c1', 11);
  const entries = traceOf(f);
  entries[0].stateBefore.layers[0].motionStatic.opacity[0] = 999;
  assert.notEqual(traceOf(f)[0].stateBefore.layers[0].motionStatic.opacity[0], 999,
    'entries() hands out clones; a caller cannot write back through a report');
});

// ---- the bundle the panel would produce ----------------------------------

test('a bundle built from the recorded window carries a fixture the codec accepts', () => {
  const f = fixture.build('t10-bundle', 9);
  drive(f, 'c1', 11);
  drive(f, 'c2', 22);
  const entries = traceOf(f);
  const bundle = bundleCodec.buildBundle({ id: f.app.meta().documentId, hash: sha(entries[0].stateBefore) }, entries, null);
  const parsed = bundleCodec.parseBundle(bundle);
  assert.equal(parsed.error, undefined, 'the codec must accept what the panel produces');
  assert.deepEqual(parsed.commands.map((c) => c.payload.value), [11, 22]);
  assert.equal(parsed.fixture.hash, f.hash, 'and the bundle names the state it truly replays from');
});
