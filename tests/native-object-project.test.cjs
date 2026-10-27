'use strict';
const test=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const vm=require('node:vm');
const Project=require('../src/js/adapters/native-object-project.js');
const Files=require('../src/js/adapters/native-object-project-file.js');
const Workflow=require('../src/js/application/native-object-save-as-workflow.js');
const Ack=require('../src/js/application/native-object-saved-acknowledgement.js');
const turn=()=>new Promise(setImmediate);
function deferred(){let resolve;const promise=new Promise(r=>{resolve=r;});return {promise,resolve};}
function host(){
  const calls=[];let installed=false,revision=0,edited=false,undo=false,preview='presented',failure=null;
  let reference=null;const copies=[];
  const info=()=>({apiVersion:2,instanceId:'instance',documentId:'document',lifecycleGeneration:1,
    documentSnapshotId:'snapshot-'+revision,contentRevision:revision,canvasW:320,canvasH:180,fps:24,totalFrames:21,
    documentDigest:(edited?'b':'a').repeat(64),byteCount:edited?200000:10000,layers:[{layerUid:'drawing'}],reference,
    undoDepth:edited?1:0,redoDepth:undo?1:0});
  async function invoke(command,args){
    const r=args?JSON.parse(args.requestJson):null;calls.push({command,r});
    if(command==='nemo_native_object_client_status')return {...info(),available:installed};
    if(command==='nemo_native_object_bootstrap'){
      assert.equal(r.projectOnly,true);assert.equal(typeof r.sourcePath,'string');assert.equal('documentJson' in r,false);
      if(r.sourcePath==='/opacity.json')throw {code:'unsupported_format'};
      if(r.sourcePath==='/bad.json')throw {code:'invalid_request'};
      installed=true;return {...info(),requestId:r.requestId,resourceCount:0,viewportAvailable:false};
    }
    if(command==='nemo_native_object_reference'&&r.action.kind==='inspect'){
      if(failure==='inspection')throw Error('Owner unavailable');
      return {...info(),requestId:r.requestId};
    }
    if(command==='nemo_native_object_preview')return {status:preview,workId:'work-'+revision,viewGeneration:1};
    if(command==='nemo_native_object_reference'||command==='nemo_native_object_dispatch'){
      assert.equal(r.expectedRevision,revision);
      if(command==='nemo_native_object_dispatch'){
        assert.ok(['command.document.object.undo','command.document.object.redo'].includes(r.operation));
        assert.deepEqual(r.payload,{command:r.operation==='command.document.object.undo'?'object.undo':'object.redo'});
      }
      if(failure==='refused')return {apiVersion:2,requestId:r.requestId,instanceId:'instance',documentId:'document',contentRevision:revision,ok:false,error:{code:'invalid_request'}};
      if(failure==='transport-before'||failure==='inspection')throw Error('Native response lost');
      if(r.operation==='command.document.object.undo'){edited=false;undo=true;reference=null;}
      else if(r.operation==='command.document.object.redo'){edited=true;undo=false;}
      else {edited=true;reference={id:'ref',name:'image.png',opacity:0.5,visible:true,offsetFrames:0};}
      revision++;
      if(failure==='transport-after')throw Error('Native response lost');
      return {apiVersion:2,requestId:r.requestId,instanceId:'instance',documentId:'document',contentRevision:revision,ok:true,result:{applied:true,historyEntriesAdded:1}};
    }
    if(command==='nemo_native_object_save_as'){
      copies.push(r);return {apiVersion:2,requestId:r.requestId,instanceId:r.instanceId,documentId:r.documentId,atRevision:r.atRevision,
        documentSnapshotId:r.documentSnapshotId,byteCount:info().byteCount,published:true,cleanupPending:false};
    }
    if(command==='nemo_native_object_export')return {apiVersion:2,requestId:r.requestId,instanceId:'instance',documentId:'document',
      contentRevision:revision,documentSnapshotId:'snapshot-'+revision,status:'succeeded',cleanupStatus:'complete',
      externalEffectDisposition:'committed',errorCode:null,artifactFiles:['frame_0001.png'],published:true,cleanupComplete:true};
    throw Error('Unexpected command: '+command);
  }
  return {invoke,info,calls,copies,preview(value){preview=value;},fail(value){failure=value;}};
}
function session(h){return Project.create({invoke:h.invoke,mapping:info=>({cssBounds:{x:0,y:0,width:320,height:180},
  physicalExtent:{width:320,height:180},compositionExtent:{width:info.canvasW,height:info.canvasH},reportedDpr:1}),frame:()=>0});}
test('post-bootstrap diagnostics preserve original errors, safe unknowns and successful request order',async()=>{
  for(const nativeCode of ['unavailable','wrong_instance','wrong_document','cancelled_before_dispatch']){
  for(const hostile of [false,true]){
    const h=host(),events=[],error={message:'PRIVATE PATH AND DOCUMENT'};
    if(hostile)Object.defineProperty(error,'code',{get(){throw Error('PRIVATE GETTER');}});
    else Object.assign(error,{code:nativeCode,details:{disposition:'notDispatched'}});
    const s=Project.create({invoke:(command,args)=>command==='nemo_native_object_preview'?Promise.reject(error):h.invoke(command,args),
      mapping:()=>({cssBounds:{width:320,height:180},physicalExtent:{width:320,height:180}}),
      openDiagnostic:(event,code)=>{if(event==='post-bootstrap-failed'){events.push(code);throw Error('PRIVATE CALLBACK');}}});
    await assert.rejects(s.openPath('/project.json'),value=>value===error);
    assert.deepEqual(events,['post-bootstrap-first-preview:invoke-rejected:'+(hostile?'unknown:other':nativeCode+':notDispatched')]);
    assert.equal(s.isActive(),false);assert.doesNotMatch(JSON.stringify(events),/PRIVATE/);
  }
  }
  const h=host(),control=host(),events=[],mapping=()=>({cssBounds:{width:320,height:180},physicalExtent:{width:320,height:180}});
  await Project.create({invoke:h.invoke,mapping,openDiagnostic:(...event)=>events.push(event)}).openPath('/project.json');
  await Project.create({invoke:control.invoke,mapping}).openPath('/project.json');
  assert.deepEqual(h.calls,control.calls);assert.deepEqual(events.map(e=>e[0]),['bootstrap-dispatched','bootstrap-resolved']);
});
async function presented(h){const s=session(h);await s.openPath('/project.json');await s.presentPreview(0);return s;}
function files(s,overrides={}){
  const effects=[];let handle='tab';
  const ports={readUI:()=>({handle,generation:1}),reveal:async()=>{},opened:(path)=>{effects.push(['open',path]);return true;},
    commit:(path)=>{effects.push(['save',path]);return true;},pick:async()=>'/new.json',repaint:()=>{},failed:()=>{},
    refused:()=>{},confirmReplace:async()=>true,notice:()=>{},...overrides};
  return {ui:Files.create(s,ports,Workflow,Ack),effects,replace(){handle='other';}};
}
test('native file Open and Save As carry paths and compact actual metadata, never document bytes',async()=>{
  const h=host(),s=session(h),f=files(s);assert.equal(await f.ui.openPath('/project.json'),true);assert.equal(f.ui.isDirty(),false);
  await s.reference({kind:'attach',id:'ref',sourcePath:'/image.png'});assert.equal(f.ui.isDirty(),true);
  assert.equal((await f.ui.saveAs()).acknowledged,true);assert.equal(f.ui.isDirty(),false);
  assert.equal(h.copies.length,1);assert.equal(h.copies[0].atRevision,1);
  assert.equal(JSON.stringify(h.calls).includes('documentJson'),false);
  assert.equal(JSON.stringify(h.calls).includes('data:image'),false);
});
test('undo to saved bytes clears dirty even though native content revision increases',async()=>{
  const h=host(),s=session(h),f=files(s);await f.ui.openPath('/project.json');
  await s.reference({kind:'attach',id:'ref',sourcePath:'/image.png'});assert.equal(f.ui.isDirty(),true);
  await s.historyFromUi('undo');assert.equal(s.info().contentRevision,2);assert.equal(f.ui.isDirty(),false);
  await s.historyFromUi('redo');assert.equal(f.ui.isDirty(),true);
  assert.deepEqual(h.calls.filter(c=>c.command==='nemo_native_object_dispatch').map(c=>[c.r.operation,c.r.payload]),[
    ['command.document.object.undo',{command:'object.undo'}],['command.document.object.redo',{command:'object.redo'}]
  ]);
  const count=h.calls.length;await assert.rejects(s.historyFromUi('clear'),/Unsupported history action/);assert.equal(h.calls.length,count);
});
for(const failure of ['refused','transport-before','transport-after']){
  test('reconciled '+failure+' re-presents current native state without retrying mutation',async()=>{
    const h=host(),s=session(h),f=files(s);await f.ui.openPath('/project.json');h.fail(failure);
    await assert.rejects(s.reference({kind:'attach',id:'ref',sourcePath:'/image.png'}));
    assert.equal(h.calls.filter(c=>c.command==='nemo_native_object_reference'&&c.r.action.kind==='attach').length,1);
    assert.equal(s.isActive(),true);assert.ok(s.readPresentedObjectSession());
    assert.equal(s.info().contentRevision,failure==='transport-after'?1:0);
    assert.equal(f.ui.isDirty(),failure==='transport-after');h.fail(null);
    assert.equal((await f.ui.saveAs()).acknowledged,true);
  });
}
test('failed reconciliation fences authority and does not present or acknowledge Save',async()=>{
  const h=host(),s=await presented(h),f=files(s),before=h.calls.filter(c=>c.command==='nemo_native_object_preview').length;
  h.fail('inspection');await assert.rejects(s.reference({kind:'remove'}),/Owner unavailable/);
  assert.equal(s.isActive(),false);assert.equal(s.blocksLegacy(),true);assert.equal(s.readPresentedObjectSession(),null);
  assert.equal(h.calls.filter(c=>c.command==='nemo_native_object_preview').length,before);
  assert.equal((await f.ui.saveAs()).acknowledged,false);assert.equal(h.copies.length,0);
});
test('Save As refuses an owner advance during picker, and a tab replacement before acknowledgement',async()=>{
  const h=host(),s=await presented(h),picker=deferred(),f=files(s,{pick:()=>picker.promise});
  const pending=f.ui.saveAs();await turn();await s.reference({kind:'remove'});picker.resolve('/new.json');
  assert.equal((await pending).acknowledged,false);assert.equal(h.copies.length,0);
  const picker2=deferred(),g=files(s,{pick:()=>picker2.promise});const next=g.ui.saveAs();await turn();g.replace();picker2.resolve('/other.json');
  assert.equal((await next).acknowledged,false);assert.equal(h.copies.length,0);
});
test('failed/deferred presentation never supplies a presented save session or opens the UI',async()=>{
  const h=host();h.preview('deferred-occluded');const s=session(h),f=files(s);
  assert.equal(await f.ui.openPath('/project.json'),false);assert.deepEqual(f.effects,[]);
  assert.equal(s.readPresentedObjectSession(),null);assert.equal((await f.ui.saveAs()).acknowledged,false);assert.equal(h.copies.length,0);
});
test('only explicit pre-install unsupported format allows the older file route',async()=>{
  const h=host(),s=session(h);assert.equal(await s.openPath('/opacity.json'),null);assert.equal(s.blocksLegacy(),false);
  await s.openPath('/project.json');assert.equal(s.blocksLegacy(),true);
  const bad=session(host());await assert.rejects(bad.openPath('/bad.json'));assert.equal(bad.blocksLegacy(),true);assert.throws(bad.getNativeIdentity);
});
test('metadata is detached and immutable; malformed or foreign inspection fences admission',async()=>{
  const h=host(),s=await presented(h);assert.equal(Object.isFrozen(s.info().layers[0]),true);
  assert.throws(()=>{s.info().layers[0].layerUid='stolen';},TypeError);
  const foreign=host(),original=foreign.invoke;foreign.invoke=async(command,args)=>{const v=await original(command,args);if(command==='nemo_native_object_reference')v.instanceId='other';return v;};
  const bad=session(foreign);await assert.rejects(bad.openPath('/project.json'),/uncorrelated/);assert.equal(bad.isActive(),false);
});
test('actual native metadata and read-side adoption retain named layers without changing UID selection',async()=>{
  const h=host(),invoke=h.invoke;
  const layers=[{layerUid:'r08_curve_layer',name:'R08 rectangle'},{layerUid:'empty',name:''},
    {layerUid:'unicode',name:' Calque 雪 🟦\n'},{layerUid:'unnamed'}];
  h.invoke=async(command,args)=>{const v=await invoke(command,args);if(v.layers)v.layers=layers;return v;};
  const s=await presented(h),meta=s.info();
  assert.deepEqual(meta.layers,layers);assert.equal(Object.hasOwn(meta.layers[3],'name'),false);
  assert.throws(()=>{meta.layers[0].name='forged';},TypeError);
  const root={state:{},selectedPaths:['stale'],userLayers:[{visible:true}]};
  Files.projectView(root,meta);
  assert.deepEqual(root.state.layers.map(l=>[l.layerUid,l.name]),[
    ['r08_curve_layer','R08 rectangle'],['empty',''],['unicode',' Calque 雪 🟦\n'],['unnamed','Layer 4']]);
  assert.deepEqual(root._layerSel,[0]);assert.deepEqual(root.selectedPaths,[]);
  assert.equal(root.userLayers[0].visible,false);assert.equal(root.state.activeLayerIdx,0);
  assert.equal(h.calls.some(c=>c.command==='nemo_native_object_dispatch'),false);
  assert.equal(h.copies.length,0);assert.deepEqual(meta.layers,layers);
});
test('malformed supplied layer names fence native admission and do not mutate the presentation',async()=>{
  for(const name of [null,undefined,false,3,[],{}]){
    const h=host(),invoke=h.invoke;
    h.invoke=async(command,args)=>{const v=await invoke(command,args);if(v.layers)v.layers=[{layerUid:'drawing',name}];return v;};
    const s=session(h);await assert.rejects(s.openPath('/project.json'),/uncorrelated/);
    assert.equal(s.isActive(),false);assert.equal(s.blocksLegacy(),true);
    assert.equal(s.readPresentedObjectSession(),null);assert.equal(h.copies.length,0);
    const root={state:{canvasW:9,layers:[{layerUid:'original',name:'Original'}]},selectedPaths:['selected']};
    const before=JSON.stringify(root);
    assert.throws(()=>Files.projectView(root,{canvasW:320,layers:[{layerUid:'drawing',name}]}),/layer name is invalid/);
    assert.equal(JSON.stringify(root),before);
  }
});
test('single-frame native export preserves pinned host route; invalid ranges do not dispatch',async()=>{
  const h=host(),s=await presented(h);assert.equal((await s.exportPng('/new-output',[0])).published,true);
  await assert.rejects(s.exportPng('/new-output',[1,0]),/frame range is invalid/);
  const r=h.calls.find(c=>c.command==='nemo_native_object_export').r;
  assert.equal(r.documentSnapshotId,'snapshot-0');assert.equal(r.maximumEncodedBytes,4*1024*1024);
  assert.equal(Object.hasOwn(r,'format'),false,'PNG remains the omitted-format default');
  assert.equal('reference' in r,false);
});
test('native export reports no success for contradictory or malformed Rust receipts',async()=>{
  const negatives={
    wrongApi:{apiVersion:1},missingApi:{apiVersion:undefined},stringApi:{apiVersion:'2'},
    failed:{status:'failed'},cancelled:{status:'cancelled'},running:{status:'running'},missingStatus:{status:undefined},
    cleanupFailed:{cleanupStatus:'failed'},cleanupPending:{cleanupStatus:'pending'},missingCleanup:{cleanupStatus:undefined},
    indeterminate:{externalEffectDisposition:'indeterminate'},contained:{externalEffectDisposition:'contained'},
    noPublication:{externalEffectDisposition:'none'},missingDisposition:{externalEffectDisposition:undefined},
    error:{errorCode:'publish_failed'},missingError:{errorCode:undefined},malformedError:{errorCode:{}},
    noFiles:{artifactFiles:[]},wrongFile:{artifactFiles:['frame_0002.png']},unsafeFile:{artifactFiles:['../frame_0001.png']},
    duplicateFiles:{artifactFiles:['frame_0001.png','frame_0001.png']},extraFile:{artifactFiles:['frame_0001.png','other.png']},
    missingFiles:{artifactFiles:undefined},stringFiles:{artifactFiles:'frame_0001.png'},nullFiles:{artifactFiles:null},
    wrongRequest:{requestId:'foreign'},wrongInstance:{instanceId:'foreign'},wrongDocument:{documentId:'foreign'},
    staleRevision:{contentRevision:1},staleSnapshot:{documentSnapshotId:'stale'},
    unpublished:{published:false},cleanupIncomplete:{cleanupComplete:false}
  };
  for(const [label,patch] of Object.entries(negatives)){
    const h=host(),invoke=h.invoke,progress=[];
    h.invoke=async(command,args)=>{const value=await invoke(command,args);
      return command==='nemo_native_object_export'?{...value,...patch}:value;};
    const s=await presented(h);
    await assert.rejects(s.exportPng('/new-output',[0],(...args)=>progress.push(args)),/publication is unconfirmed/,label);
    assert.deepEqual(progress,[],label);assert.equal(s.blocksLegacy(),true,label);
    assert.equal(h.calls.filter(c=>c.command==='nemo_native_object_export').length,1,label);
  }
});
test('two clean exports serialize through the same session with distinct requests and destinations',async()=>{
  const h=host(),invoke=h.invoke,first=deferred(),progress=[];let exports=0;
  h.invoke=async(command,args)=>{const value=await invoke(command,args);
    if(command==='nemo_native_object_export'&&++exports===1)await first.promise;return value;};
  const s=await presented(h),before=JSON.stringify(s.info());
  const a=s.exportPng('/new-a',[0],(...args)=>progress.push(args));
  const b=s.exportPng('/new-b',[0],(...args)=>progress.push(args));
  await turn();assert.equal(exports,1);assert.deepEqual(progress,[]);
  first.resolve();const [one,two]=await Promise.all([a,b]);
  const requests=h.calls.filter(c=>c.command==='nemo_native_object_export').map(c=>c.r);
  assert.equal(requests.length,2);assert.notEqual(one.requestId,two.requestId);
  assert.deepEqual(requests.map(r=>r.destinationDirectory),['/new-a','/new-b']);
  for(const r of requests){assert.equal(r.instanceId,'instance');assert.equal(r.documentId,'document');
    assert.equal(r.documentSnapshotId,'snapshot-0');assert.equal(r.contentRevision,0);assert.equal(r.lifecycleGeneration,1);}
  assert.deepEqual(progress,[[1,1],[1,1]]);assert.equal(JSON.stringify(s.info()),before);
  assert.deepEqual(await s.exportPng('/cancelled',[0],()=>assert.fail('cancelled progress'),{aborted:true}),{status:'cancelled'});
  await assert.rejects(s.exportPng('/range',[0,21]),/frame range is invalid/);
  assert.equal(h.calls.filter(c=>c.command==='nemo_native_object_export').length,2);
  assert.equal(s.blocksLegacy(),true);assert.equal(s.isActive(),true);
});
function ordinary(h,deferFrames=false){
  const fixture=fs.readFileSync('tests/project-open.test.cjs','utf8');
  const start=fixture.indexOf('function harness('),end=fixture.indexOf('\nfunction select(',start);
  const code=fixture.slice(start,end).replace('configureWindow(window);',
    "window.state=context.state;window.document=document;window.userLayers=context.userLayers;window.showToast=context.showToast;document.getElementById('drawing-canvas').getBoundingClientRect=()=>({left:0,top:0,width:320,height:180});configureWindow(window);");
  const context=vm.createContext({assert,vm,require,setTimeout,console,source:fs.readFileSync('src/js/project.js','utf8'),
    documentSource:fs.readFileSync('src/js/project-document.js','utf8'),entrySource:fs.readFileSync('src/js/project-entry.js','utf8')});
  vm.runInContext(code+'\nthis.makeHarness=harness;',context);
  return context.makeHarness({dirtyDot:true,deferFrames,configureWindow(root){
    const classes=new Set(),styles=new Map();
    root.document.documentElement={clientWidth:1280,clientHeight:900,
      classList:{contains:k=>classes.has(k),add:k=>classes.add(k),remove:k=>classes.delete(k)},
      style:{getPropertyValue:k=>styles.get(k)||'',getPropertyPriority:()=>'',setProperty:(k,v)=>styles.set(k,v),removeProperty:k=>styles.delete(k)}};
    root.stageLayer={visible:true};root.paper={view:{update(){}}};
    root.NemoNativeViewportUnderlay=require('../src/js/adapters/native-viewport-underlay.js');
    root.NemoNativeObjectProject=Project;root.NemoNativeObjectProjectFile=Files;root.NemoNativeObjectSaveAsWorkflow=Workflow;root.NemoNativeObjectSavedAcknowledgement=Ack;
    root.NemoNativeObjectInactiveProjectTabs=require('../src/js/adapters/native-object-inactive-project-tabs.js');
    let id=0;root.crypto={randomUUID:()=>`inactive-ui-${++id}`};
    root.__TAURI__={core:{invoke:h.invoke},dialog:{save:async()=>'/ordinary-save.json'},fs:{readTextFile:()=>{throw Error('Project bytes crossed JS');}}};
  }});
}
test('complete production Project Open/Save As updates tab, recents and dirty state through native adapters',async()=>{
  const h=host(),app=ordinary(h);assert.equal(await app.project.openPath('/ordinary.json'),true);
  assert.equal(app.window.document.documentElement.classList.contains('nemo-native-underlay'),true);
  assert.equal(app.project.getCurrentLabel(),'ordinary — /ordinary.json');assert.equal(app.project.isDirty(),false);
  assert.equal(app.element('project-tabs-list').children.length,1);assert.equal(app.startScreen.classList.contains('hid'),true);
  const s=Project.desktop(app.window);await s.reference({kind:'remove'});assert.equal(app.project.isDirty(),true);
  assert.equal((await app.project.saveAs()).acknowledged,true);assert.equal(app.project.getCurrentLabel(),'ordinary-save — /ordinary-save.json');
  assert.equal(app.project.isDirty(),false);assert.equal(app.mutations,0);assert.equal(app.downloads.length,0);
  assert.ok(app.writes.every(([key])=>key==='nemo-recents'),'no byte autosave or alternate document authority');
  assert.equal(h.calls.filter(c=>c.command==='nemo_native_object_bootstrap').length,1);
});

function replacementHost(){
  const calls=[],copies=[],previews=[],seen=new Map();let owner=null,generation=0,viewGeneration=0,failure=null,inactive=null;
  const size=path=>path==='/b.json'?{canvasW:640,canvasH:360}:{canvasW:320,canvasH:180};
  const info=()=>({apiVersion:2,instanceId:'instance',documentId:owner.documentId,
    lifecycleGeneration:owner.generation,documentSnapshotId:'snapshot-'+owner.generation+'-'+owner.revision,
    contentRevision:owner.revision,documentDigest:(owner.revision?'c':owner.path==='/b.json'?'b':'a').repeat(64),byteCount:10000,
    ...size(owner.path),fps:24,totalFrames:21,layers:[{layerUid:'drawing'}],reference:null,undoDepth:owner.revision,redoDepth:0});
  const status=()=>({apiVersion:2,instanceId:'instance',available:!!owner,documentId:owner&&owner.documentId,
    contentRevision:owner&&owner.revision,lifecycleGeneration:owner&&owner.generation});
  function install(path){generation++;owner={path,documentId:'document-'+generation,generation,revision:0};}
  async function invoke(command,args){
    const r=args?JSON.parse(args.requestJson):null;calls.push({command,r});
    if(command==='nemo_native_object_client_status')return status();
    if(command==='nemo_native_object_inactive_project'){
      const base={apiVersion:2,requestId:r.requestId,instanceId:r.instanceId,documentId:r.documentId,
        contentRevision:r.expectedRevision,documentSnapshotId:r.documentSnapshotId,lifecycleGeneration:r.lifecycleGeneration,retryExecution:false};
      if(r.action.kind==='inspect')return {...base,state:inactive?'inactive-retained':'inactive-absent',
        preparationToken:inactive&&inactive.preparationToken||null,project:inactive};
      if(r.action.kind==='prepare'){
        inactive={instanceId:r.instanceId,contentRevision:0,documentSnapshotId:'inactive-snapshot-1',documentDigest:'d'.repeat(64),
          byteCount:10000,canvasW:640,canvasH:360,fps:24,totalFrames:21,layers:[{layerUid:'inactive-layer'}],
          origin:{documentId:r.documentId,documentSnapshotId:r.documentSnapshotId,contentRevision:r.expectedRevision},
          preparationToken:'inactive-token-1',preparationRequestId:r.requestId};
        return {...base,state:'inactive-prepared',preparationToken:inactive.preparationToken,project:inactive};
      }
      if(r.action.kind==='close'){
        if(!inactive||inactive.preparationToken!==r.action.preparationToken)throw Error('unknown inactive token');
        inactive=null;return {...base,state:'inactive-closed',preparationToken:r.action.preparationToken,project:null};
      }
      throw Error('unexpected inactive action');
    }
    if(command==='nemo_native_object_bootstrap'){
      assert.equal(r.projectOnly,true);assert.equal('documentJson' in r,false);
      if(seen.has(r.requestId)){
        const saved=seen.get(r.requestId);if(JSON.stringify(saved.request)!==JSON.stringify(r))throw Error('changed replay');
        return saved.receipt;
      }
      if(owner)assert.deepEqual(r.replace,{documentId:owner.documentId,expectedRevision:owner.revision,
        documentSnapshotId:info().documentSnapshotId,lifecycleGeneration:owner.generation});
      else assert.equal('replace' in r,false);
      if(r.sourcePath==='/bad.json')throw {code:'replacement_refused',message:'Invalid project',
        details:{disposition:'notDispatched',retryExecution:false,reasonCode:'invalid_request'}};
      if(r.sourcePath==='/drift.json'){install('/other.json');throw {code:'replacement_refused',message:'Invalid project',
        details:{disposition:'notDispatched',retryExecution:false,reasonCode:'invalid_request'}};}
      if(r.sourcePath==='/stale.json'){install(r.sourcePath);return {apiVersion:2,requestId:'wrong',instanceId:'instance',
        documentId:owner.documentId,contentRevision:0,resourceCount:0,viewportAvailable:false};}
      if(r.sourcePath==='/replayed.json'){
        install(r.sourcePath);
        const retained={apiVersion:2,requestId:r.requestId,instanceId:'instance',documentId:owner.documentId,
          contentRevision:0,resourceCount:0,viewportAvailable:false};
        install('/other.json');return retained;
      }
      if(r.sourcePath==='/pending.json')throw {code:'replacement_pending',message:'Pending',
        details:{disposition:'pending',retryExecution:false}};
      if(r.sourcePath==='/unknown.json'){install(r.sourcePath);throw {code:'replacement_indeterminate',message:'Unknown',
        details:{disposition:'indeterminate',retryExecution:false}};}
      install(r.sourcePath);
      const receipt={apiVersion:2,requestId:r.requestId,instanceId:'instance',documentId:owner.documentId,
        contentRevision:0,resourceCount:0,viewportAvailable:false};
      if(r.sourcePath==='/bad-resource.json')receipt.resourceCount=2;
      if(r.sourcePath==='/bad-viewport.json')receipt.viewportAvailable=true;
      seen.set(r.requestId,{request:r,receipt});return receipt;
    }
    if(command==='nemo_native_object_reference'&&r.action.kind==='inspect'){
      if(r.documentId!==owner.documentId)throw Error('wrong inspected owner');
      return {...info(),requestId:r.requestId};
    }
    if(command==='nemo_native_object_preview'){
      previews.push(r);assert.equal(r.documentId,owner.documentId);
      if(failure&&failure.kind==='preview'&&r.documentId===failure.documentId)return failure.value;
      if(failure&&failure.kind==='owner-after-preview'&&r.documentId===failure.documentId){
        install('/other.json');return {status:'presented',workId:'stale-frame',viewGeneration:++viewGeneration};
      }
      if(failure&&failure.kind==='gate'&&r.documentId===failure.documentId){
        const gate=failure.gates.shift();if(gate)return gate.promise;
      }
      return {status:'presented',workId:'work-'+(++viewGeneration),viewGeneration};
    }
    if(command==='nemo_native_object_reference'||command==='nemo_native_object_dispatch'){
      assert.equal(r.documentId,owner.documentId);assert.equal(r.expectedRevision,owner.revision);
      owner.revision++;return {apiVersion:2,requestId:r.requestId,instanceId:'instance',
        documentId:owner.documentId,contentRevision:owner.revision,ok:true,result:{applied:true}};
    }
    if(command==='nemo_native_object_save_as'){
      copies.push(r);return {apiVersion:2,requestId:r.requestId,instanceId:r.instanceId,documentId:r.documentId,
        atRevision:r.atRevision,documentSnapshotId:r.documentSnapshotId,byteCount:info().byteCount,published:true,cleanupPending:false};
    }
    throw Error('Unexpected command: '+command);
  }
  return {invoke,calls,copies,previews,info,status,fail(value){failure=value;}};
}

test('replacement publishes B only after both current frame-0 receipts and post-reveal layout mapping',async()=>{
  const h=replacementHost(),first=deferred(),final=deferred(),css={width:320,height:180},s=Project.create({invoke:h.invoke,
    mapping:info=>({cssBounds:{x:0,y:0,width:css.width,height:css.height},
      physicalExtent:{width:css.width,height:css.height},compositionExtent:{width:info.canvasW,height:info.canvasH},reportedDpr:1}),frame:()=>0});
  const effects=[],f=files(s,{occlude:()=>effects.push('occlude'),reveal:async info=>{css.width=info.canvasW;css.height=info.canvasH;effects.push('reveal');},
    opened:path=>{effects.push(path);return true;}});
  assert.equal(await f.ui.openPath('/a.json'),true);
  h.fail({kind:'gate',documentId:'document-2',gates:[first,final]});
  const opening=f.ui.openPath('/b.json');await turn();
  assert.deepEqual(effects,['occlude','reveal','/a.json','occlude']);assert.equal(h.previews.length,3);
  assert.equal(h.previews[2].outputSpec.width,640);assert.equal(h.previews[2].viewport.compositionExtent.width,640);
  assert.equal(h.previews[2].viewport.cssBounds.width,320,'first B receipt precedes B UI layout');
  first.resolve({status:'presented',workId:'b-first',viewGeneration:2});await turn();
  assert.deepEqual(effects,['occlude','reveal','/a.json','occlude','reveal']);assert.equal(h.previews.length,4);
  assert.equal(h.previews[3].viewport.cssBounds.width,640,'final B receipt uses post-reveal layout');
  assert.equal(f.ui.isDirty(),true,'B is not a clean published file before final presentation');
  final.resolve({status:'presented',workId:'b-final',viewGeneration:3});
  assert.equal(await opening,true);assert.deepEqual(effects.at(-1),'/b.json');assert.equal(f.ui.isDirty(),false);
  assert.equal(s.info().documentId,'document-2');assert.equal(s.readPresentedObjectSession().presentation.workId,'b-final');
});

test('dirty cancellation only verifies native ownership; typed invalid replacement preserves A',async()=>{
  const h=replacementHost(),s=session(h),f=files(s,{confirmReplace:async()=>false});
  assert.equal(await f.ui.openPath('/a.json'),true);await s.reference({kind:'remove'});
  const count=h.calls.length;assert.equal(await f.ui.openPath('/b.json'),false);
  assert.deepEqual(h.calls.slice(count).map(c=>c.command),['nemo_native_object_client_status','nemo_native_object_reference']);
  assert.equal(h.calls.at(-1).r.action.kind,'inspect');
  assert.equal(s.info().documentId,'document-1');assert.equal(f.ui.isDirty(),true);
  const previews=h.previews.length,g=files(s);assert.equal(await g.ui.openPath('/bad.json'),false);
  assert.equal(s.isActive(),true);assert.equal(s.info().documentId,'document-1');
  assert.equal(h.previews.length,previews,'no B frame is presented on preflight refusal');
});

test('post-reservation uncertainty fences A and never publishes B path or saved baseline',async()=>{
  const h=replacementHost(),s=session(h),f=files(s);assert.equal(await f.ui.openPath('/a.json'),true);
  assert.equal(await f.ui.openPath('/unknown.json'),false);
  assert.equal(s.isActive(),false);assert.equal(s.blocksLegacy(),true);assert.equal(s.readPresentedObjectSession(),null);
  assert.equal(f.ui.isDirty(),true);assert.deepEqual(f.effects,[['open','/a.json']]);
  await assert.rejects(s.reference({kind:'remove'}));
});

test('replacement pending and deferred frame cannot restore A or publish B',async()=>{
  const h=replacementHost(),s=session(h),f=files(s);await f.ui.openPath('/a.json');
  assert.equal(await f.ui.openPath('/pending.json'),false);assert.equal(s.isActive(),false);
  const other=replacementHost(),t=session(other),g=files(t);await g.ui.openPath('/a.json');
  other.fail({kind:'preview',documentId:'document-2',value:{status:'deferred-occluded'}});
  assert.equal(await g.ui.openPath('/b.json'),false);assert.equal(t.isActive(),false);
  assert.deepEqual(g.effects,[['open','/a.json']]);
});

test('production Project Open retains inactive B metadata while native A stays active',async()=>{
  const h=replacementHost(),app=ordinary(h);
  assert.equal(await app.project.openPath('/a.json'),true);
  const tab=app.element('project-tabs-list').children[0],before=h.previews.length;
  assert.equal((await app.project.openPath('/b.json')).status,'prepared');
  assert.equal(app.element('project-tabs-list').children.length,2);
  assert.equal(app.element('project-tabs-list').children[0].dataset.tab,tab.dataset.tab);
  assert.equal(app.project.getCurrentLabel(),'a — /a.json');assert.equal(app.project.isDirty(),false);
  assert.equal(h.previews.length,before,'inactive B is metadata-only and never presented');
  assert.equal(h.calls.filter(c=>c.command==='nemo_native_object_inactive_project'&&c.r.action.kind==='prepare').length,1);
  assert.equal(app.mutations,0);
  assert.ok(app.writes.every(([key])=>key==='nemo-recents'),'no byte autosave or JS project authority');
});

test('ordinary second Open prepares inactive B without presenting or waiting for replacement frames',async()=>{
  const h=replacementHost(),app=ordinary(h,true);
  const first=app.project.openPath('/a.json');await turn();
  assert.equal(app.pendingFrames,1);app.flushFrame();await turn();
  assert.equal(app.pendingFrames,1);app.flushFrame();assert.equal(await first,true);
  const before=h.previews.length;
  assert.equal((await app.project.openPath('/b.json')).status,'prepared');
  assert.equal(h.previews.length,before,'inactive B does not request a preview frame');
  assert.equal(app.pendingFrames,0);assert.equal(app.project.getCurrentLabel(),'a — /a.json');
  assert.equal(app.element('project-tabs-list').children.length,2);
});

test('a second same-tick initial Open is denied before inactive or legacy routing',async()=>{
  const h=replacementHost(),app=ordinary(h);
  const first=app.project.openPath('/a.json');
  assert.equal(Project.desktop(app.window).blocksLegacy(),true,'queued Open denies old writers immediately');
  const blockedSave=app.project.saveAs();
  assert.equal(await app.project.openPath('/b.json'),false);
  assert.equal((await blockedSave).status,'open-in-progress');
  assert.match(app.toasts.at(-1),/Open is unavailable/);
  assert.equal(await first,true);
  assert.equal(h.calls.filter(c=>c.command==='nemo_native_object_bootstrap').length,1);
  assert.equal(app.project.getCurrentLabel(),'a — /a.json');assert.equal(app.mutations,0);
  assert.equal(h.copies.length,0);assert.equal(app.writes.filter(([key])=>key!=='nemo-recents').length,0);
});

test('Open is refused throughout Save As picker/ack, then uses the current acknowledged owner',async()=>{
  const h=replacementHost(),s=session(h),picker=deferred(),f=files(s,{pick:()=>picker.promise});
  assert.equal(await f.ui.openPath('/a.json'),true);
  const saving=f.ui.saveAs();await turn();
  const count=h.calls.filter(c=>c.command==='nemo_native_object_bootstrap').length;
  assert.equal(await f.ui.openPath('/b.json'),false);
  assert.equal(h.calls.filter(c=>c.command==='nemo_native_object_bootstrap').length,count);
  picker.resolve('/saved-a.json');assert.equal((await saving).acknowledged,true);
  assert.equal(await f.ui.openPath('/b.json'),true);assert.equal(s.info().documentId,'document-2');
});

test('actions submitted after Open begins cannot mutate B through a queued A request',async()=>{
  const h=replacementHost(),first=deferred(),s=session(h),f=files(s);await f.ui.openPath('/a.json');
  h.fail({kind:'gate',documentId:'document-2',gates:[first]});
  const opening=f.ui.openPath('/b.json');await turn();
  const before=h.calls.length;
  await assert.rejects(s.reference({kind:'remove'}),/Open is in progress/);
  assert.equal(await s.renderPreview(10),null);
  assert.equal(h.calls.length,before);
  first.resolve({status:'presented',workId:'b-first',viewGeneration:2});
  assert.equal(await opening,true);
  assert.equal(h.calls.filter(c=>c.command==='nemo_native_object_reference'&&c.r.action.kind==='remove').length,0);
});

test('ordinary picker adds inactive B; recents never activate it or replace A',async()=>{
  const h=replacementHost(),app=ordinary(h);await app.project.openPath('/a.json');
  const tab=app.element('project-tabs-list').children[0].dataset.tab;
  app.window.__TAURI__.dialog.open=async()=>'/b.json';
  app.window.__TAURI__.window={getCurrentWindow:()=>({setFocus:async()=>{}})};
  await app.project.open();assert.equal(app.project.getCurrentLabel(),'a — /a.json');
  assert.equal(app.element('project-tabs-list').children.length,2);
  const rows=app.element('start-recent-list').children;assert.equal(rows.length,1,'inactive B is not promoted into active-project recents');
  rows[0].listeners.click();await turn();
  assert.equal(app.project.getCurrentLabel(),'a — /a.json');
  assert.equal(app.element('project-tabs-list').children[0].dataset.tab,tab);
  assert.equal(app.element('project-tabs-list').children.length,2);
  assert.equal(app.mutations,0);assert.equal(h.calls.filter(c=>c.command==='nemo_native_object_bootstrap').length,1);
});

test('ordinary dirty Open prepares inactive B without replacing or confirming dirty A',async()=>{
  const h=replacementHost(),app=ordinary(h);await app.project.openPath('/a.json');
  await Project.desktop(app.window).reference({kind:'remove'});
  app.window.__TAURI__.dialog.ask=async()=>{throw Error('dirty A must not be replaced');};
  assert.equal((await app.project.openPath('/b.json')).status,'prepared');
  assert.equal(app.project.getCurrentLabel(),'a — /a.json');
  assert.equal(app.project.isDirty(),true);
  assert.equal(app.element('project-tabs-list').children.length,2);
  assert.equal(h.calls.filter(c=>c.command==='nemo_native_object_inactive_project'&&c.r.action.kind==='prepare').length,1);
});

test('preflight refusal with foreign owner drift fences instead of restoring A',async()=>{
  const h=replacementHost(),s=session(h),f=files(s);await f.ui.openPath('/a.json');
  assert.equal(await f.ui.openPath('/drift.json'),false);
  assert.equal(s.isActive(),false);assert.equal(s.blocksLegacy(),true);
  assert.equal(s.readPresentedObjectSession(),null);assert.equal(f.ui.isDirty(),true);
  assert.deepEqual(f.effects,[['open','/a.json']]);
});

test('stale bootstrap receipt and zero viewport fence without B publication',async()=>{
  const h=replacementHost(),s=session(h),f=files(s);await f.ui.openPath('/a.json');
  assert.equal(await f.ui.openPath('/stale.json'),false);assert.equal(s.isActive(),false);
  assert.deepEqual(f.effects,[['open','/a.json']]);
  const other=replacementHost();let width=320;
  const t=Project.create({invoke:other.invoke,mapping:info=>({cssBounds:{x:0,y:0,width,height:180},
    physicalExtent:{width:Math.max(1,width),height:180},compositionExtent:{width:info.canvasW,height:info.canvasH},reportedDpr:1}),frame:()=>0});
  const g=files(t);await g.ui.openPath('/a.json');width=0;
  assert.equal(await g.ui.openPath('/b.json'),false);assert.equal(t.isActive(),false);
  assert.deepEqual(g.effects,[['open','/a.json']]);
});

test('a retained B receipt cannot publish after native owner advances to C',async()=>{
  const h=replacementHost(),s=session(h),f=files(s);await f.ui.openPath('/a.json');
  assert.equal(await f.ui.openPath('/replayed.json'),false);
  assert.equal(s.isActive(),false);assert.equal(s.blocksLegacy(),true);
  assert.equal(s.readPresentedObjectSession(),null);assert.equal(f.ui.isDirty(),true);
  assert.deepEqual(f.effects,[['open','/a.json']]);
  assert.equal(h.previews.length,2,'no B frame reached presentation');
});

for(const path of ['/bad-resource.json','/bad-viewport.json']){
  test('malformed B bootstrap receipt '+path+' fences before presentation',async()=>{
    const h=replacementHost(),s=session(h),f=files(s);await f.ui.openPath('/a.json');
    assert.equal(await f.ui.openPath(path),false);
    assert.equal(s.isActive(),false);assert.equal(s.readPresentedObjectSession(),null);
    assert.deepEqual(f.effects,[['open','/a.json']]);assert.equal(h.previews.length,2);
  });
}

test('a presented B frame is rejected when the native owner changes before the status gate',async()=>{
  const h=replacementHost(),s=session(h),f=files(s);await f.ui.openPath('/a.json');
  h.fail({kind:'owner-after-preview',documentId:'document-2'});
  assert.equal(await f.ui.openPath('/b.json'),false);
  assert.equal(s.isActive(),false);assert.equal(s.readPresentedObjectSession(),null);
  assert.deepEqual(f.effects,[['open','/a.json']]);
  assert.equal(h.previews.length,3,'post-frame status stopped the second B presentation');
});

test('a deferred final B frame after transient reveal cannot publish B',async()=>{
  const h=replacementHost(),s=session(h),f=files(s);await f.ui.openPath('/a.json');
  const final=deferred();h.fail({kind:'gate',documentId:'document-2',gates:[null,final]});
  const opening=f.ui.openPath('/b.json');await turn();
  assert.equal(h.previews.length,4);assert.deepEqual(f.effects,[['open','/a.json']]);
  final.resolve({status:'deferred-occluded'});
  assert.equal(await opening,false);assert.equal(s.isActive(),false);assert.equal(f.ui.isDirty(),true);
  assert.deepEqual(f.effects,[['open','/a.json']]);
});

test('unavailable dirty-confirm dialog is irrelevant to metadata-only inactive B preparation',async()=>{
  const h=replacementHost(),app=ordinary(h);await app.project.openPath('/a.json');
  await Project.desktop(app.window).reference({kind:'remove'});
  app.window.__TAURI__.dialog.ask=async()=>{throw Error('Dialog unavailable');};
  let asked=false;app.window.__TAURI__.dialog.ask=async()=>{asked=true;throw Error('Dialog unavailable');};
  assert.equal((await app.project.openPath('/b.json')).status,'prepared');
  assert.equal(asked,false);
  assert.equal(Project.desktop(app.window).isActive(),true);
  assert.equal(app.project.getCurrentLabel(),'a — /a.json');
  assert.equal(app.element('project-tabs-list').children.length,2);
  assert.equal(app.project.isDirty(),true);
  assert.equal(app.startScreen.classList.contains('hid'),true);
});


test('bounded native sequence submits ordered source frames and confirms the full manifest',async()=>{
  const h=host(),invoke=h.invoke,progress=[];
  h.invoke=async(command,args)=>{const value=await invoke(command,args);
    if(command==='nemo_native_object_export')value.artifactFiles=['frame_0001.png','frame_0002.png'];
    return value;};
  const s=await presented(h),before=JSON.stringify(s.info());
  const result=await s.exportPng('/sequence',[0,10],(...args)=>progress.push(args));
  const request=h.calls.find(c=>c.command==='nemo_native_object_export').r;
  assert.deepEqual(request.frames,[0,10]);assert.equal(request.frame,0);
  assert.deepEqual(result.artifactFiles,['frame_0001.png','frame_0002.png']);
  assert.deepEqual(progress,[[2,2]]);assert.equal(JSON.stringify(s.info()),before);
});

for(const frames of [null,[],[0,0],[1,0],[0,21],[-1],[0,0.5],Array.from({length:33},(_,i)=>i)]){
  test('invalid sequence range '+JSON.stringify(frames)+' never dispatches',async()=>{
    const h=host(),s=await presented(h),before=JSON.stringify(s.info());
    await assert.rejects(s.exportPng('/invalid',frames),/frame range is invalid/);
    assert.equal(h.calls.filter(c=>c.command==='nemo_native_object_export').length,0);
    assert.equal(JSON.stringify(s.info()),before);
  });
}
test('sequence requires exact ordered manifest and retains submitted frame range across queue wait',async()=>{
  for(const manifest of [[],['frame_0001.png'],['frame_0002.png','frame_0001.png'],['frame_0001.png','frame_0001.png'],['frame_0001.png','other.png']]){
    const h=host(),invoke=h.invoke,progress=[];
    h.invoke=async(command,args)=>{const v=await invoke(command,args);if(command==='nemo_native_object_export')v.artifactFiles=manifest;return v;};
    const s=await presented(h);await assert.rejects(s.exportPng('/bad',[0,10],(...args)=>progress.push(args)),/publication is unconfirmed/);
    assert.deepEqual(progress,[]);assert.equal(s.blocksLegacy(),true);
  }
  const h=host(),invoke=h.invoke,first=deferred();let exports=0;
  h.invoke=async(command,args)=>{const v=await invoke(command,args);
    if(command==='nemo_native_object_export'){
      if(++exports===1)await first.promise;
      else v.artifactFiles=['frame_0001.png','frame_0002.png'];
    }return v;};
  const s=await presented(h),one=s.exportPng('/one',[0]),frames=[0,10],two=s.exportPng('/two',frames);
  frames[1]=20;await turn();assert.equal(exports,1);first.resolve();await Promise.all([one,two]);
  assert.deepEqual(h.calls.filter(c=>c.command==='nemo_native_object_export')[1].r.frames,[0,10]);
});

test('inclusive 32-frame bound is admitted and pre-cancelled sequences never dispatch',async()=>{
  const h=host(),invoke=h.invoke;
  h.invoke=async(command,args)=>{const v=await invoke(command,args);
    if(v&&'totalFrames' in v)v.totalFrames=64;
    if(command==='nemo_native_object_export')v.artifactFiles=Array.from({length:32},(_,i)=>'frame_'+String(i+1).padStart(4,'0')+'.png');
    return v;};
  const s=await presented(h),frames=Array.from({length:32},(_,i)=>i),progress=[];
  assert.equal((await s.exportPng('/bound',frames,(...v)=>progress.push(v))).artifactFiles.length,32);
  assert.deepEqual(progress,[[32,32]]);
  assert.deepEqual(await s.exportPng('/cancel',[0,1],()=>assert.fail('cancelled progress'),{aborted:true}),{status:'cancelled'});
  assert.equal(h.calls.filter(c=>c.command==='nemo_native_object_export').length,1);
});

function svgHost(){
  const h=host(),invoke=h.invoke;
  h.invoke=async(command,args)=>{const value=await invoke(command,args);
    if(command==='nemo_native_object_export'&&JSON.parse(args.requestJson).format==='svg'){
      const request=JSON.parse(args.requestJson),frames=request.frames||[request.frame];
      value.artifactFiles=frames.map((_,i)=>'frame_'+String(i+1).padStart(4,'0')+'.svg');
    }return value;};
  return h;
}
test('SVG shares the real Project queue and pin while PNG keeps its omitted format',async()=>{
  const h=svgHost(),invoke=h.invoke,first=deferred();let exports=0;
  h.invoke=async(command,args)=>{const value=await invoke(command,args);
    if(command==='nemo_native_object_export'&&++exports===1)await first.promise;return value;};
  const s=await presented(h),before=JSON.stringify(s.info()),progress=[],frames=[0,10];
  const png=s.exportPng('/png',[0]),svg=s.exportSvg('/svg',frames,(...v)=>progress.push(v));
  frames[1]=20;await turn();assert.equal(exports,1);assert.deepEqual(progress,[]);
  first.resolve();const [one,two]=await Promise.all([png,svg]);
  const requests=h.calls.filter(c=>c.command==='nemo_native_object_export').map(c=>c.r);
  assert.equal(Object.hasOwn(requests[0],'format'),false);
  assert.equal(requests[1].format,'svg');assert.deepEqual(requests[1].frames,[0,10]);
  assert.equal(requests[1].frame,0);assert.notEqual(one.requestId,two.requestId);
  assert.deepEqual(two.artifactFiles,['frame_0001.svg','frame_0002.svg']);
  for(const request of requests){assert.equal(request.instanceId,'instance');assert.equal(request.documentId,'document');
    assert.equal(request.contentRevision,0);assert.equal(request.documentSnapshotId,'snapshot-0');
    assert.equal(request.lifecycleGeneration,1);assert.equal(request.contextId,'scene-root');
    assert.equal(request.maximumEncodedBytes,4*1024*1024);}
  assert.deepEqual(progress,[[2,2]]);assert.equal(JSON.stringify(s.info()),before);
});
test('SVG rejects wrong extension, count, order and every terminal receipt correlation drift',async()=>{
  const invalid=[{artifactFiles:['frame_0001.png','frame_0002.png']},
    {artifactFiles:['frame_0002.svg','frame_0001.svg']},{artifactFiles:['frame_0001.svg']},
    {artifactFiles:['frame_0001.svg','frame_0001.svg']},{artifactFiles:['../frame_0001.svg','frame_0002.svg']},
    {requestId:'foreign'},{instanceId:'foreign'},{documentId:'foreign'},{contentRevision:1},
    {documentSnapshotId:'stale'},{status:'running'},{cleanupComplete:false},{published:false},
    {cleanupStatus:'pending'},{externalEffectDisposition:'indeterminate'},{errorCode:'publish_failed'}];
  for(const patch of invalid){const h=svgHost(),invoke=h.invoke,progress=[];
    h.invoke=async(command,args)=>{const value=await invoke(command,args);return command==='nemo_native_object_export'?{...value,...patch}:value;};
    const s=await presented(h),before=JSON.stringify(s.info());
    await assert.rejects(s.exportSvg('/svg',[0,10],(...v)=>progress.push(v)),/publication is unconfirmed/);
    assert.deepEqual(progress,[]);assert.equal(JSON.stringify(s.info()),before);
    assert.equal(h.calls.filter(c=>c.command==='nemo_native_object_export').length,1);
  }
});
test('SVG range bounds and predispatch cancellation never invoke a legacy or native writer',async()=>{
  const h=svgHost(),s=await presented(h),before=JSON.stringify(s.info());
  for(const frames of [null,[],[0,0],[1,0],[0,21],[-1],[0,0.5],Array.from({length:33},(_,i)=>i)])
    await assert.rejects(s.exportSvg('/invalid',frames),/frame range is invalid/);
  assert.deepEqual(await s.exportSvg('/cancel',[0],()=>assert.fail('no cancelled progress'),{aborted:true}),{status:'cancelled'});
  assert.equal(h.calls.filter(c=>c.command==='nemo_native_object_export').length,0);
  assert.equal(JSON.stringify(s.info()),before);
});
test('SVG sees current native history revision and refuses submission once Open begins',async()=>{
  const h=svgHost(),s=await presented(h);
  await s.reference({kind:'attach',id:'ref',sourcePath:'/image.png'});
  await s.exportSvg('/after-edit',[0]);
  const request=h.calls.find(c=>c.command==='nemo_native_object_export').r;
  assert.equal(request.contentRevision,1);assert.equal(request.documentSnapshotId,'snapshot-1');
  const blocked=svgHost(),invoke=blocked.invoke,first=deferred();let waiting=false;
  blocked.invoke=async(command,args)=>{const value=await invoke(command,args);
    if(command==='nemo_native_object_export'&&!waiting){waiting=true;await first.promise;}
    return value;};
  const session=await presented(blocked),one=session.exportPng('/first',[0]);
  await turn();const opening=session.openPath('/other.json',{confirm:async()=>false}),two=session.exportSvg('/stale',[0]);
  await assert.rejects(two,/Open is in progress/);first.resolve();await one;assert.equal(await opening,false);
  assert.equal(blocked.calls.filter(c=>c.command==='nemo_native_object_export').length,1);
  assert.equal(blocked.calls.filter(c=>c.command==='nemo_native_object_bootstrap').length,1);
  assert.equal(session.isActive(),true);assert.equal(session.info().documentId,'document');
});

test('Camera View uses native projection proof while PNG and SVG exports never inherit the session selector',async()=>{
 const h=host(),original=h.invoke;let mode='canvas';
 const view={mode:'stored-camera',cameraAvailable:true,documentToComposition:[2,0,0,2,-160,-90],compositionToDocument:[.5,0,0,.5,80,45]};
 h.invoke=async(command,args)=>{const result=await original(command,args),q=args&&JSON.parse(args.requestJson);
  if(command==='nemo_native_object_export'&&q.format==='svg')return {...result,artifactFiles:['frame_0001.svg']};
  return command==='nemo_native_object_preview'&&q.viewMode==='stored-camera'?{...result,view:structuredClone(view)}:result;};
 const s=Project.create({invoke:h.invoke,mapping:info=>({cssBounds:{width:320,height:180},physicalExtent:{width:320,height:180},compositionExtent:{width:info.canvasW,height:info.canvasH},reportedDpr:1}),frame:()=>0,viewMode:()=>mode});
 await s.openPath('/project.json');const before=JSON.stringify(s.info());const canvas=s.readPresentedObjectSession().presentation;
 assert.equal(canvas.view.mode,'canvas');mode='stored-camera';assert.equal(s.readPresentedObjectSession(),null);
 const camera=await s.presentPreview(0);assert.deepEqual(camera.view,view);assert.notEqual(camera,canvas);assert(Object.isFrozen(camera.view.documentToComposition));
 await s.exportPng('/camera-png',[0]);await s.exportSvg('/camera-svg',[0]);
 for(const {r} of h.calls.filter(c=>c.command==='nemo_native_object_export'))assert.equal(Object.hasOwn(r,'viewMode'),false);
 assert.equal(JSON.stringify(s.info()),before);mode='canvas';assert.equal(s.readPresentedObjectSession(),null);
 assert.equal((await s.presentPreview(0)).view.mode,'canvas');
});

test('Camera View refuses missing, foreign, singular or inconsistent native mapping and stale async choice',async()=>{
 for(const view of [undefined,{mode:'canvas',cameraAvailable:true,documentToComposition:[1,0,0,1,0,0],compositionToDocument:[1,0,0,1,0,0]},
  {mode:'stored-camera',cameraAvailable:true,documentToComposition:[0,0,0,0,0,0],compositionToDocument:[1,0,0,1,0,0]},
  {mode:'stored-camera',cameraAvailable:true,documentToComposition:[2,0,0,2,-160,-90],compositionToDocument:[1,0,0,1,0,0]}]){
  const h=host(),invoke=h.invoke;let mode='canvas';h.invoke=async(command,args)=>{const r=await invoke(command,args);return command==='nemo_native_object_preview'&&mode==='stored-camera'?{...r,view}:r;};
  const s=Project.create({invoke:h.invoke,mapping:()=>({cssBounds:{width:320,height:180},physicalExtent:{width:320,height:180}}),viewMode:()=>mode});
  await s.openPath('/project.json');mode='stored-camera';await assert.rejects(s.presentPreview(0));assert.equal(s.readPresentedObjectSession(),null);assert.equal(s.info().contentRevision,0);
 }
 const h=host(),invoke=h.invoke,wait=deferred();let mode='canvas',hold=false;
 h.invoke=async(command,args)=>{const r=await invoke(command,args);if(command==='nemo_native_object_preview'&&hold){await wait.promise;return {...r,view:{mode:'stored-camera',cameraAvailable:true,documentToComposition:[2,0,0,2,-160,-90],compositionToDocument:[.5,0,0,.5,80,45]}};}return r;};
 const s=Project.create({invoke:h.invoke,mapping:()=>({cssBounds:{width:320,height:180},physicalExtent:{width:320,height:180}}),viewMode:()=>mode});
 await s.openPath('/project.json');mode='stored-camera';hold=true;const pending=s.presentPreview(0);await turn();mode='canvas';wait.resolve();await assert.rejects(pending);assert.equal(s.readPresentedObjectSession(),null);
});

test('actual native camera toolbar click uses current receipt availability and never the legacy camera writer',()=>{
 const {extractFunction}=require('./fixtures/lib/sandbox.cjs'),source=fs.readFileSync(require.resolve('../src/js/camera.js'),'utf8');
 const events=[],button={hidden:false,setAttribute(k,v){this[k]=v;},addEventListener(k,fn){this[k]=fn;}};
 let available=true;const state={cameraView:false,currentFrame:4};
 Object.defineProperties(state,{cameraLayerOn:{get(){throw Error('legacy camera layer read');}},cameraKeys:{get(){throw Error('legacy camera key read');}}});
 const session={isActive:()=>true,readPresentedObjectSession:()=>({presentation:{view:{cameraAvailable:available}}}),renderPreview:frame=>events.push(frame)};
 const root={state,document:{getElementById:id=>id==='btn-camera'?button:null},SM:{t:k=>k},NemoNativeObjectProject:{desktop:()=>session},
  NemoNativeObjectProjectDesktop:require('../src/js/adapters/native-object-project-desktop.js')};
 root.window=root;const context=vm.createContext(root);
 for(const name of ['nativeSession','renderNativeViewLock','applyCameraView','initUI'])vm.runInContext(extractFunction(source,name),context);
 vm.runInContext('initUI();renderNativeViewLock();',context);assert.equal(button.hidden,false);assert.equal(button['aria-pressed'],'false');
 button.click();assert.equal(state.cameraView,true);assert.deepEqual(events,[4]);assert.equal(button['aria-pressed'],'true');
 available=false;vm.runInContext('renderNativeViewLock()',context);assert.equal(button.hidden,true);button.click();assert.deepEqual(events,[4]);
 available=true;button.click();assert.equal(state.cameraView,false);assert.deepEqual(events,[4,4]);
});

test('session view change invalidates the original pending action lease before another native paint',async()=>{
 const h=host(),entered=deferred(),release=deferred();let mode='canvas';
 const s=Project.create({invoke:h.invoke,mapping:()=>({cssBounds:{width:320,height:180},physicalExtent:{width:320,height:180}}),projectionReady:()=>true,viewMode:()=>mode});
 await s.openPath('/project.json');const original=s.readPresentedObjectSession().presentation;
 const action=s.runResourceCommand(async ctx=>{assert.equal(ctx.readPresentedObjectSession().presentation,original);entered.resolve();await release.promise;assert.equal(ctx.readPresentedObjectSession(),null);});
 await entered.promise;mode='stored-camera';assert.equal(s.readPresentedObjectSession(),null);release.resolve();await action;
 assert.equal(s.info().contentRevision,0);assert.equal(h.calls.some(c=>c.command==='nemo_native_dispatch'),false);
 const inspect=require('../src/js/adapters/native-object-open-exposure.js').inspectReceipt;
 const dormant={status:'presented',workId:'native-dormant',viewGeneration:1,view:{mode:'stored-camera',cameraAvailable:false,documentToComposition:[1,0,0,1,0,0],compositionToDocument:[1,0,0,1,0,0]}};
 assert.equal(inspect(dormant,true,false,'stored-camera').failure,null);
});

function navigationFixture(initialFilter='none'){
 const h=host(),invoke=h.invoke,hold=deferred();let filter=initialFilter,mode='canvas',frame=0,pause=false,tamper=null,rect={x:0,y:0,width:320,height:180};
 const literals=new Map([
  ['1:0,0',[[2,0,0,2,-160,-90],[.5,0,0,.5,80,45]]],
  ['1:10,-5',[[2,0,0,2,-180,-80],[.5,0,0,.5,90,40]]],
  ['1:5,-2.5',[[2,0,0,2,-170,-85],[.5,0,0,.5,85,42.5]]],
  ['2:10,-5',[[4,0,0,4,-520,-250],[.25,0,0,.25,130,62.5]]],
  ['2:-30,-15',[[4,0,0,4,-360,-210],[.25,0,0,.25,90,52.5]]],
 ]);
 h.invoke=async(command,args)=>{const result=await invoke(command,args),q=args&&JSON.parse(args.requestJson);
  if(command==='nemo_native_object_preview')result.displayFilter=q.displayFilter||'none';
  if(command!=='nemo_native_object_preview'||q.viewMode!=='stored-camera')return result;
  const n=q.navigation||{zoom:1,pan:[0,0]},pair=literals.get(n.zoom+':'+n.pan.join(','));assert.ok(pair,'independent authored navigation literal');
  if(pause)await hold.promise;
  const reply={...result,view:{mode:'stored-camera',navigation:n,cameraAvailable:true,documentToComposition:pair[0],compositionToDocument:pair[1]}};return tamper?tamper(reply):reply;};
 const s=Project.create({invoke:h.invoke,mapping:info=>({cssBounds:{...rect},physicalExtent:{width:320,height:180},compositionExtent:{width:info.canvasW,height:info.canvasH},reportedDpr:1}),frame:()=>frame,viewMode:()=>mode,displayFilter:()=>filter});
 return {h,s,async open(){await s.openPath('/project.json');mode='stored-camera';await s.presentPreview(frame);},set filter(value){filter=value;},set frame(value){frame=value;},set mode(value){mode=value;},set rect(value){rect=value;},pause(){pause=true;},release(){pause=false;hold.resolve();},tamper(fn){tamper=fn;}};
}
test('native session hand and zoom retain exact view intent through scrub while excluding export and source/history',async()=>{
 const f=navigationFixture();await f.open();const before=JSON.stringify(f.s.info());
 const hand=f.s.beginViewportNavigation('pan',{x:100,y:60});assert.ok(Object.isFrozen(hand));assert.ok(Object.isFrozen(hand.inverse));
 const pending=f.s.updateViewportNavigation(hand,{x:80,y:70});assert.equal(f.s.readPresentedObjectSession(),null);await pending;
 assert.deepEqual(f.s.readPresentedObjectSession().presentation.view.navigation,{zoom:1,pan:[10,-5]});
 const zoom=f.s.beginViewportNavigation('zoom',null);await f.s.updateViewportNavigation(zoom,null,2);
 assert.deepEqual(f.s.readPresentedObjectSession().presentation.view.documentToComposition,[4,0,0,4,-520,-250]);
 f.frame=1;await f.s.presentPreview(1);
 assert.deepEqual(f.s.readPresentedObjectSession().presentation.view.navigation,{zoom:2,pan:[10,-5]});
 await assert.rejects(f.s.updateViewportNavigation(zoom,null,2),/source changed/);
 await f.s.exportPng('/native-navigation',[0]);
 const exp=f.h.calls.filter(c=>c.command==='nemo_native_object_export').at(-1).r;
 assert.equal(Object.hasOwn(exp,'navigation'),false);assert.equal(Object.hasOwn(exp,'viewMode'),false);
 assert.equal(JSON.stringify(f.s.info()),before);
 f.mode='canvas';assert.equal(f.s.readPresentedObjectSession(),null);await f.s.renderPreview(1);
 f.mode='stored-camera';await f.s.renderPreview(1);
 assert.deepEqual(f.s.readPresentedObjectSession().presentation.view.navigation,{zoom:1,pan:[0,0]});
});
test('native navigation anchors the actual inverse and coalesces rapid input without publishing an old tool proof',async()=>{
 const f=navigationFixture();await f.open();const initial=f.s.readPresentedObjectSession().presentation;
 const zoom=f.s.beginViewportNavigation('zoom',{x:40,y:30});await f.s.updateViewportNavigation(zoom,{x:40,y:30},2);
 assert.deepEqual(f.s.readPresentedObjectSession().presentation.view.navigation,{zoom:2,pan:[-30,-15]});
 const inverse=f.s.readPresentedObjectSession().presentation.view.compositionToDocument;
 assert.deepEqual([inverse[0]*40+inverse[4],inverse[3]*30+inverse[5]],[100,60]);
 f.mode='canvas';await f.s.renderPreview(0);f.mode='stored-camera';await f.s.renderPreview(0);
 const hand=f.s.beginViewportNavigation('pan',{x:100,y:60});f.pause();
 const first=f.s.updateViewportNavigation(hand,{x:90,y:65});await turn();
 const latest=f.s.updateViewportNavigation(hand,{x:80,y:70});assert.equal(f.s.readPresentedObjectSession(),null);
 f.release();await Promise.all([first,latest]);
 const requested=f.h.calls.filter(c=>c.command==='nemo_native_object_preview').slice(-2).map(c=>c.r.navigation);
 assert.deepEqual(requested,[{zoom:1,pan:[5,-2.5]},{zoom:1,pan:[10,-5]}]);
 const current=f.s.readPresentedObjectSession().presentation;assert.notEqual(current,initial);
 assert.deepEqual(current.view.navigation,{zoom:1,pan:[10,-5]});
 f.rect={x:1,y:0,width:320,height:180};await assert.rejects(f.s.updateViewportNavigation(hand,{x:79,y:70}),/bounds changed/);
 f.mode='canvas';assert.equal(f.s.beginViewportNavigation('pan',{x:1,y:1}),null);
});

test('native view receipt refuses mismatched or non-closed navigation without a tool proof or source edit',async()=>{
 for(const navigation of [null,undefined,{zoom:2,pan:[0,0]},{zoom:1,pan:[0,0],extra:true},{zoom:1,pan:[0,Infinity]}]){
  const f=navigationFixture();f.tamper(reply=>({...reply,view:{...reply.view,navigation}}));
  await assert.rejects(f.open());assert.equal(f.s.readPresentedObjectSession(),null);assert.equal(f.s.info().contentRevision,0);
 }
});

test('native first preview carries persisted filter; toggles preserve navigation and exclude export/document history',async()=>{
 const f=navigationFixture('values');await f.open();const before=JSON.stringify(f.s.info());
 assert.ok(f.h.calls.filter(c=>c.command==='nemo_native_object_preview').every(c=>c.r.displayFilter==='values'));
 const hand=f.s.beginViewportNavigation('pan',{x:100,y:60});await f.s.updateViewportNavigation(hand,{x:80,y:70});
 const view=f.s.readPresentedObjectSession().presentation.view;
 for(const filter of ['dim','none','values']){
  f.filter=filter;assert.equal(f.s.readPresentedObjectSession(),null);await f.s.renderPreview(0);
  const shown=f.s.readPresentedObjectSession().presentation;assert.equal(shown.displayFilter,filter);assert.deepEqual(shown.view,view);
 }
 await f.s.exportPng('/unfiltered',[0]);
 assert.equal(Object.hasOwn(f.h.calls.filter(c=>c.command==='nemo_native_object_export').at(-1).r,'displayFilter'),false);
 assert.equal(JSON.stringify(f.s.info()),before);
});

test('native rapid filter changes coalesce to the newest intent and never expose stale tool proof',async()=>{
 const f=navigationFixture();await f.open();f.pause();f.filter='values';
 const first=f.s.renderPreview(0);await turn();f.filter='dim';const latest=f.s.renderPreview(0);
 assert.equal(f.s.readPresentedObjectSession(),null);f.release();await Promise.all([first,latest]);
 assert.equal(f.s.readPresentedObjectSession().presentation.displayFilter,'dim');
 assert.deepEqual(f.h.calls.filter(c=>c.command==='nemo_native_object_preview').slice(-2).map(c=>c.r.displayFilter),['values','dim']);
});

test('native filter receipts reject wrong/missing/unknown values and late direct presentation',async()=>{
 for(const value of ['none','dim',undefined,null,'blur(2px)']){
  const f=navigationFixture();await f.open();f.filter='values';f.tamper(reply=>({...reply,displayFilter:value}));
  await assert.rejects(f.s.presentPreview(0),/display-filter/);assert.equal(f.s.readPresentedObjectSession(),null);
 }
 const f=navigationFixture();await f.open();f.pause();f.filter='values';
 const pending=f.s.presentPreview(0);await turn();f.filter='dim';f.release();
 await assert.rejects(pending,/owner/);assert.equal(f.s.readPresentedObjectSession(),null);
 await f.s.renderPreview(0);assert.equal(f.s.readPresentedObjectSession().presentation.displayFilter,'dim');
});
test('an edit presented while the window is not visible is no failure and re-presents on the next visible exposure',async()=>{
  // #2343 B1 check 3: a layer edit while NSWindow occlusion said not visible produced 'Native project frame was not
  // presented (status-deferred-occluded)' and a blank canvas. The edit stands; its frame waits for the exposure.
  const h=host(),notices=[],listeners=[];
  const s=Project.create({invoke:h.invoke,mapping:info=>({cssBounds:{x:0,y:0,width:320,height:180},
    physicalExtent:{width:320,height:180},compositionExtent:{width:info.canvasW,height:info.canvasH},reportedDpr:1}),frame:()=>0,
    notice:m=>notices.push(m),subscribeWindowOcclusion:async hint=>{listeners.push(hint);return {detach(){},close(){}};}});
  await s.openPath('/project.json');assert.ok(s.readPresentedObjectSession());
  const previews=()=>h.calls.filter(c=>c.command==='nemo_native_object_preview').length,before=previews();
  h.preview('deferred-occluded');await s.reference({kind:'remove'});
  assert.equal(previews(),before+1);assert.equal(s.readPresentedObjectSession(),null);assert.deepEqual(notices,[]);
  h.preview('presented');await turn();await turn();listeners.forEach(l=>l({visible:true}));
  for(let i=0;i<6;i++)await turn();
  assert.equal(previews(),before+2,'one re-present');assert.ok(s.readPresentedObjectSession(),'presented again');assert.deepEqual(notices,[]);
});
