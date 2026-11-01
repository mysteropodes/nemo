'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const SDK = require('../src/js/adapters/native-application.js');
const ROOT = path.resolve(__dirname, '..');
const OP = 'command.document.object.property.edit';
const request = (action = {kind:'setValue', frame:-0.5, values:[1, 2]}) => ({
  apiVersion:2, requestId:'property-edit-7', instanceId:'instance-A', documentId:'document-A',
  expectedRevision:7, operation:OP, payload:{command:'object.property.edit',
    target:{contextId:'data:opaque context 雪', layerUid:'blob:opaque layer 雪', strokeId:'data:opaque stroke'},
    property:'data:opaque property', action},
});
const reply = (q, applied = true) => ({apiVersion:2, requestId:q.requestId,
  instanceId:q.instanceId, documentId:q.documentId, contentRevision:q.expectedRevision + Number(applied),
  ok:true, result:{applied, historyEntriesAdded:Number(applied)},
});
const adapter = dispatch => SDK.createNativeApplicationAdapter('property-edit-test', {dispatch});
const descriptor = JSON.parse(fs.readFileSync(path.join(ROOT,
  'engineering/application/capabilities-v2/native-property-edit.json')));
const schema = JSON.parse(fs.readFileSync(path.join(ROOT, 'engineering/application/native-transport-v2.schema.json')));

test('D36 declaration keeps mutation admission separate from read-only metadata and closes the typed action subset', () => {
  assert.equal(descriptor.id, 'native.property-edit');
  assert.deepEqual(descriptor.operations, [OP]);
  assert.equal(descriptor.authority.adapterOwnsWritableMirror, false);
  assert.equal(descriptor.authority.fallback, 'none');
  assert.equal(descriptor.resourceBoundary.maxRequestBytes, 4096);
  assert.equal(descriptor.resourceBoundary.maxReadResponseBytes, 4096);
  assert.equal(descriptor.output.$ref, '../native-transport-v2.schema.json#/$defs/ObjectPropertyEditResult');
  const branches = schema.$defs.Request.oneOf.filter(v => v.allOf[1].properties.operation.const === OP);
  assert.equal(branches.length, 1);
  assert.equal(branches[0].allOf[0].$ref, '#/$defs/RequiredRevisionRequest');
  assert.equal(branches[0].allOf[1].properties.payload.$ref, '#/$defs/ObjectPropertyEditPayload');
  const payload = schema.$defs.ObjectPropertyEditPayload;
  assert.equal(payload.additionalProperties, false);
  assert.deepEqual(payload.required, ['command', 'target', 'property', 'action']);
  assert.deepEqual(payload.properties.target.required, ['contextId', 'layerUid']);
  assert.equal(payload.properties.target.additionalProperties, false);
  assert.deepEqual(payload.properties.action.oneOf.map(v => v.properties.kind.const),
    ['setValue', 'setKey', 'removeKey', 'setAnimated', 'bake', 'setExpression', 'setExpressionEnabled', 'removeExpression', 'addControl', 'renameControl', 'removeControl', 'moveControl']);
  for (const action of payload.properties.action.oneOf) {
    assert.equal(action.additionalProperties, false);
    if (/^(?:add|rename|remove|move)Control$/.test(action.properties.kind.const)) {
      assert.equal(action.properties.frame, undefined, 'a control declaration edit carries no frame');
      assert.deepEqual(action.required, ['kind', ...Object.keys(action.properties).filter(k => k !== 'kind')]);
    } else if (['bake', 'removeExpression'].includes(action.properties.kind.const)) {
      assert.deepEqual(action.required, ['kind']);
      assert.deepEqual(Object.keys(action.properties), ['kind']);
    } else if (action.properties.kind.const === 'setExpression') {
      assert.deepEqual(action.required, ['kind', 'code']);
      assert.equal(action.properties.frame, undefined);
      assert.equal(action.properties.code.type, 'string');
      assert.equal(action.properties.code.maxLength, 4096);
    } else if (action.properties.kind.const === 'setExpressionEnabled') {
      assert.deepEqual(action.required, ['kind', 'enabled']);
      assert.equal(action.properties.enabled.type, 'boolean');
    } else {
      assert.equal(action.properties.frame.type, 'number');
      assert.equal(action.properties.frame.minimum, undefined);
    }
    assert.equal(action.properties.curvePoints, undefined);
    if (action.properties.values) {
      assert.equal(action.properties.values.minItems, 1);
      assert.equal(action.properties.values.maxItems, 4);
    }
  }
  const metadata = JSON.parse(fs.readFileSync(path.join(ROOT,
    'engineering/application/capabilities-v2/native-object-property-metadata.json')));
  assert.deepEqual(metadata.operations, ['query.document.object.property.metadata']);
  for (const example of descriptor.examples) {
    assert.doesNotThrow(() => SDK.validateRequest(example.request));
    assert.doesNotThrow(() => SDK.validateResponse(example.response, example.request.requestId, OP));
  }
});

test('all four actions preserve native frames, vectors and opaque selector bytes before a single authority dispatch', async () => {
  for (const frame of [-100.5, -Number.MIN_VALUE, 0, 1.25, 4294967296, Number.MAX_VALUE]) {
    const actions = [{kind:'removeKey', frame}, {kind:'setAnimated', frame, animated:true},
      {kind:'setAnimated', frame, animated:false}];
    for (let dimension = 1; dimension <= 4; dimension++) {
      for (const kind of ['setValue', 'setKey']) actions.push({kind, frame,
        values:Array.from({length:dimension}, (_, i) => i ? -1e20 * i : 123.125)});
    }
    for (const action of actions) {
      const q = request(action), before = structuredClone(q); let sent, calls = 0;
      const response = await adapter(value => {sent = value; calls++; return reply(value);}).editProperty(q);
      assert.equal(calls, 1); assert.notEqual(sent, q);
      assert.deepEqual(sent, before); assert.deepEqual(q, before);
      assert.deepEqual(response, reply(q));
    }
  }
  const q = request(); delete q.payload.target.strokeId;
  assert.deepEqual(await adapter(value => reply(value, false)).editProperty(q), reply(q, false));
});

test('open, missing, positional and untyped input is refused before transport, including custom curvePoints', () => {
  const mutations = [
    q => q.payload = [], q => q.payload.extra = true, q => delete q.payload.command,
    q => q.payload.command = 'object.property.track.edit', q => q.payload.target = [],
    q => q.payload.target.extra = 'data:extra', q => delete q.payload.target.layerUid,
    q => q.payload.target.layerUid = '', q => q.payload.target.strokeId = null,
    q => q.payload.property = 1, q => q.payload.property = '', q => q.payload.action = [],
    q => q.payload.action = null, q => q.payload.action.extra = true,
    q => delete q.payload.action.frame, q => q.payload.action.frame = '0',
    q => q.payload.action.frame = Infinity, q => q.payload.action.frame = NaN,
    q => q.payload.action.values = [], q => q.payload.action.values = [1, 2, 3, 4, 5],
    q => q.payload.action.values = [1, null], q => q.payload.action.values = [1, '2'],
    q => q.payload.action.values = [NaN], q => q.payload.action.values = [Infinity],
    q => q.payload.action.values = new Float32Array([1, 2]),
    q => q.payload.action.values = Array(2), q => {q.payload.action.values = [1, 2]; delete q.payload.action.values[0];},
    q => {const values = Array(1); values.extra = 1; Object.setPrototypeOf(values, {0:2, __proto__:Array.prototype}); q.payload.action.values = values;},
    q => {q.payload.action.values.extra = 1;}, q => {q.payload.action.values.toJSON = () => [9];},
    q => q.payload.action = {kind:'setKey', frame:0, values:[1], curvePoints:null},
    q => q.payload.action = {kind:'setKey', frame:0, values:[1], curvePoints:[]},
    q => q.payload.action = {kind:'removeKey', frame:0, values:[1]},
    q => q.payload.action = {kind:'setAnimated', frame:0, animated:1},
    q => q.payload.action = {kind:'setAnimated', frame:0},
    q => q.payload.action = {kind:'setAnimated', frame:0, animated:true, values:[1]},
    q => q.payload.action = {kind:'preset', frame:0, preset:'ease'},
    q => q.payload.action = {kind:'toString', frame:0},
    q => q.payload.action = {kind:['setValue'], frame:0, values:[1]},
    q => q.payload.action = {kind:['setKey'], frame:0, values:[1]},
    q => q.payload.action = {kind:['removeKey'], frame:0},
    q => q.payload.action = {kind:['setAnimated'], frame:0, animated:true},
  ];
  let calls = 0; const port = adapter(() => {calls++;});
  for (const mutate of mutations) {const q = request(); mutate(q); assert.throws(() => port.editProperty(q));}
  for (const value of [undefined, null, -1, 1.5, Number.MAX_SAFE_INTEGER + 1]) {
    const q = request(); q.expectedRevision = value; assert.throws(() => port.editProperty(q));
  }
  assert.equal(calls, 0);
});

test('opaque resource-looking strings are exempt only at exact property selectors; UTF8 request and response bounds stay 4096', async () => {
  const q = request();
  const remaining = 4096 - Buffer.byteLength(JSON.stringify(q));
  q.payload.target.layerUid += '雪'.repeat(Math.floor(remaining / 3)) + 'x'.repeat(remaining % 3);
  assert.equal(Buffer.byteLength(JSON.stringify(q)), 4096);
  assert.doesNotThrow(() => SDK.validateRequest(q));
  q.payload.target.layerUid += 'x'; assert.throws(() => SDK.validateRequest(q), /4096/);
  for (const mutate of [q => q.requestId = 'data:resource',
    q => q.payload.action.values = ['data:resource'], q => q.payload.action.frame = 'blob:resource']) {
    const q = request(); mutate(q); assert.throws(() => SDK.validateRequest(q));
  }
  for (const code of ['unavailable', 'invalid_request']) {
    const q = request();
    const r = {apiVersion:2, requestId:q.requestId, instanceId:q.instanceId, documentId:q.documentId,
      contentRevision:7, ok:false, error:{code, message:''}};
    r.error.message = 'x'.repeat(4096 - Buffer.byteLength(JSON.stringify(r)));
    assert.equal(Buffer.byteLength(JSON.stringify(r)), 4096);
    assert.deepEqual(await adapter(() => r).editProperty(q), r);
    r.error.message += 'x'; await assert.rejects(adapter(() => r).editProperty(q), /4096/);
  }
  const old = {...request(), operation:'command.document.object.layer.policy.edit',
    payload:{command:'object.layer.policy.edit', layerUid:'x'.repeat(4096), flag:'locked', value:true}};
  assert.throws(() => SDK.validateRequest(old), /4096/);
});

test('changed/no-op/replay receipts carry exact disposition and revision without a local document state', async () => {
  for (const applied of [true, false]) {
    const q = request(), frozen = reply(q, applied); let calls = 0;
    const port = adapter(() => {calls++; return structuredClone(frozen);});
    assert.deepEqual(await port.editProperty(q), frozen);
    assert.deepEqual(await port.dispatch(q), frozen);
    assert.equal(calls, 2, 'native receipt owner receives exact replay');
  }
  const q = request(); q.expectedRevision = Number.MAX_SAFE_INTEGER;
  assert.deepEqual(await adapter(value => reply(value, false)).editProperty(q), reply(q, false));
  await assert.rejects(adapter(value => reply(value)).editProperty(q));
  for (const mutate of [r => r.requestId = 'foreign', r => r.instanceId = 'foreign',
    r => r.documentId = 'foreign', r => r.contentRevision = 7,
    r => r.result.applied = false, r => r.result.historyEntriesAdded = 0,
    r => r.result.historyEntriesAdded = 2, r => r.result.extra = [], r => r.result = []]) {
    await assert.rejects(adapter(value => {const r = reply(value); mutate(r); return r;}).editProperty(request()));
  }
});

test('native refusals, cancellation and transport uncertainty never invent a successful local write or retry', async () => {
  for (const code of ['invalid_request', 'stale_revision', 'not_found', 'unavailable',
    'cancelled_before_dispatch', 'wrong_document']) {
    const q = request(); q.cancelledBeforeDispatch = code === 'cancelled_before_dispatch';
    const before = structuredClone(q); let calls = 0;
    const r = await adapter(value => {calls++; return {apiVersion:2, requestId:value.requestId,
      instanceId:value.instanceId, documentId:code === 'wrong_document' ? 'replacement' : value.documentId,
      contentRevision:8, ok:false, error:{code, message:'Native refusal'}};}).editProperty(q);
    assert.equal(calls, 1); assert.equal(r.error.code, code); assert.deepEqual(q, before);
  }
  let calls = 0; const unknown = new Error('transport outcome unknown');
  const port = adapter(() => {calls++; throw unknown;});
  for (const operation of ['query.document.object.property.metadata', 'command.document.object.property.track.edit',
    'command.document.object.vector.edit']) {const q = request(); q.operation = operation; assert.throws(() => port.editProperty(q));}
  assert.equal(calls, 0);
  assert.throws(() => port.editProperty(request()), e => e === unknown); assert.equal(calls, 1);
});

test('browser classic-script SDK exposes editProperty through the same cohesive private contract', async () => {
  const context = vm.createContext({TextEncoder});
  for (const name of ['native-font-contract.js', 'native-property-contract.js', 'native-application.js']) {
    vm.runInContext(fs.readFileSync(path.join(ROOT, 'src/js/adapters', name), 'utf8'), context, {filename:name});
  }
  context.requestJson = JSON.stringify(request());
  const r = await vm.runInContext(`(() => {
    const api = NemoNativeApplicationAdapter.createNativeApplicationAdapter('browser-property-edit', {
      dispatch(q) {return {apiVersion:2, requestId:q.requestId, instanceId:q.instanceId,
        documentId:q.documentId, contentRevision:8, ok:true, result:{applied:true, historyEntriesAdded:1}};}
    });
    return api.editProperty(JSON.parse(requestJson));
  })()`, context);
  assert.equal(r.result.applied, true); assert.equal(r.contentRevision, 8);
});


test('native bake sends only the strict intent through the existing public edit operation', async () => {
  const q=request({kind:'bake'});let count=0;
  const app=adapter(async sent=>{count++;assert.deepEqual(sent,q);return reply(sent);});
  assert.equal((await app.dispatch(q)).ok,true);assert.equal(count,1);
  for(const action of [{kind:'bake',frame:0},{kind:'bake',values:[1,2]},
    {kind:'bake',samples:[[1,2]]},{kind:'bake',tolerance:100},['bake']]) {
    assert.throws(()=>SDK.validateRequest(request(action)));
  }
  assert.equal(count,1);
});

test('expression authoring sends exact code, enablement and removal through the existing edit operation (#2376)', async () => {
  const unicode = '// café 🧭\r\nconst 値 = 1;\n';
  const actions = [{kind:'setExpression', code:''}, {kind:'setExpression', code:unicode},
    // Authored source is data: a leading data:/blob: spelling is exempt at exactly this field.
    {kind:'setExpression', code:'data:x\nblob:y'}, {kind:'setExpressionEnabled', enabled:true},
    {kind:'setExpressionEnabled', enabled:false}, {kind:'removeExpression'}];
  let count = 0;
  const app = adapter(async sent => { count++; return reply(sent); });
  for (const action of actions) {
    const q = request(action);
    assert.doesNotThrow(() => SDK.validateRequest(q));
    const r = await app.dispatch(q);
    assert.equal(r.ok, true);
  }
  assert.equal(count, actions.length);
  const exact = {kind:'setExpression', code:'x'.repeat(4096 - Buffer.byteLength(JSON.stringify(request({kind:'setExpression', code:''}))))};
  assert.doesNotThrow(() => SDK.validateRequest(request(exact)));
  // Over the code ceiling, or with a frame/curve/legacy writer shape: refused before transport.
  for (const action of [{kind:'setExpression'}, {kind:'setExpression', code:null}, {kind:'setExpression', code:7},
    {kind:'setExpression', code:'x'.repeat(4097)}, {kind:'setExpression', code:'é'.repeat(2049)},
    {kind:'setExpression', code:'x', enabled:true}, {kind:'setExpression', code:'x', frame:0},
    {kind:'setExpression', expression:{enabled:true, code:'x'}}, {kind:'setExpressionEnabled'},
    {kind:'setExpressionEnabled', enabled:1}, {kind:'setExpressionEnabled', enabled:null},
    {kind:'setExpressionEnabled', enabled:true, code:'x'}, {kind:'removeExpression', code:null},
    {kind:'removeExpression', frame:0}, {kind:'clearExpression'}, ['setExpression', 'x']]) {
    assert.throws(() => SDK.validateRequest(request(action)), undefined, JSON.stringify(action));
  }
  assert.equal(count, actions.length);
  // The code string is exempt from the resource-string guard ONLY at action.code.
  const q = request({kind:'setExpressionEnabled', enabled:true}); q.payload.property = 'data:opaque property';
  assert.doesNotThrow(() => SDK.validateRequest(q));
  const bad = request({kind:'setExpressionEnabled', enabled:true}); bad.payload.action.enabled = 'data:x';
  assert.throws(() => SDK.validateRequest(bad));
});

test('D18-B control declaration edits are strict frameless intents through the one property edit operation', async () => {
  const control = (action, property = 'xc_native_a') => {const q = request(action); q.payload.property = property; return q;};
  const accepted = [
    control({kind:'addControl', controlType:'number', name:'Speed'}),
    control({kind:'addControl', controlType:'color', name:'Tint 雪'}, 'xc_k_9'),
    control({kind:'renameControl', name:'  Velocity '}),
    control({kind:'removeControl'}),
    control({kind:'moveControl', direction:-1}),
    control({kind:'moveControl', direction:1}),
  ];
  for (const q of accepted) {
    const before = structuredClone(q); let calls = 0, sent;
    const response = await adapter(value => {calls++; sent = value; return reply(value);}).editProperty(q);
    assert.equal(calls, 1); assert.deepEqual(sent, before); assert.deepEqual(response, reply(q));
  }
  const refused = [
    control({kind:'addControl', controlType:'slider', name:'A'}), control({kind:'addControl', controlType:'number', name:''}),
    control({kind:'addControl', controlType:'number', name:4}), control({kind:'addControl', controlType:'number', name:'x'.repeat(129)}),
    control({kind:'addControl', controlType:'number'}), control({kind:'addControl', name:'A'}),
    control({kind:'addControl', controlType:'number', name:'A', frame:0}),
    control({kind:'addControl', controlType:'number', name:'A'}, 'speed'), control({kind:'addControl', controlType:'number', name:'A'}, 'xc_'),
    control({kind:'addControl', controlType:'number', name:'A'}, 'xc_a-b'), control({kind:'addControl', controlType:'number', name:'A'}, 'position'),
    control({kind:'renameControl'}), control({kind:'renameControl', name:''}), control({kind:'renameControl', name:null}),
    control({kind:'renameControl', name:'A', controlType:'number'}),
    control({kind:'removeControl', frame:0}), control({kind:'removeControl', name:'A'}),
    control({kind:'moveControl'}), control({kind:'moveControl', direction:0}), control({kind:'moveControl', direction:2}),
    control({kind:'moveControl', direction:1.5}), control({kind:'moveControl', direction:'up'}), control({kind:'moveControl', direction:true}),
    control({kind:['removeControl']}),
  ];
  let calls = 0; const port = adapter(() => {calls++;});
  for (const q of refused) assert.throws(() => port.editProperty(q), TypeError, JSON.stringify(q.payload.action));
  assert.equal(calls, 0);
});
