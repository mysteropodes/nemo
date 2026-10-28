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
test('reorder noncontiguous duplicate and mismatched prior identities refuse without mutating inputs',()=>{
  const f=fixture(),before=JSON.stringify(f);
  for(const next of [[f.metadata[1],f.metadata[0],f.metadata[2]],
    [{layerUid:'x'},f.metadata[0],f.metadata[2]],
    [{layerUid:'x'},f.metadata[0],{layerUid:'y'},...f.metadata.slice(1)],
    [f.metadata[1]], [f.metadata[0],{layerUid:'a'},f.metadata[2]],
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

test('contiguous multirow insertion and inverse removal preserve UID identity at head middle and tail',()=>{
  for(const at of [0,1,3]){
    const f=fixture(),original=f.rows.slice(),array=f.rows,selection=[2,0];
    const incoming=f.metadata.slice();incoming.splice(at,0,{layerUid:'piece-a',name:'Forme 1',visible:false},{layerUid:'piece-b'});
    const before=JSON.stringify(f),plan=Splice.plan(f.uids,f.rows,incoming,2,selection,0);
    assert.ok(plan);assert.equal(plan.delta,2);assert.equal(plan.at,at);
    assert.deepEqual(plan.addedRows,[{layerUid:'piece-a',name:'Forme 1',visible:false},
      {layerUid:'piece-b',name:'Layer '+(at+2),visible:true}]);
    assert.equal(JSON.stringify(f),before,'planning remains detached');
    assert.notStrictEqual(plan.addedRows[0],incoming[at]);
    f.rows.splice(plan.at,0,...plan.addedRows);
    assert.strictEqual(f.rows,array);
    for(const row of original)assert.strictEqual(f.rows.find(r=>r.layerUid===row.layerUid),row);
    assert.deepEqual(plan.selection,[incoming.findIndex(r=>r.layerUid==='c'),incoming.findIndex(r=>r.layerUid==='a')]);
    assert.equal(plan.active,incoming.findIndex(r=>r.layerUid==='c'));assert.equal(plan.anchor,incoming.findIndex(r=>r.layerUid==='a'));
    const undo=Splice.plan(incoming.map(r=>r.layerUid),f.rows,f.metadata,plan.active,plan.selection,plan.anchor);
    assert.ok(undo);assert.equal(undo.delta,-2);assert.equal(undo.at,at);
    f.rows.splice(undo.at,-undo.delta);assert.deepEqual(f.rows,original);
    assert.deepEqual(undo.selection,selection);assert.equal(undo.active,2);assert.equal(undo.anchor,0);
  }
});
test('multirow removal maps removed selection and anchor to a surviving UID neighbor',()=>{
  const f=fixture(),p=Splice.plan(f.uids,f.rows,[f.metadata[2]],1,[0,1],1);
  assert.ok(p);assert.equal(p.delta,-2);assert.equal(p.active,0);assert.deepEqual(p.selection,[0]);assert.equal(p.anchor,0);
  const r=Splice.plan(f.uids,f.rows,[{layerUid:'x'},f.metadata[2]],0,[0],0);
  assert.equal(r.removed,2);assert.deepEqual(r.addedRows.map(row=>row.layerUid),['x'],'a replacement is planned as one, never as a bare removal');
});

test('merge convert replaces its contiguous source rows with the minted instance and selects it (#2244)',()=>{
  for(const [uids,at] of [[['a','b'],0],[['b','c'],1],[['a','b','c'],0]]){
    const f=fixture(),old=f.rows.slice(),instance={layerUid:'native-layer:9:0',name:'Component 1',visible:true};
    const next=[...f.metadata.slice(0,at),instance,...f.metadata.slice(at+uids.length)];
    const p=Splice.plan(f.uids,f.rows,next,at,uids.map((_,i)=>at+i),at);
    assert.ok(p);assert.equal(p.at,at);assert.equal(p.removed,uids.length);assert.deepEqual(p.addedRows,[instance]);
    assert.equal(p.active,at);assert.deepEqual(p.selection,[at]);assert.equal(p.anchor,at);
    f.rows.splice(p.at,p.removed,...p.addedRows);
    assert.deepEqual(f.rows.map(r=>r.layerUid),next.map(r=>r.layerUid));
    f.rows.forEach((row,i)=>{if(row.layerUid!==instance.layerUid)assert.strictEqual(row,old[f.uids.indexOf(row.layerUid)]);});
    const undo=Splice.plan(next.map(r=>r.layerUid),f.rows,f.metadata,at,[at],at);
    assert.ok(undo);assert.equal(undo.removed,1);assert.deepEqual(undo.addedRows.map(r=>r.layerUid),uids);
    assert.equal(undo.active,at);assert.deepEqual(undo.selection,[at]);
  }
});
test('a surviving selection outside the merged run keeps its UID, and a run that re-adds an old UID refuses',()=>{
  const f=fixture(),next=[{layerUid:'m'},f.metadata[2]];
  const p=Splice.plan(f.uids,f.rows,next,2,[2],2);
  assert.ok(p);assert.equal(p.active,1);assert.deepEqual(p.selection,[1]);assert.equal(p.anchor,1);
  assert.deepEqual(p.added,{layerUid:'m',name:'Layer 1',visible:true});
  assert.equal(Splice.plan(f.uids,f.rows,[{layerUid:'m'},f.metadata[0]],0,[0],0),null);
});
