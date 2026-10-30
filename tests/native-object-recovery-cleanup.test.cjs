'use strict';
// #2360 R4: recovery cleanup and retention triggers. The cleanup module against the real scheduler with fake ports, then
// the triggers (Save As acknowledged, replace, clean quit) against a real session + file UI + project.js on a fake host.
const test=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const vm=require('node:vm');
const Project=require('../src/js/adapters/native-object-project.js');
const Files=require('../src/js/adapters/native-object-project-file.js');
const Workflow=require('../src/js/application/native-object-save-as-workflow.js');
const Ack=require('../src/js/application/native-object-saved-acknowledgement.js');
const Recovery=require('../src/js/adapters/native-object-recovery.js');
const Cleanup=require('../src/js/adapters/native-object-recovery-cleanup.js');
const Resume=require('../src/js/adapters/native-object-recovery-resume.js');
const turn=()=>new Promise(setImmediate);
const hex=n=>n.toString(16).padStart(64,'0');
function deferred(){let resolve;const promise=new Promise(done=>{resolve=done;});return {promise,resolve};}

// ---- the cleanup module against the real scheduler ------------------------------------------------------------------
function rig(options={}){
  let now=1_000_000,serial=0,timers=[],timerId=0;
  const r={writes:[],discards:[],gate:null,discardGate:null,discardFails:new Set(),rev:1,stopped:0,...options};
  const scheduler=Recovery.create({
    now:()=>now,setTimer:(fn,ms)=>{timers.push({id:++timerId,at:now+ms,fn});return timerId;},clearTimer:id=>{timers=timers.filter(t=>t.id!==id);},
    blocked:()=>false,slot:()=>r.slot||{path:null,name:'Untitled'},notice:()=>{},requestId:k=>'project-'+k+'-'+(++serial),
    hash:()=>'h4sh',
    pin:()=>({active:true,busy:false,presented:true,reference:hex(0),info:{instanceId:'instance',documentId:r.documentId||'document-1',
      lifecycleGeneration:1,contentRevision:r.rev,documentSnapshotId:'snapshot-'+r.rev,documentDigest:hex(r.rev),byteCount:1000}}),
    write:async(command,request)=>{
      r.writes.push(request);if(r.gate)await r.gate.promise;
      return {apiVersion:2,requestId:request.requestId,instanceId:request.instanceId,documentId:request.documentId,published:true,
        byteCount:1000,savedAtMs:now,cleanupPending:false,supersededRemoved:false};}});
  const realStop=scheduler.stop;
  const wrapped=Object.freeze({...scheduler,stop(){r.stopped++;realStop();}});
  const cleanupTimers=[];
  const cleanup=Cleanup.create({scheduler:wrapped,instanceId:()=>r.noInstance?null:'instance',documentId:()=>r.documentId||'document-1',requestId:k=>'project-'+k+'-'+(++serial),
    setTimer:(fn,ms)=>{const t={fn,ms,cleared:false};cleanupTimers.push(t);return t;},clearTimer:t=>{t.cleared=true;},
    invoke:async(command,args)=>{
      assert.equal(command,'nemo_native_object_recovery_discard');
      const request=JSON.parse(args.requestJson);
      assert.deepEqual(Object.keys(request).sort(),['apiVersion','instanceId','requestId','slotKey']);
      assert.equal(request.apiVersion,2);assert.equal(request.instanceId,'instance');
      r.discards.push(request.slotKey);
      if(r.discardGate)await r.discardGate.promise;
      if(r.discardFails.has(request.slotKey))throw Object.assign(new Error('unavailable'),{code:'unavailable'});
      return {apiVersion:2,requestId:request.requestId,instanceId:request.instanceId,slotKey:request.slotKey,discarded:true};}});
  scheduler.start();
  Object.assign(r,{scheduler:wrapped,raw:scheduler,cleanup,cleanupTimers,advance:ms=>{now+=ms;}});
  return r;
}

test('discardDocument drops exactly the slots the writer published for that document and releases them',async()=>{
  const r=rig();
  assert.equal((await r.raw.flush()).wrote,true);
  r.documentId='document-2';r.rev=2;r.slot={path:'/synthetic/other.json',name:'other'};r.advance(60_000);
  assert.equal((await r.raw.flush()).wrote,true);
  const first=r.writes[0].slotKey,second=r.writes[1].slotKey;
  assert.notEqual(first,second);
  const out=await r.cleanup.discardDocument('document-1');
  assert.deepEqual(r.discards,[first]);assert.deepEqual(out,{discarded:[first],failed:[]});
  assert.deepEqual(r.scheduler.claim(),[second],'the other document keeps its slot');
});

test('an extra slot (the resumed slot) is discarded with the document and duplicates are sent once',async()=>{
  const r=rig();await r.raw.flush();
  const slot=r.writes[0].slotKey;
  await r.cleanup.discardDocument('document-1',['untitled-aaaa1111',slot]);
  assert.deepEqual(r.discards,[slot,'untitled-aaaa1111'].sort((a,b)=>a===slot?-1:1));
  assert.equal(r.discards.length,2);
});

test('nothing owned, no instance, or a malformed slot name sends nothing',async()=>{
  const r=rig();
  assert.deepEqual(await r.cleanup.discardDocument('document-1'),{discarded:[],failed:[]});
  await r.raw.flush();r.noInstance=true;
  assert.deepEqual((await r.cleanup.discardDocument('document-1')).discarded,[]);
  r.noInstance=false;
  const out=await r.cleanup.discardDocument('document-9',['../etc','Bad Slot','']);
  assert.deepEqual(out.discarded,[]);assert.equal(r.discards.length,0);
});

test('a failing discard never throws, keeps the slot owned, and a later attempt retries it',async()=>{
  const r=rig();await r.raw.flush();
  const slot=r.writes[0].slotKey;r.discardFails.add(slot);
  const out=await r.cleanup.discardDocument('document-1');
  assert.deepEqual(out,{discarded:[],failed:[slot]});assert.deepEqual(r.scheduler.claim(),[slot]);
  r.discardFails.clear();
  assert.deepEqual((await r.cleanup.discardDocument('document-1')).discarded,[slot]);assert.deepEqual(r.scheduler.claim(),[]);
});

test('a write still in flight lands before the discard and its slot is covered',async()=>{
  const r=rig({gate:deferred()});
  const writing=r.raw.flush();await turn();
  assert.equal(r.writes.length,1);
  const done=r.cleanup.discardDocument('document-1');await turn();
  assert.equal(r.discards.length,0,'waits for the in-flight write');
  r.gate.resolve();await writing;const out=await done;
  assert.deepEqual(r.discards,[r.writes[0].slotKey]);assert.equal(out.discarded.length,1);
});

test('a write that starts after the claim is not discarded by it',async()=>{
  const r=rig({discardGate:deferred()});await r.raw.flush();
  const first=r.writes[0].slotKey;
  const done=r.cleanup.discardDocument('document-1');
  r.rev=2;r.slot={path:'/synthetic/saved.json',name:'saved'};r.advance(60_000);await r.raw.flush();
  const later=r.writes[1].slotKey;assert.notEqual(later,first);
  r.discardGate.resolve();await done;
  assert.deepEqual(r.discards,[first]);assert.deepEqual(r.scheduler.claim(),[later],'the post-save recovery survives');
});

test('a hanging discard is abandoned at the bound instead of blocking the caller',async()=>{
  const r=rig({discardGate:deferred()});await r.raw.flush();
  const done=r.cleanup.discardDocument('document-1');await turn();
  const timer=r.cleanupTimers.at(-1);assert.equal(timer.ms,Cleanup.LIMITS.discardMs);timer.fn();
  const out=await done;assert.equal(out.discarded.length,0);assert.equal(out.failed.length,1);
  r.discardGate.resolve();
});

test('cleanQuit stops the writer, waits for an in-flight write, drops every slot, and is bounded',async()=>{
  const r=rig({gate:deferred()});
  const writing=r.raw.flush();await turn();
  const quitting=r.cleanup.cleanQuit();await turn();
  assert.equal(r.stopped,1,'no further writes are scheduled');assert.equal(r.discards.length,0);
  r.gate.resolve();await writing;
  const out=await quitting;
  assert.deepEqual(r.discards,[r.writes[0].slotKey]);assert.equal(out.discarded.length,1);
  r.advance(60_000);r.rev=3;assert.equal((await r.raw.flush()).wrote,false,'stopped writer stays stopped');
  const hang=rig({discardGate:deferred()});await hang.raw.flush();
  const stuck=hang.cleanup.cleanQuit();await turn();
  const timer=hang.cleanupTimers.at(-1);assert.equal(timer.ms,Cleanup.LIMITS.quitMs);timer.fn();
  assert.equal((await stuck).discarded.length,0);hang.discardGate.resolve();
});

test('cleanQuit drops only the active document slots: a dirty parked tab keeps its recovery',async()=>{
  const r=rig();
  r.documentId='document-A';r.slot={path:'/p/a.json',name:'A'};r.rev=2;await r.raw.flush();            // tab A has unsaved edits and a recovery slot
  const slotA=r.writes[0].slotKey;
  r.documentId='document-B';r.slot={path:'/p/b.json',name:'B'};r.rev=3;r.advance(60_000);await r.raw.flush(); // switch to tab B, which also published one
  const slotB=r.writes[1].slotKey;
  assert.notEqual(slotA,slotB);
  assert.deepEqual(r.scheduler.claim().sort(),[slotA,slotB].sort());
  const out=await r.cleanup.cleanQuit();                            // quit with B (clean) active
  assert.deepEqual(r.discards,[slotB],"only the active document's slot is discarded");
  assert.deepEqual(out.discarded,[slotB]);
  assert.deepEqual(r.scheduler.claim(),[slotA],'the parked dirty tab keeps its recovery slot, still owned');
  assert.equal(r.stopped,1,'the writer is still stopped');
});

test('cleanQuit with no resolvable active document discards nothing',async()=>{
  const r=rig();await r.raw.flush();
  const none=Cleanup.create({scheduler:r.scheduler,instanceId:()=>'instance',documentId:()=>null,requestId:()=>'x',
    setTimer:()=>1,clearTimer:()=>{},invoke:async()=>assert.fail('no IPC')});
  assert.deepEqual(await none.cleanQuit(),{discarded:[],failed:[]});
  assert.equal(r.scheduler.claim().length,1);
});

test('cleanQuit with nothing written does no IPC and never throws',async()=>{
  const r=rig();
  assert.deepEqual(await r.cleanup.cleanQuit(),{discarded:[],failed:[]});assert.equal(r.discards.length,0);
  const f=rig();await f.raw.flush();f.discardFails.add(f.writes[0].slotKey);
  assert.equal((await f.cleanup.cleanQuit()).failed.length,1);
});

// ---- triggers against a real session + file UI on a fake host ------------------------------------------------------
const intent={w:640,h:360,fps:30,name:'Project'};
const SLOT='untitled-aaaa1111';
const gen=(n,at)=>({path:'/synthetic/app-data/recovery/'+SLOT+'/rec-'+n+'.json',savedAtMs:at,contentRevision:n,byteCount:900+n});
const ENTRY={slotKey:SLOT,displayName:'Untitled',originPath:null,generations:[gen(2,2000),gen(1,1000)]};
function host(){
  let installed=false,number=0,meta=null;
  const calls=[];
  const h={calls,before:null,failOpen:new Set(),writes:[],discards:[],discardFails:new Set(),discardGate:null,entries:[ENTRY],info:()=>meta};
  h.invoke=async(command,args)=>{
    const r=args?JSON.parse(args.requestJson):null;calls.push({command,r});
    if(h.before)await h.before(command,r);
    if(command==='nemo_native_object_recovery_write'){
      h.writes.push(r);
      return {apiVersion:2,requestId:r.requestId,instanceId:r.instanceId,documentId:r.documentId,published:true,byteCount:1000,
        savedAtMs:Date.now(),cleanupPending:false,supersededRemoved:false};}
    if(command==='nemo_native_object_recovery_discard'){
      h.discards.push(r.slotKey);if(h.discardGate)await h.discardGate.promise;
      if(h.discardFails.has(r.slotKey))throw Object.assign(new Error('unavailable'),{code:'unavailable'});
      return {apiVersion:2,requestId:r.requestId,instanceId:r.instanceId,slotKey:r.slotKey,discarded:true};}
    if(command==='nemo_native_object_recovery_list')return {apiVersion:2,requestId:r.requestId,instanceId:r.instanceId,entries:h.entries,quarantined:[]};
    let result;
    if(command==='nemo_native_object_client_status')result={apiVersion:2,instanceId:'instance',available:installed,...meta};
    else if(command==='nemo_native_object_bootstrap'){
      if(r.sourcePath&&h.failOpen.has(r.sourcePath))throw {message:'Native project could not be read',code:'unsupported_format',details:{disposition:'notDispatched'}};
      const cfg=r.newProject||{canvasW:320,canvasH:180,fps:24};
      installed=true;number++;
      meta={apiVersion:2,instanceId:'instance',documentId:'document-'+number,contentRevision:0,
        documentSnapshotId:'snapshot-'+number,lifecycleGeneration:number,documentDigest:String(number).repeat(64),byteCount:1000,
        ...cfg,totalFrames:Math.round(cfg.fps*5),layers:[{layerUid:'layer-1',name:'Layer 1',visible:true}],reference:null,undoDepth:0,redoDepth:0};
      result={apiVersion:2,requestId:r.requestId,instanceId:'instance',documentId:meta.documentId,contentRevision:0,resourceCount:0,viewportAvailable:false};
    }else if(command==='nemo_native_object_reference')result={...meta,requestId:r.requestId};
    else if(command==='nemo_native_object_preview')result={status:'presented',workId:'work-'+calls.length,viewGeneration:number};
    else if(command==='nemo_native_object_save_as')result={apiVersion:2,requestId:r.requestId,instanceId:r.instanceId,documentId:r.documentId,
      atRevision:r.atRevision,documentSnapshotId:r.documentSnapshotId,byteCount:meta.byteCount,published:true,cleanupPending:false};
    else throw Error('Unexpected native route: '+command);
    return result;
  };
  return h;
}
function session(h){return Project.create({invoke:h.invoke,
  mapping:info=>({cssBounds:{width:640,height:360},physicalExtent:{width:640,height:360},compositionExtent:{width:info.canvasW,height:info.canvasH}})});}
// The scheduler is the real one; only the dirty test is forced (the digest rule is covered by the R2 tests).
function wire(h,extra={},acknowledgement=Ack){
  const s=session(h),state={confirms:0,confirm:true,pick:'/new.json',effects:[]};let serial=0;
  const holder={};
  const scheduler=Recovery.create({now:()=>Date.now(),setTimer:()=>1,clearTimer:()=>{},blocked:()=>false,
    slot:()=>({path:null,name:'Untitled'}),notice:()=>{},requestId:k=>'project-'+k+'-'+(++serial),hash:()=>'h4sh',
    pin:()=>{const p=holder.ui.recoveryPin();return {...p,presented:true,reference:hex(0)};},
    write:(command,request)=>h.invoke(command,{requestJson:JSON.stringify(request)})});
  scheduler.start();
  const cleanup=Cleanup.create({scheduler,instanceId:()=>s.info()&&s.info().instanceId,invoke:h.invoke,requestId:k=>s.requestId(k),
    setTimer:(fn,ms)=>setTimeout(fn,ms),clearTimer:t=>clearTimeout(t)});
  const ui=Files.create(s,{readUI:()=>({handle:'tab',generation:1}),confirmReplace:async()=>{state.confirms++;return state.confirm;},
    occlude:()=>{},reveal:()=>{},restore:()=>{},opened:()=>true,created:()=>true,commit:()=>true,repaint:()=>{},
    pick:async()=>state.pick,notice:()=>{},failed:()=>state.effects.push('failed'),refused:()=>state.effects.push('refused'),
    resumedNotice:()=>{},recoveryRefused:()=>{},discardRecovery:(d,e)=>cleanup.discardDocument(d,e),...extra},Workflow,acknowledgement);
  holder.ui=ui;
  return {s,ui,scheduler,cleanup,state};
}
const settle=async()=>{for(let i=0;i<8;i++)await turn();};

test('an acknowledged Save As drops the project recovery and a failing discard never fails the Save',async()=>{
  const h=host(),w=wire(h);
  assert.equal(await w.ui.newProject(intent),true);
  assert.equal((await w.scheduler.flush()).wrote,true);
  const slot=h.writes[0].slotKey;
  const saved=await w.ui.saveAs();
  assert.equal(saved.acknowledged,true);await settle();
  assert.deepEqual(h.discards,[slot]);
  const again=host(),b=wire(again);
  assert.equal(await b.ui.newProject(intent),true);await b.scheduler.flush();
  again.discardFails.add(again.writes[0].slotKey);
  const result=await b.ui.saveAs();await settle();
  assert.equal(result.acknowledged,true,'Save is unaffected by the cleanup failure');
  assert.equal(again.discards.length,1);
});

test('Save As that is cancelled, refused, stale or not confirmed saved leaves the recovery in place',async()=>{
  const cancelled=host(),a=wire(cancelled);a.state.pick=null;
  await a.ui.newProject(intent);await a.scheduler.flush();
  assert.equal((await a.ui.saveAs()).acknowledged,false);await settle();assert.deepEqual(cancelled.discards,[]);
  const refused=host(),b=wire(refused);
  await b.ui.newProject(intent);await b.scheduler.flush();
  refused.before=async(command)=>{if(command==='nemo_native_object_save_as')throw new Error('disk full');};
  assert.equal((await b.ui.saveAs()).acknowledged,false);await settle();assert.deepEqual(refused.discards,[]);
  for(const status of ['written-stale','written-indeterminate']){
    const stale=host(),ack={...Ack,create:(p)=>{const real=Ack.create(p);
      return {...real,acknowledge:async(...args)=>({...(await real.acknowledge(...args)),status,acknowledged:false})};}};
    const c=wire(stale,{},ack);
    await c.ui.newProject(intent);await c.scheduler.flush();
    assert.equal((await c.ui.saveAs()).acknowledged,false);await settle();
    assert.deepEqual(stale.discards,[],status);
  }
});

test('Save As of a resumed session also drops the slot it was resumed from',async()=>{
  const h=host(),w=wire(h);
  assert.equal(await w.ui.openRecovered(ENTRY),true);
  assert.equal(w.ui.recoveryPin().supersedes,SLOT);
  assert.equal((await w.ui.saveAs()).acknowledged,true);await settle();
  assert.deepEqual(h.discards,[SLOT]);assert.equal(w.ui.recoveryPin().supersedes,null,'nothing left to supersede');
});

test('replacing a project through New drops the replaced project recovery only after the new project is in',async()=>{
  const h=host(),w=wire(h);
  await w.ui.newProject(intent);await w.scheduler.flush();
  const slot=h.writes[0].slotKey;
  const gate=deferred();h.before=async(command)=>{if(command==='nemo_native_object_bootstrap')await gate.promise;};
  const replacing=w.ui.newProject({...intent,name:'Next'});await settle();
  assert.equal(w.state.confirms,1);assert.deepEqual(h.discards,[],'not before the replacement exists');
  gate.resolve();assert.equal(await replacing,true);await settle();
  assert.deepEqual(h.discards,[slot]);
});

test('a declined or failed replace keeps the recovery of the project that stays',async()=>{
  const declined=host(),a=wire(declined);a.state.confirm=false;
  await a.ui.newProject(intent);await a.scheduler.flush();
  assert.equal(await a.ui.newProject(intent),false);assert.equal(a.state.confirms,1);await settle();
  assert.deepEqual(declined.discards,[]);
  const failing=host(),b=wire(failing);
  await b.ui.newProject(intent);await b.scheduler.flush();
  failing.failOpen.add('/synthetic/broken.json');
  assert.equal(await b.ui.openPath('/synthetic/broken.json'),false);await settle();
  assert.deepEqual(failing.discards,[]);assert.equal(b.state.effects.includes('refused'),true);
});

test('Open over a project drops that project recovery, and so does resuming a slot over it',async()=>{
  const h=host(),w=wire(h);
  await w.ui.newProject(intent);await w.scheduler.flush();
  const slot=h.writes[0].slotKey;
  assert.equal(await w.ui.openPath('/synthetic/other.json'),true);await settle();
  assert.deepEqual(h.discards,[slot]);
  const r=host(),x=wire(r);
  await x.ui.newProject(intent);await x.scheduler.flush();
  assert.equal(await x.ui.openRecovered(ENTRY),true);await settle();
  assert.deepEqual(r.discards,[r.writes[0].slotKey],'the replaced project recovery, not the resumed slot');
});

test('a replaced resumed project drops the slot it came from',async()=>{
  const h=host(),w=wire(h);
  assert.equal(await w.ui.openRecovered(ENTRY),true);
  assert.equal(await w.ui.newProject(intent),true);await settle();
  assert.deepEqual(h.discards,[SLOT]);
});

// ---- project.js: clean quit and the retained recovery after a confirmed quit-without-saving ----------------------------
function ordinary(h,extra={}){
  const fixture=fs.readFileSync('tests/project-open.test.cjs','utf8');
  const start=fixture.indexOf('function harness('),end=fixture.indexOf('\nfunction select(',start);
  const code=fixture.slice(start,end).replace('configureWindow(window);',
    "window.state=context.state;window.document=document;window.userLayers=context.userLayers;document.getElementById('drawing-canvas').getBoundingClientRect=()=>({left:0,top:0,width:640,height:360});configureWindow(window);");
  const context=vm.createContext({assert,vm,require,setTimeout,console,source:fs.readFileSync('src/js/project.js','utf8'),
    documentSource:fs.readFileSync('src/js/project-document.js','utf8'),entrySource:fs.readFileSync('src/js/project-entry.js','utf8')});
  vm.runInContext(code+'\nthis.makeHarness=harness;',context);
  const closer={callback:null,destroyed:0,asks:0,answer:true},captured={};
  const app=context.makeHarness({dirtyDot:true,auto:null,configureWindow(root){
    root.document.getElementById('start-resume').style.display='none';
    require('./helpers/native-viewport-underlay.cjs').install(root);
    root.NemoNativeObjectProject=Project;root.NemoNativeObjectProjectFile=Files;root.NemoNativeObjectRecoveryResume=Resume;
    root.NemoNativeObjectSaveAsWorkflow=Workflow;root.NemoNativeObjectSavedAcknowledgement=Ack;
    root.NemoNativeObjectRecoveryCleanup=Cleanup;
    root.NemoNativeObjectRecovery={bind(r,ports){let serial=0;
      captured.scheduler=Recovery.create({now:()=>Date.now(),setTimer:()=>1,clearTimer:()=>{},blocked:()=>false,slot:ports.slot,notice:ports.notice,
        requestId:k=>'project-'+k+'-'+(++serial),hash:()=>'h4sh',
        pin:()=>({...ports.pin(),presented:true,reference:hex(0)}),
        write:(command,request)=>h.invoke(command,{requestJson:JSON.stringify(request)})});
      return captured.scheduler;}};
    root.NemoNativeOpacityCutover={blocksLegacy:()=>true,requestRelease:()=>assert.fail('legacy release')};
    root.crypto={randomUUID:require('node:crypto').randomUUID};
    for(const id of ['project-tab-add','start-new'])root.document.getElementById(id).click=function(){return this.listeners.click();};
    root.SM.exportJSON=()=>assert.fail('legacy serialization');
    root.__TAURI__={core:{invoke:h.invoke},dialog:{save:async()=>'/ordinary-new.json',ask:async()=>{closer.asks++;return closer.answer;}},
      fs:{readTextFile:()=>assert.fail('project bytes in JS'),writeTextFile:()=>assert.fail('legacy file write')},
      window:{getCurrentWindow:()=>({onCloseRequested(callback){closer.callback=callback;},async destroy(){closer.destroyed++;}})}};
    root.clearTimeout=clearTimeout;
    Object.assign(root,extra);
  }});
  return {app,closer,captured};
}
async function close(closer){
  let prevented=0;
  await closer.callback({preventDefault(){prevented++;}});
  return prevented;
}

test('a clean quit drops the recovery before the window closes, and never blocks the close',async()=>{
  const h=host(),{app,closer,captured}=ordinary(h);await settle();
  assert.equal(await app.project.openPath('/synthetic/saved.json'),true);
  assert.equal(app.project.isDirty(),false,'an opened, unedited project is clean');
  assert.equal((await captured.scheduler.flush()).wrote,true);
  const slot=h.writes[0].slotKey;
  assert.equal(await close(closer),0,'the close request is not cancelled');
  assert.deepEqual(h.discards,[slot]);assert.equal(closer.asks,0,'no prompt for a clean project');
  assert.equal((await captured.scheduler.flush()).wrote,false,'writing stopped');
  const stuck=host(),other=ordinary(stuck);await settle();
  await other.app.project.openPath('/synthetic/saved.json');await other.captured.scheduler.flush();
  stuck.discardFails.add(stuck.writes[0].slotKey);
  assert.equal(await close(other.closer),0,'a failing discard cannot stop the quit');
});

test('quitting without saving keeps the recovery, and cancelling the quit keeps it too',async()=>{
  const h=host(),{app,closer,captured}=ordinary(h);await settle();
  assert.equal(await app.project.newProject({w:640,h:360,fps:30,name:'Project'}),true);
  assert.equal(app.project.isDirty(),true);
  await captured.scheduler.flush();
  closer.answer=false;assert.equal(await close(closer),1);
  assert.deepEqual(h.discards,[]);assert.equal(closer.destroyed,0);
  closer.answer=true;assert.equal(await close(closer),1);
  assert.equal(closer.asks,2);assert.equal(closer.destroyed,1);assert.deepEqual(h.discards,[],'a confirmed discard-quit keeps the recovery');
  assert.equal(await closer.callback({preventDefault(){assert.fail('second close is let through');}}),undefined);
  assert.deepEqual(h.discards,[]);
});

test('the start screen list is the startup sweep: it is requested with the instance id and nothing else',async()=>{
  const h=host(),{app}=ordinary(h);await settle();
  const lists=h.calls.filter(c=>c.command==='nemo_native_object_recovery_list');
  assert.equal(lists.length,1);assert.deepEqual(Object.keys(lists[0].r).sort(),['apiVersion','instanceId','requestId']);
  assert.ok(app);
});

test('production loads the cleanup adapter before project.js and registers it in the app-js profile',()=>{
  const html=fs.readFileSync('src/index.html','utf8');
  const at=html.indexOf('js/adapters/native-object-recovery-cleanup.js');
  assert.ok(at>=0&&at<html.indexOf('js/project.js',at));
  const profile=fs.readFileSync('engineering/boundaries/profiles/app-js.profile.json','utf8');
  assert.ok(profile.includes('native-object-recovery-cleanup.js'));
  const source=fs.readFileSync('src/js/adapters/native-object-recovery-cleanup.js','utf8').split('\n').filter(l=>l.trim()).length;
  assert.ok(source<=500);
});
