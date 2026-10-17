'use strict';
const test=require('node:test'),assert=require('node:assert/strict'),path=require('node:path');
const root=process.env.NEMO_VECTOR_REVIEW_ROOT||path.resolve(__dirname,'..');
const SDK=require(path.join(root,'src/js/adapters/native-application.js'));
const C=require(path.join(root,'src/js/adapters/native-object-vector-contract.js'));
const descriptor=require(path.join(root,'engineering/application/capabilities-v2/native-object-vector.json'));
const create=()=>structuredClone(descriptor.examples.find(x=>x.request.payload.command==='object.vector.create').request);
const api=dispatch=>SDK.createNativeApplicationAdapter('vector-correlation',{dispatch});
const success=q=>({apiVersion:2,requestId:q.requestId,instanceId:q.instanceId,documentId:q.documentId,contentRevision:q.expectedRevision+1,ok:true,result:{applied:true,historyEntriesAdded:1,operation:'create',strokeId:'native-stroke:'+(q.expectedRevision+1)+':0'}});
const refusal=(q,code='wrong_instance')=>({apiVersion:2,requestId:q.requestId,instanceId:'actual-instance',documentId:'actual-document',contentRevision:20,ok:false,error:{code,message:'Native vector request refused.'}});
test('all three same-owner vector families preserve one bounded typed wrong_instance refusal with actual owner IDs',async()=>{
 const requests=[create(),{...create(),operation:C.PARAMETRIC,payload:{command:'object.parametric.translate',stableTarget:{contextId:'scene-root',frameScope:{kind:'authored',frame:0},layerUid:'layer-a',strokeId:'stroke-a'},delta:[1,2]}},{...create(),operation:C.EFFECTS,payload:{command:'object.path.effects.set',layerUid:'layer-a',stack:[]}}];
 for(const q of requests){const before=structuredClone(q),r=refusal(q);let calls=0;assert.deepEqual(await api(()=>{calls++;return r;}).dispatch(q),r);assert.equal(calls,1);assert.deepEqual(q,before);}
});
test('native allocation matches the requested next revision and canonical decimal suffix',async()=>{
 const q=create(),prefix='native-stroke:'+(q.expectedRevision+1)+':';
 for(const id of ['native-stroke:999:0',prefix+'',prefix+'x',prefix+'00',prefix+'01',prefix+'-1',prefix+'1.0',prefix+'1e2',prefix+'雪'])await assert.rejects(api(()=>({...success(q),result:{...success(q).result,strokeId:id}})).dispatch(q),TypeError,id);
 for(const suffix of ['0','1','12'])assert.equal((await api(()=>({...success(q),result:{...success(q).result,strokeId:prefix+suffix}})).dispatch(q)).result.strokeId,prefix+suffix);
});
test('request correlation, success owners and all other failure owners stay strict',async()=>{
 const q=create();
 for(const mutate of [r=>r.requestId='foreign',r=>r.error.code='not_a_native_code',r=>r.error.extra=1,r=>r.error.message='x'.repeat(4096)]){const r=refusal(q);mutate(r);await assert.rejects(api(()=>r).dispatch(q),TypeError);}
 for(const code of ['stale_revision','not_found','invalid_request','unavailable','wrong_document'])await assert.rejects(api(()=>refusal(q,code)).dispatch(q),TypeError);
 for(const key of ['instanceId','documentId']){const r=success(q);r[key]='foreign';await assert.rejects(api(()=>r).dispatch(q),TypeError);}
 assert.throws(()=>C.correlate({...q,operation:'unrelated.operation'},refusal(q)),TypeError);
});
test('same-instance replacement and same-owner stale head refusals retain existing semantics',async()=>{
 const q=create(),replacement=refusal(q,'wrong_document');replacement.instanceId=q.instanceId;assert.deepEqual(await api(()=>replacement).dispatch(q),replacement);
 const stale=refusal(q,'stale_revision');stale.instanceId=q.instanceId;stale.documentId=q.documentId;assert.deepEqual(await api(()=>stale).dispatch(q),stale);
});
test('the exception cannot certify a false create or a receipt from another request revision',async()=>{
 const q=create(),noop=success(q);noop.contentRevision=q.expectedRevision;noop.result.applied=false;noop.result.historyEntriesAdded=0;await assert.rejects(api(()=>noop).dispatch(q),TypeError);
 const stale=success(q);stale.contentRevision=q.expectedRevision;await assert.rejects(api(()=>stale).dispatch(q),TypeError);
});
