'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const SDK = require('../src/js/adapters/native-application.js');
const OP = 'command.document.object.property.track.edit';
const ROOT = path.resolve(__dirname, '..');
const request = (action = {kind:'preset', preset:'ease'}) => ({
  apiVersion:2, requestId:'track-7', instanceId:'instance-A', documentId:'document-A',
  expectedRevision:7, operation:OP,
  payload:{command:'object.property.track.edit', selections:[{
    target:{contextId:'scene-root', layerUid:'data:authored layer 雪'}, property:'position', frame:0,
  }], action},
});
const reply = (q, applied = true) => ({
  apiVersion:2, requestId:q.requestId, instanceId:q.instanceId, documentId:q.documentId,
  contentRevision:q.expectedRevision + Number(applied), ok:true,
  result:{applied, historyEntriesAdded:Number(applied)},
});
const adapter = dispatch => SDK.createNativeApplicationAdapter('track-test', {dispatch});

test('D12 descriptor declares the exact existing native operation and closed selection/action contracts', () => {
  const descriptor = JSON.parse(fs.readFileSync(path.join(ROOT,
    'engineering/application/capabilities-v2/native-property-track-edit.json'), 'utf8'));
  const schema = JSON.parse(fs.readFileSync(path.join(ROOT,
    'engineering/application/native-transport-v2.schema.json'), 'utf8'));
  assert.deepEqual(descriptor.operations, [OP]);
  assert.equal(descriptor.authority.adapterOwnsWritableMirror, false);
  assert.equal(descriptor.authority.fallback, 'none');
  assert.equal(descriptor.resourceBoundary.maxRequestBytes, 4096);
  const branches = schema.$defs.Request.oneOf.filter(v => v.allOf[1].properties.operation.const === OP);
  assert.equal(branches.length, 1);
  assert.equal(branches[0].allOf[0].$ref, '#/$defs/RequiredRevisionRequest');
  assert.equal(branches[0].allOf[1].properties.payload.$ref, '#/$defs/ObjectPropertyTrackPayload');
  const shape = schema.$defs.ObjectPropertyTrackPayload;
  assert.equal(shape.additionalProperties, false);
  assert.deepEqual(shape.required, ['command', 'selections', 'action']);
  assert.equal(shape.properties.selections.minItems, 1);
  assert.equal(shape.properties.selections.maxItems, 32);
  assert.equal(shape.properties.selections.items.additionalProperties, false);
  assert.equal(shape.properties.selections.items.properties.target.additionalProperties, false);
  assert.deepEqual(shape.properties.action.oneOf.map(a => a.properties.kind.const), ['preset', 'influence', 'subdivide']);
  for (const example of descriptor.examples) {
    assert.doesNotThrow(() => SDK.validateRequest(example.request));
    assert.doesNotThrow(() => SDK.validateResponse(example.response, example.request.requestId, OP));
  }
});

test('SDK reaches the authority with every preset, influence edge, subdivision and opaque element selector', async () => {
  const actions = ['linear', 'ease', 'easeIn', 'easeOut', 'hold'].map(preset => ({kind:'preset', preset}));
  for (const side of ['in', 'out']) for (const percent of [-1, 0, 50.5, 100, 101]) actions.push({kind:'influence', side, percent});
  actions.push({kind:'subdivide'});
  for (const action of actions) {
    const q = request(action);
    q.payload.selections[0].target.strokeId = 'blob:authored element 雪';
    q.payload.selections[0].property = 'data:opaque control';
    q.payload.selections[0].frame = 0.5;
    const before = structuredClone(q);
    let sent;
    const r = await adapter(value => { sent = value; return reply(value); }).editPropertyTrack(q);
    assert.deepEqual(sent, before, 'finite influence is preserved for the native baseline clamp');
    assert.notEqual(sent, q);
    assert.deepEqual(q, before);
    assert.deepEqual(r, reply(q));
  }
});

test('SDK rejects open, missing, null and positional shapes before any dispatch', () => {
  const mutations = [
    q => q.payload = ['object.property.track.edit', [], {}],
    q => q.payload.extra = true, q => delete q.payload.command,
    q => q.payload.command = 'object.property.edit', q => q.payload.action = [],
    q => q.payload.action = null, q => q.payload.selections = null,
    q => q.payload.selections = [], q => q.payload.selections = Array(33).fill(q.payload.selections[0]),
    q => q.payload.selections[0] = [], q => q.payload.selections[0].extra = 1,
    q => delete q.payload.selections[0].target,
    q => q.payload.selections[0].target = [], q => q.payload.selections[0].target.extra = 1,
    q => q.payload.selections[0].target.layerUid = '',
    q => q.payload.selections[0].target.strokeId = null,
    q => q.payload.selections[0].target.contextId = 2,
    q => q.payload.selections[0].property = '', q => q.payload.selections[0].property = null,
    q => delete q.payload.selections[0].frame,
    q => q.payload.selections[0].frame = '0', q => q.payload.selections[0].frame = NaN,
    q => q.payload.action = {kind:'preset', preset:'smooth'},
    q => q.payload.action = {kind:'preset', preset:'ease', extra:0},
    q => q.payload.action = {kind:'influence', side:'both', percent:50},
    q => q.payload.action = {kind:'influence', side:'out', percent:Infinity},
    q => q.payload.action = {kind:'influence', side:'out', percent:'50'},
    q => q.payload.action = {kind:'subdivide', values:[]},
    q => q.payload.action = {kind:'subdivide', steps:2},
  ];
  let calls = 0;
  const port = adapter(() => { calls++; });
  for (const mutate of mutations) { const q = request(); mutate(q); assert.throws(() => port.editPropertyTrack(q)); }
  for (const value of [undefined, null, -1, 1.5, Number.MAX_SAFE_INTEGER + 1]) {
    const q = request(); q.expectedRevision = value;
    assert.throws(() => port.editPropertyTrack(q));
  }
  assert.equal(calls, 0);
});

test('maximum selection count and exact UTF8 envelope bytes are enforced without narrowing existing operations', () => {
  const q = request();
  q.payload.selections = Array.from({length:32}, (_, frame) => ({...structuredClone(q.payload.selections[0]), frame}));
  assert.doesNotThrow(() => SDK.validateRequest(q));
  const exact = request();
  const bytes = Buffer.byteLength(JSON.stringify(exact));
  exact.payload.selections[0].target.layerUid += 'x'.repeat(4096 - bytes);
  assert.equal(Buffer.byteLength(JSON.stringify(exact)), 4096);
  assert.doesNotThrow(() => SDK.validateRequest(exact));
  exact.payload.selections[0].target.layerUid += 'x';
  assert.throws(() => SDK.validateRequest(exact), /4096/);
  const old = {...request(), operation:'command.document.object.layer.policy.edit',
    payload:{command:'object.layer.policy.edit', layerUid:'x'.repeat(4096), flag:'locked', value:true}};
  assert.throws(() => SDK.validateRequest(old), /4096/);
});

test('native duplicate-selector refusal crosses the SDK without inventing local document state or retries', async () => {
  const q = request(); q.payload.selections.push(structuredClone(q.payload.selections[0]));
  let calls = 0;
  const result = await adapter(value => {
    calls++;
    return {apiVersion:2, requestId:value.requestId, instanceId:value.instanceId, documentId:value.documentId,
      contentRevision:7, ok:false, error:{code:'invalid_request', message:'Duplicate selected native key'}};
  }).editPropertyTrack(q);
  assert.equal(calls, 1);
  assert.equal(result.error.code, 'invalid_request');
});

test('changed, no-op and replay receipts correlate exact revision and contain no payload state', async () => {
  for (const applied of [true, false]) {
    const q = request(); const frozen = reply(q, applied); let calls = 0;
    const port = adapter(() => { calls++; return structuredClone(frozen); });
    assert.deepEqual(await port.editPropertyTrack(q), frozen);
    assert.deepEqual(await port.dispatch(q), frozen);
    assert.equal(calls, 2, 'replays must reach the sole native receipt owner');
  }
  const q = request(); q.expectedRevision = Number.MAX_SAFE_INTEGER;
  assert.deepEqual(await adapter(value => reply(value, false)).editPropertyTrack(q), reply(q, false));
  await assert.rejects(adapter(value => reply(value, true)).editPropertyTrack(q));
});

test('SDK refuses false ACKs, mismatched owner pins, open results and oversized errors', async () => {
  const mutations = [r => r.requestId = 'foreign', r => r.instanceId = 'foreign', r => r.documentId = 'foreign',
    r => r.contentRevision = 7, r => r.result.applied = false, r => r.result.historyEntriesAdded = 0,
    r => r.result.historyEntriesAdded = 2, r => r.result.extra = [], r => r.result = ['applied', true]];
  for (const mutate of mutations) {
    await assert.rejects(adapter(q => { const r = reply(q); mutate(r); return r; }).editPropertyTrack(request()));
  }
  await assert.rejects(adapter(q => ({apiVersion:2, requestId:q.requestId, instanceId:q.instanceId,
    documentId:q.documentId, contentRevision:7, ok:false, error:{code:'unavailable', message:'x'.repeat(4096)}})).editPropertyTrack(request()));
});

test('stale, cancelled and replaced-document refusals remain typed and caller input stays unchanged', async () => {
  for (const code of ['stale_revision', 'cancelled_before_dispatch', 'wrong_document', 'unavailable']) {
    const q = request(); q.cancelledBeforeDispatch = code === 'cancelled_before_dispatch';
    const before = structuredClone(q);
    const r = await adapter(value => ({apiVersion:2, requestId:value.requestId, instanceId:value.instanceId,
      documentId:code === 'wrong_document' ? 'replacement-document' : value.documentId,
      contentRevision:8, ok:false, error:{code, message:'Native refusal'}})).editPropertyTrack(q);
    assert.equal(r.error.code, code); assert.deepEqual(q, before);
  }
});

test('explicit track method does not admit another property command or recover an indeterminate transport', async () => {
  let calls = 0; const failure = new Error('transport outcome unknown');
  const port = adapter(() => { calls++; throw failure; });
  for (const op of ['command.document.object.property.edit', 'query.document.object.property.metadata',
    'command.document.object.vector.edit']) {
    const q = request(); q.operation = op; assert.throws(() => port.editPropertyTrack(q));
  }
  assert.equal(calls, 0);
  assert.throws(() => port.editPropertyTrack(request()), e => e === failure);
  assert.equal(calls, 1);
});

test('browser classic-script caller loads the cohesive property contract and uses the same explicit API', async () => {
  const context = vm.createContext({TextEncoder});
  for (const filename of ['native-font-contract.js', 'native-property-contract.js', 'native-application.js']) {
    vm.runInContext(fs.readFileSync(path.join(ROOT, 'src/js/adapters', filename), 'utf8'), context, {filename});
  }
  context.requestJson = JSON.stringify(request());
  const result = await vm.runInContext(`(async () => {
    const q = JSON.parse(requestJson);
    const api = NemoNativeApplicationAdapter.createNativeApplicationAdapter('browser-track', {
      dispatch(r) { return {apiVersion:2, requestId:r.requestId, instanceId:r.instanceId,
        documentId:r.documentId, contentRevision:8, ok:true, result:{applied:true, historyEntriesAdded:1}}; }
    });
    return api.editPropertyTrack(q);
  })()`, context);
  assert.equal(result.result.applied, true);
  assert.equal(result.contentRevision, 8);
});

function propertyScripts(html) {
  return [...html.matchAll(/<script\b([^>]*)>[\s\S]*?<\/script>/gi)]
    .map(m => ({tag:m[0], src:m[1].match(/\bsrc=["']([^"']+)/)?.[1]}));
}
function assertPropertyRegistration(html, profile) {
  const id = 'app.native.application.adapter';
  const members = profile.modules.filter(m => m.id === id);
  assert.deepEqual(members, [{id, layer:'adapters', dir:'src/js/adapters',
    files:['native-application.js', 'native-property-contract.js'],
    sizeProfile:'Platform/engine adapter', publicApi:['native-application.js']}]);
  const source = 'js/adapters/native-property-contract.js', consumer = 'js/adapters/native-application.js';
  const scripts = propertyScripts(html);
  const selected = [source, consumer].map(src => {
    const entries = scripts.filter(s => s.src === src);
    assert.equal(entries.length, 1, 'one synchronous ordinary registration: ' + src);
    assert.equal(entries[0].tag, '<script src="' + src + '"></script>');
    return scripts.indexOf(entries[0]);
  });
  assert.equal(selected[0] + 1, selected[1], 'private helper immediately precedes its SDK consumer');
  assert.equal(profile.modules.flatMap(m => m.files.map(file => m.dir + '/' + file))
    .filter(file => file === 'src/' + source).length, 1);
  const {checkSourceCoverage} = require('../scripts/nemo/lib/boundaries-coverage.cjs');
  const options = {root:ROOT, sourcePaths:[source, consumer].map(s => 'src/' + s), exclusions:[]};
  assert.equal(checkSourceCoverage({modules:members}, options).ok, true);
}

test('D12 private helper has one ordinary membership and exact synchronous helper-before-SDK registration', () => {
  const html = fs.readFileSync(path.join(ROOT, 'src/index.html'), 'utf8');
  const profile = JSON.parse(fs.readFileSync(path.join(ROOT, 'engineering/boundaries/profiles/app-js.profile.json'), 'utf8'));
  assertPropertyRegistration(html, profile);
  const a = '<script src="js/adapters/native-property-contract.js"></script>';
  const b = '<script src="js/adapters/native-application.js"></script>';
  for (const broken of [html.replace(a, ''), html.replace(a, a + '\n' + a),
    html.replace(a, a.replace('<script ', '<script async ')),
    html.replace(a, a.replace('<script ', '<script defer ')),
    html.replace(a, a.replace('<script ', '<script type="module" ')),
    html.replace(a + '\n' + b, b + '\n' + a)]) {
    assert.notEqual(broken, html);
    assert.throws(() => assertPropertyRegistration(broken, profile), {code:'ERR_ASSERTION'});
  }
  const {checkSourceCoverage} = require('../scripts/nemo/lib/boundaries-coverage.cjs');
  const member = profile.modules.find(m => m.id === 'app.native.application.adapter');
  const helper = 'src/js/adapters/native-property-contract.js';
  const options = {root:ROOT, sourcePaths:member.files.map(file => member.dir + '/' + file), exclusions:[]};
  const missing = {...member, files:member.files.filter(file => file !== 'native-property-contract.js')};
  assert.deepEqual(checkSourceCoverage({modules:[missing]}, options).violations.map(({rule,file}) => ({rule,file})),
    [{rule:'coverage-unprofiled-source', file:helper}]);
  const duplicate = {...member, id:member.id + '.duplicate', files:['native-property-contract.js']};
  assert.deepEqual(checkSourceCoverage({modules:[member,duplicate]}, options).violations.map(({rule,file}) => ({rule,file})),
    [{rule:'coverage-duplicate-declaration', file:helper}]);
});

function loadDeclaredPropertyScripts(html) {
  const context = vm.createContext({TextEncoder});
  const names = ['js/adapters/native-font-contract.js', 'js/adapters/native-property-contract.js', 'js/adapters/native-application.js'];
  for (const {src} of propertyScripts(html).filter(entry => names.includes(entry.src))) {
    vm.runInContext(fs.readFileSync(path.join(ROOT, 'src', src), 'utf8'), context, {filename:src});
  }
  context.requestJson = JSON.stringify(request());
  context.calls = 0;
  return context;
}
const browserPropertyCall = `(() => {
  const api = NemoNativeApplicationAdapter.createNativeApplicationAdapter('declared-property-loader', {
    dispatch(q) { calls++; return {apiVersion:2, requestId:q.requestId, instanceId:q.instanceId,
      documentId:q.documentId, contentRevision:8, ok:true, result:{applied:true, historyEntriesAdded:1}}; }
  });
  return api.editPropertyTrack(JSON.parse(requestJson));
})()`;

test('D12 actual declared browser sequence reaches native transport exactly once', async () => {
  const html = fs.readFileSync(path.join(ROOT, 'src/index.html'), 'utf8');
  const context = loadDeclaredPropertyScripts(html);
  const result = await vm.runInContext(browserPropertyCall, context);
  assert.equal(context.calls, 1);
  assert.equal(result.result.applied, true);
  assert.equal(result.contentRevision, 8);
});

test('D12 real SDK cannot dispatch with a missing helper or a helper loaded only after SDK capture', () => {
  const html = fs.readFileSync(path.join(ROOT, 'src/index.html'), 'utf8');
  const a = '<script src="js/adapters/native-property-contract.js"></script>';
  const b = '<script src="js/adapters/native-application.js"></script>';
  for (const broken of [html.replace(a, ''), html.replace(a + '\n' + b, b + '\n' + a)]) {
    assert.notEqual(broken, html);
    const context = loadDeclaredPropertyScripts(broken);
    assert.equal(typeof context.NemoNativeApplicationAdapter.createNativeApplicationAdapter, 'function');
    assert.throws(() => vm.runInContext(browserPropertyCall, context), {name:'TypeError'});
    assert.equal(context.calls, 0, 'malordered helper must fail before authority dispatch');
  }
});
