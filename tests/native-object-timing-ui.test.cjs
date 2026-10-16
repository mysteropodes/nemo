'use strict';
const test=require('node:test'),assert=require('node:assert/strict');
const Project=require('../src/js/adapters/native-object-project.js');
const UI=require('../src/js/adapters/native-object-ui.js');
const Files=require('../src/js/adapters/native-object-project-file.js');
const SDK=require('../src/js/adapters/native-application.js');
const Transport=require('../src/js/adapters/application-mcp.js');
const turn=()=>new Promise(setImmediate);
function deferred(){let resolve;const promise=new Promise(r=>{resolve=r;});return {promise,resolve};}
// Independent native-result oracle, including a blank authored key and a sample.
const rows=new Map([
  [0,{cellKind:'key',sourceFrame:0,hasContent:true,manualEdit:false}],
  [6,{cellKind:'interpolated',sourceFrame:6,hasContent:true,manualEdit:true}],
  [7,{cellKind:'held',sourceFrame:6,hasContent:true,manualEdit:false}],
  [12,{cellKind:'key',sourceFrame:12,hasContent:false,manualEdit:false}],
  [20,{cellKind:'unexposed',sourceFrame:null,hasContent:false,manualEdit:false}],
]);
function element(){return {children:[],style:{},dataset:{},attributes:{},textContent:'',value:'',
  setAttribute(k,v){this.attributes[k]=v;},addEventListener(name,fn){this[name]=fn;},
  appendChild(node){this.children.push(node);node.parentNode=this;return node;},
  replaceChildren(...nodes){for(const n of this.children)n.parentNode=null;this.children=[];nodes.forEach(n=>this.appendChild(n));},
  classList:{add(){},remove(){}},getBoundingClientRect(){return {left:0,top:0,width:64,height:48};}};}
function harness(){
  let active=false,revision=0,generation=1,doc='document',hold=null,fault=null,ack=null;
  const calls=[],listeners=new Map(),nodes=new Map();
  for(const id of ['layer-list','frame-hdr','frame-grid','tl-cf','tl-tf','info-frame','info-badge','info-strokes','info-sel','drawing-canvas','start-screen'])nodes.set(id,element());
  const root={state:{},userLayers:[],document:{activeElement:null,getElementById:id=>nodes.get(id),createElement:element},
    NemoNativeObjectProject:Project,NemoNativeObjectUI:UI,NemoNativeObjectProjectFile:Files,
    NemoNativeApplicationAdapter:SDK,NemoApplicationMcpTransport:Transport};
  const meta=()=>({apiVersion:2,instanceId:'instance',documentId:doc,lifecycleGeneration:generation,contentRevision:revision,
    documentSnapshotId:'snapshot-'+generation+'-'+revision,documentDigest:'a'.repeat(64),byteCount:2000,
    canvasW:64,canvasH:48,fps:24,totalFrames:21,undoDepth:0,redoDepth:0,
    layers:[{layerUid:'drawing:雪',name:'Drawing',visible:true},{layerUid:'missing',name:'Reference-only drawing',visible:true}],
    reference:{id:'reference',name:'Reference.png',visible:true,opacity:1,offsetFrames:0}});
  const binding=()=>({...meta(),subscriptionId:'subscription-'+generation});
  async function invoke(command,args){
    const r=args&&(args.request||JSON.parse(args.requestJson));calls.push({command,r});
    if(['nemo_native_status','nemo_native_object_client_status'].includes(command))return {...meta(),available:active};
    if(command==='nemo_native_object_bootstrap'){
      if(active){generation++;doc='replacement-'+generation;}active=true;revision=0;
      return {...meta(),requestId:r.requestId,resourceCount:1,viewportAvailable:false};
    }
    if(command==='nemo_native_object_reference')return {...meta(),requestId:r.requestId};
    if(command==='nemo_native_object_preview')return {status:'presented',workId:'work-'+generation+'-'+revision,viewGeneration:generation};
    if(command==='nemo_native_revision_sync'){
      if(r.action==='binding'){
        const b=binding();return {apiVersion:2,instanceId:b.instanceId,documentId:b.documentId,lifecycleGeneration:b.lifecycleGeneration,
          contentRevision:b.contentRevision,subscriptionId:b.subscriptionId};
      }
      if(r.action==='subscribe')return r.binding;
      if(r.action==='acknowledge'&&ack){const wait=ack;ack=null;await wait;}
      return null;
    }
    assert.equal(command,'nemo_native_dispatch');assert.equal(r.operation,'query.document.object.layer.timing');
    assert.deepEqual(Object.keys(r.payload).sort(),['atRevision','contextId','documentSnapshotId','frame','layerUid']);
    const response={apiVersion:2,requestId:r.requestId,instanceId:r.instanceId,documentId:r.documentId,contentRevision:r.payload.atRevision,ok:true,
      result:{instanceId:r.instanceId,documentId:r.documentId,...r.payload,...rows.get(r.payload.frame)}};
    if(r.payload.layerUid==='missing'){
      response.ok=false;delete response.result;response.error={code:'unavailable',message:'No authored timing'};
    }
    const broken=fault;fault=null;if(broken)broken(response);
    if(hold){const wait=hold;hold=null;await wait;}
    return response;
  }
  root.__TAURI__={core:{invoke},event:{listen:async(name,fn)=>{listeners.set(name,fn);return ()=>listeners.delete(name);}}};
  require('./helpers/native-viewport-underlay.cjs').install(root);
  const session=Project.desktop(root);root.updateUI=()=>Project.paintUI(root,true);
  function poison(){root.state.layers.forEach(layer=>Object.defineProperty(layer,'frames',{configurable:true,get(){throw Error('legacy frame read');}}));}
  const open=async()=>{await session.openPath('/timed.json',{commit(info){Files.projectView(root,info);}});poison();await turn();await turn();};
  return {root,session,calls,nodes,meta,open,hold(p){hold=p;},fault(fn){fault=fn;},holdAck(p){ack=p;},
    queries:()=>calls.filter(c=>c.command==='nemo_native_dispatch'),text:()=>nodes.get('frame-grid').children[0].textContent,
    async frame(frame){root.state.currentFrame=frame;Project.paintUI(root,true);await session.presentPreview(frame);Project.paintUI(root,true);await turn();await turn();},
    async replace(){await session.openPath('/replacement.json',{commit(info){Files.projectView(root,info);}});poison();await turn();},
    async external(){const from=revision;revision++;return listeners.get('nemo-native-revision')({payload:{instanceId:'instance',documentId:doc,
      lifecycleGeneration:generation,requestId:'external-'+revision,fromRevision:from,toRevision:revision}});}};
}
test('ordinary Project/SDK/Tauri/UI reads exact fixed native timing and preserves frame DOM/focus',async()=>{
  const h=harness();await h.open();const row=h.nodes.get('layer-list').children[0],cell=h.nodes.get('frame-grid').children[0];
  const field=h.nodes.get('tl-cf');h.root.document.activeElement=field;field.value='editing';
  const expected=['Key — Source frame 1 — Content','Sample — Source frame 7 — Content — Manual edit',
    'Held — Source frame 7 — Content','Key — Source frame 13 — Blank','Unexposed — No source frame — Blank'];
  for(const [index,frame] of [0,6,7,12,20].entries()){
    await h.frame(frame);assert.equal(h.text(),expected[index]);assert.equal(h.nodes.get('layer-list').children[0],row);
    assert.equal(h.nodes.get('frame-grid').children[0],cell);assert.equal(field.value,'editing');
  }
  assert.equal(h.meta().contentRevision,0);assert.equal(h.meta().undoDepth,0);
  assert.ok(h.queries().length>=5);assert.ok(h.calls.every(c=>!c.r||!c.r.operation||c.r.operation.startsWith('query.')));
  assert.equal(h.calls.filter(c=>c.r&&c.r.action==='acknowledge').length,0);
  assert.ok(h.calls.every(c=>!c.command.includes('save')&&!c.command.includes('export')));
});
test('one outstanding read coalesces rapid frame intent and never publishes an obsolete result',async()=>{
  const h=harness();await h.open();const gate=deferred();h.hold(gate.promise);await h.frame(6);
  const before=h.queries().length;
  for(const frame of [7,12,20])await h.frame(frame);
  assert.equal(h.queries().length,before);assert.match(h.text(),/unavailable/);
  gate.resolve();await turn();await turn();assert.equal(h.queries().length,before+1);
  assert.equal(h.queries().at(-1).r.payload.frame,20);assert.equal(h.text(),'Unexposed — No source frame — Blank');
});
test('selection change discards delayed timing and Reference records do not supply missing timing',async()=>{
  const h=harness();await h.open();const gate=deferred();h.hold(gate.promise);await h.frame(6);
  assert.equal(h.nodes.get('layer-list').children[1].children[0].click(),true);
  assert.equal(h.root.state.activeLayerIdx,1);
  gate.resolve();await turn();await turn();assert.equal(h.queries().at(-1).r.payload.layerUid,'missing');
  assert.equal(h.text(),'Native timing is unavailable.');assert.equal(h.root.state.currentFrame,6);
  assert.equal(h.meta().reference.name,'Reference.png');assert.equal(h.meta().contentRevision,0);
});
test('owner replacement retains one outstanding slot and rejects retired transport output',async()=>{
  const h=harness();await h.open();const gate=deferred();h.hold(gate.promise);await h.frame(6);
  const count=h.queries().length;await h.replace();assert.equal(h.queries().length,count);
  gate.resolve();await turn();await turn();assert.equal(h.queries().at(-1).r.documentId,'replacement-2');
  assert.equal(h.text(),'Key — Source frame 1 — Content');
});
test('revision publication fences old snapshot response; pending acknowledgement cannot publish timing',async()=>{
  const h=harness();await h.open();const read=deferred();h.hold(read.promise);await h.frame(6);
  const ack=deferred();h.holdAck(ack.promise);const revision=h.external();await turn();await turn();
  read.resolve();await turn();assert.match(h.text(),/unavailable/);ack.resolve();await revision;
  await new Promise(resolve=>setTimeout(resolve,10));assert.equal(h.queries().at(-1).r.payload.atRevision,1);
  assert.equal(h.text(),'Sample — Source frame 7 — Content — Manual edit');
  assert.equal(h.calls.filter(c=>c.r&&c.r.action==='acknowledge').length,1,'reader never acknowledges');
});
for(const [label,breakResponse] of [
  ['document',r=>{r.documentId='foreign';}],['revision',r=>{r.contentRevision++;}],
  ['snapshot',r=>{r.result.documentSnapshotId='other';}],['frame',r=>{r.result.frame=7;}],
  ['source',r=>{r.result.sourceFrame=0;}],['unknown field',r=>{r.result.extra=true;}],
])test('SDK refuses uncorrelated '+label+' without reviving UI or mutating history',async()=>{
  const h=harness();await h.open();h.fault(breakResponse);await h.frame(6);
  assert.equal(h.text(),'Native timing is unavailable.');assert.equal(h.meta().undoDepth,0);assert.equal(h.session.isIndeterminate(),false);
});
test('teardown invalidates pending publication and out-of-range reads are explicit unavailability',async()=>{
  const h=harness();await h.open();const gate=deferred();h.hold(gate.promise);await h.frame(6);
  const detached=h.nodes.get('frame-grid').children[0];UI.restoreInspector(h.root);gate.resolve();await turn();await turn();
  assert.match(detached.textContent,/unavailable/);const count=h.queries().length;
  assert.equal(await h.session.readLayerTiming('drawing:雪',21,h.session.info()),null);
  assert.equal(h.queries().length,count);assert.equal(h.session.isIndeterminate(),false);
});
test('current frame and selection pins fence a response even without another repaint',async()=>{
  for(const change of [h=>{h.root.state.currentFrame=7;},h=>{h.root.state.activeLayerIdx=1;}]){
    const h=harness();await h.open();const gate=deferred();h.hold(gate.promise);await h.frame(6);
    change(h);gate.resolve();await turn();await turn();assert.equal(h.text(),'Native timing is unavailable.');
  }
});
test('stale owner/revision/snapshot selectors refuse before dispatching a native timing read',async()=>{
  const h=harness();await h.open();const count=h.queries().length;
  for(const pin of [{...h.session.info(),documentId:'old'},{...h.session.info(),lifecycleGeneration:2},
    {...h.session.info(),contentRevision:1},{...h.session.info(),documentSnapshotId:'old'}])
    assert.equal(await h.session.readLayerTiming('drawing:雪',0,pin),null);
  assert.equal(h.queries().length,count);assert.equal(h.meta().contentRevision,0);
});
