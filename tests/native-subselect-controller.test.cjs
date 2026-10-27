'use strict';
const test=require('node:test'),assert=require('node:assert/strict');
const Bridge=require('../src/js/subselect-bridge.js'),Contract=require('../src/js/adapters/native-subselect-contract.js');
const Command=require('../src/js/adapters/native-object-vector-command.js'),Paper=require('./helpers/native-import-paper.cjs');
function deferred(){let resolve;const promise=new Promise(r=>resolve=r);return {promise,resolve};}
function fixture(){
  const paper=Paper.load(),rect={left:0,top:0,right:200,bottom:200,width:200,height:200},stage={getBoundingClientRect:()=>rect,setPointerCapture(){},releasePointerCapture(){}};
  const dom=()=>({style:{},setAttribute(){},appendChild(){},remove(){}}),notices=[],sent=[];
  let pin={instanceId:'i',documentId:'d',lifecycleGeneration:1,contentRevision:0,documentSnapshotId:'native-object:d:0',canvasW:200,canvasH:200,layers:[{layerUid:'L'}]};
  let presentation={...pin,owner:'native',status:'presented',contextId:'scene-root',frame:0,workId:'native-work-1',viewGeneration:1};
  let queued=false,gate=null,readGate=null,transportFailures=0,reconcileFailure=false;
  const item={stableTarget:{contextId:'scene-root',layerUid:'L',strokeId:'s',frameScope:{kind:'authored',frame:0}},closed:true,count:4,matrix:[1.5,.2,.5,1,0,0],segments:[[20,20],[80,20],[80,80],[20,80]].map(point=>({point,handleIn:[0,0],handleOut:[0,0]}))};
  let items=[item];
  const shown=()=>({...pin,presentation}),s={info:()=>pin,isActive:()=>true,blocksLegacy:()=>true,requestId:()=>`q-${sent.length}`,readPresentedObjectSession:()=>queued?null:shown(),
    readSubselect:async()=>{const data={info:pin,presentation,items};if(readGate)await readGate.promise;return data;},
    previewSubselect:async()=>({info:pin,presentation,segments:item.segments,matrix:item.matrix}),
    runResourceCommand:async action=>{const origin=shown();queued=true;try{if(gate)await gate.promise;return await action({readPresentedObjectSession:()=>origin,
      dispatch:async q=>{sent.push(q);if(transportFailures>0){transportFailures--;throw Error('Transport lost');}return {apiVersion:2,requestId:q.requestId,instanceId:q.instanceId,documentId:q.documentId,contentRevision:q.expectedRevision+1,ok:true,result:{applied:true,historyEntriesAdded:1}};},
      reconcile:async revision=>{if(reconcileFailure)throw Error('Owner read unavailable');pin={...pin,contentRevision:revision,documentSnapshotId:`native-object:d:${revision}`};presentation={...presentation,...pin};}});}finally{queued=false;}}};
  const root={paper,state:{tool:'subselect',appMode:'anim2d',currentFrame:0,activeLayerIdx:0,layers:[{layerUid:'L'}]},_layerSel:[0],showToast:m=>notices.push(m),
    document:{getElementById:()=>stage,createElementNS:dom,body:dom(),documentElement:{clientWidth:200,clientHeight:200,classList:{contains:()=>true},style:{getPropertyValue:k=>String(k.endsWith('right')||k.endsWith('bottom')?200:0)}}},
    NemoNativeObjectProject:{desktop:()=>s},NemoNativeObjectProjectFile:{projectionReadyForUI:()=>true},NemoNativeObjectVectorCommand:Command,NemoNativeSubselectContract:Contract,
    NemoNativeViewportUnderlay:{canvasMapping:{point:e=>({x:e.clientX,y:e.clientY}),transform:()=>[1,0,0,1,0,0],matchesView:()=>!!s.readPresentedObjectSession()}}};
  const controller=Bridge.create(root),event=(x,y,extra={})=>({clientX:x,clientY:y,pointerId:1,target:stage,preventDefault(){},stopImmediatePropagation(){},...extra});
  async function select(){controller.handle('down',event(100,60));await controller.settled();assert.equal(paper.project.activeLayer.children.length,0);}
  function drag(){controller.handle('down',event(40,24));controller.handle('move',event(52,34));}
  return {root,s,controller,event,select,drag,sent,notices,item,paper,setFrame:n=>{root.state.currentFrame=n;presentation={...presentation,frame:n};},setItems:v=>items=v,setGate:g=>gate=g,failTransport:count=>{transportFailures=count;reconcileFailure=true;},allowRecovery:()=>{reconcileFailure=false;},setReadGate:g=>readGate=g,replace:()=>{pin={...pin,documentId:'replacement',documentSnapshotId:'native-object:replacement:0'};presentation={...presentation,...pin};}};
}
test('subselect dispatches through a lease while the public presentation is hidden by the real command adapter',async()=>{
  const f=fixture();await f.select();f.drag();f.controller.handle('up',f.event(52,34));const result=await f.controller.settled();
  assert.equal(result.status,'applied',f.notices.join(';'));assert.equal(f.sent.length,1);
  const delta=f.sent[0].payload.intent.delta;assert.ok(Math.abs(1.5*delta[0]+.5*delta[1]-12)<1e-10);assert.ok(Math.abs(.2*delta[0]+delta[1]-10)<1e-10);
});
test('subselect pointer cancel and cancellation while queued never dispatch',async()=>{
  for(const queued of [false,true]){const f=fixture();await f.select();f.drag();const gate=deferred();if(queued){f.setGate(gate);f.controller.handle('up',f.event(52,34));}
    f.controller.cancel();gate.resolve();await f.controller.settled();assert.equal(f.sent.length,0);}
});
test('replacement document and mode changes invalidate pending reads and gestures',async()=>{
  for(const change of [f=>f.replace(),f=>f.root.state.spaceDown=true,f=>f.root.state.isPanning=true,f=>f.root.state.shadowMode=true,f=>f.root.state.activeSymbolId='component']){
    const f=fixture();await f.select();f.drag();change(f);f.controller.handle('up',f.event(52,34));await f.controller.settled();assert.equal(f.sent.length,0);
  }
  const f=fixture(),gate=deferred();f.setReadGate(gate);f.controller.handle('down',f.event(100,60));f.replace();gate.resolve();await f.controller.settled();await f.controller.setPointType('smooth');assert.equal(f.sent.length,0);
});
test('subselect inverse preserves shear and refuses singular transforms',()=>{
  const m=[1.5,.2,.5,1,8,-4],p=[35,-17],q=Bridge.inverse(m,Bridge.map(m,p));assert.ok(q.every((v,i)=>Math.abs(v-p[i])<1e-10));assert.throws(()=>Bridge.inverse([1,2,2,4,0,0],p));
});

test('native marquee matches the frozen production selection oracle without document writes',async()=>{
  const oracle=require('../native-engine/tests/fixtures/subselect-baseline/oracle.json');assert.equal(oracle.marquee.length,7);
  for(const row of oracle.marquee)for(const delayed of [false,true]){
    const f=fixture(),items=row.items.map((item,i)=>({...item,count:item.segments.length,matrix:row.matrices[i],stableTarget:{...f.item.stableTarget,strokeId:'s'+i}}));
    f.setItems(items);
    if(row.target!==undefined){f.setItems([items[row.target]]);const p=Bridge.map(items[row.target].matrix,[50,50]);f.controller.handle('down',f.event(...p));await f.controller.settled();f.setItems(items);}
    const gate=deferred();if(delayed)f.setReadGate(gate);
    f.controller.handle('down',f.event(...row.from));if(!delayed)await f.controller.settled();
    f.controller.handle('move',f.event(...row.to));f.controller.handle('up',f.event(...row.to));
    // Natural capture loss follows pointerup; it must not discard an awaiting read.
    f.controller.handle('lost',f.event(...row.to));gate.resolve();await f.controller.settled();
    assert.equal(f.sent.length,0,row.name);assert.equal(f.paper.project.activeLayer.children.length,0,row.name);
    const result=await f.controller.setPointType('smooth');
    if(row.expected.target===null){assert.equal(result.status,'refused',row.name);assert.equal(f.sent.length,0);}
    else {assert.equal(result.status,'applied',row.name+': '+f.notices.join(';'));assert.equal(f.sent.length,1);
      assert.equal(f.sent[0].payload.stableTarget.strokeId,'s'+row.expected.target,row.name);assert.deepEqual(f.sent[0].payload.intent.indices,row.expected.indices,row.name);}
  }
});
test('marquee cancellation, owner and view changes fence delayed selection and all subsequent writes',async()=>{
  for(const cancel of [f=>f.controller.key({key:'Escape',preventDefault(){},stopImmediatePropagation(){}}),f=>f.controller.handle('lost',f.event(5,5)),
    f=>f.controller.handle('cancel',f.event(5,5)),f=>f.replace(),f=>f.root.state.currentFrame=1,f=>f.root.state.tool='draw']){
    const f=fixture(),gate=deferred();f.setReadGate(gate);f.controller.handle('down',f.event(0,0));f.controller.handle('move',f.event(80,100));
    cancel(f);f.controller.handle('up',f.event(80,100));gate.resolve();await f.controller.settled();await f.controller.setPointType('smooth');assert.equal(f.sent.length,0);
  }
});
test('marquee selects multiple transformed anchors for one subsequent native drag',async()=>{
  const f=fixture();f.controller.handle('down',f.event(0,0));await f.controller.settled();f.controller.handle('move',f.event(80,100));f.controller.handle('up',f.event(80,100));await f.controller.settled();
  f.drag();f.controller.handle('up',f.event(52,34));assert.equal((await f.controller.settled()).status,'applied',f.notices.join(';'));
  assert.equal(f.sent.length,1);assert.deepEqual(f.sent[0].payload.intent.indices,[0,3]);
});
test('a new vertex drag or Alt-insert supersedes an unresolved earlier marquee',async()=>{
  for(const insert of [false,true]){
    const f=fixture();await f.select();const gate=deferred();f.setReadGate(gate);
    f.controller.handle('down',f.event(0,0));f.controller.handle('move',f.event(80,100));f.controller.handle('up',f.event(80,100));const prior=f.controller.settled();
    if(insert)f.controller.handle('down',f.event(85,30,{altKey:true}));else{f.drag();f.controller.handle('up',f.event(52,34));}
    gate.resolve();await prior;assert.equal((await f.controller.settled()).status,'applied',f.notices.join(';'));
    assert.equal(f.sent.length,1);assert.equal(f.sent[0].payload.intent.kind,insert?'insert':'move');
    if(!insert)assert.deepEqual(f.sent[0].payload.intent.indices,[0]);
  }
});
test('held gestures retain the source target and commit promotion only at pointerup',async()=>{
  for(const gesture of ['drag','click','cancel']){const f=fixture();f.setFrame(5);await f.select();
    f.controller.handle('down',f.event(40,24));assert.equal(f.sent.length,0);
    if(gesture==='drag')f.controller.handle('move',f.event(52,34));
    if(gesture==='cancel')f.controller.handle('cancel',f.event(40,24));
    f.controller.handle('up',f.event(...(gesture==='drag'?[52,34]:[40,24])));await f.controller.settled();
    assert.equal(f.sent.length,gesture==='cancel'?0:1,f.notices.join(';'));
    if(f.sent.length){assert.equal(f.sent[0].payload.atFrame,5);assert.equal(f.sent[0].payload.stableTarget.frameScope.frame,0);
      if(gesture==='click')assert.deepEqual(f.sent[0].payload.intent.delta,[0,0]);}
  }
});

test('subselect preserves an indeterminate outcome and replays the original request after retiring the gesture',async()=>{
  const f=fixture();await f.select();f.drag();f.failTransport(2);f.controller.handle('up',f.event(52,34));
  assert.equal((await f.controller.settled()).status,'indeterminate');assert.equal(f.sent.length,2);
  f.allowRecovery();assert.equal((await f.controller.recover()).status,'reconciled',f.notices.join(';'));assert.equal(f.sent.length,3);assert.deepEqual(f.sent[2],f.sent[0]);
});
test('subselect completes through the actual Project queue and its action-local presentation lease',async()=>{
  const Project=require('../src/js/adapters/native-object-project.js'),f=fixture();let revision=0,installed=false;const sent=[];
  const info=()=>({apiVersion:2,instanceId:'i',documentId:'d',lifecycleGeneration:1,contentRevision:revision,documentSnapshotId:`native-object:d:${revision}`,
    documentDigest:'a'.repeat(64),byteCount:100,canvasW:200,canvasH:200,fps:24,totalFrames:2,layers:[{layerUid:'L'}],reference:null,undoDepth:revision,redoDepth:0});
  const dispatchApplication=async q=>{
        let result;
        if(q.operation===Contract.EDIT){assert.equal(s.readPresentedObjectSession(),null);sent.push(q);revision++;result={applied:true,historyEntriesAdded:1};}
        else if(q.payload.view){const v=q.payload.view;result={...v,atRevision:revision,documentSnapshotId:info().documentSnapshotId,total:1,nextCursor:null,preview:!!v.preview,items:[{stableTarget:f.item.stableTarget,closed:true,count:4,matrix:f.item.matrix}]};delete result.limit;}
        else {const p=q.payload.subselect;result={atRevision:revision,documentSnapshotId:info().documentSnapshotId,subselect:{stableTarget:f.item.stableTarget,closed:true,total:4,cursor:0,nextCursor:null,segments:f.item.segments,preview:!!p.preview}};}
        return {apiVersion:2,requestId:q.requestId,instanceId:'i',documentId:'d',contentRevision:revision,ok:true,result};
  };
  const s=Project.create({dispatchApplication,projectionReady:()=>true,frame:()=>0,mapping:()=>({cssBounds:{x:0,y:0,width:200,height:200},physicalExtent:{width:200,height:200},compositionExtent:{width:200,height:200},reportedDpr:1}),
    invoke:async(command,args)=>{const q=args&&JSON.parse(args.requestJson);
      if(command==='nemo_native_object_client_status')return {...info(),available:installed};
      if(command==='nemo_native_object_bootstrap'){installed=true;return {...info(),requestId:q.requestId,resourceCount:0,viewportAvailable:false};}
      if(command==='nemo_native_object_reference')return {...info(),requestId:q.requestId};
      if(command==='nemo_native_object_preview')return {status:'presented',workId:`native-work-${revision+1}`,viewGeneration:1};
      throw Error('Unexpected host command '+command);
    }});
  await s.openPath('/subselect-fixture.nemo');f.root.NemoNativeObjectProject.desktop=()=>s;
  f.root.NemoNativeViewportUnderlay.canvasMapping.matchesView=()=>!!s.readPresentedObjectSession();
  await f.select();f.drag();f.controller.handle('up',f.event(52,34));assert.equal((await f.controller.settled()).status,'applied',f.notices.join(';'));assert.equal(sent.length,1);assert.equal(s.info().contentRevision,1);
});
