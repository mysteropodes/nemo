'use strict';
const test=require('node:test'),assert=require('node:assert/strict');
const Project=require('../src/js/adapters/native-object-project.js');
const Files=require('../src/js/adapters/native-object-project-file.js');
const Transport=require('../src/js/adapters/application-mcp.js');
const turn=()=>new Promise(setImmediate);
function deferred(){let resolve;const promise=new Promise(r=>{resolve=r;});return {promise,resolve};}
function scenario(){
  let active=false,revision=0,generation=0,visible,previewGate=null,replyGate=null,ackGate=null,failure=null;
  const log=[],listeners=new Map(),oldListeners=[],commands=[];
  const info=()=>({apiVersion:2,instanceId:'instance',documentId:'document-'+generation,lifecycleGeneration:generation,
    contentRevision:revision,documentSnapshotId:'snapshot-'+generation+'-'+revision,documentDigest:String(revision%10).repeat(64),
    byteCount:1000,canvasW:320,canvasH:180,fps:24,totalFrames:21,undoDepth:revision,redoDepth:0,reference:null,
    layers:[{layerUid:'legacy layer:alpha',name:'Alpha',...(visible===undefined?{}:{visible})},{layerUid:'sibling',name:'Sibling'}]});
  async function invoke(command,args){
    const r=args&&(args.request||JSON.parse(args.requestJson));
    if(command==='nemo_native_status'||command==='nemo_native_object_client_status')return {...info(),available:active};
    if(command==='nemo_native_object_bootstrap'){
      active=true;generation++;revision=0;visible=undefined;
      return {...info(),requestId:r.requestId,resourceCount:0,viewportAvailable:false};
    }
    if(command==='nemo_native_revision_sync'){
      log.push(r.action);
      if(r.action==='binding')return {...info(),subscriptionId:'subscription-'+generation};
      if(r.action==='subscribe')return r.binding;
      if(r.action==='acknowledge'){
        assert.equal(r.event.toRevision,revision);assert.equal(root.state.layers[0].visible,visible!==false);
        if(ackGate){const gate=ackGate;ackGate=null;await gate.promise;}
        return null;
      }
      if(r.action==='disconnect')return null;
    }
    if(command==='nemo_native_object_reference'&&r.action.kind==='inspect'){
      log.push('inspect-'+revision);
      const next={...info(),requestId:r.requestId};if(failure==='metadata')next.layers[0].visible=null;
      return next;
    }
    if(command==='nemo_native_object_preview'){
      if(previewGate){const gate=previewGate;previewGate=null;await gate.promise;}
      log.push('present-'+r.contentRevision);
      return {status:failure==='preview'?'refused':'presented',workId:'work-'+r.contentRevision,viewGeneration:1};
    }
    assert.equal(command,'nemo_native_object_dispatch');assert.equal(r.expectedRevision,revision);
    assert.equal(r.documentId,info().documentId);commands.push(r);
    const applied=(visible!==false)!==r.payload.visible;
    if(applied){visible=r.payload.visible;revision++;}
    const response={apiVersion:2,requestId:r.requestId,instanceId:r.instanceId,documentId:r.documentId,
      contentRevision:revision,ok:true,result:{applied,historyEntriesAdded:applied?1:0}};
    if(replyGate){const gate=replyGate;replyGate=null;await gate.promise;}
    return response;
  }
  const root={state:{},selectedPaths:[],userLayers:[],NemoNativeObjectProjectFile:Files,
    NemoApplicationMcpTransport:Transport,__TAURI__:{core:{invoke},event:{listen:async(name,callback)=>{
      listeners.set(name,callback);oldListeners.push(callback);return ()=>{if(listeners.get(name)===callback)listeners.delete(name);};
    }}},document:{getElementById:()=>({getBoundingClientRect:()=>({x:0,y:0,width:320,height:180})})},
    updateUI:()=>log.push('publish-'+revision)};
  const session=Project.desktop(root);
  async function open(){return session.openPath('/native.json',{commit:meta=>Files.projectView(root,meta)});}
  function event(from,to){return {instanceId:'instance',documentId:info().documentId,lifecycleGeneration:generation,
    requestId:'external-'+to,fromRevision:from,toRevision:to};}
  async function external(value){const from=revision;visible=value;revision++;return listeners.get('nemo-native-revision')({payload:event(from,revision)});}
  return {root,session,log,commands,open,external,listeners,oldListeners,
    holdPreview:gate=>{previewGate=gate;},holdReply:gate=>{replyGate=gate;},holdAck:gate=>{ackGate=gate;},fail:value=>{failure=value;},
    current:info};
}

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

test('external event waits for the actual object UI reply and tolerates the still-pending metadata refresh',async()=>{
  const h=scenario();await h.open();const gate=deferred();h.holdReply(gate);
  const ui=h.session.setLayerVisibility('legacy layer:alpha',false);
  for(let i=0;i<10&&!h.commands.length;i++)await turn();
  const external=h.external(true);await turn();assert.equal(h.log.includes('acknowledge'),false);
  gate.resolve();await ui;await external;
  assert.equal(h.commands.length,1);assert.equal(h.log.includes('acknowledge'),true);
  assert.equal(h.session.info().contentRevision,2);assert.equal(h.root.state.layers[0].visible,true);
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
