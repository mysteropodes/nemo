/* Ordinary composition controls read a pinned projection and submit native intents. */
(function(root,factory){var common=typeof module==='object'&&module.exports;
  var api=factory(common?require('./native-composition-desktop.js'):root.NemoNativeCompositionDesktop);
  if(common)module.exports=api;else {root.NemoNativeCompositionUI=api;api.install(root);}
}(typeof globalThis==='object'?globalThis:this,function(Desktop){
  'use strict';var installed=new WeakMap();
  function install(root){if(installed.has(root))return installed.get(root);
    var retired=false,loading=null,pending=false;
    function current(){return root.NemoNativeObjectProject&&root.NemoNativeObjectProject.desktop(root);}
    function ready(session,info){return !retired&&session===current()&&!!root.NemoNativeObjectProjectFile&&
      root.NemoNativeObjectProjectFile.projectionReadyForUI(root,info)===true;}
    function repaint(){if(root.SMMarkers)root.SMMarkers.render();if(root.SMBpm)root.SMBpm.refresh();}
    function notice(){if(typeof root.showToast==='function')root.showToast('Composition changes are not confirmed. Wait for synchronization before trying again.');}
    var model=Desktop.create({currentSession:current,available:ready,publish:function(){return true;}});
    // Repaint only after the complete pinned read becomes visible in the model cache.
    function presented(){
      if(retired||pending||loading||model.read())return loading||Promise.resolve(model.read());
      loading=model.refresh().then(function(value){repaint();return value;}).catch(function(){return null;})
        .finally(function(){loading=null;});return loading;
    }
    async function edit(action,expected){
      var shown=model.read();
      if(retired||pending||loading||!expected||!shown||shown!==expected){notice();return {status:'unavailable'};}
      pending=true;
      try {var result=await model.edit(action,expected.info);repaint();
        if(!['applied-presented','unchanged'].includes(result.status))notice();return result;
      }catch(error){notice();return {status:'unconfirmed',code:error&&error.code||'unavailable'};}
      finally{pending=false;}
    }
    var controller=Object.freeze({read:function(){return model.read();},edit:edit,presented:presented,
      willProject:function(){if(!pending)model.invalidate();},isPending:function(){return pending||!!loading;},
      dispose:function(){retired=true;model.dispose();repaint();}});
    installed.set(root,controller);root.NemoNativeCompositionController=controller;
    if(typeof root.addEventListener==='function')root.addEventListener('pagehide',controller.dispose,{once:true});
    return controller;
  }
  return Object.freeze({install:install});
}));
