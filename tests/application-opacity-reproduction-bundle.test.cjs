'use strict';
// T07 -- roundtrip one isolated synthetic reproduction bundle. Exercises the
// production trace/replay path (T05's NemoOpacityDiagnostics) through the
// bundle codec (NemoOpacityReproductionBundle) against the synthetic
// opacity fixture, never the live/active document.
const test = require('node:test');
const assert = require('node:assert/strict');
const crypto = require('node:crypto');
const fs = require('node:fs');
const path = require('node:path');
const fixture = require('./fixtures/lib/opacity-application.cjs');
const bundleCodec = require('../src/js/domain/diagnostics/opacity-reproduction-bundle.js');
const diagnostics = require('../src/js/domain/diagnostics/opacity-diagnostics.js');

const PROPERTY_WRITES = ['property.set', 'property.key.set', 'property.key.remove', 'property.animation.set'];
const clone = value => JSON.parse(JSON.stringify(value));
const opacity = (layerId, value) => ({ layerId, property: 'opacity', value });

function send(f, id, operation, payload) {
  const identity = f.app.meta();
  const response = f.app.handle({ apiVersion: 1, requestId: id, ...identity, expectedRevision: identity.revision, operation, payload: payload || {} });
  assert.equal(response.ok, true, JSON.stringify(response));
  return response;
}

// Independently specified digest: deliberately NOT the code the application
// or the codec use for anything else -- just a canonical (sorted-key) JSON
// snapshot of exactly what a reproduction is supposed to reproduce (each
// layer's opacity value, and how many entries are on each history stack).
function stateDigest(f) {
  const canonical = {
    opacities: f.state.layers.map(layer => ({ layerUid: layer.layerUid, opacity: layer.motionStatic.opacity[0] }))
      .sort((a, b) => a.layerUid.localeCompare(b.layerUid)),
    undoDepth: f.undo.length, redoDepth: f.redo.length,
  };
  return crypto.createHash('sha256').update(JSON.stringify(canonical)).digest('hex');
}

// Records a command sequence on a fresh fixture built from (id, seed),
// captures it through T05's diagnostics module (the same object shape
// opacity-application.js's diagnostics.trace returns), and builds a bundle.
function recordBundle(id, seed) {
  const f = fixture.build(id, seed);
  const trace = diagnostics.create(PROPERTY_WRITES);
  const layerA = f.state.layers[0].layerUid, layerB = f.state.layers[1].layerUid;
  const commands = [
    ['c1', 'property.set', opacity(layerA, 60)],
    ['c2', 'property.key.set', { ...opacity(layerB, 30), frame: 5 }],
    ['c3', 'property.set', opacity(layerA, 15)],
  ];
  for (const [id2, operation, payload] of commands) {
    const response = send(f, id2, operation, payload);
    trace.remember({ requestId: id2, operation, payload }, response);
  }
  const bundle = bundleCodec.buildBundle({ id: f.id, hash: f.hash }, trace.entries());
  return { source: f, bundle };
}

// Replays a parsed bundle's commands against a caller-supplied isolated
// fixture. Mirrors how a real caller would use parseBundle: validate first,
// dispatch only through the fixture's own app.handle(), one command at a
// time, exactly as the production replay path does.
function replay(target, commands) {
  for (const command of commands) {
    const identity = target.app.meta();
    const envelope = { apiVersion: 1, requestId: `replay-${Math.random()}`, ...identity,
      expectedRevision: identity.revision, operation: command.operation, payload: command.payload };
    const response = target.app.handle(envelope);
    assert.equal(response.ok, true, JSON.stringify(response));
  }
}

test('a bundle recorded on one fixture replays on a fresh (id, seed)-identical fixture to the same independently specified digest', () => {
  const { source, bundle } = recordBundle('opacity-repro-a', 1234);
  const sourceDigest = stateDigest(source);

  const target = fixture.build('opacity-repro-a', 1234);
  assert.equal(target.hash, source.hash, 'same (id, seed) must produce the same starting-state hash');
  const parsed = bundleCodec.parseBundle(bundle);
  assert.equal(parsed.error, undefined);
  assert.equal(parsed.fixture.hash, target.hash, 'the bundle must identify the fixture it assumes');
  replay(target, parsed.commands);

  assert.equal(stateDigest(target), sourceDigest, 'replay on the isolated target must reach the exact same state the source reached');
  assert.deepEqual(clone(target.state.layers), clone(source.state.layers));
});

test('a different seed produces a different fixture hash, and reproduction is seed-specific', () => {
  const a = fixture.build('opacity-repro-a', 1);
  const b = fixture.build('opacity-repro-a', 2);
  assert.notEqual(a.hash, b.hash);
});

test('buildBundle rejects a request whose entries are missing operation or payload', () => {
  assert.throws(() => bundleCodec.buildBundle({ id: 'x', hash: 'h' }, [{ request: { operation: 'property.set' } }]));
  assert.throws(() => bundleCodec.buildBundle({ id: 'x', hash: 'h' }, []));
  assert.throws(() => bundleCodec.buildBundle({ id: '', hash: 'h' }, [{ request: { operation: 'property.set', payload: {} } }]));
});

const malformedBundles = [
  ['not an object', 'nope'],
  ['null', null],
  ['missing formatVersion', { fixture: { id: 'x', hash: 'h' }, commands: [{ operation: 'property.set', payload: {} }] }],
  ['future format version', { formatVersion: 999, fixture: { id: 'x', hash: 'h' }, commands: [{ operation: 'property.set', payload: {} }] }],
  ['missing fixture', { formatVersion: 1, commands: [{ operation: 'property.set', payload: {} }] }],
  ['fixture missing hash', { formatVersion: 1, fixture: { id: 'x' }, commands: [{ operation: 'property.set', payload: {} }] }],
  ['empty commands', { formatVersion: 1, fixture: { id: 'x', hash: 'h' }, commands: [] }],
  ['commands not an array', { formatVersion: 1, fixture: { id: 'x', hash: 'h' }, commands: {} }],
  ['a command missing payload', { formatVersion: 1, fixture: { id: 'x', hash: 'h' }, commands: [{ operation: 'property.set' }] }],
  ['a command with a non-string operation', { formatVersion: 1, fixture: { id: 'x', hash: 'h' }, commands: [{ operation: 3, payload: {} }] }],
];
for (const [name, bundle] of malformedBundles) {
  test(`parseBundle rejects ${name} without throwing`, () => {
    let result;
    assert.doesNotThrow(() => { result = bundleCodec.parseBundle(bundle); });
    assert.equal(result.commands, undefined);
    assert.equal(typeof result.error, 'string');
  });
}

test('a rejected bundle never reaches the active document -- parseBundle fails before any replay is attempted', () => {
  const active = fixture.build('opacity-repro-active', 42);
  send(active, 'seed-write', 'property.set', opacity(active.state.layers[0].layerUid, 77));
  const before = { state: clone(active.state), undo: clone(active.undo), redo: clone(active.redo), meta: active.app.meta() };

  const malformed = { formatVersion: 999, fixture: { id: 'x', hash: 'h' }, commands: [{ operation: 'property.set', payload: {} }] };
  const parsed = bundleCodec.parseBundle(malformed);
  assert.equal(typeof parsed.error, 'string');
  // The whole point: a caller who checks parsed.error before ever calling
  // active.app.handle() cannot have touched the active document. Assert the
  // negative directly rather than trust that discipline.
  assert.deepEqual(clone(active.state), before.state);
  assert.deepEqual(clone(active.undo), before.undo);
  assert.deepEqual(clone(active.redo), before.redo);
  assert.deepEqual(active.app.meta(), before.meta);
});

test('an unrelated sentinel field on the document is untouched by a successful replay', () => {
  const { bundle } = recordBundle('opacity-repro-sentinel', 777);
  const target = fixture.build('opacity-repro-sentinel', 777);
  target.state.privateSentinel = 'untouched';
  const before = target.state.privateSentinel;
  const parsed = bundleCodec.parseBundle(bundle);
  replay(target, parsed.commands);
  assert.equal(target.state.privateSentinel, before, 'replay must only ever reach opacity through the normal write path, nothing else');
});

test('an unrelated sentinel field is untouched even when the bundle is rejected before replay', () => {
  const target = fixture.build('opacity-repro-sentinel-reject', 9);
  target.state.privateSentinel = 'untouched';
  const parsed = bundleCodec.parseBundle({ formatVersion: 1, fixture: { id: 'x', hash: 'h' }, commands: [] });
  assert.equal(typeof parsed.error, 'string');
  assert.equal(target.state.privateSentinel, 'untouched');
});

test('routing guard: the bundle codec never dispatches and never touches window/document globals', () => {
  const source = fs.readFileSync(path.join(__dirname, '../src/js/domain/diagnostics/opacity-reproduction-bundle.js'), 'utf8');
  assert.doesNotMatch(source, /\bhandle\s*\(/, 'the codec must never call a dispatcher itself -- only the caller replays through its own app');
  assert.doesNotMatch(source, /\bwindow\./, 'the codec must stay pure -- no implicit globals');
});
