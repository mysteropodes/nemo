'use strict';
const test=require('node:test'),assert=require('node:assert/strict');
const Gate=require('../src/js/adapters/native-settings-validation-gate.js');
const turn=()=>new Promise(setImmediate);
const schema='nemo.settings-validation/1';
function fixture(mode='settings-response',overrides={}){
  const runtime={schema:'nemo.native-runtime/1',isolated:true,state:'active',ownerTokenConfigured:true,
    pid:123,windows:['main'],taskKey:'a'.repeat(64)};
  const config={schema,taskKey:runtime.taskKey,runId:'b'.repeat(32),mode};
  const records=[],waiters=[];let packet=null,timer=null,time=0;
  const ports={now:()=>++time,setTimer:(fn,ms)=>{assert.equal(ms,60000);timer=fn;return fn;},clearTimer:()=>{timer=null;},
    delay:()=>new Promise(resolve=>waiters.push(resolve)),read:async()=>packet,
    write:async bytes=>records.push(JSON.parse(bytes)),...overrides};
  const request={requestId:'real-request-1',instanceId:'instance-1',documentId:'native-object-document-1',
    expectedRevision:0,documentSnapshotId:'native-object:native-object-document-1:0',lifecycleGeneration:1,
    ...(mode==='settings-response'?{operation:'command.document.object.project.settings.edit',payload:{action:{kind:'setFps',fps:48}}}:
      {action:{kind:'inspect',preparationToken:'native-preparation-1'}})};
  const value=mode==='settings-response'?{ok:true,contentRevision:1,result:{applied:true,historyEntriesAdded:1}}:
    {state:'inactive-retained',project:{documentId:'native-object-document-2'}};
  const gate=Gate.create(runtime,config,ports);
  return {runtime,config,ports,request,value,gate,records,
    release(extra={}){packet={schema,taskKey:config.taskKey,runId:config.runId,requestId:request.requestId,action:'release',...extra};},
    async tick(){waiters.splice(0).forEach(fn=>fn());await turn();},expire(){assert.ok(timer);timer();}};
}
test('no gate without an exact owned isolated task configuration',()=>{
  const f=fixture();
  for(const runtime of [null,{...f.runtime,isolated:false},{...f.runtime,state:'released'},
    {...f.runtime,ownerTokenConfigured:false},{...f.runtime,windows:['main','other']},{...f.runtime,pid:0}])
    assert.equal(Gate.create(runtime,f.config,f.ports),null);
  for(const config of [null,{...f.config,taskKey:'c'.repeat(64)},{...f.config,runId:'bad'},
    {...f.config,mode:'eval'},{...f.config,script:'anything'}])assert.equal(Gate.create(f.runtime,config,f.ports),null);
});
for(const mode of ['settings-response','inactive-inspect'])test(mode+' holds exactly one actual receipt until correlated release',async()=>{
  const f=fixture(mode);await f.gate.ready;
  let finished=false;
  const response=f.gate.hold(mode,f.request,f.value).then(value=>{finished=true;return value;});await turn();
  assert.equal(finished,false);assert.equal(f.records.at(-1).events.at(-1).phase,'held');
  for(const wrong of [{taskKey:'c'.repeat(64)},{runId:'d'.repeat(32)},{requestId:'another'},{extra:true},{action:'retry'}]){
    f.release(wrong);await f.tick();assert.equal(finished,false);
  }
  f.release();await f.tick();assert.equal(await response,f.value,'same receipt object, never a fabricated ACK');
  assert.deepEqual(f.records.at(-1).events.map(e=>e.phase),['ready','held','released']);
  const second={...f.request,requestId:'real-request-2'};
  assert.equal(await f.gate.hold(mode,second,f.value),f.value);assert.equal(f.records.length,3,'one-shot');
});
test('unrelated operations and initial inactive inspection do not consume the gate',async()=>{
  const a=fixture();await a.gate.ready;
  assert.equal(await a.gate.hold('settings-response',{...a.request,payload:{action:{kind:'setCanvasSize'}}},a.value),a.value);
  const failure={ok:false};assert.equal(await a.gate.hold('settings-response',a.request,failure),failure);
  const unchanged={ok:true,result:{applied:false,historyEntriesAdded:0}};
  assert.equal(await a.gate.hold('settings-response',a.request,unchanged),unchanged);
  assert.equal(a.records.length,1);
  const actual=a.gate.hold('settings-response',a.request,a.value);await turn();
  assert.equal(a.records.at(-1).events.at(-1).phase,'held');a.release();await a.tick();assert.equal(await actual,a.value);
});
test('inactive preparation inspect without a token remains untouched',async()=>{
  const f=fixture('inactive-inspect');await f.gate.ready;
  assert.equal(await f.gate.hold('inactive-inspect',{...f.request,action:{kind:'inspect'}},f.value),f.value);
  assert.equal(f.records.length,1);
});
test('expiration releases the unchanged native result even if diagnostics I/O hangs',async()=>{
  let count=0;const f=fixture('settings-response',{write:async()=>{if(++count===2)await new Promise(()=>{});}});
  await f.gate.ready;const response=f.gate.hold('settings-response',f.request,f.value);await turn();
  f.expire();assert.equal(await response,f.value);
});
test('failed native transport promises are never converted to success',async()=>{
  const root={},error=new Error('real transport failure');
  const promise=Promise.reject(error);assert.equal(Gate.hold(root,'settings-response',{},promise),promise);
  await assert.rejects(promise,value=>value===error);
});
test('absent or still initializing probe never delays ordinary production responses',async()=>{
  let reads=0;const root={__TAURI__:{core:{invoke:()=>new Promise(()=>{})},fs:{readTextFile:()=>{reads++;}}}};
  const promise=Promise.resolve({ok:true});assert.equal(Gate.hold(root,'settings-response',{},promise),promise);
  assert.deepEqual(await promise,{ok:true});assert.equal(reads,0);
});
test('standard nonisolated runtime never reads a control file',async()=>{
  let reads=0;const root={__TAURI__:{core:{invoke:async()=>({schema:'nemo.native-runtime/1',isolated:false})},
    fs:{readTextFile:()=>{reads++;}}}};
  await Gate.install(root).ready;assert.equal(reads,0);
});
test('pending readiness persistence leaves the real response Promise untouched',async()=>{
  const f=fixture();let writes=0;
  const runtime={...f.runtime,dataDir:'/owned/tauri-data',processEnvironment:{tempDir:'/owned/tmp',NEMO_REPORT_DIR:'/owned/reports'}};
  const root={__TAURI__:{core:{invoke:async()=>runtime},fs:{
    readTextFile:async()=>JSON.stringify(f.config),writeTextFile:()=>{writes++;return new Promise(()=>{});},rename:async()=>{}}}};
  Gate.install(root);await turn();assert.equal(writes,1);
  const promise=Promise.resolve(f.value);assert.equal(Gate.hold(root,'settings-response',f.request,promise),promise);
  assert.equal(await promise,f.value);
});
test('installation uses only the exact owned temporary scope and refuses sibling or foreign roots',async()=>{
  const f=fixture(),base={...f.runtime,dataDir:'/owned/tauri-data'};
  for(const env of [{tempDir:'/owned/reports',NEMO_REPORT_DIR:'/owned/reports'},
    {tempDir:'/other/tmp',NEMO_REPORT_DIR:'/owned/reports'},
    {tempDir:'/owned/tmp',NEMO_REPORT_DIR:'/other/reports'}]){
    let reads=0;const root={__TAURI__:{core:{invoke:async()=>({...base,processEnvironment:env})},fs:{readTextFile:async()=>{reads++;}}}};
    await Gate.install(root).ready;assert.equal(reads,0);
  }
  const paths=[],root={__TAURI__:{core:{invoke:async()=>({...base,processEnvironment:{tempDir:'/owned/tmp',NEMO_REPORT_DIR:'/owned/reports'}})},
    fs:{readTextFile:async p=>{paths.push(p);return JSON.stringify(f.config);},
      writeTextFile:async p=>{paths.push(p);},rename:async(a,b)=>{paths.push(a,b);}}}};
  await Gate.install(root).ready;
  assert.equal(paths.length,4);assert.ok(paths.every(p=>p.startsWith('/owned/tmp/settings-validation')));
  assert.equal(paths.at(-1),'/owned/tmp/settings-validation.json');
});
test('oversized or malformed packets fail closed using only admitted filesystem commands',async()=>{
  const f=fixture(),runtime={...f.runtime,dataDir:'/owned/tauri-data',processEnvironment:{tempDir:'/owned/tmp',NEMO_REPORT_DIR:'/owned/reports'}};
  for(const bytes of ['x'.repeat(4097),'é'.repeat(2049),'{invalid',null]){
    let writes=0;const root={__TAURI__:{core:{invoke:async()=>runtime},fs:{readTextFile:async()=>bytes,
      writeTextFile:async()=>{writes++;},rename:async()=>{}}}};
    await Gate.install(root).ready;assert.equal(writes,0);
    const promise=Promise.resolve(f.value);assert.equal(Gate.hold(root,'settings-response',f.request,promise),promise);
  }
});
