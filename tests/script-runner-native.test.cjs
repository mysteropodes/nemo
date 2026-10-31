'use strict';
// CS8-SDK (#2354): the real SMScript.run on the desktop host never reads or writes state.layers.
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const SOURCE=fs.readFileSync(path.join(__dirname,'../src/js/nemo-script.js'),'utf8'),Native=require('../src/js/application/script-native-api.js');
function boot({session,tauri=true,native=true,writes=null,extra=null,controller=undefined}){
  const calls=[],toasts=[];
  const state={};Object.defineProperty(state,'layers',{get(){throw Error('LEGACY state.layers read');}});
  let info=session&&{instanceId:'i',documentId:'d',lifecycleGeneration:1,contentRevision:0,fps:24,canvasW:8,canvasH:8,totalFrames:4,
    layers:[{layerUid:'a',name:'A',visible:true},{layerUid:'b',name:'B',visible:true}]};
  const s=session&&{blocksLegacy:()=>true,isActive:()=>true,info:()=>info,...(extra||{})};
  const context=vm.createContext({console,state,NemoScriptNativeWrites:writes?{create:(root,s)=>writes(()=>info,v=>{info=v;},s)}:undefined,_layerSel:[0],NemoScriptNativeApi:native?Native:undefined,NemoNativeLayerPlayheadCutUI:{scripting:()=>function(){}},goToFrame(){},showToast:m=>toasts.push(m),
    __TAURI__:tauri?{core:{invoke(){}}}:undefined,NemoNativeObjectProject:{desktop:()=>s||null},NemoNativeCompositionController:controller,
    SM:{toggleLayerVis(index){calls.push(['vis',index]);info={...info,layers:info.layers.map((l,i)=>i===index?{...l,visible:!l.visible}:l)};return Promise.resolve(true);},
      toggleLayerLock(){calls.push(['lock']);return Promise.resolve(true);},toggleLayerShy(){calls.push(['shy']);return Promise.resolve(true);},t:k=>k}});
  context.window=context;vm.runInContext(SOURCE,context,{filename:'nemo-script.js'});
  return {SMScript:context.SMScript,calls,toasts,get info(){return info;}};
}

test('desktop without a native document refuses before building any legacy API',()=>{
  const h=boot({session:false});
  const r=h.SMScript.run('nemo.layer(0).visible=false');
  assert.equal(r.ok,false);assert.equal(r.error,'Scripts run on an open native document only.');assert.equal(r.log.length,0);
  assert.deepEqual(h.calls,[]);
});

test('a native script reads the pinned model and commits its writes through the ordinary UI entries',async()=>{
  const h=boot({session:true});
  const r=h.SMScript.run('var a=nemo.layer("B"); a.visible=false; nemo.toast("done"); a.visible+"/"+nemo.layerCount');
  assert.equal(r.ok,true);assert.equal(r.value,'false/2');assert.deepEqual([...r.log],['toast']);
  assert.deepEqual(h.calls,[],'no write before the script returns');
  assert.deepEqual({...(await r.committed)},{ok:true,applied:1,pending:0});
  assert.deepEqual(h.calls,[['vis',1]]);assert.equal(h.info.layers[1].visible,false);
  const explicit=h.SMScript.run('return nemo.layers().map(function(l){return l.name;}).join(",")');
  assert.equal(explicit.value,'A,B');
});

test('an unavailable row fails the run with a typed code and sends nothing already queued',async()=>{
  const h=boot({session:true});
  const r=h.SMScript.run('nemo.layer(0).visible=false; nemo.layer(0).blendMode="add"');
  assert.equal(r.ok,false);assert.equal(r.code,'unavailable');assert.match(r.error,/Layer\.blendMode is unavailable on a native document \(no-native-blend-matte\)/);
  assert.equal(r.committed,undefined);assert.deepEqual(h.calls,[]);
});

test('the desktop dispatch fails closed when the native script module is not loaded',()=>{
  const r=boot({session:true,native:false}).SMScript.run('nemo.layer(0).visible=false');
  assert.equal(r.ok,false);assert.equal(r.error,'Scripts run on an open native document only.');
});

test('common page globals are shadowed inside the script (hygiene, not a sandbox)',()=>{
  const h=boot({session:true});
  const r=h.SMScript.run('[typeof state, typeof SM, typeof SMScript, typeof window, typeof globalThis].join(",")');
  assert.equal(r.ok,true);assert.equal(r.value,'undefined,undefined,undefined,undefined,undefined');
});

// A native owner double behind the write entries: it mutates the read model the way the owner would and records each call.
function owned(){
  const sent=[];let minted=0;
  const writes=(get,set)=>{
    const bump=fn=>{const i=get();set(fn({...i,contentRevision:i.contentRevision+1}));};
    return {
      add:async()=>{sent.push(['add']);const uid='n'+(++minted);bump(i=>({...i,layers:[...i.layers,{layerUid:uid,name:'Layer '+(i.layers.length+1),visible:true}]}));return {ok:true,applied:true,layerUid:uid};},
      remove:async(uid)=>{sent.push(['remove',uid]);bump(i=>({...i,layers:i.layers.filter(l=>l.layerUid!==uid)}));return {ok:true,applied:true};},
      rename:async(uid,name)=>{sent.push(['rename',uid,name]);bump(i=>({...i,layers:i.layers.map(l=>l.layerUid===uid?{...l,name}:l)}));return {ok:true,applied:true};},
      marker:async(spec)=>{sent.push(['marker',spec.action,spec.frame,spec.name]);bump(i=>i);return {ok:true,applied:true};},
    };
  };
  return {sent,writes};
}

test('a native script queues structural and marker writes with read-your-writes and commits them in order through the owner entries',async()=>{
  const o=owned(),h=boot({session:true,writes:o.writes});
  const r=h.SMScript.run('var ink=nemo.addLayer("Ink"); ink.visible=false; nemo.layer("A").remove(); nemo.markers.add(2,"Beat"); [nemo.layerCount,ink.name,ink.index,nemo.layer("B").index].join("/")');
  assert.equal(r.ok,true);assert.equal(r.value,'2/Ink/1/0','the script reads its own queued structure');
  assert.deepEqual(o.sent,[],'no write before the script returns');
  assert.deepEqual({...(await r.committed)},{ok:true,applied:5,pending:0});
  assert.deepEqual(o.sent,[['add'],['rename','n1','Ink'],['remove','a'],['marker','add',2,'Beat']]);
  assert.deepEqual(h.calls,[['vis',2]],'the flag write reached the layer the owner minted, at its index after the add');
  assert.deepEqual(h.info.layers.map(l=>l.name),['B','Ink']);
});

test('a script that fails after queuing structural writes sends nothing, and never reads state.layers',async()=>{
  const o=owned(),h=boot({session:true,writes:o.writes});
  const r=h.SMScript.run('nemo.addLayer("x"); nemo.layer("A").remove(); nemo.markers.add(1); nemo.layer(0).blendMode="add"');
  assert.equal(r.ok,false);assert.equal(r.code,'unavailable');assert.equal(r.committed,undefined);
  assert.deepEqual(o.sent,[]);assert.deepEqual(h.calls,[]);
  const thrown=h.SMScript.run('nemo.layer("A").remove(); throw new Error("boom")');
  assert.equal(thrown.ok,false);assert.match(thrown.error,/boom/);assert.deepEqual(o.sent,[]);
});

test('without the native write entries the structural rows are refused at the call and the run sends nothing',()=>{
  const h=boot({session:true});
  const r=h.SMScript.run('nemo.layer(0).visible=false; nemo.addLayer("x")');
  assert.equal(r.ok,false);assert.equal(r.code,'unavailable');assert.match(r.error,/addLayer is unavailable on a native document \(native-entry-absent\)/);
  assert.deepEqual(h.calls,[]);
});

test('the real run answers a track, timing and composition read through the session read entries, as a Promise, after pinned rounds',async()=>{
  const asked=[];
  const extra={
    readProperty:async selector=>{asked.push(selector);assert.deepEqual(Object.keys(selector).sort(),['atRevision','documentId','documentSnapshotId','instanceId','view']);
      return selector.view.kind==='rawSample'?{value:[selector.view.frame*2]}:{items:[{frame:0,v:[1]},{frame:3,v:[7],hold:true}],nextOffset:null};},
    readLayerCells:async (uid,pin)=>{asked.push(['cells',uid,pin.instanceId]);return [[0,2,'key',0,true,false],[2,2,'held',0,true,false]];}};
  const controller={read:()=>({info:{instanceId:'i',documentId:'d',lifecycleGeneration:1,contentRevision:0,documentSnapshotId:undefined},composition:{markers:[{frame:2,name:'M'}],effectiveBpm:120,effectiveBpmOffset:0,effectiveBpmShow:true}})};
  const h=boot({session:true,extra,controller});
  const r=h.SMScript.run('var a=nemo.layer("A"); [a.get("opacity",3), a.keys("opacity").map(function(k){return k.frame+":"+k.value+":"+k.hold;}).join(","), nemo.isKeyframe("A",0), nemo.isKeyframe("A",2), nemo.markers.list()[0].name, nemo.comp.beats().join("/")]');
  assert.equal(typeof r.then,'function','a script that reads tracks answers with a Promise');
  const done=await r;assert.equal(done.ok,true,done.error);
  assert.deepEqual([...done.value].map(v=>Array.isArray(v)?[...v]:v),[[6],'0:1:false,3:7:true',true,false,'M','0']);
  assert.equal(asked.filter(a=>a.view&&a.view.kind==='rawSample').length,1,'one sample, fetched once across the rounds');
  const noController=await boot({session:true,extra}).SMScript.run('nemo.markers.list()');
  assert.equal(noController.ok,false);assert.equal(noController.code,'unavailable');assert.match(noController.error,/composition-unavailable/);
  const none=await boot({session:true}).SMScript.run('nemo.layer("A").keys("opacity")');
  assert.equal(none.ok,false);assert.match(none.error,/native-entry-absent/);
  assert.deepEqual(h.calls,[],'reads never write');
});
