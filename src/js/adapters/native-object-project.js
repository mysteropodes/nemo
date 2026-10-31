/* Native project session: detached UI metadata and request ordering only.
 * Composition supplies stable, narrow UI callbacks; the presenter never resolves
 * this provider or the file adapter. Callback reads retain current native authority. */
(function(root,factory){
  var desktopBinding=typeof module==='object'&&module.exports?require('./native-object-project-desktop.js'):root.NemoNativeObjectProjectDesktop;
  var propertyBinding=typeof module==='object'&&module.exports?require('./native-object-project-properties.js'):root.NemoNativeObjectProjectProperties;
  var exposureBinding=typeof module==='object'&&module.exports?require('./native-object-open-exposure.js'):root.NemoNativeObjectOpenExposure;
  var readBinding=typeof module==='object'&&module.exports?require('./native-object-project-reads.js'):root.NemoNativeObjectProjectReads;
  var exportBinding=typeof module==='object'&&module.exports?require('./native-object-project-export.js'):root.NemoNativeObjectProjectExport;
  var api=factory(typeof module==='object'&&module.exports?require('./native-object-project-metadata.js'):root.NemoNativeObjectProjectMetadata,desktopBinding,propertyBinding,typeof module==='object'&&module.exports?require('./native-frame-edit-contract.js'):root.NemoNativeFrameEditContract,exposureBinding,exportBinding,readBinding,typeof module==='object'&&module.exports?require('./native-object-project-entry-contract.js'):root.NemoNativeObjectProjectEntryContract);
  if(typeof module==='object'&&module.exports)module.exports=api;
  else root.NemoNativeObjectProject=api;
}(typeof globalThis==='object'?globalThis:this,function(Metadata,Desktop,Properties,FrameEdit,Exposure,Exports,Reads,Entry){
  'use strict';
  function id(v){return typeof v==='string'&&/^[A-Za-z0-9][A-Za-z0-9._:/-]{0,127}$/.test(v);}
  function integer(v,min){return Number.isSafeInteger(v)&&v>=min;}
  function freeze(v){if(v&&typeof v==='object'){Object.keys(v).forEach(function(k){freeze(v[k]);});Object.freeze(v);}return v;}
  function same(a,b){return !!a&&!!b&&['instanceId','documentId','contentRevision','documentSnapshotId','lifecycleGeneration'].every(function(k){return a[k]===b[k];});}
  var metadata=Metadata&&Metadata.decode;if(typeof metadata!=='function')throw new Error('Native project metadata parser is unavailable');
  function create(ports){
    if(!Properties||typeof Properties.create!=='function')throw new Error('Native project property ports are unavailable');
    if(!Exposure||typeof Exposure.navigation!=='function'||typeof Exposure.previewScheduling!=='function')throw new Error('Native project view ports are unavailable');
    var previewFailures=new WeakMap(),view=Exposure.navigation(ports.viewMode,openDiagnostic,ports.displayFilter,ports.ghosts,ports.alphaChecker,ports.currentOutline,ports.hideShadowGuides),phase='idle',info=null,presentation=null,queue=Promise.resolve(),serial=0,pending=0,openingRequested=false;
    var subscription=null,subscriptionTicket=0,synchronizing=false,openAttempt=null,revisionAttempt=null,presentationBounds=null,openExposure=null,openingCancelled=false;
    var observation=Desktop.attemptObserver(function(){return ports.invoke.apply(ports,arguments);},openDiagnostic,
      function(){return openAttempt;},function(){return revisionAttempt;},previewFailures),invokeTracked=observation.invoke;
    function requestId(kind){return 'project-'+kind+'-'+(++serial);}
    function openDiagnostic(event,code){try{if(typeof ports.openDiagnostic==='function')ports.openDiagnostic(event,code);}catch(_){} }
    function openStage(stage){observation.stage(openAttempt,stage);}
    function openFailure(error){observation.openFailure(openAttempt,error);openAttempt=null;}
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
      var attempt={stage:'admission',invokeErrors:new Set()};
      function stage(value){observation.stage(attempt,value,'revision-stage');}
      function failure(error){return observation.revisionFailure(attempt,error);}
      if(ticket!==subscriptionTicket||!sameEpoch(pin,epoch(info)))
        return Promise.reject(failure(new Error('Native object revision subscription was replaced')));
      if(!active()||synchronizing||!sameEpoch(binding,pin)||!id(binding.subscriptionId)||
        !sameEpoch(event,pin)||!id(event.requestId)||!integer(event.fromRevision,0)||
        !integer(event.toRevision,event.fromRevision+1)||
        info.contentRevision>event.toRevision)return Promise.reject(fence(failure(new Error('Native object revision event is stale'))));
      synchronizing=true;presentation=null;previewScheduling.clearWanted();
      stage('queued');
      return enqueue(async function(){
        revisionAttempt=attempt;
        if(ticket!==subscriptionTicket||!sameEpoch(pin,epoch(info)))throw new Error('Native object revision subscription was replaced');
        guardEpoch(pin);
        stage('inspect');var next=await inspect();
        if(ticket!==subscriptionTicket||next.contentRevision!==event.toRevision)
          throw new Error('Native object revision inspection is uncorrelated');
        stage('publication');if(await ports.publishRevision(next,lease())!==true)throw new Error('Native object UI publication is unconfirmed');
        stage('present');await present(ports.frame?ports.frame():0,next,{subscriptionId:binding.subscriptionId,event:event});
        stage('final-projection');
        if(ticket!==subscriptionTicket||!same(info,next)||!ports.projectionReady(next))
          throw new Error('Native object revision changed before acknowledgment');
        stage('synchronized');return freeze(Object.assign({},event));
      }).catch(function(error){
        failure(error);
        if(ticket!==subscriptionTicket||!sameEpoch(pin,epoch(info)))throw error;
        throw fence(error);
      }).finally(function(){if(revisionAttempt===attempt)revisionAttempt=null;if(ticket===subscriptionTicket)synchronizing=false;});
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
      var framed=!!intent&&typeof intent==='object'&&Object.prototype.hasOwnProperty.call(intent,'totalFrames');
      if(!intent||typeof intent!=='object'||Array.isArray(intent)||Object.keys(intent).length!==(framed?4:3)||!integer(intent.canvasW,1)||!integer(intent.canvasH,1)||
        !Number.isFinite(intent.fps)||intent.fps<=0||framed&&(!integer(intent.totalFrames,1)||intent.totalFrames>4294967295))
        return Promise.reject(localRefusal('Native New Project scalar intent is invalid'));
      var scalars={canvasW:intent.canvasW,canvasH:intent.canvasH,fps:intent.fps};
      if(framed)scalars.totalFrames=intent.totalFrames;
      return enterProject({newProject:scalars},transition);
    }
    function activateProject(target,transition){
      try{target=Entry.target(target);}catch(error){return Promise.reject(localRefusal(error.message));}
      return enterProject({retainedProject:target},transition);
    }
    function enterProject(source,transition){
      if(openingRequested||phase!=='idle'&&phase!=='native')return Promise.reject(new Error('Native project Open is unavailable'));
      openingRequested=true;openingCancelled=false;previewScheduling.clearWanted();
      return enqueue(async function(){
        var previous=info,priorPresentation=presentation,priorBounds=presentationBounds,priorPhase=phase,dispatched=false,inBootstrap=false,reconciling=false;
        function current(){if(openingCancelled)throw new Error('Native project Open was cancelled');if(openExposure)openExposure.assertActive();if(!reconciling&&transition&&transition.isCurrent&&transition.isCurrent()!==true){
          if(dispatched)throw new Error('Native project request changed after dispatch');
          throw localRefusal('Native project request was cancelled');}}
        phase='opening';presentation=null;if(ports.surfaceChanged&&!(source.retainedProject&&priorPresentation))ports.surfaceChanged(null);// tab switch: keep the previous frame until present() replaces it
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
          var retained=source.retainedProject,rid=requestId(retained?'activate':source.newProject?'new':'open');
          var request=Entry.request(source,before,previous,rid);
          current();dispatched=true;inBootstrap=true;
          var receipt=await openExposure.join(function(){return raw(retained?'nemo_native_object_inactive_project':'nemo_native_object_bootstrap',request);});inBootstrap=false;
          if(typeof ports.openDiagnostic==='function')openAttempt={stage:'receipt',invokeErrors:new Set()};
          var revision=Entry.receipt(receipt,before,previous,rid,retained);
          openStage('installed-status');var installed=await openExposure.join(status);
          Entry.installed(installed,before,previous,receipt,revision,retained);
          openStage('metadata');var candidate=await openExposure.join(function(){return inspectFor({instanceId:installed.instanceId,documentId:installed.documentId,
            contentRevision:revision,lifecycleGeneration:installed.lifecycleGeneration});});
          var entryFrame=Entry.candidate(candidate,source,receipt);
          info=candidate;current();
          openStage('first-preview');await present(entryFrame,candidate,null,openExposure);openStage('first-owner');await openExposure.join(function(){return verifyOwner(candidate);});
          current();
          if(transition&&transition.reveal){openStage('reveal');await openExposure.join(function(){return transition.reveal(candidate,retained&&retained.view);});
            openStage('second-preview');await present(entryFrame,candidate,null,openExposure);openStage('second-owner');await openExposure.join(function(){return verifyOwner(candidate);});}
          openExposure.detach();
          current();phase='native';
          openStage('ui-commit');if(transition&&transition.commit)await openExposure.join(function(){return transition.commit(candidate,receipt);});
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
      if(error&&typeof error==='object')previewFailures.set(error,detail);return error;
    }
    function frameRequest(frame,pin){
      pin=pin||requireActive();var ctx=ports.context?ports.context():null; // #2256: an entered component presents its own context and frame count
      if(!integer(frame,0)||frame>=(ctx?ctx.totalFrames:pin.totalFrames))throw previewFailure('frame-range',new Error('Frame is outside the native project'));
      var viewport;
      try{viewport=ports.mapping(pin);}catch(error){throw previewFailure('mapping',error);}
      try{
        if(!viewport||!viewport.cssBounds||!(viewport.cssBounds.width>0)||!(viewport.cssBounds.height>0)||
            !viewport.physicalExtent||!integer(viewport.physicalExtent.width,1)||!integer(viewport.physicalExtent.height,1))
          throw new Error('Native project viewport is unavailable');
      }catch(error){throw previewFailure('viewport',error);}
      return {apiVersion:2,requestId:requestId('frame'),instanceId:pin.instanceId,documentId:pin.documentId,
        contentRevision:pin.contentRevision,documentSnapshotId:pin.documentSnapshotId,lifecycleGeneration:pin.lifecycleGeneration,
        contextId:ctx?ctx.contextId:'scene-root',scopeKind:'authored',frame:frame,
        outputSpec:{kind:'frame',format:'rgba8',width:pin.canvasW,height:pin.canvasH,colorInterpretation:'srgb',alphaMode:'straight'},
        viewport:viewport};
    }
    async function present(frame,pin,revisionSync,exposure,allowSuperseded){
      var request;
      try{request=frameRequest(frame,pin);}catch(error){if(ports.surfaceChanged)ports.surfaceChanged(null);throw error;}
      var selected=view.read(pin||requireActive()),mode=selected.mode;Exposure.requestView(request,selected);
      if(revisionSync)request.revisionSync=revisionSync;
      var bounds=Object.assign({},request.viewport.cssBounds);
      var receipt;
      function inspect(value,allowDeferred){try{return Exposure.inspectReceipt(value,['opening','native'].includes(phase)&&same(info,request)&&(allowSuperseded||Exposure.sameView(selected,view.read(info))),allowDeferred,mode,selected.navigation,selected.displayFilter,selected.ghosts,selected.alphaChecker,selected.currentOutline,selected.hideShadowGuides);}
        catch(error){throw previewFailure('receipt-access',error);}}
      try{receipt=exposure?await exposure.run(function(){return raw('nemo_native_object_preview',request);},function(){return verifyOwner(pin);},inspect):inspect(await raw('nemo_native_object_preview',request),!!occlusion&&!revisionSync);}
      catch(error){if(ports.surfaceChanged)ports.surfaceChanged(null);throw error;}
      // An edit, Undo/Redo or a navigation presented while the window is not visible waits for its exposure. A revision
      // synchronization keeps its own contract: an unpresented sync refuses and the session becomes indeterminate.
      if(!exposure&&!revisionSync&&receipt.status==='deferred-occluded'){presentation=null;if(ports.surfaceChanged)ports.surfaceChanged(null);occlusion.arm();return null;}
      if(receipt.failure){
        presentation=null;if(ports.surfaceChanged)ports.surfaceChanged(null);throw previewFailure(receipt.failure,new Error('Native project frame was not presented ('+receipt.failure+')'));
      }
      presentation=freeze(Exposure.presentedFrame(receipt,request,frame));
      presentationBounds=bounds;if(ports.surfaceChanged)ports.surfaceChanged(presentation,bounds);
      return presentation;
    }
    var previewScheduling = Exposure.previewScheduling({
      active: active,
      opening: function(){return openingRequested;},
      synchronizing: function(){return synchronizing;},
      owner: function(){return epoch(info);},
      guardEpoch: guardEpoch,
      enqueue: enqueue,
      present: present,
      readView: function(){return view.read(requireActive());},
      presented: function(){return presentation;},
      notice: function(error){if(ports.notice)ports.notice(error.message);}
    });
    var presentPreview = previewScheduling.presentPreview; /* #2381: closing the project with no replacement; hooks only, the sequence lives in native-object-project-close.js */ var closeHooks={isActive:active,opening:function(){return openingRequested;},enqueue:enqueue,requestId:requestId,invoke:invokeTracked,status:status,verify:verifyOwner,retire:retireSubscription,attach:attachSubscription,fence:fence,lock:function(){var held={info:info,presentation:presentation,bounds:presentationBounds,phase:phase};openingRequested=true;openingCancelled=false;previewScheduling.clearWanted();phase='opening';presentation=null;if(ports.surfaceChanged)ports.surfaceChanged(null);return held;},restore:function(held){info=held.info;phase=held.phase;presentation=held.presentation;if(held.presentation&&ports.surfaceChanged)ports.surfaceChanged(held.presentation,held.bounds);},done:function(){info=null;presentation=null;presentationBounds=null;phase='idle';},unlock:function(){openingRequested=false;openingCancelled=false;}};
    var occlusion=typeof ports.subscribeWindowOcclusion==='function'?Exposure.occlusionWake(ports.subscribeWindowOcclusion,function(){
      if(active()&&!openingRequested)renderPreview(ports.frame?ports.frame():0);}):null;
    var renderPreview = previewScheduling.renderPreview;
    // #2256: the lease a revision publication rides while the project queue is held (an entered component re-reads itself through it).
    function lease(){return {dispatch:function(request){return raw('nemo_native_dispatch',request);}};}
    function changed(){return ports.changed?ports.changed(info,lease()):undefined;}
    // #2256: ONE guard for every document write: nothing is edited while a component is presented (the presented context is not scene-root).
    function sceneRoot(){var ctx=ports.context?ports.context():null;return !ctx&&(!presentation||presentation.contextId===undefined||presentation.contextId==='scene-root');}
    function insideComponent(){return Promise.reject(new Error('Native edits are not available inside a component; return to the Scene first'));}
    function mutation(makeRequest,command,needsPresentation,historyAction){
      if(openingRequested||!active())return Promise.reject(new Error('Native project Open is in progress'));
      if(synchronizing)return Promise.reject(new Error('Native object revision synchronization is in progress'));
      if(!historyAction&&!sceneRoot())return insideComponent();
      var submitted=epoch(info),origin=needsPresentation?freeze(presentedSession()):null,ticket=subscriptionTicket;return enqueue(async function(){
      guardEpoch(submitted);var pin=requireActive(),request=needsPresentation?Desktop.withActionPresentation(origin,function(){return pending===1&&currentPresentation(origin,ticket)&&
        origin.presentation.frame===(ports.frame?ports.frame():0);},function(lease){return makeRequest(pin,lease);}):makeRequest(pin);presentation=null;
      var receipt;
      try{receipt=await raw(command,request);}catch(error){
        // A transport error cannot establish whether a native command committed.
        try{await inspect();await changed();}catch(unknown){throw fence(unknown);}
        try{await present(ports.frame?ports.frame():0);}catch(unpresented){if(!historyAction&&ports.notice)ports.notice(unpresented.message);}
        throw error;
      }
      if(!receipt||receipt.apiVersion!==2||receipt.requestId!==request.requestId||receipt.instanceId!==pin.instanceId||
        receipt.documentId!==pin.documentId||!integer(receipt.contentRevision,pin.contentRevision)||typeof receipt.ok!=='boolean')throw fence(new Error('Native mutation response is uncorrelated'));
      try{await inspect();await changed();}catch(unknown){throw fence(unknown);}
      if(!receipt.ok){
        try{await present(ports.frame?ports.frame():0);}catch(unpresented){if(!historyAction&&ports.notice)ports.notice(unpresented.message);}
        throw new Error('Native command refused: '+(receipt.error&&receipt.error.code||'unavailable'));
      }
      await present(ports.frame?ports.frame():0);return receipt;
    });}
    // #2256: `inContext` (the component command route of the vector family only) accepts the presentation of the ENTERED context, never another.
    function currentPresentation(origin,ticket,inContext){var held=ports.context&&ports.context();return !openingRequested&&!synchronizing&&active()&&origin&&(inContext?!!held&&origin.presentation.contextId===held.contextId:!held)&&origin.presentation===presentation&&view.matches(presentation,info)&&same(info,origin)&&ticket===subscriptionTicket&&
      (!subscription||typeof ports.subscriptionActive!=='function'||ports.subscriptionActive())&&
      typeof ports.projectionReady==='function'&&ports.projectionReady(info)===true;}
    // #2256: the resource route (composition settings, comments, brushes, font import, every scene editor) is scene-only: refused while a component is entered, at submit AND at dispatch (a command queued before the entry must not run after it). Only the component route (`mode` true, the vector family) is granted the entered presentation. The QUERY route (`mode` 'query', the project-global readers: palette, guides, shadow, perspective, symmetry, comments, composition, workspace, custom brushes) also runs inside a component, but only `query.*` operations reach the owner: every write stays refused there.
    function runFontResource(action,mode){var inContext=mode===true,query=mode==='query';if(openingRequested||synchronizing||!active())return Promise.reject(new Error('Native font owner busy or unavailable'));
      var held=function(){return !!(ports.context&&ports.context());},inHeld=function(o){var c=ports.context&&ports.context();return !!c&&!!o&&!!o.presentation&&o.presentation.contextId===c.contextId;};
      var submitted=epoch(info),origin=freeze(presentedSession()),ticket=subscriptionTicket,refused=function(){return query?held()&&!inHeld(origin):!inContext&&!sceneRoot();};if(refused())return insideComponent();
      return enqueue(async function(){guardEpoch(submitted);if(refused())return insideComponent();var running=true;
        try{return await action({
          // Read-only and action-local: only this operation's own pending entry is exempted; the public getter stays fenced.
          readPresentedObjectSession:function(){return running&&pending===1&&currentPresentation(origin,ticket,inContext||query&&held())?origin:null;},
          dispatch:function(request){guardEpoch(submitted);if(query&&!/^query\./.test(String(request&&request.operation)))return Promise.reject(new Error('Native resource queries cannot write'));return raw('nemo_native_dispatch',request);},
          reconcile:async function(revision){guardEpoch(submitted);if(query)throw new Error('Native resource queries cannot reconcile a write');var next;try{next=await inspect();guardEpoch(submitted);if(revision!==undefined&&next.contentRevision!==revision)throw new Error('Native font import revision is uncorrelated');}catch(error){throw fence(error);}
            await changed();await present(ports.frame?ports.frame():0,next);return next;}});
        }finally{running=false;}});}
    function reference(action){return mutation(function(pin){return {apiVersion:2,requestId:requestId('reference'),instanceId:pin.instanceId,
      documentId:pin.documentId,lifecycleGeneration:pin.lifecycleGeneration,expectedRevision:pin.contentRevision,
      documentSnapshotId:pin.documentSnapshotId,action:typeof action==='function'?action(pin.reference):action};},'nemo_native_object_reference');}
    // `steps` (default 1) moves that many entries as ONE native restore and one new revision: the History panel's jump (#2376 G26).
    function history(action,steps){if(action!=='undo'&&action!=='redo'||steps!==undefined&&!(Number.isSafeInteger(steps)&&steps>=1))return Promise.reject(new Error('Unsupported history action'));
      return mutation(function(pin){return {apiVersion:2,requestId:requestId(action),instanceId:pin.instanceId,documentId:pin.documentId,
        operation:'command.document.object.'+action,expectedRevision:pin.contentRevision,payload:steps>1?{command:'object.'+action,steps:steps}:{command:'object.'+action}};},'nemo_native_object_dispatch',false,action).catch(function(error){
        try{var reason=previewFailures.get(error)||(['invalid_request','unavailable','stale_revision','stale_owner','wrong_instance','wrong_document','cancelled_before_dispatch'].includes(error&&error.code)?error.code:'unavailable');
          if(phase!=='indeterminate'&&ports.notice)ports.notice('Native '+(action==='undo'?'Undo':'Redo')+' could not be confirmed ('+reason+').');}catch(_){}throw error;});}
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
      if(!same(pin,expected)||!payload||Array.isArray(payload)||Object.keys(payload).length!==4||!['command','layerUid','flag','value'].every(function(k){return Object.prototype.hasOwnProperty.call(payload,k);})||payload.command!=='object.layer.policy.edit'||!(['locked','shy'].includes(payload.flag)&&typeof payload.value==='boolean'||payload.flag==='keyLock'&&(payload.value===null||['in','out','layer'].includes(payload.value)))||typeof payload.layerUid!=='string'||!payload.layerUid||pin.layers.filter(function(l){return l.layerUid===payload.layerUid;}).length!==1)throw new Error('Native layer policy intent is stale or invalid');
      return {apiVersion:2,requestId:requestId('layer-policy'),instanceId:pin.instanceId,documentId:pin.documentId,
        operation:'command.document.object.layer.policy.edit',expectedRevision:pin.contentRevision,
        payload:{command:'object.layer.policy.edit',layerUid:payload.layerUid,flag:payload.flag,value:payload.value}};
    },'nemo_native_dispatch');}
    function editFrames(operation,payload,expected,current){return mutation(function(pin,lease){if(typeof current==='function'&&(!lease.readPresentedObjectSession()||current(lease)!==true||!lease.readPresentedObjectSession()))throw new Error('Native frame edit intent is stale');if(!same(pin,expected)||!FrameEdit||!FrameEdit.valid(operation,payload))throw new Error('Native frame edit intent is '+(same(pin,expected)?'invalid':'stale'));return {apiVersion:2,requestId:requestId('frame-edit'),instanceId:pin.instanceId,documentId:pin.documentId,operation:operation,expectedRevision:pin.contentRevision,payload:JSON.parse(JSON.stringify(payload))};},'nemo_native_dispatch',typeof current==='function');}
    function editProjectSettings(action,expected){return mutation(function(pin){
      if(!same(pin,expected))throw new Error('Native project settings intent is stale');
      return {apiVersion:2,requestId:requestId('project-settings'),instanceId:pin.instanceId,documentId:pin.documentId,
        operation:'command.document.object.project.settings.edit',expectedRevision:pin.contentRevision,
        payload:{command:'object.project.settings.edit',action:action}};
    },'nemo_native_dispatch');}
    function setLayerVisibility(layerUid,visible){
      if(!sceneRoot())return insideComponent();
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
    function presentedSession(onRefusal){
      var reason=!active()?'inactive':synchronizing?'synchronizing':
        subscription&&typeof ports.subscriptionActive==='function'&&!ports.subscriptionActive()?'subscription-inactive':
        !presentation?'presentation-missing':!view.matches(presentation,info)?'view-mismatch':
        !same(info,presentation)?'owner-mismatch':pending!==0?'queue-pending':null;
      if(reason){if(typeof onRefusal==='function')try{onRefusal(reason);}catch(_){}return null;}
      return Object.assign({available:true,kind:'native-object',presentation:presentation},info);
    }
    function capturePresentationSession(){
      return Entry.capturePresentation({same:same,readPresented:presentedSession,state:function(){
        return {active:active(),opening:openingRequested,synchronizing:synchronizing,
          info:info,queue:queue,ticket:subscriptionTicket};}});
    }
    var reads=Reads.create({requestId:requestId,
      settled:async function(){var observed;do{observed=queue;await observed;}while(observed!==queue);},
      dispatch:function(request){return raw('nemo_native_dispatch',request);},
      state:function(){return {info:info,presented:presentedSession(),ticket:subscriptionTicket,active:active(),
        opening:openingRequested,pending:pending,synchronizing:synchronizing,
        projectionReady:typeof ports.projectionReady==='function'&&ports.projectionReady(info)===true};}});
    var properties=Properties.create({same:same,freeze:freeze,requestId:requestId,mutation:mutation,
      dispatchApplication:function(request){if(typeof ports.dispatchApplication!=='function')throw new Error('Native public application client is unavailable');return ports.dispatchApplication(request);},
      state:function(){return {info:info,presentation:presentation,ticket:subscriptionTicket,active:active(),opening:openingRequested,pending:pending,
        synchronizing:synchronizing,subscribed:!!subscription,presentationCurrent:active()&&!synchronizing&&presentation&&same(info,presentation),
        get presented(){return presentedSession();},dispatchAvailable:typeof ports.dispatchApplication==='function',
        get projectionReady(){return typeof ports.projectionReady==='function'&&ports.projectionReady(info)===true;},
        get sceneRoot(){return sceneRoot();},
        get subscriptionActive(){return typeof ports.subscriptionActive==='function'&&ports.subscriptionActive()===true;}};}});
    function exportFrames(destination,frames,onProgress,signal,format){
      if(openingRequested||!active())return Promise.reject(new Error('Native project Open is in progress'));
      // #2256: the frame range, fps and work area the export reads belong to the entered component, never to the scene it would render.
      if(!sceneRoot())return Promise.reject(new Error('Export is not available inside a component; return to the Scene first'));
      if(!Exports||typeof Exports.run!=='function')return Promise.reject(new Error('Native object export adapter is unavailable'));
      var submitted=epoch(info);return Exports.run({
        enqueue:function(action){return enqueue(function(){guardEpoch(submitted);return action(requireActive());});},
        frameRequest:frameRequest,invoke:function(request){return raw('nemo_native_object_export',request);}
      },destination,frames,onProgress,signal,format);
    }
    function exportPng(destination,frames,onProgress,signal){return exportFrames(destination,frames,onProgress,signal,'png');}
    function exportSvg(destination,frames,onProgress,signal){return exportFrames(destination,frames,onProgress,signal,'svg');}
    return Object.freeze({openPath:openPath,newProject:newProject,activateProject:activateProject,closeProject:function(){var C=typeof module==='object'&&module.exports?require('./native-object-project-close.js'):globalThis.NemoNativeObjectProjectClose;return C?C.run(closeHooks):Promise.reject(new Error('Native project Close is unavailable'));},cancelOpen:function(){if(!openingRequested)return false;openingCancelled=true;if(openExposure)openExposure.cancel();return true;},isActive:active,isIndeterminate:function(){return phase==='indeterminate';},blocksLegacy:function(){return phase!=='idle'||openingRequested;},
      readSubscriptionGeneration:function(){return subscriptionTicket;},
      readPropertyMetadata:properties.readPropertyMetadata,readProperty:properties.readProperty,
      get editPropertyTrack(){return properties.available()?properties.editPropertyTrack:undefined;},
      get editProperty(){return properties.available()?properties.editProperty:undefined;},
      info:function(){return info;},requestId:requestId,runFontResource:function(action){return runFontResource(action);},runResourceCommand:function(action){return runFontResource(action);},runComponentCommand:function(action){return runFontResource(action,true);},runResourceQuery:function(action){return runFontResource(action,'query');},reference:reference,historyFromUi:function(action){return history(action);},jumpHistory:history,readHistory:function(offset){try{return reads.readHistory(requireActive(),offset);}catch(error){return Promise.reject(error);}},setLayerVisibility:setLayerVisibility,readLayerTiming:reads.readLayerTiming,readLayerRange:reads.readLayerRange,readLayerPlacement:reads.readLayerPlacement,readLayerCells:reads.readLayerCells,readImageMesh:reads.readImageMesh,readWidgets:reads.readWidgets,readGhosts:reads.readGhosts,readRig:reads.readRig,readRigWeights:reads.readRigWeights,readSubselect:reads.readSubselect,readPresentedStrokes:reads.readPresentedStrokes,previewSubselect:reads.previewSubselect,
      get editLayerPolicy(){return typeof ports.dispatchApplication==='function'&&(!ports.policyAvailable||ports.policyAvailable()===true)&&subscription&&!synchronizing&&active()&&presentedSession()&&ports.projectionReady(info)&&
        (typeof ports.subscriptionActive!=='function'||ports.subscriptionActive())&&sceneRoot()?editLayerPolicy:undefined;},
      get editFrames(){return FrameEdit&&typeof ports.dispatchApplication==='function'&&subscription&&!synchronizing&&active()&&presentedSession()&&ports.projectionReady(info)&&
        (typeof ports.subscriptionActive!=='function'||ports.subscriptionActive())&&sceneRoot()?editFrames:undefined;},
      get editProjectSettings(){return subscription&&!synchronizing&&active()&&presentedSession()&&ports.projectionReady(info)&&
        (typeof ports.subscriptionActive!=='function'||ports.subscriptionActive())&&sceneRoot()?editProjectSettings:undefined;},
      get createLayer(){return subscription&&!synchronizing&&active()&&presentedSession()&&ports.projectionReady(info)&&
        (typeof ports.subscriptionActive!=='function'||ports.subscriptionActive())&&sceneRoot()?createLayer:undefined;},
      get importBatch(){return subscription&&!synchronizing&&active()&&presentedSession()&&ports.projectionReady(info)&&
        (typeof ports.subscriptionActive!=='function'||ports.subscriptionActive())&&sceneRoot()?importBatch:undefined;},
      get toggleLayerVisibility(){return subscription&&!synchronizing&&active()&&ports.projectionReady(info)&&
        (typeof ports.subscriptionActive!=='function'||ports.subscriptionActive())&&sceneRoot()?toggleLayerVisibility:undefined;},
      beginViewportNavigation:function(kind,point){return active()&&!openingRequested&&!synchronizing&&presentation&&same(info,presentation)&&(!ports.projectionReady||ports.projectionReady(info))&&(!ports.subscriptionActive||ports.subscriptionActive())?view.begin(presentation,info,ports.frame(),ports.mapping(info),presentationBounds,kind,point):(openDiagnostic('navigation-begin','owner-unavailable'),null);},updateViewportNavigation:function(token,point,factor,target){try{if(!active()||openingRequested||synchronizing||!presentation||!same(info,presentation)||ports.projectionReady&&!ports.projectionReady(info)||ports.subscriptionActive&&!ports.subscriptionActive())throw new Error('Native navigation owner is unavailable');view.update(token,info,ports.frame(),ports.mapping(info),point,factor,target);openDiagnostic('navigation-update','view-updated');return renderPreview(ports.frame());}catch(error){openDiagnostic('navigation-update','rejected');return Promise.reject(error);}},presentPreview:presentPreview,renderPreview:renderPreview,exportPng:exportPng,exportSvg:exportSvg,readPresentedObjectSession:presentedSession,
      captureSaveSession:capturePresentationSession,capturePresentationSession:capturePresentationSession,acquirePin:acquirePin,savePinned:function(request){return raw('nemo_native_object_save_as',request);},
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
      Object.assign(ports,Desktop.composeUI(root,ports,root.NemoNativeObjectUI.createPolicyPorts(root,ports)));
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
    var entered=root.NemoNativeComponentContext;if(entered&&!session.isIndeterminate()&&entered.paint(root,frameOnly))return true; // #2256: an entered component paints read-only
    var ports=nativeObjectUiPorts(root);
    try{if(session.isIndeterminate()||!root.NemoNativeObjectUI||root.NemoNativeObjectUI.render(root,session.info(),frameOnly,ports)!==true)
      throw new Error('Native document UI presentation is unavailable');
      Desktop.didPaintUI(root,ports);}
    catch(error){return uiFailure(root,error);}
    // #2372: a repaint follows a tool, layer, frame or mode change; the Select overlay re-checks its context (one-shot).
    var overlay=root.NemoNativeSelectOverlay;if(overlay&&typeof overlay.notify==='function')overlay.notify(root);
    var aids=root.NemoNativeViewAids;if(aids&&typeof aids.notify==='function')aids.notify(root); // #2373
    return true;
  }
  function selectLayerUI(root,index,preserveLayerSel,uid,expected){
    var session=uiSession(root);if(!session)return false;
    if(expected&&!same(session.info(),expected))return true;
    if(root.NemoNativeComponentContext&&root.NemoNativeComponentContext.presented(root))return true; // #2256: no scene selection inside a component
    try{if(!root.NemoNativeObjectUI)throw new Error('Native document UI selection is unavailable');
      root.NemoNativeObjectUI.select(root,session.info(),index,preserveLayerSel,uid,session.readPresentedObjectSession(),nativeObjectUiPorts(root));}
    catch(error){return uiFailure(root,error);}
    return true; // An unconfirmed selection is consumed, never a legacy fallback.
  }
  return Object.freeze({create:create,desktop:desktop,paintUI:paintUI,selectLayerUI:selectLayerUI,
    bindUIComposition:function(root,compose){return Desktop.bindUIComposition(root,compose);}});
}));
