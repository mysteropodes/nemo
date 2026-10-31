'use strict';
// #2356 row C/C2 hands-on: the vertex selection survives every edit, Alt-drag pulls tangents, the point-type buttons reach the native owner, and a drag redraws locally.
const test=require('node:test'),assert=require('node:assert/strict');
const UI=require('../src/js/adapters/native-subselect-ui.js');
const {fixture,deferred}=require('./helpers/native-subselect.cjs');
const settle=()=>new Promise(setImmediate);
async function points(f){await f.select();f.controller.handle('down',f.event(40,24));f.controller.handle('up',f.event(40,24));await f.controller.settled();}

test('the edited item and its vertices stay selected after a move, so the next edit needs no reselect',async()=>{
  const f=fixture();await points(f);f.drag();f.controller.handle('up',f.event(52,34));
  assert.equal((await f.controller.settled()).status,'applied',f.notices.join(';'));
  assert.deepEqual(f.controller.selectionState(),{count:1});
  assert.equal((await f.controller.setPointType('smooth')).status,'applied',f.notices.join(';'));
  assert.equal(f.sent.length,2);assert.deepEqual(f.sent[1].payload.intent,{kind:'pointType',pointType:'smooth',indices:[0]});
  assert.deepEqual(f.controller.selectionState(),{count:1},'and again after the point type edit');
});
test('the selection survives a commit whose new revision reaches the canvas several reads after the info pin (installed timing)',async()=>{
  const f=fixture();await points(f);f.lagPresentation(6);f.drag();f.controller.handle('up',f.event(52,34));
  assert.equal((await f.controller.settled()).status,'applied',f.notices.join(';'));
  assert.deepEqual(f.controller.selectionState(),{count:1},'restored once the canvas caught up');
  f.lagPresentation(6);assert.equal((await f.controller.setPointType('symmetric')).status,'applied',f.notices.join(';'));
  assert.deepEqual(f.sent.at(-1).payload.intent,{kind:'pointType',pointType:'symmetric',indices:[0]});assert.deepEqual(f.controller.selectionState(),{count:1});
});
test('an applied edit keeps its handles on screen until the re-read selection replaces them (no blink)',async()=>{
  const f=fixture();await points(f);assert.equal(f.live(),1);f.liveLog();
  f.lagPresentation(6);assert.equal((await f.controller.setPointType('symmetric')).status,'applied',f.notices.join(';'));
  assert.ok(!f.liveLog().includes(0),'the overlay never left the screen');assert.equal(f.live(),1);
  f.lagPresentation(6);f.drag();f.controller.handle('up',f.event(52,34));assert.equal((await f.controller.settled()).status,'applied');
  assert.ok(!f.liveLog().includes(0),'a vertex drag does not blink either');assert.equal(f.live(),1);
  const g=fixture();await points(g);g.failTransport(2);await g.controller.setPointType('smooth');assert.equal(g.live(),0,'a failed edit still clears');
});
test('a tangent drag, delete-less scale and rotate keep the selected indices',async()=>{
  const f=fixture();f.item.segments[0].handleOut=[10,0];await points(f);
  const p=[20+10,20];const q=(x,y)=>[1.5*x+.5*y,.2*x+y];
  f.controller.handle('down',f.event(...q(...p)));f.controller.handle('move',f.event(...q(40,25)));f.controller.handle('up',f.event(...q(40,25)));
  assert.equal((await f.controller.settled()).status,'applied',f.notices.join(';'));assert.equal(f.sent.at(-1).payload.intent.kind,'handle');assert.deepEqual(f.controller.selectionState(),{count:1});
  assert.equal((await f.controller.scale([2,2],[0,0])).status,'applied');assert.equal((await f.controller.rotate(10,[0,0])).status,'applied');
  assert.deepEqual(f.sent.at(-1).payload.intent.indices,[0]);assert.deepEqual(f.controller.selectionState(),{count:1});
});
test('an Alt insert selects the new vertex and a delete leaves the item selected with no vertex',async()=>{
  const f=fixture();await points(f);
  f.controller.handle('down',f.event(85,30,{altKey:true}));assert.equal((await f.controller.settled()).status,'applied',f.notices.join(';'));
  const insert=f.sent[0].payload.intent;assert.equal(insert.kind,'insert');assert.deepEqual(f.controller.selectionState(),{count:1});
  await f.controller.setPointType('corner');assert.deepEqual(f.sent[1].payload.intent.indices,[insert.curve+1]);
  f.controller.key({key:'Delete',preventDefault(){},stopImmediatePropagation(){}});assert.equal((await f.controller.settled()).status,'applied');
  assert.equal(f.sent[2].payload.intent.kind,'delete');assert.equal(f.controller.selectionState(),null,'no vertex left to act on');
  f.controller.handle('down',f.event(40,24));f.controller.handle('up',f.event(40,24));await f.controller.settled();assert.deepEqual(f.controller.selectionState(),{count:1},'the item stayed selected: one click picks a vertex');
});
test('a refusal, a stale frame or an indeterminate outcome clears the selection',async()=>{
  const f=fixture(),gate=deferred();await points(f);f.setGate(gate);const run=f.controller.setPointType('smooth');f.setFrame(1);f.root.state.currentFrame=1;gate.resolve();await run;
  assert.equal(f.controller.selectionState(),null);
  const g=fixture();await points(g);g.failTransport(2);assert.equal((await g.controller.setPointType('smooth')).status,'indeterminate');assert.equal(g.controller.selectionState(),null);
  const h=fixture();h.root.state.tool='draw';assert.equal((await h.controller.setPointType('smooth')).status,'refused');assert.equal(h.controller.selectionState(),null);
});
test('Alt-drag from a vertex pulls a symmetric tangent pair and commits once at release; Alt+click still toggles',async()=>{
  const f=fixture();await points(f);
  f.controller.handle('down',f.event(40,24,{altKey:true}));f.controller.handle('move',f.event(52,34,{altKey:true}));await settle();
  const live=f.previews.at(-1);assert.equal(live.kind,'handle');assert.equal(live.index,0);assert.equal(live.handle,'handleOut');assert.equal(live.breakPair,false);assert.equal(f.sent.length,0);
  f.controller.handle('move',f.event(60,40,{altKey:true}));await settle();f.controller.handle('up',f.event(60,40,{altKey:true}));
  assert.equal((await f.controller.settled()).status,'applied',f.notices.join(';'));assert.equal(f.sent.length,1);assert.equal(f.sent[0].payload.intent.kind,'handle');assert.equal(f.sent[0].payload.intent.breakPair,false);
  assert.deepEqual(f.controller.selectionState(),{count:1});
  const g=fixture();await points(g);g.controller.handle('down',g.event(40,24,{altKey:true}));g.controller.handle('up',g.event(40,24,{altKey:true}));await g.controller.settled();
  assert.deepEqual(g.sent[0].payload.intent,{kind:'toggle',index:0});
});
test('a drag redraws locally on every move while the engine preview stays one in flight with the latest position',async()=>{
  const f=fixture();await points(f);let inFlight=0,most=0,calls=0,svgs=0;const latch=[];
  f.root.document.createElementNS=(ns,tag)=>{if(tag==='svg')svgs++;return {style:{},setAttribute(){},appendChild(){},remove(){}};};
  f.s.previewSubselect=async(selector,target,intent)=>{calls++;inFlight++;most=Math.max(most,inFlight);const g=deferred();latch.push(g);await g.promise;inFlight--;f.previews.push(intent);return {info:f.s.info(),presentation:f.root.NemoNativeViewportUnderlay&&f.s.readPresentedObjectSession().presentation,segments:f.item.segments,matrix:f.item.matrix};};
  f.controller.handle('down',f.event(40,24));const before=svgs;
  for(let i=1;i<=40;i++)f.controller.handle('move',f.event(40+i,24+i));
  assert.ok(svgs-before>=40,'each move paints the overlay without waiting for the engine');
  await settle();assert.equal(calls,1);assert.equal(most,1);
  latch[0].resolve();await settle();await settle();assert.equal(calls,2,'one more preview, for the latest position only');assert.equal(most,1);
  latch[1].resolve();await settle();assert.equal(f.previews.length,2);
  f.controller.handle('up',f.event(80,64));await f.controller.settled();assert.equal(f.sent.length,1);
});

function node(){
  const listeners=new Map(),n={children:[],hidden:false,disabled:false,style:{display:''},dataset:{},classList:{remove(){},toggle(){},contains(){return false;}},setAttribute(){},
    appendChild(child){return this.insertBefore(child,null);},
    insertBefore(child,next){if(child.parentNode)child.parentNode.removeChild(child);const i=next?this.children.indexOf(next):this.children.length;this.children.splice(i,0,child);child.parentNode=this;return child;},
    removeChild(child){const i=this.children.indexOf(child);this.children.splice(i,1);child.parentNode=null;},contains(child){return child===this||this.children.some(c=>c.contains(child));},
    closest(){return this;},addEventListener(type,fn,capture){if(!listeners.has(type))listeners.set(type,[]);listeners.get(type).push({fn,capture:!!capture});},
    removeEventListener(type,fn,capture){const l=listeners.get(type)||[],i=l.findIndex(x=>x.fn===fn&&x.capture===!!capture);if(i>=0)l.splice(i,1);},listeners};
  return n;
}
test('the point-type buttons reach the native owner through the row even though the legacy SM entry refuses silently',async()=>{
  const f=fixture(),{root}=f,container=node(),row=node(),buttons=['corner','smooth','symmetric','collapse'].map(()=>node()),ids=['btn-pt-corner','btn-pt-smooth','btn-pt-symmetric','btn-pt-collapse'];
  buttons.forEach((b,i)=>{b.id=ids[i];row.appendChild(b);});node().appendChild(row);
  const nodes={'sp-pointtype-row':row};ids.forEach((id,i)=>nodes[id]=buttons[i]);const get=root.document.getElementById;root.document.getElementById=id=>nodes[id]||get(id);root.document.createComment=node;
  let legacy=0;root.SM={setPointType(){legacy++;return false;}};
  buttons.forEach((b,i)=>b.addEventListener('click',()=>root.SM.setPointType(['corner','smooth','symmetric','collapse'][i])));// what timeline.js binds
  root.NemoNativeSubselectController=f.controller;root.NemoNativeSubselectUI=UI;const binding=UI.mount(root,container);
  const click=b=>{let stopped=false;const e={target:b,stopImmediatePropagation(){stopped=true;},preventDefault(){}};
    for(const l of row.listeners.get('click')||[])if(l.capture&&!stopped)l.fn(e);if(!stopped)(b.listeners.get('click')||[]).forEach(l=>l.fn(e));};
  click(buttons[0]);await settle();assert.equal(f.sent.length,0,'no vertex selected: nothing is sent');assert.ok(f.notices.length>0,'and the refusal is a toast');assert.equal(legacy,0);
  await points(f);assert.equal(row.hidden,false);
  for(const [i,type] of ['corner','smooth','symmetric','collapse'].entries()){click(buttons[i]);await f.controller.settled();assert.deepEqual(f.sent.at(-1).payload.intent,{kind:'pointType',pointType:type,indices:[0]});}
  assert.equal(f.sent.length,4);assert.equal(legacy,0);assert.deepEqual(f.controller.selectionState(),{count:1});
  binding.retire();assert.equal((row.listeners.get('click')||[]).length,0);
});
