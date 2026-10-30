'use strict';
// #2360 R3: native Resume card and openRecovered. Real session + file UI + project.js against a fake native host.
const test=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const vm=require('node:vm');
const Project=require('../src/js/adapters/native-object-project.js');
const Files=require('../src/js/adapters/native-object-project-file.js');
const Workflow=require('../src/js/application/native-object-save-as-workflow.js');
const Ack=require('../src/js/application/native-object-saved-acknowledgement.js');
const Resume=require('../src/js/adapters/native-object-recovery-resume.js');
const Recovery=require('../src/js/adapters/native-object-recovery.js');
const turn=()=>new Promise(setImmediate);
function deferred(){let resolve;const promise=new Promise(done=>{resolve=done;});return {promise,resolve};}
const intent={canvasW:640,canvasH:360,fps:30};
const SLOT='untitled-aaaa1111';
const gen=(n,at)=>({path:'/synthetic/app-data/recovery/'+SLOT+'/rec-'+n+'.json',savedAtMs:at,contentRevision:n,byteCount:900+n});
const ENTRY={slotKey:SLOT,displayName:'Untitled',originPath:null,generations:[gen(2,2000),gen(1,1000)]};
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
function ordinary(h,extra={},auto=null){
  const fixture=fs.readFileSync('tests/project-open.test.cjs','utf8');
  const start=fixture.indexOf('function harness('),end=fixture.indexOf('\nfunction select(',start);
  const code=fixture.slice(start,end).replace('configureWindow(window);',
    "window.state=context.state;window.document=document;window.userLayers=context.userLayers;document.getElementById('drawing-canvas').getBoundingClientRect=()=>({left:0,top:0,width:640,height:360});configureWindow(window);");
  const context=vm.createContext({assert,vm,require,setTimeout,console,source:fs.readFileSync('src/js/project.js','utf8'),
    documentSource:fs.readFileSync('src/js/project-document.js','utf8'),entrySource:fs.readFileSync('src/js/project-entry.js','utf8')});
  vm.runInContext(code+'\nthis.makeHarness=harness;',context);
  return context.makeHarness({dirtyDot:true,auto,configureWindow(root){
    root.document.getElementById('start-resume').style.display='none';
    require('./helpers/native-viewport-underlay.cjs').install(root);
    root.NemoNativeObjectProject=Project;root.NemoNativeObjectProjectFile=Files;root.NemoNativeObjectRecoveryResume=Resume;
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

// Recovery store + failure injection layered over the shared fake native host.
function recoveryHost(entries,failing=new Set()){
  const h=host(),base=h.invoke,opened=[];
  h.opened=opened;h.entries=entries;h.failing=failing;h.listCalls=0;h.listFails=false;
  h.invoke=async(command,args)=>{
    if(command==='nemo_native_object_recovery_list'){
      h.listCalls++;const r=JSON.parse(args.requestJson);
      assert.deepEqual(Object.keys(r).sort(),['apiVersion','instanceId','requestId']);
      if(h.listFails)throw Object.assign(new Error('recovery store unavailable'),{code:'unavailable'});
      return {apiVersion:2,requestId:r.requestId,instanceId:r.instanceId,entries:h.entries,quarantined:[]};
    }
    if(command==='nemo_native_object_bootstrap'){
      const r=JSON.parse(args.requestJson);
      if(r.sourcePath){
        opened.push(r.sourcePath);
        if(h.hold&&!h.held){h.held=true;await h.hold;}
        if(h.failing.has(r.sourcePath))throw {message:'Native project could not be read',code:'unsupported_format',details:{disposition:'notDispatched'}};
      }
    }
    return base(command,args);
  };
  return h;
}
function resumeFile(h,extra={}){
  const s=session(h),events={refused:[],failed:0,resumed:0,recoveryRefused:[],confirms:0,created:[],opened:0};
  const f=fileUI(s,{confirmReplace:async()=>{events.confirms++;return true;},
    created:n=>{events.created.push(n);return true;},opened:()=>{events.opened++;return true;},
    refused:e=>events.refused.push(e),failed:()=>events.failed++,
    resumedNotice:()=>events.resumed++,recoveryRefused:e=>events.recoveryRefused.push(e),...extra});
  return {s,f,events};
}

test('openRecovered admits the newest generation through the ordinary open route: Untitled, dirty, no recents, no recovery path kept',async()=>{
  const h=recoveryHost([ENTRY]),{s,f,events}=resumeFile(h);
  assert.equal(await f.ui.openRecovered(ENTRY),true);
  assert.deepEqual(h.opened,[ENTRY.generations[0].path]);
  const bootstrap=h.calls.filter(c=>c.command==='nemo_native_object_bootstrap').map(c=>c.r);
  assert.equal(bootstrap.length,1);assert.equal(bootstrap[0].projectOnly,true);assert.equal(bootstrap[0].sourcePath,ENTRY.generations[0].path);
  assert.deepEqual(events.created,['Untitled'],'published as an Untitled tab');assert.equal(events.opened,0,'no Open commit, so no recents entry');
  assert.equal(f.meta().path,null,'never bound to the recovery file');assert.equal(f.meta().name,'Untitled');
  assert.equal(f.ui.isDirty(),true,'dirty, so the close guard fires and Save As is offered');
  assert.equal(events.resumed,1);assert.equal(events.refused.length+events.failed,0);
  assert.equal(f.effects.includes('open'),false);
  const pin=f.ui.recoveryPin();
  assert.equal(pin.supersedes,SLOT,'the recovered session names the slot it replaces');
  assert.equal(pin.reference,pin.info.documentDigest,'not rewritten until it is edited');
  assert.ok(s.isActive());
});

test('a corrupt newest generation retries the older generation before any message',async()=>{
  const h=recoveryHost([ENTRY],new Set([ENTRY.generations[0].path])),{f,events}=resumeFile(h);
  assert.equal(await f.ui.openRecovered(ENTRY),true);
  assert.deepEqual(h.opened,ENTRY.generations.map(g=>g.path),'newest first, then the older generation');
  assert.equal(events.refused.length,0,'a retried refusal is not toasted');assert.equal(events.recoveryRefused.length,0);
  assert.equal(events.resumed,1);assert.equal(f.ui.recoveryPin().supersedes,SLOT);
});

test('when every generation is refused the failure is retryable: one notice, Open and New remain usable',async()=>{
  const h=recoveryHost([ENTRY],new Set(ENTRY.generations.map(g=>g.path))),{s,f,events}=resumeFile(h);
  assert.equal(await f.ui.openRecovered(ENTRY),false);
  assert.deepEqual(h.opened,ENTRY.generations.map(g=>g.path));
  assert.equal(events.recoveryRefused.length,1,'one message after the last generation');assert.equal(events.refused.length,0);assert.equal(events.failed,0);
  assert.equal(events.resumed,0);assert.equal(events.created.length,0);
  assert.equal(f.ui.canOpen(),true);assert.equal(s.isActive(),false,'no project was installed');
  assert.equal(f.ui.recoveryPin().supersedes,null);
  h.failing.clear();
  assert.equal(await f.ui.newProject({w:64,h:64,fps:24,name:'After'}),true,'New works after the refusal');
  assert.equal(await f.ui.openPath('/synthetic/other.json'),true,'Open works after the refusal');
});

test('an unconfirmed (dispatched) failure is not retried against the older generation',async()=>{
  const h=recoveryHost([ENTRY]);
  const base=h.invoke;
  h.invoke=async(command,args)=>{
    if(command==='nemo_native_object_bootstrap'&&JSON.parse(args.requestJson).sourcePath){h.opened.push(JSON.parse(args.requestJson).sourcePath);throw new Error('connection dropped');}
    return base(command,args);
  };
  const {f,events}=resumeFile(h);
  assert.equal(await f.ui.openRecovered(ENTRY),false);
  assert.equal(h.opened.length,1,'state is indeterminate: no second attempt');assert.equal(events.failed,1);assert.equal(events.recoveryRefused.length,0);
});

test('a second Resume while one is in flight is refused; replacing an unsaved project asks once even across a retry',async()=>{
  const h=recoveryHost([ENTRY],new Set([ENTRY.generations[0].path])),{f,events}=resumeFile(h);
  assert.equal(await f.ui.newProject({w:64,h:64,fps:24,name:'Work'}),true);
  const gate=deferred();h.hold=gate.promise;
  const first=f.ui.openRecovered(ENTRY);await turn();await turn();
  assert.equal(await f.ui.openRecovered(ENTRY),false,'double click');assert.equal(f.ui.canOpen(),false);
  gate.resolve();assert.equal(await first,true);
  assert.equal(events.confirms,1,'the replace confirmation is not repeated for the older generation');
  assert.deepEqual(h.opened,ENTRY.generations.map(g=>g.path));
});

test('a declined replace confirmation cancels without trying other generations',async()=>{
  const h=recoveryHost([ENTRY]),{f,events}=resumeFile(h,{confirmReplace:async()=>false});
  assert.equal(await f.ui.newProject({w:64,h:64,fps:24,name:'Work'}),true);
  assert.equal(await f.ui.openRecovered(ENTRY),false);
  assert.deepEqual(h.opened,[]);assert.equal(events.recoveryRefused.length,0);
});

test('openRecovered ignores an entry without usable generations',async()=>{
  const h=recoveryHost([]),{f}=resumeFile(h);
  for(const bad of [null,{},{slotKey:SLOT,generations:[]},{generations:[gen(1,1)]}])assert.equal(await f.ui.openRecovered(bad),false);
  assert.deepEqual(h.opened,[]);
});

test('the Resume model lists with the session instance and keeps only well-formed slots, newest first',async()=>{
  const calls=[];
  const old={slotKey:'older-one',displayName:'Old',originPath:'/synthetic/old.json',generations:[gen(3,500)]};
  const entries=[old,ENTRY,{slotKey:'BAD KEY',generations:[gen(1,9999)]},{slotKey:'no-gens',generations:[]},
    {slotKey:'bad-gen',generations:[{path:'relative.json',savedAtMs:1,contentRevision:1,byteCount:1}]}];
  const model=Resume.create({invoke:async(command,args)=>{calls.push(command);
      if(command==='nemo_native_object_client_status')return {apiVersion:2,instanceId:'inst-1'};
      const r=JSON.parse(args.requestJson);return {apiVersion:2,requestId:r.requestId,instanceId:r.instanceId,entries};},
    requestId:k=>'project-'+k+'-1',openRecovered:async()=>true,notice:()=>{},failureText:()=>'x'});
  const found=await model.list();
  assert.deepEqual(found.map(e=>e.slotKey),[SLOT,'older-one']);
  assert.deepEqual(found[0].generations.map(g=>g.savedAtMs),[2000,1000]);
  assert.deepEqual(calls,['nemo_native_object_client_status','nemo_native_object_recovery_list']);
});

test('presence publishes true only for a valid recovery and false for empty, mismatched or failing stores',async()=>{
  async function probe(listing){
    const published=[];
    const model=Resume.create({invoke:async(command,args)=>{
        if(command==='nemo_native_object_client_status')return listing.status||{apiVersion:2,instanceId:'inst-1'};
        if(listing.throws)throw new Error('store down');
        const r=JSON.parse(args.requestJson);return listing.reply?listing.reply(r):{apiVersion:2,requestId:r.requestId,instanceId:r.instanceId,entries:listing.entries||[]};},
      requestId:k=>'project-'+k+'-1',openRecovered:async()=>true,notice:()=>{},failureText:()=>'x'});
    await model.presence(v=>published.push(v));return published;
  }
  assert.deepEqual(await probe({entries:[ENTRY]}),[true]);
  assert.deepEqual(await probe({entries:[]}),[false]);
  assert.deepEqual(await probe({throws:true}),[],'a failing store publishes nothing (card stays hidden)');
  assert.deepEqual(await probe({status:{apiVersion:2}}),[false]);
  assert.deepEqual(await probe({reply:r=>({apiVersion:2,requestId:'other',instanceId:r.instanceId,entries:[ENTRY]})}),[false]);
  assert.deepEqual(await probe({reply:r=>({apiVersion:2,requestId:r.requestId,instanceId:'other',entries:[ENTRY]})}),[false]);
});

test('the start screen shows the card only for a valid recovery and ignores a legacy nemo-auto slot on native desktop',async()=>{
  const none=ordinary(recoveryHost([]),{},'{"legacy":true}');await turn();await turn();
  assert.equal(none.element('start-resume').style.display,'none','legacy autosave does not light the native card');
  assert.equal(none.element('start-cards').classList.contains('has-resume'),false);
  const some=ordinary(recoveryHost([ENTRY]));await turn();await turn();
  assert.equal(some.element('start-resume').style.display,'');
  assert.equal(some.element('start-cards').classList.contains('has-resume'),true);
  const broken=recoveryHost([ENTRY]);broken.listFails=true;
  const failed=ordinary(broken);await turn();await turn();
  assert.equal(failed.element('start-resume').style.display,'none');
});

test('clicking Resume opens the recovery as an Untitled dirty project with no recents entry and leaves Open/New usable',async()=>{
  const h=recoveryHost([ENTRY]),app=ordinary(h);await turn();await turn();
  assert.equal(app.element('start-resume').style.display,'');
  await app.element('start-resume').listeners.click();await turn();
  assert.deepEqual(h.opened,[ENTRY.generations[0].path]);
  assert.equal(app.startScreen.classList.contains('hid'),true,'editor revealed');
  assert.equal(app.project.getCurrentLabel(),'Untitled (not saved)');assert.equal(app.project.isDirty(),true);
  assert.ok(app.toasts.includes('Session resumed'));
  assert.equal(app.writes.some(([key])=>/recent/i.test(key)),false,'no recents write');
  assert.equal(app.writes.some(([,value])=>String(value).includes('/recovery/')),false,'recovery path is not remembered anywhere');
  assert.equal(app.element('project-tabs-list').children.length,1);
  assert.equal(app.mutations,0,'no legacy writer ran');
});

test('Resume with every generation refused toasts once, keeps the card, and Open/New still work',async()=>{
  const h=recoveryHost([ENTRY],new Set(ENTRY.generations.map(g=>g.path))),app=ordinary(h);await turn();await turn();
  await app.element('start-resume').listeners.click();await turn();
  assert.deepEqual(h.opened,ENTRY.generations.map(g=>g.path));
  assert.equal(app.toasts.filter(t=>t==='toastCannotResumeSessionCorrupt').length,1);
  assert.equal(app.element('start-resume').style.display,'','the card stays');assert.equal(app.startScreen.classList.contains('hid'),false);
  assert.equal(Project.desktop(app.window).isActive(),false);
  assert.equal(await app.project.openPath('/synthetic/ok.json'),true);
  assert.equal(app.project.getCurrentLabel().startsWith('ok'),true);
});

test('Resume when the store has emptied since startup says so instead of opening anything',async()=>{
  const h=recoveryHost([ENTRY]),app=ordinary(h);await turn();await turn();
  h.entries=[];
  await app.element('start-resume').listeners.click();await turn();
  assert.deepEqual(h.opened,[]);assert.ok(app.toasts.includes('toastCannotResumeSessionCorrupt'));
});

test('the scheduler sends supersedes with the resumed session first write only',async()=>{
  const hex=n=>n.toString(16).padStart(64,'0');let now=1_000_000,rev=1,serial=0;const writes=[];
  const pin={active:true,busy:false,presented:true,reference:hex(0),supersedes:SLOT};
  const scheduler=Recovery.create({now:()=>now,setTimer:()=>1,clearTimer:()=>{},blocked:()=>false,slot:()=>({path:null,name:'Untitled'}),
    notice:()=>{},requestId:k=>'project-'+k+'-'+(++serial),hash:()=>'h4sh',randomHex:()=>'deadbeef',
    pin:()=>({...pin,info:{instanceId:'instance',documentId:'document-1',lifecycleGeneration:1,contentRevision:rev,
      documentSnapshotId:'snapshot-'+rev,documentDigest:hex(rev),byteCount:1000}}),
    write:async(command,request)=>{writes.push(request);return {apiVersion:2,requestId:request.requestId,instanceId:request.instanceId,
      documentId:request.documentId,published:true,byteCount:1000,savedAtMs:now,cleanupPending:false,supersededRemoved:true};}});
  scheduler.start();
  assert.equal((await scheduler.flush()).wrote,true);
  assert.equal(writes[0].supersedes,SLOT);assert.notEqual(writes[0].slotKey,SLOT);
  rev=2;now+=60_000;assert.equal((await scheduler.flush()).wrote,true);
  assert.equal('supersedes' in writes[1],false,'only the first published write supersedes');
});

test('a malformed pin supersedes is ignored and never reaches the request',async()=>{
  const hex=n=>n.toString(16).padStart(64,'0');const writes=[];let serial=0;
  const scheduler=Recovery.create({now:()=>1_000_000,setTimer:()=>1,clearTimer:()=>{},blocked:()=>false,slot:()=>({path:null,name:'Untitled'}),
    notice:()=>{},requestId:k=>'project-'+k+'-'+(++serial),hash:()=>'h4sh',randomHex:()=>'deadbeef',
    pin:()=>({active:true,busy:false,presented:true,reference:hex(0),supersedes:'../etc',info:{instanceId:'instance',documentId:'document-1',
      lifecycleGeneration:1,contentRevision:1,documentSnapshotId:'snapshot-1',documentDigest:hex(1),byteCount:1000}}),
    write:async(c,r)=>{writes.push(r);return {apiVersion:2,requestId:r.requestId,instanceId:r.instanceId,documentId:r.documentId,published:true,byteCount:1,savedAtMs:1,cleanupPending:false,supersededRemoved:false};}});
  scheduler.start();await scheduler.flush();assert.equal(writes.length,1);assert.equal('supersedes' in writes[0],false);
});

test('production loads the Resume adapter before project.js and registers it in the app-js profile',()=>{
  const html=fs.readFileSync('src/index.html','utf8');
  const at=html.indexOf('js/adapters/native-object-recovery-resume.js');
  assert.ok(at>=0&&at<html.indexOf('js/project.js',at));
  const profile=fs.readFileSync('engineering/boundaries/profiles/app-js.profile.json','utf8');
  assert.ok(profile.includes('native-object-recovery-resume.js'));
});
