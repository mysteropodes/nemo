'use strict';
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),vm=require('node:vm');
const Intent=require('../src/js/adapters/native-drawing-add.js');
const Desktop=require('../src/js/adapters/native-drawing-add-desktop.js');
const Surface=require('../src/js/adapters/native-opacity-legacy-surface.js');
const Operations=require('../src/js/application/native-opacity-operations.js');
function fixture(){
  let info={instanceId:'host',documentId:'doc',lifecycleGeneration:1,contentRevision:0,documentSnapshotId:'s0',
    totalFrames:24,layers:[{layerUid:'old',name:'Old',visible:true}]},calls=0,ready=true,current;
  const root={state:{currentFrame:7,layers:info.layers},_layerSel:[0],NemoNativeDrawingAdd:Intent,notices:[]};
  const session={isActive:()=>true,blocksLegacy:()=>true,info:()=>info,
    readPresentedObjectSession:()=>({...info,available:true,presentation:{...info,owner:'native',status:'presented',frame:root.state.currentFrame}}),
    async createLayer(name,pin){assert.strictEqual(pin,info);calls++;
      const uid='native opaque:雪',revision=info.contentRevision+1;
      info={...info,contentRevision:revision,documentSnapshotId:'s'+revision,layers:info.layers.concat({layerUid:uid,name,
        visible:true,locked:false,color:'#B49BE6',parentLayerUid:null})};root.state.layers=info.layers;
      return {apiVersion:2,ok:true,instanceId:info.instanceId,documentId:info.documentId,contentRevision:revision,
        result:{applied:true,historyEntriesAdded:1,layerUid:uid}};
    }};
  current=session;
  root.NemoNativeObjectProject={desktop:()=>current,selectLayerUI(_root,index,preserve,uid,expected){
    assert.strictEqual(_root,root);assert.equal(preserve,false);assert.strictEqual(expected,info);
    assert.equal(info.layers[index].layerUid,uid);root.state.activeLayerIdx=index;root._layerSel=[index];return true;}};
  root.NemoNativeObjectProjectFile={projectionReadyForUI:(_root,pin)=>ready&&_root===root&&pin===info};
  root.showToast=message=>root.notices.push(message);
  const timeline=fs.readFileSync('src/js/timeline.js','utf8'),match=timeline.match(/  addLayer:(function\(\)\{[^\n]+\}),\n/);
  assert.ok(match,'actual shipped ordinary addLayer entry must be present');
  const poison=()=>{throw Error('LEGACY WRITE');};
  const context=vm.createContext({window:root,saveAllLayerFrames:poison,pushUndoLayers:poison,createUserLayer:poison});
  root.SM={addLayer:vm.runInContext('('+match[1]+')',context),deleteLayer:poison};
  const surface={installGuard(){},publish(){return ()=>{};},
    wrap(target,names,prefix,allow,admitted){const owner=target==='timeline'?root.SM:{};
      const disposers=names.map(name=>Surface.wrapWriter(owner,name,allow,prefix+name,admitted));
      return ()=>disposers.forEach(dispose=>dispose());}};
  const operations=Operations.create({blocksLegacy:()=>false},{surface,objectProject:()=>current},{},
    {create:()=>({})},{create:()=>({})});
  const installation=operations.install(),controller=Desktop.install(root);
  return {root,controller,calls:()=>calls,ready:value=>{ready=value;},current:value=>{current=value;},
    dispose:()=>installation.dispose()};
}
test('actual guard installation preserves native-only ordinary Add and still refuses other writers',async()=>{
  const h=fixture();
  const result=await h.root.SM.addLayer();
  assert.equal(result.status,'created-selected');assert.equal(result.layerUid,'native opaque:雪');
  assert.equal(h.calls(),1);assert.equal(h.root.state.currentFrame,7);assert.deepEqual(h.root._layerSel,[1]);
  assert.equal(h.root.SM.deleteLayer(),false,'unrelated legacy writer remains guarded');
  assert.strictEqual(Desktop.install(h.root),h.controller,'composition is idempotent');h.dispose();
});
test('missing native session or uncorrelated projection reports unavailable without legacy mutation',async()=>{
  for(const condition of ['session','projection','selection']){
    const h=fixture();if(condition==='session')h.current(null);if(condition==='projection')h.ready(false);
    if(condition==='selection')h.root._layerSel=[99];
    assert.equal((await h.root.SM.addLayer()).status,'unavailable');assert.equal(h.calls(),0);
    assert.deepEqual(h.root.notices,['Native Drawing Add is unavailable.']);h.dispose();
  }
});
test('controller disposal and a broken notice do not reopen the removed legacy route',async()=>{
  const h=fixture();h.root.showToast=()=>{throw Error('notice unavailable');};h.controller.dispose();
  assert.equal((await h.root.SM.addLayer()).status,'unavailable');assert.equal(h.calls(),0);h.dispose();
});

// #2372 presentation-busy-audit: the native queue hides the presentation while ANY command is pending (queue-pending).
function hideQueue(h){const s=h.root.NemoNativeObjectProject.desktop(),real=s.readPresentedObjectSession,q={hidden:true};
  s.readPresentedObjectSession=function(why){if(q.hidden){if(why)why('queue-pending');return null;}return real.call(this);};return q;}
const sleep=ms=>new Promise(resolve=>setTimeout(resolve,ms));
test('ordinary Add made while the queue hides the presentation waits for it and creates exactly one layer',async()=>{
  globalThis.NemoNativePresentationWait=require('../src/js/adapters/native-presentation-wait.js');
  try{
    const h=fixture(),q=hideQueue(h),added=h.root.SM.addLayer();
    await sleep(150);assert.equal(h.calls(),0,'nothing is created while the presentation is hidden');assert.deepEqual(h.root.notices,[]);
    q.hidden=false;const result=await added;
    assert.equal(result.status,'created-selected');assert.equal(h.calls(),1);assert.deepEqual(h.root._layerSel,[1]);assert.deepEqual(h.root.notices,[]);h.dispose();
  }finally{delete globalThis.NemoNativePresentationWait;}
});
test('ordinary Add whose native session was lost during the wait still refuses and creates nothing',async()=>{
  globalThis.NemoNativePresentationWait=require('../src/js/adapters/native-presentation-wait.js');
  try{
    const h=fixture(),q=hideQueue(h),added=h.root.SM.addLayer();
    await sleep(80);h.current(null);q.hidden=false;
    assert.equal((await added).status,'unavailable');assert.equal(h.calls(),0);h.dispose();
  }finally{delete globalThis.NemoNativePresentationWait;}
});
