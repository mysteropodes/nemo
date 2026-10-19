'use strict';
// PRIVATE proposal tests: mocked shared transport, not native/installed acceptance.
const test = require('node:test');
const assert = require('node:assert/strict');
const {createNativeApplicationAdapter,validateRequest} = require('../src/js/adapters/native-application.js');
const op='query.document.object.property.metadata';
const categories=['conditionalMotionRows','pathVerticesTrimStrokeBrushMesh','referenceOnlyElements','expressionDiagnostics','layerEffectMatteReveal','propertyWrites'];
function request(){return {apiVersion:2,requestId:'metadata-1',instanceId:'instance',documentId:'doc',operation:op,payload:{atRevision:0,documentSnapshotId:'native-object:doc:0',view:{kind:'properties',target:{contextId:'scene-root',layerUid:'data:opaque 雪'},offset:0,limit:2}}};}
function response(q){return {apiVersion:2,requestId:q.requestId,instanceId:q.instanceId,documentId:q.documentId,contentRevision:0,ok:true,result:{instanceId:q.instanceId,documentId:q.documentId,atRevision:0,documentSnapshotId:q.payload.documentSnapshotId,kind:'properties',target:{layerUid:q.payload.view.target.layerUid,contextId:'scene-root'},items:[{property:'opacity',dimension:1,unit:'',defaultValue:[100],animatable:true,controlType:null,controlName:'  data:雪  ',authored:{availability:'available',keyCount:0,hasStaticOverride:false,expressionEnabled:false},availability:{metadata:'available',applicability:'available',propertyWrite:'unavailable',reason:'read-only-description-not-command-availability'}}],nextOffset:1,unavailableCategories:categories.map(category=>({category,state:'unavailable',reason:'not-admitted'}))}};}
const adapter=send=>createNativeApplicationAdapter('metadata-proposal',{dispatch:async q=>send(q)});
test('vertex discovery is an explicit boolean opt-in on properties only', async()=>{
  for(const flag of [true,false]){
    const q=request();q.payload.view.includeVertices=flag;
    assert.doesNotThrow(()=>validateRequest(q));
    assert.deepEqual(await adapter(response).dispatch(q),response(q));
  }
  for(const flag of [null,1,'true',[],{}]){
    const q=request();q.payload.view.includeVertices=flag;
    assert.throws(()=>validateRequest(q));
  }
  for(const view of [{kind:'targets',offset:0,limit:1},
    {kind:'storedProperty',target:request().payload.view.target,property:'opacity'}]){
    const q=request();q.payload.view={...view,includeVertices:true};
    assert.throws(()=>validateRequest(q));
  }
});
test('opaque identities, reordered target keys and exact UTF8 control names round-trip without mutation',async()=>{const q=request(),before=structuredClone(q);const r=await adapter(response).dispatch(q);assert.deepEqual(q,before);assert.equal(r.result.controlName,undefined);assert.equal(r.result.items[0].controlName,'  data:雪  ');});
test('strict selector rejects extra lifecycle, positional view, unsupported view and expected revision before send',()=>{for(const mutate of [q=>q.payload.lifecycleGeneration=1,q=>q.payload.view=[],q=>q.payload.view.kind='writes',q=>q.expectedRevision=0,q=>q.payload.view.limit=9,q=>q.payload.view.target.extra=true]){const q=request();mutate(q);let calls=0;assert.throws(()=>adapter(()=>{calls++;}).dispatch(q));assert.equal(calls,0);}});
test('all four inner pins, original request ID, outer identities and content revision correlate',async()=>{for(const mutate of [r=>r.requestId='other',r=>r.instanceId='other',r=>r.documentId='other',r=>r.result.instanceId='other',r=>r.result.documentId='other',r=>r.result.atRevision=1,r=>r.result.documentSnapshotId='other',r=>r.contentRevision=1]){await assert.rejects(adapter(q=>{const r=response(q);mutate(r);return r;}).dispatch(request()));}});
test('cursor, complete page target, authored facts and unavailable categories cannot be weakened',async()=>{for(const mutate of [r=>r.result.nextOffset=0,r=>r.result.nextOffset=3,r=>r.result.target.extra=true,r=>r.result.items[0].authored.keyCount=-1,r=>r.result.items[0].availability.propertyWrite='available',r=>r.result.unavailableCategories.pop()]){await assert.rejects(adapter(q=>{const r=response(q);mutate(r);return r;}).dispatch(request()));}});
test('original request and inner/complete response byte bounds refuse oversized opaque metadata',async()=>{const q=request();q.payload.view.target.layerUid='雪'.repeat(1400);assert.throws(()=>validateRequest(q));await assert.rejects(adapter(q=>{const r=response(q);r.result.items[0].controlName='雪'.repeat(1100);return r;}).dispatch(request()));});
test('typed unavailable is retained; this proposal never falls back to a property writer',async()=>{const q=request();const r={apiVersion:2,requestId:q.requestId,instanceId:q.instanceId,documentId:q.documentId,contentRevision:0,ok:false,error:{code:'unavailable',message:'not admitted'}};assert.deepEqual(await adapter(()=>r).dispatch(q),r);});

test('opaque data/blob property keys are identities only at their appointed selector/result paths',async()=>{
  for(const property of ['data:gain','blob:gain','  data:gain 雪  ']){
    const q=request();q.payload.view={kind:'storedProperty',target:q.payload.view.target,property};
    const r=response(q);r.result.kind='storedProperty';r.result.nextOffset=null;r.result.items[0].property=property;
    Object.assign(r.result.items[0].availability,{applicability:'unavailable',reason:'stored-metadata-does-not-prove-applicability'});
    assert.deepEqual(await adapter(()=>r).dispatch(q),r);
    const p=request(),page=response(p);page.result.items[0].property=property;
    assert.deepEqual(await adapter(()=>page).dispatch(p),page);
  }
});
test('property-key exemptions do not admit resource strings at any unrelated metadata path',async()=>{
  for(const value of ['data:gain','blob:gain']){
    const q=request();q.payload.documentSnapshotId=value;assert.throws(()=>validateRequest(q),/resource/);
    for(const mutate of [r=>r.result.documentSnapshotId=value,r=>r.result.items[0].unit=value,r=>r.result.items[0].controlType=value,r=>r.result.unavailableCategories[0].reason=value]){
      await assert.rejects(adapter(q=>{const r=response(q);mutate(r);return r;}).dispatch(request()),/resource/);
    }
  }
});
