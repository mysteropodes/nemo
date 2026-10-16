'use strict';
const test=require('node:test'),assert=require('node:assert/strict');
const Project=require('../src/js/adapters/native-object-project.js');
const Files=require('../src/js/adapters/native-object-project-file.js');
const Transport=require('../src/js/adapters/application-mcp.js');
const Application=require('../src/js/adapters/native-application.js');
const Surface=require('../src/js/adapters/native-opacity-legacy-surface.js');
const turn=()=>new Promise(setImmediate);
function deferred(){let resolve;const promise=new Promise(r=>{resolve=r;});return {promise,resolve};}
function scenario(openFault){
  let active=false,revision=0,generation=0,visible,previewGate=null,replyGate=null,ackGate=null,failure=null;
  let nativePending=null,registered=null;
  const log=[],listeners=new Map(),oldListeners=[],commands=[],routes=[],consumers=[],diagnostics=[],openCalls=[];
  const openError={code:'unavailable',details:{disposition:'notDispatched'},message:'PRIVATE NATIVE ERROR'};
  let openStatus=0,openMetadata=0,openPreview=0;
  const info=()=>({apiVersion:2,instanceId:'instance',documentId:'document-'+generation,lifecycleGeneration:generation,
    contentRevision:revision,documentSnapshotId:'snapshot-'+generation+'-'+revision,documentDigest:String(revision%10).repeat(64),
    byteCount:1000,canvasW:320,canvasH:180,fps:24,totalFrames:21,undoDepth:revision,redoDepth:0,reference:null,
    layers:[{layerUid:'legacy layer:alpha',name:'Alpha',...(visible===undefined?{}:{visible})},{layerUid:'sibling',name:'Sibling'}]});
  async function invoke(command,args){
    const r=args&&(args.request||JSON.parse(args.requestJson));
    if(openFault)openCalls.push(command);
    if(command==='nemo_native_status'||command==='nemo_native_object_client_status'){
      if(openFault&&active){openStatus++;if(openStatus===({'installed-status':1,'first-owner':2,'second-owner':3})[openFault.stage])return {...info(),available:true,instanceId:'wrong'};}
      return {...info(),available:active};
    }
    if(command==='nemo_native_object_bootstrap'){
      active=true;generation++;revision=0;visible=undefined;
      return {...info(),requestId:openFault?.stage==='receipt'?'wrong':r.requestId,resourceCount:0,viewportAvailable:false};
    }
    if(command==='nemo_native_revision_sync'){
      log.push(r.action);
      if(r.action==='binding'){
        if(openFault?.stage==='subscriber'&&openFault.invoke&&!openFault.register)throw openError;
        return {...info(),subscriptionId:openFault?.stage==='subscriber'&&!openFault.register?'INVALID ID':'subscription-'+generation};
      }
      if(r.action==='subscribe'){if(openFault?.register)throw openError;registered=r.binding;return r.binding;}
      if(r.action==='acknowledge'){
        assert.equal(r.event.toRevision,revision);assert.equal(root.state.layers[0].visible,visible!==false);
        if(ackGate){const gate=ackGate;ackGate=null;await gate.promise;}
        nativePending=null;
        return null;
      }
      if(r.action==='disconnect'){if(openFault?.register)throw {code:'cancelled',message:'PRIVATE CLEANUP'};return null;}
    }
    if(command==='nemo_native_object_reference'&&r.action.kind==='inspect'){
      if(openFault&&++openMetadata===1&&openFault.stage==='metadata')return {...info(),requestId:'wrong'};
      log.push('inspect-'+revision);
      const next={...info(),requestId:r.requestId};if(failure==='metadata')next.layers[0].visible=null;
      return next;
    }
    if(command==='nemo_native_object_preview'){
      if(openFault&&++openPreview===({'first-preview':1,'second-preview':2})[openFault.stage]){
        if(openFault.invoke)throw openError;
        return {status:'refused'};
      }
      if(nativePending){
        if(!r.revisionSync||r.revisionSync.subscriptionId!==registered.subscriptionId||
          JSON.stringify(r.revisionSync.event)!==JSON.stringify(nativePending)||r.contentRevision!==nativePending.toRevision)
          throw Error('Native revision synchronization is pending; untagged preview refused');
      }
      if(previewGate){const gate=previewGate;previewGate=null;await gate.promise;}
      log.push('present-'+r.contentRevision);
      return {status:failure==='preview'?'refused':'presented',workId:'work-'+r.contentRevision,viewGeneration:1};
    }
    assert.equal(command,'nemo_native_dispatch');routes.push(command);assert.equal(r.expectedRevision,revision);
    assert.equal(r.documentId,info().documentId);commands.push(r);
    const applied=(visible!==false)!==r.payload.visible;
    if(applied){visible=r.payload.visible;revision++;}
    const response={apiVersion:2,requestId:r.requestId,instanceId:r.instanceId,documentId:r.documentId,
      contentRevision:revision,ok:true,result:{applied,historyEntriesAdded:applied?1:0}};
    if(replyGate){const gate=replyGate;replyGate=null;await gate.promise;}
    if(failure==='malformed-reply')response.result.historyEntriesAdded=2;
    return response;
  }
  const root={state:{},selectedPaths:[],userLayers:[],NemoNativeObjectProjectFile:Files,
    NemoNativeApplicationAdapter:Application,NemoApplicationMcpTransport:{createNativeTauriTransport:invoke=>{
      const transport=Transport.createNativeTauriTransport(invoke);
      return {...transport,subscribeRevisions:(listen,synchronize,disconnected)=>{
        consumers.push(synchronize);return transport.subscribeRevisions(listen,synchronize,disconnected);
      }};
    }},__TAURI__:{core:{invoke},event:{listen:async(name,callback)=>{
      listeners.set(name,callback);oldListeners.push(callback);return ()=>{if(listeners.get(name)===callback)listeners.delete(name);};
    }}},document:{getElementById:()=>({getBoundingClientRect:()=>({x:0,y:0,left:0,top:0,width:320,height:180})})},
    updateUI:()=>{if(openFault?.stage==='publication')throw openError;log.push('publish-'+revision);}};
  if(openFault)root.__NEMO_P38_FIXTURE_OPEN_DIAGNOSTIC__=(event,code)=>diagnostics.push([event,code]);
  require('./helpers/native-viewport-underlay.cjs').install(root);
  const session=Project.desktop(root);
  async function open(){return session.openPath('/native.json',{
    ...(openFault?{reveal:()=>{if(openFault.stage==='reveal')throw openError;}}:{}),
    commit:meta=>{if(openFault?.stage==='ui-commit')throw openError;Files.projectView(root,meta);}});}
  function event(from,to){return {instanceId:'instance',documentId:info().documentId,lifecycleGeneration:generation,
    requestId:'external-'+to,fromRevision:from,toRevision:to};}
  async function external(value){const from=revision;visible=value;revision++;nativePending=event(from,revision);
    return listeners.get('nemo-native-revision')({payload:nativePending});}
  return {root,session,log,commands,routes,open,external,listeners,oldListeners,consumers,diagnostics,openCalls,openError,
    holdPreview:gate=>{previewGate=gate;},holdReply:gate=>{replyGate=gate;},holdAck:gate=>{ackGate=gate;},fail:value=>{failure=value;},
    current:info};
}

test('real desktop Open distinguishes all first post-bootstrap boundaries and native invoke rejection',async()=>{
  const stages=['receipt','installed-status','metadata','first-preview','first-owner','reveal','second-preview','second-owner','ui-commit','publication','subscriber'];
  for(const stage of stages){
    const h=scenario({stage});await assert.rejects(h.open());
    const callbacks=['reveal','ui-commit','publication'].includes(stage);
    const diagnosticStage=['first-preview','second-preview'].includes(stage)?stage+'-status':stage;
    assert.deepEqual(h.diagnostics,[['bootstrap-dispatched',undefined],['bootstrap-resolved',undefined],
      ['post-bootstrap-failed',`post-bootstrap-${diagnosticStage}:local-refusal:${callbacks?'unavailable:notDispatched':'unknown:other'}`]]);
    assert.equal(h.session.isActive(),false);assert.equal(h.session.readPresentedObjectSession(),null);
    assert.equal(h.openCalls.filter(c=>c==='nemo_native_object_bootstrap').length,1);
    assert.doesNotMatch(JSON.stringify(h.diagnostics),/PRIVATE|INVALID ID/);
  }
  for(const fault of [{stage:'first-preview',invoke:true},{stage:'subscriber',invoke:true},{stage:'subscriber',invoke:true,register:true}]){
    const {stage}=fault,h=scenario(fault);await assert.rejects(h.open(),error=>error===h.openError);
    assert.deepEqual(h.diagnostics.at(-1),['post-bootstrap-failed',`post-bootstrap-${stage}:invoke-rejected:unavailable:notDispatched`]);
  }
  const success=scenario({stage:'none'});await success.open();
  assert.deepEqual(success.diagnostics.map(value=>value[0]),['bootstrap-dispatched','bootstrap-resolved']);
  assert.ok(success.session.isActive());assert.ok(success.session.readPresentedObjectSession());
});

test('common subscriber ACK follows exact native inspection, row publication and confirmed preview',async()=>{
  const h=scenario();await h.open();h.root.state.currentFrame=7;h.root.state.activeLayerIdx=1;
  h.root._layerSel=[1];h.root.selectedPaths=['selection'];
  const rows=h.root.state.layers,gate=deferred();h.holdPreview(gate);
  const work=h.external(false);await turn();
  assert.equal(h.log.includes('acknowledge'),false);assert.equal(h.session.readPresentedObjectSession(),null);
  assert.equal(typeof h.session.toggleLayerVisibility,'undefined');
  gate.resolve();await work;
  assert.ok(h.log.indexOf('publish-1')<h.log.indexOf('present-1'));
  assert.ok(h.log.indexOf('present-1')<h.log.indexOf('acknowledge'));
  assert.equal(h.root.state.layers,rows);assert.equal(rows[0].visible,false);
  assert.equal(h.root.state.currentFrame,7);assert.equal(h.root.state.activeLayerIdx,1);
  assert.deepEqual(h.root._layerSel,[1]);assert.deepEqual(h.root.selectedPaths,['selection']);
  assert.equal(h.session.readPresentedObjectSession().contentRevision,1);
});

test('metadata, projection or preview refusal cannot ACK or fall through to legacy',async()=>{
  for(const failure of ['metadata','projection','preview']){
    const h=scenario();await h.open();
    if(failure==='projection')h.root.state.layers[0].layerUid='foreign';else h.fail(failure);
    await h.external(false);
    assert.equal(h.log.includes('acknowledge'),false);assert.equal(h.log.includes('disconnect'),true);
    assert.equal(h.session.blocksLegacy(),true);assert.equal(h.session.readPresentedObjectSession(),null);
    assert.equal(typeof h.session.toggleLayerVisibility,'undefined');assert.equal(h.commands.length,0);
  }
});

test('two queued ordinary toggle intents derive successive native booleans rather than stale UI values',async()=>{
  const h=scenario();await h.open();
  const toggle=h.session.toggleLayerVisibility;
  const one=toggle('legacy layer:alpha'),two=toggle('legacy layer:alpha');await Promise.all([one,two]);
  assert.deepEqual(h.commands.map(r=>[r.expectedRevision,r.payload.visible]),[[0,false],[1,true]]);
  assert.equal(h.session.info().contentRevision,2);assert.equal(h.root.state.layers[0].visible,true);
  await h.external(false);assert.equal(h.log.includes('acknowledge'),true);
  assert.equal(h.root.state.layers[0].visible,false);
});

test('ordinary eye reaches the public SDK and common native endpoint with no optimistic or legacy write',async()=>{
  const h=scenario();await h.open();let legacy=0;
  h.root.SM={toggleLayerVis(){legacy++;}};
  const dispose=Surface.wrapObjectVisibility(h.root,()=>h.session,()=>true);
  const gate=deferred();h.holdReply(gate);
  const first=h.root.SM.toggleLayerVis(0),second=h.root.SM.toggleLayerVis(0);
  for(let i=0;i<10&&!h.commands.length;i++)await turn();
  assert.equal(h.root.state.layers[0].visible,true);assert.equal(h.commands.length,1);
  assert.equal(h.commands[0].payload.layerUid,'legacy layer:alpha');
  gate.resolve();await Promise.all([first,second]);
  assert.deepEqual(h.routes,['nemo_native_dispatch','nemo_native_dispatch']);
  assert.deepEqual(h.commands.map(r=>[r.expectedRevision,r.payload.visible]),[[0,false],[1,true]]);
  assert.equal(legacy,0);assert.equal(h.root.state.layers[0].visible,true);
  assert.equal(h.session.readPresentedObjectSession().contentRevision,2);dispose();
});

test('desktop public SDK absence refuses the eye without using the private or legacy writer',async()=>{
  const h=scenario();await h.open();let legacy=0;
  h.root.SM={toggleLayerVis(){legacy++;}};delete h.root.NemoNativeApplicationAdapter;
  const dispose=Surface.wrapObjectVisibility(h.root,()=>h.session,()=>true);
  await assert.rejects(h.root.SM.toggleLayerVis(0),/public application client is unavailable/);
  assert.deepEqual(h.routes,[]);assert.equal(h.commands.length,0);assert.equal(legacy,0);
  assert.equal(h.root.state.layers[0].visible,true);assert.equal(h.session.info().contentRevision,0);dispose();
});

test('public visibility no-op remains at the current revision before the next changed eye intent',async()=>{
  const h=scenario();await h.open();
  const same=await h.session.setLayerVisibility('legacy layer:alpha',true);
  assert.deepEqual(same.result,{applied:false,historyEntriesAdded:0});
  assert.equal(h.session.readPresentedObjectSession().contentRevision,0);
  const changed=await h.session.toggleLayerVisibility('legacy layer:alpha');
  assert.deepEqual(changed.result,{applied:true,historyEntriesAdded:1});
  assert.deepEqual(h.commands.map(r=>[r.expectedRevision,r.payload.visible]),[[0,true],[0,false]]);
  assert.deepEqual(h.routes,['nemo_native_dispatch','nemo_native_dispatch']);
  assert.equal(h.root.state.layers[0].visible,false);assert.equal(h.session.info().contentRevision,1);
});

test('ordinary public eye rejects malformed native disposition through the real SDK without retry',async()=>{
  const h=scenario();await h.open();let legacy=0;
  h.root.SM={toggleLayerVis(){legacy++;}};h.fail('malformed-reply');
  const dispose=Surface.wrapObjectVisibility(h.root,()=>h.session,()=>true);
  await assert.rejects(h.root.SM.toggleLayerVis(0),/invalid visibility disposition/);
  assert.deepEqual(h.routes,['nemo_native_dispatch']);assert.equal(h.commands.length,1);assert.equal(legacy,0);
  // The committed owner is reconciled, but a rejected SDK response is never delivered success.
  assert.equal(h.session.info().contentRevision,1);assert.equal(h.root.state.layers[0].visible,false);dispose();
});

test('external event waits for the actual object UI reply and tolerates the still-pending metadata refresh',async()=>{
  const h=scenario();await h.open();const gate=deferred();h.holdReply(gate);
  const ui=h.session.setLayerVisibility('legacy layer:alpha',false);
  const uiResult=ui.then(()=>{throw Error('A pending untagged UI preview cannot succeed');},error=>error);
  for(let i=0;i<10&&!h.commands.length;i++)await turn();
  const external=h.external(true);await turn();assert.equal(h.log.includes('acknowledge'),false);
  gate.resolve();assert.match((await uiResult).message,/pending/);await external;
  assert.equal(h.commands.length,1);assert.equal(h.log.includes('acknowledge'),true);
  assert.equal(h.session.info().contentRevision,2);assert.equal(h.root.state.layers[0].visible,true);
});

test('transport retirement or malformed event immediately fences stale session presentation',async()=>{
  for(const kind of ['retired','malformed']){
    const h=scenario();await h.open();
    if(kind==='retired')await h.listeners.get('nemo-native-revision-disconnected')({payload:{subscriptionId:'subscription-1'}});
    else await h.listeners.get('nemo-native-revision')({payload:{unexpected:true}});
    assert.equal(h.session.isActive(),false);assert.equal(h.session.blocksLegacy(),true);
    assert.equal(h.session.readPresentedObjectSession(),null);assert.equal(typeof h.session.toggleLayerVisibility,'undefined');
    await assert.rejects(h.session.presentPreview(0),/in progress/);
    await assert.rejects(h.session.setLayerVisibility('legacy layer:alpha',false),/in progress/);
    assert.equal(h.commands.length,0);
  }
});

test('replacement disconnects the old binding and reconnects only the adopted new projection',async()=>{
  const h=scenario();await h.open();const old=h.listeners.get('nemo-native-revision');
  await h.open();assert.equal(h.session.info().documentId,'document-2');
  await old({payload:{instanceId:'instance',documentId:'document-1',lifecycleGeneration:1,
    requestId:'late',fromRevision:0,toRevision:1}});
  assert.equal(h.session.info().contentRevision,0);assert.equal(h.log.includes('acknowledge'),false);
  await h.external(false);assert.equal(h.session.info().contentRevision,1);
  assert.equal(h.root.state.layers[0].visible,false);assert.equal(h.log.includes('acknowledge'),true);
});

test('an old consumer queued behind Open cannot fence the newly adopted owner',async()=>{
  const h=scenario();await h.open();const old=h.consumers[0];
  const replacement=h.open();
  const stale=old({instanceId:'instance',documentId:'document-1',lifecycleGeneration:1,
    requestId:'queued-old',fromRevision:0,toRevision:1},
    {instanceId:'instance',documentId:'document-1',lifecycleGeneration:1,subscriptionId:'subscription-1'});
  const rejected=assert.rejects(stale,/replaced/);
  await replacement;await rejected;
  assert.equal(h.session.isActive(),true);assert.equal(h.session.info().documentId,'document-2');
  assert.notEqual(h.session.readPresentedObjectSession(),null);assert.equal(typeof h.session.toggleLayerVisibility,'function');
  await h.external(false);assert.equal(h.log.includes('acknowledge'),true);
});

test('unpublished projection keeps the ordinary toggle ABI absent',async()=>{
  const h=scenario();await h.session.openPath('/native.json');
  assert.equal(typeof h.session.toggleLayerVisibility,'undefined');assert.equal(h.log.includes('subscribe'),false);
});

test('new object writes remain fenced until the native ACK response completes',async()=>{
  const h=scenario();await h.open();const gate=deferred();h.holdAck(gate);
  const external=h.external(false);await turn();
  assert.equal(h.log.includes('acknowledge'),true);assert.equal(typeof h.session.toggleLayerVisibility,'undefined');
  await assert.rejects(h.session.setLayerVisibility('legacy layer:alpha',true),/synchronization is pending/);
  assert.equal(h.commands.length,0);gate.resolve();await external;
  assert.equal(typeof h.session.toggleLayerVisibility,'function');
});

test('cancelled Open retires and restores the old subscription without changing its rows or frame',async()=>{
  const h=scenario();await h.open();const rows=h.root.state.layers;h.root.state.currentFrame=7;
  assert.equal(await h.session.openPath('/cancelled.json',{confirm:()=>false}),false);
  assert.equal(h.root.state.layers,rows);assert.equal(h.root.state.currentFrame,7);
  assert.equal(h.session.info().documentId,'document-1');
  await h.external(false);assert.equal(h.log.includes('acknowledge'),true);
});

test('bounded object tracking cannot invoke an arbitrary endpoint or inspect route',async()=>{
  const calls=[],transport=Transport.createNativeTauriTransport(async(command)=>{
    calls.push(command);return {apiVersion:2,available:true,instanceId:'instance',documentId:'document',contentRevision:0};
  });
  await transport.connect();const before=calls.length;
  await assert.rejects(transport.dispatchObject('nemo_native_bootstrap',{}),/Unsupported/);
  await assert.rejects(transport.dispatchObject('nemo_native_object_reference',{action:{kind:'inspect'}}),/Unsupported/);
  assert.equal(calls.length,before);
});
