/* Desktop composition for existing native Project/File settings intent ports. */
(function(root,factory){var api=factory();if(typeof module==='object'&&module.exports)module.exports=api;
  else {root.NemoNativeProjectSettingsDesktop=api;api.install(root);}}(typeof globalThis==='object'?globalThis:this,function(){
  'use strict';var installed=new WeakMap();
  function install(root){if(installed.has(root))return installed.get(root);
    function current(){return root.NemoNativeObjectProject&&root.NemoNativeObjectProject.desktop(root);}
    var intent=root.NemoNativeProjectSettingsIntent.create({currentSession:current,
      frame:function(){return root.state&&root.state.currentFrame;},
      selection:function(){var state=root.state,indices=root._layerSel;if(!state||!Array.isArray(state.layers)||!Array.isArray(indices))return null;
        var uids=indices.map(function(i){return Number.isSafeInteger(i)&&i>=0&&state.layers[i]&&state.layers[i].layerUid;});
        return uids.every(function(uid){return typeof uid==='string'&&uid;})&&new Set(uids).size===uids.length?uids:null;},
      available:function(session,info){return !!(session&&session===current()&&session.isActive()&&root.NemoNativeObjectProjectFile&&
        root.NemoNativeObjectProjectFile.projectionReadyForUI(root,info)===true);}});
    var controller=Object.freeze({edit:async function(action){var result=await intent.edit(action);
      if(!['applied-presented','unchanged','superseded'].includes(result.status))try{if(typeof root.showToast==='function')root.showToast(
        result.status==='unavailable'?'Native project settings are unavailable.':'Project settings were not confirmed. Wait for synchronization before continuing.');}catch(_){}
      return result;},isPending:intent.isPending,dispose:intent.dispose});
    installed.set(root,controller);root.NemoNativeProjectSettingsController=controller;return controller;
  }
  return Object.freeze({install:install});
}));
