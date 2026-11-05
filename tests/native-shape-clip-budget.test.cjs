'use strict';
// X-shape-clipboard-duplicate: the shape clipboard carries its detached clip inline, so a paste request and a copy / cut reply get 64 KiB (the engine's own
// clip payload bound) while every other selection-edit operation keeps 4096. Pinned across the SDK, the edit contract and the declared descriptor/schema.
const test = require('node:test');
const assert = require('node:assert/strict');
const SDK = require('../src/js/adapters/native-application.js');
const Contract = require('../src/js/adapters/native-object-edit-contract.js');
const descriptor = require('../engineering/application/capabilities-v2/native-object-selection-edit.json');
const schema = require('../engineering/application/native-transport-v2.schema.json');
const P = 'command.document.object.';
const count = (v) => Buffer.byteLength(JSON.stringify(v), 'utf8');
const record = (pad) => ({ schemaVersion: 1, family: 'closed-solid-fill-cubic-path', target: { contextId: 'scene-root', frameScope: { kind: 'authored', frame: 0 }, layerUid: 'layer-a', strokeId: 'stroke-1' }, geometry: { pad } });
const clip = (pad) => ({ contextId: 'scene-root', layerUid: 'layer-a', frame: 0, records: [record(pad)] });
function paste(pad) { return { apiVersion: 2, requestId: 'clip-budget', instanceId: 'instance-A', documentId: 'document-A', expectedRevision: 7, operation: P + 'clipboard.paste',
  payload: { command: 'object.clipboard.paste', contextId: 'scene-root', layerUid: 'layer-a', frame: 0, mode: 'motion', clip: clip(pad) } }; }
function reply(kind, pad) { return { apiVersion: 2, requestId: 'clip-budget', instanceId: 'instance-A', documentId: 'document-A', contentRevision: kind === 'cut' ? 8 : 7, ok: true,
  result: { applied: kind === 'cut', historyEntriesAdded: kind === 'cut' ? 1 : 0, clipboard: clip(pad) } }; }
const exact = (build, size) => { const base = count(build('')); const made = build('x'.repeat(size - base)); assert.equal(count(made), size); return made; };

test('the limits are declared once and agree: contract, descriptor and schema', () => {
  assert.equal(Contract.SHAPE_CLIP_BYTES, 65536);
  const ops = descriptor.operations;
  assert.equal(descriptor.resourceBoundary.maxRequestBytes, 65536);
  for (const op of ops) {
    assert.equal(descriptor.resourceBoundary.requestBytesByOperation[op], Contract.requestByteLimit(op), op);
    assert.equal(descriptor.resourceBoundary.responseBytesByOperation[op], Contract.responseByteLimit(op), op);
  }
  assert.equal(Contract.requestByteLimit(P + 'clipboard.paste'), 65536);
  assert.equal(Contract.requestByteLimit(P + 'clipboard.copy'), 4096);
  for (const op of [P + 'clipboard.copy', P + 'clipboard.cut']) assert.equal(Contract.responseByteLimit(op), 65536);
  for (const op of [P + 'selection.delete', P + 'selection.duplicate', P + 'layer.split', P + 'clipboard.paste']) assert.equal(Contract.responseByteLimit(op), 4096, op);
  assert.equal(schema.$defs.ObjectClipboardPastePayload['x-maxEncodedRequestBytes'], 65536);
  for (const kind of ['Copy', 'Cut']) assert.equal(schema.$defs['ObjectClipboard' + kind + 'Result']['x-maxEncodedResponseBytes'], 65536);
});

test('SDK: a paste request is admitted up to 65536 encoded bytes and refused one byte over', () => {
  for (const size of [4096, 4097, 65535, 65536]) assert.doesNotThrow(() => SDK.validateRequest(exact(paste, size)), String(size));
  assert.throws(() => SDK.validateRequest(exact(paste, 65537)), /65536/);
});

test('SDK: every other selection-edit request keeps 4096, including copy and cut', () => {
  // The member that can grow past a size bound without breaking shape is the stroke id list (up to 1024 ids of 128 bytes).
  const make = (op, n) => ({ apiVersion: 2, requestId: 'clip-budget', instanceId: 'instance-A', documentId: 'document-A', expectedRevision: 7, operation: P + op,
    payload: { command: 'object.' + op, contextId: 'scene-root', layerUid: 'layer-a', frame: 0, mode: 'motion', strokeIds: Array.from({ length: n }, (_, k) => 's'.repeat(90) + String(k).padStart(4, '0')) } });
  for (const op of ['clipboard.copy', 'clipboard.cut', 'selection.duplicate', 'selection.delete']) {
    let n = 1;
    while (count(make(op, n + 1)) <= 4096) n++;
    assert.doesNotThrow(() => SDK.validateRequest(make(op, n)), op + ' at ' + count(make(op, n)));
    assert.throws(() => SDK.validateRequest(make(op, n + 1)), /4096/, op + ' at ' + count(make(op, n + 1)));
  }
});

test('SDK: a copy or cut reply is admitted up to 65536 encoded bytes and refused one byte over; a paste reply stays 4096', () => {
  for (const kind of ['copy', 'cut']) {
    const make = (pad) => reply(kind, pad);
    for (const size of [4097, 65535, 65536]) assert.doesNotThrow(() => SDK.validateResponse(exact(make, size), 'clip-budget', P + 'clipboard.' + kind), kind + size);
    assert.throws(() => SDK.validateResponse(exact(make, 65537), 'clip-budget', P + 'clipboard.' + kind), /65536/);
  }
  const pasted = { apiVersion: 2, requestId: 'clip-budget', instanceId: 'instance-A', documentId: 'document-A', contentRevision: 8, ok: true, result: { applied: true, historyEntriesAdded: 1 } };
  assert.doesNotThrow(() => SDK.validateResponse(pasted, 'clip-budget', P + 'clipboard.paste'));
});
