'use strict';
const test=require('node:test'),assert=require('node:assert/strict');
const Files=require('../src/js/adapters/native-object-project-file.js');
const Metadata=require('../src/js/adapters/native-object-project-metadata.js');
const Project=require('../src/js/adapters/native-object-project.js');
const Transport=require('../src/js/adapters/application-mcp.js');
const fs=require('node:fs'),vm=require('node:vm');
const turn=()=>new Promise(setImmediate);
function deferred(){let resolve;const promise=new Promise(r=>{resolve=r;});return {promise,resolve};}
const A={layerUid:'opaque A:雪',name:'A',visible:true},B={layerUid:'B',name:'B',visible:false},C={layerUid:'C',name:'',visible:true};
const clone=rows=>rows.map(row=>({...row}));
function info(layers,revision=0){return {apiVersion:2,instanceId:'instance',documentId:'document',lifecycleGeneration:1,
  contentRevision:revision,documentSnapshotId:'snapshot-'+revision,documentDigest:'a'.repeat(64),byteCount:1000,
  canvasW:320,canvasH:180,fps:24,totalFrames:24,reference:null,undoDepth:0,redoDepth:0,layers:clone(layers)};}
function view(layers=[A,B]){
  const root={state:{},userLayers:[],selectedPaths:[]};Files.projectView(root,info(layers));
  root.state.currentFrame=7;root.state.waIn=3;root.state.waOut=19;root.state.activeLayerIdx=1;
  root._curFrame=7;root._waIn=3;root._waOut=19;root._layerSel=[0,1];root.selectedPaths=['surviving-selection'];
  root.state.refMedia={ui:'reference'};root.state.layers[0].uiOnly={expanded:true};
  return root;
}
function snapshot(root){return JSON.stringify({state:root.state,selection:root._layerSel,paths:root.selectedPaths,
  frame:root._curFrame,waIn:root._waIn,waOut:root._waOut});}
test('strict delivered matching remains false for append until the complete read-side table refresh',()=>{
  const root=view(),rows=root.state.layers,first=rows[0],second=rows[1],frames=first.frames,motion=first.motion;
  const reference=root.state.refMedia,paths=root.selectedPaths,selection=root._layerSel;
  assert.equal(Files.projectionMatches(root,info([A,B,C])),false);
  assert.equal(Files.refreshVisibility(root,info([A,B,C])),false);assert.equal(rows.length,2);
  assert.equal(Files.refreshLayerTable(root,info([A,B,C],1)),true);
  assert.equal(root.state.layers,rows);assert.equal(rows[0],first);assert.equal(rows[1],second);
  assert.equal(first.frames,frames);assert.equal(first.motion,motion);assert.deepEqual(first.uiOnly,{expanded:true});
  assert.deepEqual(Object.keys(rows[2]).sort(),['layerUid','locked','name','shy','visible']);
  assert.equal(rows[2].locked,false);assert.equal(rows[2].shy,false);
  assert.equal(Object.hasOwn(C,'locked'),false);assert.equal(Object.hasOwn(C,'shy'),false);
  assert.equal(rows[2].name,'');assert.equal(Files.projectionMatches(root,info([A,B,C])),true);
  assert.equal(root.state.currentFrame,7);assert.equal(root._curFrame,7);assert.equal(root.state.waIn,3);assert.equal(root.state.waOut,19);
  assert.equal(root._waIn,3);assert.equal(root._waOut,19);assert.equal(root.state.activeLayerIdx,1);
  assert.equal(root._layerSel,selection);assert.equal(root.selectedPaths,paths);assert.equal(root.state.refMedia,reference);
  assert.equal(Files.refreshLayerTable(root,info([A,B],2)),true);assert.equal(rows[0],first);assert.equal(rows[1],second);
  assert.equal(Files.refreshLayerTable(root,info([A,B,C],3)),true);assert.equal(rows[2].layerUid,'C');
});
test('unchanged membership refreshes only native row metadata and preserves surrounding UI values',()=>{
  const root=view(),rows=root.state.layers,selection=root._layerSel,reference=root.state.refMedia;
  assert.equal(Files.refreshLayerTable(root,info([{...A,visible:false,name:'Native name'},{layerUid:'B'}],1)),true);
  assert.equal(root.state.layers,rows);assert.equal(rows[0].visible,false);assert.equal(rows[0].name,'Native name');
  assert.equal(rows[1].visible,true);assert.equal(rows[1].name,'B');assert.equal(root._layerSel,selection);assert.equal(root.state.refMedia,reference);
});
test('last removal preserves surviving UID selection and picks a remaining UI row if only removed row was selected',()=>{
  for(const selected of [[1,2],[2]]){
    const root=view([A,B,C]),rows=root.state.layers;root._layerSel=selected;root.state.activeLayerIdx=2;
    assert.equal(Files.refreshLayerTable(root,info([A,B],1)),true);
    assert.deepEqual(root._layerSel,[1]);assert.equal(root.state.activeLayerIdx,1);assert.equal(root.state.layers,rows);
    assert.equal(root.state.layers[1].layerUid,'B');assert.equal(root.state.currentFrame,7);
  }
  const root=view([A,B,C]),empty=[];root._layerSel=empty;root.state.activeLayerIdx=0;
  assert.equal(Files.refreshLayerTable(root,info([A,B],1)),true);assert.equal(root._layerSel,empty);
});
// The admitted single-splice projection now supports head, middle and tail removal.
// Require exact surviving UID/row identity and remap indices, rather than blessing deletion generally.
for(const [at,active,selected,anchor] of [[0,1,[1],1],[1,1,[0,1],1],[2,0,[0],0]])test('single native removal at '+at+' preserves surviving UID identities and remaps the exact layer selection',()=>{
  const root=view([A,B,C]),rows=root.state.layers,original=rows.slice(),reference=root.state.refMedia,paths=root.selectedPaths;
  const frames=original.map(row=>row.frames),motion=original.map(row=>row.motion),next=clone([A,B,C]);next.splice(at,1);
  root.state.activeLayerIdx=2;root._layerSel=[0,2];root._layerSelAnchor=2;
  const beforeMetadata=JSON.stringify(next);
  assert.equal(Files.refreshLayerTable(root,info(next,1)),true);
  assert.strictEqual(root.state.layers,rows);assert.deepEqual(rows.map(row=>row.layerUid),next.map(row=>row.layerUid));
  const survivors=original.filter((_,i)=>i!==at);
  rows.forEach((row,i)=>{const old=original.indexOf(survivors[i]);assert.strictEqual(row,survivors[i]);assert.strictEqual(row.frames,frames[old]);assert.strictEqual(row.motion,motion[old]);});
  assert.equal(root.state.activeLayerIdx,active);assert.deepEqual(root._layerSel,selected);assert.equal(root._layerSelAnchor,anchor);
  assert.equal(root.state.currentFrame,7);assert.equal(root.state.waIn,3);assert.equal(root.state.waOut,19);
  assert.strictEqual(root.state.refMedia,reference);assert.strictEqual(root.selectedPaths,paths);
  assert.equal(JSON.stringify(next),beforeMetadata);assert.equal(Files.projectionMatches(root,info(next,1)),true);
});
test('single removal cannot reorder surviving UIDs or change rows before a confirmed revision',()=>{
  for(const [next,revision] of [[[C,A],1],[[A,C],0]]){
    const root=view([A,B,C]),rows=root.state.layers,original=rows.slice(),before=snapshot(root);
    assert.equal(Files.refreshLayerTable(root,info(next,revision)),false);
    assert.equal(snapshot(root),before);assert.strictEqual(root.state.layers,rows);original.forEach((row,i)=>assert.strictEqual(rows[i],row));
  }
});
test('contiguous two appends and inverse two removals project complete metadata with retained UID rows',()=>{
  const root=view(),rows=root.state.layers,original=rows.slice(),selected=root._layerSel,paths=root.selectedPaths,reference=root.state.refMedia;
  root._layerSelAnchor=1;
  const D={layerUid:'D',name:'Fourth',visible:false,locked:true,shy:true,color:'#123456',parentLayerUid:A.layerUid};
  const next=[A,B,C,D];
  assert.equal(Files.refreshLayerTable(root,info(next,1)),true);
  assert.strictEqual(root.state.layers,rows);assert.deepEqual(rows.map(row=>row.layerUid),next.map(row=>row.layerUid));
  original.forEach((row,i)=>assert.strictEqual(rows[i],row));
  for(const [i,meta] of next.entries())for(const key of Object.keys(meta))assert.equal(rows[i][key],meta[key]);
  assert.equal(Files.projectionReadyForUI(root,info(next,1)),true);
  assert.strictEqual(root._layerSel,selected);assert.equal(root.state.activeLayerIdx,1);assert.equal(root._layerSelAnchor,1);
  assert.equal(Files.refreshLayerTable(root,info([A,B],2)),true);
  assert.strictEqual(root.state.layers,rows);assert.deepEqual(rows,original);original.forEach((row,i)=>assert.strictEqual(rows[i],row));
  assert.strictEqual(root._layerSel,selected);assert.equal(root.state.activeLayerIdx,1);assert.equal(root._layerSelAnchor,1);
  assert.strictEqual(root.selectedPaths,paths);assert.strictEqual(root.state.refMedia,reference);
  assert.equal(Files.projectionReadyForUI(root,info([A,B],2)),true);
});
test('contiguous removal of both rows projects the empty native table without replacing its array',()=>{
  const root=view(),rows=root.state.layers;root._layerSelAnchor=1;
  assert.equal(Files.refreshLayerTable(root,info([],1)),true);
  assert.strictEqual(root.state.layers,rows);assert.deepEqual(rows,[]);assert.deepEqual(root._layerSel,[]);
  assert.equal(root.state.activeLayerIdx,0);assert.equal(root._layerSelAnchor,0);
  assert.equal(Files.projectionReadyForUI(root,info([],1)),true);
});
for(const [name,layers,revision=1] of [
  ['reordered',[B,A]],['same-revision single removal',[B],0],['replaced',[A,C]],
  ['noncontiguous inserts',[{layerUid:'X'},A,{layerUid:'Y'},B]],
  ['same-revision two appends',[A,B,C,{layerUid:'D'}],0],['same-revision two removals',[],0],['duplicate',[A,B,A]],['empty UID',[A,B,{layerUid:''}]],
  ['late malformed visibility',[{...A,visible:false},{...B,visible:'false'}]],
  ['late malformed name',[{...A,visible:false},{...B,name:null}]],['document payload',[A,B,{...C,frames:[]}]],
])test('unsupported '+name+' refuses without partial row, selection or reference mutation',()=>{
  const root=view(),before=snapshot(root),rows=root.state.layers,first=rows[0],second=rows[1];
  try{assert.equal(Files.refreshLayerTable(root,info(layers,revision)),false);}catch(error){if(error.code==='ERR_ASSERTION')throw error;assert.match(error.message,/Native layer/);}
  assert.equal(snapshot(root),before);assert.equal(root.state.layers,rows);assert.equal(rows[0],first);assert.equal(rows[1],second);
});
for(const [key,value] of [['instanceId','foreign'],['documentId','foreign'],['lifecycleGeneration',2],['canvasW',321],['fps',30],['totalFrames',25]]){
  test('foreign epoch or changed composition '+key+' cannot refresh rows',()=>{
    const root=view(),before=snapshot(root);assert.equal(Files.refreshLayerTable(root,{...info([A,B,C]),[key]:value}),false);
    assert.equal(snapshot(root),before);
  });
}
test('locally corrupted membership and sparse next metadata are rejected before writes',()=>{
  const root=view();root.state.layers[0].layerUid='foreign';const before=snapshot(root);
  assert.equal(Files.refreshLayerTable(root,info([A,B,C])),false);assert.equal(snapshot(root),before);
  const clean=view(),sparse=[A,B];sparse.length=3;const cleanBefore=snapshot(clean);
  assert.throws(()=>Files.refreshLayerTable(clean,{...info([A,B]),layers:sparse}),/identity is invalid/);
  assert.equal(snapshot(clean),cleanBefore);
});
test('old or same-revision membership cannot masquerade as a later native undo',()=>{
  for(const revision of [0,1]){
    const root=view();assert.equal(Files.refreshLayerTable(root,info([A,B,C],1)),true);const before=snapshot(root);
    assert.equal(Files.refreshLayerTable(root,info([A,B],revision)),false);assert.equal(snapshot(root),before);
  }
});
function shippedTimeline(root){
  const source=fs.readFileSync('src/js/timeline.js','utf8');
  function shipped(name){const start=source.indexOf('function '+name+'('),end=source.indexOf('\n}\n',start);
    assert.ok(start>=0&&end>start);return source.slice(start,end+2);}
  function element(){return {style:{setProperty(){}},dataset:{},classList:{add(){},remove(){},toggle(){}},
    appendChild(){},addEventListener(){},setAttribute(){},getBoundingClientRect:()=>({width:320,height:180})};}
  const nodes=new Map(),context={state:root.state,selectedPaths:root.selectedPaths,FC:30,ROW_H:24,SM:{t:()=>'{n}'},
    document:{getElementById:id=>{if(!nodes.has(id))nodes.set(id,element());return nodes.get(id);},
      createElement:element,querySelectorAll:()=>[]},getEffectiveStrokes:()=>[],selHas:()=>false,
    computeLayerRenderOrder:()=>root.state.layers.map((_,idx)=>({type:'layer',idx})).reverse()};
  for(const name of ['syncDocFields','updateWaBar','updateOmMarkers','updatePlayhead','renderLayerList',
    'updateCompInstancePanel','updateDuplicatorPanel','updateFootagePanel','updateSelPropsPanel','updateFsSelPanel',
    'updateRevisionPanel','updateMaskPanel','updateCornersPanel','updateEllipseArcPanel','updateStarPanel',
    'updateTextActionsPanel','updateTextPropsPanel','updatePropsContext','_tlScrollSnapshot','_tlScrollRestore',
    'installLayerReorderGrip','layerCurveRowExtraHeight','playheadLeftPx','syncPlayheadToViewport','renderTweenCurveStrips'])context[name]=()=>0;
  context.window=context;vm.createContext(context);
  vm.runInContext(['hexToRgbTriplet','updateUI','renderTimeline','renderKeyframeCellsInto'].map(shipped).join('\n'),context);
  return context;
}
test('known ordinary timeline frame-reader gap remains explicit for metadata projections, with no invented frames',()=>{
  const root=view(),ui=shippedTimeline(root);
  assert.doesNotThrow(()=>ui.updateUI(true),'baseline active-row badge tolerates an empty compatibility frame array');
  assert.throws(()=>ui.updateUI(false),/isKeyframe/,'baseline full timeline already requires legacy frame records');
  assert.equal(Files.refreshLayerTable(root,info([A,B,C],1)),true);
  assert.doesNotThrow(()=>ui.updateUI(true),'append preserves the working active-row reader');
  assert.throws(()=>ui.updateUI(false),/reading '0'/,'new metadata row does not fabricate legacy frame records');
  root.state.activeLayerIdx=2;
  assert.throws(()=>ui.updateUI(true),/reading '7'/,'selecting the new row needs a separately admitted native UI reader');
  assert.equal(Object.hasOwn(root.state.layers[2],'frames'),false);
});
function desktop(){
  let active=false,revision=0,layers=clone([A,B]),pending=null,previewGate=null,fault=null;
  const listeners=new Map(),log=[],calls=[],binding={apiVersion:2,instanceId:'instance',documentId:'document',lifecycleGeneration:1,
    subscriptionId:'subscription',contentRevision:0};
  const meta=()=>({...info(layers),contentRevision:revision,documentSnapshotId:'snapshot-'+revision,documentDigest:String(revision).repeat(64)});
  const root={state:{},userLayers:[],selectedPaths:[],NemoNativeObjectProjectFile:Files,
    NemoApplicationMcpTransport:Transport,document:{getElementById:()=>({getBoundingClientRect:()=>({x:0,y:0,left:0,top:0,width:320,height:180})})},
    updateUI:()=>log.push('rows:'+root.state.layers.map(row=>row.layerUid).join('|'))};
  async function invoke(command,args){
    const r=args&&(args.request||JSON.parse(args.requestJson));calls.push({command,r});
    if(command==='nemo_native_status'||command==='nemo_native_object_client_status')return {...meta(),available:active};
    if(command==='nemo_native_object_bootstrap'){active=true;return {...meta(),requestId:r.requestId,resourceCount:0,viewportAvailable:false};}
    if(command==='nemo_native_revision_sync'){
      log.push(r.action);
      if(r.action==='binding')return binding;
      if(r.action==='subscribe')return r.binding;
      if(r.action==='acknowledge'){assert.deepEqual(root.state.layers.map(row=>row.layerUid),layers.map(row=>row.layerUid));pending=null;return null;}
      if(r.action==='disconnect')return null;
    }
    if(command==='nemo_native_object_reference'&&r.action.kind==='inspect'){
      log.push('inspect:'+revision);const next={...meta(),requestId:r.requestId};
      if(fault==='foreign')next.documentId='foreign';return next;
    }
    if(command==='nemo_native_object_preview'){
      if(pending){assert.equal(r.revisionSync.subscriptionId,binding.subscriptionId);assert.deepEqual(r.revisionSync.event,pending);}
      if(previewGate){const gate=previewGate;previewGate=null;await gate.promise;}
      log.push('present:'+revision+':'+r.frame);
      return {status:fault==='preview'?'refused':'presented',workId:'work-'+revision,viewGeneration:1};
    }
    assert.equal(command,'nemo_native_object_dispatch');assert.equal(r.expectedRevision,revision);
    assert.ok(['command.document.object.undo','command.document.object.redo'].includes(r.operation));
    layers=clone(r.operation.endsWith('.undo')?[A,B]:[A,B,C]);revision++;
    return {apiVersion:2,requestId:r.requestId,instanceId:r.instanceId,documentId:r.documentId,
      contentRevision:revision,ok:true,result:{applied:true,historyEntriesAdded:1}};
  }
  root.__TAURI__={core:{invoke},event:{listen:async(name,callback)=>{listeners.set(name,callback);return ()=>listeners.delete(name);}}};
  require('./helpers/native-viewport-underlay.cjs').install(root);
  const session=Project.desktop(root);
  return {root,session,log,calls,meta,hold:gate=>{previewGate=gate;},fail:value=>{fault=value;},
    async open(){await session.openPath('/native.json',{commit:value=>Files.projectView(root,value)});log.length=0;root.state.currentFrame=7;root.state.waIn=3;root.state.waOut=19;root._layerSel=[1];root.state.activeLayerIdx=1;},
    async external(next){const from=revision;layers=clone(next);revision++;pending={instanceId:'instance',documentId:'document',lifecycleGeneration:1,
      requestId:'external-'+revision,fromRevision:from,toRevision:revision};return listeners.get('nemo-native-revision')({payload:pending});}};
}
test('real desktop transport subscriber updates append/undo/redo rows before confirmed preview and ACK',async()=>{
  const h=desktop();await h.open();const rows=h.root.state.layers,first=rows[0],second=rows[1];
  for(const next of [[A,B,C],[A,B],[A,B,C]]){
    h.log.length=0;const gate=deferred();h.hold(gate);const change=h.external(next);await turn();
    assert.equal(h.log.includes('acknowledge'),false);assert.equal(h.session.readPresentedObjectSession(),null);
    assert.deepEqual(rows.map(row=>row.layerUid),next.map(row=>row.layerUid));
    gate.resolve();await change;
    assert.ok(h.log.findIndex(entry=>entry.startsWith('rows:'))<h.log.findIndex(entry=>entry.startsWith('present:')));
    assert.ok(h.log.findIndex(entry=>entry.startsWith('present:'))<h.log.indexOf('acknowledge'));
    assert.equal(h.session.readPresentedObjectSession().contentRevision,h.meta().contentRevision);
    assert.equal(rows[0],first);assert.equal(rows[1],second);assert.equal(h.root.state.currentFrame,7);
    assert.equal(h.root.state.waIn,3);assert.equal(h.root.state.waOut,19);assert.deepEqual(h.root._layerSel,[1]);
  }
});
test('local existing history publication uses native metadata before presenting the changed table',async()=>{
  const h=desktop();await h.open();const rows=h.root.state.layers,first=rows[0];
  for(const operation of ['redo','undo','redo']){
    h.log.length=0;await h.session.historyFromUi(operation);
    assert.equal(h.root.state.layers,rows);assert.equal(rows[0],first);assert.equal(rows.length,operation==='undo'?2:3);
    assert.ok(h.log.findIndex(entry=>entry.startsWith('rows:'))<h.log.findIndex(entry=>entry.startsWith('present:')));
    assert.ok(h.log.some(entry=>entry.endsWith(':7')));assert.equal(h.root.state.currentFrame,7);
    assert.deepEqual(h.root._layerSel,[1]);assert.ok(h.session.readPresentedObjectSession());
  }
});
for(const fault of ['duplicate','reordered','foreign','preview','corrupt-ui'])test('invalid '+fault+' external update fences delivery and never ACKs',async()=>{
  const h=desktop();await h.open();
  if(fault==='corrupt-ui')h.root.state.layers[0].layerUid='foreign';
  if(['foreign','preview'].includes(fault))h.fail(fault);
  const before=snapshot(h.root),next=fault==='duplicate'?[A,B,A]:fault==='reordered'?[B,A]:[A,B,C];
  await h.external(next);
  assert.equal(h.log.includes('acknowledge'),false);assert.equal(h.session.isActive(),false);assert.equal(h.session.readPresentedObjectSession(),null);
  if(fault!=='preview')assert.equal(snapshot(h.root),before);
  assert.equal(h.calls.some(call=>call.command==='nemo_native_dispatch'),false,'no Add Layer/public command was introduced');
});


function decoded(layers,revision=0){
  const request={requestId:'parent-meta',instanceId:'instance',documentId:'document'};
  return Metadata.decode({...info(layers,revision),requestId:request.requestId},request);
}
test('real metadata decode copies and freezes opaque parent UIDs without normalizing their bytes',()=>{
  for(const parent of ['opaque A:雪','親:é/🙂','  native UID : 雪  ']){
    const rows=[{layerUid:parent},{layerUid:'child:雪',parentLayerUid:parent}];
    const source={...info(rows),requestId:'parent-meta'},before=JSON.stringify(source);
    const value=Metadata.decode(source,{requestId:'parent-meta',instanceId:'instance',documentId:'document'});
    assert.equal(value.layers[1].parentLayerUid,parent);
    assert.equal(Object.isFrozen(value),true);assert.equal(Object.isFrozen(value.layers),true);assert.equal(Object.isFrozen(value.layers[1]),true);
    assert.notStrictEqual(value.layers,source.layers);assert.notStrictEqual(value.layers[1],source.layers[1]);
    assert.equal(JSON.stringify(source),before);
    source.layers[1].parentLayerUid='later input mutation';assert.equal(value.layers[1].parentLayerUid,parent);
  }
  const missing=decoded([{layerUid:'child'}]),nil=decoded([{layerUid:'child',parentLayerUid:null}]);
  assert.equal(Object.hasOwn(missing.layers[0],'parentLayerUid'),false);
  assert.equal(Object.hasOwn(nil.layers[0],'parentLayerUid'),true);assert.equal(nil.layers[0].parentLayerUid,null);
});
test('real metadata decode rejects late empty or nonstring parent without mutating the source table',()=>{
  for(const parent of ['',0,false,{},[],undefined]){
    const source={...info([{...A,parentLayerUid:'B'},{...B,parentLayerUid:parent}]),requestId:'parent-meta'};
    const before=JSON.stringify(source);
    assert.throws(()=>Metadata.decode(source,{requestId:'parent-meta',instanceId:'instance',documentId:'document'}),/uncorrelated/);
    assert.equal(JSON.stringify(source),before);
  }
});
test('projectView and correlated refresh preserve exact parent strings and clear null or missing links',()=>{
  const parent='opaque A:雪',rows=[{...A,layerUid:parent},{...B,parentLayerUid:parent}];
  const root={state:{},userLayers:[],selectedPaths:[]},initial=decoded(rows);
  Files.projectView(root,initial);const table=root.state.layers,child=table[1],frames=child.frames,motion=child.motion;
  assert.equal(child.parentLayerUid,parent);assert.equal(Files.projectionMatches(root,initial),true);
  assert.equal(Files.projectionReadyForUI(root,initial),true);
  assert.equal(Object.isFrozen(initial.layers[1]),true);assert.equal(child.layerUid,'B');
  for(const [revision,nextParent] of [[1,null],[2,parent],[3,undefined]]){
    const nextRows=[rows[0],{...B}];if(nextParent!==undefined)nextRows[1].parentLayerUid=nextParent;
    const next=decoded(nextRows,revision);
    assert.equal(Files.refreshLayerTable(root,next),true);
    assert.strictEqual(root.state.layers,table);assert.strictEqual(table[1],child);
    assert.strictEqual(child.frames,frames);assert.strictEqual(child.motion,motion);
    assert.equal(child.parentLayerUid,nextParent===undefined?null:nextParent);
    assert.equal(Files.projectionMatches(root,next),true);assert.equal(Files.projectionReadyForUI(root,next),true);
  }
});
test('same-revision parent drift cannot become correlated by matching layer IDs or local row mutation',()=>{
  const rows=[A,{...B,parentLayerUid:A.layerUid}],root={state:{},userLayers:[]};
  const original=decoded(rows);Files.projectView(root,original);
  for(const value of [null,'other:雪',undefined]){
    const nextRows=[A,{...B}];if(value!==undefined)nextRows[1].parentLayerUid=value;
    const drift=decoded(nextRows),before=snapshot(root);
    assert.equal(Files.projectionMatches(root,drift),false);assert.equal(Files.projectionReadyForUI(root,drift),false);
    assert.equal(Files.refreshLayerTable(root,drift),false);assert.equal(snapshot(root),before);
  }
  root.state.layers[1].parentLayerUid='locally invented parent';
  const corrupted=snapshot(root);
  assert.equal(Files.projectionMatches(root,original),false);assert.equal(Files.projectionReadyForUI(root,original),false);
  assert.equal(Files.refreshLayerTable(root,decoded(rows,1)),false);assert.equal(snapshot(root),corrupted);
});
test('malformed parent tables and foreign or stale revision pins refuse atomically',()=>{
  const rows=[A,{...B,parentLayerUid:A.layerUid}];
  for(const parent of ['',false,0,{},[],undefined]){
    const root={state:{},userLayers:[]};Files.projectView(root,decoded(rows));const before=snapshot(root),table=root.state.layers;
    const bad=info([{...A,visible:false},{...B,parentLayerUid:parent}],1);
    assert.throws(()=>Files.refreshLayerTable(root,bad),/Native layer parent/);
    assert.equal(snapshot(root),before);assert.strictEqual(root.state.layers,table);
    assert.throws(()=>Files.projectView(root,bad),/Native layer parent/);assert.equal(snapshot(root),before);
  }
  const root={state:{},userLayers:[]};Files.projectView(root,decoded(rows,2));
  for(const mutate of [v=>v.contentRevision=1,v=>v.documentSnapshotId='foreign-same-revision',
    v=>v.instanceId='foreign',v=>v.documentId='foreign',v=>v.lifecycleGeneration=2]){
    const bad=info([A,{...B,parentLayerUid:null}],2);mutate(bad);const before=snapshot(root);
    assert.equal(Files.refreshLayerTable(root,bad),false);assert.equal(snapshot(root),before);
  }
});
