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
    layers:[Object.assign({layerUid:'alpha'},options.lockedAlpha?{locked:true}:{}),{layerUid:'beta'}],reference:null,undoDepth:revision,redoDepth:0});
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
    const applied=options.noop!==true&&q.operation!==P+'frames.clipboard.copy';
    if(applied){
      if(q.operation===P+'frames.insert'||q.operation===P+'frames.duplicate')total++;
      if(q.operation===P+'frames.extend')total+=q.payload.count;
      if(q.operation===P+'frames.remove')total-=q.payload.to-q.payload.from+1;
      revision++;
    }
    const reply={apiVersion:2,requestId:q.requestId,instanceId:'instance',documentId:'document',contentRevision:revision,
      ok:true,result:{applied,historyEntriesAdded:applied?1:0}};
    if(q.operation===P+'frames.repeat')Object.assign(reply.result,options.repeatCounts||{cycledLayers:2,skippedComponentLayers:0});
    if(/frames\.clipboard\.(copy|cut)$/.test(q.operation))reply.result.clipboard=options.packet===null?null:
      options.packet?structuredClone(options.packet):{instanceId:'instance',documentId:'document',cells:[{relLayer:0,relFrame:0,keyframe:true,records:[],orderElements:[]}]};
    if(q.operation===P+'frames.move')reply.result.selection=applied?structuredClone(options.moveSelection===undefined?[{layerUid:'alpha',frame:3},{layerUid:'beta',frame:4}]:options.moveSelection):null;
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
    assert.equal(h.commands().length,1,action);
    if(action==='cmdInsertBlankKey'){
      assert.deepEqual(h.commands()[0].q.payload,{command:'object.layer.key.blank',layerUid:'beta',cell:2});
      assert.deepEqual(h.navigation,[]);assert.equal(h.session.info().totalFrames,12);
    }
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
    ['global','insertBlankKeyframe',[],P+'layer.key.blank',{layerUid:'beta',cell:2}],
    ['timeline','insertBlankKeyframe',[],P+'layer.key.blank',{layerUid:'beta',cell:2}],
    ['global','insertKeyframeAt',[0,4],P+'layer.key.promote',{layerUids:['alpha'],cells:[4]}],
    ['timeline','clearKeyframe',[],P+'layer.key.clear',{layerUid:'beta',cell:2}],
    ['timeline','convertToKeyframes',[],P+'layer.key.promote',{layerUids:['beta'],cells:[2]}],
    ['global','removeFrame',[],P+'frames.remove',{from:2,to:2}],
    ['timeline','removeFrameSpan',[],P+'frames.remove',{from:1,to:2}],
    ['global','removeTweenSpan',[0,1,5],P+'layer.tween.remove',{layerUid:'alpha',from:1,to:5}],
    ['timeline','moveKeyframe',[1,1,4],P+'layer.key.move',{layerUid:'beta',activeLayerUid:'beta',from:1,to:4}],
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

test('moving a key on another layer preserves the active-layer prefix in the exact payload',async()=>{
  const h=harness();await h.open();
  assert.equal(h.root.state.activeLayerIdx,1);
  const route=h.controller.routeWriter('timeline','moveKeyframe',[0,1,4]);
  assert.equal(route.handled,true);
  assert.equal((await route.result).status,'applied-presented');
  assert.equal(h.commands().length,1);
  assert.equal(h.commands()[0].q.operation,P+'layer.key.move');
  assert.deepEqual({...h.commands()[0].q.payload},{command:'object.layer.key.move',layerUid:'alpha',activeLayerUid:'beta',from:1,to:4});
  assert.equal(h.root.state.activeLayerIdx,1);
});

test('ordinary Cycle button prompt crosses the real legacy wrapper with baseline count parsing and cancellation',async()=>{
  const Surface=require('../src/js/adapters/native-opacity-legacy-surface.js');
  const source=fs.readFileSync(require.resolve('../src/js/timeline.js'),'utf8');
  const start=source.indexOf('function initCycleAndPropagate(){');
  const body=source.slice(start,source.indexOf("if(document.readyState==='loading')document.addEventListener('DOMContentLoaded',initCycleAndPropagate)",start));
  for(const [input,want] of [['2',2],['2.9 rounds',2],['0',1],['-8',1],['999',50],['invalid',1],['',1],[null,null]]){
    const h=harness();await h.open();let click,pending,legacy=0;
    h.root.__TAURI__={core:{invoke(){throw Error('Unexpected private writer');}}};
    h.root.NemoNativeFrameEditDesktop=Desktop;h.root.SM={importJSON(){},repeatSelection(){legacy++;}};
    const dispose=Surface.desktopPorts(h.root,{}).surface.wrap('timeline',[],'timeline-',()=>false,()=>{});
    const wrapped=h.root.SM.repeatSelection;h.root.SM.repeatSelection=value=>(pending=wrapped(value));
    const context={window:h.root,selBounds:()=>({minF:1,maxF:2}),prompt:()=>input,
      document:{getElementById:id=>id==='btn-cycle'?{addEventListener:(event,fn)=>{assert.equal(event,'click');click=fn;}}:null}};
    vm.runInNewContext(body+'\ninitCycleAndPropagate();',context);click();
    if(want===null)assert.equal(pending,undefined);
    else{
      assert.equal((await pending).status,'applied-presented',String(input));
      assert.equal(h.commands()[0].q.payload.times,want,String(input));
    }
    assert.equal(h.commands().length,want===null?0:1);assert.equal(legacy,0);dispose();
  }
});

test('repeat feedback distinguishes drawing cycles, component skips, semantic no-op and failed confirmation',async()=>{
  for(const [options,expected] of [
    [{repeatCounts:{cycledLayers:1,skippedComponentLayers:2}},'Repeated 1 drawing layer. Skipped 2 component layers.'],
    [{repeatCounts:{cycledLayers:0,skippedComponentLayers:2}},'No drawing layers to repeat. Skipped 2 component layers.'],
    [{noop:true,repeatCounts:{cycledLayers:1,skippedComponentLayers:0}},'Repeat left the drawing layers unchanged.'],
    [{noop:true,repeatCounts:{cycledLayers:0,skippedComponentLayers:1}},'No drawing layers to repeat. Skipped 1 component layer.']
  ]){
    const h=harness(options);await h.open();await h.controller.routeWriter('timeline','repeatSelection',['2']).result;
    assert.deepEqual(h.notices,[expected]);
  }
  for(const options of [{refusal:true},{badAck:true}]){
    const h=harness(options);await h.open();await h.controller.routeWriter('timeline','repeatSelection',['2']).result;
    assert.equal(h.notices.length,1);assert.doesNotMatch(h.notices[0],/Repeated|Skipped|No drawing/);
  }
});

test('public Shift stays unavailable without any legacy or native write',async()=>{
  const h=harness();await h.open();
  for(const name of ['shiftLayerFrames']){
    const route=h.controller.routeWriter('timeline',name,[[],1,1]);
    assert.equal(route.handled,true);assert.equal((await route.result).status,'unavailable');
  }
  assert.equal(h.commands().length,0);
  assert.equal(h.notices.length,1);assert.ok(h.notices.every(message=>/unavailable/i.test(message)));
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
  assert.equal(vm.runInNewContext(
    'NemoNativeFrameEditDesktop.install(globalThis) === NemoNativeFrameEditController',context),true);
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
  for(const name of ['shiftLayerFrames']){
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
    h.root.__TAURI__={core:{invoke(){throw Error('Unexpected surface I/O');}}};
    h.root.SM={importJSON(){},copyFrames(){legacy++;}};
    h.root.insertFrame=function(){legacy++;};
    if(mode==='broken')h.root.NemoNativeFrameEditDesktop={install(){throw Error('Missing intent dependency');}};
    if(mode==='inactive')h.root.NemoNativeFrameEditDesktop=Desktop;
    const surface=Surface.desktopPorts(h.root,{}).surface;
    const dispose=surface.wrap('global',[],'app-',()=>{guards++;return true;});
    if(mode==='missingHostActive')delete h.root.__TAURI__;
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


test('shipped loader installs the desktop consumer once after its actual dependencies',()=>{
  const html=fs.readFileSync(require('node:path').join(__dirname,'../src/index.html'),'utf8');
  const tag=name=>'<script src="js/adapters/'+name+'.js"></script>';
  const own=tag('native-frame-edit-desktop'),at=html.indexOf(own);
  assert.ok(at>=0);
  assert.equal(html.indexOf(own,at+1),-1);
  for(const name of ['native-frame-edit-intent','native-application','native-object-project','native-object-project-file']){
    const dependency=html.indexOf(tag(name));
    assert.ok(dependency>=0&&dependency<at,name+' must exist before immediate install');
  }
});


test('ordinary Main F7 and wrapped global/SM F7 dispatch active UID once, ignoring another selected layer',async()=>{
  const main=fs.readFileSync(require.resolve('../src/js/timeline.js'),'utf8');
  const start=main.indexOf('function runCommandShortcutAction(action,e){');assert.ok(start>=0);
  const line=main.slice(start,main.indexOf('\n',start));
  const Surface=require('../src/js/adapters/native-opacity-legacy-surface.js');
  for(const route of ['main','global','timeline']){
    const h=harness();await h.open();h.root._layerSel=[0];h.root._sel.frames=[{layer:0,frame:8}];
    let legacy=0,guards=0,prevented=0;
    if(route==='main'){
      const context={...h.root,window:null,insertBlankKeyframe(){legacy++;}};context.window=context;
      vm.runInNewContext(fs.readFileSync(require.resolve('../src/js/adapters/native-frame-edit-desktop.js'),'utf8'),context);
      vm.runInNewContext(line,context);
      context.runCommandShortcutAction('cmdInsertBlankKey',{preventDefault(){prevented++;}});
      await turn();await turn();assert.equal(prevented,1);
    }else{
      h.root.__TAURI__={core:{invoke(){throw Error('No private surface transport');}}};
      h.root.NemoNativeFrameEditDesktop=Desktop;
      h.root.insertBlankKeyframe=()=>{legacy++;};h.root.SM={importJSON(){},insertBlankKeyframe(){legacy++;}};
      const surface=Surface.desktopPorts(h.root,{}).surface;
      const stop=surface.wrap(route,[],route+'-',()=>{guards++;return true;});
      try{
        const object=route==='global'?h.root:h.root.SM;
        assert.deepEqual({...await object.insertBlankKeyframe()},{status:'applied-presented',applied:true});
      }finally{stop();}
    }
    assert.deepEqual([legacy,guards,h.commands().length],[0,0,1],route);
    assert.equal(h.commands()[0].q.operation,P+'layer.key.blank');assert.equal(h.commands()[0].q.expectedRevision,0);
    assert.deepEqual(h.commands()[0].q.payload,{command:'object.layer.key.blank',layerUid:'beta',cell:2});
    assert.equal(h.session.info().contentRevision,1);assert.equal(h.session.info().undoDepth,1);
    assert.equal(h.session.readPresentedObjectSession().presentation.frame,2);
    assert.equal(h.root.state.currentFrame,2);assert.equal(h.root.state.totalFrames,12);assert.deepEqual(h.navigation,[]);
    assert.deepEqual(h.root._layerSel,[0]);assert.deepEqual(h.root._sel.frames,[{layer:0,frame:8}]);
  }
});

test('F7 pending, native refusal and malformed ACK consume without fallback or playhead mutation',async()=>{
  for(const [options,status]of [[{refusal:true},'refused'],[{badAck:true},'unconfirmed'],[{noop:true},'unconfirmed']]){
    const h=harness(options);await h.open();
    const route=h.controller.routeWriter('global','insertBlankKeyframe',[]);
    assert.equal(route.handled,true);assert.equal((await route.result).status,status);
    assert.equal(h.commands().length,1);assert.deepEqual(h.navigation,[]);assert.equal(h.root.state.currentFrame,2);
  }
  const h=harness();await h.open();const gate=h.hold();
  const first=h.controller.routeWriter('timeline','insertBlankKeyframe',[]);await turn();
  assert.equal((await h.controller.routeWriter('global','insertBlankKeyframe',[]).result).status,'busy');
  assert.equal(h.commands().length,1);assert.equal(h.root.state.currentFrame,2);
  gate.resolve();assert.equal((await first.result).status,'applied-presented');
  assert.equal(h.session.info().contentRevision,1);assert.deepEqual(h.navigation,[]);
});


test('real Desktop/Intent/SDK/Project chain copies detached native data, cuts once, pastes at the active anchor and paints selection',async()=>{
  const h=harness();await h.open();let paints=0;h.root.selApplyCSS=()=>paints++;
  const original=[{legacy:true}];h.root._sel.clipboard=original;
  const copy=await h.controller.routeWriter('timeline','copyFrames',[]).result;
  assert.deepEqual({...copy},{status:'unchanged',applied:false});assert.equal(h.info().contentRevision,0);
  assert.equal(h.commands()[0].q.operation,P+'frames.clipboard.copy');
  assert.deepEqual(h.commands()[0].q.payload.selection,[{layerUid:'alpha',frame:1},{layerUid:'beta',frame:2}]);
  const packet=h.root._sel.clipboard;assert.notEqual(packet,original);assert.equal(packet.length,1);
  assert.deepEqual(Object.keys(packet),['instanceId','documentId','cells']);assert.equal(h.root._lastClipKind,'frames');
  h.root.state.activeLayerIdx=0;
  const pasted=await h.controller.routeWriter('timeline','pasteFrames',[]).result;
  assert.equal(pasted.status,'applied-presented');assert.equal(h.info().contentRevision,1);
  const paste=h.commands()[1].q.payload;assert.equal(paste.layerUid,'alpha');assert.equal(paste.frame,2);
  assert.deepEqual(paste.cells,packet.cells);assert.deepEqual(h.root._sel.frames,[{layer:0,frame:2}]);assert.equal(paints,1);
  const cut=await h.controller.routeWriter('timeline','cutFrames',[]).result;
  assert.equal(cut.status,'applied-presented');assert.equal(h.info().contentRevision,2);
  assert.deepEqual(h.root._sel.frames,[]);assert.equal(paints,2);assert.equal(h.root._sel.clipOp,'cut');assert.equal(h.commands().length,3);
});
test('null or refused clipboard ACK never replaces a prior clipboard, and missing selection never sends',async()=>{
  for(const options of [{packet:null,noop:true},{refusal:true}]){
    const h=harness(options);await h.open();const prior=[{old:true}];h.root._sel.clipboard=prior;
    await h.controller.run({kind:'copyFrames'});assert.equal(h.root._sel.clipboard,prior);assert.equal(h.info().contentRevision,0);
    h.root._sel.frames=[];const before=h.commands().length;
    assert.equal((await h.controller.run({kind:'cutFrames'})).code,'frame_edit_selection_required');assert.equal(h.commands().length,before);
  }
});
test('newer clipboard choice, changed selection or document replacement while ACK is pending prevents stale UI publication',async()=>{
  for(const fault of ['frame-clipboard','canvas-clipboard','kind','selection','replacement']){
    const h=harness();await h.open();const prior=[{old:true}];h.root._sel.clipboard=prior;h.root._canvasClip={snaps:[]};
    const gate=h.hold(),pending=h.controller.run({kind:'copyFrames'});await turn();
    let chosen=prior;
    if(fault==='frame-clipboard'){chosen=[{new:true}];h.root._sel.clipboard=chosen;}
    if(fault==='canvas-clipboard')h.root._canvasClip={snaps:[{new:true}]};
    if(fault==='kind')h.root._lastClipKind='canvas';
    if(fault==='selection')h.root._sel.frames=[{layer:1,frame:4}];
    if(fault==='replacement')h.replace({});
    gate.resolve();const result=await pending;assert.notEqual(result.status,'unchanged',fault);
    assert.equal(h.root._sel.clipboard,chosen,fault);assert.equal(h.commands().length,1);assert.equal(h.info().contentRevision,0);
  }
});
test('opaque data/blob record/order UIDs survive the actual SDK in both directions while raw payloads still refuse',async()=>{
  const record=JSON.parse(fs.readFileSync('engineering/application/examples/native-object-v1/cases.json','utf8')).records[0];
  record.target.layerUid='data:layer';record.target.strokeId='blob:stroke';
  const packet={instanceId:'instance',documentId:'document',cells:[{relLayer:0,relFrame:0,keyframe:true,records:[record],
    orderElements:[{layerUid:'data:layer',strokeId:'blob:stroke',naturalPosition:0,order:{}}]}]};
  const h=harness({packet});await h.open();assert.equal((await h.controller.run({kind:'copyFrames'})).status,'unchanged');
  assert.equal((await h.controller.run({kind:'pasteFrames'})).status,'applied-presented');
  assert.equal(h.commands()[1].q.payload.cells[0].records[0].target.strokeId,'blob:stroke');
  const SDK=require('../src/js/adapters/native-application.js');
  const q=h.commands()[1].q;q.payload.cells[0].records[0].geometry.raw='data:image/png;base64,AA';
  assert.throws(()=>SDK.validateRequest(q),TypeError);
});
test('forged clipboard source identity cannot publish or be pasted across a document replacement',async()=>{
  const packet={instanceId:'foreign',documentId:'document',cells:[]};const h=harness({packet});await h.open();
  const prior=[{old:true}];h.root._sel.clipboard=prior;
  assert.notEqual((await h.controller.run({kind:'copyFrames'})).status,'unchanged');assert.equal(h.root._sel.clipboard,prior);
  h.root._sel.clipboard=packet;const sent=h.commands().length;
  assert.equal((await h.controller.run({kind:'pasteFrames'})).code,'frame_edit_stale_intent');assert.equal(h.commands().length,sent);
});

test('changed Cut with a null packet is unconfirmed and cannot replace the clipboard or clear selection',async()=>{
  const h=harness({packet:null});await h.open();const prior=[{old:true}],selected=structuredClone(h.root._sel.frames);
  h.root._sel.clipboard=prior;const result=await h.controller.run({kind:'cutFrames'});
  assert.notEqual(result.status,'applied-presented');assert.equal(h.root._sel.clipboard,prior);
  assert.deepEqual(h.root._sel.frames,selected);assert.equal(h.commands().length,1);
});


test('actual grid Move call and SM wrapper apply only native destinations after ACK/inspect/present',async()=>{
  const Surface=require('../src/js/adapters/native-opacity-legacy-surface.js');
  for(const [frame,actual,lockedAlpha]of [[3,[{layerUid:'beta',frame:4}],true],[11,[{layerUid:'alpha',frame:11}],false],[12,[],false]]){
    const h=harness({moveSelection:actual,lockedAlpha});await h.open();let legacy=0,css=0;
    h.root.__TAURI__={core:{invoke(){throw Error('no private writer transport');}}};
    h.root.NemoNativeFrameEditDesktop=Desktop;h.root.SM={importJSON(){},moveFrames(){legacy++;}};
    h.root.selApplyCSS=()=>{css++;assert.equal(h.session.readPresentedObjectSession().contentRevision,1);};
    const stop=Surface.desktopPorts(h.root,{}).surface.wrap('timeline',[],'timeline-',()=>{legacy++;return true;});
    const main=fs.readFileSync(require.resolve('../src/js/timeline.js'),'utf8');
    const line=main.split('\n').find(line=>line.includes('window.SM.moveFrames(_sel.frames.slice(),b.minL+offL,b.minF+offF)'));
    assert.ok(line,'ordinary frame-grid calls the wrapped exact SM signature');
    try{
      const returned=vm.runInNewContext(line.trim(),{window:h.root,_sel:h.root._sel,b:{minL:0,minF:1},offL:0,offF:frame-1});
      assert.equal((await returned).status,'applied-presented');
    }finally{stop();}
    assert.equal(legacy,0);assert.equal(h.commands().length,1);assert.equal(css,1);
    assert.deepEqual(h.commands()[0].q.payload,{command:'object.frames.move',selection:[{layerUid:'alpha',frame:1},{layerUid:'beta',frame:2}],layerUid:'alpha',frame});
    assert.deepEqual(h.root._sel.frames,actual.map(c=>({layer:c.layerUid==='alpha'?0:1,frame:c.frame})));
    assert.equal(h.root.state.currentFrame,2);assert.equal(h.root.state.totalFrames,12);assert.deepEqual(h.navigation,[]);
  }
});

test('Move null no-op preserves selection; delayed ACK cannot replace newer UI or owner',async()=>{
  const h=harness({noop:true});await h.open();const selected=h.root._sel.frames;
  assert.equal((await h.controller.routeWriter('timeline','moveFrames',[selected,0,1]).result).status,'unchanged');
  assert.equal(h.root._sel.frames,selected);assert.equal(h.session.info().contentRevision,0);
  for(const change of [r=>{r._sel.frames=[{layer:1,frame:8}];},r=>{r._layerSel=[0];},r=>{r.state.activeLayerIdx=0;},r=>{r.state.currentFrame=4;}]){
    const h=harness();await h.open();const explicit=structuredClone(h.root._sel.frames),gate=h.hold();
    const first=h.controller.routeWriter('timeline','moveFrames',[explicit,0,3]);await turn();change(h.root);
    const newer=structuredClone(h.root._sel.frames);assert.equal((await h.controller.run({kind:'insertKey'})).status,'busy');
    gate.resolve();assert.equal((await first.result).status,'applied-unconfirmed');assert.deepEqual(h.root._sel.frames,newer);
    assert.equal(h.commands().length,1);assert.deepEqual(h.navigation,[]);
  }
  const r=harness();await r.open();const gate=r.hold();const pending=r.controller.routeWriter('timeline','moveFrames',[structuredClone(r.root._sel.frames),0,3]);
  await turn();r.replace({isActive:()=>true,info:r.info});gate.resolve();
  assert.equal((await pending.result).status,'applied-unconfirmed');assert.deepEqual(r.root._sel.frames,[{layer:0,frame:1},{layer:1,frame:2}]);
});

test('Move malformed receipt or foreign destination never mutates selection and mismatched input never dispatches',async()=>{
  for(const moveSelection of [null,[{layerUid:'foreign',frame:3}],[{layerUid:'alpha',frame:12}],[{layerUid:'alpha',frame:3,extra:1}]]){
    const h=harness({moveSelection});await h.open();const selected=structuredClone(h.root._sel.frames);
    assert.notEqual((await h.controller.routeWriter('timeline','moveFrames',[selected,0,3]).result).status,'applied-presented');
    assert.deepEqual(h.root._sel.frames,selected);assert.equal(h.commands().length,1);
  }
  const h=harness();await h.open();
  assert.equal((await h.controller.routeWriter('timeline','moveFrames',[[{layer:1,frame:2}],0,3]).result).code,'frame_edit_stale_intent');
  assert.equal(h.commands().length,0);
});

test('Desktop range action preserves its own selected-key token through real Intent and Project queue',async()=>{
 const h=harness();await h.open();const token={};let current=true;
 Object.defineProperty(token,'isCurrent',{value:function(info,after,queued){if(queued)assert.equal(h.session.readPresentedObjectSession(),null);return current;}});
 const gesture={command:'object.layer.range.drag',contextId:'scene-root',layerUid:'alpha',activeLayerUid:'beta',part:'in',barSelection:[],samples:[0,2,1],plainClick:false,alt:false,frame:2,selectedKeys:[]};
 const out=await h.controller.run({kind:'rangeDrag',gesture,capture:token});assert.equal(out.status,'applied-presented');assert.equal(h.commands().length,1);assert.deepEqual(h.commands()[0].q.payload,gesture);assert.deepEqual(h.navigation,[]);
 current=false;assert.equal((await h.controller.run({kind:'rangeDrag',gesture,capture:token})).status,'unavailable');assert.equal(h.commands().length,1);
});

test('Project queued guard grants only its own current presentation lease and retires it before dispatch',async()=>{
  const op=P+'frames.extend',payload={command:'object.frames.extend',cell:2,count:1};
  let escaped,dispatched=false;const h=harness({beforeAck(){dispatched=true;assert.equal(escaped.readPresentedObjectSession(),null);}});
  await h.open();const origin=h.session.readPresentedObjectSession();
  const out=await h.session.editFrames(op,payload,h.session.info(),lease=>{
    escaped=lease;assert.equal(Object.isFrozen(lease),true);assert.equal(h.session.readPresentedObjectSession(),null);
    const shown=lease.readPresentedObjectSession();assert.ok(shown);assert.strictEqual(shown.presentation,origin.presentation);
    assert.equal(shown.documentSnapshotId,origin.documentSnapshotId);assert.equal(shown.presentation.frame,2);return true;
  });
  assert.equal(out.ok,true);assert.equal(out.result.applied,true);assert.equal(h.commands().length,1);assert.equal(dispatched,true);
  assert.equal(escaped.readPresentedObjectSession(),null);assert.equal(h.session.readPresentedObjectSession().contentRevision,1);
  for(const fault of ['frame-before','projection','extra-pending','current-false','frame-in-guard','throwing-guard']){
    const options={},g=harness(options);await g.open();let checked=0,lease;
    const action=g.session.editFrames(op,payload,g.session.info(),proof=>{
      checked++;lease=proof;assert.equal(g.session.readPresentedObjectSession(),null);
      assert.ok(proof.readPresentedObjectSession());
      if(fault==='frame-in-guard')g.root.state.currentFrame++;
      if(fault==='throwing-guard')throw Error('guard threw');return fault!=='current-false';
    });
    if(fault==='frame-before')g.root.state.currentFrame++;
    if(fault==='projection')options.ready=false;
    const competing=fault==='extra-pending'?g.session.presentPreview(2):null;
    await assert.rejects(action,/intent is stale|guard threw/);if(competing)await competing;
    assert.equal(g.commands().length,0);assert.equal(g.session.info().contentRevision,0);
    assert.equal(checked,['frame-before','projection','extra-pending'].includes(fault)?0:1);
    if(lease)assert.equal(lease.readPresentedObjectSession(),null);
  }
});
