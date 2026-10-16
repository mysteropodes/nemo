'use strict';
const test=require('node:test');const assert=require('node:assert/strict');
const {validateRequest,validateResponse,createNativeApplicationAdapter}=require('../src/js/adapters/native-application.js');
const op='command.document.object.project.settings.edit';
function request(action,revision=7){return {apiVersion:2,requestId:'settings',instanceId:'instance',documentId:'document',expectedRevision:revision,operation:op,payload:{command:'object.project.settings.edit',action}};}
function response(applied=true){return {apiVersion:2,requestId:'settings',instanceId:'instance',documentId:'document',contentRevision:applied?8:7,ok:true,result:{applied,historyEntriesAdded:applied?1:0}};}
test('F04 public adapter admits only exact owner actions and finite numeric command inputs',()=>{
  for(const action of [
    {kind:'setCanvasSize',width:1,height:4294967295},{kind:'setFps',fps:29.97},{kind:'setFps',fps:0.5},{kind:'setFps',fps:-5},
    {kind:'setCanvasBackground',color:'#aBc'},{kind:'setCanvasBackground',color:'AbC12380'},
    {kind:'setWorkArea',inFrame:20.5,outFrame:-3.25},
  ])assert.doesNotThrow(()=>validateRequest(request(action)));
  for(const action of [
    {kind:'setCanvasSize',width:0,height:1},{kind:'setCanvasSize',width:1.5,height:1},{kind:'setCanvasSize',width:1,height:4294967296},
    {kind:'setFps',fps:NaN},{kind:'setFps',fps:Infinity},{kind:'setFps',fps:'24'},
    {kind:'setCanvasBackground',color:null},{kind:'setCanvasBackground',color:'white'},{kind:'setCanvasBackground',color:'#abcd'},
    {kind:'setWorkArea',inFrame:2},{kind:'setWorkArea',inFrame:2,outFrame:null},{kind:'setWorkArea',inFrame:Infinity,outFrame:3},
    {kind:'setTotalFrames',totalFrames:99},{kind:'setFps',fps:24,path:'/private/file'},
  ])assert.throws(()=>validateRequest(request(action)),TypeError);
  const missing=request({kind:'setFps',fps:24});delete missing.expectedRevision;assert.throws(()=>validateRequest(missing),TypeError);
  const unknown=request({kind:'setFps',fps:24});unknown.payload.project={};assert.throws(()=>validateRequest(unknown),TypeError);
});
test('F04 receipt exposes only real applied/history result or typed refusal',()=>{
  for(const applied of [false,true])assert.doesNotThrow(()=>validateResponse(response(applied),'settings',op));
  for(const result of [{applied:true,historyEntriesAdded:0},{applied:false,historyEntriesAdded:1},{applied:true,historyEntriesAdded:1,project:{}}]){
    assert.throws(()=>validateResponse({...response(),result},'settings',op),TypeError);
  }
  const refused={...response(),ok:false,error:{code:'stale_revision',message:'Settings expected revision is stale.'}};delete refused.result;
  assert.doesNotThrow(()=>validateResponse(refused,'settings',op));
});
test('F04 shared adapter correlates identity and exact changed/no-op revision',async()=>{
  for(const applied of [false,true]){
    const r=request({kind:'setFps',fps:29.97});const good=response(applied);
    assert.deepEqual(await createNativeApplicationAdapter('ui',{dispatch:()=>good}).dispatch(r),good);
    for(const wrong of [{...good,contentRevision:applied?7:8},{...good,instanceId:'other'},{...good,documentId:'other'}]){
      await assert.rejects(createNativeApplicationAdapter('ui',{dispatch:()=>wrong}).dispatch(r),TypeError);
    }
  }
});
