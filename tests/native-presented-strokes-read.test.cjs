'use strict';
// #2362: Select and the Labs knife read every painted record of the presented layer through the hit-test listing
// (bounds and role, never refused for being outside the editable scope) and page geometry only for the records kept.
const test=require('node:test'),assert=require('node:assert/strict');
const Reads=require('../src/js/adapters/native-object-project-reads.js');
function fixture(mode){
  const pin={instanceId:'i',documentId:'d',lifecycleGeneration:1,contentRevision:0,documentSnapshotId:'native-object:d:0'};
  const presentation={...pin,owner:'native',status:'presented',contextId:'scene-root',frame:0,workId:'work-1',viewGeneration:1};
  const state={active:true,opening:false,pending:0,synchronizing:false,projectionReady:true,ticket:1,info:pin,presented:{...pin,available:true,presentation}};
  const target=s=>({contextId:'scene-root',layerUid:'L',strokeId:s,frameScope:{kind:'authored',frame:0}});
  const segment=i=>({point:[i,10],handleIn:[0,0],handleOut:[0,0]}),outline=Array.from({length:20},(_,i)=>segment(i));
  const items=[{stableTarget:target('ink'),closed:false,count:2,matrix:[1,0,0,1,0,0],bounds:[0,10,1,10],role:'plain',filled:false},
    {stableTarget:target('ribbon'),closed:true,count:20,matrix:[1,0,0,1,0,0],bounds:[0,10,19,10],role:'ribbon',filled:false}];
  const sent=[];let serial=0;
  const reads=Reads.create({state:()=>state,settled:async()=>{},requestId:()=>`q-${++serial}`,dispatch:async q=>{
    sent.push(q);const p=q.payload;let result;const rev=state.info.contentRevision;
    if(p.view){const {limit,...view}=p.view;
      result={...view,atRevision:rev,documentSnapshotId:state.info.documentSnapshotId,total:2,nextCursor:null,preview:false,items:items.map(i=>({...i}))};
      if(mode==='unmarked')delete result.hitTest;
    }else{
      const page=p.subselect,ribbon=p.stableTarget.strokeId==='ribbon',source=page.center?outline.slice(0,18):ribbon?outline:outline.slice(0,2),total=mode==='count'&&ribbon?19:source.length;
      const end=Math.min(page.cursor+page.limit,total);
      result={atRevision:rev,documentSnapshotId:state.info.documentSnapshotId,subselect:{stableTarget:p.stableTarget,closed:ribbon&&!page.center,total,cursor:page.cursor,nextCursor:end===total?null:end,preview:false,segments:source.slice(page.cursor,end),hitTest:true}};
      if(page.center)Object.assign(result.subselect,{center:true,widths:source.slice(page.cursor,end).map((_,i)=>page.cursor+i+0.5)});
    }
    return {apiVersion:2,requestId:q.requestId,instanceId:'i',documentId:'d',contentRevision:rev,ok:true,result};
  }});
  return {read:(wanted,centres)=>reads.readPresentedStrokes({contextId:'scene-root',layerUid:'L',frame:0},state.info,wanted,centres),sent,outline,pin,presentation,state};
}
test('the hit-test listing names every record and pages geometry only for the records kept',async()=>{
  const f=fixture(),r=await f.read(item=>item.role==='plain');
  assert.deepEqual(r.items.map(i=>[i.stableTarget.strokeId,i.role,Array.isArray(i.segments)]),[['ink','plain',true],['ribbon','ribbon',false]]);
  assert.deepEqual(f.sent.map(q=>q.payload.view?['view',q.payload.view.hitTest]:['geometry',q.payload.stableTarget.strokeId,q.payload.subselect.hitTest]),
    [['view',true],['geometry','ink',true]]);
  const all=await fixture().read();
  assert.deepEqual(all.items[1].segments,f.outline,'without a filter every painted outline is paged (16 + 4)');
});
test('an unmarked listing or geometry that disagrees with it is refused',async()=>{
  await assert.rejects(fixture('unmarked').read(),undefined,'unmarked');
  await assert.rejects(fixture('count').read(),undefined,'count');
});
test('with centres a kept ribbon also carries its centreline and widths, paged like its outline',async()=>{
  const f=fixture(),r=await f.read(undefined,true);
  assert.equal(r.items[0].center,undefined,'a plain stroke has none');
  const c=r.items[1].center;
  assert.deepEqual([c.length,c[0].width,c[17].width,c[17].point],[18,0.5,17.5,[17,10]]);
  assert.deepEqual(f.sent.filter(q=>q.payload.subselect&&q.payload.subselect.center).map(q=>q.payload.subselect.cursor),[0,16]);
  assert.equal((await fixture().read()).items[1].center,undefined,'only when asked');
});

test('a repeated read of an unchanged layer (a hover) lists and pages nothing again; a new revision reads afresh',async()=>{
  const f=fixture(),wanted=item=>item.role==='plain';
  const first=await f.read(wanted),after=f.sent.length;
  assert.ok(after>0);
  const second=await f.read(wanted);
  assert.equal(f.sent.length,after,'the second read dispatches nothing');
  assert.deepEqual(second.items.map(i=>[i.stableTarget.strokeId,Array.isArray(i.segments)]),first.items.map(i=>[i.stableTarget.strokeId,Array.isArray(i.segments)]));
  await f.read(item=>item.role==='ribbon');
  const geometry=f.sent.filter(q=>q.payload.stableTarget).length;
  await f.read(item=>item.role==='ribbon');
  assert.equal(f.sent.filter(q=>q.payload.stableTarget).length,geometry,'a record paged once stays paged');
  assert.equal(f.sent.filter(q=>q.payload.view).length,1,'the listing was read exactly once');
  // A commit moves the pinned revision: the key changes and the layer is listed again.
  const next={...f.pin,contentRevision:1,documentSnapshotId:'native-object:d:1'};
  f.state.info=next;f.state.presented={...next,available:true,presentation:{...f.presentation,...next}};
  const views=f.sent.filter(q=>q.payload.view).length;
  await f.read(wanted);
  assert.equal(f.sent.filter(q=>q.payload.view).length,views+1,'the new revision lists the layer again');
});
