'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const contract = require('../src/js/adapters/native-audio-track-contract.js');
const {validateRequest, validateResponse, createNativeApplicationAdapter} = require('../src/js/adapters/native-application.js');
const op = 'command.document.object.audio.track.edit';
function request(payload) {
  return {apiVersion:2, requestId:'audio-edit', instanceId:'instance', documentId:'document',
    expectedRevision:7, operation:op, payload};
}
function response(applied) {
  return {apiVersion:2, requestId:'audio-edit', instanceId:'instance', documentId:'document',
    contentRevision:applied ? 8 : 7, ok:true, result:{applied, historyEntriesAdded:applied ? 1 : 0}};
}
function rename(name = '', audioId = 'track') { return {kind:'rename', audioId, name}; }

test('four exact metadata variants preserve absent, null, zero and unclamped finite trims', () => {
  for (const payload of [
    {kind:'setMuted', audioId:'track', muted:true}, {kind:'setMuted', audioId:'track', muted:false},
    rename(), rename('blob:literal name', 'data:literal-id'),
    {kind:'setTrim', audioId:'track', trimStart:-0.625},
    {kind:'setTrim', audioId:'track', trimEnd:null},
    {kind:'setTrim', audioId:'track', trimEnd:0},
    {kind:'setTrim', audioId:'track', trimStart:1e100, trimEnd:-2.75},
    ...['name', 'muted', 'trimStart', 'trimEnd'].map(field => ({kind:'unset', audioId:'track', field})),
  ]) {
    const before = JSON.stringify(payload);
    assert.doesNotThrow(() => contract.payload(op, payload));
    assert.doesNotThrow(() => validateRequest(request(payload)));
    assert.equal(JSON.stringify(payload), before);
  }
  const onlyEnd = {kind:'setTrim', audioId:'track', trimEnd:null};
  contract.payload(op, onlyEnd);
  assert.equal(Object.hasOwn(onlyEnd, 'trimStart'), false);
  assert.equal(onlyEnd.trimEnd, null);
});

test('Unicode byte limits use Rust blank semantics and refuse malformed Unicode without coercion', () => {
  for (const id of ['a'.repeat(255), 'é'.repeat(127) + 'a', '\uFEFF', ' a ', '😀'.repeat(63) + 'abc']) {
    assert.doesNotThrow(() => contract.payload(op, rename('', id)));
  }
  for (const id of ['', '\u0085\u2003', 'a\0', 'a'.repeat(256), 'é'.repeat(128),
    '😀'.repeat(64), '\uD800', '\uDC00', 'x\uD800y']) {
    assert.throws(() => contract.payload(op, rename('', id)), TypeError);
  }
  for (const name of ['', ' \u0085 ', 'é'.repeat(127) + 'a', '😀'.repeat(63) + 'abc']) {
    assert.doesNotThrow(() => contract.payload(op, rename(name)));
  }
  for (const name of ['x\0', 'é'.repeat(128), '😀'.repeat(64), '\uD800', '\uDC00']) {
    assert.throws(() => contract.payload(op, rename(name)), TypeError);
  }
});

test('shape errors, nullable start, nonfinite values and other audio writes are refused', () => {
  for (const payload of [null, [], ['setMuted', 'track', true],
    {kind:'setMuted', audioId:'track', muted:0}, {kind:'setMuted', audioId:'track', muted:'false'},
    {kind:'setMuted', audioId:'track', muted:null}, {kind:'setMuted', audioId:'track'},
    rename(null), rename(0), {kind:'rename', name:'x'}, {kind:'setTrim', audioId:'track'},
    ...[null, undefined, false, '0', NaN, Infinity, -Infinity, new Number(1), 1n]
      .map(trimStart => ({kind:'setTrim', audioId:'track', trimStart})),
    ...[undefined, false, '0', NaN, Infinity].map(trimEnd => ({kind:'setTrim', audioId:'track', trimEnd})),
    {kind:'unset', audioId:'track', field:'volume'}, {kind:'unset', audioId:'track', field:null},
    {kind:'unset', audioId:'track', field:'name', name:'x'},
    {kind:'rename', audioId:'track', name:'x', dataB64:'AAAA'},
    {kind:'setMuted', audioId:'track', muted:true, offsetFrames:0},
    {kind:'setVolume', audioId:'track', volume:0.5}, {kind:'setMotion', audioId:'track', motion:{}},
    {kind:'remove', audioId:'track'}, Object.assign(new Date(), rename('x')),
  ]) assert.throws(() => contract.payload(op, payload), TypeError);
  const nullPrototype = Object.assign(Object.create(null), rename(''));
  assert.doesNotThrow(() => contract.payload(op, nullPrototype));
  assert.throws(() => contract.payload('old-operation', rename()), TypeError);
  const noRevision = request(rename()); delete noRevision.expectedRevision;
  assert.throws(() => validateRequest(noRevision), TypeError);
  const huge = request(rename()); huge.requestId = 'x'.repeat(4096);
  assert.throws(() => validateRequest(huge), TypeError);
});

test('actual shared UI and MCP adapters correlate owner, revision and boolean Command result', async () => {
  for (const label of ['ui', 'mcp']) for (const applied of [false, true]) {
    const req = request({kind:'setMuted', audioId:'track', muted:true});
    const good = response(applied);
    assert.deepEqual(await createNativeApplicationAdapter(label, {dispatch:() => good}).dispatch(req), good);
    for (const bad of [
      {...good, instanceId:'foreign'}, {...good, documentId:'foreign'}, {...good, requestId:'foreign'},
      {...good, contentRevision:applied ? 7 : 8},
      {...good, result:{applied, historyEntriesAdded:applied ? 0 : 1}},
      {...good, result:{...good.result, audioTracks:[]}},
    ]) await assert.rejects(createNativeApplicationAdapter(label, {dispatch:() => bad}).dispatch(req), TypeError);
    const refused = {...good, ok:false, error:{code:'not_found', message:'AudioID not found'}};
    delete refused.result;
    assert.deepEqual(await createNativeApplicationAdapter(label, {dispatch:() => refused}).dispatch(req), refused);
    assert.doesNotThrow(() => validateResponse(good, req.requestId, op));
  }
});

test('pure browser child composes original contract methods and narrow opaque metadata paths', () => {
  const context = {TextEncoder}; vm.createContext(context);
  vm.runInContext(fs.readFileSync(path.join(__dirname, '../src/js/adapters/native-audio-track-contract.js'), 'utf8'), context);
  const api = context.NemoNativeAudioTrackContract;
  assert.equal(api.OP, op); assert.equal(Object.isFrozen(api), true);
  assert.equal(api.admitted(op), true); assert.equal(api.requiresRevision(op), true);
  assert.equal(api.admitted('old'), false);
  for (const name of ['payload.audioId', 'payload.name']) assert.equal(api.paths(op).has(name), true);
  for (const name of ['payload.dataB64', 'payload.offsetFrames', 'result.audioId']) assert.equal(api.paths(op).has(name), false);
  assert.equal(api.paths('old'), null);
  const calls = [];
  const base = {admitted:x => x === 'old', requiresRevision:x => x === 'old',
    payload:(...args) => calls.push(['payload', ...args]), result:(...args) => calls.push(['result', ...args]),
    correlate:(...args) => calls.push(['correlate', ...args]), paths:() => null, requestByteLimit:() => 1234};
  const composed = api.compose(base);
  assert.equal(composed.admitted('old'), true); assert.equal(composed.admitted(op), true);
  assert.equal(composed.requestByteLimit('old'), 1234);
  composed.payload('old', {}); composed.result('old', {}); composed.correlate({operation:'old'}, {});
  assert.deepEqual(calls.map(row => row[0]), ['payload', 'result', 'correlate']);
  assert.equal(Object.isFrozen(composed), true);
});

test('descriptor examples use the registered contract and browser bootstrap child ordering', () => {
  const descriptor = JSON.parse(fs.readFileSync(path.join(__dirname, '../engineering/application/capabilities-v2/native-audio-track.json'), 'utf8'));
  assert.equal(descriptor.id, 'native.audio-track'); assert.deepEqual(descriptor.operations, [op]);
  assert.equal(descriptor.authority.adapterOwnsWritableMirror, false);
  assert.equal(descriptor.authority.fallback, 'none');
  assert.equal(descriptor.resourceBoundary.maxRequestBytes, 4096);
  for (const sample of descriptor.examples) {
    assert.doesNotThrow(() => validateRequest(sample.request));
    assert.doesNotThrow(() => validateResponse(sample.response, sample.request.requestId, op));
  }
  const html = fs.readFileSync(path.join(__dirname, '../src/index.html'), 'utf8');
  const child = 'js/adapters/native-audio-track-contract.js', adapter = 'js/adapters/native-application.js';
  assert.equal(html.split('src="' + child + '"').length - 1, 1);
  assert.ok(html.indexOf(child) < html.indexOf(adapter));
});

// Pin the actual committed declarative request union as well as the SDK ingress.
// These checks are not a substitute for Root's full generated-schema actuals.
test('schema registers one required-revision Audio branch and agrees on closed examples/refusals', () => {
  const schema = JSON.parse(fs.readFileSync(path.join(__dirname, '../engineering/application/native-transport-v2.schema.json'), 'utf8'));
  const branches = schema.$defs.Request.oneOf.filter(branch =>
    branch.allOf[1].properties.operation.const === op);
  assert.equal(branches.length, 1);
  assert.equal(branches[0].allOf[0].$ref, '#/$defs/RequiredRevisionRequest');
  assert.equal(branches[0].allOf[1].properties.payload.$ref, '#/$defs/ObjectAudioTrackPayload');
  assert.ok(schema.$defs.RequiredRevisionRequest.allOf.some(part => part.required?.includes('expectedRevision')));
  assert.equal(schema.$defs.RequestBase.additionalProperties, false);
  const variants = schema.$defs.ObjectAudioTrackPayload.oneOf;
  assert.deepEqual(variants.map(item => item.properties.kind.const).sort(), ['rename', 'setMuted', 'setTrim', 'unset']);
  for (const payload of [rename(''), {kind:'setMuted', audioId:'track', muted:false},
    {kind:'setTrim', audioId:'track', trimStart:0.125, trimEnd:null},
    {kind:'unset', audioId:'track', field:'trimEnd'}]) {
    const variant = variants.find(item => item.properties.kind.const === payload.kind);
    assert.equal(variant.additionalProperties, false);
    assert.ok(variant.required.every(key => Object.hasOwn(payload, key)));
    assert.ok(Object.keys(payload).every(key => Object.hasOwn(variant.properties, key)));
    assert.doesNotThrow(() => validateRequest(request(payload)));
    const unknown = request({...payload, unknown:true});
    assert.equal(Object.hasOwn(variant.properties, 'unknown'), false);
    assert.throws(() => validateRequest(unknown), TypeError);
    const noRevision = request(payload); delete noRevision.expectedRevision;
    assert.throws(() => validateRequest(noRevision), TypeError);
  }
  const trim = variants.find(item => item.properties.kind.const === 'setTrim');
  assert.deepEqual(trim.anyOf, [{required:['trimStart']}, {required:['trimEnd']}]);
  assert.equal(trim.properties.trimStart.type, 'number');
  assert.deepEqual(trim.properties.trimEnd.type, ['number', 'null']);
});
