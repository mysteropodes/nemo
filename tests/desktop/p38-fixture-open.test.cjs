'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const test = require('node:test');
const crypto = require('node:crypto');
const script = fs.readFileSync('tests/desktop/p38-fixture-open-bootstrap.js', 'utf8');
const fixtures = Object.fromEntries(['A', 'B'].map(id => [id,
  `/task/tmp/p38-fixture-open-12345678-1234-1234-1234-123456789abc/${id}.json`]));
const hashes = Object.fromEntries(['A', 'B'].map(id => [id,
  crypto.createHash('sha256').update(fs.readFileSync(`tests/fixtures/p38/open/${id}.json`)).digest('hex')]));
function execute(openResults, configPatch = {}, runtime = {}) {
  const calls = [], opened = [];
  const config = { schema:'nemo.p38-fixture-open-bootstrap/1', runId:'12345678-1234-1234-1234-123456789abc',
    taskKey:'a'.repeat(64), sourceSha:'b'.repeat(40), buildSha:'b'.repeat(40), manifestSha256:'c'.repeat(64),
    pid:42, fixtures:{...fixtures}, ...configPatch };
  const checkpointInvoke = async (_name,args)=>{calls.push(JSON.parse(args.payloadJson));};
  const baseInvoke = function(name,args){
    if(name==='nemo_test_p38_fixture_open_probe')return checkpointInvoke(name,args);
    return runtime.invoke ? Reflect.apply(runtime.invoke,this,arguments) : Promise.resolve({available:false});
  };
  const core = {};
  if(runtime.observerInstallFailure)Object.defineProperty(core,'invoke',{configurable:true,get:()=>baseInvoke,set:()=>{throw new Error('private observer install detail');}});
  else core.invoke=(runtime.lateInvoke||runtime.missingInvoke)?null:baseInvoke;
  const fs = {readTextFile:function(){return runtime.readTextFile ? Reflect.apply(runtime.readTextFile,this,arguments) : Promise.resolve('{}');}};
  const project = runtime.missingOpenPath ? {} : {openPath:runtime.openPath || async function(path){opened.push(path);return openResults.shift();}};
  const originalOpen=project.openPath, originalInvoke=runtime.missingInvoke?null:baseInvoke, originalRead=fs.readTextFile;
  const window = { [ '__NEMO_P38_FIXTURE_OPEN__' ]:config, __TAURI__:{core,fs} };
  if(!runtime.lateBind)window.SMProject=project;
  let tick=0;
  const clock=runtime.expireImmediately?{now:()=>tick++===0?0:60001}:Date;
  vm.runInNewContext(script, {window, Date:clock, setTimeout, Promise, Object, Number, Error});
  if(runtime.lateInvoke)core.invoke=baseInvoke;
  if(runtime.bind)runtime.bind({project,core,fs,window});
  if(runtime.lateBind)window.SMProject=project;
  return Promise.resolve(window.__NEMO_P38_FIXTURE_OPEN_DONE__).then(result=>({result,calls,opened,
    restored:project.openPath===originalOpen&&core.invoke===originalInvoke&&fs.readTextFile===originalRead}));
}
test('fixed A/B sequence accepts legacy undefined and adapter prepared results', async()=>{
  for (const values of [[undefined,undefined],[{status:'prepared'},{status:'prepared'}]]) {
    const r=await execute([...values]); assert.equal(r.result,'complete');
    assert.deepEqual(r.opened, [fixtures.A,fixtures.B]); assert.deepEqual(r.calls.map(x=>x.phase), ['ready','a-opened','b-opened']);
    assert.deepEqual(r.calls.slice(1).map(x=>x.fixtureSha256), [hashes.A,hashes.B]);
    assert.equal(r.restored,true);
  }
});
for (const result of [false, {status:'refused'}, {status:'unavailable'}, {status:'indeterminate'},
  {status:'cancelled'}, {status:'prepared',extra:true}, null, true]) test(`open refusal does not advance to B: ${JSON.stringify(result)}`, async()=>{
  const r=await execute([result]); assert.equal(r.result,'refused'); assert.deepEqual(r.opened,[fixtures.A]);
  assert.deepEqual(r.calls.map(x=>x.phase), ['ready','a-opened']); assert.equal(r.calls[1].status,'failed');
  assert.equal(r.restored,true);
  if(result===false)assert.equal(r.calls[1].failureCode,'a-pre-native-refusal');
});
test('tampered static identity sends no checkpoint and opens no fixture', async()=>{
  const r=await execute([undefined], {buildSha:'d'.repeat(40)});
  assert.equal(r.result,undefined); assert.deepEqual(r.calls,[]); assert.deepEqual(r.opened,[]);
});

test('pre-ready openPath absence persists one fixed cursor-zero refusal and opens no fixture', async()=>{
  const r=await execute([], {}, {missingOpenPath:true,expireImmediately:true});
  assert.equal(r.result,'refused'); assert.deepEqual(r.opened,[]);
  assert.deepEqual(r.calls.map(x=>[x.phase,x.status,x.failureCode,x.fixtureId,x.fixtureSha256]),
    [['ready','failed','ready-open-path-unavailable',undefined,undefined]]);
  assert.doesNotMatch(JSON.stringify(r.calls),/private|exception|must not leak/);
  assert.equal(r.restored,true);
});

test('absent Tauri invoke cannot fabricate a persisted pre-ready checkpoint', async()=>{
  const r=await execute([], {}, {missingInvoke:true,expireImmediately:true});
  assert.equal(r.result,'refused'); assert.deepEqual(r.calls,[]); assert.deepEqual(r.opened,[]);
});

test('native observer installation failure with valid openPath is not mislabeled openPath-unavailable', async()=>{
  const r=await execute([], {}, {observerInstallFailure:true,expireImmediately:true});
  assert.equal(r.result,'refused'); assert.deepEqual(r.opened,[]); assert.deepEqual(r.calls,[]);
  assert.equal(r.restored,true);
});

test('categorical native outcomes preserve invoke receiver, arguments, and Promise identity', async()=>{
  const expectedPath=fixtures.A, observed={args:null,identity:false};
  const statusArgs={statusProbe:'opaque'}, bootstrapArgs={requestJson:'opaque'};
  const statusExpected=Promise.resolve({available:false});
  const bootstrapExpected=Promise.reject({code:'unsupported_format'});
  let projectRef, coreRef;
  const result=await execute([], {}, {openPath:function(path){
    assert.equal(this,projectRef);
    observed.args=arguments.length===1&&path===expectedPath;
    const status=Reflect.apply(coreRef.invoke,coreRef,['nemo_native_object_client_status',statusArgs]);
    const bootstrap=Reflect.apply(coreRef.invoke,coreRef,['nemo_native_object_bootstrap',bootstrapArgs]);
    observed.identity=status===statusExpected&&bootstrap===bootstrapExpected;
    assert.equal(status instanceof Promise,true); assert.equal(bootstrap instanceof Promise,true);
    return Promise.resolve(false);
  }, invoke:function(command){
    assert.equal(this,coreRef); assert.equal(arguments.length,2);
    if(command==='nemo_native_object_client_status') { assert.equal(arguments[1],statusArgs); return statusExpected; }
    if(command==='nemo_native_object_bootstrap') { assert.equal(arguments[1],bootstrapArgs); return bootstrapExpected; }
    throw new Error('unexpected invoke');
  },bind:refs=>{projectRef=refs.project;coreRef=refs.core;}});
  assert.equal(result.result,'refused'); assert.equal(observed.args,true); assert.equal(observed.identity,true);
  assert.equal(result.calls[1].failureCode,'a-native-bootstrap-unsupported-format');
  assert.equal(result.restored,true);
});

test('fs-read outcome is categorical and arbitrary rejection text is not retained', async()=>{
  let coreRef, fsRef;
  const result=await execute([], {}, {openPath:function(){
    return Reflect.apply(coreRef.invoke,coreRef,['nemo_native_object_client_status'])
      .then(()=>Reflect.apply(coreRef.invoke,coreRef,['nemo_native_object_bootstrap']))
      .then(()=>Reflect.apply(fsRef.readTextFile,fsRef,['/private/path/A.json']),
        ()=>Reflect.apply(fsRef.readTextFile,fsRef,['/private/path/A.json']))
      .then(()=>false,()=>false);
  }, invoke:function(command){
    if(command==='nemo_native_object_client_status')return Promise.resolve({available:false});
    if(command==='nemo_native_object_bootstrap')return Promise.reject({code:'unsupported_format',message:'must not leak'});
    throw new Error('unexpected invoke');
  }, readTextFile:function(){return Promise.reject(new Error('secret path and bytes'));},
  bind:refs=>{coreRef=refs.core;fsRef=refs.fs;}});
  assert.equal(result.calls[1].failureCode,'a-fs-read-rejected');
  assert.doesNotMatch(JSON.stringify(result.calls),/private|secret|unsupported_format|must not leak/);
  assert.equal(result.restored,true);
});

test('early native observation precedes and sees the adapter cached invoke binding', async()=>{
  let cachedInvoke;
  const result=await execute([], {}, {lateBind:true,bind:refs=>{cachedInvoke=refs.core.invoke.bind(refs.core);},
    openPath:function(){
      return Reflect.apply(cachedInvoke,this,['nemo_native_object_bootstrap',{requestJson:'opaque'}])
        .then(()=>false,()=>false);
    },invoke:function(command){
      assert.equal(command,'nemo_native_object_bootstrap');
      return Promise.reject({code:'unsupported_format'});
    }});
  assert.equal(result.calls[1].failureCode,'a-native-bootstrap-unsupported-format');
  assert.equal(result.restored,true);
});

test('late native observation cannot label an adapter-cached open as pre-native refusal', async()=>{
  let cachedInvoke;
  const result=await execute([], {}, {lateInvoke:true,
    bind:refs=>{cachedInvoke=refs.core.invoke.bind(refs.core);},
    openPath:function(){
      return Reflect.apply(cachedInvoke,this,['nemo_native_object_bootstrap',{requestJson:'opaque'}])
        .then(()=>false,()=>false);
    },invoke:function(command){
      assert.equal(command,'nemo_native_object_bootstrap');
      return Promise.reject({code:'unsupported_format'});
    }});
  assert.equal(result.calls[1].failureCode,'a-observer-late-untrusted');
  assert.equal(result.restored,true);
});

test('late core with an early project adapter fails closed for A and B', async()=>{
  for (const fixtureId of ['A','B']) {
    let cachedInvoke, openCount=0;
    const result=await execute(fixtureId==='B'?[undefined]:[], {}, {lateInvoke:true,
      bind:refs=>{cachedInvoke=refs.core.invoke.bind(refs.core);},
      openPath:function(path){
        openCount++;
        if (fixtureId==='B' && path===fixtures.A) return undefined;
        return Reflect.apply(cachedInvoke,this,['nemo_native_object_bootstrap',{requestJson:'opaque'}])
          .then(()=>false,()=>false);
      },invoke:function(command){
        assert.equal(command,'nemo_native_object_bootstrap');
        return Promise.reject({code:'unsupported_format'});
      }});
    assert.equal(result.result,'refused');
    assert.equal(openCount,fixtureId==='A'?1:2);
    assert.equal(result.calls.at(-1).failureCode,`${fixtureId.toLowerCase()}-observer-late-untrusted`);
    assert.equal(result.restored,true);
  }
});

test('synchronous invoke and fs-read throws preserve the original error identity', async()=>{
  const invokeMarker={kind:'invoke-marker'}, fsMarker={kind:'fs-marker'};
  let coreRef,fsRef;
  const invokeResult=await execute([], {}, {openPath:function(){
    try { Reflect.apply(coreRef.invoke,coreRef,['nemo_native_object_client_status']); }
    catch(error) { assert.equal(error,invokeMarker); }
    return false;
  },invoke:function(){throw invokeMarker;},bind:refs=>{coreRef=refs.core;}});
  assert.equal(invokeResult.calls[1].failureCode,'a-native-status-rejected');
  assert.equal(invokeResult.restored,true);
  const fsResult=await execute([], {}, {openPath:function(){
    try { Reflect.apply(fsRef.readTextFile,fsRef,['/private/path/A.json']); }
    catch(error) { assert.equal(error,fsMarker); }
    return false;
  },readTextFile:function(){throw fsMarker;},bind:refs=>{fsRef=refs.fs;}});
  assert.equal(fsResult.calls[1].failureCode,'a-fs-read-rejected');
  assert.equal(fsResult.restored,true);
});

test('native status gates are reported as fixed availability categories', async()=>{
  for (const [available, expected] of [[false,'a-native-status-empty-no-bootstrap'],
    [true,'a-native-status-available-no-bootstrap']]) {
    let coreRef;
    const result=await execute([], {}, {openPath:function(){
      return Reflect.apply(coreRef.invoke,coreRef,['nemo_native_object_client_status']).then(()=>false);
    }, invoke:function(command){
      assert.equal(command,'nemo_native_object_client_status');
      return Promise.resolve({available});
    }, bind:refs=>{coreRef=refs.core;}});
    assert.equal(result.calls[1].failureCode,expected);
    assert.equal(result.restored,true);
  }
});

test('owner runner requires observed original-window fixture and alert before ack', async()=>{
  const fs = require('node:fs'), os = require('node:os'), path = require('node:path');
  const {acknowledge,visibleEvidence} = require('./p38-fixture-open-installed.cjs');
  const base=fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(),'p38-installed-'))), tmp=path.join(base,'tmp');
  fs.mkdirSync(tmp); const root=path.join(tmp,'p38-fixture-open-test'); fs.mkdirSync(root);
  try {
    for (const id of ['A','B']) fs.copyFileSync(`tests/fixtures/p38/open/${id}.json`,path.join(root,`${id}.json`));
    const fixturePins=['A','B'].map(id=>{const file=path.join(root,`${id}.json`),stat=fs.statSync(file);
      return {id,file,sha256:hashes[id],device:stat.dev,inode:stat.ino};});
    const pins={schema:'nemo.p38-fixture-open/1',runId:'12345678-1234-1234-1234-123456789abc',
      taskKey:'a'.repeat(64),sourceSha:'b'.repeat(40),buildSha:'b'.repeat(40),pid:process.pid,window:'main',
      fixtures:fixturePins,origin:'tauri://localhost/index.html',checkpoint:1,waiting:true,failed:false};
    const pinBytes=JSON.stringify({schema:pins.schema,runId:pins.runId,taskKey:pins.taskKey,sourceSha:pins.sourceSha,
      buildSha:pins.buildSha,pid:pins.pid,window:pins.window,origin:pins.origin,fixtures:pins.fixtures});
    pins.manifestSha256=crypto.createHash('sha256').update(pinBytes).digest('hex');
    fs.writeFileSync(path.join(root,'manifest.json'),JSON.stringify(pins));
    const expected={...pins,phase:'a-opened',windowId:9};
    const snapshot={pid:process.pid,windowId:9,matchedAX:true,nodes:[
      {path:'1',role:'AXStaticText',control:'A.json'}, {path:'2',role:'AXStaticText',alert:'switch-unavailable'}]};
    assert.throws(()=>acknowledge(root,expected,'a'.repeat(32),'accepted',
      {...snapshot,nodes:[{path:'1',role:'AXStaticText',control:'B.json'}]}),/probe-visible-state-refused/);
    for(const windowId of [undefined,0,-1,4294967296])
      assert.throws(()=>visibleEvidence(snapshot,{...expected,windowId}),/probe-window-pin-refused/);
    assert.throws(()=>acknowledge(root,{...expected,windowId:undefined},'a'.repeat(32),'refused'),/probe-window-pin-refused/);
    assert.throws(()=>visibleEvidence(snapshot,{...expected,phase:'unknown'}),/probe-phase-refused/);
    const alias=path.join(base,'same-A.json'), original=path.join(base,'original-A.json');
    fs.copyFileSync(path.join(root,'A.json'),alias); fs.renameSync(path.join(root,'A.json'),original);
    fs.symlinkSync(alias,path.join(root,'A.json'));
    assert.throws(()=>acknowledge(root,expected,'a'.repeat(32),'accepted',snapshot),/probe-file-refused/);
    fs.unlinkSync(path.join(root,'A.json')); fs.renameSync(original,path.join(root,'A.json'));
    const pinned=path.join(base,'pinned-A.json'); fs.renameSync(path.join(root,'A.json'),pinned);
    fs.copyFileSync(alias,path.join(root,'A.json'));
    assert.throws(()=>acknowledge(root,expected,'a'.repeat(32),'accepted',snapshot),/probe-fixture-identity-refused/);
    fs.unlinkSync(path.join(root,'A.json')); fs.renameSync(pinned,path.join(root,'A.json'));
    const result=acknowledge(root,expected,'a'.repeat(32),'accepted',snapshot);
    assert.equal(result.phase,'a-opened');
    const ack=JSON.parse(fs.readFileSync(path.join(root,'ack.json'),'utf8'));
    assert.equal(ack.decision,'accepted'); assert.equal(ack.ownerToken,'a'.repeat(32));
  } finally { fs.rmSync(base,{recursive:true,force:true}); }
});
