/* Native project session: detached UI metadata and request ordering only. */
(function(root,factory){
  var api=factory();
  if(typeof module==='object'&&module.exports)module.exports=api;
  else root.NemoNativeObjectProject=api;
}(typeof globalThis==='object'?globalThis:this,function(){
  'use strict';
  function id(v){return typeof v==='string'&&/^[A-Za-z0-9][A-Za-z0-9._:/-]{0,127}$/.test(v);}
  function integer(v,min){return Number.isSafeInteger(v)&&v>=min;}
  function freeze(v){if(v&&typeof v==='object'){Object.keys(v).forEach(function(k){freeze(v[k]);});Object.freeze(v);}return v;}
  function same(a,b){return !!a&&!!b&&['instanceId','documentId','contentRevision','documentSnapshotId','lifecycleGeneration'].every(function(k){return a[k]===b[k];});}
  function metadata(v,request){
    if(!v||v.apiVersion!==2||v.requestId!==request.requestId||v.instanceId!==request.instanceId||v.documentId!==request.documentId||
      !id(v.documentSnapshotId)||!integer(v.contentRevision,0)||!integer(v.lifecycleGeneration,1)||
      !/^[a-f0-9]{64}$/.test(v.documentDigest)||!integer(v.byteCount,1)||v.byteCount>32*1024*1024||
      !integer(v.canvasW,1)||!integer(v.canvasH,1)||!integer(v.totalFrames,1)||!(v.fps>0)||
      !Array.isArray(v.layers)||!v.layers.every(function(l){return l&&typeof l.layerUid==='string';}))throw new Error('Native project metadata is uncorrelated');
    // Copy only admitted UI fields; embedded files/objects never become JS state.
    var result={};['apiVersion','instanceId','documentId','contentRevision','documentSnapshotId','lifecycleGeneration',
      'documentDigest','byteCount','canvasW','canvasH','fps','totalFrames','undoDepth','redoDepth'].forEach(function(k){result[k]=v[k];});
    result.layers=v.layers.map(function(l){return {layerUid:l.layerUid};});
    var r=v.reference;
    if(r!==null&&(!r||!id(r.id)||typeof r.name!=='string'||typeof r.visible!=='boolean'||!Number.isFinite(r.opacity)||
      r.opacity<0||r.opacity>1||!Number.isSafeInteger(r.offsetFrames)))throw new Error('Native reference metadata is invalid');
    result.reference=r===null?null:{id:r.id,name:r.name,visible:r.visible,opacity:r.opacity,offsetFrames:r.offsetFrames};
    return freeze(result);
  }
  function create(ports){
    var phase='idle',info=null,presentation=null,queue=Promise.resolve(),serial=0,pending=0,rendering=null,wantedFrame=null;
    function requestId(kind){return 'project-'+kind+'-'+(++serial);}
    function raw(command,request){return ports.invoke(command,{requestJson:JSON.stringify(request)});}
    function active(){return phase==='native';}
    function requireActive(){if(!active()||!info)throw new Error('Native project is unavailable; reopen Nemo');return info;}
    function enqueue(action){pending++;var next=queue.then(action);queue=next.catch(function(){});return next.finally(function(){pending--;});}
    function fence(error){phase='indeterminate';presentation=null;if(ports.failed)ports.failed(error);return error;}
    async function inspect(requestIdValue){
      var current=info;if(!current)throw new Error('Native project identity unavailable');
      var request={apiVersion:2,requestId:requestIdValue||requestId('inspect'),instanceId:current.instanceId,documentId:current.documentId,action:{kind:'inspect'}};
      var next=metadata(await raw('nemo_native_object_reference',request),request);
      if(next.lifecycleGeneration!==current.lifecycleGeneration||next.contentRevision<current.contentRevision)throw new Error('Native project owner changed');
      if(!same(info,next))presentation=null;
      info=next;return next;
    }
    async function openPath(path){
      if(phase!=='idle')throw new Error('Native project replacement is unavailable');
      phase='opening';
      try{
        var status=await ports.invoke('nemo_native_object_client_status');
        if(!status||status.apiVersion!==2||!id(status.instanceId)||status.available!==false)throw new Error('Native project slot is occupied or unavailable');
        var rid=requestId('open'),receipt=await raw('nemo_native_object_bootstrap',{apiVersion:2,requestId:rid,instanceId:status.instanceId,sourcePath:path,projectOnly:true});
        if(!receipt||receipt.requestId!==rid||receipt.instanceId!==status.instanceId||!id(receipt.documentId)||receipt.contentRevision!==0)throw new Error('Native project Open could not be reconciled');
        var installed=await ports.invoke('nemo_native_object_client_status');
        if(!installed||installed.available!==true||installed.documentId!==receipt.documentId||installed.instanceId!==status.instanceId||!integer(installed.lifecycleGeneration,1))throw new Error('Native project installation is uncorrelated');
        info={instanceId:installed.instanceId,documentId:installed.documentId,contentRevision:0,lifecycleGeneration:installed.lifecycleGeneration};
        await inspect();phase='native';return info;
      }catch(error){
        if(!info&&error&&error.code==='unsupported_format'){phase='idle';return null;}
        throw fence(error);
      }
    }
    function frameRequest(frame){
      var pin=requireActive();if(!integer(frame,0)||frame>=pin.totalFrames)throw new Error('Frame is outside the native project');
      return {apiVersion:2,requestId:requestId('frame'),instanceId:pin.instanceId,documentId:pin.documentId,
        contentRevision:pin.contentRevision,documentSnapshotId:pin.documentSnapshotId,lifecycleGeneration:pin.lifecycleGeneration,
        contextId:'scene-root',scopeKind:'authored',frame:frame,
        outputSpec:{kind:'frame',format:'rgba8',width:pin.canvasW,height:pin.canvasH,colorInterpretation:'srgb',alphaMode:'straight'},
        viewport:ports.mapping(pin)};
    }
    async function present(frame){
      var request=frameRequest(frame),receipt=await raw('nemo_native_object_preview',request);
      if(!same(info,request)||!receipt||receipt.status!=='presented'||!id(receipt.workId)||!integer(receipt.viewGeneration,1)){
        presentation=null;throw new Error('Native project frame was not presented');
      }
      presentation=freeze({owner:'native',status:'presented',contextId:'scene-root',frame:frame,workId:receipt.workId,
        viewGeneration:receipt.viewGeneration,instanceId:request.instanceId,documentId:request.documentId,
        contentRevision:request.contentRevision,documentSnapshotId:request.documentSnapshotId,lifecycleGeneration:request.lifecycleGeneration});
      return presentation;
    }
    function presentPreview(frame){return enqueue(function(){return present(frame);});}
    function renderPreview(frame){
      if(!active())return Promise.resolve(null);
      wantedFrame=frame;
      if(rendering)return rendering;
      rendering=(async function(){var receipt;
        while(wantedFrame!==null){var latest=wantedFrame;wantedFrame=null;receipt=await presentPreview(latest);}
        return receipt;
      }()).catch(function(error){wantedFrame=null;if(ports.notice)ports.notice(error.message);return null;}).finally(function(){rendering=null;});
      return rendering;
    }
    function changed(){if(ports.changed)ports.changed(info);}
    function mutation(makeRequest,command){return enqueue(async function(){
      var pin=requireActive(),request=makeRequest(pin);presentation=null;
      var receipt;
      try{receipt=await raw(command,request);}catch(error){
        // A transport error cannot establish whether a native command committed.
        try{await inspect();changed();}catch(unknown){throw fence(unknown);}
        throw error;
      }
      if(!receipt||receipt.apiVersion!==2||receipt.requestId!==request.requestId||receipt.instanceId!==pin.instanceId||
        receipt.documentId!==pin.documentId||!integer(receipt.contentRevision,pin.contentRevision)||typeof receipt.ok!=='boolean')throw fence(new Error('Native mutation response is uncorrelated'));
      await inspect();changed();
      if(!receipt.ok)throw new Error('Native command refused: '+(receipt.error&&receipt.error.code||'unavailable'));
      await present(ports.frame?ports.frame():0);return receipt;
    });}
    function reference(action){return mutation(function(pin){return {apiVersion:2,requestId:requestId('reference'),instanceId:pin.instanceId,
      documentId:pin.documentId,lifecycleGeneration:pin.lifecycleGeneration,expectedRevision:pin.contentRevision,
      documentSnapshotId:pin.documentSnapshotId,action:typeof action==='function'?action(pin.reference):action};},'nemo_native_object_reference');}
    function history(action){if(action!=='undo'&&action!=='redo')return Promise.reject(new Error('Unsupported history action'));
      return mutation(function(pin){return {apiVersion:2,requestId:requestId(action),instanceId:pin.instanceId,documentId:pin.documentId,
        operation:'command.document.object.'+action,expectedRevision:pin.contentRevision,payload:{}};},'nemo_native_object_dispatch');}
    async function acquirePin(request){
      requireActive();var value=await inspect(request.requestId);
      if(request.operation!=='query.document.project.info'||request.instanceId!==value.instanceId||request.documentId!==value.documentId||request.payload.atRevision!==value.contentRevision)throw new Error('Native save pin changed');
      return {apiVersion:2,requestId:request.requestId,instanceId:value.instanceId,documentId:value.documentId,contentRevision:value.contentRevision,ok:true,
        result:{atRevision:value.contentRevision,documentSnapshotId:value.documentSnapshotId,documentDigest:value.documentDigest,byteCount:value.byteCount}};
    }
    function presentedSession(){return active()&&presentation&&same(info,presentation)&&pending===0?
      Object.assign({available:true,kind:'native-object',presentation:presentation},info):null;}
    function exportPng(destination,frames,onProgress,signal){return enqueue(async function(){
      if(!Array.isArray(frames)||frames.length!==1)throw new Error('Native object sequence export is unavailable');
      if(signal&&signal.aborted)return {status:'cancelled'};
      var request=frameRequest(frames[0]);request.destinationDirectory=destination;request.maximumEncodedBytes=4*1024*1024;
      var receipt=await raw('nemo_native_object_export',request);
      if(!receipt||receipt.requestId!==request.requestId||receipt.instanceId!==request.instanceId||receipt.documentId!==request.documentId||
        receipt.contentRevision!==request.contentRevision||receipt.documentSnapshotId!==request.documentSnapshotId||receipt.published!==true||receipt.cleanupComplete!==true)throw new Error('Native export publication is unconfirmed');
      if(onProgress)onProgress(1,1);return receipt;
    });}
    return Object.freeze({openPath:openPath,isActive:active,blocksLegacy:function(){return phase!=='idle';},
      info:function(){return info;},requestId:requestId,reference:reference,historyFromUi:history,
      presentPreview:presentPreview,renderPreview:renderPreview,exportPng:exportPng,readPresentedObjectSession:presentedSession,
      acquirePin:acquirePin,savePinned:function(request){return raw('nemo_native_object_save_as',request);},
      getNativeIdentity:function(){if(phase==='idle')return null;var pin=requireActive();return {documentId:pin.documentId,generation:pin.lifecycleGeneration};},
      identity:requireActive,requestRelease:function(){return false;},releaseCurrent:function(){return false;},
      persistenceJSON:function(){throw new Error('Native project files must be saved through the native file command');}});
  }
  var desktops=new WeakMap();
  function desktop(root){
    if(!root.__TAURI__||!root.__TAURI__.core)return null;
    if(desktops.has(root))return desktops.get(root);
    var session=create({invoke:root.__TAURI__.core.invoke.bind(root.__TAURI__.core),
      frame:function(){return root.state.currentFrame;},
      mapping:function(info){var b=root.document.getElementById('drawing-canvas').getBoundingClientRect(),d=root.devicePixelRatio||1;
        return {cssBounds:{x:b.left,y:b.top,width:b.width,height:b.height},physicalExtent:{width:Math.max(1,Math.round(b.width*d)),height:Math.max(1,Math.round(b.height*d))},compositionExtent:{width:info.canvasW,height:info.canvasH},reportedDpr:d};},
      changed:function(){if(root.SMReference)root.SMReference.refreshNative();if(root.SMProject)root.SMProject.refreshActiveTabDirtyDot();},
      notice:function(message){if(root.showToast)root.showToast(message);},
      failed:function(){if(root.showToast)root.showToast('Native project state is unconfirmed. Reopen Nemo before continuing.');}});
    desktops.set(root,session);return session;
  }
  return Object.freeze({create:create,desktop:desktop});
}));
