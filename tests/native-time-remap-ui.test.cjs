'use strict';
// Real Time Remap UI child, placement contract and project projection; the native
// owner, its queue and the desktop placement read are modelled. These controls
// establish no native kernel, decoder count, installed UI or pixel parity.
const test=require('node:test'),assert=require('node:assert/strict');
const Remap=require('../src/js/adapters/native-time-remap-ui.js');
const ProjectFile=require('../src/js/adapters/native-object-project-file.js');
const OP='command.document.object.project.placement.edit';
async function settle(){for(let i=0;i<20;i++)await Promise.resolve();}
function fixture(placement){
  let info={instanceId:'instance',documentId:'document',lifecycleGeneration:3,contentRevision:7,documentSnapshotId:'snapshot-7',
    canvasW:320,canvasH:180,fps:24,totalFrames:48,layers:[{layerUid:'a',name:'Ink'},{layerUid:'v',name:'Clip'}]};
  const effects=[],calls=[],reads=[];
  const legacy={toggleTimeRemap(){effects.push('legacy');return true;},isTimeRemapped(){effects.push('legacy');return true;},
    footageInterpMode(){effects.push('legacy');return 'blend';},setFootageInterpMode(){effects.push('legacy');return true;},t:key=>key};
  const root={state:{},SM:Object.assign({},legacy),showToast(message){effects.push(['toast',message]);},updateUI(){effects.push('updateUI');}};
  ProjectFile.projectView(root,info);root.state.appMode='motion';root.state.activeSymbolId=null;root.state.currentFrame=10;root._curFrame=10;
  let facts=placement,active=true,blocks=true,serial=0;
  const control={receipt:null,range:{inPoint:4,outPoint:30},failDispatch:null};
  function presented(){return {...info,available:true,kind:'native-object'};}
  const session={isActive:()=>active,blocksLegacy:()=>blocks,info:()=>info,readPresentedObjectSession:presented,
    requestId:label=>label+'-'+(++serial),
    async readLayerPlacement(uid,frame,pin){reads.push(['placement',uid,frame,pin.documentSnapshotId]);
      return {kind:'layerPlacement',instanceId:pin.instanceId,documentId:pin.documentId,atRevision:pin.contentRevision,
        documentSnapshotId:pin.documentSnapshotId,contextId:'scene-root',layerUid:uid,frame,placement:uid==='v'?facts:null};},
    async readLayerRange(uid,frame,pin){reads.push(['range',uid,frame]);
      return {kind:'layerRange',instanceId:pin.instanceId,documentId:pin.documentId,atRevision:pin.contentRevision,
        documentSnapshotId:pin.documentSnapshotId,contextId:'scene-root',layerUid:uid,frame,
        range:{rawInPoint:control.range.inPoint,rawOutPoint:control.range.outPoint,...control.range,hasCustomRange:true,hasTimeLink:false}};},
    async runResourceCommand(action){
      effects.push('queue');
      return action({readPresentedObjectSession:presented,
        async dispatch(request){calls.push(structuredClone(request));if(control.failDispatch)throw control.failDispatch;
          return control.receipt?control.receipt(request):{apiVersion:2,requestId:request.requestId,instanceId:info.instanceId,
            documentId:info.documentId,contentRevision:request.expectedRevision+1,ok:true,result:{applied:true,historyEntriesAdded:1}};},
        async reconcile(revision){effects.push(['reconcile',revision]);
          if(revision!==undefined){info={...info,contentRevision:revision,documentSnapshotId:'snapshot-'+revision};
            assert.equal(ProjectFile.refreshLayerTable(root,info),true);}return info;}});
    }};
  const ports={currentSession:()=>session,projectionReady:value=>ProjectFile.projectionReadyForUI(root,value)};
  const api=Remap.createPorts(root,ports);
  return {root,api,legacy,control,effects,calls,reads,setFacts:v=>{facts=v;},setBlocks:v=>{blocks=v;},setActive:v=>{active=v;}};
}
const video=extra=>({placementId:'p1',assetId:'v1',assetKind:'video',offsetFrames:0,timeRemap:null,interpMode:null,
  frameBlend:null,sourceFrameCount:48,...extra});

test('enable dispatches the bounded intent with the native range and the bound source frame count',async()=>{
  const f=fixture(video());assert.ok(f.api.bindTimeRemapEntries());
  assert.equal(await f.root.SM.toggleTimeRemap(1),true);
  assert.equal(f.calls.length,1);
  assert.equal(f.calls[0].operation,OP);assert.equal(f.calls[0].expectedRevision,7);
  assert.deepEqual(f.calls[0].payload,{kind:'enableTimeRemap',placementId:'p1',inFrame:4,outFrame:30,lastSourceFrame:47});
  assert.deepEqual(f.effects.filter(e=>Array.isArray(e)),[['reconcile',8],['toast','toastTimeRemapEnabledFromSuffix47']]);
  assert.equal(f.effects.includes('legacy'),false);
});

test('disable and the footage mode edit the existing placement; an unchanged mode dispatches nothing',async()=>{
  const f=fixture(video({timeRemap:{keyCount:2,keyAtFrame:false},interpMode:'blend',frameBlend:true}));f.api.bindTimeRemapEntries();
  assert.equal(await f.root.SM.toggleTimeRemap(1),true);
  assert.deepEqual(f.calls[0].payload,{kind:'disableTimeRemap',placementId:'p1'});
  assert.equal(await f.root.SM.setFootageInterpMode(1,'blend'),true);
  assert.equal(f.calls.length,1,'the stored mode is not re-sent');
  assert.equal(await f.root.SM.setFootageInterpMode(1,'motion'),true);
  assert.deepEqual(f.calls[1].payload,{kind:'setInterpMode',placementId:'p1',interpMode:'motion'});
  assert.equal(f.effects.includes('legacy'),false);
});

test('an unknown source frame count keeps Enable unavailable and a layer without video only explains',async()=>{
  for(const count of [null,0]){
    const f=fixture(video({sourceFrameCount:count}));f.api.bindTimeRemapEntries();
    assert.equal(await f.root.SM.toggleTimeRemap(1),false);
    assert.equal(f.calls.length,0);assert.equal(f.reads.some(r=>r[0]==='range'),false);
  }
  const f=fixture(video());f.api.bindTimeRemapEntries();
  assert.equal(await f.root.SM.toggleTimeRemap(0),false);
  assert.equal(f.calls.length,0);
  assert.deepEqual(f.effects.filter(e=>Array.isArray(e)),[['toast','toastTimeRemapAppliesToComponents']]);
});

test('synchronous answers come from one read of the presented snapshot and redraw once when they differ',async()=>{
  const f=fixture(video({timeRemap:{keyCount:2,keyAtFrame:true},interpMode:'motion'}));f.api.bindTimeRemapEntries();
  assert.equal(f.root.SM.isTimeRemapped(1),false,'the first answer is the default while the read is pending');
  await settle();
  assert.equal(f.root.SM.isTimeRemapped(1),true);assert.equal(f.root.SM.footageInterpMode(1),'motion');
  assert.equal(f.reads.filter(r=>r[0]==='placement').length,1);
  assert.equal(f.effects.filter(e=>e==='updateUI').length,1);
  assert.equal(f.root.SM.isTimeRemapped(0),false);await settle();
  assert.equal(f.effects.filter(e=>e==='updateUI').length,1,'a layer without video keeps the default without a redraw');
});

test('a refused, uncorrelated or no-op edit reports false and an uncertain receipt reconciles without replay',async()=>{
  const noop=fixture(video());noop.api.bindTimeRemapEntries();
  noop.control.receipt=r=>({apiVersion:2,requestId:r.requestId,instanceId:'instance',documentId:'document',contentRevision:7,ok:true,result:{applied:false,historyEntriesAdded:0}});
  assert.equal(await noop.root.SM.toggleTimeRemap(1),false);
  assert.equal(noop.effects.some(e=>Array.isArray(e)&&e[0]==='reconcile'),false);
  const lost=fixture(video());lost.api.bindTimeRemapEntries();lost.control.failDispatch=new Error('transport lost');
  assert.equal(await lost.root.SM.toggleTimeRemap(1),false);
  assert.equal(lost.calls.length,1);
  assert.deepEqual(lost.effects.filter(e=>Array.isArray(e)),[['reconcile',undefined]]);
  const foreign=fixture(video());foreign.api.bindTimeRemapEntries();
  foreign.control.receipt=r=>({apiVersion:2,requestId:r.requestId,instanceId:'other',documentId:'document',contentRevision:8,ok:true,result:{applied:true,historyEntriesAdded:1}});
  assert.equal(await foreign.root.SM.toggleTimeRemap(1),false);
});

test('a second action while one is pending is consumed, and the entries stay native while the owner blocks legacy',async()=>{
  const f=fixture(video());f.api.bindTimeRemapEntries();
  const first=f.root.SM.toggleTimeRemap(1);
  assert.equal(await f.root.SM.toggleTimeRemap(1),false);
  assert.equal(await first,true);assert.equal(f.calls.length,1);
  f.api.disposeTimeRemapEntries();
  assert.notStrictEqual(f.root.SM.toggleTimeRemap,f.legacy.toggleTimeRemap,'an active owner keeps the refusing entries');
  assert.equal(await f.root.SM.toggleTimeRemap(1),false,'a retired binding never dispatches');
  f.setBlocks(false);f.setActive(false);f.api.disposeTimeRemapEntries();
  assert.strictEqual(f.root.SM.toggleTimeRemap,f.legacy.toggleTimeRemap);
  assert.strictEqual(f.root.SM.setFootageInterpMode,f.legacy.setFootageInterpMode);
});
