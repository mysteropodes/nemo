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
    if(command==='nemo_native_object_export')return {...info(),requestId:r.requestId,published:true,cleanupComplete:true};
    throw Error('Unexpected command: '+command);
  }
  return {invoke,info,calls,copies,preview(value){preview=value;},fail(value){failure=value;}};
}
function session(h){return Project.create({invoke:h.invoke,mapping:info=>({cssBounds:{x:0,y:0,width:320,height:180},
  physicalExtent:{width:320,height:180},compositionExtent:{width:info.canvasW,height:info.canvasH},reportedDpr:1}),frame:()=>0});}
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
test('single-frame native export uses pinned host route; sequences do not fall back to Paper',async()=>{
  const h=host(),s=await presented(h);assert.equal((await s.exportPng('/new-output',[0])).published,true);
  await assert.rejects(s.exportPng('/new-output',[0,1]),/sequence export is unavailable/);
  const r=h.calls.find(c=>c.command==='nemo_native_object_export').r;
  assert.equal(r.documentSnapshotId,'snapshot-0');assert.equal(r.maximumEncodedBytes,4*1024*1024);
  assert.equal('reference' in r,false);
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
    root.NemoNativeObjectProject=Project;root.NemoNativeObjectProjectFile=Files;root.NemoNativeObjectSaveAsWorkflow=Workflow;root.NemoNativeObjectSavedAcknowledgement=Ack;
    root.NemoNativeObjectInactiveProjectTabs=require('../src/js/adapters/native-object-inactive-project-tabs.js');
    let id=0;root.crypto={randomUUID:()=>`inactive-ui-${++id}`};
    root.__TAURI__={core:{invoke:h.invoke},dialog:{save:async()=>'/ordinary-save.json'},fs:{readTextFile:()=>{throw Error('Project bytes crossed JS');}}};
  }});
}
test('complete production Project Open/Save As updates tab, recents and dirty state through native adapters',async()=>{
  const h=host(),app=ordinary(h);assert.equal(await app.project.openPath('/ordinary.json'),true);
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

test('dirty cancellation sends no native request; typed invalid replacement preserves A',async()=>{
  const h=replacementHost(),s=session(h),f=files(s,{confirmReplace:async()=>false});
  assert.equal(await f.ui.openPath('/a.json'),true);await s.reference({kind:'remove'});
  const count=h.calls.length;assert.equal(await f.ui.openPath('/b.json'),false);assert.equal(h.calls.length,count);
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
