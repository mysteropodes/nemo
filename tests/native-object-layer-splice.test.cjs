'use strict';
// Pure UI splice planning: no native document or Paper mutation is modeled here.
const test=require('node:test'),assert=require('node:assert/strict');
const Splice=require('../src/js/adapters/native-object-layer-splice.js');
function fixture(){
  const rows=['a','b','c'].map((layerUid,i)=>({layerUid,name:'Old '+i,visible:true,frames:[{sentinel:i}],motion:{sentinel:i}}));
  return {uids:['a','b','c'],rows,metadata:rows.map(({layerUid,name,visible})=>({layerUid,name,visible}))};
}
function apply(f,p){
  if(p.delta>0)f.rows.splice(p.at,0,p.added);
  if(p.delta<0)f.rows.splice(p.at,1);
}
test('middle native insertion preserves array and surviving row identity while remapping selection by UID',()=>{
  const f=fixture(),array=f.rows,old=f.rows.slice(),selected=[2,0],before=JSON.stringify(f);
  const next=[f.metadata[0],{layerUid:'native-layer:1:0',name:'Ink (2)',visible:false},...f.metadata.slice(1)];
  const p=Splice.plan(f.uids,f.rows,next,2,selected,0);
  assert.ok(p);assert.equal(p.delta,1);assert.equal(p.at,1);
  assert.deepEqual(p.added,{layerUid:'native-layer:1:0',name:'Ink (2)',visible:false});
  assert.equal(p.active,3);assert.deepEqual(p.selection,[3,0]);assert.equal(p.anchor,0);
  assert.equal(JSON.stringify(f),before,'planning is detached');
  assert.notStrictEqual(p.added,next[1]);apply(f,p);assert.strictEqual(f.rows,array);
  assert.strictEqual(f.rows[0],old[0]);assert.strictEqual(f.rows[2],old[1]);assert.strictEqual(f.rows[3],old[2]);
  assert.deepEqual(selected,[2,0]);assert.equal(Object.hasOwn(p.added,'frames'),false);
});
test('middle deletion follows surviving UID selection active and anchor instead of old numeric slots',()=>{
  const f=fixture(),old=f.rows.slice(),selected=[2,0];
  const p=Splice.plan(f.uids,f.rows,[f.metadata[0],f.metadata[2]],2,selected,2);
  assert.ok(p);assert.equal(p.delta,-1);assert.equal(p.at,1);
  assert.equal(p.active,1);assert.deepEqual(p.selection,[1,0]);assert.equal(p.anchor,1);
  apply(f,p);assert.deepEqual(f.rows,[old[0],old[2]]);assert.strictEqual(f.rows[1],old[2]);
});
test('unchanged layer order preserves selection reference and uses detached metadata-free plan',()=>{
  const f=fixture(),selection=[0,2];
  const p=Splice.plan(f.uids,f.rows,f.metadata,2,selection,0);
  assert.ok(p);assert.equal(p.delta,0);assert.strictEqual(p.selection,selection);
  assert.equal(p.active,2);assert.equal(p.anchor,0);
  assert.deepEqual(f.rows.map(r=>r.frames),[[{sentinel:0}],[{sentinel:1}],[{sentinel:2}]]);
});
test('head and tail insertion default names are based on native destination ordinal without payload aliases',()=>{
  for(const at of [0,3]){
    const f=fixture(),next=f.metadata.slice();next.splice(at,0,{layerUid:'new'});
    const p=Splice.plan(f.uids,f.rows,next,1,[1],1);
    assert.ok(p);assert.equal(p.at,at);assert.deepEqual(p.added,{layerUid:'new',name:'Layer '+(at+1),visible:true});
    next[at].layerUid='tampered';assert.equal(p.added.layerUid,'new');
  }
});
test('reorder replacement multiedit duplicate and mismatched prior identities refuse without mutating inputs',()=>{
  const f=fixture(),before=JSON.stringify(f);
  for(const next of [[f.metadata[1],f.metadata[0],f.metadata[2]],
    [{layerUid:'x'},f.metadata[1],f.metadata[2]],
    [f.metadata[0],{layerUid:'x'},{layerUid:'y'},...f.metadata.slice(1)],
    [f.metadata[0]], [f.metadata[0],{layerUid:'a'},f.metadata[2]],
    [f.metadata[0],{layerUid:''},f.metadata[2]], [f.metadata[0],{},f.metadata[2]],null]){
    assert.equal(Splice.plan(f.uids,f.rows,next,0,[0],0),null);
  }
  assert.equal(Splice.plan(['a','wrong','c'],f.rows,f.metadata,0,[0],0),null);
  assert.equal(JSON.stringify(f),before);
});
test('removed active and selected UID chooses surviving neighbor while unset anchor stays unset',()=>{
  const f=fixture();
  const p=Splice.plan(f.uids,f.rows,[f.metadata[0],f.metadata[2]],1,[1],-1);
  assert.ok(p);assert.equal(p.active,0);assert.deepEqual(p.selection,[0]);assert.equal(p.anchor,-1);
  for(const anchor of [null,undefined])assert.equal(Splice.plan(f.uids,f.rows,[f.metadata[0],f.metadata[2]],1,[1],anchor).anchor,anchor);
  const empty=Splice.plan(['a'],[f.rows[0]],[],0,[0],0);
  assert.ok(empty);assert.equal(empty.delta,-1);assert.deepEqual(empty.selection,[]);
});
test('consecutive cut undo and redo plans retain all surviving UI row objects',()=>{
  const f=fixture(),original=f.rows.slice(),inserted={layerUid:'cut',name:'Copy',visible:true};
  const metadata=[f.metadata[0],inserted,...f.metadata.slice(1)];
  const first=Splice.plan(f.uids,f.rows,metadata,2,[2],2);apply(f,first);const copy=f.rows[1];
  const second=Splice.plan(['a','cut','b','c'],f.rows,f.metadata,3,[3],3);apply(f,second);
  assert.equal(second.active,2);assert.deepEqual(second.selection,[2]);
  original.forEach((row,i)=>assert.strictEqual(f.rows[i],row));
  const third=Splice.plan(f.uids,f.rows,metadata,2,[2],2);apply(f,third);
  assert.notStrictEqual(f.rows[1],copy,'redo creates a fresh detached UI projection for the returned UID');
  assert.strictEqual(f.rows[0],original[0]);assert.strictEqual(f.rows[2],original[1]);assert.strictEqual(f.rows[3],original[2]);
});
