'use strict';
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),vm=require('node:vm');
const Intent=require('../src/js/adapters/native-project-settings-intent.js'),Desktop=require('../src/js/adapters/native-project-settings-desktop.js');
function fixture(){
 let info={instanceId:'host',documentId:'doc',lifecycleGeneration:1,contentRevision:0,documentSnapshotId:'s0',
   canvasW:320,canvasH:180,fps:24,totalFrames:120,layers:[{layerUid:'opaque old'}]},current,ready=true,gate=null,fault=null;
 const calls=[],root={state:{currentFrame:7,layers:info.layers},_layerSel:[0],NemoNativeProjectSettingsIntent:Intent,notices:[]};
 const session={info:()=>info,isActive:()=>true,
  readPresentedObjectSession:()=>({...info,available:true,presentation:{...info,owner:'native',status:fault==='preview'?'refused':'presented',frame:root.state.currentFrame}}),
  async editProjectSettings(action,pin){assert.strictEqual(pin,info);calls.push(action);if(gate)await gate;
   if(fault==='refuse')return {apiVersion:2,ok:false,instanceId:info.instanceId,documentId:info.documentId,contentRevision:info.contentRevision,error:{code:'unavailable'}};
   const next={...info};if(action.kind==='setCanvasSize'){next.canvasW=action.width;next.canvasH=action.height;}
   if(action.kind==='setFps')next.fps=Math.max(1,Math.min(60,action.fps));
   if(action.kind==='setCanvasBackground')next.canvasBg=action.color;
   if(action.kind==='setWorkArea'){next.waIn=action.inFrame;next.waOut=action.outFrame;}
   const applied=JSON.stringify(next)!==JSON.stringify(info);next.contentRevision+=applied?1:0;next.documentSnapshotId='s'+next.contentRevision;info=next;
   return {apiVersion:2,ok:true,instanceId:info.instanceId,documentId:info.documentId,contentRevision:info.contentRevision,
     result:{applied,historyEntriesAdded:fault==='history'?9:applied?1:0}};}};
 current=session;root.NemoNativeObjectProject={desktop:()=>current};root.NemoNativeObjectProjectFile={projectionReadyForUI:(r,p)=>r===root&&p===info&&ready};root.showToast=m=>root.notices.push(m);
 const timeline=fs.readFileSync(require('node:path').join(__dirname,'../src/js/timeline.js'),'utf8');
 const poison=()=>{throw Error('LEGACY WRITE');};const context=vm.createContext({window:root,state:new Proxy({},{set:poison}),stopPlay:poison,startPlay:poison,syncDocFields:poison,drawStage:poison});
 const sm={};for(const name of ['setFps','setCanvasSize','setCanvasBg','setWorkArea']){
  const re=new RegExp(name+':(function\\([^)]*\\)\\{[^\\n]+?\\})(?=,|$)');const match=timeline.match(re);assert.ok(match,name);sm[name]=vm.runInContext('('+match[1]+')',context);}
 const controller=Desktop.install(root);return {root,sm,controller,calls,info:()=>info,ready:v=>{ready=v;},fault:v=>{fault=v;},
  replace:()=>{current={...session};},hold:()=>{let release;gate=new Promise(r=>{release=r;});return release;}};
}
for(const [name,args,expected] of [
 ['setCanvasSize',[1920,1080],{kind:'setCanvasSize',width:1920,height:1080}],
 ['setFps',[29.97],{kind:'setFps',fps:29.97}],
 ['setCanvasBg',['aBcD0980'],{kind:'setCanvasBackground',color:'aBcD0980'}],
 ['setWorkArea',[-2.5,100.25],{kind:'setWorkArea',inFrame:-2.5,outFrame:100.25}],
])test('actual proposed ordinary '+name+' reaches the typed intent without legacy state writes',async()=>{
 const h=fixture(),r=await h.sm[name](...args);assert.equal(r.status,'applied-presented');assert.deepEqual(h.calls,[expected]);assert.equal(h.info().contentRevision,1);assert.equal(h.root.state.currentFrame,7);assert.deepEqual(h.root._layerSel,[0]);
});
test('no-op has zero history and unchanged revision',async()=>{const h=fixture();const r=await h.sm.setFps(24);assert.equal(r.status,'unchanged');assert.equal(r.applied,false);assert.equal(h.info().contentRevision,0);});
for(const [value,expected] of [[-5,1],[120,60]])test('FPS raw '+value+' is clamped only by native port',async()=>{const h=fixture();assert.equal((await h.sm.setFps(value)).status,'applied-presented');assert.equal(h.calls[0].fps,value);assert.equal(h.info().fps,expected);});
test('pending receipt preserves the final input per setting without optimistic UI writes',async()=>{const h=fixture(),release=h.hold(),p=h.sm.setCanvasSize(1920,1080);assert.equal(h.controller.isPending(),true);assert.equal(h.info().canvasW,320);const earlier=h.sm.setFps(30),latest=h.sm.setFps(40),background=h.sm.setCanvasBg('#aBc');assert.equal((await earlier).status,'superseded');assert.equal(h.calls.length,1);release();assert.equal((await p).status,'applied-presented');assert.equal((await latest).status,'applied-presented');assert.equal((await background).status,'applied-presented');assert.equal(h.calls.length,3);assert.equal(h.info().fps,40);assert.equal(h.info().canvasBg,'#aBc');});
test('late replacement or frame/selection change cannot confirm the new presentation',async()=>{for(const kind of ['replacement','frame','selection','dispose','readiness']){const h=fixture(),release=h.hold(),p=h.sm.setFps(30);if(kind==='replacement')h.replace();if(kind==='frame')h.root.state.currentFrame=8;if(kind==='selection')h.root._layerSel=[];if(kind==='dispose')h.controller.dispose();if(kind==='readiness')h.ready(false);release();assert.equal((await p).status,'applied-unconfirmed',kind);}});
test('native refusal and malformed history are never confirmed',async()=>{for(const fault of ['refuse','history']){const h=fixture();h.fault(fault);assert.equal((await h.sm.setFps(30)).status,'unconfirmed');}});
test('missing readiness, invalid values and retired controller never dispatch',async()=>{const h=fixture();h.ready(false);assert.equal((await h.sm.setFps(30)).status,'unavailable');h.ready(true);for(const action of [{kind:'setFps',fps:NaN},{kind:'setCanvasSize',width:0,height:2},{kind:'setCanvasBackground',color:'ggg'},{kind:'setWorkArea',inFrame:0,outFrame:null},{kind:'setFps',fps:24,extra:true},{kind:'setTotalFrames',frames:24}])assert.equal((await h.controller.edit(action)).status,'unavailable');h.controller.dispose();assert.equal((await h.sm.setFps(30)).status,'unavailable');assert.equal(h.calls.length,0);});
test('missing controller refuses without entering the old ordinary setter',async()=>{const h=fixture();delete h.root.NemoNativeProjectSettingsController;assert.equal((await h.sm.setFps(30)).status,'unavailable');assert.equal(h.calls.length,0);});

test('queued intent is never retried after refusal, replacement or lost presentation',async()=>{for(const fault of ['refuse','history','replacement','preview','dispose']){const h=fixture(),release=h.hold(),first=h.sm.setFps(30),queued=h.sm.setCanvasSize(1920,1080);if(fault==='replacement')h.replace();else if(fault==='dispose')h.controller.dispose();else h.fault(fault);release();await first;assert.equal((await queued).status,'unavailable',fault);assert.equal(h.calls.length,1);}});

test('rapid drag retains its final value with bounded native publications',async()=>{const h=fixture(),release=h.hold(),active=h.sm.setCanvasSize(640,360),requests=[];for(let fps=1;fps<=50;fps++)requests.push(h.sm.setFps(fps));for(const earlier of requests.slice(0,-1))assert.equal((await earlier).status,'superseded');assert.equal(h.calls.length,1);assert.equal(h.info().fps,24);release();await active;assert.equal((await requests.at(-1)).status,'applied-presented');assert.equal(h.info().fps,50);assert.equal(h.calls.length,2);});

for(const [key,expected] of [['b',{kind:'setWorkArea',inFrame:7,outFrame:20}],['n',{kind:'setWorkArea',inFrame:2,outFrame:7}]])test('ordinary work-area keyboard '+key+' delegates without writing projected settings',async()=>{
 const h=fixture();h.root.state.waIn=2;h.root.state.waOut=20;h.root.state.totalFrames=120;
 const text=fs.readFileSync(require('node:path').join(__dirname,'../src/js/timeline.js'),'utf8');
 const start=text.indexOf("  if((k==='"+key+"'||k==='"+key.toUpperCase()+"')&&_pointerOverTimeline){");assert.ok(start>=0);
 const block=text.slice(start,text.indexOf('\n  }',start)+4);let dispatched,prevented=0;
 h.root.SM={setWorkArea(...args){dispatched=h.sm.setWorkArea(...args);return dispatched;}};h.root.updateWaBar=()=>{};
 const state=new Proxy(h.root.state,{set(){throw Error('LEGACY WRITE');}});
 vm.runInNewContext('(function(k,event){'+block+'})',{window:h.root,state,_pointerOverTimeline:true,updateWaBar:()=>{}})(key,{preventDefault(){prevented++;}});
 assert.equal(prevented,1);assert.equal((await dispatched).status,'applied-presented');assert.deepEqual(h.calls,[expected]);assert.equal(h.root.state.waIn,2);assert.equal(h.root.state.waOut,20);
});
