'use strict';
// CS8-SDK (#2354): the native script API reads a pinned model, queues writes, drains them
// through the ordinary UI entry points, and refuses unbound rows before any write.
const test=require('node:test'),assert=require('node:assert/strict');
const Api=require('../src/js/application/script-native-api.js');
function harness(layers){
  let info={instanceId:'i',documentId:'d',lifecycleGeneration:1,contentRevision:0,fps:24,canvasW:320,canvasH:180,totalFrames:12,layers:layers.map(l=>({...l}))};
  const calls=[],frames=[],toasts=[];let refuse=null;
  const write={};
  for(const key of ['visible','locked','shy'])write[key]=index=>{calls.push([key,index]);if(refuse===key)return Promise.resolve(false);
    const l=info.layers[index];info={...info,contentRevision:info.contentRevision+1,layers:info.layers.map((x,i)=>i===index?{...x,[key]:key==='visible'?x.visible===false:!(x[key]===true)}:x)};
    void l;return Promise.resolve(true);};
  const ports={info:()=>info,write,goToFrame:f=>frames.push(f),selection:()=>[1],toast:m=>toasts.push(m)};
  return {ports,calls,frames,toasts,get info(){return info;},set info(v){info=v;},refuse(k){refuse=k;}};
}
const L=[{layerUid:'a',name:'A',visible:true},{layerUid:'b',name:'B',visible:false,locked:true},{layerUid:'c',visible:true,shy:true}];

test('reads are detached, layer handles follow layerUid, and the vocabulary is preserved',()=>{
  const h=harness(L),run=Api.createRun(h.ports),n=run.nemo;
  assert.equal(n.layerCount,3);assert.equal(n.fps,24);assert.equal(n.frameCount,12);
  assert.deepEqual(n.layers().map(l=>[l.name,l.visible,l.locked,l.shy]),[['A',true,false,false],['B',false,true,false],['Layer 3',true,false,true]]);
  assert.equal(n.layer('B').layerUid,'b');assert.equal(n.layer(2).layerUid,'c');assert.equal(n.activeLayer().layerUid,'b');
  assert.deepEqual(n.selection.layers().map(l=>l.layerUid),['b']);
  h.info.layers[0].name='mutated after the run started';assert.equal(n.layer(0).name,'A');
  n.goToFrame(5);n.toast('hi');assert.deepEqual(h.frames,[5]);assert.deepEqual(h.toasts,['hi']);assert.deepEqual(n.log(),['toast']);
  assert.ok(n.help().includes('Layer.visible'));
  assert.throws(()=>n.layer('missing'),/no layer/);
});

test('writes are queued with read-your-writes, cancel out, and drain in order through the UI entries',async()=>{
  const h=harness(L),run=Api.createRun(h.ports),n=run.nemo;
  const a=n.layer('A'),b=n.layer('B');
  a.visible=false;assert.equal(a.visible,false);a.visible=true;a.visible=true;
  b.locked=false;b.visible=true;n.layer('c').shy=false;
  assert.deepEqual(h.calls,[],'nothing is written while the script runs');
  assert.deepEqual(await run.commit(),{ok:true,applied:3,pending:0});
  assert.deepEqual(h.calls,[['locked',1],['visible',1],['shy',2]]);
  assert.equal(h.info.layers[1].locked,false);assert.equal(h.info.layers[1].visible,true);assert.equal(h.info.layers[2].shy,false);
});

test('unbound and unavailable rows refuse with a typed error and never fall back',()=>{
  const n=Api.createRun(harness(L).ports).nemo,layer=n.layer(0);
  const typed=(fn,row,reason)=>assert.throws(fn,e=>e.code==='unavailable'&&e.row===row&&e.reason===reason);
  typed(()=>layer.blendMode,'sdk:nemo-script.Layer.blendMode','no-native-blend-matte');
  typed(()=>{layer.blendMode='add';},'sdk:nemo-script.Layer.blendMode','no-native-blend-matte');
  typed(()=>{layer.name='x';},'sdk:nemo-script.Layer.name','not-yet-bound');
  typed(()=>layer.duplicate,'sdk:nemo-script.Layer.duplicate','not-yet-bound');
  typed(()=>n.addLayer('x'),'sdk:nemo-script.addLayer','not-yet-bound');
  typed(()=>n.undoGroup(()=>{}),'sdk:nemo-script.undoGroup','script-run-grouping');
  typed(()=>n.markers.add(1),'sdk:nemo-script.markers.add','not-yet-bound');
  typed(()=>n.camera.keys(),'sdk:nemo-script.camera.keys','no-native-camera');
  typed(()=>n.comp.setWorkArea(0,1),'sdk:nemo-script.comp.setWorkArea','no-native-work-area');
});

test('a replaced document, a removed layer or a refused write stops the drain (mutation-policy negative controls)',async()=>{
  let h=harness(L),run=Api.createRun(h.ports);run.nemo.layer(0).visible=false;
  h.info={...h.info,documentId:'other'};
  assert.deepEqual(await run.commit(),{ok:false,applied:0,pending:1,error:'document-replaced'});assert.deepEqual(h.calls,[]);
  h=harness(L);run=Api.createRun(h.ports);run.nemo.layer(0).visible=false;h.info={...h.info,lifecycleGeneration:2};
  assert.equal((await run.commit()).error,'document-replaced');
  h=harness(L);run=Api.createRun(h.ports);run.nemo.layer('A').visible=false;h.info={...h.info,layers:h.info.layers.slice(1)};
  assert.equal((await run.commit()).error,'layer-gone');assert.deepEqual(h.calls,[]);
  h=harness(L);run=Api.createRun(h.ports);run.nemo.layer('B').locked=false;run.nemo.layer('A').visible=false;h.refuse('locked');
  assert.deepEqual(await run.commit(),{ok:false,applied:0,pending:2,error:'write-refused'});assert.deepEqual(h.calls,[['locked',1]]);
  h=harness(L);run=Api.createRun(h.ports);run.nemo.layer('A').visible=false;
  h.info={...h.info,layers:[h.info.layers[2],h.info.layers[0],h.info.layers[1]]};
  assert.deepEqual(await run.commit(),{ok:true,applied:1,pending:0});assert.deepEqual(h.calls,[['visible',1]],'resolved by layerUid after a reorder');
  h=harness(L);run=Api.createRun(h.ports);run.nemo.layer('A').visible=false;run.discard();
  assert.deepEqual(await run.commit(),{ok:true,applied:0,pending:0});assert.deepEqual(h.calls,[]);
  assert.throws(()=>Api.createRun({...h.ports,info:()=>null}),/no native document/);
});

test('undeclared members throw, frame routes to presentation, and the active layer is the active index',()=>{
  const h=harness(L),n=Api.createRun({...h.ports,frame:()=>3,activeIndex:()=>2}).nemo;
  const undeclared=(fn,pattern)=>assert.throws(fn,e=>e.code==='unavailable'&&e.reason==='undeclared'&&pattern.test(e.message));
  undeclared(()=>n.backgrond,/backgrond is not part/);undeclared(()=>{n.typo=1;},/typo/);
  undeclared(()=>n.layer(0).opacityy,/Layer\.opacityy/);undeclared(()=>{n.layer(0).custom=1;},/Layer\.custom/);
  undeclared(()=>n.comp.workAreaa,/comp\.workAreaa/);undeclared(()=>n.selection.everything(),/selection\.everything/);
  assert.equal(n.frame,3);n.frame=7;assert.deepEqual(h.frames,[7]);
  assert.equal(n.activeLayer().layerUid,'c');assert.equal(typeof n.then,'undefined','a returned API is not mistaken for a thenable');
  assert.equal(n.layer(0).name,'A');assert.ok(n.help().includes('Layer.name'));
  assert.throws(()=>{n.layer(0).name='x';},e=>e.code==='unavailable'&&e.reason==='not-yet-bound');
  assert.throws(()=>Api.createRun(harness([{layerUid:'x'},{layerUid:'x'}]).ports),/duplicate layer id x/);
});

test('a write the native read model does not confirm is reported, not counted',async()=>{
  const h=harness(L),run=Api.createRun({...h.ports,write:{...h.ports.write,visible:()=>Promise.resolve(true)}});
  run.nemo.layer('A').visible=false;
  assert.deepEqual(await run.commit(),{ok:false,applied:0,pending:1,error:'write-unconfirmed'});
});
