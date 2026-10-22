'use strict';
// Linked-video link contract (T12 point 8) through the actual contract module and the full SDK
// (admitted through the placement-contract seam). A controlled transport boundary: no native or installed proof.
const test=require('node:test'),assert=require('node:assert/strict');
const Contract=require('../src/js/adapters/native-video-asset-contract.js');
const SDK=require('../src/js/adapters/native-application.js');
const LINK='command.document.object.asset.video.link';
function payload(intent={kind:'newLayer'}){return {documentSnapshotId:'native-object:document:0',lifecycleGeneration:1,source:{kind:'file',path:'/footage/Take A.mov'},intent};}
function request(p=payload()){return {apiVersion:2,requestId:'video-1',instanceId:'instance',documentId:'document',expectedRevision:0,operation:LINK,payload:p};}
function response(q){const next=q.expectedRevision+1,newLayer=q.payload.intent.kind==='newLayer';return {
 apiVersion:2,requestId:q.requestId,instanceId:q.instanceId,documentId:q.documentId,contentRevision:next,ok:true,
 result:{applied:true,historyEntriesAdded:1,assetId:'asset-'+q.requestId,placementId:newLayer?'placement-'+q.requestId:q.payload.intent.placementId,
  target:{contextId:'scene-root',layerUid:newLayer?'native-layer:'+next+':0':'V'},newLayer,sourceWidth:32,sourceHeight:16,sourceFrameCount:20}};}
function ownerRefusal(q,code,instanceId,documentId){return {apiVersion:2,requestId:q.requestId,instanceId,documentId,contentRevision:q.expectedRevision,ok:false,error:{code,message:'Native video link refused.'}};}

test('the operation is the T12 link op and the SDK admits it through the composed placement contract',async()=>{
 assert.equal(Contract.LINK,LINK);assert.equal(Contract.admitted(LINK),true);assert.equal(Contract.admitted('command.document.object.asset.attach'),false);
 assert.equal(Contract.requiresRevision(LINK),true);
 for(const intent of [{kind:'newLayer'},{kind:'replace',placementId:'p1'}]){
  const q=request(payload(intent));assert.doesNotThrow(()=>SDK.validateRequest(q));
  const r=await SDK.createNativeApplicationAdapter('ui',{dispatch:async()=>response(q)}).dispatch(q);
  assert.equal(r.result.newLayer,intent.kind==='newLayer');assert.equal(r.result.sourceFrameCount,20);
 }
 assert.ok(Contract.paths(LINK).has('payload.source.path'));assert.equal(Contract.paths('other').size,0);
});
test('closed payload: exact members, file source, bounded path and two intents only',()=>{
 assert.doesNotThrow(()=>Contract.payload(LINK,payload()));assert.doesNotThrow(()=>Contract.payload(LINK,payload({kind:'replace',placementId:'placement-video-1'})));
 const long='/'+'a'.repeat(2047);assert.doesNotThrow(()=>Contract.payload(LINK,{...payload(),source:{kind:'file',path:long}}));
 for(const mutate of [p=>delete p.lifecycleGeneration,p=>p.lifecycleGeneration=0,p=>p.lifecycleGeneration=1.5,p=>p.documentSnapshotId='',p=>p.documentSnapshotId='x'.repeat(129),
  p=>p.extra=1,p=>p.source.bytes='AAAA',p=>p.source.kind='url',p=>p.source.path='',p=>p.source.path='/a\u0000b.mov',p=>p.source.path='/'+'a'.repeat(2048),
  p=>p.intent={kind:'replace'},p=>p.intent={kind:'replace',placementId:'  '},p=>p.intent={kind:'replace',placementId:'x'.repeat(256)},
  p=>p.intent={kind:'newLayer',placementId:'p1'},p=>p.intent={kind:'authoredFrame'},p=>p.intent=null,p=>p.source=null]){
  const p=payload();mutate(p);assert.throws(()=>Contract.payload(LINK,p),JSON.stringify(p).slice(0,120));assert.throws(()=>SDK.validateRequest(request(p)));
 }
 assert.throws(()=>Contract.payload('command.document.object.asset.attach',payload()));
});
test('closed result: applied once, positive source facts, a bounded target context and no extra members',()=>{
 const q=request();assert.doesNotThrow(()=>Contract.result(LINK,response(q).result));
 assert.doesNotThrow(()=>Contract.result(LINK,{...response(q).result,target:{contextId:'sym_comp',layerUid:'V'}}));
 for(const mutate of [r=>r.applied=false,r=>r.historyEntriesAdded=0,r=>delete r.sourceFrameCount,r=>r.sourceWidth=0,r=>r.sourceHeight=-1,r=>r.sourceFrameCount=1.5,
  r=>r.extra=true,r=>r.target.contextId='',r=>r.target.contextId='  ',r=>r.target.contextId='c'.repeat(256),r=>r.target.contextId='a\u0000b',r=>r.target.contextId=7,r=>r.target.strokeId='s',r=>r.newLayer='yes',r=>r.assetId='',r=>r.placementId=null]){
  const r=response(q).result;mutate(r);assert.throws(()=>Contract.result(LINK,r),JSON.stringify(r));
 }
});
test('correlation refuses forged revision, intent disposition, minted ids, foreign request and foreign owner',async()=>{
 const fresh=request(),replace=request(payload({kind:'replace',placementId:'p1'}));
 assert.doesNotThrow(()=>Contract.correlate(fresh,response(fresh)));assert.doesNotThrow(()=>Contract.correlate(replace,response(replace)));
 for(const [q,mutate] of [[fresh,r=>r.contentRevision=2],[fresh,r=>r.result.newLayer=false],[fresh,r=>r.result.placementId='placement-other'],
  [fresh,r=>r.result.target.layerUid='native-layer:9:0'],[replace,r=>r.result.placementId='p2'],[replace,r=>r.result.newLayer=true],
  [fresh,r=>r.requestId='foreign'],[fresh,r=>r.documentId='foreign'],[fresh,r=>r.instanceId='foreign'],[fresh,r=>r.result.target.contextId='symbol']]){
  const r=response(q);mutate(r);assert.throws(()=>Contract.correlate(q,r));
  await assert.rejects(SDK.createNativeApplicationAdapter('ui',{dispatch:async()=>r}).dispatch(q));
 }
 // A response to another operation never correlates with this contract.
 assert.throws(()=>Contract.correlate({...fresh,operation:'command.document.object.asset.attach'},response(fresh)));
});
test('only actual-owner WrongInstance/WrongDocument refusals may name another owner',async()=>{
 const q=request();
 for(const [code,instance,document] of [['wrong_instance','actual-instance','actual-document'],['wrong_document',q.instanceId,'actual-document']]){
  assert.doesNotThrow(()=>Contract.correlate(q,ownerRefusal(q,code,instance,document)));
  assert.equal((await SDK.createNativeApplicationAdapter('ui',{dispatch:async()=>ownerRefusal(q,code,instance,document)}).dispatch(q)).error.code,code);
 }
 for(const answer of [ownerRefusal(q,'wrong_document','actual-instance','actual-document'),ownerRefusal(q,'wrong_instance',q.instanceId,'actual-document'),
  ownerRefusal(q,'stale_revision','actual-instance','actual-document'),{...response(q),instanceId:'actual-instance'}]){
  assert.throws(()=>Contract.correlate(q,answer));await assert.rejects(SDK.createNativeApplicationAdapter('ui',{dispatch:async()=>answer}).dispatch(q));
 }
});
test('composition keeps another contract reachable and the video op first',()=>{
 const base={admitted:op=>op==='other',requiresRevision:()=>false,payload:()=>'base',result:()=>'base',correlate:()=>'base',paths:()=>new Set(['x'])};
 const joined=Contract.compose(base);assert.equal(joined.admitted('other'),true);assert.equal(joined.admitted(LINK),true);
 assert.equal(joined.payload('other',{}),'base');assert.throws(()=>joined.payload(LINK,{}));assert.equal(joined.requiresRevision('other'),false);
});
test('a replace inside a component correlates with its own context; a new layer must land in the scene root',async()=>{
 const replace=request(payload({kind:'replace',placementId:'p1'})),fresh=request();
 const inside=response(replace);inside.result.target.contextId='sym_comp';
 assert.doesNotThrow(()=>Contract.correlate(replace,inside));
 assert.equal((await SDK.createNativeApplicationAdapter('ui',{dispatch:async()=>inside}).dispatch(replace)).result.target.contextId,'sym_comp');
 const moved=response(fresh);moved.result.target.contextId='sym_comp';
 assert.throws(()=>Contract.correlate(fresh,moved),/video layer target mismatch/);
});
test('the advertised path bound fits the 4096-byte request with every other field at its maximum',()=>{
 const id=n=>'a'.repeat(n),at=n=>({apiVersion:2,requestId:id(128),instanceId:id(128),documentId:id(128),expectedRevision:Number.MAX_SAFE_INTEGER,operation:LINK,
  payload:{documentSnapshotId:id(128),lifecycleGeneration:Number.MAX_SAFE_INTEGER,source:{kind:'file',path:'/'+id(n-1)},intent:{kind:'replace',placementId:id(255)}}});
 assert.equal(Contract.MAX_PATH_BYTES,2048);
 assert.ok(Buffer.byteLength(JSON.stringify(at(2048)))<=4096);
 assert.doesNotThrow(()=>SDK.validateRequest(at(2048)));
 assert.throws(()=>SDK.validateRequest(at(2049)));
});
