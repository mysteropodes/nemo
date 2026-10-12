'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const entry = require('../src/js/adapters/native-opacity-project-entry.js');
const document = require('../src/js/project-document.js');
const TOKEN = 'c8179e77-6df2-40bb-8a18-2a369cff860c';
function fixture(options = {}) {
  let sequence = 0, bytesA = 'native-A-exact-bytes', localGeneration = 3, hostGeneration = 17;
  const identity = { instanceId: 'instance-A', documentId: 'native-document-1', contentRevision: 4 };
  const display = { activeTabId: 'tab-A', name: 'A', path: '/A.json', frame: 10 };
  const source = JSON.parse(fs.readFileSync('tests/animation/fixtures/curve-workflow.json', 'utf8'));
  source.layers[0].motionStatic = { opacity: [60] };
  const files = { '/B.json': JSON.stringify(source), '/bad.json': '{invalid' };
  const calls = [], rows = [], notices = [], effects = [];
  const native = { blocksLegacy: () => true, isActive: () => true, identity: () => ({ ...identity }),
    getNativeIdentity: () => ({ documentId: identity.documentId, generation: localGeneration }),
    persistenceJSON: () => bytesA };
  const root = { location: { protocol: 'tauri:', hostname: 'localhost', port: '' },
    NemoNativeOpacityCutover: native, SMProjectDocument: document,
    crypto: { randomUUID: () => 'request-' + ++sequence },
    SM: { exportJSON() { effects.push('Paper export'); throw Error('retired writer'); },
      importJSON() { effects.push('Paper import'); throw Error('retired writer'); } },
    __TAURI__: { fs: { async readTextFile(path) { calls.push(['read', path]);
      if (options.read) return options.read(path, files);
      if (!Object.hasOwn(files, path)) throw Error('missing file'); return files[path]; } },
      dialog: { async open() { calls.push(['picker']); return options.picker ? options.picker() : '/B.json'; } },
      core: { async invoke(name, args) {
        assert.equal(this, root.__TAURI__.core); calls.push([name, structuredClone(args)]);
        if (options.invoke) { const override = await options.invoke(name, args); if (override !== undefined) return override; }
        if (name === 'nemo_native_revision_sync') return { ...identity, lifecycleGeneration: hostGeneration, subscriptionId: 'subscription-observation' };
        const r = args.request;
        if (name === 'nemo_native_prepare_inactive_project') return {
          apiVersion: 2, requestId: r.requestId, preparationToken: TOKEN, state: 'inactive-prepared',
          format: 'nemo.native-opacity-document', formatVersion: 1, layerCount: 1, totalFrames: 21, geometryCount: 21 };
        if (name === 'nemo_native_cancel_inactive_project') return {
          apiVersion: 2, requestId: r.requestId, preparationToken: r.preparationToken, state: 'inactive-cancelled' };
        throw Error('unapproved native call ' + name);
      } } } };
  const ports = { readUi: () => ({ ...display }), notice: message => notices.push(message),
    commit(meta) { if (options.commit) return options.commit(meta, rows); rows.push(meta); return true; },
    remove(meta) { if (options.remove) return options.remove(meta, rows); rows.splice(rows.indexOf(meta), 1); return true; } };
  const controller = entry.createInactiveEntry(root, ports);
  function snapshot() { return JSON.stringify({ identity, localGeneration, hostGeneration, bytesA, display, effects }); }
  return { root, ports, controller, calls, rows, notices, effects, files, display, identity, snapshot,
    edit() { identity.contentRevision++; bytesA += '-edited'; }, local() { localGeneration++; }, host() { hostGeneration++; } };
}
function named(f, name) { return f.calls.filter(call => call[0] === name); }
function deferred() { let resolve; const promise = new Promise(r => { resolve = r; }); return { promise, resolve }; }

test('real supported B converter sends exact native projection/all21 geometry and commits only opaque metadata', async () => {
  const f = fixture(), before = f.snapshot();
  assert.equal((await f.controller.openPicker()).status, 'prepared');
  assert.equal(f.snapshot(), before); assert.equal(f.rows.length, 1);
  const meta = f.rows[0]; assert.equal(Object.isFrozen(meta), true);
  assert.deepEqual(Object.keys(meta).sort(), ['geometryCount', 'kind', 'layerCount', 'name', 'path', 'preparationToken', 'totalFrames'].sort());
  assert.equal(meta.kind, 'native-inactive'); assert.equal(meta.name, 'B'); assert.equal(meta.preparationToken, TOKEN);
  assert.equal(meta.geometryCount, 21);
  const request = named(f, 'nemo_native_prepare_inactive_project')[0][1].request;
  assert.equal(request.lifecycleGeneration, 17); assert.notEqual(request.lifecycleGeneration, 3);
  assert.equal(request.documentId, 'native-document-1'); assert.equal(request.expectedRevision, 4);
  assert.deepEqual(request.projection, { format: 'nemo.native-opacity-document', formatVersion: 1, totalFrames: 21,
    layers: [{ layerUid: 'r08_curve_layer', motionStatic: { opacity: [60] } }] });
  for (const [index, resource] of request.resources.entries()) {
    assert.deepEqual(resource, { resourceId: `geometry/r08/frame-${index}`, resourceVersion: 'v1',
      layers: [{ layerUid: 'r08_curve_layer', bounds: [20,60,40,80], transform: [1,0,0,1,index * 6.4,0], paint: { red:255, green:0, blue:0 } }] });
  }
  assert.equal(named(f, 'nemo_native_revision_sync').length, 2);
  assert.ok(named(f, 'nemo_native_revision_sync').every(c => c[1].request.action === 'binding'));
  assert.equal((await f.controller.openPath('/B.json')).status, 'unavailable');
  assert.equal(named(f, 'nemo_native_prepare_inactive_project').length, 1);
  assert.equal((await f.controller.close({ ...meta })).status, 'unavailable');
  assert.equal((await f.controller.close(meta)).status, 'cancelled'); assert.equal(f.rows.length, 0);
  const cancel = named(f, 'nemo_native_cancel_inactive_project')[0][1].request;
  assert.equal(cancel.lifecycleGeneration, 17); assert.equal(cancel.preparationToken, TOKEN);
  assert.equal((await f.controller.close(meta)).status, 'unavailable'); assert.equal(f.snapshot(), before);
});
for (const value of [null, [], ['/B.json','/A.json'], '/A.json', 10]) test('cancel/unsupported picker leaves A and rows unchanged: ' + JSON.stringify(value), async () => {
  const f = fixture({ picker: () => value }), before = f.snapshot();
  assert.ok(['cancelled','refused'].includes((await f.controller.openPicker()).status));
  assert.equal(f.snapshot(), before); assert.equal(f.rows.length, 0); assert.equal(named(f, 'nemo_native_prepare_inactive_project').length, 0);
});
for (const path of ['/bad.json','/missing.json']) test('bad/read-error source refuses before native prepare: ' + path, async () => {
  const f = fixture(), before = f.snapshot(); assert.equal((await f.controller.openPath(path)).status, 'refused');
  assert.equal(f.snapshot(), before); assert.equal(f.rows.length, 0); assert.equal(named(f, 'nemo_native_prepare_inactive_project').length, 0);
});
test('picker exception is bounded and preserves admitted A', async () => {
  const f = fixture({ picker: () => { throw Error('picker failed'); } }), before = f.snapshot();
  assert.equal((await f.controller.openPicker()).status, 'refused'); assert.equal(f.snapshot(), before);
});
for (const change of ['revision','local','name','path','frame','release']) test('held file read rejects changed A/UI: ' + change, async () => {
  const gate = deferred(), f = fixture({ read: async () => { await gate.promise; return f.files['/B.json']; } });
  const pending = f.controller.openPath('/B.json');
  if (change === 'revision') f.edit(); else if (change === 'local') f.local();
  else if (change === 'release') f.root.NemoNativeOpacityCutover = null;
  else f.display[change] = change === 'frame' ? 20 : 'changed';
  const afterChange = f.snapshot(); gate.resolve(); assert.equal((await pending).status, 'refused');
  assert.equal(f.snapshot(), afterChange); assert.equal(f.rows.length, 0); assert.equal(named(f, 'nemo_native_prepare_inactive_project').length, 0);
});
test('cancelPending and overlapping calls do not publish or prepare a replacement', async () => {
  const gate = deferred(), f = fixture({ read: async () => { await gate.promise; return f.files['/B.json']; } });
  const pending = f.controller.openPath('/B.json');
  assert.equal((await f.controller.openPicker()).status, 'unavailable'); f.controller.cancelPending(); gate.resolve();
  assert.equal((await pending).status, 'cancelled'); assert.equal(f.rows.length, 0);
  assert.equal(named(f, 'nemo_native_prepare_inactive_project').length, 0);
});
for (const field of ['state','format','formatVersion','layerCount','totalFrames','geometryCount','extra']) test('correlated malformed receipt is cancelled, never appended: ' + field, async () => {
  const f = fixture({ invoke(name, args) { if (name !== 'nemo_native_prepare_inactive_project') return;
    return { apiVersion:2, requestId:args.request.requestId, preparationToken:TOKEN, state:'inactive-prepared',
      format:'nemo.native-opacity-document',formatVersion:1,layerCount:1,totalFrames:21,geometryCount:21,[field]:'wrong' }; } });
  const before=f.snapshot(); assert.equal((await f.controller.openPath('/B.json')).status,'refused');
  assert.equal(f.snapshot(),before); assert.equal(f.rows.length,0); assert.equal(named(f,'nemo_native_cancel_inactive_project').length,1);
});
for (const change of ['revision','host','local','frame']) test('post-prepare drift never appends and cleanup is original-owner scoped: '+change, async () => {
  let f;
  f = fixture({ invoke(name) { if (name === 'nemo_native_prepare_inactive_project') {
    if(change==='revision')f.edit();else if(change==='host')f.host();else if(change==='local')f.local();else f.display.frame=20;
  } } });
  const outcome = await f.controller.openPath('/B.json'); assert.equal(f.rows.length,0);
  const cancels=named(f,'nemo_native_cancel_inactive_project');
  if(change==='local'||change==='host'){assert.equal(outcome.status,'indeterminate');assert.equal(cancels.length,0);}
  else {assert.equal(outcome.status,'refused');assert.equal(cancels.length,1);assert.equal(cancels[0][1].request.expectedRevision,f.identity.contentRevision);}
});
test('known native refusal preserves A and permits a fresh attempt without retrying the old request', async () => {
  const f = fixture({ invoke(name) { if(name==='nemo_native_prepare_inactive_project')throw {code:'preparation_busy'}; } }), before=f.snapshot();
  assert.equal((await f.controller.openPath('/B.json')).status,'refused'); assert.equal(f.snapshot(),before); assert.equal(f.controller.inspect().indeterminate,false);
});
for (const response of ['lost','wrong-id','missing-token']) test('lost/uncorrelated prepare stays indeterminate with no fabricated cleanup: '+response, async () => {
  const f=fixture({invoke(name,args){if(name!=='nemo_native_prepare_inactive_project')return;
    if(response==='lost')throw Error('IPC lost');return {apiVersion:2,requestId:response==='wrong-id'?'foreign':args.request.requestId,preparationToken:response==='missing-token'?null:TOKEN};}});
  assert.equal((await f.controller.openPath('/B.json')).status,'indeterminate');assert.equal(f.rows.length,0);
  assert.equal(named(f,'nemo_native_cancel_inactive_project').length,0);
  assert.equal((await f.controller.openPath('/B.json')).status,'unavailable');assert.equal(named(f,'nemo_native_prepare_inactive_project').length,1);
});
for (const behavior of ['false','throw-after-effect','thenable','undefined']) test('atomic commit sink disposition is bounded: '+behavior, async () => {
  const f=fixture({commit(meta,rows){if(behavior==='false')return false;if(behavior==='throw-after-effect'){rows.push(meta);throw Error('sink uncertain');}
    if(behavior==='thenable')return Promise.resolve(true);return undefined;}}), before=f.snapshot();
  const result=await f.controller.openPath('/B.json');assert.equal(f.snapshot(),before);
  assert.equal(result.status,behavior==='false'?'refused':'indeterminate');
  assert.equal(named(f,'nemo_native_cancel_inactive_project').length,behavior==='false'?1:0);
  if(behavior!=='false')assert.equal((await f.controller.openPath('/B.json')).status,'unavailable');
});
for (const behavior of ['false','throw','thenable']) test('remove sink failure never retries or rewrites A: '+behavior, async () => {
  const f=fixture({remove(){if(behavior==='throw')throw Error('remove failed');return behavior==='thenable'?Promise.resolve(true):false;}});
  await f.controller.openPath('/B.json');const before=f.snapshot();assert.equal((await f.controller.close(f.rows[0])).status,'indeterminate');
  assert.equal(f.snapshot(),before);assert.equal((await f.controller.close(f.rows[0])).status,'unavailable');
});
test('an edited A can cancel its own retained B with the current revision, without touching A', async () => {
  const f=fixture();await f.controller.openPath('/B.json');f.edit();const before=f.snapshot();
  assert.equal((await f.controller.close(f.rows[0])).status,'cancelled');assert.equal(f.snapshot(),before);
  assert.equal(named(f,'nemo_native_cancel_inactive_project')[0][1].request.expectedRevision,5);
});
test('cancel reply mismatch quarantines the original row and no foreign token is targeted', async () => {
  const f=fixture({invoke(name,args){if(name==='nemo_native_cancel_inactive_project')return {apiVersion:2,requestId:args.request.requestId,preparationToken:'foreign',state:'inactive-cancelled'};}});
  await f.controller.openPath('/B.json');assert.equal((await f.controller.close(f.rows[0])).status,'indeterminate');assert.equal(f.rows.length,1);
  assert.equal(named(f,'nemo_native_cancel_inactive_project')[0][1].request.preparationToken,TOKEN);
});
test('replayed cancelled token cannot append a second row', async () => {
  const f=fixture();await f.controller.openPath('/B.json');await f.controller.close(f.rows[0]);
  assert.equal((await f.controller.openPath('/B.json')).status,'indeterminate');assert.equal(f.rows.length,0);
});
for (const change of ['instanceId','documentId','contentRevision','lifecycleGeneration','subscriptionId','extra']) test('malformed/stale real binding shape refuses before native prepare: '+change, async () => {
  const f=fixture({invoke(name){if(name!=='nemo_native_revision_sync')return;
    return {instanceId:'instance-A',documentId:'native-document-1',contentRevision:4,lifecycleGeneration:17,subscriptionId:'subscription',[change]:change==='lifecycleGeneration'?0:change==='subscriptionId'?'/wrong':'wrong'};}});
  const before=f.snapshot();assert.equal((await f.controller.openPath('/B.json')).status,'refused');assert.equal(f.snapshot(),before);
  assert.equal(named(f,'nemo_native_prepare_inactive_project').length,0);assert.equal(f.rows.length,0);
});
for (const change of ['identity','local','surface','crypto']) test('unknown active A admission refuses: '+change, async () => {
  const f=fixture();if(change==='identity')f.root.NemoNativeOpacityCutover.identity=()=>null;
  if(change==='local')f.root.NemoNativeOpacityCutover.getNativeIdentity=()=>({documentId:'native-document-1',generation:0});
  if(change==='surface')delete f.root.__TAURI__;if(change==='crypto')delete f.root.crypto;
  assert.equal((await f.controller.openPath('/B.json')).status,'refused');assert.equal(f.rows.length,0);assert.equal(named(f,'nemo_native_prepare_inactive_project').length,0);
});
test('commit port changing active A cannot be reported as a successful prepared row', async () => {
  let f;f=fixture({commit(meta,rows){rows.push(meta);f.edit();return true;}});
  assert.equal((await f.controller.openPath('/B.json')).status,'indeterminate');assert.equal(f.identity.contentRevision,5);
  assert.equal(named(f,'nemo_native_cancel_inactive_project').length,0);assert.equal((await f.controller.openPath('/B.json')).status,'unavailable');
});
test('cancel after held native prepare cleans only its correlated token and never commits', async () => {
  const gate=deferred(),f=fixture({invoke:async name=>{if(name==='nemo_native_prepare_inactive_project')await gate.promise;}});
  const pending=f.controller.openPath('/B.json');
  while(!named(f,'nemo_native_prepare_inactive_project').length)await new Promise(r=>setImmediate(r));
  f.controller.cancelPending();gate.resolve();assert.equal((await pending).status,'cancelled');assert.equal(f.rows.length,0);
  assert.equal(named(f,'nemo_native_cancel_inactive_project')[0][1].request.preparationToken,TOKEN);
});
test('wire payload excludes shell/frames/legacy JSON and input file is untouched', async () => {
  const f=fixture(),original=f.files['/B.json'];await f.controller.openPath('/B.json');
  const r=named(f,'nemo_native_prepare_inactive_project')[0][1].request;
  assert.deepEqual(Object.keys(r).sort(),['apiVersion','requestId','instanceId','documentId','expectedRevision','lifecycleGeneration','projection','resources'].sort());
  assert.equal(f.files['/B.json'],original);assert.deepEqual(f.effects,[]);
});
