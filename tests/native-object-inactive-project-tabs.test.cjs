'use strict';
const assert = require('node:assert/strict');
const test = require('node:test');
const adapter = require('../src/js/adapters/native-object-inactive-project-tabs.js');

const token = 'bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb';
function project(request, preparationRequestId=request.requestId, preparationToken=token) {
  return { instanceId:request.instanceId,contentRevision:0,
    documentSnapshotId:'snapshot-B',documentDigest:'a'.repeat(64),byteCount:128,canvasW:320,canvasH:180,
    fps:24,totalFrames:2,layers:[{layerUid:'layer-b'}],reference:null,
    origin:{documentId:request.documentId,documentSnapshotId:request.documentSnapshotId,contentRevision:request.expectedRevision},
    preparationToken,preparationRequestId };
}
function receipt(request,state,meta=null,tokenValue=meta?meta.preparationToken:null) {
  return {apiVersion:2,requestId:request.requestId,instanceId:request.instanceId,documentId:request.documentId,
    contentRevision:request.expectedRevision,documentSnapshotId:request.documentSnapshotId,
    lifecycleGeneration:request.lifecycleGeneration,state,preparationToken:tokenValue,project:meta,retryExecution:false};
}
function fixture(custom={}) {
  const owner={apiVersion:2,instanceId:'instance-a',documentId:'native-A',contentRevision:4,
    documentSnapshotId:'snapshot-A',lifecycleGeneration:17};
  const calls=[],rows=[],notices=[];let nextId=0,retained=null;
  const ports={owner:()=>({...owner}),requestId:()=>`request-${++nextId}`,
    baseName:path=>path.split('/').at(-1).replace(/\.json$/i,''),
    commit(meta){if(!rows.some(row=>row.preparationToken===meta.preparationToken))rows.push(meta);return true;},
    remove(meta){const at=rows.findIndex(row=>row.preparationToken===meta.preparationToken);if(at<0)return false;rows.splice(at,1);return true;},
    notice:text=>notices.push(text),
    async invoke(command,args){assert.equal(command,'nemo_native_object_inactive_project');const req=JSON.parse(args.requestJson);calls.push(req);
      if(custom.invoke)return custom.invoke({req,calls,owner,rows,notices,get retained(){return retained;},set retained(v){retained=v;}});
      if(req.action.kind==='inspect')return receipt(req,retained?'inactive-retained':'inactive-absent',retained,retained&&retained.preparationToken);
      if(req.action.kind==='prepare'){retained=project(req);return receipt(req,'inactive-prepared',retained);}
      if(req.action.kind==='close'){retained=null;return receipt(req,'inactive-closed',null,req.action.preparationToken);}
      throw Error('unexpected action');
    }};
  return {owner,calls,rows,notices,get retained(){return retained;},set retained(v){retained=v;},tabs:adapter.create({},ports)};
}

test('prepare uses the final typed DTO and retains only bounded metadata',async()=>{
  const f=fixture(),result=await f.tabs.openPath('/Projects/B.json');
  assert.equal(result.status,'prepared');assert.equal(f.calls.length,2);
  assert.deepEqual(f.calls[0].action,{kind:'inspect'});
  const req=f.calls[1];assert.deepEqual(Object.keys(req).sort(),['action','apiVersion','documentId','documentSnapshotId','expectedRevision','instanceId','lifecycleGeneration','requestId'].sort());
  assert.deepEqual(req.action,{kind:'prepare',sourcePath:'/Projects/B.json'});
  assert.equal(req.apiVersion,2);assert.equal(req.documentId,'native-A');assert.equal(req.expectedRevision,4);
  assert.equal(req.documentSnapshotId,'snapshot-A');assert.equal(req.lifecycleGeneration,17);
  assert.deepEqual(Object.keys(f.rows[0]).sort(),['byteCount','canvasH','canvasW','contentRevision','digest','documentSnapshotId','fps','instanceId','kind','lifecycleGeneration','name','originRevision','originSnapshotId','ownerDocumentId','path','preparationRequestId','preparationToken','totalFrames'].sort());
  assert.equal(f.rows[0].name,'B');assert.equal(f.rows[0].path,'/Projects/B.json');
  assert.equal(Object.hasOwn(f.rows[0],'layers'),false,'drawing records never enter retained JS tab state');
});

test('a lost prepare reply replays the identical request once and publishes the correlated retained B',async()=>{
  let first=true,stored;const f=fixture({invoke({req}){
    if(req.action.kind==='inspect')return receipt(req,stored?'inactive-retained':'inactive-absent',stored,stored&&stored.preparationToken);
    if(req.action.kind==='prepare'){
      if(!stored){stored=project(req);first=false;throw Error('reply lost after native retention');}
      return receipt(req,'inactive-prepared',stored);
    }
  }});
  const result=await f.tabs.openPath('/B.json');assert.equal(result.status,'prepared');
  const prepares=f.calls.filter(r=>r.action.kind==='prepare');assert.equal(prepares.length,2);
  assert.equal(JSON.stringify(prepares[0]),JSON.stringify(prepares[1]));assert.equal(f.rows.length,1);
});

test('malformed prepare reply reconciles only through inspect and original request correlation',async()=>{
  let retained;const f=fixture({invoke({req}){
    if(req.action.kind==='inspect'){
      const v=receipt(req,retained?'inactive-retained':'inactive-absent',retained,retained&&retained.preparationToken);
      if(req.action.recoverRequestId)v.recoveredRequest={requestId:req.action.recoverRequestId,outcome:{ok:true,value:{state:'inactive-prepared',preparationToken:token}}};
      return v;
    }
    retained=project(req);return {...receipt(req,'inactive-prepared',retained),requestId:'wrong-request'};
  }});
  const result=await f.tabs.openPath('/B.json');assert.equal(result.status,'prepared');
  assert.equal(f.rows.length,1);assert.equal(f.rows[0].preparationRequestId,f.calls.find(r=>r.action.kind==='prepare').requestId);
  assert.equal(f.calls.at(-1).action.kind,'inspect');assert.equal(f.calls.at(-1).action.recoverRequestId,f.rows[0].preparationRequestId);
});

test('unknown prepare outcome remains fenced and cannot issue a fresh request',async()=>{
  const f=fixture({invoke({req}){
    if(req.action.kind==='inspect')return receipt(req,'inactive-absent');
    throw Error('transport outcome unknown');
  }});
  assert.equal((await f.tabs.openPath('/B.json')).status,'indeterminate');
  const original=f.calls.find(r=>r.action.kind==='prepare');
  assert.equal((await f.tabs.openPath('/B.json')).status,'indeterminate');
  const prepares=f.calls.filter(r=>r.action.kind==='prepare');assert.equal(prepares.length,2);
  assert.ok(prepares.every(r=>JSON.stringify(r)===JSON.stringify(original)));
  assert.equal(f.rows.length,0);
});

test('owner replacement prevents uncertain prepare reconciliation and leaves no fabricated row',async()=>{
  const f=fixture({invoke({req,calls,owner}){
    if(req.action.kind==='inspect')return receipt(req,'inactive-absent');
    owner.documentId='native-A2';throw Error('lost prepare reply');
  }});
  const result=await f.tabs.openPath('/B.json');assert.notEqual(result.status,'prepared');
  assert.equal(f.rows.length,0);assert.equal(f.calls.filter(r=>r.action.kind==='inspect').length,1,'no inspect is sent to a replacement owner');
});

test('close removes B only after a correlated terminal close receipt',async()=>{
  const f=fixture();await f.tabs.openPath('/B.json');const row=f.rows[0];
  const result=await f.tabs.close(row);assert.equal(result.status,'closed');assert.equal(f.rows.length,0);
  const close=f.calls.find(r=>r.action.kind==='close');assert.equal(close.action.preparationToken,token);
});

test('lost close receipt removes the row only after inspect recovers the exact close',async()=>{
  let retained,closeId;
  const f=fixture({invoke({req}){
    if(req.action.kind==='inspect'){
      const v=receipt(req,retained?'inactive-retained':'inactive-absent',retained,retained&&retained.preparationToken);
      if(req.action.recoverRequestId===closeId)v.recoveredRequest={requestId:closeId,outcome:{ok:true,value:{state:'inactive-closed',preparationToken:token,project:null}}};
      return v;
    }
    if(req.action.kind==='prepare'){retained=project(req);return receipt(req,'inactive-prepared',retained);}
    closeId=req.requestId;retained=null;throw Error('close reply lost');
  }});
  await f.tabs.openPath('/B.json');assert.equal(f.rows.length,1);
  assert.equal((await f.tabs.close(f.rows[0])).status,'closed');assert.equal(f.rows.length,0);
  assert.equal(Object.hasOwn(f.calls.at(-1).action,'preparationToken'),false,'closed-state recovery must inspect without a token');
});

test('refused close preserves B and does not fabricate a terminal removal',async()=>{
  const f=fixture({invoke({req}){
    if(req.action.kind==='inspect')return receipt(req,'inactive-absent');
    if(req.action.kind==='prepare'){const p=project(req);return receipt(req,'inactive-prepared',p);}
    throw {code:'stale_revision',message:'owner changed',details:{disposition:'notDispatched',retryExecution:false}};
  }});
  await f.tabs.openPath('/B.json');const row=f.rows[0];
  assert.equal((await f.tabs.close(row)).status,'refused');assert.equal(f.rows.length,1);
});

test('tab click inspects B then reports unavailable without activation',async()=>{
  const f=fixture();await f.tabs.openPath('/B.json');
  assert.equal((await f.tabs.activateClick(f.rows[0])).status,'unavailable');
  assert.equal(f.calls.at(-1).action.kind,'inspect');assert.equal(f.calls.at(-1).action.preparationToken,token);
  assert.match(f.notices.at(-1),/Switching project tabs is unavailable/);assert.equal(f.rows.length,1);
});
