'use strict';
// Project length (Frames field, Trim to Work Area): the actual timeline.js entries reach the typed native settings
// intent, never a legacy writer, and the playhead/toast follow-up matches timeline.js at 1065e9d8.
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),vm=require('node:vm'),path=require('node:path');
const Intent=require('../src/js/adapters/native-project-settings-intent.js'),Desktop=require('../src/js/adapters/native-project-settings-desktop.js');
const Length=require('../src/js/adapters/native-project-length.js');
const {validateRequest}=require('../src/js/adapters/native-application.js');
const timeline=fs.readFileSync(path.join(__dirname,'../src/js/timeline.js'),'utf8');

function fixture(over){
  over=over||{};
  let info={instanceId:'host',documentId:'doc',lifecycleGeneration:1,contentRevision:0,documentSnapshotId:'s0',canvasW:320,canvasH:180,fps:24,
    totalFrames:120,layers:[{layerUid:'a'}],...(over.info||{})},fault=null;
  const calls=[],gone=[],toasts=[],fields={'proj-frames':{value:''},'tl-total':{value:''}};
  const root={state:{currentFrame:over.frame===undefined?7:over.frame,layers:info.layers,totalFrames:info.totalFrames,waIn:info.waIn,waOut:info.waOut},
    _layerSel:[0],NemoNativeProjectSettingsIntent:Intent,NemoNativeProjectLength:Length,
    document:{getElementById:id=>fields[id]||null},showToast:m=>toasts.push(m),SM:{t:k=>k},
    goToFrame:async f=>{gone.push(f);root.state.currentFrame=f;}};
  const session={info:()=>info,isActive:()=>true,
    readPresentedObjectSession:()=>({...info,available:true,presentation:{...info,owner:'native',status:'presented',frame:root.state.currentFrame}}),
    async editProjectSettings(action,pin){
      assert.strictEqual(pin,info);calls.push(action);
      const next={...info};
      if(action.kind==='setTotalFrames'){
        const wanted=Math.max(1,Math.min(999,Math.floor(action.frames)));next.totalFrames=wanted;
        // The engine clamps the work-area end, then the start (timeline.js setTotalFrames).
        if(next.waOut!==undefined&&next.waOut>=wanted)next.waOut=wanted-1;
        if(next.waIn!==undefined&&next.waIn>=(next.waOut===undefined?wanted-1:next.waOut))next.waIn=Math.max(0,(next.waOut===undefined?wanted-1:next.waOut)-1);
      }
      if(action.kind==='trimToWorkArea'){
        const begin=Math.max(0,Math.floor(next.waIn||0)),end=Math.min(next.totalFrames-1,Math.floor(next.waOut===undefined?next.totalFrames-1:next.waOut));
        if(!(end<=begin||begin===0&&end===next.totalFrames-1)){next.totalFrames=end-begin+1;next.waIn=0;next.waOut=end-begin;}
      }
      if(fault==='total')next.totalFrames+=1;
      const applied=JSON.stringify(next)!==JSON.stringify(info);next.contentRevision+=applied?1:0;next.documentSnapshotId='s'+next.contentRevision;info=next;
      // The native projection keeps the editor playhead and mirrored settings inside the new document.
      root.state.totalFrames=info.totalFrames;root.state.waIn=info.waIn;root.state.waOut=info.waOut;
      root.state.currentFrame=Math.max(0,Math.min(info.totalFrames-1,root.state.currentFrame));
      return {apiVersion:2,ok:true,instanceId:info.instanceId,documentId:info.documentId,contentRevision:info.contentRevision,result:{applied,historyEntriesAdded:applied?1:0}};}};
  root.NemoNativeObjectProject={desktop:()=>session};
  root.NemoNativeObjectProjectFile={projectionReadyForUI:()=>true};
  const poison=()=>{throw Error('LEGACY WRITE');};
  const context=vm.createContext({window:root,state:new Proxy({},{set:poison,get:poison}),saveAllLayerFrames:poison,pushUndoLayers:poison,loadFrame:poison,goToFrame:poison});
  const sm={};
  for(const name of ['setTotalFrames','trimToWorkArea']){
    const match=timeline.match(new RegExp('\\n  '+name+':(function\\([^)]*\\)\\{[^\\n]+\\})(?=,\\n)'));assert.ok(match,name);
    sm[name]=vm.runInContext('('+match[1]+')',context);
  }
  const controller=Desktop.install(root);
  return {root,sm,controller,calls,gone,toasts,fields,info:()=>info,fault:v=>{fault=v;}};
}

test('the Frames field grows through the typed intent with one dispatch and no legacy write',async()=>{
  const h=fixture();const r=await h.sm.setTotalFrames(200);
  assert.equal(r.status,'applied-presented');assert.deepEqual(h.calls,[{kind:'setTotalFrames',frames:200}]);
  assert.equal(h.info().totalFrames,200);assert.equal(h.info().contentRevision,1);assert.equal(h.root.state.currentFrame,7);
  assert.equal(h.fields['proj-frames'].value,200);assert.equal(h.fields['tl-total'].value,200);
});
test('shrinking below the playhead clamps it into the shorter timeline and clamps the work area like the baseline',async()=>{
  const h=fixture({frame:80,info:{waIn:50,waOut:119}});const r=await h.sm.setTotalFrames(30);
  assert.equal(r.status,'applied-presented');assert.deepEqual(h.calls,[{kind:'setTotalFrames',frames:30}]);
  assert.deepEqual([h.info().totalFrames,h.info().waIn,h.info().waOut],[30,28,29]);assert.equal(h.root.state.currentFrame,29);
});
for(const [input,sent,total] of [[0,1,1],[null,1,1],[-4,1,1],[5000,999,999],[Infinity,999,999],[12.9,12,12],['48',48,48]])test('the Frames field clamps '+JSON.stringify(input)+' to 1..999 before the intent',async()=>{
  const h=fixture();const r=await h.sm.setTotalFrames(input);assert.equal(r.status,'applied-presented');assert.deepEqual(h.calls,[{kind:'setTotalFrames',frames:sent}]);assert.equal(h.info().totalFrames,total);
});
test('an unparsable entry never dispatches and the inputs revert to the document length',async()=>{
  const h=fixture();h.root.state.totalFrames=120;
  for(const bad of [NaN,undefined,'abc'])assert.equal((await h.sm.setTotalFrames(bad)).status,'unavailable');
  assert.equal(h.calls.length,0);
});
test('the same length is an unchanged no-op without history',async()=>{
  const h=fixture();const r=await h.sm.setTotalFrames(120);assert.equal(r.status,'unchanged');assert.equal(r.applied,false);assert.equal(h.info().contentRevision,0);
});
test('a length the document did not take is never confirmed',async()=>{
  const h=fixture();h.fault('total');assert.equal((await h.sm.setTotalFrames(200)).status,'applied-unconfirmed');
});
test('without the native controller both entries are unavailable and touch no legacy state',async()=>{
  const h=fixture();h.controller.dispose();delete h.root.NemoNativeProjectSettingsController;
  assert.equal((await h.sm.setTotalFrames(10)).status,'unavailable');assert.equal((await h.sm.trimToWorkArea()).status,'unavailable');assert.equal(h.calls.length,0);
});

test('Trim to Work Area dispatches once, re-bases the playhead on the start and reports the new length',async()=>{
  const h=fixture({frame:30,info:{waIn:10,waOut:49}});const r=await h.sm.trimToWorkArea();
  assert.equal(r.status,'applied-presented');assert.deepEqual(h.calls,[{kind:'trimToWorkArea'}]);
  assert.deepEqual([h.info().totalFrames,h.info().waIn,h.info().waOut],[40,0,39]);assert.equal(h.info().contentRevision,1);
  // 30 - 10: the baseline's currentFrame re-base.
  assert.deepEqual(h.gone,[20]);assert.equal(h.root.state.currentFrame,20);
  assert.deepEqual(h.toasts,['toastCompReducedToWorkArea40 frames)']);assert.equal(h.fields['tl-total'].value,40);
});
test('a playhead before the work area lands on the first kept frame and one after it on the last',async()=>{
  const before=fixture({frame:3,info:{waIn:10,waOut:49}});await before.sm.trimToWorkArea();assert.equal(before.root.state.currentFrame,0);
  const after=fixture({frame:100,info:{waIn:10,waOut:49}});await after.sm.trimToWorkArea();assert.equal(after.root.state.currentFrame,39);
});
test('Trim refuses a work area of one frame or less with the baseline toast and no dispatch',async()=>{
  for(const [waIn,waOut] of [[5,5],[9,4]]){
    const h=fixture({info:{waIn,waOut}});assert.equal((await h.sm.trimToWorkArea()).status,'unavailable');
    assert.deepEqual(h.toasts,['hsWorkAreaTooShort']);assert.equal(h.calls.length,0);
  }
});
test('Trim refuses a work area that already covers the whole timeline with the baseline toast',async()=>{
  for(const info of [{},{waIn:0,waOut:119}]){
    const h=fixture({info});assert.equal((await h.sm.trimToWorkArea()).status,'unavailable');
    assert.deepEqual(h.toasts,['toastWorkAreaAlreadyCoversAll']);assert.equal(h.calls.length,0);
  }
});
test('Trim refuses inside a Component or a montage view, as the baseline does',async()=>{
  const inside=fixture({info:{waIn:10,waOut:49}});inside.root.state.activeSymbolId='sym';
  assert.equal((await inside.sm.trimToWorkArea()).status,'unavailable');assert.deepEqual(inside.toasts,['toastCloseComponentFirst']);
  const montage=fixture({info:{waIn:10,waOut:49}});montage.root.state.activeMontageViewId='m';
  assert.equal((await montage.sm.trimToWorkArea()).status,'unavailable');assert.deepEqual(montage.toasts,['toastCloseMontageFirst']);
  assert.equal(inside.calls.length+montage.calls.length,0);
});
test('a trim the document did not take is never confirmed and does not move the playhead',async()=>{
  const h=fixture({frame:30,info:{waIn:10,waOut:49}});h.fault('total');
  assert.equal((await h.sm.trimToWorkArea()).status,'applied-unconfirmed');assert.deepEqual(h.gone,[]);assert.ok(!h.toasts.some(t=>t.startsWith('toastCompReducedToWorkArea')));
});

test('the public contract admits exactly the two length actions',()=>{
  const req=action=>({apiVersion:2,requestId:'settings',instanceId:'i',documentId:'d',expectedRevision:7,operation:'command.document.object.project.settings.edit',
    payload:{command:'object.project.settings.edit',action}});
  for(const action of [{kind:'setTotalFrames',frames:240},{kind:'setTotalFrames',frames:0.5},{kind:'trimToWorkArea'}])assert.doesNotThrow(()=>validateRequest(req(action)));
  for(const action of [{kind:'setTotalFrames'},{kind:'setTotalFrames',totalFrames:9},{kind:'setTotalFrames',frames:'9'},{kind:'setTotalFrames',frames:NaN},
    {kind:'setTotalFrames',frames:9,extra:1},{kind:'trimToWorkArea',frames:9}])assert.throws(()=>validateRequest(req(action)),TypeError);
});
