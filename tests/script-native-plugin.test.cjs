'use strict';
// CS8-SDK (#2354, I34): a plugin panel reaches the document only through the native script API, one run per event tick.
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const Api=require('../src/js/application/script-native-api.js'),Run=require('../src/js/application/script-native-run.js'),Plugin=require('../src/js/application/script-native-plugin.js');
const Groups=require('../src/js/application/script-native-groups.js');
const PLUGIN_SOURCE=fs.readFileSync(path.join(__dirname,'../src/js/nemo-plugin.js'),'utf8');
const turn=()=>new Promise(r=>setImmediate(r));

// A native session double and the write entries behind it: each entry mutates the read model the way the owner would.
function fixture(extra={}){
  let info={instanceId:'i',documentId:'d',lifecycleGeneration:1,contentRevision:0,fps:24,canvasW:8,canvasH:8,totalFrames:12,
    layers:[{layerUid:'a',name:'A',visible:true},{layerUid:'b',name:'B',visible:true}]};
  const sent=[],toasts=[];let minted=0;
  const bump=fn=>{info=fn({...info,contentRevision:info.contentRevision+1});};
  const writes={
    add:async()=>{sent.push(['add']);const uid='n'+(++minted);bump(i=>({...i,layers:[...i.layers,{layerUid:uid,name:'Layer '+(i.layers.length+1),visible:true}]}));return {ok:true,applied:true,layerUid:uid};},
    remove:async(uid)=>{sent.push(['remove',uid]);bump(i=>({...i,layers:i.layers.filter(l=>l.layerUid!==uid)}));return {ok:true,applied:true};},
    rename:async(uid,name)=>{sent.push(['rename',uid,name]);bump(i=>({...i,layers:i.layers.map(l=>l.layerUid===uid?{...l,name}:l)}));return {ok:true,applied:true};},
    marker:async spec=>{sent.push(['marker',spec.action,spec.frame,spec.name]);bump(i=>i);return {ok:true,applied:true};},
  };
  const session={blocksLegacy:()=>true,isActive:()=>true,info:()=>info,...extra};
  const root={NemoNativeObjectProject:{desktop:()=>session},NemoScriptNativeWrites:{create:()=>writes},_layerSel:[1],state:{currentFrame:0,activeLayerIdx:0},
    showToast:m=>toasts.push(m),goToFrame(){},
    SM:{toggleLayerVis(index){sent.push(['vis',index]);bump(i=>({...i,layers:i.layers.map((l,k)=>k===index?{...l,visible:!l.visible}:l)}));return Promise.resolve(true);}}};
  return {root,sent,toasts,get info(){return info;},set info(v){info=v;},writes};
}
function plugin(f,open={on:true}){return Plugin.create(f.root,{isOpen:()=>open.on}).api('p1',{close(){open.on=false;},setTitle(t){f.title=t;}});}

test('a plugin handler reads the pinned model and its edits drain through the same native entries a script uses (I34)',async()=>{
  const f=fixture(),nemo=plugin(f);
  assert.equal(nemo.layerCount,2);assert.equal(nemo.layer('A').name,'A');assert.deepEqual(nemo.layers().map(l=>l.name),['A','B']);
  nemo.layer('A').name='Renamed';nemo.layer('B').visible=false;
  assert.deepEqual(f.sent,[],'nothing is written while the handler runs');
  await turn();
  assert.deepEqual(f.sent,[['rename','a','Renamed'],['vis',1]]);assert.equal(f.info.layers[0].name,'Renamed');
  // The same edits as a script run reach the same entries in the same order.
  const g=fixture();const run=Api.createRun(Run.portsOf(g.root));run.nemo.layer('A').name='Renamed';run.nemo.layer('B').visible=false;await run.commit();
  assert.deepEqual(g.sent,f.sent);
});

test('one event tick is one run and one undo group: the drain brackets the undo depth and records the entries it added',async()=>{
  let depth=7;const groups=Groups.shared;groups.forget();
  const f=fixture({readHistory:()=>Promise.resolve({info:{instanceId:'i',documentId:'d',lifecycleGeneration:1,contentRevision:f.info.contentRevision,documentSnapshotId:'s'},undoDepth:depth,redoDepth:0})});
  f.root.NemoScriptNativeGroups=Groups;
  const w={...f.writes};for(const k of ['rename','marker'])f.writes[k]=async(...a)=>{depth+=1;return w[k](...a);};
  const nemo=plugin(f);
  nemo.layer('A').name='One';nemo.markers.add(2,'M');nemo.layer('B').name='Two';
  await turn();
  assert.equal(groups.plan('undo',{instanceId:'i',documentId:'d',lifecycleGeneration:1,contentRevision:3}),3,'one UI undo moves the whole tick');
  groups.forget();
});

test('layer handles outlive the tick: they follow layerUid across reorders and resolve a created layer through the id the owner minted',async()=>{
  const f=fixture(),nemo=plugin(f),b=nemo.layer('B'),made=nemo.addLayer('Ink');
  assert.equal(made.name,'Ink');assert.equal(made.index,2);
  await turn();
  f.info={...f.info,layers:[f.info.layers[2],f.info.layers[1],f.info.layers[0]]};
  b.visible=false;made.name='Ink 2';
  await turn();
  assert.deepEqual(f.sent.slice(-2),[['vis',1],['rename','n1','Ink 2']],'b resolved by id at its new index; the new layer by its minted id');
  assert.equal(made.layerUid,'n1');assert.equal(b.layerUid,'b');
  f.info={...f.info,layers:f.info.layers.filter(l=>l.layerUid!=='b')};
  assert.throws(()=>b.visible,e=>e.code==='refused'&&/layer "b" is not in this document/.test(e.message));
  await turn();
  f.info={...f.info,layers:[{layerUid:'x',name:'b'},{layerUid:'b',name:'Real'}]};
  assert.equal(nemo.layer('b').layerUid,'x','nemo.layer(string) matches a name first, like a script');assert.equal(b.name,'Real','a held handle still means the layer it was made for');
});

test('groups and chaining re-bind each tick; a handle passed back to the API is unwrapped to its id',async()=>{
  const f=fixture(),nemo=plugin(f);
  assert.equal(nemo.markers.add(3,'M'),nemo.markers);assert.equal(nemo.comp.fps,24);
  nemo.layer('A').visible=false;await turn();
  assert.deepEqual(f.sent,[['marker','add',3,'M'],['vis',0]]);
  assert.deepEqual(nemo.selection.layers().map(l=>l.layerUid),['b']);
  assert.equal(nemo.layer('A').visible,false);assert.equal(typeof nemo.then,'undefined','a facade is not mistaken for a thenable');
  assert.equal(typeof nemo.layer(0).then,'undefined');
  assert.ok(nemo.help().includes('Layer.visible'));assert.equal(nemo.undoGroup(()=>7),7);
  nemo.close();nemo.setTitle('T');assert.equal(f.title,'T');
});

test('rows with no native operation, and reads that need a prefetched round, refuse with their typed reason',()=>{
  const f=fixture(),nemo=plugin(f),a=nemo.layer('A');
  const typed=(fn,row,reason)=>assert.throws(fn,e=>e.code==='unavailable'&&e.row===row&&e.reason===reason);
  typed(()=>a.blendMode,'sdk:nemo-script.Layer.blendMode','no-native-blend-matte');typed(()=>a.strokes,'sdk:nemo-script.Layer.strokes','no-native-stroke-read');
  typed(()=>nemo.addPivot('x'),'sdk:nemo-script.addPivot','no-native-pivot');typed(()=>a.get('opacity'),'sdk:nemo-script.Layer.get','native-entry-absent');
  assert.throws(()=>nemo.backgrond,e=>e.code==='unavailable'&&e.reason==='undeclared');
  assert.deepEqual(f.sent,[]);
});

test('a closed plugin refuses every member, and a tick still open when the plugin closes is discarded: nothing is sent',async()=>{
  const f=fixture(),open={on:true},nemo=plugin(f,open),a=nemo.layer('A');
  a.name='x';open.on=false;await turn();
  assert.deepEqual(f.sent,[],'the plugin was closed before its drain');
  assert.throws(()=>nemo.layerCount,/plugin "p1" is closed/);assert.throws(()=>{a.name='y';},/is closed/);assert.throws(()=>a.name,/is closed/);assert.throws(()=>nemo.addLayer('z'),/is closed/);
  const control=Plugin.create(f.root,{isOpen:()=>false});assert.equal(control.isOpen('p1'),false);
});

test('a document replaced or closed between the tick and its drain gets no write, and a missing session refuses the tick (I29/I34)',async()=>{
  const f=fixture(),nemo=plugin(f);
  nemo.layer('A').name='x';f.info={...f.info,documentId:'other'};await turn();
  assert.deepEqual(f.sent,[]);assert.ok(f.toasts.some(t=>/Plugin writes stopped: document-replaced/.test(t)));
  const wrong=fixture(),n2=plugin(wrong);n2.layer('A').name='x';wrong.info={...wrong.info,instanceId:'elsewhere'};await turn();assert.deepEqual(wrong.sent,[]);
  const none=fixture();none.root.NemoNativeObjectProject={desktop:()=>null};
  assert.throws(()=>plugin(none).layerCount,e=>e.code==='refused'&&/open native document only/.test(e.message));
  assert.deepEqual(none.sent,[]);
});

// ---- the host: nemo-plugin.js hands the iframe the native facade, never the legacy builder ----
function host(parts={}){
  const legacy={version:1,layer(){throw new Error('LEGACY');}},calls=[];
  const ctx={TextDecoder,JSON,Promise,Error,Object,Array,Uint8Array,setTimeout,SMScript:{api(){calls.push('legacy');return legacy;}},...parts};
  ctx.window=ctx;vm.createContext(ctx);vm.runInContext(PLUGIN_SOURCE,ctx,{filename:'nemo-plugin.js'});
  return {ctx,legacy,calls};
}

// Just enough DOM for mount(): elements are inert records, the iframe keeps the srcdoc it was given.
function fakeDom(){
  const make=tag=>{const e={tag,style:{},children:[],className:'',parentNode:null,_text:'',
    appendChild(c){c.parentNode=e;e.children.push(c);return c;},addEventListener(){},remove(){if(e.parentNode)e.parentNode.children=e.parentNode.children.filter(x=>x!==e);e.parentNode=null;},
    insertBefore(c){c.parentNode=e;e.children.unshift(c);},removeChild(c){c.parentNode=null;e.children=e.children.filter(x=>x!==c);},replaceChild(c,o){e.children=e.children.map(x=>x===o?c:x);},
    set textContent(v){e._text=v;},get textContent(){return e._text;},get firstChild(){return e.children[0]||null;},offsetLeft:0,offsetTop:0};return e;};
  const body=make('body'),frames=[];
  const document={body,createElement(tag){const e=make(tag);if(tag==='iframe')frames.push(e);return e;},addEventListener(){}};
  class DOMParser{parseFromString(){const head=make('head'),d=make('html');d.querySelector=sel=>sel==='head'?head:null;d.querySelectorAll=()=>[];d.createElement=t=>make(t);
    d.documentElement={get outerHTML(){return head.children.map(c=>c._text).join('\n');}};return d;}}
  return {document,DOMParser,frames,body};
}
const enc=s=>new TextEncoder().encode(s);
const PACKAGE={'nemo-plugin.json':enc('{"name":"Fixture","id":"fixture","main":"index.html"}'),'index.html':enc('<html><head></head><body>x</body></html>')};

test('on a native host a mounted plugin gets the native facade through the bridge; the legacy builder is never touched (I34)',async()=>{
  const f=fixture(),dom=fakeDom(),h=host({__TAURI__:{core:{}},NemoScriptNativeApi:Api,NemoScriptNativePlugin:Plugin,document:dom.document,DOMParser:dom.DOMParser,btoa,...f.root});
  const loaded=h.ctx.SMPlugin.loadFiles(PACKAGE);
  assert.equal(loaded.manifest.id,'fixture');assert.equal(h.ctx.SMPlugin.openCount(),1);assert.equal(h.ctx.SMPlugin.isOpen('fixture'),true);
  assert.match(dom.frames[0].srcdoc,/parent\.SMPlugin\.hostApi\("fixture"/);assert.match(dom.frames[0].srcdoc,/function\(\)\{ return parent\.SMScript\.api\(\); \}/,'the legacy builder is only a thunk the host may call');
  const legacyThunk=()=>{h.calls.push('legacy');return h.legacy;},nemo=h.ctx.SMPlugin.hostApi('fixture',legacyThunk);
  assert.notEqual(nemo,h.legacy);assert.deepEqual(h.calls,[]);
  nemo.layer('A').name='Bridged';nemo.setTitle('Renamed panel');await turn();
  assert.deepEqual(f.sent,[['rename','a','Bridged']]);assert.equal(dom.frames.length,1);
  nemo.layer('B').visible=false;h.ctx.SMPlugin.closeAll();await turn();
  assert.deepEqual(f.sent,[['rename','a','Bridged']],'closing the panel discards the tick still open');
  assert.equal(h.ctx.SMPlugin.openCount(),0);assert.throws(()=>nemo.layerCount,/is closed/);assert.deepEqual(h.calls,[]);
  const missing=host({__TAURI__:{core:{}}});
  assert.throws(()=>missing.ctx.SMPlugin.hostApi('p1',()=>{missing.calls.push('legacy');return {};}),/module natif/);assert.deepEqual(missing.calls,[],'a native host without the module fails closed');
});

test('only a host with no native model falls back to the legacy builder, with close and setTitle attached',()=>{
  const h=host();const nemo=h.ctx.SMPlugin.hostApi('p1',()=>{h.calls.push('legacy');return h.legacy;});
  assert.equal(nemo,h.legacy);assert.deepEqual(h.calls,['legacy']);assert.equal(typeof nemo.close,'function');assert.equal(typeof nemo.setTitle,'function');
});

test('a malformed manifest or package throws before anything is mounted or written (I34)',async()=>{
  const f=fixture(),h=host({__TAURI__:{core:{}},NemoScriptNativeApi:Api,NemoScriptNativePlugin:Plugin,...f.root});
  assert.throws(()=>h.ctx.SMPlugin.loadFiles({}),/nemo-plugin\.json/);
  assert.throws(()=>h.ctx.SMPlugin.loadFiles({'nemo-plugin.json':enc('{not json')}),/illisible/);
  assert.throws(()=>h.ctx.SMPlugin.loadFiles({'nemo-plugin.json':enc('{"name":"x"}')}),/sans « main »/);
  assert.throws(()=>h.ctx.SMPlugin.loadFiles({'nemo-plugin.json':enc('{"name":"x","main":"index.html"}')}),/absent du paquet/);
  assert.equal(h.ctx.SMPlugin.openCount(),0);assert.deepEqual(f.sent,[]);assert.equal(f.info.contentRevision,0);
});

test('the plugin rows are bound host rows and the legacy script API is not handed out on a native host',()=>{
  for(const row of ['closeAll','openCount','setTitle']){const r=Api.ROUTES.find(x=>x.row==='sdk:nemo-plugin.'+row);assert.equal(r.kind,'host');assert.equal(r.bound,true,row);}
  const h=host({__TAURI__:{core:{}},NemoScriptNativeApi:Api,NemoScriptNativePlugin:Plugin});
  for(const name of ['closeAll','openCount','setTitle'])assert.equal(typeof h.ctx.SMPlugin[name],'function',name);
});
