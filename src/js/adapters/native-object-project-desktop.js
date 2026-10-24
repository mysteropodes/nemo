/* Native desktop integration ports only; Project owns session and lifecycle state. */
(function(root,factory){
  var api=factory();
  if(typeof module==='object'&&module.exports)module.exports=api;
  else root.NemoNativeObjectProjectDesktop=api;
}(typeof globalThis==='object'?globalThis:this,function(){
  'use strict';
  var openLogs=new WeakMap(),uiCompositions=new WeakMap();
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
  var previewDetails=['frame-range','mapping','viewport','receipt-access','owner','receipt','status','work-id','view-generation','view',
    'status-stale-discarded','status-deferred-timeout','status-deferred-occluded','status-failed-validation','status-failed-device-lost',
    'status-failed-retry-lost','status-failed-retry-outdated','status-failed-recreate','status-failed-reconfigure'];
  function safeCode(event,code){
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
      'bootstrap-dispatched','bootstrap-resolved','bootstrap-rejected','post-bootstrap-failed','bpm-settings-refused',
      'guide-entry-refused','guide-release-refused','guide-edit-refused','navigation-begin','navigation-update','camera-control','camera-presented'].includes(event))return;
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
      ['bpm-settings-refused','guide-entry-refused','guide-release-refused','guide-edit-refused','navigation-begin','navigation-update','camera-control','camera-presented'].includes(event)?safeCode(event,code):code);}catch(_){}
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
    var invoke=root.__TAURI__.core.invoke.bind(root.__TAURI__.core),transport=null,presentationVersion=0;
    function settingsNotice(kind,info){[root.NemoNativeProjectSettingsController,root.NemoNativeCompositionController,root.NemoNativePaletteController,root.NemoNativeGuideController,root.NemoNativeShadowController,root.NemoNativePerspectiveController,root.NemoNativeWorkspaceController,root.NemoNativeCustomBrushController,root.NemoNativeCommentsController].forEach(function(c){if(c&&typeof c[kind]==='function')c[kind](info);});}
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
    return Object.assign({},root.NemoNativeViewportUnderlay.desktop(root),{invoke:invoke,publishRevision:publishRevision,
      viewMode:function(){return root.state&&root.state.cameraView===true?'stored-camera':'canvas';},
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
        var call=trackedInvoke||invoke;
        var ownTransport=transport=root.NemoApplicationMcpTransport.createNativeTauriTransport(function(command,args){
          var response=call(command,args);
          if(command==='nemo_native_revision_sync'&&args&&args.request.action==='acknowledge')
            return Promise.resolve(response).then(function(value){
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
        if(request.operation==='command.document.object.layer.policy.edit'){if(typeof client.editLayerPolicy!=='function')throw new Error('Native public layer policy SDK is unavailable');return client.editLayerPolicy(request);}
        return client.dispatch(request);
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
          settingsNotice('presented',info);
          renderCameraViewLock(root);
          if(root.NemoNativeObjectUI&&root.NemoNativeObjectUI.refreshPolicy)root.NemoNativeObjectUI.refreshPolicy(root);
          // A frame paint can precede native completion and leave Motion unavailable.
          // Refresh its existing inspector only after the current frame is presented.
          if(shown&&root.state&&root.state.appMode==='motion'&&shown.frame===root.state.currentFrame&&
            root.NemoNativeObjectUI&&typeof root.NemoNativeObjectUI.paintMotionInspector==='function')
            root.NemoNativeObjectUI.paintMotionInspector(root);
          if(shown)openDiagnostic(root,'camera-presented',cameraPresentedCode(shown));
        }catch(_){}},0);
      },
      changed:function(info){if(root.NemoNativeObjectProjectFile&&root.NemoNativeObjectProjectFile.hasProjection(root))publishRevision(info);
        else{if(root.SMReference)root.SMReference.refreshNative();if(root.SMProject)root.SMProject.refreshActiveTabDirtyDot();}},
      notice:function(message){if(root.showToast)root.showToast(message);},
      failed:function(){if(root.showToast)root.showToast('Native project state is unconfirmed. Reopen Nemo before continuing.');}});
  }
  return Object.freeze({createPorts:createPorts,didPaintUI:didPaintUI,openDiagnostic:openDiagnostic,bindUIComposition:bindUIComposition,composeUI:composeUI,withActionPresentation:withActionPresentation,bindCameraUI:bindCameraUI,renderCameraViewLock:renderCameraViewLock});
}));
