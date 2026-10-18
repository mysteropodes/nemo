'use strict';
// Actual client/command/SDK contracts with a controlled transport boundary. No native or installed proof.
const test=require('node:test'),assert=require('node:assert/strict');
const Raster=require('../src/js/adapters/native-object-raster-asset.js');
const Contract=require('../src/js/adapters/native-object-item-contract.js');
const SDK=require('../src/js/adapters/native-application.js');
function deferred(){let resolve;return {promise:new Promise(r=>{resolve=r;}),resolve:v=>resolve(v)};}
function payload(placement={kind:'newLayer'}){return {documentSnapshotId:'native-object:document:0',lifecycleGeneration:1,name:'still.png',mediaType:'image/png',source:{kind:'file',path:'/admitted/still.png'},placement};}
function request(p=payload()){return {apiVersion:2,requestId:'image-one',instanceId:'instance',documentId:'document',expectedRevision:0,operation:Contract.ATTACH,payload:p};}
function response(q){const next=q.expectedRevision+1,newLayer=q.payload.placement.kind==='newLayer';return {
 apiVersion:2,requestId:q.requestId,instanceId:q.instanceId,documentId:q.documentId,contentRevision:next,ok:true,
 result:{applied:true,historyEntriesAdded:1,assetId:'canonical-existing-asset',naturalWidth:640,naturalHeight:480,displayWidth:240,displayHeight:180,
  target:{contextId:'scene-root',layerUid:newLayer?'native-layer:'+next+':0':q.payload.placement.layerUid,
   frameScope:{kind:'authored',frame:newLayer?0:q.payload.placement.frame},strokeId:'native-raster:'+next+':0'},newLayer}};}
function fixture(options={}){
 let pin={instanceId:'instance',documentId:'document',lifecycleGeneration:1,contentRevision:0,documentSnapshotId:'native-object:document:0'},serial=0,queue=Promise.resolve();
 const calls=[];const seen=new Map();let first=true;
 const context={dispatch:async q=>{
  calls.push(structuredClone(q));let answer=seen.get(q.requestId);
  if(!answer){answer=options.respond?options.respond(q):response(q);seen.set(q.requestId,structuredClone(answer));
   if(answer.ok){pin={...pin,contentRevision:answer.contentRevision,documentSnapshotId:'native-object:document:'+answer.contentRevision};}}
  if(options.lostAck&&first){first=false;throw Error('lost ACK');}return structuredClone(answer);
 },reconcile:async revision=>{if(revision!==undefined)assert.equal(revision,pin.contentRevision);return pin;}};
 const session={isActive:()=>true,info:()=>pin,requestId:()=> 'image-'+(++serial),runResourceCommand:action=>{
  const pending=queue.then(()=>action(context));queue=pending.catch(()=>{});return pending;
 }};
 const client=Raster.create(session,{imageRoute:options.route===undefined?true:options.route,pickImage:options.pick});
 return {client,session,calls,change:key=>{pin={...pin,...key};}};
}
test('full SDK dispatch and actual command client accept only correlated PNG placement results',async()=>{
 for(const placement of [{kind:'newLayer'},{kind:'authoredFrame',layerUid:'legacy layer:alpha',frame:3}]){
  const q=request(payload(placement));SDK.validateRequest(q);
  const port=SDK.createNativeApplicationAdapter('ui',{dispatch:async()=>response(q)});assert.equal((await port.dispatch(q)).result.assetId,'canonical-existing-asset');
  const f=fixture();const receipt=await f.client.importStillPath('/admitted/still.png',placement);
  assert.equal(receipt.assetId,'canonical-existing-asset');assert.equal(f.calls.length,1);assert.deepEqual(f.calls[0].payload.placement,placement);
  assert.equal(f.calls[0].payload.lifecycleGeneration,1);assert.equal(f.calls[0].payload.name,'still.png');
 }
});
test('image route never enables catalog-only attach or remove and refuses before picker when absent',async()=>{
 const f=fixture({route:false,pick:()=>assert.fail('unavailable route must not pick')});
 await assert.rejects(f.client.importStillPicked({kind:'newLayer'}),e=>e.code==='unavailable');assert.equal(f.calls.length,0);
 const image=fixture();await assert.rejects(image.client.attachPath('/x.png'),e=>e.code==='unavailable');await assert.rejects(image.client.remove('asset-a'),e=>e.code==='unavailable');assert.equal(image.calls.length,0);
});
test('unsupported codec and opaque DOM File never enter a native command or JavaScript byte reader',async()=>{
 const f=fixture();const opaque={get path(){assert.fail('never infer File.path');},arrayBuffer(){assert.fail('no JS byte read');}};
 await assert.rejects(f.client.importStillPath(opaque,{kind:'newLayer'}));await assert.rejects(f.client.importStillPath('/still.jpg',{kind:'newLayer'}),e=>e.code==='unavailable');assert.equal(f.calls.length,0);
});
test('picker origin is captured before await and checked again inside the actual serialized command',async()=>{
 const gate=deferred(),f=fixture({pick:()=>gate.promise});const pending=f.client.importStillPicked({kind:'newLayer'});
 f.change({lifecycleGeneration:2});gate.resolve('/picked.png');await assert.rejects(pending,e=>e.code==='stale_revision');assert.equal(f.calls.length,0);
 const stopped=fixture();await assert.rejects(stopped.client.importStillPath('/picked.png',{kind:'newLayer'},{current:()=>false}),e=>e.code==='stale_revision');assert.equal(stopped.calls.length,0);
});
test('cancelled real picker produces no command; native path and original name survive unchanged',async()=>{
 const cancelled=fixture({pick:async()=>null});assert.equal(await cancelled.client.importStillPicked({kind:'newLayer'}),null);assert.equal(cancelled.calls.length,0);
 const f=fixture({pick:async()=>'/picked/My Still.PNG'});await f.client.importStillPicked({kind:'newLayer'});
 assert.equal(f.calls[0].payload.source.path,'/picked/My Still.PNG');assert.equal(f.calls[0].payload.name,'My Still.PNG');
});
test('transport ACK loss retries the exact full source/placement body and consumes the canonical owner result',async()=>{
 const f=fixture({lostAck:true});const receipt=await f.client.importStillPath('/picked.png',{kind:'newLayer'});
 assert.equal(receipt.assetId,'canonical-existing-asset');assert.equal(f.calls.length,2);assert.deepEqual(f.calls[0],f.calls[1]);
});
test('closed payload rejects missing lifecycle, extra source bytes, unknown placement and unsupported codec',()=>{
 for(const mutate of [p=>delete p.lifecycleGeneration,p=>p.source.bytes='page bytes',p=>p.placement.foo=1,p=>p.mediaType='image/jpeg',p=>p.placement={kind:'authoredFrame',layerUid:'drawing',frame:-1}]){
  const p=payload();mutate(p);assert.throws(()=>Contract.payload(Contract.ATTACH,p));assert.throws(()=>SDK.validateRequest(request(p)));
 }
 const p=payload();delete p.placement;delete p.lifecycleGeneration;assert.doesNotThrow(()=>Contract.payload(Contract.ATTACH,p),'catalog-only compatibility validation is retained');
});
test('SDK rejects forged owner/revision/canonical placement disposition and incomplete dimensions',async()=>{
 for(const mutate of [r=>r.result={applied:true,historyEntriesAdded:1},r=>r.result.newLayer=false,r=>r.result.target.frameScope.frame=9,
  r=>r.result.target.strokeId='native-raster:99:0',r=>r.result.target.layerUid='foreign-layer',r=>r.result.target.frameScope.kind='reference',
  r=>r.result.naturalWidth=0,r=>r.result.displayWidth=9999,r=>r.contentRevision=2,r=>r.documentId='foreign']){
  const q=request(),r=response(q);mutate(r);const port=SDK.createNativeApplicationAdapter('ui',{dispatch:async()=>r});
  await assert.rejects(async()=>port.dispatch(q));
 }
});
test('a catalog request cannot accept the richer placement result by coincidence',()=>{
 const q=request();delete q.payload.placement;delete q.payload.lifecycleGeneration;const r=response(request());assert.throws(()=>Contract.correlate(q,r));
});

function ownerRefusal(q,code,instanceId=q.instanceId,documentId=q.documentId){return {apiVersion:2,requestId:q.requestId,instanceId,documentId,contentRevision:q.expectedRevision,ok:false,error:{code,message:'Native image owner refusal'}};}
test('current full SDK preserves only declared placed-PNG actual-owner WrongInstance/WrongDocument refusals',async()=>{
 const q=request();
 for(const [code,instance,document]of [['wrong_instance','actual-instance','actual-document'],['wrong_instance','actual-instance',q.documentId],['wrong_document',q.instanceId,'actual-document'],['unavailable',q.instanceId,q.documentId]]){
  let calls=0;const answer=ownerRefusal(q,code,instance,document);const port=SDK.createNativeApplicationAdapter('png-owner',{dispatch:async()=>{calls++;return answer;}});
  assert.equal((await port.dispatch(q)).error.code,code);assert.equal(calls,1);
 }
});
test('PNG refusal exception never accepts another request, foreign success, unrelated errors or WrongDocument foreign instance',async()=>{
 const q=request();
 for(const answer of [ownerRefusal(q,'stale_revision','actual-instance','actual-document'),ownerRefusal(q,'unavailable',q.instanceId,'actual-document'),
  ownerRefusal(q,'wrong_document','actual-instance','actual-document'),ownerRefusal(q,'wrong_instance',q.instanceId,'actual-document'),
  {...ownerRefusal(q,'wrong_instance','actual-instance','actual-document'),requestId:'foreign'},
  {...response(q),instanceId:'actual-instance',documentId:'actual-document'}]){
  await assert.rejects(SDK.createNativeApplicationAdapter('png-owner',{dispatch:async()=>answer}).dispatch(q));
 }
});
test('catalog-only attach and remove never gain placed-PNG owner exceptions',async()=>{
 const catalog=request();delete catalog.payload.placement;delete catalog.payload.lifecycleGeneration;
 const remove={...request(),operation:Contract.REMOVE,payload:{documentSnapshotId:'native-object:document:0',assetId:'asset-a'}};
 for(const q of [catalog,remove])await assert.rejects(SDK.createNativeApplicationAdapter('png-owner',{dispatch:async()=>ownerRefusal(q,'wrong_instance','actual-instance','actual-document')}).dispatch(q),/owner mismatch/);
});
