'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const core = require('../src/js/application/opacity-application.js');
const domain = require('../src/js/domain/animation/opacity.js');

const clone = value => JSON.parse(JSON.stringify(value));
const opacity = value => ({ layerId: 'layer-a', property: 'opacity', value });

// Exercise the production service and writer with deterministic document/history
// ports. Browser history and render/export are covered by their consumer suites.
function fixture() {
  let sequence = 0, refreshes = 0, writes = 0;
  const state = { currentFrame: 0, totalFrames: 24,
    layers: [{ layerUid: 'layer-a', motionStatic: { opacity: [100] } }] };
  const undo = [], redo = [];
  function restore(from, to) {
    if (!from.length) return false;
    to.push(clone(state.layers)); state.layers = from.pop(); return true;
  }
  const app = core.create({
    newId: () => `replay-${++sequence}`, state: () => state, context: () => 'document',
    canMutate: () => true, snapshot: () => clone(state),
    valueAtFrame: layer => layer.motionStatic.opacity,
    write(operation, layer, payload) {
      assert.equal(operation, 'property.set');
      writes++;
      domain.setValue(layer, [payload.value], state.currentFrame);
    },
    history: {
      checkpoint() { undo.push(clone(state.layers)); redo.length = 0; },
      undo: () => restore(undo, redo), redo: () => restore(redo, undo),
    },
    afterMutation() { refreshes++; },
  });
  function request(id, operation, payload = {}) {
    const identity = app.meta();
    return { apiVersion: 1, requestId: id, ...identity,
      expectedRevision: identity.revision, operation, payload };
  }
  function send(id, operation, payload) {
    const response = app.handle(request(id, operation, payload));
    assert.equal(response.ok, true, JSON.stringify(response));
    return response;
  }
  return { app, state, undo, redo, request, send,
    effects: () => clone({ layers: state.layers, undo, redo, writes, refreshes, identity: app.meta() }) };
}

test('a traced command replays through the shared writer once and uses one undo entry', () => {
  const f = fixture();
  f.send('original', 'property.set', opacity(25));
  const recorded = f.send('trace', 'diagnostics.trace').result.entries[0].request;
  assert.equal(recorded.requestId, 'original');
  f.send('undo-original', 'history.undo');
  assert.equal(f.state.layers[0].motionStatic.opacity[0], 100);
  const replay = f.request('repeat', 'diagnostics.replay', { request: recorded });
  const writesBeforeReplay = f.effects().writes;
  const response = f.app.handle(replay);
  assert.equal(response.ok, true);
  assert.equal(response.result.replay.result.value, 25);
  assert.equal(response.revision, replay.expectedRevision + 1);
  assert.equal(f.effects().writes, writesBeforeReplay + 1, 'one call to the shared writer');
  assert.equal(f.state.layers[0].motionStatic.opacity[0], 25);
  assert.equal(f.undo.length, 1);
  assert.equal(f.redo.length, 0);
  const applied = f.effects();
  assert.deepEqual(f.app.handle(clone(replay)), response);
  assert.deepEqual(f.effects(), applied, 'an identical retry has no additional effects');
  const changed = clone(replay); changed.payload.request.payload.value = 60;
  assert.equal(f.app.handle(changed).error.code, 'invalid_request');
  assert.deepEqual(f.effects(), applied, 'changed-body retry cannot overwrite the replay');
  f.send('undo-replay', 'history.undo');
  assert.equal(f.state.layers[0].motionStatic.opacity[0], 100);
  assert.equal(f.undo.length, 0);
  f.send('redo-replay', 'history.redo');
  assert.equal(f.state.layers[0].motionStatic.opacity[0], 25);
});

const invalidRecords = [
  ['missing record', undefined],
  ['null record', null],
  ['recursive replay', { operation: 'diagnostics.replay', payload: {} }],
  ['history command', { operation: 'history.undo', payload: {} }],
  ['missing property payload', { operation: 'property.set' }],
  ['null property payload', { operation: 'property.set', payload: null }],
  ['array property payload', { operation: 'property.set', payload: [] }],
  ['invalid opacity', { operation: 'property.set', payload: opacity(101) }],
  ['missing layer', { operation: 'property.set', payload: { ...opacity(25), layerId: 'absent' } }],
];
for (const [name, recorded] of invalidRecords) {
  test(`replay rejects ${name} without throwing or changing document/history`, () => {
    const f = fixture(), before = f.effects();
    // JSON serialization matches the native/MCP boundary, including omitted fields.
    const request = clone(f.request('invalid', 'diagnostics.replay', { request: recorded }));
    let response;
    assert.doesNotThrow(() => { response = f.app.handle(request); });
    assert.equal(response.ok, false);
    assert.equal(response.error.code, 'invalid_request');
    assert.deepEqual(f.effects(), before);
    f.send('usable-after-rejection', 'property.set', opacity(30));
    assert.equal(f.state.layers[0].motionStatic.opacity[0], 30);
    assert.equal(f.undo.length, 1);
  });
}

test('stale replay and old-document retries reject before touching the new state', () => {
  const f = fixture();
  const payload = { request: { operation: 'property.set', payload: opacity(20) } };
  const stale = f.request('stale', 'diagnostics.replay', payload);
  f.send('newer', 'property.set', opacity(70));
  const before = f.effects();
  assert.equal(f.app.handle(stale).error.code, 'stale_revision');
  assert.deepEqual(f.effects(), before);
  const replay = f.request('accepted-replay', 'diagnostics.replay', payload);
  assert.equal(f.app.handle(replay).ok, true);
  f.state.layers = [{ layerUid: 'layer-a', motionStatic: { opacity: [90] } }];
  f.undo.length = 0; f.redo.length = 0; f.app.documentChanged();
  const replacement = f.effects();
  assert.equal(f.app.handle(replay).error.code, 'wrong_document');
  assert.deepEqual(f.effects(), replacement);
});

test('trace results are detached and obey the advertised retention bound', () => {
  const f = fixture();
  const capabilities = f.send('capabilities', 'capabilities').result;
  for (let i = 0; i < capabilities.traceRetention + 3; i++) {
    f.send(`edit-${i}`, 'property.set', opacity(i));
  }
  const entries = f.send('trace', 'diagnostics.trace').result.entries;
  assert.equal(entries.length, capabilities.traceRetention);
  assert.equal(entries[0].request.requestId, 'edit-3');
  const last = clone(entries.at(-1));
  entries.at(-1).request.payload.value = 99;
  const again = f.send('trace-again', 'diagnostics.trace').result.entries;
  assert.deepEqual(again.at(-1), last, 'caller edits cannot alter retained commands');
  const identity = f.app.meta();
  f.app.documentChanged();
  assert.notEqual(f.app.meta().documentId, identity.documentId);
  assert.deepEqual(f.send('new-document-trace', 'diagnostics.trace').result.entries, []);
});
