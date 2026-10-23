(function(root,factory){var api=factory();if(typeof module==='object'&&module.exports)module.exports=api;else root.NemoNativeObjectProjectMetadata=api;}(typeof globalThis==='object'?globalThis:this,function(){'use strict';
  function id(v){return typeof v==='string'&&/^[A-Za-z0-9][A-Za-z0-9._:/-]{0,127}$/.test(v);}
  function integer(v,min){return Number.isSafeInteger(v)&&v>=min;}
  function freeze(v){if(v&&typeof v==='object'){Object.keys(v).forEach(function(k){freeze(v[k]);});Object.freeze(v);}return v;}
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
        (!Object.prototype.hasOwnProperty.call(l,'color')||typeof l.color==='string'&&/^#[a-fA-F0-9]{6}$/.test(l.color))&&
        (!Object.prototype.hasOwnProperty.call(l,'parentLayerUid')||l.parentLayerUid===null||
          typeof l.parentLayerUid==='string'&&l.parentLayerUid.length>0);}))throw new Error('Native project metadata is uncorrelated');
    // Copy only admitted UI fields; embedded files/objects never become JS state.
    var result={};['apiVersion','instanceId','documentId','contentRevision','documentSnapshotId','lifecycleGeneration',
      'documentDigest','byteCount','canvasW','canvasH','fps','totalFrames','undoDepth','redoDepth'].forEach(function(k){result[k]=v[k];});
    ['canvasBg','waIn','waOut'].forEach(function(k){if(Object.prototype.hasOwnProperty.call(v,k))result[k]=v[k];});
    result.layers=v.layers.map(function(l){var row={layerUid:l.layerUid};
      if(Object.prototype.hasOwnProperty.call(l,'name'))row.name=l.name;
      if(Object.prototype.hasOwnProperty.call(l,'visible'))row.visible=l.visible;
      ['locked','shy','color','parentLayerUid'].forEach(function(k){if(Object.prototype.hasOwnProperty.call(l,k))row[k]=l[k];});return row;});
    var r=v.reference;
    if(r!==null&&(!r||!id(r.id)||typeof r.name!=='string'||typeof r.visible!=='boolean'||!Number.isFinite(r.opacity)||
      r.opacity<0||r.opacity>1||!Number.isSafeInteger(r.offsetFrames)))throw new Error('Native reference metadata is invalid');
    result.reference=r===null?null:{id:r.id,name:r.name,visible:r.visible,opacity:r.opacity,offsetFrames:r.offsetFrames};
    return freeze(result);
  }
  return Object.freeze({decode:metadata});
}));
