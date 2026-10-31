(function(root,factory){var api=factory();if(typeof module==='object'&&module.exports)module.exports=api;else root.NemoNativeObjectProjectMetadata=api;}(typeof globalThis==='object'?globalThis:this,function(){'use strict';
  function id(v){return typeof v==='string'&&/^[A-Za-z0-9][A-Za-z0-9._:/-]{0,127}$/.test(v);}
  function integer(v,min){return Number.isSafeInteger(v)&&v>=min;}
  function freeze(v){if(v&&typeof v==='object'){Object.keys(v).forEach(function(k){freeze(v[k]);});Object.freeze(v);}return v;}
  // #2371 layer read model: optional rows members (kind, folder, component, matte, blend, opacity) and two top-level members.
  var KINDS=['draw','image','sequence','video','camera','effect','text','null','folder','component','montage','widget','effector','guide','unknown'];
  var ROW_KEYS=['kind','folderCollapsed','folderId','componentId','blendMode','matteMode','matteSourceLayerUid','timeLinkUid','followPathUid','followPathAlign','mattesMore','isMatteSource','opacity','opacityAnimated','effects','effectsOversize','pathFx','pathFxOversize'];
  function has(o,k){return Object.prototype.hasOwnProperty.call(o,k);}
  // Bounds match what the engine can emit, not a tighter guess: the desktop route refuses a whole project-info answer above
  // 4 MiB (PROJECT_INFO_LIMIT_BYTES, native_object_project_route.rs), so no string or list in an answer that reached
  // here can be longer than that. Modes and blend names are stored as given (a file may carry any string, even empty);
  // ids are non-empty because engine admission refuses an empty reference, folder id or symbol id.
  var MAX_INFO=4*1024*1024;
  function str(v){return typeof v==='string'&&v.length<=MAX_INFO;}
  function text(v){return str(v)&&v.length>0;}
  // A stack travels in a row only up to the engine's 4096-byte row bound (EFFECTS_ROW_BYTES), so even the smallest entry
  // ({"type":"a","enabled":true}, 27 bytes) gives at most ~150 entries: 256 is above that, so an engine answer never trips it.
  // A longer stack reaches here as `effectsOversize` / `pathFxOversize`, which are admitted as flags.
  var MAX_STACK=256;
  function rowModel(l){
    return (!has(l,'kind')||KINDS.indexOf(l.kind)>=0)&&(!has(l,'folderCollapsed')||typeof l.folderCollapsed==='boolean')&&
      ['folderId','componentId','matteSourceLayerUid','timeLinkUid','followPathUid'].every(function(k){return !has(l,k)||text(l[k]);})&&
      ['blendMode','matteMode'].every(function(k){return !has(l,k)||str(l[k]);})&&
      (!has(l,'mattesMore')||Array.isArray(l.mattesMore)&&l.mattesMore.length<=MAX_INFO&&l.mattesMore.every(function(m){return m&&Object.keys(m).length===2&&text(m.uid)&&str(m.mode);}))&&
      ['isMatteSource','followPathAlign','opacityAnimated','effectsOversize','pathFxOversize'].every(function(k){return !has(l,k)||l[k]===true;})&&
      (!has(l,'effects')||Array.isArray(l.effects)&&l.effects.length>0&&l.effects.length<=MAX_STACK&&!has(l,'effectsOversize')&&l.effects.every(function(e){return e&&typeof e==='object'&&!Array.isArray(e)&&str(e.type)&&typeof e.enabled==='boolean';}))&&(!has(l,'pathFx')||Array.isArray(l.pathFx)&&l.pathFx.length>0&&l.pathFx.length<=MAX_STACK&&!has(l,'pathFxOversize')&&l.pathFx.every(function(e){return e&&typeof e==='object'&&!Array.isArray(e)&&text(e.type);}))&&(!has(l,'opacity')||Number.isFinite(l.opacity));
  }
  function topModel(v){
    return (!has(v,'layerFolders')||Array.isArray(v.layerFolders)&&v.layerFolders.length<=MAX_INFO&&v.layerFolders.every(function(f){
      return f&&text(f.folderId)&&typeof f.collapsed==='boolean'&&(!has(f,'name')||str(f.name))&&Object.keys(f).length===(has(f,'name')?3:2);}))&&
      (!has(v,'layerModelUnavailable')||Array.isArray(v.layerModelUnavailable)&&v.layerModelUnavailable.length<=32&&v.layerModelUnavailable.every(text));
  }
  function clone(v){return JSON.parse(JSON.stringify(v));}
  function same(a,b){return !!a&&!!b&&['instanceId','documentId','contentRevision','documentSnapshotId','lifecycleGeneration'].every(function(k){return a[k]===b[k];});}
  function metadata(v,request){
    if(!v||v.apiVersion!==2||v.requestId!==request.requestId||v.instanceId!==request.instanceId||v.documentId!==request.documentId||
      !id(v.documentSnapshotId)||!integer(v.contentRevision,0)||!integer(v.lifecycleGeneration,1)||
      !/^[a-f0-9]{64}$/.test(v.documentDigest)||!integer(v.byteCount,1)||v.byteCount>32*1024*1024||
      !integer(v.canvasW,1)||!integer(v.canvasH,1)||!integer(v.totalFrames,1)||!Number.isFinite(v.fps)||!(v.fps>0)||
      (Object.prototype.hasOwnProperty.call(v,'canvasBg')&&(typeof v.canvasBg!=='string'||!/^#?(?:[a-fA-F0-9]{3}|[a-fA-F0-9]{6}|[a-fA-F0-9]{8})$/.test(v.canvasBg)))||
      ['waIn','waOut'].some(function(k){return Object.prototype.hasOwnProperty.call(v,k)&&!Number.isFinite(v[k]);})||
      !Array.isArray(v.layers)||!v.layers.every(function(l){return l&&typeof l.layerUid==='string'&&
        (!Object.prototype.hasOwnProperty.call(l,'name')||typeof l.name==='string')&&
        (!Object.prototype.hasOwnProperty.call(l,'visible')||typeof l.visible==='boolean')&&
        ['locked','shy'].every(function(k){return !Object.prototype.hasOwnProperty.call(l,k)||typeof l[k]==='boolean';})&&
        (!Object.prototype.hasOwnProperty.call(l,'widget')||l.widget===true)&&
        (!Object.prototype.hasOwnProperty.call(l,'color')||typeof l.color==='string'&&/^#[a-fA-F0-9]{6}$/.test(l.color))&&
        (!Object.prototype.hasOwnProperty.call(l,'parentLayerUid')||l.parentLayerUid===null||
          typeof l.parentLayerUid==='string'&&l.parentLayerUid.length>0)&&rowModel(l);})||!topModel(v))throw new Error('Native project metadata is uncorrelated');
    // Copy only admitted UI fields; embedded files/objects never become JS state.
    var result={};['apiVersion','instanceId','documentId','contentRevision','documentSnapshotId','lifecycleGeneration',
      'documentDigest','byteCount','canvasW','canvasH','fps','totalFrames','undoDepth','redoDepth'].forEach(function(k){result[k]=v[k];});
    ['canvasBg','waIn','waOut'].forEach(function(k){if(Object.prototype.hasOwnProperty.call(v,k))result[k]=v[k];});
    result.layers=v.layers.map(function(l){var row={layerUid:l.layerUid};
      if(Object.prototype.hasOwnProperty.call(l,'name'))row.name=l.name;
      if(Object.prototype.hasOwnProperty.call(l,'visible'))row.visible=l.visible;
      ['locked','shy','color','parentLayerUid','widget'].concat(ROW_KEYS).forEach(function(k){if(has(l,k))row[k]=clone(l[k]);});return row;});
    ['layerFolders','layerModelUnavailable'].forEach(function(k){if(has(v,k))result[k]=clone(v[k]);});
    var r=v.reference;
    if(r!==null&&(!r||!id(r.id)||typeof r.name!=='string'||typeof r.visible!=='boolean'||!Number.isFinite(r.opacity)||
      r.opacity<0||r.opacity>1||!Number.isSafeInteger(r.offsetFrames)))throw new Error('Native reference metadata is invalid');
    result.reference=r===null?null:{id:r.id,name:r.name,visible:r.visible,opacity:r.opacity,offsetFrames:r.offsetFrames};
    return freeze(result);
  }
  return Object.freeze({decode:metadata});
}));
