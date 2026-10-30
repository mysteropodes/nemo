'use strict';
// Real installed key listener, frame intent, SDK validation and Project queue. Native I/O is modeled;
// these checks do not establish installed-app or native-kernel acceptance.
const test=require('node:test'),assert=require('node:assert/strict'),vm=require('node:vm');
const {execFileSync}=require('node:child_process');
const Bridge=require('../src/js/subselect-bridge.js');
const Project=require('../src/js/adapters/native-object-project.js');
const FrameDesktop=require('../src/js/adapters/native-frame-edit-desktop.js');
const FrameIntent=require('../src/js/adapters/native-frame-edit-intent.js');
const SDK=require('../src/js/adapters/native-application.js');
const {fixture,deferred}=require('./helpers/native-subselect.cjs');
const DELETE='command.document.object.layer.key.clear_cells';
const POINT='command.document.object.subselect.edit';

test('frozen editor gives selected frame cells priority over selected vertices for Delete and Backspace',()=>{
  const source=execFileSync('git',['show','46442ea96c09e6bfaadf956eb8586e273a658085:src/js/timeline.js'],{encoding:'utf8'});
  const start=source.indexOf('function onKeyDown(event){'),end=source.indexOf('function onKeyUp(event)',start);
  assert.ok(start>=0&&end>start);
  for(const key of ['Delete','Backspace']){
    const calls=[],root={state:{tool:'subselect',appMode:'anim2d'},_sel:{frames:[{layer:0,frame:0}]},_nodeSel:[0],selectedPaths:[{}],
      runToolShortcut:()=>false,runCommandShortcut:()=>false,SM:{deleteSelectedFrames:()=>calls.push('frames')},
      nodeEditTargetPath:()=>{throw Error('Point deletion must not run');}};
    root.window=root;vm.runInNewContext(source.slice(start,end),root);
    root.onKeyDown({key,target:{tagName:'DIV'},preventDefault:()=>calls.push('prevent')});
    assert.deepEqual(calls,['prevent','frames']);
  }
});

async function harness(){
  const f=fixture(),root=f.root;let revision=0,installed=false,gate=null;
  const sent=[],history=[];
  const info=()=>({apiVersion:2,instanceId:'i',documentId:'d',lifecycleGeneration:1,contentRevision:revision,
    documentSnapshotId:`native-object:d:${revision}`,documentDigest:'a'.repeat(64),byteCount:100,
    canvasW:200,canvasH:200,fps:24,totalFrames:2,layers:[{layerUid:'L'}],reference:null,undoDepth:revision,redoDepth:0});
  async function dispatchApplication(q){
    let result;
    if(q.operation===DELETE||q.operation===POINT){
      assert.equal(s.readPresentedObjectSession(),null,'Project owns the mutation lease');sent.push(q);
      if(gate)await gate.promise;
      revision++;result={applied:true,historyEntriesAdded:1};history.push(q.operation);
    }else if(q.payload.view){
      const v=q.payload.view;result={...v,atRevision:revision,documentSnapshotId:info().documentSnapshotId,total:1,nextCursor:null,
        preview:!!v.preview,items:[{stableTarget:f.item.stableTarget,closed:true,count:4,matrix:f.item.matrix}]};delete result.limit;
    }else{
      const p=q.payload.subselect;result={atRevision:revision,documentSnapshotId:info().documentSnapshotId,
        subselect:{stableTarget:f.item.stableTarget,closed:true,total:4,cursor:0,nextCursor:null,segments:f.item.segments,preview:!!p.preview}};
    }
    return {apiVersion:2,requestId:q.requestId,instanceId:'i',documentId:'d',contentRevision:revision,ok:true,result};
  }
  const s=Project.create({dispatchApplication,projectionReady:()=>true,publishRevision:()=>true,frame:()=>root.state.currentFrame,
    subscribeRevisions:async()=>({instanceId:'i',documentId:'d',lifecycleGeneration:1,contentRevision:revision,subscriptionId:'subscription'}),
    mapping:()=>({cssBounds:{x:0,y:0,width:200,height:200},physicalExtent:{width:200,height:200},compositionExtent:{width:200,height:200},reportedDpr:1}),
    invoke:async(command,args)=>{const q=args&&JSON.parse(args.requestJson);
      if(command==='nemo_native_object_client_status')return {...info(),available:installed};
      if(command==='nemo_native_object_bootstrap'){installed=true;return {...info(),requestId:q.requestId,resourceCount:0,viewportAvailable:false};}
      if(command==='nemo_native_object_reference')return {...info(),requestId:q.requestId};
      if(command==='nemo_native_object_preview')return {status:'presented',workId:`native-work-${revision+1}`,viewGeneration:1};
      throw Error('Unexpected host command '+command);
    }});
  await s.openPath('/subselect-keyboard.nemo');root.NemoNativeObjectProject.desktop=()=>s;
  root.NemoNativeViewportUnderlay.canvasMapping.matchesView=()=>!!s.readPresentedObjectSession();
  root._sel={frames:[]};root.NemoNativeFrameEditIntent=FrameIntent;root.NemoNativeApplicationAdapter=SDK;
  const frame=FrameDesktop.install(root);
  const d=Object.assign(new EventTarget(),root.document,{readyState:'complete',tagName:'DIV'});root.document=d;
  root.addEventListener=()=>{};
  Bridge.install(root);const controller=root.NemoNativeSubselectController;
  let bubbled=0;d.addEventListener('keydown',()=>bubbled++);
  async function select(){controller.handle('down',f.event(100,60));await controller.settled();
    controller.handle('down',f.event(40,24));controller.handle('up',f.event(40,24));assert.equal(controller.selectionState().count,1);}
  function press(key,target){const e=new Event('keydown',{cancelable:true});e.key=key;
    if(target)Object.defineProperty(e,'target',{value:target});d.dispatchEvent(e);return e;}
  return {...f,controller,s,sent,history,frame,select,press,bubbled:()=>bubbled,hold:()=>{gate=deferred();return gate;}};
}

test('installed Subselect Delete routes selected cells through native frame ownership, ahead of points',async()=>{
  for(const key of ['Delete','Backspace']){
    const h=await harness();await h.select();h.root._sel.frames=[{layer:0,frame:1}];
    assert.equal(h.press(key).defaultPrevented,true);
    const result=await h.controller.settled();assert.equal(result.status,'applied-presented',JSON.stringify(result)+' '+h.notices.join(';'));
    assert.equal(h.bubbled(),0);assert.equal(h.sent.length,1);assert.equal(h.sent[0].operation,DELETE);
    assert.deepEqual(h.sent[0].payload,{command:'object.layer.key.clear_cells',targets:[{layerUid:'L',frame:1}]});
    assert.deepEqual(h.history,[DELETE]);assert.equal(h.s.info().undoDepth,1);assert.equal(h.controller.selectionState(),null);
  }
});

test('without frame cells Delete keeps the native point command, while editable fields keep their keys',async()=>{
  const h=await harness();await h.select();
  for(const target of [{tagName:'INPUT'},{tagName:'TEXTAREA'},{tagName:'SELECT'},{tagName:'DIV',isContentEditable:true}]){
    assert.equal(h.press('Delete',target).defaultPrevented,false);assert.equal(h.press('Escape',target).defaultPrevented,false);
  }
  assert.equal(h.sent.length,0);assert.equal(h.controller.selectionState().count,1);
  h.press('Backspace');assert.equal((await h.controller.settled()).status,'applied');
  assert.equal(h.sent.length,1);assert.equal(h.sent[0].operation,POINT);assert.deepEqual(h.sent[0].payload.intent,{indices:[0],kind:'delete'});
});

test('frame deletion needs no point selection and cancels an active vertex gesture without a second command',async()=>{
  for(const gesture of [false,true]){
    const h=await harness();if(gesture){await h.select();h.controller.handle('down',h.event(40,24));h.controller.handle('move',h.event(52,34));}
    h.root._sel.frames=[{layer:0,frame:1}];h.press('Delete');
    h.controller.handle('up',h.event(52,34));await h.controller.settled();
    assert.deepEqual(h.history,[DELETE]);assert.equal(h.bubbled(),0);assert.equal(h.controller.selectionState(),null);
  }
});

test('missing, stale or busy frame owner never falls through to point or legacy deletion',async()=>{
  for(const change of [h=>delete h.root.NemoNativeFrameEditController,
    h=>h.root.NemoNativeObjectProjectFile.projectionReadyForUI=()=>false,
    h=>h.root.NemoNativeObjectUI={isFrameSelectionPending:()=>true}]){
    const h=await harness();await h.select();h.root._sel.frames=[{layer:0,frame:1}];change(h);
    h.press('Backspace');const result=await h.controller.settled();
    assert.ok(['unavailable','busy'].includes(result.status),JSON.stringify(result));
    assert.equal(h.sent.length,0);assert.equal(h.bubbled(),0);assert.ok(h.notices.length);
  }
});

test('held frame command captures cells synchronously and repeated Delete cannot dispatch again',async()=>{
  const h=await harness();await h.select();h.root._sel.frames=[{layer:0,frame:1}];const gate=h.hold();
  h.press('Delete');h.root._sel.frames=[{layer:0,frame:0}];h.press('Delete');
  await new Promise(setImmediate);assert.equal(h.sent.length,1);
  assert.deepEqual(h.sent[0].payload.targets,[{layerUid:'L',frame:1}]);
  gate.resolve();assert.equal((await h.controller.settled()).status,'applied-presented');
  assert.deepEqual(h.history,[DELETE]);assert.equal(h.bubbled(),0);
});

test('an existing pending point command is not replaced by a frame Delete',async()=>{
  const h=await harness();await h.select();const gate=h.hold();h.press('Delete');
  h.root._sel.frames=[{layer:0,frame:1}];h.press('Backspace');
  await new Promise(setImmediate);assert.equal(h.sent.length,1);assert.equal(h.sent[0].operation,POINT);
  gate.resolve();assert.equal((await h.controller.settled()).status,'applied');assert.deepEqual(h.history,[POINT]);
});

test('a disposed controller cannot consume Delete or Backspace or invoke the frame owner',async()=>{
  for(const key of ['Delete','Backspace']){
    const h=await harness();await h.select();h.controller.dispose();h.root._sel.frames=[{layer:0,frame:1}];
    assert.equal(h.press(key).defaultPrevented,false);await h.controller.settled();
    assert.equal(h.sent.length,0);assert.deepEqual(h.history,[]);assert.equal(h.bubbled(),1);
  }
});
