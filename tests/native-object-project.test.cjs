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
  const calls=[];let installed=false,revision=0,edited=false,undo=false,preview='presented';
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
      installed=true;return {...info(),requestId:r.requestId};
    }
    if(command==='nemo_native_object_reference'&&r.action.kind==='inspect')return {...info(),requestId:r.requestId};
    if(command==='nemo_native_object_preview')return {status:preview,workId:'work-'+revision,viewGeneration:1};
    if(command==='nemo_native_object_reference'||command==='nemo_native_object_dispatch'){
      assert.equal(r.expectedRevision,revision);
      if(r.operation==='command.document.object.undo'){edited=false;undo=true;reference=null;}
      else if(r.operation==='command.document.object.redo'){edited=true;undo=false;}
      else {edited=true;reference={id:'ref',name:'image.png',opacity:0.5,visible:true,offsetFrames:0};}
      revision++;
      return {apiVersion:2,requestId:r.requestId,instanceId:'instance',documentId:'document',contentRevision:revision,ok:true,result:{applied:true,historyEntriesAdded:1}};
    }
    if(command==='nemo_native_object_save_as'){
      copies.push(r);return {apiVersion:2,requestId:r.requestId,instanceId:r.instanceId,documentId:r.documentId,atRevision:r.atRevision,
        documentSnapshotId:r.documentSnapshotId,byteCount:info().byteCount,published:true,cleanupPending:false};
    }
    if(command==='nemo_native_object_export')return {...info(),requestId:r.requestId,published:true,cleanupComplete:true};
    throw Error('Unexpected command: '+command);
  }
  return {invoke,info,calls,copies,preview(value){preview=value;}};
}
function session(h){return Project.create({invoke:h.invoke,mapping:()=>({}),frame:()=>0});}
async function presented(h){const s=session(h);await s.openPath('/project.json');await s.presentPreview(0);return s;}
function files(s,overrides={}){
  const effects=[];let handle='tab';
  const ports={readUI:()=>({handle,generation:1}),reveal:async()=>{},opened:(path)=>effects.push(['open',path]),
    commit:(path)=>{effects.push(['save',path]);return true;},pick:async()=>'/new.json',repaint:()=>{},failed:()=>{},notice:()=>{},...overrides};
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
  await s.openPath('/project.json');assert.equal(s.blocksLegacy(),true);await assert.rejects(s.openPath('/other.json'),/replacement/);
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
function ordinary(h){
  const fixture=fs.readFileSync('tests/project-open.test.cjs','utf8');
  const start=fixture.indexOf('function harness('),end=fixture.indexOf('\nfunction select(',start);
  const code=fixture.slice(start,end).replace('configureWindow(window);',
    "window.state=context.state;window.document=document;window.userLayers=context.userLayers;window.showToast=context.showToast;document.getElementById('drawing-canvas').getBoundingClientRect=()=>({left:0,top:0,width:320,height:180});configureWindow(window);");
  const context=vm.createContext({assert,vm,require,setTimeout,console,source:fs.readFileSync('src/js/project.js','utf8'),
    documentSource:fs.readFileSync('src/js/project-document.js','utf8'),entrySource:fs.readFileSync('src/js/project-entry.js','utf8')});
  vm.runInContext(code+'\nthis.makeHarness=harness;',context);
  return context.makeHarness({dirtyDot:true,configureWindow(root){
    root.NemoNativeObjectProject=Project;root.NemoNativeObjectProjectFile=Files;root.NemoNativeObjectSaveAsWorkflow=Workflow;root.NemoNativeObjectSavedAcknowledgement=Ack;
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
  const before=h.calls.length;assert.equal(await app.project.openPath('/another.json'),false);assert.equal(h.calls.length,before);
});
