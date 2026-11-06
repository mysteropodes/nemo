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

// The application core receives the same production modules the desktop bootstrap composes.
test('Text application requires explicit real reader and writer ports without fallback or dispatch',()=>{
  const Tool=require('../src/js/adapters/native-text-tool.js'),f=fixture();
  for(const changes of [{reader:null},{writer:null},{reader:{}},{writer:{create:false}}]){
    assert.throws(()=>Tool.create({...f.ports,...changes}),/Native Text ports unavailable/);
    assert.equal(f.reads.length,0);assert.equal(f.owner.log.received.length,0);assert.equal(f.owner.log.executed.length,0);
  }
  assert.equal(f.ports.reader,require('../src/js/adapters/native-text-read.js'));
  assert.equal(f.ports.writer,require('../src/js/adapters/native-object-text-layout.js'));
});

// #2267 T-text-context: held frames (copy key), Motion (in place) and an entered component's own context.
test('a held frame edits the text of its key through the copy key; Motion edits that key in place; a key cell needs neither',async()=>{
  const Tool=require('../src/js/adapters/native-text-tool.js'),t=target('held text',0);
  const f=fixture();f.seed(t,block('Key text'));f.frame(3);
  const draft=await f.core.openExisting(t);assert.equal(draft.block.text,'Key text','the playhead shows the text of an earlier key');
  await f.core.commit({text:'Edited'});
  const q=f.owner.log.received[0].payload;assert.equal(q.command,'object.text.edit');assert.deepEqual(q.stableTarget,t);assert.equal(q.keyframe,'copy');assert.equal(q.frame,3);
  const inherit=fixture();inherit.seed(t,block('Key text'));inherit.frame(3);
  const core=Tool.create({...inherit.ports,inherit:()=>true});await core.openExisting(t);await core.commit({text:'In place'});
  const p=inherit.owner.log.received[0].payload;assert.equal(p.keyframe,undefined);assert.equal(p.frame,undefined);assert.deepEqual(p.stableTarget,t);
  const keyed=fixture();keyed.seed(t,block('Key text'));await keyed.core.openExisting(t);await keyed.core.commit({text:'Same cell'});
  const k=keyed.owner.log.received[0].payload;assert.equal(k.keyframe,undefined);assert.equal(k.frame,undefined);
  const later=fixture();later.seed(target('later',5),block('Later key'));later.frame(3);
  await assert.rejects(later.core.openExisting(target('later',5)),e=>e.code==='unavailable','text keyed after the playhead is not shown there');
});
test('new text on a held frame asks for the copy key, in both modes', async()=>{
  const Tool=require('../src/js/adapters/native-text-tool.js');
  for(const inherit of [false,true]){
    const f=fixture();f.frame(4);const core=Tool.create({...f.ports,inherit:()=>inherit});await core.openNew(placement);await core.commit({text:'Fresh'});
    const q=f.owner.log.received[0].payload;assert.equal(q.command,'object.text.create');assert.equal(q.keyframe,'copy');assert.deepEqual(q.target,target('',4));
  }
});
test('inside an entered component the draft, its reads and its writes carry the component context and ride the component route', async()=>{
  const Tool=require('../src/js/adapters/native-text-tool.js'),f=fixture(),routes=[];
  const session={...f.session,runResourceCommand:a=>{routes.push('scene');return f.session.runResourceCommand(a);},runComponentCommand:a=>{routes.push('component');return f.session.runResourceCommand(a);}};
  const inside={...target('comp text',0),contextId:'comp-a'};
  // the double stores and lists targets by their context; the owner double validates the payload with the real contract
  f.seed(inside,block('Inside'));
  const core=Tool.create({...f.ports,session,context:p=>({...f.context(p),contextId:'comp-a'})});
  const page=await core.page(0,8);assert.equal(page.context.contextId,'comp-a');assert.deepEqual(page.page.items.map(i=>i.target),[inside]);
  await core.openExisting(inside);await core.commit({text:'Edited inside'});
  assert.deepEqual(f.owner.log.received[0].payload.stableTarget,inside);
  assert.ok(routes.length>0&&routes.every(r=>r==='component'),routes.join());
  const other=Tool.create({...f.ports,session,context:p=>({...f.context(p),contextId:'comp-b'})});
  await assert.rejects(other.openExisting(inside),e=>e.code==='unavailable','a block of another context is never opened here');
  const made=Tool.create({...f.ports,session,context:p=>({...f.context(p),contextId:'comp-a'})});await made.openNew(placement);await made.commit({text:'New inside'});
  assert.deepEqual(f.owner.log.received[1].payload.target,{...target('',0),contextId:'comp-a'});
});
