'use strict';
const test=require('node:test'),assert=require('node:assert/strict');
const Project=require('../src/js/adapters/native-object-project.js');
const Desktop=require('../src/js/adapters/native-object-project-desktop.js');
const turn=()=>new Promise(setImmediate);
function deferred(){let resolve;const promise=new Promise(r=>resolve=r);return {promise,resolve};}
function harness(options={}){
  let installed=false,revision=0,documentId='document',generation=1,source='snapshot',hint,closed,removes=0,checks=0,closes=0;
  const calls=[],effects=[],receipts=options.receipts||[{status:'deferred-occluded',workId:'work-1',viewGeneration:1},
    {status:'presented',workId:'work-1',viewGeneration:1},{status:'presented',workId:'work-2',viewGeneration:2}];
  const info=()=>({apiVersion:2,instanceId:'instance',documentId,lifecycleGeneration:generation,contentRevision:revision,
    documentSnapshotId:source,canvasW:320,canvasH:180,fps:24,totalFrames:21,documentDigest:'a'.repeat(64),byteCount:100,
    layers:[{layerUid:'drawing',visible:true}],reference:null,undoDepth:0,redoDepth:0});
  const invoke=async(command,args)=>{
    calls.push(command);const request=args&&JSON.parse(args.requestJson);
    if(command==='nemo_native_object_client_status'){checks++;if(options.ownerGate&&checks===3)await options.ownerGate.promise;if(options.reconcileGate&&checks===2)await options.reconcileGate.promise;return {...info(),available:installed};}
    if(command==='nemo_native_object_bootstrap'){if(options.bootstrapError)throw options.bootstrapError;installed=true;return {...info(),requestId:request.requestId,resourceCount:0,viewportAvailable:false};}
    if(command==='nemo_native_object_reference')return {...info(),requestId:request.requestId};
    if(command==='nemo_native_object_preview'){
      if(options.duringPreview)options.duringPreview(hint);
      if(options.previewError)throw options.previewError;
      if(options.previewGate)await options.previewGate.promise;
      return receipts.shift()||{status:'presented',workId:'later',viewGeneration:3};
    }
    throw Error('Unexpected native write '+command);
  };
  const ports={invoke,mapping:()=>({cssBounds:{width:320,height:180},physicalExtent:{width:320,height:180}}),
    subscribeWindowOcclusion:async(onHint,onClose)=>{effects.push('listen');hint=onHint;closed=onClose;
      if(options.registrationGate)await options.registrationGate.promise;
      if(options.initialHint)onHint(options.initialHint);
      let removed=false;const detach=()=>{if(!removed){removed=true;removes++;effects.push('remove');if(options.badDetach)throw Error('unsubscribe failed');}};
      return {detach,close:()=>{closes++;detach();if(options.badClose)throw Error('close cleanup failed');}};}};
  if(options.openDiagnostic)ports.openDiagnostic=options.openDiagnostic;
  if(options.revisionGate)Object.assign(ports,{projectionReady:()=>true,publishRevision:()=>true,unsubscribeRevisions:async()=>{},
    subscribeRevisions:async(a,b,c,isCurrent)=>{await options.revisionGate.promise;if(isCurrent())options.onSubscriptionStart();return {...info(),subscriptionId:'revision-sub'};}});
  const session=Project.create(ports);
  const transition={isCurrent:()=>options.current?options.current():true,reveal:()=>{effects.push('reveal');},commit:()=>{effects.push('commit');}};
  return {session,calls,effects,transition,closes:()=>closes,open:()=>session.openPath('/actual-file.json',transition),hint:v=>hint(v),close:()=>closed(Error('window closed')),
    owner(kind){if(kind==='revision')revision++;if(kind==='document')documentId='foreign';if(kind==='generation')generation++;if(kind==='snapshot')source='foreign';},
    removes:()=>removes,previews:()=>calls.filter(x=>x==='nemo_native_object_preview').length,checks:()=>checks};
}
test('hidden Open resumes retained work only on visible exposure then requires both presented receipts',async()=>{
  const h=harness(),pending=h.open();await turn();assert.equal(h.previews(),1);assert.equal(h.effects.includes('reveal'),false);
  h.hint({visible:false});h.hint({visible:true,documentId:'foreign'});await turn();assert.equal(h.previews(),1);
  h.hint({visible:true});await pending;assert.equal(h.previews(),3);assert.equal(h.removes(),1);
  assert.deepEqual(h.effects,['listen','reveal','remove','commit']);assert.equal(h.session.isActive(),true);
  assert.ok(h.session.readPresentedObjectSession());assert.equal(h.calls.filter(x=>x==='nemo_native_object_bootstrap').length,1);
  h.hint({visible:false});h.hint({visible:true});await turn();assert.equal(h.previews(),3);
});
test('exposure during deferred receipt cannot be lost; repeated visible hints do not spin',async()=>{
  let emitted=false;const h=harness({duringPreview:hint=>{if(!emitted){emitted=true;hint({visible:true});}}});await h.open();assert.equal(h.previews(),3);
  const stuck=harness({initialHint:{visible:true}}),pending=stuck.open();await turn();stuck.hint({visible:true});await turn();assert.equal(stuck.previews(),1);
  stuck.session.cancelOpen();await assert.rejects(pending,/cancel/);assert.equal(stuck.removes(),1);
});
for(const field of ['revision','document','generation','snapshot'])test('foreign or stale '+field+' cannot re-present or publish after exposure',async()=>{
  const h=harness(),pending=h.open();await turn();h.owner(field);h.hint({visible:true});await assert.rejects(pending);
  assert.equal(h.previews(),1);assert.equal(h.effects.includes('reveal'),false);assert.equal(h.session.isActive(),false);assert.equal(h.removes(),1);
});
for(const mode of ['cancel','close','never-visible'])test(mode+' settles pending Open and ignores all late events without timer retries',async()=>{
  const h=harness(),pending=h.open();await turn();assert.equal(h.previews(),1);
  if(mode==='close')h.close();else h.session.cancelOpen();
  await assert.rejects(pending);h.hint({visible:false});h.hint({visible:true});await turn();
  assert.equal(h.previews(),1);assert.equal(h.removes(),1);assert.equal(h.session.isIndeterminate(),true);assert.equal(h.session.readPresentedObjectSession(),null);
  assert.equal(h.effects.includes('commit'),false);assert.equal(h.calls.filter(x=>x==='nemo_native_object_bootstrap').length,1);
});
test('cancel before subscription resolves removes late listener without bootstrap',async()=>{
  const gate=deferred(),h=harness({registrationGate:gate}),pending=h.open();await turn();h.session.cancelOpen();await assert.rejects(pending);
  gate.resolve();await turn();assert.equal(h.removes(),1);assert.deepEqual(h.calls,[]);
});
test('cancel during native preview ignores late receipt and cannot reveal or re-present',async()=>{
  const gate=deferred(),h=harness({previewGate:gate}),pending=h.open();await turn();h.session.cancelOpen();await assert.rejects(pending);
  gate.resolve();h.hint({visible:true});await turn();assert.equal(h.previews(),1);assert.equal(h.effects.includes('reveal'),false);assert.equal(h.removes(),1);
});
for(const change of ['work','view'])test('retained deferred '+change+' identity drift is refused',async()=>{
  const next={status:'presented',workId:change==='work'?'foreign':'work-1',viewGeneration:change==='view'?2:1};
  const h=harness({receipts:[{status:'deferred-occluded',workId:'work-1',viewGeneration:1},next]}),pending=h.open();await turn();h.hint({visible:true});
  await assert.rejects(pending,/work changed/);assert.equal(h.effects.includes('reveal'),false);assert.equal(h.removes(),1);
});
test('already-visible valid presentation neither waits nor adds native work',async()=>{
  const h=harness({initialHint:{visible:true},receipts:[{status:'presented',workId:'first',viewGeneration:1},{status:'presented',workId:'second',viewGeneration:2}]});
  await h.open();assert.equal(h.previews(),2);assert.equal(h.removes(),1);assert.equal(h.session.isActive(),true);
});
for(const status of ['deferred-timeout','failed-validation','stale-discarded'])test(status+' remains a refusal rather than an exposure retry',async()=>{
  const h=harness({receipts:[{status,workId:'work-1',viewGeneration:1}]});await assert.rejects(h.open());assert.equal(h.previews(),1);assert.equal(h.removes(),1);
});
test('desktop listens to existing native event and pagehide; teardown removes both subscriptions',async()=>{
  let callback,closing,unlisten=0,removeClose=0;const root={__TAURI__:{core:{invoke:()=>{}},event:{listen:async(name,fn)=>{assert.equal(name,'nemo-native-window-occlusion');callback=fn;return ()=>unlisten++;}}},
    addEventListener:(name,fn)=>{assert.equal(name,'pagehide');closing=fn;},removeEventListener:(name,fn)=>{assert.equal(name,'pagehide');assert.equal(fn,closing);removeClose++;},
    NemoNativeViewportUnderlay:{desktop:()=>({})}};
  const hints=[],cancel=[];const remove=await Desktop.createPorts(root).subscribeWindowOcclusion(v=>hints.push(v),e=>cancel.push(e));
  callback({payload:{visible:true}});closing();assert.deepEqual(hints,[{visible:true}]);assert.equal(cancel[0].message,'Native project window closed');remove.detach();assert.equal(unlisten,1);assert.equal(removeClose,0);remove.close();assert.equal(unlisten,1);assert.equal(removeClose,1);
});

test('same-tick cancellation before queued admission performs no bootstrap or subscription',async()=>{
  const h=harness(),pending=h.open();assert.equal(h.session.cancelOpen(),true);await assert.rejects(pending,/cancel/);
  assert.deepEqual(h.calls,[]);assert.deepEqual(h.effects,[]);assert.equal(h.session.isIndeterminate(),true);
  assert.equal(h.session.cancelOpen(),false);
});
test('cancellation settles immediately while deferred owner verification is unresolved', {timeout:2000},async()=>{
  const gate=deferred(),h=harness({ownerGate:gate}),pending=h.open();await turn();assert.equal(h.checks(),3);
  h.session.cancelOpen();await assert.rejects(pending,/cancel/);assert.equal(h.removes(),1);assert.equal(h.closes(),1);
  gate.resolve();h.hint({visible:true});await turn();assert.equal(h.previews(),1);assert.equal(h.effects.includes('commit'),false);
});
for(const stage of ['first-owner','reveal','commit'])test('cancellation during pending '+stage+' cannot complete Open', {timeout:2000},async()=>{
  const gate=deferred(),h=harness({receipts:[{status:'presented',workId:'first',viewGeneration:1},{status:'presented',workId:'second',viewGeneration:2}],...(stage==='first-owner'?{ownerGate:gate}:{})});
  if(stage!=='first-owner')h.transition[stage]=()=>gate.promise;
  const pending=h.open();await turn();h.session.cancelOpen();await assert.rejects(pending,/cancel/);
  assert.equal(h.session.isIndeterminate(),true);assert.equal(h.removes(),1);assert.equal(h.closes(),1);
  gate.resolve();await turn();assert.equal(h.session.isActive(),false);assert.equal(h.session.readPresentedObjectSession(),null);
});
for(const stage of ['detach','close'])test('throwing '+stage+' fails closed while finally bookkeeping permits a real next refusal',async()=>{
  const h=harness({receipts:[{status:'presented',workId:'first',viewGeneration:1},{status:'presented',workId:'second',viewGeneration:2}],...(stage==='detach'?{badDetach:true}:{badClose:true})});
  await assert.rejects(h.open(),/cleanup|unsubscribe/);assert.equal(h.session.isIndeterminate(),true);
  assert.equal(h.session.cancelOpen(),false);assert.equal(h.removes(),1);
  if(stage==='detach')assert.equal(h.effects.includes('commit'),false);
  await assert.rejects(h.open(),/unavailable/);assert.equal(h.previews(),2);
});
test('desktop rejects an invalid raw event unsubscriber and removes its close hook',async()=>{
  let removed=0;const root={__TAURI__:{core:{invoke:()=>{}},event:{listen:async()=>undefined}},addEventListener:()=>{},removeEventListener:()=>removed++,NemoNativeViewportUnderlay:{desktop:()=>({})}};
  await assert.rejects(Desktop.createPorts(root).subscribeWindowOcclusion(()=>{},()=>{}),/unsubscribe/);assert.equal(removed,1);
});
test('window close remains cancellable after native listener detaches during pending commit', {timeout:2000},async()=>{
  const gate=deferred(),h=harness({receipts:[{status:'presented',workId:'first',viewGeneration:1},{status:'presented',workId:'second',viewGeneration:2}]});
  h.transition.commit=()=>gate.promise;const pending=h.open();await turn();assert.equal(h.removes(),1);assert.equal(h.closes(),0);
  h.close();await assert.rejects(pending,/closed/);assert.equal(h.closes(),1);assert.equal(h.session.isIndeterminate(),true);gate.resolve();
});
test('late revision transport registration cleans its own cancelled binding without disconnecting a newer transport',async()=>{
  const gate=deferred(),transports=[];let firstCurrent=true;
  const root={__TAURI__:{core:{invoke:()=>{}},event:{listen:async()=>()=>{}}},NemoNativeViewportUnderlay:{desktop:()=>({})},
    NemoApplicationMcpTransport:{createNativeTauriTransport:()=>{const index=transports.length,t={disconnects:0,
      connect:()=>index===0?gate.promise:Promise.resolve(),disconnect:async()=>{t.disconnects++;},subscribeRevisions:async()=>({subscriptionId:'s-'+index})};transports.push(t);return t;}}};
  const ports=Desktop.createPorts(root),first=ports.subscribeRevisions(()=>{},()=>{},null,()=>firstCurrent);
  await turn();const second=await ports.subscribeRevisions(()=>{},()=>{},null,()=>true);assert.equal(second.subscriptionId,'s-1');
  firstCurrent=false;gate.resolve();await assert.rejects(first,/cancelled/);assert.equal(transports[0].disconnects,1);assert.equal(transports[1].disconnects,0);
  await ports.unsubscribeRevisions();assert.equal(transports[1].disconnects,1);
});

for(const refusal of ['unsupported','notDispatched'])test('cancel inside pending '+refusal+' reconciliation can never return a legacy fallback', {timeout:2000},async()=>{
  const gate=deferred(),error=refusal==='unsupported'?{code:'unsupported_format'}:{code:'invalid_request',details:{disposition:'notDispatched'}};
  const h=harness({bootstrapError:error,reconcileGate:gate}),pending=h.open();await turn();assert.equal(h.checks(),2);
  h.session.cancelOpen();await assert.rejects(pending,/cancel/);assert.equal(h.session.isIndeterminate(),true);assert.equal(h.removes(),1);
  gate.resolve();await turn();assert.equal(h.previews(),0);assert.equal(h.effects.includes('commit'),false);assert.equal(h.session.isActive(),false);
});
test('cancel during pending prior-UI restoration fences rather than returning unsupported fallback', {timeout:2000},async()=>{
  const gate=deferred(),h=harness({bootstrapError:{code:'unsupported_format'}});h.transition.restore=()=>gate.promise;
  const pending=h.open();await turn();h.session.cancelOpen();await assert.rejects(pending,/cancel/);gate.resolve();await turn();
  assert.equal(h.session.isIndeterminate(),true);assert.equal(h.previews(),0);assert.equal(h.effects.includes('commit'),false);
});
test('same-turn cancel and revision connect completion cannot start a subscription after cancellation', {timeout:2000},async()=>{
  const gate=deferred();let starts=0;const h=harness({revisionGate:gate,onSubscriptionStart:()=>starts++,receipts:[{status:'presented',workId:'first',viewGeneration:1},{status:'presented',workId:'second',viewGeneration:2}]});
  const pending=h.open();await turn();h.session.cancelOpen();gate.resolve();await assert.rejects(pending,/cancel/);await turn();
  assert.equal(starts,0);assert.equal(h.session.isIndeterminate(),true);assert.equal(h.removes(),1);
});
test('older transport connect cannot subscribe after a newer transport is selected even if old predicate remains true',async()=>{
  const gate=deferred(),transports=[];
  const root={__TAURI__:{core:{invoke:()=>{}},event:{listen:async()=>()=>{}}},NemoNativeViewportUnderlay:{desktop:()=>({})},
    NemoApplicationMcpTransport:{createNativeTauriTransport:()=>{const index=transports.length,t={starts:0,disconnects:0,
      connect:()=>index===0?gate.promise:Promise.resolve(),disconnect:async()=>{t.disconnects++;},subscribeRevisions:async()=>{t.starts++;return {subscriptionId:'s-'+index};}};transports.push(t);return t;}}};
  const ports=Desktop.createPorts(root),first=ports.subscribeRevisions(()=>{},()=>{},null,()=>true);await turn();await ports.subscribeRevisions(()=>{},()=>{},null,()=>true);
  gate.resolve();await assert.rejects(first,/cancelled/);assert.equal(transports[0].starts,0);assert.equal(transports[0].disconnects,1);assert.equal(transports[1].starts,1);assert.equal(transports[1].disconnects,0);
});

test('request stale at entry preserves typed predispatch refusal and verified empty prior slot',async()=>{
  const h=harness({current:()=>false});await assert.rejects(h.open(),error=>error.code==='replacement_refused'&&error.details.disposition==='notDispatched');
  assert.deepEqual(h.calls,['nemo_native_object_client_status']);assert.equal(h.session.isIndeterminate(),false);assert.equal(h.session.blocksLegacy(),false);
  assert.deepEqual(h.effects,[]);assert.equal(h.session.cancelOpen(),false);
});
test('retired transition during unsupported reconciliation cannot fall through to legacy import',async()=>{
  const gate=deferred();let current=true;const h=harness({current:()=>current,bootstrapError:{code:'unsupported_format'},reconcileGate:gate});
  const pending=h.open();await turn();current=false;gate.resolve();await assert.rejects(pending);assert.equal(h.previews(),0);
  assert.equal(h.session.isIndeterminate(),false);assert.equal(h.session.blocksLegacy(),false);assert.equal(h.effects.includes('commit'),false);
});


test('connected deferred recovery samples every receipt accessor once and publishes only inspected identities',async()=>{
  const counts=[{status:0,workId:0,viewGeneration:0},{status:0,workId:0,viewGeneration:0},{status:0,workId:0,viewGeneration:0}];
  const receipts=['deferred-occluded','presented','presented'].map((status,i)=>Object.defineProperties({},{
    status:{get(){assert.equal(++counts[i].status,1,'status accessor must not be sampled again');return status;}},
    workId:{get(){assert.equal(++counts[i].workId,1);return i===2?'work-2':'work-1';}},
    viewGeneration:{get(){assert.equal(++counts[i].viewGeneration,1);return i===2?2:1;}}
  }));
  const h=harness({receipts}),pending=h.open();await turn();assert.equal(h.previews(),1);
  assert.deepEqual(counts,[{status:1,workId:1,viewGeneration:1},{status:0,workId:0,viewGeneration:0},{status:0,workId:0,viewGeneration:0}]);
  h.hint({visible:true});await pending;assert.equal(h.previews(),3);assert.equal(h.session.isActive(),true);
  assert.deepEqual(counts,Array.from({length:3},()=>({status:1,workId:1,viewGeneration:1})));
  assert.deepEqual(h.effects,['listen','reveal','remove','commit']);assert.equal(h.removes(),1);
  assert.equal(h.session.readPresentedObjectSession().presentation.workId,'work-2');
});
for(const resumed of [false,true])test('connected '+(resumed?'resumed':'initial')+' status refusal never reads later receipt identity',async()=>{
  for(const status of ['stale-discarded','deferred-timeout','failed-validation','deferred-PRIVATE','deferred-occluded:PRIVATE',{},null]){
    let reads=0;const diagnostics=[],receipt=Object.defineProperties({},{status:{get(){reads++;return status;}},
      workId:{get(){assert.fail('status refusal read work identity');}},viewGeneration:{get(){assert.fail('status refusal read view identity');}}});
    const receipts=resumed?[{status:'deferred-occluded',workId:'work-1',viewGeneration:1},receipt]:[receipt];
    const h=harness({receipts,openDiagnostic:(event,code)=>diagnostics.push({event,code})}),pending=h.open();
    const rejection=assert.rejects(pending,/frame was not presented/);if(resumed){await turn();assert.equal(h.previews(),1);h.hint({visible:true});}await rejection;
    const known=['stale-discarded','deferred-timeout','failed-validation'].includes(status);
    assert.equal(diagnostics.find(e=>e.event==='post-bootstrap-failed').code,'post-bootstrap-first-preview-'+(known?'status-'+status:'status')+':local-refusal:unknown:other');
    assert.equal(reads,1);assert.equal(h.previews(),resumed?2:1);assert.equal(h.effects.includes('reveal'),false);
    assert.equal(h.session.readPresentedObjectSession(),null);assert.equal(h.removes(),1);assert.doesNotMatch(JSON.stringify(diagnostics),/PRIVATE/);
  }
});
for(const resumed of [false,true])test('connected '+(resumed?'resumed':'initial')+' accessor throw retains original object and receipt-access witness',async()=>{
  for(const field of ['status','workId','viewGeneration']){
    const original={message:'PRIVATE document bytes'},diagnostics=[],reads={status:0,workId:0,viewGeneration:0};
    Object.defineProperty(original,'code',{get(){throw Error('PRIVATE error code');}});
    const values={status:resumed?'presented':'deferred-occluded',workId:'work-1',viewGeneration:1};
    const receipt=Object.defineProperties({},Object.fromEntries(Object.keys(values).map(key=>[key,{get(){reads[key]++;if(key===field)throw original;return values[key];}}])));
    const receipts=resumed?[{status:'deferred-occluded',workId:'work-1',viewGeneration:1},receipt]:[receipt];
    const h=harness({receipts,openDiagnostic:(event,code)=>diagnostics.push({event,code})}),pending=h.open();
    const rejection=assert.rejects(pending,error=>error===original);if(resumed){await turn();h.hint({visible:true});}await rejection;
    assert.equal(reads.status,1);assert.equal(reads.workId,field==='status'?0:1);assert.equal(reads.viewGeneration,field==='viewGeneration'?1:0);
    assert.equal(diagnostics.find(e=>e.event==='post-bootstrap-failed').code,'post-bootstrap-first-preview-receipt-access:local-refusal:unknown:other');
    assert.doesNotMatch(JSON.stringify(diagnostics),/PRIVATE|bytes/);assert.equal(h.previews(),resumed?2:1);
    assert.equal(h.effects.includes('reveal'),false);assert.equal(h.session.isIndeterminate(),true);assert.equal(h.removes(),1);
  }
});
test('connected invoke rejection remains invoke-rejected rather than receipt-access and retains original object',async()=>{
  const original={message:'PRIVATE native rejection'},diagnostics=[];Object.defineProperty(original,'code',{get(){throw Error('PRIVATE error code');}});
  const h=harness({previewError:original,openDiagnostic:(event,code)=>diagnostics.push({event,code})});
  await assert.rejects(h.open(),error=>error===original);
  assert.equal(diagnostics.find(e=>e.event==='post-bootstrap-failed').code,'post-bootstrap-first-preview:invoke-rejected:unknown:other');
  assert.doesNotMatch(JSON.stringify(diagnostics),/PRIVATE/);assert.equal(h.previews(),1);assert.equal(h.effects.includes('reveal'),false);assert.equal(h.removes(),1);
});
