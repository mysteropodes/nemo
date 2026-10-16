/* Updater platform ports. UI retains prompts, progress, scheduling and update handles. */
(function(root,factory){
  var api=factory();
  if(typeof module==='object'&&module.exports)module.exports=api;
  else root.NemoUpdaterPlatform=api;
}(typeof globalThis==='object'?globalThis:this,function(){
  'use strict';
  function create(ports){
    return Object.freeze({
      available:function(){return ports.available();},
      hasVersion:function(){return ports.hasVersion();},
      version:function(){return ports.version();},
      check:function(){return ports.check();},
      install:function(update,onProgress){return ports.install(update,onProgress);},
      confirm:function(message,options){return ports.confirm(message,options);},
      canRelaunch:function(){return ports.canRelaunch();},
      relaunch:function(){return ports.relaunch();},
      canListen:function(){return ports.canListen();},
      listen:function(event,handler){return ports.listen(event,handler);}
    });
  }
  function desktop(root){
    return create({
      available:function(){return typeof root.__TAURI__!=='undefined'&&root.__TAURI__.updater;},
      hasVersion:function(){return typeof root.__TAURI__!=='undefined'&&root.__TAURI__.app;},
      version:function(){return root.__TAURI__.app.getVersion();},
      check:function(){return root.__TAURI__.updater.check();},
      install:function(update,onProgress){return update.downloadAndInstall(onProgress);},
      confirm:function(message,options){return root.__TAURI__.dialog.confirm(message,options);},
      canRelaunch:function(){return !!root.__TAURI__.process;},
      relaunch:function(){return root.__TAURI__.process.relaunch();},
      canListen:function(){return !!root.__TAURI__.event;},
      listen:function(event,handler){return root.__TAURI__.event.listen(event,handler);}
    });
  }
  return Object.freeze({create:create,desktop:desktop});
}));
