'use strict';
const assert = require('node:assert/strict');
const test = require('node:test');
const fs = require('node:fs');
const { createNativeApplicationAdapter, validateRequest, validateResponse } = require('../src/js/adapters/native-application.js');
const cases = JSON.parse(fs.readFileSync(require('node:path').join(__dirname, '../engineering/application/examples/native-object-v1/cases.json')));
const timingCases = JSON.parse(fs.readFileSync(require('node:path').join(__dirname, '../engineering/application/examples/native-layer-timing-v1/cases.json')));
const { createNativeTauriTransport } = require('../src/js/adapters/application-mcp.js');
const copy = value => structuredClone(value);
function timingRead(frame = 2) {
  return { apiVersion: 2, requestId: 'timing-read', instanceId: 'instance-A', documentId: 'document-A',
    operation: 'query.document.object.layer.timing', payload: { atRevision: 0, documentSnapshotId: 'native-object:document-A:0',
      contextId: 'scene-root', layerUid: 'data:opaque 雪', frame } };
}
function timingResponse(request) {
  return { apiVersion: 2, requestId: request.requestId, instanceId: request.instanceId, documentId: request.documentId,
    contentRevision: request.payload.atRevision, ok: true, result: { instanceId: request.instanceId, documentId: request.documentId,
      ...request.payload, ...timingCases.drawing.expected[request.payload.frame] } };
}
function read() { return copy(cases.read.request); }
function readResponse(request) {
  const value = copy(cases.read.response);
  Object.assign(value, { requestId: request.requestId, instanceId: request.instanceId,
    documentId: request.documentId, contentRevision: request.payload.atRevision });
  value.result.atRevision = request.payload.atRevision;
  value.result.documentSnapshotId = `native-object:${request.documentId}:${request.payload.atRevision}`;
  value.result.object.target = copy(request.payload.stableTarget);
  return value;
}
function presentationRead(revision = 7) {
  return { apiVersion: 2, requestId: 'presentation-read', instanceId: 'instance-A', documentId: 'document-A',
    operation: 'query.document.object', payload: { atRevision: revision, view: { kind: 'presentation' } } };
}
function presentationResponse(request) {
  return { apiVersion: 2, requestId: request.requestId, instanceId: request.instanceId, documentId: request.documentId,
    contentRevision: request.payload.atRevision, ok: true, result: { kind: 'presentation', atRevision: request.payload.atRevision,
      documentSnapshotId: `native-object:${request.documentId}:${request.payload.atRevision}`, contextId: 'scene-root',
      frame: 11, workId: 'mesh-work-fixture-11', viewGeneration: 13 } };
}
function visibility(visible = false) {
  return { apiVersion: 2, requestId: 'visibility-7', instanceId: 'instance-A', documentId: 'document-A',
    operation: 'command.document.object.layer.visibility.set', expectedRevision: 7,
    payload: { command: 'object.layer.visibility.set', layerUid: 'legacy layer:alpha', visible } };
}
function commandResponse(request, applied) {
  return { apiVersion: 2, requestId: request.requestId, instanceId: request.instanceId, documentId: request.documentId,
    contentRevision: request.expectedRevision + (applied ? 1 : 0), ok: true,
    result: { applied, historyEntriesAdded: applied ? 1 : 0 } };
}
function adapter(respond) {
  const calls = [];
  return { calls, api: createNativeApplicationAdapter('object-sdk', { dispatch: async request => {
    calls.push(copy(request)); return respond(request);
  } }) };
}
function change(value, pointer, replacement, remove = false) {
  const result = copy(value), keys = Array.isArray(pointer) ? pointer : pointer.split('/').slice(1);
  let parent = result;
  for (const key of keys.slice(0, -1)) parent = parent[key];
  if (remove) delete parent[keys.at(-1)]; else parent[keys.at(-1)] = replacement;
  return result;
}

test('actual shared adapter preserves complete pinned read and opaque targets without caller mutation', async () => {
  for (const uid of ['0', 'legacy layer:alpha', 'data:layer-id', 'blob:layer-id', '层 / alpha']) {
    const request = read(); request.payload.stableTarget.layerUid = uid;
    request.payload.stableTarget.strokeId = `stroke ${uid}`;
    const original = copy(request), h = adapter(readResponse);
    const result = await h.api.dispatch(request);
    assert.deepEqual(result, readResponse(original));
    assert.deepEqual(h.calls, [original]); assert.deepEqual(request, original);
    assert.notStrictEqual(h.calls[0], request);
  }
});

test('presentation bootstrap is closed, bounded and correlated through the shared SDK', async () => {
  const request = presentationRead(), original = copy(request), h = adapter(presentationResponse);
  const result = await h.api.dispatch(request);
  assert.deepEqual(result, presentationResponse(original));
  assert.deepEqual(h.calls, [original]); assert.deepEqual(request, original);
  assert.equal(result.result.kind, 'presentation'); assert.equal(result.result.atRevision, request.payload.atRevision);
  for (const [path, value, remove] of [
    ['/payload/atRevision', -1], ['/payload/atRevision', Number.MAX_SAFE_INTEGER + 1],
    ['/payload/view', null], ['/payload/view/kind', 'imageMesh'], ['/payload/view/extra', true],
    ['/payload/expectedRevision', 7], ['/expectedRevision', 7], ['/payload/extra', true],
  ]) assert.throws(() => validateRequest(change(request, path, value, remove)), undefined, path);
  for (const [path, replacement, remove] of [
    ['/result/extra', true], ['/result/kind', 'imageMesh'], ['/result/documentSnapshotId', ''],
    ['/result/contextId', ''], ['/result/frame', -1], ['/result/frame', 4294967296],
    ['/result/viewGeneration', 0], ['/result/viewGeneration', Number.MAX_SAFE_INTEGER + 1],
    ['/contentRevision', 8], ['/result/atRevision', 8], ['/requestId', 'different-request'],
    ['/instanceId', 'different-instance'], ['/documentId', 'different-document'],
    ['/result/workId', 'x'.repeat(129)],
  ]) await assert.rejects(adapter(req => change(presentationResponse(req), path, replacement, remove)).api.dispatch(request), undefined, path);
  await assert.rejects(adapter(req => ({ ...presentationResponse(req), result: null })).api.dispatch(request), /result/);
  const stale = copy(presentationResponse(request)); stale.result.documentSnapshotId = 'native-object:document-A:8';
  await assert.rejects(adapter(() => stale).api.dispatch(request), /snapshot mismatch/);
  const foreign = copy(presentationResponse(request)); foreign.result.documentSnapshotId = 'native-object:other-document:7';
  await assert.rejects(adapter(() => foreign).api.dispatch(request), /snapshot mismatch/,
    'a correctly correlated envelope cannot carry another document snapshot');
});

test('visibility uses required revision, strict boolean and exact changed/no-op receipt', async () => {
  for (const visible of [false, true]) for (const applied of [false, true]) {
    const request = visibility(visible), h = adapter(value => commandResponse(value, applied));
    assert.deepEqual(await h.api.dispatch(request), commandResponse(request, applied));
    assert.deepEqual(h.calls, [request]);
  }
  const request = visibility(); request.payload.layerUid = 'data:opaque-layer';
  assert.strictEqual(validateRequest(request), request);
});

test('reference frame scope uses the same closed record contract and exact selector', async () => {
  const record = cases.records.find(value => value.target.frameScope.kind === 'reference');
  assert.ok(record, 'retained reference oracle must exist');
  const request = read(); request.payload.stableTarget = copy(record.target);
  const response = readResponse(request); response.result.object = copy(record);
  assert.deepEqual(await adapter(() => response).api.dispatch(request), response);
});

test('malformed visibility inputs refuse before transport', async () => {
  const request = visibility(), values = [
    change(request, '/expectedRevision', null), change(request, '/expectedRevision', -1),
    change(request, '/expectedRevision', 0, true), change(request, '/payload/visible', 0),
    change(request, '/payload/visible', 'false'), change(request, '/payload/visible', null),
    change(request, '/payload/visible', false, true), change(request, '/payload/layerUid', ''),
    change(request, '/payload/layerUid', 1), change(request, '/payload/command', 'layer.visibility.set'),
    change(request, '/payload/extra', true), change(request, '/payload', []),
    change(request, '/operation', 'command.document.object.fill.set'),
  ];
  for (const value of values) {
    const h = adapter(() => { throw Error('must not dispatch'); });
    assert.throws(() => h.api.dispatch(value)); assert.equal(h.calls.length, 0);
  }
});

test('object query selectors are closed, pinned, u32 framed and opaque', () => {
  const request = read();
  for (const [path, value] of [
    ['/payload/atRevision', -1], ['/payload/atRevision', Number.MAX_SAFE_INTEGER + 1],
    ['/expectedRevision', 7], ['/payload/stableTarget/contextId', 'other-scene'],
    ['/payload/stableTarget/layerUid', ''], ['/payload/stableTarget/strokeId', false],
    ['/payload/stableTarget/frameScope/kind', 'evaluated'], ['/payload/stableTarget/frameScope/frame', 4294967296],
    ['/payload/stableTarget/frameScope/frame', 0.5], ['/payload/stableTarget/extra', true],
    ['/payload/stableTarget/frameScope', []], ['/payload/extra', true],
  ]) assert.throws(() => validateRequest(change(request, path, value)), path);
});

test('independent retained invalid-record corpus rejects unsupported geometry/fill/target shapes', () => {
  const request = read(), response = readResponse(request);
  for (const invalid of cases.invalidRecords) {
    const value = copy(response);
    const replacement = invalid.jsonValue ? JSON.parse(invalid.jsonValue) : invalid.value;
    value.result.object = change(value.result.object, invalid.path, replacement, invalid.delete);
    assert.throws(() => validateResponse(value, request.requestId, request.operation), invalid.label);
  }
  for (const value of [NaN, Infinity, '1', true]) {
    const response = readResponse(request); response.result.object.geometry.segments[0].point.x = value;
    assert.throws(() => validateResponse(response, request.requestId, request.operation));
  }
});

test('valid-looking read replies cannot substitute identity, selected revision, snapshot or target', async () => {
  const request = read();
  for (const [path, value] of [
    ['/requestId', 'other-request'], ['/instanceId', 'other-instance'], ['/documentId', 'other-document'],
    ['/contentRevision', 8], ['/result/atRevision', 8], ['/result/documentSnapshotId', 'snapshot-A-seven'],
    ['/result/object/target/layerUid', 'sibling'], ['/result/object/target/strokeId', 'sibling-stroke'],
    ['/result/object/target/frameScope/kind', 'reference'], ['/result/object/target/frameScope/frame', 8],
  ]) {
    const h = adapter(() => change(readResponse(request), path, value));
    await assert.rejects(h.api.dispatch(request), undefined, path); assert.equal(h.calls.length, 1);
  }
  const historical = copy(cases.read.response);
  assert.throws(() => validateResponse(historical, request.requestId, request.operation));
});

test('visibility replies cannot invent disposition, history, revisions or owner identity', async () => {
  const request = visibility();
  for (const [path, value] of [
    ['/result/applied', 'true'], ['/result/historyEntriesAdded', 0], ['/result/historyEntriesAdded', 2],
    ['/result/extra', true], ['/contentRevision', 7], ['/contentRevision', 9],
    ['/instanceId', 'other-instance'], ['/documentId', 'other-document'],
  ]) await assert.rejects(adapter(() => change(commandResponse(request, true), path, value)).api.dispatch(request));
  await assert.rejects(adapter(() => change(commandResponse(request, false), '/contentRevision', 8)).api.dispatch(request));
});

test('complete envelope bounds and resource prohibition remain enforced outside opaque UID fields', () => {
  const request = read(); request.payload.stableTarget.layerUid = '层'.repeat(1500);
  assert.throws(() => validateRequest(request), /4096/);
  const bounded = read(), response = readResponse(bounded);
  response.result.object.target.layerUid = '层'.repeat(1500);
  assert.throws(() => validateResponse(response, bounded.requestId, bounded.operation), /4096/);
  const command = visibility(), tooLarge = commandResponse(command, true);
  tooLarge.requestId = 'a'.repeat(4097);
  assert.throws(() => validateResponse(tooLarge, tooLarge.requestId, command.operation), /4096/);
  const resource = readResponse(bounded); resource.result.object.geometry.segments[0].point.x = 'data:raw-pixels';
  assert.throws(() => validateResponse(resource, bounded.requestId, bounded.operation), /resource/);
  assert.throws(() => validateRequest({ ...visibility(), payload: { ...visibility().payload, pixels: new Uint8Array(4) } }));
});

test('typed replacement refusal stays observable without successful wrong-owner data', async () => {
  const request = visibility(), response = { ...commandResponse(request, false), documentId: 'replacement-document', ok: false,
    error: { code: 'wrong_document', message: 'Owner was replaced' } };
  delete response.result;
  assert.deepEqual(await adapter(() => response).api.dispatch(request), response);
  await assert.rejects(adapter(() => ({ ...response, error: { code: 'unavailable', message: 'not a replacement' } })).api.dispatch(request));
});

function creation(name = 'Drawing') {
  return { ...visibility(), operation: 'command.document.object.layer.create',
    payload: { command: 'object.layer.create', name } };
}
function created(request) {
  return { ...commandResponse(request, true),
    result: { applied: true, historyEntriesAdded: 1, layerUid: 'native-layer:8:0' } };
}

test('prepared creation adapter preserves scalar display names and native identity without claiming availability', async () => {
  for (const name of ['', 'Drawing', '层 / ink', 'data:display name', 'blob:display name']) {
    const request = creation(name), original = copy(request), h = adapter(created);
    assert.strictEqual(validateRequest(request), request);
    assert.deepEqual(await h.api.dispatch(request), created(request));
    assert.deepEqual(request, original); assert.deepEqual(h.calls, [original]);
  }
  const request = creation(), refusal = { ...created(request), contentRevision: 7, ok: false,
    error: { code: 'unavailable', message: 'Native timeline reader pending' } };
  delete refusal.result;
  assert.deepEqual(await adapter(() => refusal).api.dispatch(request), refusal);
});

test('creation shape rejects malformed caller identity, raw resource extras and invalid name before transport', () => {
  const base = creation();
  for (const [pointer, value, remove] of [
    ['/payload', []], ['/payload/name', null], ['/payload/name', 1], ['/payload/name', false],
    ['/payload/name', '', true], ['/payload/command', 'layer.create'], ['/payload/layerUid', 'caller-id'],
    ['/payload/frames', []], ['/payload/pixels', new Uint8Array(4)],
    ['/expectedRevision', 7, true], ['/expectedRevision', null], ['/expectedRevision', -1],
    ['/expectedRevision', Number.MAX_SAFE_INTEGER + 1], ['/payload/name', '层'.repeat(1500)],
    ['/payload/data', 'data:raw-pixels'],
  ]) {
    const h = adapter(() => { throw Error('must not dispatch'); });
    assert.throws(() => h.api.dispatch(change(base, pointer, value, remove)), undefined, pointer);
    assert.equal(h.calls.length, 0);
  }
});

test('creation success must retain exact disposition UID and expected plus one revision in the same owner', async () => {
  const request = creation(), response = created(request);
  for (const [pointer, value, remove] of [
    ['/result/layerUid', '', true], ['/result/layerUid', ''], ['/result/layerUid', null],
    ['/result/layerUid', 1], ['/result/applied', false], ['/result/applied', 'true'],
    ['/result/historyEntriesAdded', 0], ['/result/historyEntriesAdded', 2], ['/result/extra', true],
    ['/contentRevision', 7], ['/contentRevision', 9], ['/instanceId', 'other'], ['/documentId', 'other'],
    ['/requestId', 'other'], ['/result/layerUid', '层'.repeat(1500)],
  ]) await assert.rejects(adapter(() => change(response, pointer, value, remove)).api.dispatch(request), undefined, pointer);
  const bounded = creation(); bounded.expectedRevision = Number.MAX_SAFE_INTEGER;
  await assert.rejects(adapter(created).api.dispatch(bounded));
  const historical = created(request);
  assert.deepEqual(await adapter(() => historical).api.dispatch(request), historical,
    'retained historical UID remains valid; this is not a current-presence query');
});

test('timing SDK preserves the accepted eight-row oracle and exact opaque selector without mutation', async () => {
  for (const expected of timingCases.drawing.expected) {
    const request = timingRead(expected.frame), before = copy(request), reply = timingResponse(request), a = adapter(() => reply);
    assert.deepEqual(await a.api.dispatch(request), reply); assert.deepEqual(request, before);
    assert.deepEqual(a.calls[0], before); assert.equal(reply.result.layerUid, 'data:opaque 雪');
    for (const key of ['cellKind', 'sourceFrame', 'hasContent', 'manualEdit']) assert.equal(reply.result[key], expected[key]);
  }
});
test('timing selector rejects malformed values before sending and retains typed unsupported refusals', async () => {
  for (const [pointer, value, remove] of [
    ['/expectedRevision', 0], ['/payload/lifecycleGeneration', 1], ['/payload/atRevision', -1], ['/payload/atRevision', Number.MAX_SAFE_INTEGER + 1],
    ['/payload/documentSnapshotId', ''], ['/payload/contextId', ''], ['/payload/layerUid', ''], ['/payload/layerUid', null],
    ['/payload/frame', -1], ['/payload/frame', 1.5], ['/payload/frame', 4294967296], ['/payload/frame', 0, true], ['/payload/extra', true],
  ]) {
    const a = adapter(timingResponse); await assert.rejects(async () => a.api.dispatch(change(timingRead(), pointer, value, remove))); assert.equal(a.calls.length, 0);
  }
  const request = timingRead(); request.payload.contextId = 'component-context';
  const refusal = { ...timingResponse(request), ok: false, error: { code: 'unavailable', message: 'Unsupported timing context' } }; delete refusal.result;
  assert.deepEqual(await adapter(() => refusal).api.dispatch(request), refusal);
  const maxFrame = timingRead(); maxFrame.payload.frame = 4294967295; assert.equal(validateRequest(maxFrame), maxFrame);
  const maxRevision = timingRead(); maxRevision.payload.atRevision = Number.MAX_SAFE_INTEGER; assert.equal(validateRequest(maxRevision), maxRevision);
});
test('timing replies cannot substitute owner, snapshot, selector or exposure semantics', async () => {
  const request = timingRead(), response = timingResponse(request);
  for (const [pointer, value, remove] of [
    ['/instanceId', 'other'], ['/documentId', 'other'], ['/contentRevision', 1], ['/requestId', 'other'],
    ['/result/instanceId', 'other'], ['/result/documentId', 'other'], ['/result/atRevision', 1], ['/result/documentSnapshotId', 'other'],
    ['/result/contextId', 'other'], ['/result/layerUid', 'other'], ['/result/frame', 1], ['/result/cellKind', 'key'],
    ['/result/sourceFrame', 2], ['/result/manualEdit', true], ['/result/hasContent', 1], ['/result/extra', true], ['/result/frame', 0, true],
  ]) await assert.rejects(adapter(() => change(response, pointer, value, remove)).api.dispatch(request), undefined, pointer);
  const beforeKey = timingRead(0), empty = timingResponse(beforeKey);
  for (const patch of [{ sourceFrame: 0 }, { hasContent: true }, { manualEdit: true }]) {
    await assert.rejects(adapter(() => ({ ...empty, result: { ...empty.result, ...patch } })).api.dispatch(beforeKey));
  }
  const blankKey = timingRead(5); assert.equal((await adapter(timingResponse).api.dispatch(blankKey)).result.hasContent, false);
});
test('timing request and COMPLETE response enforce actual UTF8 4096 bounds with only UID opacity', async () => {
  const request = timingRead(), bytes = value => Buffer.byteLength(JSON.stringify(value));
  request.payload.layerUid = 'data:雪';
  while (bytes(timingResponse(request)) < 4096) request.payload.layerUid += 'x';
  const exact = timingResponse(request); assert.equal(bytes(exact), 4096); assert.ok(bytes(request) < 4096);
  assert.deepEqual(await adapter(() => exact).api.dispatch(request), exact);
  request.payload.layerUid += 'x'; await assert.rejects(adapter(timingResponse).api.dispatch(request));
  while (bytes(request) < 4096) request.payload.layerUid += 'x';
  assert.equal(bytes(request), 4096); assert.equal(validateRequest(request), request);
  request.payload.layerUid += 'x'; assert.throws(() => validateRequest(request));
  const resource = timingRead(); resource.payload.documentSnapshotId = 'data:binary'; assert.throws(() => validateRequest(resource));
});
test('actual Tauri transport retires delayed timing replies across disconnect/reconnect without a UI write', async () => {
  let finish, documentId = 'document-A'; const calls = [];
  const transport = createNativeTauriTransport(async (command, args) => {
    calls.push(command); if (command === 'nemo_native_status') return { apiVersion: 2, available: true, instanceId: 'instance-A', documentId, contentRevision: 0 };
    assert.equal(command, 'nemo_native_dispatch'); return new Promise(resolve => { finish = () => resolve(timingResponse(args.request)); });
  });
  await transport.connect(); const api = createNativeApplicationAdapter('timing-sdk', transport), pending = api.dispatch(timingRead());
  const rejected = assert.rejects(pending, /disconnected/); await transport.disconnect(); documentId = 'document-B'; await transport.connect();
  finish(); await rejected; assert.equal(transport.status().documentId, 'document-B'); assert.equal(calls.filter(x => x === 'nemo_native_dispatch').length, 1);
});
test('actual Tauri transport permits committed-head timing reads while publication awaits its own ACK', async () => {
  const listeners = new Map(), actions = []; let finish;
  const binding = { instanceId: 'instance-A', documentId: 'document-A', contentRevision: 0, lifecycleGeneration: 1, subscriptionId: 'timing-sub' };
  const transport = createNativeTauriTransport(async (command, args) => {
    if (command === 'nemo_native_status') return { apiVersion: 2, available: true, ...binding };
    if (command === 'nemo_native_revision_sync') { actions.push(args.request.action); return args.request.action === 'acknowledge' ? null : binding; }
    assert.equal(command, 'nemo_native_dispatch'); return timingResponse(args.request);
  });
  await transport.connect(); const gate = new Promise(resolve => { finish = resolve; });
  await transport.subscribeRevisions(async (name, callback) => { listeners.set(name, callback); return () => {}; }, async event => { await gate; return event; });
  const event = { instanceId: binding.instanceId, documentId: binding.documentId, lifecycleGeneration: 1, requestId: 'external-write', fromRevision: 0, toRevision: 1 };
  const applying = listeners.get('nemo-native-revision')({ payload: event }); await new Promise(setImmediate);
  assert.equal(transport.isSynchronizing(), true);
  const request = timingRead(); request.payload.atRevision = 1; request.payload.documentSnapshotId = 'native-object:document-A:1';
  const response = await createNativeApplicationAdapter('pending-reader', transport).dispatch(request);
  assert.equal(response.ok, true); assert.equal(actions.includes('acknowledge'), false); assert.equal(transport.isSynchronizing(), true);
  finish(); await applying; assert.equal(actions.filter(action => action === 'acknowledge').length, 1); assert.equal(transport.isSynchronizing(), false);
});
test('actual Tauri transport invalidates a successful wrong-owner timing response before SDK delivery', async () => {
  const transport = createNativeTauriTransport(async (command, args) => command === 'nemo_native_status'
    ? { apiVersion: 2, available: true, instanceId: 'instance-A', documentId: 'document-A', contentRevision: 0 }
    : { ...timingResponse(args.request), documentId: 'document-B' });
  await transport.connect(); await assert.rejects(createNativeApplicationAdapter('wrong-owner', transport).dispatch(timingRead()));
  assert.equal(transport.status(), null);
});

test('range-only timing view is closed correlated and distinct from original cell exposure',async()=>{
 const q=timingRead();q.payload.view='range';const range={rawInPoint:-2.5,rawOutPoint:30,inPoint:0,outPoint:20,hasCustomRange:true,hasTimeLink:false};
 function response(request){const {view,...selector}=request.payload;return {apiVersion:2,requestId:request.requestId,instanceId:request.instanceId,documentId:request.documentId,contentRevision:selector.atRevision,ok:true,result:{kind:'layerRange',instanceId:request.instanceId,documentId:request.documentId,...selector,range:{...range}}};}
 const h=adapter(response);assert.deepEqual((await h.api.dispatch(q)).result.range,range);assert.equal(h.calls.length,1);
 for(const mutate of [r=>r.result.range.rawInPoint=Infinity,r=>r.result.range.hasTimeLink=null,r=>delete r.result.range.rawOutPoint,r=>r.result.range.extra=0,r=>r.result.layerUid='foreign',r=>r.result.frame++]){
   const bad=adapter(request=>{const r=response(request);mutate(r);return r;});await assert.rejects(bad.api.dispatch(q),TypeError);
 }
 const original=timingRead();await assert.rejects(adapter(response).api.dispatch(original),TypeError);
 await assert.rejects(adapter(timingResponse).api.dispatch(q),TypeError);
 for(const view of [null,'timing',{},false])assert.throws(()=>validateRequest({...q,payload:{...q.payload,view}}),TypeError);
});
