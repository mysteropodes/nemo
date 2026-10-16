'use strict';
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path');
const SDK=require('../src/js/adapters/native-application.js');
const OP='command.document.object.layer.policy.edit',root=path.resolve(__dirname,'..');
const request=(flag='locked',value=true)=>({apiVersion:2,requestId:'policy-7',instanceId:'instance-A',documentId:'document-A',expectedRevision:7,operation:OP,payload:{command:'object.layer.policy.edit',layerUid:'data:authored opaque layer',flag,value}});
const reply=(q,applied)=>({apiVersion:2,requestId:q.requestId,instanceId:q.instanceId,documentId:q.documentId,contentRevision:q.expectedRevision+(applied?1:0),ok:true,result:{applied,historyEntriesAdded:applied?1:0}});
const descriptor=JSON.parse(fs.readFileSync(path.join(root,'engineering/application/capabilities-v2/native-layer-policy.json'),'utf8'));
test('F06 descriptor and closed schema declare one exact revisioned flag command',()=>{
 const schema=JSON.parse(fs.readFileSync(path.join(root,'engineering/application/native-transport-v2.schema.json'),'utf8'));
 assert.deepEqual(descriptor.operations,[OP]);assert.equal(descriptor.id,'native.layer-policy');assert.equal(descriptor.authority.adapterOwnsWritableMirror,false);assert.equal(descriptor.authority.fallback,'none');
 const def=schema.$defs.ObjectLayerPolicyPayload;assert.equal(def.additionalProperties,false);assert.deepEqual(def.required,['command','layerUid','flag','value']);assert.deepEqual(def.properties.flag.enum,['locked','shy']);assert.equal(def.properties.value.type,'boolean');
 const branches=schema.$defs.Request.oneOf.filter(v=>v.allOf[1].properties.operation.const===OP);assert.equal(branches.length,1);assert.equal(branches[0].allOf[0].$ref,'#/$defs/RequiredRevisionRequest');assert.equal(branches[0].allOf[1].properties.payload.$ref,'#/$defs/ObjectLayerPolicyPayload');
 assert.equal(descriptor.examples[0].request.operation,OP);assert.equal(descriptor.examples[0].request.payload.flag,'locked');assert.equal(descriptor.examples[0].response.result.historyEntriesAdded,1);
});
test('SDK explicit policy method validates both flags and changed/noop correlated receipts',async()=>{
 for(const flag of ['locked','shy'])for(const applied of [true,false]){
  const q=request(flag,applied),original=structuredClone(q);let sent;
  const adapter=SDK.createNativeApplicationAdapter('policy-test',{dispatch(value){sent=value;return reply(value,applied);}});
  const response=await adapter.editLayerPolicy(q);assert.deepEqual(sent,original);assert.notEqual(sent,q);assert.deepEqual(q,original);assert.deepEqual(response,reply(q,applied));
 }
});
test('SDK policy request rejects open/missing/null/positional fields and unsupported flags before transport',()=>{
 let calls=0;const adapter=SDK.createNativeApplicationAdapter('policy-test',{dispatch(){calls++;throw Error('must not dispatch');}});
 for(const key of ['command','layerUid','flag','value']){let q=request();delete q.payload[key];assert.throws(()=>adapter.editLayerPolicy(q));q=request();q.payload[key]=null;assert.throws(()=>adapter.editLayerPolicy(q));}
 for(const [key,value] of [['extra',true],['command','object.layer.visibility.set'],['layerUid',''],['flag','solo'],['flag',{locked:null}],['value',1],['value','true']]){const q=request();q.payload[key]=value;assert.throws(()=>adapter.editLayerPolicy(q));}
 const q=request();q.payload=['object.layer.policy.edit','layer','locked',true];assert.throws(()=>adapter.editLayerPolicy(q));
 for(const revision of [undefined,null,-1,1.5,9007199254740992]){const q=request();q.expectedRevision=revision;assert.throws(()=>adapter.editLayerPolicy(q));}
 assert.equal(calls,0);
});
test('SDK policy response refuses mismatched identity/result/revision and oversized error envelope',async()=>{
 const q=request();for(const mutate of [r=>r.requestId='foreign',r=>r.instanceId='foreign',r=>r.documentId='foreign',r=>r.contentRevision=7,r=>r.result.applied=false,r=>r.result.historyEntriesAdded=0,r=>r.result.extra='no']){
  const adapter=SDK.createNativeApplicationAdapter('policy-test',{dispatch(){const r=reply(q,true);mutate(r);return r;}});await assert.rejects(adapter.editLayerPolicy(q));
 }
 const adapter=SDK.createNativeApplicationAdapter('policy-test',{dispatch(){return {...reply(q,true),ok:false,result:undefined,error:{code:'unavailable',message:'x'.repeat(4096)}};}});await assert.rejects(adapter.editLayerPolicy(q));
});
test('SDK stale refusal does not fabricate a flag write and caller input stays immutable while pending',async()=>{
 const q=request('shy',true),before=structuredClone(q);let finish;
 const adapter=SDK.createNativeApplicationAdapter('policy-test',{dispatch(){return new Promise(resolve=>{finish=resolve;});}});
 const pending=adapter.editLayerPolicy(q);assert.deepEqual(q,before);
 finish({apiVersion:2,requestId:q.requestId,instanceId:q.instanceId,documentId:q.documentId,contentRevision:8,ok:false,error:{code:'stale_revision',message:'Stale policy revision'}});
 const response=await pending;assert.equal(response.ok,false);assert.equal(response.error.code,'stale_revision');assert.deepEqual(q,before);
});
test('SDK explicit policy method cannot dispatch as a read or another mutation operation',()=>{
 let calls=0;const adapter=SDK.createNativeApplicationAdapter('policy-test',{dispatch(){calls++;}});
 for(const operation of ['query.document.object.property.metadata','query.document.object','command.document.object.layer.visibility.set']){const q=request();q.operation=operation;assert.throws(()=>adapter.editLayerPolicy(q));}
 assert.equal(calls,0);
});
test('SDK generic dispatcher and explicit method retain literal exact replay receipts without local state',async()=>{
 const q=request(),r=reply(q,true);let calls=0;const adapter=SDK.createNativeApplicationAdapter('policy-test',{dispatch(){calls++;return structuredClone(r);}});
 assert.deepEqual(await adapter.dispatch(q),r);assert.deepEqual(await adapter.editLayerPolicy(q),r);assert.equal(calls,2,'both calls reach the authority; SDK must not invent its own writable receipt owner');
});
