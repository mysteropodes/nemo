'use strict';
// A2 desktop-factory fixture adapted to actual frozen F06; native and DOM boundaries are fake.
const test=require('node:test'),assert=require('node:assert/strict');
const Project=require('../src/js/adapters/native-object-project.js');
const Ports=require('../src/js/adapters/native-object-project-desktop.js');
const Underlay=require('../src/js/adapters/native-viewport-underlay.js');
const Files=require('../src/js/adapters/native-object-project-file.js');
const Application=require('../src/js/adapters/native-application.js');
const CompositionUI=require('../src/js/adapters/native-composition-ui.js');
const PaletteUI=require('../src/js/adapters/native-palette-ui.js');
const CommentsUI=require('../src/js/adapters/native-comments-ui.js');
const PerspectiveUI=require('../src/js/adapters/native-perspective-ui.js');
const vm=require('node:vm'),fs=require('node:fs');
const turn=()=>new Promise(setImmediate);
const timerTurn=()=>new Promise(resolve=>setTimeout(resolve,0));
function gate(){let resolve;const promise=new Promise(r=>resolve=r);return {promise,resolve};}
async function bounded(promise,label,ending='settle'){let timer;try{return await Promise.race([promise,new Promise((_,reject)=>{
  timer=setTimeout(()=>reject(Error(label+' did not '+ending)),1000);})]);}finally{clearTimeout(timer);}}
function fixture(initialLayers=[{layerUid:'drawing',visible:true}],realSubscription=false){
  let installed=false,revision=0,generation=0,documentId=null,held=null,foreign=false,projectionRefused=false,rawAck=null,previewHold=null,replyMutation=null;
  let paletteReplyHold=null,commentsReplyHold=null;
  let syncFault=null,ackCheck=null;
  const listeners=new Map();
  const log=[],calls=[],classes=new Set(),styles=new Map(),transports=[],notifications=[];
  let layerRows=initialLayers.map(row=>({...row}));
  let rect={left:20.5,top:30.25,width:320.5,height:180.25};
  const document={addEventListener(){},removeEventListener(){},documentElement:{clientWidth:1200,clientHeight:900,
    classList:{contains:k=>classes.has(k),add:k=>classes.add(k),remove:k=>classes.delete(k)},
    style:{getPropertyValue:k=>styles.get(k)||'',getPropertyPriority:()=>'',setProperty:(k,v)=>styles.set(k,v),removeProperty:k=>styles.delete(k)}},
    getElementById:id=>{
      if(['palette-grid','palette-tabs','btn-palette-new','btn-palette-add-fill','btn-palette-add-stroke',
        'btn-palette-replace','btn-palette-swap','palette-replace-scope','fb-avatars-pop','zoom-scrub'].includes(id))return null;
      assert.equal(id,'drawing-canvas');return {getBoundingClientRect:()=>rect};}};
  const info=()=>({apiVersion:2,instanceId:'instance',documentId,contentRevision:revision,documentSnapshotId:'snapshot-'+generation+'-'+revision,lifecycleGeneration:generation,
    documentDigest:'a'.repeat(64),byteCount:100,canvasW:320,canvasH:180,fps:24,totalFrames:4,layers:layerRows.map(row=>({...row})),reference:null});
  const root={addEventListener(){},removeEventListener(){},document,stageLayer:{visible:true},paper:{view:{update(){}}},state:{currentFrame:0},devicePixelRatio:2,NemoNativeViewportUnderlay:Underlay,
    NemoNativeObjectProject:Project,
    NemoNativeObjectProjectFile:{...Files,refreshLayerTable:(r,i)=>{log.push('project');return !projectionRefused&&Files.refreshLayerTable(r,i);}},
    NemoNativeApplicationAdapter:Application,NemoNativeObjectUI:{refreshTiming:r=>{assert.equal(r,root);log.push('timing');},refreshPolicy:r=>{assert.equal(r,root);log.push('policy');}},
    NemoNativeProjectSettingsController:{willProject:i=>{assert.equal(i.documentId,documentId);log.push('willProject');},
      presented:i=>{notifications.push(i);log.push('presented');}},
    SMReference:{refreshNative:()=>log.push('reference')},SMProject:{refreshActiveTabDirtyDot:()=>log.push('dirty')},
    updateUI:()=>log.push('update'),showToast:text=>log.push('toast:'+text)};
  async function invoke(command,args){
    const r=args&&(args.request||JSON.parse(args.requestJson));calls.push({command,r});
    if(syncFault&&syncFault.command===command)throw syncFault.error;
    if(command==='nemo_native_object_client_status')return foreign?{...info(),available:true,instanceId:'foreign'}:{...info(),available:installed,instanceId:'instance'};
    if(command==='nemo_native_object_bootstrap'){installed=true;generation++;documentId='document-'+generation;revision=0;return {...info(),requestId:r.requestId,resourceCount:0,viewportAvailable:false};}
    if(command==='nemo_native_object_reference'){assert.equal(r.action.kind,'inspect');return {...info(),requestId:r.requestId};}
    if(command==='nemo_native_object_preview'){if(previewHold)await previewHold.promise;return {status:syncFault&&syncFault.status||'presented',workId:'work-'+calls.length,viewGeneration:1};}
    if(realSubscription&&command==='nemo_native_status')return {...info(),available:true};
    if(command==='nemo_native_revision_sync'){
      if(realSubscription&&r.action!=='acknowledge')return r.action==='binding'?{...info(),subscriptionId:'subscription-'+generation}:r.action==='subscribe'?r.binding:null;
      assert.equal(r.action,'acknowledge');if(ackCheck)ackCheck(r);log.push('native-ack');if(rawAck)await rawAck.promise;return null;
    }
    if(command==='nemo_native_dispatch'){
      log.push(r.operation);const p=r.payload;
      let result;
      const pin=info(),projectRead=['composition','palette','comments','perspective'].some(kind=>r.operation==='query.document.object.project.'+kind+'.read');
      if(projectRead){
        assert.equal(p.atRevision,pin.contentRevision);assert.equal(p.documentSnapshotId,pin.documentSnapshotId);
        assert.equal(p.contextId,'scene-root');
      }
      if(r.operation==='query.document.object.property.metadata'){
        if(held)await held.promise;
        result={instanceId:'instance',documentId:pin.documentId,atRevision:pin.contentRevision,
          documentSnapshotId:pin.documentSnapshotId,kind:'targets',target:null,
          items:[{kind:'layer',target:{contextId:'scene-root',layerUid:'drawing'}}],nextOffset:null,
          unavailableCategories:['conditionalMotionRows','pathVerticesTrimStrokeBrushMesh','referenceOnlyElements',
            'expressionDiagnostics','layerEffectMatteReveal','propertyWrites'].map(category=>({category,state:'unavailable',reason:'not-admitted'}))};
      }else if(r.operation==='query.document.object.project.composition.read'||r.operation==='query.document.object.project.palette.read'){
        const composition=r.operation==='query.document.object.project.composition.read';
        let value;
        if(p.selector.kind==='settings'){
          if(!composition&&paletteReplyHold)await paletteReplyHold.promise;
          value=composition?{markersPresent:false,markerCount:0,effectiveBpm:120,effectiveBpmOffset:0,effectiveBpmShow:false}:
            {palettesPresent:true,paletteCount:0,effectiveActivePaletteIdx:0};
        }else{
          assert.equal(p.selector.kind,composition?'markerPage':'palettePage');
          assert.equal(p.selector.cursor,0);assert.equal(p.selector.limit,16);
          value=composition?{kind:'markerPage',markersPresent:false,markerCount:0,cursor:0,items:[],nextCursor:null}:
            {kind:'palettePage',palettesPresent:true,paletteCount:0,cursor:0,items:[],nextCursor:null};
        }
        result=value;
      }else if(r.operation==='query.document.object.project.comments.read'){
        let value;
        if(p.selector.kind==='settings'){
          if(commentsReplyHold){const own=commentsReplyHold;commentsReplyHold=null;own.entered();await own.promise;}
          value={commentsPresent:false,commentCount:0};
        }else{
          assert.equal(p.selector.kind,'commentsPage');assert.equal(p.selector.cursor,0);assert.equal(p.selector.limit,16);
          value={kind:'projectCommentsPage',commentsPresent:false,commentCount:0,cursor:0,items:[],nextCursor:null};
        }
        result=value;
      }else if(r.operation==='query.document.object.project.perspective.read'){
        const settings={perspectiveVPsPresent:false,perspectiveVPsNull:false,perspectiveVPCount:0,effectiveVPCount:2,vpSource:'default'};
        if(p.selector.kind==='settings')result=settings;
        else{
          assert.equal(p.selector.kind,'vpPage');assert.equal(p.selector.cursor,0);assert.equal(p.selector.limit,16);
          result={kind:'perspectiveVPPage',...settings,cursor:0,items:[{ordinal:1,x:-48,y:90,locked:false},{ordinal:2,x:368,y:90,locked:false}],nextCursor:null};
        }
      }else if(r.operation==='command.document.object.layer.policy.edit'){
        assert.equal(r.expectedRevision,revision);revision++;result={applied:true,historyEntriesAdded:1};
      }else throw Error('unexpected fake public operation');
      if(projectRead)result={instanceId:'instance',documentId:pin.documentId,atRevision:pin.contentRevision,
        documentSnapshotId:pin.documentSnapshotId,contextId:'scene-root',result};
      const reply={apiVersion:2,requestId:r.requestId,instanceId:'instance',documentId:pin.documentId,contentRevision:r.operation.startsWith('query.')?pin.contentRevision:revision,ok:true,result};
      if(replyMutation)replyMutation(reply);return reply;
    }
    throw Error('Unexpected native fake command '+command);
  }
  root.__TAURI__={core:{invoke},event:{listen:async(name,callback)=>{listeners.set(name,callback);return ()=>listeners.delete(name);}}};
  // This is only a transport boundary. Protocol/SDK/session implementations remain production modules.
  root.NemoApplicationMcpTransport={createNativeTauriTransport:call=>{
    const transport={call,connected:false,binding:null,synchronize:null,disconnected:null,
      connect:async()=>{log.push('connect');transport.connected=true;},
      subscribeRevisions:async(listen,synchronize,disconnected)=>{log.push('subscribe');assert.equal(typeof listen,'function');transport.synchronize=synchronize;transport.disconnected=disconnected;transport.binding={...info(),subscriptionId:'subscription-'+generation};return transport.binding;},
      status:()=>transport.connected?transport.binding:null,isSynchronizing:()=>false,
      disconnect:async()=>{log.push('disconnect');transport.connected=false;},
      dispatch:r=>call('nemo_native_dispatch',{request:r}),dispatchObject:(c,r)=>call(c,{requestJson:JSON.stringify(r)})};
    transports.push(transport);return transport;
  }};
  if(realSubscription)root.NemoApplicationMcpTransport={createNativeTauriTransport:call=>{
    const value=require('../src/js/adapters/application-mcp.js').createNativeTauriTransport(call);transports.push(value);return value;
  }};
  const session=Project.desktop(root);
  const open=async()=>{const value=await session.openPath('/fake.json',{reveal:async i=>{Files.projectView(root,i);root.updateUI();}});
    await timerTurn();return value;};
  return {root,session,open,log,calls,classes,styles,transports,notifications,info,
    layers:value=>{layerRows=value.map(row=>({...row}));},paletteReplyGate:value=>{paletteReplyHold=value;},
    commentsReplyGate:value=>{commentsReplyHold=value;},
    syncFault:value=>{syncFault=value;},ackCheck:value=>{ackCheck=value;},
    hold:v=>held=v,foreign:v=>foreign=v,rect:v=>rect=v,refuseProjection:v=>projectionRefused=v,ackGate:v=>rawAck=v,previewGate:v=>previewHold=v,mutateReply:v=>replyMutation=v,
    async emit(){const t=transports.at(-1),from=revision;revision++;
      if(realSubscription)return listeners.get('nemo-native-revision')({payload:{instanceId:'instance',documentId,lifecycleGeneration:generation,
        requestId:'event-'+revision,fromRevision:from,toRevision:revision}});
      const event={...info(),requestId:'event-'+revision,fromRevision:from,toRevision:revision};return t.synchronize(event,t.binding);}};
}
test('actual Project desktop owns one cached session; builder creates ports without session/cache/initial I/O',()=>{
  const f=fixture();assert.equal(Project.desktop(f.root),f.session);assert.equal(f.calls.length,0);assert.equal(f.transports.length,0);
  const p=Ports.createPorts(f.root);assert.equal(typeof p.invoke,'function');assert.equal(typeof p.surfaceChanged,'function');assert.equal(f.calls.length,0);
  assert.equal(Project.desktop({}),null);assert.notEqual(fixture().session,f.session);
});
test('desktop factory preserves actual underlay mapping, frame and projection readiness',async()=>{
  const f=fixture(),p=Ports.createPorts(f.root);assert.equal(p.projectionReady(f.info()),false);await f.open();
  assert.equal(p.projectionReady(f.session.info()),true);f.root.state.currentFrame=2;assert.equal(p.frame(),2);
  const mapping=p.mapping(f.session.info());assert.deepEqual(mapping.cssBounds,{x:20.5,y:30.25,width:320.5,height:180.25});
  assert.deepEqual(mapping.physicalExtent,{width:641,height:361});assert.deepEqual(mapping.compositionExtent,{width:320,height:180});assert.equal(mapping.reportedDpr,2);
  assert.ok(f.classes.has('nemo-native-underlay'));assert.equal(f.styles.get('--nemo-native-left'),'20.5px');
  f.rect({left:45,top:60,width:200,height:100});await f.session.presentPreview(2);assert.equal(f.styles.get('--nemo-native-left'),'45px');
  assert.equal(f.calls.filter(c=>c.command==='nemo_native_object_preview').at(-1).r.frame,2);
});
test('actual desktop subscription and revision publication preserve ordered UI ports and one common transport',async()=>{
  const f=fixture();await f.open();assert.equal(f.transports.length,1);assert.deepEqual(f.log.filter(x=>['connect','subscribe'].includes(x)),['connect','subscribe']);
  const start=f.log.length;await f.emit();const after=f.log.slice(start);assert.deepEqual(after.slice(0,5),['willProject','project','update','reference','dirty']);
  assert.equal(f.session.info().contentRevision,1);assert.ok(f.classes.has('nemo-native-underlay'));assert.equal(f.transports.length,1);
});
test('actual desktop tracked invoke delays timing/settings notification until native ACK resolves',async()=>{
  const f=fixture();await f.open();const wait=gate();f.ackGate(wait);const t=f.transports[0],before=f.log.length;
  const pending=t.call('nemo_native_revision_sync',{request:{action:'acknowledge'}});await turn();assert.deepEqual(f.log.slice(before),['native-ack']);
  wait.resolve();await pending;await new Promise(resolve=>setTimeout(resolve,0));assert.deepEqual(f.log.slice(before),['native-ack','timing','policy','presented']);
});
test('cancelled replacement verifies original owner then restores exact underlay without a new preview',async()=>{
  const f=fixture();await f.open();const before=f.calls.length,wait=gate();const pending=f.session.openPath('/replacement',{confirm:()=>wait.promise});await turn();assert.equal(f.classes.has('nemo-native-underlay'),false);
  wait.resolve(false);assert.equal(await pending,false);assert.ok(f.classes.has('nemo-native-underlay'));assert.equal(f.styles.get('--nemo-native-left'),'20.5px');
  assert.deepEqual(f.calls.slice(before).map(c=>c.command),['nemo_native_object_client_status','nemo_native_object_reference']);
});
test('cancelled foreign owner fences and cannot restore underlay or dispatch metadata',async()=>{
  const f=fixture();await f.open();const before=f.calls.length,wait=gate();const pending=f.session.openPath('/replacement',{confirm:()=>wait.promise});await turn();f.foreign(true);wait.resolve(false);
  await assert.rejects(pending,/owner changed/);assert.equal(f.classes.has('nemo-native-underlay'),false);assert.equal(f.session.isIndeterminate(),true);
  assert.equal(f.calls.slice(before).filter(c=>c.command==='nemo_native_object_preview').length,0);await assert.rejects(f.session.readPropertyMetadata(selector(f))); 
  assert.ok(f.log.some(x=>x.startsWith('toast:Native project state is unconfirmed')));
});
test('projection refusal fences before UI publication and clears Main underlay',async()=>{
  const f=fixture();await f.open();f.refuseProjection(true);const start=f.log.length;
  await assert.rejects(f.emit(),/projection is uncorrelated/);assert.equal(f.session.isIndeterminate(),true);assert.equal(f.classes.has('nemo-native-underlay'),false);
  assert.deepEqual(f.log.slice(start).filter(x=>['willProject','project','update','reference','dirty'].includes(x)),['willProject','project']);
});
test('revision diagnostics witness synchronization order without claiming native ACK',async()=>{
  const f=fixture(),events=[];f.root.__NEMO_P38_FIXTURE_OPEN_DIAGNOSTIC__=(event,code)=>events.push([event,code]);
  await f.open();events.length=0;await f.emit();
  assert.deepEqual(events.filter(e=>e[0].startsWith('revision-')),[
    ['revision-stage','queued'],['revision-stage','inspect'],['revision-stage','publication'],
    ['revision-stage','present'],['revision-stage','final-projection'],['revision-stage','synchronized']]);
  assert.equal(f.session.info().contentRevision,1);assert.equal(f.session.isIndeterminate(),false);
  assert.equal(f.log.includes('native-ack'),false); // This fixture calls the session callback, not the transport ACK.
});
for(const [mode,expected]of [
  ['inspect','inspect:invoke-rejected:unavailable:none'],
  ['publication','publication:local-refusal:unavailable:none'],
  ['present','present:invoke-rejected:unavailable:none'],
  ['preview-status','present:local-refusal:unknown:status-deferred-occluded'],
  ['final-projection','final-projection:local-refusal:unknown:none']]){
  test('revision '+mode+' diagnostic preserves original failure and fencing',async()=>{
    const f=fixture(),events=[],original={code:'unavailable',message:'PRIVATE /Users/secret/document.json',stack:'PRIVATE STACK'};
    f.root.__NEMO_P38_FIXTURE_OPEN_DIAGNOSTIC__=(event,code)=>events.push([event,code]);
    await f.open();events.length=0;const before=f.calls.length;
    if(mode==='inspect')f.syncFault({command:'nemo_native_object_reference',error:original});
    if(mode==='publication')f.root.updateUI=()=>{throw original;};
    if(mode==='present')f.syncFault({command:'nemo_native_object_preview',error:original});
    if(mode==='preview-status')f.syncFault({status:'deferred-occluded'});
    if(mode==='final-projection')f.root.NemoNativeObjectProjectFile.projectionMatches=()=>false;
    let caught;try{await f.emit();assert.fail('synchronization must refuse');}catch(error){caught=error;}
    if(['inspect','publication','present'].includes(mode))assert.strictEqual(caught,original);
    const failures=events.filter(e=>e[0]==='revision-failed');assert.equal(failures.length,1);
    assert.equal(failures[0][1],expected);
    assert.equal(events.some(e=>e[1]==='synchronized'),false);
    assert.equal(f.session.isIndeterminate(),true);assert.equal(f.classes.has('nemo-native-underlay'),false);
    assert.equal(f.transports[0].connected,false);assert.equal(f.log.includes('native-ack'),false);
    const calls=f.calls.slice(before);assert.equal(calls.filter(c=>c.command==='nemo_native_object_reference').length,1);
    assert.equal(calls.filter(c=>c.command==='nemo_native_object_preview').length,['inspect','publication'].includes(mode)?0:1);
    assert.doesNotMatch(JSON.stringify(events),/PRIVATE|Users|secret|STACK/);
  });
}

function selector(f,view={kind:'targets',offset:0,limit:8}){
  const p=f.session.info();return {instanceId:p.instanceId,documentId:p.documentId,atRevision:p.contentRevision,
    documentSnapshotId:p.documentSnapshotId,view};
}
function readCalls(f){return f.calls.filter(c=>c.r&&c.r.operation==='query.document.object.property.metadata');}
function retained(f){return {info:f.session.info(),presented:f.session.readPresentedObjectSession(),generation:f.session.readSubscriptionGeneration()};}
function assertRetained(f,pin){assert.strictEqual(f.session.info(),pin.info);assert.deepEqual(f.session.readPresentedObjectSession(),pin.presented);assert.equal(f.session.readSubscriptionGeneration(),pin.generation);assert.equal(f.session.isIndeterminate(),false);}

test('metadata reader uses original subscribed SDK transport and resolves frozen correlated inner only',async()=>{
  const f=fixture();await f.open();const pin=retained(f),q=selector(f),before=f.calls.length;
  const result=await f.session.readPropertyMetadata(q);
  assert.deepEqual(Object.keys(result).sort(),['instanceId','documentId','atRevision','documentSnapshotId','kind','target','items','nextOffset','unavailableCategories'].sort());
  assert.equal(result.instanceId,q.instanceId);assert.equal(result.documentId,q.documentId);assert.equal(result.atRevision,q.atRevision);assert.equal(result.documentSnapshotId,q.documentSnapshotId);
  assert.deepEqual(result.items,[{kind:'layer',target:{contextId:'scene-root',layerUid:'drawing'}}]);assert.equal(Object.isFrozen(result.items[0].target),true);
  const calls=f.calls.slice(before);assert.equal(calls.length,1);assert.equal(calls[0].command,'nemo_native_dispatch');
  assert.match(calls[0].r.requestId,/^project-metadata-\d+$/);assert.equal('expectedRevision' in calls[0].r,false);
  assert.deepEqual(calls[0].r.payload,{atRevision:q.atRevision,documentSnapshotId:q.documentSnapshotId,view:q.view});
  assert.equal(f.transports.length,1);assertRetained(f,pin);
});
test('cached UI metadata callback is stable and delegates to the current sole Project session',async()=>{
  const f=fixture();await f.open();const captured=[];
  f.root.NemoNativeObjectUI.render=(_r,_info,_frame,ports)=>{captured.push(ports);return true;};
  assert.equal(Project.paintUI(f.root,false),true);assert.equal(Project.paintUI(f.root,true),true);
  assert.strictEqual(captured[0],captured[1]);assert.equal(Object.isFrozen(captured[0]),true);
  assert.strictEqual(captured[0].currentSession(),f.session);assert.strictEqual(Project.desktop(f.root),f.session);
  assert.equal((await captured[0].readPropertyMetadata(selector(f))).kind,'targets');
  const old=selector(f);await f.session.openPath('/second.json',{reveal:async i=>Files.projectView(f.root,i)});
  await assert.rejects(captured[0].readPropertyMetadata(old));
  assert.equal((await captured[0].readPropertyMetadata(selector(f))).documentId,f.session.info().documentId);
});
test('an entered component receives the frameOnly flag of the repaint (the scrub contract reaches its own paint)',async()=>{
  const f=await fixture();await f.open();const seen=[];
  f.root.NemoNativeComponentContext={paint:(_r,frameOnly)=>{seen.push(frameOnly);return true;}};
  f.root.NemoNativeObjectUI.render=()=>{throw new Error('the scene paint must not run');};
  assert.equal(Project.paintUI(f.root,true),true);assert.equal(Project.paintUI(f.root,false),true);assert.equal(Project.paintUI(f.root),true);
  assert.deepEqual(seen,[true,false,undefined]);
});
test('metadata exact selector and SDK view validation reject malformed inputs before transport or owner changes',async()=>{
  const f=fixture();await f.open();const pin=retained(f),good=selector(f),bad=[null,[],{...good,extra:true}];
  for(const key of Object.keys(good)){const q={...good};delete q[key];bad.push(q);}
  for(const key of ['instanceId','documentId','atRevision','documentSnapshotId'])bad.push({...good,[key]:key==='atRevision'?1:'foreign'});
  bad.push({...good,view:null},{...good,view:{kind:'targets',offset:0,limit:9}},{...good,view:{kind:'targets',offset:0,limit:8,extra:true}});
  const before=readCalls(f).length;for(const q of bad)await assert.rejects(f.session.readPropertyMetadata(q));
  assert.equal(readCalls(f).length,before);assertRetained(f,pin);
});
test('inactive, unsubscribed or projection-unready metadata reads never dispatch',async()=>{
  const cold=fixture();await assert.rejects(cold.session.readPropertyMetadata({instanceId:'i',documentId:'d',atRevision:0,documentSnapshotId:'s',view:{kind:'targets',offset:0,limit:8}}));assert.equal(cold.calls.length,0);
  for(const mode of ['unsubscribed','projection']){const f=fixture();await f.open();const pin=retained(f);
    if(mode==='unsubscribed')f.transports[0].connected=false;else f.root.NemoNativeObjectProjectFile.projectionMatches=()=>false;
    await assert.rejects(f.session.readPropertyMetadata(selector(f)));assert.equal(readCalls(f).length,0);assert.strictEqual(f.session.info(),pin.info);assert.equal(f.session.isIndeterminate(),false);
  }
});
test('same-revision presentation replacement rejects an in-flight read without entering its queue',async()=>{
  const f=fixture();await f.open();const hold=gate();f.hold(hold);const q=selector(f),pin=f.session.info();
  const pending=f.session.readPropertyMetadata(q);await turn();await f.session.presentPreview(2);
  hold.resolve();await assert.rejects(pending,/presentation changed/);assert.strictEqual(f.session.info(),pin);assert.equal(f.session.readPresentedObjectSession().presentation.frame,2);
});
test('same-owner cancelled replacement changes subscription generation and rejects the old read',async()=>{
  const f=fixture();await f.open();const old=retained(f),hold=gate();f.hold(hold);
  const pending=f.session.readPropertyMetadata(selector(f));await turn();assert.equal(await f.session.openPath('/cancel',{confirm:async()=>false}),false);
  assert.deepEqual(f.session.info(),old.info);assert.ok(f.session.readSubscriptionGeneration()>old.generation);
  hold.resolve();await assert.rejects(pending,/presentation changed/);assert.equal(f.session.isIndeterminate(),false);
});
test('pending preview rejects an in-flight read immediately and does not settle or advance its work',async()=>{
  const f=fixture();await f.open();const readHold=gate(),previewHold=gate();f.hold(readHold);f.previewGate(previewHold);
  const pending=f.session.readPropertyMetadata(selector(f));await turn();let finished=false;
  const preview=f.session.presentPreview(2).then(()=>{finished=true;});await turn();readHold.resolve();
  await assert.rejects(pending,/presentation changed/);assert.equal(finished,false);
  previewHold.resolve();await preview;assert.equal(finished,true);
});
function bpmEntryDOM(f){
  const document=f.root.document,lookup=document.getElementById,pagehide=[];
  function element(tag){const el={tagName:tag,children:[],style:{},events:{},classList:{toggle(){}},
    setAttribute(k,v){this[k]=String(v);},addEventListener(k,fn){this.events[k]=fn;},
    appendChild(child){child.parentNode=this;this.children.push(child);return child;},
    focus(){document.activeElement=this;},select(){},remove(){if(this.parentNode)this.parentNode.children=this.parentNode.children.filter(v=>v!==this);}};return el;}
  const button=element('button');document.body=element('body');document.readyState='complete';
  document.createElement=element;document.activeElement=null;
  document.getElementById=id=>id==='btn-bpm'?button:id==='frame-grid'?null:lookup(id);
  f.root.window=f.root;f.root.addEventListener=(kind,fn)=>{if(kind==='pagehide')pagehide.push(fn);};
  f.root.NemoNativeObjectProjectDesktop=Ports;
  return {document,button,pagehide,dialogs:()=>document.body.children.filter(v=>v.id==='bpm-settings-dialog')};
}
test('actual Composition recovers after a same-frame pending preview without weakening presentation guards',()=>withPendingBpmEntry(async c=>{
  const {f,hold,composition,preview,before,frame}=c;
    assert.equal(f.calls.filter(c=>c.command==='nemo_native_object_preview').length,c.previewCount+1);
    assert.equal(f.session.readPresentedObjectSession(),null);
    const refused=[];assert.equal(composition.read(code=>refused.push(code)),null);
    assert.deepEqual(refused,['presentation-missing']);
    assert.strictEqual(f.session.info(),before.info);assert.equal(f.root.state.currentFrame,frame);
    hold.resolve();await preview;await timerTurn();await turn();await timerTurn();await turn();
    const recovered=composition.read();assert.ok(recovered,'existing presented fanout must recover the complete composition projection');
    assert.equal(recovered.composition.effectiveBpm,120);assert.equal(recovered.composition.effectiveBpmOffset,0);
    assert.equal(recovered.composition.effectiveBpmShow,false);
    for(const key of ['instanceId','documentId','lifecycleGeneration','contentRevision','documentSnapshotId'])
      assert.equal(recovered.info[key],before.info[key]);
    assert.strictEqual(f.session.info(),before.info);assert.equal(f.session.readSubscriptionGeneration(),before.generation);
    assert.equal(f.root.state.currentFrame,frame);assert.equal(f.session.isIndeterminate(),false);
    assert.ok(f.calls.filter(c=>c.command==='nemo_native_object_preview').every(c=>c.r.frame===frame));
    assert.ok(f.calls.filter(c=>c.command==='nemo_native_dispatch').every(c=>c.r.operation.startsWith('query.')));
    const settled=f.calls.length;await timerTurn();await turn();assert.equal(f.calls.length,settled);
},false));
async function withPendingBpmEntry(check,bpm=true){
  const f=fixture(),hold=gate(),dom=bpm?bpmEntryDOM(f):null,c={f,hold,dom,jobs:[],composition:null,replacement:null,preview:null,timer:null};
  try{
    await f.open();c.composition=CompositionUI.install(f.root);c.origin=await c.composition.presented();assert.ok(c.origin);
    if(bpm)vm.runInNewContext(fs.readFileSync(require.resolve('../src/js/bpm-grid.js'),'utf8'),f.root);
    else {await timerTurn();await turn();}
    c.before=retained(f);c.frame=f.root.state.currentFrame;
    c.previewCount=f.calls.filter(call=>call.command==='nemo_native_object_preview').length;
    assert.ok(c.composition.read());
    f.previewGate(hold);c.preview=f.session.renderPreview(c.frame);await turn();
    assert.equal(f.session.readPresentedObjectSession(),null);
    await check(c);
  }finally{
    clearTimeout(c.timer);if(dom)dom.pagehide.forEach(fn=>fn());if(c.composition)c.composition.dispose();if(c.replacement)c.replacement.dispose();
    hold.resolve();await Promise.allSettled([c.preview,...c.jobs]);await timerTurn();await turn();
  }
}
async function ordinaryBpmEntry(requests){
  await withPendingBpmEntry(async c=>{
    const {f,hold,dom,composition,before,frame,preview}=c;let prevented=false;
    for(let i=0;i<requests;i++)dom.button.events.contextmenu({preventDefault(){prevented=true;}});
    assert.equal(prevented,true);assert.equal(dom.document.body.children.length,0,'a pending preview cannot authorize a dialog');
    const waiting=requests>1?composition.readWhenPresented():null;
    assert.equal(dom.dialogs().length,0);assert.strictEqual(f.session.info(),before.info);
    hold.resolve();await preview;await timerTurn();await turn();await timerTurn();await turn();
    assert.ok(composition.read(),'the real completion fanout recovered the composition');
    if(waiting){
      const value=await bounded(waiting,'duplicate entry waiter');assert.strictEqual(value,composition.read(),'duplicate requests must resolve the actual current projection');
    }
    assert.equal(dom.document.body.children.filter(v=>v.id==='bpm-settings-dialog').length,1,
      'the original settings gesture must open once after its unchanged owner is presented');
    assert.equal(dom.dialogs().length,1);assert.equal(dom.document.body.children.length,1);
    assertRetained(f,{...before,presented:f.session.readPresentedObjectSession()});
    assert.equal(f.root.state.currentFrame,frame);
    assert.ok(f.calls.filter(call=>call.command==='nemo_native_dispatch').every(call=>call.r.operation.startsWith('query.')));
    assert.equal(f.session.info().contentRevision,0);assert.equal(f.session.readSubscriptionGeneration(),before.generation);
    const count=f.calls.length;await timerTurn();await turn();assert.equal(f.calls.length,count);
  });
}
test('one ordinary BPM entry survives a same-frame pending preview without a second gesture',()=>ordinaryBpmEntry(1));
test('duplicate ordinary BPM entries share one bounded wait and open one form without native edits',()=>ordinaryBpmEntry(4));

test('deferred ordinary BPM entry cannot survive a real revision, owner, subscription, lifecycle or controller change',async()=>{
  for(const mode of ['revision','owner','subscription','disconnected','disposed','pagehide','controller','competing-edit'])await withPendingBpmEntry(async c=>{
    const {f,hold,dom,composition,origin,before,preview}=c;let change=null;
    dom.button.events.contextmenu({preventDefault(){}});
    const waiting=composition.readWhenPresented();assert.equal(dom.dialogs().length,0);
    if(mode==='revision')change=f.emit();
    else if(mode==='owner')change=f.session.openPath('/replacement.json',{reveal:async info=>Files.projectView(f.root,info)});
    else if(mode==='subscription')change=f.session.openPath('/cancel.json',{confirm:async()=>false});
    else if(mode==='disconnected')f.transports[0].disconnected(new Error('controlled genuine subscription retirement'));
    else if(mode==='disposed')composition.dispose();
    else if(mode==='pagehide')dom.pagehide.forEach(fn=>fn());
    else if(mode==='controller'){const other=fixture();c.replacement=CompositionUI.install(other.root);f.root.NemoNativeCompositionController=c.replacement;}
    else assert.equal((await composition.edit({kind:'setBpm',bpmShow:true},origin)).status,'unavailable');
    if(change){c.jobs.push(change);change.catch(()=>{});}
    hold.resolve();await preview;if(change)await change;
    await timerTurn();await turn();await timerTurn();await turn();
    // Exercise late publication through the replaced controller's real callback; the current slot remains replaced.
    if(mode==='controller')await composition.presented();
    const resolved=await bounded(waiting,mode+' waiter','retire');if(mode!=='controller')assert.equal(resolved,null,mode);
    assert.equal(dom.dialogs().length,0,mode+' must not replay the old ordinary entry');
    assert.equal(dom.document.body.children.length,0,mode);
    assert.ok(f.calls.filter(call=>call.command==='nemo_native_dispatch').every(call=>call.r.operation.startsWith('query.')),mode);
    if(mode==='revision')assert.equal(f.session.info().contentRevision,1);
    else assert.equal(f.session.info().contentRevision,0);
    if(mode==='owner')assert.notEqual(f.session.info().documentId,before.info.documentId);
    if(mode==='subscription'){assert.strictEqual(f.session.info(),before.info);assert.ok(f.session.readSubscriptionGeneration()>before.generation);}
    if(mode==='disconnected')assert.equal(f.session.isIndeterminate(),true);
    const count=f.calls.length;await timerTurn();await turn();assert.equal(f.calls.length,count,mode);
  });
});

test('readiness loss and native synchronization both refuse delayed reads without owner fencing',async()=>{
  for(const mode of ['disconnect','projection','revision']){const f=fixture();await f.open();const hold=gate();f.hold(hold);const pending=f.session.readPropertyMetadata(selector(f));await turn();
    if(mode==='disconnect')f.transports[0].connected=false;
    else if(mode==='projection')f.root.NemoNativeObjectProjectFile.projectionMatches=()=>false;
    else await f.emit();
    hold.resolve();await assert.rejects(pending,/presentation changed/);assert.equal(f.session.isIndeterminate(),false);
  }
});
test('actual SDK rejects uncorrelated or malformed metadata envelopes and preserves owner pins',async()=>{
  for(const mutate of [r=>r.requestId='wrong',r=>r.instanceId='wrong',r=>r.documentId='wrong',r=>r.contentRevision=99,
    r=>r.result.documentSnapshotId='wrong',r=>r.result.atRevision=99,r=>r.result.kind='properties',
    r=>r.result.nextOffset=2,r=>r.result.items[0].extra=true,r=>r.result.unavailableCategories.pop(),
    r=>{r.ok=false;delete r.result;r.error={code:'unavailable',message:'x'.repeat(4096)};}]){
    const f=fixture();await f.open();const pin=retained(f);f.mutateReply(mutate);
    await assert.rejects(f.session.readPropertyMetadata(selector(f)));assertRetained(f,pin);
  }
});
test('provider unavailable and missing SDK reject reads without fallback or writable state changes',async()=>{
  const f=fixture();await f.open();const pin=retained(f);
  f.mutateReply(r=>{r.ok=false;delete r.result;r.error={code:'unavailable',message:'missing provider'};});
  await assert.rejects(f.session.readPropertyMetadata(selector(f)),e=>e.code==='unavailable');assertRetained(f,pin);
  f.root.NemoNativeApplicationAdapter={};await assert.rejects(f.session.readPropertyMetadata(selector(f)),/client is unavailable/);assertRetained(f,pin);
});
test('factory retains policy-specific SDK dispatch and policy notifications after native ACK',async()=>{
  const f=fixture();await f.open();const ports=Ports.createPorts(f.root);let dispatched=0;
  assert.equal(ports.policyAvailable(),false); // This separate test factory has no subscription transport yet.
  const t=f.transports[0];f.root.NemoNativeApplicationAdapter={createNativeApplicationAdapter:()=>({dispatch:()=>assert.fail('generic policy route'),editLayerPolicy:q=>{dispatched++;return q;}})};
  assert.equal(typeof t.call,'function');const q={operation:'command.document.object.layer.policy.edit'};
  // Test a connected factory instance directly; it still creates no Project session.
  await ports.subscribeRevisions(()=>{},()=>{});assert.equal(ports.policyAvailable(),true);
  assert.strictEqual(await ports.dispatchApplication(q),q);assert.equal(dispatched,1);
  const before=f.log.length;ports.presented(f.session.info());await timerTurn();assert.deepEqual(f.log.slice(before),['presented','timing','policy']);
});

test('SDK-valid advancing partial-page cursor is preserved without a local completeness guess',async()=>{
  const f=fixture();await f.open();f.mutateReply(r=>r.result.nextOffset=1);
  const result=await f.session.readPropertyMetadata(selector(f));assert.equal(result.nextOffset,1);assert.equal(result.items.length,1);
});


test('real desktop ports project frozen opaque parent metadata through revision refresh without document writers',async()=>{
  const parent='親:雪/opaque',base=[{layerUid:parent,visible:true},{layerUid:'drawing',visible:true,parentLayerUid:parent}];
  const f=fixture(base);await f.open();const ports=Ports.createPorts(f.root),table=f.root.state.layers,child=table[1];
  const initial=f.session.info();assert.equal(initial.layers[1].parentLayerUid,parent);
  assert.equal(Object.isFrozen(initial.layers[1]),true);assert.equal(child.parentLayerUid,parent);
  assert.equal(ports.projectionReady(initial),true);
  for(const nextRows of [[base[0],{...base[1],parentLayerUid:null}],base,[base[0],{layerUid:'drawing',visible:true}]]){
    f.layers(nextRows);await f.emit();
    const current=f.session.info();assert.strictEqual(f.root.state.layers,table);assert.strictEqual(table[1],child);
    assert.equal(child.parentLayerUid,Object.hasOwn(nextRows[1],'parentLayerUid')?nextRows[1].parentLayerUid:null);
    assert.equal(ports.projectionReady(current),true);assert.equal(Object.isFrozen(current.layers[1]),true);
  }
  assert.equal(f.calls.some(c=>c.r&&c.r.operation&&c.r.operation.startsWith('command.')),false);
});
test('actual desktop projection readiness rejects same-revision parent drift with complete UI state intact',async()=>{
  const parent='親:雪',f=fixture([{layerUid:parent},{layerUid:'drawing',parentLayerUid:parent}]);await f.open();
  const ports=Ports.createPorts(f.root),current=f.session.info(),before=JSON.stringify(f.root.state);
  const changed={...current,layers:current.layers.map((row,i)=>i===1?{...row,parentLayerUid:null}:{...row})};
  assert.equal(ports.projectionReady(changed),false);
  assert.equal(Files.refreshLayerTable(f.root,changed),false);assert.equal(JSON.stringify(f.root.state),before);
  assert.equal(ports.projectionReady(current),true);assert.strictEqual(f.session.info(),current);
});
test('real desktop metadata rejects a late malformed parent before any partial projection publication',async()=>{
  const parent='親:雪',base=[{layerUid:parent,visible:true},{layerUid:'drawing',visible:true,parentLayerUid:parent}];
  const f=fixture(base);await f.open();const before=JSON.stringify(f.root.state),table=f.root.state.layers;
  f.layers([{...base[0],visible:false},{...base[1],parentLayerUid:''}]);
  await assert.rejects(f.emit(),/metadata.*uncorrelated/i);
  assert.equal(JSON.stringify(f.root.state),before);assert.strictEqual(f.root.state.layers,table);
  assert.equal(f.session.isIndeterminate(),true);
  assert.equal(f.calls.some(c=>c.r&&c.r.operation&&c.r.operation.startsWith('command.')),false);
});

test('actual desktop queue settles composition and palette read-only projections before synchronous controller fanout can hide them',async()=>{
  const f=fixture();await f.open();f.root.NemoNativeObjectProject=Project;
  const before=retained(f),hold=gate();f.paletteReplyGate(hold);
  let composition=null,palette=null,paletteRead=null;
  try{
    composition=CompositionUI.install(f.root);palette=PaletteUI.install(f.root);
    // The original synchronous resource-finally fanout started Palette before
    // the composition continuation. The first result now survives that boundary.
    const first=await composition.presented();
    paletteRead=palette.presented();
    assert.ok(first,'the first composition projection must survive another controller entering the real queue');
    hold.resolve();await paletteRead;
    const compositionValue=composition.read(),paletteValue=palette.read();
    assert.ok(compositionValue);assert.ok(paletteValue);
    assert.deepEqual(compositionValue.info,paletteValue.info);
    assert.equal(compositionValue.info.contentRevision,0);
    for(const key of ['instanceId','documentId','lifecycleGeneration','contentRevision','documentSnapshotId'])
      assert.equal(compositionValue.info[key],before.info[key]);
    const queries=f.calls.filter(call=>call.command==='nemo_native_dispatch');
    assert.deepEqual(queries.filter(call=>call.r.operation==='query.document.object.project.composition.read')
      .map(call=>call.r.payload.selector.kind),['settings','markerPage']);
    assert.deepEqual(queries.filter(call=>call.r.operation==='query.document.object.project.palette.read')
      .map(call=>call.r.payload.selector.kind),['settings','palettePage']);
    assert.equal(queries.length,4);
    assert.ok(queries.every(call=>call.r.operation.startsWith('query.')&&!Object.hasOwn(call.r,'expectedRevision')));
    assert.equal(f.transports.length,1);assertRetained(f,before);
  }finally{
    // Capture any already-held Palette read before disposal, even if an earlier
    // assertion fails. Retire both controllers before releasing the fake reply
    // so their existing presented callbacks cannot start a microtask retry loop.
    if(palette&&!paletteRead)paletteRead=palette.presented();
    if(composition)composition.dispose();
    if(palette)palette.dispose();
    hold.resolve();
    if(paletteRead)await paletteRead;
  }
});

test('completion notification drops stale revision, owner, subscription and same-revision presentation captures',async()=>{
  for(const mode of ['revision','owner','subscription','presentation']){
    const f=fixture();await f.open();const ports=Ports.createPorts(f.root),old=f.session.info();
    const before=f.notifications.length,start=f.log.length;
    ports.presented(old);
    if(mode==='revision')await f.emit();
    if(mode==='owner')await f.session.openPath('/replacement.json',{reveal:async info=>Files.projectView(f.root,info)});
    if(mode==='subscription')assert.equal(await f.session.openPath('/cancel.json',{confirm:async()=>false}),false);
    if(mode==='presentation')await f.session.presentPreview(2);
    await timerTurn();
    const fresh=f.notifications.slice(before);
    // Revision synchronization captured a null public presentation while busy;
    // the separately tested native ACK timer owns its final notification.
    const expected=mode==='revision'?0:1;
    assert.equal(fresh.length,expected,mode);
    assert.deepEqual(f.log.slice(start).filter(value=>['presented','policy'].includes(value)),
      expected?['presented','policy']:[],mode);
    if(expected)assert.strictEqual(fresh[0],f.session.info());
    if(mode==='revision')assert.equal(f.session.info().contentRevision,1);
    if(mode==='owner')assert.notEqual(f.session.info().documentId,old.documentId);
    if(mode==='subscription')assert.strictEqual(f.session.info(),old);
    if(mode==='presentation'){
      assert.strictEqual(f.session.info(),old);
      assert.equal(f.session.readPresentedObjectSession().presentation.frame,2);
    }
  }
});

test('timing and Motion completion require the captured owner, document and frame; Motion also requires active mode',async()=>{
  for(const mode of ['current','frame','mode','owner','document','unconfirmed']){
    const f=fixture();await f.open();const ports=Ports.createPorts(f.root),paints=[],timings=[];
    f.root.state.appMode='motion';
    f.root.NemoNativeObjectUI.paintMotionInspector=root=>paints.push(root);
    f.root.NemoNativeObjectUI.refreshTiming=root=>timings.push(root);
    ports.presented(f.session.info());
    if(mode==='frame')f.root.state.currentFrame=1;
    if(mode==='mode')f.root.state.appMode='animation';
    if(mode==='owner')delete f.root.NemoNativeObjectProject;
    if(mode==='document')f.root.document={...f.root.document};
    if(mode==='unconfirmed')f.transports[0].disconnected(new Error('controlled subscription loss'));
    await timerTurn();
    assert.deepEqual(paints,mode==='current'?[f.root]:[],mode);
    assert.deepEqual(timings,['current','mode'].includes(mode)?[f.root]:[],mode);
  }
});

test('newest same-port completion is coalesced and callback exceptions cannot reject settled native work',async()=>{
  const f=fixture();await f.open();const ports=Ports.createPorts(f.root),pin=retained(f);
  const before=f.notifications.length,start=f.log.length;
  ports.presented(pin.info);ports.presented(pin.info);await timerTurn();
  assert.equal(f.notifications.length-before,1);
  assert.deepEqual(f.log.slice(start),['presented','timing','policy']);assertRetained(f,pin);
  let attempted=0;
  f.root.NemoNativeProjectSettingsController.presented=()=>{attempted++;throw Error('controlled advisory callback failure');};
  await f.session.presentPreview(1);await timerTurn();
  assert.equal(attempted,1);
  assert.equal(f.session.info().contentRevision,0);
  assert.equal(f.session.readPresentedObjectSession().presentation.frame,1);
  assert.equal(f.session.isIndeterminate(),false);
});

test('fresh fenced completion with two null presentations still renders disabled lifecycle UI',async()=>{
  const f=fixture();await f.open();const ports=Ports.createPorts(f.root),info=f.session.info(),disabled=[];
  const original=f.root.NemoNativeProjectSettingsController.presented;
  f.root.NemoNativeProjectSettingsController.presented=value=>{
    disabled.push(f.session.isIndeterminate()&&f.session.readPresentedObjectSession()===null);original(value);
  };
  const before=f.notifications.length,start=f.log.length;
  ports.presented(info);
  f.transports[0].disconnected(new Error('controlled subscription loss'));
  assert.equal(f.session.isIndeterminate(),true);
  assert.equal(f.session.readPresentedObjectSession(),null);
  ports.presented(f.session.info());await timerTurn();
  assert.deepEqual(disabled,[true]);assert.equal(f.notifications.length-before,1);
  assert.strictEqual(f.notifications.at(-1),info);
  assert.deepEqual(f.log.slice(start).filter(value=>['presented','policy'].includes(value)),['presented','policy']);
  assert.equal(f.session.readPresentedObjectSession(),null);
});

test('unknown current owner and lookalike metadata cannot receive completion notification',async()=>{
  const f=fixture();await f.open();const ports=Ports.createPorts(f.root),info=f.session.info();
  const before=f.notifications.length,start=f.log.length,calls=f.calls.length;
  ports.presented(info);delete f.root.NemoNativeObjectProject;await timerTurn();
  assert.equal(f.notifications.length,before);assert.deepEqual(f.log.slice(start),[]);
  f.root.NemoNativeObjectProject=Project;
  ports.presented({...info});await timerTurn();
  assert.equal(f.notifications.length,before);assert.equal(f.calls.length,calls);
});


// Positive original-consumer oracle: native and DOM replies are controlled,
// while Project/Desktop notification, SDK, models and controllers are production.
test('ordinary Open readonly lifecycle converges and composition recovers after Comments queue settlement',async()=>{
  const f=fixture(),compositionPublications=[],commentPublications=[],refusals=[];
  let composition=null,comments=null;
  function readGate(){const held=gate(),started=gate();return {...held,started:started.promise,entered:started.resolve};}
  const first=readGate(),second=readGate(),firstCommentPublication=gate();
  // Capture only complete readbacks through the existing controller repaint ports.
  f.root.SMBpm={refresh(){const value=composition&&composition.read();if(value)compositionPublications.push(value);}};
  f.root.refreshFbAvatars=()=>{const value=comments&&comments.read();if(value){
    if(commentPublications[commentPublications.length-1]!==value)commentPublications.push(value);
    firstCommentPublication.resolve(value);}};
  try{
    composition=CompositionUI.install(f.root);comments=CommentsUI.install(f.root);
    await turn(); // Original inactive installation settles before the ordinary user Open.
    f.commentsReplyGate(first);await f.open();await bounded(first.started,'first Comments settings');
    assert.equal(compositionPublications.length,1,'ordinary Open must publish the complete composition before Comments enters');
    const shown=compositionPublications[0],owner=f.session.info();
    assert.equal(shown.composition.effectiveBpm,120);assert.equal(shown.composition.effectiveBpmOffset,0);
    assert.equal(shown.composition.effectiveBpmShow,false);assert.deepEqual(shown.composition.markers,[]);
    assert.equal(Object.isFrozen(shown),true);assert.equal(shown.info.contentRevision,0);
    assert.equal(f.session.readPresentedObjectSession(),null,'held native queue work must remain unavailable');
    assert.equal(composition.read(code=>refusals.push(code)),null,'a busy reader cannot expose the hidden presentation');
    assert.equal(composition.read(code=>refusals.push(code)),null,'repeated busy reads must remain unavailable');
    // No direct presented() or manual refresh requests any later work. Hold a
    // repeated query if production issues one, so a failing oracle cannot loop.
    f.commentsReplyGate(second);first.resolve();
    await bounded(firstCommentPublication.promise,'first complete Comments projection');
    await timerTurn();await turn(); // Drain the existing post-completion callback, not a delay-based retry.
    const recovered=composition.read(code=>refusals.push(code));
    const queries=f.calls.filter(call=>call.command==='nemo_native_dispatch');
    const commentQueries=queries.filter(call=>call.r.operation==='query.document.object.project.comments.read');
    assert.equal(commentQueries.filter(call=>call.r.payload.selector.kind==='settings').length,1,
      'current same-pin Comments must not start another read; refusal observations='+JSON.stringify(refusals));
    assert.deepEqual(commentQueries.map(call=>call.r.payload.selector.kind),['settings','commentsPage']);
    assert.ok(recovered,'BPM composition must recover after readonly queue completion without a manual refresh');
    assert.deepEqual(recovered.info,shown.info);assert.deepEqual(recovered.composition,shown.composition);
    assert.equal(f.session.info(),owner);assert.ok(f.session.readPresentedObjectSession());
    assert.equal(commentPublications.length,1);assert.deepEqual(commentPublications[0].info,shown.info);
    assert.ok(queries.every(call=>call.r.operation.startsWith('query.')&&!Object.hasOwn(call.r,'expectedRevision')));
    const count=queries.length;await timerTurn();await turn();
    assert.equal(f.calls.filter(call=>call.command==='nemo_native_dispatch').length,count,'settled callbacks must converge; observations='+JSON.stringify({refusals:refusals,
      dispatches:f.calls.filter(call=>call.command==='nemo_native_dispatch').map(call=>({operation:call.r.operation,
        selectorKind:call.r.payload&&call.r.payload.selector&&call.r.payload.selector.kind,atRevision:call.r.payload&&call.r.payload.atRevision})),
      infoRevision:f.session.info().contentRevision,publicPresented:!!f.session.readPresentedObjectSession()}));
    assert.equal(f.session.info().contentRevision,0);assert.equal(f.transports.length,1);assert.equal(f.session.isIndeterminate(),false);
  }finally{
    // Retire actual controllers before release so failed assertions cannot leak a timer/query loop.
    if(composition)composition.dispose();if(comments)comments.dispose();
    first.resolve();second.resolve();await turn();await timerTurn();await turn();
  }
});

test('ordinary Open metadata settles when Perspective repaint re-enters the actual coalescing native preview queue',async()=>{
  const f=fixture(),published=gate(),renders=new Set(),seen=new Set();
  let composition=null,comments=null,perspective=null,repaints=0,timer,off=null,opening=null;
  function observed(kind,value){if(value){seen.add(kind);if(seen.size===3)published.resolve();}}
  f.root.SMBpm={refresh(){observed('composition',composition&&composition.read());}};
  f.root.refreshFbAvatars=()=>observed('comments',comments&&comments.read());
  // Preserve Project's actual wantedFrame/rendering coalescing without limiting finite repaints.
  f.root.SMEngineBridge={renderNow(){repaints++;
    const job=f.session.renderPreview(f.root.state.currentFrame);renders.add(job);return job;}};
  try{
    composition=CompositionUI.install(f.root);comments=CommentsUI.install(f.root);perspective=PerspectiveUI.install(f.root);
    off=perspective.onRead(value=>observed('perspective',value));
    await turn();opening=f.open();
    await Promise.race([opening.then(()=>published.promise),new Promise((_,reject)=>{
      timer=setTimeout(()=>{f.root.SMEngineBridge.renderNow=()=>{};
        reject(Error('ordinary Open metadata did not settle with Perspective preview re-entry: '+JSON.stringify({seen:[...seen],repaints,
          calls:f.calls.slice(-12).map(call=>({command:call.command,operation:call.r&&call.r.operation,
            selector:call.r&&call.r.payload&&call.r.payload.selector&&call.r.payload.selector.kind}))})));},1000);})]);
    clearTimeout(timer);await timerTurn();await turn();
    const owner=f.session.info(),before=retained(f);
    const values=[composition.read(),comments.read(),perspective.read()];
    assert.ok(values.every(Boolean),'all ordinary metadata controllers must remain readable after publication');
    for(const value of values)assert.deepEqual(value.info,values[0].info);
    assert.equal(values[0].composition.effectiveBpm,120);assert.equal(values[0].composition.effectiveBpmOffset,0);
    assert.equal(values[0].composition.effectiveBpmShow,false);
    assert.deepEqual(values[2].perspective.items,[{ordinal:1,x:-48,y:90,locked:false},{ordinal:2,x:368,y:90,locked:false}]);
    assert.strictEqual(f.session.info(),owner);assert.ok(f.session.readPresentedObjectSession());
    const count=f.calls.length,renderCount=repaints;await timerTurn();await turn();await timerTurn();await turn();
    assert.equal(f.calls.length,count,'stable metadata must not schedule further native preview or query work');
    assert.equal(repaints,renderCount,'stable notifications must not repaint into the preview queue');
    assert.ok(f.calls.filter(call=>call.command==='nemo_native_dispatch').every(call=>call.r.operation.startsWith('query.')));
    assert.equal(owner.contentRevision,0);assert.equal(f.transports.length,1);assertRetained(f,before);
  }finally{
    clearTimeout(timer);f.root.SMEngineBridge.renderNow=()=>{};
    if(off)off();if(perspective)perspective.dispose();if(composition)composition.dispose();if(comments)comments.dispose();
    await Promise.allSettled([...renders,opening]);await timerTurn();await turn();
  }
});

test('desktop reads only existing session Camera View choice and keeps the native UI lock hook',()=>{
 const seen=[],root={state:{cameraView:false},__TAURI__:{core:{invoke(){throw Error('no host request');}}},NemoNativeViewportUnderlay:Underlay,
  SMCamera:{renderNativeViewLock(){seen.push('lock');}}};
 const ports=Ports.createPorts(root);assert.equal(ports.viewMode(),'canvas');root.state.cameraView=true;assert.equal(ports.viewMode(),'stored-camera');
 Ports.didPaintUI(root,{bindPolicyEntries(){seen.push('policy');}});assert.deepEqual(seen,['policy','lock']);
 Object.defineProperties(root.state,{cameraLayerOn:{get(){throw Error('legacy camera layer read');}},cameraKeys:{get(){throw Error('legacy camera keys read');}}});
 assert.equal(ports.viewMode(),'stored-camera');
});

test('confirmed final presentation is observed before its notification resumes playback',async()=>{
  const f=fixture();await f.open();const files=new Map();
  f.root.state.playing=false;f.root.__TAURI__.path={appDataDir:async()=>'/isolated'};
  f.root.__TAURI__.fs={mkdir:async()=>{},writeTextFile:async(path,bytes)=>files.set(path,bytes),
    rename:async(from,to)=>files.set(to,files.get(from))};
  f.root.NemoNativeProjectSettingsController.presented=()=>{
    Ports.settingsDiagnostic(f.root,'resume',{kind:'setFps'});f.root.state.playing=true;
  };
  Ports.createPorts(f.root).presented(f.session.info());await timerTurn();await turn();
  const events=JSON.parse(files.get('/isolated/diagnostics/native-settings.json')).events.slice(-2);
  assert.deepEqual(events.map(row=>row.event),['preview-presented','resume']);
  assert.equal(events[0].contentRevision,0);assert.equal(events[0].frame,0);
  assert.equal(f.root.state.playing,true);
});

test('actual desktop split publication handles three to five to three to five rows through one subscription',async()=>{
  const base=[{layerUid:'A',name:'Hero',visible:true},{layerUid:'B',name:'Destination',visible:true},
    {layerUid:'C',name:'Locked',visible:false,locked:true}];
  const pieces=[{layerUid:'native-layer:1:0',name:'Forme 1',visible:true,locked:false,shy:true,color:'#123456',parentLayerUid:'B'},
    {layerUid:'native-layer:1:1',name:'Forme 2',visible:false,color:'#abcdef',parentLayerUid:null}];
  const split=[...pieces,{...base[0],name:'Forme 3'},...base.slice(1)];
  const f=fixture(base,true);await f.open();const table=f.root.state.layers,original=table.slice(),ports=Ports.createPorts(f.root);
  f.root.state.activeLayerIdx=0;f.root._layerSel=[0,2];f.root._layerSelAnchor=2;
  for(const [index,next] of [split,base,split].entries()){
    const begin=f.calls.length;let acknowledgments=0;
    f.ackCheck(request=>{
      acknowledgments++;assert.equal(request.event.toRevision,index+1);
      assert.deepEqual(table.map(row=>row.layerUid),next.map(row=>row.layerUid));
      assert.equal(ports.projectionReady(f.session.info()),true);
      assert.ok(f.calls.slice(begin).some(call=>call.command==='nemo_native_object_preview'));
      assert.equal(f.root.state.activeLayerIdx,index===1?0:2);
      assert.deepEqual(f.root._layerSel,index===1?[0,2]:[2,4]);assert.equal(f.root._layerSelAnchor,index===1?2:4);
      for(const [i,meta] of next.entries())for(const key of Object.keys(meta))assert.equal(table[i][key],meta[key]);
    });
    f.layers(next);await f.emit();assert.equal(acknowledgments,1);
    assert.equal(f.session.isIndeterminate(),false);assert.equal(f.session.info().contentRevision,index+1);
    assert.strictEqual(f.root.state.layers,table);assert.deepEqual(table.map(r=>r.layerUid),next.map(r=>r.layerUid));
    original.forEach(row=>assert.strictEqual(table.find(r=>r.layerUid===row.layerUid),row));
    const offset=index===1?0:2;
    assert.equal(f.root.state.activeLayerIdx,offset);assert.deepEqual(f.root._layerSel,[offset,offset+2]);assert.equal(f.root._layerSelAnchor,offset+2);
    assert.equal(ports.projectionReady(f.session.info()),true);assert.ok(f.session.readPresentedObjectSession());
    for(const row of table){assert.ok(Array.isArray(row.frames)||pieces.some(p=>p.layerUid===row.layerUid));}
    for(const [i,meta] of next.entries())for(const key of ['name','visible','locked','shy','color','parentLayerUid']){
      if(Object.hasOwn(meta,key))assert.equal(table[i][key],meta[key]);
    }
    assert.ok(f.calls.slice(begin).some(c=>c.command==='nemo_native_object_preview'));
    assert.equal(f.calls.slice(begin).filter(c=>c.command==='nemo_native_revision_sync').length,1);
  }
  assert.equal(f.transports.length,1);
});
test('actual desktop refuses noncontiguous reorder replacement and duplicate splices without partial publication',async()=>{
  const base=[{layerUid:'A'},{layerUid:'B'},{layerUid:'C'}];
  for(const next of [[{layerUid:'x'},base[0],{layerUid:'y'},...base.slice(1)],
    [{layerUid:'x'},{layerUid:'y'},base[1],base[0],base[2]],
    [{layerUid:'x'},{layerUid:'y'},base[0],base[1],{layerUid:'replacement'}],
    [{layerUid:'x'},{layerUid:'x'},...base]]){
    const f=fixture(base,true);await f.open();
    const table=f.root.state.layers,before=JSON.stringify(f.root.state),calls=f.calls.length,pin=f.session.info();
    f.layers(next);await f.emit();assert.strictEqual(f.root.state.layers,table);assert.equal(JSON.stringify(f.root.state),before);
    assert.equal(Files.projectionReadyForUI(f.root,pin),true,'last published projection pin remains intact');
    assert.equal(f.session.isIndeterminate(),true);assert.equal(f.session.readPresentedObjectSession(),null);
    assert.equal(f.calls.slice(calls).some(c=>c.command==='nemo_native_revision_sync'&&c.r.action==='acknowledge'),false);
  }
});
test('multirow file projection preserves full prior state and pin on stale foreign and malformed metadata',async()=>{
  const f=fixture([{layerUid:'A'},{layerUid:'B'},{layerUid:'C'}]);await f.open();
  const pin=f.session.info(),table=f.root.state.layers,before=JSON.stringify(f.root.state),ports=Ports.createPorts(f.root);
  const next={...pin,contentRevision:1,documentSnapshotId:'next',layers:[{layerUid:'x'},{layerUid:'y'},...pin.layers]};
  const bad=[{...next,instanceId:'foreign'},{...next,documentId:'foreign'},
    {...next,lifecycleGeneration:pin.lifecycleGeneration+1},{...next,contentRevision:0},
    {...next,contentRevision:-1},{...next,documentSnapshotId:''}];
  for(const value of bad){
    assert.equal(Files.refreshLayerTable(f.root,value),false);assert.equal(JSON.stringify(f.root.state),before);
    assert.strictEqual(f.root.state.layers,table);assert.equal(ports.projectionReady(pin),true);
  }
  for(const changed of [{name:42},{parentLayerUid:''},{visible:'yes'},{locked:null},{color:'#oops'},{extra:true}]){
    const layers=next.layers.map((row,i)=>i===4?{...row,...changed}:row);
    assert.throws(()=>Files.refreshLayerTable(f.root,{...next,layers}));
    assert.equal(JSON.stringify(f.root.state),before);assert.strictEqual(f.root.state.layers,table);
    assert.equal(ports.projectionReady(pin),true);
  }
});
