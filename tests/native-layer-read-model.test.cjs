'use strict';
// #2371: the layer read model (kind, folder, component, matte, blend, opacity) crosses the metadata contract exactly.
const test=require('node:test'),assert=require('node:assert/strict');
const Metadata=require('../src/js/adapters/native-object-project-metadata.js');
const Files=require('../src/js/adapters/native-object-project-file.js');
const request={requestId:'model',instanceId:'instance',documentId:'document'};
const base=layers=>({apiVersion:2,requestId:'model',instanceId:'instance',documentId:'document',lifecycleGeneration:1,
  contentRevision:0,documentSnapshotId:'snapshot-0',documentDigest:'a'.repeat(64),byteCount:1000,canvasW:320,canvasH:180,
  fps:24,totalFrames:24,reference:null,undoDepth:0,redoDepth:0,layers});
const FULL={layerUid:'m',name:'M',visible:true,kind:'component',folderCollapsed:true,folderId:'f1',componentId:'sym',
  blendMode:'multiply',matteMode:'luma',matteSourceLayerUid:'src',timeLinkUid:'lead',followPathUid:'guide',mattesMore:[{uid:'x',mode:'alpha'}],isMatteSource:true,
  opacity:42.5,opacityAnimated:true};
test('every read-model member is copied into a frozen row and the top-level members beside it',()=>{
  const raw={...base([FULL,{layerUid:'plain'}]),layerFolders:[{folderId:'f1',collapsed:false,name:'F'},{folderId:'f2',collapsed:true}],
    layerModelUnavailable:['linkGroup','kind:video']};
  const read=Metadata.decode(raw,request);
  assert.deepEqual(read.layers[0],FULL);assert.deepEqual(read.layers[1],{layerUid:'plain'});
  assert.deepEqual(read.layerFolders,raw.layerFolders);assert.deepEqual(read.layerModelUnavailable,['linkGroup','kind:video']);
  assert.notEqual(read.layers[0].mattesMore,raw.layers[0].mattesMore);
  assert.equal(Object.isFrozen(read.layers[0].mattesMore[0]),true);assert.equal(Object.isFrozen(read.layerFolders[0]),true);
});
test('a read without the model keeps its exact earlier shape (no invented member)',()=>{
  const read=Metadata.decode(base([{layerUid:'a',name:'A',visible:true,locked:false}]),request);
  assert.deepEqual(read.layers,[{layerUid:'a',name:'A',visible:true,locked:false}]);
  assert.equal('layerFolders' in read,false);assert.equal('layerModelUnavailable' in read,false);
});
test('malformed members are refused before any UI projection',()=>{
  const bad=[{kind:'folderish'},{kind:3},{folderCollapsed:'yes'},{folderId:''},{componentId:7},{blendMode:null},{matteMode:[]},
    {matteSourceLayerUid:''},{timeLinkUid:''},{followPathUid:7},{mattesMore:{}},{mattesMore:[{uid:'x'}]},{mattesMore:[{uid:'x',mode:'a',extra:1}]},
    {isMatteSource:false},{opacityAnimated:1},{opacity:'50'},{opacity:Infinity},{opacity:NaN}];
  for(const extra of bad)assert.throws(()=>Metadata.decode(base([{layerUid:'a',...extra}]),request),/uncorrelated/,JSON.stringify(extra));
  for(const top of [{layerFolders:{}},{layerFolders:[{folderId:'',collapsed:true}]},{layerFolders:[{folderId:'f',collapsed:1}]},
    {layerFolders:[{folderId:'f',collapsed:true,name:3}]},{layerFolders:[{folderId:'f',collapsed:true,extra:1}]},
    {layerModelUnavailable:'linkGroup'},{layerModelUnavailable:[1]}])
    assert.throws(()=>Metadata.decode({...base([{layerUid:'a'}]),...top},request),/uncorrelated/,JSON.stringify(top));
});
test('the layer table admits the model members without copying them into UI state rows',()=>{
  const root={state:{},userLayers:[],selectedPaths:[]};
  const info=Metadata.decode(base([FULL,{layerUid:'plain',kind:'draw'}]),request);
  Files.projectView(root,info);
  assert.deepEqual(root.state.layers.map(l=>l.layerUid),['m','plain']);
  for(const row of root.state.layers)for(const key of ['kind','folderCollapsed','componentId','blendMode','matteMode','opacity'])
    assert.equal(Object.hasOwn(row,key),false,key);
  assert.equal(Files.projectionMatches(root,info),true);
});
test('the layer table still refuses a member that is not part of any contract',()=>{
  const root={state:{},userLayers:[],selectedPaths:[]};
  assert.throws(()=>Files.projectView(root,{...base([{layerUid:'a',surprise:1}])}),/unsupported/);
});
// #2371 review: the engine emits lists and strings as stored; a valid answer must never read as "uncorrelated".
test('decoder limits admit everything the engine can emit (long lists, empty and long strings, hundreds of rich rows)',()=>{
  const uid=n=>'ly_'+String(n).padStart(5,'0')+'_mh3k2j9a4f';
  const rich=n=>({layerUid:uid(n),name:'Layer '+n+' — Forme '+(n%17)+' (détail très long)',visible:true,locked:false,shy:false,
    color:'#5FA875',parentLayerUid:n?uid(n-1):null,kind:'draw',folderId:'folder-'+(n%40),blendMode:'multiply',matteMode:'alphaInverted',
    matteSourceLayerUid:uid(n+1),mattesMore:[{uid:uid(n+2),mode:'luma'},{uid:uid(n+3),mode:'alpha'}],isMatteSource:true,opacity:50.5,opacityAnimated:true});
  const rows=Array.from({length:400},(_,n)=>rich(n));
  const folders=Array.from({length:2000},(_,n)=>({folderId:'fold-'+n,collapsed:n%2===0,name:'F'+n}));
  const read=Metadata.decode({...base(rows),layerFolders:folders,layerModelUnavailable:['linkGroup']},request);
  assert.equal(read.layers.length,400);assert.deepEqual(read.layers[399],rows[399]);assert.equal(read.layerFolders.length,2000);
  const many=Array.from({length:5000},(_,n)=>({uid:'s'+n,mode:''}));
  const long='x'.repeat(1024*1024);
  const edge=Metadata.decode(base([{layerUid:'a',blendMode:'',matteMode:'',mattesMore:many,matteSourceLayerUid:long,folderId:long,componentId:long}]),request);
  assert.equal(edge.layers[0].mattesMore.length,5000);assert.equal(edge.layers[0].matteMode,'');assert.equal(edge.layers[0].folderId.length,long.length);
  // The route refuses any answer over 4 MiB, so a single string past that can never be a real answer.
  const over='y'.repeat(4*1024*1024+1);
  for(const extra of [{folderId:over},{matteSourceLayerUid:over},{blendMode:over},{mattesMore:[{uid:'u',mode:over}]}])
    assert.throws(()=>Metadata.decode(base([{layerUid:'a',...extra}]),request),/uncorrelated/);
  assert.throws(()=>Metadata.decode({...base([{layerUid:'a'}]),layerFolders:[{folderId:'f',collapsed:true,name:over}]},request),/uncorrelated/);
  // Empty ids stay refused: engine admission never lets one through.
  assert.throws(()=>Metadata.decode(base([{layerUid:'a',mattesMore:[{uid:'',mode:'alpha'}]}]),request),/uncorrelated/);
});
