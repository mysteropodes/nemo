'use strict';
// #2371 U11: the native layer rows draw the baseline (1065e9d8 renderLayerList) decorations for the facts the read model
// carries. The DOM diff runs the baseline renderer itself (tests/helpers/baseline-layer-rows.cjs) and compares class-normalised
// projections (CLAUDE.md 5bis); when the baseline commit is not in the local object store only the committed capture is used.
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path');
const UI=require('../src/js/adapters/native-object-ui.js'),Files=require('../src/js/adapters/native-object-project-file.js');
const Presenter=require('../src/js/adapters/native-layer-list-presenter.js'),Range=require('../src/js/adapters/native-layer-range-desktop.js');
const H=require('./helpers/baseline-layer-rows.cjs'),kindsFor=require('./helpers/layer-kind-global.cjs');
const BASELINE='1065e9d8',CAPTURE=path.join(__dirname,'fixtures','native-layer-row-baseline.json');
// One fixture, two spellings: baseline layer data (ld) and the native read-model rows that describe the same layers.
const LD=[
 {layerUid:'u0',name:'Backdrop',color:'#aa3333',visible:true},
 {layerUid:'u1',name:'Open',isFolderLayer:true,folderCollapsed:false,color:'#3366cc',visible:true},
 {layerUid:'u2',name:'Child',parentLayerUid:'u1',color:'#33aa33',blendMode:'multiply',visible:true},
 {layerUid:'u3',name:'Instance',symbolId:'s1',visible:true},
 {layerUid:'u4',name:'Masked',matteMode:'alpha',matteSourceLayerUid:'u5',visible:true},
 {layerUid:'u5',name:'Source',visible:true},
 {layerUid:'u6',name:'Closed',isFolderLayer:true,folderCollapsed:true,visible:true},
 {layerUid:'u7',name:'Inner',parentLayerUid:'u6',color:'#999999',visible:true}];
const ROWS=[
 {layerUid:'u0',name:'Backdrop',visible:true,color:'#aa3333',kind:'draw'},
 {layerUid:'u1',name:'Open',visible:true,color:'#3366cc',kind:'folder',folderCollapsed:false},
 {layerUid:'u2',name:'Child',visible:true,color:'#33aa33',kind:'draw',parentLayerUid:'u1',blendMode:'multiply',opacity:50},
 {layerUid:'u3',name:'Instance',visible:true,kind:'component',componentId:'s1'},
 {layerUid:'u4',name:'Masked',visible:true,kind:'draw',matteMode:'alpha',matteSourceLayerUid:'u5'},
 {layerUid:'u5',name:'Source',visible:true,kind:'draw',isMatteSource:true},
 {layerUid:'u6',name:'Closed',visible:true,kind:'folder',folderCollapsed:true},
 {layerUid:'u7',name:'Inner',visible:true,color:'#999999',kind:'draw',parentLayerUid:'u6'}];
// The baseline draws neither the shape-tree disclosure (shapesGroups*) nor blend/opacity badges on the row.
function comparable(p){return {rowClasses:p.rowClasses,nodes:p.nodes.filter(n=>!/shapesGroups/.test(n.title)&&!/native-layer-(blend|opacity)/.test(n.cls))};}
// Baseline click-to-change hints and picker titles are interactive text; the structure (classes, glyph markup, colour) is compared.
function strip(p){return {rowClasses:p.rowClasses,nodes:p.nodes.map(n=>({...n,title:/lkind/.test(n.cls)||/Component|Calque d/.test(n.title)?n.title:''}))};}
function baselineCapture(){
 let text=null;try{text=H.source(BASELINE,'src/js/timeline.js');}catch(_){}
 return text?H.renderBaseline(text,LD.map(l=>({...l}))):null;
}
function rowsFor(rows,extra={}){
 const doc=H.fakeDocument(),info={layers:rows.map(r=>({...r})),layerFolders:[],layerModelUnavailable:[],...extra};
 return Presenter.render({},{document:doc,info,selection:[],shyMode:false,policyOwner:null,onPick(){},onVisibility(){},onPolicy(){},
  layerKind:kindsFor({}),blendLabels:{multiply:'Multiply'},matteLabel:m=>m}).rows;
}

test('decorations match the baseline renderer row for row (kind icon, folder arrow, colour dot, component and matte badges, indent class)',()=>{
 const stored=JSON.parse(fs.readFileSync(CAPTURE,'utf8')),live=baselineCapture();
 if(live)assert.deepEqual(live.map(r=>({layerUid:r.layerUid,row:strip(comparable(r.row))})),stored.map(r=>({layerUid:r.layerUid,row:strip(r.row)})),'committed capture is what the baseline renders');
 const rows=rowsFor(ROWS);assert.equal(rows.length,ROWS.length);
 const byUid=Object.fromEntries(stored.map(r=>[r.layerUid,r.row]));
 rows.forEach((row,i)=>{
  // The baseline omits the children of a collapsed folder (u7); the native list keeps every row so the grid stays row-for-row.
  if(!byUid[ROWS[i].layerUid]){assert.equal(ROWS[i].layerUid,'u7');return;}
  assert.deepEqual(strip(comparable(H.project(row))),strip(byUid[ROWS[i].layerUid]),'row '+ROWS[i].layerUid);
 });
});

test('blend and opacity indicators show only a non-default value and say what they are',()=>{
 const child=H.project(rowsFor(ROWS)[2]);
 const blend=child.nodes.find(n=>/native-layer-blend/.test(n.cls)),opacity=child.nodes.find(n=>/native-layer-opacity/.test(n.cls));
 assert.equal(blend.title,'Blend: Multiply');assert.equal(opacity.title,'Opacity: 50%');assert.match(opacity.html,/50%/);
 const plain=rowsFor([{layerUid:'a',kind:'draw',blendMode:'normal',opacity:100},{layerUid:'b',kind:'draw',opacity:100,opacityAnimated:true},{layerUid:'c',kind:'draw'}]);
 assert.equal(H.project(plain[0]).nodes.length,0,'normal blend at 100% adds nothing');
 assert.match(H.project(plain[1]).nodes[0].title,/animated/);assert.equal(H.project(plain[2]).nodes.length,0);
});

test('a folder chevron is display only and says native collapse is unavailable; children are not hidden',()=>{
 const rows=rowsFor(ROWS),arrow=rows[6].children.find(c=>/larrow/.test(c.className));
 assert.equal(arrow.textContent,'▸');assert.match(arrow.title,/native folder collapse is unavailable/);assert.equal(arrow.dataset.unavailable,'true');
 assert.equal(rows[7].hidden,false,'a collapsed folder keeps its children (the grid reserves their rows)');
});

test('a layer carrying none of the facts renders only its four controls; unknown, unavailable and malformed facts invent nothing',()=>{
 const rows=rowsFor([{layerUid:'a'},{layerUid:'b',kind:'unknown'},{layerUid:'c',kind:'video'}]);
 assert.deepEqual(rows.slice(0,2).map(r=>r.children.length),[4,4]);
 assert.ok(rows[2].children.some(c=>/lkind-video/.test(c.className)),'a video row renders the baseline video icon');
 const coloured=rowsFor([{layerUid:'d',kind:'camera',color:'not a colour'}]);
 assert.equal(coloured[0].children.some(c=>/layer-color-dot/.test(c.className)),true,'a colour slot is shared down the list');
 assert.equal(H.project(coloured[0]).nodes.find(n=>/layer-color-dot/.test(n.cls)).dot,'#8b8b9e','a malformed colour falls back to the baseline neutral dot');
});

test('organisational folder membership indents the row and names the folder without adding a row',()=>{
 const rows=rowsFor([{layerUid:'a',kind:'draw',folderId:'f1'},{layerUid:'b',kind:'draw'}],{layerFolders:[{folderId:'f1',name:'Sketches',collapsed:true}]});
 assert.equal(rows.length,2);assert.ok(rows[0].className.split(' ').includes('in-folder'));assert.match(rows[0].title,/Folder: Sketches \(collapsed/);
 assert.equal(rows[1].className.split(' ').includes('in-folder'),false);
});

// ---- the native UI: row alignment and the frameOnly contract ----
function rootFor(){
 const doc=H.fakeDocument(),nodes=new Map(['layer-list','frame-hdr','frame-grid'].map(id=>[id,Object.assign(new H.Node('div'),{id})]));
 doc.getElementById=id=>nodes.get(id)||null;doc.addEventListener=()=>{};
 return {state:{currentFrame:1,activeLayerIdx:0,hideShy:false},userLayers:[],document:doc,_layerSel:[0],FC:10,addEventListener(){},NemoNativeObjectProjectFile:Files,
  NemoNativeObjectUI:UI,NemoNativeLayerRangeDesktop:Range,SMLayerKind:kindsFor({}),BLEND_MODE_LABELS:{multiply:'Multiply'}};
}
function info(layers,rev=3,extra={}){return {apiVersion:2,instanceId:'instance',documentId:'document',lifecycleGeneration:1,contentRevision:rev,documentSnapshotId:'document-snapshot-'+rev,
 documentDigest:'4'.repeat(64),byteCount:1000,canvasW:320,canvasH:180,fps:24,totalFrames:24,reference:null,undoDepth:0,redoDepth:0,layers:layers.map(l=>({...l})),...extra};}
function paint(r,value,frameOnly){
 Files.projectView(r,value);r._layerSel=[0];
 const ports=r.ports||(r.ports={projectionReady:i=>Files.projectionReadyForUI(r,i),currentSession:()=>null,selectLayer:()=>true,repaint:()=>true,readShyMode:()=>false});
 assert.equal(UI.render(r,value,frameOnly,ports),true);return ports;
}

test('left rows equal right rows: one frame-grid row per layer, folder layers and folder members included',()=>{
 const r=rootFor(),value=info(ROWS,3,{layerFolders:[{folderId:'f1',name:'F',collapsed:false}]});
 value.layers[0].folderId='f1';
 paint(r,value,false);
 const left=r.document.getElementById('layer-list').children,grid=r.document.getElementById('frame-grid');
 const right=grid.children.find(c=>/native-layer-range-rows/.test(c.className)).children;
 assert.equal(left.length,ROWS.length);assert.equal(right.length,left.length,'every left row has its right row');
 left.forEach((row,i)=>{assert.equal(right[i].dataset.layerUid,row.dataset.layerUid,'row '+i+' pairs the same layer');assert.equal(!!right[i].hidden,!!row.hidden);});
});

test('frameOnly keeps the decorated rows; a changed decoration fact rebuilds them',()=>{
 const r=rootFor(),list=r.document.getElementById('layer-list');
 paint(r,info(ROWS),false);const before=list.children.slice();
 paint(r,info(ROWS),true);
 assert.ok(list.children.every((row,i)=>row===before[i]),'a frame change reuses every row, decorations included');
 paint(r,info(ROWS.map((l,i)=>i===2?{...l,opacity:25}:{...l}),4),true);
 assert.notStrictEqual(list.children[2],before[2],'a new opacity fact is shown');
 assert.ok(H.project(list.children[2]).nodes.some(n=>/native-layer-opacity/.test(n.cls)&&/25%/.test(n.html)));
});

test('frameOnly with an unchanged revision serialises nothing and reuses every row; a new revision rebuilds',()=>{
 const r=rootFor(),list=r.document.getElementById('layer-list');
 paint(r,info(ROWS),false);const before=list.children.slice();
 const real=JSON.stringify;let calls=0;JSON.stringify=function(){calls++;return real.apply(this,arguments);};
 try{
  const ports=r.ports,value=info(ROWS);Files.projectView(r,value);r._layerSel=[0];
  assert.equal(UI.render(r,value,true,ports),true);
  assert.equal(calls,0,'a frame advance runs no JSON.stringify over the layer rows or folders');
 }finally{JSON.stringify=real;}
 assert.ok(list.children.every((row,i)=>row===before[i]),'every row is reused');
 paint(r,info(ROWS,4),true);
 assert.notStrictEqual(list.children[0],before[0],'a new content revision rebuilds');
});

test('frameOnly reuse at 2000 layers stays far below a per-layer serialisation cost',()=>{
 const many=Array.from({length:2000},(_,i)=>({layerUid:'l'+i,name:'Layer '+i,visible:true,locked:false,shy:false}));
 const r=rootFor();paint(r,info(many),false);
 const ports=r.ports,value=info(many);Files.projectView(r,value);r._layerSel=[0];
 const t=process.hrtime.bigint();for(let k=0;k<5;k++)assert.equal(UI.render(r,value,true,ports),true);
 const ms=Number(process.hrtime.bigint()-t)/1e6/5;
 assert.ok(ms<25,'frameOnly render of 2000 layers took '+ms.toFixed(1)+' ms');
});

test('the four controls stay the first four children; flex order places the decorations around them',()=>{
 const r=rootFor(),list=r.document.getElementById('layer-list');
 paint(r,info(ROWS),false);
 const row=list.children[1],kids=row.children;
 assert.deepEqual(kids.slice(0,4).map(c=>c.dataset.nativeLayerDecor),[undefined,undefined,undefined,undefined]);
 assert.equal(kids[0].textContent,'Open \u2014 Visible');
 const decor=kids.slice(4);assert.ok(decor.length>=3&&decor.every(c=>c.dataset.nativeLayerDecor==='true'));
 assert.ok(decor.slice(0,3).every(c=>Number(c.style.order)<0),'arrow, dot and kind icon sit before the name');
 assert.equal(kids[0].style.order,'0');assert.ok(kids.slice(1,4).every(c=>c.style.order==='2'),'eye, lock and shy follow the badges');
 const plain=rowsFor([{layerUid:'x'}])[0];assert.equal(plain.children[0].style.order,undefined,'an undecorated row sets no order');
});
