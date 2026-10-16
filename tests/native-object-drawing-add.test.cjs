'use strict';
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),vm=require('node:vm');
const UI=require('../src/js/adapters/native-object-ui.js'),Files=require('../src/js/adapters/native-object-project-file.js');
const Project=require('../src/js/adapters/native-object-project.js'),Transport=require('../src/js/adapters/application-mcp.js');
const Workflow=require('../src/js/application/native-object-save-as-workflow.js'),Ack=require('../src/js/application/native-object-saved-acknowledgement.js');
const SDK=require('../src/js/adapters/native-application.js');
const A={layerUid:'opaque A:雪',name:'<b>A</b>',visible:true},B={layerUid:'B',name:'B',visible:false},C={layerUid:'C',name:'',visible:true};
const clone=rows=>rows.map(x=>({...x}));
function metadata(layers=[A,B],revision=0,doc='document',generation=1){return {apiVersion:2,instanceId:'instance',documentId:doc,lifecycleGeneration:generation,
  contentRevision:revision,documentSnapshotId:doc+'-snapshot-'+revision,documentDigest:String(revision+1).repeat(64),byteCount:1000,
  canvasW:320,canvasH:180,fps:24,totalFrames:24,reference:null,undoDepth:0,redoDepth:0,layers:clone(layers)};}
function element(){
  const listeners={},classes=new Set();
  const el={children:[],style:{},dataset:{},attributes:{},value:'',max:0,textContent:'',className:'',type:'',offsetWidth:200,offsetHeight:120,
    classList:{add:x=>classes.add(x),remove:x=>classes.delete(x),contains:x=>classes.has(x),toggle(x,on){if(on===undefined)on=!classes.has(x);if(on)classes.add(x);else classes.delete(x);return on;}},
    setAttribute(k,v){this.attributes[k]=v;},addEventListener(k,f){listeners[k]=f;},
    replaceChildren(...children){this.children.forEach(child=>{child.parentNode=null;});this.children=[];children.forEach(child=>this.appendChild(child));},
    appendChild(child){if(child.parentNode)child.parentNode.removeChild(child);this.children.push(child);child.parentNode=this;return child;},
    insertBefore(child,next){if(child.parentNode)child.parentNode.removeChild(child);const i=next?this.children.indexOf(next):this.children.length;assert.ok(i>=0);this.children.splice(i,0,child);child.parentNode=this;return child;},
    removeChild(child){const i=this.children.indexOf(child);assert.ok(i>=0);this.children.splice(i,1);child.parentNode=null;},
    dispatch(type,event={}){return listeners[type]?.({target:this,preventDefault(){},stopPropagation(){},...event});},click(){return this.dispatch('click');},listener:type=>listeners[type],scrollIntoView(){},
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
const timeline=fs.readFileSync('src/js/timeline.js','utf8');
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
function desktop(initial=[A,B]){
  let active=false,revision=0,layers=clone(initial),doc='document',generation=1,gate=null,pending=null,fault=null,dispatchGate=null;
  let undo=[],redo=[];const log=[],calls=[],listeners=new Map(),root=shippedEntrypoints(view());
  root.NemoNativeObjectProject=Project;root.NemoApplicationMcpTransport=Transport;root.NemoNativeApplicationAdapter=SDK;
  root.showToast=message=>log.push('toast:'+message);
  const meta=()=>({...metadata(layers,revision,doc,generation),undoDepth:undo.length,redoDepth:redo.length});
  const binding=()=>({apiVersion:2,instanceId:'instance',documentId:doc,lifecycleGeneration:generation,subscriptionId:'subscription-'+generation,contentRevision:revision});
  async function invoke(command,args){const r=args&&(args.request||JSON.parse(args.requestJson));calls.push({command,request:r});
    if(command==='nemo_native_status'||command==='nemo_native_object_client_status')return {...meta(),available:active};
    if(command==='nemo_native_object_bootstrap'){
      if(active){generation++;doc='replacement-'+generation;}active=true;revision=0;layers=clone(initial);undo=[];redo=[];
      return {...meta(),requestId:r.requestId,resourceCount:0,viewportAvailable:false};
    }
    if(command==='nemo_native_object_reference')return {...meta(),requestId:r.requestId};
    if(command==='nemo_native_revision_sync'){
      log.push(r.action);if(r.action==='binding')return binding();if(r.action==='subscribe')return r.binding;
      if(r.action==='acknowledge'){log.push('ACK');assert.equal(root.document.nodes.get('layer-list').children.length,layers.length);pending=null;return null;}
      if(r.action==='disconnect')return null;
    }
    if(command==='nemo_native_object_preview'){
      if(pending)assert.deepEqual(r.revisionSync.event,pending);
      log.push('preview');if(gate){const wait=gate;gate=null;await wait;}
      return {status:fault==='preview'?'refused':'presented',workId:'work-'+generation+'-'+revision,viewGeneration:generation};
    }
    if(command==='nemo_native_dispatch'){
      if(r.operation==='query.document.object.layer.timing'&&r.payload.layerUid.startsWith('native opaque new:'))return {apiVersion:2,requestId:r.requestId,instanceId:r.instanceId,documentId:r.documentId,contentRevision:revision,ok:true,result:{...r.payload,instanceId:r.instanceId,documentId:r.documentId,cellKind:r.payload.frame===0?'key':'held',sourceFrame:0,hasContent:false,manualEdit:false}};
      if(r.operation==='query.document.object.layer.timing')return {apiVersion:2,requestId:r.requestId,instanceId:r.instanceId,
        documentId:r.documentId,contentRevision:revision,ok:false,error:{code:'unavailable',message:'This metadata fixture has no authored timing'}};
      if(r.operation==='command.document.object.layer.create'){
        assert.equal(r.expectedRevision,revision);log.push('public-create');
        if(dispatchGate){const wait=dispatchGate;dispatchGate=null;await wait;}
        if(fault==='create-refused')return {apiVersion:2,requestId:r.requestId,instanceId:r.instanceId,documentId:r.documentId,contentRevision:revision,ok:false,error:{code:'unavailable',message:'Native create refused'}};
        undo.push(clone(layers));redo=[];const uid='native opaque new:雪 '+(revision+1);
        layers=layers.concat({layerUid:uid,name:r.payload.name,visible:true,...(fault==='missing-defaults'?{}:{locked:false,color:'#F0A585',parentLayerUid:null})});revision++;
        return {apiVersion:2,requestId:fault==='wrong-request'?'foreign':r.requestId,instanceId:r.instanceId,documentId:r.documentId,
          contentRevision:revision,ok:true,result:{applied:true,historyEntriesAdded:1,layerUid:fault==='wrong-uid'?'B':uid}};
      }
      assert.equal(r.operation,'command.document.object.layer.visibility.set');assert.equal(r.expectedRevision,revision);
      assert.equal(r.payload.command,'object.layer.visibility.set');assert.equal(typeof r.payload.visible,'boolean');
      log.push('public-visibility');if(dispatchGate){const wait=dispatchGate;dispatchGate=null;await wait;}
      if(fault==='visibility-refused')return {apiVersion:2,requestId:r.requestId,instanceId:r.instanceId,documentId:r.documentId,
        contentRevision:revision,ok:false,error:{code:'unavailable',message:'Native visibility refused'}};
      undo.push(clone(layers));redo=[];layers=layers.map(layer=>layer.layerUid===r.payload.layerUid?{...layer,visible:r.payload.visible}:layer);revision++;
    }else{
      assert.equal(command,'nemo_native_object_dispatch');assert.ok(['command.document.object.undo','command.document.object.redo'].includes(r.operation));
      if(r.operation.endsWith('.undo')){assert.ok(undo.length);redo.push(clone(layers));layers=undo.pop();}
      else{assert.ok(redo.length);undo.push(clone(layers));layers=redo.pop();}revision++;
    }
    return {apiVersion:2,requestId:r.requestId,instanceId:r.instanceId,documentId:r.documentId,contentRevision:revision,ok:true,result:{applied:true,historyEntriesAdded:1}};
  }
  root.__TAURI__={core:{invoke},event:{listen:async(name,callback)=>{listeners.set(name,callback);return ()=>listeners.delete(name);}},dialog:{ask:async()=>true}};
  const session=Project.desktop(root);
  const render=UI.render;root.NemoNativeObjectUI={...UI,render(r,i,frameOnly,ports){log.push('UI:'+i.contentRevision);return render(r,i,frameOnly,ports);}};
  root.SMProjectDocument={baseName:()=> 'Project'};root.NemoNativeObjectSaveAsWorkflow=Workflow;root.NemoNativeObjectSavedAcknowledgement=Ack;
  root.requestAnimationFrame=f=>setImmediate(f);
  const file=Files.bind(root,{read:()=>({tabId:'tab',name:'Project'}),touchRecent(){},commit(){},renderRecents(){},
    opened(){log.push('opened');},notice(message){log.push('notice:'+message);},show(){root.document.nodes.get('start-screen').classList.remove('hid');},
    hide(){root.document.nodes.get('start-screen').classList.add('hid');},repaint(){}});
  return {root,session,file,log,calls,meta,hold(p){gate=p;},holdDispatch(p){dispatchGate=p;},fault(v){fault=v;},
    async open(){assert.equal(await file.openPath('/native.json'),true,log.join('|'));log.length=0;poisonFrames(root);},
    async external(next){const from=revision;undo.push(clone(layers));redo=[];layers=clone(next);revision++;pending={instanceId:'instance',documentId:doc,lifecycleGeneration:generation,
      requestId:'external-'+revision,fromRevision:from,toRevision:revision};return listeners.get('nemo-native-revision')({payload:pending});}};
}
const Add=require('../src/js/adapters/native-drawing-add.js');
const tick=()=>new Promise(setImmediate);
function caller(h){let available=true,current=h.session;
  const ports={currentSession:()=>current,available:()=>available,frame:()=>h.root.state.currentFrame,
    selection:()=>h.root._layerSel.map(i=>h.root.state.layers[i]?.layerUid),
    select:(uid,info)=>Project.selectLayerUI(h.root,info.layers.findIndex(r=>r.layerUid===uid),false,uid,info)};
  const add=Add.create(ports);h.root.NemoNativeDrawingAddController=add;
  const start=timeline.indexOf('  addLayer:function('),end=timeline.indexOf('\n  // Null layer',start);
  assert.ok(start>=0&&end>start);
  h.root.SM.addLayer=vm.runInNewContext('('+timeline.slice(start,end).trim().replace('addLayer:','').replace(/,\s*$/,'')+')',{
    window:h.root,state:h.root.state,Promise,
    saveAllLayerFrames(){throw Error('LEGACY WRITE');},pushUndoLayers(){throw Error('LEGACY HISTORY');},createUserLayer(){throw Error('LEGACY UID');}});
  return {add,ports,available:v=>{available=v;},current:v=>{current=v;}};
}
const creates=h=>h.calls.filter(c=>c.command==='nemo_native_dispatch'&&c.request.operation==='command.document.object.layer.create');
test('ordinary Add uses public SDK returned UID, native fields, current frame and one native history entry',async()=>{
  const h=desktop();await h.open();h.root.state.currentFrame=7;await h.session.presentPreview(7);const original=h.root.state.layers[0],c=caller(h);
  const result=await h.root.SM.addLayer();assert.equal(result.status,'created-selected');assert.match(result.layerUid,/^native opaque new:雪 /);
  assert.deepEqual(c.ports.selection(),[result.layerUid]);assert.equal(h.root.state.currentFrame,7);assert.equal(h.root.state.layers[0],original);
  assert.equal(creates(h).length,1);assert.deepEqual(creates(h)[0].request.payload,{command:'object.layer.create',name:'Layer 1'});
  assert.equal(h.session.info().undoDepth,1);assert.equal(h.session.info().redoDepth,0);
  const added=h.session.info().layers[2];assert.equal(added.locked,false);assert.equal(added.color,'#F0A585');assert.equal(added.parentLayerUid,null);
  assert.equal(Object.hasOwn(h.session.info().layers[0],'color'),false,'older metadata stays absent');
  const timing=await h.session.readLayerTiming(result.layerUid,7,h.session.info());
  assert.equal(timing.cellKind,'held');assert.equal(timing.sourceFrame,0);assert.equal(timing.hasContent,false);
  await h.session.historyFromUi('undo');assert.equal(h.session.info().layers.length,2);await h.session.historyFromUi('redo');
  assert.equal(h.session.info().layers[2].layerUid,result.layerUid);assert.equal(creates(h).length,1);
});
test('ordinary Add chooses the first unused baseline Layer N display name',async()=>{
  const h=desktop([{...A,name:'Layer 1'},{...B,name:'Layer 3'}]);await h.open();caller(h);
  assert.equal((await h.root.SM.addLayer()).status,'created-selected');assert.equal(creates(h)[0].request.payload.name,'Layer 2');
});
test('held final preview prevents optimistic selection and duplicate in-flight Add',async()=>{
  const h=desktop();await h.open();const c=caller(h),before=c.ports.selection();let release;h.hold(new Promise(r=>{release=r;}));
  const task=h.root.SM.addLayer();await tick();assert.equal(h.session.readPresentedObjectSession(),null);assert.deepEqual(c.ports.selection(),before);
  assert.deepEqual(await h.root.SM.addLayer(),{status:'busy'});assert.equal(creates(h).length,1);release();
  assert.equal((await task).status,'created-selected');assert.equal(c.add.isPending(),false);
});
for(const fault of ['create-refused','wrong-request','wrong-uid','preview'])test('ordinary Add '+fault+' cannot publish a selected or fabricated UID',async()=>{
  const h=desktop();await h.open();const c=caller(h),before=c.ports.selection();h.fault(fault);const result=await h.root.SM.addLayer();
  assert.equal(result.status,'unconfirmed');assert.deepEqual(c.ports.selection(),before);assert.equal(creates(h).length,1);
  if(fault==='create-refused'){assert.equal(h.session.info().layers.length,2);assert.equal(h.session.info().undoDepth,0);}
});
test('missing native field defaults never select a merely appended compatibility row',async()=>{
  const h=desktop();await h.open();const c=caller(h),before=c.ports.selection();h.fault('missing-defaults');
  assert.equal((await h.root.SM.addLayer()).status,'created-unselected');assert.deepEqual(c.ports.selection(),before);
});
test('stale queued pin refuses before native create dispatch',async()=>{
  const h=desktop();await h.open();const pin=h.session.info();await h.external([{...A,visible:false},B]);
  await assert.rejects(h.session.createLayer('Layer 1',pin),/stale/);assert.equal(creates(h).length,0);assert.equal(h.session.info().layers.length,2);
});
test('availability refusal and missing ordinary caller consume native route without legacy fallback',async()=>{
  const h=desktop();await h.open();const c=caller(h);c.available(false);assert.equal((await h.root.SM.addLayer()).status,'unavailable');
  delete h.root.NemoNativeDrawingAddController;assert.equal((await h.root.SM.addLayer()).status,'unavailable');assert.equal(creates(h).length,0);
});
for(const change of ['dispose','frame','session','capability'])test('late '+change+' intent fences selection after an actual create receipt',async()=>{
  const h=desktop();await h.open();const c=caller(h),before=c.ports.selection();let release;h.holdDispatch(new Promise(r=>{release=r;}));
  const task=h.root.SM.addLayer();await tick();if(change==='dispose')c.add.dispose();if(change==='frame')h.root.state.currentFrame=3;if(change==='session')c.current({});if(change==='capability')c.available(false);
  release();assert.equal((await task).status,'created-unselected');assert.deepEqual(c.ports.selection(),before);assert.equal(creates(h).length,1);
});

test('a consumed selection callback is not mistaken for confirmed selected UID',async()=>{
  const h=desktop();await h.open();const c=caller(h),before=c.ports.selection();c.ports.select=()=>true;
  assert.equal((await h.root.SM.addLayer()).status,'created-unselected');assert.deepEqual(c.ports.selection(),before);
});

test('retired caller cannot create another layer from a stale callback',async()=>{
  const h=desktop();await h.open();const c=caller(h);c.add.dispose();
  assert.equal((await h.root.SM.addLayer()).status,'unavailable');assert.equal(creates(h).length,0);
});
