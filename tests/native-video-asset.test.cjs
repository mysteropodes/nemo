'use strict';
// T12 point 8 linked-video import/replace client and UI: the actual images.js menu handler (Replace is the native
// Footage inspector, tests/native-time-remap-ui.test.cjs), the drop router, the desktop composition, the video provider, the video client, the command module and the
// full SDK. Controlled dialog, transport and native projection boundary: no native probe, decode, GPU or installed proof.
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const Desktop=require('../src/js/adapters/native-import-desktop.js'),Drop=require('../src/js/adapters/native-import-drop.js');
const Video=require('../src/js/adapters/native-video-asset.js'),Provider=require('../src/js/adapters/native-import-video.js');
const SDK=require('../src/js/adapters/native-application.js');
const LINK='command.document.object.asset.video.link';
const deferred=()=>{let resolve;return {promise:new Promise(r=>{resolve=r;}),resolve:v=>resolve(v)};};
const turn=()=>new Promise(setImmediate);
const FILTER=[{name:'Videos',extensions:['mp4','mov','mkv','webm','avi']}];

function fixture(options={}){
 let pin={instanceId:'instance',documentId:'document',lifecycleGeneration:1,contentRevision:0,documentSnapshotId:'native-object:document:0',totalFrames:24,
  layers:[{layerUid:'V',name:'Take A',visible:true,locked:false}]},count=0,serial=Promise.resolve(),session,active=true,first=true,projected=true;
 const events=new Map(),toasts=[],requests=[],picks=[],receipts=new Map(),placements=new Map([['V','p1']]);
 const state={currentFrame:0,activeLayerIdx:0,layers:[{layerUid:'V',name:'Take A'}]};
 const root={state,_layerSel:[0],SM:{t:k=>k,nativeVideoPlacement:li=>{const uid=state.layers[li]&&state.layers[li].layerUid;return placements.has(uid)?{placementId:placements.get(uid),assetKind:'video'}:null;}},
  showToast:s=>toasts.push(s)};
 function answer(req){
  if(options.refuse)return {apiVersion:2,requestId:req.requestId,instanceId:req.instanceId,documentId:req.documentId,contentRevision:pin.contentRevision,ok:false,error:{code:options.refuse,message:'Native video link refused.',...(options.details?{details:options.details}:{})}};
  const next=req.expectedRevision+1,intent=req.payload.intent,fresh=intent.kind==='newLayer';
  const layerUid=fresh?'native-layer:'+next+':0':[...placements].find(([,id])=>id===intent.placementId)[0];
  const response={apiVersion:2,requestId:req.requestId,instanceId:req.instanceId,documentId:req.documentId,contentRevision:next,ok:true,
   result:{applied:true,historyEntriesAdded:1,assetId:'asset-'+req.requestId,placementId:fresh?'placement-'+req.requestId:intent.placementId,
    target:{contextId:!fresh&&options.context||'scene-root',layerUid},newLayer:fresh,sourceWidth:32,sourceHeight:16,sourceFrameCount:12}};
  pin={...pin,contentRevision:next,documentSnapshotId:'native-object:document:'+next,layers:fresh?[...pin.layers,{layerUid,name:'B',visible:true,locked:false}]:pin.layers};
  if(fresh)placements.set(layerUid,response.result.placementId);
  state.layers=pin.layers.map(r=>({layerUid:r.layerUid}));
  return response;
 }
 function makeSession(){return {
  info:()=>pin,isActive:()=>active,blocksLegacy:()=>true,requestId:label=>label+'-'+(++count),
  importBatch:()=>assert.fail('video link must not use vector import.batch'),readSubscriptionGeneration:()=>1,
  readPresentedObjectSession:()=>({available:projected,...pin,presentation:{...pin,owner:'native',status:projected?'presented':'pending',frame:state.currentFrame}}),
  runResourceCommand:action=>{const result=serial.then(()=>options.queue?options.queue.promise:undefined).then(()=>action({
   dispatch:q=>SDK.createNativeApplicationAdapter('ui',{dispatch:async req=>{
    requests.push(structuredClone(req));
    let response=receipts.get(req.requestId);
    if(!response){response=answer(req);receipts.set(req.requestId,structuredClone(response));}
    if(options.loseAlways||(options.loseBoth&&requests.length<=2))throw Error('lost both exact replies');
    if(options.lostAck&&first){first=false;throw Error('lost ACK');}
    return structuredClone(response);
   }}).dispatch(q),
   readPresentedObjectSession:()=>session.readPresentedObjectSession(),
   reconcile:async revision=>{if(revision!==undefined)assert.equal(revision,pin.contentRevision);if(options.unconfirmed)projected=false;if(options.afterCommit)options.afterCommit();return pin;},
  }));serial=result.catch(()=>{});return result;},
 };}
 session=makeSession();
 root.NemoNativeObjectProject={desktop:()=>options.noSession?null:session,selectLayerUI:(host,index)=>{state.activeLayerIdx=index;root._layerSel=[index];return true;}};
 root.NemoNativeObjectProjectFile={projectionReadyForUI:()=>projected};
 root.NemoNativeObjectRasterAsset=require('../src/js/adapters/native-object-raster-asset.js');
 if(options.client!==false)root.NemoNativeVideoAsset=Video;
 if(options.provider!==false)root.NemoNativeImportVideo=Provider;
 root.NemoNativeImportCore=require('../src/js/adapters/native-import-core.js');root.NemoNativeImportDrop=Drop;
 for(const [key,name]of Object.entries({NemoNativeImportSvg:'svg',NemoNativeImportFigma:'figma',NemoNativeImportPsd:'psd',NemoNativeImportMontage:'montage'}))root[key]=require('../src/js/adapters/native-import-'+name+'.js');
 root.__TAURI__={dialog:{open:async opts=>{picks.push(opts);return options.pick?options.pick(opts):'/footage/Take B.mov';}},fs:{readFile:()=>assert.fail('no JS video bytes'),readTextFile:()=>assert.fail('no JS text')}};
 const nodes=new Map();const node=id=>{if(!nodes.has(id))nodes.set(id,{id,value:'',dataset:{},style:{},textContent:'',title:'',disabled:false,_wired:false,
  addEventListener:(t,f)=>events.set(id+':'+t,f),removeEventListener(){},removeAttribute(name){this[name]='';},click:()=>assert.fail('never trigger a DOM picker')});return nodes.get(id);};
 ['image-input','video-input','btn-import-img','btn-import-video','btn-footage-replace'].forEach(node);
 const document={readyState:'loading',getElementById:id=>nodes.get(id)||null,addEventListener:(t,f)=>events.set('document:'+t,f),removeEventListener(){}};
 root.document=document;root.paper={project:{}};
 const controller=Desktop.install(root,options.declared===false?{videoRoute:false}:undefined);
 const legacy={SMNativeVideo:{replaceNativeVideoSource:()=>assert.fail('never the legacy relink writer')}};
 Object.assign(root,legacy);
 vm.runInContext(fs.readFileSync(path.join(__dirname,'../src/js/images.js'),'utf8'),vm.createContext({window:root,document,state,SM:root.SM,showToast:root.showToast,console,
  SMNativeVideo:legacy.SMNativeVideo,FileReader:function(){assert.fail('no JS decoder');},Image:function(){assert.fail('no JS decoder');},
  saveAllLayerFrames:()=>assert.fail('no legacy save'),pushUndoLayers:()=>assert.fail('no legacy undo'),createUserLayer:()=>assert.fail('no legacy writer'),
  localStorage:{getItem:()=>null},setTimeout,clearTimeout}));
 return {root,state,controller,requests,picks,toasts,events,nodes,node,placements,
  menu:()=>events.get('btn-import-video:click')(),
  change:next=>{pin={...pin,...next};},bump:()=>{pin={...pin,contentRevision:pin.contentRevision+1,documentSnapshotId:'native-object:document:'+(pin.contentRevision+1)};},deactivate:()=>{active=false;},show:()=>{projected=true;},
  reopen:()=>{pin={...pin,lifecycleGeneration:pin.lifecycleGeneration+1};session=makeSession();},
 };
}
test('client sends ONE closed link command per intent with the pinned snapshot and lifecycle',async()=>{
 const h=fixture(),session=h.root.NemoNativeObjectProject.desktop(h.root),client=Video.create(session,{videoRoute:true});
 const fresh=await client.importPath('/footage/Take A.mov');
 assert.deepEqual(h.requests[0],{apiVersion:2,requestId:'video-1',instanceId:'instance',documentId:'document',expectedRevision:0,operation:LINK,
  payload:{documentSnapshotId:'native-object:document:0',lifecycleGeneration:1,source:{kind:'file',path:'/footage/Take A.mov'},intent:{kind:'newLayer'}}});
 assert.equal(fresh.newLayer,true);assert.equal(fresh.placementId,'placement-video-1');assert.equal(fresh.contentRevision,1);assert.ok(Object.isFrozen(fresh));
 const replaced=await client.replacePath('p1','/footage/Take B.mov');
 assert.deepEqual(h.requests[1].payload,{documentSnapshotId:'native-object:document:1',lifecycleGeneration:1,source:{kind:'file',path:'/footage/Take B.mov'},intent:{kind:'replace',placementId:'p1'}});
 assert.equal(replaced.newLayer,false);assert.equal(replaced.placementId,'p1');assert.equal(replaced.target.layerUid,'V');assert.equal(h.requests.length,2);
});
test('client refuses locally: no route, bad path, no picker, stale origin; cancelled picker sends nothing',async()=>{
 const h=fixture(),session=h.root.NemoNativeObjectProject.desktop(h.root);
 await assert.rejects(Video.create(session,{videoRoute:false,pickVideo:()=>assert.fail('no picker without a route')}).importPicked(),e=>e.code==='unavailable'&&e.reason==='video_host_route_unavailable');
 const client=Video.create(session,{videoRoute:true,pickVideo:async()=>null});
 for(const bad of ['',null,5,'/a\u0000b.mov'])await assert.rejects(client.importPath(bad),e=>e.code==='invalid_request');
 await assert.rejects(client.replacePicked(''),e=>e.code==='invalid_request');
 await assert.rejects(Video.create(session,{videoRoute:true}).importPicked(),e=>e.code==='unavailable'&&e.reason==='picker_unavailable');
 await assert.rejects(client.importPath('/a.mov',{current:()=>false}),e=>e.code==='stale_revision');
 assert.equal(await client.importPicked(),null);assert.equal(h.requests.length,0);
 // The advertised path bound is reachable; one byte more is refused locally before any request.
 const over='/'+'a'.repeat(2044)+'.mov';await assert.rejects(client.importPath(over),e=>e.code==='invalid_request');assert.equal(h.requests.length,0);
 const limit='/'+'a'.repeat(2043)+'.mov';assert.equal((await client.importPath(limit)).newLayer,true);assert.equal(h.requests[0].payload.source.path,limit);
});
test('ambiguous outcome replays the SAME request id and body; a later replay returns the stored receipt once',async()=>{
 const lost=fixture({lostAck:true}),client=Video.create(lost.root.NemoNativeObjectProject.desktop(lost.root),{videoRoute:true});
 const receipt=await client.importPath('/footage/Take A.mov');assert.equal(lost.requests.length,2);assert.deepEqual(lost.requests[0],lost.requests[1]);assert.equal(receipt.contentRevision,1);
 const h=fixture({loseBoth:true}),both=Video.create(h.root.NemoNativeObjectProject.desktop(h.root),{videoRoute:true});
 const error=await both.replacePath('p1','/footage/Take B.mov').then(()=>assert.fail('indeterminate'),e=>e);
 assert.equal(error.code,'video_command_indeterminate');assert.equal(error.requestId,'video-1');assert.equal(typeof error.replay,'function');
 const replayed=await error.replay();assert.equal(h.requests.length,3);for(const q of h.requests)assert.deepEqual(q,h.requests[0]);
 assert.equal(replayed.placementId,'p1');assert.equal(replayed.requestId,'video-1');
});
test('desktop binding picks with the video filter and is rebound for a reopened owner',async()=>{
 const h=fixture(),first=Video.desktop(h.root,{videoRoute:true});assert.equal(Video.desktop(h.root),first);
 await first.importPicked();assert.deepEqual(h.picks,[{title:'Import Video',multiple:false,filters:FILTER}]);
 await first.replacePicked('p1');assert.deepEqual(h.picks[1],{title:'Relink / replace video file',multiple:false,filters:FILTER});
 h.reopen();const second=Video.desktop(h.root,{videoRoute:true});assert.notEqual(second,first);
 await second.importPicked();assert.equal(h.requests[2].payload.lifecycleGeneration,2);
 assert.deepEqual(Video.VIDEO_EXTENSIONS,['mp4','mov','mkv','webm','avi']);
});
test('the video menu reaches the native provider and selects the new linked layer (newLayer intent)',async()=>{
 const h=fixture();assert.equal(h.controller.video.declared(),true);
 const result=await h.menu();assert.equal(result.status,'imported',JSON.stringify(result));assert.equal(result.readback,'confirmed');assert.equal(result.selected,true);
 assert.equal(h.requests.length,1);assert.deepEqual(h.requests[0].payload.intent,{kind:'newLayer'});assert.equal(h.requests[0].payload.source.path,'/footage/Take B.mov');
 assert.deepEqual(h.picks,[{title:'Import Video',multiple:false,filters:FILTER}]);assert.deepEqual(h.root._layerSel,[1]);
 const again=await h.root.SM.importVideo();assert.equal(again.status,'imported');assert.equal(h.requests.length,2);
});
test('unavailable without a session, provider, client or declared route; DOM files and drops stay unavailable',async()=>{
 const off=fixture({declared:false});assert.equal(off.controller.video.declared(),false);
 assert.equal((await off.menu()).status,'unavailable');assert.equal(off.picks.length,0);
 const inactive=fixture();inactive.deactivate();const r=await inactive.controller.video.openFile();assert.equal(r.status,'unavailable');assert.equal(inactive.picks.length,0);
 const noProvider=fixture({provider:false});assert.equal(noProvider.controller.video,null);
 assert.equal((await noProvider.menu()).reason,'native_media_picker_absent');assert.equal(noProvider.picks.length,0);
 const noClient=fixture({client:false});assert.equal((await noClient.menu()).reason,'native_video_client_absent');
 const h=fixture();const opaque={name:'take.mov',type:'video/quicktime',get path(){assert.fail('no File.path');},arrayBuffer(){assert.fail('no JS bytes');}};
 assert.equal((await h.controller.video.importFiles([opaque])).reason,'native_drop_handle_absent');
 assert.equal((await h.controller.drop.drop([opaque])).status,'unavailable');assert.equal((await h.root.SM.importVideoFile(opaque)).status,'unavailable');
 assert.equal(h.requests.length,0);assert.equal(h.picks.length,0);
 const noSession=fixture({noSession:true});assert.equal(Video.desktop(noSession.root,{videoRoute:true}),null);
});
test('typed native refusals and picker cancellation are reported and never retried',async()=>{
 for(const [code,status] of [['invalid_request','invalid'],['not_found','refused'],['stale_revision','stale'],['unavailable','unavailable']]){
  const h=fixture({refuse:code});const r=await h.controller.video.openFile();assert.equal(r.status,status,code);assert.equal(h.requests.length,1,code);
 }
 const h=fixture({pick:()=>null});assert.equal((await h.menu()).status,'cancelled');assert.equal(h.requests.length,0);
});
test('busy while picking; a context change while picking refuses before dispatch',async()=>{
 const gate=deferred(),h=fixture({pick:()=>gate.promise}),pending=h.menu();await turn();
 assert.equal((await h.controller.video.openFile()).status,'busy');h.state.currentFrame=1;gate.resolve('/footage/Take B.mov');
 assert.equal((await pending).status,'stale');assert.equal(h.requests.length,0);
});
test('both replies lost latch the exact owner/request; recovery replays the same request once and keeps selection',async()=>{
 const h=fixture({loseBoth:true});const first=await h.menu();
 assert.equal(first.status,'unconfirmed');assert.equal(first.code,'video_command_indeterminate');assert.equal(first.recovery,'exact-request');assert.equal(first.requestId,'video-1');
 assert.deepEqual(first.owner,{instanceId:'instance',documentId:'document',lifecycleGeneration:1});
 assert.equal((await h.menu()).requestId,'video-1');assert.equal(h.picks.length,1);assert.equal(h.requests.length,2);
 const recovered=await h.controller.video.recover();assert.equal(recovered.status,'imported');assert.equal(recovered.selected,false);
 assert.equal(h.requests.length,3);assert.deepEqual(h.requests[2],h.requests[0]);assert.equal(h.controller.video.status(),null);
 assert.equal((await h.controller.video.recover()).status,'nothing');
});
test('a reopened owner clears the latch; a known receipt without presentation waits for presentation',async()=>{
 const h=fixture({loseAlways:true});await h.menu();h.change({contentRevision:5});assert.equal((await h.menu()).status,'unconfirmed');
 h.reopen();assert.equal(h.controller.video.status(),null);
 const shown=fixture({unconfirmed:true});const r=await shown.menu();assert.equal(r.status,'unconfirmed');assert.equal(r.recovery,'presentation');
 shown.show();const done=await shown.controller.video.recover();assert.equal(done.status,'imported');assert.equal(shown.requests.length,1);
});
test('Replace calls replaceActive with the pinned placement id and relinks in place (replace intent)',async()=>{
 const h=fixture();
 const calls=[];const original=h.controller.video.replaceActive;
 const spy=Object.freeze(Object.assign({},h.controller.video,{replaceActive:id=>{calls.push(id);return original(id);}}));
 h.root.NemoNativeImportController=Object.freeze(Object.assign({},h.controller,{video:spy}));
 const result=await h.root.NemoNativeImportController.video.replaceActive('p1');assert.deepEqual(calls,['p1']);
 assert.equal(result.status,'replaced',JSON.stringify(result));assert.equal(result.readback,'confirmed');assert.equal(result.selected,false);
 assert.equal(h.requests.length,1);assert.deepEqual(h.requests[0].payload.intent,{kind:'replace',placementId:'p1'});
 assert.deepEqual(h.picks,[{title:'Relink / replace video file',multiple:false,filters:FILTER}]);assert.deepEqual(h.root._layerSel,[0]);assert.equal(h.state.layers.length,1);
});
test('Replace refuses a placement the pinned read no longer names',async()=>{
 const h=fixture();h.placements.set('V','p2');
 const result=await h.controller.video.replaceActive('p1');assert.equal(result.status,'stale');assert.equal(result.reason,'placement_changed');assert.equal(h.picks.length,0);assert.equal(h.requests.length,0);
 assert.equal((await h.controller.video.replaceActive(null)).status,'invalid');
});
test('a failing picker is an ordinary unavailable outcome that dispatches nothing and latches nothing',async()=>{
 let calls=0;const h=fixture({pick:()=>{if(++calls===1)throw new Error('dialog plugin rejected');return '/footage/Take B.mov';}});
 const failed=await h.menu();assert.equal(failed.status,'unavailable');assert.equal(failed.reason,'picker_failed');
 assert.equal(h.requests.length,0);assert.equal(h.controller.video.status(),null);
 assert.equal((await h.menu()).status,'imported');assert.equal(h.requests.length,1);
});
test('a host probe timeout reported notDispatched stays retryable and is never latched',async()=>{
 const h=fixture({refuse:'unavailable',details:{disposition:'notDispatched',committed:false,retryExecution:false}});
 const r=await h.menu();assert.equal(r.status,'unavailable');assert.equal(h.controller.video.status(),null);
 assert.equal((await h.menu()).status,'unavailable');assert.equal(h.requests.length,2);assert.equal(h.picks.length,2);
 const unknown=fixture({refuse:'unavailable',details:{disposition:'indeterminate',committed:null,retryExecution:false}});
 assert.equal((await unknown.menu()).status,'unconfirmed');
});
test('importing a new layer inside a component is unavailable before the picker; Replace there still relinks',async()=>{
 const h=fixture();h.state.activeSymbolId='sym_comp';
 const r=await h.menu();assert.equal(r.status,'unavailable');assert.equal(r.reason,'component_context_import_unavailable');
 assert.equal(h.picks.length,0);assert.equal(h.requests.length,0);assert.equal(h.controller.video.status(),null);
 const replaced=await h.controller.video.replaceActive('p1');assert.equal(replaced.status,'replaced',JSON.stringify(replaced));assert.equal(h.requests.length,1);
});
test('a replace receipt naming another target context than the pinned read is not confirmed',async()=>{
 const h=fixture({context:'sym_comp'});const r=await h.controller.video.replaceActive('p1');
 assert.equal(r.status,'unconfirmed');assert.equal(r.recovery,'presentation');assert.equal(r.receipt.target.contextId,'sym_comp');assert.equal(h.requests.length,1);
});
test('a moved playhead or a later revision during the probe still confirms the receipt from the presented chain',async()=>{
 let h;h=fixture({afterCommit:()=>{h.state.currentFrame=7;}});const moved=await h.menu();
 assert.equal(moved.status,'imported',JSON.stringify(moved));assert.equal(moved.readback,'confirmed');assert.equal(h.controller.video.status(),null);
 let later;later=fixture({afterCommit:()=>later.bump()});const r=await later.menu();
 assert.equal(r.status,'imported',JSON.stringify(r));assert.equal(r.receipt.contentRevision,1);assert.equal(later.root.NemoNativeObjectProject.desktop().info().contentRevision,2);
 let gone;gone=fixture({afterCommit:()=>{gone.change({layers:[gone.root.NemoNativeObjectProject.desktop().info().layers[0]]});}});
 assert.equal((await gone.menu()).status,'unconfirmed');
});
test('a presentation latch clears on the next status read once the chain presents the receipt, without a recover() caller',async()=>{
 const h=fixture({unconfirmed:true});const r=await h.menu();assert.equal(r.status,'unconfirmed');assert.equal(r.recovery,'presentation');
 assert.equal((await h.menu()).status,'unconfirmed');assert.equal(h.requests.length,1);
 h.show();h.bump();h.state.currentFrame=3;assert.equal(h.controller.video.status(),null);
 const next=await h.menu();assert.equal(next.requestId,'video-2');assert.equal(h.requests.length,2);
 const ctxFixture=fixture({context:'sym_comp'});await ctxFixture.controller.video.replaceActive('p1');ctxFixture.bump();
 assert.equal(ctxFixture.controller.video.status().status,'unconfirmed');
});
