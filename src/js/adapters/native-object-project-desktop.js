/* Native desktop integration ports only; Project owns session and lifecycle state. */
(function(root,factory){
  var api=factory();
  if(typeof module==='object'&&module.exports)module.exports=api;
  else root.NemoNativeObjectProjectDesktop=api;
}(typeof globalThis==='object'?globalThis:this,function(){
  'use strict';
  function createPorts(root){
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
    return Object.assign({},root.NemoNativeViewportUnderlay.desktop(root),{invoke:invoke,publishRevision:publishRevision,
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
                root.NemoNativeObjectUI.refreshTiming(root);if(root.NemoNativeObjectUI&&root.NemoNativeObjectUI.refreshPolicy)root.NemoNativeObjectUI.refreshPolicy(root);settingsNotice('presented');}catch(_){}},0);
              return value;
            });
          return response;
        });
        await transport.connect();return transport.subscribeRevisions(root.__TAURI__.event.listen.bind(root.__TAURI__.event),synchronize,disconnected);
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
      openDiagnostic:typeof root.__NEMO_P38_FIXTURE_OPEN_DIAGNOSTIC__==='function'?function(event,code){try{root.__NEMO_P38_FIXTURE_OPEN_DIAGNOSTIC__(event,code);}catch(_){} }:null,
      presented:function(info){
        settingsNotice('presented',info);
        if(root.NemoNativeObjectUI&&root.NemoNativeObjectUI.refreshPolicy)root.NemoNativeObjectUI.refreshPolicy(root);
      },
      changed:function(info){if(root.NemoNativeObjectProjectFile&&root.NemoNativeObjectProjectFile.hasProjection(root))publishRevision(info);
        else{if(root.SMReference)root.SMReference.refreshNative();if(root.SMProject)root.SMProject.refreshActiveTabDirtyDot();}},
      notice:function(message){if(root.showToast)root.showToast(message);},
      failed:function(){if(root.showToast)root.showToast('Native project state is unconfirmed. Reopen Nemo before continuing.');}});
  }
  return Object.freeze({createPorts:createPorts});
}));
