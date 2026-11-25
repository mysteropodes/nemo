'use strict';
// select-bridge's native canvas intent consumes every canvas-area pointer event on a native object project. The eraser,
// the shape tools, the pen and the bucket own a native controller that listens on the SAME element, registered AFTER
// select-bridge (script order), so a consumed event never reached them: on the installed trial they did nothing at all,
// with no message. Same family as #2356 SEL-F1 (Direct Select), for the tools SEL-F1 did not cover.
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),vm=require('node:vm');
const Intent=require('../src/js/adapters/select-canvas-intent.js');
const SELECT=fs.readFileSync(require.resolve('../src/js/select-bridge.js'),'utf8');
const INDEX=fs.readFileSync(require.resolve('../src/index.html'),'utf8');
const DOWNSTREAM=['eraser','line','rect','ellipse','star','speechbubble','pen','fill'];
const EVENTS=['pointerdown','pointermove','pointerup','pointercancel','lostpointercapture'];
function node(name){return {name,listeners:[],addEventListener(type,fn,opt){this.listeners.push({type,fn,capture:!!(opt&&opt.capture)});}};}
// Capture-phase dispatch from the document down to the target, honouring stopImmediatePropagation.
function dispatch(path,type,event){
  let stopped=false;event.stopImmediatePropagation=()=>{stopped=true;};event.preventDefault=()=>{};
  for(const n of path)for(const l of n.listeners)if(!stopped&&l.type===type&&l.capture)l.fn(event);
  return stopped;
}
function harness(tool,owns){
  const counts={select:0,legacy:0,downstream:0};
  const doc=node('document'),area=node('canvas-area'),canvas=node('drawing-canvas');
  const root={state:{tool},NemoSelectCanvasIntent:Intent,NemoNativeOpacityCutover:{blocksLegacy:()=>owns!==false},addEventListener(){}};
  const sandbox={window:root,state:root.state,nativePointerIntent:null,nativePaperToolPointer:null,
    nativePointer:()=>{const hit=e=>{counts.select++;e.stopImmediatePropagation();e.preventDefault();};return {down:hit,move:hit,up:hit,cancelEvent:hit};},
    onDown:()=>counts.legacy++,onMove:()=>counts.legacy++,onUp:()=>counts.legacy++,nativePaperRotateRingPress:()=>false,target:area};
  // The real nativePointerEvent and its real listener registration.
  const fn=SELECT.slice(SELECT.indexOf('function nativePointerEvent(method, e) {'),SELECT.indexOf('// The engine-off Paper Tool entry'));
  const lines=SELECT.split('\n').filter(l=>l.includes('target.addEventListener')&&l.includes('nativePointerEvent(')&&!l.includes('contextmenu'));
  assert.equal(lines.length,5);
  vm.runInNewContext(fn+'\n'+lines.join('\n'),sandbox);
  // A downstream tool bridge registers AFTER select-bridge on the same element, as in index.html.
  for(const type of EVENTS)area.addEventListener(type,()=>{counts.downstream++;},{capture:true});
  return {counts,fire:type=>dispatch([doc,area,canvas],type,{target:canvas,clientX:100,clientY:60,pointerId:1,button:0,isPrimary:true})};
}

test('the bridges of the downstream tools are loaded after select-bridge, which is why they need the pass-through',()=>{
  const at=name=>{const i=INDEX.indexOf('js/'+name+'.js');assert.ok(i>=0,name);return i;};
  for(const name of ['shape-bridge','eraser-bridge','fill-bridge','pen-bridge'])assert.ok(at(name)>at('select-bridge'),name);
});

test('the native select intent leaves the gestures of every downstream-owned tool to their controller',()=>{
  for(const tool of DOWNSTREAM){
    const h=harness(tool);
    for(const type of EVENTS)assert.equal(h.fire(type),false,tool+' '+type+' must not be consumed by select-bridge');
    assert.equal(h.counts.downstream,EVENTS.length,tool+' controller receives every event');
    assert.equal(h.counts.select,0,tool);
    assert.equal(h.counts.legacy,0,tool+': the legacy Paper select handlers stay out on a native project');
  }
});

test('Select itself, and any tool without a downstream controller, is still consumed by the select intent',()=>{
  for(const tool of ['select','draw','hand','unknown-tool']){
    const h=harness(tool);
    for(const type of EVENTS)assert.equal(h.fire(type),true,tool+' '+type);
    assert.equal(h.counts.select,EVENTS.length,tool);
    assert.equal(h.counts.downstream,0,tool);
  }
});

test('a legacy (non-native) project keeps its legacy routing for these tools',()=>{
  const h=harness('eraser',false);
  h.fire('pointerdown');
  assert.equal(h.counts.select,0);
  assert.equal(h.counts.legacy,1);
  assert.equal(h.counts.downstream,1);
});

test('ownedDownstream names exactly the tools with a later native controller',()=>{
  for(const tool of DOWNSTREAM)assert.equal(Intent.ownedDownstream(tool),true,tool);
  for(const tool of ['select','subselect','draw','fillbrush','text','rig','hand',undefined,null,''])assert.equal(Intent.ownedDownstream(tool),false,String(tool));
});
