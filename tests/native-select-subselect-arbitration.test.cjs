'use strict';
// #2356 SEL-F1: select-bridge's native canvas intent consumes every canvas-area pointer event on a native
// object project. Direct Select must still receive its gestures, whatever the script/registration order.
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),vm=require('node:vm');
const Bridge=require('../src/js/subselect-bridge.js');
const {fixture}=require('./helpers/native-subselect.cjs');
const SELECT=fs.readFileSync(require.resolve('../src/js/select-bridge.js'),'utf8');
function node(name,parent){
  const n={name,parent,listeners:[],setPointerCapture(){},releasePointerCapture(){},getBoundingClientRect:()=>({left:0,top:0,right:200,bottom:200,width:200,height:200}),
    addEventListener(type,fn,opt){this.listeners.push({type,fn,capture:!!(opt&&opt.capture)});}};
  return n;
}
// Capture-phase dispatch from the document down to the target, honouring stopImmediatePropagation.
function dispatch(path,type,event){
  let stopped=false;event.stopImmediatePropagation=()=>{stopped=true;};event.preventDefault=()=>{};
  for(const n of path)for(const l of n.listeners)if(!stopped&&l.type===type&&l.capture)l.fn(event);
  return stopped;
}
function harness(tool){
  const f=fixture(),{root}=f,counts={select:0,legacy:0,subselect:0};
  const doc=node('document',null),area=node('canvas-area',doc),canvas=node('drawing-canvas',area);
  Object.assign(doc,{readyState:'complete',...root.document,getElementById:id=>id==='canvas-area'?area:id==='drawing-canvas'?canvas:null});
  root.document=doc;root.addEventListener=()=>{};root.state.tool=tool;
  const read=f.s.readSubselect;f.s.readSubselect=async(...a)=>{counts.subselect++;return read(...a);};
  // select-bridge side: the real nativePointerEvent and its real listener registration, on canvas-area.
  const sandbox={window:root,state:root.state,nativePointerIntent:null,
    nativePointer:()=>{const hit=e=>{counts.select++;e.stopImmediatePropagation();e.preventDefault();};return {down:hit,move:hit,up:hit,cancelEvent:hit};},
    onDown:()=>counts.legacy++,onMove:()=>{},onUp:()=>{},target:area};
  root.NemoNativeOpacityCutover={blocksLegacy:()=>true};
  const fn=SELECT.slice(SELECT.indexOf('function nativePointerEvent(method, e) {'),SELECT.indexOf('// Same handle-position math'));
  const lines=SELECT.split('\n').filter(l=>l.includes('target.addEventListener')&&l.includes('nativePointerEvent(')&&!l.includes('contextmenu'));
  assert.equal(lines.length,5);
  vm.runInNewContext(fn+'\n'+lines.join('\n'),sandbox);
  // subselect-bridge registers AFTER select-bridge, as in index.html (2576 after 2574).
  Bridge.install(root);
  return {...f,counts,area,canvas,doc,fire:(type,x,y)=>dispatch([doc,area,canvas],type,{target:canvas,clientX:x,clientY:y,pointerId:1,button:0,isPrimary:true})};
}

test('Direct Select reaches its controller although select-bridge registered first on canvas-area',async()=>{
  const h=harness('subselect');
  const stopped=h.fire('pointerdown',100,60);
  assert.equal(h.counts.select,0,'select intent must not consume a Direct Select gesture');
  assert.equal(h.counts.subselect,1,'Direct Select reads the native listing on pointerdown');
  assert.equal(stopped,true);
});

test('Select (V) still reaches the select intent and never Direct Select',()=>{
  const h=harness('select');
  h.fire('pointerdown',100,60);h.fire('pointermove',101,61);h.fire('pointerup',101,61);
  assert.equal(h.counts.select,3,'the select intent still consumes every event of its own gesture');
  assert.equal(h.counts.subselect,0);
});

test('select-bridge no longer swallows subselect moves/ups when no document-level owner exists',()=>{
  const h=harness('subselect');
  // Drop the document-level subselect listeners: the secondary guard alone must leave the event unconsumed.
  h.doc.listeners.length=0;
  for(const type of ['pointerdown','pointermove','pointerup','pointercancel','lostpointercapture'])
    assert.equal(h.fire(type,100,60),false,type);
  assert.equal(h.counts.select,0);
});
