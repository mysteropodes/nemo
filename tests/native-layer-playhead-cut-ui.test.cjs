'use strict';
// Real cut UI child and projection; queue/transport/presentation delays are modeled.
// These controls establish no native kernel, installed UI, or async scripting parity.
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const Cut=require('../src/js/adapters/native-layer-playhead-cut-ui.js');
const ProjectFile=require('../src/js/adapters/native-object-project-file.js');
const Splice=require('../src/js/adapters/native-object-layer-splice.js');
const OP='command.document.object.layer.cut';
function deferred(){let resolve,reject;const promise=new Promise((a,b)=>{resolve=a;reject=b;});return {promise,resolve,reject};}
async function tick(){await Promise.resolve();await Promise.resolve();await Promise.resolve();}
function fixture(){
  let info={instanceId:'instance',documentId:'document',lifecycleGeneration:3,contentRevision:7,documentSnapshotId:'snapshot-7',
    canvasW:320,canvasH:180,fps:24,totalFrames:24,layers:[{layerUid:'a',name:'Ink'},{layerUid:'b',name:'Background'}]};
  const effects=[],calls=[],root={state:{},_layerSel:[],_layerSelAnchor:0,selectedPaths:[],NemoNativeObjectLayerSplice:Splice,
    SM:{splitLayerAtPlayhead(){effects.push('legacy');return 'legacy';}},showToast(message){effects.push(['notice',message]);}};
  ProjectFile.projectView(root,info);root.state.appMode='drawing';root.state.activeSymbolId=null;root.state.currentFrame=10;root._curFrame=10;
  let session,active=true,blocks=true,pending=false,current,subscription=2,serial=0;
  const control={dispatchGate:null,reconcileGate:null,receipt:null,failDispatch:null,failReconcile:null};
  function presented(){return {...info,available:true,kind:'native-object',presentation:{owner:'native',status:'presented',contextId:'scene-root',frame:root.state.currentFrame,
    instanceId:info.instanceId,documentId:info.documentId,lifecycleGeneration:info.lifecycleGeneration,contentRevision:info.contentRevision,documentSnapshotId:info.documentSnapshotId}};}
  session={isActive:()=>active,blocksLegacy:()=>blocks,info:()=>info,readPresentedObjectSession:()=>pending?null:presented(),
    readSubscriptionGeneration:()=>subscription,requestId:()=> 'cut-ui-'+(++serial),
    async runResourceCommand(action){
      assert.equal(pending,false,'child uses existing single resource queue');pending=true;effects.push('queue');
      const origin=presented();
      try{return await action({readPresentedObjectSession:()=>origin,
        async dispatch(request){calls.push(structuredClone(request));effects.push('dispatch');if(control.dispatchGate)await control.dispatchGate.promise;
          if(control.failDispatch)throw control.failDispatch;
          return control.receipt?control.receipt(request):{apiVersion:2,requestId:request.requestId,instanceId:info.instanceId,documentId:info.documentId,
            contentRevision:8,ok:true,result:{applied:true,historyEntriesAdded:1,layerUid:'native-layer:8:0'}};},
        async reconcile(revision){effects.push(['reconcile',revision]);if(control.reconcileGate)await control.reconcileGate.promise;if(control.failReconcile)throw control.failReconcile;
          info={...info,contentRevision:8,documentSnapshotId:'snapshot-8',layers:[info.layers[0],{layerUid:'native-layer:8:0',name:'Ink (2)'},...info.layers.slice(1)]};
          assert.equal(ProjectFile.refreshLayerTable(root,info),true,'actual project projection must admit middle native splice');return info;}});
      }finally{pending=false;effects.push('settled');}
    }};
  current=session;
  const ports={currentSession:()=>current,projectionReady:value=>ProjectFile.projectionReadyForUI(root,value),
    selectLayer(index,preserve,uid,expected){assert.equal(pending,false,'selection waits until resource queue settles');
      assert.strictEqual(current,session);assert.equal(expected.contentRevision,8);assert.equal(uid,'native-layer:8:0');
      effects.push(['select',index,uid]);root.state.activeLayerIdx=index;root._layerSel=[index];root._layerSelAnchor=index;return true;},
    repaint(){effects.push('repaint');return true;}};
  const previous=root.SM.splitLayerAtPlayhead,api=Cut.createPorts(root,ports);
  return {root,session,api,ports,previous,control,effects,calls,info:()=>info,
    setInfo:value=>{info=value;},replaceSession:()=>{current={...session};},setActive:value=>{active=value;},setBlocks:value=>{blocks=value;},
    changeSubscription:()=>{subscription++;},pending:()=>pending};
}
test('ordinary split entry dispatches once and selects returned UID only after real projection and presentation settle',async()=>{
  const f=fixture(),gate=deferred();f.control.reconcileGate=gate;
  assert.ok(f.api.bindCutEntries());const oldRows=f.root.state.layers.slice(),array=f.root.state.layers;
  const work=f.root.SM.splitLayerAtPlayhead();await tick();
  assert.equal(f.calls.length,1);assert.deepEqual(f.calls[0].payload,{command:'object.layer.cut',contextId:'scene-root',layerUid:'a',frame:10});
  assert.equal(f.calls[0].operation,OP);assert.equal(f.calls[0].expectedRevision,7);
  assert.equal(f.calls[0].instanceId,'instance');assert.equal(f.calls[0].documentId,'document');
  assert.equal(f.effects.some(value=>Array.isArray(value)&&value[0]==='select'),false);
  assert.equal(f.root.state.layers.length,2);assert.strictEqual(f.root.state.layers,array);
  assert.equal(await f.root.SM.splitLayerAtPlayhead(),false,'a second pending ordinary action is consumed');
  gate.resolve();assert.equal(await work,true);
  assert.strictEqual(f.root.state.layers,array);assert.strictEqual(f.root.state.layers[0],oldRows[0]);assert.strictEqual(f.root.state.layers[2],oldRows[1]);
  assert.deepEqual(f.root._layerSel,[1]);assert.equal(f.root.state.activeLayerIdx,1);
  assert.deepEqual(f.effects.filter(value=>Array.isArray(value)&&value[0]==='select'),[['select',1,'native-layer:8:0']]);
  assert.ok(f.effects.indexOf('settled')<f.effects.findIndex(value=>Array.isArray(value)&&value[0]==='select'));
  assert.equal(f.effects.includes('legacy'),false);
});
test('explicit context-menu layer uses stable native UID without changing prior selection optimistically',async()=>{
  const f=fixture(),gate=deferred();f.control.dispatchGate=gate;f.api.bindCutEntries();
  const work=f.root.SM.splitLayerAtPlayhead(1);await tick();
  assert.equal(f.calls.length,1);assert.equal(f.calls[0].payload.layerUid,'b');
  assert.equal(f.root.state.activeLayerIdx,0);assert.deepEqual(f.root._layerSel,[0]);
  // This fixture models a source-b split receipt in the correct native slot.
  f.control.receipt=request=>({apiVersion:2,requestId:request.requestId,instanceId:'instance',documentId:'document',contentRevision:7,ok:true,
    result:{applied:false,historyEntriesAdded:0}});
  gate.resolve();assert.equal(await work,false,'a correlated no-op is consumed without reporting an applied split');
  assert.equal(f.effects.some(value=>Array.isArray(value)&&['select','reconcile'].includes(value[0])),false);
  assert.equal(f.effects.includes('legacy'),false);
});
test('no-op preserves selection and redo-facing UI without reconcile or invented UID',async()=>{
  const f=fixture();f.control.receipt=request=>({apiVersion:2,requestId:request.requestId,instanceId:'instance',documentId:'document',contentRevision:7,ok:true,
    result:{applied:false,historyEntriesAdded:0}});f.api.bindCutEntries();
  const rows=f.root.state.layers,selected=f.root._layerSel;
  assert.equal(await f.root.SM.splitLayerAtPlayhead(),false);
  assert.strictEqual(f.root.state.layers,rows);assert.strictEqual(f.root._layerSel,selected);
  assert.equal(f.info().contentRevision,7);assert.equal(f.effects.some(value=>Array.isArray(value)&&['reconcile','select'].includes(value[0])),false);
});
test('refused malformed foreign and rejected receipts never select or invoke captured Paper writer',async()=>{
  const variants=[
    r=>({apiVersion:2,requestId:r.requestId,instanceId:'instance',documentId:'document',contentRevision:7,ok:false,error:{code:'unavailable',message:'No provider'}}),
    r=>({apiVersion:2,requestId:r.requestId,instanceId:'instance',documentId:'document',contentRevision:8,ok:true,result:{applied:true,historyEntriesAdded:0,layerUid:'native-layer:8:0'}}),
    r=>({apiVersion:2,requestId:'foreign',instanceId:'instance',documentId:'document',contentRevision:8,ok:true,result:{applied:true,historyEntriesAdded:1,layerUid:'native-layer:8:0'}}),
    r=>({apiVersion:2,requestId:r.requestId,instanceId:'foreign',documentId:'document',contentRevision:8,ok:true,result:{applied:true,historyEntriesAdded:1,layerUid:'native-layer:8:0'}}),
    r=>({apiVersion:2,requestId:r.requestId,instanceId:'instance',documentId:'foreign',contentRevision:8,ok:true,result:{applied:true,historyEntriesAdded:1,layerUid:'native-layer:8:0'}}),
    r=>({apiVersion:2,requestId:r.requestId,instanceId:'instance',documentId:'document',contentRevision:7,ok:true,result:{applied:true,historyEntriesAdded:1,layerUid:'native-layer:8:0'}}),
    r=>({apiVersion:2,requestId:r.requestId,instanceId:'instance',documentId:'document',contentRevision:8,ok:true,result:{applied:true,historyEntriesAdded:1,layerUid:'missing'}}),
  ];
  for(const receipt of variants){const f=fixture();f.control.receipt=receipt;f.api.bindCutEntries();assert.equal(await f.root.SM.splitLayerAtPlayhead(),false);
    assert.equal(f.effects.some(value=>Array.isArray(value)&&value[0]==='select'),false);assert.equal(f.effects.includes('legacy'),false);}
  for(const phase of ['failDispatch','failReconcile']){const f=fixture();f.control[phase]=Error('modeled '+phase);f.api.bindCutEntries();
    assert.equal(await f.root.SM.splitLayerAtPlayhead(),false);assert.equal(f.effects.includes('legacy'),false);
    assert.equal(f.effects.some(value=>Array.isArray(value)&&value[0]==='select'),false);}
});
test('delayed cut cannot overwrite newer frame selection owner or retired binding',async()=>{
  for(const change of ['frame','active','selection','owner','subscription','dispose']){
    const f=fixture(),gate=deferred();f.control.dispatchGate=gate;f.api.bindCutEntries();
    const work=f.root.SM.splitLayerAtPlayhead();await tick();assert.equal(f.calls.length,1);
    if(change==='frame')f.root.state.currentFrame=11;
    if(change==='active')f.root.state.activeLayerIdx=1;
    if(change==='selection')f.root._layerSel=[1];
    if(change==='owner')f.replaceSession();
    if(change==='subscription')f.changeSubscription();
    if(change==='dispose'){
      const bound=f.root.SM.splitLayerAtPlayhead;f.api.disposeCutEntries();
      assert.strictEqual(f.root.SM.splitLayerAtPlayhead,bound);assert.notStrictEqual(bound,f.previous);
      assert.equal(await bound(),false);assert.equal(f.calls.length,1,'retired pending entry cannot dispatch again');
    }
    gate.resolve();assert.equal(await work,false,change);
    assert.equal(f.effects.some(value=>Array.isArray(value)&&value[0]==='select'),false,change);
    assert.equal(f.effects.includes('legacy'),false,change);
  }
});
test('binding is idempotent and retirement restores only its captured entry',()=>{
  const f=fixture();f.api.bindCutEntries();const bound=f.root.SM.splitLayerAtPlayhead;
  f.api.bindCutEntries();assert.strictEqual(f.root.SM.splitLayerAtPlayhead,bound);
  f.setBlocks(false);f.api.disposeCutEntries();assert.strictEqual(f.root.SM.splitLayerAtPlayhead,f.previous);
  f.setBlocks(true);f.api.bindCutEntries();const newer=()=>false;f.root.SM.splitLayerAtPlayhead=newer;
  f.setBlocks(false);f.api.disposeCutEntries();assert.strictEqual(f.root.SM.splitLayerAtPlayhead,newer);
});
test('unpresented invalid UI and inactive owners refuse before dispatch without legacy fallback',async()=>{
  for(const change of ['inactive','unblocked','nonroot','frame','mode','selected','anchor','uid','projection','presentation','transport']){
    const f=fixture();f.api.bindCutEntries();
    if(change==='inactive')f.setActive(false);
    if(change==='unblocked')f.setBlocks(false);
    if(change==='nonroot')f.root.state.activeSymbolId='symbol';
    if(change==='frame')f.root.state.currentFrame=24;
    if(change==='mode')f.root.state.appMode='montage';
    if(change==='selected')f.root._layerSel=[0,0];
    if(change==='anchor')f.root._layerSelAnchor=99;
    if(change==='uid')f.root.state.layers[0].layerUid='foreign';
    if(change==='projection')f.ports.projectionReady=()=>false;
    if(change==='presentation')f.session.readPresentedObjectSession=()=>null;
    if(change==='transport')f.session.readSubscriptionGeneration=()=>null;
    assert.equal(await f.root.SM.splitLayerAtPlayhead(),false,change);
    assert.deepEqual(f.calls,[],change);assert.equal(f.effects.includes('legacy'),false,change);
  }
  for(const index of [-1,0.5,2,'0']){const f=fixture();f.api.bindCutEntries();assert.equal(await f.root.SM.splitLayerAtPlayhead(index),false);assert.deepEqual(f.calls,[]);}
});
test('real Cmd Shift D and context-menu callers consume the bound async split without follow-up legacy writes',async()=>{
  const source=fs.readFileSync(path.join(__dirname,'../src/js/timeline.js'),'utf8');
  const marker="if((event.metaKey||event.ctrlKey)&&event.shiftKey&&(event.key==='d'||event.key==='D')){";
  const start=source.indexOf(marker);assert.ok(start>=0);
  const end=source.indexOf('\n  }',start);assert.ok(end>start);
  const keyHandler=source.slice(start,end+'\n  }'.length);
  const f=fixture();f.api.bindCutEntries();let prevented=0;
  const context={window:f.root,event:{metaKey:true,ctrlKey:false,shiftKey:true,key:'D',preventDefault(){prevented++;}}};
  vm.createContext(context);vm.runInContext('(function(){'+keyHandler+'})()',context);await tick();await tick();
  assert.equal(prevented,1);assert.equal(f.calls.length,1);assert.equal(f.effects.includes('legacy'),false);
  const action='action:function(){window.SM.splitLayerAtPlayhead(idx4);}';assert.ok(source.includes(action));
  const g=fixture();g.api.bindCutEntries();const menu={window:g.root,idx4:1};vm.createContext(menu);
  vm.runInContext('('+action.slice('action:'.length)+')()',menu);await tick();await tick();
  assert.equal(g.calls.length,1);assert.equal(g.calls[0].payload.layerUid,'b');assert.equal(g.effects.includes('legacy'),false);
});
test('synchronous scripting refuses native mode before playhead layer read or dispatch and preserves ordinary immediate result',()=>{
  const effects=[],ports={nativeBlocked:()=>true,layers(){effects.push('layers');return [];},move(){effects.push('move');},
    split(){effects.push('split');},layer(){effects.push('layer');},fail(message){effects.push(['fail',message]);throw Error(message);}};
  assert.throws(()=>Cut.splitSynchronous(0,10,ports),/asynchronous scripting/);
  assert.deepEqual(effects,[['fail','native split requires asynchronous scripting support']]);
  const rows=[{uid:'old'}],newLayer={uid:'copy'};effects.length=0;
  Object.assign(ports,{nativeBlocked:()=>false,layers:()=>rows,move:frame=>effects.push(['move',frame]),
    split:index=>{effects.push(['split',index]);rows.splice(index+1,0,newLayer);},layer:index=>{effects.push(['layer',index]);return rows[index];}});
  assert.strictEqual(Cut.splitSynchronous(0,10.9,ports),newLayer);
  assert.deepEqual(effects,[['move',10],['split',0],['layer',1]]);
  ports.split=()=>{};assert.throws(()=>Cut.splitSynchronous(0,null,ports),/coupe impossible/);
});
test('composed lifecycle binds and retires cut alongside existing policy ports without replacing policy API',()=>{
  const f=fixture(),effects=[],policy={marker:17,bindPolicyEntries(){effects.push('bind-policy');return true;},disposePolicyEntries(){effects.push('dispose-policy');}};
  const joined=Cut.compose(f.root,f.ports,policy);assert.equal(joined.marker,17);
  assert.equal(joined.bindPolicyEntries(),true);assert.notStrictEqual(f.root.SM.splitLayerAtPlayhead,f.previous);
  f.setBlocks(false);joined.disposePolicyEntries();assert.strictEqual(f.root.SM.splitLayerAtPlayhead,f.previous);
  assert.deepEqual(effects,['bind-policy','dispose-policy']);
});
test('active and indeterminate native disposal retains a refusing entry until legitimate retirement',async()=>{
  for(const active of [true,false]){
    const f=fixture();f.setActive(active);f.api.bindCutEntries();const bound=f.root.SM.splitLayerAtPlayhead;
    f.api.disposeCutEntries();assert.strictEqual(f.root.SM.splitLayerAtPlayhead,bound);
    assert.equal(await bound(),false);assert.deepEqual(f.calls,[]);assert.equal(f.effects.includes('legacy'),false);
    f.api.bindCutEntries();assert.strictEqual(f.root.SM.splitLayerAtPlayhead,bound,'blocked retirement cannot install a new live entry');
    f.setBlocks(false);f.api.disposeCutEntries();assert.strictEqual(f.root.SM.splitLayerAtPlayhead,f.previous);
    assert.equal(f.root.SM.splitLayerAtPlayhead(),'legacy','the captured browser handler is restored only after native blocking ends');
    assert.deepEqual(f.effects,['legacy']);
  }
});

// Reuse the existing actual Project/Files/UI desktop harness, retaining its
// modeled native IPC boundary. Its original tests are never evaluated here.
function actualDesktopFixture(ProjectOverride){
  const filename=path.join(__dirname,'native-object-ui.test.cjs'),source=fs.readFileSync(filename,'utf8');
  const prefixEnd=source.indexOf("test('metadata presenter");
  const start=source.indexOf('function desktop(initial=[A,B]){'),end=source.indexOf("\ntest('actual Open/New",start);
  assert.ok(prefixEnd>0&&start>prefixEnd&&end>start);
  const localRequire=require('node:module').createRequire(filename);
  const context={require(name){
    if(ProjectOverride&&name==='../src/js/adapters/native-object-project.js')return ProjectOverride;
    if(ProjectOverride&&name==='../src/js/bootstrap/native-opacity-application.js')return {bindObjectUI:installBrowserBootstrap};
    return localRequire(name);
  },
    console,setImmediate,setTimeout,clearTimeout};
  vm.createContext(context);
  vm.runInContext(source.slice(0,prefixEnd)+'\n'+source.slice(start,end)+'\nglobalThis.makeDesktop=desktop;',context);
  return context.makeDesktop();
}
function installBrowserBootstrap(root){
  const deferred=[];
  root.NemoNativeOpacityLegacySurface=require('../src/js/adapters/native-opacity-legacy-surface.js');
  root.setTimeout=(callback,delay)=>{assert.equal(delay,0);deferred.push(callback);return deferred.length;};
  // Keep the bootstrap's window identity equal to the fixture's actual root.
  const bootstrapContext=vm.createContext({globalThis:root});
  vm.runInContext(fs.readFileSync(path.join(__dirname,'../src/js/bootstrap/native-opacity-application.js'),'utf8'),bootstrapContext);
  assert.equal(deferred.length,1,'actual classic bootstrap defers unrelated opacity activation without executing it');
}
function browserProjectBeforeCut(){
  const context={NemoNativeObjectProjectMetadata:require('../src/js/adapters/native-object-project-metadata.js'),
    NemoNativeObjectProjectDesktop:require('../src/js/adapters/native-object-project-desktop.js'),
    NemoNativeObjectProjectProperties:require('../src/js/adapters/native-object-project-properties.js'),
    NemoNativeFrameEditContract:require('../src/js/adapters/native-frame-edit-contract.js'),
    NemoNativeObjectOpenExposure:require('../src/js/adapters/native-object-open-exposure.js')};
  vm.createContext(context);
  vm.runInContext(fs.readFileSync(path.join(__dirname,'../src/js/adapters/native-object-project.js'),'utf8'),context);
  assert.equal(context.NemoNativeLayerPlayheadCutUI,undefined,'actual Project classic script loads before optional cut UI');
  return context.NemoNativeObjectProject;
}
test('actual CJS Project paint binds cut without a CutUI browser global on the real UI and File root',async()=>{
  const Project=require('../src/js/adapters/native-object-project.js'),h=actualDesktopFixture();
  let legacy=0;const original=()=>{legacy++;return 'legacy';};h.root.SM.splitLayerAtPlayhead=original;
  assert.equal(Object.hasOwn(h.root,'NemoNativeLayerPlayheadCutUI'),false);
  await h.open();assert.doesNotThrow(()=>Project.paintUI(h.root));
  assert.notStrictEqual(h.root.SM.splitLayerAtPlayhead,original);
  assert.equal(await h.root.SM.splitLayerAtPlayhead(-1),false);
  assert.equal(legacy,0);assert.equal(h.session.isActive(),true);
});
test('actual classic Project resolves a late cut child lazily and missing child never exposes a Paper split',async()=>{
  const Project=browserProjectBeforeCut(),h=actualDesktopFixture(Project);let legacy=0;
  const original=()=>{legacy++;return 'legacy';};h.root.SM.splitLayerAtPlayhead=original;
  assert.equal(h.root.NemoNativeLayerPlayheadCutUI,undefined);
  h.root.TextEncoder=TextEncoder;vm.createContext(h.root);
  vm.runInContext(fs.readFileSync(path.join(__dirname,'../src/js/adapters/native-layer-playhead-cut-contract.js'),'utf8'),h.root);
  vm.runInContext(fs.readFileSync(path.join(__dirname,'../src/js/adapters/native-layer-playhead-cut-ui.js'),'utf8'),h.root);
  assert.equal(typeof h.root.NemoNativeLayerPlayheadCutUI.compose,'function');
  await h.open();assert.doesNotThrow(()=>Project.paintUI(h.root));
  assert.notStrictEqual(h.root.SM.splitLayerAtPlayhead,original);
  assert.equal(await h.root.SM.splitLayerAtPlayhead(-1),false);assert.equal(legacy,0);
  const missingProject=browserProjectBeforeCut(),missing=actualDesktopFixture(missingProject);let paper=0;
  const paperSplit=()=>{paper++;return 'legacy';};missing.root.SM.splitLayerAtPlayhead=paperSplit;
  const Legacy=require('../src/js/adapters/native-opacity-legacy-surface.js');
  Legacy.wrapWriter(missing.root.SM,'splitLayerAtPlayhead',()=>!missingProject.desktop(missing.root).blocksLegacy(),'timeline-edit');
  const denied=missing.root.SM.splitLayerAtPlayhead;
  const operations=fs.readFileSync(path.join(__dirname,'../src/js/application/native-opacity-operations.js'),'utf8');
  assert.ok(operations.includes("'splitLayerAtPlayhead'"),'actual ordinary writer-denial list retains split');
  assert.equal(missing.root.NemoNativeLayerPlayheadCutUI,undefined);
  await missing.file.openPath('/native.json');
  assert.equal(missing.session.blocksLegacy(),true,'missing native child cannot restore browser authority');
  assert.throws(()=>missingProject.paintUI(missing.root),/unavailable|compose|undefined|presentation/);
  assert.strictEqual(missing.root.SM.splitLayerAtPlayhead,denied,'failed binding preserves the actual existing denial wrapper');
  assert.notStrictEqual(denied,paperSplit);assert.equal(await missing.root.SM.splitLayerAtPlayhead(),false);assert.equal(paper,0);
});

// Only the existing harness's native IPC model is extended. The real Project
// queue, common SDK, transport, metadata projection and presenter stay intact.
function actualMalformedAdvancingCutFixture(failInspection){
  const filename=path.join(__dirname,'native-object-ui.test.cjs'),source=fs.readFileSync(filename,'utf8');
  const prefixEnd=source.indexOf("test('metadata presenter"),start=source.indexOf('function desktop(initial=[A,B]){');
  const end=source.indexOf("\ntest('actual Open/New",start);
  assert.ok(prefixEnd>0&&start>prefixEnd&&end>start);
  let desktop=source.slice(start,end);
  const marker="    if(command==='nemo_native_object_reference')return {...meta(),requestId:r.requestId};";
  assert.equal(desktop.split(marker).length,2,'one actual IPC inspection branch');
  const cutModel=`    if(command==='nemo_native_dispatch'&&r.operation==='command.document.object.layer.cut'){
      assert.equal(r.expectedRevision,revision);assert.equal(r.payload.frame,10);
      assert.equal(r.payload.layerUid,layers[0].layerUid);
      undo.push(clone(layers));redo=[];
      layers.splice(1,0,{layerUid:'native-layer:cut:1',name:'Ink (2)',visible:true,locked:false});revision++;
      log.push('malformed-cut-committed:'+revision);
      return malformedCutReceipt(r,revision);
    }
    if(command==='nemo_native_object_reference'&&revision>0){
      log.push('inspect-after-uncertain-cut');
      if(failInspection)throw Error('Modeled inspection failure after native commit');
    }
`;
  desktop=desktop.replace(marker,cutModel+marker);
  function malformedCutReceipt(request,revision){
    // Native IPC produces ordinary values in the SDK realm, not VM prototypes.
    return {apiVersion:2,requestId:request.requestId,instanceId:request.instanceId,documentId:request.documentId,
      contentRevision:revision,ok:true,result:{applied:true,historyEntriesAdded:0,layerUid:'native-layer:cut:1'}};
  }
  const localRequire=require('node:module').createRequire(filename),context={require:localRequire,console,setImmediate,setTimeout,clearTimeout,
    failInspection,malformedCutReceipt};
  vm.createContext(context);
  vm.runInContext(source.slice(0,prefixEnd)+'\n'+desktop+'\nglobalThis.makeDesktop=desktop;',context);
  const h=context.makeDesktop();let paper=0;
  h.root.SM.splitLayerAtPlayhead=()=>{paper++;return 'legacy';};h.root.state.appMode='drawing';
  return {...h,paper:()=>paper};
}
test('post-transport common SDK rejection reconciles actual advanced owner before any old presentation becomes ready',async()=>{
  const h=actualMalformedAdvancingCutFixture(false);await h.open();h.root.state.currentFrame=10;
  await h.session.presentPreview(10);const prior=h.session.readPresentedObjectSession(),selection=h.root._layerSel.slice();
  assert.ok(prior);assert.equal(prior.contentRevision,0);
  assert.equal(await h.root.SM.splitLayerAtPlayhead(),false,'malformed result never claims a selected successful cut');
  assert.equal(h.meta().contentRevision,1,'native model really committed before SDK rejected the malformed result');
  assert.equal(h.session.info().contentRevision,1,'existing resource reconcile inspects the authoritative advanced revision');
  assert.ok(h.log.includes('inspect-after-uncertain-cut'));
  assert.equal(h.root.state.layers.length,3,'actual projection applies the real adjacent UID insertion');
  assert.deepEqual(Array.from(h.root._layerSel),Array.from(selection),'uncertainty does not select the new half');
  const final=h.session.readPresentedObjectSession();
  assert.ok(final);assert.equal(final.contentRevision,1);assert.equal(final.presentation.contentRevision,1);
  assert.notEqual(final.documentSnapshotId,prior.documentSnapshotId);assert.equal(final.presentation.frame,10);
  assert.equal(h.calls.filter(call=>call.request&&call.request.operation===OP).length,1,'the real transport dispatches once');
  assert.equal(h.log.filter(entry=>entry==='malformed-cut-committed:1').length,1,'no mutation retry');
  assert.equal(h.paper(),0);
});
test('failed inspection after post-transport SDK rejection fences existing owner rather than exposing its old pin',async()=>{
  const h=actualMalformedAdvancingCutFixture(true);await h.open();h.root.state.currentFrame=10;await h.session.presentPreview(10);
  assert.equal(h.session.readPresentedObjectSession().contentRevision,0);
  assert.equal(await h.root.SM.splitLayerAtPlayhead(),false);
  assert.equal(h.meta().contentRevision,1);assert.equal(h.session.isIndeterminate(),true);
  assert.equal(h.session.blocksLegacy(),true);assert.equal(h.session.readPresentedObjectSession(),null);
  assert.ok(h.log.includes('inspect-after-uncertain-cut'));assert.equal(h.paper(),0);
  assert.equal(await h.root.SM.splitLayerAtPlayhead(),false,'indeterminate owner cannot retry the mutation');
  assert.equal(h.log.filter(entry=>entry==='malformed-cut-committed:1').length,1);
});

test('actual bootstrap composition is per-root idempotent and refuses replaced document or registration',async()=>{
  const Bootstrap=require('../src/js/bootstrap/native-opacity-application.js');
  const Desktop=require('../src/js/adapters/native-object-project-desktop.js');
  const Project=require('../src/js/adapters/native-object-project.js');
  const a=actualDesktopFixture(),b=actualDesktopFixture();
  assert.notStrictEqual(a.root,b.root);assert.equal(Bootstrap.bindObjectUI(a.root),true);
  await a.open();await b.open();assert.equal(Project.paintUI(a.root),true);assert.equal(Project.paintUI(b.root),true);
  assert.throws(()=>Project.bindUIComposition(a.root,()=>{}),/already bound/);
  const document=a.root.document;a.root.document={};
  assert.throws(()=>Bootstrap.bindObjectUI(a.root),/binding changed/);
  assert.throws(()=>Desktop.composeUI(a.root,{},{}),/unavailable/);
  a.root.document=document;assert.equal(Bootstrap.bindObjectUI(a.root),true);
  assert.equal(Project.paintUI(a.root),true);assert.equal(Project.paintUI(b.root),true);
});
