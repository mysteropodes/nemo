/* Desktop settings intent and presentation effects; native records stay authoritative. */
(function(root,factory){var api=factory();if(typeof module==='object'&&module.exports)module.exports=api;
  else {root.NemoNativeProjectSettingsDesktop=api;api.install(root);}}(typeof globalThis==='object'?globalThis:this,function(){
  'use strict';var installed=new WeakMap(),keys=['instanceId','documentId','lifecycleGeneration','contentRevision','documentSnapshotId'];
  function same(a,b){return !!a&&!!b&&keys.every(function(k){return a[k]===b[k];});}
  function equal(a,b){return Array.isArray(a)&&Array.isArray(b)&&a.length===b.length&&a.every(function(x,i){return x===b[i];});}
  function install(root){if(installed.has(root))return installed.get(root);
    var resume=null,refresh=null,retired=false,preparing=null;
    function paintPlayback(){try{var button=root.document&&root.document.getElementById('btn-play');if(!button)return;
      var playing=root.state.playing===true||!!resume;button.innerHTML='<span class="material-symbols-rounded">'+(playing?'\u{e034}':'\u{e037}')+'</span>';
      button.classList[playing?'add':'remove']('playing');}catch(_){} }
    function setResume(value){resume=value;paintPlayback();}
    function cancelPlaybackResume(){if(!resume)return false;resume.cancelled=true;setResume(null);return true;}
    function current(){return root.NemoNativeObjectProject&&root.NemoNativeObjectProject.desktop(root);}
    function observe(event,value){try{var session=current(),info=session&&session.info(),api=root.NemoNativeObjectProjectDesktop;
      if(api&&api.settingsDiagnostic)api.settingsDiagnostic(root,event,Object.assign({},info,value));}catch(_){} }
    function selection(){var state=root.state,indices=root._layerSel;if(!state||!Array.isArray(state.layers)||!Array.isArray(indices))return null;
      var uids=indices.map(function(i){return Number.isSafeInteger(i)&&i>=0&&state.layers[i]&&state.layers[i].layerUid;});
      return uids.every(function(uid){return typeof uid==='string'&&uid;})&&new Set(uids).size===uids.length?uids:null;}
    function context(session,info){return {session:session,info:info,frame:root.state.currentFrame,selection:selection()};}
    function owns(value,session,info){return value&&value.session===session&&['instanceId','documentId','lifecycleGeneration'].every(function(k){return value.info[k]===info[k];})&&
      value.frame===root.state.currentFrame&&equal(value.selection,selection());}
    function notice(message){try{if(typeof root.showToast==='function')root.showToast(message);}catch(_){} }
    function willProject(info){
      if(retired)return;var session=current();if(!session||!session.isActive()||!same(session.info(),info)){setResume(null);refresh=null;return;}
      if(resume&&!owns(resume,session,info))setResume(null);
      if(refresh&&!owns(refresh,session,info))refresh=null;
      if(root.state.fps!==info.fps){
        refresh=context(session,info);
        if(root.state.playing===true){
          var token=typeof root.stopPlay==='function'?root.stopPlay('native-settings-publication'):null;
          setResume(token&&token.wasNative&&typeof token.canResume==='function'?Object.assign(context(session,info),{token:token}):null);
        }
      }
      if(resume)resume.info=info;if(refresh)refresh.info=info;
    }
    function presented(){
      if(retired||!resume&&!refresh)return true;if(preparing||intent.isPending())return false;
      var session=current(),info=session&&session.info(),value=session&&session.readPresentedObjectSession();
      if(!session||!info||!session.isActive()||!value||value.available!==true||!same(info,value)||!same(info,value.presentation)||
        value.presentation.owner!=='native'||value.presentation.status!=='presented'||value.presentation.frame!==root.state.currentFrame)return false;
      if(refresh){var repaint=refresh;refresh=null;
        if(!owns(repaint,session,info)||!same(repaint.info,info)){setResume(null);return false;}
        try{if(root.SMAudio&&typeof root.SMAudio.invalidateWaveforms==='function')root.SMAudio.invalidateWaveforms();}
        catch(_){setResume(null);notice('FPS changed; the audio timeline could not refresh.');return false;}
      }
      if(resume){var restart=resume;setResume(null);
        if(owns(restart,session,info)&&same(restart.info,info)&&root.state.playing!==true&&restart.token.canResume()&&typeof root.startPlay==='function'){observe('resume',{kind:'setFps'});root.startPlay();if(root.state.playing!==true){notice('FPS changed; playback remains paused.');return false;}}
      }
      return true;
    }
    var intent=root.NemoNativeProjectSettingsIntent.create({currentSession:current,
      frame:function(){return root.state&&root.state.currentFrame;},selection:selection,
      available:function(session,info){return !!(session&&session===current()&&session.isActive()&&root.NemoNativeObjectProjectFile&&
        root.NemoNativeObjectProjectFile.projectionReadyForUI(root,info)===true);}});
    async function submit(action){var result=await intent.edit(action);
      if(['applied-presented','unchanged'].includes(result.status)){if(!presented()&&!intent.isPending())result={status:'applied-unconfirmed',applied:result.applied};}
      else if(result.status!=='superseded'){setResume(null);refresh=null;}
      if(!['applied-presented','unchanged','superseded'].includes(result.status))notice(
        result.status==='unavailable'?'Native project settings are unavailable.':'Project settings were not confirmed. Wait for synchronization before continuing.');
      observe('outcome',{kind:action&&action.kind,status:result.status});return result;}
    function unavailable(){return {status:'unavailable'};}
    function prepare(action){
      return new Promise(function(resolve){var kind=action&&action.kind;
        if(!['setCanvasSize','setFps','setCanvasBackground','setWorkArea'].includes(kind)){resolve(unavailable());return;}
        var item={action:Object.freeze(Object.assign({},action)),resolve:resolve},prior=preparing.items.get(kind);
        if(prior)prior.resolve({status:'superseded'});preparing.items.set(kind,item);
      });
    }
    function edit(action){
      observe('intent',{kind:action&&action.kind});
      if(retired)return Promise.resolve(unavailable());if(preparing)return prepare(action);
      if(action&&action.kind==='setFps'&&Number.isFinite(action.fps)&&root.state.playing===true){
        var session=current(),info=session&&session.info();if(!session||!info||typeof root.stopPlay!=='function')return Promise.resolve(unavailable());
        var plan=context(session,info),work={items:new Map()};setResume(plan);preparing=work;var result=prepare(action);
        // Retire the clock and supersede its pending frame in this same turn.
        // Deferring Stop lets that frame publish after plan captured its context.
        var stopped;try{stopped=root.stopPlay('native-settings-intent');}catch(error){stopped=Promise.reject(error);}paintPlayback();
        Promise.resolve(stopped).then(function(receipt){
          if(preparing!==work)return;preparing=null;
          if(retired||!receipt||receipt.presented!==true||!receipt.token||!receipt.token.wasNative||!owns(plan,current(),current().info())||root.state.playing===true){
            setResume(null);refresh=null;work.items.forEach(function(item){item.resolve(unavailable());});return;
          }
          plan.token=receipt.token;setResume(plan.cancelled?null:plan);work.items.forEach(function(item){item.resolve(submit(item.action));});
        }).catch(function(){if(preparing===work)preparing=null;setResume(null);refresh=null;work.items.forEach(function(item){item.resolve(unavailable());});});
        return result;
      }
      return submit(action);
    }
    var controller=Object.freeze({edit:edit,willProject:willProject,presented:presented,cancelPlaybackResume:cancelPlaybackResume,isPending:function(){return !!preparing||intent.isPending();},
      dispose:function(){retired=true;setResume(null);refresh=null;if(preparing){preparing.items.forEach(function(item){item.resolve(unavailable());});preparing=null;}intent.dispose();}});
    installed.set(root,controller);root.NemoNativeProjectSettingsController=controller;return controller;
  }
  return Object.freeze({install:install});
}));
