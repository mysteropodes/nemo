'use strict';
// L04-N public declaration of the six native component and duplicator setup operations: contract, common SDK, capability
// files, closed schema and the baseline exposed-property keys agree. Needs the patched SDK (patch 2) and schema (patch 1).
const test = require('node:test'), assert = require('node:assert/strict'), fs = require('node:fs'), path = require('node:path');
const contract = require('../src/js/adapters/native-object-component-contract.js');
const SDK = require('../src/js/adapters/native-application.js');

const ROOT = path.resolve(__dirname, '..');
const P = 'command.document.object.';
const oracle = JSON.parse(fs.readFileSync(path.join(__dirname, 'native-object-component-fixtures', 'oracle.json'), 'utf8'));
// [suffix, literal payload]. Independent of the contract table: command strings and members are spelled out here.
const VALID = [
  ['component.convert', { command: 'object.component.convert', contextId: 'scene-root', layerUids: ['layer-a', 'layer-b'] }],
  ['component.expose', { command: 'object.component.expose', symbolId: 'native-component:6:0', key: 'ep_mfz9q1k2_8x2a', targetStrokeId: 'stroke-1', targetField: 'opacity', label: 'Eye', default: 100 }],
  ['component.place', { command: 'object.component.place', symbolId: 'native-component:6:0', name: 'Hand' }],
  ['duplicator.set', { command: 'object.duplicator.set', layerUid: 'layer-a', enabled: true, seed: 482913 }],
  ['duplicator.objects', { command: 'object.duplicator.objects', layerUids: ['layer-a', 'layer-b'], seed: 17 }],
  ['duplicator.path', { command: 'object.duplicator.path', layerUid: 'layer-a', pathLayerUid: 'layer-b' }],
];
const OPTIONAL = { 'component.expose': ['default'], 'duplicator.set': ['seed'] };
const envelope = (suffix, payload, extra) => Object.assign({ apiVersion: 2, requestId: 'component-1', instanceId: 'instance-A', documentId: 'document-A',
  operation: P + suffix, expectedRevision: 7, payload }, extra);
const payloadOf = (suffix) => VALID.find(([s]) => s === suffix)[1];

test('the contract admits exactly the six declared operations, all revisioned commands', () => {
  assert.deepEqual(contract.OPERATIONS, VALID.map(([s]) => P + s));
  for (const op of contract.OPERATIONS) { assert.equal(contract.admitted(op), true); assert.equal(contract.requiresRevision(op), true); }
  for (const op of ['command.document.object.layer.create', 'command.document.object.layer.policy.edit', 'command.document.object.layer.split',
    'command.document.object.component.enter', 'command.document.object.duplicator.convert', 'command.document.object.component', 'query.document.object',
    P + 'component.convert.extra', P + 'toString', 'component.convert', '', null, undefined]) {
    assert.equal(contract.admitted(op), false, String(op));
  }
});

test('every literal payload is accepted and every one-member mutation is refused', () => {
  for (const [suffix, payload] of VALID) {
    contract.payload(P + suffix, payload);
    for (const key of Object.keys(payload)) {
      const optional = (OPTIONAL[suffix] || []).includes(key);
      const missing = { ...payload }; delete missing[key];
      if (optional) contract.payload(P + suffix, missing);
      else assert.throws(() => contract.payload(P + suffix, missing), TypeError, suffix + ' without ' + key);
      // null is a legal value only where the declared domain names it.
      if (!(suffix === 'duplicator.path' && key === 'pathLayerUid')) assert.throws(() => contract.payload(P + suffix, { ...payload, [key]: null }), TypeError, suffix + ' null ' + key);
    }
    assert.throws(() => contract.payload(P + suffix, { ...payload, extra: 1 }), TypeError, suffix + ' extra');
    assert.throws(() => contract.payload(P + suffix, Object.keys(payload).map((k) => payload[k])), TypeError, suffix + ' positional');
    assert.throws(() => contract.payload(P + suffix, { ...payload, command: 'object.layer.policy.edit' }), TypeError, suffix + ' foreign command');
  }
  assert.throws(() => contract.payload(P + 'component.convert', payloadOf('component.place')), TypeError, 'a payload of another operation');
  assert.throws(() => contract.payload('command.document.object.layer.create', { command: 'object.layer.create', name: 'x' }), TypeError);
});

test('bounds mirror the owner: ids 128 bytes without NUL, 256 layers, ep_ keys, u32 seeds, a nullable path', () => {
  const ok = (suffix, patch) => contract.payload(P + suffix, { ...payloadOf(suffix), ...patch });
  const no = (suffix, patch) => assert.throws(() => contract.payload(P + suffix, { ...payloadOf(suffix), ...patch }), TypeError, suffix + JSON.stringify(patch).slice(0, 60));
  ok('component.place', { symbolId: 's'.repeat(128) }); no('component.place', { symbolId: 's'.repeat(129) });
  ok('component.place', { symbolId: 'é'.repeat(64) }); no('component.place', { symbolId: 'é'.repeat(65) });
  no('component.place', { symbolId: '' }); no('component.place', { symbolId: 'a\u0000b' }); ok('component.place', { symbolId: 'a\u0007b' });
  ok('component.place', { name: '' }); ok('component.place', { name: 'n'.repeat(2000) }); no('component.place', { name: 7 });
  ok('component.convert', { contextId: 'native-component:6:0' }); no('component.convert', { contextId: null }); no('component.convert', { contextId: 7 });
  const many = (n) => Array.from({ length: n }, (_, i) => 'l' + i);
  ok('component.convert', { layerUids: many(256) }); no('component.convert', { layerUids: many(257) }); no('component.convert', { layerUids: [] }); no('component.convert', { layerUids: [''] });
  ok('component.convert', { layerUids: ['a', 'a'] });
  no('duplicator.objects', { layerUids: [] });
  ok('component.expose', { key: 'ep_x' }); no('component.expose', { key: 'ep_' }); no('component.expose', { key: 'ex_abc' }); no('component.expose', { key: 'EP_abc' });
  ok('component.expose', { key: 'ep_' + 'k'.repeat(125) }); no('component.expose', { key: 'ep_' + 'k'.repeat(126) });
  no('component.expose', { targetField: '' }); ok('component.expose', { targetField: '__visible' });
  ok('component.expose', { label: '' }); no('component.expose', { label: null });
  // `default`: absent, zero and a negative are three accepted states; null, strings and non-finite numbers are refused.
  const withoutDefault = { ...payloadOf('component.expose') }; delete withoutDefault.default;
  contract.payload(P + 'component.expose', withoutDefault);
  ok('component.expose', { default: 0 }); ok('component.expose', { default: -2.5 });
  for (const bad of [null, '100', true, [1], NaN, Infinity]) no('component.expose', { default: bad });
  for (const suffix of ['duplicator.set', 'duplicator.objects']) {
    ok(suffix, { seed: 0 }); ok(suffix, { seed: 999999 }); ok(suffix, { seed: 4294967295 });
    for (const bad of [4294967296, -1, 0.5, '7', null, NaN]) no(suffix, { seed: bad });
  }
  const withoutSeed = { ...payloadOf('duplicator.set') }; delete withoutSeed.seed;
  contract.payload(P + 'duplicator.set', withoutSeed);
  contract.payload(P + 'duplicator.set', { ...withoutSeed, enabled: false });
  no('duplicator.set', { enabled: 1 }); no('duplicator.set', { enabled: 'true' });
  ok('duplicator.path', { pathLayerUid: null }); ok('duplicator.path', { pathLayerUid: '' }); ok('duplicator.path', { pathLayerUid: 'p'.repeat(128) });
  no('duplicator.path', { pathLayerUid: 'p'.repeat(129) }); no('duplicator.path', { pathLayerUid: 7 });
  const absent = { ...payloadOf('duplicator.path') }; delete absent.pathLayerUid;
  assert.throws(() => contract.payload(P + 'duplicator.path', absent), TypeError, 'the clear is an explicit null, never an absent member');
});

test('every key the baseline expose() ever made is a valid exposed key of the contract', () => {
  assert.equal(oracle.expose.length > 50, true);
  for (const row of oracle.expose) {
    const body = { ...payloadOf('component.expose'), key: row.key };
    // A key with an empty random tail (random 0) is "ep_<clock>_": still ep_ plus more bytes.
    contract.payload(P + 'component.expose', body);
  }
  assert.throws(() => contract.payload(P + 'component.expose', { ...payloadOf('component.expose'), key: 'ep_' }), TypeError);
});

test('results: compact receipts correlate applied and history count; a minted layer is named by the operations that create one', () => {
  const compact = { applied: true, historyEntriesAdded: 1 }, noop = { applied: false, historyEntriesAdded: 0 }, minted = { applied: true, historyEntriesAdded: 1, layerUid: 'native-layer:8:0' };
  const creators = ['component.place', 'duplicator.objects'];
  for (const [suffix] of VALID) {
    const op = P + suffix;
    if (creators.includes(suffix)) {
      contract.result(op, minted);
      for (const bad of [compact, noop, { applied: false, historyEntriesAdded: 0, layerUid: 'x' }, { applied: true, historyEntriesAdded: 1, layerUid: '' }, { applied: true, historyEntriesAdded: 0, layerUid: 'x' }, null]) {
        assert.throws(() => contract.result(op, bad), TypeError, suffix + JSON.stringify(bad));
      }
      continue;
    }
    contract.result(op, compact); contract.result(op, noop);
    if (suffix === 'component.convert') contract.result(op, minted);
    else assert.throws(() => contract.result(op, minted), TypeError, suffix + ' never names a layer');
    for (const bad of [{ applied: true, historyEntriesAdded: 0 }, { applied: false, historyEntriesAdded: 1 }, { applied: 1, historyEntriesAdded: 1 }, { applied: true }, { applied: true, historyEntriesAdded: 1, document: {} }, null, []]) {
      assert.throws(() => contract.result(op, bad), TypeError, suffix + JSON.stringify(bad));
    }
  }
});

test('the common SDK admits each operation, requires expectedRevision, accepts opaque data: identities, and refuses binary payloads', () => {
  for (const [suffix, payload] of VALID) {
    SDK.validateRequest(envelope(suffix, payload));
    SDK.validateRequest(envelope(suffix, payload, { cancelledBeforeDispatch: true }));
    const without = envelope(suffix, payload); delete without.expectedRevision;
    assert.throws(() => SDK.validateRequest(without), /expectedRevision presence/, suffix);
    assert.throws(() => SDK.validateRequest(envelope(suffix, payload, { expectedRevision: -1 })), TypeError, suffix);
    assert.throws(() => SDK.validateRequest(envelope(suffix, { ...payload, extra: true })), TypeError, suffix);
  }
  SDK.validateRequest(envelope('component.place', { command: 'object.component.place', symbolId: 'data:opaque component', name: 'blob:not a resource' }));
  assert.throws(() => SDK.validateRequest(envelope('component.convert', { ...payloadOf('component.convert'), layerUids: [new Uint8Array(4)] })), TypeError);
  assert.throws(() => SDK.validateRequest(envelope('duplicator.path', payloadOf('duplicator.path'), { operation: P + 'duplicator.paths' })), /not admitted/);
});

test('the SDK correlates each receipt: identity, document replacement and the exact revision advance', async () => {
  for (const [suffix, payload] of VALID) {
    const q = envelope(suffix, payload);
    const creates = ['component.place', 'duplicator.objects'].includes(suffix) || suffix === 'component.convert' && payload.layerUids.length > 1;
    const result = (applied) => (creates ? { applied: true, historyEntriesAdded: 1, layerUid: 'native-layer:8:0' } : { applied, historyEntriesAdded: applied ? 1 : 0 });
    const reply = (applied, mutate) => () => { const r = { apiVersion: 2, requestId: 'component-1', instanceId: 'instance-A', documentId: 'document-A',
      contentRevision: 7 + (applied ? 1 : 0), ok: true, result: result(applied) }; if (mutate) mutate(r); return r; };
    const send = (dispatch) => SDK.createNativeApplicationAdapter('c', { dispatch }).dispatch(q);
    assert.equal((await send(reply(true))).contentRevision, 8);
    if (!creates) assert.equal((await send(reply(false))).contentRevision, 7);
    for (const mutate of [(r) => { r.contentRevision = creates ? 7 : 9; }, (r) => { r.instanceId = 'foreign'; }, (r) => { r.documentId = 'foreign'; }, (r) => { r.requestId = 'foreign'; }]) {
      await assert.rejects(send(reply(true, mutate)), TypeError, suffix);
    }
    if (!creates) await assert.rejects(send(reply(true, (r) => { r.contentRevision = 7; })), /revision mismatch/);
    const replaced = await send(() => ({ apiVersion: 2, requestId: 'component-1', instanceId: 'instance-A', documentId: 'document-B', contentRevision: 3, ok: false, error: { code: 'wrong_document', message: 'replaced' } }));
    assert.equal(replaced.error.code, 'wrong_document');
    await assert.rejects(send(() => ({ apiVersion: 2, requestId: 'component-1', instanceId: 'instance-A', documentId: 'document-B', contentRevision: 3, ok: false, error: { code: 'not_found', message: 'x' } })), /document mismatch/);
  }
});

test('the composition keeps the font contract and declares nothing else', () => {
  const font = require('../src/js/adapters/native-font-contract.js');
  const composed = contract.compose(font);
  assert.equal(composed.admitted('command.document.object.asset.font.import'), true);
  assert.equal(contract.admitted('command.document.object.asset.font.import'), false);
  assert.equal(composed.admitted(P + 'component.place'), true);
  assert.equal(composed.admitted('command.document.object.layer.policy.edit'), false, 'the F06 policy command stays a plain SDK operation');
  assert.equal(composed.requiresRevision('command.document.object.asset.font.import'), true);
  assert.equal(composed.requiresRevision('query.document.object.asset.font'), false);
  assert.equal(composed.requiresRevision(P + 'duplicator.set'), true);
  assert.equal(composed.paths('query.document.object'), null, 'operations of neither contract fall through to the SDK');
  assert.equal(composed.paths(P + 'component.place').has('payload.name'), true);
  assert.equal(composed.paths(P + 'component.place').has('result.layerUid'), true);
  assert.equal(composed.paths(P + 'component.place').has('result.something'), false);
  assert.equal(composed.paths('command.document.object.asset.font.import').has('payload.source.family'), true);
  assert.equal(contract.compose(null).admitted(P + 'component.place'), true, 'without a base contract only the six operations are declared');
  assert.equal(contract.compose(null).admitted('command.document.object.asset.font.import'), false);
  // The SDK is wired to the composition: a font operation is still admitted and judged by the font contract (a TypeError that is not "not admitted").
  const fontRequest = { apiVersion: 2, requestId: 'font-1', instanceId: 'instance-A', documentId: 'document-A', expectedRevision: 7,
    operation: 'command.document.object.asset.font.import', payload: { command: 'object.asset.font.import' } };
  assert.throws(() => SDK.validateRequest(fontRequest), (error) => error instanceof TypeError && !/not admitted/.test(error.message));
});

test('capability descriptors and the closed transport schema declare exactly the contract operations', () => {
  const dir = path.join(ROOT, 'engineering/application/capabilities-v2');
  const files = ['component', 'duplicator'].map((k) => path.join(dir, 'native-object-' + k + '.json'));
  const schema = JSON.parse(fs.readFileSync(path.join(ROOT, 'engineering/application/native-transport-v2.schema.json'), 'utf8'));
  const declared = [];
  const ids = new Set();
  for (const file of files) {
    const d = JSON.parse(fs.readFileSync(file, 'utf8'));
    assert.equal(d.schemaVersion, 2); assert.equal(d.apiVersion, 2); assert.deepEqual(d.availability, { state: 'available', reason: null });
    assert.equal(d.authority.adapterOwnsWritableMirror, false); assert.equal(d.authority.fallback, 'none'); assert.equal(d.resourceBoundary.maxRequestBytes, 4096);
    assert.equal(ids.has(d.id), false); ids.add(d.id);
    assert.equal(d.input.oneOf.length, d.operations.length);
    for (const example of d.examples) {
      SDK.validateRequest(example.request);
      SDK.validateResponse(example.response, example.request.requestId, example.request.operation);
      assert.equal(d.operations.includes(example.request.operation), true);
      assert.equal(example.response.contentRevision, example.request.expectedRevision + (example.response.result.applied ? 1 : 0));
      contract.payload(example.request.operation, example.request.payload);
      contract.result(example.request.operation, example.response.result);
    }
    assert.deepEqual(d.examples.map((e) => e.request.operation), d.operations, 'one copyable example per operation, in declaration order');
    declared.push(...d.operations);
  }
  assert.deepEqual(declared.slice().sort(), contract.OPERATIONS.slice().sort());
  assert.equal(new Set(declared).size, 6);
  assert.deepEqual([...ids].sort(), ['native.object.component', 'native.object.duplicator']);
  for (const [suffix, payload] of VALID) {
    const branches = schema.$defs.Request.oneOf.filter((b) => b.allOf[1].properties.operation.const === P + suffix);
    assert.equal(branches.length, 1, suffix);
    assert.equal(branches[0].allOf[0].$ref, '#/$defs/RequiredRevisionRequest');
    const def = schema.$defs[branches[0].allOf[1].properties.payload.$ref.replace('#/$defs/', '')];
    assert.equal(def.additionalProperties, false);
    assert.deepEqual(def.required.slice().sort(), Object.keys(payload).filter((k) => !(OPTIONAL[suffix] || []).includes(k)).sort(), suffix);
    assert.deepEqual(Object.keys(def.properties).sort(), Object.keys(payload).sort(), suffix + ' declares every member');
    assert.equal(def.properties.command.const, payload.command);
  }
  assert.equal(schema.$defs.Request.oneOf.filter((b) => b.allOf[1].properties.operation.const === 'command.document.object.layer.policy.edit').length, 1, 'F06 branch is untouched');
});

test('convert success receipt kind matches single or merged selection and never lies about no-op', () => {
  for(const [layerUids,merged] of [[['a'],false],[['a','a'],false],[['a','b'],true],[['a','b','a'],true]]){
    const q=envelope('component.convert',{command:'object.component.convert',contextId:'scene-root',layerUids});
    const compact={applied:true,historyEntriesAdded:1},minted={...compact,layerUid:'native-layer:8:0'},noop={applied:false,historyEntriesAdded:0};
    for(const result of [compact,minted,noop]){
      const response={instanceId:q.instanceId,documentId:q.documentId,contentRevision:7+(result.applied?1:0),ok:true,result};
      const matches=merged?result===minted:result===compact;
      if(matches)assert.doesNotThrow(()=>contract.correlate(q,response));
      else assert.throws(()=>contract.correlate(q,response),/selected layer count/);
    }
  }
});
