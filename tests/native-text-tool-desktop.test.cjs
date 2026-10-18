'use strict';
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const Desktop=require('../src/js/adapters/native-text-tool-desktop.js');
const {fixture:portsFixture,block,target,deferred}=require('./native-text-tool-support.cjs');
const turn=()=>new Promise(setImmediate);
class Element{
 constructor(tag,doc){this.tagName=tag.toUpperCase();this.ownerDocument=doc;this.children=[];this.listeners=new Map();this.style={};this.attributes={};this.value='';this.disabled=false;this.hidden=false;this.textContent='';this.classes=new Set();this.classList={add:s=>this.classes.add(s),contains:s=>this.classes.has(s),remove:s=>this.classes.delete(s)};}
 appendChild(n){if(n.parentNode)n.parentNode.removeChild(n);n.parentNode=this;this.children.push(n);return n;}
 removeChild(n){this.children.splice(this.children.indexOf(n),1);n.parentNode=null;return n;}
 replaceChildren(...nodes){for(const n of [...this.children])this.removeChild(n);nodes.forEach(n=>this.appendChild(n));}
 setAttribute(k,v){this.attributes[k]=String(v);if(k==='id')this.id=v;}getAttribute(k){return this.attributes[k];}
 contains(n){return this===n||this.children.some(c=>c.contains(n));}
 addEventListener(k,fn,o){const capture=typeof o==='boolean'?o:!!o?.capture;const list=this.listeners.get(k)||[];list.push({fn,capture});this.listeners.set(k,list);}
 removeEventListener(k,fn,o){const capture=typeof o==='boolean'?o:!!o?.capture;this.listeners.set(k,(this.listeners.get(k)||[]).filter(x=>x.fn!==fn||x.capture!==capture));}
 focus(){this.ownerDocument.activeElement=this;}
 fire(kind,data={}){const e={target:this,type:kind,key:'',button:0,pointerId:1,isPrimary:true,buttons:kind==='pointerup'?0:1,clientX:100,clientY:100,...data,stopped:false,prevented:false,stopImmediatePropagation(){this.stopped=true;},preventDefault(){this.prevented=true;}};
  const ancestors=[];for(let p=this.parentNode;p;p=p.parentNode)ancestors.push(p);
  const run=(n,capture)=>{e.currentTarget=n;for(const r of n.listeners.get(kind)||[]){if(r.capture===capture)r.fn(e);if(e.stopped)return;}};
  for(const n of ancestors.slice().reverse()){run(n,true);if(e.stopped)return e;}run(this,true);if(!e.stopped)run(this,false);if(!e.stopped)for(const n of ancestors){run(n,false);if(e.stopped)break;}return e;
 }
}
async function fixture(selectionIntent=false,desktopModule=Desktop,options={}){
 const f=portsFixture(options),doc=new Element('document');doc.ownerDocument=doc;doc.readyState='complete';doc.createElement=tag=>new Element(tag,doc);doc.body=doc.createElement('body');doc.appendChild(doc.body);doc.documentElement=doc.createElement('html');doc.documentElement.clientWidth=1200;doc.documentElement.clientHeight=800;doc.documentElement.classList.add('nemo-native-underlay');
 function find(n,id){if(n.id===id)return n;for(const c of n.children){const r=find(c,id);if(r)return r;}return null;}doc.getElementById=id=>find(doc.body,id);
 const add=(tag,id,parent=doc.body)=>{const n=doc.createElement(tag);n.id=id;parent.appendChild(n);return n;};
 const stage=add('div','canvas-area'),canvas=add('canvas','drawing-canvas',stage),zoom=add('input','zoom-scrub',stage),container=add('div','native-inspector');let rect={left:20,top:30,width:640,height:360};canvas.getBoundingClientRect=()=>({...rect});const published={'--nemo-native-left':'20px','--nemo-native-top':'30px','--nemo-native-right':'660px','--nemo-native-bottom':'390px'};doc.documentElement.style.getPropertyValue=k=>published[k];
 const pop=add('div','text-popover');pop.style.display='none';for(const [tag,id]of [['textarea','text-input'],['input','text-size'],['select','text-font'],['input','text-color'],['select','text-align'],['button','text-apply'],['button','text-cancel']])add(tag,id,pop);
 const root=new Element('window',doc);root.document=doc;doc.parentNode=root;root.state={appMode:'anim2d',tool:'text',activeLayerIdx:0,currentFrame:0,strokeColor:'#123456',layers:[{layerUid:'L0'}]};root._layerSel=[0];root.window=root;
 const baseInfo=f.session.info;f.session.info=()=>({...baseInfo(),canvasW:320,canvasH:180,totalFrames:8,layers:[{layerUid:'L0',locked:false}]});f.session.readPresentedObjectSession=()=>({...f.session.info(),presentation:{frame:root.state.currentFrame}});
 const logs={legacy:0,zoom:0,selected:0};let intent;zoom.addEventListener('pointerdown',()=>logs.zoom++);canvas.addEventListener('pointerdown',()=>logs.legacy++);
 doc.getElementById('text-apply').addEventListener('click',()=>logs.legacy++);
 root.NemoNativeTextToolDesktop=desktopModule;root.NemoNativeOpacityCutover={blocksLegacy:()=>true};root.NemoSelectCanvasIntent={create(){if(!selectionIntent)throw new Error('Text reached generic native selection route');intent=require('../src/js/adapters/select-canvas-intent.js').create({context:()=> 'stable',probe:()=>Object.freeze({mode:'clear'}),select:()=>logs.selected++});return intent;}};
 const binding=desktopModule.mount(root,container,{currentSession:()=>f.session,fontResources:()=>({list:async()=>[{assetId:'document font / 雪',name:'Imported',available:true}]})});await turn();
 const script=fs.readFileSync(path.join(__dirname,'../src/js/select-bridge.js'),'utf8');
 vm.runInNewContext(script,{window:root,document:doc,state:root.state,console,selectedPaths:[],userLayers:[],project:{layers:[]},view:{zoom:1},setTimeout,clearTimeout},{filename:'actual-select-bridge.js'});
 const get=id=>doc.getElementById(id);
 return {...f,root,doc,get,binding,canvas,zoom,logs,turn,intent:()=>intent,resize:r=>{rect=r;},input:(id,value)=>{get(id).value=value;get(id).fire('input');},apply:()=>get('text-apply').fire('click'),
  click:async(x=100,y=100)=>{canvas.fire('pointerdown',{clientX:x,clientY:y});canvas.fire('pointerup',{clientX:x,clientY:y});await turn();}};
}
test('actual select-bridge capture routes ordinary point text to existing editor and suppresses legacy writers',async()=>{
 const f=await fixture();await f.click(100,110);assert.equal(f.get('text-popover').style.display,'block');assert.equal(f.logs.legacy,0);
 f.input('text-input','Native point');f.apply();await f.turn();assert.equal(f.owner.log.executed.length,1);assert.equal(f.logs.legacy,0);
 assert.deepEqual(f.owner.log.received[0].payload.block.anchorTopLeft,{x:40,y:40});assert.equal(f.owner.log.received[0].payload.block.fixedWidth,null);
});
test('drag-a-box types in a DOM draft then creates one native area block without Paper placeholder content',async()=>{
 const f=await fixture();f.canvas.fire('pointerdown',{clientX:100,clientY:110});f.canvas.fire('pointermove',{clientX:300,clientY:190});f.canvas.fire('pointerup',{clientX:300,clientY:190});await f.turn();
 const area=f.get('native-text-area-draft');assert.ok(area);assert.equal(f.owner.items.size,0);area.value='Area text';area.fire('input');area.fire('keydown',{key:'Enter',ctrlKey:true});await f.turn();
 assert.equal(f.owner.log.executed.length,1);assert.equal(f.owner.log.received[0].payload.block.fixedWidth,100);assert.equal(f.owner.log.received[0].payload.block.text,'Area text');assert.equal(f.logs.legacy,0);
});
test('stage chrome controls stay live while Text mode is active',async()=>{
 const f=await fixture();const e=f.zoom.fire('pointerdown');assert.equal(f.logs.zoom,1);assert.equal(e.stopped,false);assert.equal(e.prevented,false);assert.equal(f.owner.log.executed.length,0);assert.equal(f.logs.legacy,0);
});
test('existing and reopened native text rows bind exact document font identity through the existing font control',async()=>{
 const f=await fixture(),t=target('reopened text / 雪');f.seed(t,block('Saved text'));await f.binding.refresh(undefined,true);
 const select=f.get('native-text-controls').children.find(n=>n.tagName==='SELECT');select.value='0';select.fire('change');await f.turn();
 assert.equal(f.get('text-input').value,'Saved text');const font=f.get('text-font');const asset=font.children.find(n=>n.textContent.includes('document font / 雪'));assert.ok(asset);
 f.input('text-font',asset.value);f.apply();await f.turn();assert.equal(f.owner.log.executed.length,1);assert.deepEqual(f.owner.log.received[0].payload.patch,{font:{kind:'asset',assetId:'document font / 雪'}});
 assert.deepEqual(f.owner.log.received[0].payload.stableTarget,t);assert.deepEqual(f.owner.items.get(t.strokeId).anchorTopLeft,{x:10,y:20});assert.equal(f.owner.items.get(t.strokeId).bold,true);
});
test('Escape, pointercancel and lost capture cancel the intent without opening or dispatching a text draft',async()=>{
 for(const event of ['pointercancel','lostpointercapture']){const f=await fixture();f.canvas.fire('pointerdown');f.canvas.fire(event);f.canvas.fire('pointerup');await f.turn();assert.equal(f.binding.core.snapshot().draft,null);assert.equal(f.owner.log.executed.length,0);}
 const f=await fixture();f.canvas.fire('pointerdown');f.doc.fire('keydown',{key:'Escape'});f.canvas.fire('pointerup');await f.turn();assert.equal(f.binding.core.snapshot().draft,null);
});
test('resize, owner advance and unpresented contexts cannot commit a stale visible editor',async()=>{
 const f=await fixture();await f.click();f.input('text-input','Stale');f.resize({left:20,top:30,width:600,height:360});f.apply();await f.turn();assert.equal(f.owner.log.executed.length,0);assert.equal(f.logs.legacy,0);
 const g=await fixture();g.session.readPresentedObjectSession=()=>null;await g.click();assert.equal(g.binding.core.snapshot().draft,null);assert.equal(g.owner.log.executed.length,0);
});
test('closing a native editor with Apply/Cancel never reaches preexisting legacy callbacks',async()=>{
 const f=await fixture();f.get('text-apply').fire('click');assert.equal(f.logs.legacy,0);await f.click();f.get('text-cancel').fire('click');assert.equal(f.get('text-popover').style.display,'none');assert.equal(f.owner.log.executed.length,0);
});

test('switching Select to Text retires the prior captured selection intent before Select resumes',async()=>{
 const f=await fixture(true);f.root.state.tool='select';f.canvas.fire('pointerdown',{pointerId:7,buttons:1});assert.ok(f.intent().snapshot());
 f.root.state.tool='text';f.canvas.fire('pointermove',{pointerId:7,buttons:1});assert.equal(f.intent().snapshot(),null);
 f.root.state.tool='select';f.canvas.fire('pointerup',{pointerId:7,buttons:0});assert.equal(f.logs.selected,0);assert.equal(f.owner.log.executed.length,0);
});

test('actual bootstrap registers text dependencies once and preserves property helper to SDK adjacency',()=>{
 const html=fs.readFileSync(path.join(__dirname,'../src/index.html'),'utf8'),scripts=[...html.matchAll(/<script\b[^>]*\bsrc="([^"]+)"/g)].map(m=>m[1]);
 assert.equal(new Set(scripts).size,scripts.length);
 const at=name=>{const key='js/adapters/'+name+'.js';assert.equal(scripts.filter(s=>s===key).length,1);return scripts.indexOf(key);};
 assert.equal(at('native-property-contract')+1,at('native-application'));
 assert.ok(at('native-text-read-contract')<at('native-application'));assert.ok(at('native-application')<at('native-text-read'));
 assert.ok(at('native-object-item-command')<at('native-object-text-layout'));assert.ok(at('native-object-text-layout')<at('native-text-tool'));
 assert.equal(at('native-text-read')+1,at('native-text-tool'));assert.equal(at('native-text-tool')+1,at('native-text-tool-desktop'));
});

test('normal release capture and foreign cancellation preserve the matching completed pointer discovery',async()=>{
 const f=await fixture(),gate=require('./native-text-tool-support.cjs').deferred();f.hold(gate.promise);
 f.canvas.fire('pointerdown',{pointerId:9});f.canvas.fire('pointerup',{pointerId:9});f.canvas.fire('lostpointercapture',{pointerId:9});f.canvas.fire('pointercancel',{pointerId:3,isPrimary:false});
 gate.resolve();await f.turn();assert.ok(f.binding.core.snapshot().draft);assert.equal(f.get('text-popover').style.display,'block');assert.equal(f.owner.log.executed.length,0);
});
test('tool retirement while discovery is pending rejects the late draft after returning to Text',async()=>{
 const f=await fixture(true),gate=require('./native-text-tool-support.cjs').deferred();f.hold(gate.promise);f.canvas.fire('pointerdown');f.canvas.fire('pointerup');
 f.root.state.tool='select';f.canvas.fire('pointermove');f.root.state.tool='text';gate.resolve();await f.turn();assert.equal(f.binding.core.snapshot().draft,null);assert.equal(f.get('text-popover').style.display,'none');
});
test('nonprimary, modified and nonleft Text downs cannot stage a draft',async()=>{
 for(const props of [{isPrimary:false},{ctrlKey:true},{metaKey:true},{altKey:true},{shiftKey:true},{button:2,buttons:2},{buttons:0}]){
  const f=await fixture();f.canvas.fire('pointerdown',props);f.canvas.fire('pointerup');await f.turn();assert.equal(f.binding.core.snapshot().draft,null);assert.equal(f.owner.log.executed.length,0);
 }
});

test('release from a foreign pointer cannot finish a captured Text gesture',async()=>{
 const f=await fixture();f.canvas.fire('pointerdown',{pointerId:1,isPrimary:true,clientX:100,clientY:110});f.canvas.fire('pointerup',{pointerId:2,isPrimary:false,clientX:300,clientY:190});await f.turn();
 assert.equal(f.binding.core.snapshot().draft,null);assert.equal(f.get('text-popover').style.display,'none');assert.equal(f.owner.log.executed.length,0);
});
test('Text→Select→Text cannot resurrect a retired Text pointerdown',async()=>{
 const f=await fixture(true);f.canvas.fire('pointerdown',{pointerId:7,isPrimary:true});f.root.state.tool='select';f.canvas.fire('pointermove',{pointerId:7,buttons:1});f.root.state.tool='text';f.canvas.fire('pointerup',{pointerId:7,buttons:0});await f.turn();
 assert.equal(f.binding.core.snapshot().draft,null);assert.equal(f.get('text-popover').style.display,'none');assert.equal(f.owner.log.executed.length,0);
});
for(const cancel of ['Escape','pointercancel'])test(cancel+' retires pending discovery before late draft publication',async()=>{
 const f=await fixture(),gate=deferred();f.hold(gate.promise);f.canvas.fire('pointerdown',{pointerId:1});f.canvas.fire('pointerup',{pointerId:1});await Promise.resolve();assert.equal(f.binding.core.snapshot().busy,true);
 if(cancel==='Escape')f.doc.fire('keydown',{key:'Escape'});else f.canvas.fire('pointercancel',{pointerId:1});gate.resolve();await f.turn();
 assert.equal(f.binding.core.snapshot().draft,null);assert.equal(f.get('text-popover').style.display,'none');assert.equal(f.owner.log.executed.length,0);
});

test('late document font enumeration cannot overwrite the user-selected bundled font',async()=>{
 const f=await fixture(),gate=deferred();f.binding.ports.fontResources=()=>({list:()=>gate.promise});await f.click();
 const font=f.get('text-font'),bold=font.children.find(n=>n.textContent==='Roboto-Bold');assert.ok(bold);f.input('text-font',bold.value);
 gate.resolve([{assetId:'document font / 雪',name:'Imported',available:true}]);await f.turn();f.input('text-input','User chose Bold');f.apply();await f.turn();
 assert.equal(f.owner.log.executed.length,1);assert.deepEqual(f.owner.log.received[0].payload.block.font,{kind:'bundled',fontKey:'Roboto-Bold'});
});
test('a changed canvas DOM rectangle cannot be used before the native underlay confirms its new bounds',async()=>{
 const f=await fixture();const published={'--nemo-native-left':'20px','--nemo-native-top':'30px','--nemo-native-right':'660px','--nemo-native-bottom':'390px'};
 f.doc.documentElement.style.getPropertyValue=k=>published[k];f.resize({left:120,top:30,width:640,height:360});await f.click(140,110);
 assert.equal(f.binding.core.snapshot().draft,null);assert.equal(f.get('text-popover').style.display,'none');assert.equal(f.owner.log.executed.length,0);
});

test('retired queued completion cannot disable a replacement owner editor sharing the popover',async()=>{
 const f=await fixture();await f.click();f.input('text-input','Old queued text');const gate=deferred();f.hold(gate.promise);f.apply();await Promise.resolve();assert.equal(f.binding.core.snapshot().busy,true);
 f.binding.retire();
 const next=portsFixture();next.state.document='document-B';next.state.generation++;
 const originalInfo=next.session.info;next.session.info=()=>({...originalInfo(),canvasW:320,canvasH:180,totalFrames:8,layers:[{layerUid:'L0',locked:false}]});next.session.readPresentedObjectSession=()=>({...next.session.info(),presentation:{frame:0}});
 const binding=Desktop.mount(f.root,f.binding.container,{currentSession:()=>next.session,fontResources:()=>({list:async()=>[]})});await f.turn();await binding.core.openNew({anchorTopLeft:{x:20,y:20},fixedWidth:null});
 assert.equal(f.get('text-apply').disabled,false,'replacement draft is ready before old queue settles');assert.ok(binding.core.snapshot().draft);
 gate.resolve();await f.turn();
 console.log(JSON.stringify({newDraft:!!binding.core.snapshot().draft,newBusy:binding.core.snapshot().busy,disabled:f.get('text-apply').disabled,oldCommands:f.owner.log.executed.length,newCommands:next.owner.log.executed.length}));
 assert.equal(f.get('text-apply').disabled,false,'old callbacks must not disable the new owner editor');
});


test('browser Text bootstrap injects actual existing reader and writer modules into the pure core',async()=>{
 // VM objects cross into the Node-realm SDK through the fixture's explicit JSON boundary.
 const browser={NemoNativeTextRead:require('../src/js/adapters/native-text-read.js'),NemoNativeObjectTextLayout:require('../src/js/adapters/native-object-text-layout.js'),NemoNativeObjectItemContract:require('../src/js/adapters/native-object-item-contract.js')};
 const coreSource=fs.readFileSync(path.join(__dirname,'../src/js/adapters/native-text-tool.js'),'utf8');
 const desktopSource=fs.readFileSync(path.join(__dirname,'../src/js/adapters/native-text-tool-desktop.js'),'utf8');
 vm.runInNewContext(coreSource,browser,{filename:'actual-native-text-tool.js'});
 vm.runInNewContext(desktopSource,browser,{filename:'actual-native-text-tool-desktop.js'});
 const f=await fixture(false,browser.NemoNativeTextToolDesktop,{browserJsonBoundary:true});await f.click();assert.equal(f.get('text-popover').style.display,'block',JSON.stringify(f.binding.core.snapshot()));f.input('text-input','Browser ports');f.apply();await f.turn();
 assert.equal(f.owner.log.executed.length,1,JSON.stringify({state:f.binding.core.snapshot(),status:f.get('native-text-controls').children[2].textContent,received:f.owner.log.received}));assert.equal(f.owner.log.received[0].payload.block.text,'Browser ports');assert.equal(f.logs.legacy,0);
 for(const missing of ['NemoNativeTextRead','NemoNativeObjectTextLayout']){
   const absent={...browser,[missing]:null};vm.runInNewContext(desktopSource,absent,{filename:'missing-port-desktop.js'});
   const refused=await fixture(false,absent.NemoNativeTextToolDesktop);assert.equal(refused.binding,null);
   assert.equal(refused.owner.log.received.length,0);assert.equal(refused.owner.log.executed.length,0);
   assert.equal(refused.get('native-text-controls'),null);
 }
});
