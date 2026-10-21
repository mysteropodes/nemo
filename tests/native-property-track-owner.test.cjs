'use strict';
// Actual Project, desktop ports, SDK and subscribed transport; simulated native host.
// These tests do not execute Rust, render native pixels or establish installed acceptance.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const Project = require('../src/js/adapters/native-object-project.js');
const Files = require('../src/js/adapters/native-object-project-file.js');
const Transport = require('../src/js/adapters/application-mcp.js');
const SDK = require('../src/js/adapters/native-application.js');
const turn = () => new Promise(setImmediate);
function gate() { let resolve; const promise = new Promise(r => { resolve = r; }); return {promise, resolve}; }
function pin(info) { return Object.fromEntries(['instanceId', 'documentId', 'lifecycleGeneration', 'contentRevision', 'documentSnapshotId'].map(k => [k, info[k]])); }
function payload(preset = 'ease') {
  return {command:'object.property.track.edit', selections:[{target:{contextId:'scene-root', layerUid:'data:layer 雪'}, property:'opacity', frame:-1.5}], action:{kind:'preset', preset}};
}
function selector(f, view = {kind:'authoredKeys', target:payload().selections[0].target, property:'opacity', offset:0, limit:8}) {
  const p = f.session.info();
  return {instanceId:p.instanceId, documentId:p.documentId, atRevision:p.contentRevision, documentSnapshotId:p.documentSnapshotId, view};
}
function fixture() {
  let installed = false, generation = 0, revision = 0, shape = 'linear';
  let replyGate = null, previewGate = null, ackGate = null, readGate = null, pending = null, registered = null, badReply = null, refusal = null;
  const listeners = new Map(), requests = [], log = [], capturedPorts = [];
  const info = () => ({apiVersion:2, instanceId:'instance', documentId:'document-'+generation, lifecycleGeneration:generation,
    contentRevision:revision, documentSnapshotId:'snapshot-'+generation+'-'+revision, documentDigest:String(revision % 10).repeat(64),
    byteCount:1000, canvasW:320, canvasH:180, fps:24, totalFrames:21, undoDepth:revision, redoDepth:0, reference:null,
    layers:[{layerUid:'data:layer 雪', name:'Layer', visible:true}]});
  function response(q, result) { return {apiVersion:2, requestId:q.requestId, instanceId:q.instanceId, documentId:q.documentId, contentRevision:revision, ok:true, result}; }
  async function invoke(command, args) {
    const q = args && (args.request || JSON.parse(args.requestJson));
    if (command === 'nemo_native_status' || command === 'nemo_native_object_client_status') return {...info(), available:installed};
    if (command === 'nemo_native_object_bootstrap') {
      installed = true; generation++; revision = 0; shape = 'linear';
      return {...info(), requestId:q.requestId, resourceCount:0, viewportAvailable:false};
    }
    if (command === 'nemo_native_object_reference') { assert.equal(q.action.kind, 'inspect'); log.push('inspect-'+revision); return {...info(), requestId:q.requestId}; }
    if (command === 'nemo_native_object_preview') {
      if (pending) {
        assert.deepEqual(q.revisionSync, {subscriptionId:registered.subscriptionId, event:pending}, 'pending external revision requires tagged preview');
      }
      if (previewGate) { const wait = previewGate; previewGate = null; await wait.promise; }
      log.push('present-'+q.contentRevision);
      return {status:'presented', workId:'work-'+revision, viewGeneration:1};
    }
    if (command === 'nemo_native_revision_sync') {
      log.push(q.action);
      if (q.action === 'binding') return {...info(), subscriptionId:'subscription-'+generation};
      if (q.action === 'subscribe') { registered = q.binding; return q.binding; }
      if (q.action === 'acknowledge') {
        assert.deepEqual(q.event, pending); assert.equal(root.state.layers[0].layerUid, 'data:layer 雪');
        if (ackGate) { const wait = ackGate; ackGate = null; await wait.promise; }
        pending = null; return null;
      }
      if (q.action === 'disconnect') return null;
    }
    assert.equal(command, 'nemo_native_dispatch'); requests.push(structuredClone(q));
    if (q.operation.startsWith('query.')) {
      const p = q.payload, v = p.view;
      if (readGate) { const wait = readGate; readGate = null; await wait.promise; }
      const common = {instanceId:q.instanceId, documentId:q.documentId, atRevision:p.atRevision, documentSnapshotId:p.documentSnapshotId, kind:v.kind};
      let result;
      if (q.operation.endsWith('.metadata')) {
        result = {...common, target:null, items:[{kind:'layer', target:{contextId:'scene-root', layerUid:'data:layer 雪'}}], nextOffset:null,
          unavailableCategories:['conditionalMotionRows','pathVerticesTrimStrokeBrushMesh','referenceOnlyElements','expressionDiagnostics','layerEffectMatteReveal','propertyWrites'].map(category => ({category, state:'unavailable', reason:'not-admitted'}))};
      } else {
        result = {...common, target:structuredClone(v.target), property:v.property, dimension:1};
        if (v.kind === 'rawSample') Object.assign(result, {frame:v.frame, value:[25]});
        else Object.assign(result, {trackPresent:true, items:[{frame:-1.5, v:[25], curvePoints:null}, {frame:10, v:[80], hold:false}], nextOffset:null});
      }
      const r = response(q, result); if (badReply) badReply(r); return r;
    }
    assert.equal(q.operation, 'command.document.object.property.track.edit'); assert.equal(q.expectedRevision, revision);
    // Like the actual Tauri UI route (external=false), a local command emits no
    // self-event/ACK wait. Only the separate external() helper creates that fence.
    if (refusal) return {apiVersion:2, requestId:q.requestId, instanceId:q.instanceId, documentId:q.documentId, contentRevision:revision, ok:false, error:{code:refusal, message:'Native track refused'}};
    const applied = q.payload.action.preset !== shape; shape = q.payload.action.preset;
    if (applied) revision++;
    const r = response(q, {applied, historyEntriesAdded:applied ? 1 : 0});
    if (replyGate) { const wait = replyGate; replyGate = null; await wait.promise; }
    if (badReply) badReply(r); return r;
  }
  const root = {state:{currentFrame:0}, selectedPaths:[], userLayers:[], NemoNativeObjectProjectFile:Files,
    NemoNativeApplicationAdapter:SDK, NemoApplicationMcpTransport:Transport,
    NemoNativeObjectUI:{render(_root, _info, _frame, ports) { capturedPorts.push(ports); return true; }},
    __TAURI__:{core:{invoke}, event:{listen:async (name, callback) => { listeners.set(name, callback); return () => { if (listeners.get(name) === callback) listeners.delete(name); }; }}},
    document:{getElementById:() => ({getBoundingClientRect:() => ({left:0, top:0, width:320, height:180})})},
    updateUI:() => log.push('publish-'+revision)};
  require('./helpers/native-viewport-underlay.cjs').install(root);
  const session = Project.desktop(root);
  return {root, session, requests, log, capturedPorts,
    open:() => session.openPath('/native.json', {commit:meta => Files.projectView(root, meta)}),
    holdReply:wait => { replyGate = wait; }, holdPreview:wait => { previewGate = wait; }, holdAck:wait => { ackGate = wait; }, holdRead:wait => { readGate = wait; },
    badReply:fn => { badReply = fn; }, refuse:code => { refusal = code; },
    async external() {
      const from = revision; revision++;
      pending = {instanceId:'instance', documentId:info().documentId, lifecycleGeneration:generation, requestId:'external-'+revision, fromRevision:from, toRevision:revision};
      return listeners.get('nemo-native-revision')({payload:pending});
    }};
}
async function spin(predicate) { for (let i = 0; i < 30 && !predicate(); i++) await turn(); assert.ok(predicate(), 'expected protocol boundary reached'); }

test('property facet is fail-closed without its ordinary classic-script child', () => {
  const source = fs.readFileSync(path.join(__dirname, '../src/js/adapters/native-object-project.js'), 'utf8');
  const parser = require('../src/js/adapters/native-object-project-metadata.js');
  const desktop = require('../src/js/adapters/native-object-project-desktop.js');
  const box = {NemoNativeObjectProjectMetadata:parser, NemoNativeObjectProjectDesktop:desktop};
  vm.runInNewContext(source, box);
  assert.throws(() => box.NemoNativeObjectProject.create({}), /property ports are unavailable/);
});

test('view facet is fail-closed before session initialization without its ordinary classic-script child', () => {
  const source = fs.readFileSync(path.join(__dirname, '../src/js/adapters/native-object-project.js'), 'utf8');
  const box = {NemoNativeObjectProjectMetadata:require('../src/js/adapters/native-object-project-metadata.js'),
    NemoNativeObjectProjectDesktop:require('../src/js/adapters/native-object-project-desktop.js'),
    NemoNativeObjectProjectProperties:require('../src/js/adapters/native-object-project-properties.js')};
  vm.runInNewContext(source, box);
  let reads = 0; const ports = Object.defineProperty({}, 'viewMode', {get(){reads++; throw Error('unexpected session initialization');}});
  assert.throws(() => box.NemoNativeObjectProject.create(ports), /view ports are unavailable/);
  assert.equal(reads, 0);
});

test('actual SDK and subscribed desktop return immutable correlated D11 samples and exact authored DTOs', async () => {
  const f = fixture(); await f.open(); const before = f.session.info();
  const q = selector(f), result = await f.session.readProperty(q);
  assert.deepEqual(result.items, [{frame:-1.5, v:[25], curvePoints:null}, {frame:10, v:[80], hold:false}]);
  assert.equal(Object.isFrozen(result.items[0].v), true); assert.equal(result.atRevision, before.contentRevision);
  const raw = await f.session.readProperty(selector(f, {kind:'rawSample', target:q.view.target, property:'opacity', frame:-0.25}));
  assert.deepEqual(raw.value, [25]); assert.equal(raw.frame, -0.25); assert.strictEqual(f.session.info(), before);
  const metadata = await f.session.readPropertyMetadata(selector(f, {kind:'targets', offset:0, limit:8}));
  assert.equal(Object.isFrozen(metadata.items[0].target), true); assert.equal(metadata.kind, 'targets');
  assert.equal(f.requests.every(r => !Object.hasOwn(r, 'expectedRevision')), true);
  assert.equal(f.requests[0].operation, 'query.document.object.property.read');
  assert.match(f.requests[0].requestId, /^project-property-read-\d+$/);
  assert.deepEqual(f.requests[0].payload, {atRevision:q.atRevision, documentSnapshotId:q.documentSnapshotId, view:q.view});
});

test('cached UI property ports use one Project owner and a live readiness getter', async () => {
  const f = fixture(); assert.equal(f.session.editPropertyTrack, undefined); await f.open();
  Project.paintUI(f.root, false); Project.paintUI(f.root, true);
  const ports = f.capturedPorts[0]; assert.strictEqual(ports, f.capturedPorts[1]); assert.strictEqual(ports.currentSession(), f.session);
  assert.equal(typeof ports.editPropertyTrack, 'function'); assert.equal((await ports.readProperty(selector(f))).kind, 'authoredKeys');
  const wait = gate(); f.holdAck(wait); const external = f.external(); await spin(() => f.log.includes('acknowledge'));
  assert.equal(ports.editPropertyTrack, undefined); await assert.rejects(ports.readProperty(selector(f)));
  wait.resolve(); await external; assert.equal(typeof ports.editPropertyTrack, 'function');
});

test('local subscribed mutation completes inspection and presentation without any self ACK or second queue', {timeout:2000}, async () => {
  const f = fixture(); await f.open(); const before = pin(f.session.info()), start = f.log.length;
  const result = await f.session.editPropertyTrack(payload(), before, () => true);
  assert.deepEqual(result.result, {applied:true, historyEntriesAdded:1}); assert.equal(result.contentRevision, 1);
  assert.equal(f.session.info().contentRevision, 1); assert.equal(f.session.readPresentedObjectSession().contentRevision, 1);
  assert.ok(f.log.slice(start).includes('inspect-1')); assert.ok(f.log.slice(start).includes('publish-1')); assert.ok(f.log.slice(start).includes('present-1'));
  assert.equal(f.log.slice(start).includes('acknowledge'), false); assert.equal(f.requests.length, 1);
  const noop = await f.session.editPropertyTrack(payload(), pin(f.session.info()), () => true);
  assert.deepEqual(noop.result, {applied:false, historyEntriesAdded:0}); assert.equal(f.session.info().contentRevision, 1);
});

test('submitted payload is copied before enqueue and intent plus exact pin are checked at dispatch', async () => {
  const f = fixture(); await f.open(); const wait = gate(); f.holdPreview(wait);
  const preview = f.session.presentPreview(0), copied = payload(), before = pin(f.session.info());
  const edit = f.session.editPropertyTrack; // Already queued work makes the getter absent.
  assert.equal(edit, undefined); wait.resolve(); await preview;
  const run = f.session.editPropertyTrack, submitted = payload();
  const work = run(submitted, before, () => true); submitted.action.preset = 'hold'; submitted.selections[0].frame = 99;
  await work; assert.equal(f.requests[0].payload.action.preset, 'ease'); assert.equal(f.requests[0].payload.selections[0].frame, -1.5);
  const count = f.requests.length; await assert.rejects(run(copied, before, () => true), /stale/);
  let calls = 0; const currentPin = pin(f.session.info());
  await assert.rejects(run(payload('hold'), currentPin, () => ++calls === 1), /stale/);
  assert.equal(calls, 2); assert.equal(f.requests.length, count);
});

test('malformed selectors and payload types fail before transport with no normalizing omission', async () => {
  const f = fixture(); await f.open(); const p = pin(f.session.info()), q = selector(f), before = f.requests.length;
  for (const bad of [null, [], {...q, extra:true}, {...q, atRevision:99}, {...q, view:{...q.view, extra:true}}]) await assert.rejects(f.session.readProperty(bad));
  for (const change of [p => { p.extra = undefined; }, p => { p.selections[0].frame = NaN; }, p => { p.action = ['preset', 'ease']; }, p => { p.selections = Array(1); }]) {
    const value = payload(); change(value); await assert.rejects(f.session.editPropertyTrack(value, p, () => true));
  }
  await assert.rejects(f.session.editPropertyTrack(payload(), p));
  assert.equal(f.requests.length, before); assert.equal(f.session.info().contentRevision, 0);
});

test('native refusal and malformed success never report applied or retry a command', async () => {
  const f = fixture(); await f.open(); f.refuse('not_found');
  await assert.rejects(f.session.editPropertyTrack(payload(), pin(f.session.info()), () => true), /Native command refused: not_found/);
  assert.equal(f.requests.length, 1); assert.equal(f.session.info().contentRevision, 0);
  f.refuse(null); f.badReply(r => { r.result.historyEntriesAdded = 2; });
  await assert.rejects(f.session.editPropertyTrack(payload(), pin(f.session.info()), () => true), /invalid property track disposition/);
  assert.equal(f.requests.length, 2); assert.equal(f.session.info().contentRevision, 1);
  assert.equal(f.session.isIndeterminate(), false); // Existing mutation reconciliation still inspects/presents the committed owner.
});

test('late read is discarded across presentation replacement and owner reopen', async () => {
  for (const reopen of [false, true]) {
    const f = fixture(); await f.open(); const wait = gate(); f.holdRead(wait);
    const read = f.session.readProperty(selector(f)), rejected = assert.rejects(read);
    await spin(() => f.requests.length === 1);
    if (reopen) await f.open(); else await f.session.presentPreview(1);
    wait.resolve(); await rejected; assert.equal(f.session.isActive(), true); assert.equal(f.requests.length, 1);
  }
});

test('external revision drains an actual in-flight UI reply then synchronizes and ACKs without queue deadlock', {timeout:2000}, async () => {
  const f = fixture(); await f.open(); const wait = gate(); f.holdReply(wait);
  const mutation = f.session.editPropertyTrack(payload(), pin(f.session.info()), () => true);
  const refused = assert.rejects(mutation, /pending external revision requires tagged preview/);
  await spin(() => f.requests.length === 1);
  const external = f.external(); await turn(); assert.equal(f.log.includes('acknowledge'), false);
  wait.resolve(); await refused; await external;
  assert.equal(f.session.info().contentRevision, 2); assert.equal(f.session.readPresentedObjectSession().contentRevision, 2);
  assert.equal(f.log.filter(x => x === 'acknowledge').length, 1); assert.equal(f.session.isIndeterminate(), false);
  assert.equal(typeof f.session.editPropertyTrack, 'function'); assert.equal(f.requests.length, 1);
});

test('captured edit function cannot bypass pending external ACK and recovers only after it completes', {timeout:2000}, async () => {
  const f = fixture(); await f.open(); const edit = f.session.editPropertyTrack, wait = gate(); f.holdAck(wait);
  const external = f.external(); await spin(() => f.log.includes('acknowledge'));
  await assert.rejects(edit(payload(), pin(f.session.info()), () => true), /unavailable/); assert.equal(f.requests.length, 0);
  wait.resolve(); await external;
  await f.session.editPropertyTrack(payload(), pin(f.session.info()), () => true); assert.equal(f.requests.length, 1);
});

test('D11 result correlation and bounds are enforced by actual SDK before a frozen inner result can escape', async () => {
  const mutations = [r => { r.result.target.layerUid = 'foreign'; }, r => { r.result.property = 'rotation'; },
    r => { r.result.items[0].v = [1, 2]; }, r => { r.result.items[0].curvePoints = Array.from({length:180}, (_, i) => ({x:i / 179, y:0, tx:0.123456789012345})); },
    r => { r.requestId = 'foreign'; }, r => { r.result.documentSnapshotId = 'foreign'; }];
  for (const mutate of mutations) {
    const f = fixture(); await f.open(); const before = f.session.info(); f.badReply(mutate);
    await assert.rejects(f.session.readProperty(selector(f))); assert.equal(f.requests.length, 1);
    assert.strictEqual(f.session.info(), before); assert.equal(f.session.isIndeterminate(), false);
  }
});

test('absent public SDK cannot create a private or legacy fallback for a captured property intent', async () => {
  const f = fixture(); await f.open(); const edit = f.session.editPropertyTrack, before = pin(f.session.info());
  delete f.root.NemoNativeApplicationAdapter;
  await assert.rejects(edit(payload(), before, () => true), /public application client is unavailable/);
  assert.equal(f.requests.length, 0); assert.equal(f.session.info().contentRevision, 0);
  assert.equal(f.session.isIndeterminate(), false); assert.equal(f.session.readPresentedObjectSession().contentRevision, 0);
});


test('idle and pending owner reads/getters short-circuit before readiness callbacks', async () => {
  const calls = [], session = Project.create({
    projectionReady(value) { calls.push(['projection', value]); throw Error('INCIDENTAL NULL PROJECTION'); },
    subscriptionActive() { calls.push(['subscription']); throw Error('INCIDENTAL SUBSCRIPTION'); }
  });
  assert.doesNotThrow(() => assert.equal(session.editPropertyTrack, undefined));
  await assert.rejects(session.readPropertyMetadata({instanceId:'instance', documentId:'document', atRevision:0,
    documentSnapshotId:'snapshot', view:{kind:'targets', offset:0, limit:8}}), /owner or presentation is unavailable/);
  assert.deepEqual(calls, []);
  const f = fixture(); await f.open(); const wait = gate(); f.holdReply(wait);
  const mutation = f.session.editPropertyTrack(payload(), pin(f.session.info()), () => true);
  await spin(() => f.requests.length === 1);
  let callbacks = 0;
  const previous = f.root.NemoNativeObjectProjectFile;
  f.root.NemoNativeObjectProjectFile = {...previous, projectionMatches() { callbacks++; throw Error('INCIDENTAL PENDING PROJECTION'); }};
  assert.equal(f.session.editPropertyTrack, undefined);
  await assert.rejects(f.session.readProperty(selector(f)), /owner or presentation is unavailable/);
  await assert.rejects(f.session.readPropertyMetadata(selector(f, {kind:'targets', offset:0, limit:8})), /owner or presentation is unavailable/);
  assert.equal(callbacks, 0);
  f.root.NemoNativeObjectProjectFile = previous; wait.resolve(); await mutation;
});

test('superseded read rejects before querying replacement presentation readiness', async () => {
  const f = fixture(); await f.open(); const wait = gate(); f.holdRead(wait);
  const read = f.session.readProperty(selector(f)); await spin(() => f.requests.length === 1);
  await f.session.presentPreview(1);
  let callbacks = 0; const previous = f.root.NemoNativeObjectProjectFile;
  f.root.NemoNativeObjectProjectFile = {...previous, projectionMatches() { callbacks++; throw Error('INCIDENTAL REPLACEMENT PROJECTION'); }};
  const rejected = assert.rejects(read, /owner or presentation changed/); wait.resolve(); await rejected;
  assert.equal(callbacks, 0); f.root.NemoNativeObjectProjectFile = previous;
});
