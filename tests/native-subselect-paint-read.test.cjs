'use strict';
const test=require('node:test'),assert=require('node:assert/strict');
const Reads=require('../src/js/adapters/native-object-project-reads.js');
function fixture(mode){
  const pin={instanceId:'i',documentId:'d',lifecycleGeneration:1,contentRevision:0,documentSnapshotId:'native-object:d:0'};
  const presentation={...pin,owner:'native',status:'presented',contextId:'scene-root',frame:0,workId:'work-1',viewGeneration:1};
  const state={active:true,opening:false,pending:0,synchronizing:false,projectionReady:true,ticket:1,info:pin,presented:{...pin,available:true,presentation}};
  const target={contextId:'scene-root',layerUid:'L',strokeId:'s',frameScope:{kind:'authored',frame:0}};
  const segment=i=>({point:[i,10],handleIn:[0,0],handleOut:[0,0]}),center=[segment(0),segment(1)],paint=Array.from({length:20},(_,i)=>segment(i));
  const sent=[];let serial=0;
  const reads=Reads.create({state:()=>state,settled:async()=>{},requestId:()=>`q-${++serial}`,dispatch:async q=>{
    sent.push(q);const p=q.payload;let result;
    if(p.view){const {limit,...view}=p.view;
      const item={stableTarget:target,closed:false,count:2,matrix:[1,0,0,1,0,0]};
      if(mode!=='plain'&&mode!=='types')item.paintCount=mode==='count'?21:20;
      result={...view,atRevision:0,documentSnapshotId:pin.documentSnapshotId,total:1,nextCursor:null,preview:false,items:[item]};
    }else{
      const page=p.subselect,total=page.paint?(mode==='total'&&page.cursor?19:20):2,source=page.paint?paint:center;
      const end=Math.min(page.cursor+page.limit,total);
      result={atRevision:0,documentSnapshotId:pin.documentSnapshotId,subselect:{stableTarget:target,closed:!!page.paint,total,cursor:page.cursor,nextCursor:end===total?null:end,preview:false,segments:source.slice(page.cursor,end)}};
      if(mode==='types'&&!page.paint)result.subselect.pointTypes=['corner','smooth'].slice(page.cursor,end);
      if(page.paint){result.subselect.paint=true;
        if(mode==='missing-marker')delete result.subselect.paint;
        if(mode==='open')result.subselect.closed=false;
        if(mode==='origin'&&page.cursor)result.documentSnapshotId='native-object:other:0';
        if(mode==='stale'&&page.cursor)state.presented={...state.presented,presentation:{...presentation,workId:'work-2'}};
      }
    }
    return {apiVersion:2,requestId:q.requestId,instanceId:'i',documentId:'d',contentRevision:0,ok:true,result};
  }});
  return {read:()=>reads.readSubselect({contextId:'scene-root',layerUid:'L',frame:0},pin),sent,center,paint};
}
test('pressure reads join separately paged painted geometry without replacing editable centers',async()=>{
  const f=fixture(),r=await f.read(),item=r.items[0];
  assert.deepEqual(item.segments,f.center);assert.deepEqual(item.paint,{closed:true,segments:f.paint});
  assert.equal(item.closed,false);assert.equal(item.count,2);assert.equal(item.paintCount,20);
  assert.equal(Object.isFrozen(item.paint.segments),true);
  assert.deepEqual(f.sent.map(q=>q.payload.subselect&&[q.payload.subselect.cursor,q.payload.subselect.paint]),[undefined,[0,undefined],[0,true],[16,true]]);
});
test('ordinary subselect responses keep their existing shape and require no paint query',async()=>{
  const f=fixture('plain'),r=await f.read();assert.equal(f.sent.length,2);assert.equal(Object.hasOwn(r.items[0],'paint'),false);assert.equal(Object.hasOwn(r.items[0],'paintCount'),false);
});
test('paint paging refuses mismatched marker, closedness, total, presentation count and read origin',async()=>{
  for(const mode of ['missing-marker','open','total','count','origin','stale']){
    const f=fixture(mode);await assert.rejects(f.read(),undefined,mode);
    assert.ok(f.sent.every(q=>q.operation==='query.document.object'),mode);
  }
});
test('#2356: the plain read carries the type each vertex shows onto the listed item, and a page without types adds none',async()=>{
  const typed=await fixture('types').read();assert.deepEqual(typed.items[0].pointTypes,['corner','smooth']);assert.equal(Object.hasOwn(typed.items[0],'paint'),false);
  const plain=await fixture('plain').read();assert.equal(Object.hasOwn(plain.items[0],'pointTypes'),false);
});
