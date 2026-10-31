'use strict';
// CS8-SDK-A (#2354): each queued script write reaches the SAME native entry the ordinary UI uses, with the literal request.
// Lifecycle requests cross the real common v2 SDK (native-object-edit-harness); the other entries are the UI controllers' doubles.
const test=require('node:test'),assert=require('node:assert/strict');
const {createHarness}=require('./native-object-edit-harness.cjs');
const Writes=require('../src/js/application/script-native-writes.js');
const Intent=require('../src/js/adapters/native-object-layer-lifecycle-intent.js'),Edit=require('../src/js/adapters/native-object-edit-contract.js');
const P='command.document.object.';

function world(options={}){
  const h=createHarness({layers:['layer-a','layer-b','layer-c'],step:(r)=>(r.operation===P+'layer.duplicate'?{result:{applied:true,historyEntriesAdded:1,layerUid:'native-layer:9:0'}}:{applied:true}),...options.harness});
  const reconciled=[];let active=true;
  const info=()=>({instanceId:'instance-A',documentId:'document-A',lifecycleGeneration:1,contentRevision:h.state.revision,documentSnapshotId:'s'+h.state.revision,layers:h.state.layers.map(layerUid=>({layerUid}))});
  const session={isActive:()=>active,info,requestId:h.ports.requestId,
    runResourceCommand:(action)=>action({
      dispatch:async(request)=>{const r=await h.adapter.dispatch(request);if(!r.ok)throw new Error('Native command refused: '+r.error.code);return r;},
      reconcile:async(rev)=>{reconciled.push(rev);return info();}}),
    readPresentedObjectSession:()=>null,...options.session};
  const root={NemoNativeObjectLayerLifecycleIntent:Intent,NemoNativeObjectEditContract:Edit,state:{currentFrame:0},...options.root};
  return {h,session,root,reconciled,setActive:v=>{active=v;}};
}

test('nothing is offered that the host does not have: absent entries are absent, never stubbed',()=>{
  assert.deepEqual(Object.keys(Writes.create({},null)),[]);
  assert.deepEqual(Object.keys(Writes.create({},{runResourceCommand(){},requestId(){}})),[],'a session without the lifecycle modules offers no layer edits');
  const w=world();assert.deepEqual(Object.keys(Writes.create(w.root,w.session)).sort(),['duplicate','remove','rename']);
});

test('remove, duplicate and rename send the literal lifecycle requests through the resource command route and reconcile each receipt',async()=>{
  const w=world(),ops=Writes.create(w.root,w.session);
  const removed=await ops.remove('layer-b');
  assert.deepEqual(w.h.state.sent[0],{apiVersion:2,requestId:'layer-delete-1',instanceId:'instance-A',documentId:'document-A',operation:P+'layer.delete',expectedRevision:7,
    payload:{command:'object.layer.delete',selection:{active:'layer-b',selected:['layer-b']}}});
  assert.deepEqual({ok:removed.ok,applied:removed.applied},{ok:true,applied:true});
  const dup=await ops.duplicate('layer-a');
  assert.deepEqual(w.h.state.sent[1].payload,{command:'object.layer.duplicate',layerUid:'layer-a'});assert.equal(w.h.state.sent[1].expectedRevision,8);
  assert.deepEqual(dup,{ok:true,applied:true,layerUid:'native-layer:9:0'});
  const named=await ops.rename('layer-c','Ink');
  assert.deepEqual(w.h.state.sent[2].payload,{command:'object.layer.rename',layerUid:'layer-c',name:'Ink'});assert.equal(named.ok,true);
  assert.deepEqual(w.reconciled,[8,9,10],'every applied receipt is reconciled before the next step reads the model');
});

test('a stale layer, an owner refusal, a missing session and an inactive document are refusals, never sent or retried',async()=>{
  let w=world(),ops=Writes.create(w.root,w.session);
  assert.equal((await ops.rename('layer-gone','x')).ok,false);assert.deepEqual(w.h.state.sent,[],'a layer the projection no longer lists is refused before sending');
  w=world({harness:{step:(r)=>({error:{code:'not_found',message:'no'}})}});ops=Writes.create(w.root,w.session);
  const refused=await ops.remove('layer-a');assert.equal(refused.ok,false);assert.equal(refused.code,'not_found');assert.equal(w.h.state.sent.length,1);assert.deepEqual(w.reconciled,[]);
  w=world();ops=Writes.create(w.root,w.session);w.setActive(false);
  const gone=await ops.duplicate('layer-a');assert.equal(gone.ok,false);assert.deepEqual(w.h.state.sent,[]);
  w=world({harness:{step:()=>({drop:'socket closed'})}});ops=Writes.create(w.root,w.session);
  const dropped=await ops.rename('layer-a','x');assert.equal(dropped.ok,false);assert.equal(w.h.state.sent.length,1,'an indeterminate transport failure is not replayed');
});

test('addLayer is Drawing Add: a created layer is ok whether or not the selection followed, every other status is a refusal',async()=>{
  const answers=[{status:'created-selected',layerUid:'u1'},{status:'created-unselected',layerUid:'u2'},{status:'busy'},{status:'unconfirmed'},{status:'created-selected'}];
  const root={NemoNativeDrawingAddController:{add:()=>Promise.resolve(answers.shift())}};
  const ops=Writes.create(root,null);
  assert.deepEqual(await ops.add(),{ok:true,applied:true,layerUid:'u1'});assert.deepEqual(await ops.add(),{ok:true,applied:true,layerUid:'u2'});
  assert.deepEqual(await ops.add(),{ok:false,code:'busy'});assert.deepEqual(await ops.add(),{ok:false,code:'unconfirmed'});
  assert.equal((await ops.add()).ok,false,'a created answer without the new id cannot be tied to a layer');
  assert.equal((await Writes.create({NemoNativeDrawingAddController:{add:()=>Promise.reject(new Error('x'))}},null).add()).ok,false);
});

test('split moves the playhead, waits until the cut sees it presented, then asks the ordinary cut',async()=>{
  const seen=[];let presentedFrame=0;
  const root={state:{currentFrame:0},goToFrame(f){seen.push(['go',f]);root.state.currentFrame=f;setTimeout(()=>{presentedFrame=f;},0);},setTimeout:(fn)=>setTimeout(fn,1),
    SM:{splitLayerAtPlayhead:async(i)=>{seen.push(['cut',i,root.state.currentFrame]);return true;}}};
  const session={readPresentedObjectSession:()=>({presentation:{frame:presentedFrame}})};
  const ops=Writes.create(root,session);
  assert.deepEqual(await ops.split(2,5),{ok:true,applied:true});assert.deepEqual(seen,[['go',5],['cut',2,5]]);
  seen.length=0;assert.equal((await ops.split(1,null)).ok,true);assert.deepEqual(seen,[['cut',1,5]],'no frame cuts where the playhead stands');
  seen.length=0;assert.equal((await ops.split(1,5)).ok,true);assert.deepEqual(seen,[['cut',1,5]],'a playhead already there is not moved');
  root.SM.splitLayerAtPlayhead=async()=>false;assert.deepEqual(await ops.split(1,5),{ok:false,code:'cut-refused'});
  const stuck=Writes.create({...root,goToFrame(f){root.state.currentFrame=f;}},{readPresentedObjectSession:()=>({presentation:{frame:99}})});
  assert.deepEqual(await stuck.split(0,7),{ok:false,code:'playhead-not-presented'});
});

test('property and track edits are the session writers with the presented pin and a liveness check',async()=>{
  const calls=[];let live=true;const pinNow={instanceId:'i',documentId:'d',lifecycleGeneration:1,contentRevision:3,documentSnapshotId:'s'};
  const session={isActive:()=>live,info:()=>pinNow,editProperty:function(payload,pin,current){calls.push(['edit',payload,pin,current(),this===session]);return Promise.resolve({applied:true,historyEntriesAdded:1});},
    editPropertyTrack:function(payload,pin,current){calls.push(['track',payload,current()]);return Promise.resolve({applied:false,historyEntriesAdded:0});}};
  const ops=Writes.create({},session);
  assert.deepEqual(await ops.property({layerUid:'a',property:'position',frame:4,action:{kind:'setValue',values:[1,2]}}),{ok:true,applied:true});
  assert.deepEqual(calls[0],['edit',{command:'object.property.edit',target:{contextId:'scene-root',layerUid:'a'},property:'position',action:{frame:4,kind:'setValue',values:[1,2]}},pinNow,true,true]);
  assert.deepEqual(await ops.track({layerUid:'a',property:'rotation',frame:8,preset:'hold'}),{ok:true,applied:false});
  assert.deepEqual(calls[1],['track',{command:'object.property.track.edit',selections:[{target:{contextId:'scene-root',layerUid:'a'},property:'rotation',frame:8}],action:{kind:'preset',preset:'hold'}},true]);
  const stale=Writes.create({},{...session,editProperty(payload,pin,current){live=false;return Promise.resolve({applied:current()});}});
  assert.deepEqual(await stale.property({layerUid:'a',property:'opacity',frame:0,action:{kind:'setValue',values:[1]}}),{ok:true,applied:false},'the writer sees the session go inactive');
  const bad=Writes.create({},{...session,editProperty:()=>Promise.resolve({ok:1})});
  assert.deepEqual(await bad.property({layerUid:'a',property:'opacity',frame:0,action:{kind:'setValue',values:[1]}}),{ok:false,code:'malformed-receipt'});
  const refused=Writes.create({},{...session,editProperty:()=>Promise.reject(new Error('Native property edit intent is stale'))});
  assert.match((await refused.property({layerUid:'a',property:'opacity',frame:0,action:{kind:'setValue',values:[1]}})).message,/stale/);
  assert.deepEqual(Object.keys(Writes.create({},{...session,editPropertyTrack:undefined})).sort(),['property'],'a session without the track writer offers no track entry');
});

test('markers go through the composition controller: add by frame, remove by the ordinal the presented projection lists',async()=>{
  const edits=[];let shown={composition:{markers:[{ordinal:1,frame:3,name:'a'},{ordinal:2,frame:9}]}};
  const root={NemoNativeCompositionController:{readWhenPresented:()=>Promise.resolve(shown),edit:(action,expected)=>{edits.push([action,expected===shown]);return Promise.resolve({status:'applied-presented',applied:true});}}};
  const ops=Writes.create(root,null);
  assert.deepEqual(await ops.marker({action:'add',frame:12,name:'Beat'}),{ok:true,applied:true});assert.deepEqual(edits[0],[{kind:'addMarker',frame:12,name:'Beat'},true]);
  await ops.marker({action:'add',frame:13,name:''});assert.deepEqual(edits[1][0],{kind:'addMarker',frame:13},'an empty name is not sent');
  await ops.marker({action:'remove',frame:9});assert.deepEqual(edits[2][0],{kind:'removeMarker',ordinal:2});
  assert.deepEqual(await ops.marker({action:'remove',frame:40}),{ok:true,applied:false});assert.equal(edits.length,3,'no marker there: nothing is sent');
  root.NemoNativeCompositionController.edit=()=>Promise.resolve({status:'unconfirmed',code:'x'});assert.deepEqual(await ops.marker({action:'add',frame:1}),{ok:false,code:'unconfirmed'});
  root.NemoNativeCompositionController.edit=()=>Promise.resolve({status:'unchanged',applied:false});assert.deepEqual(await ops.marker({action:'add',frame:1}),{ok:true,applied:false});
  shown=null;assert.deepEqual(await ops.marker({action:'add',frame:1}),{ok:false,code:'composition-unavailable'});
});
