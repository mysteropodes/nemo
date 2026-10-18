'use strict';
// A2 desktop-factory fixture adapted to actual frozen F06; native and DOM boundaries are fake.
const test=require('node:test'),assert=require('node:assert/strict');
const Project=require('../src/js/adapters/native-object-project.js');
const Ports=require('../src/js/adapters/native-object-project-desktop.js');
const Underlay=require('../src/js/adapters/native-viewport-underlay.js');
const Files=require('../src/js/adapters/native-object-project-file.js');
const Application=require('../src/js/adapters/native-application.js');
const turn=()=>new Promise(setImmediate);
function gate(){let resolve;const promise=new Promise(r=>resolve=r);return {promise,resolve};}
function fixture(){
  let installed=false,revision=0,generation=0,documentId=null,held=null,foreign=false,projectionRefused=false,rawAck=null,previewHold=null,replyMutation=null;
  const log=[],calls=[],classes=new Set(),styles=new Map(),transports=[];
  let rect={left:20.5,top:30.25,width:320.5,height:180.25};
  const document={documentElement:{clientWidth:1200,clientHeight:900,
    classList:{contains:k=>classes.has(k),add:k=>classes.add(k),remove:k=>classes.delete(k)},
    style:{getPropertyValue:k=>styles.get(k)||'',getPropertyPriority:()=>'',setProperty:(k,v)=>styles.set(k,v),removeProperty:k=>styles.delete(k)}},
    getElementById:id=>{assert.equal(id,'drawing-canvas');return {getBoundingClientRect:()=>rect};}};
  const info=()=>({apiVersion:2,instanceId:'instance',documentId,contentRevision:revision,documentSnapshotId:'snapshot-'+generation+'-'+revision,lifecycleGeneration:generation,
    documentDigest:'a'.repeat(64),byteCount:100,canvasW:320,canvasH:180,fps:24,totalFrames:4,layers:[{layerUid:'drawing',visible:true}],reference:null});
  const root={document,stageLayer:{visible:true},paper:{view:{update(){}}},state:{currentFrame:0},devicePixelRatio:2,NemoNativeViewportUnderlay:Underlay,
    NemoNativeObjectProjectFile:{...Files,refreshLayerTable:(r,i)=>{log.push('project');return !projectionRefused&&Files.refreshLayerTable(r,i);}},
    NemoNativeApplicationAdapter:Application,NemoNativeObjectUI:{refreshTiming:r=>{assert.equal(r,root);log.push('timing');},refreshPolicy:r=>{assert.equal(r,root);log.push('policy');}},
    NemoNativeProjectSettingsController:{willProject:i=>{assert.equal(i.documentId,documentId);log.push('willProject');},presented:()=>log.push('presented')},
    SMReference:{refreshNative:()=>log.push('reference')},SMProject:{refreshActiveTabDirtyDot:()=>log.push('dirty')},
    updateUI:()=>log.push('update'),showToast:text=>log.push('toast:'+text)};
  async function invoke(command,args){
    const r=args&&(args.request||JSON.parse(args.requestJson));calls.push({command,r});
    if(command==='nemo_native_object_client_status')return foreign?{...info(),available:true,instanceId:'foreign'}:{...info(),available:installed,instanceId:'instance'};
    if(command==='nemo_native_object_bootstrap'){installed=true;generation++;documentId='document-'+generation;revision=0;return {...info(),requestId:r.requestId,resourceCount:0,viewportAvailable:false};}
    if(command==='nemo_native_object_reference'){assert.equal(r.action.kind,'inspect');return {...info(),requestId:r.requestId};}
    if(command==='nemo_native_object_preview'){if(previewHold)await previewHold.promise;return {status:'presented',workId:'work-'+calls.length,viewGeneration:1};}
    if(command==='nemo_native_revision_sync'){assert.equal(r.action,'acknowledge');log.push('native-ack');if(rawAck)await rawAck.promise;return null;}
    if(command==='nemo_native_dispatch'){
      log.push(r.operation);const p=r.payload;
      let result;
      const pin=info();
      if(r.operation==='query.document.object.property.metadata'){
        if(held)await held.promise;
        result={instanceId:'instance',documentId:pin.documentId,atRevision:pin.contentRevision,
          documentSnapshotId:pin.documentSnapshotId,kind:'targets',target:null,
          items:[{kind:'layer',target:{contextId:'scene-root',layerUid:'drawing'}}],nextOffset:null,
          unavailableCategories:['conditionalMotionRows','pathVerticesTrimStrokeBrushMesh','referenceOnlyElements',
            'expressionDiagnostics','layerEffectMatteReveal','propertyWrites'].map(category=>({category,state:'unavailable',reason:'not-admitted'}))};
      }else if(r.operation==='command.document.object.layer.policy.edit'){
        assert.equal(r.expectedRevision,revision);revision++;result={applied:true,historyEntriesAdded:1};
      }else throw Error('unexpected fake public operation');
      const reply={apiVersion:2,requestId:r.requestId,instanceId:'instance',documentId:pin.documentId,contentRevision:r.operation.startsWith('query.')?pin.contentRevision:revision,ok:true,result};
      if(replyMutation)replyMutation(reply);return reply;
    }
    throw Error('Unexpected native fake command '+command);
  }
  root.__TAURI__={core:{invoke},event:{listen:async()=>()=>{}}};
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
  const session=Project.desktop(root);
  const open=()=>session.openPath('/fake.json',{reveal:async i=>{Files.projectView(root,i);root.updateUI();}});
  return {root,session,open,log,calls,classes,styles,transports,info,
    hold:v=>held=v,foreign:v=>foreign=v,rect:v=>rect=v,refuseProjection:v=>projectionRefused=v,ackGate:v=>rawAck=v,previewGate:v=>previewHold=v,mutateReply:v=>replyMutation=v,
    async emit(){const t=transports.at(-1),from=revision;revision++;const event={...info(),requestId:'event-'+revision,fromRevision:from,toRevision:revision};return t.synchronize(event,t.binding);}};
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
  const before=f.log.length;ports.presented(f.session.info());assert.deepEqual(f.log.slice(before),['presented','policy']);
});

test('SDK-valid advancing partial-page cursor is preserved without a local completeness guess',async()=>{
  const f=fixture();await f.open();f.mutateReply(r=>r.result.nextOffset=1);
  const result=await f.session.readPropertyMetadata(selector(f));assert.equal(result.nextOffset,1);assert.equal(result.items.length,1);
});
