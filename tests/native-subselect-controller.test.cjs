'use strict';
const test=require('node:test'),assert=require('node:assert/strict');
const Bridge=require('../src/js/subselect-bridge.js');
const Contract=require('../src/js/adapters/native-subselect-contract.js');
const {fixture,deferred}=require('./helpers/native-subselect.cjs');
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

test('native pointer modifiers match frozen point, group, Shift and Alt transition traces',async()=>{
  const oracle=require('../native-engine/tests/fixtures/subselect-baseline/modifiers.json');
  function close(actual,expected,label){
    if(typeof expected==='number'){assert.ok(Math.abs(actual-expected)<1e-9,`${label}: ${actual} != ${expected}`);return;}
    if(expected&&typeof expected==='object'){assert.deepEqual(Object.keys(actual),Object.keys(expected),label);for(const k of Object.keys(expected))close(actual[k],expected[k],label+'.'+k);return;}
    assert.equal(actual,expected,label);
  }
  for(const row of oracle.cases){const f=fixture();f.item.matrix=row.matrix;f.item.segments=structuredClone(row.segments);
    const event=(p,extra={})=>f.event(...Bridge.map(row.matrix,p),extra);
    f.controller.handle('down',event([50,50]));await f.controller.settled();
    for(const index of row.indices){const p=row.segments[index].point;f.controller.handle('down',event(p,{shiftKey:true}));f.controller.handle('up',event(p,{shiftKey:true}));}
    f.controller.handle('down',event(row.start,{shiftKey:row.downShift,altKey:row.altKey}));
    for(const step of row.steps){f.controller.handle('move',event(step.point,{shiftKey:step.shiftKey,altKey:step.altKey}));await new Promise(setImmediate);
      assert.ok(f.previews.length,row.name+': '+f.notices.join(';'));close(f.previews.at(-1),step.intent,row.name);assert.equal(f.sent.length,0);}
    const last=row.steps.at(-1);f.controller.handle('up',event(last.point,{shiftKey:last.shiftKey,altKey:last.altKey}));
    assert.equal((await f.controller.settled()).status,'applied',row.name+': '+f.notices.join(';'));assert.equal(f.sent.length,1);close(f.sent[0].payload.intent,last.intent,row.name);
  }
});
