'use strict';
const test=require('node:test'),assert=require('node:assert/strict');
const {fixture,block,target,deferred}=require('./native-text-tool-support.cjs');
const placement={anchorTopLeft:{x:12,y:34},fixedWidth:null};
test('point and area intents use native create and one serialized command per actual commit',async()=>{
  for(const fixedWidth of [null,180]){
    const f=fixture();await f.core.openNew({...placement,fixedWidth});assert.equal(f.owner.log.received.length,0,'discovery is read-only');
    const receipt=await f.core.commit({text:'Hello 雪'});assert.equal(receipt.historyEntriesAdded,1);assert.equal(f.owner.log.executed.length,1);
    const request=f.owner.log.received[0];assert.equal(request.payload.command,'object.text.create');assert.deepEqual(request.payload.target,target('',0));
    assert.deepEqual(request.payload.block.anchorTopLeft,{x:12,y:34});assert.equal(request.payload.block.fixedWidth,fixedWidth);assert.equal(request.payload.block.color,'#123456');assert.equal(f.committed.length,1);
    const nativeTarget=(await f.core.page(0,8)).page.items[0].target;assert.equal(nativeTarget.strokeId,receipt.strokeId,'selection discovers allocated native identity');
  }
});
test('existing and reopened authoritative targets edit text/font without moving anchor or dropping untouched fields',async()=>{
  const f=fixture(),t=target('imported stable / 雪');f.seed(t,block('Original'));const page=await f.core.page(0,8);assert.deepEqual(page.page.items,[{target:t}]);
  const draft=await f.core.openExisting(page.page.items[0].target);assert.equal(draft.block.text,'Original');assert.ok(Object.isFrozen(draft.block));
  const receipt=await f.core.commit({font:{kind:'asset',assetId:'document font / 雪'}});assert.equal(receipt.strokeId,t.strokeId);
  const q=f.owner.log.received[0];assert.deepEqual(q.payload.patch,{font:{kind:'asset',assetId:'document font / 雪'}});assert.deepEqual(q.payload.stableTarget,t);
  assert.deepEqual(f.owner.items.get(t.strokeId).anchorTopLeft,{x:10,y:20});assert.equal(f.owner.items.get(t.strokeId).bold,true);assert.equal(f.owner.items.get(t.strokeId).letterSpacing,2);
  const reopened=fixture();reopened.state.document='document-reopened';reopened.seed(t,f.owner.items.get(t.strokeId));
  await reopened.core.openExisting((await reopened.core.page(0,8)).page.items[0].target);await reopened.core.commit({text:'After reopen'});
  assert.deepEqual(reopened.owner.log.received[0].payload.stableTarget,t);assert.equal(reopened.owner.items.get(t.strokeId).font.assetId,'document font / 雪');
});
test('selection, frame, revision and viewport changes while queued reject before any mutation dispatch',async()=>{
  for(const change of [f=>f.selection('other'),f=>f.frame(1),f=>{f.state.revision++;},f=>f.view('resized')]){
    const f=fixture();await f.core.openNew(placement);const gate=deferred();f.hold(gate.promise);const pending=f.core.commit({text:'Stale'});change(f);gate.resolve();
    await assert.rejects(pending,e=>e.code==='stale_revision');assert.equal(f.owner.log.received.length,0);assert.equal(f.owner.log.executed.length,0);
  }
});
test('document lifecycle replacement never sends the old draft to the new owner',async()=>{
  const f=fixture();await f.core.openNew(placement);f.state.document='replacement';f.state.generation++;
  await assert.rejects(f.core.commit({text:'Old draft'}),e=>e.code==='stale_revision');assert.equal(f.owner.log.executed.length,0);
});
test('cancel and blank content publish no command or optimistic native item',async()=>{
  const f=fixture();await f.core.openNew(placement);assert.equal(f.core.cancel(),true);assert.equal(f.owner.log.executed.length,0);
  await f.core.openNew(placement);assert.equal((await f.core.commit({text:'   '})).skipped,'blank_text');assert.equal(f.owner.items.size,0);assert.equal(f.committed.length,0);
});
test('duplicate Apply during pending acknowledgement does not allocate another request',async()=>{
  const gate=deferred(),options={respond:async r=>{await gate.promise;return r;}},f=fixture(options);
  await f.core.openNew(placement);const pending=f.core.commit({text:'Once'});await Promise.resolve();await assert.rejects(f.core.commit({text:'Twice'}),e=>e.code==='unavailable');gate.resolve();await pending;
  assert.equal(f.owner.log.executed.length,1);assert.equal(f.owner.items.size,1);
});
test('unknown outcome fences new writes and recovers only the retained request/body after context changes',async()=>{
  const f=fixture({faults:['after','before']});await f.core.openNew(placement);
  await assert.rejects(f.core.commit({text:'Pending'}),e=>e.code==='item_command_indeterminate');assert.equal(f.core.snapshot().unknown,true);
  const ids=f.events.ids.slice();f.frame(1);await assert.rejects(f.core.commit({text:'Duplicate'}),e=>e.code==='item_command_indeterminate');assert.deepEqual(f.events.ids,ids);
  const receipt=await f.core.resolve();assert.equal(receipt.applied,true);assert.equal(f.core.snapshot().unknown,false);assert.equal(f.owner.log.executed.length,1);
  assert.equal(new Set(f.owner.log.received.map(q=>q.requestId)).size,1);assert.equal(new Set(f.owner.log.received.map(q=>JSON.stringify(q))).size,1);
});
test('unknown outcome can retire only when a genuinely different native owner appears',async()=>{
  const f=fixture({faults:['after','before']});await f.core.openNew(placement);await assert.rejects(f.core.commit({text:'Pending'}));assert.equal(f.core.snapshot().unknown,true);
  f.state.generation++;f.state.document='new-owner';f.owner.items.clear();f.targets.clear();assert.equal(f.core.snapshot().unknown,false);assert.equal(f.core.snapshot().draft,null);
});
test('font-only editing does not resend a large unchanged text body through the4096-byte mutation port',async()=>{
  const f=fixture(),t=target('long-native-text');f.seed(t,block('Snow 雪'.repeat(700)));await f.core.openExisting(t);await f.core.commit({font:{kind:'bundled',fontKey:'Roboto-Bold'}});
  const q=f.owner.log.received[0];assert.deepEqual(Object.keys(q.payload.patch),['font']);assert.ok(Buffer.byteLength(JSON.stringify(q))<4096);
});
test('no matching native text record or presented frame cannot become an editable draft',async()=>{
  const f=fixture();await assert.rejects(f.core.openExisting(target('missing')),e=>e.code==='not_found');f.presented(false);await assert.rejects(f.core.openNew(placement),e=>e.code==='unavailable');assert.equal(f.owner.log.executed.length,0);
});

test('read cancellation invalidates late new and existing block discovery without retaining a busy writer',async()=>{
 for(const existing of [false,true]){const f=fixture(),gate=deferred();f.seed();f.hold(gate.promise);const pending=existing?f.core.openExisting(target()):f.core.openNew(placement);
  assert.equal(f.core.snapshot().discovering,true);assert.equal(f.core.cancel(),true);assert.equal(f.core.snapshot().busy,false);gate.resolve();await assert.rejects(pending,e=>e.code==='stale_revision');
  assert.equal(f.drafts.length,0);assert.equal(f.core.snapshot().draft,null);assert.equal(f.owner.log.executed.length,0);await f.core.openNew(placement);assert.ok(f.core.snapshot().draft);
 }
});
test('cancel cannot erase queued commands or indeterminate same-request recovery',async()=>{
 const f=fixture();await f.core.openNew(placement);const gate=deferred();f.hold(gate.promise);const pending=f.core.commit({text:'Queued'});
 assert.equal(f.core.snapshot().discovering,false);assert.equal(f.core.cancel(),false);assert.equal(f.core.snapshot().busy,true);gate.resolve();await pending;assert.equal(f.owner.log.executed.length,1);
 const g=fixture({faults:['after','before']});await g.core.openNew(placement);await assert.rejects(g.core.commit({text:'Unknown'}),e=>e.code==='item_command_indeterminate');
 assert.equal(g.core.cancel(),false);assert.equal(g.core.snapshot().unknown,true);await g.core.resolve();assert.equal(g.owner.log.executed.length,1);assert.equal(new Set(g.owner.log.received.map(q=>JSON.stringify(q))).size,1);
});
test('retiring a pending read releases discovery state and cannot publish its eventual draft',async()=>{
 const f=fixture(),gate=deferred();f.hold(gate.promise);const pending=f.core.openNew(placement);f.core.retire();assert.equal(f.core.snapshot().busy,false);assert.equal(f.core.snapshot().discovering,false);
 gate.resolve();await assert.rejects(pending,e=>e.code==='stale_revision');assert.equal(f.drafts.length,0);assert.equal(f.owner.log.executed.length,0);
});
