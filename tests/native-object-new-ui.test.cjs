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
function deferred(){let resolve;const promise=new Promise(done=>{resolve=done;});return {promise,resolve};}
const intent={canvasW:640,canvasH:360,fps:30};
function host(){
  let installed=false,number=0,meta=null;
  const calls=[],events=[];
  const h={calls,events,before:null,after:null,info:()=>meta};
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
      installed=true;number++;
      meta={apiVersion:2,instanceId:'instance',documentId:'document-'+number,contentRevision:0,
        documentSnapshotId:'snapshot-'+number,lifecycleGeneration:number,documentDigest:String(number).repeat(64),byteCount:1000,
        ...cfg,totalFrames:Math.round(cfg.fps*5),layers:[{layerUid:'layer-1',name:'Layer 1',visible:true}],reference:null,undoDepth:0,redoDepth:0};
      result={apiVersion:2,requestId:r.requestId,instanceId:'instance',documentId:meta.documentId,
        contentRevision:0,resourceCount:0,viewportAvailable:false};
    }else if(command==='nemo_native_object_reference')result={...meta,requestId:r.requestId};
    else if(command==='nemo_native_object_preview'){events.push('present:'+meta.documentId);result={status:'presented',workId:'work-'+calls.length,viewGeneration:number};}
    else if(command==='nemo_native_object_save_as')result={apiVersion:2,requestId:r.requestId,instanceId:r.instanceId,documentId:r.documentId,
      atRevision:r.atRevision,documentSnapshotId:r.documentSnapshotId,byteCount:meta.byteCount,published:true,cleanupPending:false};
    else throw Error('Unexpected native route: '+command);
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
