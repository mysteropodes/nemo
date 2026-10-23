'use strict';
// L01-J through the REAL common v2 SDK (native-application.js with the composed contracts). Needs the composed candidate
// (the SDK patch and the new contract). The transport double stands only for the process boundary: every request and every
// response still crosses the real request/response validators, so nothing about validation is mocked.
const test = require('node:test');
const assert = require('node:assert/strict');
const SDK = require('../src/js/adapters/native-application.js');
const Vector = require('../src/js/adapters/native-object-vector.js');
const Contract = require('../src/js/adapters/native-object-vector-contract.js');
const { createFixture, clone } = require('./native-object-vector-support.cjs');

const seg = (x, y) => ({ point: { x, y }, handleIn: { x: 0, y: 0 }, handleOut: { x: 0, y: 0 } });
const target = (strokeId, kind = 'authored') => ({ contextId: 'scene-root', frameScope: { kind, frame: 3 }, layerUid: 'L1', strokeId });
const vectorRecord = (strokeId, kind) => ({
  schemaVersion: 1, family: 'editable-vector-path', target: target(strokeId, kind),
  geometry: { kind: 'cubic-path', closed: false, coordinateSpace: 'document-world', handleSpace: 'relative', segments: [seg(10, 20), seg(110, 70)] },
  fill: null,
  vector: { stroke: { kind: 'solid', r: 0, g: 0.5, b: 1, a: 1 }, hasRealStroke: true, strokeWidth: 4, strokeCap: 'round', strokeJoin: 'round', opacity: 1 },
});
const legacyRecord = () => ({
  schemaVersion: 1, family: 'closed-solid-fill-cubic-path', target: target('s1'),
  geometry: { kind: 'cubic-path', closed: true, coordinateSpace: 'document-world', handleSpace: 'relative', segments: [seg(0, 0), seg(10, 0), seg(10, 10)] },
  fill: { kind: 'solid', r: 1, g: 0, b: 0, a: 1 },
});
const request = (operation, payload, revision = 7) => ({ apiVersion: 2, requestId: 'vector-7', instanceId: 'instance-A', documentId: 'document-A', operation, expectedRevision: revision, payload });
const reply = (q, contentRevision, result) => ({ apiVersion: 2, requestId: q.requestId, instanceId: q.instanceId, documentId: q.documentId, contentRevision, ok: true, result });
const sdk = (respond) => SDK.createNativeApplicationAdapter('vector-public', { dispatch: async (q) => respond(q) });
const createRequest = () => request(Contract.VECTOR, { command: 'object.vector.create', record: vectorRecord('') });
const receipt = { applied: true, historyEntriesAdded: 1, operation: 'create', strokeId: 'native-stroke:8:0' };

test('the three operations are admitted by the SDK and the font, item and edit contracts still are', () => {
  for (const q of [createRequest(),
    request(Contract.VECTOR, { command: 'object.vector.delete', stableTarget: target('native-stroke:8:0') }),
    request(Contract.PARAMETRIC, { command: 'object.parametric.translate', stableTarget: target('native-stroke:8:0'), delta: [1, 2] }),
    request(Contract.EFFECTS, { command: 'object.path.effects.set', layerUid: 'L1', stack: [{ type: 'zigzag' }] }),
    // composed neighbours: the lane-2 group family (edit seam), the lane-6 item family, the F06 policy command
    request('command.document.object.group.rename', { command: 'object.group.rename', layerUid: 'L1', contextId: 'scene-root', groupId: 'g1', name: 'n' }),
    request('command.document.object.layer.policy.edit', { command: 'object.layer.policy.edit', layerUid: 'L1', flag: 'locked', value: true }),
  ]) assert.equal(SDK.validateRequest(q), q, q.operation);
  const item = request('command.document.object.item.edit', { command: 'object.raster.delete', stableTarget: target('native-raster:3:0') });
  assert.equal(SDK.validateRequest(item), item);
  assert.throws(() => SDK.validateRequest(request('command.document.object.vector.create', {})), /operation is not admitted/);
});

test('a vector request stays closed and revisioned while admitting complete records above 4096 bytes', () => {
  const q = createRequest();
  const noRevision = clone(q); delete noRevision.expectedRevision;
  assert.throws(() => SDK.validateRequest(noRevision), /expectedRevision presence/);
  assert.throws(() => SDK.validateRequest(Object.assign(clone(q), { extra: 1 })), /missing or unknown fields/);
  const widened = clone(q); widened.payload.record.vector.strokeGradient = {};
  assert.throws(() => SDK.validateRequest(widened), TypeError);
  const named = clone(q); named.payload.record.target.strokeId = 'native-stroke:1:0';
  assert.throws(() => SDK.validateRequest(named), /empty for a create/);
  const big = clone(q); big.payload.record.geometry.segments = Array.from({ length: 200 }, (_, i) => seg(i, i));
  assert.ok(Contract.bytes(JSON.stringify(big)) > 4096);
  assert.equal(SDK.validateRequest(big), big);
  const excessive = clone(q); excessive.payload.record.target.layerUid = 'L'.repeat(65536);
  assert.throws(() => SDK.validateRequest(excessive), /request exceeds 65536 encoded bytes/);
  // identities may legitimately start with data:/blob:, everything else keeps the raw-payload guard
  const opaque = request(Contract.VECTOR, { command: 'object.vector.delete', stableTarget: Object.assign(target('data:stroke'), { layerUid: 'blob:layer' }) });
  assert.equal(SDK.validateRequest(opaque), opaque);
  const hidden = clone(q); hidden.payload.record.vector.paintOrder = 'data:text/plain';
  assert.throws(() => SDK.validateRequest(hidden), TypeError);
});

test('the receipt is validated and correlated by the SDK: owner, document, exact revision advance, named stroke', async () => {
  const q = createRequest();
  const good = await sdk((r) => reply(r, 8, receipt)).dispatch(q);
  assert.deepEqual(good.result, receipt);
  const tampered = [
    (r) => reply(r, 9, receipt), (r) => reply(r, 7, receipt), (r) => reply(r, 8, Object.assign({}, receipt, { strokeId: 'other:8:0' })),
    (r) => reply(r, 8, Object.assign({}, receipt, { operation: 'delete' })), (r) => reply(r, 8, { applied: true, historyEntriesAdded: 1 }),
    (r) => Object.assign(reply(r, 8, receipt), { documentId: 'document-B' }), (r) => Object.assign(reply(r, 8, receipt), { instanceId: 'instance-B' }),
  ];
  for (const respond of tampered) await assert.rejects(sdk(respond).dispatch(q), TypeError);
  const noop = request(Contract.PARAMETRIC, { command: 'object.parametric.translate', stableTarget: target('native-stroke:8:0'), delta: [0, 0] });
  const compact = await sdk((r) => reply(r, 7, { applied: false, historyEntriesAdded: 0 })).dispatch(noop);
  assert.equal(compact.contentRevision, 7);
  await assert.rejects(sdk((r) => reply(r, 8, { applied: false, historyEntriesAdded: 0 })).dispatch(noop), /revision mismatch/);
  const refusal = await sdk((r) => ({ apiVersion: 2, requestId: r.requestId, instanceId: r.instanceId, documentId: r.documentId, contentRevision: 7, ok: false, error: { code: 'stale_revision', message: 'stale' } })).dispatch(q);
  assert.equal(refusal.ok, false);
});

function readRequest(strokeId = 'native-stroke:8:0', kind = 'authored') {
  return { apiVersion: 2, requestId: 'read-1', instanceId: 'instance-A', documentId: 'document-A', operation: 'query.document.object', payload: { atRevision: 5, stableTarget: target(strokeId, kind) } };
}
const readReply = (q, object) => reply(q, 5, { atRevision: 5, documentSnapshotId: 'native-object:document-A:5', object });

test('the object read returns either record family through the exact-key validator', async () => {
  const legacy = await sdk((q) => readReply(q, Object.assign(legacyRecord(), { target: q.payload.stableTarget }))).dispatch(readRequest('s1'));
  assert.equal(legacy.result.object.family, 'closed-solid-fill-cubic-path');
  const open = vectorRecord('native-stroke:8:0');
  assert.equal((await sdk((q) => readReply(q, open)).dispatch(readRequest())).result.object.geometry.closed, false);
  const ribbon = vectorRecord('native-stroke:8:0'); ribbon.vector.stroke = null; ribbon.vector.hasRealStroke = false; ribbon.fill = { kind: 'solid', r: 0, g: 0, b: 0, a: 1 };
  ribbon.vectorExt = { pressure: { centerSegments: [{ point: { x: 1, y: 2 }, handleIn: { x: 0, y: 0 }, handleOut: { x: 3, y: 0 }, width: 2 }], widthProfile: [{ t: 0, width: 2 }], strokeProfile: 'even', profileBase: 3 }, boxAngle: 30 };
  assert.deepEqual((await sdk((q) => readReply(q, ribbon)).dispatch(readRequest())).result.object.vectorExt.boxAngle, 30);
  const reference = vectorRecord('native-stroke:8:0', 'reference');
  assert.equal((await sdk((q) => readReply(q, reference)).dispatch(readRequest('native-stroke:8:0', 'reference'))).ok, true, 'reference-scope storage is readable');
  const bad = [
    (r) => { r.vector.hasRealStroke = false; }, (r) => { r.vectorExt = {}; }, (r) => { r.geometry.segments.length = 1; }, (r) => { r.fillGradient = {}; },
    (r) => { r.vector.strokeCap = 'arrow'; }, (r) => { r.target.strokeId = ''; }, (r) => { delete r.fill; },
  ];
  for (const mutate of bad) { const r = vectorRecord('native-stroke:8:0'); mutate(r); await assert.rejects(sdk((q) => readReply(q, r)).dispatch(readRequest()), TypeError); }
  // the legacy family keeps its exact keys and its closed-only geometry
  const withVector = Object.assign(legacyRecord(), { vector: open.vector });
  await assert.rejects(sdk((q) => readReply(q, withVector)).dispatch(readRequest('s1')), TypeError);
  const openLegacy = legacyRecord(); openLegacy.geometry.closed = false;
  await assert.rejects(sdk((q) => readReply(q, openLegacy)).dispatch(readRequest('s1')), TypeError);
  await assert.rejects(sdk((q) => readReply(q, null)).dispatch(readRequest()), TypeError);
});

test('end to end: the intent client over the real SDK creates, no-ops and deletes with correlated receipts', async () => {
  const fixture = createFixture({ sdk: SDK });
  const client = Vector.create(fixture.session);
  const dictionary = { segments: [{ point: [10, 20], handleIn: [0, 0], handleOut: [0, 0] }, { point: [110, 70], handleIn: [0, 0], handleOut: [0, 0] }], closed: false,
    strokeColor: '#112233', hasRealStroke: true, strokeWidth: 4, strokeCap: 'round', strokeJoin: 'round', fillColor: null, opacity: 1 };
  const created = await client.createStroke('L1', 3, dictionary);
  assert.deepEqual([created.status, created.strokeId, created.revision], ['applied', 'native-stroke:8:0', 8]);
  const same = await client.replaceStroke({ layerUid: 'L1', frame: 3, strokeId: 'native-stroke:8:0' }, dictionary);
  assert.deepEqual([same.status, same.revision], ['noop', 8]);
  const stack = await client.setPathEffects('L1', [client.defaultEffect('zigzag')]);
  assert.deepEqual([stack.status, stack.revision], ['applied', 9]);
  const removed = await client.deleteStroke({ layerUid: 'L1', frame: 3, strokeId: 'native-stroke:8:0' });
  assert.deepEqual([removed.status, removed.revision], ['applied', 10]);
  assert.deepEqual(fixture.owner.log.received.map((r) => r.operation), [Contract.VECTOR, Contract.VECTOR, Contract.EFFECTS, Contract.VECTOR]);
  const lying = createFixture({ sdk: SDK, respond: (response) => { response.result.strokeId = 'other:8:0'; return response; } });
  assert.equal((await Vector.create(lying.session).createStroke('L1', 3, dictionary)).status, 'indeterminate', 'the SDK rejects a receipt for another stroke; the owner did execute, so the outcome is indeterminate and replayable');
});
