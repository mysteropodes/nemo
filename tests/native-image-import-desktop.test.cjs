'use strict';
// Actual images.js/menu/router/desktop/client/command/SDK; controlled dialog, transport and native projection boundary.
// UI/contract proof only: no native decode, GPU or installed proof.
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const Desktop=require('../src/js/adapters/native-import-desktop.js'),Drop=require('../src/js/adapters/native-import-drop.js');
const Raster=require('../src/js/adapters/native-object-raster-asset.js'),SDK=require('../src/js/adapters/native-application.js');
const deferred=()=>{let resolve;return {promise:new Promise(r=>{resolve=r;}),resolve:v=>resolve(v)};};
const turn=()=>new Promise(setImmediate);
function fixture(options={}) {
 let pin={instanceId:'instance',documentId:'document',lifecycleGeneration:1,contentRevision:0,documentSnapshotId:'native-object:document:0',totalFrames:21,
  layers:[{layerUid:'drawing',name:'Drawing',visible:true,locked:false}]},count=0,serial=Promise.resolve(),session;
 const events=new Map(),toasts=[],requests=[],picks=[],receipts=new Map();let first=true,projected=true;
 const state={currentFrame:0,activeLayerIdx:0,layers:[{layerUid:'drawing'}]},root={state,_layerSel:[0],SM:{t:k=>k},showToast:s=>toasts.push(s)};
 function makeSession(){return {
  info:()=>pin,isActive:()=>true,blocksLegacy:()=>true,requestId:()=> 'png-'+(++count),
  importBatch:()=>assert.fail('image import must not use vector import.batch'),readSubscriptionGeneration:()=>1,
  readPresentedObjectSession:()=>({available:projected,...pin,presentation:{...pin,owner:'native',status:projected?'presented':'pending',frame:state.currentFrame}}),
  runResourceCommand:action=>{const result=serial.then(()=>options.queue?options.queue.promise:undefined).then(()=>action({
   dispatch:q=>SDK.createNativeApplicationAdapter('ui',{dispatch:async req=>{
    requests.push(structuredClone(req));
    if(options.nativeRefusal)return {apiVersion:2,requestId:req.requestId,instanceId:req.instanceId,documentId:req.documentId,contentRevision:pin.contentRevision,ok:false,
     error:{code:'unavailable',message:'native import disposition is unresolved',details:options.nativeRefusal}};
    let response=receipts.get(req.requestId);
    if(!response){const next=req.expectedRevision+1;response={apiVersion:2,requestId:req.requestId,instanceId:req.instanceId,documentId:req.documentId,contentRevision:next,ok:true,
     result:{applied:true,historyEntriesAdded:1,assetId:'canonical-existing-png',naturalWidth:2,naturalHeight:1,displayWidth:2,displayHeight:1,
      target:{contextId:'scene-root',layerUid:'native-layer:'+next+':0',frameScope:{kind:'authored',frame:0},strokeId:'native-raster:'+next+':0'},newLayer:true}};
     receipts.set(req.requestId,structuredClone(response));pin={...pin,contentRevision:next,documentSnapshotId:'native-object:document:'+next,
      layers:[...pin.layers,{layerUid:response.result.target.layerUid,name:req.payload.name,visible:true,locked:false}]};state.layers=pin.layers.map(r=>({layerUid:r.layerUid}));}
    if(options.loseAlways||(options.loseBoth&&requests.length<=2))throw Error('lost both exact replies');
    if(options.lostAck&&first){first=false;throw Error('lost ACK');}return structuredClone(response);
   }}).dispatch(q),
   readPresentedObjectSession:()=>session.readPresentedObjectSession(),
   reconcile:async revision=>{if(revision!==undefined)assert.equal(revision,pin.contentRevision);if(options.unconfirmed)projected=false;return pin;},
  }));serial=result.catch(()=>{});return result;},
 };}
 session=makeSession();
 root.NemoNativeObjectProject={desktop:()=>session,selectLayerUI:(host,index)=>{if(!options.consumeOnly){state.activeLayerIdx=index;root._layerSel=[index];}return true;}};
 root.NemoNativeObjectProjectFile={projectionReadyForUI:()=>projected};root.NemoNativeObjectRasterAsset=Raster;
 root.NemoNativeImportCore=require('../src/js/adapters/native-import-core.js');root.NemoNativeImportDrop=Drop;
 for(const [key,name]of Object.entries({NemoNativeImportSvg:'svg',NemoNativeImportFigma:'figma',NemoNativeImportPsd:'psd',NemoNativeImportMontage:'montage'}))root[key]=require('../src/js/adapters/native-import-'+name+'.js');
 root.__TAURI__={dialog:{open:async opts=>{picks.push(opts);return options.pick?options.pick(): '/picked/My Still.PNG';}},fs:{readFile:()=>assert.fail('no JS bytes'),readTextFile:()=>assert.fail('no JS image text')}};
 const nodes=new Map();const node=id=>{if(!nodes.has(id))nodes.set(id,{value:'',addEventListener:(t,f)=>events.set(id+':'+t,f),removeEventListener(){},click:()=>assert.fail('never trigger DOM image picker')});return nodes.get(id);};
 ['image-input','video-input','btn-import-img','btn-import-video'].forEach(node);
 const document={readyState:'loading',getElementById:id=>nodes.get(id)||null,addEventListener:(t,f)=>events.set('document:'+t,f),removeEventListener(){}};
 root.document=document;root.paper={project:{}};
 const controller=Desktop.install(root,options.declared===false?{imageRoute:false}:undefined);
 vm.runInContext(fs.readFileSync(path.join(__dirname,'../src/js/images.js'),'utf8'),vm.createContext({window:root,document,state,SM:root.SM,showToast:root.showToast,console,
  FileReader:function(){assert.fail('no JS decoder');},Image:function(){assert.fail('no JS decoder');},
  saveAllLayerFrames:()=>assert.fail('no legacy save'),pushUndoLayers:()=>assert.fail('no legacy undo'),createUserLayer:()=>assert.fail('no legacy writer'),
  localStorage:{getItem:()=>null},setTimeout,clearTimeout}));
 return {root,state,controller,requests,picks,toasts,events,
  menu:()=>events.get('btn-import-img:click')(),change:next=>{pin={...pin,...next};},
  replace:()=>{pin={...pin,lifecycleGeneration:pin.lifecycleGeneration+1};session=makeSession();},
  select:()=>{root._layerSel=[];},show:()=>{projected=true;},
 };
}
test('explicitly disabled image route refuses before picker',async()=>{
 const h=fixture({declared:false});assert.equal((await h.menu()).status,'unavailable');assert.equal(h.picks.length,0);assert.equal(h.requests.length,0);
});
test('ordinary menu uses one scoped PNG picker, actual command/SDK and verified presented selection',async()=>{
 const h=fixture();const result=await h.menu();assert.equal(result.status,'imported',JSON.stringify(result));assert.equal(result.readback,'confirmed');assert.equal(result.selected,true);
 assert.deepEqual(h.picks,[{title:'Import Image',multiple:false,filters:[{name:'PNG Image',extensions:['png']}]}]);
 assert.equal(h.requests.length,1);const q=h.requests[0];assert.equal(q.operation,'command.document.object.asset.attach');
 assert.equal(q.payload.name,'My Still.PNG');assert.equal(q.payload.source.path,'/picked/My Still.PNG');assert.deepEqual(q.payload.placement,{kind:'newLayer'});
 assert.equal(result.receipt.assetId,'canonical-existing-png');assert.deepEqual(h.root._layerSel,[1]);
 const second=await h.root.SM.importImages();assert.equal(second.status,'imported');assert.equal(h.requests.length,2);assert.equal(second.receipt.assetId,'canonical-existing-png');
});
test('cancelled picker imports nothing and concurrent second call is busy',async()=>{
 const gate=deferred(),h=fixture({pick:()=>gate.promise});const pending=h.menu();await turn();assert.equal((await h.menu()).status,'busy');
 gate.resolve(null);assert.equal((await pending).status,'cancelled');assert.equal(h.requests.length,0);
});
test('frame, selection, revision, lifecycle and session changes while picking refuse before dispatch',async()=>{
 for(const kind of ['frame','selection','revision','lifecycle','session']){
  const gate=deferred(),h=fixture({pick:()=>gate.promise}),pending=h.menu();await turn();
  if(kind==='frame')h.state.currentFrame=1;if(kind==='selection')h.select();if(kind==='revision')h.change({contentRevision:1,documentSnapshotId:'native-object:document:1'});
  if(kind==='lifecycle')h.change({lifecycleGeneration:2});if(kind==='session')h.replace();
  gate.resolve('/picked.png');assert.equal((await pending).status,'stale',kind);assert.equal(h.requests.length,0,kind);
 }
});
test('queued command checks picker intent again after another mutation',async()=>{
 const queue=deferred(),h=fixture({queue});const pending=h.menu();await turn();h.state.currentFrame=1;queue.resolve();
 assert.equal((await pending).status,'stale');assert.equal(h.requests.length,0);
});
test('lost ACK retries the same SDK request without another placement',async()=>{
 const h=fixture({lostAck:true});const result=await h.menu();assert.equal(result.status,'imported',JSON.stringify(result));assert.equal(h.requests.length,2);
 assert.deepEqual(h.requests[0],h.requests[1]);assert.equal(result.receipt.contentRevision,1);assert.equal(h.state.layers.length,2);
});
test('acknowledgement without presentation is unconfirmed and preserves selection',async()=>{
 const h=fixture({unconfirmed:true});const result=await h.menu();assert.equal(result.status,'unconfirmed');assert.ok(result.receipt);assert.deepEqual(h.root._layerSel,[0]);
});
test('selection consumption alone is never reported as successful selection',async()=>{
 const h=fixture({consumeOnly:true});const result=await h.menu();assert.equal(result.status,'imported',JSON.stringify(result));assert.equal(result.selected,false);
});
test('opaque DOM files/drop and video stay unavailable with PNG menu route',async()=>{
 const h=fixture();const opaque={name:'still.png',type:'image/png',get path(){assert.fail('no File.path');},arrayBuffer(){assert.fail('no JS bytes');}};
 assert.equal((await h.root.SM.importImageFiles([opaque])).status,'unavailable');assert.equal((await h.controller.drop.drop([opaque])).status,'unavailable');
 assert.equal((await Drop.desktopMedia(h.root,null,'video')).status,'unavailable');assert.equal(h.picks.length,0);assert.equal(h.requests.length,0);
});
test('dispose during picker revokes intent before dispatch',async()=>{
 const gate=deferred(),h=fixture({pick:()=>gate.promise}),pending=h.menu();await turn();h.controller.dispose();gate.resolve('/picked.png');
 assert.equal((await pending).status,'unavailable');assert.equal(h.requests.length,0);
});
test('reopened native project binds a new client with current lifecycle',async()=>{
 const h=fixture();assert.equal((await h.menu()).status,'imported');h.replace();assert.equal((await h.menu()).status,'imported');assert.equal(h.requests[1].payload.lifecycleGeneration,2);
});

test('both missing replies latch the exact owner/request and ordinary menu cannot issue a new picker or placement',async()=>{
 const h=fixture({loseBoth:true});const first=await h.menu();assert.equal(first.status,'unconfirmed');assert.equal(first.code,'asset_command_indeterminate');
 assert.equal(first.requestId,'png-1');assert.equal(first.recovery,'exact-request');assert.equal(h.requests.length,2);assert.deepEqual(h.requests[0],h.requests[1]);
 assert.deepEqual(first.owner,{instanceId:'instance',documentId:'document',lifecycleGeneration:1});
 const second=await h.menu();assert.equal(second.requestId,'png-1');assert.equal(h.requests.length,2);assert.equal(h.picks.length,1);assert.equal(h.state.layers.length,2);
});
test('explicit exact recovery retains full body/identity, creates once and does not steal selection',async()=>{
 const h=fixture({loseBoth:true});await h.menu();const recovered=await h.controller.image.recover();assert.equal(recovered.status,'imported');assert.equal(recovered.selected,false);
 assert.equal(h.picks.length,1);assert.equal(h.requests.length,3);assert.deepEqual(h.requests[2],h.requests[0]);assert.equal(h.state.layers.length,2);assert.equal(h.controller.image.status(),null);
 assert.equal((await h.menu()).status,'imported');assert.equal(h.requests[3].requestId,'png-2');assert.equal(h.picks.length,2);
});
test('repeated uncertain recovery stays latched and cannot allocate another request',async()=>{
 const options={loseAlways:true},h=fixture(options);await h.menu();const held=await h.controller.image.recover();assert.equal(held.status,'unconfirmed');
 assert.equal(h.requests.length,4);for(const q of h.requests)assert.deepEqual(q,h.requests[0]);assert.equal(h.state.layers.length,2);
 await h.menu();assert.equal(h.requests.length,4);assert.equal(h.picks.length,1);options.loseAlways=false;
 assert.equal((await h.controller.image.recover()).status,'imported');assert.equal(h.requests.length,5);assert.deepEqual(h.requests[4],h.requests[0]);assert.equal(h.state.layers.length,2);
});
test('native pending/unknown/committed dispositions survive typed refusal and require exact resolution or owner reopen',async()=>{
 for(const details of [{disposition:'indeterminate',committed:null,retryExecution:false},{disposition:'pending',committed:false,retryExecution:false},{disposition:'unknown',committed:null,retryExecution:false},{disposition:'indeterminate',committed:true,retryExecution:false}]){
  const options={nativeRefusal:details},h=fixture(options);const first=await h.menu();assert.equal(first.status,'unconfirmed');assert.deepEqual(first.details,details);assert.equal(first.recovery,'reopen-owner');
  await h.menu();await h.controller.image.recover();assert.equal(h.requests.length,1);assert.equal(h.picks.length,1);
  options.nativeRefusal=null;await h.menu();assert.equal(h.requests.length,1,'a server becoming healthy does not erase its prior unknown receipt');
  h.replace();assert.equal((await h.menu()).status,'imported');assert.equal(h.requests[1].payload.lifecycleGeneration,2);
 }
});
test('ordinary revision/frame/selection changes never clear an uncertain owner latch',async()=>{
 const h=fixture({loseBoth:true});await h.menu();h.change({contentRevision:7,documentSnapshotId:'native-object:document:7'});h.state.currentFrame=1;h.select();
 assert.equal((await h.menu()).status,'unconfirmed');assert.equal(h.requests.length,2);assert.equal(h.picks.length,1);
 h.replace();assert.equal((await h.menu()).status,'imported');assert.equal(h.picks.length,2);assert.equal(h.requests[2].payload.lifecycleGeneration,2);
});
test('known native receipt with unconfirmed presentation stays latched until actual presentation without any second request',async()=>{
 const h=fixture({unconfirmed:true});const first=await h.menu();assert.equal(first.status,'unconfirmed');assert.equal(first.requestId,'png-1');assert.equal(first.recovery,'presentation');
 await h.menu();assert.equal(h.requests.length,1);assert.equal(h.picks.length,1);h.show();
 const recovered=await h.controller.image.recover();assert.equal(recovered.status,'imported');assert.equal(recovered.selected,false);assert.equal(h.requests.length,1);assert.equal(h.controller.image.status(),null);
});
test('known not-dispatched native refusal does not become an uncertain commit',async()=>{
 const h=fixture({nativeRefusal:{disposition:'notDispatched',committed:false,retryExecution:false}});assert.equal((await h.menu()).status,'unavailable');assert.equal(h.controller.image.status(),null);
});

test('temporarily missing or empty owner information never proves an explicit reopen',async()=>{
 const h=fixture({loseBoth:true});await h.menu();h.change({instanceId:'',documentId:'',lifecycleGeneration:0});
 assert.equal((await h.menu()).status,'unconfirmed');assert.equal(h.requests.length,2);assert.equal(h.picks.length,1);
 h.change({instanceId:'instance',documentId:'document',lifecycleGeneration:1});
 assert.equal((await h.controller.image.recover()).status,'imported');assert.deepEqual(h.requests[2],h.requests[0]);
});

// Regression of the installed ac0 failure: production Project owns pending/presentation,
// subscription tickets, reconciliation and serialization. Only native I/O and DOM are controlled.
const Project=require('../src/js/adapters/native-object-project.js'),Files=require('../src/js/adapters/native-object-project-file.js');
async function realProjectFixture(options={}) {
 const events=new Map(),requests=[],picks=[],toasts=[],state={},root={state,_layerSel:[],SM:{t:k=>k},paper:{project:{}},showToast:s=>toasts.push(s)};
 let installed=false,revision=0,generation=1,documentId='document',subscribed=true,previewSerial=0,extra=[],sync,disconnected,binding,session,surface;
 const metadata=()=>({apiVersion:2,instanceId:'instance',documentId,lifecycleGeneration:generation,contentRevision:revision,
  documentSnapshotId:`snapshot-${documentId}-${revision}`,documentDigest:String(revision%10).repeat(64),byteCount:1000+revision,
  canvasW:320,canvasH:180,fps:24,totalFrames:21,layers:[{layerUid:'drawing',name:'Drawing',visible:true,locked:false},...extra],reference:null,undoDepth:revision,redoDepth:0});
 async function invoke(command,args) {
  const r=args&&JSON.parse(args.requestJson);
  if(command==='nemo_native_object_client_status')return {...metadata(),available:installed};
  if(command==='nemo_native_object_bootstrap'){
   if(installed){generation++;documentId='document-'+generation;revision=0;extra=[];}
   installed=true;return {...metadata(),requestId:r.requestId,resourceCount:0,viewportAvailable:false};
  }
  if(command==='nemo_native_object_reference'&&r.action.kind==='inspect')return {...metadata(),requestId:r.requestId};
  if(command==='nemo_native_object_preview')return {status:'presented',workId:'work-'+(++previewSerial),viewGeneration:1};
  if(command==='nemo_native_object_dispatch'){
   assert.equal(r.expectedRevision,revision);revision++;
   return {apiVersion:2,requestId:r.requestId,instanceId:'instance',documentId,contentRevision:revision,ok:true,result:{applied:true,historyEntriesAdded:1}};
  }
  assert.fail('unexpected native boundary: '+command);
 }
 const sdk=SDK.createNativeApplicationAdapter('ui',{dispatch:async request=>{
  assert.equal(session.readPresentedObjectSession(),null,'the public presentation MUST remain hidden inside the actual queue');
  assert.equal(request.expectedRevision,revision);assert.equal(request.documentId,documentId);
  requests.push(structuredClone(request));
  if(options.dispatchGate)await options.dispatchGate.promise;
  revision++;const layerUid='native-layer:'+revision+':0';extra.push({layerUid,name:request.payload.name,visible:true,locked:false});
  return {apiVersion:2,requestId:request.requestId,instanceId:'instance',documentId,contentRevision:revision,ok:true,
   result:{applied:true,historyEntriesAdded:1,assetId:'canonical-png',naturalWidth:2,naturalHeight:1,displayWidth:2,displayHeight:1,
    target:{contextId:'scene-root',layerUid,frameScope:{kind:'authored',frame:0},strokeId:'native-raster:'+revision+':0'},newLayer:true}};
 }});
 const ready=pin=>Files.projectionReadyForUI(root,pin);
 session=Project.create({invoke,frame:()=>state.currentFrame,projectionReady:ready,surfaceChanged:value=>{surface=value;},
  mapping:pin=>({cssBounds:{x:0,y:0,width:320,height:180},physicalExtent:{width:320,height:180},compositionExtent:{width:pin.canvasW,height:pin.canvasH},reportedDpr:1}),
  subscribeRevisions:async(synchronize,failed)=>{sync=synchronize;disconnected=failed;return binding={...metadata(),subscriptionId:'subscription-'+generation};},
  unsubscribeRevisions:async()=>{},subscriptionActive:()=>subscribed,
  publishRevision:pin=>Files.refreshLayerTable(root,pin),changed:pin=>Files.refreshLayerTable(root,pin),
  dispatchApplication:request=>sdk.dispatch(request),
 });
 root.NemoNativeObjectProject={desktop:()=>session,selectLayerUI:(_root,index)=>{state.activeLayerIdx=index;root._layerSel=[index];return true;}};
 root.NemoNativeObjectProjectFile=Files;root.NemoNativeObjectRasterAsset=Raster;
 root.NemoNativeImportCore=require('../src/js/adapters/native-import-core.js');root.NemoNativeImportDrop=Drop;
 for(const [key,name]of Object.entries({NemoNativeImportSvg:'svg',NemoNativeImportFigma:'figma',NemoNativeImportPsd:'psd',NemoNativeImportMontage:'montage'}))root[key]=require('../src/js/adapters/native-import-'+name+'.js');
 root.__TAURI__={dialog:{open:async opts=>{picks.push(opts);return options.pick?options.pick():'/picked/still.png';}}};
 const nodes=new Map();for(const id of ['image-input','video-input','btn-import-img','btn-import-video'])nodes.set(id,{
  addEventListener:(type,handler)=>events.set(id+':'+type,handler),removeEventListener(){},click:()=>assert.fail('no DOM image picker')});
 const document={readyState:'loading',getElementById:id=>nodes.get(id)||null,addEventListener:(type,handler)=>events.set('document:'+type,handler),removeEventListener(){}};
 root.document=document;const controller=Desktop.install(root);
 vm.runInContext(fs.readFileSync(path.join(__dirname,'../src/js/images.js'),'utf8'),vm.createContext({window:root,document,state,SM:root.SM,showToast:root.showToast,console,
  FileReader:()=>assert.fail('no JS decoder'),Image:()=>assert.fail('no JS image'),saveAllLayerFrames:()=>assert.fail('no legacy save'),
  pushUndoLayers:()=>assert.fail('no legacy history'),createUserLayer:()=>assert.fail('no legacy writer'),localStorage:{getItem:()=>null},setTimeout,clearTimeout}));
 const open=(path='/project.json',transition={reveal:pin=>Files.projectView(root,pin)})=>session.openPath(path,transition);
 await open();assert.ok(session.readPresentedObjectSession());assert.equal(ready(session.info()),true);
 return {root,state,session,controller,requests,picks,toasts,open,menu:()=>events.get('btn-import-img:click')(),
  subscribe:value=>{subscribed=value;},disconnect:()=>disconnected(Error('retired')),
  presentationHeader:()=>({available:true,...session.info(),presentation:surface}),
  async emit(){const fromRevision=revision;revision++;return sync({...metadata(),requestId:'external',fromRevision,toRevision:revision},binding);},
 };
}
test('real Project queue imports unchanged menu PNG once while public presentation remains fenced',async()=>{
 const h=await realProjectFixture(),origin=h.session.readPresentedObjectSession();
 const result=await h.menu();assert.equal(result.status,'imported',JSON.stringify(result));assert.equal(result.readback,'confirmed');assert.equal(result.selected,true);
 assert.equal(h.requests.length,1);assert.equal(h.picks.length,1);assert.equal(h.requests[0].operation,'command.document.object.asset.attach');
 assert.equal(h.requests[0].payload.documentSnapshotId,origin.documentSnapshotId);assert.equal(h.requests[0].expectedRevision,0);
 assert.equal(result.receipt.contentRevision,1);assert.equal(result.receipt.historyEntriesAdded,1);assert.equal(h.session.info().undoDepth,1);
 assert.equal(h.session.readPresentedObjectSession().contentRevision,1);assert.deepEqual(h.root._layerSel,[1]);
});
test('real Project action-local presentation expires on resolution, rejection and later action reuse',async()=>{
 const h=await realProjectFixture();let saved;
 await h.session.runResourceCommand(scope=>{saved=scope;assert.equal(h.session.readPresentedObjectSession(),null);assert.ok(scope.readPresentedObjectSession());});
 assert.equal(saved.readPresentedObjectSession(),null);
 await h.session.runResourceCommand(scope=>{assert.ok(scope.readPresentedObjectSession());assert.equal(saved.readPresentedObjectSession(),null);});
 await assert.rejects(h.session.runResourceCommand(scope=>{saved=scope;assert.ok(scope.readPresentedObjectSession());throw Error('local abort');}),/local abort/);
 assert.equal(saved.readPresentedObjectSession(),null);assert.ok(h.session.readPresentedObjectSession());assert.equal(h.requests.length,0);
});
test('real Project held work blocks prequeue menu admission without a picker or leaked presentation',async()=>{
 const h=await realProjectFixture(),hold=deferred();let scope;
 const work=h.session.runResourceCommand(ctx=>{scope=ctx;return hold.promise;});await turn();
 assert.equal(h.session.readPresentedObjectSession(),null);assert.ok(scope.readPresentedObjectSession());
 assert.equal((await h.menu()).status,'unavailable');assert.equal(h.picks.length,0);assert.equal(h.requests.length,0);
 hold.resolve();await work;assert.equal(scope.readPresentedObjectSession(),null);assert.ok(h.session.readPresentedObjectSession());
});
test('real Project rejects changed picker frame, selection, context, active layer, projection, revision, owner and subscription',async()=>{
 for(const kind of ['frame','selection','context','active-layer','projection','revision','owner','subscription-ticket','subscription-inflight','retired']){
  const picker=deferred(),h=await realProjectFixture({pick:()=>picker.promise}),pending=h.menu();await turn();
  if(kind==='frame')h.state.currentFrame=1;
  if(kind==='selection')h.root._layerSel=[];
  if(kind==='context')h.state.activeSymbolId='foreign-symbol';
  if(kind==='active-layer')h.state.activeLayerIdx=7;
  if(kind==='projection')h.state.layers[0].name='changed-read-side-row';
  if(kind==='revision')await h.session.historyFromUi('undo');
  if(kind==='owner')await h.open('/replacement.json');
  if(kind==='subscription-ticket')await h.open('/cancelled.json',{confirm:()=>false});
  if(kind==='subscription-inflight')h.subscribe(false);
  if(kind==='retired')h.controller.dispose();
  picker.resolve('/picked/still.png');const result=await pending;
  assert.equal(result.status,kind==='retired'?'unavailable':'stale',kind+': '+JSON.stringify(result));assert.equal(h.requests.length,0,kind);
  assert.equal(h.controller.image.status(),null,kind+' must not invent an uncertain commit before dispatch');
 }
});
test('real Project validates queued context again after prequeue admission and before asset dispatch',async()=>{
 for(const kind of ['frame','selection','context','active-layer','projection','subscription-inflight','queued-follower','replacement-opening','subscription-sync']){
  const h=await realProjectFixture(),follow=deferred();let other;
  const pending=h.menu();
  // The resolved picker continuation admits and enters Project first. This microtask
  // runs before queue.then(action), so all mutations below happen AFTER admission.
  queueMicrotask(()=>{
   assert.equal(h.session.readPresentedObjectSession(),null,'real resource queue must already be pending');
   if(kind==='frame')h.state.currentFrame=1;
   if(kind==='selection')h.root._layerSel=[];
   if(kind==='context')h.state.activeSymbolId='foreign-symbol';
   if(kind==='active-layer')h.state.activeLayerIdx=7;
   if(kind==='projection')h.state.layers[0].name='changed-read-side-row';
   if(kind==='subscription-inflight')h.subscribe(false);
   if(kind==='queued-follower')other=h.session.runResourceCommand(()=>follow.promise);
   if(kind==='replacement-opening')other=h.open('/cancelled.json',{confirm:()=>false});
   if(kind==='subscription-sync')other=h.emit();
  });
  const result=await pending;follow.resolve();if(other)await other;
  assert.equal(result.status,'stale',kind+': '+JSON.stringify(result));assert.equal(h.requests.length,0,kind);assert.equal(h.controller.image.status(),null,kind);
 }
});
test('real Project inflight import preserves menu busy exclusion until exact reconciliation',async()=>{
 const dispatchGate=deferred(),h=await realProjectFixture({dispatchGate}),pending=h.menu();await turn();
 assert.equal(h.requests.length,1);assert.equal(h.session.readPresentedObjectSession(),null);assert.equal((await h.menu()).status,'busy');assert.equal(h.picks.length,1);
 dispatchGate.resolve();assert.equal((await pending).status,'imported');assert.ok(h.session.readPresentedObjectSession());assert.equal(h.requests.length,1);
});
test('real Project cancelled picker leaves exact owner/revision and presentation unchanged',async()=>{
 const h=await realProjectFixture({pick:()=>null}),before=h.session.readPresentedObjectSession();
 assert.equal((await h.menu()).status,'cancelled');assert.equal(h.requests.length,0);assert.equal(h.picks.length,1);
 assert.deepEqual(h.session.readPresentedObjectSession(),before);assert.equal(h.controller.image.status(),null);
});

test('real Project queued presentation header cannot be rewritten to revive a later presentation',async()=>{
 const h=await realProjectFixture();let retainedScope,header,original;
 await h.session.runResourceCommand(async scope=>{
  retainedScope=scope;header=scope.readPresentedObjectSession();original=header.presentation;
  assert.ok(header);assert.equal(Object.isFrozen(header),true);assert.equal(Object.isFrozen(header.presentation),true);
  await scope.reconcile(0);
  const later=h.presentationHeader();assert.notStrictEqual(later.presentation,original,'real reconcile presents a new frame receipt');
  assert.equal(h.session.readPresentedObjectSession(),null,'public getter remains fenced during the resource action');
  assert.equal(scope.readPresentedObjectSession(),null,'the original presentation must be invalid after reconcile');
  assert.throws(()=>Object.assign(header,later),TypeError,'a consumer cannot retarget the private original guard header');
  assert.strictEqual(header.presentation,original);assert.equal(scope.readPresentedObjectSession(),null);
 });
 assert.equal(retainedScope.readPresentedObjectSession(),null,'retaining the immutable header never retains authority');
 assert.notStrictEqual(h.session.readPresentedObjectSession().presentation,original);assert.equal(h.requests.length,0);
});
