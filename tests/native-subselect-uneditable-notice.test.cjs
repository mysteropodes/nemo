'use strict';
// #2356: a Direct Select click on a record that has no editable points (a brush/pressure stroke the editable listing omits) says so.
const test=require('node:test'),assert=require('node:assert/strict');
const {fixture}=require('./helpers/native-subselect.cjs');
function withBrush(f,reads){
  const brush={stableTarget:{...f.item.stableTarget,strokeId:'brush'},role:'ribbon',bounds:[100,100,150,150],matrix:[1,0,0,1,0,0],strokeWidth:0,closed:false,count:19};
  const plain={...f.item,role:'plain',bounds:[20,20,80,80]};
  f.s.readPresentedStrokes=async(selector,pin,wanted)=>{reads.push({selector,wanted:wanted()});return {info:pin,presentation:f.s.readPresentedObjectSession().presentation,items:[plain,brush]};};
}
async function click(f,x,y){f.controller.handle('down',f.event(x,y));await f.controller.settled();f.controller.handle('up',f.event(x,y));return f.controller.settled();}
test('a Direct Select click on a non-editable stroke gives one honest notice and selects nothing',async()=>{
  const f=fixture(),reads=[];withBrush(f,reads);
  const result=await click(f,120,120);
  assert.equal(result.status,'cleared');
  assert.deepEqual(f.notices,['This stroke has no editable points.']);
  assert.equal(reads.length,1,'one hit-test read, no geometry paged');assert.equal(reads[0].wanted,false);
  assert.equal(f.sent.length,0,'the legacy writer and the native owner are never written');
});
test('a click on empty canvas, or an editable shape, gives no notice',async()=>{
  const f=fixture(),reads=[];withBrush(f,reads);
  await click(f,190,190);assert.deepEqual(f.notices,[]);
  const g=fixture();g.s.readPresentedStrokes=async()=>{throw new Error('read failed');};
  const r=await click(g,120,120);assert.equal(r.status,'cleared');assert.deepEqual(g.notices,[],'a failed courtesy read stays silent');
  const h=fixture();assert.equal((await click(h,120,120)).status,'cleared');assert.deepEqual(h.notices,[]);
});
test('a click that selects an editable item does not read the hit-test listing',async()=>{
  const f=fixture(),reads=[];withBrush(f,reads);
  await f.select();assert.equal(reads.length,0);assert.deepEqual(f.notices,[]);
});
