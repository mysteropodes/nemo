'use strict';
// #2360 R2: pin the session info() field contract the recovery scheduler consumes (T12 lane isolation).
const test=require('node:test');
const assert=require('node:assert/strict');
const Project=require('../src/js/adapters/native-object-project.js');
const Files=require('../src/js/adapters/native-object-project-file.js');
const Workflow=require('../src/js/application/native-object-save-as-workflow.js');
const Ack=require('../src/js/application/native-object-saved-acknowledgement.js');
const Recovery=require('../src/js/adapters/native-object-recovery.js');
const turn=()=>new Promise(setImmediate);
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

test('a real opened native session exposes every info() field the recovery scheduler reads', async () => {
  const h = host(), s = session(h), f = fileUI(s);
  assert.equal(await f.ui.newProject({ w: 64, h: 64, fps: 24, name: 'Pin' }), true);
  const info = s.info();
  const read = Recovery.readInfo(info);
  assert.ok(read, 'info() satisfies the scheduler contract');
  assert.deepEqual(Object.keys(read), Recovery.INFO_FIELDS);
  for (const field of Recovery.INFO_FIELDS) assert.deepEqual(read[field], info[field], field);
  assert.equal(Number.isSafeInteger(info.contentRevision), true);
  assert.match(info.documentDigest, /^[a-f0-9]{64}$/);
  assert.equal(info.byteCount > 0, true);
  // Retyping or dropping any consumed field must be refused, so a T12 change fails here and not in the field.
  for (const field of Recovery.INFO_FIELDS) {
    const dropped = { ...info }; delete dropped[field];
    assert.equal(Recovery.readInfo(dropped), null, 'missing ' + field);
    assert.equal(Recovery.readInfo({ ...info, [field]: typeof info[field] === 'number' ? String(info[field]) : 7 }), null, 'retyped ' + field);
  }
  assert.equal(Recovery.readInfo({ ...info, documentDigest: 'A'.repeat(64) }), null);
  assert.equal(Recovery.readInfo(null), null);
});

test('the file UI recovery pin reports a freshly created project clean and reports its first digest as the reference', async () => {
  const h = host(), s = session(h), f = fileUI(s);
  assert.equal(typeof f.ui.recoveryPin, 'function');
  assert.equal(await f.ui.newProject({ w: 64, h: 64, fps: 24, name: 'Pin' }), true);
  const pin = f.ui.recoveryPin();
  assert.equal(pin.active, true); assert.equal(pin.busy, false); assert.equal(pin.presented, true);
  assert.equal(pin.reference, pin.info.documentDigest, 'a new project is clean at its first digest');
  assert.equal(f.ui.isDirty(), true, 'the legacy dirty rule is unchanged: an unsaved new project still guards close');
});

test('the recovery pin follows an acknowledged Save As and reports busy while opening', async () => {
  const h = host(), s = session(h), f = fileUI(s);
  assert.equal(await f.ui.newProject({ w: 64, h: 64, fps: 24, name: 'Pin' }), true);
  const saved = await f.ui.saveAs();
  assert.equal(saved.acknowledged, true);
  const pin = f.ui.recoveryPin();
  assert.equal(pin.reference, pin.info.documentDigest); assert.equal(f.ui.isDirty(), false);
  const gate = h.before = (command) => command === 'nemo_native_object_bootstrap' ? new Promise((resolve) => { h.release = resolve; }) : undefined;
  const opening = f.ui.newProject({ w: 64, h: 64, fps: 24, name: 'Again' });
  await turn(); await turn();
  assert.equal(f.ui.recoveryPin().busy, true, 'opening is busy');
  h.release(); h.before = null; await opening;
  assert.equal(f.ui.recoveryPin().busy, false);
  assert.ok(gate);
});
