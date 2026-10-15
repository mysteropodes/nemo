'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const test = require('node:test');
const crypto = require('node:crypto');
const ProjectDocument = require('../../src/js/project-document.js');
const InactiveTabs = require('../../src/js/adapters/native-object-inactive-project-tabs.js');
const NativeObjectProject = require('../../src/js/adapters/native-object-project.js');
const script = fs.readFileSync('tests/desktop/p38-fixture-open-bootstrap.js', 'utf8');
const fixtures = Object.fromEntries(['A', 'B'].map(id => [id,
  `/task/tmp/p38-fixture-open-12345678-1234-1234-1234-123456789abc/${id}.json`]));
const hashes = Object.fromEntries(['A', 'B'].map(id => [id,
  crypto.createHash('sha256').update(fs.readFileSync(`tests/fixtures/p38/native-project/${id}.json`)).digest('hex')]));
const emptyStatus = Object.freeze({apiVersion:2,instanceId:'desktop-instance',available:false,
  route:'nemo_native_object_dispatch',operations:[],documentId:null,contentRevision:null,lifecycleGeneration:null});
const statusA = Object.freeze({apiVersion:2,instanceId:'desktop-instance',available:true,
  route:'nemo_native_object_dispatch',operations:['query.document.object','command.document.object.fill.set','command.document.object.undo','command.document.object.redo'],documentId:'native-doc-A',contentRevision:0,lifecycleGeneration:1});
const statusB = Object.freeze({apiVersion:2,instanceId:'desktop-instance',available:true,
  route:'nemo_native_object_dispatch',operations:['query.document.object','command.document.object.fill.set','command.document.object.undo','command.document.object.redo'],documentId:'native-doc-B',contentRevision:0,lifecycleGeneration:2});
function execute(openResults, configPatch = {}, runtime = {}) {
  const calls = [], opened = [], statusCalls = [], commands = [], readCalls = [], order = [];
  const config = {schema:'nemo.p38-fixture-open-bootstrap/1',runId:'12345678-1234-1234-1234-123456789abc',
    taskKey:'a'.repeat(64),sourceSha:'b'.repeat(40),buildSha:'b'.repeat(40),manifestSha256:'c'.repeat(64),
    pid:42,fixtures:{...fixtures},...configPatch};
  const statusResults = runtime.statusResults ? [...runtime.statusResults] : [emptyStatus,statusA];
  let core, fsBridge, project, window, inactiveProject=null, rowB=null, serial=0, closedClicks=0;
  const pin={apiVersion:2,instanceId:statusA.instanceId,documentId:statusA.documentId,contentRevision:0,
    lifecycleGeneration:1,documentSnapshotId:'snapshot-A',documentDigest:'d'.repeat(64),undoDepth:0,redoDepth:0};
  const notices=[], rows=[], actions=[], preparedResults=[];
  const nameNode=name=>({textContent:name});
  function makeRow(name,id,active){
    const row={dataset:{tab:id},classList:{contains:value=>value==='act'&&active},
      querySelector:selector=>selector==='.pt-name'?nameNode(name):selector==='.pt-close'?{
        click(){closedClicks++;actions.push('close-B');
          if(runtime.closeThrows)throw new Error('private close error');
          if(!runtime.noClose)tabs.close(tabs.held());}}:null,
      click(){actions.push('switch-B');if(!runtime.noSwitch)tabs.activateClick(tabs.held());}};
    return row;
  }
  const bar={querySelectorAll:selector=>selector==='.project-tab'?rows:selector==='[role="alert"]'?notices:[]};
  const tabs=InactiveTabs.create({}, {owner:()=>pin,requestId:()=> 'adapter-'+(++serial),
    invoke:(command,args)=>Reflect.apply(invoke,core,[command,args]),baseName:ProjectDocument.baseName,
    commit(meta){rowB=makeRow(meta.name,'row-B',false);rows.push(rowB);return true;},
    remove(){if(!runtime.retainRow)rows.splice(rows.indexOf(rowB),1);return true;},
    notice(text){notices.length=0;notices.push({textContent:text,getClientRects:()=>runtime.hiddenAlert?[]:[{}]});}});
  const invoke = function(command,args){
    commands.push(command);
    if(command==='nemo_test_p38_fixture_open_probe'){
      const packet=JSON.parse(args.payloadJson); calls.push(packet); order.push(packet.phase);
      if(runtime.onCheckpoint)runtime.onCheckpoint({packet,rows,notices,pin,window});
      return runtime.rejectCheckpointAt===packet.phase?Promise.reject(new Error('private checkpoint failure')):
        packet.phase==='ready'&&runtime.readyPromise?runtime.readyPromise:
        packet.phase==='b-opened'&&runtime.bPromise?runtime.bPromise:
        Promise.resolve(packet.phase==='ready'?(runtime.readyDiagnostic===undefined?{ownerAcknowledged:true}:runtime.readyDiagnostic):undefined);
    }
    if(command==='nemo_native_object_client_status'){
      statusCalls.push({receiverMatchesCore:this===core,argumentCount:arguments.length});
      const result=statusResults.length?statusResults.shift():statusA;
      if(result instanceof Error)throw result;
      if(result&&result.reject) return Promise.reject(result.error||new Error('private status failure'));
      return Promise.resolve(result);
    }
    if(command==='nemo_native_object_reference'){
      const request=JSON.parse(args.requestJson);return Promise.resolve({...pin,requestId:request.requestId,
        ...(runtime.referencePatch||{})});
    }
    if(command==='nemo_native_object_inactive_project'){
      const req=JSON.parse(args.requestJson), action=req.action;
      if(action.kind==='prepare')inactiveProject={preparationToken:'prepared-B',preparationRequestId:req.requestId,
        instanceId:pin.instanceId,documentSnapshotId:'snapshot-B',contentRevision:0,documentDigest:'e'.repeat(64),
        byteCount:1024,canvasW:640,canvasH:360,fps:30,totalFrames:21,layers:[{layerUid:'B'}],
        origin:{documentId:pin.documentId,documentSnapshotId:pin.documentSnapshotId,contentRevision:0}};
      const token=inactiveProject?.preparationToken||null;
      if(action.kind==='close'&&!runtime.retainNative)inactiveProject=null;
      const value={apiVersion:2,requestId:req.requestId,instanceId:req.instanceId,documentId:req.documentId,
        contentRevision:req.expectedRevision,documentSnapshotId:req.documentSnapshotId,
        lifecycleGeneration:req.lifecycleGeneration,retryExecution:false,
        state:action.kind==='prepare'?'inactive-prepared':action.kind==='close'?'inactive-closed':
          inactiveProject?'inactive-retained':'inactive-absent',
        preparationToken:action.kind==='close'?token:inactiveProject?.preparationToken||null,project:inactiveProject};
      if(runtime.onInactive)runtime.onInactive({action,pin,rows,notices,value});
      return Promise.resolve(value);
    }
    return Promise.reject(new Error('unexpected native command'));
  };
  core={};
  Object.defineProperty(core,'invoke',{enumerable:true,configurable:false,get:()=>runtime.missingInvoke?undefined:invoke});
  Object.freeze(core);
  const readTextFile=function(path){
    readCalls.push({path,receiverMatchesFs:this===fsBridge,argumentCount:arguments.length}); order.push('read-A');
    return runtime.readTextFile?Reflect.apply(runtime.readTextFile,this,[path]):Promise.resolve(fs.readFileSync('tests/fixtures/p38/native-project/A.json','utf8'));
  };
  fsBridge={};
  Object.defineProperty(fsBridge,'readTextFile',{enumerable:true,configurable:false,get:()=>{
    if(runtime.throwReadGetter)throw new Error('private fs API path');
    return runtime.missingRead?undefined:runtime.malformedRead?42:readTextFile;
  }});
  Object.freeze(fsBridge);
  const openPath=runtime.openPath||async function(path){opened.push(path); order.push(path===fixtures.A?'open-A':'open-B');
    const events=runtime.openEvents||[['bootstrap-dispatched'],['bootstrap-resolved']];
    for(const [event,code] of events)window.__NEMO_P38_FIXTURE_OPEN_DIAGNOSTIC__(event,code);
    const result=openResults.shift();
    if(path===fixtures.A){if(result===true)rows.push(makeRow(ProjectDocument.baseName(path),'row-A',true));return result;}
    if(result!==true)return result;
    const prepared=await tabs.openPath(path);preparedResults.push(prepared);
    if(runtime.patchPrepared)runtime.patchPrepared(prepared,rows);
    return runtime.preparedResult?runtime.preparedResult(prepared):prepared;};
  project={};
  if(!runtime.missingOpenPath)Object.defineProperty(project,'openPath',{enumerable:true,configurable:false,get:()=>openPath});
  project.getCurrentLabel=()=> runtime.label||'A — '+fixtures.A;
  project.isDirty=()=>runtime.dirty===undefined?false:runtime.dirty;
  Object.freeze(project);
  const coreDescriptor=Object.getOwnPropertyDescriptor(core,'invoke');
  const fsDescriptor=Object.getOwnPropertyDescriptor(fsBridge,'readTextFile');
  const projectDescriptor=Object.getOwnPropertyDescriptor(project,'openPath');
  window={__NEMO_P38_FIXTURE_OPEN__:config,__TAURI__:Object.freeze({core,fs:runtime.missingFs?undefined:fsBridge})};
  window.NemoNativeObjectProject={desktop:()=>({isActive:()=>true,info:()=>pin,
    readPresentedObjectSession:()=>({...pin,presentation:{...pin,status:'presented',frame:0}})})};
  window.state={currentFrame:0};window.document={getElementById:()=>bar};
  window.getComputedStyle=()=>({display:'block',visibility:runtime.hiddenAlert?'hidden':'visible'});
  if(!runtime.lateBind)window.SMProject=project;
  let tick=0;
  const clock=runtime.fastTimeout?{now:()=>Math.floor(tick++/4)*6000}:runtime.expireImmediately?{now:()=>tick++===0?0:60001}:Date;
  vm.runInNewContext(script,{window,Date:clock,setTimeout,Promise,Object,Number,Error,Reflect});
  if(runtime.bind)runtime.bind({project,core,fs:fsBridge,window,readCalls,opened,calls,rows,actions});
  if(runtime.lateBind)window.SMProject=project;
  return Promise.resolve(window.__NEMO_P38_FIXTURE_OPEN_DONE__).then(result=>({result,calls,opened,statusCalls,commands,readCalls,order,closedClicks,actions,preparedResults,
    bridgesPreserved:Object.isFrozen(core)&&Object.isFrozen(fsBridge)&&Object.isFrozen(project)&&
      Object.getOwnPropertyDescriptor(core,'invoke').get===coreDescriptor.get&&
      Object.getOwnPropertyDescriptor(fsBridge,'readTextFile').get===fsDescriptor.get&&
      Object.getOwnPropertyDescriptor(project,'openPath')?.get===projectDescriptor?.get}));
}
test('owner ACK permits the real A read before ordinary native opens',async()=>{
  const r=await execute([true,true]);
  assert.equal(r.result,'complete');
  assert.deepEqual(r.readCalls,[{path:fixtures.A,receiverMatchesFs:true,argumentCount:1}]);
  assert.deepEqual(r.order,['ready','read-A','open-A','a-opened','open-B','b-opened','b-closed']);
  assert.equal(r.bridgesPreserved,true);
});
test('production-shaped inactive B preparation preserves A and is accepted',async()=>{
  const r=await execute([true,true]);
  assert.equal(r.result,'complete');
  assert.equal(r.preparedResults[0].status,'prepared');
  assert.equal(r.preparedResults[0].meta.ownerDocumentId,statusA.documentId);
  assert.deepEqual(r.opened,[fixtures.A,fixtures.B]);
  assert.deepEqual(r.calls.map(x=>x.phase),['ready','a-opened','b-opened','b-closed']);
  assert.deepEqual(r.actions,['switch-B','close-B']);
});
test('exact native A/B opens and correlated status advance fixed checkpoints without bridge mutation', async()=>{
  const r=await execute([true,true]);
  assert.equal(r.result,'complete');
  assert.deepEqual(r.opened,[fixtures.A,fixtures.B]);
  assert.deepEqual(r.calls.map(x=>x.phase),['ready','a-opened','b-opened','b-closed']);
  assert.deepEqual(r.calls.slice(1).map(x=>x.fixtureSha256),[hashes.A,hashes.B,hashes.B]);
  assert.equal(r.statusCalls.every(x=>x.argumentCount===1&&x.receiverMatchesCore),true);
  assert.ok(r.commands.includes('nemo_native_object_reference'));
  assert.ok(r.commands.includes('nemo_native_object_inactive_project'));
  assert.equal(r.bridgesPreserved,true);
  assert.doesNotMatch(JSON.stringify(r.calls),/desktop-instance|native-doc|documentId|lifecycleGeneration/);
});
for(const result of [undefined,null,false,{status:'prepared'},{status:'returned'},42]) test(`only exact true native open result is accepted: ${String(result)}`,async()=>{
  const r=await execute([result]);
  assert.equal(r.result,'refused');
  assert.deepEqual(r.opened,[fixtures.A]);
  assert.deepEqual(r.calls.map(x=>x.phase),['ready','a-opened']);
  assert.equal(r.calls[1].status,'failed');
  assert.equal(r.calls[1].failureCode,'a-native-bootstrap-resolved-open-refused');
  assert.equal(r.bridgesPreserved,true);
});
test('native open throw records only the fixed refusal and does not continue',async()=>{
  const r=await execute([],{},{openPath:async function(path){throw new Error('private path and bytes '+path);}});
  assert.equal(r.result,'refused');
  assert.deepEqual(r.opened,[]);
  assert.equal(r.calls.at(-1).failureCode,'a-open-threw');
  assert.doesNotMatch(JSON.stringify(r.calls),/private|task\/tmp/);
});
test('a real exact staged-file read rejection stops before production Open',async()=>{
  const r=await execute([true],{},{readTextFile(){return Promise.reject(new Error('private A read path'));}});
  assert.equal(r.result,'refused'); assert.deepEqual(r.opened,[]);
  assert.deepEqual(r.readCalls,[{path:fixtures.A,receiverMatchesFs:true,argumentCount:1}]);
  assert.deepEqual(r.calls.map(x=>[x.phase,x.status,x.failureCode]),[
    ['ready','ready',undefined],['a-opened','failed','a-fs-read-rejected']]);
  assert.deepEqual(r.statusCalls,[]);
  assert.doesNotMatch(JSON.stringify(r.calls),/private|task\/tmp/);
});
for(const readyDiagnostic of [null,false,{}, {ownerAcknowledged:false}, {ownerAcknowledged:'true'},
  {aFixtureScope:'allowed'}, {ownerAcknowledged:true,private:'path'}]) test('malformed owner ACK fails before any A read or Open: '+JSON.stringify(readyDiagnostic),async()=>{
  const r=await execute([true],{},{readyDiagnostic});
  assert.equal(r.result,'refused'); assert.deepEqual(r.opened,[]);
  assert.deepEqual(r.readCalls,[]);
  assert.equal(r.calls.at(-1).failureCode,'a-pre-native-refusal');
  assert.doesNotMatch(JSON.stringify(r.calls),/private|path|aFixtureScope|ownerAcknowledged/);
});
for(const runtime of [{missingFs:true},{missingRead:true},{malformedRead:true},{throwReadGetter:true}]) test('missing or malformed fs API fails before Open: '+JSON.stringify(runtime),async()=>{
  const r=await execute([true],{},runtime);
  assert.equal(r.result,'refused'); assert.deepEqual(r.opened,[]); assert.deepEqual(r.readCalls,[]);
  assert.equal(r.calls.at(-1).failureCode,'a-pre-native-refusal');
  assert.equal(r.bridgesPreserved,true);
  assert.doesNotMatch(JSON.stringify(r.calls),/private|task\/tmp/);
});
test('synchronous fs read throw remains sanitized and stops before Open',async()=>{
  const r=await execute([true],{},{readTextFile(){throw new Error('private A bytes/path');}});
  assert.equal(r.result,'refused');assert.deepEqual(r.opened,[]);assert.equal(r.readCalls.length,1);
  assert.equal(r.calls.at(-1).failureCode,'a-fs-read-rejected');
  assert.doesNotMatch(JSON.stringify(r.calls),/private|bytes|path|task\/tmp/);
});
for(const value of [undefined,null,true,42,{},'', 'x'.repeat(1048577)]) test('malformed read result stops before Open: '+(typeof value),async()=>{
  const r=await execute([true],{},{readTextFile:()=>Promise.resolve(value)});
  assert.equal(r.result,'refused'); assert.deepEqual(r.opened,[]); assert.equal(r.readCalls.length,1);
  assert.equal(r.calls.at(-1).failureCode,'a-fs-read-rejected');
});
test('no A read or Open occurs while owner ready ACK is pending',async()=>{
  let release,observed;
  const readyPromise=new Promise(resolve=>{release=resolve;});
  const completion=execute([true,true],{},{readyPromise,bind:value=>{observed=value;}});
  await new Promise(resolve=>setImmediate(resolve));
  assert.deepEqual(observed.calls.map(x=>x.phase),['ready']);
  assert.deepEqual(observed.readCalls,[]); assert.deepEqual(observed.opened,[]);
  release({ownerAcknowledged:true});
  const r=await completion;assert.equal(r.result,'complete');assert.equal(r.readCalls.length,1);
});
test('allowOpen gate refusal and bootstrap failures retain distinct fixed stages',async()=>{
  const gated=await execute([false],{},{openEvents:[['allow-open-refused']]});
  assert.equal(gated.calls.at(-1).failureCode,'a-pre-native-refusal');
  const unsupported=await execute([false],{},{openEvents:[['bootstrap-dispatched'],['bootstrap-rejected','unsupported-format']]});
  assert.equal(unsupported.calls.at(-1).failureCode,'a-native-bootstrap-unsupported-format');
  const rejected=await execute([false],{},{openEvents:[['bootstrap-dispatched'],['bootstrap-rejected','other']]});
  assert.equal(rejected.calls.at(-1).failureCode,'a-native-bootstrap-rejected');
  const resolved=await execute([false],{},{openEvents:[['bootstrap-dispatched'],['bootstrap-resolved']]});
  assert.equal(resolved.calls.at(-1).failureCode,'a-native-bootstrap-resolved-open-refused');
  for(const value of [gated,unsupported,rejected,resolved])
    assert.doesNotMatch(JSON.stringify(value.calls),/private|task\/tmp|unavailable/);
});
test('native object adapter reports only fixed bootstrap outcomes and preserves rejection',async()=>{
  const events=[], error={code:'unavailable',message:'private native path'};
  const session=NativeObjectProject.create({openDiagnostic:(event,code)=>events.push([event,code]),invoke(command){
    if(command==='nemo_native_object_client_status')return Promise.resolve(emptyStatus);
    if(command==='nemo_native_object_bootstrap')return Promise.reject(error);
    throw new Error('unexpected command');
  }});
  await assert.rejects(session.openPath(fixtures.A),e=>e===error);
  assert.deepEqual(events,[['bootstrap-dispatched',undefined],['bootstrap-rejected','other']]);
  assert.doesNotMatch(JSON.stringify(events),/private native path|unavailable/);
});
test('post-bootstrap first finite stage and reason survive probe categorization without private components',async()=>{
  for(const nativeCode of ['unavailable','wrong_instance','wrong_document','cancelled_before_dispatch']){
  for(const stage of ['receipt','installed-status','metadata','first-preview','first-owner','reveal','second-preview','second-owner','ui-commit','publication','subscriber']){
    const code='post-bootstrap-'+stage+':invoke-rejected:'+nativeCode+':notDispatched';
    const r=await execute([false],{},{openEvents:[['bootstrap-dispatched'],['bootstrap-resolved'],
      ['post-bootstrap-failed',code],['post-bootstrap-failed','post-bootstrap-receipt:local-refusal:unknown:other']]});
    assert.equal(r.calls.at(-1).failureCode,'a-'+code);assert.deepEqual(r.opened,[fixtures.A]);
  }
  }
  for(const code of ['post-bootstrap-PRIVATE:invoke-rejected:unknown:other',
    'post-bootstrap-receipt:PRIVATE:unknown:other','post-bootstrap-receipt:local-refusal:PRIVATE:other',
    'post-bootstrap-receipt:local-refusal:unknown:PRIVATE','post-bootstrap-receipt:local-refusal:unknown:other:PRIVATE',null]){
    const r=await execute([false],{},{openEvents:[['bootstrap-dispatched'],['bootstrap-resolved'],['post-bootstrap-failed',code]]});
    assert.equal(r.calls.at(-1).failureCode,'a-native-bootstrap-resolved-open-refused');assert.doesNotMatch(JSON.stringify(r.calls),/PRIVATE/);
  }
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
  const r=await execute([true,true],{},{statusResults:[emptyStatus,statusA,statusA,changed]});
  assert.equal(r.result,'refused');
  assert.deepEqual(r.opened,[fixtures.A]);
  assert.deepEqual(r.calls.map(x=>[x.phase,x.status]),[['ready','ready'],['a-opened','returned'],['b-opened','failed']]);
  assert.equal(r.calls.at(-1).failureCode,'b-pre-native-refusal');
});
for(const badStatus of [statusB,{...statusA,contentRevision:1},{...statusA,instanceId:'other-instance'}])
  test('B preparation cannot replace or change the A owner',async()=>{
    const r=await execute([true,true],{},{statusResults:[emptyStatus,statusA,statusA,statusA,badStatus]});
    assert.equal(r.result,'refused');assert.deepEqual(r.opened,[fixtures.A,fixtures.B]);
    assert.equal(r.calls.at(-1).failureCode,'b-a-state-changed');assert.equal(r.closedClicks,0);
  });
test('B remains visible until accepted owner ACK and a refused ACK never closes it',async()=>{
  let release,observed;
  const bPromise=new Promise((resolve,reject)=>{release={resolve,reject};});
  const completion=execute([true,true],{},{bPromise,bind:value=>{observed=value;}});
  while(!observed.calls.some(value=>value.phase==='b-opened'))await new Promise(resolve=>setImmediate(resolve));
  assert.deepEqual(observed.actions,['switch-B']);assert.equal(observed.rows.length,2);
  release.reject(new Error('private refused owner ACK'));
  const r=await completion;assert.equal(r.result,'refused');assert.equal(r.closedClicks,0);
  assert.deepEqual(r.calls.map(value=>value.phase),['ready','a-opened','b-opened']);
});
for(const [field,value] of [['path',fixtures.A],['ownerDocumentId','wrong-A'],['originSnapshotId','wrong-snapshot'],
  ['originRevision',1],['preparationToken',null],['documentSnapshotId','snapshot-A'],['digest','bad']])
  test('prepared B must match its native owner and fixture: '+field,async()=>{
    const r=await execute([true,true],{},{preparedResult:p=>({...p,meta:{...p.meta,[field]:value}})});
    assert.equal(r.result,'refused');assert.equal(r.calls.at(-1).failureCode,'b-open-unaccepted');
    assert.equal(r.closedClicks,0);
  });
for(const [field,value] of [['documentSnapshotId','changed-A'],['documentDigest','f'.repeat(64)],
  ['undoDepth',1],['redoDepth',1],['contentRevision',1],['lifecycleGeneration',2]])
  test('B switch preserves every native A pin: '+field,async()=>{
    const r=await execute([true,true],{},{fastTimeout:true,onInactive({action,pin}){
      if(action.kind==='inspect'&&action.preparationToken)pin[field]=value;}});
    assert.equal(r.result,'refused');assert.equal(r.calls.at(-1).failureCode,'b-a-state-changed');
    assert.equal(r.closedClicks,0);
  });
for(const runtime of [{noSwitch:true},{hiddenAlert:true},{noClose:true},{retainNative:true},{retainRow:true},{closeThrows:true}])
  test('real switch/close controls and visible native state are required: '+JSON.stringify(runtime),async()=>{
    const r=await execute([true,true],{},{...runtime,fastTimeout:true});
    assert.equal(r.result,'refused');
    assert.equal(r.calls.at(-1).failureCode,runtime.noSwitch||runtime.hiddenAlert?'b-switch-unaccepted':
      runtime.closeThrows?'b-close-threw':'b-close-unaccepted');
    assert.equal(r.calls.some(x=>x.phase==='b-closed'&&x.status==='returned'),false);
    assert.doesNotMatch(JSON.stringify(r.calls),/private|snapshot|native-doc|task\/tmp/);
  });
for(const field of ['undoDepth','redoDepth','documentDigest'])test('close cannot change A: '+field,async()=>{
  const r=await execute([true,true],{},{fastTimeout:true,onInactive({action,pin}){
    if(action.kind==='close')pin[field]=field==='documentDigest'?'f'.repeat(64):1;}});
  assert.equal(r.result,'refused');assert.equal(r.calls.at(-1).failureCode,'b-close-a-state-changed');
});
for(const changed of ['requestId','token','snapshot','origin','digest'])test('B native inspect receipt is correlated: '+changed,async()=>{
  const r=await execute([true,true],{},{fastTimeout:true,onInactive({action,value}){
    if(action.kind!=='inspect'||!action.preparationToken)return;
    if(changed==='requestId')value.requestId='foreign-request';
    else if(changed==='token')value.preparationToken='foreign-token';
    else value.project={...value.project,[changed==='snapshot'?'documentSnapshotId':
      changed==='digest'?'documentDigest':'origin']:changed==='origin'?{documentId:'wrong-owner'}:'foreign'};
  }});
  assert.equal(r.result,'refused');assert.equal(r.calls.at(-1).failureCode,'b-switch-unaccepted');
  assert.equal(r.closedClicks,0);
});
for(const changed of ['rowId','duplicate','activeB'])test('two distinct rows must retain active A: '+changed,async()=>{
  const r=await execute([true,true],{},{patchPrepared(_prepared,rows){
    if(changed==='rowId')rows[1].dataset.tab=rows[0].dataset.tab;
    if(changed==='duplicate')rows.push(rows[1]);
    if(changed==='activeB')rows[1].classList.contains=()=>true;
  }});
  assert.equal(r.result,'refused');assert.equal(r.calls.at(-1).failureCode,'b-open-unaccepted');
  assert.equal(r.closedClicks,0);
});
for(const changed of ['path','dirty','frame'])test('B preparation preserves A UI baseline: '+changed,async()=>{
  const runtime={onInactive({action}){
    if(action.kind==='inspect'&&action.preparationToken){
      if(changed==='path')runtime.label='wrong — '+fixtures.B;
      if(changed==='dirty')runtime.dirty=true;
      if(changed==='frame')observed.window.state.currentFrame=1;
    }
  },bind:value=>{observed=value;}};
  let observed;const r=await execute([true,true],{},runtime);
  assert.equal(r.result,'refused');assert.equal(r.calls.at(-1).failureCode,'b-a-state-changed');
  assert.equal(r.closedClicks,0);
});
for(const changed of ['row','alert','A'])test('B close rechecks prerequisites after owner ACK: '+changed,async()=>{
  const r=await execute([true,true],{},{onCheckpoint({packet,rows,notices,pin}){
    if(packet.phase==='b-opened'){
      if(changed==='row')rows.splice(1,1);
      if(changed==='alert')notices.length=0;
      if(changed==='A')pin.undoDepth=1;
    }
  }});
  assert.equal(r.result,'refused');assert.equal(r.calls.at(-1).failureCode,'b-close-precondition-refused');
  assert.equal(r.closedClicks,0);
});
test('A write capability cannot disappear during inactive B preparation',async()=>{
  const r=await execute([true,true],{},{statusResults:[emptyStatus,statusA,statusA,statusA,{...statusA,operations:[]}]});
  assert.equal(r.result,'refused');assert.equal(r.calls.at(-1).failureCode,'b-a-state-changed');
});
function regionSnapshot(controls=[],alert=false,pid=42) {
  return {pid,windowId:9,matchedAX:true,truncated:false,depthClipped:false,childrenUnreadable:false,
    foreignChildren:false,complete:true,projectTabs:{path:'1',token:'12345678-1234-1234-1234-123456789abc',unique:true},nodes:[
      {path:'0',role:'AXButton',control:'B'}, {path:'1',role:'AXGroup',control:'Project tabs'},
      ...controls.map((control,index)=>({path:'1.'+index,role:'other',control,selected:index===0})),
      ...(alert?[{path:'2',role:'AXStaticText',alert:'switch-unavailable'}]:[])]};
}
test('physical owner evidence requires the retained semantic region and exact selected rows',()=>{
  const {visibleEvidence}=require('./p38-fixture-open-installed.cjs');
  const snapshot=regionSnapshot,expected=phase=>({phase,pid:42,windowId:9,regionToken:snapshot().projectTabs.token});
  for(const [phase,controls,alert] of [['ready',[],false],['a-opened',['A.json'],false],
    ['b-opened',['A.json','B.json'],true],['b-closed',['A'],false]])
    assert.doesNotThrow(()=>visibleEvidence(snapshot(controls,alert),expected(phase)));
  for(const [phase,controls,alert] of [['a-opened',['A.json','B.json'],true],
    ['b-opened',['B.json'],true],['b-opened',['A.json','B.json'],false],['b-closed',['A.json','B.json'],false]])
    assert.throws(()=>visibleEvidence(snapshot(controls,alert),expected(phase)),/probe-visible-state-refused/);
  const good=snapshot(['A.json','B.json'],true),pin=expected('b-opened');
  for(const change of [s=>delete s.projectTabs,s=>s.projectTabs.unique=false,s=>s.projectTabs.token='bad',
    s=>s.projectTabs.token='87654321-1234-1234-1234-123456789abc',s=>s.nodes.push({...s.nodes[1],path:'3'}),
    s=>s.nodes[2].selected=false,s=>s.nodes[3].selected=true,s=>delete s.nodes[2].selected,
    s=>s.nodes.push({...s.nodes[2],path:'1.8'}),s=>s.nodes[2].blocked='child-owner',
    ...['truncated','depthClipped','childrenUnreadable','foreignChildren'].map(k=>s=>s[k]=true),
    s=>s.complete=false,s=>delete s.complete]){
    const bad=structuredClone(good);change(bad);assert.throws(()=>visibleEvidence(bad,pin));
  }
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
test('first inspection diagnostic uses only literal finite categories, with no later read',async()=>{
  const {observeProjectTabs}=require('./p38-fixture-open-installed.cjs'),identity={pid:42,windowId:9};
  const token='12345678-1234-1234-1234-123456789abc';
  const reply=request=>({status:'ok',type:'inspect',...identity,matchedAX:true,complete:true,
    truncated:false,depthClipped:false,childrenUnreadable:false,foreignChildren:false,
    nodes:request.projectTabs?[{path:'0',role:'AXGroup',control:'Project tabs',childCount:0}]:[
      {path:'',role:'AXWindow',childCount:1},{path:'0',role:'AXGroup',control:'Project tabs',childCount:0}],
    ...(request.projectTabs?{projectTabs:{path:'0',token}}:{})});
  const cases=[
    ['inspection-status-refused',s=>s.status='refused'],
    ['inspection-status-refused',s=>s.type='private-type'],
    ['inspection-identity-refused',s=>s.pid=777],
    ['inspection-identity-refused',s=>s.windowId=10],
    ['inspection-identity-refused',s=>s.matchedAX=false],
    ['inspection-projection-refused',s=>s.nodes[0].path='/private/AX/path'],
    ['inspection-projection-refused',s=>s.nodes[0].childCount=-1],
    ['inspection-root-refused',s=>s.nodes=[]],
    ['inspection-root-refused',s=>s.nodes[0].path='2'],
    ['inspection-completeness-refused',s=>s.foreignChildren=true],
    ['inspection-completeness-refused',s=>s.childrenUnreadable=true],
    ['inspection-completeness-refused',s=>s.nodes[0].blocked='child-owner'],
    ['inspection-completeness-refused',s=>delete s.nodes[0].childCount],
    ['inspection-completeness-refused',s=>delete s.complete],
    ['inspection-marker-refused',s=>s.nodes[1].control='private-title'],
    ['inspection-marker-refused',s=>{s.nodes[0].childCount=2;s.nodes.push({...s.nodes[1],path:'1'});}],
  ];
  for(const [code,mutate] of cases){let reads=0;await assert.rejects(observeProjectTabs(async request=>{
    reads++;const raw=reply(request);mutate(raw);raw.private='/private/secret';return raw;
  },identity),error=>{assert.equal(error.code,code);assert.equal(error.message,code);return true;});assert.equal(reads,1,code);}
  let reads=0;await assert.rejects(observeProjectTabs(async()=>{reads++;throw Error('/private/secret token pid777');},identity),
    {code:'inspection-transport-refused'});assert.equal(reads,1);
  await assert.rejects(observeProjectTabs(async()=>{throw Error('must not read');},identity,{actions:63}),{code:'inspection-budget-refused'});
  for(const change of [s=>delete s.projectTabs,s=>s.projectTabs.path='1',s=>s.complete=false,
    s=>s.projectTabs.token='87654321-1234-1234-1234-123456789abc']){
    let calls=0;await assert.rejects(observeProjectTabs(async request=>{calls++;const raw=reply(request);
      if(request.projectTabs)change(raw);return raw;},identity,{actions:0,token}),{code:'inspection-region-refused'});
    assert.equal(calls,2);
  }
  const good=await observeProjectTabs(async request=>reply(request),identity);assert.equal(good.projectTabs.unique,true);
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
    const snapshot=regionSnapshot(['A.json'],false,process.pid);
    const expected={...pins,phase:'a-opened',windowId:9,regionToken:snapshot.projectTabs.token};
    assert.throws(()=>acknowledge(root,expected,'a'.repeat(32),'accepted',
      {...snapshot,nodes:[snapshot.nodes[1],{path:'1.0',role:'other',control:'B.json',selected:true}]}),/probe-visible-state-refused/);
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
