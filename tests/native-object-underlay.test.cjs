'use strict';
const test=require('node:test'),assert=require('node:assert/strict');
const Project=require('../src/js/adapters/native-object-project.js'),Underlay=require('../src/js/adapters/native-viewport-underlay.js');
const turn=()=>new Promise(setImmediate);
function gate(){let resolve;const promise=new Promise(r=>{resolve=r;});return {promise,resolve};}
function fixture(withSubscription=false){
 const calls=[],events=[],classes=new Set(),styles=new Map();let owner=null,generation=0,rect={x:20,y:30,width:320,height:180},lastBounds=null,previewReply=null,mappingFailure=false,bootstrapFailure=null,finallyCalls=0;
 const document={documentElement:{clientWidth:1200,clientHeight:900,classList:{contains:k=>classes.has(k),add:k=>classes.add(k),remove:k=>classes.delete(k)},style:{getPropertyValue:k=>styles.get(k)||'',getPropertyPriority:()=>'',setProperty:(k,v)=>styles.set(k,v),removeProperty:k=>styles.delete(k)}}};
 const underlay=Underlay.create({document,measure:()=>({left:lastBounds.x,top:lastBounds.y,width:lastBounds.width,height:lastBounds.height})});
 const info=()=>({apiVersion:2,instanceId:'instance',documentId:owner.documentId,lifecycleGeneration:owner.generation,documentSnapshotId:'snapshot-'+owner.generation,contentRevision:0,canvasW:320,canvasH:180,fps:24,totalFrames:21,documentDigest:'a'.repeat(64),byteCount:10000,layers:[{layerUid:'drawing'}],reference:null,undoDepth:0,redoDepth:0});
 async function invoke(command,args){const r=args&&JSON.parse(args.requestJson);calls.push({command,r});
  if(command==='nemo_native_object_client_status')return {apiVersion:2,instanceId:'instance',available:!!owner,...(owner?info():{})};
  if(command==='nemo_native_object_bootstrap'){if(bootstrapFailure)throw bootstrapFailure;owner={documentId:'document-'+(++generation),generation};return {...info(),requestId:r.requestId,resourceCount:0,viewportAvailable:false};}
  if(command==='nemo_native_object_reference')return {...info(),requestId:r.requestId};
  if(command==='nemo_native_object_preview'){if(previewReply)return previewReply(r);return {status:'presented',workId:'work-'+calls.length,viewGeneration:1};}
  throw Error('Unexpected mock command');
 }
 let subscriptionError;
 const session=Project.create({...(withSubscription?{projectionReady:()=>true,publishRevision:()=>true,subscribeRevisions:async(onEvent,onError)=>{subscriptionError=onError;return {...info(),subscriptionId:'subscription-1'};}}:{}),invoke,frame:()=>0,mapping:()=>{if(mappingFailure)throw Error('Mapping unavailable');return {cssBounds:rect,physicalExtent:{width:320,height:180},compositionExtent:{width:320,height:180},reportedDpr:1};},
  surfaceChanged(value,bounds){events.push({value,bounds});if(value){lastBounds=bounds;underlay.present(value);}else underlay.clear();},presented(){finallyCalls++;}});
 return {session,calls,events,classes,styles,underlay,fence(){subscriptionError(Error('Lost native subscription'));},get finallyCalls(){return finallyCalls;},set rect(v){rect=v;},set preview(v){previewReply=v;},set mappingFailure(v){mappingFailure=v;},set bootstrapFailure(v){bootstrapFailure=v;},get previews(){return calls.filter(x=>x.command==='nemo_native_object_preview');}};
}
const active=f=>f.classes.has('nemo-native-underlay');
async function opened(){const f=fixture();await f.session.openPath('/a.json');assert.ok(active(f));return f;}
test('initial construction invokes no surface callback and remains opaque',()=>{const f=fixture();assert.equal(f.events.length,0);assert.equal(active(f),false);});
test('successful preview publishes correlated owner and exact latched request cssBounds',async()=>{
 const f=await opened(),g=gate();f.preview=()=>g.promise;f.rect={x:50,y:70,width:640,height:360};const pending=f.session.presentPreview(1);await turn();const sent=f.previews.at(-1).r;
 f.rect={x:90,y:100,width:100,height:80};g.resolve({status:'presented',workId:'later-work',viewGeneration:2});await pending;
 const e=f.events.at(-1);assert.deepEqual(e.bounds,sent.viewport.cssBounds);assert.deepEqual(e.bounds,{x:50,y:70,width:640,height:360});
 for(const k of ['instanceId','documentId','lifecycleGeneration','contentRevision','documentSnapshotId'])assert.equal(e.value[k],sent[k]);assert.equal(e.value.frame,1);assert.equal(e.value.workId,'later-work');assert.ok(active(f));
});
for(const status of ['failed-device-lost','deferred-occluded','stale-discarded'])test(status+' clears actual surface despite enqueue final callback',async()=>{const f=await opened(),before=f.finallyCalls;f.preview=()=>({status});await assert.rejects(f.session.presentPreview(1));assert.equal(active(f),false);assert.equal(f.events.at(-1).value,null);assert.equal(f.finallyCalls,before+1);});
test('raw preview rejection clears actual surface',async()=>{const f=await opened();f.preview=()=>Promise.reject(Error('Rejected mock'));await assert.rejects(f.session.presentPreview(1));assert.equal(active(f),false);});
test('replacement opening clears; cancelled confirmation restores prior receipt/bounds without preview',async()=>{
 const f=await opened(),before=f.previews.length,old=f.events.at(-1),g=gate();const pending=f.session.openPath('/b.json',{confirm:()=>g.promise});await turn();assert.equal(active(f),false);g.resolve(false);assert.equal(await pending,false);assert.equal(f.previews.length,before);assert.deepEqual(f.events.at(-1),old);assert.ok(active(f));
});
test('replacement notDispatched refusal restores previous surface without extra native preview',async()=>{const f=await opened(),before=f.previews.length,old=f.events.at(-1);f.bootstrapFailure={code:'invalid_request',details:{disposition:'notDispatched'}};await assert.rejects(f.session.openPath('/bad.json'));assert.equal(f.previews.length,before);assert.deepEqual(f.events.at(-1),old);assert.ok(active(f));});
test('post-dispatch uncertainty fences and clears instead of reviving old surface',async()=>{const f=await opened();f.bootstrapFailure=Error('Unknown transport effect');await assert.rejects(f.session.openPath('/unknown.json'));assert.equal(f.session.isIndeterminate(),true);assert.equal(active(f),false);assert.equal(f.events.at(-1).value,null);});
test('mapping failure clears an already presented surface',async()=>{const f=await opened();f.mappingFailure=true;await assert.rejects(f.session.presentPreview(1));assert.equal(active(f),false);});
test('hostile receipt accessor failure clears an already presented surface',async()=>{const f=await opened();f.preview=()=>Object.defineProperty({},'status',{get(){throw Error('Malformed mock receipt');}});await assert.rejects(f.session.presentPreview(1));assert.equal(active(f),false);});
test('late successful preview after same-session subscription fence cannot reactivate surface',async()=>{
 const f=fixture(true);await f.session.openPath('/a.json');const g=gate();f.preview=()=>g.promise;const pending=f.session.presentPreview(1);await turn();
 f.fence();assert.equal(f.session.isIndeterminate(),true);assert.equal(active(f),false);g.resolve({status:'presented',workId:'late',viewGeneration:2});await assert.rejects(pending);assert.equal(active(f),false);
});
