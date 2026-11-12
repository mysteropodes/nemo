'use strict';
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),vm=require('node:vm');
const UI=require('../src/js/adapters/native-object-ui.js'),Files=require('../src/js/adapters/native-object-project-file.js');
const Project=require('../src/js/adapters/native-object-project.js'),Transport=require('../src/js/adapters/application-mcp.js');
const Workflow=require('../src/js/application/native-object-save-as-workflow.js'),Ack=require('../src/js/application/native-object-saved-acknowledgement.js');
const SDK=require('../src/js/adapters/native-application.js');
const path=require('node:path'),ROOT=path.resolve(__dirname,'..'),source=p=>fs.readFileSync(path.join(ROOT,p),'utf8');
const A={layerUid:'opaque A:雪',name:'<b>A</b>',visible:true},B={layerUid:'B',name:'B',visible:false},C={layerUid:'C',name:'',visible:true};
const clone=rows=>rows.map(x=>({...x}));
function metadata(layers=[A,B],revision=0,doc='document',generation=1){return {apiVersion:2,instanceId:'instance',documentId:doc,lifecycleGeneration:generation,
  contentRevision:revision,documentSnapshotId:doc+'-snapshot-'+revision,documentDigest:String(revision+1).repeat(64),byteCount:1000,
  canvasW:320,canvasH:180,fps:24,totalFrames:24,reference:null,undoDepth:0,redoDepth:0,layers:clone(layers)};}
function element(){
  const listeners={},classes=new Set();
  const el={children:[],style:{},dataset:{},attributes:{},value:'',max:0,textContent:'',className:'',type:'',offsetWidth:200,offsetHeight:120,
    classList:{add:x=>classes.add(x),remove:x=>classes.delete(x),contains:x=>classes.has(x),toggle(x,on){if(on===undefined)on=!classes.has(x);if(on)classes.add(x);else classes.delete(x);return on;}},
    setAttribute(k,v){this.attributes[k]=v;},addEventListener(k,f,capture=false){(listeners[k]||(listeners[k]=[])).push({f,capture});},removeEventListener(k,f,capture=false){listeners[k]=(listeners[k]||[]).filter(x=>x.f!==f||x.capture!==capture);},
    replaceChildren(...children){this.children.forEach(child=>{child.parentNode=null;});this.children=[];children.forEach(child=>this.appendChild(child));},
    appendChild(child){if(child.parentNode)child.parentNode.removeChild(child);this.children.push(child);child.parentNode=this;return child;},
    insertBefore(child,next){if(child.parentNode)child.parentNode.removeChild(child);const i=next?this.children.indexOf(next):this.children.length;assert.ok(i>=0);this.children.splice(i,0,child);child.parentNode=this;return child;},
    removeChild(child){const i=this.children.indexOf(child);assert.ok(i>=0);this.children.splice(i,1);child.parentNode=null;},
    dispatch(type,event={}){let stopped=false,result;const e={target:this,preventDefault(){},stopPropagation(){},stopImmediatePropagation(){stopped=true;},...event};for(const capture of[true,false])for(const x of(listeners[type]||[])){if(x.capture===capture&&!stopped)result=x.f.call(this,e);}return result;},click(){return this.dispatch('click');},listener:type=>function(event){return el.dispatch(type,event);},scrollIntoView(){},
    contains(child){while(child){if(child===this)return true;child=child.parentNode;}return false;},
    querySelector(selector){for(const child of this.children){if(selector[0]==='.'&&child.className.split(' ').includes(selector.slice(1)))return child;const nested=child.querySelector(selector);if(nested)return nested;}return null;},
    getBoundingClientRect:()=>({left:0,top:0,width:320,height:180,bottom:180})};
  Object.defineProperty(el,'nextSibling',{get(){if(!this.parentNode)return null;return this.parentNode.children[this.parentNode.children.indexOf(this)+1]||null;}});
  Object.defineProperty(el,'innerHTML',{set(){this.replaceChildren();}});
  return el;
}
function document(){const nodes=new Map();for(const id of ['layer-list','frame-hdr','frame-grid','tl-cf','tl-tf','info-frame','info-badge','info-strokes','info-sel','drawing-canvas','start-screen'])nodes.set(id,element());
  nodes.get('start-screen').classList.add('hid');const doc={nodes,body:element(),created:0,activeElement:null,readyState:'complete',
    getElementById(id){if(nodes.has(id))return nodes.get(id);function find(node){if(node.id===id)return node;for(const child of node.children){const found=find(child);if(found)return found;}return null;}for(const node of nodes.values()){const found=find(node);if(found)return found;}return null;},
    createElement(){doc.created++;return element();},createComment:()=>element(),addEventListener(){},removeEventListener(){}};return doc;}
const pick=(root,index)=>root.document.nodes.get('layer-list').children[index].children[0];
const eye=(root,index)=>root.document.nodes.get('layer-list').children[index].children[1];
const timeline=source('src/js/timeline.js');
function shipped(name){const start=timeline.indexOf('function '+name+'('),end=timeline.indexOf('\n}\n',start);assert.ok(start>=0&&end>start);return timeline.slice(start,end+2);}
function shippedEntrypoints(root){
  const start=timeline.indexOf('  setActiveLayer:function('),end=timeline.indexOf('\n  toggleLayerVis:',start);
  assert.ok(start>=0&&end>start);
  const legacy=()=>{throw Error('LEGACY FRAME READER');};
  const context=vm.createContext({window:root,state:root.state,document:root.document,console,
    syncDocFields:legacy,_tlScrollSnapshot:legacy,reconcileLayerIndexState:legacy,saveAllLayerFrames:legacy,
    activateUL:legacy,clearSel:legacy});
  vm.runInContext(['updateUI','renderTimeline','renderLayerList'].map(shipped).join('\n'),context);
  root.updateUI=context.updateUI;root.renderTimeline=context.renderTimeline;root.renderLayerList=context.renderLayerList;
  root.SM={setActiveLayer:vm.runInContext('('+timeline.slice(start,end).replace('  setActiveLayer:','').replace(/,\s*$/,'')+')',context)};
  return root;
}
function view(){const root={state:{},userLayers:[],document:document(),NemoNativeObjectProjectFile:Files,NemoNativeObjectUI:UI};Files.projectView(root,metadata());root.state.currentFrame=7;return root;}
const fixtureBindings=new WeakMap();
function fixturePorts(root){if(!fixtureBindings.has(root))fixtureBindings.set(root,{
  projectionReady:info=>Files.projectionReadyForUI(root,info),currentSession:()=>Project.desktop(root),
  selectLayer:(index,preserve,uid,expected)=>Project.selectLayerUI(root,index,preserve,uid,expected),
  repaint:frameOnly=>Project.paintUI(root,frameOnly)});return fixtureBindings.get(root);}
function presented(info,frame=7){return {...info,available:true,kind:'native-object',presentation:{...info,owner:'native',status:'presented',frame,workId:'work',viewGeneration:1}};}
function poisonFrames(root){root.state.layers.forEach(row=>Object.defineProperty(row,'frames',{configurable:true,get(){throw Error('LEGACY FRAME READER');}}));}
const base={UI,Files,Project,element,document,metadata,view,fixturePorts};const ui=source('src/js/ui.js');
function panel(){const root=base.view();const panel=base.element(),canvas=base.element();panel.id='props-panel';canvas.id='canvas-sec';panel.appendChild(canvas);root.document.nodes.set(panel.id,panel);root.document.nodes.set(canvas.id,canvas);
 for(const id of ['p-cw','p-ch','p-cbg','tl-fps','proj-fps','btn-dims-lock']){const el=base.element();el.id=id;el.value=id==='p-cbg'?'#ffffff':'';root.document.nodes.set(id,el);canvas.appendChild(el);}root.NemoNativeProjectSettingsController={edit(){}};return {root,panel,canvas};}
function nativeRender(h,change={}){const info={...base.metadata(),...change};base.Files.refreshLayerTable(h.root,info);return {info,result:base.UI.render(h.root,info,false,{...base.fixturePorts(h.root),projectSettings:()=>h.root.NemoNativeProjectSettingsController})};}
function workArea(fps=24){const root={state:{waIn:0,waOut:23,fps,totalFrames:24},_waIn:0,_waOut:23,_totalF:24,events:{},calls:[]};root.document=base.document();const bar=base.element(),left=base.element(),right=base.element(),lab=base.element();bar.querySelector=s=>s==='.wa-handle.left'?left:s==='.wa-handle.right'?right:s==='.wa-dur'?lab:null;root.document.nodes.set('wa-bar',bar);root.addEventListener=(kind,fn)=>{root.events[kind]=fn;};root.SM={setWorkArea:(a,b)=>{root.calls.push({inFrame:a,outFrame:b});return Promise.resolve({status:'unavailable'});}};
 const a=ui.indexOf('  function initWaDrag(){'),b=ui.indexOf('\n  }',a)+4,c=ui.indexOf('  window.updateWaBar=function(){'),d=ui.indexOf('\n  };',c)+5;
 vm.runInNewContext(ui.slice(a,b)+'\n'+ui.slice(c,d)+'\ninitWaDrag();',{window:root,state:root.state,document:root.document,FC:20});return {root,bar,left,right,lab};}
function playback(){const Motion=require(path.join(ROOT,'src/js/adapters/native-opacity-motion-surface.js'));let revision=0,fps=24,frame=0,playing=true,callback,stops=0;const owner={isActive:()=>true,identity:()=>({instanceId:'i',documentId:'d',contentRevision:revision})};let scheduler;scheduler=Motion.createPlaybackScheduler({frame:()=>frame,fps:()=>fps,playing:()=>playing,controller:()=>owner,advance:n=>n+1,navigate:async n=>{frame=n;return true;},stop:()=>{stops++;playing=false;scheduler.stop();},now:()=>0,request:fn=>{callback=fn;return 1;},cancel:()=>{}});return {start:()=>scheduler.start(owner),tick:async ms=>{callback(ms);await Promise.resolve();await Promise.resolve();},editFps:()=>{fps=48;revision++;},frame:()=>frame,playing:()=>playing,stops:()=>stops};}
const h={...base,panel,nativeRender,workArea,playback,timeline,ui,source,ROOT};
function playbackConsumer(){let meta={instanceId:'i',documentId:'d',lifecycleGeneration:1,contentRevision:0,documentSnapshotId:'s0',canvasW:320,canvasH:180,fps:24,totalFrames:120,layers:[{layerUid:'L'}]},now=0,ready=true,gate=null,beforeGate=null,restoreGate=null,writes=0,raf=0;const callbacks=new Map(),audio={starts:0,stops:0,redraws:0};
 const root={state:{...meta,currentFrame:0,playing:false,playDir:1,waIn:0,waOut:119},_layerSel:[0],document:h.document(),notices:[]};root.document.nodes.set('btn-play',h.element());
 root.requestAnimationFrame=fn=>{callbacks.set(++raf,fn);return raf;};root.cancelAnimationFrame=id=>callbacks.delete(id);
 const session={info:()=>meta,isActive:()=>true,readPresentedObjectSession:()=>ready?{...meta,available:true,presentation:{...meta,owner:'native',status:'presented',frame:root.state.currentFrame}}:null,
  async editProjectSettings(action,before){if(before!==meta)throw Error('stale');writes++;if(beforeGate)await beforeGate;meta={...meta,fps:action.fps,contentRevision:meta.contentRevision+1,documentSnapshotId:'s'+(meta.contentRevision+1)};
   const controller=root.NemoNativeProjectSettingsController;if(controller.willProject)controller.willProject(meta);root.state.fps=meta.fps;ready=false;if(gate)await gate;ready=true;if(controller.presented)controller.presented();
   return {apiVersion:2,ok:true,instanceId:meta.instanceId,documentId:meta.documentId,contentRevision:meta.contentRevision,result:{applied:true,historyEntriesAdded:1}};}};
 root.NemoNativeOpacityCutover={isActive:()=>true,blocksLegacy:()=>true,identity:()=>({instanceId:meta.instanceId,documentId:meta.documentId,contentRevision:meta.contentRevision})};
 root.NemoNativeObjectProject={desktop:()=>session};root.NemoNativeObjectProjectFile={projectionReadyForUI:()=>true};root.NemoNativeProjectSettingsIntent=require(path.join(h.ROOT,'src/js/adapters/native-project-settings-intent.js'));
 root.SMAudio={onPlayStart(){audio.starts++;},onPlayStop(){audio.stops++;},onLoop(){},invalidateWaveforms(){audio.redraws++;}};root.showToast=m=>root.notices.push(m);
 const source=h.timeline;function fn(name){const a=source.indexOf('function '+name+'('),b=source.indexOf('\n}',a);if(a<0||b<a)throw Error('missing '+name);return source.slice(a,b+2);}
 const context=vm.createContext({window:root,state:root.state,document:root.document,SMAudio:root.SMAudio,showToast:root.showToast,
 NemoNativeOpacityMotionSurface:require(path.join(h.ROOT,'src/js/adapters/native-opacity-motion-surface.js')),performance:{now:()=>now},
 goToFrame:async frame=>{root.state.currentFrame=frame;if(restoreGate)await restoreGate;return true;},updateUI:()=>{},renderOS:()=>{},renderArcs:()=>{},cancelAnimationFrame:root.cancelAnimationFrame,clearInterval});
 vm.runInContext('var playInt=null,playRaf=null,nativePlayScheduler=null;'+['startNativePlay','advancePlayFrame','startPlay','stopPlay'].map(fn).join('\n'),context);root.startPlay=context.startPlay;root.stopPlay=context.stopPlay;
 const controller=require(path.join(h.ROOT,'src/js/adapters/native-project-settings-desktop.js')).install(root);
 return {root,audio,start:()=>root.startPlay(),tick:async t=>{now=t;const entry=[...callbacks].at(-1);if(entry){callbacks.delete(entry[0]);entry[1](t);}await Promise.resolve();await Promise.resolve();},editFps:(fps=48)=>controller.edit({kind:'setFps',fps}),writes:()=>writes,hold:()=>{let release;gate=new Promise(r=>{release=r;});return release;},holdBeforeCommit:()=>{let release;beforeGate=new Promise(r=>{release=r;});return release;},holdRestore:()=>{let release;restoreGate=new Promise(r=>{release=r;});return release;}};
};

test('actual native presenter refreshes dimension and FPS input values',()=>{const p=h.panel();const r=h.nativeRender(p,{contentRevision:1,documentSnapshotId:'document-snapshot-1',canvasW:960,canvasH:540,fps:30});assert.equal(r.result,true);for(const [id,value]of[['p-cw',960],['p-ch',540],['tl-fps',30],['proj-fps',30]])assert.equal(Number(p.root.document.nodes.get(id).value),value);});
test('actual width input dispatches unlocked and ratio-locked sizes without document mutation',()=>{const p=h.panel(),root=p.root,calls=[];root.SM={setCanvasSize:(w,z)=>calls.push({w,h:z})};root.state.canvasW=320;root.state.canvasH=180;const start=h.timeline.indexOf('var _dimsLockRatio=null;'),end=h.timeline.indexOf('// Paint the panel',start);vm.runInNewContext(h.timeline.slice(start,end),{window:root,state:root.state,document:root.document});const width=root.document.nodes.get('p-cw'),lock=root.document.nodes.get('btn-dims-lock');width.value='960';width.listener('input').call(width);lock.listener('click').call(lock);width.listener('input').call(width);assert.deepEqual(calls,[{w:960,h:180},{w:960,h:540}]);assert.equal(root.state.canvasW,320);assert.equal(root.state.canvasH,180);});
test('actual work-area drag sends final coordinates while state/bar await native projection',()=>{const p=h.workArea();p.left.listener('mousedown')({clientX:0,preventDefault(){},stopPropagation(){}});p.root.events.mousemove({clientX:80});p.root.events.mouseup();assert.deepEqual(p.root.calls,[{inFrame:4,outFrame:23}]);assert.equal(p.root.state.waIn,0);assert.equal(p.bar.style.left,'0px');p.root.state.waIn=4;p.root.updateWaBar();assert.equal(p.bar.style.left,'80px');assert.equal(p.bar.style.width,'400px');});
test('actual native playback clock advances 24 frames over 1000ms at initial 24fps',async()=>{const p=h.playback();assert.equal(p.start(),true);await p.tick(1000);assert.equal(p.frame(),24);assert.equal(p.playing(),true);});

test('F04 visible Document controls remain visible in native inspector',()=>{const p=h.panel();h.nativeRender(p);for(const id of ['p-cw','p-ch','proj-fps','p-cbg']){let node=p.root.document.nodes.get(id);while(node){assert.notEqual(node.hidden,true,id);node=node.parentNode;}}});
test('F04 native background readback updates the existing background control',()=>{const p=h.panel();h.nativeRender(p,{contentRevision:1,documentSnapshotId:'document-snapshot-1',canvasBg:'#112233'});assert.equal(p.root.document.nodes.get('p-cbg').value,'#112233');});
test('F04 frame-only/native refresh preserves focused numeric edit text',()=>{const p=h.panel(),field=p.root.document.nodes.get('p-cw');field.value='777';p.root.document.activeElement=field;h.nativeRender(p);assert.equal(field.value,'777');});
test('F04 positive fractional storage FPS has matching work-area duration display',()=>{const p=h.workArea(0.5);p.root.updateWaBar();assert.equal(p.lab.textContent,'24 f · 48.0 s');});
test('F04 FPS native revision change preserves playing continuity after acceptance',async()=>{const p=playbackConsumer();p.start();await p.tick(100);assert.equal((await p.editFps()).status,'applied-presented');await p.tick(200);assert.equal(p.root.state.playing,true);assert.equal(p.root.state.currentFrame,6);assert.equal(p.audio.starts,2);assert.equal(p.audio.redraws,1);});

test('F04 explicit user stop during pending FPS edit cancels automatic resume',async()=>{const p=playbackConsumer();p.start();await p.tick(100);const release=p.hold(),edit=p.editFps();p.root.stopPlay();release();await edit;assert.equal(p.root.state.playing,false);assert.equal(p.audio.starts,1);});
test('F04 audio redraw failure is reported after native commit and playback remains paused',async()=>{const p=playbackConsumer();p.start();await p.tick(100);p.root.SMAudio.invalidateWaveforms=()=>{throw Error('audio UI unavailable');};const result=await p.editFps();assert.equal(result.status,'applied-unconfirmed');assert.equal(p.root.state.fps,48);assert.equal(p.root.state.playing,false);});
test('F05 the view-only Rulers toggle is reachable in the native Document box, unguarded, and released back (#2378)',()=>{const p=h.panel(),hidden=h.element(),rulers=h.element();
 hidden.hidden=true;rulers.id='btn-rulers';hidden.appendChild(rulers);p.canvas.appendChild(hidden);p.root.document.nodes.set('btn-rulers',rulers);let clicks=0;rulers.addEventListener('click',()=>clicks++);
 h.nativeRender(p);let node=rulers,inBox=false;while(node){assert.notEqual(node.hidden,true);inBox=inBox||node.id==='native-project-settings';node=node.parentNode;}
 assert.equal(inBox,true);assert.deepEqual(rulers.parentNode.children.map(c=>c.textContent),['Rulers','']);rulers.dispatch('click');assert.equal(clicks,1,'its own toggle still runs');
 for(const type of ['pointerdown','focus','input','change'])rulers.dispatch(type);assert.equal(p.root.NemoNativeProjectSettingsController.edits,undefined,'no settings edit is attempted');
 h.UI.restoreInspector(p.root);assert.strictEqual(rulers.parentNode,hidden);});
test('F04 inspector release restores original control nodes and listeners',()=>{const p=h.panel(),field=p.root.document.nodes.get('p-cw'),parent=field.parentNode;let calls=0;field.addEventListener('input',()=>calls++);h.nativeRender(p);assert.notStrictEqual(field.parentNode,parent);h.UI.restoreInspector(p.root);assert.strictEqual(field.parentNode,parent);field.dispatch('input');assert.equal(calls,1);});
test('F04 replacement blocks a gesture begun in the old document until fresh focus',()=>{const p=h.panel(),field=p.root.document.nodes.get('p-cw');let calls=0;field.addEventListener('input',()=>calls++);h.nativeRender(p);field.dispatch('pointerdown');const next={...h.metadata(),documentId:'replacement',lifecycleGeneration:2,documentSnapshotId:'replacement-snapshot-0',canvasW:640};h.Files.projectView(p.root,next);p.root.state.currentFrame=7;h.UI.render(p.root,next,false,{...h.fixturePorts(p.root),projectSettings:()=>p.root.NemoNativeProjectSettingsController});field.dispatch('input');assert.equal(calls,0);field.dispatch('focus');field.dispatch('input');assert.equal(calls,1);});
test('F04 dirty numeric blur/change cannot write into a replacement document before fresh focus',()=>{
 for(const id of['p-cw','proj-fps'])for(const replaceDocument of[true,false]){
  const p=h.panel(),root=p.root,field=root.document.nodes.get(id),writes=[],events=[];let current=h.metadata(),focusedValue;
  root.NemoNativeObjectProjectDesktop={settingsDiagnostic(_root,event,value){events.push({event,...value});}};
  root.NemoNativeProjectSettingsController={edit(action){writes.push({documentId:current.documentId,generation:current.lifecycleGeneration,action});}};
  root.SM={setCanvasSize:(w,height)=>root.NemoNativeProjectSettingsController.edit({kind:'setCanvasSize',width:w,height}),setFps:fps=>root.NemoNativeProjectSettingsController.edit({kind:'setFps',fps})};
  const listener=h.timeline.split('\n').find(line=>line.startsWith("document.getElementById('"+id+"').addEventListener('change'"));vm.runInNewContext(listener,{window:root,state:root.state,document:root.document,_dimsLockRatio:null});
  field.focus=()=>{root.document.activeElement=field;focusedValue=field.value;field.dispatch('focus',{type:'focus'});};
  field.blur=()=>{if(root.document.activeElement!==field)return;root.document.activeElement=null;if(field.value!==focusedValue)field.dispatch('change',{type:'change'});field.dispatch('blur',{type:'blur'});};
  h.nativeRender(p);field.focus();field.value=id==='p-cw'?'777':'29.97';
  current={...h.metadata(),documentId:replaceDocument?'replacement':'document',lifecycleGeneration:2,documentSnapshotId:'replacement-snapshot-0',canvasW:640,fps:48};
  h.Files.projectView(root,current);root.state.currentFrame=7;h.UI.render(root,current,false,{...h.fixturePorts(root),projectSettings:()=>root.NemoNativeProjectSettingsController});
  assert.equal(writes.length,0,id+' stale blur/change');assert.equal(root.document.activeElement,null);assert.equal(Number(field.value),id==='p-cw'?640:48);
  const rejection=events.find(e=>e.event==='stale-rejected');assert.equal(rejection.field,id);assert.equal(rejection.sourceEvent,'change');
  assert.equal(rejection.gestureDocumentId,'document');assert.equal(rejection.gestureLifecycleGeneration,1);
  assert.ok(events.findIndex(e=>e.event==='owner')<events.indexOf(rejection));
  field.focus();field.value=id==='p-cw'?'800':'31.5';field.blur();assert.equal(writes.length,1);assert.equal(writes[0].documentId,current.documentId);assert.equal(writes[0].generation,2);assert.equal(id==='p-cw'?writes[0].action.width:writes[0].action.fps,id==='p-cw'?800:31.5);
 }
});
test('F04 fractional FPS control forwards the actual typed finite value',()=>{const p=h.panel(),root=p.root,calls=[];root.SM={setFps:value=>calls.push(value)};for(const id of['proj-fps','tl-fps']){const line=h.timeline.split('\n').find(x=>x.startsWith("document.getElementById('"+id+"').addEventListener('change'"));require('vm').runInNewContext(line,{document:root.document,window:root});const field=root.document.nodes.get(id);field.value='29.97';field.dispatch('change');}assert.deepEqual(calls,[29.97,29.97]);});
test('F04 background control preserves popup alpha and refuses stale cached alpha on native RGB input',()=>{const p=h.panel(),root=p.root,calls=[];root.SM={setCanvasBg:value=>calls.push(value)};h.nativeRender(p,{contentRevision:1,documentSnapshotId:'document-snapshot-1',canvasBg:'#11223380'});const field=root.document.nodes.get('p-cbg'),line=h.timeline.split('\n').find(x=>x.startsWith("document.getElementById('p-cbg').addEventListener('input'"));require('vm').runInNewContext(line,{document:root.document,window:root});field.dispatch('focus');field.dispatch('input');field.value='#445566';field.dispatch('input');assert.deepEqual(calls,['#11223380','#445566']);});

test('F04 dragging a one-frame work area at zero keeps its one-frame extent',()=>{const p=h.workArea();p.root.state.waOut=0;p.root._waOut=0;p.bar.listener('mousedown')({target:p.bar,clientX:0,preventDefault(){},stopPropagation(){}});p.root.events.mousemove({clientX:80});p.root.events.mouseup();assert.deepEqual(p.root.calls,[{inFrame:4,outFrame:4}]);assert.equal(p.root.state.waOut,0);});

test('F04 FPS intent stops the old clock before delayed native dispatch/publication',async()=>{const p=playbackConsumer();p.start();await p.tick(100);const release=p.holdBeforeCommit(),edit=p.editFps();for(let n=0;n<12&&p.writes()===0;n++)await Promise.resolve();assert.equal(p.writes(),1);assert.equal(p.root.state.playing,false);await p.tick(200);assert.equal(p.root.state.currentFrame,2);release();assert.equal((await edit).status,'applied-presented');await p.tick(300);assert.equal(p.root.state.currentFrame,6);assert.equal(p.root.state.playing,true);});
test('F04 rapid FPS input stays bounded during native same-frame restoration',async()=>{const p=playbackConsumer();p.start();await p.tick(100);const release=p.holdRestore(),requests=[];for(let fps=1;fps<=50;fps++)requests.push(p.editFps(fps));for(const earlier of requests.slice(0,-1))assert.equal((await earlier).status,'superseded');assert.equal(p.writes(),0);release();assert.equal((await requests.at(-1)).status,'applied-presented');assert.equal(p.writes(),1);assert.equal(p.root.state.fps,50);assert.equal(p.root.state.playing,true);});
test('F04 delayed native width/height input then change preserves locked and unlocked dimensions',async()=>{
 for(const id of['p-cw','p-ch'])for(const locked of[true,false]){
  const p=h.panel(),root=p.root,calls=[],requests=[];let info=h.metadata(),release,publications=0;const gate=new Promise(resolve=>{release=resolve;});h.nativeRender(p);
  const session={info:()=>info,readPresentedObjectSession:()=>presented(info,7),async editProjectSettings(action,before){assert.strictEqual(before,info);calls.push(action);if(calls.length===1)await gate;
   const applied=info.canvasW!==action.width||info.canvasH!==action.height;if(applied){publications++;info={...info,canvasW:action.width,canvasH:action.height,contentRevision:info.contentRevision+1,documentSnapshotId:'ratio-'+publications};h.Files.refreshLayerTable(root,info);}
   return {apiVersion:2,ok:true,instanceId:info.instanceId,documentId:info.documentId,contentRevision:info.contentRevision,result:{applied,historyEntriesAdded:applied?1:0}};}};
  const intent=require('../src/js/adapters/native-project-settings-intent.js').create({currentSession:()=>session,frame:()=>7,selection:()=>[A.layerUid],available:()=>true});
  root.SM={setCanvasSize(w,height){const request=intent.edit({kind:'setCanvasSize',width:w,height});requests.push(request);return request;}};
  const start=h.timeline.indexOf('var _dimsLockRatio=null;'),end=h.timeline.indexOf('// Paint the panel',start),change=h.timeline.split('\n').find(line=>line.startsWith("document.getElementById('"+id+"').addEventListener('change'"));
  vm.runInNewContext(h.timeline.slice(start,end)+'\n'+change,{window:root,state:root.state,document:root.document});if(locked)root.document.nodes.get('btn-dims-lock').click();
  const field=root.document.nodes.get(id);field.dispatch('focus');field.value='';field.dispatch('input');
  assert.equal(calls.length,0,'clearing a dimension must not publish a coerced 1');assert.equal(requests.length,0);
  field.value=id==='p-cw'?'640':'360';field.dispatch('input');field.dispatch('change');
  assert.equal(calls.length,1);assert.equal(requests.length,2);assert.equal(root.state.canvasW,320);assert.equal(root.state.canvasH,180);release();await Promise.all(requests);
  assert.deepEqual([info.canvasW,info.canvasH],locked?[640,360]:id==='p-cw'?[640,180]:[320,360],id+' ratio='+locked);assert.equal(publications,1);assert.equal(calls.length,2);assert.equal((await requests[1]).status,'unchanged');
 }
});

test('F04 native dimension draft guard rejects invalid input/change but preserves authored one and live ratios',()=>{
 for(const id of['p-cw','p-ch'])for(const locked of[true,false]){
  const p=h.panel(),root=p.root,calls=[];root.SM={setCanvasSize:(w,height)=>calls.push([w,height])};h.nativeRender(p);
  const start=h.timeline.indexOf('var _dimsLockRatio=null;'),end=h.timeline.indexOf('// Paint the panel',start);
  const changes=['p-cw','p-ch'].map(key=>h.timeline.split('\n').find(line=>line.startsWith("document.getElementById('"+key+"').addEventListener('change'"))).join('\n');
  vm.runInNewContext(h.timeline.slice(start,end)+'\n'+changes,{window:root,state:root.state,document:root.document});
  if(locked)root.document.nodes.get('btn-dims-lock').click();const field=root.document.nodes.get(id);field.dispatch('focus');
  for(const value of['',' ','0','-','-1','1.5','Infinity','NaN','4294967296','1e3'])for(const event of['input','change']){field.value=value;field.dispatch(event);assert.equal(calls.length,0,id+' '+event+' '+JSON.stringify(value));}
  field.value='1';field.dispatch('input');field.dispatch('change');assert.equal(calls.length,2);assert.equal(calls[0][id==='p-cw'?0:1],1);
  field.value=id==='p-cw'?'640':'360';field.dispatch('input');field.dispatch('change');
  assert.deepEqual(calls.slice(2),[locked?[640,360]:id==='p-cw'?[640,180]:[320,360],locked?[640,360]:id==='p-cw'?[640,180]:[320,360]]);
  assert.deepEqual([root.state.canvasW,root.state.canvasH],[320,180],'UI must not become the document writer');
 }
});
