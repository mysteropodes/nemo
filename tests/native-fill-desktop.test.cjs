'use strict';
const test=require('node:test');
const assert=require('node:assert/strict');
const fill=require('../src/js/adapters/native-fill-desktop.js');
const contract=require('../src/js/adapters/native-object-edit-contract.js');
const vector=require('../src/js/adapters/native-object-vector-contract.js');
const stroke=require('../src/js/adapters/native-object-vector-stroke.js');
const OP='command.document.object.fill.trace';
function fixture(options={}){
  let pin={instanceId:'instance-A',documentId:'document-A',lifecycleGeneration:1,contentRevision:0,documentSnapshotId:'snap-0'};
  const dispatched=[],reconciled=[],receipts=new Map();let release;let executions=0;let first=true;
  const session={isActive:()=>true,info:()=>pin,requestId:()=>`fill-${dispatched.length+1}`,
    runResourceCommand:async(action)=>{
      if(options.hold)await new Promise((resolve)=>{release=resolve;});
      return action({dispatch:async(request)=>{
        dispatched.push(structuredClone(request));
        if(!receipts.has(request.requestId)){
          executions++;receipts.set(request.requestId,{apiVersion:2,requestId:request.requestId,instanceId:pin.instanceId,
            documentId:pin.documentId,ok:true,contentRevision:pin.contentRevision+1,result:{applied:true,historyEntriesAdded:1}});
        }
        if(options.ambiguous&&first){first=false;throw new Error('response lost after commit');}
        return structuredClone(receipts.get(request.requestId));
      },reconcile:async(revision)=>{reconciled.push(revision);pin={...pin,contentRevision:revision??1,documentSnapshotId:'snap-1'};}});
    }};
  const editor=fill.create({current:()=>session,ready:()=>true,target:()=>({layerUid:'L',contextId:'scene-root',frame:3})});
  return {session,editor,dispatched,reconciled,executions:()=>executions,
    change:()=>{pin={...pin,documentId:'document-B'};},release:()=>release()};
}
const input={point:[12,34],color:'#369',opacity:0.5,remove:false};

test('ordinary fill reaches the shared revisioned native route and reconciles after its receipt',async()=>{
  const f=fixture();const result=await f.editor.fill(input);
  assert.equal(f.executions(),1);assert.equal(result.applied,true);assert.equal(result.contentRevision,1);
  assert.deepEqual(f.dispatched[0],{apiVersion:2,requestId:'fill-1',instanceId:'instance-A',documentId:'document-A',
    expectedRevision:0,operation:OP,payload:{command:'object.fill.trace',contextId:'scene-root',layerUid:'L',frame:3,
      point:[12,34],color:'#336699ff',opacity:0.5,remove:false}});
  assert.deepEqual(f.reconciled,[1]);
});

test('a lost response replays the identical request and executes once',async()=>{
  const f=fixture({ambiguous:true});const result=await f.editor.fill(input);
  assert.equal(result.applied,true);assert.equal(f.executions(),1);assert.equal(f.dispatched.length,2);
  assert.deepEqual(f.dispatched[0],f.dispatched[1]);assert.equal(f.reconciled.length,1);
});

test('a queued fill refuses an owner change before any native mutation',async()=>{
  const f=fixture({hold:true});const pending=f.editor.fill(input);f.change();f.release();
  await assert.rejects(pending,{code:'stale_owner'});assert.equal(f.dispatched.length,0);assert.equal(f.executions(),0);
});

test('source pointer coordinates are copied before a queued command executes',async()=>{
  const f=fixture({hold:true});const original={...input,point:[12,34]};const pending=f.editor.fill(original);
  original.point[0]=999;f.release();await pending;assert.deepEqual(f.dispatched[0].payload.point,[12,34]);
});

test('the public fill shape refuses malformed input before dispatch',async()=>{
  for(const change of [{point:[1]},{point:[NaN,2]},{point:[1e13,2]},{color:'red'},{opacity:2}]){
    const f=fixture();await assert.rejects(f.editor.fill({...input,...change}));assert.equal(f.dispatched.length,0);
  }
  const p={command:'object.fill.trace',layerUid:'L',contextId:'scene-root',frame:0,point:[0,0],color:'#ffffff00',opacity:0,remove:true};
  assert.doesNotThrow(()=>contract.payload(OP,p));
  assert.throws(()=>contract.payload(OP,{...p,legacyFallback:true}));
  assert.throws(()=>contract.payload(OP,{...p,remove:'true'}));
});

test('fill provenance round-trips authored zero and wall identities without aliasing',()=>{
  const dict={segments:[[0,0],[40,0],[40,40],[0,40]].map((point)=>({point,handleIn:[0,0],handleOut:[0,0]})),
    closed:true,fillColor:'#336699',strokeColor:null,hasRealStroke:false,opacity:0.5,
    fillSeed:[12,34],fillSeeds:[[12,34],[20,20]],fillWalls:['wall-a','wall-b'],fillGapPx:0};
  const mapped=stroke.toRecord(dict,{layerUid:'L',frame:0,strokeId:'fill-a'});assert.equal(mapped.ok,true);
  assert.deepEqual(mapped.unmapped,[]);
  assert.deepEqual(mapped.record.vectorExt.fillTrace,{seed:[12,34],seeds:[[12,34],[20,20]],walls:['wall-a','wall-b'],gapPx:0});
  assert.doesNotThrow(()=>vector.record(mapped.record,false));
  const restored=stroke.toDict(mapped.record);
  for(const key of ['fillSeed','fillSeeds','fillWalls','fillGapPx'])assert.deepEqual(restored[key],dict[key]);
  restored.fillWalls[0]='other';assert.equal(mapped.record.vectorExt.fillTrace.walls[0],'wall-a');
  assert.throws(()=>vector.record({...mapped.record,vectorExt:{fillTrace:{seed:[1],gapPx:0}}},false));
});

test('native pointer capture cannot enter the legacy writer when projection is unavailable',()=>{
  const events=[];const root={state:{tool:'fill',playing:false,activeLayerIdx:0},
    NemoNativeOpacityCutover:{blocksLegacy:()=>true},showToast:(s)=>events.push(s)};
  assert.equal(fill.handlePointerDown(root,{stopImmediatePropagation:()=>events.push('stop'),preventDefault:()=>events.push('prevent')}),true);
  assert.deepEqual(events.slice(0,2),['stop','prevent']);
  assert.equal(fill.handlePointerDown({state:{tool:'fill'}},{ }),false);
});

function cameraPointerFixture(hold=false){
 const f=fixture({hold}),before=f.session.info,run=f.session.runResourceCommand;
 const rect={left:20,top:30,width:640,height:360},view={mode:'stored-camera',cameraAvailable:true,documentToComposition:[2,0,0,2,-160,-90],compositionToDocument:[.5,0,0,.5,80,45]};
 f.session.info=()=>({...before(),canvasW:320,canvasH:180,layers:[{layerUid:'L',locked:false}]});
 const presentation={...f.session.info(),owner:'native',status:'presented',frame:3,view};
 const shown=()=>({...f.session.info(),presentation});f.session.readPresentedObjectSession=shown;
 f.session.runResourceCommand=action=>run(ctx=>action({...ctx,readPresentedObjectSession:shown}));
 const root={state:{tool:'fill',playing:false,currentFrame:3,activeLayerIdx:0,cameraView:true,fillColor:'#369',opacity:50},
  NemoNativeOpacityCutover:{blocksLegacy:()=>true},NemoNativeObjectProject:{desktop:()=>f.session},NemoNativeObjectProjectFile:{projectionReadyForUI:()=>true},
  document:{getElementById:()=>({getBoundingClientRect:()=>rect}),documentElement:{clientWidth:1200,clientHeight:900,
   classList:{contains:()=>true},style:{getPropertyValue:k=>String({left:20,top:30,right:660,bottom:390}[k.replace('--nemo-native-','')])}}},
  SMEngineBridge:{screenToWorld(){assert.fail('legacy Paper/WASM coordinates');}},showToast(){}};
 const ports={mapping:require('../src/js/adapters/native-viewport-underlay.js').canvasMapping};
 const pointer=()=>fill.handlePointerDown(root,{clientX:100,clientY:90,stopImmediatePropagation(){},preventDefault(){}},ports);
 return {...f,root,pointer,ports};
}
test('actual Fill pointer uses current native camera proof and its inverse through the existing command queue',async()=>{
 const f=cameraPointerFixture();assert.equal(f.pointer(),true);await new Promise(setImmediate);await new Promise(setImmediate);
 assert.equal(f.dispatched.length,1);assert.deepEqual(f.dispatched[0].payload.point,[100,60]);assert.equal(f.executions(),1);
 const stale=cameraPointerFixture(true);stale.pointer();stale.root.state.cameraView=false;stale.release();await new Promise(setImmediate);
 assert.equal(stale.dispatched.length,0);assert.equal(stale.executions(),0);
});


test('native Fill refuses missing injected mapping and its bootstrap supplies the real public mapping',async()=>{
 const f=cameraPointerFixture();f.root.NemoNativeViewportUnderlay={canvasMapping:f.ports.mapping};f.ports.mapping=undefined;
 assert.equal(f.pointer(),true);await new Promise(setImmediate);assert.equal(f.dispatched.length,0);
 const ready=cameraPointerFixture(),listeners=new Map(),document=ready.root.document;
 ready.root.NemoNativeFillDesktop=fill;ready.root.NemoNativeViewportUnderlay={canvasMapping:ready.ports.mapping};
 const canvas={getBoundingClientRect:()=>({left:20,top:30,width:640,height:360}),addEventListener:(kind,fn)=>listeners.set(kind,fn)};
 document.readyState='complete';document.addEventListener=()=>{};
 document.getElementById=id=>['canvas-area','drawing-canvas'].includes(id)?canvas:null;
 ready.root.window=ready.root;
 require('node:vm').runInNewContext(require('node:fs').readFileSync(require('node:path').join(__dirname,'../src/js/fill-bridge.js'),'utf8'),ready.root);
 listeners.get('pointerdown')({clientX:100,clientY:90,stopImmediatePropagation(){},preventDefault(){}});
 await new Promise(setImmediate);await new Promise(setImmediate);
 assert.equal(ready.dispatched.length,1);assert.deepEqual(ready.dispatched[0].payload.point,[100,60]);
});
