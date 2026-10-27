'use strict';
const test=require('node:test'),assert=require('node:assert/strict');
const C=require('../src/js/adapters/native-subselect-contract.js'),Q=require('../src/js/adapters/native-object-query-contract.js');
const target={contextId:'scene-root',layerUid:'layer-a',frameScope:{kind:'authored',frame:0},strokeId:'stroke-a'};
const p={atRevision:7,documentSnapshotId:'native-object:document:7',stableTarget:target,subselect:{cursor:0,limit:16}};
const r={atRevision:7,documentSnapshotId:p.documentSnapshotId,subselect:{stableTarget:target,closed:false,total:2,cursor:0,nextCursor:null,preview:false,segments:[{point:[0,0],handleIn:[0,0],handleOut:[0,0]},{point:[10,0],handleIn:[0,0],handleOut:[0,0]}]}};
test('subselect accepts independent baseline intents and closed geometry pages',()=>{
  const oracle=require('../native-engine/tests/fixtures/subselect-baseline/oracle.json');
  for(const c of oracle.cases)assert.equal(C.intent(c.intent),true,JSON.stringify(c.intent));
  Q.payload(p);Q.result(r);assert.equal(Q.correlate(p,r,null,'document'),true);
  assert.throws(()=>Q.payload({...p,view:{kind:'presentation'}}));
  assert.throws(()=>Q.payload({...p,subselect:{...p.subselect,preview:null}}));
  for(const patch of [{cursor:1},{total:3},{nextCursor:0},{preview:true},{stableTarget:{...target,strokeId:'foreign'}}])assert.throws(()=>Q.correlate(p,{...r,subselect:{...r.subselect,...patch}},null,'document'));
  assert.throws(()=>Q.correlate(p,r,null,'replacement'));
});
test('subselect rejects duplicate indices, unknown fields and nonfinite or singular wire values',()=>{
  for(const intent of [{kind:'move',indices:[0,0],delta:[0,0]},{kind:'move',indices:[0],delta:[NaN,0]},{kind:'toggle',index:256},{kind:'toggle',index:1,extra:true},{kind:'insert',curve:0,time:0}])assert.equal(C.intent(intent),false);
});
test('subselect atFrame is an optional bounded integer, with source identity retained separately',()=>{
  const payload={command:'object.subselect.edit',stableTarget:target,intent:{kind:'move',indices:[0],delta:[0,0]}};
  C.payload(C.EDIT,payload);for(const atFrame of [0,5,4294967295])C.payload(C.EDIT,{...payload,atFrame});
  for(const atFrame of [-1,1.5,4294967296,null,'5',NaN])assert.throws(()=>C.payload(C.EDIT,{...payload,atFrame}));
});
test('subselect presentation pages bind native transforms to the exact view and preview disposition',()=>{
  const view={kind:'subselect',contextId:'scene-root',layerUid:'layer-a',frame:0,workId:'native-work-1',viewGeneration:1,cursor:0,limit:8};
  const p={atRevision:7,documentSnapshotId:'native-object:document:7',view};
  const r={...view,atRevision:7,documentSnapshotId:p.documentSnapshotId,total:1,nextCursor:null,preview:false,items:[{stableTarget:target,closed:false,count:2,matrix:[1,.2,.3,1,8,9]}]};delete r.limit;
  Q.payload(p);Q.result(r);Q.correlate(p,r,null,'document');
  for(const patch of [{workId:'native-work-2'},{viewGeneration:2},{frame:1},{preview:true},{total:2}])assert.throws(()=>Q.correlate(p,{...r,...patch},null,'document'));
  assert.throws(()=>Q.result({...r,items:[{...r.items[0],matrix:[1,2,2,4,0,0]}]}));
  const preview={stableTarget:target,intent:{kind:'move',indices:[0],delta:[5,7]}};
  Q.correlate({...p,view:{...view,preview}},{...r,preview:true},null,'document');
  assert.throws(()=>Q.payload({...p,view:{...view,preview:{...preview,extra:1}}}));
});
test('application transport preserves opaque source IDs in native subselect preview and selection pages',()=>{
  const A=require('../src/js/adapters/native-application.js');
  const t={...target,layerUid:'data:opaque layer',strokeId:'blob:opaque stroke'};
  const view={kind:'subselect',contextId:'scene-root',layerUid:t.layerUid,frame:0,workId:'native-work-1',viewGeneration:1,cursor:0,limit:8,
    preview:{stableTarget:t,intent:{kind:'move',indices:[0],delta:[1,2]}}};
  const q={apiVersion:2,requestId:'opaque-subselect',instanceId:'instance',documentId:'document',operation:'query.document.object',payload:{atRevision:7,documentSnapshotId:p.documentSnapshotId,view}};
  const result={...view,atRevision:7,documentSnapshotId:p.documentSnapshotId,total:1,nextCursor:null,preview:true,items:[{stableTarget:t,closed:false,count:2,matrix:[1,0,0,1,0,0]}]};delete result.limit;
  A.validateRequest(q);
  A.validateResponse({apiVersion:2,requestId:q.requestId,instanceId:q.instanceId,documentId:q.documentId,contentRevision:7,ok:true,result},q.requestId,q.operation);
});
