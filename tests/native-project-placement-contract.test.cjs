'use strict';
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const {validateRequest,validateResponse,createNativeApplicationAdapter}=require('../src/js/adapters/native-application.js');
const op='command.document.object.project.placement.edit';
function request(payload,revision=7){return {apiVersion:2,requestId:'placement',instanceId:'instance',documentId:'document',expectedRevision:revision,operation:op,payload};}
function response(applied=true){return {apiVersion:2,requestId:'placement',instanceId:'instance',documentId:'document',contentRevision:applied?8:7,ok:true,result:{applied,historyEntriesAdded:applied?1:0}};}
function place(target={kind:'drawing',contextId:'scene-root',layerUid:'L'}){return {kind:'place',placement:{id:'p1',assetId:'v1',target,offsetFrames:0}};}

test('original placement DTO accepts four edits and preserves opaque IDs without resource payload admission',()=>{
  for(const payload of [place(),place({kind:'drawing',contextId:'scene-root',layerUid:'L',strokeId:'item'}),place({kind:'audioTrack',trackUid:'track'}),
    {kind:'setOffset',placementId:'p1',offsetFrames:-Number.MAX_SAFE_INTEGER},{kind:'shiftOffset',placementId:'p1',deltaFrames:Number.MAX_SAFE_INTEGER},
    {kind:'setOffset',placementId:'p1',offsetFrames:-0.5},{kind:'shiftOffset',placementId:'p1',deltaFrames:0.25},
    {kind:'place',placement:{...place({kind:'audioTrack',trackUid:'track'}).placement,offsetFrames:-1.25}},
    {kind:'remove',placementId:'blob:opaque-source-id'},{kind:'remove',placementId:'\uFEFF'},
  ])assert.doesNotThrow(()=>validateRequest(request(payload)));
  for(const payload of [
    {kind:'setOffset',placementId:'p1',offsetFrames:null},{kind:'place',placement:{...place().placement,offsetFrames:0.5}},
    {kind:'shiftOffset',placementId:'p1',deltaFrames:Number.MAX_SAFE_INTEGER+1},
    {kind:'remove',placementId:'p1',extra:0},{kind:'remove',placementId:'\u0085'},
    {kind:'remove',placementId:'é'.repeat(128)},{kind:'remove',placementId:'p\0'},
    place({kind:'drawing',contextId:'scene-root',layerUid:'L',strokeId:null}),
    place({kind:'audioTrack',trackUid:'track',path:'/tmp/audio.wav'}),
    {kind:'place',placement:{...place().placement,data:'data:audio/wav;base64,AAAA'}},
  ])assert.throws(()=>validateRequest(request(payload)),TypeError);
  const missing=request({kind:'remove',placementId:'p'});delete missing.expectedRevision;
  assert.throws(()=>validateRequest(missing),TypeError);
  const huge=request({kind:'remove',placementId:'p'});huge.requestId='x'.repeat(4096);
  assert.throws(()=>validateRequest(huge),TypeError);
});

test('real shared adapter correlates placement owner and exact changed/no-op receipt revision on UI and MCP ports',async()=>{
  for(const label of ['ui','mcp'])for(const applied of [false,true]){
    const req=request({kind:'shiftOffset',placementId:'p1',deltaFrames:1}),good=response(applied);
    assert.deepEqual(await createNativeApplicationAdapter(label,{dispatch:()=>good}).dispatch(req),good);
    for(const wrong of [{...good,contentRevision:applied?7:8},{...good,instanceId:'foreign'},{...good,documentId:'foreign'},{...good,requestId:'foreign'},
      {...good,result:{applied,historyEntriesAdded:applied?0:1}},{...good,result:{...good.result,project:{}}}]){
      await assert.rejects(createNativeApplicationAdapter(label,{dispatch:()=>wrong}).dispatch(req),TypeError);
    }
    const refused={...good,ok:false,error:{code:'unavailable',message:'AudioTrack owner unavailable'}};delete refused.result;
    assert.deepEqual(await createNativeApplicationAdapter(label,{dispatch:()=>refused}).dispatch(req),refused);
    assert.doesNotThrow(()=>validateResponse(good,'placement',op));
  }
});

test('browser pure placement child composes without replacing the previous contract or publishing state',()=>{
  const context={TextEncoder};vm.createContext(context);
  vm.runInContext(fs.readFileSync(path.join(__dirname,'../src/js/adapters/native-project-placement-contract.js'),'utf8'),context);
  const api=context.NemoNativeProjectPlacementContract;
  assert.equal(api.OP,op);assert.ok(Object.isFrozen(api));assert.equal(api.admitted(op),true);
  const seen=[];const base={admitted:x=>x==='old',requiresRevision:x=>x==='old',payload:(...x)=>seen.push(['payload',...x]),result:(...x)=>seen.push(['result',...x]),correlate:(...x)=>seen.push(['correlate',...x]),paths:()=>null,requestByteLimit:()=>1234};
  const joined=api.compose(base);assert.equal(joined.admitted('old'),true);assert.equal(joined.requiresRevision(op),true);assert.equal(joined.requestByteLimit('old'),1234);
  joined.payload('old',{});joined.result('old',{});joined.correlate({operation:'old'},{});assert.deepEqual(seen.map(x=>x[0]),['payload','result','correlate']);
});

test('registered descriptor example uses the same shared request and response contract and bootstrap child ordering',()=>{
  const descriptor=JSON.parse(fs.readFileSync(path.join(__dirname,'../engineering/application/capabilities-v2/native-project-placement.json'),'utf8'));
  assert.equal(descriptor.id,'native.project-placement');assert.deepEqual(descriptor.operations,[op]);
  assert.equal(descriptor.authority.adapterOwnsWritableMirror,false);assert.equal(descriptor.authority.fallback,'none');
  assert.equal(descriptor.resourceBoundary.maxRequestBytes,4096);
  for(const sample of descriptor.examples){assert.doesNotThrow(()=>validateRequest(sample.request));assert.doesNotThrow(()=>validateResponse(sample.response,sample.request.requestId,op));}
  const html=fs.readFileSync(path.join(__dirname,'../src/index.html'),'utf8'),child='js/adapters/native-project-placement-contract.js',adapter='js/adapters/native-application.js';
  assert.equal(html.split('src="'+child+'"').length-1,1);assert.ok(html.indexOf(child)<html.indexOf(adapter));
});

test('T12 video timing edits: bounded remap intents and the footage interpolation mode',()=>{
  for(const payload of [
    {kind:'enableTimeRemap',placementId:'p1',inFrame:0,outFrame:24,lastSourceFrame:47},
    {kind:'enableTimeRemap',placementId:'p1',inFrame:-3.5,outFrame:2,lastSourceFrame:0},
    {kind:'removeTimeRemapKey',placementId:'p1',frame:24},{kind:'disableTimeRemap',placementId:'p1'},
    {kind:'setInterpMode',placementId:'p1',interpMode:'none'},{kind:'setInterpMode',placementId:'p1',interpMode:'motion'},
    {kind:'setInterpMode',placementId:'p1',interpMode:'m'.repeat(32)},
  ])assert.doesNotThrow(()=>validateRequest(request(payload)),JSON.stringify(payload));
  for(const payload of [
    {kind:'setTimeRemap',placementId:'p1',timeRemap:null},
    {kind:'enableTimeRemap',placementId:'p1',inFrame:0,outFrame:24},
    {kind:'enableTimeRemap',placementId:'p1',inFrame:'0',outFrame:24,lastSourceFrame:47},
    {kind:'enableTimeRemap',placementId:'p1',inFrame:0,outFrame:Infinity,lastSourceFrame:47},
    {kind:'enableTimeRemap',placementId:'p1',inFrame:0,outFrame:24,lastSourceFrame:47,keys:[]},
    {kind:'removeTimeRemapKey',placementId:'p1'},{kind:'removeTimeRemapKey',placementId:'p1',frame:NaN},
    {kind:'disableTimeRemap',placementId:'p1',frame:0},{kind:'disableTimeRemap'},
    {kind:'setInterpMode',placementId:'p1'},{kind:'setInterpMode',placementId:'p1',interpMode:''},
    {kind:'setInterpMode',placementId:'p1',interpMode:1},{kind:'setInterpMode',placementId:'p1',interpMode:'a\0'},
    {kind:'setInterpMode',placementId:'p1',interpMode:'m'.repeat(33)},
    {kind:'setInterpMode',placementId:'p1',interpMode:'blend',frameBlend:true},
  ])assert.throws(()=>validateRequest(request(payload)),TypeError,JSON.stringify(payload));
});
