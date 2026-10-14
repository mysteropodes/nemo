'use strict';
const test=require('node:test');
const assert=require('node:assert/strict');
const Project=require('../src/js/adapters/native-object-project.js');
const Files=require('../src/js/adapters/native-object-project-file.js');
const turn=()=>new Promise(setImmediate);
function deferred(){let resolve;const promise=new Promise(r=>{resolve=r;});return {promise,resolve};}

// Adapter protocol oracle only; the Rust raw-client test exercises the real owner.
function scenario(layers=[{layerUid:'legacy layer:alpha'},{layerUid:'sibling',visible:true}]){
  let active=false,revision=0,failure=null,hold=null;
  layers=structuredClone(layers);
  const calls=[],changes=[];
  function info(){return {apiVersion:2,instanceId:'visibility-instance',documentId:'visibility-document',
    lifecycleGeneration:1,documentSnapshotId:'visibility-snapshot-'+revision,contentRevision:revision,
    documentDigest:String(revision%10).repeat(64),byteCount:1000,canvasW:320,canvasH:180,fps:24,
    totalFrames:21,layers:structuredClone(layers),reference:null,undoDepth:revision,redoDepth:0};}
  async function invoke(command,args){
    const r=args?JSON.parse(args.requestJson):null;calls.push({command,r});
    if(command==='nemo_native_object_client_status')return {...info(),available:active};
    if(command==='nemo_native_object_bootstrap'){
      active=true;return {...info(),requestId:r.requestId,resourceCount:0,viewportAvailable:false};
    }
    if(command==='nemo_native_object_reference')return {...info(),requestId:r.requestId};
    if(command==='nemo_native_object_preview')return {status:failure==='preview'?'refused':'presented',
      workId:'visible-work-'+revision,viewGeneration:1};
    assert.equal(command,'nemo_native_object_dispatch');
    assert.equal(r.instanceId,'visibility-instance');assert.equal(r.documentId,'visibility-document');
    assert.equal(r.expectedRevision,revision);
    assert.equal(r.operation,'command.document.object.layer.visibility.set');
    assert.equal(r.payload.command,'object.layer.visibility.set');
    assert.equal(typeof r.payload.visible,'boolean');
    if(hold){const gate=hold;hold=null;await gate.promise;}
    const response={apiVersion:2,requestId:r.requestId,instanceId:r.instanceId,documentId:r.documentId,
      contentRevision:revision,ok:failure!=='refused'};
    if(failure==='refused')return {...response,error:{code:'stale_revision'}};
    const layer=layers.find(l=>l.layerUid===r.payload.layerUid);
    assert.ok(layer);const applied=(layer.visible!==false)!==r.payload.visible;
    if(applied){layer.visible=r.payload.visible;revision++;}
    if(failure==='transport-after')throw Error('Native visibility response lost');
    return {...response,contentRevision:revision,documentId:failure==='foreign'?'replaced':r.documentId,
      result:{applied,historyEntriesAdded:applied?1:0}};
  }
  const session=Project.create({invoke,frame:()=>7,mapping:i=>({cssBounds:{x:0,y:0,width:320,height:180},
    physicalExtent:{width:320,height:180},compositionExtent:{width:i.canvasW,height:i.canvasH},reportedDpr:1}),
    changed:i=>changes.push(i)});
  return {session,calls,changes,open:()=>session.openPath('/visibility.json'),
    commands:()=>calls.filter(c=>c.command==='nemo_native_object_dispatch'),
    fail:value=>{failure=value;},hold:gate=>{hold=gate;}};
}

test('strict native visibility metadata remains detached and initial rows preserve false and absence',async()=>{
  const source=[{layerUid:'legacy layer:alpha',name:'Hidden',visible:false},{layerUid:'default'}];
  const h=scenario(source);await h.open();const meta=h.session.info();
  assert.deepEqual(meta.layers,source);assert.equal(Object.hasOwn(meta.layers[1],'visible'),false);
  assert.throws(()=>{meta.layers[0].visible=true;},TypeError);
  source[0].visible=true;assert.equal(meta.layers[0].visible,false);
  const root={state:{},selectedPaths:['old'],userLayers:[]};Files.projectView(root,meta);
  assert.deepEqual(root.state.layers.map(l=>[l.layerUid,l.visible]),[['legacy layer:alpha',false],['default',true]]);
  assert.equal(h.commands().length,0);assert.deepEqual(meta.layers[1],{layerUid:'default'});
});

test('present malformed visibility fences admission and refuses projection before any UI mutation',async()=>{
  for(const visible of [null,undefined,0,'false',[],{}]){
    const h=scenario([{layerUid:'layer',visible}]);await assert.rejects(h.open(),/uncorrelated/);
    assert.equal(h.session.isActive(),false);assert.equal(h.session.blocksLegacy(),true);
    assert.equal(h.session.readPresentedObjectSession(),null);assert.equal(h.commands().length,0);
    const root={state:{canvasW:12,layers:[{layerUid:'original',visible:true}]},selectedPaths:['selected']};
    const before=structuredClone(root);
    assert.throws(()=>Files.projectView(root,{layers:[{layerUid:'layer',visible}]}),/visibility is invalid/);
    assert.deepEqual(root,before);
  }
});

test('queued UID visibility uses refreshed revisions and exposes no optimistic edit',async()=>{
  const h=scenario();await h.open();const before=h.session.info(),gate=deferred();h.hold(gate);
  const hide=h.session.setLayerVisibility('legacy layer:alpha',false);
  const show=h.session.setLayerVisibility('legacy layer:alpha',true);
  for(let i=0;i<10&&!h.commands().length;i++)await turn();
  assert.equal(h.commands().length,1);assert.strictEqual(h.session.info(),before);
  assert.equal(Object.hasOwn(before.layers[0],'visible'),false);
  gate.resolve();await Promise.all([hide,show]);
  assert.deepEqual(h.commands().map(c=>[c.r.expectedRevision,c.r.payload.layerUid,c.r.payload.visible]),[
    [0,'legacy layer:alpha',false],[1,'legacy layer:alpha',true]]);
  assert.equal(h.session.info().contentRevision,2);assert.equal(h.session.info().layers[0].visible,true);
  assert.deepEqual(h.changes.slice(-2).map(i=>i.layers[0].visible),[false,true]);
  assert.equal(h.session.readPresentedObjectSession().contentRevision,2);
  assert.equal(h.calls.at(-1).command,'nemo_native_object_preview');
});

test('invalid intent and unknown or ambiguous UID never dispatch or invalidate presented state',async()=>{
  for(const [uid,visible] of [['',false],['missing',false],[3,false],['legacy layer:alpha',0],['legacy layer:alpha',null]]){
    const h=scenario();await h.open();const before=h.session.info(),presented=h.session.readPresentedObjectSession();
    await assert.rejects(h.session.setLayerVisibility(uid,visible),/target or value/);
    assert.equal(h.commands().length,0);assert.strictEqual(h.session.info(),before);
    assert.deepEqual(h.session.readPresentedObjectSession(),presented);
  }
  const h=scenario([{layerUid:'duplicate'},{layerUid:'duplicate'}]);await h.open();
  await assert.rejects(h.session.setLayerVisibility('duplicate',false),/target or value/);
  assert.equal(h.commands().length,0);
});

test('native refusal restores actual presentation without changing visibility or retrying',async()=>{
  const h=scenario();await h.open();h.fail('refused');
  await assert.rejects(h.session.setLayerVisibility('legacy layer:alpha',false),/stale_revision/);
  assert.equal(h.commands().length,1);assert.equal(h.session.info().contentRevision,0);
  assert.equal(Object.hasOwn(h.session.info().layers[0],'visible'),false);
  assert.equal(h.session.readPresentedObjectSession().contentRevision,0);
});

test('lost response reconciles native commit while remaining a rejected action with no retry',async()=>{
  const h=scenario();await h.open();h.fail('transport-after');
  await assert.rejects(h.session.setLayerVisibility('legacy layer:alpha',false),/response lost/);
  assert.equal(h.commands().length,1);assert.equal(h.session.info().contentRevision,1);
  assert.equal(h.session.info().layers[0].visible,false);
  assert.equal(h.session.readPresentedObjectSession().contentRevision,1);
});

test('unconfirmed preview or foreign mutation receipt cannot become delivered visibility success',async()=>{
  for(const failure of ['preview','foreign']){
    const h=scenario();await h.open();h.fail(failure);
    await assert.rejects(h.session.setLayerVisibility('legacy layer:alpha',false),
      failure==='preview'?/not presented/:/uncorrelated/);
    assert.equal(h.commands().length,1);assert.equal(h.session.readPresentedObjectSession(),null);
    assert.equal(h.session.blocksLegacy(),true);
  }
});
