'use strict';
// U06 (#2370): temporary tool holds, the shape flyout and the rail badges (tool-holds.js, tool-rail.js), against a fake DOM.
// The hold behaviour is baseline 1065e9d8's, characterized then moved out of timeline.js; the lost-keyup and flyout-cleanup
// cases are the cleanup this change adds.
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path');
const Holds=require('../src/js/application/tool-holds.js'),Rail=require('../src/js/application/tool-rail.js');
const Registry=require('../src/js/application/shortcut-registry.js');
const timeline=fs.readFileSync(path.join(__dirname,'../src/js/timeline.js'),'utf8');

function bus(){const m={};return {m,addEventListener(t,f){(m[t]=m[t]||[]).push(f);},removeEventListener(t,f){m[t]=(m[t]||[]).filter(x=>x!==f);},
  emit(t,e){(m[t]||[]).slice().forEach(f=>f(e||{}));},count(t){return (m[t]||[]).length;}};}
function fakeTimers(){let t=0,id=0;const q=new Map();
  return {setTimer(fn,ms){q.set(++id,{at:t+ms,fn});return id;},clearTimer(i){q.delete(i);},pending:()=>q.size,
    advance(ms){t+=ms;[...q].filter(([,v])=>v.at<=t).sort((a,b)=>a[1].at-b[1].at).forEach(([k,v])=>{if(q.delete(k))v.fn();});}};}
function rig(over){
  const doc=Object.assign(bus(),{activeElement:null,hidden:false}),win=bus(),tm=fakeTimers();let clock=0;
  const state=Object.assign({tool:'draw',playing:false,spaceDown:false,altDown:false,isPanning:false,spaceUsedForPan:false},over);
  const log={tools:[],plays:0,cursors:[]};let pen=false;
  const holds=Holds.create({state,doc,win,now:()=>clock,setTimer:tm.setTimer,clearTimer:tm.clearTimer,
    setTool(t){log.tools.push(t);state.tool=t;},togglePlay(){log.plays++;},penActive:()=>pen,setCursor:c=>log.cursors.push(c)});
  holds.mount();
  const ev=(key,o)=>Object.assign({key,preventDefault(){this.prevented=true;}},o);
  return {doc,win,tm,state,log,holds,ev,tick:ms=>{clock+=ms;},setPen:v=>{pen=v;}};
}

test('Space held pans: armed once, auto-repeat does not re-enter, a press makes it a pan not a tap',()=>{
  const r=rig(),e=r.ev(' ');
  r.holds.spaceKeyDown(e);assert.equal(e.prevented,true);assert.equal(r.state.spaceDown,true);assert.deepEqual(r.log.cursors,['grab']);
  r.tick(100);r.holds.spaceKeyDown(r.ev(' '));r.holds.spaceKeyDown(r.ev(' '));
  assert.deepEqual(r.log.cursors,['grab'],'repeat keydowns do not re-arm');
  r.doc.emit('pointerdown');assert.equal(r.state.spaceUsedForPan,true);
  r.holds.keyUp(r.ev(' '));
  assert.equal(r.state.spaceDown,false);assert.equal(r.log.plays,0,'a pan is not a tap');assert.equal(r.log.cursors.at(-1),'crosshair','cursor back to the draw tool');
});

test('Space tap plays once; a slow hold, a Pen path in progress, or a repeat keyup do not',()=>{
  let r=rig();r.holds.spaceKeyDown(r.ev(' '));r.tick(100);r.holds.keyUp(r.ev(' '));r.holds.keyUp(r.ev(' '));
  assert.equal(r.log.plays,1,'one tap plays once, the second keyup finds no hold');
  r=rig();r.holds.spaceKeyDown(r.ev(' '));r.tick(300);r.holds.keyUp(r.ev(' '));assert.equal(r.log.plays,0,'held past the tap window');
  r=rig({tool:'pen'});r.setPen(true);r.holds.spaceKeyDown(r.ev(' '));r.tick(50);r.holds.keyUp(r.ev(' '));assert.equal(r.log.plays,0,'inert under a pen path');
  r=rig();r.holds.keyUp(r.ev(' '));assert.equal(r.log.plays,0,'a Space typed in a field never armed, so its release is not a tap');
});

test('Cmd/Ctrl+Space is the OS shortcut and never arms the hold',()=>{
  const r=rig();for(const o of [{metaKey:true},{ctrlKey:true}]){const e=r.ev(' ',o);assert.equal(r.holds.spaceKeyDown(e),false);assert.equal(e.prevented,undefined);}
  assert.equal(r.state.spaceDown,false);
});

test('blur during a Space hold resets it once; the late keyup is not a tap',()=>{
  const r=rig();r.holds.spaceKeyDown(r.ev(' '));r.state.isPanning=true;r.state.altDown=true;
  r.win.emit('blur');
  assert.equal(r.state.spaceDown,false);assert.equal(r.state.isPanning,false);assert.equal(r.state.altDown,false);
  const cursors=r.log.cursors.length;r.win.emit('blur');assert.equal(r.log.cursors.length,cursors,'a second blur changes nothing');
  r.tick(10);r.holds.keyUp(r.ev(' '));assert.equal(r.log.plays,0);
});

test('Alt hold sets and clears altDown; blur clears a lost Alt keyup',()=>{
  const r=rig();r.holds.altKeyDown();assert.equal(r.state.altDown,true);r.holds.keyUp(r.ev('Alt'));assert.equal(r.state.altDown,false);
  r.holds.altKeyDown();r.win.emit('blur');assert.equal(r.state.altDown,false);
});

test('Cmd held past the arm delay switches to Select and the keyup restores the tool exactly once',()=>{
  const r=rig({tool:'draw'});
  r.doc.emit('keydown',r.ev('Meta'));assert.deepEqual(r.log.tools,[],'not before the delay');
  r.tm.advance(Holds.CMD_HOLD_MS);assert.deepEqual(r.log.tools,['select']);assert.equal(r.holds.holdingSelect(),true);
  r.doc.emit('keyup',r.ev('Meta'));assert.deepEqual(r.log.tools,['select','draw']);
  r.doc.emit('keyup',r.ev('Meta'));r.win.emit('blur');assert.deepEqual(r.log.tools,['select','draw'],'no second restore');
  assert.equal(state(r).tool,'draw');
});
const state=(r)=>r.state;

test('Ctrl works like Cmd; modifier auto-repeat does not stack a second hold',()=>{
  const r=rig({tool:'eraser'});r.doc.emit('keydown',r.ev('Control'));r.doc.emit('keydown',r.ev('Control'));
  assert.equal(r.tm.pending(),1);r.tm.advance(500);assert.deepEqual(r.log.tools,['select']);
  r.doc.emit('keyup',r.ev('Control'));assert.deepEqual(r.log.tools,['select','eraser']);
});

test('a shortcut combo inside the arm delay cancels the hold; a quick release arms nothing',()=>{
  let r=rig();r.doc.emit('keydown',r.ev('Meta'));r.doc.emit('keydown',r.ev('z'));r.tm.advance(1000);assert.deepEqual(r.log.tools,[]);
  r.doc.emit('keyup',r.ev('Meta'));assert.deepEqual(r.log.tools,[]);
  r=rig();r.doc.emit('keydown',r.ev('Meta'));r.doc.emit('keyup',r.ev('Meta'));assert.equal(r.tm.pending(),0);r.tm.advance(1000);assert.deepEqual(r.log.tools,[]);
});

test('blur while Cmd is armed or already holding restores the tool once; a pending arm is cancelled',()=>{
  let r=rig({tool:'pen'});r.doc.emit('keydown',r.ev('Meta'));r.tm.advance(200);assert.deepEqual(r.log.tools,['select']);
  r.win.emit('blur');assert.deepEqual(r.log.tools,['select','pen']);r.doc.emit('keyup',r.ev('Meta'));r.win.emit('blur');assert.deepEqual(r.log.tools,['select','pen']);
  r=rig();r.doc.emit('keydown',r.ev('Meta'));r.win.emit('blur');assert.equal(r.tm.pending(),0);r.tm.advance(1000);assert.deepEqual(r.log.tools,[]);
  r=rig({tool:'zoom'});r.doc.emit('keydown',r.ev('Meta'));r.tm.advance(200);r.doc.hidden=true;r.doc.emit('visibilitychange');
  assert.deepEqual(r.log.tools,['select','zoom'],'a hidden page is a lost keyup too');
});

test('the Cmd hold is not eligible in a field, while playing, on Select/Subselect, in a pen path or under a Space hold',()=>{
  const cases=[r=>{r.doc.activeElement={tagName:'INPUT'};},r=>{r.state.playing=true;},r=>{r.state.tool='select';},r=>{r.state.tool='subselect';},r=>{r.setPen(true);},r=>{r.state.spaceDown=true;}];
  for(const prep of cases){const r=rig();prep(r);r.doc.emit('keydown',r.ev('Meta'));assert.equal(r.tm.pending(),0);r.tm.advance(1000);assert.equal(r.log.tools.length,0);}
  const r=rig();r.doc.emit('keydown',r.ev('Meta'));r.state.playing=true;r.tm.advance(1000);assert.deepEqual(r.log.tools,[],'state changed during the wait');
});

test('mount replaces nothing it does not own and dispose removes every listener',()=>{
  const doc=Object.assign(bus(),{}),win=bus();const h=Holds.create({state:{},doc,win,setTool(){},togglePlay(){}});const m=h.mount();
  assert.equal(doc.count('keydown')+doc.count('keyup')+doc.count('pointerdown')+doc.count('visibilitychange')+win.count('blur'),5);
  m.dispose();assert.equal(doc.count('keydown')+doc.count('keyup')+doc.count('pointerdown')+doc.count('visibilitychange')+win.count('blur'),0);
});

// ---- flyout ----
function fakeEl(tag){const el={tag,children:[],style:{},className:'',classList:{toggled:{},toggle(c,v){el.classList.toggled[c]=v;}},listeners:{},innerHTML:'',title:'',removed:false,
  addEventListener(t,f){(el.listeners[t]=el.listeners[t]||[]).push(f);},appendChild(c){el.children.push(c);c.parent=el;return c;},remove(){el.removed=true;},
  contains(x){return x===el||el.children.some(c=>c.contains&&c.contains(x));},getBoundingClientRect:()=>({right:40,top:100,width:120,height:40}),
  querySelector(sel){return sel==='.sk'?el.sk||null:null;},closest(){return el;}};return el;}
function railRig(){
  const doc=Object.assign(bus(),{body:fakeEl('body'),buttons:{}}),win=Object.assign(bus(),{innerWidth:1000,innerHeight:800}),tm=fakeTimers();
  Rail.SHAPE_TOOLS.forEach(t=>{const b=fakeEl('button');b.innerHTML='<svg/><span class="sk">X</span>';b.title=t;doc.buttons[t]=b;});
  doc.createElement=fakeEl;doc.querySelector=sel=>{const m=/data-tool="([^"]+)"/.exec(sel);return m?doc.buttons[m[1]]||null:null;};
  const tools=[];const g=Rail.createShapeGroup({doc,win,getTool:()=>'rect',setTool:t=>tools.push(t),setTimer:tm.setTimer,clearTimer:tm.clearTimer});
  const listeners=()=>doc.count('pointerdown')+doc.count('keydown')+win.count('blur');
  return {doc,win,tm,g,tools,listeners};
}

test('flyout: arms after a tick, closes on an outside press, and removes every listener',()=>{
  const r=railRig();r.g.open(r.doc.buttons.rect);assert.equal(r.g.isOpen(),true);assert.equal(r.listeners(),0,'not armed in the opening tick');
  r.tm.advance(0);assert.equal(r.listeners(),3);
  const flyout=r.doc.body.children[0];r.doc.emit('pointerdown',{target:flyout.children[0]});assert.equal(r.g.isOpen(),true,'a press inside keeps it open');
  r.doc.emit('pointerdown',{target:{}});assert.equal(r.g.isOpen(),false);assert.equal(flyout.removed,true);assert.equal(r.listeners(),0);
});

test('flyout: closing before the arming tick leaves no listener behind (the stranded-listener leak)',()=>{
  const r=railRig();r.g.open(r.doc.buttons.rect);r.g.close();r.tm.advance(10);
  assert.equal(r.listeners(),0);assert.equal(r.tm.pending(),0);
  r.g.open(r.doc.buttons.rect);r.g.open(r.doc.buttons.line);r.tm.advance(10);assert.equal(r.listeners(),3,'reopen does not double up');
});

test('flyout: Escape and window blur close it; a pick selects the tool, fronts it and closes',()=>{
  let r=railRig();r.g.open(r.doc.buttons.rect);r.tm.advance(0);r.doc.emit('keydown',{key:'a'});assert.equal(r.g.isOpen(),true);r.doc.emit('keydown',{key:'Escape'});assert.equal(r.g.isOpen(),false);assert.equal(r.listeners(),0);
  r=railRig();r.g.open(r.doc.buttons.rect);r.tm.advance(0);r.win.emit('blur');assert.equal(r.g.isOpen(),false);assert.equal(r.listeners(),0);
  r=railRig();r.g.open(r.doc.buttons.rect);r.tm.advance(0);const item=r.doc.body.children[0].children[2];
  assert.equal(item.innerHTML.includes('class="sk"'),false,'the flyout item drops the key badge');
  item.listeners.click[0]({stopPropagation(){}});
  assert.deepEqual(r.tools,['ellipse']);assert.equal(r.doc.buttons.ellipse.classList.toggled['stack-front'],true);assert.equal(r.doc.buttons.rect.classList.toggled['stack-front'],false);
  assert.equal(r.g.isOpen(),false);assert.equal(r.listeners(),0);
});

test('flyout: a long press opens it, an early release does not, and the click after a long press is swallowed',()=>{
  const r=railRig(),stack=Object.assign(bus(),{});stack.addEventListener=function(t,f,c){(this.m[t]=this.m[t]||[]).push(f);};
  r.g.attach(stack);const down={target:{closest:()=>r.doc.buttons.rect}};
  stack.emit('pointerdown',down);r.tm.advance(100);stack.emit('pointerup');r.tm.advance(1000);assert.equal(r.g.isOpen(),false);
  stack.emit('pointerdown',down);r.tm.advance(Rail.LONG_PRESS_MS);assert.equal(r.g.isOpen(),true);
  let stopped=0;stack.emit('click',{stopImmediatePropagation(){stopped++;}});stack.emit('click',{stopImmediatePropagation(){stopped++;}});assert.equal(stopped,1);
});

// ---- badges ----
function badgeRig(){
  const doc={buttons:{}};const sk={};
  ['select','draw'].forEach(a=>{const s={textContent:'?'};doc.buttons[a]={querySelector:()=>s};sk[a]=s;});
  doc.querySelector=sel=>{const m=/data-tool="([^"]+)"/.exec(sel);return m?doc.buttons[m[1]]||null:null;};
  const store={};const reg=Registry.create({storage:{getItem:k=>store[k]||null,setItem:(k,v)=>{store[k]=v;},removeItem:k=>{delete store[k];}}});
  return {sk,reg,badges:Rail.createBadges({doc,registry:reg})};
}

test('badges show the bound key, follow a remap, and return to the defaults after a reset',()=>{
  const r=badgeRig();r.badges.syncAll();assert.equal(r.sk.select.textContent,'V');assert.equal(r.sk.draw.textContent,'B');
  r.reg.setKey('select','y');r.badges.sync('select');assert.equal(r.sk.select.textContent,'Y');assert.equal(r.sk.draw.textContent,'B');
  r.reg.reset();assert.equal(r.sk.select.textContent,'Y','the registry alone never touches the DOM');
  r.badges.syncAll();assert.equal(r.sk.select.textContent,'V');
  assert.equal(r.badges.sync('nonexistent'),false);
});

// ---- wiring in timeline.js ----
test('timeline.js routes the holds, the flyout and the badge refresh through the modules',()=>{
  assert.match(timeline,/spaceKeyDown\(event\)/);assert.match(timeline,/toolHolds\.altKeyDown\(\)/);assert.match(timeline,/function onKeyUp\(event\)\{toolHolds\.keyUp\(event\);\}/);
  assert.match(timeline,/NemoKeyboardArbiter\.mount\(document,\{keydown:onKeyDown,keyup:onKeyUp\}\);\s*toolHolds\.mount\(\);/);
  assert.match(timeline,/resetShortcuts:function\(\)\{var r=shortcutRegistry\.reset\(\);syncAllToolButtonShortcutBadges\(\);return r;\}/,'a reset refreshes the rail badges');
  assert.doesNotMatch(timeline,/_cmdHold|SPACE_TAP_MS|state\.spaceDown=true/,'no hold logic left inline');
  assert.match(timeline,/window\.SMShapeGroup=\{ensureFront:shapeGroup\.ensureFront\}/);
  const html=fs.readFileSync(path.join(__dirname,'../src/index.html'),'utf8');
  for(const f of ['tool-holds.js','tool-rail.js'])assert.ok(html.indexOf('js/application/'+f)>0&&html.indexOf('js/application/'+f)<html.indexOf('js/timeline.js'),f+' loads before timeline.js');
});
