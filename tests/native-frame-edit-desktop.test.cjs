'use strict';
// Real desktop/intent/SDK/Project chain. Native I/O, projection publication and navigator host are modeled;
// these controls establish no installed or native-kernel acceptance.
const test=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const vm=require('node:vm');
const Desktop=require('../src/js/adapters/native-frame-edit-desktop.js');
const Intent=require('../src/js/adapters/native-frame-edit-intent.js');
const Project=require('../src/js/adapters/native-object-project.js');
const SDK=require('../src/js/adapters/native-application.js');
const P='command.document.object.';
const turn=()=>new Promise(setImmediate);
function deferred(){let resolve;const promise=new Promise(r=>{resolve=r;});return {promise,resolve};}
function harness(options={}){
  let active=false,revision=0,total=12,hold=null,preview='presented',ownerOverride=null;
  const calls=[],navigation=[],notices=[];
  const state={currentFrame:2,activeLayerIdx:1,totalFrames:12,layers:[{layerUid:'alpha'},{layerUid:'beta'}]};
  const root={state,_layerSel:[1],_sel:{frames:[{layer:0,frame:1},{layer:1,frame:2}]},
    NemoNativeFrameEditIntent:Intent,NemoNativeApplicationAdapter:SDK,showToast:m=>notices.push(m)};
  const info=()=>({apiVersion:2,instanceId:'instance',documentId:'document',lifecycleGeneration:1,
    documentSnapshotId:'snapshot-'+revision,contentRevision:revision,documentDigest:String(revision%10).repeat(64),
    byteCount:1000,canvasW:320,canvasH:180,fps:24,totalFrames:total,
    layers:[{layerUid:'alpha'},{layerUid:'beta'}],reference:null,undoDepth:revision,redoDepth:0});
  const publish=i=>{state.totalFrames=i.totalFrames;state.layers=i.layers.map(row=>({...row}));return true;};
  async function invoke(command,args){
    const q=args&&(args.request||JSON.parse(args.requestJson));calls.push({command,q});
    if(command==='nemo_native_object_client_status')return {...info(),available:active};
    if(command==='nemo_native_object_bootstrap'){active=true;return {...info(),requestId:q.requestId,resourceCount:0,viewportAvailable:false};}
    if(command==='nemo_native_object_reference')return {...info(),requestId:q.requestId};
    if(command==='nemo_native_object_preview')return {status:preview,workId:'work-'+revision,viewGeneration:1};
    assert.equal(command,'nemo_native_dispatch');
    assert.equal(q.expectedRevision,revision);assert.equal(q.documentId,'document');
    if(hold)await hold.promise;
    if(options.beforeAck)options.beforeAck(root);
    if(options.refusal)return {apiVersion:2,requestId:q.requestId,instanceId:'instance',documentId:'document',
      contentRevision:revision,ok:false,error:{code:'stale_revision',message:'Stale frame edit'}};
    const applied=options.noop!==true;
    if(applied){
      if(q.operation===P+'frames.insert'||q.operation===P+'frames.duplicate')total++;
      if(q.operation===P+'frames.extend')total+=q.payload.count;
      if(q.operation===P+'frames.remove')total-=q.payload.to-q.payload.from+1;
      revision++;
    }
    const reply={apiVersion:2,requestId:q.requestId,instanceId:'instance',documentId:'document',contentRevision:revision,
      ok:true,result:{applied,historyEntriesAdded:applied?1:0}};
    if(options.badAck)reply.requestId='foreign-receipt';
    return reply;
  }
  const session=Project.create({invoke,frame:()=>state.currentFrame,projectionReady:()=>options.ready!==false,
    publishRevision:publish,changed:publish,
    mapping:i=>({cssBounds:{x:0,y:0,width:320,height:180},physicalExtent:{width:320,height:180},
      compositionExtent:{width:i.canvasW,height:i.canvasH},reportedDpr:1}),
    subscribeRevisions:async()=>({instanceId:'instance',documentId:'document',lifecycleGeneration:1,
      contentRevision:revision,subscriptionId:'subscription'}),
    dispatchApplication:q=>SDK.createNativeApplicationAdapter('ui',{dispatch:value=>invoke('nemo_native_dispatch',{request:value})}).dispatch(q)});
  root.NemoNativeObjectProject={desktop:()=>ownerOverride||session};
  // Only the File UI's publication qualification is modeled, not a document owner.
  root.NemoNativeObjectProjectFile={projectionReadyForUI:(r,i)=>options.ready!==false&&
    r.state.totalFrames===i.totalFrames&&r.state.layers.every((row,index)=>row.layerUid===i.layers[index].layerUid)};
  root.goToFrame=async frame=>{navigation.push(frame);await session.presentPreview(frame);state.currentFrame=frame;};
  const controller=Desktop.install(root);
  return {root,session,controller,calls,navigation,notices,info,
    commands:()=>calls.filter(row=>row.command==='nemo_native_dispatch'),
    open:async()=>{await session.openPath('/frames.json');publish(session.info());await session.presentPreview(state.currentFrame);},
    hold:()=>{hold=deferred();return hold;},replace:s=>{ownerOverride=s;},preview:s=>{preview=s;}};
}

test('install is idempotent and five shortcuts synchronously consume one real native intent',async()=>{
  for(const action of ['cmdInsertFrame','cmdInsertKey','cmdInsertBlankKey','cmdDuplicateKey','cmdExtendExposure']){
    const h=harness();await h.open();let prevented=0;
    assert.equal(Desktop.install(h.root),h.controller);
    assert.equal(h.controller.shortcut(action,{preventDefault(){prevented++;}}),true);
    assert.equal(prevented,1);await turn();await turn();
    assert.equal(h.commands().length,action==='cmdInsertBlankKey'?0:1,action);
    if(action==='cmdInsertBlankKey')assert.match(h.notices.at(-1),/unavailable/i);
    assert.equal(h.controller.shortcut('not-a-frame-action',{}),false);
  }
});

test('real ACK/inspect/presentation precede a native navigator hint without optimistic UI writes',async()=>{
  const h=harness();await h.open();const gate=h.hold();
  const pending=h.controller.run({kind:'insertFrame'});await turn();
  assert.equal(h.root.state.currentFrame,2);assert.deepEqual(h.navigation,[]);
  assert.equal(h.controller.isPending(),true);
  assert.deepEqual({...await h.controller.run({kind:'insertKey'})},{status:'busy'});
  gate.resolve();const result=await pending;
  assert.equal(result.status,'applied-presented');assert.equal(h.commands().length,1);
  assert.deepEqual(h.commands()[0].q.payload,{command:'object.frames.insert',cell:2,layerUids:['beta']});
  assert.deepEqual(h.navigation,[3]);assert.equal(h.root.state.currentFrame,3);
  assert.equal(h.session.info().contentRevision,1);
});

test('every mapped writer uses the actual signature and reaches exactly its expected operation',async()=>{
  const cases=[
    ['global','insertFrame',[],P+'frames.insert',{cell:2,layerUids:['beta']}],
    ['global','insertKeyframe',[],P+'layer.key.promote',{cells:[2],layerUids:['beta']}],
    ['global','insertKeyframeAt',[0,4],P+'layer.key.promote',{layerUids:['alpha'],cells:[4]}],
    ['timeline','clearKeyframe',[],P+'layer.key.clear',{layerUid:'beta',cell:2}],
    ['timeline','convertToKeyframes',[],P+'layer.key.promote',{layerUids:['beta'],cells:[2]}],
    ['global','removeFrame',[],P+'frames.remove',{from:2,to:2}],
    ['timeline','removeFrameSpan',[],P+'frames.remove',{from:1,to:2}],
    ['global','removeTweenSpan',[0,1,5],P+'layer.tween.remove',{layerUid:'alpha',from:1,to:5}],
    ['timeline','moveKeyframe',[1,1,4],P+'layer.key.move',{layerUid:'beta',from:1,to:4}],
    ['timeline','deleteSelectedFrames',[],P+'layer.key.clear_cells',{targets:[{layerUid:'alpha',frame:1},{layerUid:'beta',frame:2}]}],
    ['timeline','duplicateKeyframe',[],P+'frames.duplicate',{layerUid:'beta',cell:2}],
    ['timeline','extendExposure',[3],P+'frames.extend',{cell:2,count:3}],
    ['timeline','duplicateSelectedFrames',[],P+'frames.clipboard.duplicate',{layerUid:'beta',selection:[{layerUid:'alpha',frame:1},{layerUid:'beta',frame:2}]}],
    ['timeline','repeatSelection',[2],P+'frames.repeat',{fromLayerUid:'alpha',toLayerUid:'beta',fromFrame:1,toFrame:2,times:2}]
  ];
  for(const [target,name,args,operation,payload]of cases){
    const h=harness();await h.open();const route=h.controller.routeWriter(target,name,args);
    assert.equal(route.handled,true,name);const result=await route.result;
    assert.equal(result.status,'applied-presented',name+': '+JSON.stringify(result));
    assert.equal(h.commands().length,1,name);assert.equal(h.commands()[0].q.operation,operation);
    const actual={...h.commands()[0].q.payload};delete actual.command;assert.deepEqual(actual,payload,name);
  }
});

test('F7, clipboard packets and unsupported multi-cell moves consume without any legacy or native write',async()=>{
  const h=harness();await h.open();
  for(const name of ['insertBlankKeyframe','copyFrames','cutFrames','pasteFrames','moveFrames','shiftLayerFrames']){
    const route=h.controller.routeWriter('timeline',name,[[],1,1]);
    assert.equal(route.handled,true);assert.equal((await route.result).status,'unavailable');
  }
  assert.equal(h.commands().length,0);
  assert.equal(h.notices.length,6);assert.ok(h.notices.every(message=>/unavailable/i.test(message)));
  assert.equal(h.controller.routeWriter('timeline','notAWriter',[]),null);
  assert.equal(h.controller.routeWriter('global','moveKeyframe',[0,1,2]),null);
  assert.ok(Object.isFrozen(Desktop.writerNames('timeline')));
});

test('missing selections, projection or native presentation fail closed without invented UI data',async()=>{
  for(const fault of ['layerSelection','cellSelection','UID','presentation','projection']){
    const h=harness();await h.open();
    if(fault==='layerSelection')delete h.root._layerSel;
    if(fault==='cellSelection')delete h.root._sel;
    if(fault==='UID')h.root.state.layers[1].layerUid='foreign';
    if(fault==='presentation')h.preview('unavailable');
    if(fault==='projection')h.root.NemoNativeObjectProjectFile.projectionReadyForUI=()=>false;
    if(fault==='presentation'){
      await assert.rejects(h.session.presentPreview(2));
    }
    assert.equal((await h.controller.run({kind:'insertFrame'})).status,'unavailable',fault);
    assert.equal(h.commands().length,0,fault);assert.deepEqual(h.navigation,[]);
  }
});

test('stale owner and changed captured UI suppress hints after the real native command',async()=>{
  for(const change of [r=>{r.state.activeLayerIdx=0;},r=>{r._layerSel=[0];},
    r=>{r._sel.frames=[{layer:1,frame:3}];},r=>{r.state.currentFrame=4;}]){
    const h=harness({beforeAck:change});await h.open();
    const result=await h.controller.run({kind:'insertFrame'});
    assert.equal(result.applied,true);assert.deepEqual(h.navigation,[]);
  }
  const h=harness();await h.open();const gate=h.hold();const pending=h.controller.run({kind:'insertFrame'});await turn();
  h.replace({isActive:()=>true,info:h.info});gate.resolve();
  assert.equal((await pending).status,'applied-unconfirmed');assert.deepEqual(h.navigation,[]);
});

test('refused, unconfirmed and no-op receipts never move the playhead',async()=>{
  for(const options of [{refusal:true},{badAck:true},{noop:true}]){
    const h=harness(options);await h.open();const result=await h.controller.run({kind:'insertFrame'});
    assert.notEqual(result.status,'applied-presented');assert.deepEqual(h.navigation,[]);
  }
});

test('browser idle returns unhandled while native uncertainty still consumes mapped writers',async()=>{
  const h=harness();assert.equal(h.controller.routeWriter('global','insertFrame',[]),null);
  assert.equal(h.controller.shortcut('cmdInsertFrame',{}),false);
  h.root.NemoNativeOpacityCutover={blocksLegacy:()=>true};
  assert.equal(h.controller.routeWriter('global','insertFrame',[]).handled,true);
  assert.equal((await h.controller.routeWriter('timeline','copyFrames',[]).result).status,'unavailable');
});

test('classic UMD installs one controller and actual Main shortcut handler routes before legacy callbacks',async()=>{
  const h=harness();await h.open();
  const context={...h.root,window:null};context.window=context;
  const text=fs.readFileSync(require.resolve('../src/js/adapters/native-frame-edit-desktop.js'),'utf8');
  vm.runInNewContext(text,context);
  const first=context.NemoNativeFrameEditController;
  assert.equal(context.NemoNativeFrameEditDesktop.install(context),first);
  const main=fs.readFileSync(require.resolve('../src/js/timeline.js'),'utf8');
  const start=main.indexOf('function runCommandShortcutAction(action,e){');assert.ok(start>=0);
  const line=main.slice(start,main.indexOf('\n',start));
  assert.match(line,/NemoNativeFrameEdit/,'the ordinary Main handler must include the actual desktop hook');
  let legacy=0,prevented=0;
  Object.assign(context,{insertFrame(){legacy++;},insertKeyframe(){legacy++;},insertBlankKeyframe(){legacy++;},
    SM:{duplicateKeyframe(){legacy++;},extendExposure(){legacy++;}}});
  vm.runInNewContext(line,context);
  context.runCommandShortcutAction('cmdInsertFrame',{preventDefault(){prevented++;}});
  await turn();await turn();assert.equal(legacy,0);assert.equal(prevented,1);assert.equal(h.commands().length,1);
});

test('actual legacy desktop surface wraps the full writer list and routes before captured guard/writer',async()=>{
  const Surface=require('../src/js/adapters/native-opacity-legacy-surface.js');
  const h=harness();await h.open();let legacy=0,guards=0,admissions=0;
  h.root.__TAURI__={core:{invoke(){throw Error('Surface wrapper must not invoke a private transport');}}};
  h.root.NemoNativeFrameEditDesktop=Desktop;h.root.SM={importJSON(){}};
  const originals={global:{},timeline:{}};
  for(const target of ['global','timeline']){
    const object=target==='global'?h.root:h.root.SM;
    for(const name of Desktop.writerNames(target)){
      object[name]=function(){legacy++;throw Error('Captured legacy writer ran: '+name);};
      originals[target][name]=object[name];
    }
  }
  const surface=Surface.desktopPorts(h.root,{}).surface;
  const allow=()=>{guards++;return false;},admitted=()=>admissions++;
  const removeGlobal=surface.wrap('global',[],'app-',allow,admitted);
  const removeTimeline=surface.wrap('timeline',[],'timeline-',allow,admitted);
  for(const target of ['global','timeline']){
    const object=target==='global'?h.root:h.root.SM;
    for(const name of Desktop.writerNames(target))assert.notEqual(object[name],originals[target][name],name);
  }
  assert.equal((await h.root.insertKeyframeAt(0,4)).status,'applied-presented');
  assert.equal((await h.root.SM.extendExposure(3)).status,'applied-presented');
  assert.deepEqual(h.commands().map(c=>c.q.payload),[
    {command:'object.layer.key.promote',layerUids:['alpha'],cells:[4]},
    {command:'object.frames.extend',cell:2,count:3}]);
  for(const name of ['copyFrames','cutFrames','pasteFrames','moveFrames','shiftLayerFrames']){
    assert.equal((await h.root.SM[name]([],1,1)).status,'unavailable',name);
  }
  assert.deepEqual([legacy,guards,admissions],[0,0,0]);
  removeTimeline();removeGlobal();
  for(const target of ['global','timeline']){
    const object=target==='global'?h.root:h.root.SM;
    for(const name of Desktop.writerNames(target))assert.equal(object[name],originals[target][name]);
  }
});

test('actual legacy surface never falls through when helper is missing, broken or native session is inactive',async()=>{
  const Surface=require('../src/js/adapters/native-opacity-legacy-surface.js');
  for(const mode of ['missing','broken','inactive','missingHostActive']){
    const h=harness();if(mode!=='inactive')await h.open();let legacy=0,guards=0;
    if(mode!=='missingHostActive')h.root.__TAURI__={core:{invoke(){throw Error('Unexpected surface I/O');}}};
    h.root.SM={importJSON(){},copyFrames(){legacy++;}};
    h.root.insertFrame=function(){legacy++;};
    if(mode==='broken')h.root.NemoNativeFrameEditDesktop={install(){throw Error('Missing intent dependency');}};
    if(mode==='inactive')h.root.NemoNativeFrameEditDesktop=Desktop;
    const surface=Surface.desktopPorts(h.root,{}).surface;
    const dispose=surface.wrap('global',[],'app-',()=>{guards++;return true;});
    const result=await h.root.insertFrame();
    assert.ok(['unavailable','unconfirmed'].includes(result.status));
    assert.deepEqual([legacy,guards,h.commands().length],[0,0,0]);dispose();
  }
});


test('typed failures remain visible without changing the returned native status',async()=>{
  for(const [options,status,message]of [[{refusal:true},'refused',/refused/i],
    [{badAck:true},'unconfirmed',/not confirmed/i]]){
    const h=harness(options);await h.open();
    const result=await h.controller.run({kind:'insertFrame'});
    assert.equal(result.status,status);assert.match(h.notices.at(-1),message);assert.deepEqual(h.navigation,[]);
  }
  const h=harness();await h.open();const gate=h.hold();
  const running=h.controller.run({kind:'insertKey'});await turn();
  assert.equal((await h.controller.run({kind:'insertKey'})).status,'busy');
  assert.match(h.notices.at(-1),/already pending/i);gate.resolve();await running;
});
