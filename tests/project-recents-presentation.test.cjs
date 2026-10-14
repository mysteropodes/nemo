'use strict';
const assert=require('node:assert/strict');
const fs=require('node:fs');
const vm=require('node:vm');
const test=require('node:test');
const entrySource=fs.readFileSync('src/js/project-entry.js','utf8');
const fixtureSource=fs.readFileSync('tests/project-open.test.cjs','utf8');
const start=fixtureSource.indexOf('function harness('),end=fixtureSource.indexOf('\nfunction select(',start);
assert.ok(start>=0&&end>start,'reuse only the existing test DOM fixture, never a production classifier');
const seedAnchor='const downloads = [], toasts = [], frames = [], writes = [], windowEvents = {};';
const harnessSource=fixtureSource.slice(start,end);
assert.equal(harnessSource.split(seedAnchor).length,2);
const rows=[
  {path:'/owned/A.json',name:'same <name> & text',canvasW:320,canvasH:180,fps:24,lastOpened:1700000000000},
  {path:'/owned/B.json',name:'same <name> & text',lastOpened:1700000100000}
];
const plain=value=>JSON.parse(JSON.stringify(value));
const nextTurn=()=>new Promise(resolve=>setImmediate(resolve));
function fixture(seed=rows,configureWindow=()=>{}){
  const context=vm.createContext({assert,vm,require,setTimeout,console,recentSeed:JSON.stringify(seed),
    source:fs.readFileSync('src/js/project.js','utf8'),entrySource,
    documentSource:fs.readFileSync('src/js/project-document.js','utf8')});
  // Seed only test-owned storage before DOMContentLoaded; shipped scripts stay intact.
  const seeded=harnessSource.replace(seedAnchor,
    "const downloads = [], toasts = [], frames = [], writes = [['nemo-recents',recentSeed]], windowEvents = {};");
  vm.runInContext(seeded+'\nthis.makeHarness=harness;',context);
  const app=context.makeHarness({configureWindow});
  return {app,context};
}
function stored(app){return JSON.parse(app.writes.findLast(([key])=>key==='nemo-recents')[1]);}
function directView(){
  const root={};vm.runInNewContext(entrySource,{window:root});
  const make=()=>{
    const node={children:[],listeners:{},className:'',textContent:'',title:'',
      appendChild(child){this.children.push(child);},addEventListener(type,fn){this.listeners[type]=fn;}};
    Object.defineProperty(node,'innerHTML',{get(){return this.html||'';},set(value){this.html=value;this.children=[];}});
    return node;
  };
  const list=make();return {api:root.SMProjectEntry,list,document:{getElementById:()=>list,createElement:make}};
}
test('view owns localized empty state and checks missing root before all ports',()=>{
  const f=directView(),calls=[];
  f.api.renderRecents(f.document,{readRows:()=>[],translate:key=>{calls.push(key);return 'No recent projects';}});
  assert.equal(f.list.innerHTML,'<div class="start-empty">No recent projects</div>');
  assert.deepEqual(calls,['startNoRecents']);
  f.api.renderRecents({getElementById:()=>null},new Proxy({},{get(){throw Error('no port may be resolved');}}));
});
test('shipped startup retains the baseline empty display for a stored value without list length',()=>{
  const {app}=fixture({});
  assert.equal(app.element('start-recent-list').children.length,0);
  assert.deepEqual(stored(app),{});
});
test('view preserves order, duplicate names, literal text and dimension/date presentation',()=>{
  const f=directView(),calls=[],display=rows.map(({path,...row},key)=>({...row,key}));
  f.api.renderRecents(f.document,{readRows:()=>display,translate:key=>key,openRow:key=>calls.push(['open',key]),removeRow:key=>calls.push(['remove',key])});
  assert.equal(f.list.children.length,2);
  const first=f.list.children[0],second=f.list.children[1];
  assert.equal(first.children[1].children[0].textContent,'same <name> & text');
  assert.equal(second.children[1].children[0].textContent,'same <name> & text');
  assert.equal(first.children[1].children[1].textContent,'320×180 · 24fps · '+new Date(rows[0].lastOpened).toLocaleDateString());
  assert.equal(second.children[1].children[1].textContent,new Date(rows[1].lastOpened).toLocaleDateString());
  assert.equal(first.children[2].title,'startRemoveFromList');
  assert.deepEqual(calls,[],'rendering performs no Open/remove authority action');
  second.listeners.click();let stopped=false;
  first.children[2].listeners.click({stopPropagation(){stopped=true;}});
  assert.equal(stopped,true);assert.deepEqual(calls,[['open',1],['remove',0]]);
});
test('shipped removal rerenders path-free rows without document or history changes',()=>{
  const {app}=fixture(),seen=[];
  const render=app.window.SMProjectEntry.renderRecents;
  app.window.SMProjectEntry.renderRecents=(document,ports)=>render(document,{...ports,readRows(){const result=ports.readRows();seen.push(plain(result));return result;}});
  const before=plain({writes:app.writes,json:app.json,mutations:app.mutations,label:app.project.getCurrentLabel()});
  // Real re-entry refresh path, not a private call to project.js internals.
  app.element('start-recent-list').children[0].children[2].listeners.click({stopPropagation(){}});
  assert.equal(seen.length,1);assert.equal('path' in seen[0][0],false);
  assert.deepEqual(Object.keys(seen[0][0]).sort(),['key','lastOpened','name']);
  assert.equal(app.json,before.json);assert.equal(app.mutations,before.mutations);
  assert.equal(app.project.getCurrentLabel(),before.label);
  assert.deepEqual(stored(app),[rows[1]]);
  assert.equal(app.writes.length,before.writes.length+1,'only the ordinary remove storage write occurs');
});
test('shipped row click uses its captured path even if stored order changes; read refusal publishes nothing',async()=>{
  const reads=[];
  const {app}=fixture(rows,root=>{root.__TAURI__={fs:{async readTextFile(path){reads.push(path);throw Error('missing fixture');}},core:{invoke(){}}};});
  const row=app.element('start-recent-list').children[1];
  app.writes.push(['nemo-recents',JSON.stringify([...rows].reverse())]);
  const before=plain({writes:app.writes,json:app.json,mutations:app.mutations,label:app.project.getCurrentLabel()});
  row.listeners.click();await nextTurn();
  assert.deepEqual(reads,['/owned/B.json']);
  assert.deepEqual(plain(app.writes),before.writes);assert.equal(app.json,before.json);
  assert.equal(app.mutations,before.mutations);assert.equal(app.project.getCurrentLabel(),before.label);
  assert.ok(app.toasts.includes('Could not read file — it may have moved or been deleted'));
});
test('ordinary Open picker cancel does not change recent rows or publish a document',async()=>{
  let reads=0;
  const {app}=fixture(rows,root=>{root.__TAURI__={dialog:{async open(){return null;}},fs:{async readTextFile(){reads++;}},core:{invoke(){}}};});
  const before=plain({writes:app.writes,json:app.json,mutations:app.mutations});
  await app.project.open();
  assert.equal(reads,0);assert.deepEqual(plain(app.writes),before.writes);
  assert.equal(app.json,before.json);assert.equal(app.mutations,before.mutations);
  assert.equal(app.element('start-recent-list').children.length,2);
});
test('native busy-Open refusal from a real recent row retains document/history/recents',async()=>{
  let reads=0;const identity={documentId:'native-A',generation:1};
  const {app}=fixture(rows,root=>{root.__TAURI__={fs:{async readTextFile(){reads++;}},core:{invoke(){}}};
    root.NemoNativeObjectProjectFile={bind:()=>({canOpen:()=>false,isActive:()=>true,isDirty:()=>false})};
    root.NemoNativeOpacityCutover={blocksLegacy:()=>true,isActive:()=>true,getNativeIdentity:()=>identity,persistenceJSON:()=>'{"native":true}'};});
  const before=plain({writes:app.writes,json:app.json,mutations:app.mutations});
  app.element('start-recent-list').children[0].listeners.click();await nextTurn();
  assert.equal(reads,0);assert.deepEqual(plain(app.writes),before.writes);
  assert.equal(app.json,before.json);assert.equal(app.mutations,before.mutations);
  assert.ok(app.toasts.includes('Native project Open is unavailable while another Open or Save is in progress.'));
});
test('successful ordinary native Open republishes recents only after current presented admission',async()=>{
  const reads=[];
  const {app,context}=fixture(rows,root=>{root.__TAURI__={fs:{async readTextFile(path){reads.push(path);return '{"supported":true}';}},core:{invoke(){}}};});
  const a=fixtureSource.indexOf('function deferred()'),b=fixtureSource.indexOf("\ntest('browser Resume",a);
  assert.ok(a>=0&&b>a);
  vm.runInContext(fixtureSource.slice(a,b)+'\nthis.installNativeOpen=installNativeOpen;',context);
  const native=context.installNativeOpen(app),before=plain(app.writes);
  app.element('start-recent-list').children[1].listeners.click();await nextTurn();
  assert.deepEqual(plain(app.writes),before,'no publication while native presentation is pending');
  native.first.resolve(native.receipt);native.visible.resolve(native.receipt);await nextTurn();
  assert.deepEqual(reads,['/owned/B.json']);
  assert.equal(stored(app)[0].path,'/owned/B.json');assert.equal(stored(app)[0].name,'B');
  assert.equal(app.element('start-recent-list').children[0].children[1].children[0].textContent,'B');
  assert.equal(app.project.isDirty(),false);assert.equal(app.mutations,0);
});
test('successful native Save As updates storage and the shipped recent view through ordinary publication',async()=>{
  const current={id:'native-A',revision:7,generation:1,bytes:'{"native":"A"}'},writes=[];
  const {app}=fixture([],root=>{
    root.SM.exportJSON=()=>{throw Error('Paper writer forbidden');};
    root.NemoNativeOpacityCutover={blocksLegacy:()=>true,isActive:()=>true,
      identity:()=>({instanceId:'native-instance',documentId:current.id,contentRevision:current.revision}),
      getNativeIdentity:()=>({documentId:current.id,generation:current.generation}),persistenceJSON:()=>current.bytes};
    root.__TAURI__={core:{invoke(){}},dialog:{save:async()=>'/owned/A.json'},
      fs:{async writeTextFile(path,bytes){writes.push([path,bytes]);},async rename(){},async remove(){}}};
  });
  app.project.enterEditor();const before=plain(current);await app.project.saveAs();
  assert.deepEqual(plain(current),before);assert.ok(writes.length>0);
  assert.equal(stored(app)[0].path,'/owned/A.json');assert.equal(stored(app)[0].name,'A');
  assert.equal(app.element('start-recent-list').children[0].children[1].children[0].textContent,'A');
  assert.equal(app.project.isDirty(),false);
});
