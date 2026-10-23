'use strict';
const test=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const vm=require('node:vm');
const Project=require('../src/js/adapters/native-object-project.js');
const Files=require('../src/js/adapters/native-object-project-file.js');
const Workflow=require('../src/js/application/native-object-save-as-workflow.js');
const Ack=require('../src/js/application/native-object-saved-acknowledgement.js');
const InactiveTabs=require('../src/js/adapters/native-object-inactive-project-tabs.js');
const turn=()=>new Promise(setImmediate);
function deferred(){let resolve;const promise=new Promise(done=>{resolve=done;});return {promise,resolve};}
const intent={canvasW:640,canvasH:360,fps:30};
function host(){
  let installed=false,number=0,meta=null,inactive=null;
  const calls=[],events=[];
  const h={calls,events,before:null,after:null,info:()=>meta,inactive:()=>inactive};
  h.invoke=async(command,args)=>{
    const r=args?JSON.parse(args.requestJson):null;calls.push({command,r});
    if(h.before)await h.before(command,r);
    let result;
    if(command==='nemo_native_object_client_status')result={apiVersion:2,instanceId:'instance',available:installed,...meta};
    else if(command==='nemo_native_object_bootstrap'){
      if(installed)assert.deepEqual(r.replace,{documentId:meta.documentId,expectedRevision:meta.contentRevision,
        documentSnapshotId:meta.documentSnapshotId,lifecycleGeneration:meta.lifecycleGeneration});
      else assert.equal('replace' in r,false);
      const cfg=r.newProject||{canvasW:320,canvasH:180,fps:24};
      if(r.newProject){assert.deepEqual(Object.keys(r).sort(),['apiVersion','instanceId','newProject','requestId',...(installed?['replace']:[])].sort());
        assert.deepEqual(Object.keys(r.newProject).sort(),['canvasH','canvasW','fps']);}
      else{assert.equal(r.projectOnly,true);assert.equal(typeof r.sourcePath,'string');}
      installed=true;number++;inactive=null;
      meta={apiVersion:2,instanceId:'instance',documentId:'document-'+number,contentRevision:0,
        documentSnapshotId:'snapshot-'+number,lifecycleGeneration:number,documentDigest:String(number).repeat(64),byteCount:1000,
        ...cfg,totalFrames:Math.round(cfg.fps*5),layers:[{layerUid:'layer-1',name:'Layer 1',visible:true}],reference:null,undoDepth:0,redoDepth:0};
      result={apiVersion:2,requestId:r.requestId,instanceId:'instance',documentId:meta.documentId,
        contentRevision:0,resourceCount:0,viewportAvailable:false};
    }else if(command==='nemo_native_object_reference')result={...meta,requestId:r.requestId};
    else if(command==='nemo_native_object_preview'){events.push('present:'+meta.documentId);result={status:'presented',workId:'work-'+calls.length,viewGeneration:number};}
    else if(command==='nemo_native_object_save_as')result={apiVersion:2,requestId:r.requestId,instanceId:r.instanceId,documentId:r.documentId,
      atRevision:r.atRevision,documentSnapshotId:r.documentSnapshotId,byteCount:meta.byteCount,published:true,cleanupPending:false};
    else if(command==='nemo_native_object_inactive_project'){
      if(r.action.kind==='prepare')inactive={instanceId:r.instanceId,contentRevision:0,
        documentSnapshotId:'snapshot-B',documentDigest:'b'.repeat(64),byteCount:128,
        canvasW:320,canvasH:180,fps:24,totalFrames:2,layers:[{layerUid:'layer-B'}],
        preparationToken:'inactive-B',preparationRequestId:r.requestId,
        origin:{documentId:r.documentId,contentRevision:r.expectedRevision,documentSnapshotId:r.documentSnapshotId}};
      if(r.action.kind==='close')inactive=null;
      result={apiVersion:2,requestId:r.requestId,instanceId:r.instanceId,documentId:r.documentId,
        contentRevision:r.expectedRevision,documentSnapshotId:r.documentSnapshotId,lifecycleGeneration:r.lifecycleGeneration,
        state:r.action.kind==='prepare'?'inactive-prepared':r.action.kind==='close'?'inactive-closed':inactive?'inactive-retained':'inactive-absent',
        preparationToken:r.action.kind==='close'?r.action.preparationToken:inactive&&inactive.preparationToken,
        project:inactive,retryExecution:false};
    }else throw Error('Unexpected native route: '+command);
    return h.after?await h.after(command,r,result):result;
  };
  return h;
}
function session(h,extra={}){return Project.create({invoke:h.invoke,
  mapping:info=>({cssBounds:{width:640,height:360},physicalExtent:{width:640,height:360},compositionExtent:{width:info.canvasW,height:info.canvasH}}),...extra});}
function fileUI(s,extra={}){
  const effects=[];let name='Untitled',path=null;
  const ui=Files.create(s,{readUI:()=>({handle:'tab',generation:1}),confirmReplace:async()=>true,
    occlude:()=>effects.push('occlude'),reveal:()=>effects.push('reveal'),restore:()=>effects.push('restore'),
    opened:(p)=>{path=p;effects.push('open');return true;},created:n=>{name=n;path=null;effects.push('create');return true;},
    commit:p=>{path=p;effects.push('save');return true;},repaint:()=>{},pick:async()=>'/new.json',notice:()=>{},
    failed:()=>effects.push('failed'),refused:()=>effects.push('refused'),...extra},Workflow,Ack);
  return {ui,effects,meta:()=>({name,path})};
}
function ordinary(h,extra={}){
  const fixture=fs.readFileSync('tests/project-open.test.cjs','utf8');
  const start=fixture.indexOf('function harness('),end=fixture.indexOf('\nfunction select(',start);
  const code=fixture.slice(start,end).replace('configureWindow(window);',
    "window.state=context.state;window.document=document;window.userLayers=context.userLayers;document.getElementById('drawing-canvas').getBoundingClientRect=()=>({left:0,top:0,width:640,height:360});configureWindow(window);");
  const context=vm.createContext({assert,vm,require,setTimeout,console,source:fs.readFileSync('src/js/project.js','utf8'),
    documentSource:fs.readFileSync('src/js/project-document.js','utf8'),entrySource:fs.readFileSync('src/js/project-entry.js','utf8')});
  vm.runInContext(code+'\nthis.makeHarness=harness;',context);
  return context.makeHarness({dirtyDot:true,configureWindow(root){
    require('./helpers/native-viewport-underlay.cjs').install(root);
    root.NemoNativeObjectProject=Project;root.NemoNativeObjectProjectFile=Files;
    root.NemoNativeObjectSaveAsWorkflow=Workflow;root.NemoNativeObjectSavedAcknowledgement=Ack;
    root.NemoNativeOpacityCutover={blocksLegacy:()=>true,requestRelease:()=>assert.fail('legacy release')};
    root.crypto={randomUUID:require('node:crypto').randomUUID};
    // Match DOM click dispatch rather than the older harness's click marker.
    for(const id of ['project-tab-add','start-new'])root.document.getElementById(id).click=function(){return this.listeners.click();};
    root.SM.exportJSON=()=>assert.fail('legacy serialization');
    root.__TAURI__={core:{invoke:h.invoke},dialog:{save:async()=>'/ordinary-new.json',ask:async()=>true},
      fs:{readTextFile:()=>assert.fail('project bytes in JS'),writeTextFile:()=>assert.fail('legacy file write')}};
    Object.assign(root,extra);
  }});
}
function form(app){
  app.element('start-new').listeners.click();app.element('np-preset').value='640x360';
  app.element('np-fps').value='30';app.element('np-name').value='Named blank';
  return ()=>app.element('np-create').listeners.click();
}
function nativeNewEntry(app,route){
  if(route==='plus')return app.element('project-tab-add').click();
  // Execute the production burger entry and its real click delegate.
  const lines=fs.readFileSync('src/js/timeline.js','utf8').split('\n');
  const click=lines.find(line=>line.startsWith('function clickEl('));
  const entry=lines.find(line=>line.includes("label:tt('menuNewProject')"));
  assert.ok(click&&entry,'ordinary New menu entry exists');
  return vm.runInNewContext(click+'\n('+entry.trim().replace(/,$/,'')+').action()',
    {document:app.window.document,tt:key=>key});
}
function blank64(app){
  app.element('np-preset').value='custom';app.element('np-w').value='64';app.element('np-h').value='64';
  app.element('np-fps').value='24';app.element('np-name').value='PNG baseline';
  return ()=>app.element('np-create').listeners.click();
}
function observedInactive(h){
  let controller;
  const app=ordinary(h,{NemoNativeObjectInactiveProjectTabs:{create(root,ports){controller=InactiveTabs.create(root,ports);return controller;}}});
  return {app,tabs:()=>controller};
}
for(const route of ['plus','burger'])test('ordinary native '+route+' New opens the replacement form then submits exact64x64 intent',async()=>{
  const h=host(),app=ordinary(h);await app.project.openPath('/old.json');
  const beforeCalls=h.calls.length,beforeWrites=JSON.stringify(app.writes);
  nativeNewEntry(app,route);
  assert.equal(app.element('start-newpanel').style.display,'block');assert.equal(app.startScreen.classList.contains('hid'),false);
  assert.equal(h.calls.length,beforeCalls,'opening the form cannot replace the owner');
  assert.equal(app.project.getCurrentLabel(),'old — /old.json');assert.equal(app.element('project-tab-add').title,'newProjectHdr');
  assert.equal(app.element('project-tab-add').getAttribute('data-i18n-title'),'newProjectHdr');
  assert.ok(app.toasts.includes('Creating a new project replaces the current project.'));
  assert.equal(await blank64(app)(),true);
  const request=h.calls.filter(c=>c.command==='nemo_native_object_bootstrap').at(-1).r;
  assert.deepEqual(request.newProject,{canvasW:64,canvasH:64,fps:24});assert.equal(request.replace.documentId,'document-1');
  assert.equal(app.project.getCurrentLabel(),'PNG baseline (not saved)');assert.equal(app.project.isDirty(),true);
  assert.equal(app.element('project-tabs-list').children.length,1,'no second native tab is fabricated');
  assert.equal(app.mutations,0);assert.equal(JSON.stringify(app.writes),beforeWrites);
});
test('native New form Cancel and dirty confirmation decline preserve the owner without legacy work',async()=>{
  const h=host(),app=ordinary(h);await app.project.openPath('/old.json');
  let before=h.calls.filter(c=>c.command==='nemo_native_object_bootstrap').length;
  nativeNewEntry(app,'plus');app.element('np-cancel').listeners.click();
  assert.equal(h.calls.filter(c=>c.command==='nemo_native_object_bootstrap').length,before);
  assert.equal(app.project.getCurrentLabel(),'old — /old.json');
  assert.equal(app.startScreen.classList.contains('hid'),true,'safe Cancel restores the existing editor');
  assert.equal(app.element('start-newpanel').style.display,'none');
  assert.equal(Project.desktop(app.window).identity().documentId,'document-1');
  assert.equal(app.element('project-tabs-list').children.length,1);
  nativeNewEntry(app,'plus');await blank64(app)();const owner=h.info().documentId;
  let confirmations=0;app.window.__TAURI__.dialog.ask=async()=>{confirmations++;return false;};
  before=h.calls.filter(c=>c.command==='nemo_native_object_bootstrap').length;
  nativeNewEntry(app,'burger');assert.equal(await blank64(app)(),false);
  assert.equal(confirmations,1);assert.equal(h.info().documentId,owner);
  assert.equal(h.calls.filter(c=>c.command==='nemo_native_object_bootstrap').length,before);
  assert.equal(Project.desktop(app.window).isActive(),true);assert.equal(app.mutations,0);
});
test('original start-screen form Cancel retains its default presentation',()=>{
  const app=ordinary(host());form(app);app.element('np-cancel').listeners.click();
  assert.equal(app.startScreen.classList.contains('hid'),false);
  assert.equal(app.element('start-newpanel').style.display,'none');
  assert.equal(app.mutations,0);assert.equal(app.writes.length,0);
});
for(const drift of ['revision','snapshot','owner','missing-readiness'])test('native entry Cancel refuses '+drift+' drift without revealing an old editor',async()=>{
  const h=host(),app=ordinary(h);await app.project.openPath('/old.json');nativeNewEntry(app,'plus');
  const beforeWrites=JSON.stringify(app.writes),s=Project.desktop(app.window),old=s.identity(),gate=deferred();let pending;
  if(drift==='owner'){
    // Replace through File UI so its real projection/metadata publication runs.
    assert.equal(await app.project.newProject({w:64,h:64,fps:24,name:'Changed owner'}),true);
    assert.equal(s.readPresentedObjectSession().documentId,'document-2');
    assert.notEqual(s.identity().lifecycleGeneration,old.lifecycleGeneration);
    assert.equal(app.project.getCurrentLabel(),'Changed owner (not saved)');
  }
  else if(drift==='missing-readiness'){
    h.before=async command=>{if(command==='nemo_native_object_preview')await gate.promise;};
    pending=s.presentPreview(0);await turn();assert.equal(s.readPresentedObjectSession(),null);
  }else{
    if(drift==='revision')h.info().contentRevision++;
    h.info().documentSnapshotId+='-changed';
    await s.acquirePin({operation:'query.document.project.info',requestId:'read-new-pin',instanceId:old.instanceId,
      documentId:old.documentId,payload:{atRevision:h.info().contentRevision}});
    await s.presentPreview(0);assert.ok(s.readPresentedObjectSession(),'drift is presented, not merely pending');
    assert.equal(old.contentRevision,0);assert.equal(old.documentSnapshotId,'snapshot-1','captured metadata cannot follow host aliases');
  }
  assert.equal(app.element('np-cancel').listeners.click(),false);
  assert.equal(app.startScreen.classList.contains('hid'),false);
  assert.equal(app.element('start-newpanel').style.display,'block');
  if(pending){gate.resolve();await pending;}
  assert.equal(app.mutations,0);assert.equal(JSON.stringify(app.writes),beforeWrites,'Cancel adds no preference or document writes');
});
for(const stage of ['status','bootstrap','final-preview'])test('native entry late Cancel during '+stage+' keeps the editor covered',async()=>{
  const h=host(),app=ordinary(h),gate=deferred();await app.project.openPath('/old.json');nativeNewEntry(app,'plus');
  const beforeWrites=JSON.stringify(app.writes);let previews=0;h.before=async command=>{
    if(stage==='status'&&command==='nemo_native_object_client_status'||stage==='bootstrap'&&command==='nemo_native_object_bootstrap'||
      stage==='final-preview'&&command==='nemo_native_object_preview'&&++previews===2)await gate.promise;
  };
  const creating=blank64(app)();await turn();assert.equal(app.element('np-cancel').listeners.click(),false);
  assert.equal(app.startScreen.classList.contains('hid'),false);
  assert.equal(app.element('start-newpanel').style.display,'block');gate.resolve();assert.equal(await creating,false);
  assert.equal(app.startScreen.classList.contains('hid'),false);assert.equal(app.toasts.includes('New project created'),false);
  if(stage==='status')assert.equal(Project.desktop(app.window).identity().documentId,'document-1');
  else assert.equal(Project.desktop(app.window).isActive(),false);
  assert.equal(app.mutations,0);assert.equal(JSON.stringify(app.writes),beforeWrites,'Cancel adds no preference or document writes');
});
test('obsolete native entry Cancel cannot restore the old editor after completed Create',async()=>{
  const h=host(),app=ordinary(h);await app.project.openPath('/old.json');nativeNewEntry(app,'plus');
  assert.equal(await blank64(app)(),true);assert.equal(h.info().documentId,'document-2');
  assert.equal(app.element('np-cancel').listeners.click(),false);assert.equal(app.startScreen.classList.contains('hid'),false);
  assert.equal(Project.desktop(app.window).identity().documentId,'document-2');assert.equal(app.project.getCurrentLabel(),'PNG baseline (not saved)');
});
test('native entry failure overlay survives Cancel after an unconfirmed final frame',async()=>{
  const h=host(),app=ordinary(h);await app.project.openPath('/old.json');nativeNewEntry(app,'plus');let previews=0;
  h.after=async(command,r,v)=>command==='nemo_native_object_preview'&&++previews===2?{status:'deferred-occluded'}:v;
  assert.equal(await blank64(app)(),false);const alert=app.element('start-newpanel').children.at(-1),message=alert.textContent;
  assert.match(message,/creation failed.*may have changed/i);assert.equal(app.element('np-cancel').listeners.click(),false);
  assert.equal(app.startScreen.classList.contains('hid'),false);assert.equal(app.element('start-newpanel').style.display,'block');
  assert.equal(alert.style.display,'block');assert.equal(alert.textContent,message);assert.equal(Project.desktop(app.window).isActive(),false);
});
test('retained inactive project blocks New entry and a form submitted after preparation; terminal close re-enables',async()=>{
  const h=host(),f=observedInactive(h),app=f.app;await app.project.openPath('/old.json');
  nativeNewEntry(app,'plus');const submit=blank64(app),before=h.calls.filter(c=>c.command==='nemo_native_object_bootstrap').length;
  assert.equal((await app.project.openPath('/B.json')).status,'prepared');const held=f.tabs().held(),owner=h.info().documentId;
  assert.equal(nativeNewEntry(app,'plus'),false);assert.equal(await submit(),false);
  assert.equal(h.calls.filter(c=>c.command==='nemo_native_object_bootstrap').length,before);
  assert.equal(h.info().documentId,owner);assert.strictEqual(f.tabs().held(),held);assert.ok(h.inactive());
  assert.equal(app.element('project-tabs-list').children.length,2);assert.equal(app.mutations,0);
  assert.equal(app.element('np-cancel').listeners.click(),false);assert.equal(app.startScreen.classList.contains('hid'),false);
  assert.equal(app.element('start-newpanel').style.display,'block');assert.strictEqual(f.tabs().held(),held);
  assert.equal((await f.tabs().close(held)).status,'closed');nativeNewEntry(app,'plus');
  assert.equal(await blank64(app)(),true);assert.equal(h.inactive(),null);
});
test('busy and unknown inactive preparation block ordinary New even without a parked row',async()=>{
  for(const state of ['busy','unknown']){
    const h=host(),f=observedInactive(h),app=f.app;await app.project.openPath('/old.json');
    nativeNewEntry(app,'plus');const gate=deferred();h.before=async(command,r)=>{
      if(command!=='nemo_native_object_inactive_project')return;
      if(state==='busy')await gate.promise;
      else if(r.action.kind==='prepare'||r.action.recoverRequestId)throw Error('unknown retained outcome');
    };
    const preparing=app.project.openPath('/B.json');await turn();
    if(state==='unknown')await preparing;
    const before=h.calls.filter(c=>c.command==='nemo_native_object_bootstrap').length;
    assert.equal(nativeNewEntry(app,'plus'),false);
    assert.equal(await app.project.newProject({w:64,h:64,fps:24,name:'Unsafe'}),false);
    assert.equal(h.calls.filter(c=>c.command==='nemo_native_object_bootstrap').length,before);
    assert.equal(h.info().documentId,'document-1');assert.equal(app.mutations,0);
    assert.equal(app.element('np-cancel').listeners.click(),false);assert.equal(app.startScreen.classList.contains('hid'),false);
    assert.equal(app.element('start-newpanel').style.display,'block');
    if(state==='unknown'){assert.equal(f.tabs().held(),null);assert.equal(f.tabs().isBusy(),false);}
    else{gate.resolve();await preparing;}
  }
});
test('native New form snapshots scalar fields before pending admission and cannot follow mutable UI aliases',async()=>{
  const h=host(),gate=deferred(),app=ordinary(h);await app.project.openPath('/old.json');
  nativeNewEntry(app,'burger');const submit=blank64(app);
  h.before=async command=>{if(command==='nemo_native_object_client_status')await gate.promise;};
  const creating=submit();await turn();
  app.element('np-w').value='999';app.element('np-name').value='Changed after submit';gate.resolve();
  assert.equal(await creating,true);
  assert.deepEqual(h.calls.filter(c=>c.command==='nemo_native_object_bootstrap').at(-1).r.newProject,{canvasW:64,canvasH:64,fps:24});
  assert.equal(app.project.getCurrentLabel(),'PNG baseline (not saved)');assert.equal(app.mutations,0);
});
test('pending native replacement cannot be superseded by another New entry or inactive Open',async()=>{
  const h=host(),gate=deferred(),app=ordinary(h);await app.project.openPath('/old.json');
  nativeNewEntry(app,'plus');h.before=async command=>{if(command==='nemo_native_object_client_status')await gate.promise;};
  const creating=blank64(app)();await turn();
  assert.equal(nativeNewEntry(app,'plus'),false);assert.equal(await app.project.openPath('/B.json'),false);
  assert.equal(h.calls.filter(c=>c.command==='nemo_native_object_inactive_project').length,0);
  gate.resolve();assert.equal(await creating,true);
  assert.equal(h.calls.filter(c=>c.command==='nemo_native_object_bootstrap').length,2);
  assert.equal(app.project.getCurrentLabel(),'PNG baseline (not saved)');assert.equal(app.mutations,0);
});
test('missing or throwing inactive eligibility never falls back to legacy New',async()=>{
  for(const predicate of [undefined,()=>{throw Error('unavailable inactive ownership');}]){
    const h=host(),app=ordinary(h,{NemoNativeObjectInactiveProjectTabs:{create(root,ports){
      return Object.freeze({...InactiveTabs.create(root,ports),canReplaceActiveProject:predicate});
    }}});await app.project.openPath('/old.json');
    assert.equal(nativeNewEntry(app,'plus'),false);assert.equal(await app.project.newProject({w:64,h:64,fps:24}),false);
    assert.equal(h.calls.filter(c=>c.command==='nemo_native_object_bootstrap').length,1);
    assert.equal(h.info().documentId,'document-1');assert.equal(app.project.getCurrentLabel(),'old — /old.json');assert.equal(app.mutations,0);
  }
});
test('ordinary New form uses the real native session, remains unsaved, then Save As acknowledges the actual file',async()=>{
  const h=host(),app=ordinary(h),create=form(app);
  assert.equal(await create(),true);
  assert.deepEqual(h.calls.find(c=>c.command==='nemo_native_object_bootstrap').r.newProject,intent);
  assert.equal(app.project.getCurrentLabel(),'Named blank (not saved)');assert.equal(app.project.isDirty(),true);
  assert.equal(app.startScreen.classList.contains('hid'),true);assert.equal(app.element('project-tabs-list').children.length,1);
  assert.equal(h.events.length,2);assert.equal(app.mutations,0);assert.equal(app.writes.length,0);
  assert.equal(app.window.state.layers[0].name,'Layer 1');assert.equal(app.window.state.layers[0].frames.length,0);
  assert.equal((await app.project.saveAs()).acknowledged,true);
  assert.equal(app.project.getCurrentLabel(),'ordinary-new — /ordinary-new.json');assert.equal(app.project.isDirty(),false);
  assert.ok(app.writes.every(([key])=>key==='nemo-recents'));assert.equal(app.downloads.length,0);
});
test('New replaces a saved Open using exact owner pins, then preserves ordinary Open routing',async()=>{
  const h=host(),s=session(h),f=fileUI(s);
  assert.equal(await f.ui.openPath('/old.json'),true);assert.equal(f.ui.isDirty(),false);
  assert.equal(await f.ui.newProject({w:640,h:360,fps:30,name:'Blank'}),true);
  assert.deepEqual(f.meta(),{name:'Blank',path:null});assert.equal(f.ui.isDirty(),true);
  const request=h.calls.filter(c=>c.command==='nemo_native_object_bootstrap')[1].r;
  assert.equal(request.replace.documentId,'document-1');assert.equal(request.replace.documentSnapshotId,'snapshot-1');
  assert.equal(s.info().documentId,'document-2');assert.equal(await f.ui.openPath('/again.json'),true);
  assert.equal(s.info().documentId,'document-3');assert.equal(f.ui.isDirty(),false);
});
test('New retires the previous subscriber and publishes the replacement before attaching a fresh one',async()=>{
  const h=host(),events=h.events;
  const s=session(h,{projectionReady:()=>true,publishRevision:()=>true,
    subscribeRevisions:async()=>{events.push('subscribe');return {...h.info(),subscriptionId:'subscription-'+h.info().documentId};},
    unsubscribeRevisions:async()=>{events.push('retire');},changed:()=>events.push('changed')});
  const f=fileUI(s,{created:()=>{events.push('commit');return true;}});
  await f.ui.newProject({w:640,h:360,fps:30});events.length=0;
  await f.ui.newProject({w:640,h:360,fps:30});
  assert.deepEqual(events,['retire','present:document-2','present:document-2','commit','changed','subscribe']);
  assert.equal(s.readPresentedObjectSession().documentId,'document-2');
});
for(const where of ['confirm','status'])test('cancelled New while awaiting '+where+' never dispatches or changes the old owner',async()=>{
  const h=host(),s=session(h),gate=deferred();let current=true;
  const f=fileUI(s,{confirmReplace:where==='confirm'?()=>gate.promise:async()=>true});await f.ui.newProject({w:640,h:360,fps:30});
  if(where==='status')h.before=async command=>{if(command==='nemo_native_object_client_status')await gate.promise;};
  const next=f.ui.newProject({w:320,h:180,fps:24},()=>current);await turn();current=false;gate.resolve(where==='confirm');
  assert.equal(await next,false);assert.equal(s.isActive(),true);assert.equal(s.info().documentId,'document-1');
  assert.equal(h.calls.filter(c=>c.command==='nemo_native_object_bootstrap').length,1);
});
test('form cancel and duplicate clicks while native status is pending submit at most one request',async()=>{
  for(const cancel of [false,true]){
    const h=host(),gate=deferred();h.before=async command=>{if(command==='nemo_native_object_client_status'&&!h.info())await gate.promise;};
    const app=ordinary(h),create=form(app),first=create();await turn();assert.equal(create(),false);
    if(cancel)app.element('np-cancel').listeners.click();gate.resolve();await first;
    assert.equal(h.calls.filter(c=>c.command==='nemo_native_object_bootstrap').length,cancel?0:1);
    assert.equal(app.mutations,0);assert.equal(app.writes.length,0);
  }
});
for(const bad of [null,{}, {canvasW:0,canvasH:1,fps:24},{canvasW:1.5,canvasH:1,fps:24},
  {canvasW:1,canvasH:1,fps:Infinity},{...intent,documentJson:'{}'}])test('invalid scalar New intent '+JSON.stringify(bad)+' refuses before dispatch',async()=>{
  const h=host(),s=session(h);await assert.rejects(s.newProject(bad),e=>e.details.disposition==='notDispatched');assert.deepEqual(h.calls,[]);
});
for(const fault of ['receipt','metadata','preview','lost-bootstrap','ui'])test('post-dispatch '+fault+' failure fences New and never reports created/saved',async()=>{
  const h=host();h.after=async(command,r,v)=>{
    if(fault==='receipt'&&command==='nemo_native_object_bootstrap')return {...v,requestId:'foreign'};
    if(fault==='metadata'&&command==='nemo_native_object_reference')return {...v,canvasW:1};
    if(fault==='preview'&&command==='nemo_native_object_preview')return {...v,status:'deferred-occluded'};
    if(fault==='lost-bootstrap'&&command==='nemo_native_object_bootstrap')throw Error('Response lost');return v;
  };
  const s=session(h),f=fileUI(s,fault==='ui'?{created:()=>{throw Error('UI failed');}}:{});
  assert.equal(await f.ui.newProject({w:640,h:360,fps:30}),false);
  assert.equal(s.isActive(),false);assert.equal(s.blocksLegacy(),true);assert.equal(s.readPresentedObjectSession(),null);
  assert.equal(f.ui.isDirty(),true);assert.ok(!f.effects.includes('create'));assert.ok(f.effects.includes('failed'));
  assert.equal(await f.ui.newProject({w:640,h:360,fps:30}),false);
  assert.equal(h.calls.filter(c=>c.command==='nemo_native_object_bootstrap').length,1);
});
test('explicit not-dispatched native refusal preserves the old owner and UI destination',async()=>{
  const h=host(),s=session(h),f=fileUI(s);await f.ui.openPath('/old.json');
  h.before=async command=>{if(command==='nemo_native_object_bootstrap')throw {code:'replacement_refused',details:{disposition:'notDispatched'}};};
  assert.equal(await f.ui.newProject({w:640,h:360,fps:30}),false);
  assert.equal(s.isActive(),true);assert.equal(s.info().documentId,'document-1');assert.equal(f.meta().path,'/old.json');
  assert.equal(f.ui.isDirty(),false);assert.equal(f.effects.includes('failed'),false);
});
test('New currency lost after dispatch fences without publishing a UI or saved baseline',async()=>{
  const h=host(),s=session(h),f=fileUI(s);let current=true;
  h.after=async(command,r,v)=>{if(command==='nemo_native_object_preview')current=false;return v;};
  assert.equal(await f.ui.newProject({w:640,h:360,fps:30},()=>current),false);
  assert.equal(s.isActive(),false);assert.equal(f.effects.includes('create'),false);assert.equal(f.ui.isDirty(),true);
});
test('missing New session method and missing desktop adapter both refuse without legacy release',async()=>{
  const h=host(),s=session(h),f=fileUI({...s,newProject:undefined});
  assert.equal(await f.ui.newProject({w:640,h:360,fps:30}),false);assert.deepEqual(h.calls,[]);
  const app=ordinary(host(),{NemoNativeObjectProjectFile:null});
  assert.equal(await form(app)(),false);assert.equal(app.mutations,0);assert.equal(app.writes.length,0);
});
test('ordinary form replaces an active native project without legacy save/release or recents for the new name',async()=>{
  const h=host(),app=ordinary(h);await app.project.openPath('/old.json');
  const oldRecents=JSON.stringify(app.writes);app.element('btn-new').listeners.click();
  assert.equal(await form(app)(),true);
  assert.equal(app.project.getCurrentLabel(),'Named blank (not saved)');assert.equal(app.project.isDirty(),true);
  assert.equal(app.element('project-tabs-list').children.length,1);assert.equal(app.mutations,0);
  assert.equal(JSON.stringify(app.writes),oldRecents);
  assert.equal(h.calls.filter(c=>c.command==='nemo_native_object_save_as').length,0);
});
test('refusing dirty replacement keeps destination, owner and subscriber without a bootstrap',async()=>{
  const h=host(),s=session(h),f=fileUI(s,{confirmReplace:async()=>false});
  await f.ui.newProject({w:640,h:360,fps:30,name:'First'});
  assert.equal(await f.ui.newProject({w:320,h:180,fps:24,name:'Second'}),false);
  assert.equal(f.meta().name,'First');assert.equal(s.info().documentId,'document-1');
  assert.equal(s.isActive(),true);assert.equal(s.readPresentedObjectSession().documentId,'document-1');
  assert.equal(h.calls.filter(c=>c.command==='nemo_native_object_bootstrap').length,1);
});
test('save picker in flight excludes New, and pending New excludes Open and Save',async()=>{
  const h=host(),s=session(h),gate=deferred(),f=fileUI(s,{pick:()=>gate.promise});
  await f.ui.newProject({w:640,h:360,fps:30});const saving=f.ui.saveAs();await turn();
  assert.equal(await f.ui.newProject({w:320,h:180,fps:24}),false);
  gate.resolve(null);await saving;
  const status=deferred();h.before=async command=>{if(command==='nemo_native_object_client_status')await status.promise;};
  const next=f.ui.newProject({w:320,h:180,fps:24});await turn();
  assert.equal(await f.ui.openPath('/ignored.json'),false);assert.equal((await f.ui.saveAs()).acknowledged,false);
  status.resolve();await next;
  assert.equal(h.calls.filter(c=>c.command==='nemo_native_object_bootstrap').length,2);
});
test('second presentation and subscriber failure cannot establish a usable New owner',async()=>{
  for(const failure of ['final-frame','subscriber']){
    const h=host();let previews=0;
    h.after=async(command,r,v)=>command==='nemo_native_object_preview'&&++previews===2&&failure==='final-frame'?{status:'deferred-occluded'}:v;
    const s=session(h,{projectionReady:()=>true,publishRevision:()=>true,subscribeRevisions:async()=>{throw Error('Subscriber unavailable');}});
    const f=fileUI(s);assert.equal(await f.ui.newProject({w:640,h:360,fps:30}),false);
    assert.equal(s.isActive(),false);assert.equal(s.readPresentedObjectSession(),null);assert.equal(f.ui.isDirty(),true);
    assert.ok(f.effects.includes('failed'));if(failure==='final-frame')assert.equal(f.effects.includes('create'),false);
  }
});
test('New captures UI intent before asynchronous admission; native owner drift refuses before dispatch',async()=>{
  const h=host(),s=session(h),gate=deferred(),f=fileUI(s);let delay=true;
  h.before=async command=>{if(delay&&command==='nemo_native_object_client_status')await gate.promise;};
  const cfg={w:640,h:360,fps:30,name:'Original'},next=f.ui.newProject(cfg);
  cfg.w=1;cfg.name='Mutated';delay=false;gate.resolve();assert.equal(await next,true);
  assert.equal(s.info().canvasW,640);assert.equal(f.meta().name,'Original');
  h.after=async(command,r,v)=>command==='nemo_native_object_client_status'?{...v,lifecycleGeneration:9}:v;
  assert.equal(await f.ui.newProject({w:320,h:180,fps:24}),false);
  assert.equal(h.calls.filter(c=>c.command==='nemo_native_object_bootstrap').length,1);
  assert.equal(f.meta().name,'Original');assert.equal(s.isActive(),false);
});
test('ordinary Cancel after native dispatch fences the new owner and shows no creation or save success',async()=>{
  const h=host(),gate=deferred();h.before=async command=>{if(command==='nemo_native_object_bootstrap')await gate.promise;};
  const app=ordinary(h),first=form(app)();await turn();app.element('np-cancel').listeners.click();gate.resolve();
  assert.equal(await first,false);assert.equal(Project.desktop(app.window).isActive(),false);
  assert.equal(app.project.getCurrentLabel(),'Untitled (not saved)');assert.equal(app.project.isDirty(),true);
  assert.equal(app.toasts.includes('New project created'),false);assert.equal(app.mutations,0);assert.equal(app.writes.length,0);
});
test('ordinary post-dispatch final-frame failure keeps the form and cannot publish New metadata',async()=>{
  const h=host();let previews=0;
  h.after=async(command,r,v)=>command==='nemo_native_object_preview'&&++previews===2?{status:'deferred-occluded'}:v;
  const app=ordinary(h);assert.equal(await form(app)(),false);
  assert.equal(app.project.getCurrentLabel(),'Untitled (not saved)');assert.equal(app.project.isDirty(),true);
  assert.equal(app.startScreen.classList.contains('hid'),false);assert.equal(app.element('start-newpanel').style.display,'block');
  assert.match(app.toasts.at(-1),/creation failed.*may have changed/i);assert.equal(app.toasts.includes('New project created'),false);
});
test('request currency lost during reveal cannot publish the presented New owner',async()=>{
  const h=host(),s=session(h);let current=true;
  const f=fileUI(s,{reveal:async()=>{current=false;}});
  assert.equal(await f.ui.newProject({w:640,h:360,fps:30},()=>current),false);
  assert.equal(s.isActive(),false);assert.equal(s.readPresentedObjectSession(),null);assert.equal(f.effects.includes('create'),false);
});
for(const supersede of [false,true])test('ordinary late '+(supersede?'newer request':'Cancel')+' during final presentation cannot publish old New',async()=>{
  const h=host(),gate=deferred();let previews=0;
  h.before=async command=>{if(command==='nemo_native_object_preview'&&++previews===2)await gate.promise;};
  const app=ordinary(h),create=form(app),first=create();await turn();
  assert.equal(app.startScreen.classList.contains('hid'),true,'own reveal has hidden the form before final frame');
  app.element('np-cancel').listeners.click();
  if(supersede){app.element('btn-new').listeners.click();await form(app)();}
  gate.resolve();await first;
  assert.equal(Project.desktop(app.window).isActive(),false);assert.equal(app.project.getCurrentLabel(),'Untitled (not saved)');
  assert.equal(app.toasts.includes('New project created'),false);assert.equal(app.mutations,0);assert.equal(app.writes.length,0);
});
test('request generation cancelled while the new subscriber attaches cannot return delivered success',async()=>{
  const h=host(),gate=deferred(),s=session(h,{projectionReady:()=>true,publishRevision:()=>true,
    subscribeRevisions:async()=>{await gate.promise;return {...h.info(),subscriptionId:'new-subscription'};}});
  let current=true;const notices=[];
  const f=fileUI(s,{createdNotice:()=>notices.push('created')});
  const creating=f.ui.newProject({w:640,h:360,fps:30},()=>current);await turn();current=false;gate.resolve();
  assert.equal(await creating,false);assert.equal(s.isActive(),false);assert.equal(s.readPresentedObjectSession(),null);
  assert.deepEqual(notices,[]);assert.equal(f.ui.isDirty(),true);
});

async function pendingBlankSave(){
  const h=host(),app=ordinary(h);
  app.element('start-new').listeners.click();
  assert.equal(await blank64(app)(),true);
  const s=Project.desktop(app.window),pin=structuredClone(s.info()),gate=deferred();
  assert.deepEqual([pin.canvasW,pin.canvasH,pin.fps,pin.totalFrames],[64,64,24,120]);
  assert.equal(app.window.state.canvasBg,'#ffffff');
  assert.deepEqual([app.window.state.waIn,app.window.state.waOut],[0,119]);
  let entered=false,picked=0;
  app.window.__TAURI__.dialog.save=async()=>{picked++;return '/settled-blank.json';};
  h.before=async command=>{if(command==='nemo_native_object_preview'){entered=true;await gate.promise;}};
  const preview=s.presentPreview(0);preview.catch(()=>{});
  await turn();assert.equal(entered,true);assert.equal(s.readPresentedObjectSession(),null);
  return {h,app,s,pin,gate,preview,picked:()=>picked,
    writes:()=>h.calls.filter(c=>c.command==='nemo_native_object_save_as')};
}
test('ordinary New 64 by 64 Save As waits for admitted presentation and saves the exact 120-frame owner once',async()=>{
  const f=await pendingBlankSave();let completed=false;
  const saving=f.app.project.saveAs().then(value=>{completed=true;return value;});
  try{
    await turn();assert.equal(completed,false,'the accepted save intent must wait, not lose the click');
    assert.equal(f.picked(),0);assert.equal(f.writes().length,0);
    assert.equal(f.app.project.getCurrentLabel(),'PNG baseline (not saved)');
    f.gate.resolve();await f.preview;
    assert.equal((await saving).acknowledged,true);
    assert.equal(f.picked(),1);assert.equal(f.writes().length,1);
    const request=f.writes()[0].r;
    assert.deepEqual([request.instanceId,request.documentId,request.atRevision,request.documentSnapshotId],
      [f.pin.instanceId,f.pin.documentId,f.pin.contentRevision,f.pin.documentSnapshotId]);
    assert.deepEqual(f.s.info(),f.pin);
    assert.equal(f.app.project.getCurrentLabel(),'settled-blank — /settled-blank.json');
    assert.equal(f.app.project.isDirty(),false);assert.equal(f.app.mutations,0);assert.equal(f.app.downloads.length,0);
    assert.ok(f.app.writes.every(([key])=>key==='nemo-recents'));
  }finally{f.gate.resolve();await Promise.allSettled([f.preview,saving]);}
});
for(const drift of ['revision','snapshot','lifecycle','document','newer-queue','replacement','deferred']){
  test('pending ordinary Save As refuses '+drift+' before picker or publication',async()=>{
    const f=await pendingBlankSave(),other=[];
    const saving=f.app.project.saveAs();
    try{
      await turn();assert.equal(f.picked(),0);assert.equal(f.writes().length,0);
      if(drift==='revision')f.h.info().contentRevision++;
      if(drift==='snapshot')f.h.info().documentSnapshotId+='-advanced';
      if(drift==='lifecycle')f.h.info().lifecycleGeneration++;
      if(drift==='document')f.h.info().documentId='replacement-owner';
      if(drift==='newer-queue')other.push(f.s.presentPreview(0));
      if(drift==='replacement')other.push(f.s.newProject({canvasW:64,canvasH:64,fps:24}));
      if(drift==='deferred')f.h.after=(command,request,result)=>command==='nemo_native_object_preview'?
        {...result,status:'deferred-occluded'}:result;
      other.forEach(p=>p.catch(()=>{}));
      f.gate.resolve();await Promise.allSettled([f.preview,...other]);
      assert.equal((await saving).acknowledged,false);
      assert.equal(f.picked(),0);assert.equal(f.writes().length,0);
      assert.equal(f.app.project.getCurrentLabel(),'PNG baseline (not saved)');
      assert.equal(f.app.project.isDirty(),true);assert.equal(f.app.mutations,0);
    }finally{f.gate.resolve();await Promise.allSettled([f.preview,saving,...other]);}
  });
}
test('pending ordinary Save As retains picker cancellation without publication or a saved label',async()=>{
  const f=await pendingBlankSave();let cancelled=0;
  f.app.window.__TAURI__.dialog.save=async()=>{cancelled++;return null;};
  const saving=f.app.project.saveAs();
  try{
    await turn();assert.equal(cancelled,0);f.gate.resolve();await f.preview;
    assert.equal((await saving).acknowledged,false);assert.equal(cancelled,1);
    assert.equal(f.writes().length,0);assert.equal(f.app.project.getCurrentLabel(),'PNG baseline (not saved)');
    assert.equal(f.app.project.isDirty(),true);
  }finally{f.gate.resolve();await Promise.allSettled([f.preview,saving]);}
});
test('pending Save As retains its original UI ticket and refuses replacement before the picker',async()=>{
  const h=host(),s=session(h),gate=deferred();let generation=1,picked=0;
  const f=fileUI(s,{readUI:()=>({handle:'tab',generation}),pick:async()=>{picked++;return '/wrong-tab.json';}});
  assert.equal(await f.ui.newProject({w:64,h:64,fps:24,name:'Original'}),true);
  h.before=async command=>{if(command==='nemo_native_object_preview')await gate.promise;};
  const preview=s.presentPreview(0);preview.catch(()=>{});await turn();const saving=f.ui.saveAs();
  try{
    await turn();generation++;gate.resolve();await preview;
    assert.equal((await saving).acknowledged,false);assert.equal(picked,0);
    assert.equal(h.calls.filter(c=>c.command==='nemo_native_object_save_as').length,0);
    assert.deepEqual(f.meta(),{name:'Original',path:null});assert.equal(f.ui.isDirty(),true);
  }finally{gate.resolve();await Promise.allSettled([preview,saving]);}
});
