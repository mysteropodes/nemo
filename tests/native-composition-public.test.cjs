'use strict';
const test=require('node:test'),assert=require('node:assert/strict');
const SDK=require('../src/js/adapters/native-application.js');
const C=require('../src/js/adapters/native-composition-contract.js');
const read=selector=>({apiVersion:2,requestId:'read-7',instanceId:'instance',documentId:'document',operation:C.READ,
  payload:{atRevision:7,documentSnapshotId:'snapshot-7',contextId:'scene-root',selector}});
const edit=action=>({apiVersion:2,requestId:'edit-7',instanceId:'instance',documentId:'document',expectedRevision:7,operation:C.EDIT,
  payload:{command:'object.project.composition.edit',action}});
function reply(q,value){return {apiVersion:2,requestId:q.requestId,instanceId:q.instanceId,documentId:q.documentId,
  contentRevision:q.operation===C.EDIT?q.expectedRevision+(value.applied?1:0):q.payload.atRevision,ok:true,result:value};}
function result(q,value){return {instanceId:q.instanceId,documentId:q.documentId,atRevision:q.payload.atRevision,
  documentSnapshotId:q.payload.documentSnapshotId,contextId:q.payload.contextId,result:value};}
const settings=()=>({markersPresent:false,markerCount:0,effectiveBpm:120,effectiveBpmOffset:0,effectiveBpmShow:false});
function client(respond){return SDK.createNativeApplicationAdapter('composition-ui',{dispatch:respond});}

test('real shared SDK admits composition read/edit through its existing injected transport',async()=>{
  const calls=[],sdk=client(q=>{calls.push(q);return q.operation===C.EDIT?reply(q,{applied:true,historyEntriesAdded:1}):reply(q,result(q,settings()));});
  const r=await sdk.dispatch(read({kind:'settings'}));assert.equal(r.result.result.effectiveBpm,120);
  const w=await sdk.dispatch(edit({kind:'setBpm',bpm:132,bpmShow:false}));assert.equal(w.contentRevision,8);
  assert.equal(calls.length,2);assert.equal(calls[0].operation,C.READ);assert.equal(calls[1].operation,C.EDIT);
});

test('shared SDK enforces expected revision presence and refuses closed-shape violations before transport',async()=>{
  let calls=0;const sdk=client(()=>{calls++;throw Error('must not dispatch');});
  const missing=edit({kind:'setBpm'});delete missing.expectedRevision;
  for(const q of [missing,{...read({kind:'settings'}),expectedRevision:7},edit({kind:'setCompositionMetadata',markers:[]}),
    edit({kind:'setBpm',bpm:null}),read({kind:'markerPage',cursor:0,limit:17})])await assert.rejects(async()=>sdk.dispatch(q));
  assert.equal(calls,0);
});

test('shared SDK rejects uncorrelated native revision snapshot and selector replies',async()=>{
  for(const mutate of [r=>{r.contentRevision=8;},r=>{r.result.documentSnapshotId='other';},r=>{r.result.instanceId='other';},
    r=>{r.result.result={kind:'markerPage',markersPresent:false,markerCount:0,cursor:0,items:[],nextCursor:null};}]) {
    const sdk=client(q=>{const r=reply(q,result(q,settings()));mutate(r);return r;});await assert.rejects(sdk.dispatch(read({kind:'settings'})));
  }
  const wrongEdit=client(q=>({...reply(q,{applied:true,historyEntriesAdded:1}),contentRevision:7}));
  await assert.rejects(wrongEdit.dispatch(edit({kind:'setBpm',bpm:132})));
});

test('shared SDK passes typed wrong-document refusal and refuses other foreign-owner replies',async()=>{
  const refusal=(q,code)=>({apiVersion:2,requestId:q.requestId,instanceId:q.instanceId,documentId:'replacement',contentRevision:9,ok:false,error:{code,message:'Owner replaced'}});
  const sdk=client(q=>refusal(q,'wrong_document'));const r=await sdk.dispatch(read({kind:'settings'}));assert.equal(r.ok,false);assert.equal(r.error.code,'wrong_document');
  const wrong=client(q=>refusal(q,'unavailable'));await assert.rejects(wrong.dispatch(read({kind:'settings'})));
});

test('shared SDK applies the complete encoded 4096-byte request and response budgets',async()=>{
  let calls=0;const sdk=client(q=>{calls++;return reply(q,{applied:true,historyEntriesAdded:1});});
  await assert.rejects(async()=>sdk.dispatch(edit({kind:'addMarker',frame:1,name:'\u0001'.repeat(1024)})));assert.equal(calls,0);
  const oversized=client(q=>reply(q,result(q,{kind:'markerPage',markersPresent:true,markerCount:7,cursor:0,
    items:Array.from({length:7},(_,i)=>({ordinal:i+1,frame:i,name:'é'.repeat(512)})),nextCursor:null})));
  await assert.rejects(oversized.dispatch(read({kind:'markerPage',cursor:0,limit:16})));
});

test('shared SDK retains original settings actions and opaque authored marker names',async()=>{
  const old={...edit({kind:'setBpm'}),operation:'command.document.object.project.settings.edit',payload:{command:'object.project.settings.edit',action:{kind:'setFps',fps:29.97}}};
  assert.doesNotThrow(()=>SDK.validateRequest(old));
  assert.throws(()=>SDK.validateRequest({...old,payload:{...old.payload,action:{kind:'setBpm',bpm:132}}}));
  const sdk=client(q=>reply(q,result(q,{index:1,frame:2,time:2/24,name:'data:chapter',color:null})));
  const r=await sdk.dispatch(read({kind:'markerNamed',name:'data:chapter'}));assert.equal(r.result.result.name,'data:chapter');
  const mismatched=client(q=>reply(q,result(q,{index:1,frame:2,time:2/24,name:'other',color:null})));
  await assert.rejects(mismatched.dispatch(read({kind:'markerNamed',name:'data:chapter'})));
});
