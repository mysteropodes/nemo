'use strict';
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),vm=require('node:vm');
const UI=require('../src/js/adapters/native-object-ui.js'),Files=require('../src/js/adapters/native-object-project-file.js');
const Project=require('../src/js/adapters/native-object-project.js'),Transport=require('../src/js/adapters/application-mcp.js');
const Workflow=require('../src/js/application/native-object-save-as-workflow.js'),Ack=require('../src/js/application/native-object-saved-acknowledgement.js');
const A={layerUid:'opaque A:雪',name:'<b>A</b>',visible:true},B={layerUid:'B',name:'B',visible:false},C={layerUid:'C',name:'',visible:true};
const clone=rows=>rows.map(x=>({...x}));
function metadata(layers=[A,B],revision=0,doc='document',generation=1){return {apiVersion:2,instanceId:'instance',documentId:doc,lifecycleGeneration:generation,
  contentRevision:revision,documentSnapshotId:doc+'-snapshot-'+revision,documentDigest:String(revision+1).repeat(64),byteCount:1000,
  canvasW:320,canvasH:180,fps:24,totalFrames:24,reference:null,undoDepth:0,redoDepth:0,layers:clone(layers)};}
function element(){
  const listeners={},classes=new Set();
  return {children:[],style:{},dataset:{},attributes:{},value:'',max:0,textContent:'',className:'',type:'',
    classList:{add:x=>classes.add(x),remove:x=>classes.delete(x),contains:x=>classes.has(x)},
    setAttribute(k,v){this.attributes[k]=v;},addEventListener(k,f){listeners[k]=f;},
    replaceChildren(...children){this.children=children;},click(){return listeners.click?.();},
    getBoundingClientRect:()=>({left:0,top:0,width:320,height:180})};
}
function document(){const nodes=new Map();for(const id of ['layer-list','frame-hdr','frame-grid','tl-cf','tl-tf','info-frame','info-badge','info-strokes','info-sel','drawing-canvas','start-screen'])nodes.set(id,element());
  nodes.get('start-screen').classList.add('hid');return {nodes,activeElement:null,getElementById:id=>nodes.get(id)||null,createElement:()=>element()};}
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
function presented(info,frame=7){return {...info,available:true,kind:'native-object',presentation:{...info,owner:'native',status:'presented',frame,workId:'work',viewGeneration:1}};}
function poisonFrames(root){root.state.layers.forEach(row=>Object.defineProperty(row,'frames',{configurable:true,get(){throw Error('LEGACY FRAME READER');}}));}
test('metadata presenter builds real text/button rows and explicit unavailable timing without reading frames',()=>{
  const root=view();poisonFrames(root);assert.equal(UI.render(root,metadata()),true);
  const rows=root.document.nodes.get('layer-list').children;
  assert.equal(rows.length,2);assert.equal(rows[0].textContent,'<b>A</b> — Visible');assert.equal(rows[0].dataset.layerUid,A.layerUid);
  assert.equal(rows[1].textContent,'B — Hidden');assert.equal(rows[0].type,'button');
  assert.equal(root.document.nodes.get('frame-hdr').children[0].textContent,'Frame 8 / 24');
  assert.match(root.document.nodes.get('frame-grid').children[0].textContent,/timing is unavailable/);
  assert.equal(root.document.nodes.get('info-badge').style.display,'none');
});
test('selection needs exact outer AND final-presented identity, never constructs frame data',()=>{
  const root=view();assert.equal(Files.refreshLayerTable(root,metadata([A,B,C],1)),true);
  const next=metadata([A,B,C],1);poisonFrames(root);
  for(const pin of [null,presented(next,6),presented({...next,documentId:'foreign'}),{...presented(next),presentation:{...presented(next).presentation,documentSnapshotId:'foreign'}}]){
    assert.equal(UI.select(root,next,2,false,'C',pin),false);assert.equal(root.state.activeLayerIdx,0);
  }
  assert.equal(UI.select(root,next,2,false,'C',presented(next)),true);
  assert.equal(root.state.activeLayerIdx,2);assert.deepEqual(root._layerSel,[2]);assert.equal(root.state.currentFrame,7);
  assert.equal(root.document.nodes.get('layer-list').children[2].attributes['aria-pressed'],'true');
  assert.equal(UI.select(root,next,2,false,'replaced',presented(next)),false);
});
test('stale projection and absent UI nodes refuse; failed selection restores editor state',()=>{
  const root=view(),selection=root._layerSel,paths=root.selectedPaths;
  assert.throws(()=>UI.render(root,{...metadata(),documentSnapshotId:'foreign'}),/uncorrelated/);
  root.document.nodes.delete('frame-grid');
  assert.throws(()=>UI.select(root,metadata(),1,false,'B',presented(metadata())),/unavailable/);
  assert.equal(root.state.activeLayerIdx,0);assert.equal(root._layerSel,selection);assert.equal(root.selectedPaths,paths);
});
function desktop(){
  let active=false,revision=0,layers=clone([A,B]),doc='document',generation=1,gate=null,pending=null,fault=null;
  const log=[],listeners=new Map(),root=shippedEntrypoints(view());
  root.NemoNativeObjectProject=Project;root.NemoApplicationMcpTransport=Transport;
  const meta=()=>metadata(layers,revision,doc,generation);
  const binding=()=>({apiVersion:2,instanceId:'instance',documentId:doc,lifecycleGeneration:generation,subscriptionId:'subscription-'+generation,contentRevision:revision});
  async function invoke(command,args){const r=args&&(args.request||JSON.parse(args.requestJson));
    if(command==='nemo_native_status'||command==='nemo_native_object_client_status')return {...meta(),available:active};
    if(command==='nemo_native_object_bootstrap'){
      if(active){generation++;doc='replacement-'+generation;}active=true;revision=0;layers=clone([A,B]);
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
    assert.equal(command,'nemo_native_object_dispatch');layers=clone(r.operation.endsWith('.undo')?[A,B]:[A,B,C]);revision++;
    return {apiVersion:2,requestId:r.requestId,instanceId:r.instanceId,documentId:r.documentId,contentRevision:revision,ok:true,result:{applied:true,historyEntriesAdded:1}};
  }
  root.__TAURI__={core:{invoke},event:{listen:async(name,callback)=>{listeners.set(name,callback);return ()=>listeners.delete(name);}},dialog:{ask:async()=>true}};
  const session=Project.desktop(root);
  const render=UI.render;root.NemoNativeObjectUI={...UI,render(r,i){log.push('UI:'+i.contentRevision);return render(r,i);}};
  root.SMProjectDocument={baseName:()=> 'Project'};root.NemoNativeObjectSaveAsWorkflow=Workflow;root.NemoNativeObjectSavedAcknowledgement=Ack;
  root.requestAnimationFrame=f=>setImmediate(f);
  const file=Files.bind(root,{read:()=>({tabId:'tab',name:'Project'}),touchRecent(){},commit(){},renderRecents(){},
    opened(){log.push('opened');},notice(message){log.push('notice:'+message);},show(){root.document.nodes.get('start-screen').classList.remove('hid');},
    hide(){root.document.nodes.get('start-screen').classList.add('hid');},repaint(){}});
  return {root,session,file,log,meta,hold(p){gate=p;},fault(v){fault=v;},
    async open(){assert.equal(await file.openPath('/native.json'),true,log.join('|'));log.length=0;poisonFrames(root);},
    async external(next){const from=revision;layers=clone(next);revision++;pending={instanceId:'instance',documentId:doc,lifecycleGeneration:generation,
      requestId:'external-'+revision,fromRevision:from,toRevision:revision};return listeners.get('nemo-native-revision')({payload:pending});}};
}
test('actual Open/New and four shipped UI entrypoints avoid legacy frame readers; row click selects through a final receipt',async()=>{
  const h=desktop();await h.open();
  const retired=h.root.document.nodes.get('layer-list').children[0];
  for(const name of ['updateUI','renderTimeline','renderLayerList'])assert.doesNotThrow(()=>h.root[name]());
  h.root.document.nodes.get('layer-list').children[1].click();assert.equal(h.root.state.activeLayerIdx,1);
  assert.doesNotThrow(()=>h.root.SM.setActiveLayer(0));assert.equal(h.root.state.activeLayerIdx,0);
  h.log.length=0;
  assert.equal(await h.file.newProject({w:320,h:180,fps:24,name:'New'}),true);
  assert.equal(h.session.info().documentId,'replacement-2');assert.match(h.root.document.nodes.get('frame-hdr').children[0].textContent,/Frame 1/);
  assert.ok(h.log.indexOf('preview')<h.log.indexOf('UI:0'),'actual reveal follows the first presented preview');
  h.root.SM.setActiveLayer(1);retired.click();assert.equal(h.root.state.activeLayerIdx,1,'retired row cannot select a replacement document with reused UID');
});
test('real subscriber paints added metadata before preview/ACK; pending preview cannot select the new row',async()=>{
  const h=desktop();await h.open();let done;h.hold(new Promise(r=>{done=r;}));
  const change=h.external([A,B,C]);await new Promise(setImmediate);
  assert.equal(h.root.document.nodes.get('layer-list').children.length,3);assert.equal(h.log.includes('ACK'),false);
  h.root.document.nodes.get('layer-list').children[2].click();assert.equal(h.root.state.activeLayerIdx,0);
  done();await change;
  assert.ok(h.log.indexOf('UI:1')<h.log.indexOf('preview'));assert.ok(h.log.indexOf('preview')<h.log.indexOf('ACK'));
  poisonFrames(h.root);h.root.document.nodes.get('layer-list').children[2].click();assert.equal(h.root.state.activeLayerIdx,2);
});
test('real local history updates selected-row metadata and actual native UI before preview',async()=>{
  const h=desktop();await h.open();await h.session.historyFromUi('redo');
  assert.equal(h.root.document.nodes.get('layer-list').children.length,3);
  assert.ok(h.log.indexOf('UI:1')<h.log.indexOf('preview'));assert.equal(h.log.includes('ACK'),false);
  h.root.SM.setActiveLayer(2);await h.session.historyFromUi('undo');assert.equal(h.root.state.activeLayerIdx,1);
  assert.equal(h.root.document.nodes.get('layer-list').children.length,2);
});
for(const fault of ['missing','refused','stale','preview'])test('native '+fault+' failure never falls back or acknowledges an external revision',async()=>{
  const h=desktop();await h.open();
  if(fault==='missing')delete h.root.NemoNativeObjectUI;
  if(fault==='refused')h.root.NemoNativeObjectUI={...UI,render:()=>false};
  if(fault==='stale')h.root.NemoNativeObjectUI={...UI,render:(root,info)=>UI.render(root,{...info,documentId:'foreign'})};
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
