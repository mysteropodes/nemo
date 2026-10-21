/* Ordinary composition controls read a pinned projection and submit native intents. */
(function(root,factory){var common=typeof module==='object'&&module.exports;
  var api=factory(common?require('./native-composition-desktop.js'):root.NemoNativeCompositionDesktop);
  if(common)module.exports=api;else {root.NemoNativeCompositionUI=api;api.install(root);}
}(typeof globalThis==='object'?globalThis:this,function(Desktop){
  'use strict';var installed=new WeakMap();
  function install(root){if(installed.has(root))return installed.get(root);
    var retired=false,loading=null,pending=false,viewEpoch=0,readIntent=null;
    var pinKeys=['instanceId','documentId','lifecycleGeneration','contentRevision','documentSnapshotId'];
    function current(){return root.NemoNativeObjectProject&&root.NemoNativeObjectProject.desktop(root);}
    function ready(session,info){return !retired&&session===current()&&!!root.NemoNativeObjectProjectFile&&
      root.NemoNativeObjectProjectFile.projectionReadyForUI(root,info)===true;}
    function repaint(){if(root.SMMarkers)root.SMMarkers.render();if(root.SMBpm)root.SMBpm.refresh();}
    function notice(){if(typeof root.showToast==='function')root.showToast('Composition changes are not confirmed. Wait for synchronization before trying again.');}
    var model=Desktop.create({currentSession:current,available:ready,publish:function(){return true;}});
    function finishRead(value){
      var intent=readIntent;readIntent=null;
      if(intent)intent.resolve(value);
    }
    function cancelRead(){viewEpoch++;finishRead(null);}
    function readOwner(){
      var session=current(),info=session&&session.info();
      if(retired||pending||!session||!session.isActive()||!info||
        typeof session.readSubscriptionGeneration!=='function'||!ready(session,info))return null;
      var pin={};pinKeys.forEach(function(key){pin[key]=info[key];});
      return {session:session,info:info,pin:pin,ticket:session.readSubscriptionGeneration(),epoch:viewEpoch};
    }
    function sameReadOwner(stamp){
      var session=current(),info=session&&session.info();
      return !retired&&!pending&&stamp.epoch===viewEpoch&&session===stamp.session&&session.isActive()&&
        !!info&&pinKeys.every(function(key){return info[key]===stamp.pin[key];})&&
        session.readSubscriptionGeneration()===stamp.ticket&&ready(session,info);
    }
    function settleRead(completed){
      if(!readIntent)return;
      try{
        if(!sameReadOwner(readIntent.stamp)){finishRead(null);return;}
        var value=model.read();
        if(value)finishRead(value);
        else if(completed&&readIntent.stamp.session.readPresentedObjectSession())finishRead(null);
      }catch(_){finishRead(null);}
    }
    function readWhenPresented(){
      var stamp;
      try{stamp=readOwner();}catch(_){stamp=null;}
      if(!stamp){finishRead(null);return Promise.resolve(null);}
      if(readIntent){
        var existing=readIntent.promise;
        try{if(sameReadOwner(readIntent.stamp)){settleRead(false);return existing;}}catch(_){}
        finishRead(null);
      }
      var value=model.read();if(value)return Promise.resolve(value);
      var intent={stamp:stamp,promise:null,resolve:null};
      intent.promise=new Promise(function(resolve){intent.resolve=resolve;});readIntent=intent;
      // Register first, then use the existing path if settlement already happened.
      // A fenced public presentation waits for the ordinary completion fanout.
      try{if(stamp.session.readPresentedObjectSession())presented();}catch(_){finishRead(null);}
      return intent.promise;
    }
    // Repaint only after the complete pinned read becomes visible in the model cache.
    function presented(){
      settleRead(false);
      if(retired||pending||loading||model.read())return loading||Promise.resolve(model.read());
      loading=model.refresh().then(function(value){repaint();return value;}).catch(function(){return null;})
        .finally(function(){loading=null;settleRead(true);});return loading;
    }
    async function edit(action,expected){
      cancelRead();
      var shown=model.read();
      if(retired||pending||loading||!expected||!shown||shown!==expected){notice();return {status:'unavailable'};}
      pending=true;
      try {var result=await model.edit(action,expected.info);repaint();
        if(!['applied-presented','unchanged'].includes(result.status))notice();return result;
      }catch(error){notice();return {status:'unconfirmed',code:error&&error.code||'unavailable'};}
      finally{pending=false;}
    }
    var controller=Object.freeze({read:function(onRefusal){return model.read(onRefusal);},readWhenPresented:readWhenPresented,edit:edit,presented:presented,
      willProject:function(){cancelRead();if(!pending)model.invalidate();},isPending:function(){return pending||!!loading;},
      dispose:function(){retired=true;cancelRead();model.dispose();repaint();}});
    installed.set(root,controller);root.NemoNativeCompositionController=controller;
    if(typeof root.addEventListener==='function')root.addEventListener('pagehide',controller.dispose,{once:true});
    return controller;
  }
  // The synchronous scripting facade reads the same projection as ordinary UI.
  // Its browser functions remain the originals; native writes require the async API.
  function scripting(root,api,fail) {
    function native(){return !!(root.__TAURI__&&root.__TAURI__.core);}
    function composition(){
      var controller=root.NemoNativeCompositionController;
      var projection=controller&&controller.read();
      if(!projection)fail('Native composition is unavailable');
      return projection.composition;
    }
    function refuse(field){
      if(native())fail('Native '+field+' edits require the composition application API');
    }
    var bpm=Object.getOwnPropertyDescriptor(api.comp,'bpm');
    var offset=Object.getOwnPropertyDescriptor(api.comp,'bpmOffset');
    Object.defineProperties(api.comp,{
      bpm:{enumerable:bpm.enumerable,configurable:bpm.configurable,
        get:function(){
          if(!native())return bpm.get.call(api.comp);
          var current=composition();
          return current.effectiveBpmShow?current.effectiveBpm:0;
        },
        set:function(value){refuse('BPM');bpm.set.call(api.comp,value);}},
      bpmOffset:{enumerable:offset.enumerable,configurable:offset.configurable,
        get:function(){return native()?composition().effectiveBpmOffset:offset.get.call(api.comp);},
        set:function(value){refuse('BPM');offset.set.call(api.comp,value);}}
    });
    var markers=api.markers;
    api.markers=Object.assign({},markers,{
      list:function(){
        if(!native())return markers.list.call(markers);
        return composition().markers.map(function(marker){
          return {frame:marker.frame,name:marker.name||'',color:marker.color||null};
        });
      },
      add:function(){refuse('marker');return markers.add.apply(markers,arguments);},
      removeAt:function(){refuse('marker');return markers.removeAt.apply(markers,arguments);}
    });
    return api;
  }
  return Object.freeze({install:install,scripting:scripting});
}));
