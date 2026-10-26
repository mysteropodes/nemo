'use strict';
const test=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const vm=require('node:vm');
const Project=require('../src/js/adapters/native-object-project.js');
const Files=require('../src/js/adapters/native-object-project-file.js');
const Workflow=require('../src/js/application/native-object-save-as-workflow.js');
const Ack=require('../src/js/application/native-object-saved-acknowledgement.js');
const turn=()=>new Promise(setImmediate);
function deferred(){let resolve;const promise=new Promise(r=>{resolve=r;});return {promise,resolve};}
function host(settings={}){
  const calls=[];let installed=false,revision=0,edited=false,undo=false,preview='presented',failure=null;
  let reference=null;const copies=[];
  const info=()=>({apiVersion:2,instanceId:'instance',documentId:'document',lifecycleGeneration:1,
    documentSnapshotId:'snapshot-'+revision,contentRevision:revision,canvasW:320,canvasH:180,fps:24,totalFrames:21,
    documentDigest:(edited?'b':'a').repeat(64),byteCount:edited?200000:10000,layers:[{layerUid:'drawing'}],reference,
    undoDepth:edited?1:0,redoDepth:undo?1:0,...settings});
  async function invoke(command,args){
    const r=args?JSON.parse(args.requestJson):null;calls.push({command,r});
    if(command==='nemo_native_object_client_status')return {...info(),available:installed};
    if(command==='nemo_native_object_bootstrap'){
      assert.equal(r.projectOnly,true);assert.equal(typeof r.sourcePath,'string');assert.equal('documentJson' in r,false);
      if(r.sourcePath==='/opacity.json')throw {code:'unsupported_format'};
      if(r.sourcePath==='/bad.json')throw {code:'invalid_request'};
      installed=true;return {...info(),requestId:r.requestId,resourceCount:0,viewportAvailable:false};
    }
    if(command==='nemo_native_object_reference'&&r.action.kind==='inspect'){
      if(failure==='inspection')throw Error('Owner unavailable');
      return {...info(),requestId:r.requestId};
    }
    if(command==='nemo_native_object_preview')return {status:preview,workId:'work-'+revision,viewGeneration:1};
    if(command==='nemo_native_object_reference'||command==='nemo_native_object_dispatch'){
      assert.equal(r.expectedRevision,revision);
      if(command==='nemo_native_object_dispatch'){
        assert.ok(['command.document.object.undo','command.document.object.redo'].includes(r.operation));
        assert.deepEqual(r.payload,{command:r.operation==='command.document.object.undo'?'object.undo':'object.redo'});
      }
      if(failure==='refused')return {apiVersion:2,requestId:r.requestId,instanceId:'instance',documentId:'document',contentRevision:revision,ok:false,error:{code:'invalid_request'}};
      if(failure==='transport-before'||failure==='inspection')throw Error('Native response lost');
      if(r.operation==='command.document.object.undo'){edited=false;undo=true;reference=null;}
      else if(r.operation==='command.document.object.redo'){edited=true;undo=false;}
      else {edited=true;reference={id:'ref',name:'image.png',opacity:0.5,visible:true,offsetFrames:0};}
      revision++;
      if(failure==='transport-after')throw Error('Native response lost');
      return {apiVersion:2,requestId:r.requestId,instanceId:'instance',documentId:'document',contentRevision:revision,ok:true,result:{applied:true,historyEntriesAdded:1}};
    }
    if(command==='nemo_native_object_save_as'){
      copies.push(r);return {apiVersion:2,requestId:r.requestId,instanceId:r.instanceId,documentId:r.documentId,atRevision:r.atRevision,
        documentSnapshotId:r.documentSnapshotId,byteCount:info().byteCount,published:true,cleanupPending:false};
    }
    if(command==='nemo_native_object_export')return {apiVersion:2,requestId:r.requestId,instanceId:'instance',documentId:'document',
      contentRevision:revision,documentSnapshotId:'snapshot-'+revision,status:'succeeded',cleanupStatus:'complete',
      externalEffectDisposition:'committed',errorCode:null,artifactFiles:['frame_0001.png'],published:true,cleanupComplete:true};
    throw Error('Unexpected command: '+command);
  }
  return {invoke,info,calls,copies,preview(value){preview=value;},fail(value){failure=value;}};
}
function session(h){return Project.create({invoke:h.invoke,mapping:info=>({cssBounds:{x:0,y:0,width:320,height:180},
  physicalExtent:{width:320,height:180},compositionExtent:{width:info.canvasW,height:info.canvasH},reportedDpr:1}),frame:()=>0});}
function info(settings={},revision=0){return {apiVersion:2,instanceId:'instance',documentId:'document',contentRevision:revision,
 documentSnapshotId:'snapshot-'+revision,lifecycleGeneration:1,documentDigest:'a'.repeat(64),byteCount:1000,
 canvasW:320,canvasH:180,fps:24,totalFrames:21,layers:[{layerUid:'drawing'}],reference:null,...settings};}
function rootView(value=info()){const root={state:{},userLayers:[]};Files.projectView(root,value);root.state.currentFrame=7;root._curFrame=7;return root;}
test('old metadata stays absent and initial projection keeps baseline white and full work area',async()=>{
 const h=host(),s=session(h);await s.openPath('/project.json');
 for(const k of ['canvasBg','waIn','waOut'])assert.equal(Object.hasOwn(s.info(),k),false);
 const root=rootView(s.info());assert.equal(root.state.canvasBg,'#ffffff');assert.equal(root.state.waIn,0);assert.equal(root.state.waOut,20);
 root.state.waIn=3;root.state.waOut=19;assert.equal(Files.refreshLayerTable(root,s.info()),true);
 assert.equal(root.state.waIn,3);assert.equal(root.state.waOut,19,'older absent refresh preserves established UI values');
});
for(const value of [
 {canvasBg:'#aBc',waIn:-2.5},
 {canvasBg:'aBc',waIn:3.25},
 {canvasBg:'Aa00fF',waOut:-4},
 {canvasBg:'aB12Cd80'},
 {canvasBg:'#Aa00fF',waOut:0},
 {canvasBg:'#aB12Cd80',waIn:100.25,waOut:-20.5},
])test('optional literal background/work-area values are copied independently '+JSON.stringify(value),async()=>{
 const h=host(value),s=session(h);await s.openPath('/project.json');const meta=s.info(),root=rootView(meta);
 assert.equal(Object.isFrozen(meta),true);
 for(const k of ['canvasBg','waIn','waOut']){
  assert.equal(Object.hasOwn(meta,k),Object.hasOwn(value,k));if(Object.hasOwn(value,k))assert.equal(root.state[k],value[k]);
 }
 assert.equal(root._waIn,root.state.waIn);assert.equal(root._waOut,root.state.waOut);
 assert.equal(Files.projectionReadyForUI(root,meta),true);
});
for(const fps of [0.5,29.97,120])test('positive finite storage FPS is preserved without command clamp '+fps,async()=>{
 const h=host({fps}),s=session(h);await s.openPath('/project.json');assert.equal(s.info().fps,fps);assert.equal(rootView(s.info()).state.fps,fps);
});
test('settings revision refresh preserves rows, frame, selection and duration; undo into absence restores defaults',()=>{
 const root=rootView(),rows=root.state.layers,row=rows[0],selection=root._layerSel;
 const next=info({canvasW:640,canvasH:360,fps:0.5,canvasBg:'#AabbCC80',waIn:-2.5,waOut:40.75},1);
 assert.equal(Files.refreshLayerTable(root,next),true);assert.equal(Files.projectionReadyForUI(root,next),true);
 for(const k of ['canvasW','canvasH','fps','canvasBg','waIn','waOut'])assert.equal(root.state[k],next[k]);
 assert.equal(root.state.layers,rows);assert.equal(rows[0],row);assert.equal(root._layerSel,selection);assert.equal(root.state.currentFrame,7);
 assert.equal(root.state.totalFrames,21);assert.equal(root._curFrame,7);assert.equal(root._waIn,-2.5);assert.equal(root._waOut,40.75);
 root.state.canvasBg='#ffffff';assert.equal(Files.projectionReadyForUI(root,next),false);
 assert.equal(Files.refreshLayerTable(root,info({},2)),true);assert.equal(root.state.canvasBg,'#ffffff');assert.equal(root.state.waIn,0);assert.equal(root.state.waOut,20);
 assert.equal(Files.projectionReadyForUI(root,info({},2)),true);
});
test('same-revision settings or duration changes, foreign epochs and stale snapshots refuse without mutation',()=>{
 for(const change of [{fps:12},{canvasW:100},{canvasBg:'#fff'},{waIn:0},{waOut:0},{documentId:'other'},
   {lifecycleGeneration:2},{documentSnapshotId:'other'},{totalFrames:22}]){
  const root=rootView(),before=JSON.stringify(root.state),rows=root.state.layers;
  assert.equal(Files.refreshLayerTable(root,info(change)),false);assert.equal(JSON.stringify(root.state),before);assert.equal(root.state.layers,rows);
 }
});
test('correlated duration growth and Undo project native work-area settings while retaining UI row identity',()=>{
 const root=rootView(),rows=root.state.layers,row=rows[0],selected=root._layerSel;
 const grown=info({totalFrames:30,waOut:29},1);
 assert.equal(Files.refreshLayerTable(root,grown),true);assert.equal(Files.projectionReadyForUI(root,grown),true);
 assert.deepEqual([root.state.totalFrames,root._totalF,root.state.waOut,root._waOut],[30,30,29,29]);
 assert.equal(root.state.layers,rows);assert.equal(rows[0],row);assert.equal(root._layerSel,selected);
 assert.equal(root.state.currentFrame,7);assert.equal(root._curFrame,7);
 root.state.currentFrame=29;root._curFrame=29;
 const undone=info({},2);assert.equal(Files.refreshLayerTable(root,undone),true);
 assert.deepEqual([root.state.totalFrames,root._totalF,root.state.waOut,root._waOut,root.state.currentFrame,root._curFrame],[21,21,20,20,20,20]);
 assert.equal(Files.projectionReadyForUI(root,undone),true);
 assert.equal(root.state.layers,rows);assert.equal(rows[0],row);assert.equal(root._layerSel,selected);
 const redone=info({totalFrames:30,waOut:29},3);assert.equal(Files.refreshLayerTable(root,redone),true);
 assert.equal(Files.projectionReadyForUI(root,redone),true);assert.equal(root.state.currentFrame,20);
});
test('duration publication rejects stale/foreign/malformed metadata and prior UI-duration drift without mutation',()=>{
 for(const values of [{totalFrames:30,contentRevision:-1},{totalFrames:30,documentId:'other'},
   {totalFrames:30,lifecycleGeneration:2}]){
  const root=rootView(),before=JSON.stringify(root.state);
  assert.equal(Files.refreshLayerTable(root,info(values,1)),false);assert.equal(JSON.stringify(root.state),before);
 }
 for(const totalFrames of [0,-1,1.5,Infinity]){
  const root=rootView(),before=JSON.stringify(root.state);
  assert.throws(()=>Files.refreshLayerTable(root,info({totalFrames},1)),/settings/);assert.equal(JSON.stringify(root.state),before);
 }
 const root=rootView();root.state.totalFrames=22;const before=JSON.stringify(root.state);
 assert.equal(Files.refreshLayerTable(root,info({totalFrames:30},1)),false);assert.equal(JSON.stringify(root.state),before);
});
test('malformed/nonfinite optional settings or FPS refuse before session and projectView publication',async()=>{
 const invalid=[{fps:0},{fps:-1},{fps:Infinity},{fps:NaN},{fps:'24'},{fps:null},{canvasBg:null},{canvasBg:'#abcd'},
  {canvasBg:'#GGG'},{canvasBg:'#12345'},{waIn:null},{waIn:'0'},{waIn:Infinity},{waOut:NaN},{waOut:false}];
 for(const values of invalid){
  const h=host(values),s=session(h);await assert.rejects(s.openPath('/project.json'));assert.equal(s.info(),null);
  const root=rootView(),before=JSON.stringify(root.state),rows=root.state.layers,bad=info(values,1);
  assert.throws(()=>Files.projectView(root,bad),/settings/);assert.equal(JSON.stringify(root.state),before);assert.equal(root.state.layers,rows);
  assert.throws(()=>Files.refreshLayerTable(root,bad),/settings/);assert.equal(JSON.stringify(root.state),before);
  assert.equal(Files.projectionReadyForUI(root,bad),false);
 }
});
function settingsOwnerPort(){
  let value=info(),active=false;const writes=[];
  const invoke=async(command,args)=>{const request=args&&JSON.parse(args.requestJson);
    if(command==='nemo_native_object_client_status')return {...value,available:active};
    if(command==='nemo_native_object_bootstrap'){active=true;return {...value,requestId:request.requestId,resourceCount:0,viewportAvailable:false};}
    if(command==='nemo_native_object_reference'&&request.action.kind==='inspect')return {...value,requestId:request.requestId};
    if(command==='nemo_native_object_preview')return {status:'presented',workId:'settings-work',viewGeneration:1};
    throw Error('Unexpected settings owner operation: '+command);};
  const session=Project.create({invoke,projectionReady:()=>true,publishRevision:()=>true,frame:()=>0,
    mapping:()=>({cssBounds:{x:0,y:0,width:320,height:180},physicalExtent:{width:320,height:180},compositionExtent:{width:320,height:180},reportedDpr:1}),
    subscribeRevisions:async()=>({instanceId:'instance',documentId:'document',lifecycleGeneration:1,contentRevision:0,subscriptionId:'settings-subscription'}),
    dispatchApplication:async request=>{writes.push(request);assert.equal(request.operation,'command.document.object.project.settings.edit');
      assert.equal(request.expectedRevision,0);assert.deepEqual(request.payload,{command:'object.project.settings.edit',action:{kind:'setFps',fps:31}});
      value=info({fps:31},1);return {apiVersion:2,requestId:request.requestId,instanceId:'instance',documentId:'document',contentRevision:1,ok:true,result:{applied:true,historyEntriesAdded:1}};}});
  return {session,writes};
}
test('actual Project queue dispatches the shared settings request and presents native readback',async()=>{
  const h=settingsOwnerPort(),s=h.session;await s.openPath('/project.json');const before=s.info();
  assert.equal(typeof s.editProjectSettings,'function');const result=await s.editProjectSettings({kind:'setFps',fps:31},before);
  assert.equal(result.ok,true);assert.equal(h.writes.length,1);assert.equal(s.info().fps,31);assert.equal(s.info().contentRevision,1);
  assert.equal(s.readPresentedObjectSession().presentation.status,'presented');assert.equal(before.fps,24);
});
test('actual queued settings refuses a superseded expected pin before a second native dispatch',async()=>{
  const h=settingsOwnerPort(),s=h.session;await s.openPath('/project.json');const before=s.info(),write=s.editProjectSettings;
  const first=write({kind:'setFps',fps:31},before),second=write({kind:'setFps',fps:42},before);
  const refusal=assert.rejects(second,/intent is stale/);await first;await refusal;
  assert.equal(h.writes.length,1);assert.equal(s.info().fps,31);assert.equal(s.info().contentRevision,1);
});
