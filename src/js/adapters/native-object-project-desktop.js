/* Native desktop integration ports only; Project owns session and lifecycle state. */
(function(root,factory){
  var api=factory();
  if(typeof module==='object'&&module.exports)module.exports=api;
  else root.NemoNativeObjectProjectDesktop=api;
}(typeof globalThis==='object'?globalThis:this,function(){
  'use strict';
  var openLogs=new WeakMap(),settingsLogs=new WeakMap(),uiCompositions=new WeakMap();
  // Read-only chronology for ordinary settings and their native pins. Storage is
  // bounded and fire-and-forget; a missing, failed or hung sink cannot gate work.
  function settingsDiagnostic(root,event,value){
    if(!['input','change','focus','blur','pointerdown','stale-rejected','owner','intent','dispatch','metadata','acknowledged','outcome','stop','resume',
      'preview-presented','export-request','export-result'].includes(event))return;
    try{
      function own(key){var d=value&&Object.getOwnPropertyDescriptor(value,key);return d&&Object.prototype.hasOwnProperty.call(d,'value')?d.value:undefined;}
      var row={event:event};
      [['instanceId',/^[a-f0-9]{8}(?:-[a-f0-9]{4}){3}-[a-f0-9]{12}$/],
        ['documentId',/^native-object-document-[0-9]+$/],
        ['gestureDocumentId',/^native-object-document-[0-9]+$/],
        ['documentSnapshotId',/^native-object:native-object-document-[0-9]+:[0-9]+$/],
        ['requestId',/^project-[a-z-]+-[0-9]+$/]].forEach(function(pair){var v=own(pair[0]);if(typeof v==='string'&&v.length<=128&&pair[1].test(v))row[pair[0]]=v;});
      ['contentRevision','expectedRevision','atRevision','fromRevision','toRevision','lifecycleGeneration','gestureLifecycleGeneration','frame','viewGeneration'].forEach(function(key){var v=own(key);if(Number.isSafeInteger(v)&&v>=0)row[key]=v;});
      if(['p-cw','p-ch','proj-fps','p-cbg','btn-dims-lock'].includes(own('field')))row.field=own('field');
      if(['input','change'].includes(own('sourceEvent')))row.sourceEvent=own('sourceEvent');
      var kind=own('kind'),status=own('status');
      if(['setCanvasSize','setFps','setCanvasBackground','setWorkArea','user','native-settings-intent','native-settings-publication'].includes(kind))row.kind=kind;
      if(['applied-presented','unchanged','superseded','unavailable','unconfirmed','applied-unconfirmed','succeeded','failed','cancelled'].includes(status))row.status=status;
      var log=settingsLogs.get(root);if(!log){log={startedAt:Date.now(),serial:0,events:[],writing:false,pending:null};settingsLogs.set(root,log);}
      row.sequence=++log.serial;row.at=Date.now();log.events.push(row);log.events=log.events.slice(-128);
      log.pending=JSON.stringify({schemaVersion:1,kind:'native-settings-diagnostic',startedAt:log.startedAt,events:log.events});
      if(log.writing)return;log.writing=true;
      (async function(){try{
        var api=root.__TAURI__,base=await api.path.appDataDir(),dir=base.replace(/[\\/]+$/,'')+'/diagnostics';
        await api.fs.mkdir(dir,{recursive:true});
        while(log.pending!==null){var bytes=log.pending;log.pending=null;
          await api.fs.writeTextFile(dir+'/native-settings.pending.json',bytes);
          await api.fs.rename(dir+'/native-settings.pending.json',dir+'/native-settings.json');}
      }catch(_){log.pending=null;}finally{log.writing=false;}})();
    }catch(_){}
  }
  function bindUIComposition(root,compose){
    if(!root||!root.document||typeof compose!=='function')throw new TypeError('Native UI composition is unavailable');
    var bound=uiCompositions.get(root);
    if(bound){
      if(bound.document!==root.document||bound.compose!==compose)throw new Error('Native UI composition is already bound');
      return true;
    }
    uiCompositions.set(root,{document:root.document,compose:compose});return true;
  }
  function composeUI(root,base,policy){
    var bound=uiCompositions.get(root);
    if(!bound||bound.document!==root.document)throw new Error('Native UI composition is unavailable');
    return bound.compose(base,policy);
  }
  var stages=['receipt','installed-status','metadata','first-preview','first-owner','reveal','second-preview','second-owner','ui-commit','publication','subscriber'];
  var previewDetails=['frame-range','mapping','viewport','receipt-access','owner','receipt','status','work-id','view-generation','view','display-filter',
    'status-stale-discarded','status-deferred-timeout','status-deferred-occluded','status-failed-validation','status-failed-device-lost',
    'status-failed-retry-lost','status-failed-retry-outdated','status-failed-recreate','status-failed-reconfigure'];
  var revisionStages=['admission','queued','inspect','publication','present','final-projection','synchronized'];
  // Observation only: the project owner supplies current attempts and decides all
  // stages, fencing and retirement. Preserve invoke values and rejection identity.
  function attemptObserver(invoke,emit,readOpen,readRevision,previewFailures){
    function errorCode(error){
      var code='unknown';
      try{var value=error&&error.code;if(['invalid_request','unavailable','unsupported_format','replacement_refused',
        'stale_revision','stale_owner','cancelled','wrong_instance','wrong_document','cancelled_before_dispatch'].includes(value))code=value;}catch(_){}
      return code;
    }
    function stage(attempt,value,event){
      if(!attempt)return;
      attempt.stage=value;attempt.invokeErrors=new Set();
      if(event)emit(event,value);
    }
    function tracked(command,args){
      var open=readOpen(),openStage=open&&open.stage,revision=readRevision(),revisionStage=revision&&revision.stage;
      function rejected(error){
        if(open&&readOpen()===open&&open.stage===openStage)open.invokeErrors.add(error);
        if(revision&&readRevision()===revision&&revision.stage===revisionStage)revision.invokeErrors.add(error);
        throw error;
      }
      var result;
      try{result=arguments.length>1?invoke(command,args):invoke(command);}catch(error){return rejected(error);}
      return (open||revision)&&result&&typeof result.then==='function'?result.then(function(value){return value;},rejected):result;
    }
    function origin(attempt,error){return attempt.invokeErrors.has(error)?'invoke-rejected':'local-refusal';}
    function openFailure(attempt,error){
      if(!attempt)return;
      var code=errorCode(error),disposition='other';
      try{if(error&&error.details&&error.details.disposition==='notDispatched')disposition='notDispatched';}catch(_){}
      emit('post-bootstrap-failed','post-bootstrap-'+attempt.stage+':'+origin(attempt,error)+':'+code+':'+disposition);
    }
    function revisionFailure(attempt,error){
      var detail=error&&typeof error==='object'?previewFailures.get(error)||'none':'none';
      emit('revision-failed',attempt.stage+':'+origin(attempt,error)+':'+errorCode(error)+':'+detail);
      return error;
    }
    return Object.freeze({invoke:tracked,stage:stage,openFailure:openFailure,revisionFailure:revisionFailure});
  }
  function safeCode(event,code){
    if(event==='revision-stage')return revisionStages.includes(code)?code:'unknown';
    if(event==='revision-failed'){
      if(typeof code!=='string')return 'unknown';
      var revision=code.split(':');
      return revision.length===4&&revisionStages.includes(revision[0])&&['invoke-rejected','local-refusal'].includes(revision[1])&&
        ['unknown','invalid_request','unavailable','unsupported_format','replacement_refused','stale_revision','stale_owner','cancelled',
          'wrong_instance','wrong_document','cancelled_before_dispatch'].includes(revision[2])&&
        (revision[3]==='none'||previewDetails.includes(revision[3]))?code:'unknown';
    }
    if(event==='tab-capture-refused')return ['inactive','synchronizing','subscription-inactive','presentation-missing',
      'view-mismatch','owner-mismatch','queue-pending','capture-superseded','tab-unavailable','capture-unavailable','ui-changed'].includes(code)?code:'unknown';
    if(event==='camera-control')return ['native-refused','selected-canvas','selected-stored-camera'].includes(code)?code:'unknown';
    if(event==='camera-presented')return ['canvas','stored-camera'].includes(code)?code:'unknown';
    if(event==='bpm-settings-refused')return ['controller-missing','cache-empty','retired','generation-stale',
      'session-replaced','inactive','five-pin-stale','subscription-stale','presentation-missing',
      'presentation-mismatch','projection-unready','exception'].includes(code)?code:'unknown';
    if(['guide-entry-refused','guide-release-refused','guide-edit-refused'].includes(event))return ['cache-empty','retired','generation-stale',
      'session-replaced','inactive','five-pin-stale','subscription-stale','presentation-missing','presentation-mismatch',
      'projection-unready','exception','ui-pending','ui-loading','session-missing','info-missing','identity-missing',
      'projection-missing','state-missing','guide-pin-mismatch','presentation-owner','presentation-status','view-missing',
      'presentation-pin-mismatch','context-mismatch','work-id-invalid','view-generation-invalid','frame-mismatch','view-mismatch',
      'canvas-bounds-unavailable','area-bounds-unavailable','affine-unavailable','coordinates-unavailable','epoch-stale',
      'guide-projection-replaced','presentation-replaced','geometry-changed','origin-missing','origin-changed','gesture-active',
      'area-missing','container-missing','axis-invalid','ordinal-invalid','guides-locked','rulers-off','point-unavailable',
      'native-unconfirmed','applied-unconfirmed','edit-exception'].includes(code)?code:'unknown';
    if(event==='navigation-begin'){
      if(code==='owner-unavailable')return code;
      if(typeof code!=='string')return 'unknown';
      var navigation=code.split(':');
      return navigation.length===2&&['pan','zoom','other'].includes(navigation[0])&&
        ['view-refused','admitted'].includes(navigation[1])?code:'unknown';
    }
    if(event==='navigation-update')return ['view-updated','rejected'].includes(code)?code:'unknown';
    if(event==='bootstrap-rejected')return code==='unsupported-format'?'unsupported-format':'other';
    if(event==='file-stage')return ['projection','ui-refresh','hide-start','layout-wait','layout-settled'].includes(code)?code:'unknown';
    if(event==='file-failed')return ['not-dispatched','unconfirmed'].includes(code)?code:'unknown';
    if(event!=='post-bootstrap-failed')return null;
    if(typeof code!=='string')return 'unknown';
    var parts=code.split(':'),stage=parts[0].replace(/^post-bootstrap-/,'');
    var known=stages.includes(stage)||['first-preview','second-preview'].some(function(prefix){return previewDetails.some(function(detail){return stage===prefix+'-'+detail;});});
    return parts.length===4&&known&&['invoke-rejected','local-refusal'].includes(parts[1])&&
      ['unknown','invalid_request','unavailable','unsupported_format','replacement_refused','stale_revision','stale_owner','cancelled',
        'wrong_instance','wrong_document','cancelled_before_dispatch'].includes(parts[2])&&['other','notDispatched'].includes(parts[3])?code:'unknown';
  }
  // Production-only observation: fixed vocabulary, no paths, names, messages, stacks or document bytes.
  // A single coalesced writer keeps the last 32 events; storage can fail or hang without delaying Open.
  function openDiagnostic(root,event,code){
    if(!['file-open-started','file-new-started','file-completed','file-cancelled','file-failed','file-stage',
      'bootstrap-dispatched','bootstrap-resolved','bootstrap-rejected','post-bootstrap-failed','revision-stage','revision-failed','bpm-settings-refused',
      'guide-entry-refused','guide-release-refused','guide-edit-refused','navigation-begin','navigation-update','camera-control','camera-presented','tab-capture-refused'].includes(event))return;
    try{
      var log=openLogs.get(root);if(!log){log={startedAt:Date.now(),events:[],serial:0,attempt:0,writing:false,pending:null};openLogs.set(root,log);}
      if(event==='file-open-started'||event==='file-new-started')log.attempt++;
      log.events.push({sequence:++log.serial,attempt:log.attempt,event:event,code:safeCode(event,code)});
      log.events=log.events.slice(-32);
      log.pending=JSON.stringify({schemaVersion:1,kind:'native-open-diagnostic',startedAt:log.startedAt,updatedAt:Date.now(),events:log.events});
      if(!log.writing){
        log.writing=true;
        (async function(){
          try{
            var api=root.__TAURI__,base=await api.path.appDataDir(),dir=base.replace(/[\\/]+$/,'')+'/diagnostics';
            await api.fs.mkdir(dir,{recursive:true});
            while(log.pending!==null){var bytes=log.pending;log.pending=null;
              await api.fs.writeTextFile(dir+'/native-open.pending.json',bytes);
              await api.fs.rename(dir+'/native-open.pending.json',dir+'/native-open.json');}
          }catch(_){log.pending=null;}
          finally{log.writing=false;}
        })();
      }
    }catch(_){}
    // Preserve the existing optional fixture observer; it is never required to collect production evidence.
    try{if(typeof root.__NEMO_P38_FIXTURE_OPEN_DIAGNOSTIC__==='function')root.__NEMO_P38_FIXTURE_OPEN_DIAGNOSTIC__(event,
      ['revision-stage','revision-failed','bpm-settings-refused','guide-entry-refused','guide-release-refused','guide-edit-refused','navigation-begin','navigation-update','camera-control','camera-presented','tab-capture-refused'].includes(event)?safeCode(event,code):code);}catch(_){}
  }
  // The captured read proof exists only during this action's synchronous guard.
  function withActionPresentation(origin,current,action){
    var running=true,lease=Object.freeze({readPresentedObjectSession:function(){return running&&origin&&current()===true?origin:null;}});
    try{return action(lease);}finally{running=false;}
  }
  // Inspect only own data slots of the already-correlated presentation. Never
  // invoke an accessor just to observe mode; no owner/frame/geometry is logged.
  function cameraPresentedCode(presentation) {
    try {
      var view=Object.getOwnPropertyDescriptor(presentation,'view');
      var mode=view&&Object.prototype.hasOwnProperty.call(view,'value')&&Object.getOwnPropertyDescriptor(view.value,'mode');
      return mode&&Object.prototype.hasOwnProperty.call(mode,'value')&&['canvas','stored-camera'].includes(mode.value)?mode.value:'unknown';
    } catch (_) { return 'unknown'; }
  }
  var cameraUIBindings = new WeakMap();
  function cameraSession(root) {
    var project = root.NemoNativeObjectProject;
    return project && typeof project.desktop === 'function' ? project.desktop(root) : null;
  }
  function nativeCameraUI(root, session) {
    return !!(root.__TAURI__ && root.__TAURI__.core) || !!(session &&
      (session.isActive() || typeof session.blocksLegacy === 'function' && session.blocksLegacy()));
  }
  // Reuse the actual camera toolbar button; do not insert an unmatched timeline row.
  function renderCameraViewLock(root) {
    if (cameraUIBindings.get(root) !== root.document) return false;
    var session = cameraSession(root);
    if (!nativeCameraUI(root, session)) return false;
    var button = root.document.getElementById('btn-camera');
    if (!button) return false;
    var shown = session && session.readPresentedObjectSession(), presentation = shown && shown.presentation;
    button.hidden = !(presentation && presentation.view && presentation.view.cameraAvailable === true);
    button.title = root.SM.t('titleCameraViewLockHint');
    button.setAttribute('aria-pressed', String(root.state.cameraView === true));
    return true;
  }
  function bindCameraUI(root, legacy) {
    var document = root.document, button = document.getElementById('btn-camera');
    cameraUIBindings.set(root, document);
    if (button) button.addEventListener('click', function () {
      var selectedCameraView, session = cameraSession(root);
      if (nativeCameraUI(root, session)) {
        var shown = session && session.readPresentedObjectSession(), presentation = shown && shown.presentation;
        if (!presentation || !presentation.view || presentation.view.cameraAvailable !== true) {
          openDiagnostic(root,'camera-control','native-refused');return;
        }
        // Capture the existing assignment value without another state read.
        root.state.cameraView = selectedCameraView = !root.state.cameraView;
        var preview = session.renderPreview(root.state.currentFrame);
        renderCameraViewLock(root);
        // This witnesses a selected value after the existing synchronous path,
        // not native completion. Preserve the original return/promise unchanged.
        openDiagnostic(root,'camera-control',selectedCameraView?'selected-stored-camera':'selected-canvas');
        return preview;
      }
      legacy.toggle();
    });
    // pushUndo() here (2026-07: "récupère des données exploitable pour
    // résoudre les soucis" — the feedback action log/undo stack was
    // silently skipping this button entirely, unlike a camera keyframe
    // added/removed by DRAGGING, which already calls pushUndo() via
    // onDown in camera.js) — cameraKeys IS part of
    // layersSnapshotNow()'s undo snapshot, so this was a real gap, not a
    // structural exclusion like StoryBoard/audio below.
    var addKey = document.getElementById('btn-cam-addkey');
    if (addKey) addKey.addEventListener('click', function () {
      if (!nativeCameraUI(root, cameraSession(root))) legacy.addKey();
    });
    var editEase = document.getElementById('btn-cam-ease-edit');
    if (editEase) editEase.addEventListener('click', function () {
      if (!nativeCameraUI(root, cameraSession(root))) legacy.editEase();
    });
  }
  function didPaintUI(root,ports){
    if(ports.bindPolicyEntries)ports.bindPolicyEntries();
    if(root.SMCamera&&typeof root.SMCamera.renderNativeViewLock==='function')root.SMCamera.renderNativeViewLock();
  }
  function createPorts(root){
    var nativeInvoke=root.__TAURI__.core.invoke.bind(root.__TAURI__.core),transport=null,presentationVersion=0;
    function invokeProjectObserved(command,args){
      var pin;
      if(command==='nemo_native_object_export')try{pin=JSON.parse(args.requestJson);settingsDiagnostic(root,'export-request',pin);}catch(_){}
      var response=nativeInvoke(command,args);
      if(pin)Promise.resolve(response).then(function(receipt){settingsDiagnostic(root,'export-result',receipt);},function(){});
      return response;
    }
    function settingsNotice(kind,info){[root.NemoNativeProjectSettingsController,root.NemoNativeCompositionController,root.NemoNativePaletteController,root.NemoNativeGuideController,root.NemoNativeShadowController,root.NemoNativePerspectiveController,root.NemoNativeSymmetryController,root.NemoNativeWorkspaceController,root.NemoNativeCustomBrushController,root.NemoNativeCommentsController].forEach(function(c){if(c&&typeof c[kind]==='function')c[kind](info);});}
    function presentationStamp(info){
      var project=root.NemoNativeObjectProject;
      if(!project||typeof project.desktop!=='function')return null;
      var session=project.desktop(root);
      if(!session||typeof session.info!=='function'||session.info()!==info||!info||
        typeof session.readSubscriptionGeneration!=='function'||typeof session.readPresentedObjectSession!=='function')return null;
      var shown=session.readPresentedObjectSession(),presentation=shown===null?null:shown&&shown.presentation;
      if(presentation!==null&&(!presentation||typeof presentation!=='object'))return null;
      return {session:session,info:info,ticket:session.readSubscriptionGeneration(),document:root.document,presentation:presentation};
    }
    function publishRevision(info,lease){
      settingsDiagnostic(root,'metadata',info);
      settingsNotice('willProject',info);
      // #2256: inside an entered component the scene rows are not the projected rows; the context re-reads itself through the lease the
      // publication runs in (its context, frame count and layers are settled BEFORE the frame of this revision is presented).
      var entered=root.NemoNativeComponentContext;if(entered&&entered.presented(root))return entered.revision(root,info,lease).then(function(){root.updateUI();return true;});
      if(!root.NemoNativeObjectProjectFile||!root.NemoNativeObjectProjectFile.refreshLayerTable(root,info))
        throw new Error('Native object row projection is uncorrelated');
      if(typeof root.updateUI!=='function')throw new Error('Native document UI publication is unavailable');
      root.updateUI();
      if(root.SMReference)root.SMReference.refreshNative();
      if(root.SMProject)root.SMProject.refreshActiveTabDirtyDot();
      return true;
    }
    return Object.assign({},root.NemoNativeViewportUnderlay.desktop(root),{invoke:invokeProjectObserved,publishRevision:publishRevision,
      context:function(){var c=root.NemoNativeComponentContext;return c?c.presented(root):null;},
      viewMode:function(){return root.state&&root.state.cameraView===true?'stored-camera':'canvas';},
      // The held mirror check (#2374) rides the same final-surface transform as the color check.
      // #2358: onion skin / Ghost All composed under the live frame by the native renderer; the selection rides every preview request.
      ghosts:function(info){var ui=root.NemoNativeObjectGhostUI;return ui&&typeof ui.selection==='function'?ui.selection(root,info):null;},
      // "Show transparency" (#2373): a view flag the renderer reads, so the checker is engine pixels behind the artwork.
      alphaChecker:function(){return !!(root.state&&root.state.previewAlphaBg===true);},
      // "Current frame outline" (#2372): likewise a view flag the renderer reads; the checkbox only flips state.currentFrameOutline.
      currentOutline:function(){return !!(root.state&&root.state.currentFrameOutline===true);},
      // "Shadow guides" off (#2373): the flag is the inverse of state.showShadowGuides (shown by default, as in the baseline).
      hideShadowGuides:function(){return !!(root.state&&root.state.showShadowGuides===false);},
      displayFilter:function(){var labs=root.SMLabs,color=labs&&typeof labs.getViewFilter==='function'?labs.getViewFilter():'none';
        return labs&&typeof labs.isMirrored==='function'&&labs.isMirrored()?(color==='none'?'mirrored':color+'-mirrored'):color;},
      subscribeWindowOcclusion:root.__TAURI__.event&&typeof root.__TAURI__.event.listen==='function'?async function(hint,cancel){
        var closing=function(){cancel(new Error('Native project window closed'));},remove,detached=false;
        var observesClose=typeof root.addEventListener==='function'&&typeof root.removeEventListener==='function';
        function close(){if(observesClose)root.removeEventListener('pagehide',closing);detach();}
        function detach(){if(!detached){detached=true;if(typeof remove==='function')remove();}}
        if(observesClose)root.addEventListener('pagehide',closing,{once:true});
        try{remove=await root.__TAURI__.event.listen('nemo-native-window-occlusion',function(event){hint(event&&event.payload);});
          if(typeof remove!=='function')throw new Error('Native window exposure unsubscribe is unavailable');}
        catch(error){if(observesClose)root.removeEventListener('pagehide',closing);throw error;}
        return Object.freeze({detach:detach,close:close});
      }:undefined,
      subscribeRevisions:async function(synchronize,disconnected,trackedInvoke,isCurrent){
        if(!root.NemoApplicationMcpTransport||!root.__TAURI__.event||typeof root.__TAURI__.event.listen!=='function')
          return null; // Private file Open remains usable; no toggle/subscriber is advertised.
        var call=trackedInvoke||invokeProjectObserved;
        var ownTransport=transport=root.NemoApplicationMcpTransport.createNativeTauriTransport(function(command,args){
          var response=call(command,args);
          if(command==='nemo_native_revision_sync'&&args&&args.request.action==='acknowledge')
            return Promise.resolve(response).then(function(value){
              settingsDiagnostic(root,'acknowledged',args.request.event);
              // Observe the completed ACK after transport clears its in-flight flag.
              setTimeout(function(){try{if(root.NemoNativeObjectUI&&root.NemoNativeObjectUI.refreshTiming)
                root.NemoNativeObjectUI.refreshTiming(root);if(root.NemoNativeObjectUI&&root.NemoNativeObjectUI.refreshPolicy)root.NemoNativeObjectUI.refreshPolicy(root);settingsNotice('presented');}catch(_){}},0);
              return value;
            });
          return response;
        });
        await ownTransport.connect();
        if(transport!==ownTransport||isCurrent&&!isCurrent()){await ownTransport.disconnect();throw new Error('Native project subscription was cancelled');}
        var result=await ownTransport.subscribeRevisions(root.__TAURI__.event.listen.bind(root.__TAURI__.event),synchronize,disconnected);
        if(transport!==ownTransport||isCurrent&&!isCurrent()){await ownTransport.disconnect();throw new Error('Native project subscription was cancelled');}
        return result;
      },
      unsubscribeRevisions:function(){return transport?transport.disconnect():Promise.resolve();},
      subscriptionActive:function(){return !!transport&&!!transport.status()&&!transport.isSynchronizing();},
      dispatchMutation:function(command,request){return transport.dispatchObject(command,request);},
      policyAvailable:function(){try{return !!transport&&!!root.NemoNativeApplicationAdapter&&typeof root.NemoNativeApplicationAdapter.createNativeApplicationAdapter==='function'&&typeof root.NemoNativeApplicationAdapter.createNativeApplicationAdapter('ui',transport).editLayerPolicy==='function';}catch(_){return false;}},
      dispatchApplication:function(request){
        if(!transport||!root.NemoNativeApplicationAdapter||typeof root.NemoNativeApplicationAdapter.createNativeApplicationAdapter!=='function')
          throw new Error('Native public application client is unavailable');
        var client=root.NemoNativeApplicationAdapter.createNativeApplicationAdapter('ui',transport);
        if(request.operation==='command.document.object.project.settings.edit')
          settingsDiagnostic(root,'dispatch',Object.assign({},request,{kind:request.payload.action.kind}));
        if(request.operation==='command.document.object.layer.policy.edit'){if(typeof client.editLayerPolicy!=='function')throw new Error('Native public layer policy SDK is unavailable');return client.editLayerPolicy(request);}
        var response=client.dispatch(request),gate=root.NemoNativeSettingsValidationGate;
        return gate&&request.operation==='command.document.object.project.settings.edit'?
          gate.hold(root,'settings-response',request,response):response;
      },
      openDiagnostic:function(event,code){openDiagnostic(root,event,code);},
      presented:function(info){
        var version=++presentationVersion,stamp;
        try{stamp=presentationStamp(info);}catch(_){return;}
        if(!stamp)return;
        // Let the completed resource's promise continuations publish their caches
        // before another controller enters the same native queue.
        setTimeout(function(){try{
          if(version!==presentationVersion)return;
          var current=presentationStamp(info);
          if(!current||current.session!==stamp.session||current.info!==stamp.info||current.ticket!==stamp.ticket||
            current.document!==stamp.document||current.presentation!==stamp.presentation)return;
          var shown=current.presentation,canvas=root.document.getElementById('drawing-canvas'),pill=root.document.getElementById('zoom-scrub'),rect=canvas&&canvas.getBoundingClientRect();
          if(shown&&shown.view&&rect&&pill&&root.document.activeElement!==pill){var m=shown.view.documentToComposition,z=Math.hypot(m[0]*rect.width/info.canvasW,m[1]*rect.height/info.canvasH);if(Number.isFinite(z))pill.value=Math.round(z*100);}
          if(shown&&root.state&&root.state.playing!==true)settingsDiagnostic(root,'preview-presented',shown);
          settingsNotice('presented',info);
          renderCameraViewLock(root);
          // A timing read refused while this queue was busy needs the settled
          // presentation signal; its existing readiness pin deduplicates reads.
          if(shown&&root.state&&shown.frame===root.state.currentFrame&&
            root.NemoNativeObjectUI&&typeof root.NemoNativeObjectUI.refreshTiming==='function')
            root.NemoNativeObjectUI.refreshTiming(root);
          if(root.NemoNativeObjectUI&&root.NemoNativeObjectUI.refreshPolicy)root.NemoNativeObjectUI.refreshPolicy(root);
          if(root.SMImageMeshUI&&typeof root.SMImageMeshUI.refresh==='function')root.SMImageMeshUI.refresh();
          // A frame paint can precede native completion and leave Motion unavailable.
          // Refresh its existing inspector only after the current frame is presented.
          if(shown&&root.state&&root.state.appMode==='motion'&&shown.frame===root.state.currentFrame&&
            root.NemoNativeObjectUI&&typeof root.NemoNativeObjectUI.paintMotionInspector==='function')
            root.NemoNativeObjectUI.paintMotionInspector(root);
          if(shown)openDiagnostic(root,'camera-presented',cameraPresentedCode(shown));
        }catch(_){}},0);
      },
      changed:function(info,lease){if(root.NemoNativeObjectProjectFile&&root.NemoNativeObjectProjectFile.hasProjection(root))return publishRevision(info,lease);
        else{if(root.SMReference)root.SMReference.refreshNative();if(root.SMProject)root.SMProject.refreshActiveTabDirtyDot();}},
      notice:function(message){if(root.showToast)root.showToast(message);},
      failed:function(){if(root.showToast)root.showToast('Native project state is unconfirmed. Reopen Nemo before continuing.');}});
  }
  return Object.freeze({createPorts:createPorts,attemptObserver:attemptObserver,didPaintUI:didPaintUI,openDiagnostic:openDiagnostic,settingsDiagnostic:settingsDiagnostic,bindUIComposition:bindUIComposition,composeUI:composeUI,withActionPresentation:withActionPresentation,bindCameraUI:bindCameraUI,renderCameraViewLock:renderCameraViewLock});
}));
