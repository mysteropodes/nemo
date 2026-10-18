'use strict';
// These tests control provider/native/DOM boundaries; no real native media provider is claimed.
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const Drop=require('../src/js/adapters/native-import-drop.js');
const turn=()=>new Promise(setImmediate);
function deferred(){let resolve;return {promise:new Promise(r=>{resolve=r;}),resolve:v=>resolve(v)};}
function opaque(name,type){return Object.freeze({name,type,get path(){assert.fail('File.path must remain unread');},arrayBuffer(){assert.fail('JS must not read media bytes');},text(){assert.fail('JS must not decode media');}});}
const png=()=>opaque('still.png','image/png'),svg=()=>opaque('drawing.svg','image/svg+xml'),video=()=>opaque('clip.mp4','video/mp4');
function harness({providers={},blocked=true,router=true}={}){
  const events=new Map(),elements=new Map(),toasts=[],calls=[],legacy=[];
  function element(id){if(!elements.has(id))elements.set(id,{id,value:'',dataset:{},style:{},innerHTML:'',classList:{add(){},remove(){},toggle(){},contains(){return false;}},
    addEventListener(type,fn){events.set(id+':'+type,fn);},click(){legacy.push('input.click');}});return elements.get(id);}
  ['image-input','video-input','btn-import-img','btn-import-video','media-grid'].forEach(element);
  const document={readyState:'loading',getElementById:id=>elements.get(id)||null,addEventListener:(name,fn)=>events.set('document:'+name,fn)};
  let revision=0,generation=1,active=true;
  const state={currentFrame:0,activeLayerIdx:0,mediaLibrary:[],symbols:{}};
  const info=()=>({instanceId:'instance',documentId:'document',lifecycleGeneration:generation,contentRevision:revision,documentSnapshotId:'snapshot-'+revision});
  const session={blocksLegacy:()=>blocked,isActive:()=>active,info};
  const root={document,state,SM:{t:key=>key},NemoNativeObjectProject:{desktop:()=>session},showToast:text=>toasts.push(text),
    __TAURI__:{core:{},dialog:{open:()=>{legacy.push('dialog');throw Error('legacy picker');}},fs:{readFile:()=>{legacy.push('readFile');throw Error('legacy byte read');}}}};
  if(router)root.NemoNativeImportDrop=Drop;
  root.NemoNativeImportController={drop:Drop.create({providers,svg:{importFiles:()=>assert.fail('image menu SVG must not enter vector import')},svgAvailable:()=>true}),surface:r=>calls.push(['surface',r])};
  const context=vm.createContext({window:root,document,state,SM:root.SM,showToast:root.showToast,console,
    FileReader:function(){legacy.push('FileReader');throw Error('legacy decode');},Image:function(){legacy.push('Image');throw Error('legacy decode');},
    saveAllLayerFrames:()=>legacy.push('save'),pushUndoLayers:()=>legacy.push('undo'),createUserLayer:()=>legacy.push('layer'),
    localStorage:{getItem:()=>null},setTimeout,clearTimeout});
  vm.runInContext(fs.readFileSync(path.join(__dirname,'../src/js/images.js'),'utf8'),context);
  function library(){vm.runInContext(fs.readFileSync(path.join(__dirname,'../src/js/media-library.js'),'utf8'),context);events.get('document:DOMContentLoaded')();}
  return {root,session,state,info,toasts,calls,legacy,events,library,
    revise(){revision++;},replace(){generation++;},retire(){active=false;},
    input:async(kind,files)=>events.get(kind+'-input:change')({target:{files,value:'picked'}}),
    menu:kind=>events.get('btn-import-'+(kind==='image'?'img':'video')+':click')(),
    drop:files=>events.get('media-grid:drop')({preventDefault(){},dataTransfer:{types:['Files'],files}})};
}
function provider(log,group,result){return {available:()=>true,importFiles:async files=>{log.push({group,files});return result||{status:'imported',readback:'confirmed'};}};}
function sourceCount(log){return log.filter(x=>x[0]==='surface').length;}
test('actual public and captured input/menu callers deny absent providers before all legacy paths',async()=>{
  const h=harness();const results=[await h.root.SM.importImageFiles([png()]),await h.root.SM.importVideoFile(video()),
    await h.input('image',[png()]),await h.input('video',[video()]),await h.menu('image'),await h.menu('video'),
    await h.root.SM.importImages(),await h.root.SM.importVideo()];
  assert.ok(results.every(r=>r.status==='unavailable'));assert.deepEqual(h.legacy,[]);
  assert.equal(sourceCount(h.calls),8);
});
test('native ownership still denies legacy import when the routing module or composed controller is absent',async()=>{
  for(const missing of ['router','controller','indeterminate']){
    const h=harness({router:missing!=='router'});h.library();
    if(missing==='controller')delete h.root.NemoNativeImportController;
    if(missing==='indeterminate')h.retire();
    for(const r of [await h.root.SM.importImageFiles([png()]),await h.root.SM.importVideoFile(video()),await h.menu('image'),await h.input('image',[png()]),await h.input('video',[video()]),await h.drop([png(),video()])])assert.equal(r.status,'unavailable');
    assert.deepEqual(h.legacy,[]);
  }
});
test('actual image input passes opaque files unchanged in order, including SVG image grouping',async()=>{
  const log=[],h=harness({providers:{image:provider(log,'image')}}),files=[png(),svg()];
  assert.equal((await h.input('image',files)).status,'routed');assert.equal(log.length,1);
  assert.equal(log[0].files[0],files[0]);assert.equal(log[0].files[1],files[1]);assert.deepEqual(h.legacy,[]);
});
test('actual media drop preflights every image/video group before any provider runs',async()=>{
  const log=[],h=harness({providers:{image:provider(log,'image')}});h.library();
  const result=await h.drop([svg(),video()]);assert.equal(result.status,'unavailable');
  assert.deepEqual(result.blocked.map(b=>b.group),['video']);assert.equal(log.length,0);assert.deepEqual(h.legacy,[]);
});
test('actual media drop awaits registered controlled providers and preserves baseline order',async()=>{
  const log=[],gate=deferred();
  const h=harness({providers:{image:{available:()=>true,importFiles:async files=>{log.push({group:'image',files});await gate.promise;return {status:'imported',readback:'confirmed'};}},video:provider(log,'video')}});h.library();
  const files=[video(),svg(),png(),video()];let settled=false;
  const pending=h.drop(files).then(r=>{settled=true;return r;});await turn();assert.equal(settled,false);assert.equal(log.length,1);
  gate.resolve();assert.equal((await pending).status,'routed');
  assert.deepEqual(log.map(x=>x.group),['image','video','video']);assert.equal(log[0].files[0],files[1]);assert.equal(log[0].files[1],files[2]);
  assert.equal(log[1].files[0],files[0]);assert.equal(log[2].files[0],files[3]);assert.deepEqual(h.legacy,[]);
});
for(const status of ['refused','stale','busy','unavailable','unconfirmed','cancelled']){
 test('actual mixed caller preserves '+status+' and never dispatches the next group or legacy writer',async()=>{
  const log=[],h=harness({providers:{image:provider(log,'image',{status}),video:provider(log,'video')}});h.library();
  assert.equal((await h.drop([png(),video()])).status,status);assert.equal(log.length,1);assert.deepEqual(h.legacy,[]);
 });
}
test('unconfirmed ACK/readback and unknown provider results never count as routed success',async()=>{
 for(const result of [undefined,{status:'imported',readback:'unconfirmed'},{status:'imported'},{status:'routed'},{status:'invented'}]){
  const log=[],h=harness({providers:{image:{available:()=>true,importFiles:async files=>{log.push(files);return result;}},video:provider(log,'video')}});h.library();
  assert.equal((await h.drop([png(),video()])).status,'unconfirmed');assert.equal(log.length,1);assert.deepEqual(h.legacy,[]);
 }
});
test('a thrown provider is not retried; prior confirmed effects remain accounted for',async()=>{
 const log=[],h=harness({providers:{image:provider(log,'image'),video:{available:()=>true,importFiles:async()=>{log.push({group:'video'});throw Error('lost ACK');}}}});h.library();
 const result=await h.drop([png(),video(),video()]);assert.equal(result.status,'unconfirmed');assert.equal(result.applied.length,1);
 assert.equal(log.length,2);assert.deepEqual(h.legacy,[]);
});
test('origin changed by availability preflight is refused before native submission',async()=>{
 for(const change of ['revision','owner','frame','layer']){
  const log=[];let h;h=harness({providers:{image:{available(){if(change==='revision')h.revise();if(change==='owner')h.replace();if(change==='frame')h.state.currentFrame++;if(change==='layer')h.state.activeLayerIdx++;return true;},importFiles:provider(log,'image').importFiles}}});
  assert.equal((await h.root.SM.importImageFiles([png()])).status,'stale');assert.equal(log.length,0);assert.deepEqual(h.legacy,[]);
 }
});
test('session replacement while a provider awaits prevents further submission',async()=>{
 const gate=deferred(),log=[],h=harness({providers:{image:{available:()=>true,importFiles:async()=>{await gate.promise;return {status:'imported',readback:'confirmed'};}},video:provider(log,'video')}});h.library();
 const pending=h.drop([png(),video()]);h.replace();gate.resolve();const result=await pending;
 assert.equal(result.status,'stale');assert.equal(result.applied.length,1);assert.equal(log.length,0);assert.deepEqual(h.legacy,[]);
});
test('a provider-owned revision/selection advance does not invalidate the remaining same-owner batch',async()=>{
 let h;const log=[];h=harness({providers:{image:{available:()=>true,importFiles:async()=>{h.revise();h.state.activeLayerIdx++;return {status:'imported',readback:'confirmed'};}},video:provider(log,'video')}});h.library();
 assert.equal((await h.drop([png(),video()])).status,'routed');assert.equal(log.length,1);
});
test('non-native menu controls retain their existing input action and native unknown drops retain the hint',async()=>{
 const h=harness({blocked:false});delete h.root.__TAURI__;await h.menu('image');await h.menu('video');assert.deepEqual(h.legacy,['input.click','input.click']);
 const native=harness();native.library();assert.equal((await native.drop([opaque('note.txt','text/plain')])).status,'empty');
 assert.deepEqual(native.toasts,['toastFormatNotRecognizedDropHint']);assert.deepEqual(native.legacy,[]);
});
test('extracted native-inactive video dispatch keeps sequential order, picked metadata and baseline failure notice',async()=>{
 const calls=[],notices=[],gate=deferred();let complete=false;
 const files=[{name:'first'},{name:'bad'},{name:'last'}];
 const pending=Drop.videoFiles(files,[{path:'admitted'},null,null],async picked=>{calls.push(picked.path);await gate.promise;},
  async file=>{calls.push(file.name);if(file.name==='bad')throw Error('decode refused');},(...args)=>notices.push(args)).then(()=>{complete=true;});
 await turn();assert.deepEqual(calls,['admitted']);assert.equal(complete,false);gate.resolve();await pending;
 assert.deepEqual(calls,['admitted','bad','last']);assert.deepEqual(notices,[['2/3 vidéos importées — échec : bad','warn']]);
});
test('extracted input routing resets input and preserves image/video empty selection semantics',()=>{
 const dispatched=[];const image={target:{files:[],value:'picked'}},video={target:{files:[],value:'picked'}};
 Drop.fileInput(image,'image',files=>dispatched.push(files));Drop.fileInput(video,'video',files=>dispatched.push(files));
 assert.equal(image.target.value,'');assert.equal(video.target.value,'');assert.equal(dispatched.length,1);
});
test('extracted native-inactive library dispatch retains classification and public callback order',async()=>{
 const calls=[],h=harness({blocked:false});h.root.SM.importImageFiles=files=>calls.push(['images',files]);h.root.SM.importVideoFile=file=>calls.push(['video',file]);h.library();
 const files=[video(),svg(),png(),video()];await h.drop(files);assert.deepEqual(calls.map(x=>x[0]),['images','video','video']);
 assert.equal(calls[0][1][0],files[1]);assert.equal(calls[0][1][1],files[2]);assert.equal(calls[1][1],files[0]);assert.equal(calls[2][1],files[3]);
});
