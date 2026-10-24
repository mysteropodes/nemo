/* Pure entry validation; New/Open and retained activation keep distinct contracts. */
(function(root,factory){
  var api=factory();
  if(typeof module==='object'&&module.exports)module.exports=api;
  else root.NemoNativeObjectProjectEntryContract=api;
}(typeof globalThis==='object'?globalThis:this,function(){
  'use strict';
  function id(v){return typeof v==='string'&&/^[A-Za-z0-9][A-Za-z0-9._:/-]{0,127}$/.test(v);}
  function integer(v,min){return Number.isSafeInteger(v)&&v>=min;}
  function exact(v,keys){return !!v&&typeof v==='object'&&!Array.isArray(v)&&
    Object.keys(v).length===keys.length&&keys.every(function(k){return Object.hasOwn(v,k);});}
  function target(value){
    if(!value||value.kind!=='native-object-inactive'||!id(value.instanceId)||!id(value.documentId)||
        !id(value.ownerDocumentId)||value.documentId===value.ownerDocumentId||!integer(value.lifecycleGeneration,1)||
        !integer(value.contentRevision,0)||!id(value.documentSnapshotId)||!id(value.preparationToken)||
        !/^[a-f0-9]{64}$/.test(value.digest)||!integer(value.totalFrames,1)||
        !Array.isArray(value.layerUids)||new Set(value.layerUids).size!==value.layerUids.length||
        value.layerUids.some(function(uid){return typeof uid!=='string'||!uid;})||
        !value.view||!integer(value.view.frame,0)||value.view.frame>=value.totalFrames||
        !Array.isArray(value.view.layerUids)||new Set(value.view.layerUids).size!==value.view.layerUids.length||
        value.view.layerUids.some(function(uid){return !value.layerUids.includes(uid);})||
        [value.view.activeLayerUid,value.view.anchorLayerUid].some(function(uid){return uid!==null&&!value.layerUids.includes(uid);}))
      throw new Error('Native retained project intent is invalid');
    return Object.freeze({kind:value.kind,instanceId:value.instanceId,documentId:value.documentId,
      ownerDocumentId:value.ownerDocumentId,lifecycleGeneration:value.lifecycleGeneration,
      contentRevision:value.contentRevision,documentSnapshotId:value.documentSnapshotId,
      preparationToken:value.preparationToken,digest:value.digest,totalFrames:value.totalFrames,
      layerUids:Object.freeze(value.layerUids.slice()),view:Object.freeze({frame:value.view.frame,
        layerUids:Object.freeze(value.view.layerUids.slice()),activeLayerUid:value.view.activeLayerUid,
        anchorLayerUid:value.view.anchorLayerUid})});
  }
  function request(source,before,previous,rid){
    var retained=source.retainedProject;
    if(retained){
      target(retained);
      if(!previous||retained.instanceId!==before.instanceId||retained.ownerDocumentId!==previous.documentId||
          retained.lifecycleGeneration!==previous.lifecycleGeneration)
        throw new Error('Native retained project owner changed');
      return {apiVersion:2,requestId:rid,instanceId:before.instanceId,documentId:previous.documentId,
        expectedRevision:previous.contentRevision,documentSnapshotId:previous.documentSnapshotId,
        lifecycleGeneration:previous.lifecycleGeneration,action:{kind:'activate',preparationToken:retained.preparationToken,
          targetDocumentId:retained.documentId,targetRevision:retained.contentRevision,targetSnapshotId:retained.documentSnapshotId}};
    }
    var value=Object.assign({apiVersion:2,requestId:rid,instanceId:before.instanceId},source);
    if(previous)value.replace={documentId:previous.documentId,expectedRevision:previous.contentRevision,
      documentSnapshotId:previous.documentSnapshotId,lifecycleGeneration:previous.lifecycleGeneration};
    return value;
  }
  function receipt(value,before,previous,rid,retained){
    if(!value||value.apiVersion!==2||value.requestId!==rid||value.instanceId!==before.instanceId||
        !id(value.documentId)||value.viewportAvailable!==false||previous&&value.documentId===previous.documentId)
      throw new Error(retained?'Native retained project receipt is uncorrelated':'Native project Open could not be reconciled');
    if(!retained){
      if(value.contentRevision!==0||!integer(value.resourceCount,0)||value.resourceCount>1)
        throw new Error('Native project Open could not be reconciled');
      return 0;
    }
    var inactive=value.inactiveProject;
    if(!exact(value,['apiVersion','requestId','instanceId','documentId','contentRevision','documentSnapshotId',
          'lifecycleGeneration','inactiveProject','viewportAvailable','retryExecution'])||
        !exact(inactive,['documentId','contentRevision','documentSnapshotId','preparationToken','preparationRequestId','documentDigest'])||
        value.documentId!==retained.documentId||value.contentRevision!==retained.contentRevision||
        value.documentSnapshotId!==retained.documentSnapshotId||!integer(value.lifecycleGeneration,previous.lifecycleGeneration+1)||
        value.retryExecution!==false||!inactive||inactive.documentId!==previous.documentId||
        inactive.contentRevision!==previous.contentRevision||inactive.documentSnapshotId!==previous.documentSnapshotId||
        inactive.preparationRequestId!==rid||!id(inactive.preparationToken)||inactive.documentDigest!==previous.documentDigest||
        !/^[a-f0-9]{64}$/.test(inactive.documentDigest)||JSON.stringify(value).length>4096)
      throw new Error('Native retained project receipt is uncorrelated');
    Object.freeze(inactive);Object.freeze(value);
    return retained.contentRevision;
  }
  function installed(value,before,previous,receipt,revision,retained){
    if(!value||value.apiVersion!==2||value.available!==true||value.documentId!==receipt.documentId||
        value.instanceId!==before.instanceId||value.contentRevision!==revision||
        !integer(value.lifecycleGeneration,previous?previous.lifecycleGeneration+1:1)||
        retained&&value.lifecycleGeneration!==receipt.lifecycleGeneration)
      throw new Error('Native project installation is uncorrelated');
  }
  function candidate(info,source,receipt){
    if(source.newProject&&(info.canvasW!==source.newProject.canvasW||info.canvasH!==source.newProject.canvasH||
        info.fps!==source.newProject.fps||receipt.resourceCount!==0))
      throw new Error('Native New Project metadata is uncorrelated');
    if(source.retainedProject&&(info.documentSnapshotId!==source.retainedProject.documentSnapshotId||
        info.documentDigest!==source.retainedProject.digest))
      throw new Error('Native retained metadata is uncorrelated');
    var frame=source.retainedProject?source.retainedProject.view.frame:0;
    if(!integer(frame,0)||frame>=info.totalFrames)throw new Error('Native retained frame is unavailable');
    return frame;
  }
  return Object.freeze({target:target,request:request,receipt:receipt,installed:installed,candidate:candidate});
}));
