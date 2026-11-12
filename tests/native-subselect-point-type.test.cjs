'use strict';
// #2356 owner decision (Ilya, 2026-10-31): Smooth = collinear any lengths, Symmetric = collinear equal, Corner = broken, Collapse removes the handles.
// The controller shows the selected vertex's type, a local drag follows it, Alt keeps the opposite handle where it is, and the contract names the new kinds.
const test=require('node:test'),assert=require('node:assert/strict');
const UI=require('../src/js/adapters/native-subselect-ui.js'),Contract=require('../src/js/adapters/native-subselect-contract.js');
const {fixture}=require('./helpers/native-subselect.cjs');
const settle=()=>new Promise(setImmediate);
const q=(x,y)=>[1.5*x+.5*y,.2*x+y];
async function points(f){await f.select();f.controller.handle('down',f.event(40,24));f.controller.handle('up',f.event(40,24));await f.controller.settled();}

test('with one vertex selected the controller names its type; none, many or an unlisted type name nothing',async()=>{
  const f=fixture();f.item.pointTypes=['smooth','corner','symmetric','smooth'];await points(f);
  assert.deepEqual(f.controller.selectionState(),{count:1,pointType:'smooth'});assert.ok(Object.isFrozen(f.controller.selectionState()));
  const g=fixture();await points(g);assert.deepEqual(g.controller.selectionState(),{count:1},'an engine that lists no types');
});

test('the row marks the selected vertex type as the active button, Collapse is never a state, and retiring clears it',async()=>{
  const classes=new Map(),btn=id=>({id,disabled:false,hidden:false,style:{},classList:{toggle(c,on){classes.set(id,on?c:'');return on;},remove(){classes.set(id,'');}}});
  const buttons=['corner','smooth','symmetric','collapse'].map(t=>btn('btn-pt-'+t));
  const f=fixture(),{root}=f;f.item.pointTypes=['symmetric','corner','symmetric','smooth'];
  const listeners=[],row={hidden:false,style:{},parentNode:{insertBefore(){},removeChild(){}},contains:b=>buttons.includes(b),addEventListener(){},removeEventListener(){}};
  const container={appendChild(n){n.parentNode=container;},insertBefore(){},removeChild(){}};
  const nodes={'sp-pointtype-row':row};buttons.forEach(b=>nodes[b.id]=b);
  const get=root.document.getElementById;root.document.getElementById=id=>nodes[id]||get(id);root.document.createComment=()=>({parentNode:null});
  root.NemoNativeSubselectController=f.controller;root.NemoNativeSubselectUI=UI;
  const binding=UI.mount(root,container);assert.ok(binding,listeners.length+' mounted');
  return points(f).then(()=>{
    UI.refresh(root);
    assert.equal(classes.get('btn-pt-symmetric'),'ac');assert.ok(['','ac'].includes(classes.get('btn-pt-corner')||'')&&classes.get('btn-pt-corner')!=='ac');
    assert.notEqual(classes.get('btn-pt-collapse'),'ac');assert.notEqual(classes.get('btn-pt-smooth'),'ac');
    binding.retire();assert.ok(buttons.every(b=>classes.get(b.id)===''));
  });
});

test('the contract accepts Collapse, refuses unknown kinds, and validates the type list of a plain read page',()=>{
  const intent=pointType=>({kind:'pointType',indices:[0],pointType});
  for(const t of ['corner','smooth','symmetric','collapse'])assert.equal(Contract.intent(intent(t)),true,t);
  assert.equal(Contract.intent(intent('round')),false);
  const target={contextId:'scene-root',layerUid:'L',frameScope:{kind:'authored',frame:0},strokeId:'s'};
  const seg={point:[0,0],handleIn:[0,0],handleOut:[0,0]};
  const page=extra=>({atRevision:1,documentSnapshotId:'native-object:d:1',subselect:{stableTarget:target,closed:false,total:2,cursor:0,nextCursor:null,segments:[seg,seg],preview:false,...extra}});
  assert.doesNotThrow(()=>Contract.readResult(page({pointTypes:['corner','smooth']})));
  for(const bad of [['corner'],['corner','collapse'],['corner',1],'corner'])assert.throws(()=>Contract.readResult(page({pointTypes:bad})),/point types/,JSON.stringify(bad));
  assert.throws(()=>Contract.readResult(page({pointTypes:['corner','smooth'],hitTest:true})),/point types/);
  assert.throws(()=>Contract.readResult(page({pointTypes:['corner','smooth'],paint:true})),/Invalid subselect/);
});

// A tangent drag from vertex 0 (point 20,20, handleOut 10,0, handleIn -5,5): the endpoint Alt keeps is the mirror of where the opposite handle is.
async function paired(type){
  const f=fixture();f.item.segments[0].handleOut=[10,0];f.item.segments[0].handleIn=[-5,5];f.item.pointTypes=[type,'corner','corner','corner'];await points(f);
  f.controller.handle('down',f.event(...q(30,20)));
  f.controller.handle('move',f.event(...q(50,40)));await settle();
  f.controller.handle('move',f.event(...q(50,40),{altKey:true}));await settle();
  const intent=f.previews.at(-1);f.controller.cancel();return intent;
}
test('Alt during a tangent drag keeps the opposite handle where the vertex type left it',async()=>{
  const anchor=[20,20],drag=[30,20],old=[-5,5];
  let intent=await paired('symmetric');assert.equal(intent.breakPair,true);
  assert.deepEqual(intent.pairedPoint.map(Math.round),[50,40],'symmetric: the mirror of the drag, as before');
  intent=await paired('corner');
  assert.deepEqual(intent.pairedPoint,[anchor[0]-old[0],anchor[1]-old[1]],'corner: the opposite never moved');
  intent=await paired('smooth');
  const len=Math.hypot(...old)/Math.hypot(...drag),opposite=[-drag[0]*len,-drag[1]*len];
  assert.ok(Math.abs(intent.pairedPoint[0]-(anchor[0]-opposite[0]))<1e-9&&Math.abs(intent.pairedPoint[1]-(anchor[1]-opposite[1]))<1e-9,'smooth: on the line, its own length');
});
