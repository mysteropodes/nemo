'use strict';
// Actual Cycle handler -> public UI -> real DOM form -> existing wrapped
// Desktop/Intent/Project/SDK chain. Native I/O remains modeled, not installed credit.
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),vm=require('node:vm');
const Selection=require('../src/js/adapters/native-frame-cell-selection.js');
const UI=require('../src/js/adapters/native-object-ui.js'),Desktop=require('../src/js/adapters/native-frame-edit-desktop.js');
const Surface=require('../src/js/adapters/native-opacity-legacy-surface.js');
const desktopSource=fs.readFileSync(require.resolve('./native-frame-edit-desktop.test.cjs'),'utf8');
const makeDesktop=vm.compileFunction(desktopSource.slice(0,desktopSource.indexOf("test('"))+'\nreturn harness;',['require'])(require);
const cellSource=fs.readFileSync(require.resolve('./native-frame-cell-selection.test.cjs'),'utf8');
const ca=cellSource.indexOf('// BEGIN REUSABLE NATIVE CELL HEADER FIXTURE'),cb=cellSource.indexOf('// END REUSABLE NATIVE CELL HEADER FIXTURE');
const makeCells=vm.compileFunction(cellSource.slice(ca,cb)+'\nreturn selectionHarness;',['assert','Selection'])(assert,Selection);
const timeline=fs.readFileSync(require.resolve('../src/js/timeline.js'),'utf8'),start=timeline.indexOf('function initCycleAndPropagate(){');
const cycleBody=timeline.slice(start,timeline.indexOf("if(document.readyState==='loading')document.addEventListener('DOMContentLoaded',initCycleAndPropagate)",start));
const bs=timeline.indexOf('function selBounds(){'),bounds=timeline.slice(bs,timeline.indexOf('\n}',bs)+2);
const turn=()=>new Promise(setImmediate);

async function harness(options={}){
  const h=makeDesktop({repeatCounts:{cycledLayers:1,skippedComponentLayers:1}});await h.open();
  const ui=makeCells();Selection.retire(ui.root);const root=h.root;let legacy=0,last,prompts=0;
  Object.assign(root,{document:ui.doc,FC:20,_layerSel:[0,1],NemoNativeFrameCellSelection:Selection,NemoNativeObjectUI:UI,
    addEventListener:ui.root.addEventListener,removeEventListener:ui.root.removeEventListener,fire:ui.root.fire,
    NemoNativeFrameEditDesktop:Desktop,__TAURI__:{core:{invoke(){throw Error('PRIVATE WRITER');}}},
    SM:{importJSON(){},repeatSelection(){legacy++;throw Error('PAPER WRITER');}}});root._sel.frames=[];
  const dispose=Surface.desktopPorts(root,{}).surface.wrap('timeline',[],'timeline-',()=>false,()=>{});
  const wrapped=root.SM.repeatSelection;root.SM.repeatSelection=value=>(last=wrapped(value));
  const ports={currentSession:()=>root.NemoNativeObjectProject.desktop(root),projectionReady:()=>true,frameEditPending:()=>h.controller.isPending()};
  Selection.mount(root,ui.caption,h.session.info(),ports);const strip=ui.caption.children[0];strip.children[4].fire('click');
  strip.fire('mousedown',{clientX:110});ui.doc.fire('mouseup',{clientX:130});assert.equal(root._sel.frames.length,4);
  const context={window:root,document:ui.doc,_sel:root._sel,showToast:root.showToast,SM:{t:v=>v},
    prompt(){prompts++;if(options.promptNull)return null;throw Error('NATIVE BARE PROMPT');}};
  vm.runInNewContext(bounds+'\n'+cycleBody+'\ninitCycleAndPropagate();',context);
  function dialog(){const overlay=ui.doc.body.children.find(n=>n.className==='native-repeat-dialog-overlay');
    if(!overlay)return null;const form=overlay.children[0];return {overlay,form,input:form.children[2],repeat:form.children[3],cancel:form.children[4]};}
  return {...h,ui,ports,strip,dialog,prompts:()=>prompts,legacy:()=>legacy,last:()=>last,
    click(){if(!ui.cycle.disabled)ui.cycle.fire('click');},forceClick(){ui.cycle.fire('click');},
    dispose(){Selection.retire(root);dispose();}};
}

test('native original Cycle opens labeled default2 DOM input and submits once without bare prompt',async()=>{
  for(const promptNull of [false,true]){
    const h=await harness({promptNull});h.click();const d=h.dialog();assert.ok(d);assert.equal(d.form.attributes.role,'dialog');
    assert.equal(d.input.attributes['aria-label'],'Repetitions');assert.equal(d.input.value,'2');assert.equal(d.repeat.textContent,'Repeat');assert.equal(d.cancel.textContent,'Cancel');
    assert.equal(h.prompts(),0);assert.equal(h.ui.cycle.disabled,true);assert.equal(h.commands().length,0);
    d.repeat.fire('click');d.repeat.fire('click');d.form.fire('submit');assert.equal((await h.last()).status,'applied-presented');
    assert.equal(h.commands().length,1);assert.deepEqual(h.commands()[0].q.payload,{command:'object.frames.repeat',fromLayerUid:'alpha',toLayerUid:'beta',fromFrame:0,toFrame:1,times:2});
    assert.equal(h.dialog(),null);assert.equal(h.legacy(),0);h.dispose();
  }
});
test('native form retains raw baseline parseInt/clamp behavior including Enter',async()=>{
  for(const [raw,want] of [['2.9 rounds',2],['0',1],['-8',1],['999',50],['invalid',1],['',1]]){
    const h=await harness();h.click();const d=h.dialog();d.input.value=raw;d.input.fire('keydown',{key:'Enter'});
    assert.equal((await h.last()).status,'applied-presented');assert.equal(h.commands()[0].q.payload.times,want);assert.equal(h.prompts(),0);h.dispose();
  }
});
test('Cancel Escape blur retire and stale owner/selection/presentation never dispatch or replace prior cells',async()=>{
  for(const fault of ['cancel','escape','blur','retire','cells','rows','frame','FC','owner','revision','presentation','pending','detached','controller','writer']){
    const h=await harness();h.click();const d=h.dialog(),prior=h.root._sel.frames;
    if(fault==='cancel')d.cancel.fire('click');if(fault==='escape')h.ui.doc.fire('keydown',{key:'Escape'});
    if(fault==='blur')h.root.fire('blur');
    if(fault==='retire')Selection.retire(h.root);if(fault==='cells')h.root._sel.frames=prior.slice();
    if(fault==='rows')h.root._layerSel=[1];if(fault==='frame')h.root.state.currentFrame=3;if(fault==='FC')h.root.FC=21;
    if(fault==='owner')h.replace({...h.session,info:()=>({...h.session.info(),documentId:'foreign'})});
    if(fault==='revision')h.replace({...h.session,info:()=>({...h.session.info(),contentRevision:1,documentSnapshotId:'snapshot-1'})});
    if(fault==='presentation')h.ports.currentSession=()=>({...h.session,readPresentedObjectSession:()=>null});if(fault==='pending')h.ports.frameEditPending=()=>true;
    if(fault==='controller')h.root.NemoNativeFrameEditController={...h.controller};if(fault==='writer')h.root.SM.repeatSelection=()=>{throw Error('REPLACED WRITER');};
    if(fault==='detached')h.ui.caption.replaceChildren();
    d.repeat.fire('click');await turn();assert.equal(h.commands().length,0,fault);assert.equal(h.legacy(),0,fault);assert.equal(h.prompts(),0);
    if(fault!=='cells')assert.equal(h.root._sel.frames,prior,fault);
    if(!['cancel','escape','blur','retire'].includes(fault))assert.match(h.notices.at(-1),/Repeat refused/,fault);h.dispose();
  }
});
test('native unavailable controller/session/form stays consumed with legacy writer and prompt poisoned',async()=>{
  for(const fault of ['controller','session','throwing-session','form','creation','unpresented']){
    const h=await harness();if(fault==='controller')delete h.root.NemoNativeFrameEditController;
    if(fault==='session')h.root.NemoNativeObjectProject={desktop:()=>null};
    if(fault==='throwing-session')h.root.NemoNativeObjectProject={desktop(){throw Error('NO DESKTOP');}};
    if(fault==='form')delete h.root.NemoNativeFrameCellSelection;if(fault==='unpresented')h.ports.frameEditPending=()=>true;
    if(fault==='creation'){const create=h.ui.doc.createElement;h.ui.doc.createElement=tag=>{if(tag==='form')throw Error('NO FORM');return create(tag);};}
    h.click();await turn();assert.equal(h.dialog(),null,fault);assert.equal(h.commands().length,0);assert.equal(h.legacy(),0);assert.equal(h.prompts(),0);h.dispose();
  }
});
