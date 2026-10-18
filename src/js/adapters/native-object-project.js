/* Native project session: detached UI metadata and request ordering only.
 * Composition supplies stable, narrow UI callbacks; the presenter never resolves
 * this provider or the file adapter. Callback reads retain current native authority. */
(function(root,factory){
  var desktopBinding=typeof module==='object'&&module.exports?require('./native-object-project-desktop.js'):root.NemoNativeObjectProjectDesktop;
  var propertyBinding=typeof module==='object'&&module.exports?require('./native-object-project-properties.js'):root.NemoNativeObjectProjectProperties;
  var exposureBinding=typeof module==='object'&&module.exports?require('./native-object-open-exposure.js'):root.NemoNativeObjectOpenExposure;
  var api=factory(typeof module==='object'&&module.exports?require('./native-object-project-metadata.js'):root.NemoNativeObjectProjectMetadata,desktopBinding,propertyBinding,typeof module==='object'&&module.exports?require('./native-frame-edit-contract.js'):root.NemoNativeFrameEditContract,exposureBinding);
  if(typeof module==='object'&&module.exports)module.exports=api;
  else root.NemoNativeObjectProject=api;
}(typeof globalThis==='object'?globalThis:this,function(Metadata,Desktop,Properties,FrameEdit,Exposure){
  'use strict';
  function id(v){return typeof v==='string'&&/^[A-Za-z0-9][A-Za-z0-9._:/-]{0,127}$/.test(v);}
  function integer(v,min){return Number.isSafeInteger(v)&&v>=min;}
  function freeze(v){if(v&&typeof v==='object'){Object.keys(v).forEach(function(k){freeze(v[k]);});Object.freeze(v);}return v;}
  function same(a,b){return !!a&&!!b&&['instanceId','documentId','contentRevision','documentSnapshotId','lifecycleGeneration'].every(function(k){return a[k]===b[k];});}
  var metadata=Metadata&&Metadata.decode;if(typeof metadata!=='function')throw new Error('Native project metadata parser is unavailable');
  function create(ports){
    var phase='idle',info=null,presentation=null,queue=Promise.resolve(),serial=0,pending=0,rendering=null,wantedFrame=null,openingRequested=false;
    var subscription=null,subscriptionTicket=0,synchronizing=false,openAttempt=null,presentationBounds=null,openExposure=null,openingCancelled=false;
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
      },invokeTracked,function(){return !openingCancelled&&(!openExposure||!openExposure.isStopped())&&ticket===subscriptionTicket&&active()&&sameEpoch(pin,epoch(info));});
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
      openingRequested=true;openingCancelled=false;wantedFrame=null;
      return enqueue(async function(){
        var previous=info,priorPresentation=presentation,priorBounds=presentationBounds,priorPhase=phase,dispatched=false,inBootstrap=false,reconciling=false;
        function current(){if(openingCancelled)throw new Error('Native project Open was cancelled');if(openExposure)openExposure.assertActive();if(!reconciling&&transition&&transition.isCurrent&&transition.isCurrent()!==true){
          if(dispatched)throw new Error('Native project request changed after dispatch');
          throw localRefusal('Native project request was cancelled');}}
        phase='opening';presentation=null;if(ports.surfaceChanged)ports.surfaceChanged(null);
        try{
          openExposure=Exposure.create(ports.subscribeWindowOcclusion,current);current();await openExposure.connect();
          await openExposure.join(retireSubscription);
          if(transition&&transition.confirm&&await openExposure.join(function(){return transition.confirm(previous);})!==true){
            if(previous)await openExposure.join(function(){return verifyOwner(previous);});phase=priorPhase;presentation=priorPresentation;if(previous)await openExposure.join(attachSubscription);
            if(priorPresentation&&ports.surfaceChanged)ports.surfaceChanged(priorPresentation,priorBounds);return false;}
          current();
          if(transition&&transition.occlude)await openExposure.join(function(){return transition.occlude();});
          var before=await openExposure.join(status);
          if(!before||before.apiVersion!==2||!id(before.instanceId)||
              (previous?before.available!==true||before.instanceId!==previous.instanceId||before.documentId!==previous.documentId||
                before.contentRevision!==previous.contentRevision||before.lifecycleGeneration!==previous.lifecycleGeneration:before.available!==false))
            throw localRefusal('Native project slot changed before Open');
          var rid=requestId(source.newProject?'new':'open'),request=Object.assign({apiVersion:2,requestId:rid,instanceId:before.instanceId},source);
          if(previous)request.replace={documentId:previous.documentId,expectedRevision:previous.contentRevision,
            documentSnapshotId:previous.documentSnapshotId,lifecycleGeneration:previous.lifecycleGeneration};
          current();dispatched=true;inBootstrap=true;
          var receipt=await openExposure.join(function(){return raw('nemo_native_object_bootstrap',request);});inBootstrap=false;
          if(typeof ports.openDiagnostic==='function')openAttempt={stage:'receipt',invokeErrors:new Set()};
          if(!receipt||receipt.apiVersion!==2||receipt.requestId!==rid||receipt.instanceId!==before.instanceId||
              !id(receipt.documentId)||receipt.contentRevision!==0||!integer(receipt.resourceCount,0)||
              receipt.resourceCount>1||receipt.viewportAvailable!==false||previous&&receipt.documentId===previous.documentId)
            throw new Error('Native project Open could not be reconciled');
          openStage('installed-status');var installed=await openExposure.join(status);
          if(!installed||installed.apiVersion!==2||installed.available!==true||installed.documentId!==receipt.documentId||
              installed.instanceId!==before.instanceId||installed.contentRevision!==0||
              !integer(installed.lifecycleGeneration,previous?previous.lifecycleGeneration+1:1))
            throw new Error('Native project installation is uncorrelated');
          openStage('metadata');var candidate=await openExposure.join(function(){return inspectFor({instanceId:installed.instanceId,documentId:installed.documentId,
            contentRevision:0,lifecycleGeneration:installed.lifecycleGeneration});});
          if(source.newProject&&(candidate.canvasW!==source.newProject.canvasW||candidate.canvasH!==source.newProject.canvasH||
            candidate.fps!==source.newProject.fps||receipt.resourceCount!==0))throw new Error('Native New Project metadata is uncorrelated');
          info=candidate;current();
          openStage('first-preview');await present(0,candidate,null,openExposure);openStage('first-owner');await openExposure.join(function(){return verifyOwner(candidate);});
          current();
          if(transition&&transition.reveal){openStage('reveal');await openExposure.join(function(){return transition.reveal(candidate);});
            openStage('second-preview');await present(0,candidate,null,openExposure);openStage('second-owner');await openExposure.join(function(){return verifyOwner(candidate);});}
          openExposure.detach();
          current();phase='native';
          openStage('ui-commit');if(transition&&transition.commit)await openExposure.join(function(){return transition.commit(candidate);});
          current();
          openStage('publication');changed();openStage('subscriber');await openExposure.join(attachSubscription);
          current();
          return candidate;
        }catch(error){
          openFailure(error);
          if(openingCancelled||openExposure&&openExposure.isStopped())throw fence(error);
          if(!dispatched||inBootstrap&&(disposition(error)==='notDispatched'||!source.newProject&&!previous&&error&&error.code==='unsupported_format')){
            reconciling=true; // Retired request currency cannot block safe prior-owner reconciliation; explicit stop still fences.
            try{
              if(previous)await openExposure.join(function(){return verifyOwner(previous);});
              else{var empty=await openExposure.join(status);if(!empty||empty.apiVersion!==2||empty.available!==false)throw new Error('Native project slot changed');}
              current();info=previous;phase=priorPhase;presentation=priorPresentation;
              if(transition&&transition.restore)await openExposure.join(function(){return transition.restore(previous);});
              if(priorPresentation&&ports.surfaceChanged)ports.surfaceChanged(priorPresentation,priorBounds);
              if(previous)await openExposure.join(attachSubscription);current();
            }catch(unknown){throw fence(unknown);}
            reconciling=false;
            if(!source.newProject&&!previous&&error&&error.code==='unsupported_format'){current();return null;}
            throw disposition(error)==='notDispatched'?error:localRefusal(error&&error.message||'Native project Open was not dispatched');
          }
          throw fence(error);
        }finally{var retiring=openExposure;openExposure=null;openAttempt=null;openingRequested=false;openingCancelled=false;
          if(retiring)try{retiring.close();}catch(error){throw fence(error);}}
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
    async function present(frame,pin,revisionSync,exposure){
      var request;
      try{request=frameRequest(frame,pin);}catch(error){if(ports.surfaceChanged)ports.surfaceChanged(null);throw error;}
      if(revisionSync)request.revisionSync=revisionSync;
      var bounds=Object.assign({},request.viewport.cssBounds);
      var receipt;
      function inspect(value,allowDeferred){try{return Exposure.inspectReceipt(value,['opening','native'].includes(phase)&&same(info,request),allowDeferred);}
        catch(error){throw previewFailure('receipt-access',error);}}
      try{receipt=exposure?await exposure.run(function(){return raw('nemo_native_object_preview',request);},function(){return verifyOwner(pin);},inspect):inspect(await raw('nemo_native_object_preview',request),false);}
      catch(error){if(ports.surfaceChanged)ports.surfaceChanged(null);throw error;}
      if(receipt.failure){
        presentation=null;if(ports.surfaceChanged)ports.surfaceChanged(null);throw previewFailure(receipt.failure,new Error('Native project frame was not presented'));
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
    function runFontResource(action){if(openingRequested||synchronizing||!active())return Promise.reject(new Error('Native font owner busy or unavailable'));
      var submitted=epoch(info);return enqueue(async function(){guardEpoch(submitted);return action({dispatch:function(request){guardEpoch(submitted);return raw('nemo_native_dispatch',request);},
        reconcile:async function(revision){guardEpoch(submitted);var next;try{next=await inspect();guardEpoch(submitted);if(revision!==undefined&&next.contentRevision!==revision)throw new Error('Native font import revision is uncorrelated');}catch(error){throw fence(error);}
          changed();await present(ports.frame?ports.frame():0,next);return next;}});});}
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
    function importBatch(payload,expected){return mutation(function(pin){
      if(!same(pin,expected)||!payload||typeof payload!=='object'||Array.isArray(payload)||payload.command!=='object.import.batch')throw new Error('Native import intent is stale or invalid');
      return {apiVersion:2,requestId:requestId('import-batch'),instanceId:pin.instanceId,documentId:pin.documentId,operation:'command.document.object.import.batch',expectedRevision:pin.contentRevision,payload:payload};
    },'nemo_native_dispatch');}
    function editLayerPolicy(payload,expected){return mutation(function(pin){
      if(!same(pin,expected)||!payload||Array.isArray(payload)||Object.keys(payload).length!==4||!['command','layerUid','flag','value'].every(function(k){return Object.prototype.hasOwnProperty.call(payload,k);})||payload.command!=='object.layer.policy.edit'||!['locked','shy'].includes(payload.flag)||typeof payload.value!=='boolean'||typeof payload.layerUid!=='string'||!payload.layerUid||pin.layers.filter(function(l){return l.layerUid===payload.layerUid;}).length!==1)throw new Error('Native layer policy intent is stale or invalid');
      return {apiVersion:2,requestId:requestId('layer-policy'),instanceId:pin.instanceId,documentId:pin.documentId,
        operation:'command.document.object.layer.policy.edit',expectedRevision:pin.contentRevision,
        payload:{command:'object.layer.policy.edit',layerUid:payload.layerUid,flag:payload.flag,value:payload.value}};
    },'nemo_native_dispatch');}
    function editFrames(operation,payload,expected){return mutation(function(pin){if(!same(pin,expected)||!FrameEdit||!FrameEdit.valid(operation,payload))throw new Error('Native frame edit intent is '+(same(pin,expected)?'invalid':'stale'));return {apiVersion:2,requestId:requestId('frame-edit'),instanceId:pin.instanceId,documentId:pin.documentId,operation:operation,expectedRevision:pin.contentRevision,payload:JSON.parse(JSON.stringify(payload))};},'nemo_native_dispatch');}
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
    if(!Properties||typeof Properties.create!=='function')throw new Error('Native project property ports are unavailable');
    var properties=Properties.create({same:same,freeze:freeze,requestId:requestId,mutation:mutation,
      dispatchApplication:function(request){if(typeof ports.dispatchApplication!=='function')throw new Error('Native public application client is unavailable');return ports.dispatchApplication(request);},
      state:function(){return {info:info,presentation:presentation,ticket:subscriptionTicket,active:active(),opening:openingRequested,pending:pending,
        synchronizing:synchronizing,subscribed:!!subscription,presentationCurrent:active()&&!synchronizing&&presentation&&same(info,presentation),
        get presented(){return presentedSession();},dispatchAvailable:typeof ports.dispatchApplication==='function',
        get projectionReady(){return typeof ports.projectionReady==='function'&&ports.projectionReady(info)===true;},
        get subscriptionActive(){return typeof ports.subscriptionActive==='function'&&ports.subscriptionActive()===true;}};}});
    function exportFrames(destination,frames,onProgress,signal,format){
      if(openingRequested||!active())return Promise.reject(new Error('Native project Open is in progress'));
      var submitted=epoch(info),requested=Array.isArray(frames)?frames.slice():null;return enqueue(async function(){
      guardEpoch(submitted);
      var pin=requireActive();
      if(!requested||requested.length===0||requested.length>32||!requested.every(function(frame,i){
        return integer(frame,0)&&frame<pin.totalFrames&&(i===0||requested[i-1]<frame);
      }))throw new Error('Native object export frame range is invalid');
      if(signal&&signal.aborted)return {status:'cancelled'};
      var request=frameRequest(requested[0]);if(requested.length>1)request.frames=requested;
      var manifest=requested.map(function(_,i){return 'frame_'+String(i+1).padStart(4,'0')+'.'+format;});
      if(format==='svg')request.format='svg';
      request.destinationDirectory=destination;request.maximumEncodedBytes=4*1024*1024;
      var receipt=await raw('nemo_native_object_export',request);
      if(!receipt||receipt.apiVersion!==2||receipt.requestId!==request.requestId||receipt.instanceId!==request.instanceId||receipt.documentId!==request.documentId||
        receipt.contentRevision!==request.contentRevision||receipt.documentSnapshotId!==request.documentSnapshotId||receipt.published!==true||receipt.cleanupComplete!==true||
        receipt.status!=='succeeded'||receipt.cleanupStatus!=='complete'||receipt.externalEffectDisposition!=='committed'||receipt.errorCode!==null||
        !Array.isArray(receipt.artifactFiles)||receipt.artifactFiles.length!==manifest.length||!manifest.every(function(name,i){return receipt.artifactFiles[i]===name;}))throw new Error('Native export publication is unconfirmed');
      if(onProgress)onProgress(requested.length,requested.length);return receipt;
    });}
    function exportPng(destination,frames,onProgress,signal){return exportFrames(destination,frames,onProgress,signal,'png');}
    function exportSvg(destination,frames,onProgress,signal){return exportFrames(destination,frames,onProgress,signal,'svg');}
    return Object.freeze({openPath:openPath,newProject:newProject,cancelOpen:function(){if(!openingRequested)return false;openingCancelled=true;if(openExposure)openExposure.cancel();return true;},isActive:active,isIndeterminate:function(){return phase==='indeterminate';},blocksLegacy:function(){return phase!=='idle'||openingRequested;},
      readSubscriptionGeneration:function(){return subscriptionTicket;},
      readPropertyMetadata:properties.readPropertyMetadata,readProperty:properties.readProperty,
      get editPropertyTrack(){return properties.available()?properties.editPropertyTrack:undefined;},
      get editProperty(){return properties.available()?properties.editProperty:undefined;},
      info:function(){return info;},requestId:requestId,runFontResource:runFontResource,runResourceCommand:runFontResource,reference:reference,historyFromUi:history,setLayerVisibility:setLayerVisibility,readLayerTiming:readLayerTiming,
      get editLayerPolicy(){return typeof ports.dispatchApplication==='function'&&(!ports.policyAvailable||ports.policyAvailable()===true)&&subscription&&!synchronizing&&active()&&presentedSession()&&ports.projectionReady(info)&&
        (typeof ports.subscriptionActive!=='function'||ports.subscriptionActive())?editLayerPolicy:undefined;},
      get editFrames(){return FrameEdit&&typeof ports.dispatchApplication==='function'&&subscription&&!synchronizing&&active()&&presentedSession()&&ports.projectionReady(info)&&
        (typeof ports.subscriptionActive!=='function'||ports.subscriptionActive())?editFrames:undefined;},
      get editProjectSettings(){return subscription&&!synchronizing&&active()&&presentedSession()&&ports.projectionReady(info)&&
        (typeof ports.subscriptionActive!=='function'||ports.subscriptionActive())?editProjectSettings:undefined;},
      get createLayer(){return subscription&&!synchronizing&&active()&&presentedSession()&&ports.projectionReady(info)&&
        (typeof ports.subscriptionActive!=='function'||ports.subscriptionActive())?createLayer:undefined;},
      get importBatch(){return subscription&&!synchronizing&&active()&&presentedSession()&&ports.projectionReady(info)&&
        (typeof ports.subscriptionActive!=='function'||ports.subscriptionActive())?importBatch:undefined;},
      get toggleLayerVisibility(){return subscription&&!synchronizing&&active()&&ports.projectionReady(info)&&
        (typeof ports.subscriptionActive!=='function'||ports.subscriptionActive())?toggleLayerVisibility:undefined;},
      presentPreview:presentPreview,renderPreview:renderPreview,exportPng:exportPng,exportSvg:exportSvg,readPresentedObjectSession:presentedSession,
      acquirePin:acquirePin,savePinned:function(request){return raw('nemo_native_object_save_as',request);},
      getNativeIdentity:function(){if(phase==='idle')return null;var pin=requireActive();return {documentId:pin.documentId,generation:pin.lifecycleGeneration};},
      identity:requireActive,requestRelease:function(){return false;},releaseCurrent:function(){return false;},
      persistenceJSON:function(){throw new Error('Native project files must be saved through the native file command');}});
  }
  var desktops=new WeakMap(),uiBindings=new WeakMap();
  function desktop(root){
    if(!root.__TAURI__||!root.__TAURI__.core)return null;
    if(desktops.has(root))return desktops.get(root);
    if(!Desktop||typeof Desktop.createPorts!=='function')throw new Error('Native desktop ports factory is unavailable');
    var session=create(Desktop.createPorts(root));
    desktops.set(root,session);return session;
  }
  function uiSession(root){var session=desktop(root);return session&&session.blocksLegacy()?session:null;}
  function nativeObjectUiPorts(root){
    var ports=uiBindings.get(root);if(ports)return ports;
    ports={
      projectionReady:function(info){return !!root.NemoNativeObjectProjectFile&&root.NemoNativeObjectProjectFile.projectionReadyForUI(root,info)===true;},
      currentSession:function(){return uiSession(root);},
      fontResources:function(){var f=root.SMVectorText;return f&&['listNativeFontAssets','loadNativeFontAsset','importNativeFontSource'].every(function(k){return typeof f[k]==='function';})?Object.freeze({list:f.listNativeFontAssets,load:f.loadNativeFontAsset,importSource:f.importNativeFontSource}):null;},
      readPropertyMetadata:function(selector){
        var session=uiSession(root);
        if(!session)return Promise.reject(new Error('Native metadata session is unavailable'));
        return session.readPropertyMetadata(selector);
      },
      readProperty:function(selector){var session=uiSession(root);if(!session)return Promise.reject(new Error('Native property session is unavailable'));return session.readProperty(selector);},
      get editPropertyTrack(){var session=uiSession(root);return session&&session.editPropertyTrack;},
      get editProperty(){var session=uiSession(root);return session&&session.editProperty;},
      selectLayer:function(index,preserve,uid,expected){return selectLayerUI(root,index,preserve,uid,expected);},
      projectSettings:function(){return root.NemoNativeProjectSettingsController||null;},
      repaint:function(frameOnly){return paintUI(root,frameOnly);}
    };
    if(root.NemoNativeObjectUI&&typeof root.NemoNativeObjectUI.createPolicyPorts==='function')
      Object.assign(ports,root.NemoNativeObjectUI.createPolicyPorts(root,ports));
    Object.freeze(ports);uiBindings.set(root,ports);return ports;
  }
  function uiFailure(root,error){
    var start=root.document&&root.document.getElementById('start-screen');if(start)start.classList.remove('hid');
    throw error;
  }
  function paintUI(root,frameOnly){
    var session=uiSession(root);if(!session){
      var old=uiBindings.get(root);
      if(old&&old.disposePolicyEntries)old.disposePolicyEntries();
      uiBindings.delete(root);
      if(root.NemoNativeObjectUI&&root.NemoNativeObjectUI.restoreInspector)root.NemoNativeObjectUI.restoreInspector(root);
      return false;
    }
    var ports=nativeObjectUiPorts(root);
    try{if(session.isIndeterminate()||!root.NemoNativeObjectUI||root.NemoNativeObjectUI.render(root,session.info(),frameOnly,ports)!==true)
      throw new Error('Native document UI presentation is unavailable');
      if(ports.bindPolicyEntries)ports.bindPolicyEntries();}
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
