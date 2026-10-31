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
  blendMode:'multiply',matteMode:'luma',matteSourceLayerUid:'src',mattesMore:[{uid:'x',mode:'alpha'}],isMatteSource:true,
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
    {matteSourceLayerUid:''},{mattesMore:{}},{mattesMore:[{uid:'x'}]},{mattesMore:[{uid:'x',mode:'a',extra:1}]},
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
