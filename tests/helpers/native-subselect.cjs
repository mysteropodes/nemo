'use strict';
const assert=require('node:assert/strict');
const Bridge=require('../../src/js/subselect-bridge.js'),Contract=require('../../src/js/adapters/native-subselect-contract.js');
const Command=require('../../src/js/adapters/native-object-vector-command.js'),Paper=require('./native-import-paper.cjs');
function deferred(){let resolve;const promise=new Promise(r=>resolve=r);return {promise,resolve};}
function fixture(){
  const paper=Paper.load(),rect={left:0,top:0,right:200,bottom:200,width:200,height:200},stage={getBoundingClientRect:()=>rect,setPointerCapture(){},releasePointerCapture(){}};
  const dom=()=>({style:{},setAttribute(){},appendChild(){},remove(){}}),notices=[],sent=[],previews=[];
  let pin={instanceId:'i',documentId:'d',lifecycleGeneration:1,contentRevision:0,documentSnapshotId:'native-object:d:0',canvasW:200,canvasH:200,layers:[{layerUid:'L'}]};
  let presentation={...pin,owner:'native',status:'presented',contextId:'scene-root',frame:0,workId:'native-work-1',viewGeneration:1};
  let queued=false,busy=false,gate=null,readGate=null,transportFailures=0,reconcileFailure=false;
  const item={stableTarget:{contextId:'scene-root',layerUid:'L',strokeId:'s',frameScope:{kind:'authored',frame:0}},closed:true,count:4,matrix:[1.5,.2,.5,1,0,0],segments:[[20,20],[80,20],[80,80],[20,80]].map(point=>({point,handleIn:[0,0],handleOut:[0,0]}))};
  let items=[item];
  const shown=()=>({...pin,presentation}),s={info:()=>pin,isActive:()=>true,blocksLegacy:()=>true,requestId:()=>`q-${sent.length}`,readPresentedObjectSession:why=>{if(queued||busy){if(why)why('queue-pending');return null;}return shown();},
    readSubselect:async()=>{const data={info:pin,presentation,items};if(readGate)await readGate.promise;return data;},
    previewSubselect:async(selector,target,intent)=>{previews.push(intent);return {info:pin,presentation,segments:item.segments,matrix:item.matrix};},
    runResourceCommand:async action=>{const origin=busy?null:shown();/* the Project records the presentation a command is SUBMITTED under: none while the queue is busy */queued=true;try{if(gate)await gate.promise;return await action({readPresentedObjectSession:()=>origin,
      dispatch:async q=>{sent.push(q);if(transportFailures>0){transportFailures--;throw Error('Transport lost');}return {apiVersion:2,requestId:q.requestId,instanceId:q.instanceId,documentId:q.documentId,contentRevision:q.expectedRevision+1,ok:true,result:{applied:true,historyEntriesAdded:1}};},
      reconcile:async revision=>{if(reconcileFailure)throw Error('Owner read unavailable');pin={...pin,contentRevision:revision,documentSnapshotId:`native-object:d:${revision}`};presentation={...presentation,...pin};}});}finally{queued=false;}}};
  const root={paper,setTimeout:fn=>setImmediate(fn),NemoNativePresentationWait:require('../../src/js/adapters/native-presentation-wait.js'),state:{tool:'subselect',appMode:'anim2d',currentFrame:0,activeLayerIdx:0,layers:[{layerUid:'L'}]},_layerSel:[0],showToast:m=>notices.push(m),
    document:{getElementById:()=>stage,createElementNS:dom,body:dom(),documentElement:{clientWidth:200,clientHeight:200,classList:{contains:()=>true},style:{getPropertyValue:k=>String(k.endsWith('right')||k.endsWith('bottom')?200:0)}}},
    NemoNativeObjectProject:{desktop:()=>s},NemoNativeObjectProjectFile:{projectionReadyForUI:()=>true},NemoNativeObjectVectorCommand:Command,NemoNativeSubselectContract:Contract,
    NemoNativeViewportUnderlay:{canvasMapping:{point:e=>({x:e.clientX,y:e.clientY}),transform:()=>[1,0,0,1,0,0],matchesView:()=>!!s.readPresentedObjectSession()}}};
  const controller=Bridge.create(root),event=(x,y,extra={})=>({clientX:x,clientY:y,pointerId:1,target:stage,preventDefault(){},stopImmediatePropagation(){},...extra});
  async function select(){controller.handle('down',event(100,60));await controller.settled();assert.equal(paper.project.activeLayer.children.length,0);}
  function drag(){controller.handle('down',event(40,24));controller.handle('move',event(52,34));}
  return {root,s,controller,event,setBusy:v=>busy=v,select,drag,sent,previews,notices,item,paper,setFrame:n=>{root.state.currentFrame=n;presentation={...presentation,frame:n};},setItems:v=>items=v,setGate:g=>gate=g,failTransport:count=>{transportFailures=count;reconcileFailure=true;},allowRecovery:()=>{reconcileFailure=false;},setReadGate:g=>readGate=g,replace:()=>{pin={...pin,documentId:'replacement',documentSnapshotId:'native-object:replacement:0'};presentation={...presentation,...pin};}};
}
module.exports={fixture,deferred};
