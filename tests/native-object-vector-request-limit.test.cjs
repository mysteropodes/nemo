'use strict';
// Actual SDK and command-route ingress; only the native process boundary is simulated.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const SDK = require('../src/js/adapters/native-application.js');
const Contract = require('../src/js/adapters/native-object-vector-contract.js');
const Command = require('../src/js/adapters/native-object-vector-command.js');
const { createFixture, clone } = require('./native-object-vector-support.cjs');
const target = () => ({ contextId: 'scene-root', frameScope: { kind: 'authored', frame: 3 }, layerUid: 'L', strokeId: 'native-stroke:7:0' });
const envelope = (operation, payload) => ({ apiVersion: 2, requestId: 'project-vector-1', instanceId: 'instance-A', documentId: 'document-A', expectedRevision: 7, operation, payload });
const vector = () => envelope(Contract.VECTOR, { command: 'object.vector.delete', stableTarget: target() });
const parametric = () => envelope(Contract.PARAMETRIC, { command: 'object.parametric.translate', stableTarget: target(), delta: [1, 2] });
const effects = () => envelope(Contract.EFFECTS, { command: 'object.path.effects.set', layerUid: 'L', stack: [] });
function padded(make, size, unicode = '') {
  const q = make(), payload = q.payload, holder = payload.stableTarget || payload;
  holder.layerUid = unicode;
  const padding = size - Buffer.byteLength(JSON.stringify(q));
  assert.ok(padding >= 0);
  holder.layerUid += 'x'.repeat(padding);
  assert.equal(Buffer.byteLength(JSON.stringify(q)), size);
  return q;
}
const refusal = (q, message) => ({ apiVersion: 2, requestId: q.requestId, instanceId: q.instanceId, documentId: q.documentId,
  contentRevision: 7, ok: false, error: { code: 'not_found', message } });
function replyAt(q, size) {
  const r = refusal(q, '');
  r.error.message = 'r'.repeat(size - Buffer.byteLength(JSON.stringify(r)));
  assert.equal(Buffer.byteLength(JSON.stringify(r)), size);
  return r;
}

test('vector full-envelope UTF-8 limit includes 65536 bytes and refuses 65537 before transport', async () => {
  const calls = [];
  const adapter = SDK.createNativeApplicationAdapter('vector-limit', { dispatch: async q => { calls.push(clone(q)); return refusal(q, 'missing'); } });
  for (const bytes of [4097, 65535, 65536]) {
    const q = padded(vector, bytes, 'é😀');
    assert.ok(JSON.stringify(q).length < bytes, 'UTF-8 byte length differs from JS string length');
    assert.equal(SDK.validateRequest(q), q);
    assert.equal((await adapter.dispatch(q)).error.code, 'not_found');
    assert.deepEqual(calls.at(-1), q, 'one intact envelope, including the original opaque identity');
  }
  assert.equal(calls.length, 3);
  await assert.rejects(async () => adapter.dispatch(padded(vector, 65537, 'é😀')), /request exceeds 65536 encoded bytes/);
  assert.equal(calls.length, 3, 'rejected envelope sends nothing');
});

test('parametric, effects and neighboring layer-policy requests keep the inclusive 4096-byte bound', () => {
  const policy = () => envelope('command.document.object.layer.policy.edit', { command: 'object.layer.policy.edit', layerUid: 'L', flag: 'locked', value: true });
  for (const make of [parametric, effects, policy]) {
    const q = padded(make, 4096, 'é😀');
    assert.equal(SDK.validateRequest(q), q);
    assert.throws(() => SDK.validateRequest(padded(make, 4097, 'é😀')), /request exceeds 4096 encoded bytes/);
  }
  assert.equal(Contract.requestByteLimit(Contract.VECTOR), 65536);
  for (const op of [Contract.PARAMETRIC, Contract.EFFECTS, Contract.VECTOR + '.other', 'query.document.object', null]) assert.equal(Contract.requestByteLimit(op), 4096);
});

test('larger vector requests never widen any vector, parametric or path-effect reply', async () => {
  for (const make of [vector, parametric, effects]) {
    const q = make();
    const good = SDK.createNativeApplicationAdapter('reply-limit', { dispatch: async () => replyAt(q, 4096) });
    assert.equal((await good.dispatch(q)).error.code, 'not_found');
    const bad = SDK.createNativeApplicationAdapter('reply-limit', { dispatch: async () => replyAt(q, 4097) });
    await assert.rejects(bad.dispatch(q), /response exceeds 4096 encoded bytes/);
  }
});

test('actual command queue uses the same full-envelope limit and leaves over-budget requests unsent', async () => {
  const fixture = createFixture({ sdk: SDK });
  const command = Command.create(fixture.session, 'vector');
  const q = padded(vector, 65536, 'é😀');
  await assert.rejects(command.execute(q.operation, () => q.payload), err => err.code === 'not_found');
  assert.equal(fixture.owner.log.received.length, 1);
  assert.deepEqual(fixture.owner.log.received[0], q);
  const over = padded(vector, 65537, 'é😀');
  // Serial request 2 has the same length as 1, so the complete envelope remains exactly 65537 bytes.
  await assert.rejects(command.execute(over.operation, () => over.payload), err => err.code === 'request_too_large' && err.limit === 65536 && err.bytes === 65537);
  assert.equal(fixture.owner.log.received.length, 1);
  const ordinary = padded(effects, 4097);
  await assert.rejects(command.execute(ordinary.operation, () => ordinary.payload), err => err.code === 'request_too_large' && err.limit === 4096);
  assert.equal(fixture.owner.log.received.length, 1);
});

test('published vector descriptor and schema state the exact per-operation request and reply budgets', () => {
  const descriptor = JSON.parse(fs.readFileSync(path.join(__dirname, '../engineering/application/capabilities-v2/native-object-vector.json'), 'utf8'));
  assert.deepEqual(descriptor.resourceBoundary.requestBytesByOperation, {
    [Contract.VECTOR]: 65536, [Contract.PARAMETRIC]: 4096, [Contract.EFFECTS]: 4096,
  });
  assert.equal(descriptor.resourceBoundary.maxRequestBytes, 65536);
  assert.equal(descriptor.resourceBoundary.maxReadResponseBytes, 4096);
  const schema = JSON.parse(fs.readFileSync(path.join(__dirname, '../engineering/application/native-transport-v2.schema.json'), 'utf8'));
  assert.match(schema.$defs.ObjectVectorEditPayload.description, /complete original UTF-8 request envelope.*65536/);
});
