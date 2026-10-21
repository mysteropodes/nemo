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
test('metadata presenter builds real text/button rows and explicit unavailable timing without reading frames',()=>{
  const root=view();poisonFrames(root);assert.equal(UI.render(root,metadata(),false,fixturePorts(root)),true);
  const rows=root.document.nodes.get('layer-list').children;
  assert.equal(rows.length,2);assert.equal(pick(root,0).textContent,'<b>A</b> — Visible');assert.equal(rows[0].dataset.layerUid,A.layerUid);
  assert.equal(pick(root,1).textContent,'B — Hidden');assert.equal(pick(root,0).type,'button');assert.equal(eye(root,1).textContent,'Show');
  assert.equal(root.document.nodes.get('frame-hdr').children[0].textContent,'Frame 8 / 24');
  assert.match(root.document.nodes.get('frame-grid').children[0].textContent,/timing is unavailable/);
  assert.equal(root.document.nodes.get('info-badge').style.display,'none');
});
test('selection needs exact outer AND final-presented identity, never constructs frame data',()=>{
  const root=view();assert.equal(Files.refreshLayerTable(root,metadata([A,B,C],1)),true);
  const next=metadata([A,B,C],1);poisonFrames(root);
  for(const pin of [null,presented(next,6),presented({...next,documentId:'foreign'}),{...presented(next),presentation:{...presented(next).presentation,documentSnapshotId:'foreign'}}]){
    assert.equal(UI.select(root,next,2,false,'C',pin,fixturePorts(root)),false);assert.equal(root.state.activeLayerIdx,0);
  }
  assert.equal(UI.select(root,next,2,false,'C',presented(next),fixturePorts(root)),true);
  assert.equal(root.state.activeLayerIdx,2);assert.deepEqual(root._layerSel,[2]);assert.equal(root.state.currentFrame,7);
  assert.equal(pick(root,2).attributes['aria-pressed'],'true');
  assert.equal(UI.select(root,next,2,false,'replaced',presented(next),fixturePorts(root)),false);
});
test('stale projection and absent UI nodes refuse; failed selection restores editor state',()=>{
  const root=view(),selection=root._layerSel,paths=root.selectedPaths;
  assert.throws(()=>UI.render(root,{...metadata(),documentSnapshotId:'foreign'},false,fixturePorts(root)),/uncorrelated/);
  root.document.nodes.delete('frame-grid');
  assert.throws(()=>UI.select(root,metadata(),1,false,'B',presented(metadata()),fixturePorts(root)),/unavailable/);
  assert.equal(root.state.activeLayerIdx,0);assert.equal(root._layerSel,selection);assert.equal(root.selectedPaths,paths);
});
test('unadmitted inspector values are hidden explicitly, with original nodes/visibility restored on non-object routing',()=>{
  const root=view(),panel=element(),width=element(),hiddenSection=element();
  width.value=1920;width.hidden=false;hiddenSection.hidden=true;panel.appendChild(width);panel.appendChild(hiddenSection);
  root.document.nodes.set('props-panel',panel);UI.render(root,metadata(),false,fixturePorts(root));
  assert.equal(width.hidden,true);assert.equal(width.value,1920,'do not fabricate native property values in legacy controls');
  assert.equal(panel.children[2].children[1].textContent,'Native drawing property inspectors are unavailable.');
  UI.render(root,metadata(),false,fixturePorts(root));assert.equal(panel.children.length,3,'one owned availability notice');
  const foreign=element();panel.appendChild(foreign);root.NemoNativeObjectProject=Project;
  assert.equal(Project.paintUI(root),false);assert.equal(panel.children.length,3);assert.equal(panel.children[0],width);
  assert.equal(width.hidden,false);assert.equal(hiddenSection.hidden,true);assert.equal(panel.children[2],foreign);
});
test('native inspector reuses actual bound Reference nodes, preserves collapsed layout and restores original parents/styles',async()=>{
  const root=view(),doc=root.document;
  function add(id,parent){const n=element();n.id=id;n.hidden=false;doc.nodes.set(id,n);if(parent)parent.appendChild(n);return n;}
  const panel=add('props-panel'),header=add('props-panel-hdr-row',panel),rail=add('props-panel-rail',panel),oldRail=element();rail.appendChild(oldRail);
  panel.classList.add('collapsed');panel.style.width='';root._propsExpandedWidth='310px';
  const layer=add('layer-sec',panel);layer.style.display='block';const home=element();layer.appendChild(home);
  const trigger=add('p-ref-menu',home),next=element();home.appendChild(next);
  const input=add('ref-file-input',layer),pop=add('ref-pop',layer);
  add('btn-ref-import',pop);add('btn-ref-remove',pop);add('p-ref-name',pop);
  const on=add('p-ref-on',pop);add('p-ref-opacity',pop);add('p-ref-offset',pop);
  const oldShow={'layer-sec':true};root._lastPropsShow=oldShow;root.SM={t:key=>key};root.innerWidth=1000;root.innerHeight=800;
  let reference={id:'native-ref',name:'Native reference.png',visible:true,opacity:0.5,offsetFrames:0};const settings=[];
  root.NemoNativeObjectReference={desktop:()=>({read:()=>reference,settings:async patch=>{settings.push(patch);reference={...reference,...patch};}})};
  Object.defineProperty(root.state,'refMedia',{get(){throw Error('legacy reference read');}});
  const context=vm.createContext({window:root,document:doc,state:root.state,SM:root.SM,showToast:()=>{},
    requestAnimationFrame:f=>f(),syncPropsPanelCollapseTitle:()=>{},localStorage:{setItem(){}}});
  root.localStorage=context.localStorage;
  root.SMWorkspace=require('../src/js/adapters/native-workspace-ui.js').presentation(root,{order:()=>null,applyOrder(){}});
  vm.runInContext(shipped('renderPropsPanelRail')+'\n'+shipped('togglePropsPanelCollapse'),context);
  root.renderPropsPanelRail=context.renderPropsPanelRail;root.togglePropsPanelCollapse=context.togglePropsPanelCollapse;
  vm.runInContext(fs.readFileSync('src/js/reference-bridge.js','utf8'),context);
  const inputBinding=input.listener('change'),settingBinding=on.listener('change');
  UI.render(root,metadata(),false,fixturePorts(root));
  assert.equal(doc.getElementById('p-ref-menu'),trigger);assert.equal(doc.getElementById('ref-file-input'),input);assert.equal(doc.getElementById('ref-pop'),pop);
  assert.equal(input.listener('change'),inputBinding);assert.equal(on.listener('change'),settingBinding);
  assert.notEqual(trigger.parentNode,home);assert.equal(layer.hidden,true);assert.equal(layer.style.display,'none');
  assert.equal(header.hidden,false);assert.equal(rail.hidden,false);assert.equal(panel.classList.contains('collapsed'),true);assert.equal(panel.style.width,'');
  assert.match(doc.getElementById('native-object-inspector').className,/psec/);
  assert.match(fs.readFileSync('src/css/style.css','utf8'),/#props-panel\.collapsed \.psec\{display:none!important;/);
  assert.equal(rail.children.length,1);rail.children[0].click();assert.equal(panel.classList.contains('collapsed'),false);assert.equal(panel.style.width,'310px');
  trigger.click();assert.equal(pop.parentNode,doc.body,'actual reference bridge opens the SAME popup outside hidden legacy ancestors');
  on.checked=false;on.dispatch('change');await new Promise(setImmediate);
  assert.deepEqual(settings.map(x=>({...x})),[{visible:false}]);assert.match(trigger.textContent,/Native reference.png/);
  root.SMReference.openOptions();assert.equal(pop.parentNode,layer,'actual close returns the same bound controls');
  root.NemoNativeObjectProject=Project;assert.equal(Project.paintUI(root),false);
  assert.equal(trigger.parentNode,home);assert.equal(trigger.nextSibling,next);assert.equal(layer.hidden,false);assert.equal(layer.style.display,'block');
  assert.equal(root._lastPropsShow,oldShow);assert.equal(rail.children[0],oldRail);
  assert.equal(input.listener('change'),inputBinding);assert.equal(on.listener('change'),settingBinding);
  trigger.click();assert.equal(pop.parentNode,doc.body,'restored trigger keeps its original live binding');
});
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
      if(r.operation==='query.document.object.layer.timing')return {apiVersion:2,requestId:r.requestId,instanceId:r.instanceId,
        documentId:r.documentId,contentRevision:revision,ok:false,error:{code:'unavailable',message:'This metadata fixture has no authored timing'}};
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
  require('./helpers/native-viewport-underlay.cjs').install(root);
  const session=Project.desktop(root);
  const render=UI.render;root.NemoNativeObjectUI={...UI,render(r,i,frameOnly,ports){log.push('UI:'+i.contentRevision);return render(r,i,frameOnly,ports);}};
  require('../src/js/bootstrap/native-opacity-application.js').bindObjectUI(root);
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
test('actual Open/New and four shipped UI entrypoints avoid legacy frame readers; row click selects through a final receipt',async()=>{
  const h=desktop();await h.open();
  const retired=pick(h.root,0);
  for(const name of ['updateUI','renderTimeline','renderLayerList'])assert.doesNotThrow(()=>h.root[name]());
  pick(h.root,1).click();assert.equal(h.root.state.activeLayerIdx,1);
  assert.doesNotThrow(()=>h.root.SM.setActiveLayer(0));assert.equal(h.root.state.activeLayerIdx,0);
  h.log.length=0;
  assert.equal(await h.file.newProject({w:320,h:180,fps:24,name:'New'}),true);
  assert.equal(h.session.info().documentId,'replacement-2');assert.match(h.root.document.nodes.get('frame-hdr').children[0].textContent,/Frame 1/);
  assert.ok(h.log.indexOf('preview')<h.log.indexOf('UI:0'),'actual reveal follows the first presented preview');
  h.root.SM.setActiveLayer(1);retired.click();assert.equal(h.root.state.activeLayerIdx,1,'retired row cannot select a replacement document with reused UID');
});
test('actual frame-only updates preserve native row/control identity and focus without DOM allocations',async()=>{
  const h=desktop();await h.open();const root=h.root,row=root.document.nodes.get('layer-list').children[0],button=eye(root,0),created=root.document.created;
  root.document.activeElement=button;
  for(let frame=1;frame<9;frame++){root.state.currentFrame=frame;root.updateUI(true);root.renderLayerList(true);
    assert.equal(root.document.nodes.get('layer-list').children[0],row);assert.equal(eye(root,0),button);}
  assert.equal(root.document.activeElement,button);assert.equal(root.document.created,created);
  assert.equal(root.document.nodes.get('frame-hdr').children[0].textContent,'Frame 9 / 24');
  await h.session.presentPreview(8);root.SM.setActiveLayer(1);
  assert.notEqual(root.document.nodes.get('layer-list').children[0],row,'selection changes invalidate row bindings');
  const selected=root.document.nodes.get('layer-list').children[0];
  assert.equal(await h.file.newProject({w:320,h:180,fps:24,name:'Replacement'}),true);
  assert.notEqual(root.document.nodes.get('layer-list').children[0],selected,'replacement document invalidates cached rows');
});
test('frame-only cache invalidates on native revision, selection and replaced row/control DOM',async()=>{
  const h=desktop();await h.open();const root=h.root,list=root.document.nodes.get('layer-list'),before=list.children[0];
  await h.external([A,B,C]);assert.notEqual(list.children[0],before);
  let prior=list.children[0];root._layerSel=[0,1];root.updateUI(true);assert.notEqual(list.children[0],prior);
  prior=list.children[0];prior.replaceChildren(element(),eye(root,0));root.updateUI(true);assert.notEqual(list.children[0],prior);
  prior=list.children[0];list.replaceChildren(element());root.updateUI(true);assert.notEqual(list.children[0],prior);assert.equal(list.children.length,3);
});
test('frame-only cache never bypasses current projection validation',async()=>{
  const h=desktop();await h.open();const root=h.root,created=root.document.created;
  root.state.layers[0].visible=false;
  assert.throws(()=>root.updateUI(true),/uncorrelated/);assert.equal(root.document.created,created);
  assert.equal(root.document.nodes.get('start-screen').classList.contains('hid'),false);
});
test('real subscriber paints added metadata before preview/ACK; pending preview cannot select the new row',async()=>{
  const h=desktop();await h.open();let done;h.hold(new Promise(r=>{done=r;}));
  const change=h.external([A,B,C]);await new Promise(setImmediate);
  assert.equal(h.root.document.nodes.get('layer-list').children.length,3);assert.equal(h.log.includes('ACK'),false);
  pick(h.root,2).click();assert.equal(h.root.state.activeLayerIdx,0);
  done();await change;
  assert.ok(h.log.indexOf('UI:1')<h.log.indexOf('preview'));assert.ok(h.log.indexOf('preview')<h.log.indexOf('ACK'));
  poisonFrames(h.root);pick(h.root,2).click();assert.equal(h.root.state.activeLayerIdx,2);
});
test('real local history updates selected-row metadata and actual native UI before preview',async()=>{
  const h=desktop();await h.open();await h.external([A,B,C]);await h.session.historyFromUi('undo');h.log.length=0;await h.session.historyFromUi('redo');
  assert.equal(h.root.document.nodes.get('layer-list').children.length,3);
  assert.ok(h.log.indexOf('UI:1')<h.log.indexOf('preview'));assert.equal(h.log.includes('ACK'),false);
  h.root.SM.setActiveLayer(2);await h.session.historyFromUi('undo');assert.equal(h.root.state.activeLayerIdx,1);
  assert.equal(h.root.document.nodes.get('layer-list').children.length,2);
});
for(const fault of ['missing','refused','stale','preview'])test('native '+fault+' failure never falls back or acknowledges an external revision',async()=>{
  const h=desktop();await h.open();
  if(fault==='missing')delete h.root.NemoNativeObjectUI;
  if(fault==='refused')h.root.NemoNativeObjectUI={...UI,render:()=>false};
  if(fault==='stale')h.root.NemoNativeObjectUI={...UI,render:(root,info,frameOnly,ports)=>UI.render(root,{...info,documentId:'foreign'},frameOnly,ports)};
  if(fault==='preview')h.fault('preview');
  await h.external([A,B,C]);assert.equal(h.log.includes('ACK'),false);
  assert.equal(h.session.isIndeterminate(),true);
  for(const name of ['updateUI','renderTimeline','renderLayerList'])assert.throws(()=>h.root[name](),/Native/);
  assert.equal(h.root.document.nodes.get('start-screen').classList.contains('hid'),false);
});
test('non-object ownership retains the exact shipped legacy entry routes',()=>{
  const root=shippedEntrypoints(view());root.NemoNativeObjectProject=Project;
  for(const name of ['updateUI','renderTimeline','renderLayerList'])assert.throws(()=>root[name](),/LEGACY FRAME READER/);
  assert.throws(()=>root.SM.setActiveLayer(1),/LEGACY FRAME READER/);
});
test('ordinary eye button uses real desktop/SDK/shared transport, waits for native metadata and stays usable after preview',async()=>{
  const h=desktop();await h.open();const row=h.root.state.layers[0],before=h.calls.length;
  assert.ok(eye(h.root,0),'ordinary native visibility control must exist');
  let dispatchDone,previewDone;h.holdDispatch(new Promise(r=>{dispatchDone=r;}));h.hold(new Promise(r=>{previewDone=r;}));
  const change=eye(h.root,0).click();await new Promise(setImmediate);
  assert.equal(h.root.state.layers[0],row);assert.equal(row.visible,true);assert.equal(h.meta().layers[0].visible,true);
  assert.equal(eye(h.root,0).textContent,'Hide','no optimistic row or eye change');
  dispatchDone();await new Promise(setImmediate);
  assert.equal(row.visible,false);assert.equal(h.meta().layers[0].visible,false);assert.equal(eye(h.root,0).textContent,'Show');
  assert.notEqual(eye(h.root,0).disabled,true,'pre-preview rendering must not permanently disable the control');
  assert.equal(await eye(h.root,0).click(),false,'pending presentation cannot dispatch a second click');
  assert.equal(h.calls.slice(before).filter(c=>c.command==='nemo_native_dispatch'&&c.request.operation==='command.document.object.layer.visibility.set').length,1);
  previewDone();assert.equal((await change).ok,true);
  assert.ok(h.log.indexOf('public-visibility')<h.log.indexOf('UI:1'));assert.ok(h.log.indexOf('UI:1')<h.log.indexOf('preview'));
  assert.equal((await eye(h.root,0).click()).ok,true);assert.equal(row.visible,true);assert.equal(h.meta().undoDepth,2);
  await h.session.historyFromUi('undo');assert.equal(row.visible,false);assert.equal(eye(h.root,0).textContent,'Show');
  await h.session.historyFromUi('redo');assert.equal(row.visible,true);assert.equal(eye(h.root,0).textContent,'Hide');
  const sent=h.calls.slice(before);assert.equal(sent.filter(c=>c.command==='nemo_native_dispatch'&&c.request.operation==='command.document.object.layer.visibility.set').length,2);
  assert.ok(sent.filter(c=>c.command==='nemo_native_dispatch').every(c=>['command.document.object.layer.visibility.set','query.document.object.layer.timing'].includes(c.request.operation)));
  assert.equal(sent.filter(c=>c.command==='nemo_native_object_dispatch').length,2,'private object route used ONLY by the existing history controls');
  assert.ok(sent.filter(c=>c.command==='nemo_native_object_dispatch').every(c=>/\.(undo|redo)$/.test(c.request.operation)));
  assert.equal(h.log.includes('ACK'),false,'local command completion is not an external subscription ACK');
});
test('retired eye controls reject changed revision and replacement owner even when UID is reused',async()=>{
  const h=desktop();await h.open();const retired=eye(h.root,0);
  await h.external([A,B,C]);const before=h.calls.length;
  assert.equal(await retired.click(),false);assert.equal(h.calls.length,before);
  const oldOwner=eye(h.root,0);assert.equal(await h.file.newProject({w:320,h:180,fps:24,name:'Replacement'}),true);
  const after=h.calls.length;assert.equal(await oldOwner.click(),false);assert.equal(h.calls.length,after);
  assert.equal((await eye(h.root,0).click()).ok,true,'current owner control still uses the admitted visibility route');
});
test('eye refuses during external synchronization without leaving its pre-preview replacement disabled',async()=>{
  const h=desktop();await h.open();let done;h.hold(new Promise(r=>{done=r;}));
  const change=h.external([A,B,C]);await new Promise(setImmediate);const before=h.calls.length;
  assert.equal(await eye(h.root,0).click(),false);assert.equal(h.calls.length,before);
  done();await change;assert.equal((await eye(h.root,0).click()).ok,true);
});
for(const fault of ['sdk-missing','visibility-refused'])test('eye '+fault+' is a visible refusal with no optimistic mutation, private fallback or retry',async()=>{
  const h=desktop();await h.open();const before=h.calls.length;
  if(fault==='sdk-missing')delete h.root.NemoNativeApplicationAdapter;else h.fault(fault);
  assert.equal(await eye(h.root,0).click(),false);assert.equal(h.meta().layers[0].visible,true);assert.equal(h.root.state.layers[0].visible,true);
  assert.equal(h.meta().undoDepth,0);assert.equal(eye(h.root,0).textContent,'Hide');
  const sent=h.calls.slice(before);assert.equal(sent.filter(c=>c.command==='nemo_native_dispatch').length,fault==='sdk-missing'?0:1);
  assert.equal(sent.filter(c=>c.command==='nemo_native_object_dispatch').length,0);
  assert.ok(h.log.includes('toast:Native layer visibility was not confirmed.'));
});
test('visibility preview failure and broken refusal notice never become successful eye publication',async()=>{
  const h=desktop();await h.open();h.fault('preview');h.root.showToast=()=>{throw Error('broken notice');};
  assert.equal(await eye(h.root,0).click(),false);
  assert.equal(h.session.readPresentedObjectSession(),null);assert.equal(h.log.includes('ACK'),false);
  const sent=h.calls.filter(c=>c.command==='nemo_native_dispatch'&&c.request.operation==='command.document.object.layer.visibility.set');assert.equal(sent.length,1);
  const count=h.calls.length;assert.equal(await eye(h.root,0).click(),false);assert.equal(h.calls.length,count,'unconfirmed receipt must prevent redispatch');
});
for(const fault of ['missing','refused','stale'])test('retained eye refuses '+fault+' presenter before dispatching native visibility',async()=>{
  const h=desktop();await h.open();const button=eye(h.root,0),before=h.calls.length;
  if(fault==='missing')delete h.root.NemoNativeObjectUI;
  if(fault==='refused')h.root.NemoNativeObjectUI={...UI,render:()=>false};
  if(fault==='stale')h.root.NemoNativeObjectUI={...UI,render:(root,info,frameOnly,ports)=>UI.render(root,{...info,documentId:'foreign'},frameOnly,ports)};
  assert.equal(await button.click(),false);assert.equal(h.calls.length,before);
  assert.equal(h.meta().layers[0].visible,true);assert.equal(h.root.document.nodes.get('start-screen').classList.contains('hid'),false);
});
test('actual presenter and events use explicit narrow ports without resolving concrete providers',async()=>{
  const root=view(),info=metadata(),calls=[];let current=null;
  const session={info:()=>info,isActive:()=>true,readPresentedObjectSession:()=>presented(info),
    toggleLayerVisibility:uid=>{calls.push(['toggle',uid]);return Promise.resolve({ok:true});}};
  const ports=Object.freeze({projectionReady:value=>Files.projectionReadyForUI(root,value),currentSession:()=>{calls.push('read-live');return current;},
    selectLayer:(index,preserve,uid,expected)=>{calls.push(['select',index,preserve,uid,expected]);return true;},
    repaint:frameOnly=>UI.render(root,info,frameOnly,ports)});
  for(const name of ['NemoNativeObjectProject','NemoNativeObjectProjectFile'])Object.defineProperty(root,name,{get(){throw Error('concrete provider lookup: '+name);}});
  assert.equal(UI.render(root,info,false,ports),true);pick(root,1).click();
  assert.deepEqual(calls.find(x=>Array.isArray(x)&&x[0]==='select'),['select',1,false,'B',info]);
  const button=eye(root,0);assert.equal(await button.click(),false);assert.equal(calls.filter(x=>Array.isArray(x)&&x[0]==='toggle').length,0);
  current=session;assert.equal((await button.click()).ok,true);assert.deepEqual(calls.findLast(x=>Array.isArray(x)&&x[0]==='toggle'),['toggle',A.layerUid]);
  current={...session,info:()=>({...info,documentId:'replacement'})};assert.equal(await button.click(),false);
  current={...session,info:()=>({...info,contentRevision:1})};assert.equal(await button.click(),false);
  assert.equal(calls.filter(x=>Array.isArray(x)&&x[0]==='toggle').length,1,'retired pins cannot use the live replacement');
  assert.equal(root.state.layers[0].visible,true,'callback dispatch never applies optimistic metadata');
  assert.doesNotMatch(fs.readFileSync('src/js/adapters/native-object-ui.js','utf8'),/NemoNativeObjectProject(?:File)?\b/);
});
test('production composition reuses frozen callback identities and resolves session state at click time',async()=>{
  const h=desktop(),bindings=[],render=h.root.NemoNativeObjectUI.render;
  h.root.NemoNativeObjectUI={...UI,render(root,info,frameOnly,ports){bindings.push(ports);return render(root,info,frameOnly,ports);}};
  await h.open();const first=bindings[0],old=first.currentSession().info(),button=eye(h.root,0);
  h.root.updateUI(true);assert.equal(eye(h.root,0),button);assert.equal(Object.isFrozen(first),true);
  assert.deepEqual(Object.keys(first).sort(),['bindPolicyEntries','clearDrawingSelection','currentSession','disposePolicyEntries','editProperty','editPropertyTrack','fontResources','layerPolicy','projectSettings','projectionReady','readProperty','readPropertyMetadata','readShyMode','readTransportGeneration','repaint','selectLayer','setShyMode']);assert.equal(first.projectSettings(),null);const settings=Object.freeze({edit(){}});h.root.NemoNativeProjectSettingsController=settings;assert.strictEqual(first.projectSettings(),settings);delete h.root.NemoNativeProjectSettingsController;
  assert.equal(first.fontResources(),null);
  const fontCalls=[],assetId=' opaque font / 雪 ',source={kind:'google',family:'Poppins',weight:700,italic:false},signal={aborted:false};
  h.root.SMVectorText={listNativeFontAssets:async()=>{fontCalls.push(['list']);return [{assetId}];},
    loadNativeFontAsset:async id=>{fontCalls.push(['load',id]);return {loaded:id};},
    importNativeFontSource:async(value,cancel)=>{fontCalls.push(['import',value,cancel]);return {assetId};}};
  const fonts=first.fontResources();assert.equal(Object.isFrozen(fonts),true);
  assert.deepEqual(Object.keys(fonts).sort(),['importSource','list','load']);
  assert.deepEqual(await fonts.list(),[{assetId}]);assert.deepEqual(await fonts.load(assetId),{loaded:assetId});
  assert.deepEqual(await fonts.importSource(source,signal),{assetId});
  assert.deepEqual(fontCalls,[['list'],['load',assetId],['import',source,signal]]);
  delete h.root.SMVectorText;assert.equal(first.fontResources(),null);
  const generation=first.readTransportGeneration();assert.equal(generation,first.currentSession().readSubscriptionGeneration());
  await h.external([A,B,C]);assert.equal(first.currentSession().info().contentRevision,old.contentRevision+1);
  assert.equal(first.readTransportGeneration(),generation,'content revision does not invent a subscription generation');
  await h.file.newProject({w:320,h:180,fps:24,name:'Replacement'});
  assert.notEqual(first.currentSession().info().documentId,old.documentId);assert.ok(bindings.every(value=>value===first));
  assert.ok(first.readTransportGeneration()>generation,'replacement advances the existing owner subscription ticket');
  const before=h.calls.length;assert.equal(await button.click(),false);assert.equal(h.calls.length,before);
});
for(const name of ['projectionReady','currentSession','selectLayer','repaint'])test('actual row and eye events fail closed if injected '+name+' disappears',async()=>{
  const h=desktop();let installed;
  h.root.NemoNativeObjectUI={...UI,render(root,info,frameOnly,ports){installed={...ports};return UI.render(root,info,frameOnly,installed);}};
  await h.open();const row=pick(h.root,1),button=eye(h.root,0),before=h.calls.length;
  installed[name]=null;assert.equal(row.click(),false);assert.equal(await button.click(),false);
  assert.equal(h.root.state.activeLayerIdx,0);assert.equal(h.calls.length,before);assert.equal(h.meta().layers[0].visible,true);
});
for(const bad of [undefined,{}, {projectionReady:()=>true}])test('missing or incomplete explicit ports cannot publish or acknowledge a real revision '+Object.keys(bad||{}).join(','),async()=>{
  const h=desktop();await h.open();
  h.root.NemoNativeObjectUI={...UI,render:(root,info,frameOnly)=>UI.render(root,info,frameOnly,bad)};
  await h.external([A,B,C]);assert.equal(h.log.includes('ACK'),false);assert.equal(h.session.isIndeterminate(),true);
  assert.equal(h.root.document.nodes.get('start-screen').classList.contains('hid'),false);
  assert.throws(()=>h.root.updateUI(true),/Native/);
});


test('ordinary native render refreshes stale boot metadata and actual work-area label without enabling writers',()=>{
  const root=view(),info={...metadata(),fps:24,totalFrames:2};
  Files.projectView(root,info);root.state.currentFrame=0;
  for(const [id,value] of Object.entries({'tl-fps':12,'tl-total':24,'proj-fps':12,'proj-frames':24,'p-cw':1920,'p-ch':1080})){
    const el=element();el.value=value;el.disabled=true;
    el.addEventListener('change',()=>{throw Error('legacy writer dispatched');});root.document.nodes.set(id,el);
  }
  const bar=element(),label=element();label.className='wa-dur';label.textContent='24 f · 2.00 s';bar.appendChild(label);
  root.document.nodes.set('wa-bar',bar);
  const uiSource=fs.readFileSync('src/js/ui.js','utf8'),start=uiSource.indexOf('window.updateWaBar=function(){'),end=uiSource.indexOf('\n  };',start);
  assert.ok(start>=0&&end>start);
  vm.runInNewContext(uiSource.slice(start,end+5),{window:root,state:root.state,document:root.document,FC:40});
  poisonFrames(root);
  UI.render(root,info,false,fixturePorts(root));
  for(const [id,value] of Object.entries({'tl-fps':24,'tl-total':2,'proj-fps':24,'proj-frames':2,'p-cw':320,'p-ch':180})){
    assert.equal(root.document.nodes.get(id).value,value,id);assert.equal(root.document.nodes.get(id).disabled,true);
  }
  assert.equal(label.textContent,'2 f · 0.08 s');assert.equal(bar.style.width,'80px');
  assert.equal(root.document.nodes.get('tl-tf').textContent,2);
  assert.equal(root.document.nodes.get('tl-cf').max,2);
  assert.equal(root.document.nodes.get('frame-hdr').children[0].textContent,'Frame 1 / 2');
  assert.equal(root.state.fps,24);assert.equal(root.state.totalFrames,2);
});

test('native optional Drawing fields retain exact values and absent old metadata stays absent',async()=>{
  for(const color of ['#B49BE6','#aBc123','#000000']){
    const seed={...A,locked:false,color,parentLayerUid:null},h=desktop([seed,B]);await h.open();
    const meta=h.session.info(),row=h.root.state.layers[0];
    for(const k of ['locked','color','parentLayerUid'])assert.equal(meta.layers[0][k],seed[k]);
    assert.equal(Object.isFrozen(meta.layers[0]),true);assert.equal(row.color,color);assert.equal(row.locked,false);assert.equal(row.parentLayerUid,null);
    for(const k of ['locked','color','parentLayerUid'])assert.equal(Object.hasOwn(meta.layers[1],k),false);
    assert.equal(h.root.state.layers[1].color,'#5FA875');assert.equal(Files.projectionReadyForUI(h.root,meta),true);
    seed.color='#ffffff';assert.equal(meta.layers[0].color,color,'session metadata is detached');
    h.root.SM.setActiveLayer(1);const selection=h.root._layerSel,frame=h.root.state.currentFrame;
    const next={...A,locked:false,color:'#EE7C58',parentLayerUid:null};await h.external([next,B]);
    assert.equal(h.root.state.layers[0],row);assert.equal(row.color,'#EE7C58');assert.equal(h.session.info().layers[0].color,'#EE7C58');
    assert.equal(h.root.state.activeLayerIdx,1);assert.equal(h.root._layerSel,selection);assert.equal(h.root.state.currentFrame,frame);
    assert.equal(Files.projectionReadyForUI(h.root,h.session.info()),true);assert.ok(h.log.includes('ACK'));
    await h.session.historyFromUi('undo');assert.equal(row.color,color);await h.session.historyFromUi('redo');assert.equal(row.color,'#EE7C58');
  }
});
test('Drawing field projection refresh preserves rows and rejects invalid tables before any publication',()=>{
  const invalid=[{locked:null},{locked:0},{shy:null},{shy:0},{color:null},{color:123},{color:'#fff'},{color:'#GG0000'},
    {color:'123456'},{parentLayerUid:''},{parentLayerUid:false},{parentLayerUid:0},{parentLayerUid:[]},{parentLayerUid:{}},
    {unsupportedDrawingField:true}];
  for(const fields of invalid){
    const root=view(),before=JSON.stringify(root.state),rows=root.state.layers,selection=root._layerSel;
    const info=metadata([{...A,color:'#123456'},{...B,...fields}],1);
    assert.throws(()=>Files.refreshLayerTable(root,info),/Native layer/);assert.equal(JSON.stringify(root.state),before);
    assert.equal(root.state.layers,rows);assert.equal(root._layerSel,selection);assert.equal(Files.projectionReadyForUI(root,info),false);
    if(Object.hasOwn(fields,'parentLayerUid'))assert.equal(Files.refreshVisibility(root,info),false);
    else assert.throws(()=>Files.refreshVisibility(root,info),/Native layer/);
    assert.equal(JSON.stringify(root.state),before);
    assert.throws(()=>Files.projectView(root,info),/Native layer/);assert.equal(JSON.stringify(root.state),before);
  }
  const root=view(),row=root.state.layers[0],next=metadata([{...A,locked:false,color:'#b49Be6',parentLayerUid:null},B],1);
  assert.equal(Files.refreshLayerTable(root,next),true);assert.equal(row.color,'#b49Be6');assert.equal(Files.projectionReadyForUI(root,next),true);
  row.color='#000000';assert.equal(Files.projectionReadyForUI(root,next),false);assert.equal(Files.refreshVisibility(root,next),true);
  assert.equal(row.color,'#b49Be6');assert.equal(Files.projectionReadyForUI(root,next),true);
  const old=metadata([A,B],2);assert.equal(Files.refreshLayerTable(root,old),true);assert.equal(row.color,'#b49Be6');
  assert.equal(Files.refreshVisibility(root,old),true);assert.equal(row.color,'#b49Be6');assert.equal(Files.projectionReadyForUI(root,old),true);
  const absent=metadata([A,B,C],3);assert.equal(Files.refreshLayerTable(root,absent),true);
  for(const key of ['color','parentLayerUid'])assert.equal(Object.hasOwn(root.state.layers[2],key),false);
  for(const flag of ['locked','shy']){assert.equal(root.state.layers[2][flag],false);assert.equal(Object.hasOwn(absent.layers[2],flag),false);}
  assert.equal(Files.projectionReadyForUI(root,absent),true);
  const added={...C,locked:false,color:'#7EA0FA',parentLayerUid:null};assert.equal(Files.refreshLayerTable(root,metadata([A,B,added],4)),true);
  assert.equal(root.state.layers[2].color,'#7EA0FA');assert.equal(root.state.layers[2].locked,false);assert.equal(root.state.layers[2].parentLayerUid,null);
});
test('unsupported Drawing fields are rejected by the session before detached metadata or UI publication',async()=>{
  for(const fields of [{locked:null},{locked:'false'},{shy:null},{shy:'false'},{color:null},{color:'#123'},{color:'#12345G'},
    {parentLayerUid:''},{parentLayerUid:false},{parentLayerUid:0},{parentLayerUid:[]},{parentLayerUid:{}}]){
    const h=desktop([{...A,...fields},B]),before=JSON.stringify(h.root.state),rows=h.root.state.layers;
    assert.equal(await h.file.openPath('/native.json'),false);assert.equal(h.session.info(),null);
    assert.equal(JSON.stringify(h.root.state),before);assert.equal(h.root.state.layers,rows);assert.equal(h.log.includes('ACK'),false);
  }
});

test('Drawing field refresh preserves final-presentation selection fence while ACK is pending',async()=>{
  const h=desktop();await h.open();const root=h.root,row=root.state.layers[0],selection=root._layerSel;
  let release;h.hold(new Promise(r=>{release=r;}));
  const next=[{...A,locked:false,color:'#Ee7c58',parentLayerUid:null},B],change=h.external(next);
  await new Promise(setImmediate);assert.equal(row.color,'#Ee7c58');assert.equal(h.log.includes('ACK'),false);
  pick(root,1).click();assert.equal(root.state.activeLayerIdx,0);assert.equal(root._layerSel,selection);
  assert.equal(h.session.readPresentedObjectSession(),null);release();await change;
  assert.ok(h.log.indexOf('preview')<h.log.indexOf('ACK'));assert.equal(Files.projectionReadyForUI(root,h.session.info()),true);
  pick(root,1).click();assert.equal(root.state.activeLayerIdx,1);assert.deepEqual(root._layerSel,[1]);
  const badPins=[{...h.meta(),contentRevision:0},{...h.meta(),documentSnapshotId:'wrong'},
    {...h.meta(),documentId:'foreign'},{...h.meta(),lifecycleGeneration:99}];
  for(const pin of badPins){
    const selected=root._layerSel,frame=root.state.currentFrame;assert.equal(Files.refreshLayerTable(root,pin),false);
    assert.equal(root.state.layers[0],row);assert.equal(row.color,'#Ee7c58');assert.equal(root._layerSel,selected);assert.equal(root.state.currentFrame,frame);
  }
});

test('admitted native lock and shy booleans preserve authored presence in session and readonly UI projection',async()=>{
  for(const flags of [{locked:true},{shy:true},{locked:true,shy:false}]){
    const h=desktop([{...A,...flags},B]);await h.open();const meta=h.session.info(),row=h.root.state.layers[0];
    for(const flag of ['locked','shy']){assert.equal(Object.hasOwn(meta.layers[0],flag),Object.hasOwn(flags,flag));if(Object.hasOwn(flags,flag))assert.equal(meta.layers[0][flag],flags[flag]);assert.equal(row[flag],flags[flag]===true);}
    assert.equal(Object.isFrozen(meta.layers[0]),true);assert.equal(Files.projectionReadyForUI(h.root,meta),true);
  }
});

for(const modifier of ['metaKey','ctrlKey'])test('ordinary native row '+modifier+' toggles selected layers without a document/history write',async()=>{
  const h=desktop();await h.open();const root=h.root,before=h.meta();
  const writes=()=>h.calls.filter(c=>c.command==='nemo_native_object_dispatch'||c.command==='nemo_native_dispatch'&&!c.request.operation.startsWith('query.')).length;
  const count=writes();pick(root,1).dispatch('click',{[modifier]:true,shiftKey:true});
  assert.deepEqual(root._layerSel,[0,1]);assert.equal(root.state.activeLayerIdx,1);assert.equal(root._layerSelAnchor,1);
  assert.equal(pick(root,0).attributes['aria-pressed'],'true');assert.equal(pick(root,1).attributes['aria-pressed'],'true');
  pick(root,0).dispatch('click',{[modifier]:true});assert.deepEqual(root._layerSel,[1]);
  assert.equal(root.state.activeLayerIdx,0);assert.equal(pick(root,0).attributes['aria-pressed'],'false');
  pick(root,0).click();assert.deepEqual(root._layerSel,[0]);assert.equal(root._layerSelAnchor,0);
  assert.equal(writes(),count);assert.deepEqual(h.meta(),before);poisonFrames(root);
});
test('ordinary native Shift row clicks retain the range anchor and preserve boolean programmatic selection',async()=>{
  const h=desktop();await h.open();await h.external([A,B,C]);const root=h.root;
  root._layerSelAnchor=-1;root._layerSel=[];root.state.activeLayerIdx=0;
  pick(root,2).dispatch('click',{shiftKey:true});assert.deepEqual(root._layerSel,[0,1,2]);assert.equal(root._layerSelAnchor,-1);
  pick(root,1).dispatch('click',{shiftKey:true});assert.deepEqual(root._layerSel,[0,1]);assert.equal(root._layerSelAnchor,-1);
  pick(root,2).click();pick(root,0).dispatch('click',{shiftKey:true});assert.deepEqual(root._layerSel,[0,1,2]);assert.equal(root._layerSelAnchor,2);
  root._layerSelAnchor=-1;root._layerSel=[];root.state.activeLayerIdx=2;
  pick(root,0).dispatch('click',{shiftKey:true});assert.deepEqual(root._layerSel,[0,1,2]);
  pick(root,1).dispatch('click',{shiftKey:true});assert.deepEqual(root._layerSel,[0,1]);assert.equal(root._layerSelAnchor,-1);
  const selection=root._layerSel;root.SM.setActiveLayer(1,true);assert.equal(root._layerSel,selection);
  root.SM.setActiveLayer(1);assert.deepEqual(root._layerSel,[1]);assert.equal(root._layerSelAnchor,1);
});
test('native row modifiers reject retired controls and consume post-drag clicks without changing selection',async()=>{
  const h=desktop();await h.open();const root=h.root,retired=pick(root,0);
  pick(root,1).dispatch('click',{ctrlKey:true});const selection=root._layerSel;
  assert.equal(retired.dispatch('click',{shiftKey:true}),false);assert.equal(root._layerSel,selection);
  root._layerDragJustEnded=true;assert.equal(pick(root,0).dispatch('click',{metaKey:true}),false);
  assert.equal(root._layerDragJustEnded,false);assert.equal(root._layerSel,selection);
  pick(root,0).dispatch('click',{metaKey:true});assert.deepEqual(root._layerSel,[1]);
});
for(const fault of ['refused','foreign'])test('native modified row refuses '+fault+' presentation before changing selection',async()=>{
  const h=desktop();await h.open();const root=h.root,selection=root._layerSel,active=root.state.activeLayerIdx,anchor=root._layerSelAnchor;
  if(fault==='refused')h.fault('preview');
  else h.root.NemoNativeObjectUI={...UI,select:(r,i,index,preserve,uid,presentation,ports)=>UI.select(r,i,index,preserve,uid,{...presentation,documentId:'foreign'},ports)};
  if(fault==='refused'){await assert.rejects(h.session.presentPreview(7));assert.equal(h.session.readPresentedObjectSession(),null);}
  pick(root,1).dispatch('click',{ctrlKey:true});assert.equal(root._layerSel,selection);assert.equal(root.state.activeLayerIdx,active);assert.equal(root._layerSelAnchor,anchor);
});
test('native modified row rolls back every selection field when presentation repaint throws',async()=>{
  const h=desktop();await h.open();const root=h.root;
  root.selectedPaths=['sentinel'];root.state.selectedStrokeIndices=[3];root._layerActiveExplicit=false;
  const old={active:root.state.activeLayerIdx,selection:root._layerSel,anchor:root._layerSelAnchor,paths:root.selectedPaths,indices:root.state.selectedStrokeIndices,explicit:root._layerActiveExplicit};
  const create=root.document.createElement,failure=Error('fixture render refusal');root.document.createElement=()=>{throw failure;};
  try{assert.throws(()=>pick(root,1).dispatch('click',{metaKey:true}),error=>error===failure);}finally{root.document.createElement=create;}
  assert.equal(root.state.activeLayerIdx,old.active);assert.equal(root._layerSel,old.selection);assert.equal(root._layerSelAnchor,old.anchor);
  assert.equal(root.selectedPaths,old.paths);assert.equal(root.state.selectedStrokeIndices,old.indices);assert.equal(root._layerActiveExplicit,old.explicit);
});

test('native modified row refuses an actual in-flight external revision until final presentation',async()=>{
  const h=desktop();await h.open();const root=h.root;let release;h.hold(new Promise(resolve=>{release=resolve;}));
  const change=h.external([A,B,C]);await new Promise(setImmediate);
  const selection=root._layerSel,active=root.state.activeLayerIdx,anchor=root._layerSelAnchor;
  try{pick(root,2).dispatch('click',{shiftKey:true});assert.equal(root._layerSel,selection);assert.equal(root.state.activeLayerIdx,active);assert.equal(root._layerSelAnchor,anchor);}
  finally{release();await change;}
  pick(root,2).dispatch('click',{shiftKey:true});assert.deepEqual(root._layerSel,[0,1,2]);
});

// Parent metadata is an admitted native read-side field. Graph validity (unknown/self/cycle)
// stays in the real native owner; these literals use an existing, distinct parent UID.
test('Drawing projection admits an existing opaque parent and retains row selection and revision fences',()=>{
  const parent=A.layerUid,root=view(),rows=root.state.layers,child=rows[1],selection=root._layerSel,frame=root.state.currentFrame;
  const next=metadata([A,{...B,parentLayerUid:parent}],1);
  assert.equal(Files.refreshLayerTable(root,next),true);
  assert.equal(root.state.layers,rows);assert.equal(root.state.layers[1],child);
  assert.equal(child.parentLayerUid,parent);assert.equal(root._layerSel,selection);assert.equal(root.state.currentFrame,frame);
  assert.equal(Files.projectionReadyForUI(root,next),true);
  const prior=JSON.stringify(root.state),drift={...next,layers:[A,{...B,parentLayerUid:null}]};
  assert.equal(Files.refreshLayerTable(root,drift),false);assert.equal(Files.refreshVisibility(root,drift),false);
  assert.equal(JSON.stringify(root.state),prior);assert.equal(Files.projectionReadyForUI(root,drift),false);
  assert.equal(Files.projectionReadyForUI(root,next),true);
  for(const layers of [[A,{...B,parentLayerUid:null}],[A,{...B,parentLayerUid:parent}],[A,B]]){
    const revision=next.contentRevision+1;next.contentRevision=revision;next.documentSnapshotId='document-snapshot-'+revision;
    const info={...next,layers};assert.equal(Files.refreshLayerTable(root,info),true);
    assert.equal(root.state.layers,rows);assert.equal(root.state.layers[1],child);
    assert.equal(child.parentLayerUid,Object.hasOwn(layers[1],'parentLayerUid')?layers[1].parentLayerUid:null);
    assert.equal(root._layerSel,selection);assert.equal(root.state.currentFrame,frame);assert.equal(Files.projectionReadyForUI(root,info),true);
  }
});
test('Drawing session publishes frozen admitted parent metadata only through the confirmed native presentation',async()=>{
  const parent=A.layerUid,h=desktop([A,{...B,parentLayerUid:parent}]);await h.open();
  const root=h.root,meta=h.session.info(),rows=root.state.layers,child=rows[1],selection=root._layerSel,frame=root.state.currentFrame;
  assert.equal(meta.layers[1].parentLayerUid,parent);assert.equal(Object.isFrozen(meta.layers[1]),true);
  assert.equal(child.parentLayerUid,parent);assert.equal(Files.projectionReadyForUI(root,meta),true);
  let release;h.hold(new Promise(resolve=>{release=resolve;}));
  const change=h.external([A,{...B,parentLayerUid:null}]);await new Promise(setImmediate);
  assert.equal(h.log.includes('ACK'),false);assert.equal(h.session.readPresentedObjectSession(),null);
  pick(root,1).click();assert.equal(root._layerSel,selection);assert.equal(root.state.activeLayerIdx,0);
  release();await change;
  const next=h.session.info();assert.equal(next.contentRevision,meta.contentRevision+1);assert.equal(next.layers[1].parentLayerUid,null);
  assert.equal(Object.isFrozen(next.layers[1]),true);assert.equal(meta.layers[1].parentLayerUid,parent,'old detached metadata remains exact');
  assert.equal(root.state.layers,rows);assert.equal(root.state.layers[1],child);assert.equal(child.parentLayerUid,null);
  assert.equal(root._layerSel,selection);assert.equal(root.state.currentFrame,frame);assert.equal(Files.projectionReadyForUI(root,next),true);
  assert.ok(h.log.indexOf('preview')<h.log.indexOf('ACK'));
  assert.equal(h.calls.some(call=>call.request&&call.request.operation&&call.request.operation.startsWith('command.')),false);
});

test('metadata presenter invokes native bar mount without adding persistent range fields to projected layers',()=>{
 const root=view(),info=metadata();let mounted=0,retired=0;const before=JSON.stringify(root.state.layers);
 root.NemoNativeLayerRangeDesktop={mount(r,grid,value,ports){assert.strictEqual(r,root);assert.strictEqual(grid,root.document.nodes.get('frame-grid'));assert.strictEqual(value,info);assert.ok(ports);mounted++;},retire(r){assert.strictEqual(r,root);retired++;},owns:()=>false};
 assert.equal(UI.render(root,info,false,fixturePorts(root)),true);assert.equal(mounted,1);assert.equal(JSON.stringify(root.state.layers),before);
 UI.restoreInspector(root);assert.equal(retired,1);
});
