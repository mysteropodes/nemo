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
 fire(kind,data={}){const e={target:this,type:kind,bubbles:kind!=='blur'&&kind!=='focus',key:'',button:0,pointerId:1,isPrimary:true,buttons:kind==='pointerup'?0:1,clientX:100,clientY:100,...data,stopped:false,prevented:false,stopImmediatePropagation(){this.stopped=true;},preventDefault(){this.prevented=true;}};
  const ancestors=[];for(let p=this.parentNode;p;p=p.parentNode)ancestors.push(p);
  const run=(n,capture)=>{e.currentTarget=n;for(const r of n.listeners.get(kind)||[]){if(r.capture===capture)r.fn(e);if(e.stopped)return;}};
  for(const n of ancestors.slice().reverse()){run(n,true);if(e.stopped)return e;}run(this,true);if(!e.stopped)run(this,false);if(!e.stopped&&e.bubbles)for(const n of ancestors){run(n,false);if(e.stopped)break;}return e;
 }
}
async function fixture(selectionIntent=false,desktopModule=Desktop,options={}){
 const f=portsFixture(options),doc=new Element('document');doc.ownerDocument=doc;doc.readyState='complete';doc.createElement=tag=>new Element(tag,doc);doc.body=doc.createElement('body');doc.appendChild(doc.body);doc.documentElement=doc.createElement('html');doc.documentElement.clientWidth=1200;doc.documentElement.clientHeight=800;doc.documentElement.classList.add('nemo-native-underlay');
 function find(n,id){if(n.id===id)return n;for(const c of n.children){const r=find(c,id);if(r)return r;}return null;}doc.getElementById=id=>find(doc.body,id);
 const add=(tag,id,parent=doc.body)=>{const n=doc.createElement(tag);n.id=id;parent.appendChild(n);return n;};
 const stage=add('div','canvas-area'),canvas=add('canvas','drawing-canvas',stage),zoom=add('input','zoom-scrub',stage),container=add('div','native-inspector');let rect={left:20,top:30,width:640,height:360};canvas.getBoundingClientRect=()=>({...rect});const published={'--nemo-native-left':'20px','--nemo-native-top':'30px','--nemo-native-right':'660px','--nemo-native-bottom':'390px'};doc.documentElement.style.getPropertyValue=k=>published[k];
 const pop=add('div','text-popover');pop.style.display='none';for(const [tag,id]of [['textarea','text-input'],['input','text-size'],['select','text-font'],['input','text-color'],['select','text-align'],['button','text-apply'],['button','text-cancel']])add(tag,id,pop);
 const root=new Element('window',doc);root.document=doc;doc.parentNode=root;root.state={appMode:'anim2d',tool:'text',activeLayerIdx:0,currentFrame:0,strokeColor:'#123456',layers:[{layerUid:'L0'}]};root._layerSel=[0];root.window=root;
 const baseInfo=f.session.info;f.session.info=()=>({...baseInfo(),canvasW:320,canvasH:180,totalFrames:8,layers:[{layerUid:'L0',locked:false}]});root.state.cameraView=options.cameraView?.mode==='stored-camera';
 const pinKeys=['instanceId','documentId','lifecycleGeneration','contentRevision','documentSnapshotId'];
 let running=0,presentation=null,presentationKey=null,lastLease=null;
 function shown(){const pin=f.session.info(),key=JSON.stringify([pinKeys.map(k=>pin[k]),root.state.currentFrame,root.state.cameraView,options.cameraView]);
  if(key!==presentationKey){presentationKey=key;presentation={...pin,owner:'native',status:'presented',contextId:'scene-root',frame:root.state.currentFrame,workId:'work-'+pin.contentRevision,viewGeneration:1,...(options.cameraView?{view:options.cameraView}:{})};}
  return {...pin,presentation};}
 f.session.readPresentedObjectSession=why=>{if(running){if(why)why('queue-pending');return null;}return shown();};
 const runResource=f.session.runResourceCommand;
 f.session.runResourceCommand=action=>{const origin=f.session.readPresentedObjectSession();running++;
  return runResource(async ctx=>{let live=true;const lease={...ctx,readPresentedObjectSession(){
    if(!live||running!==1||!origin||!f.session.isActive())return null;const next=shown();
    if(pinKeys.some(k=>origin[k]!==next[k])||origin.presentation!==next.presentation)return null;
    if(options.actionReader==='refused')return null;
    if(options.actionReader==='foreign')return {...origin,documentId:'foreign'};
    if(options.actionReader==='view')return {...origin,presentation:{...origin.presentation,view:{mode:'canvas',navigation:{zoom:2,pan:[0,0]}}}};
    return origin;}};
   if(options.actionReader==='missing')delete lease.readPresentedObjectSession;lastLease=lease;
   try{return await action(lease);}finally{live=false;}
  }).finally(()=>{running--;});};
 root.NemoNativeObjectProject={desktop:()=>f.session};
 // Controlled immutable native Perspective projection; never read legacy Perspective fields.
 let perspectiveMode='disabled',perspectiveOwner=f.session;const perspectiveListeners=new Set();
 const perspectiveKeys=['instanceId','documentId','lifecycleGeneration','contentRevision','documentSnapshotId'],perspectiveProofs=new WeakMap();
 root.NemoNativePerspectiveController=Object.freeze({read(){
  const owner=perspectiveOwner,info=owner.info(),shown=owner.readPresentedObjectSession();
  if(perspectiveMode==='unavailable'||perspectiveMode==='publication-wait'||!owner.isActive()||!shown||perspectiveKeys.some(k=>shown[k]!==info[k]))return null;
  const pin=Object.fromEntries(perspectiveKeys.map(k=>[k,info[k]]));
  if(perspectiveMode==='stale')pin.documentSnapshotId='stale-perspective-snapshot';
  const value=Object.freeze({info:Object.freeze(pin),perspective:Object.freeze({settings:Object.freeze({perspectiveEnabled:perspectiveMode==='enabled'})})});
  perspectiveProofs.set(value,typeof owner.readSubscriptionGeneration==='function'?owner.readSubscriptionGeneration():0);return value;
 },readForCommand(value){
  const owner=perspectiveOwner,info=owner.info(),ticket=typeof owner.readSubscriptionGeneration==='function'?owner.readSubscriptionGeneration():0;
  return (perspectiveMode==='disabled'||perspectiveMode==='publication-wait')&&owner.isActive()&&value&&perspectiveProofs.has(value)&&perspectiveProofs.get(value)===ticket&&perspectiveKeys.every(k=>value.info[k]===info[k])?value:null;
 },onRead(listener){perspectiveListeners.add(listener);return ()=>perspectiveListeners.delete(listener);}});

 const logs={legacy:0,zoom:0,selected:0};let intent;zoom.addEventListener('pointerdown',()=>logs.zoom++);canvas.addEventListener('pointerdown',()=>logs.legacy++);
 doc.getElementById('text-apply').addEventListener('click',()=>logs.legacy++);
 root.NemoNativePresentationWait=require('../src/js/adapters/native-presentation-wait.js');root.setTimeout=fn=>setImmediate(fn);
 root.NemoNativeTextToolDesktop=desktopModule;root.NemoNativeOpacityCutover={blocksLegacy:()=>true};root.NemoSelectCanvasIntent={create(){if(!selectionIntent)throw new Error('Text reached generic native selection route');intent=require('../src/js/adapters/select-canvas-intent.js').create({context:()=> 'stable',probe:()=>Object.freeze({mode:'clear'}),select:()=>logs.selected++});return intent;}};
 const binding=desktopModule.mount(root,container,{currentSession:()=>f.session,fontResources:()=>({list:async()=>[{assetId:'document font / 雪',name:'Imported',available:true}]})});await turn();
 const script=fs.readFileSync(path.join(__dirname,'../src/js/select-bridge.js'),'utf8');
 vm.runInNewContext(script,{window:root,document:doc,state:root.state,console,selectedPaths:[],userLayers:[],project:{layers:[]},view:{zoom:1},setTimeout,clearTimeout},{filename:'actual-select-bridge.js'});
 const get=id=>doc.getElementById(id);
 return {...f,pendingResources:()=>running,actionLease:()=>lastLease,perspectiveRead:()=>perspectiveListeners.forEach(fn=>fn(root.NemoNativePerspectiveController.read())),perspectiveListenerCount:()=>perspectiveListeners.size,perspectiveMode:value=>perspectiveMode=value,perspectiveOwner:owner=>perspectiveOwner=owner,root,doc,get,binding,canvas,zoom,logs,turn,intent:()=>intent,resize:r=>{rect=r;},input:(id,value)=>{get(id).value=value;get(id).fire('input');},apply:()=>get('text-apply').fire('click'),
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
 for(const props of [{isPrimary:false},{ctrlKey:true},{metaKey:true},{altKey:true},{shiftKey:true},{button:2,buttons:2},{buttons:2},{buttons:3}]){
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
 f.perspectiveOwner(next.session);f.root.NemoNativeObjectProject={desktop:()=>next.session};
 const binding=Desktop.mount(f.root,f.binding.container,{currentSession:()=>next.session,fontResources:()=>({list:async()=>[]})});await f.turn();await binding.core.openNew({anchorTopLeft:{x:20,y:20},fixedWidth:null});
 assert.equal(f.get('text-apply').disabled,false,'replacement draft is ready before old queue settles');assert.ok(binding.core.snapshot().draft);
 gate.resolve();await f.turn();
 console.log(JSON.stringify({newDraft:!!binding.core.snapshot().draft,newBusy:binding.core.snapshot().busy,disabled:f.get('text-apply').disabled,oldCommands:f.owner.log.executed.length,newCommands:next.owner.log.executed.length}));
 assert.equal(f.get('text-apply').disabled,false,'old callbacks must not disable the new owner editor');
});


test('browser Text bootstrap injects actual existing reader and writer modules into the pure core',async()=>{
 // VM objects cross into the Node-realm SDK through the fixture's explicit JSON boundary.
 const browser={NemoNativeViewportUnderlay:require('../src/js/adapters/native-viewport-underlay.js'),NemoNativeTextRead:require('../src/js/adapters/native-text-read.js'),NemoNativeObjectTextLayout:require('../src/js/adapters/native-object-text-layout.js'),NemoNativeObjectItemContract:require('../src/js/adapters/native-object-item-contract.js')};
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


test('temporary pointer diagnostics eagerly retain the actual rejected scalars without opening or mutating',async()=>{
 const f=await fixture(),rows=[];f.root.console={info:line=>rows.push(line)};
 const e=f.canvas.fire('pointerdown',{buttons:2,isTrusted:true,pointerType:'mouse'});e.buttons=1;
 const rejected=rows.map(line=>JSON.parse(line.slice('[nemo-native-text-input] '.length))).find(row=>row.decision==='reject.buttons.down');
 assert.ok(rejected);assert.equal(rejected.buttons,2);assert.equal(rejected.isPrimary,true);assert.equal(rejected.button,0);assert.equal('isTrusted' in rejected,false);assert.equal('targetId' in rejected,false);
 assert.equal(f.binding.core.snapshot().draft,null);assert.equal(f.owner.log.executed.length,0);assert.equal(f.logs.legacy,0);
 assert.ok(rows.every(line=>typeof line==='string'&&!line.includes('Native point')));
});
test('unavailable or throwing diagnostic output cannot alter ordinary Text draft and commit behavior',async()=>{
 const f=await fixture();f.root.console={info(){throw new Error('diagnostic sink unavailable');}};
 await f.click(100,110);assert.equal(f.get('text-popover').style.display,'block');f.input('text-input','Actual point');f.apply();await f.turn();
 assert.equal(f.owner.log.executed.length,1);assert.equal(f.logs.legacy,0);assert.equal(f.owner.items.size,1);
});

test('diagnostics preserve original short-circuit reads for rejected stage and nonprimary input',async()=>{
 const f=await fixture(),rows=[];f.root.console={info:line=>rows.push(line)};let untouched=0;
 const e={target:f.canvas,pointerId:19,isPrimary:false,stopImmediatePropagation(){},preventDefault(){}};
 for(const key of ['buttons','button','clientX','clientY','altKey','isTrusted','pointerType','timeStamp'])Object.defineProperty(e,key,{get(){untouched++;throw new Error('original guard must not read '+key);}});
 assert.equal(Desktop.handle(f.root,'down',e),true);assert.equal(untouched,0);assert.equal(f.binding.core.snapshot().draft,null);
 const other={get id(){untouched++;throw new Error('foreign target id');}};
 assert.equal(Desktop.handle(f.root,'down',{target:other,stopImmediatePropagation(){},preventDefault(){}}),true);assert.equal(untouched,0);
 assert.ok(rows.some(line=>line.includes('reject.primary')));assert.ok(rows.some(line=>line.includes('reject.stage')));assert.equal(f.owner.log.executed.length,0);
});

test('actual bridge pairs zero-mask primary click into point-only draft and one explicit native Apply',async()=>{
 const f=await fixture(),rows=[];f.root.console={info:line=>rows.push(JSON.parse(line.slice('[nemo-native-text-input] '.length)))};
 const before=f.reads.length,e=f.canvas.fire('pointerdown',{button:0,buttons:0,clientX:100,clientY:110});await f.turn();
 assert.equal(e.buttons,0,'the original mask is never rewritten to fabricate a held press');
 assert.equal(f.binding.core.snapshot().draft,null);assert.equal(f.get('text-popover').style.display,'none');assert.equal(f.reads.length,before);
 assert.equal(f.owner.items.size,0);assert.equal(f.owner.log.executed.length,0);
 f.canvas.fire('pointerup',{button:0,buttons:0,clientX:100,clientY:110});await f.turn();
 assert.equal(f.get('text-popover').style.display,'block');assert.equal(f.get('native-text-area-draft'),null);
 assert.deepEqual(f.binding.core.snapshot().draft.block.anchorTopLeft,{x:40,y:40});assert.equal(f.binding.core.snapshot().draft.block.fixedWidth,null);
 assert.equal(f.owner.items.size,0,'paired click stages only; native content still requires Apply');
 const down=rows.find(r=>r.decision==='down.clickOnly');assert.ok(down);assert.equal(down.buttons,0);assert.equal(rows.some(r=>r.decision==='down.accepted'),false);
 f.input('text-input','Paired native point');f.apply();await f.turn();
 assert.equal(f.owner.log.executed.length,1);assert.equal(f.owner.items.size,1);assert.equal(f.owner.log.received[0].payload.block.fixedWidth,null);
 assert.deepEqual(f.owner.log.received[0].payload.block.anchorTopLeft,{x:40,y:40});assert.equal(f.logs.legacy,0);
});
test('zero-mask paired point requires a real matching up; down alone or orphan up cannot open',async()=>{
 const f=await fixture(),before=f.reads.length;f.canvas.fire('pointerdown',{buttons:0});await f.turn();
 assert.equal(f.binding.core.snapshot().draft,null);assert.equal(f.binding.core.snapshot().discovering,false);assert.equal(f.reads.length,before);
 f.root.fire('blur');f.canvas.fire('pointerup',{buttons:0});await f.turn();assert.equal(f.binding.core.snapshot().draft,null);
 const g=await fixture();g.canvas.fire('pointerup',{buttons:0});await g.turn();assert.equal(g.binding.core.snapshot().draft,null);assert.equal(g.get('text-popover').style.display,'none');
 assert.equal(f.owner.log.executed.length+g.owner.log.executed.length,0);assert.equal(f.logs.legacy+g.logs.legacy,0);
});
test('zero-mask down never becomes a held area on any move mask',async()=>{
 for(const buttons of [0,1,2,3]){
  const f=await fixture();f.canvas.fire('pointerdown',{buttons:0,clientX:100,clientY:110});
  f.canvas.fire('pointermove',{buttons,clientX:300,clientY:190});f.canvas.fire('pointerup',{buttons:0,clientX:300,clientY:190});await f.turn();
  assert.equal(f.binding.core.snapshot().draft,null,'move mask '+buttons);assert.equal(f.get('native-text-area-draft'),null);assert.equal(f.get('text-popover').style.display,'none');
  assert.equal(f.owner.log.executed.length,0);assert.equal(f.logs.legacy,0);
 }
});
test('zero-mask release has a 20px point slop and never invents area width',async()=>{
 for(const [dx,dy,allowed]of [[20,0,true],[0,20,true],[21,0,false],[0,21,false],[20,20,false],[-21,0,false],[0,-21,false]]){
  const f=await fixture();f.canvas.fire('pointerdown',{buttons:0,clientX:100,clientY:110});f.canvas.fire('pointerup',{buttons:0,clientX:100+dx,clientY:110+dy});await f.turn();
  const draft=f.binding.core.snapshot().draft;assert.equal(!!draft,allowed,JSON.stringify({dx,dy}));
  if(draft){assert.equal(draft.block.fixedWidth,null);assert.deepEqual(draft.block.anchorTopLeft,{x:40,y:40});}
  assert.equal(f.get('native-text-area-draft'),null);assert.equal(f.owner.log.executed.length,0);assert.equal(f.logs.legacy,0);
 }
});
test('zero-mask pair preserves release button, mask, pointer, primary, modifiers and stage refusals',async()=>{
 for(const props of [{buttons:1},{buttons:2},{buttons:3},{button:2,buttons:0},{pointerId:2},{isPrimary:false},{altKey:true},{ctrlKey:true},{metaKey:true},{shiftKey:true}]){
  const f=await fixture();f.canvas.fire('pointerdown',{buttons:0,pointerId:1});f.canvas.fire('pointerup',{buttons:0,...props});await f.turn();
  assert.equal(f.binding.core.snapshot().draft,null,JSON.stringify(props));assert.equal(f.owner.log.executed.length,0);assert.equal(f.logs.legacy,0);
 }
 const f=await fixture();f.canvas.fire('pointerdown',{buttons:0});f.zoom.fire('pointerup',{buttons:0});await f.turn();
 assert.equal(f.binding.core.snapshot().draft,null,'stage chrome up cannot complete a zero-mask point intent');assert.equal(f.owner.log.executed.length,0);
});
test('zero-mask pair cancels on Escape, capture loss, pointercancel, blur and tool roundtrip',async()=>{
 for(const cancel of ['Escape','lostpointercapture','pointercancel','blur','tool']){
  const f=await fixture(true);f.canvas.fire('pointerdown',{buttons:0});
  if(cancel==='Escape')f.doc.fire('keydown',{key:'Escape'});
  else if(cancel==='blur')f.root.fire('blur');
  else if(cancel==='tool'){f.root.state.tool='select';f.canvas.fire('pointermove',{buttons:0});f.root.state.tool='text';}
  else f.canvas.fire(cancel,{pointerId:1});
  f.canvas.fire('pointerup',{buttons:0});await f.turn();assert.equal(f.binding.core.snapshot().draft,null,cancel);assert.equal(f.get('text-popover').style.display,'none');
  assert.equal(f.owner.log.executed.length,0);assert.equal(f.logs.legacy,0);
 }
});
test('zero-mask pair cannot open against changed native pin, frame, selection, view or presentation',async()=>{
 for(const change of ['owner','revision','frame','selection','view','unpresented','symbol','inactive']){
  const f=await fixture();f.canvas.fire('pointerdown',{buttons:0});
  if(change==='owner'){f.state.document='replaced-document';f.state.generation++;}
  if(change==='revision')f.state.revision++;
  if(change==='frame')f.root.state.currentFrame=1;
  if(change==='selection')f.root._layerSel=[];
  if(change==='view')f.resize({left:20,top:30,width:600,height:360});
  if(change==='unpresented')f.session.readPresentedObjectSession=()=>null;
  if(change==='symbol')f.root.state.activeSymbolId='other-context';
  if(change==='inactive')f.state.active=false;
  f.canvas.fire('pointerup',{buttons:0});await f.turn();assert.equal(f.binding.core.snapshot().draft,null,change);assert.equal(f.get('text-popover').style.display,'none');
  assert.equal(f.owner.log.executed.length,0);assert.equal(f.logs.legacy,0);
 }
});
test('zero-mask paired discovery cancellation cannot publish a late editor',async()=>{
 const f=await fixture(),gate=deferred();f.hold(gate.promise);f.canvas.fire('pointerdown',{buttons:0});f.canvas.fire('pointerup',{buttons:0});await Promise.resolve();
 assert.equal(f.binding.core.snapshot().discovering,true);f.root.fire('blur');gate.resolve();await f.turn();
 assert.equal(f.binding.core.snapshot().draft,null);assert.equal(f.get('text-popover').style.display,'none');assert.equal(f.owner.log.executed.length,0);
});
test('invalid down mask2 keeps original guard getter order/count and eager immutable sampled value',async()=>{
 const f=await fixture(),reads=[],rows=[],values={pointerId:1,isPrimary:true,altKey:false,ctrlKey:false,metaKey:false,shiftKey:false,button:0,clientX:100,clientY:110,buttons:2};
 f.root.console={info:line=>rows.push(JSON.parse(line.slice('[nemo-native-text-input] '.length)))};
 const e={target:f.canvas,stopImmediatePropagation(){},preventDefault(){}};
 for(const key of Object.keys(values))Object.defineProperty(e,key,{get(){reads.push(key);return values[key];}});
 for(const key of ['isTrusted','pointerType','timeStamp'])Object.defineProperty(e,key,{get(){assert.fail('unread input '+key);}});
 assert.equal(Desktop.handle(f.root,'down',e),true);values.buttons=0;
 assert.deepEqual(reads,['pointerId','isPrimary','altKey','ctrlKey','metaKey','shiftKey','button','clientX','clientY','buttons']);
 const rejected=rows.find(r=>r.decision==='reject.buttons.down');assert.ok(rejected);assert.equal(rejected.buttons,2);assert.equal('isTrusted' in rejected,false);
 assert.equal(f.binding.core.snapshot().draft,null);assert.equal(f.owner.log.executed.length,0);
});

test('app-owned hex field creates #123456 through ordinary Apply while retaining the native picker',async()=>{
 const f=await fixture();f.root.state.strokeColor='#000000';await f.click();
 const hex=f.get('native-text-color-hex'),picker=f.get('text-color');assert.ok(hex);assert.ok(picker);assert.equal(hex.getAttribute('aria-label'),'Text color hex');assert.equal(hex.value,'#000000');assert.equal(hex.disabled,false);
 f.input('text-input','Hex point');f.input('native-text-color-hex','#123456');assert.equal(picker.value,'#123456');assert.equal(hex.getAttribute('aria-invalid'),'false');assert.equal(f.owner.log.executed.length,0);
 f.apply();await f.turn();assert.equal(f.owner.log.executed.length,1);assert.equal(f.owner.log.received[0].payload.block.color,'#123456');assert.equal([...f.owner.items.values()][0].color,'#123456');assert.equal(f.logs.legacy,0);
});
test('hex edit publishes #224466 alongside mixed edits and preserves the existing alpha and other properties',async()=>{
 const f=await fixture(),t=target('hex-alpha-text'),original={...block('Original'),color:'#11223380'};f.seed(t,original);await f.binding.core.openExisting(t);
 assert.equal(f.get('native-text-color-hex').value,'#112233');assert.equal(f.get('text-color').value,'#112233');
 f.input('native-text-color-hex','#224466');f.input('text-input','Mixed edit');f.input('text-size','56');f.apply();await f.turn();
 assert.equal(f.owner.log.executed.length,1);assert.deepEqual(f.owner.log.received[0].payload.patch,{color:'#22446680',text:'Mixed edit',size:56});
 const saved=f.owner.items.get(t.strokeId);assert.equal(saved.color,'#22446680');assert.equal(saved.bold,true);assert.equal(saved.letterSpacing,2);assert.deepEqual(saved.font,original.font);assert.deepEqual(saved.anchorTopLeft,original.anchorTopLeft);assert.equal(f.logs.legacy,0);
 await f.binding.core.openExisting(t);assert.equal(f.get('native-text-color-hex').value,'#224466');assert.equal(f.get('text-color').value,'#224466');
 f.input('text-input','Text only');f.apply();await f.turn();assert.deepEqual(f.owner.log.received[1].payload.patch,{text:'Text only'});assert.equal(f.owner.items.get(t.strokeId).color,'#22446680');
});
test('invalid hex blocks Apply and keyboard submission without committing the last valid picker color',async()=>{
 const f=await fixture();await f.click();f.input('text-input','Keep draft');f.input('native-text-color-hex','#123456');
 for(const invalid of ['', '#12', '#gggggg', '224466', '#22446680', ' #224466']){
  f.input('native-text-color-hex',invalid);assert.equal(f.get('text-color').value,'#123456');assert.equal(f.get('native-text-color-hex').getAttribute('aria-invalid'),'true');assert.equal(f.get('text-apply').disabled,true);
  f.apply();f.get('native-text-color-hex').fire('keydown',{key:'Enter',ctrlKey:true});await f.turn();
  assert.equal(f.owner.log.received.length,0);assert.equal(f.owner.log.executed.length,0);assert.equal(f.owner.items.size,0);assert.ok(f.binding.core.snapshot().draft);assert.equal(f.get('text-input').value,'Keep draft');
 }
 f.input('native-text-color-hex','#224466');assert.equal(f.get('text-apply').disabled,false);f.apply();await f.turn();assert.equal(f.owner.log.executed.length,1);assert.equal(f.owner.log.received[0].payload.block.color,'#224466');assert.equal(f.owner.log.received[0].payload.block.text,'Keep draft');assert.equal(f.logs.legacy,0);
});
test('both picker events synchronize hex, repair invalid input and preserve alpha on publication',async()=>{
 for(const event of ['input','change']){
  const f=await fixture(),t=target('picker-'+event);f.seed(t,{...block(),color:'#11223340'});await f.binding.core.openExisting(t);
  f.input('native-text-color-hex','#broken');const picker=f.get('text-color');picker.value='#abcdef';picker.fire(event);
  assert.equal(f.get('native-text-color-hex').value,'#abcdef');assert.equal(f.get('native-text-color-hex').getAttribute('aria-invalid'),'false');assert.equal(f.get('text-apply').disabled,false);
  f.apply();await f.turn();assert.equal(f.owner.log.executed.length,1);assert.deepEqual(f.owner.log.received[0].payload.patch,{color:'#abcdef40'});assert.equal(f.logs.legacy,0);
 }
});
test('cancel and reopen reset invalid hex without duplicating controls or publishing a stale color',async()=>{
 const f=await fixture();await f.click();const hex=f.get('native-text-color-hex');f.input('text-input','Cancelled');f.input('native-text-color-hex','#bad');f.get('text-cancel').fire('click');
 assert.equal(f.owner.log.executed.length,0);assert.equal(f.get('text-popover').style.display,'none');assert.equal(hex.disabled,true);assert.equal(hex.getAttribute('aria-invalid'),'false');
 f.root.state.strokeColor='#224466';await f.click();assert.equal(f.get('native-text-color-hex'),hex);assert.equal(hex.value,'#224466');assert.equal(hex.disabled,false);assert.equal(f.get('text-apply').disabled,false);
 const same=Desktop.mount(f.root,f.binding.container,f.binding.ports);assert.equal(same,f.binding);
 const count=n=>(n.id==='native-text-color-hex'?1:0)+n.children.reduce((sum,c)=>sum+count(c),0);assert.equal(count(f.doc.body),1);
 f.input('text-input','Reopened');f.apply();await f.turn();assert.equal(f.owner.log.executed.length,1);assert.equal(f.owner.log.received[0].payload.block.color,'#224466');assert.equal(f.logs.legacy,0);
});
test('busy hex edits cannot alter an in-flight publication or dispatch a second command',async()=>{
 const f=await fixture();await f.click();f.input('text-input','Queued hex');f.input('native-text-color-hex','#123456');const gate=deferred();f.hold(gate.promise);f.apply();await Promise.resolve();
 assert.equal(f.binding.core.snapshot().busy,true);assert.equal(f.get('native-text-color-hex').disabled,true);assert.equal(f.get('text-apply').disabled,true);
 f.input('native-text-color-hex','#224466');f.apply();f.get('native-text-color-hex').fire('keydown',{key:'Enter',metaKey:true});gate.resolve();await f.turn();
 assert.equal(f.owner.log.executed.length,1);assert.equal(f.owner.log.received[0].payload.block.color,'#123456');assert.equal(f.get('text-popover').style.display,'none');assert.equal(f.logs.legacy,0);
});
test('retire removes hex input and its listeners while a replacement editor retains independent color state',async()=>{
 const f=await fixture();await f.click();const oldHex=f.get('native-text-color-hex');f.input('native-text-color-hex','#123456');f.binding.retire();assert.equal(f.get('native-text-color-hex'),null);
 for(const event of ['input','change','keydown'])assert.equal((oldHex.listeners.get(event)||[]).length,0);
 const next=Desktop.mount(f.root,f.binding.container,f.binding.ports);await f.turn();await next.core.openNew({anchorTopLeft:{x:20,y:20},fixedWidth:null});
 const hex=f.get('native-text-color-hex');assert.ok(hex);assert.notEqual(hex,oldHex);f.input('native-text-color-hex','#224466');oldHex.value='#bad';oldHex.fire('input');oldHex.fire('keydown',{key:'Enter',ctrlKey:true});
 assert.equal(hex.value,'#224466');assert.equal(hex.disabled,false);assert.equal(f.get('text-color').value,'#224466');assert.equal(f.get('text-apply').disabled,false);assert.equal(f.owner.log.executed.length,0);
 f.input('text-input','New owner');f.apply();await f.turn();assert.equal(f.owner.log.executed.length,1);assert.equal(f.owner.log.received[0].payload.block.color,'#224466');assert.equal(f.logs.legacy,0);
});
test('area draft refuses invalid hex on blur then publishes #336699 by the existing keyboard route',async()=>{
 const f=await fixture();f.canvas.fire('pointerdown',{clientX:100,clientY:110});f.canvas.fire('pointermove',{clientX:300,clientY:190});f.canvas.fire('pointerup',{clientX:300,clientY:190});await f.turn();
 const area=f.get('native-text-area-draft');assert.ok(area);area.value='Hex area';area.fire('input');f.input('native-text-color-hex','#invalid');area.fire('blur');await f.turn();assert.equal(f.owner.log.executed.length,0);assert.ok(f.binding.core.snapshot().draft);
 f.input('native-text-color-hex','#336699');area.fire('keydown',{key:'Enter',ctrlKey:true});await f.turn();assert.equal(f.owner.log.executed.length,1);assert.equal(f.owner.log.received[0].payload.block.color,'#336699');assert.equal(f.owner.log.received[0].payload.block.fixedWidth,100);assert.equal(f.owner.log.received[0].payload.block.text,'Hex area');assert.equal(f.logs.legacy,0);
});

test('area draft blur after a pointerdown inside the popover chrome does not commit; blur after a stage pointerdown still commits',async()=>{
 for(const inside of [true,false]){
  const f=await fixture();f.canvas.fire('pointerdown',{clientX:100,clientY:110});f.canvas.fire('pointermove',{clientX:300,clientY:190});f.canvas.fire('pointerup',{clientX:300,clientY:190});await f.turn();
  const area=f.get('native-text-area-draft');assert.ok(area);area.value='Chrome text';area.fire('input');
  (inside?f.get('text-popover'):f.canvas).fire('pointerdown',{clientX:5,clientY:5});area.fire('blur',{relatedTarget:null});await f.turn();
  if(inside){assert.equal(f.owner.log.executed.length,0,'popover padding click must not commit');assert.ok(f.binding.core.snapshot().draft);
   f.canvas.fire('pointerdown',{clientX:5,clientY:5});area.fire('blur',{relatedTarget:null});await f.turn();assert.equal(f.owner.log.executed.length,1,'a later outside blur commits');}
  else assert.equal(f.owner.log.executed.length,1,'stage blur commits');
 }
});

test('native hex layout preserves existing controls and restores prior inline wrapping across remounts',async()=>{
 const f=await fixture(),parent=f.get('text-color').parentNode,controls=parent.children.filter(n=>n!==f.get('native-text-color-hex').parentNode);
 f.binding.retire();assert.deepEqual(parent.children,controls);parent.style.flexWrap='wrap-reverse';
 const binding=Desktop.mount(f.root,f.binding.container,f.binding.ports);await f.turn();
 const label=f.get('native-text-color-hex').parentNode;assert.equal(label.parentNode,parent);assert.equal(parent.style.flexWrap,'wrap');assert.equal(label.style.flexBasis,'100%','hex takes a separate full row instead of squeezing original size/font/picker');
 assert.deepEqual(parent.children.filter(n=>n!==label),controls,'the original controls retain their nodes and order');
 await binding.core.openNew({anchorTopLeft:{x:20,y:20},fixedWidth:null});assert.equal(f.get('native-text-color-hex').disabled,false);
 binding.retire();assert.equal(parent.style.flexWrap,'wrap-reverse','native mount restores pre-existing inline layout');assert.deepEqual(parent.children,controls);assert.equal(f.get('native-text-color-hex'),null);assert.equal(f.owner.log.executed.length,0);
});


test('element blur keeps an invalid area draft while real window blur cancels it without publication',async()=>{
 const f=await fixture();let windowBlur=0,capturedBlur=0;f.root.addEventListener('blur',()=>windowBlur++);f.root.addEventListener('blur',()=>capturedBlur++,true);
 f.canvas.fire('pointerdown',{clientX:100,clientY:110});f.canvas.fire('pointermove',{clientX:300,clientY:190});f.canvas.fire('pointerup',{clientX:300,clientY:190});await f.turn();
 const area=f.get('native-text-area-draft');area.value='Pending color';area.fire('input');f.input('native-text-color-hex','#invalid');
 const event=area.fire('blur');await f.turn();assert.equal(event.bubbles,false);assert.equal(capturedBlur,1);assert.equal(windowBlur,0);assert.ok(f.binding.core.snapshot().draft);assert.equal(f.owner.log.executed.length,0);
 f.root.fire('blur');await f.turn();assert.equal(windowBlur,1);assert.equal(f.binding.core.snapshot().draft,null);assert.equal(f.owner.log.executed.length,0);assert.equal(f.logs.legacy,0);
});
test('actual Text inspector mounts late animator capability once and retires its native child',async()=>{
 const f=await fixture();let mounts=0,refreshes=0,retires=0;
 f.root.NemoNativeTextAnimatorDesktop={mount(root,container){assert.equal(root,f.root);assert.equal(container,f.get('native-inspector'));mounts++;return {refresh(){refreshes++;},retire(){retires++;}};}};
 await f.binding.refresh();await f.binding.refresh();assert.equal(mounts,1);assert.ok(refreshes>=1);
 f.binding.retire();assert.equal(retires,1);
});

test('actual native Text admission refuses unavailable, stale and enabled Perspective projections',async()=>{
 for(const mode of ['unavailable','stale','enabled']){
  const f=await fixture();f.perspectiveMode(mode);
  Object.defineProperty(f.root.state,'perspectiveEnabled',{get(){throw new Error('legacy Perspective read');},set(){throw new Error('legacy Perspective write');}});
  await f.click();assert.equal(f.binding.core.snapshot().draft,null,mode);assert.equal(f.owner.log.executed.length,0,mode);assert.equal(f.logs.legacy,0,mode);
 }
});

test('fresh Perspective read coalesces existing Text refresh and removes its listener on retirement',async()=>{
 const f=await fixture();let refreshes=0;const original=f.binding.refresh;
 f.binding.refresh=function(){refreshes++;return original.apply(this,arguments);};
 f.perspectiveRead();f.perspectiveRead();f.perspectiveRead();await f.turn();assert.equal(refreshes,1);
 assert.equal(f.perspectiveListenerCount(),1);f.binding.retire();assert.equal(f.perspectiveListenerCount(),0);
 f.perspectiveRead();await f.turn();assert.equal(refreshes,1);
});
test('Text command settles once while fresh Perspective read gates post-reconcile editor publication',async()=>{
 const f=await fixture();await f.click();f.input('text-input','Confirmed native text');f.apply();
 f.perspectiveMode('publication-wait');await f.turn();await f.turn();
 assert.equal(f.owner.log.executed.length,1);assert.equal(f.binding.core.snapshot().busy,false);
 assert(f.binding.core.snapshot().draft,'old editor is not promoted as current while native metadata is unavailable');
 f.perspectiveMode('disabled');f.perspectiveRead();await f.turn();await f.turn();
 assert.equal(f.binding.core.snapshot().draft,null);assert.equal(f.get('text-popover').style.display,'none');
 assert.equal(f.owner.log.executed.length,1,'publication must never replay the text command');
});

test('native camera receipt maps actual point Text and area draft while a view change refuses the old draft',async()=>{
 const cameraView={mode:'stored-camera',cameraAvailable:true,documentToComposition:[2,0,0,2,-160,-90],compositionToDocument:[.5,0,0,.5,80,45]};
 const f=await fixture(false,Desktop,{cameraView});await f.click(100,110);f.input('text-input','Camera text');f.apply();await f.turn();
 assert.deepEqual(f.owner.log.received[0].payload.block.anchorTopLeft,{x:100,y:65});assert.equal(f.logs.legacy,0);
 const area=await fixture(false,Desktop,{cameraView});area.canvas.fire('pointerdown',{clientX:100,clientY:110});area.canvas.fire('pointermove',{clientX:300,clientY:190});area.canvas.fire('pointerup',{clientX:300,clientY:190});await area.turn();
 const draft=area.get('native-text-area-draft');assert.ok(draft);assert.equal(draft.style.width,'50px');assert.equal(draft.style.transform,'matrix(4,0,0,4,0,0)');
 draft.value='Area camera';draft.fire('input');draft.fire('keydown',{key:'Enter',ctrlKey:true});await area.turn();
 assert.equal(area.owner.log.received[0].payload.block.fixedWidth,50);
 const stale=await fixture(false,Desktop,{cameraView});await stale.click();stale.input('text-input','stale');stale.root.state.cameraView=false;stale.apply();await stale.turn();assert.equal(stale.owner.log.executed.length,0);
});

test('Text Apply uses the real action-local proof while the public getter stays pending-fenced',async()=>{
 const f=await fixture();await f.click();f.input('text-input','Leased Text');
 const gate=deferred();f.hold(gate.promise);f.apply();await Promise.resolve();
 assert.ok(f.pendingResources()>0);assert.equal(f.session.readPresentedObjectSession(),null);assert.equal(f.owner.log.executed.length,0);
 gate.resolve();await f.turn();assert.equal(f.owner.log.executed.length,1);assert.equal(f.logs.legacy,0);
 assert.equal(f.owner.log.received[0].payload.block.text,'Leased Text');assert.equal(f.actionLease().readPresentedObjectSession(),null,'action-local proof expires after the action');
});
test('queued Text refuses missing, foreign, refused and altered-view action-local readers',async()=>{
 for(const actionReader of ['missing','foreign','refused','view']){
  const f=await fixture(false,Desktop,{actionReader});await f.click();f.input('text-input','Must refuse');f.apply();await f.turn();
  assert.equal(f.owner.log.executed.length,0,actionReader);assert.equal(f.owner.log.received.length,0,actionReader);assert.equal(f.logs.legacy,0);
 }
});
test('navigated Canvas area Text draft uses the actual full affine and commits the same inverse placement',async()=>{
 const view={mode:'canvas',navigation:{zoom:2,pan:[0,0]},cameraAvailable:true,documentToComposition:[2,0,0,2,-160,-90],compositionToDocument:[.5,0,0,.5,80,45]};
 const f=await fixture(false,Desktop,{cameraView:view});
 f.canvas.fire('pointerdown',{clientX:100,clientY:110});f.canvas.fire('pointermove',{clientX:300,clientY:190});f.canvas.fire('pointerup',{clientX:300,clientY:190});await f.turn();
 const area=f.get('native-text-area-draft');assert.ok(area);assert.equal(area.style.transform,'matrix(4,0,0,4,0,0)');
 assert.equal(area.style.width,'50px');assert.equal(area.style.left,'100px');assert.equal(area.style.top,'110px');
 area.value='Canvas affine';area.fire('input');area.fire('keydown',{key:'Enter',ctrlKey:true});await f.turn();
 const q=f.owner.log.received[0];assert.equal(f.owner.log.executed.length,1);assert.deepEqual(q.payload.block.anchorTopLeft,{x:100,y:65});assert.equal(q.payload.block.fixedWidth,50);assert.equal(f.root.state.cameraView,false);assert.equal(f.logs.legacy,0);
});

test('held Text drag shows one live dashed guide with baseline look and exact client geometry',async()=>{
 const f=await fixture();assert.equal(f.get('native-text-drag-guide'),null);
 f.canvas.fire('pointerdown',{buttons:1,clientX:100,clientY:110});assert.equal(f.get('native-text-drag-guide'),null,'down alone creates none');
 f.canvas.fire('pointermove',{buttons:1,clientX:300,clientY:190});
 const g=f.get('native-text-drag-guide');assert.ok(g);
 assert.deepEqual([g.style.left,g.style.top,g.style.width,g.style.height],['100px','110px','200px','80px']);
 assert.equal(g.style.position,'fixed');assert.equal(g.style.pointerEvents,'none');assert.equal(g.style.zIndex,'200');
 assert.equal(g.style.border,'1px dashed rgba(255,184,108,.9)');assert.equal(g.style.background,'rgba(255,184,108,.06)');
 f.canvas.fire('pointermove',{buttons:1,clientX:40,clientY:112});
 assert.equal(f.get('native-text-drag-guide'),g,'same single element is updated');assert.equal(f.doc.body.children.filter(n=>n.id==='native-text-drag-guide').length,1);
 assert.deepEqual([g.style.left,g.style.top,g.style.width,g.style.height],['40px','110px','60px','24px'],'min height 24, left follows min(x)');
 f.canvas.fire('pointerup',{buttons:0,clientX:40,clientY:112});await f.turn();assert.equal(f.get('native-text-drag-guide'),null);
});
test('Text drag guide needs more than 4px of horizontal travel',async()=>{
 const f=await fixture();f.canvas.fire('pointerdown',{buttons:1,clientX:100,clientY:110});
 for(const x of [100,102,104,96])f.canvas.fire('pointermove',{buttons:1,clientX:x,clientY:150}),assert.equal(f.get('native-text-drag-guide'),null,String(x));
 f.canvas.fire('pointermove',{buttons:1,clientX:105,clientY:150});assert.ok(f.get('native-text-drag-guide'));
 f.canvas.fire('pointermove',{buttons:1,clientX:103,clientY:150});assert.equal(f.get('native-text-drag-guide'),null,'shrinking back inside slop removes it');
});
test('Text drag guide is removed on up, Escape, blur, tool switch and retire',async()=>{
 for(const end of ['up','Escape','blur','tool','retire','cancelEvent']){
  const f=await fixture(true);f.canvas.fire('pointerdown',{buttons:1,clientX:100,clientY:110});f.canvas.fire('pointermove',{buttons:1,clientX:300,clientY:190});assert.ok(f.get('native-text-drag-guide'),end);
  if(end==='up')f.canvas.fire('pointerup',{buttons:0,clientX:300,clientY:190});
  else if(end==='Escape')f.doc.fire('keydown',{key:'Escape'});
  else if(end==='blur')f.root.fire('blur');
  else if(end==='tool'){f.root.state.tool='select';f.canvas.fire('pointermove',{buttons:1,clientX:310,clientY:190});}
  else if(end==='cancelEvent')f.canvas.fire('pointercancel',{pointerId:1});
  else f.binding.retire();
  await f.turn();assert.equal(f.get('native-text-drag-guide'),null,end);assert.equal(f.doc.body.children.some(n=>n.id==='native-text-drag-guide'),false,end);
 }
});
test('zero-mask clickOnly Text pair never creates a drag guide',async()=>{
 const f=await fixture();f.canvas.fire('pointerdown',{buttons:0,clientX:100,clientY:110});
 f.canvas.fire('pointermove',{buttons:0,clientX:300,clientY:190});f.canvas.fire('pointermove',{buttons:1,clientX:320,clientY:190});
 assert.equal(f.get('native-text-drag-guide'),null);f.canvas.fire('pointerup',{buttons:0,clientX:320,clientY:190});await f.turn();assert.equal(f.get('native-text-drag-guide'),null);
});

const px=v=>parseFloat(v);
const rectOf=n=>({l:px(n.style.left),t:px(n.style.top),r:px(n.style.left)+(n.style.width?px(n.style.width):240),b:px(n.style.top)+(n.offsetHeight||px(n.style.height))});
const overlaps=(a,b)=>a.l<b.r&&b.l<a.r&&a.t<b.b&&b.t<a.b;
async function dragOpen(f,x0,y0,x1,y1){
 f.get('text-popover').offsetHeight=200;
 f.canvas.fire('pointerdown',{buttons:1,clientX:x0,clientY:y0});f.canvas.fire('pointermove',{buttons:1,clientX:x1,clientY:y1});f.canvas.fire('pointerup',{buttons:0,clientX:x1,clientY:y1});await f.turn();
}
test('area Text draft is dock-positioned below the dragged box and styled like the guide',async()=>{
 const f=await fixture();await dragOpen(f,100,110,300,190);
 const area=f.get('native-text-area-draft'),pop=f.get('text-popover');assert.ok(area);
 assert.equal(area.style.width,'200px','fixedWidth*scale');assert.equal(area.style.height,'80px','dragged client height');
 assert.equal(area.style.border,'1px dashed rgba(255,184,108,.9)');assert.equal(area.style.background,'rgba(255,184,108,.06)');assert.equal(area.style.resize,'none');
 assert.equal(pop.style.left,'100px');assert.equal(pop.style.top,'198px','at.y + height + 8');
 assert.equal(overlaps({l:100,t:110,r:300,b:190},rectOf(pop)),false);
});
test('area Text draft popover flips above, or beside, when there is no room below',async()=>{
 const f=await fixture();f.doc.documentElement.clientHeight=500;await dragOpen(f,100,330,300,380);
 const pop=f.get('text-popover'),area=f.get('native-text-area-draft');
 assert.equal(pop.style.top,'122px','flipped to at.y - popH - 8');assert.equal(overlaps(rectOf(area),rectOf(pop)),false);
 const g=await fixture();g.doc.documentElement.clientHeight=400;await dragOpen(g,100,110,300,190);
 assert.equal(overlaps(rectOf(g.get('native-text-area-draft')),rectOf(g.get('text-popover'))),false,'side dock when neither below nor above fits');
});
test('area Text draft height has a 24px floor and click placement keeps the baseline +10,+10 popover',async()=>{
 const f=await fixture();await dragOpen(f,100,110,300,112);assert.equal(f.get('native-text-area-draft').style.height,'24px');
 const c=await fixture();c.get('text-popover').offsetHeight=200;await c.click(100,110);
 assert.equal(c.get('native-text-area-draft'),null);assert.equal(c.get('text-popover').style.left,'110px');assert.equal(c.get('text-popover').style.top,'120px');
});
test('stashed drag height is cleared by cancel and commit so a later API-opened area draft uses the default height',async()=>{
 for(const end of ['cancel','commit']){
  const f=await fixture();await dragOpen(f,100,110,300,190);assert.equal(f.get('native-text-area-draft').style.height,'80px',end);
  if(end==='cancel')f.get('text-cancel').fire('click');else{const a=f.get('native-text-area-draft');a.value='x';a.fire('input');a.fire('keydown',{key:'Enter',ctrlKey:true});}
  await f.turn();assert.equal(f.get('native-text-area-draft'),null,end);
  await f.binding.core.openNew({anchorTopLeft:{x:20,y:20},fixedWidth:100});
  assert.equal(f.get('native-text-area-draft').style.height,'120px',end+': default max(24,size*1.25*scale)');
 }
});

test('#2267 editing a block with stored rotation 30 turns the in-place overlay about the anchor screen point; rotation 0 and absent are unchanged',async()=>{
 const f=await fixture(),t=target('rot-30');f.seed(t,{...block('Turned'),rotation:30});await f.binding.core.openExisting(t);
 const area=f.get('native-text-area-draft');assert.ok(area,'turned existing block edits in an overlay, not only the popover');
 assert.equal(area.style.left,'40px','anchor (10,20) -> screen (40,70) at scale 2');assert.equal(area.style.top,'70px');
 assert.equal(area.style.transform,'rotate(30deg)');assert.equal(area.style.transformOrigin,'0 0');
 assert.equal(area.value,'Turned');area.value='Turned!';area.fire('input');area.fire('keydown',{key:'Enter',ctrlKey:true});await f.turn();
 const sent=f.owner.log.received.at(-1).payload;assert.equal(sent.patch.text,'Turned!');assert.equal('rotation' in sent.patch,false,'a patch never carries rotation');
 for(const stored of [{rotation:0},{}]){const g=await fixture(),u=target('rot-none');g.seed(u,{...block('Flat'),...stored});await g.binding.core.openExisting(u);
  assert.equal(g.get('native-text-area-draft'),null,'unturned existing text keeps the popover editor');}
});
test('#2267 turned overlay composes the turn with the presented view affine and docks the popover outside the turned frame',async()=>{
 const cameraView={mode:'stored-camera',cameraAvailable:true,documentToComposition:[2,0,0,2,-160,-90],compositionToDocument:[.5,0,0,.5,80,45]};
 const f=await fixture(false,Desktop,{cameraView}),t=target('rot-90');f.seed(t,{...block('Cam'),rotation:90,fixedWidth:40});await f.binding.core.openExisting(t);
 const area=f.get('native-text-area-draft');assert.ok(area);const v=area.style.transform.match(/-?[\d.e-]+/g).map(Number);
 const want=[0,4,-4,0,0,0];v.forEach((x,i)=>assert.ok(Math.abs(x-want[i])<1e-9,'matrix '+v+' vs '+want));
 assert.equal(area.style.width,'40px');assert.equal(area.style.transformOrigin,'0 0');
 const pop=f.get('text-popover'),rect=n=>({l:parseFloat(n.style.left),t:parseFloat(n.style.top)});
 assert.ok(rect(pop).t>=parseFloat(area.style.top)||rect(pop).l>=parseFloat(area.style.left),'popover is not parked on the anchor');
});

// #2372 presentation-busy-audit: the native queue hides the presentation while ANY command is pending (queue-pending).
async function hideQueue(f){let release;const hold=new Promise(r=>release=r),busy=f.session.runResourceCommand(()=>hold);await f.turn();
 assert.equal(f.session.readPresentedObjectSession(),null,'the queue hides the presentation');return async()=>{release();await busy;};}
const settleTurns=async(f,n=12)=>{for(let i=0;i<n;i++)await f.turn();};
test('a Text click during the queue-pending window waits for the presentation and opens the editor once, with no unavailable notice',async()=>{
 const f=await fixture(),notices=[];f.root.showToast=m=>notices.push(m);const free=await hideQueue(f);
 f.canvas.fire('pointerdown',{button:0,buttons:0,clientX:100,clientY:110});f.canvas.fire('pointerup',{button:0,buttons:0,clientX:100,clientY:110});
 await settleTurns(f,3);assert.equal(f.binding.core.snapshot().draft,null,'held while hidden');assert.equal(f.get('text-popover').style.display,'none');
 await free();await settleTurns(f);
 assert.equal(f.get('text-popover').style.display,'block');assert.deepEqual(f.binding.core.snapshot().draft.block.anchorTopLeft,{x:40,y:40});
 assert.deepEqual(notices,[]);assert.equal(f.owner.items.size,0,'opening writes nothing');
});
test('a Text drag whose release lands in the busy window still opens its area once',async()=>{
 const f=await fixture(),notices=[];f.root.showToast=m=>notices.push(m);
 f.canvas.fire('pointerdown',{button:0,buttons:1,clientX:100,clientY:110});f.canvas.fire('pointermove',{button:0,buttons:1,clientX:260,clientY:150});
 const free=await hideQueue(f);f.canvas.fire('pointerup',{button:0,buttons:0,clientX:260,clientY:150});await settleTurns(f,3);
 assert.equal(f.get('text-popover').style.display,'none','the release is held while hidden');
 await free();await settleTurns(f);assert.equal(f.get('text-popover').style.display,'block');assert.deepEqual(notices,[]);assert.ok(f.binding.core.snapshot().draft.block.fixedWidth>0);
});
test('a Text click is dropped silently when the tool left or the playhead moved during the wait',async()=>{
 for(const change of [r=>r.state.tool='draw',r=>r.state.currentFrame=1]){
  const f=await fixture(),notices=[];f.root.showToast=m=>notices.push(m);const free=await hideQueue(f);
  f.canvas.fire('pointerdown',{button:0,buttons:0,clientX:100,clientY:110});f.canvas.fire('pointerup',{button:0,buttons:0,clientX:100,clientY:110});change(f.root);
  await free();await settleTurns(f);assert.equal(f.get('text-popover').style.display,'none');assert.equal(f.binding.core.snapshot().draft,null);assert.deepEqual(notices,[]);
 }
});
test('a queue that never frees gives a Text click one clear notice and no editor',async()=>{
 const f=await fixture(),notices=[];f.root.showToast=m=>notices.push(m);const free=await hideQueue(f);
 f.canvas.fire('pointerdown',{button:0,buttons:0,clientX:100,clientY:110});f.canvas.fire('pointerup',{button:0,buttons:0,clientX:100,clientY:110});
 await settleTurns(f,40);assert.deepEqual(notices,['The canvas is still busy - try again.']);assert.equal(f.get('text-popover').style.display,'none');await free();
});
