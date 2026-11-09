'use strict';
const test=require('node:test'),assert=require('node:assert/strict');
const View=require('../src/js/adapters/native-motion-property-view.js');
const fixture=require('../engineering/application/examples/native-property-metadata-v1/cases.json');
const clone=structuredClone;
function context(q){const pin={instanceId:q.instanceId,documentId:q.documentId,contentRevision:q.atRevision,documentSnapshotId:q.documentSnapshotId,lifecycleGeneration:1};return {pin,frame:0,viewGeneration:1,presented:{...pin,available:true,presentation:{...pin,owner:'native',status:'presented',frame:0}}};}
for(const c of fixture.cases.filter(c=>c.expectedResult))test('literal metadata fixture: '+c.name,()=>{
 const decoded=View.decodePage(c.request,c.expectedResult,context(c.request));assert.equal(decoded.available,true,c.name);assert.equal(decoded.readOnly,true);assert.ok(Object.isFrozen(decoded));
 assert.deepEqual(decoded.items,c.expectedResult.items);assert.equal(decoded.nextOffset,c.expectedResult.nextOffset);
});
const positive=fixture.cases.find(c=>c.expectedResult),q=positive.request;
test('selector refuses UI lifecycle, frame and raw SDK envelope carriers',()=>{for(const field of ['frame','lifecycleGeneration','presentation'])assert.equal(View.decodePage({...q,[field]:1},positive.expectedResult,context(q)).available,false);assert.equal(View.decodePage(q,{ok:true,result:positive.expectedResult},context(q)).available,false);});
test('outer and final presentation identities and current frame must all match',()=>{for(const field of ['instanceId','documentId','contentRevision','documentSnapshotId','lifecycleGeneration'])for(const place of ['outer','inner']){const c=context(q);(place==='outer'?c.presented:c.presented.presentation)[field]='foreign';assert.equal(View.decodePage(q,positive.expectedResult,c).available,false);}const c=context(q);c.frame=1;assert.equal(View.decodePage(q,positive.expectedResult,c).available,false);});
test('foreign result, duplicate item and bad continuation cannot become accepted pages',()=>{for(const change of [r=>r.documentId='foreign',r=>r.items.push(clone(r.items[0])),r=>r.nextOffset=999]){const r=clone(positive.expectedResult);change(r);assert.equal(View.decodePage(q,r,context(q)).available,false);}});
test('authored facts distinguish explicit default-valued static overrides from absent state',()=>{const row=clone(fixture.propertyPageOracle[0]);row.authored.hasStaticOverride=true;assert.equal(View.filterRow(row,'modified').matches,true);row.authored.hasStaticOverride=false;assert.equal(View.filterRow(row,'modified').matches,false);row.authored.expressionEnabled=true;assert.equal(View.filterRow(row,'expressions').matches,true);for(const mode of ['effects','errors','mattes'])assert.equal(View.filterRow(row,mode).available,false);});
test('pending reads are fenced after replacement and disposal',async()=>{let c=context(q),release;const client=View.createConsumer({readContext:()=>c,query:()=>new Promise(r=>release=r)});const pending=client.load(q.view);c=clone(c);c.viewGeneration++;release(positive.expectedResult);assert.equal((await pending).available,false);const next=client.load(q.view);client.dispose();release(positive.expectedResult);assert.equal((await next).available,false);});
test('vertex discovery is an explicit opt-in view and its continuation keeps it (C-motion-canvas-4 Path group)',async()=>{
  const pin={instanceId:'i',documentId:'d',contentRevision:4,documentSnapshotId:'s',lifecycleGeneration:1};
  const ctx={pin,frame:0,viewGeneration:1,presented:{...pin,available:true,presentation:{...pin,owner:'native',status:'presented',frame:0}}};
  const target={contextId:'scene-root',layerUid:'L',strokeId:'S'},row=clone(fixture.propertyPageOracle[0]),seen=[];
  const consumer=View.createConsumer({readContext:()=>ctx,query:async q=>{seen.push(q);return {instanceId:'i',documentId:'d',atRevision:4,documentSnapshotId:'s',kind:'properties',target:clone(target),items:[clone(row)],
    nextOffset:q.view.offset+1,unavailableCategories:[{category:'propertyWrites',state:'unavailable',reason:'x'}]};}});
  const page=await consumer.load({kind:'properties',target,offset:12,limit:8,includeVertices:true});
  assert.equal(page.available,true,page.reason);assert.equal(page.includeVertices,true);assert.deepEqual(seen[0].view,{kind:'properties',target,offset:12,limit:8,includeVertices:true});
  assert.deepEqual(View.continuation(page),{kind:'properties',target,offset:13,limit:8,includeVertices:true});
  const plain=await View.createConsumer({readContext:()=>ctx,query:async q=>({instanceId:'i',documentId:'d',atRevision:4,documentSnapshotId:'s',kind:'properties',target:clone(target),items:[],nextOffset:null,unavailableCategories:[{category:'propertyWrites',state:'unavailable',reason:'x'}]})}).load({kind:'properties',target,offset:0,limit:8});
  assert.equal(plain.includeVertices,false);
  for(const bad of [{includeVertices:false},{includeVertices:'yes'}])assert.equal((await consumer.load({kind:'properties',target,offset:0,limit:8,...bad})).available,false,JSON.stringify(bad));
});
