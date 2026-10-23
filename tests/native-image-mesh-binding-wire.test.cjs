'use strict';
// Additive agreement controls. The original public corpus and its cardinalities stay intact.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const contract = require('../src/js/adapters/native-object-rig-contract.js');
const SDK = require('../src/js/adapters/native-application.js');
const { validator } = require('./native-object-rig-schema.cjs');
const schema = JSON.parse(fs.readFileSync(path.join(__dirname, '../engineering/application/native-transport-v2.schema.json'), 'utf8'));
const operation = 'command.document.object.image.mesh.edit';
const target = () => ({ contextId: 'component-A', layerUid: 'inner', strokeId: 'pixels', frameScope: { kind: 'authored', frame: 23 } });
const mesh = () => ({ outline: [[0, 0], [1, 0], [0, 1]], verts: [[0, 0], [1, 0], [0, 1]], tris: [0, 1, 2], offsets: [[0, 0], [0, 0], [0, 0]], cols: 1, rows: 64 });
const action = (value) => ({ command: 'object.image.mesh.edit', action: value });
const attach = () => action({ kind: 'attach', target: target(), mesh: mesh() });
const envelope = (payload) => ({ apiVersion: 2, requestId: 'binding-agreement', instanceId: 'instance-A', documentId: 'document-A', expectedRevision: 7, operation, payload });
function validCases() {
  const reference = target(); reference.frameScope = { kind: 'reference', frame: 4294967295 };
  const rows = [attach(), action({ kind: 'detach', target: reference }),
    action({ kind: 'duplicate', contextId: 'component-A', layerUid: 'inner' }),
    action({ kind: 'releaseIfUnused', meshId: 'im_1' }), action({ kind: 'releaseIfUnused', meshId: '' })];
  for (const meshId of ['im_1', '', null]) rows.push(action({ kind: 'propagate', target: target(), meshId }));
  // Owner-computed topology and executable-context admission remain actual native owner checks.
  const ownerInvalid = attach(); ownerInvalid.action.mesh.tris = [999]; rows.push(ownerInvalid);
  const emptyArrays = attach(); emptyArrays.action.mesh = { outline: [], verts: [], tris: [] }; rows.push(emptyArrays);
  return rows;
}
function faults() {
  const rows = [];
  const locate = (value, keys) => keys.reduce((object, key) => object[key], value);
  for (const pointer of ['command', 'action', 'action.kind', 'action.target', 'action.target.contextId',
    'action.target.layerUid', 'action.target.strokeId', 'action.target.frameScope',
    'action.target.frameScope.kind', 'action.target.frameScope.frame', 'action.mesh',
    'action.mesh.outline', 'action.mesh.verts', 'action.mesh.tris']) {
    const payload = attach(), keys = pointer.split('.'), key = keys.pop();
    delete locate(payload, keys)[key]; rows.push(payload);
  }
  for (const [pointer, value] of [
    ['command', 'object.rig.edit'], ['action', null], ['action.kind', 'attachVideo'], ['action.kind', 'detachVideo'],
    ['action.target', []], ['action.target.contextId', ''], ['action.target.layerUid', ''], ['action.target.strokeId', ''],
    ['action.target.frameScope.kind', 'unknown'], ['action.target.frameScope.frame', -1],
    ['action.target.frameScope.frame', 0.5], ['action.target.frameScope.frame', 4294967296],
    ['action.mesh.verts', [[0]]], ['action.mesh.outline', [[0, 1, 2]]], ['action.mesh.tris', [-1]],
    ['action.mesh.tris', [0.5]], ['action.mesh.tris', [4294967296]], ['action.mesh.offsets', null],
    ['action.mesh.cols', 0], ['action.mesh.cols', 65], ['action.mesh.rows', 1.5],
  ]) {
    const payload = attach(), keys = pointer.split('.'), key = keys.pop();
    locate(payload, keys)[key] = value; rows.push(payload);
  }
  for (const keys of [[], ['action'], ['action', 'target'], ['action', 'target', 'frameScope'], ['action', 'mesh']]) {
    const payload = attach(); locate(payload, keys).extra = true; rows.push(payload);
  }
  rows.push({ ...attach(), meshId: 'im_1' }, action({ kind: 'propagate', target: target() }),
    action({ kind: 'duplicate', contextId: '', layerUid: 'inner' }),
    action({ kind: 'resetOffsets' }));
  return rows;
}

test('binding closed shapes agree across the actual JS contract, transport schema and common SDK', () => {
  const branches = schema.$defs.Request.oneOf.filter((branch) => branch.allOf[1].properties.operation.const === operation);
  assert.equal(branches.length, 1);
  assert.equal(branches[0].allOf[0].$ref, '#/$defs/RequiredRevisionRequest');
  const payloadSchema = branches[0].allOf[1].properties.payload;
  const v = validator(schema);
  for (const payload of validCases()) {
    contract.payload(operation, payload);
    assert.equal(v.ok(payloadSchema, payload), true, JSON.stringify(payload));
    SDK.validateRequest(envelope(payload));
    const noRevision = envelope(payload); delete noRevision.expectedRevision;
    assert.throws(() => SDK.validateRequest(noRevision), TypeError);
  }
  for (const payload of faults()) {
    assert.throws(() => contract.payload(operation, payload), TypeError, JSON.stringify(payload));
    assert.equal(v.ok(payloadSchema, payload), false, JSON.stringify(payload));
    assert.throws(() => SDK.validateRequest(envelope(payload)), TypeError, JSON.stringify(payload));
  }
  // The additive binding branch does not raise the existing public request-byte budget.
  const large = attach(); large.action.mesh.verts = Array.from({ length: 400 }, () => [0.123456789, 0.987654321]);
  contract.payload(operation, large);
  assert.throws(() => SDK.validateRequest(envelope(large)), /4096/);
});

test('binding requests retain the existing compact receipt and exact revision correlation', async () => {
  for (const payload of validCases().slice(0, 8)) {
    const request = envelope(payload);
    const send = (applied, mutate) => SDK.createNativeApplicationAdapter('binding-wire', { dispatch: () => {
      const response = { apiVersion: 2, requestId: request.requestId, instanceId: request.instanceId,
        documentId: request.documentId, contentRevision: 7 + Number(applied), ok: true,
        result: { applied, historyEntriesAdded: Number(applied) } };
      if (mutate) mutate(response);
      return response;
    } }).dispatch(request);
    assert.equal((await send(true)).contentRevision, 8);
    assert.equal((await send(false)).contentRevision, 7);
    for (const mutate of [(r) => { r.contentRevision = 7; }, (r) => { r.requestId = 'foreign'; },
      (r) => { r.instanceId = 'foreign'; }, (r) => { r.documentId = 'foreign'; },
      (r) => { r.result.historyEntriesAdded = 0; }, (r) => { r.result.meshId = 'im_1'; }]) {
      await assert.rejects(send(true, mutate), TypeError);
    }
  }
});
