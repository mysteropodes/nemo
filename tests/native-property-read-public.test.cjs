'use strict';
// Public contract and injected transport only; no native/installed acceptance.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const SDK = require('../src/js/adapters/native-application.js');
const ROOT = path.resolve(__dirname, '..');
const OP = 'query.document.object.property.read';
function request(kind = 'rawSample') {
  const view = {kind, target:{contextId:'scene-root', layerUid:'data:layer 雪'}, property:'opacity'};
  if (kind === 'rawSample') view.frame = -1.5;
  else Object.assign(view, {offset:0, limit:2});
  return {apiVersion:2, requestId:'property-read', instanceId:'instance', documentId:'document',
    operation:OP, payload:{atRevision:7, documentSnapshotId:'native-object:document:7', view}};
}
function response(q, fields = {}) {
  const view = q.payload.view;
  const result = {instanceId:q.instanceId, documentId:q.documentId, atRevision:q.payload.atRevision,
    documentSnapshotId:q.payload.documentSnapshotId, kind:view.kind, target:{...view.target},
    property:view.property, dimension:1};
  if (view.kind === 'rawSample') Object.assign(result, {frame:view.frame, value:[20]});
  else Object.assign(result, {trackPresent:true, items:[{frame:-2, v:[10], curvePoints:[]}, {frame:2, v:[90]}], nextOffset:null});
  Object.assign(result, fields);
  return {apiVersion:2, requestId:q.requestId, instanceId:q.instanceId, documentId:q.documentId,
    contentRevision:q.payload.atRevision, ok:true, result};
}
const adapter = dispatch => SDK.createNativeApplicationAdapter('property-read-test', {dispatch});
const read = (q, r) => adapter(() => r).readProperty(q);

test('D11 descriptor and inline schema declare only the exact pinned native property read', () => {
  const d = JSON.parse(fs.readFileSync(path.join(ROOT, 'engineering/application/capabilities-v2/native-property-read.json')));
  const s = JSON.parse(fs.readFileSync(path.join(ROOT, 'engineering/application/native-transport-v2.schema.json')));
  assert.equal(d.id, 'native.property-read'); assert.deepEqual(d.operations, [OP]);
  assert.equal(d.authority.adapterOwnsWritableMirror, false); assert.equal(d.authority.fallback, 'none');
  assert.deepEqual([d.resourceBoundary.maxRequestBytes, d.resourceBoundary.maxReadResponseBytes,
    d.resourceBoundary.maxProviderResponseBytes], [4096, 4096, 3072]);
  const branches = s.$defs.Request.oneOf.filter(b => b.allOf[1].properties.operation.const === OP);
  assert.equal(branches.length, 1); assert.equal(branches[0].allOf[0].$ref, '#/$defs/ForbiddenRevisionRequest');
  assert.equal(branches[0].allOf[1].properties.payload.$ref, '#/$defs/ObjectPropertyReadPayload');
  assert.equal(s.$defs.ObjectPropertyReadPayload.additionalProperties, false);
  assert.equal(s.$defs.ObjectPropertyReadKey.additionalProperties, false);
  for (const example of d.examples) {
    assert.doesNotThrow(() => SDK.validateRequest(example.request));
    assert.doesNotThrow(() => SDK.validateResponse(example.response, example.request.requestId, OP));
  }
  // Existing literal native sampler oracle: -2:[10] -> 2:[90], at-1.5 =20.
  assert.deepEqual(d.examples[0].response.result.value, [20]);
});

test('raw reads preserve finite signed fractional frames and all dimensions without evaluating in JS', async () => {
  for (const frame of [-100, -1.5, 0, 0.25, 12]) for (const dimension of [1, 2, 3, 4]) {
    const q = request(); q.payload.view.frame = frame;
    const values = [-125.5, 0, 255, 1000].slice(0, dimension);
    const r = response(q, {dimension, value:values}), before = structuredClone(q);
    let calls = 0, sent;
    assert.deepEqual(await adapter(value => { calls++; sent = value; return r; }).readProperty(q), r);
    assert.equal(calls, 1); assert.deepEqual(sent, before); assert.notEqual(sent, q); assert.deepEqual(q, before);
  }
});

test('authored DTOs preserve absent null empty curves hold tangents and handles exactly', async () => {
  const q = request('authoredKeys'); q.payload.view.limit = 5;
  const items = [{frame:-2, v:[10]}, {frame:-1.5, v:[20], curvePoints:null},
    {frame:0, v:[50], curvePoints:[], hold:false},
    {frame:1.5, v:[80], curvePoints:[{x:3, y:-2, tx:4}], hIn:[0, 1]},
    {frame:2, v:[90], curvePoints:[{x:0, y:0, ty:-2}, {x:1, y:1}], hOut:[5, 6], hold:true}];
  const r = response(q, {items}), before = structuredClone(r);
  assert.deepEqual(await read(q, r), before); assert.deepEqual(r, before);
  assert.equal(Object.hasOwn(r.result.items[0], 'curvePoints'), false);
  assert.equal(r.result.items[1].curvePoints, null); assert.deepEqual(r.result.items[2].curvePoints, []);
  assert.equal(Object.hasOwn(r.result.items[3].curvePoints[0], 'ty'), false);
  for (const dimension of [1, 2, 3, 4]) {
    const values = [10, 20, 30, 40].slice(0, dimension);
    const page = response(q, {dimension, items:[{frame:0.25, v:values}]});
    assert.deepEqual(await read(q, page), page);
  }
  for (const trackPresent of [false, true]) {
    const empty = response(q, {trackPresent, items:[], nextOffset:null}); assert.deepEqual(await read(q, empty), empty);
  }
});

test('request objects are closed at every level and forbid expectedRevision before dispatch', () => {
  let calls = 0; const port = adapter(() => { calls++; });
  for (const kind of ['rawSample', 'authoredKeys']) {
    const original = request(kind);
    for (const selected of [[], ['payload'], ['payload', 'view'], ['payload', 'view', 'target']]) {
      const object = selected.reduce((value, key) => value[key], original);
      for (const key of Object.keys(object)) for (const remove of [true, false]) {
        const q = structuredClone(original), target = selected.reduce((value, key) => value[key], q);
        if (remove) delete target[key]; else target[key] = null;
        assert.throws(() => port.readProperty(q));
      }
      const q = structuredClone(original); selected.reduce((value, key) => value[key], q).extra = true;
      assert.throws(() => port.readProperty(q));
    }
    for (const expectedRevision of [undefined, null, 0, 7]) {
      const q = request(kind); q.expectedRevision = expectedRevision; assert.throws(() => port.readProperty(q));
    }
  }
  for (const mutate of [q => q.payload = [], q => q.payload.view = [], q => q.payload.view.kind = 'effectiveSample',
    q => q.payload.view.target.strokeId = null, q => q.payload.view.target.contextId = '',
    q => q.payload.view.target.layerUid = '', q => q.payload.view.property = '',
    q => q.payload.view.frame = NaN, q => q.payload.view.frame = Infinity, q => q.payload.view.frame = '1.5',
    q => q.cancelledBeforeDispatch = 1]) {
    const q = request(); mutate(q); assert.throws(() => port.readProperty(q));
  }
  for (const [offset, limit] of [[-1, 2], [0.5, 2], [4294967296, 2], [0, 0], [0, 9], [0, 1.5]]) {
    const q = request('authoredKeys'); Object.assign(q.payload.view, {offset, limit}); assert.throws(() => port.readProperty(q));
  }
  assert.equal(calls, 0);
});

test('opaque Unicode data/blob identities are admitted only at appointed read fields', async () => {
  for (const text of ['data:opaque 雪', 'blob:opaque', '  raw opaque : /  ']) {
    const q = request(); q.payload.documentSnapshotId = text; q.payload.view.property = text;
    q.payload.view.target = {contextId:text, layerUid:text, strokeId:text};
    const r = response(q); assert.deepEqual(await read(q, r), r);
    const bad = {apiVersion:2, requestId:q.requestId, instanceId:q.instanceId, documentId:q.documentId,
      contentRevision:7, ok:false, error:{code:'unavailable', message:'data:binary'}};
    await assert.rejects(read(q, bad));
  }
});

test('raw replies reject malformed dimensions nonfinite/sparse vectors and unknown result fields', async () => {
  for (const mutate of [r => r.result.dimension = 0, r => r.result.dimension = 5,
    r => r.result.dimension = 1.5, r => r.result.value = [], r => r.result.value = [1, 2],
    r => r.result.value = [NaN], r => r.result.value = [Infinity], r => r.result.value = ['20'],
    r => r.result.value = Array(1), r => r.result.frame = NaN, r => r.result.extra = true,
    r => r.result.trackPresent = true, r => r.result = [], r => r.result.target.extra = true]) {
    const q = request(), r = response(q); mutate(r); await assert.rejects(read(q, r));
  }
});

test('authored key DTOs enforce optional presence vector sizes and native curve/key ordering', async () => {
  for (const item of [{frame:0, v:[1], hold:null}, {frame:0, v:[1], curvePoints:{}},
    {frame:0, v:[1], hIn:null}, {frame:0, v:[1], hOut:[]}, {frame:0, v:[1], hIn:[1, NaN]},
    {frame:0, v:[1], curvePoints:[{x:0, y:0, tx:null}]},
    {frame:0, v:[1], curvePoints:[{x:0, y:0, extra:1}]},
    {frame:0, v:[1], curvePoints:[{x:0, y:0}, {x:0, y:1}]},
    {frame:0, v:[1], curvePoints:[{x:0.1, y:0}, {x:1, y:1}]},
    {frame:0, v:[1], curvePoints:[{x:0, y:0}, {x:0.9, y:1}]},
    {frame:0, v:[1], curvePoints:Array(1)}, {frame:0, v:[]}, {frame:0, v:[1], id:'fabricated'}]) {
    const q = request('authoredKeys'); await assert.rejects(read(q, response(q, {items:[item]})));
  }
  for (const frames of [[1, 1], [2, 1]]) {
    const q = request('authoredKeys'); await assert.rejects(read(q, response(q, {items:frames.map(frame => ({frame, v:[1]}))})));
  }
  const q = request('authoredKeys'); await assert.rejects(read(q, response(q, {dimension:2})));
});

test('every outer/inner owner revision snapshot selector and sample frame pin is correlated', async () => {
  for (const mutate of [r => r.requestId = 'other', r => r.instanceId = 'other', r => r.documentId = 'other',
    r => r.contentRevision = 8, r => r.result.instanceId = 'other', r => r.result.documentId = 'other',
    r => r.result.atRevision = 8, r => r.result.documentSnapshotId = 'other',
    r => r.result.target.layerUid = 'other', r => r.result.target.strokeId = 'added',
    r => r.result.property = 'other', r => r.result.frame = 0, r => r.result.kind = 'authoredKeys']) {
    const q = request(), r = response(q); mutate(r); await assert.rejects(read(q, r));
  }
});

test('pinned authored pagination requires a full nonterminal page and exact advancing cursor', async () => {
  const q = request('authoredKeys'), first = response(q, {nextOffset:2}); assert.deepEqual(await read(q, first), first);
  const next = request('authoredKeys'); next.payload.view.offset = 2;
  const terminal = response(next, {items:[{frame:9, v:[90]}]}); assert.deepEqual(await read(next, terminal), terminal);
  for (const fields of [{nextOffset:0}, {nextOffset:1}, {nextOffset:3}, {nextOffset:4294967296},
    {items:[], nextOffset:2}, {items:[{frame:0, v:[1]}], nextOffset:1},
    {trackPresent:false}, {trackPresent:false, items:[], nextOffset:2}]) {
    await assert.rejects(read(q, response(q, fields)));
  }
  const small = request('authoredKeys'); small.payload.view.limit = 1; await assert.rejects(read(small, response(small)));
});

function fillBytes(value, setter, expected) {
  setter(''); const count = expected - Buffer.byteLength(JSON.stringify(value));
  assert.ok(count >= 0); setter('x'.repeat(count)); assert.equal(Buffer.byteLength(JSON.stringify(value)), expected);
}
test('inclusive UTF8 request/provider/envelope limits reject oversize without truncation', async () => {
  const q = request(); fillBytes(q, text => q.payload.view.target.layerUid = text, 4096);
  assert.doesNotThrow(() => SDK.validateRequest(q)); q.payload.view.target.layerUid += 'x'; assert.throws(() => SDK.validateRequest(q));
  const small = request(), r = response(small);
  fillBytes(r.result, text => r.result.property = text, 3072); assert.doesNotThrow(() => SDK.validateResponse(r, small.requestId, OP));
  r.result.property += 'x'; assert.throws(() => SDK.validateResponse(r, small.requestId, OP));
  const failure = {apiVersion:2, requestId:small.requestId, instanceId:small.instanceId, documentId:small.documentId,
    contentRevision:7, ok:false, error:{code:'unavailable', message:''}};
  fillBytes(failure, text => failure.error.message = text, 4096); assert.deepEqual(await read(small, failure), failure);
  failure.error.message += 'x'; await assert.rejects(read(small, failure));
  const unicode = request(); unicode.payload.view.target.layerUid = '雪'.repeat(1400); assert.throws(() => SDK.validateRequest(unicode));
});

test('typed refusals retain native cancellation/stale/replacement dispositions with no retry or fallback', async () => {
  for (const code of ['invalid_request', 'wrong_instance', 'wrong_document', 'stale_revision',
    'unavailable', 'not_found', 'cancelled_before_dispatch']) {
    const q = request(); q.cancelledBeforeDispatch = code === 'cancelled_before_dispatch';
    const r = {apiVersion:2, requestId:q.requestId, instanceId:q.instanceId,
      documentId:code === 'wrong_document' ? 'replacement' : q.documentId,
      contentRevision:7, ok:false, error:{code, message:'Native refused'}};
    let calls = 0; const before = structuredClone(q);
    assert.deepEqual(await adapter(() => { calls++; return r; }).readProperty(q), r);
    assert.equal(calls, 1); assert.deepEqual(q, before);
  }
  const failure = new Error('unknown transport outcome'); let calls = 0;
  const port = adapter(() => { calls++; throw failure; });
  assert.throws(() => port.readProperty(request()), error => error === failure); assert.equal(calls, 1);
  for (const operation of ['query.document.object.property.metadata', 'command.document.object.property.edit',
    'command.document.object.property.track.edit']) {
    const q = request(); q.operation = operation; assert.throws(() => port.readProperty(q));
  }
  assert.equal(calls, 1);
});

test('classic browser helper/SDK expose the same read with the literal finite native reply', async () => {
  const context = vm.createContext({TextEncoder});
  for (const file of ['native-font-contract.js', 'native-property-contract.js', 'native-application.js']) {
    vm.runInContext(fs.readFileSync(path.join(ROOT, 'src/js/adapters', file), 'utf8'), context, {filename:file});
  }
  context.requestJson = JSON.stringify(request());
  const value = await vm.runInContext(`(() => {
    const q = JSON.parse(requestJson);
    return NemoNativeApplicationAdapter.createNativeApplicationAdapter('browser-read', {dispatch(r) {
      return {apiVersion:2, requestId:r.requestId, instanceId:r.instanceId, documentId:r.documentId,
        contentRevision:7, ok:true, result:{instanceId:r.instanceId, documentId:r.documentId,
          atRevision:7, documentSnapshotId:r.payload.documentSnapshotId, kind:'rawSample',
          target:r.payload.view.target, property:r.payload.view.property, dimension:1, frame:-1.5, value:[20]}};
    }}).readProperty(q);
  })()`, context);
  assert.equal(value.result.value[0], 20); assert.equal(value.contentRevision, 7);
});

test('authored-key read accepts only actual nullable standing lock modes while preserving DTO shape',async()=>{
 for(const lockTo of [null,'in','out','layer']){const q=request('authoredKeys'),r=response(q);r.result.items[0].lockTo=lockTo;assert.deepEqual((await read(q,r)).result.items[0],r.result.items[0]);}
 for(const lockTo of [false,0,'',{},'unknown']){const q=request('authoredKeys'),r=response(q);r.result.items[0].lockTo=lockTo;await assert.rejects(read(q,r),TypeError);}
});
