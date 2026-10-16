/* Native project session: detached UI metadata and request ordering only.
 * Composition supplies stable, narrow UI callbacks; the presenter never resolves
 * this provider or the file adapter. Callback reads retain current native authority. */
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
      !integer(v.canvasW,1)||!integer(v.canvasH,1)||!integer(v.totalFrames,1)||!Number.isFinite(v.fps)||!(v.fps>0)||
      (Object.prototype.hasOwnProperty.call(v,'canvasBg')&&(typeof v.canvasBg!=='string'||!/^#?(?:[a-fA-F0-9]{3}|[a-fA-F0-9]{6}|[a-fA-F0-9]{8})$/.test(v.canvasBg)))||
      ['waIn','waOut'].some(function(k){return Object.prototype.hasOwnProperty.call(v,k)&&!Number.isFinite(v[k]);})||
      !Array.isArray(v.layers)||!v.layers.every(function(l){return l&&typeof l.layerUid==='string'&&
        (!Object.prototype.hasOwnProperty.call(l,'name')||typeof l.name==='string')&&
        (!Object.prototype.hasOwnProperty.call(l,'visible')||typeof l.visible==='boolean')&&
        (!Object.prototype.hasOwnProperty.call(l,'locked')||l.locked===false)&&
        (!Object.prototype.hasOwnProperty.call(l,'color')||typeof l.color==='string'&&/^#[a-fA-F0-9]{6}$/.test(l.color))&&
        (!Object.prototype.hasOwnProperty.call(l,'parentLayerUid')||l.parentLayerUid===null);}))throw new Error('Native project metadata is uncorrelated');
    // Copy only admitted UI fields; embedded files/objects never become JS state.
    var result={};['apiVersion','instanceId','documentId','contentRevision','documentSnapshotId','lifecycleGeneration',
      'documentDigest','byteCount','canvasW','canvasH','fps','totalFrames','undoDepth','redoDepth'].forEach(function(k){result[k]=v[k];});
    ['canvasBg','waIn','waOut'].forEach(function(k){if(Object.prototype.hasOwnProperty.call(v,k))result[k]=v[k];});
    result.layers=v.layers.map(function(l){var row={layerUid:l.layerUid};
      if(Object.prototype.hasOwnProperty.call(l,'name'))row.name=l.name;
      if(Object.prototype.hasOwnProperty.call(l,'visible'))row.visible=l.visible;
      ['locked','color','parentLayerUid'].forEach(function(k){if(Object.prototype.hasOwnProperty.call(l,k))row[k]=l[k];});return row;});
    var r=v.reference;
    if(r!==null&&(!r||!id(r.id)||typeof r.name!=='string'||typeof r.visible!=='boolean'||!Number.isFinite(r.opacity)||
      r.opacity<0||r.opacity>1||!Number.isSafeInteger(r.offsetFrames)))throw new Error('Native reference metadata is invalid');
    result.reference=r===null?null:{id:r.id,name:r.name,visible:r.visible,opacity:r.opacity,offsetFrames:r.offsetFrames};
    return freeze(result);
  }
  function create(ports){
    var phase='idle',info=null,presentation=null,queue=Promise.resolve(),serial=0,pending=0,rendering=null,wantedFrame=null,openingRequested=false;
    var subscription=null,subscriptionTicket=0,synchronizing=false,openAttempt=null,presentationBounds=null;
    function requestId(kind){return 'project-'+kind+'-'+(++serial);}
    function openDiagnostic(event,code){if(typeof ports.openDiagnostic==='function')try{ports.openDiagnostic(event,code);}catch(_){} }
    function openStage(stage){if(openAttempt){openAttempt.stage=stage;openAttempt.invokeErrors=new Set();}}
    function invokeTracked(command,args){
      var attempt=openAttempt,stage=attempt&&attempt.stage;
      function rejected(error){if(attempt&&openAttempt===attempt&&attempt.stage===stage)attempt.invokeErrors.add(error);throw error;}
      var result;try{result=arguments.length>1?ports.invoke(command,args):ports.invoke(command);}catch(error){return rejected(error);}
      return attempt&&result&&typeof result.then==='function'?result.then(function(value){return value;},rejected):result;
    }
    function openFailure(error){
      if(!openAttempt)return;
      var code='unknown',disposition='other';
      try{var value=error&&error.code;if(['invalid_request','unavailable','unsupported_format','replacement_refused','stale_revision','stale_owner','cancelled','wrong_instance','wrong_document','cancelled_before_dispatch'].includes(value))code=value;
        if(error&&error.details&&error.details.disposition==='notDispatched')disposition='notDispatched';}catch(_){}
      openDiagnostic('post-bootstrap-failed','post-bootstrap-'+openAttempt.stage+':'+
        (openAttempt.invokeErrors.has(error)?'invoke-rejected':'local-refusal')+':'+code+':'+disposition);
      openAttempt=null;
    }
    function raw(command,request){
      if(command==='nemo_native_dispatch'){
        if(typeof ports.dispatchApplication!=='function')throw new Error('Native public application client is unavailable');
        return ports.dispatchApplication(request);
      }
      if(subscription&&typeof ports.dispatchMutation==='function'&&
        (command==='nemo_native_object_dispatch'||command==='nemo_native_object_reference'&&request.action.kind!=='inspect'))
        return ports.dispatchMutation(command,request);
      if(command!=='nemo_native_object_bootstrap'||typeof ports.openDiagnostic!=='function')return invokeTracked(command,{requestJson:JSON.stringify(request)});
      openDiagnostic('bootstrap-dispatched');
      var result;try{result=ports.invoke(command,{requestJson:JSON.stringify(request)});}catch(error){openDiagnostic('bootstrap-rejected',error&&error.code==='unsupported_format'?'unsupported-format':'other');throw error;}
      function rejected(error){openDiagnostic('bootstrap-rejected',error&&error.code==='unsupported_format'?'unsupported-format':'other');throw error;}
      if(result&&typeof result.then==='function')return result.then(function(value){openDiagnostic('bootstrap-resolved');return value;},rejected);
      openDiagnostic('bootstrap-resolved');return result;
    }
    function active(){return phase==='native';}
    function requireActive(){if(!active()||!info)throw new Error('Native project is unavailable; reopen Nemo');return info;}
    function enqueue(action){pending++;var next=queue.then(action);queue=next.catch(function(){});return next.finally(function(){pending--;if(pending===0&&ports.presented)ports.presented(info);});}
    function fence(error){phase='indeterminate';presentation=null;if(ports.surfaceChanged)ports.surfaceChanged(null);retireSubscription().catch(function(){});if(ports.failed)ports.failed(error);return error;}
    function epoch(value){return value&&{instanceId:value.instanceId,documentId:value.documentId,lifecycleGeneration:value.lifecycleGeneration};}
    function sameEpoch(a,b){return !!a&&!!b&&a.instanceId===b.instanceId&&a.documentId===b.documentId&&a.lifecycleGeneration===b.lifecycleGeneration;}
    function guardEpoch(value){if(!active()||!sameEpoch(value,epoch(info)))throw new Error('Native project owner changed before queued action');}
    async function retireSubscription(){
      subscriptionTicket++;subscription=null;synchronizing=false;
      if(typeof ports.unsubscribeRevisions==='function')await ports.unsubscribeRevisions();
    }
    async function attachSubscription(){
      if(typeof ports.subscribeRevisions!=='function'||typeof ports.publishRevision!=='function'||
        typeof ports.projectionReady!=='function'||!ports.projectionReady(info))return;
      var pin=epoch(info),ticket=++subscriptionTicket;
      var value=await ports.subscribeRevisions(function(event,binding){return synchronizeRevision(event,pin,ticket,binding);},function(error){
        if(ticket===subscriptionTicket&&sameEpoch(pin,epoch(info)))fence(error);
      },invokeTracked);
      if(value===null)return;
      if(ticket!==subscriptionTicket||!active()||!sameEpoch(pin,epoch(info))||
        !sameEpoch(value,pin)||value.contentRevision!==info.contentRevision||!id(value.subscriptionId))
        throw new Error('Native object revision subscription is uncorrelated');
      subscription=freeze(Object.assign({},value));
    }
    function synchronizeRevision(event,pin,ticket,binding){
      if(ticket!==subscriptionTicket||!sameEpoch(pin,epoch(info)))
        return Promise.reject(new Error('Native object revision subscription was replaced'));
      if(!active()||synchronizing||!sameEpoch(binding,pin)||!id(binding.subscriptionId)||
        !sameEpoch(event,pin)||!id(event.requestId)||!integer(event.fromRevision,0)||
        !integer(event.toRevision,event.fromRevision+1)||
        info.contentRevision>event.toRevision)return Promise.reject(fence(new Error('Native object revision event is stale')));
      synchronizing=true;presentation=null;wantedFrame=null;
      return enqueue(async function(){
        if(ticket!==subscriptionTicket||!sameEpoch(pin,epoch(info)))throw new Error('Native object revision subscription was replaced');
        guardEpoch(pin);
        var next=await inspect();
        if(ticket!==subscriptionTicket||next.contentRevision!==event.toRevision)
          throw new Error('Native object revision inspection is uncorrelated');
        if(await ports.publishRevision(next)!==true)throw new Error('Native object UI publication is unconfirmed');
        await present(ports.frame?ports.frame():0,next,{subscriptionId:binding.subscriptionId,event:event});
        if(ticket!==subscriptionTicket||!same(info,next)||!ports.projectionReady(next))
          throw new Error('Native object revision changed before acknowledgment');
        return freeze(Object.assign({},event));
      }).catch(function(error){
        if(ticket!==subscriptionTicket||!sameEpoch(pin,epoch(info)))throw error;
        throw fence(error);
      }).finally(function(){if(ticket===subscriptionTicket)synchronizing=false;});
    }
    function disposition(error){return error&&error.details&&error.details.disposition;}
    function localRefusal(message){return {code:'replacement_refused',message:message,details:{disposition:'notDispatched',retryExecution:false}};}
    async function status(){return invokeTracked('nemo_native_object_client_status');}
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
    function openPath(path,transition){return enterProject({sourcePath:path,projectOnly:true},transition);}
    function newProject(intent,transition){
      if(!intent||typeof intent!=='object'||Array.isArray(intent)||Object.keys(intent).length!==3||!integer(intent.canvasW,1)||!integer(intent.canvasH,1)||
        !Number.isFinite(intent.fps)||intent.fps<=0)
        return Promise.reject(localRefusal('Native New Project scalar intent is invalid'));
      return enterProject({newProject:{canvasW:intent.canvasW,canvasH:intent.canvasH,fps:intent.fps}},transition);
    }
    function enterProject(source,transition){
      if(openingRequested||phase!=='idle'&&phase!=='native')return Promise.reject(new Error('Native project Open is unavailable'));
      openingRequested=true;wantedFrame=null;
      return enqueue(async function(){
        var previous=info,priorPresentation=presentation,priorBounds=presentationBounds,priorPhase=phase,dispatched=false,inBootstrap=false;
        function current(){if(transition&&transition.isCurrent&&transition.isCurrent()!==true){
          if(dispatched)throw new Error('Native project request changed after dispatch');
          throw localRefusal('Native project request was cancelled');}}
        phase='opening';presentation=null;if(ports.surfaceChanged)ports.surfaceChanged(null);
        try{
          current();
          await retireSubscription();
          if(transition&&transition.confirm&&await transition.confirm(previous)!==true){
            phase=priorPhase;presentation=priorPresentation;if(previous)await attachSubscription();
            if(priorPresentation&&ports.surfaceChanged)ports.surfaceChanged(priorPresentation,priorBounds);return false;}
          current();
          if(transition&&transition.occlude)await transition.occlude();
          var before=await status();
          if(!before||before.apiVersion!==2||!id(before.instanceId)||
              (previous?before.available!==true||before.instanceId!==previous.instanceId||before.documentId!==previous.documentId||
                before.contentRevision!==previous.contentRevision||before.lifecycleGeneration!==previous.lifecycleGeneration:before.available!==false))
            throw localRefusal('Native project slot changed before Open');
          var rid=requestId(source.newProject?'new':'open'),request=Object.assign({apiVersion:2,requestId:rid,instanceId:before.instanceId},source);
          if(previous)request.replace={documentId:previous.documentId,expectedRevision:previous.contentRevision,
            documentSnapshotId:previous.documentSnapshotId,lifecycleGeneration:previous.lifecycleGeneration};
          current();dispatched=true;inBootstrap=true;
          var receipt=await raw('nemo_native_object_bootstrap',request);inBootstrap=false;
          if(typeof ports.openDiagnostic==='function')openAttempt={stage:'receipt',invokeErrors:new Set()};
          if(!receipt||receipt.apiVersion!==2||receipt.requestId!==rid||receipt.instanceId!==before.instanceId||
              !id(receipt.documentId)||receipt.contentRevision!==0||!integer(receipt.resourceCount,0)||
              receipt.resourceCount>1||receipt.viewportAvailable!==false||previous&&receipt.documentId===previous.documentId)
            throw new Error('Native project Open could not be reconciled');
          openStage('installed-status');var installed=await status();
          if(!installed||installed.apiVersion!==2||installed.available!==true||installed.documentId!==receipt.documentId||
              installed.instanceId!==before.instanceId||installed.contentRevision!==0||
              !integer(installed.lifecycleGeneration,previous?previous.lifecycleGeneration+1:1))
            throw new Error('Native project installation is uncorrelated');
          openStage('metadata');var candidate=await inspectFor({instanceId:installed.instanceId,documentId:installed.documentId,
            contentRevision:0,lifecycleGeneration:installed.lifecycleGeneration});
          if(source.newProject&&(candidate.canvasW!==source.newProject.canvasW||candidate.canvasH!==source.newProject.canvasH||
            candidate.fps!==source.newProject.fps||receipt.resourceCount!==0))throw new Error('Native New Project metadata is uncorrelated');
          info=candidate;current();
          openStage('first-preview');await present(0,candidate);openStage('first-owner');await verifyOwner(candidate);
          current();
          if(transition&&transition.reveal){openStage('reveal');await transition.reveal(candidate);
            openStage('second-preview');await present(0,candidate);openStage('second-owner');await verifyOwner(candidate);}
          current();phase='native';
          openStage('ui-commit');if(transition&&transition.commit)await transition.commit(candidate);
          current();
          openStage('publication');changed();openStage('subscriber');await attachSubscription();
          current();
          return candidate;
        }catch(error){
          openFailure(error);
          if(!dispatched||inBootstrap&&(disposition(error)==='notDispatched'||!source.newProject&&!previous&&error&&error.code==='unsupported_format')){
            try{
              if(previous)await verifyOwner(previous);
              else{var empty=await status();if(!empty||empty.apiVersion!==2||empty.available!==false)throw new Error('Native project slot changed');}
              info=previous;phase=priorPhase;presentation=priorPresentation;
              if(transition&&transition.restore)await transition.restore(previous);
              if(priorPresentation&&ports.surfaceChanged)ports.surfaceChanged(priorPresentation,priorBounds);
              if(previous)await attachSubscription();
            }catch(unknown){throw fence(unknown);}
            if(!source.newProject&&!previous&&error&&error.code==='unsupported_format')return null;
            throw disposition(error)==='notDispatched'?error:localRefusal(error&&error.message||'Native project Open was not dispatched');
          }
          throw fence(error);
        }finally{openAttempt=null;openingRequested=false;}
      });
    }
    function previewFailure(detail,error){
      if(openAttempt&&['first-preview','second-preview'].includes(openAttempt.stage))openAttempt.stage+='-'+detail;
      return error;
    }
    function frameRequest(frame,pin){
      pin=pin||requireActive();if(!integer(frame,0)||frame>=pin.totalFrames)throw previewFailure('frame-range',new Error('Frame is outside the native project'));
      var viewport;
      try{viewport=ports.mapping(pin);}catch(error){throw previewFailure('mapping',error);}
      try{
        if(!viewport||!viewport.cssBounds||!(viewport.cssBounds.width>0)||!(viewport.cssBounds.height>0)||
            !viewport.physicalExtent||!integer(viewport.physicalExtent.width,1)||!integer(viewport.physicalExtent.height,1))
          throw new Error('Native project viewport is unavailable');
      }catch(error){throw previewFailure('viewport',error);}
      return {apiVersion:2,requestId:requestId('frame'),instanceId:pin.instanceId,documentId:pin.documentId,
        contentRevision:pin.contentRevision,documentSnapshotId:pin.documentSnapshotId,lifecycleGeneration:pin.lifecycleGeneration,
        contextId:'scene-root',scopeKind:'authored',frame:frame,
        outputSpec:{kind:'frame',format:'rgba8',width:pin.canvasW,height:pin.canvasH,colorInterpretation:'srgb',alphaMode:'straight'},
        viewport:viewport};
    }
    function previewReceiptFailure(request,receipt){
      if(!['opening','native'].includes(phase)||!same(info,request))return 'owner';
      if(!receipt)return 'receipt';
      var status=receipt.status;
      if(status!=='presented')return ['stale-discarded','deferred-timeout','deferred-occluded','failed-validation',
        'failed-device-lost','failed-retry-lost','failed-retry-outdated','failed-recreate','failed-reconfigure'].includes(status)?'status-'+status:'status';
      if(!id(receipt.workId))return 'work-id';
      if(!integer(receipt.viewGeneration,1))return 'view-generation';
      return null;
    }
    async function present(frame,pin,revisionSync){
      var request;
      try{request=frameRequest(frame,pin);}catch(error){if(ports.surfaceChanged)ports.surfaceChanged(null);throw error;}
      if(revisionSync)request.revisionSync=revisionSync;
      var bounds=Object.assign({},request.viewport.cssBounds);
      var receipt;
      try{receipt=await raw('nemo_native_object_preview',request);}
      catch(error){if(ports.surfaceChanged)ports.surfaceChanged(null);throw error;}
      var failure;
      try{failure=previewReceiptFailure(request,receipt);}catch(error){if(ports.surfaceChanged)ports.surfaceChanged(null);throw previewFailure('receipt-access',error);}
      if(failure){
        presentation=null;if(ports.surfaceChanged)ports.surfaceChanged(null);throw previewFailure(failure,new Error('Native project frame was not presented'));
      }
      presentation=freeze({owner:'native',status:'presented',contextId:'scene-root',frame:frame,workId:receipt.workId,
        viewGeneration:receipt.viewGeneration,instanceId:request.instanceId,documentId:request.documentId,
        contentRevision:request.contentRevision,documentSnapshotId:request.documentSnapshotId,lifecycleGeneration:request.lifecycleGeneration});
      presentationBounds=bounds;if(ports.surfaceChanged)ports.surfaceChanged(presentation,bounds);
      return presentation;
    }
    function presentPreview(frame){
      if(openingRequested||synchronizing||!active())return Promise.reject(new Error('Native project Open or synchronization is in progress'));
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
      if(synchronizing)return Promise.reject(new Error('Native object revision synchronization is in progress'));
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
    function visibilityIntent(layerUid,value){
      return mutation(function(pin){
        var matches=pin.layers.filter(function(layer){return layer.layerUid===layerUid;});
        var visible=typeof value==='function'&&matches.length===1?value(matches[0]):value;
        if(typeof layerUid!=='string'||!layerUid||typeof visible!=='boolean'||
          matches.length!==1)
          throw new Error('Native layer visibility target or value is invalid');
        return {apiVersion:2,requestId:requestId('visibility'),instanceId:pin.instanceId,documentId:pin.documentId,
          operation:'command.document.object.layer.visibility.set',expectedRevision:pin.contentRevision,
          payload:{command:'object.layer.visibility.set',layerUid:layerUid,visible:visible}};
      },'nemo_native_dispatch');
    }
    function createLayer(name,expected){return mutation(function(pin){
      if(!same(pin,expected)||typeof name!=='string'||!name)throw new Error('Native Drawing create intent is stale or invalid');
      return {apiVersion:2,requestId:requestId('layer-create'),instanceId:pin.instanceId,documentId:pin.documentId,
        operation:'command.document.object.layer.create',expectedRevision:pin.contentRevision,payload:{command:'object.layer.create',name:name}};
    },'nemo_native_dispatch');}
    function editProjectSettings(action,expected){return mutation(function(pin){
      if(!same(pin,expected))throw new Error('Native project settings intent is stale');
      return {apiVersion:2,requestId:requestId('project-settings'),instanceId:pin.instanceId,documentId:pin.documentId,
        operation:'command.document.object.project.settings.edit',expectedRevision:pin.contentRevision,
        payload:{command:'object.project.settings.edit',action:action}};
    },'nemo_native_dispatch');}
    function setLayerVisibility(layerUid,visible){
      if(typeof visible!=='boolean')return Promise.reject(new Error('Native layer visibility target or value is invalid'));
      return visibilityIntent(layerUid,visible);
    }
    function toggleLayerVisibility(layerUid){
      return visibilityIntent(layerUid,function(layer){
        if(!subscription||synchronizing||typeof ports.subscriptionActive==='function'&&!ports.subscriptionActive()||
          !ports.projectionReady(info)||!presentation||!same(info,presentation))
          throw new Error('Native layer visibility synchronization is unavailable');
        return layer.visible===false;
      });
    }
    async function acquirePin(request){
      requireActive();var value=await inspect(request.requestId);
      if(request.operation!=='query.document.project.info'||request.instanceId!==value.instanceId||request.documentId!==value.documentId||request.payload.atRevision!==value.contentRevision)throw new Error('Native save pin changed');
      return {apiVersion:2,requestId:request.requestId,instanceId:value.instanceId,documentId:value.documentId,contentRevision:value.contentRevision,ok:true,
        result:{atRevision:value.contentRevision,documentSnapshotId:value.documentSnapshotId,documentDigest:value.documentDigest,byteCount:value.byteCount}};
    }
    function presentedSession(){return active()&&!synchronizing&&(!subscription||typeof ports.subscriptionActive!=='function'||ports.subscriptionActive())&&presentation&&same(info,presentation)&&pending===0?
      Object.assign({available:true,kind:'native-object',presentation:presentation},info):null;}
    async function readLayerTiming(layerUid,frame,pin){
      // Observe existing work without entering, advancing or settling its queue.
      async function settled(){var observed;do{observed=queue;await observed;}while(observed!==queue);}
      function ready(){var value=presentedSession();return !openingRequested&&same(info,pin)&&value&&
        value.presentation.frame===frame&&integer(frame,0)&&frame<pin.totalFrames&&
        pin.layers.filter(function(layer){return layer.layerUid===layerUid;}).length===1;}
      try{
        await settled();if(!ready())return null;
        var ticket=subscriptionTicket,request={apiVersion:2,requestId:requestId('timing'),instanceId:pin.instanceId,documentId:pin.documentId,
          operation:'query.document.object.layer.timing',payload:{atRevision:pin.contentRevision,documentSnapshotId:pin.documentSnapshotId,
            contextId:'scene-root',layerUid:layerUid,frame:frame}};
        var response=await raw('nemo_native_dispatch',request);
        await settled();
        if(ticket!==subscriptionTicket||!ready()||!response||response.ok!==true||response.instanceId!==pin.instanceId||
          response.documentId!==pin.documentId||response.contentRevision!==pin.contentRevision)return null;
        var result=response.result;
        if(!result||Object.keys(result).length!==11||result.instanceId!==pin.instanceId||result.documentId!==pin.documentId||
          ['atRevision','documentSnapshotId','contextId','layerUid','frame'].some(function(k){return result[k]!==request.payload[k];}))return null;
        return freeze(Object.assign({},result));
      }catch(_){return null;} // A refused read never fences, mutates or revives an owner.
    }
    function exportPng(destination,frames,onProgress,signal){
      if(openingRequested||!active())return Promise.reject(new Error('Native project Open is in progress'));
      var submitted=epoch(info),requested=Array.isArray(frames)?frames.slice():null;return enqueue(async function(){
      guardEpoch(submitted);
      var pin=requireActive();
      if(!requested||requested.length===0||requested.length>32||!requested.every(function(frame,i){
        return integer(frame,0)&&frame<pin.totalFrames&&(i===0||requested[i-1]<frame);
      }))throw new Error('Native object export frame range is invalid');
      if(signal&&signal.aborted)return {status:'cancelled'};
      var request=frameRequest(requested[0]);if(requested.length>1)request.frames=requested;
      var manifest=requested.map(function(_,i){return 'frame_'+String(i+1).padStart(4,'0')+'.png';});
      request.destinationDirectory=destination;request.maximumEncodedBytes=4*1024*1024;
      var receipt=await raw('nemo_native_object_export',request);
      if(!receipt||receipt.apiVersion!==2||receipt.requestId!==request.requestId||receipt.instanceId!==request.instanceId||receipt.documentId!==request.documentId||
        receipt.contentRevision!==request.contentRevision||receipt.documentSnapshotId!==request.documentSnapshotId||receipt.published!==true||receipt.cleanupComplete!==true||
        receipt.status!=='succeeded'||receipt.cleanupStatus!=='complete'||receipt.externalEffectDisposition!=='committed'||receipt.errorCode!==null||
        !Array.isArray(receipt.artifactFiles)||receipt.artifactFiles.length!==manifest.length||!manifest.every(function(name,i){return receipt.artifactFiles[i]===name;}))throw new Error('Native export publication is unconfirmed');
      if(onProgress)onProgress(requested.length,requested.length);return receipt;
    });}
    return Object.freeze({openPath:openPath,newProject:newProject,isActive:active,isIndeterminate:function(){return phase==='indeterminate';},blocksLegacy:function(){return phase!=='idle'||openingRequested;},
      info:function(){return info;},requestId:requestId,reference:reference,historyFromUi:history,setLayerVisibility:setLayerVisibility,readLayerTiming:readLayerTiming,
      get editProjectSettings(){return subscription&&!synchronizing&&active()&&presentedSession()&&ports.projectionReady(info)&&
        (typeof ports.subscriptionActive!=='function'||ports.subscriptionActive())?editProjectSettings:undefined;},
      get createLayer(){return subscription&&!synchronizing&&active()&&presentedSession()&&ports.projectionReady(info)&&
        (typeof ports.subscriptionActive!=='function'||ports.subscriptionActive())?createLayer:undefined;},
      get toggleLayerVisibility(){return subscription&&!synchronizing&&active()&&ports.projectionReady(info)&&
        (typeof ports.subscriptionActive!=='function'||ports.subscriptionActive())?toggleLayerVisibility:undefined;},
      presentPreview:presentPreview,renderPreview:renderPreview,exportPng:exportPng,readPresentedObjectSession:presentedSession,
      acquirePin:acquirePin,savePinned:function(request){return raw('nemo_native_object_save_as',request);},
      getNativeIdentity:function(){if(phase==='idle')return null;var pin=requireActive();return {documentId:pin.documentId,generation:pin.lifecycleGeneration};},
      identity:requireActive,requestRelease:function(){return false;},releaseCurrent:function(){return false;},
      persistenceJSON:function(){throw new Error('Native project files must be saved through the native file command');}});
  }
  var desktops=new WeakMap(),uiBindings=new WeakMap();
  function desktop(root){
    if(!root.__TAURI__||!root.__TAURI__.core)return null;
    if(desktops.has(root))return desktops.get(root);
    var invoke=root.__TAURI__.core.invoke.bind(root.__TAURI__.core),transport=null;
    function settingsNotice(kind,info){var c=root.NemoNativeProjectSettingsController;if(c&&typeof c[kind]==='function')c[kind](info);}
    function publishRevision(info){
      settingsNotice('willProject',info);
      if(!root.NemoNativeObjectProjectFile||!root.NemoNativeObjectProjectFile.refreshLayerTable(root,info))
        throw new Error('Native object row projection is uncorrelated');
      if(typeof root.updateUI!=='function')throw new Error('Native document UI publication is unavailable');
      root.updateUI();
      if(root.SMReference)root.SMReference.refreshNative();
      if(root.SMProject)root.SMProject.refreshActiveTabDirtyDot();
      return true;
    }
    var session=create(Object.assign({},root.NemoNativeViewportUnderlay.desktop(root),{invoke:invoke,publishRevision:publishRevision,
      subscribeRevisions:async function(synchronize,disconnected,trackedInvoke){
        if(!root.NemoApplicationMcpTransport||!root.__TAURI__.event||typeof root.__TAURI__.event.listen!=='function')
          return null; // Private file Open remains usable; no toggle/subscriber is advertised.
        var call=trackedInvoke||invoke;
        transport=root.NemoApplicationMcpTransport.createNativeTauriTransport(function(command,args){
          var response=call(command,args);
          if(command==='nemo_native_revision_sync'&&args&&args.request.action==='acknowledge')
            return Promise.resolve(response).then(function(value){
              // Observe the completed ACK after transport clears its in-flight flag.
              setTimeout(function(){try{if(root.NemoNativeObjectUI&&root.NemoNativeObjectUI.refreshTiming)
                root.NemoNativeObjectUI.refreshTiming(root);settingsNotice('presented');}catch(_){}},0);
              return value;
            });
          return response;
        });
        await transport.connect();return transport.subscribeRevisions(root.__TAURI__.event.listen.bind(root.__TAURI__.event),synchronize,disconnected);
      },
      unsubscribeRevisions:function(){return transport?transport.disconnect():Promise.resolve();},
      subscriptionActive:function(){return !!transport&&!!transport.status()&&!transport.isSynchronizing();},
      dispatchMutation:function(command,request){return transport.dispatchObject(command,request);},
      dispatchApplication:function(request){
        if(!transport||!root.NemoNativeApplicationAdapter||typeof root.NemoNativeApplicationAdapter.createNativeApplicationAdapter!=='function')
          throw new Error('Native public application client is unavailable');
        return root.NemoNativeApplicationAdapter.createNativeApplicationAdapter('ui',transport).dispatch(request);
      },
      openDiagnostic:typeof root.__NEMO_P38_FIXTURE_OPEN_DIAGNOSTIC__==='function'?function(event,code){try{root.__NEMO_P38_FIXTURE_OPEN_DIAGNOSTIC__(event,code);}catch(_){} }:null,
      presented:function(info){settingsNotice('presented',info);},
      changed:function(info){if(root.NemoNativeObjectProjectFile&&root.NemoNativeObjectProjectFile.hasProjection(root))publishRevision(info);
        else{if(root.SMReference)root.SMReference.refreshNative();if(root.SMProject)root.SMProject.refreshActiveTabDirtyDot();}},
      notice:function(message){if(root.showToast)root.showToast(message);},
      failed:function(){if(root.showToast)root.showToast('Native project state is unconfirmed. Reopen Nemo before continuing.');}}));
    desktops.set(root,session);return session;
  }
  function uiSession(root){var session=desktop(root);return session&&session.blocksLegacy()?session:null;}
  function nativeObjectUiPorts(root){
    var ports=uiBindings.get(root);if(ports)return ports;
    ports=Object.freeze({
      projectionReady:function(info){return !!root.NemoNativeObjectProjectFile&&root.NemoNativeObjectProjectFile.projectionReadyForUI(root,info)===true;},
      currentSession:function(){return uiSession(root);},
      selectLayer:function(index,preserve,uid,expected){return selectLayerUI(root,index,preserve,uid,expected);},
      projectSettings:function(){return root.NemoNativeProjectSettingsController||null;},
      repaint:function(frameOnly){return paintUI(root,frameOnly);}
    });uiBindings.set(root,ports);return ports;
  }
  function uiFailure(root,error){
    var start=root.document&&root.document.getElementById('start-screen');if(start)start.classList.remove('hid');
    throw error;
  }
  function paintUI(root,frameOnly){
    var session=uiSession(root);if(!session){
      uiBindings.delete(root);
      if(root.NemoNativeObjectUI&&root.NemoNativeObjectUI.restoreInspector)root.NemoNativeObjectUI.restoreInspector(root);
      return false;
    }
    try{if(session.isIndeterminate()||!root.NemoNativeObjectUI||root.NemoNativeObjectUI.render(root,session.info(),frameOnly,nativeObjectUiPorts(root))!==true)
      throw new Error('Native document UI presentation is unavailable');}
    catch(error){return uiFailure(root,error);}
    return true;
  }
  function selectLayerUI(root,index,preserveLayerSel,uid,expected){
    var session=uiSession(root);if(!session)return false;
    if(expected&&!same(session.info(),expected))return true;
    try{if(!root.NemoNativeObjectUI)throw new Error('Native document UI selection is unavailable');
      root.NemoNativeObjectUI.select(root,session.info(),index,preserveLayerSel,uid,session.readPresentedObjectSession(),nativeObjectUiPorts(root));}
    catch(error){return uiFailure(root,error);}
    return true; // An unconfirmed selection is consumed, never a legacy fallback.
  }
  return Object.freeze({create:create,desktop:desktop,paintUI:paintUI,selectLayerUI:selectLayerUI});
}));
