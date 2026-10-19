'use strict';
// L05-K: the public declaration of the native frame-edit operations. The descriptor, the closed transport schema and the
// common v2 SDK must say the same thing, and the SDK must refuse everything outside it before any transport is reached.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const SDK = require('../src/js/adapters/native-application.js');
const Contract = require('../src/js/adapters/native-frame-edit-contract.js');
const Font = require('../src/js/adapters/native-font-contract.js');
const root = path.resolve(__dirname, '..');
const read = (file) => JSON.parse(fs.readFileSync(path.join(root, file), 'utf8'));
const descriptor = read('engineering/application/capabilities-v2/native-frame-edit.json');
const schema = read('engineering/application/native-transport-v2.schema.json');
const P = 'command.document.object.';
const OPS = Contract.OPERATIONS;
const request = (operation, payload, extra) => Object.assign({ apiVersion: 2, requestId: 'frame-edit-7', instanceId: 'instance-A', documentId: 'document-A', expectedRevision: 7, operation, payload }, extra);
const reply = (q, applied) => ({ apiVersion: 2, requestId: q.requestId, instanceId: q.instanceId, documentId: q.documentId, contentRevision: q.expectedRevision + (applied ? 1 : 0), ok: true, result: Object.assign({ applied, historyEntriesAdded: applied ? 1 : 0 }, q.operation === P + 'frames.repeat' ? { cycledLayers: 1, skippedComponentLayers: 0 } : {}) });
const OPTIONAL = { 'object.frames.insert': ['layerUids'], 'object.layer.key.promote': ['layerUids'] };

test('the descriptor, the schema and the contract declare the same twelve operations', () => {
  assert.equal(OPS.length, 12);
  assert.deepEqual(descriptor.operations, OPS);
  assert.equal(descriptor.id, 'native.frame-edit');
  assert.equal(descriptor.availability.state, 'available');
  assert.equal(descriptor.authority.adapterOwnsWritableMirror, false);
  assert.equal(descriptor.authority.fallback, 'none');
  assert.equal(descriptor.resourceBoundary.maxRequestBytes, 4096);
  assert.equal(descriptor.input.oneOf.length, 12);
  assert.deepEqual(descriptor.examples.map((e) => e.request.operation), OPS);
  for (const op of OPS) {
    const sample = Contract.example(op), command = Contract.commandOf(op);
    const branches = schema.$defs.Request.oneOf.filter((b) => b.allOf[1].properties.operation.const === op);
    assert.equal(branches.length, 1, op);
    assert.equal(branches[0].allOf[0].$ref, '#/$defs/RequiredRevisionRequest');
    const name = branches[0].allOf[1].properties.payload.$ref.replace('#/$defs/', '');
    assert.ok(descriptor.input.oneOf.some((i) => i.$ref === '../native-transport-v2.schema.json#/$defs/' + name), op);
    const def = schema.$defs[name];
    assert.equal(def.additionalProperties, false); assert.equal(def.properties.command.const, command);
    const optional = OPTIONAL[command] || [];
    assert.deepEqual(def.required.slice().sort(), Object.keys(sample).filter((k) => !optional.includes(k)).sort(), op);
    assert.deepEqual(Object.keys(def.properties).sort(), Object.keys(sample).concat(optional.filter((k) => !(k in sample))).sort(), op);
  }
  assert.equal(schema.$defs.ObjectFramesExtendPayload.properties.count.maximum, 999);
  for (const op of ['command.document.object.frames.clipboard.paste', 'command.document.object.frames.clipboard.cut']) {
    assert.equal(descriptor.operations.includes(op), false);
    assert.equal(schema.$defs.Request.oneOf.some((b) => b.allOf[1].properties.operation.const === op), false);
  }
});

test('every descriptor example is a valid request and a correlated receipt for the shared client', async () => {
  for (const example of descriptor.examples) {
    assert.doesNotThrow(() => SDK.validateRequest(example.request), example.request.operation);
    assert.doesNotThrow(() => SDK.validateResponse(example.response, example.request.requestId, example.request.operation));
    assert.deepEqual(example.request.payload, Contract.example(example.request.operation));
    const adapter = SDK.createNativeApplicationAdapter('frame-edit-test', { dispatch() { return structuredClone(example.response); } });
    assert.deepEqual(await adapter.dispatch(example.request), example.response);
  }
});

test('shapes are closed: missing, null, unknown and wrong-command fields are refused before transport', () => {
  let calls = 0;
  const adapter = SDK.createNativeApplicationAdapter('frame-edit-test', { dispatch() { calls++; throw new Error('must not dispatch'); } });
  for (const op of OPS) {
    const sample = Contract.example(op);
    for (const key of Object.keys(sample)) {
      const missing = structuredClone(sample); delete missing[key];
      assert.throws(() => adapter.dispatch(request(op, missing)), TypeError, op + ' without ' + key);
      const nulled = structuredClone(sample); nulled[key] = null;
      assert.throws(() => adapter.dispatch(request(op, nulled)), TypeError, op + ' null ' + key);
    }
    assert.throws(() => adapter.dispatch(request(op, Object.assign({}, sample, { extra: 1 }))), TypeError);
    assert.throws(() => adapter.dispatch(request(op, [op])), TypeError);
    for (const other of OPS) if (other !== op) assert.throws(() => adapter.dispatch(request(op, Contract.example(other))), TypeError, op + ' with ' + other + ' payload');
    for (const revision of [undefined, null, -1, 1.5]) {
      const q = request(op, sample); if (revision === undefined) delete q.expectedRevision; else q.expectedRevision = revision;
      assert.throws(() => adapter.dispatch(q), TypeError, op + ' revision ' + revision);
    }
  }
  assert.equal(calls, 0);
});

test('numbers, lists and opaque names follow the owner parsers', () => {
  const ok = (op, payload) => assert.doesNotThrow(() => SDK.validateRequest(request(op, payload)), JSON.stringify(payload));
  const bad = (op, payload) => assert.throws(() => SDK.validateRequest(request(op, payload)), TypeError, JSON.stringify(payload));
  const insert = (cell, extra) => Object.assign({ command: 'object.frames.insert', cell }, extra);
  ok(P + 'frames.insert', insert(0)); ok(P + 'frames.insert', insert(4294967295));
  for (const cell of [-1, 1.5, 4294967296, '3', null, NaN, Infinity]) bad(P + 'frames.insert', insert(cell));
  for (const layerUids of [[], ['a', 'a'], [''], [1], 'a', null]) bad(P + 'frames.insert', insert(1, { layerUids }));
  ok(P + 'frames.insert', insert(1, { layerUids: ['a', 'b'] }));
  for (const [count, accepted] of [[0, true], [999, true], [1000, false], [-1, false], [1.5, false]]) {
    (accepted ? ok : bad)(P + 'frames.extend', { command: 'object.frames.extend', cell: 0, count });
  }
  const promote = (cells) => ({ command: 'object.layer.key.promote', cells });
  bad(P + 'layer.key.promote', promote([])); bad(P + 'layer.key.promote', promote([2, 2])); bad(P + 'layer.key.promote', promote([-1]));
  ok(P + 'layer.key.promote', promote([2, 5])); ok(P + 'layer.key.promote', { command: 'object.layer.key.promote', cells: [2], layerUids: ['a'] });
  const blank = (layerUid, cell) => ({ command: 'object.layer.key.blank', layerUid, cell });
  ok(P + 'layer.key.blank', blank('layer-a', 0)); ok(P + 'layer.key.blank', blank('data:opaque', 4294967295));
  for (const cell of [-1, 1.5, 4294967296, '3', null]) bad(P + 'layer.key.blank', blank('layer-a', cell));
  for (const layerUid of ['', null, 3]) bad(P + 'layer.key.blank', blank(layerUid, 2));
  const clear = (targets) => ({ command: 'object.layer.key.clear_cells', targets });
  bad(P + 'layer.key.clear_cells', clear([]));
  bad(P + 'layer.key.clear_cells', clear([{ layerUid: 'a', frame: 1 }, { layerUid: 'a', frame: 1 }]));
  bad(P + 'layer.key.clear_cells', clear([{ layerUid: '', frame: 1 }]));
  bad(P + 'layer.key.clear_cells', clear([{ layerUid: 'a', frame: 1, extra: 0 }]));
  ok(P + 'layer.key.clear_cells', clear([{ layerUid: 'a', frame: 1 }, { layerUid: 'b', frame: 1 }, { layerUid: 'a', frame: 2 }]));
  const repeat = (times) => ({ command: 'object.frames.repeat', fromLayerUid: 'a', toLayerUid: 'b', fromFrame: 0, toFrame: 1, times });
  for (const [times, accepted] of [[0, true], [-3, true], [9007199254740991, true], [9007199254740992, false], [1.5, false], ['2', false], [null, false]]) {
    (accepted ? ok : bad)(P + 'frames.repeat', repeat(times));
  }
  bad(P + 'frames.duplicate', { command: 'object.frames.duplicate', layerUid: '', cell: 1 });
  bad(P + 'frames.clipboard.duplicate', { command: 'object.frames.clipboard.duplicate', layerUid: 'a', selection: [] });
  // Opaque authored names may begin with data:, only inside the named layer fields.
  const opaque = 'data:opaque authored layer';
  ok(P + 'layer.key.clear', { command: 'object.layer.key.clear', layerUid: opaque, cell: 0 });
  ok(P + 'frames.insert', insert(0, { layerUids: [opaque] }));
  ok(P + 'layer.key.clear_cells', clear([{ layerUid: opaque, frame: 0 }]));
  ok(P + 'frames.clipboard.duplicate', { command: 'object.frames.clipboard.duplicate', layerUid: opaque, selection: [{ layerUid: opaque, frame: 0 }] });
  ok(P + 'frames.repeat', Object.assign(repeat(2), { fromLayerUid: opaque, toLayerUid: opaque }));
  assert.throws(() => SDK.validateRequest(Object.assign(request(P + 'frames.remove', Contract.example(P + 'frames.remove')), { requestId: 'data:forbidden' })), TypeError);
});

test('a request over the 4096-byte message bound is refused before transport', () => {
  const targets = []; for (let frame = 0; frame < 200; frame++) targets.push({ layerUid: 'layer-' + frame, frame });
  assert.throws(() => SDK.validateRequest(request(P + 'layer.key.clear_cells', { command: 'object.layer.key.clear_cells', targets })), /4096/);
  assert.doesNotThrow(() => SDK.validateRequest(request(P + 'layer.key.clear_cells', { command: 'object.layer.key.clear_cells', targets: targets.slice(0, 20) })));
});

test('clipboard packets and unsupported blank-key aliases are not admitted', () => {
  for (const op of [P + 'frames.clipboard.paste', P + 'frames.clipboard.cut', P + 'frames.clipboard.copy', P + 'layer.key.insert_blank', P + 'frames.insert_blank']) {
    assert.equal(Contract.admitted(op), false);
    assert.throws(() => SDK.validateRequest(request(op, { command: 'object.frames.clipboard.paste' })), /not admitted/);
  }
});

test('receipts must carry exactly one revision per applied change and the same owner', async () => {
  for (const op of OPS) {
    const q = request(op, Contract.example(op));
    for (const applied of op === P + 'layer.key.blank' ? [true] : [true, false]) {
      const adapter = SDK.createNativeApplicationAdapter('frame-edit-test', { dispatch() { return reply(q, applied); } });
      assert.deepEqual(await adapter.dispatch(q), reply(q, applied));
    }
    const wrong = [
      (r) => { r.contentRevision = 7; }, (r) => { r.contentRevision = 9; }, (r) => { r.result.applied = false; }, (r) => { r.result.historyEntriesAdded = 0; },
      (r) => { r.result.extra = 1; }, (r) => { r.instanceId = 'foreign'; }, (r) => { r.documentId = 'foreign'; }, (r) => { r.requestId = 'foreign'; }, (r) => { delete r.result; }
    ];
    for (const mutate of wrong) {
      const adapter = SDK.createNativeApplicationAdapter('frame-edit-test', { dispatch() { const r = reply(q, true); mutate(r); return r; } });
      await assert.rejects(adapter.dispatch(q), op);
    }
    const stale = { apiVersion: 2, requestId: q.requestId, instanceId: q.instanceId, documentId: q.documentId, contentRevision: 8, ok: false, error: { code: 'stale_revision', message: 'Stale frame edit' } };
    assert.deepEqual(await SDK.createNativeApplicationAdapter('frame-edit-test', { dispatch() { return stale; } }).dispatch(q), stale);
    const replaced = Object.assign({}, stale, { documentId: 'document-B', error: { code: 'wrong_document', message: 'Document was replaced' } });
    assert.deepEqual(await SDK.createNativeApplicationAdapter('frame-edit-test', { dispatch() { return replaced; } }).dispatch(q), replaced);
    const foreign = Object.assign({}, stale, { documentId: 'document-B' });
    await assert.rejects(SDK.createNativeApplicationAdapter('frame-edit-test', { dispatch() { return foreign; } }).dispatch(q));
  }
});

test('join keeps the font declarations and refuses everything it was not given', () => {
  const joined = Contract.join(Font), fontOp = 'command.document.object.asset.font.import';
  assert.equal(Font.admitted(fontOp), true);
  assert.equal(joined.admitted(fontOp), true); assert.equal(joined.requiresRevision(fontOp), Font.requiresRevision(fontOp));
  assert.deepEqual(joined.paths(fontOp), Font.paths(fontOp));
  for (const op of OPS) { assert.equal(joined.admitted(op), true); assert.equal(joined.requiresRevision(op), true); assert.equal(joined.paths(op).has('payload.layerUid'), true); }
  for (const op of ['query.document.object', P + 'layer.policy.edit', P + 'frames.clipboard.paste']) {
    assert.equal(joined.admitted(op), false); assert.equal(joined.requiresRevision(op), false);
  }
  const alone = Contract.join(null);
  assert.equal(alone.admitted(fontOp), false); assert.equal(alone.admitted(OPS[0]), true);
  assert.throws(() => alone.payload(fontOp, {}), TypeError);
  assert.equal(joined.paths(P + 'layer.key.clear').has('payload.cell'), false);
});

test('controls: each shape accepts its literal payload and rejects the one-off mutation of it', () => {
  const mutations = {
    'object.frames.extend': (p) => Object.assign(p, { count: 1000 }),
    'object.layer.key.promote': (p) => Object.assign(p, { cells: [3, 3] }),
    'object.layer.key.clear_cells': (p) => Object.assign(p, { targets: [] }),
    'object.frames.repeat': (p) => Object.assign(p, { times: 0.5 }),
    'object.frames.insert': (p) => Object.assign(p, { cell: -1 })
  };
  for (const op of OPS) {
    const command = Contract.commandOf(op);
    assert.doesNotThrow(() => Contract.payload(op, Contract.example(op)));
    const mutate = mutations[command] || ((p) => Object.assign(p, { command: 'object.frames.nothing' }));
    assert.throws(() => Contract.payload(op, mutate(Contract.example(op))), TypeError, op);
  }
});

test('valid() is the non-throwing verdict of payload() for every operation', () => {
  for (const op of OPS) {
    const example = Contract.example(op);
    assert.equal(Contract.valid(op, example), true, op);
    assert.equal(Contract.valid(op, Object.assign({}, example, { extra: 1 })), false, op + ' unknown field');
    assert.equal(Contract.valid(op, null), false, op + ' null');
  }
  assert.equal(Contract.valid('command.document.object.frames.clipboard.paste', { command: 'object.frames.clipboard.paste' }), false);
  assert.equal(Contract.valid(undefined, {}), false);
});

// Native owner counts are exercised by Rust; these are actual SDK contract refusal controls.
test('repeat counts are required, closed, bounded and exclusive to repeat, including applied growth with zero cycles', async () => {
  const op=P+'frames.repeat', q=request(op,Contract.example(op));
  for(const applied of [true,false]) {
    const r=reply(q,applied);r.result.cycledLayers=0;r.result.skippedComponentLayers=2;
    const client=SDK.createNativeApplicationAdapter('frame-edit-test',{dispatch(){return structuredClone(r);}});
    assert.deepEqual(await client.dispatch(q),r);
  }
  for(const key of ['cycledLayers','skippedComponentLayers']) {
    for(const value of [undefined,-1,1.5,4294967296,NaN,Infinity,'1',null]) {
      const r=reply(q,true);if(value===undefined)delete r.result[key];else r.result[key]=value;
      assert.throws(()=>Contract.result(op,r.result),TypeError);
    }
  }
  for(const other of OPS.filter(x=>x!==op))assert.throws(()=>Contract.result(other,reply(q,true).result),TypeError);
  assert.deepEqual(schema.$defs.ObjectFrameRepeatResult.required,['applied','historyEntriesAdded','cycledLayers','skippedComponentLayers']);
  assert.equal(schema.$defs.ObjectFrameRepeatResult.additionalProperties,false);
});


test('F7 success always checkpoints once, including an already blank key; no-op and extra-field receipts refuse', async () => {
  const op = P + 'layer.key.blank', q = request(op, { command: 'object.layer.key.blank', layerUid: 'active', cell: 2 });
  const adapter = (response) => SDK.createNativeApplicationAdapter('frame-edit-test', { dispatch() { return response; } });
  assert.deepEqual(await adapter(reply(q, true)).dispatch(q), reply(q, true));
  await assert.rejects(adapter(reply(q, false)).dispatch(q));
  const extra = reply(q, true); extra.result.cycledLayers = 0;
  await assert.rejects(adapter(extra).dispatch(q));
  assert.throws(() => SDK.validateRequest(request(op, { command: 'object.layer.key.blank', layerUid: 'active', cell: 2, layerUids: ['other'] })), TypeError);
  assert.throws(() => SDK.validateRequest(request(op, { command: 'object.layer.key.blank', layerUid: 'x'.repeat(4096), cell: 2 })), /4096/);
});
