'use strict';
// Actual JS contract/mapper/SDK/client; observed transport replies are NOT native execution evidence.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const C = require('../src/js/adapters/native-object-vector-contract.js');
const Stroke = require('../src/js/adapters/native-object-vector-stroke.js');
const Vector = require('../src/js/adapters/native-object-vector.js');
const SDK = require('../src/js/adapters/native-application.js');
const { forbidLegacyWriters } = require('./native-object-vector-support.cjs');
const clone = (value) => structuredClone(value);
const oracle = JSON.parse(fs.readFileSync(path.join(__dirname, 'native-object-vector-fixtures/stroke-oracle.json'), 'utf8'));
const dict = () => clone(oracle.cases.find((row) => row.id === 'open-line').in);
const at = { layerUid: 'L1', frame: 3, strokeId: 'mask-a' };
const stable = () => ({ contextId: 'scene-root', layerUid: 'L1', strokeId: 'mask-a', frameScope: { kind: 'authored', frame: 3 } });
const payload = (maskEdit) => ({ command: 'object.vector.mask.edit', stableTarget: stable(), maskEdit });
const request = (maskEdit) => ({ apiVersion: 2, requestId: 'mask-7', instanceId: 'instance-A', documentId: 'document-A',
  expectedRevision: 7, operation: C.VECTOR, payload: payload(maskEdit) });
const success = (q, applied = true) => ({ apiVersion: 2, requestId: q.requestId, instanceId: q.instanceId, documentId: q.documentId,
  contentRevision: q.expectedRevision + Number(applied), ok: true,
  result: { applied, historyEntriesAdded: Number(applied), operation: 'mask.edit', strokeId: 'mask-a' } });
const maskTags = (value) => Object.fromEntries(['isMask', 'maskMode', 'maskFeather'].filter((key) => Object.hasOwn(value, key)).map((key) => [key, value[key]]));

// This bounded observer has no record store or mask implementation. It accepts
// only the explicitly named command and emits its preselected transport receipt.
// In particular it never sends mask.edit to ownerDouble's delete-style else branch.
function observedSession(options = {}) {
  const seen = [], reconciled = [];
  let revision = 7, serial = 0, routes = 0;
  const command = options.command || 'object.vector.mask.edit';
  const info = () => ({ instanceId: 'instance-A', documentId: 'document-A', lifecycleGeneration: 3,
    contentRevision: revision, documentSnapshotId: 'snapshot-' + revision });
  const transport = SDK.createNativeApplicationAdapter('mask-observer', { dispatch(q) {
    assert.equal(q.operation, C.VECTOR);
    assert.equal(q.payload.command, command);
    C.payload(q.operation, q.payload);
    seen.push(clone(q));
    const applied = options.applied === undefined ? true : options.applied;
    let reply;
    if (command === 'object.vector.mask.edit') reply = success(q, applied);
    else if (command === 'object.vector.create') reply = { ...success(q),
      result: { applied: true, historyEntriesAdded: 1, operation: 'create', strokeId: 'native-stroke:8:0' } };
    else assert.fail('unexpected observer command');
    revision = reply.contentRevision;
    if (options.transform) reply = options.transform(clone(reply), q);
    return Object.freeze(reply);
  } });
  const session = {
    isActive: () => options.active !== false,
    info,
    requestId: () => 'observed-mask-' + (++serial),
    runResourceCommand(action) {
      routes += 1;
      return action({ dispatch: (q) => transport.dispatch(q), reconcile: async (pin) => { reconciled.push(pin); } });
    },
  };
  if (options.noRoute) delete session.runResourceCommand;
  return { seen, reconciled, session, info, routes: () => routes, vector: Vector.create(session) };
}

test('stored mask tags retain absence null false zero negative and arbitrary spelling through real mapping and validation', () => {
  for (const tags of [{}, { isMask: null }, { isMask: false }, { isMask: true }, { maskMode: null },
    { maskMode: '' }, { maskMode: 'future-mode'.repeat(70) }, { maskFeather: null }, { maskFeather: 0 },
    { maskFeather: -3 }, { isMask: true, maskMode: 'intersect', maskFeather: 2.5 }]) {
    const source = Object.assign(dict(), tags), before = clone(source);
    const mapped = Stroke.toRecord(source, at);
    assert.equal(mapped.ok, true);
    assert.deepEqual(maskTags(mapped.record.vectorExt || {}), tags);
    assert.equal(mapped.unmapped.some((key) => ['isMask', 'maskMode', 'maskFeather'].includes(key)), false);
    C.record(mapped.record, false);
    assert.deepEqual(maskTags(Stroke.toDict(mapped.record)), tags);
    const expected = Stroke.toRecord(dict(), at).record;
    assert.deepEqual(mapped.record.geometry, expected.geometry);
    assert.deepEqual(mapped.record.vector, expected.vector);
    assert.deepEqual(mapped.record.fill, expected.fill);
    assert.deepEqual(source, before);
  }
});

test('malformed mask tags are named and never silently dropped by the mapper or public record validator', () => {
  for (const tags of [{ isMask: 1 }, { isMask: 'true' }, { isMask: undefined }, { maskMode: 7 },
    { maskMode: [] }, { maskFeather: '2' }, { maskFeather: false }, { maskFeather: NaN }, { maskFeather: Infinity }]) {
    const mapped = Stroke.toRecord(Object.assign(dict(), tags), at);
    assert.equal(mapped.ok, false);
    assert.equal(mapped.code, 'invalid_dict');
    const record = Stroke.toRecord(dict(), at).record;
    record.vectorExt = tags;
    assert.throws(() => C.record(record, false), TypeError);
  }
  const unknown = Stroke.toRecord(dict(), at).record;
  unknown.vectorExt = { isMask: true, unownedMaskCache: [] };
  assert.throws(() => C.record(unknown, false), TypeError);
});

test('mode feather and unset have exact closed existing-operation envelopes', () => {
  assert.equal(C.operationOf('object.vector.mask.edit'), C.VECTOR);
  assert.deepEqual(C.members('object.vector.mask.edit'), ['stableTarget', 'maskEdit']);
  assert.equal(C.requestByteLimit(C.VECTOR), 65536);
  for (const intent of [{ kind: 'mode', mode: 'add' }, { kind: 'mode', mode: 'subtract' },
    { kind: 'mode', mode: 'intersect' }, { kind: 'feather', value: 0 }, { kind: 'feather', value: -2 }, { kind: 'unset' }]) {
    const p = payload(intent), before = clone(p);
    C.payload(C.VECTOR, p);
    SDK.validateRequest(request(intent));
    assert.deepEqual(p, before);
    for (const key of ['command', 'stableTarget', 'maskEdit']) {
      const missing = clone(p); delete missing[key];
      assert.throws(() => C.payload(C.VECTOR, missing), TypeError);
    }
    assert.throws(() => C.payload(C.VECTOR, { ...p, layerMasks: [] }), TypeError);
    assert.throws(() => C.payload(C.PARAMETRIC, p), TypeError);
  }
  for (const intent of [null, [], {}, { kind: 'mode' }, { kind: 'mode', mode: 'future' },
    { kind: 'feather', value: null }, { kind: 'feather', value: '2' }, { kind: 'feather', value: NaN },
    { kind: 'unset', value: 0 }, { kind: 'feather', value: 2, mode: 'add' }]) {
    assert.throws(() => C.payload(C.VECTOR, payload(intent)), TypeError);
  }
});

test('real common SDK accepts correlated changed and noop receipts and rejects foreign or stale success', async () => {
  const q = request({ kind: 'mode', mode: 'subtract' });
  for (const applied of [false, true]) {
    const reply = success(q, applied);
    const api = SDK.createNativeApplicationAdapter('mask-receipt', { dispatch: () => reply });
    assert.deepEqual(await api.dispatch(q), reply);
  }
  for (const mutate of [r => { r.requestId = 'foreign'; }, r => { r.instanceId = 'foreign'; },
    r => { r.documentId = 'foreign'; }, r => { r.contentRevision = 7; },
    r => { r.result.operation = 'delete'; }, r => { r.result.strokeId = 'other'; },
    r => { r.result.historyEntriesAdded = 0; }]) {
    const reply = success(q); mutate(reply);
    const api = SDK.createNativeApplicationAdapter('mask-foreign', { dispatch: () => reply });
    await assert.rejects(api.dispatch(q), TypeError);
  }
});

test('typed owner replacement and stale refusals remain correlated without blessing foreign successes', async () => {
  const q = request({ kind: 'feather', value: 4 });
  for (const code of ['wrong_instance', 'wrong_document', 'stale_revision']) {
    const reply = { apiVersion: 2, requestId: q.requestId,
      instanceId: code === 'wrong_instance' ? 'actual-instance' : q.instanceId,
      documentId: code === 'stale_revision' ? q.documentId : 'actual-document', contentRevision: 19,
      ok: false, error: { code, message: 'Native mask edit refused.' } };
    const api = SDK.createNativeApplicationAdapter('mask-refusal', { dispatch: () => reply });
    assert.deepEqual(await api.dispatch(q), reply);
  }
});

test('actual editMask client routes each intent through real SDK with current pins and never a legacy writer', async () => {
  const trap = forbidLegacyWriters(['state', 'userLayers', 'selectedPaths', 'saveActiveLayerFrame', 'saveAllLayerFrames', 'pushUndo', 'loadFrame', 'Paper', 'SMEngineBridge']);
  try {
    const observed = observedSession();
    const intents = [{ kind: 'mode', mode: 'subtract' }, { kind: 'feather', value: -4 }, { kind: 'unset' }];
    for (const intent of intents) {
      const reply = await observed.vector.editMask(at, intent);
      assert.equal(reply.status, 'applied');
      assert.equal(reply.operation, 'object.vector.mask.edit');
      assert.equal(reply.strokeId, 'mask-a');
      assert.equal(reply.receipt.operation, 'mask.edit');
    }
    assert.deepEqual(observed.seen.map((q, i) => ({ operation: q.operation, expectedRevision: q.expectedRevision,
      instanceId: q.instanceId, documentId: q.documentId, payload: q.payload })), intents.map((intent, i) => ({
      operation: C.VECTOR, expectedRevision: 7 + i, instanceId: 'instance-A', documentId: 'document-A', payload: payload(intent) })));
    assert.deepEqual(observed.reconciled, [8, 9, 10]);
    assert.equal(observed.routes(), 3);
    assert.deepEqual(trap.touched, []);
  } finally { trap.restore(); }
});

test('actual client create carries mask metadata in the original vector record without replacing its paint', async () => {
  const observed = observedSession({ command: 'object.vector.create' });
  const source = Object.assign(dict(), { isMask: true, maskMode: 'subtract', maskFeather: 3 });
  const result = await observed.vector.createStroke('L1', 3, source);
  assert.equal(result.status, 'applied');
  assert.equal(result.strokeId, 'native-stroke:8:0');
  const sent = observed.seen[0].payload.record;
  assert.deepEqual(sent.vectorExt, { isMask: true, maskMode: 'subtract', maskFeather: 3 });
  const plain = Stroke.toRecord(dict(), { layerUid: 'L1', frame: 3, strokeId: '' }).record;
  assert.deepEqual(sent.geometry, plain.geometry);
  assert.deepEqual(sent.vector, plain.vector);
  assert.deepEqual(sent.fill, plain.fill);
});

test('actual client keeps noop and malformed/stale/unavailable outcomes without sending invalid intents', async () => {
  const noop = observedSession({ applied: false });
  assert.equal((await noop.vector.editMask(at, { kind: 'mode', mode: 'subtract' })).status, 'noop');
  assert.deepEqual(noop.reconciled, [7]);
  const invalid = observedSession();
  for (const intent of [{ kind: 'mode', mode: 'future' }, { kind: 'unset', value: 0 }, { kind: 'feather', value: Infinity }]) {
    assert.equal((await invalid.vector.editMask(at, intent)).status, 'invalid');
  }
  assert.equal((await invalid.vector.editMask({ ...at, strokeId: '' }, { kind: 'unset' })).status, 'invalid');
  const stale = { instanceId: 'instance-A', documentId: 'document-A', contentRevision: 6, documentSnapshotId: 'snapshot-6' };
  assert.equal((await invalid.vector.editMask(at, { kind: 'unset' }, { origin: stale })).status, 'stale');
  assert.deepEqual(invalid.seen, []);
  assert.equal((await Vector.create(null).editMask(at, { kind: 'unset' })).status, 'unavailable');
  for (const options of [{ active: false }, { noRoute: true }]) {
    const unavailable = observedSession(options);
    assert.equal((await unavailable.vector.editMask(at, { kind: 'unset' })).status, 'unavailable');
    assert.deepEqual(unavailable.seen, []);
  }
});
