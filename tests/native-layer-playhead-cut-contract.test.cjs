'use strict';
// Actual shared adapter and pure child contract; transport receipts are stubs,
// never a native owner/history or installed desktop acceptance claim.
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const Child=require('../src/js/adapters/native-layer-playhead-cut-contract.js');
const SDK=require('../src/js/adapters/native-application.js');
const OP='command.document.object.layer.cut';
function payload(){return {command:'object.layer.cut',contextId:'scene-root',layerUid:'a',frame:10};}
function request(value=payload()){return {apiVersion:2,requestId:'cut',instanceId:'instance',documentId:'document',expectedRevision:7,operation:OP,payload:value};}
function response(applied=true){return {apiVersion:2,requestId:'cut',instanceId:'instance',documentId:'document',contentRevision:applied?8:7,ok:true,result:applied?{applied:true,historyEntriesAdded:1,layerUid:'native-layer:8:0'}:{applied:false,historyEntriesAdded:0}};}

test('cut source DTO is exact and bounded while nonroot opaque identities still reach native refusal',()=>{
 assert.equal(Child.OP,OP);assert.ok(Object.isFrozen(Child));assert.equal(Child.admitted(OP),true);assert.equal(Child.requiresRevision(OP),true);
 for(const value of [payload(),{...payload(),contextId:'component context/α',layerUid:' layer / β ',frame:4294967295},
  {...payload(),contextId:'é'.repeat(64),layerUid:'x'.repeat(128),frame:0}]){
  assert.doesNotThrow(()=>Child.payload(OP,value));assert.doesNotThrow(()=>SDK.validateRequest(request(value)));
 }
 for(const key of ['command','contextId','layerUid','frame']){
  const missing=payload();delete missing[key];assert.throws(()=>SDK.validateRequest(request(missing)),TypeError);
  assert.throws(()=>SDK.validateRequest(request({...payload(),[key]:null})),TypeError);
 }
 for(const value of [{...payload(),extra:true},{...payload(),command:'object.layer.shift'},{...payload(),layerUid:''},
  {...payload(),contextId:''},{...payload(),layerUid:'é'.repeat(65)},{...payload(),frame:-1},{...payload(),frame:0.5},
  {...payload(),frame:4294967296},{...payload(),frame:'10'},[payload()]])assert.throws(()=>SDK.validateRequest(request(value)),TypeError);
 const missing=request();delete missing.expectedRevision;assert.throws(()=>SDK.validateRequest(missing),TypeError);
 assert.throws(()=>SDK.validateRequest({...request(),expectedRevision:null}),TypeError);
 assert.throws(()=>SDK.validateRequest({...request(),requestId:'x'.repeat(4096)}),TypeError);
 assert.equal(Child.paths(OP).has('payload.contextId'),true);assert.equal(Child.paths(OP).has('payload.layerUid'),true);
 assert.equal(Child.paths(OP).has('result.layerUid'),false,'new native ID retains transport identifier validation');
});

test('actual UI and MCP shared adapter correlate exact cut changed/noop receipts and preserve typed unavailability',async()=>{
 for(const label of ['ui','mcp'])for(const applied of [false,true]){
  const req=request(),good=response(applied),calls=[];
  const adapter=SDK.createNativeApplicationAdapter(label,{dispatch(value){calls.push(value);return good;}});
  assert.deepEqual(await adapter.dispatch(req),good);assert.equal(calls.length,1);assert.deepEqual(calls[0],req);assert.notStrictEqual(calls[0],req);
  for(const wrong of [{...good,requestId:'foreign'},{...good,instanceId:'foreign'},{...good,documentId:'foreign'},
   {...good,contentRevision:applied?7:8},{...good,result:{applied:true,historyEntriesAdded:1}},
   {...good,result:{applied:false,historyEntriesAdded:0,layerUid:'native-layer:8:0'}},
   {...good,result:{applied:true,historyEntriesAdded:1,layerUid:'bad uid'}},
   {...good,result:{...good.result,document:{}}}]){
   await assert.rejects(SDK.createNativeApplicationAdapter(label,{dispatch:()=>wrong}).dispatch(req),TypeError);
  }
  const refused={...good,ok:false,error:{code:'unavailable',message:'Nonroot source has no cut owner'}};delete refused.result;
  assert.deepEqual(await SDK.createNativeApplicationAdapter(label,{dispatch:()=>refused}).dispatch(req),refused);
  const replaced={...refused,documentId:'new-document',error:{code:'wrong_document',message:'Replaced owner'}};
  assert.deepEqual(await SDK.createNativeApplicationAdapter(label,{dispatch:()=>replaced}).dispatch(req),replaced);
  await assert.rejects(SDK.createNativeApplicationAdapter(label,{dispatch:()=>({...refused,documentId:'new-document'})}).dispatch(req),TypeError);
 }
});

test('browser cut child composes base methods and byte policy without owning state or replacing other operations',()=>{
 const context={TextEncoder};vm.createContext(context);
 vm.runInContext(fs.readFileSync(path.join(__dirname,'../src/js/adapters/native-layer-playhead-cut-contract.js'),'utf8'),context);
 const api=context.NemoNativeLayerPlayheadCutContract,seen=[];
 const base={admitted:op=>op==='old',requiresRevision:op=>op==='old',payload:(...args)=>seen.push(['payload',...args]),
  result:(...args)=>seen.push(['result',...args]),correlate:(...args)=>seen.push(['correlate',...args]),paths:()=>null,requestByteLimit:()=>1234};
 const joined=api.compose(base);assert.equal(joined.admitted('old'),true);assert.equal(joined.admitted(OP),true);
 assert.equal(joined.requiresRevision(OP),true);assert.equal(joined.requestByteLimit('old'),1234);
 joined.payload('old',{});joined.result('old',{});joined.correlate({operation:'old'},{});
 assert.deepEqual(seen.map(value=>value[0]),['payload','result','correlate']);assert.equal(base.admitted(OP),false);
 // Build plain objects in the child's own realm; cross-realm prototype identity
 // is not the operation's DTO semantics or an idempotence oracle.
 assert.doesNotThrow(()=>vm.runInContext(`NemoNativeLayerPlayheadCutContract.payload('${OP}',{command:'object.layer.cut',contextId:'scene-root',layerUid:'a',frame:10});NemoNativeLayerPlayheadCutContract.result('${OP}',{applied:true,historyEntriesAdded:1,layerUid:'native-layer:8:0'});`,context));
 assert.equal(Object.prototype.hasOwnProperty.call(context,'state'),false);
});

test('cut descriptor examples use shared DTO receipts and the real classic child loads before common adapter',()=>{
 const descriptor=JSON.parse(fs.readFileSync(path.join(__dirname,'../engineering/application/capabilities-v2/native-layer-playhead-cut.json'),'utf8'));
 assert.equal(descriptor.id,'native.layer-playhead-cut');assert.deepEqual(descriptor.operations,[OP]);
 assert.equal(descriptor.authority.adapterOwnsWritableMirror,false);assert.equal(descriptor.authority.fallback,'none');
 assert.equal(descriptor.resourceBoundary.maxRequestBytes,4096);assert.ok(descriptor.examples.length>=2);
 for(const example of descriptor.examples){assert.doesNotThrow(()=>SDK.validateRequest(example.request));assert.doesNotThrow(()=>SDK.validateResponse(example.response,example.request.requestId,OP));assert.doesNotThrow(()=>Child.correlate(example.request,example.response));}
 const html=fs.readFileSync(path.join(__dirname,'../src/index.html'),'utf8'),child='js/adapters/native-layer-playhead-cut-contract.js',adapter='js/adapters/native-application.js';
 assert.equal(html.split('src="'+child+'"').length-1,1);assert.ok(html.indexOf(child)<html.indexOf(adapter));
});
