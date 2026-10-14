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
  crypto.createHash('sha256').update(fs.readFileSync(`tests/fixtures/p38/native-project/${id}.json`)).digest('hex')]));
const emptyStatus = Object.freeze({apiVersion:2,instanceId:'desktop-instance',available:false,
  route:'nemo_native_object_dispatch',operations:[],documentId:null,contentRevision:null,lifecycleGeneration:null});
const statusA = Object.freeze({apiVersion:2,instanceId:'desktop-instance',available:true,
  route:'nemo_native_object_dispatch',operations:['query.document.object'],documentId:'native-doc-A',contentRevision:0,lifecycleGeneration:1});
const statusB = Object.freeze({apiVersion:2,instanceId:'desktop-instance',available:true,
  route:'nemo_native_object_dispatch',operations:['query.document.object'],documentId:'native-doc-B',contentRevision:0,lifecycleGeneration:2});
function execute(openResults, configPatch = {}, runtime = {}) {
  const calls = [], opened = [], statusCalls = [], commands = [];
  const config = {schema:'nemo.p38-fixture-open-bootstrap/1',runId:'12345678-1234-1234-1234-123456789abc',
    taskKey:'a'.repeat(64),sourceSha:'b'.repeat(40),buildSha:'b'.repeat(40),manifestSha256:'c'.repeat(64),
    pid:42,fixtures:{...fixtures},...configPatch};
  const statusResults = runtime.statusResults ? [...runtime.statusResults] : [emptyStatus,statusA,statusA,statusB];
  let core, fsBridge, project;
  const invoke = function(command,args){
    commands.push(command);
    if(command==='nemo_test_p38_fixture_open_probe'){
      const packet=JSON.parse(args.payloadJson); calls.push(packet);
      return runtime.rejectCheckpointAt===packet.phase?Promise.reject(new Error('private checkpoint failure')):Promise.resolve();
    }
    if(command==='nemo_native_object_client_status'){
      statusCalls.push({receiverMatchesCore:this===core,argumentCount:arguments.length});
      const result=statusResults.shift();
      if(result instanceof Error)throw result;
      if(result&&result.reject) return Promise.reject(result.error||new Error('private status failure'));
      return Promise.resolve(result);
    }
    return Promise.reject(new Error('unexpected native command'));
  };
  core={};
  Object.defineProperty(core,'invoke',{enumerable:true,configurable:false,get:()=>runtime.missingInvoke?undefined:invoke});
  Object.freeze(core);
  const readTextFile=function(){throw new Error('legacy file read must not be used by the probe');};
  fsBridge={};
  Object.defineProperty(fsBridge,'readTextFile',{enumerable:true,configurable:false,get:()=>readTextFile});
  Object.freeze(fsBridge);
  const openPath=runtime.openPath||async function(path){opened.push(path);return openResults.shift();};
  project={};
  if(!runtime.missingOpenPath)Object.defineProperty(project,'openPath',{enumerable:true,configurable:false,get:()=>openPath});
  Object.freeze(project);
  const coreDescriptor=Object.getOwnPropertyDescriptor(core,'invoke');
  const fsDescriptor=Object.getOwnPropertyDescriptor(fsBridge,'readTextFile');
  const projectDescriptor=Object.getOwnPropertyDescriptor(project,'openPath');
  const window={__NEMO_P38_FIXTURE_OPEN__:config,__TAURI__:Object.freeze({core,fs:fsBridge})};
  if(!runtime.lateBind)window.SMProject=project;
  let tick=0;
  const clock=runtime.expireImmediately?{now:()=>tick++===0?0:60001}:Date;
  vm.runInNewContext(script,{window,Date:clock,setTimeout,Promise,Object,Number,Error,Reflect});
  if(runtime.bind)runtime.bind({project,core,fs:fsBridge,window});
  if(runtime.lateBind)window.SMProject=project;
  return Promise.resolve(window.__NEMO_P38_FIXTURE_OPEN_DONE__).then(result=>({result,calls,opened,statusCalls,commands,
    bridgesPreserved:Object.isFrozen(core)&&Object.isFrozen(fsBridge)&&Object.isFrozen(project)&&
      Object.getOwnPropertyDescriptor(core,'invoke').get===coreDescriptor.get&&
      Object.getOwnPropertyDescriptor(fsBridge,'readTextFile').get===fsDescriptor.get&&
      Object.getOwnPropertyDescriptor(project,'openPath')?.get===projectDescriptor?.get}));
}
test('exact native A/B opens and correlated status advance fixed checkpoints without bridge mutation', async()=>{
  const r=await execute([true,true]);
  assert.equal(r.result,'complete');
  assert.deepEqual(r.opened,[fixtures.A,fixtures.B]);
  assert.deepEqual(r.calls.map(x=>x.phase),['ready','a-opened','b-opened']);
  assert.deepEqual(r.calls.slice(1).map(x=>x.fixtureSha256),[hashes.A,hashes.B]);
  assert.deepEqual(r.statusCalls.map(x=>x.argumentCount),[1,1,1,1]);
  assert.equal(r.statusCalls.every(x=>x.receiverMatchesCore),true);
  assert.deepEqual(r.commands,['nemo_test_p38_fixture_open_probe','nemo_native_object_client_status',
    'nemo_native_object_client_status','nemo_test_p38_fixture_open_probe','nemo_native_object_client_status',
    'nemo_native_object_client_status','nemo_test_p38_fixture_open_probe']);
  assert.equal(r.bridgesPreserved,true);
  assert.doesNotMatch(JSON.stringify(r.calls),/desktop-instance|native-doc|documentId|lifecycleGeneration/);
});
for(const result of [undefined,null,false,{status:'prepared'},{status:'returned'},42]) test(`only exact true native open result is accepted: ${String(result)}`,async()=>{
  const r=await execute([result]);
  assert.equal(r.result,'refused');
  assert.deepEqual(r.opened,[fixtures.A]);
  assert.deepEqual(r.calls.map(x=>x.phase),['ready','a-opened']);
  assert.equal(r.calls[1].status,'failed');
  assert.equal(r.calls[1].failureCode,result===false?'a-open-false':'a-open-unaccepted');
  assert.equal(r.bridgesPreserved,true);
});
test('native open throw records only the fixed refusal and does not continue',async()=>{
  const r=await execute([],{},{openPath:async function(path){throw new Error('private path and bytes '+path);}});
  assert.equal(r.result,'refused');
  assert.deepEqual(r.opened,[]);
  assert.equal(r.calls.at(-1).failureCode,'a-open-threw');
  assert.doesNotMatch(JSON.stringify(r.calls),/private|task\/tmp/);
});
test('invalid empty native status refuses before opening A',async()=>{
  const r=await execute([true],{},{statusResults:[null]});
  assert.equal(r.result,'refused');
  assert.deepEqual(r.opened,[]);
  assert.equal(r.calls.at(-1).failureCode,'a-pre-native-refusal');
});
for(const badStatus of [
  {...statusA,apiVersion:1},
  {...statusA,instanceId:'other-instance'},
  {...statusA,available:false},
  {...statusA,route:'legacy'},
  {...statusA,documentId:null},
  {...statusA,contentRevision:1},
  {...statusA,lifecycleGeneration:2},
  null,
  new Error('private status details'),
]) test(`malformed or contradictory A status refuses before success checkpoint: ${String(badStatus&&badStatus.apiVersion)}`,async()=>{
  const r=await execute([true],{},{statusResults:[emptyStatus,badStatus]});
  assert.equal(r.result,'refused');
  assert.deepEqual(r.opened,[fixtures.A]);
  assert.deepEqual(r.calls.map(x=>x.phase),['ready','a-opened']);
  assert.equal(r.calls.at(-1).status,'failed');
  assert.equal(r.calls.at(-1).failureCode,'a-open-unaccepted');
  assert.doesNotMatch(JSON.stringify(r.calls),/private status details|other-instance|native-doc/);
});
test('native state must remain A until the B open starts',async()=>{
  const changed={...statusA,lifecycleGeneration:2};
  const r=await execute([true,true],{},{statusResults:[emptyStatus,statusA,changed,statusB]});
  assert.equal(r.result,'refused');
  assert.deepEqual(r.opened,[fixtures.A]);
  assert.deepEqual(r.calls.map(x=>[x.phase,x.status]),[['ready','ready'],['a-opened','returned'],['b-opened','failed']]);
  assert.equal(r.calls.at(-1).failureCode,'b-pre-native-refusal');
});
for(const badStatus of [
  {...statusB,documentId:statusA.documentId},
  {...statusB,lifecycleGeneration:statusA.lifecycleGeneration},
  {...statusB,contentRevision:1},
  {...statusB,instanceId:'other-instance'},
  {...statusB,route:'legacy'},
]) test(`B replacement must advance native identity and generation: ${badStatus.documentId}/${badStatus.lifecycleGeneration}`,async()=>{
  const r=await execute([true,true],{},{statusResults:[emptyStatus,statusA,statusA,badStatus]});
  assert.equal(r.result,'refused');
  assert.deepEqual(r.opened,[fixtures.A,fixtures.B]);
  assert.deepEqual(r.calls.map(x=>[x.phase,x.status]),[
    ['ready','ready'],['a-opened','returned'],['b-opened','failed']]);
  assert.equal(r.calls.at(-1).failureCode,'b-open-unaccepted');
});
test('status query failure is reduced to a fixed refusal without raw details',async()=>{
  const r=await execute([true],{},{statusResults:[emptyStatus,new Error('private status path') ]});
  assert.equal(r.result,'refused');
  assert.equal(r.calls.at(-1).failureCode,'a-open-unaccepted');
  assert.doesNotMatch(JSON.stringify(r.calls),/private status path|native-doc|desktop-instance/);
});
test('checkpoint refusal stops before opening the next fixture',async()=>{
  const r=await execute([true,true],{},{rejectCheckpointAt:'a-opened'});
  assert.equal(r.result,'refused');
  assert.deepEqual(r.opened,[fixtures.A]);
  assert.deepEqual(r.calls.map(x=>x.phase),['ready','a-opened']);
});
test('tampered static identity sends no checkpoint and opens no fixture',async()=>{
  const r=await execute([true,true],{buildSha:'d'.repeat(40)});
  assert.equal(r.result,undefined); assert.deepEqual(r.calls,[]); assert.deepEqual(r.opened,[]);
});
test('pre-ready openPath absence persists one fixed cursor-zero refusal and opens no fixture',async()=>{
  const r=await execute([],{}, {missingOpenPath:true,expireImmediately:true});
  assert.equal(r.result,'refused'); assert.deepEqual(r.opened,[]);
  assert.deepEqual(r.calls.map(x=>[x.phase,x.status,x.failureCode,x.fixtureId,x.fixtureSha256]),
    [['ready','failed','ready-open-path-unavailable',undefined,undefined]]);
  assert.doesNotMatch(JSON.stringify(r.calls),/exception|private/);
  assert.equal(r.bridgesPreserved,true);
});
test('absent Tauri invoke cannot fabricate a persisted pre-ready checkpoint',async()=>{
  const r=await execute([],{}, {missingInvoke:true,expireImmediately:true});
  assert.equal(r.result,'refused'); assert.deepEqual(r.calls,[]); assert.deepEqual(r.opened,[]);
  assert.equal(r.bridgesPreserved,true);
});

test('owner runner requires observed original-window fixture and alert before ack', async()=>{
  const fs = require('node:fs'), os = require('node:os'), path = require('node:path');
  const {acknowledge,visibleEvidence} = require('./p38-fixture-open-installed.cjs');
  const base=fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(),'p38-installed-'))), tmp=path.join(base,'tmp');
  fs.mkdirSync(tmp); const root=path.join(tmp,'p38-fixture-open-test'); fs.mkdirSync(root);
  try {
    for (const id of ['A','B']) fs.copyFileSync(`tests/fixtures/p38/native-project/${id}.json`,path.join(root,`${id}.json`));
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
