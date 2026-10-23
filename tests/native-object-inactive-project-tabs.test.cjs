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
      if(custom.invoke){const result=await custom.invoke({req,calls,owner,rows,notices,get retained(){return retained;},set retained(v){retained=v;}});if(result!==undefined)return result;}
      if(req.action.kind==='inspect')return receipt(req,retained?'inactive-retained':'inactive-absent',retained,retained&&retained.preparationToken);
      if(req.action.kind==='prepare'){retained=project(req);return receipt(req,'inactive-prepared',retained);}
      if(req.action.kind==='close'){retained=null;return receipt(req,'inactive-closed',null,req.action.preparationToken);}
      throw Error('unexpected action');
    }};
  return {owner,calls,rows,notices,get retained(){return retained;},set retained(v){retained=v;},tabs:adapter.create({},ports)};
}

test('replacement eligibility is a read-only fence through busy preparation, retained B and correlated close',async()=>{
  let resume;const gate=new Promise(resolve=>{resume=resolve;});
  const f=fixture({async invoke({req}){if(req.action.kind==='prepare')await gate;}});
  assert.equal(f.tabs.canReplaceActiveProject(),true);assert.equal(f.calls.length,0);
  const preparing=f.tabs.openPath('/B.json');await new Promise(setImmediate);
  assert.equal(f.tabs.isBusy(),true);assert.equal(f.tabs.canReplaceActiveProject(),false);
  resume();assert.equal((await preparing).status,'prepared');
  const held=f.tabs.held(),calls=f.calls.length;
  assert.equal(f.tabs.isBusy(),false);assert.equal(f.tabs.canReplaceActiveProject(),false);
  assert.equal(f.calls.length,calls,'eligibility must not send a native request');
  assert.equal(Object.isFrozen(held),true);assert.throws(()=>{held.preparationToken='forged';},TypeError);
  assert.equal((await f.tabs.close({...held})).status,'unavailable','a mutable alias cannot close retained B');
  assert.equal(f.tabs.canReplaceActiveProject(),false);assert.strictEqual(f.tabs.held(),held);
  assert.equal((await f.tabs.close(held)).status,'closed');assert.equal(f.tabs.canReplaceActiveProject(),true);
});

test('unknown preparation outcome blocks replacement even with no visible inactive row',async()=>{
  const f=fixture({invoke({req}){
    if(req.action.kind==='inspect'&&!req.action.recoverRequestId)return receipt(req,'inactive-absent');
    throw Error('unknown prepare or recovery outcome');
  }});
  assert.equal((await f.tabs.openPath('/B.json')).status,'refused');
  assert.equal(f.tabs.isBusy(),false);assert.equal(f.tabs.held(),null);assert.equal(f.rows.length,0);
  const calls=f.calls.length;assert.equal(f.tabs.canReplaceActiveProject(),false);
  assert.equal(f.calls.length,calls,'uncertainty is not resolved by asking for eligibility');
});

test('pending and unknown close outcomes cannot release the replacement fence',async()=>{
  let resume,closing=false;const gate=new Promise(resolve=>{resume=resolve;});
  const f=fixture({async invoke({req}){
    if(req.action.kind==='close'){closing=true;await gate;throw Error('lost close outcome');}
    if(closing&&req.action.recoverRequestId)throw Error('unknown close recovery');
  }});
  await f.tabs.openPath('/B.json');const held=f.tabs.held(),close=f.tabs.close(held);
  await new Promise(setImmediate);assert.equal(f.tabs.isBusy(),true);assert.equal(f.tabs.canReplaceActiveProject(),false);
  resume();assert.equal((await close).status,'refused');assert.equal(f.tabs.isBusy(),false);
  assert.strictEqual(f.tabs.held(),held);assert.equal(f.tabs.canReplaceActiveProject(),false);
});

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
      if(req.action.recoverRequestId===closeId)v.recoveredRequest={requestId:closeId,outcome:{ok:true,value:{state:'inactive-closed',preparationToken:token}}};
      return v;
    }
    if(req.action.kind==='prepare'){retained=project(req);return receipt(req,'inactive-prepared',retained);}
    closeId=req.requestId;retained=null;throw Error('close reply lost');
  }});
  await f.tabs.openPath('/B.json');assert.equal(f.rows.length,1);
  assert.equal((await f.tabs.close(f.rows[0])).status,'closed');assert.equal(f.rows.length,0);
  assert.equal(Object.hasOwn(f.calls.at(-1).action,'preparationToken'),false,'closed-state recovery must inspect without a token');
});

test('cancelled-before-dispatch prepare makes no row and a later explicit Open gets a new request',async()=>{
  let cancelled=true;const f=fixture({invoke({req}){if(req.action.kind==='prepare'&&cancelled){cancelled=false;throw {code:'cancelled_before_dispatch',message:'cancelled',details:{disposition:'notDispatched',retryExecution:false}};}}});
  assert.equal((await f.tabs.openPath('/B.json')).status,'refused');assert.equal(f.rows.length,0);
  const first=f.calls.find(call=>call.action.kind==='prepare').requestId;
  assert.equal((await f.tabs.openPath('/B.json')).status,'prepared');
  const attempts=f.calls.filter(call=>call.action.kind==='prepare');assert.equal(attempts.length,2);
  assert.notEqual(attempts[1].requestId,first);assert.equal(f.rows.length,1);
});

test('wrong-token inspect refusal leaves B visible and never activates it',async()=>{
  let refuse=false;const f=fixture({invoke({req}){
    if(refuse&&req.action.kind==='inspect'&&req.action.preparationToken)
      throw {code:'preparation_expired',message:'unknown token',details:{disposition:'notDispatched',retryExecution:false}};
  }});
  await f.tabs.openPath('/B.json');refuse=true;
  assert.equal((await f.tabs.activateClick(f.rows[0])).status,'indeterminate');
  assert.equal(f.rows.length,1);assert.equal(f.rows[0].preparationToken,token);
  assert.equal(f.calls.at(-1).action.preparationToken,token);
});

test('a contradictory retained project cannot be removed by a forged recovered-close outcome',async()=>{
  let retained,closeId;
  const f=fixture({invoke({req}){
    if(req.action.kind==='inspect'){
      const v=receipt(req,'inactive-absent',retained, null);
      if(req.action.recoverRequestId===closeId)v.recoveredRequest={requestId:closeId,outcome:{ok:true,value:{state:'inactive-closed',preparationToken:token}}};
      return v;
    }
    if(req.action.kind==='prepare'){retained=project(req);return receipt(req,'inactive-prepared',retained);}
    if(req.action.kind==='close'){closeId=req.requestId;throw Error('close reply lost');}
  }});
  await f.tabs.openPath('/B.json');
  assert.equal((await f.tabs.close(f.rows[0])).status,'indeterminate');
  assert.equal(f.rows.length,1);assert.equal(f.rows[0].preparationToken,token);
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
