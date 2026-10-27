'use strict';
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),vm=require('node:vm');
const UI=require('../src/js/adapters/native-subselect-ui.js');
const {fixture,deferred}=require('./helpers/native-subselect.cjs');
function node(){
  const listeners=new Map(),n={children:[],hidden:false,disabled:false,style:{display:''},dataset:{},classList:{remove(){},contains(){return false;}},
    setAttribute(){},replaceChildren(...children){this.children.slice().forEach(child=>this.removeChild(child));children.forEach(child=>this.appendChild(child));},
    appendChild(child){return this.insertBefore(child,null);},
    insertBefore(child,next){if(child.parentNode)child.parentNode.removeChild(child);const i=next?this.children.indexOf(next):this.children.length;assert.ok(i>=0);this.children.splice(i,0,child);child.parentNode=this;return child;},
    removeChild(child){const i=this.children.indexOf(child);assert.ok(i>=0);this.children.splice(i,1);child.parentNode=null;},
    contains(child){return child===this||this.children.some(n=>n.contains(child));},
    addEventListener(type,fn){if(!listeners.has(type))listeners.set(type,[]);listeners.get(type).push(fn);},
    click(){if(!this.disabled)(listeners.get('click')||[]).forEach(fn=>fn());},listeners};
  return n;
}
function ui(){
  const f=fixture(),{root,controller}=f,home=node(),container=node(),row=node(),next=node(),buttons=['corner','smooth','symmetric'].map(()=>node());
  row.style.display='none';row.style.gap='4px';home.appendChild(row);home.appendChild(next);buttons.forEach(b=>row.appendChild(b));
  const nodes={'sp-pointtype-row':row};['corner','smooth','symmetric'].forEach((type,i)=>nodes['btn-pt-'+type]=buttons[i]);
  const get=root.document.getElementById;root.document.getElementById=id=>nodes[id]||get(id);root.document.createComment=node;
  root.NemoNativeSubselectController=controller;root.NemoNativeSubselectUI=UI;
  // Execute the existing production handlers and native routing branch, with a
  // poisoned legacy target. Presentation must retain these exact bound nodes.
  const tools=fs.readFileSync('src/js/tools.js','utf8'),start=tools.indexOf('function setPointType(type){'),end=tools.indexOf('\nfunction ',start+1);
  const context=vm.createContext({window:root,document:root.document,showToast:root.showToast,nodeEditTargetPath(){throw Error('legacy point writer');}});
  vm.runInContext(tools.slice(start,end),context);root.SM={setPointType:context.setPointType};
  const timeline=fs.readFileSync('src/js/timeline.js','utf8');
  vm.runInContext(timeline.split('\n').filter(line=>line.startsWith("document.getElementById('btn-pt-")).join('\n'),context);
  let raf=null;root.requestAnimationFrame=fn=>{raf=fn;};
  return {...f,home,container,row,next,buttons,nodes,tick(){const fn=raf;raf=null;if(fn)fn();},mount:()=>UI.mount(root,container)};
}
async function points(f){await f.select();f.controller.handle('down',f.event(40,24));f.controller.handle('up',f.event(40,24));}
test('existing point-type buttons become available for native points and each sends exactly one guarded command',async()=>{
  for(const [i,type] of ['corner','smooth','symmetric'].entries()){
    const f=ui(),binding=f.mount();assert.equal(f.row.parentNode,f.container);assert.equal(f.row.style.display,'none');
    await f.select();assert.equal(f.row.hidden,true,'a selected path without points is insufficient');
    await points(f);assert.equal(f.row.hidden,false);assert.equal(f.row.style.display,'flex');assert.ok(f.buttons.every(b=>!b.disabled));
    assert.deepEqual(f.controller.selectionState(),{count:1});assert.ok(Object.isFrozen(f.controller.selectionState()));
    assert.equal(f.mount(),binding);assert.equal(f.buttons[i].listeners.get('click').length,1);
    const gate=deferred();f.setGate(gate);f.buttons[i].click();assert.equal(f.row.hidden,true);assert.equal(f.controller.selectionState(),null);
    f.buttons[i].click();gate.resolve();const result=await f.controller.settled();
    assert.equal(result.status,'applied',f.notices.join(';'));assert.equal(result.receipt.historyEntriesAdded,1);assert.equal(f.sent.length,1);
    assert.deepEqual(f.sent[0].payload.intent,{indices:[0],kind:'pointType',pointType:type});assert.equal(f.row.hidden,true);
    binding.retire();assert.deepEqual(f.home.children,[f.row,f.next]);assert.equal(f.row.hidden,false);assert.equal(f.row.style.display,'none');assert.equal(f.row.style.gap,'4px');
    assert.ok(f.buttons.every(b=>!b.disabled));binding.retire();assert.deepEqual(f.home.children,[f.row,f.next]);
  }
});
test('gesture, cancellation, stale owner/view and tool changes hide the same native controls without writes',async()=>{
  for(const change of [f=>f.controller.cancel(),f=>f.controller.handle('cancel',f.event(40,24)),f=>f.replace(),f=>f.root.state.currentFrame++,f=>f.root.state.tool='draw',f=>f.root.state.spaceDown=true,f=>f.root._layerSel=[]]){
    const f=ui();f.mount();await points(f);assert.equal(f.row.hidden,false);change(f);f.tick();
    assert.equal(f.controller.selectionState(),null);assert.equal(f.row.hidden,true);f.buttons[0].click();assert.equal(f.sent.length,0);
  }
  const f=ui();f.mount();await points(f);f.controller.handle('down',f.event(40,24));assert.equal(f.row.hidden,true,'drag');
  f.controller.handle('cancel',f.event(40,24));assert.equal(f.row.hidden,true);assert.equal(f.sent.length,0);
  await points(f);f.controller.handle('down',f.event(0,0));assert.equal(f.row.hidden,true,'marquee read');
  f.controller.cancel();await f.controller.settled();assert.equal(f.sent.length,0);
});
test('mount replacement restores original controls and does not steal a row moved by another presenter',async()=>{
  const f=ui();f.row.hidden=true;f.buttons[1].disabled=true;const first=f.mount();await points(f);
  const secondContainer=node(),second=UI.mount(f.root,secondContainer);assert.notEqual(first,second);assert.equal(f.container.children.length,0);
  first.retire();assert.equal(f.row.parentNode,secondContainer);second.retire();assert.deepEqual(f.home.children,[f.row,f.next]);assert.equal(f.row.hidden,true);assert.equal(f.buttons[1].disabled,true);
  const third=f.mount(),foreign=node();foreign.appendChild(f.row);third.retire();assert.equal(f.row.parentNode,foreign);assert.deepEqual(f.home.children,[f.next]);
});
test('native metadata paints mount the actual point controls and restore them before removing the inspector',async()=>{
  const NativeUI=require('../src/js/adapters/native-object-ui.js'),f=ui(),{root,nodes}=f;
  for(const id of ['props-panel','layer-list','frame-hdr','frame-grid']){nodes[id]=node();nodes[id].id=id;}
  nodes['props-panel'].appendChild(f.home);root.document.createElement=node;
  const get=root.document.getElementById;root.document.getElementById=id=>nodes[id]||(['canvas-area','drawing-canvas'].includes(id)?get(id):null);
  const info={...f.s.info(),totalFrames:2,fps:24},ports={projectionReady:()=>true,currentSession:()=>f.s,selectLayer(){},repaint(){}};
  NativeUI.render(root,info,false,ports);const section=nodes['props-panel'].children.find(n=>n.id==='native-object-inspector');
  assert.ok(section);assert.equal(f.row.parentNode,section);assert.equal(f.home.hidden,true);
  await points(f);assert.equal(f.row.hidden,false);NativeUI.render(root,info,false,ports);assert.equal(f.row.parentNode,section);
  NativeUI.restoreInspector(root);assert.deepEqual(f.home.children,[f.row,f.next]);assert.equal(f.home.hidden,false);assert.equal(section.parentNode,null);
  NativeUI.render(root,info,false,ports);assert.notEqual(f.row.parentNode,section);
  nodes['props-panel']=node();NativeUI.render(root,info,false,ports);assert.equal(f.row.parentNode.id,'native-object-inspector');
  NativeUI.restoreInspector(root);assert.deepEqual(f.home.children,[f.row,f.next]);assert.ok(f.buttons.every(b=>b.listeners.get('click').length===1));
});
