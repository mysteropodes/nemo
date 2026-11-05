'use strict';
// Legacy project import, slice 1 (JS side): the host converts a baseline file into a NEW native document; the UI must treat it
// as unsaved, keep the original file out of every Save, and route old autosaves and history snapshots through the same open.
const test=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const vm=require('node:vm');
const LI=require('../src/js/adapters/native-legacy-import.js');
const Project=require('../src/js/adapters/native-object-project.js');
const Files=require('../src/js/adapters/native-object-project-file.js');
const Workflow=require('../src/js/application/native-object-save-as-workflow.js');
const Ack=require('../src/js/application/native-object-saved-acknowledgement.js');
const Resume=require('../src/js/adapters/native-object-recovery-resume.js');
const turn=()=>new Promise(setImmediate);
const REPORT={sourceVersion:13,migrations:['framesOnly:1'],warnings:[{path:'layers[0].frames[2]',code:'hidden_strokes',message:'m'}],warningsTotal:3,
  ignored:['mediaMode'],counts:{layers:1,keyframes:2,strokes:3}};
const OLD='{"version":13,"layers":[{"name":"L","frames":[{"strokes":[],"isKeyframe":true}]}]}';

test('reportOf trusts only a plain legacyImport object and bounds what it keeps',()=>{
  assert.equal(LI.reportOf(null),null);assert.equal(LI.reportOf({}),null);assert.equal(LI.reportOf({legacyImport:[]}),null);
  assert.equal(LI.isLegacy({legacyImport:REPORT}),true);
  const many={legacyImport:{warnings:Array.from({length:80},(_,i)=>({path:'p'+i,code:'c',message:'x'.repeat(900)})),warningsTotal:80}};
  const kept=LI.reportOf(many);
  assert.equal(kept.warnings.length,50);assert.equal(kept.warnings[0].message.length,300);assert.equal(kept.warningsTotal,80);
  assert.equal(Object.isFrozen(kept),true);
  assert.equal(LI.reportOf({legacyImport:{warningsTotal:-4,warnings:[]}}).warningsTotal,0);
});

test('the open notice keeps the baseline wording, says the file is untouched, and names what was not carried over',()=>{
  const clean=LI.notice('Film',LI.reportOf({legacyImport:{counts:{}}}));
  assert.equal(clean,'Opened: Film — imported as a new project; the original file is not modified. Use Save As to keep your work.');
  const withLoss=LI.notice('Film',LI.reportOf({legacyImport:REPORT}));
  assert.match(withLoss,/^Opened: Film — imported as a new project/);
  assert.match(withLoss,/3 item\(s\) were not carried over \(first: layers\[0\]\.frames\[2\]\)\.$/);
  const newer=LI.reportOf({legacyImport:{sourceVersion:14,warnings:[{path:'version',code:'newer_format_version',message:'m'}],warningsTotal:1}});
  const text=LI.notice('Film',newer,k=>({toastFileFromNewerVersionHint:'NEWER v',toastUpdateAppBeforeResave:' UPDATE'})[k]);
  assert.equal(text.startsWith('NEWER v14 UPDATE — Opened: Film'),true);
  assert.doesNotMatch(text,/not carried over/,'the version warning is not counted as lost content');
});

test('the original file can never be the Save As destination, whatever the spelling',()=>{
  assert.equal(LI.samePath('/Users/me/Old.json','/users/me//old.json'),true);
  assert.equal(LI.samePath('C:\\a\\b.json','c:/a/b.json'),true);
  assert.equal(LI.samePath('/a/b.json','/a/b-copy.json'),false);
  assert.equal(LI.samePath(null,'/a'),false);
});

test('text entry points write one file under app data and use the ordinary open',async()=>{
  const writes=[],opened=[];
  const deps={appDataDir:async()=>'/app/data/',fs:{mkdir:async(d,o)=>writes.push(['mkdir',d,o]),writeTextFile:async(p,t)=>writes.push(['write',p,t])},open:async p=>{opened.push(p);return true;}};
  assert.equal(await LI.openText(deps,OLD,'My: film/2'),true);
  assert.deepEqual(writes,[['mkdir','/app/data/legacy-import',{recursive:true}],['write','/app/data/legacy-import/My film 2.json',OLD]]);
  assert.deepEqual(opened,['/app/data/legacy-import/My film 2.json']);
  assert.equal(await LI.openText(deps,'','x'),false);
  assert.equal(LI.safeName('///'),'Untitled');
  assert.equal(LI.isNativeText('{"format":"nemo.native-project-document","x":1}'),true);
  assert.equal(LI.isNativeText(OLD),false);
  assert.equal(LI.looksLikeProject(OLD),true);assert.equal(LI.looksLikeProject('{"legacy":true}'),false);
  assert.equal(LI.looksLikeProject('{"frames":[]}'),true);assert.equal(LI.looksLikeProject('{"format":"x","layers":[]}'),false);
  assert.equal(LI.looksLikeProject('not json'),false);
  const native={fs:{readTextFile:async()=>'{"format":"nemo.native-project-document","formatVersion":1}'},open:async()=>assert.fail('native snapshot')};
  assert.equal(await LI.restoreSnapshot(native,'/h/1.json','Film'),false);
  opened.length=0;writes.length=0;
  assert.equal(await LI.restoreSnapshot({...deps,fs:{...deps.fs,readTextFile:async()=>OLD}},'/h/1.json','Film'),true);
  assert.equal(writes[1][1],'/app/data/legacy-import/Film (restored).json');
});

test('Resume prefers the Rust recovery store and falls back to an old baseline autosave only when it is a project',async()=>{
  const log=[];
  const native=(entries)=>({presence:async publish=>{publish(entries>0);return entries>0;},list:async()=>new Array(entries).fill(1),resume:async()=>{log.push('native');return 'native';}});
  const legacy=(present)=>({present:async()=>present,resume:async()=>{log.push('legacy');return 'legacy';}});
  const seen=[];
  const both=LI.combineResume(native(1),legacy(true));
  await both.presence(v=>seen.push(v));assert.deepEqual(seen,[true]);
  assert.equal(await both.resume(),'native');
  seen.length=0;
  const only=LI.combineResume(native(0),legacy(true));
  await only.presence(v=>seen.push(v));await turn();assert.deepEqual(seen,[true]);
  assert.equal(await only.resume(),'legacy');
  seen.length=0;
  const none=LI.combineResume(native(0),legacy(false));
  await none.presence(v=>seen.push(v));await turn();assert.deepEqual(seen,[false]);
  assert.equal(await none.resume(),'native','nothing to resume reads as the native empty state');
  assert.deepEqual(log,['native','legacy','native']);
  const notes=[];
  const slot=LI.autosaveSource({storage:{getItem:()=>null},idb:{get:async()=>OLD},appDataDir:async()=>'/d',fs:{mkdir:async()=>{},writeTextFile:async()=>{}},
    open:async()=>true,notice:t=>notes.push(t),corruptText:()=>'corrupt'});
  assert.equal(await slot.present(),true);assert.equal(await slot.resume(),true);assert.deepEqual(notes,['Session resumed']);
  const bad=LI.autosaveSource({storage:{getItem:()=>'{"legacy":true}'},appDataDir:async()=>'/d',fs:{},open:async()=>true,notice:t=>notes.push(t),corruptText:()=>'corrupt'});
  assert.equal(await bad.present(),false);assert.equal(await bad.resume(),false);assert.equal(notes.at(-1),'corrupt');
});

// ---- the real session, file UI and project.js against a fake native host ----
function host(){
  let installed=false,number=0,meta=null;
  const calls=[],h={calls,legacyFor:null};
  h.invoke=async(command,args)=>{
    const r=args?JSON.parse(args.requestJson):null;calls.push({command,r});
    let result;
    if(command==='nemo_native_object_client_status')result={apiVersion:2,instanceId:'instance',available:installed,...meta};
    else if(command==='nemo_native_object_bootstrap'){
      installed=true;number++;
      const cfg=r.newProject||{canvasW:320,canvasH:180,fps:24};
      meta={apiVersion:2,instanceId:'instance',documentId:'document-'+number,contentRevision:0,documentSnapshotId:'snapshot-'+number,lifecycleGeneration:number,
        documentDigest:String(number).repeat(64),byteCount:1000,...cfg,totalFrames:Math.round(cfg.fps*5),layers:[{layerUid:'layer-1',name:'Layer 1',visible:true}],reference:null,undoDepth:0,redoDepth:0};
      result={apiVersion:2,requestId:r.requestId,instanceId:'instance',documentId:meta.documentId,contentRevision:0,resourceCount:0,viewportAvailable:false};
      if(r.sourcePath&&h.legacyFor&&h.legacyFor(r.sourcePath))result.legacyImport=REPORT;
    }else if(command==='nemo_native_object_reference')result={...meta,requestId:r.requestId};
    else if(command==='nemo_native_object_preview')result={status:'presented',workId:'work-'+calls.length,viewGeneration:number};
    else if(command==='nemo_native_object_save_as')result={apiVersion:2,requestId:r.requestId,instanceId:r.instanceId,documentId:r.documentId,atRevision:r.atRevision,
      documentSnapshotId:r.documentSnapshotId,byteCount:meta.byteCount,published:true,cleanupPending:false};
    else if(command==='nemo_native_object_recovery_list')result={apiVersion:2,requestId:r.requestId,instanceId:r.instanceId,entries:[],quarantined:[]};
    else throw Error('Unexpected native route: '+command);
    return result;
  };
  return h;
}
function ordinary(h,extra={},auto=null){
  const fixture=fs.readFileSync('tests/project-open.test.cjs','utf8');
  const start=fixture.indexOf('function harness('),end=fixture.indexOf('\nfunction select(',start);
  const code=fixture.slice(start,end).replace('configureWindow(window);',
    "window.state=context.state;window.document=document;window.userLayers=context.userLayers;document.getElementById('drawing-canvas').getBoundingClientRect=()=>({left:0,top:0,width:640,height:360});configureWindow(window);");
  const context=vm.createContext({assert,vm,require,setTimeout,console,source:fs.readFileSync('src/js/project.js','utf8'),
    documentSource:fs.readFileSync('src/js/project-document.js','utf8'),entrySource:fs.readFileSync('src/js/project-entry.js','utf8')});
  vm.runInContext(code+'\nthis.makeHarness=harness;',context);
  const files=[],dirs=[],dialogs=[];
  return Object.assign(context.makeHarness({dirtyDot:true,auto,configureWindow(root){
    root.document.getElementById('start-resume').style.display='none';
    require('./helpers/native-viewport-underlay.cjs').install(root);
    root.NemoNativeObjectProject=Project;root.NemoNativeObjectProjectFile=Files;root.NemoNativeObjectRecoveryResume=Resume;root.NemoNativeLegacyImport=LI;
    root.NemoNativeObjectSaveAsWorkflow=Workflow;root.NemoNativeObjectSavedAcknowledgement=Ack;
    root.NemoNativeOpacityCutover={blocksLegacy:()=>true,requestRelease:()=>assert.fail('legacy release')};
    root.crypto={randomUUID:require('node:crypto').randomUUID};
    for(const id of ['project-tab-add','start-new'])root.document.getElementById(id).click=function(){return this.listeners.click();};
    root.SM.exportJSON=()=>assert.fail('legacy serialization');
    root.SM.t=key=>key;
    root.__TAURI__={core:{invoke:h.invoke},dialog:{save:async o=>{dialogs.push(o);return h.saveTo||'/elsewhere/copy.json';},ask:async()=>true},
      path:{appDataDir:async()=>'/appdata'},
      fs:{readTextFile:async p=>{if(h.snapshots&&h.snapshots[p])return h.snapshots[p];assert.fail('project bytes in JS');},
        mkdir:async d=>dirs.push(d),writeTextFile:async(p,t)=>files.push([p,t])}};
    Object.assign(root,extra);
  }}),{files,dirs,dialogs});
}

test('opening a baseline file through project.js makes a new unsaved document and never remembers the old path',async()=>{
  const h=host();h.legacyFor=p=>p==='/Users/me/old-film.json';
  const app=ordinary(h);await turn();await turn();
  await app.project.openPath('/Users/me/old-film.json');await turn();
  assert.equal(app.project.getCurrentLabel(),'old-film (not saved)');
  assert.equal(app.project.isDirty(),true,'unsaved until its first Save As');
  assert.ok(app.toasts.some(t=>/^Opened: old-film — imported as a new project; the original file is not modified/.test(t)),JSON.stringify(app.toasts));
  assert.equal(app.writes.some(([key])=>/recent/i.test(key)),false,'the baseline path is not added to Recents');
  assert.equal(app.mutations,0,'no legacy writer ran');
  // The first Save goes through Save As, and the original file is never an acceptable destination.
  h.saveTo='/Users/me/OLD-FILM.json';
  const refused=await app.project.saveAs();
  assert.equal(h.calls.some(c=>c.command==='nemo_native_object_save_as'),false,'nothing was written over the original');
  assert.ok(app.toasts.some(t=>/never overwritten/.test(t)),JSON.stringify(app.toasts));
  assert.notEqual(refused&&refused.acknowledged,true);
  h.saveTo='/Users/me/old-film-v2.json';
  await app.project.saveAs();await turn();
  const save=h.calls.find(c=>c.command==='nemo_native_object_save_as');
  assert.ok(save,'a different destination saves normally');
  assert.equal(save.r.destinationPath,'/Users/me/old-film-v2.json');
});

test('an ordinary native file still opens as itself: titled, clean, remembered',async()=>{
  const h=host();
  const app=ordinary(h);await turn();await turn();
  await app.project.openPath('/Users/me/native.json');await turn();
  assert.equal(app.project.getCurrentLabel().startsWith('native'),true);
  assert.equal(app.project.getCurrentLabel().includes('not saved'),false);
  assert.equal(app.writes.some(([key])=>/recent/i.test(key)),true);
});

test('Resume opens an old baseline autosave when the recovery store is empty',async()=>{
  const h=host();
  const app=ordinary(h,{},OLD);await turn();await turn();
  assert.equal(app.element('start-resume').style.display,'','the card appears for a baseline autosave');
  await app.element('start-resume').listeners.click();await turn();
  assert.deepEqual(app.files.filter(f=>f[0].startsWith('/appdata/legacy-import/')),[['/appdata/legacy-import/Session.json',OLD]]);
  assert.deepEqual(h.calls.filter(c=>c.command==='nemo_native_object_bootstrap').map(c=>c.r.sourcePath),['/appdata/legacy-import/Session.json']);
  assert.ok(app.toasts.includes('Session resumed')||app.toasts.some(t=>/Opened: Session/.test(t)),JSON.stringify(app.toasts));
});

test('restoring a history snapshot replaces the open document with the converted snapshot',async()=>{
  const h=host();h.legacyFor=p=>p.includes('legacy-import');h.snapshots={'/appdata/history/film/1.json':OLD};
  const app=ordinary(h);await turn();await turn();
  await app.project.openPath('/Users/me/native.json');await turn();
  const before=h.calls.filter(c=>c.command==='nemo_native_object_bootstrap').length;
  assert.equal(await app.project.restoreVersion('/appdata/history/film/1.json'),true);await turn();
  const opens=h.calls.filter(c=>c.command==='nemo_native_object_bootstrap');
  assert.equal(opens.length,before+1);
  assert.equal(opens.at(-1).r.sourcePath.startsWith('/appdata/legacy-import/'),true);
  assert.equal(opens.at(-1).r.replace.documentId,'document-1','the open document is the one replaced');
  assert.ok(app.toasts.includes('toastVersionRestored'),JSON.stringify(app.toasts));
});

test('a typed refusal names the field in the toast, keeps the open document and never falls back to the legacy importer',async()=>{
  const h=host(),base=h.invoke;
  h.invoke=async(command,args)=>{
    const r=args&&JSON.parse(args.requestJson);
    if(command==='nemo_native_object_bootstrap'&&r.sourcePath==='/Users/me/bad.json')
      throw {code:'legacy_unsupported_field',message:'legacy project field layers[0].motion: this layer content has no native owner in this release',details:{disposition:'notDispatched',retryExecution:false}};
    if(command==='nemo_native_object_inactive_project'&&r.action.kind!=='prepare')
      return {apiVersion:2,requestId:r.requestId,instanceId:r.instanceId,documentId:r.documentId,contentRevision:r.expectedRevision,
        documentSnapshotId:r.documentSnapshotId,lifecycleGeneration:r.lifecycleGeneration,state:'inactive-absent',preparationToken:null,project:null,retryExecution:false};
    if(command==='nemo_native_object_inactive_project'&&r.action.sourcePath==='/Users/me/bad.json')
      throw {code:'legacy_unsupported_field',message:'legacy project field layers[3].isNullLayer: this layer content has no native owner in this release',details:{disposition:'notDispatched',retryExecution:false}};
    return base(command,args);
  };
  const app=ordinary(h);await turn();await turn();
  assert.equal(await app.project.openPath('/Users/me/bad.json'),false,'refused at the start screen');
  assert.ok(app.toasts.some(t=>/layers\[0\]\.motion/.test(t)),JSON.stringify(app.toasts));
  assert.equal(app.mutations,0,'the legacy importer did not run');
  await app.project.openPath('/Users/me/native.json');await turn();
  const label=app.project.getCurrentLabel();
  assert.deepEqual(await app.project.openPath('/Users/me/bad.json'),{status:'refused'},'refused into a new tab while a project is open');
  const bar=app.element('project-tabs-bar').children.at(-1);
  assert.match(String(bar&&bar.textContent),/layers\[3\]\.isNullLayer/,'the tab bar alert names the field');
  assert.equal(app.project.getCurrentLabel(),label,'the open document is untouched by a refusal');
  assert.ok(h.calls.every(c=>c.command!=='nemo_native_object_save_as'));
});

test('a baseline file opened into a new tab carries no path, so a Save can never reach the original',async()=>{
  const h=host(),base=h.invoke;
  h.invoke=async(command,args)=>{
    const r=args&&JSON.parse(args.requestJson);
    if(command!=='nemo_native_object_inactive_project')return base(command,args);
    const project=r.action.kind==='prepare'?{instanceId:r.instanceId,contentRevision:0,documentSnapshotId:'snapshot-B',documentDigest:'b'.repeat(64),byteCount:128,
      canvasW:320,canvasH:180,fps:24,totalFrames:2,layers:[{layerUid:'layer-B'}],preparationToken:'inactive-B',preparationRequestId:r.requestId,
      origin:{documentId:r.documentId,contentRevision:r.expectedRevision,documentSnapshotId:r.documentSnapshotId},legacyImport:REPORT}:null;
    return {apiVersion:2,requestId:r.requestId,instanceId:r.instanceId,documentId:r.documentId,contentRevision:r.expectedRevision,
      documentSnapshotId:r.documentSnapshotId,lifecycleGeneration:r.lifecycleGeneration,state:project?'inactive-prepared':'inactive-absent',
      preparationToken:project&&project.preparationToken,project,retryExecution:false};
  };
  const app=ordinary(h);await turn();await turn();
  await app.project.openPath('/Users/me/native.json');await turn();
  const result=await app.project.openPath('/Users/me/old-film.json');
  assert.equal(result.status,'prepared');
  assert.equal(result.meta.path,null);assert.equal(result.meta.name,'old-film');assert.equal(result.meta.legacy,true);
  const bar=app.element('project-tabs-bar').children.at(-1);
  assert.match(String(bar&&bar.textContent),/Imported as a new project; the original file is not modified/);
});
