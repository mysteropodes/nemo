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
    var phase='idle',info=null,presentation=null,queue=Promise.resolve(),serial=0,pending=0,rendering=null,wantedFrame=null,openingRequested=false;
    function requestId(kind){return 'project-'+kind+'-'+(++serial);}
    function raw(command,request){return ports.invoke(command,{requestJson:JSON.stringify(request)});}
    function active(){return phase==='native';}
    function requireActive(){if(!active()||!info)throw new Error('Native project is unavailable; reopen Nemo');return info;}
    function enqueue(action){pending++;var next=queue.then(action);queue=next.catch(function(){});return next.finally(function(){pending--;});}
    function fence(error){phase='indeterminate';presentation=null;if(ports.failed)ports.failed(error);return error;}
    function epoch(value){return value&&{instanceId:value.instanceId,documentId:value.documentId,lifecycleGeneration:value.lifecycleGeneration};}
    function sameEpoch(a,b){return !!a&&!!b&&a.instanceId===b.instanceId&&a.documentId===b.documentId&&a.lifecycleGeneration===b.lifecycleGeneration;}
    function guardEpoch(value){if(!active()||!sameEpoch(value,epoch(info)))throw new Error('Native project owner changed before queued action');}
    function disposition(error){return error&&error.details&&error.details.disposition;}
    function localRefusal(message){return {code:'replacement_refused',message:message,details:{disposition:'notDispatched',retryExecution:false}};}
    async function status(){return ports.invoke('nemo_native_object_client_status');}
    async function inspectFor(pin){
      var request={apiVersion:2,requestId:requestId('inspect'),instanceId:pin.instanceId,documentId:pin.documentId,action:{kind:'inspect'}};
      var next=metadata(await raw('nemo_native_object_reference',request),request);
      if(next.lifecycleGeneration!==pin.lifecycleGeneration||next.contentRevision!==pin.contentRevision)throw new Error('Native project owner changed');
      return next;
    }
    async function verifyOwner(pin){
      var value=await status();
      if(!value||value.apiVersion!==2||value.available!==true||value.instanceId!==pin.instanceId||
          value.documentId!==pin.documentId||value.contentRevision!==pin.contentRevision||
          value.lifecycleGeneration!==pin.lifecycleGeneration)throw new Error('Native project owner changed');
      var next=await inspectFor(pin);
      if(!same(next,pin))throw new Error('Native project snapshot changed');
      return next;
    }
    async function inspect(requestIdValue){
      var current=info;if(!current)throw new Error('Native project identity unavailable');
      var request={apiVersion:2,requestId:requestIdValue||requestId('inspect'),instanceId:current.instanceId,documentId:current.documentId,action:{kind:'inspect'}};
      var next=metadata(await raw('nemo_native_object_reference',request),request);
      if(next.lifecycleGeneration!==current.lifecycleGeneration||next.contentRevision<current.contentRevision)throw new Error('Native project owner changed');
      if(!same(info,next))presentation=null;
      info=next;return next;
    }
    function openPath(path,transition){
      if(openingRequested||phase!=='idle'&&phase!=='native')return Promise.reject(new Error('Native project Open is unavailable'));
      openingRequested=true;wantedFrame=null;
      return enqueue(async function(){
        var previous=info,priorPresentation=presentation,priorPhase=phase,dispatched=false,inBootstrap=false;
        phase='opening';presentation=null;
        try{
          if(transition&&transition.confirm&&await transition.confirm(previous)!==true){phase=priorPhase;presentation=priorPresentation;return false;}
          if(transition&&transition.occlude)await transition.occlude();
          var before=await status();
          if(!before||before.apiVersion!==2||!id(before.instanceId)||
              (previous?before.available!==true||before.instanceId!==previous.instanceId||before.documentId!==previous.documentId||
                before.contentRevision!==previous.contentRevision||before.lifecycleGeneration!==previous.lifecycleGeneration:before.available!==false))
            throw localRefusal('Native project slot changed before Open');
          var rid=requestId('open'),request={apiVersion:2,requestId:rid,instanceId:before.instanceId,sourcePath:path,projectOnly:true};
          if(previous)request.replace={documentId:previous.documentId,expectedRevision:previous.contentRevision,
            documentSnapshotId:previous.documentSnapshotId,lifecycleGeneration:previous.lifecycleGeneration};
          dispatched=true;inBootstrap=true;
          var receipt=await raw('nemo_native_object_bootstrap',request);inBootstrap=false;
          if(!receipt||receipt.apiVersion!==2||receipt.requestId!==rid||receipt.instanceId!==before.instanceId||
              !id(receipt.documentId)||receipt.contentRevision!==0||!integer(receipt.resourceCount,0)||
              receipt.resourceCount>1||receipt.viewportAvailable!==false||previous&&receipt.documentId===previous.documentId)
            throw new Error('Native project Open could not be reconciled');
          var installed=await status();
          if(!installed||installed.apiVersion!==2||installed.available!==true||installed.documentId!==receipt.documentId||
              installed.instanceId!==before.instanceId||installed.contentRevision!==0||
              !integer(installed.lifecycleGeneration,previous?previous.lifecycleGeneration+1:1))
            throw new Error('Native project installation is uncorrelated');
          var candidate=await inspectFor({instanceId:installed.instanceId,documentId:installed.documentId,
            contentRevision:0,lifecycleGeneration:installed.lifecycleGeneration});
          info=candidate;
          await present(0,candidate);await verifyOwner(candidate);
          if(transition&&transition.reveal){await transition.reveal(candidate);
            await present(0,candidate);await verifyOwner(candidate);}
          phase='native';changed();
          if(transition&&transition.commit)await transition.commit(candidate);
          return candidate;
        }catch(error){
          if(!dispatched||inBootstrap&&(disposition(error)==='notDispatched'||!previous&&error&&error.code==='unsupported_format')){
            try{
              if(previous)await verifyOwner(previous);
              else{var empty=await status();if(!empty||empty.apiVersion!==2||empty.available!==false)throw new Error('Native project slot changed');}
              info=previous;phase=priorPhase;presentation=priorPresentation;
              if(transition&&transition.restore)await transition.restore(previous);
            }catch(unknown){throw fence(unknown);}
            if(!previous&&error&&error.code==='unsupported_format')return null;
            throw disposition(error)==='notDispatched'?error:localRefusal(error&&error.message||'Native project Open was not dispatched');
          }
          throw fence(error);
        }finally{openingRequested=false;}
      });
    }
    function frameRequest(frame,pin){
      pin=pin||requireActive();if(!integer(frame,0)||frame>=pin.totalFrames)throw new Error('Frame is outside the native project');
      var viewport=ports.mapping(pin);
      if(!viewport||!viewport.cssBounds||!(viewport.cssBounds.width>0)||!(viewport.cssBounds.height>0)||
          !viewport.physicalExtent||!integer(viewport.physicalExtent.width,1)||!integer(viewport.physicalExtent.height,1))
        throw new Error('Native project viewport is unavailable');
      return {apiVersion:2,requestId:requestId('frame'),instanceId:pin.instanceId,documentId:pin.documentId,
        contentRevision:pin.contentRevision,documentSnapshotId:pin.documentSnapshotId,lifecycleGeneration:pin.lifecycleGeneration,
        contextId:'scene-root',scopeKind:'authored',frame:frame,
        outputSpec:{kind:'frame',format:'rgba8',width:pin.canvasW,height:pin.canvasH,colorInterpretation:'srgb',alphaMode:'straight'},
        viewport:viewport};
    }
    async function present(frame,pin){
      var request=frameRequest(frame,pin),receipt=await raw('nemo_native_object_preview',request);
      if(!same(info,request)||!receipt||receipt.status!=='presented'||!id(receipt.workId)||!integer(receipt.viewGeneration,1)){
        presentation=null;throw new Error('Native project frame was not presented');
      }
      presentation=freeze({owner:'native',status:'presented',contextId:'scene-root',frame:frame,workId:receipt.workId,
        viewGeneration:receipt.viewGeneration,instanceId:request.instanceId,documentId:request.documentId,
        contentRevision:request.contentRevision,documentSnapshotId:request.documentSnapshotId,lifecycleGeneration:request.lifecycleGeneration});
      return presentation;
    }
    function presentPreview(frame){
      if(openingRequested||!active())return Promise.reject(new Error('Native project Open is in progress'));
      var submitted=epoch(info);return enqueue(function(){guardEpoch(submitted);return present(frame);});
    }
    function renderPreview(frame){
      if(!active()||openingRequested)return Promise.resolve(null);
      wantedFrame=frame;
      if(rendering)return rendering;
      rendering=(async function(){var receipt;
        while(wantedFrame!==null){var latest=wantedFrame;wantedFrame=null;receipt=await presentPreview(latest);}
        return receipt;
      }()).catch(function(error){wantedFrame=null;if(ports.notice)ports.notice(error.message);return null;}).finally(function(){rendering=null;});
      return rendering;
    }
    function changed(){if(ports.changed)ports.changed(info);}
    function mutation(makeRequest,command){
      if(openingRequested||!active())return Promise.reject(new Error('Native project Open is in progress'));
      var submitted=epoch(info);return enqueue(async function(){
      guardEpoch(submitted);var pin=requireActive(),request=makeRequest(pin);presentation=null;
      var receipt;
      try{receipt=await raw(command,request);}catch(error){
        // A transport error cannot establish whether a native command committed.
        try{await inspect();changed();}catch(unknown){throw fence(unknown);}
        try{await present(ports.frame?ports.frame():0);}catch(unpresented){if(ports.notice)ports.notice(unpresented.message);}
        throw error;
      }
      if(!receipt||receipt.apiVersion!==2||receipt.requestId!==request.requestId||receipt.instanceId!==pin.instanceId||
        receipt.documentId!==pin.documentId||!integer(receipt.contentRevision,pin.contentRevision)||typeof receipt.ok!=='boolean')throw fence(new Error('Native mutation response is uncorrelated'));
      try{await inspect();changed();}catch(unknown){throw fence(unknown);}
      if(!receipt.ok){
        try{await present(ports.frame?ports.frame():0);}catch(unpresented){if(ports.notice)ports.notice(unpresented.message);}
        throw new Error('Native command refused: '+(receipt.error&&receipt.error.code||'unavailable'));
      }
      await present(ports.frame?ports.frame():0);return receipt;
    });}
    function reference(action){return mutation(function(pin){return {apiVersion:2,requestId:requestId('reference'),instanceId:pin.instanceId,
      documentId:pin.documentId,lifecycleGeneration:pin.lifecycleGeneration,expectedRevision:pin.contentRevision,
      documentSnapshotId:pin.documentSnapshotId,action:typeof action==='function'?action(pin.reference):action};},'nemo_native_object_reference');}
    function history(action){if(action!=='undo'&&action!=='redo')return Promise.reject(new Error('Unsupported history action'));
      return mutation(function(pin){return {apiVersion:2,requestId:requestId(action),instanceId:pin.instanceId,documentId:pin.documentId,
        operation:'command.document.object.'+action,expectedRevision:pin.contentRevision,payload:{command:'object.'+action}};},'nemo_native_object_dispatch');}
    async function acquirePin(request){
      requireActive();var value=await inspect(request.requestId);
      if(request.operation!=='query.document.project.info'||request.instanceId!==value.instanceId||request.documentId!==value.documentId||request.payload.atRevision!==value.contentRevision)throw new Error('Native save pin changed');
      return {apiVersion:2,requestId:request.requestId,instanceId:value.instanceId,documentId:value.documentId,contentRevision:value.contentRevision,ok:true,
        result:{atRevision:value.contentRevision,documentSnapshotId:value.documentSnapshotId,documentDigest:value.documentDigest,byteCount:value.byteCount}};
    }
    function presentedSession(){return active()&&presentation&&same(info,presentation)&&pending===0?
      Object.assign({available:true,kind:'native-object',presentation:presentation},info):null;}
    function exportPng(destination,frames,onProgress,signal){
      if(openingRequested||!active())return Promise.reject(new Error('Native project Open is in progress'));
      var submitted=epoch(info);return enqueue(async function(){
      guardEpoch(submitted);
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
