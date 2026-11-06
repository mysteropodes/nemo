/* Project length: the Frames field and Trim to Work Area. UI guards, toasts and the playhead follow-up around the
   native settings controller (timeline.js setTotalFrames / trimToWorkArea at 1065e9d8); the document stays native. */
(function(root,factory){var api=factory();if(typeof module==='object'&&module.exports)module.exports=api;
  else root.NemoNativeProjectLength=api;}(typeof globalThis==='object'?globalThis:this,function(){
  'use strict';
  function unavailable(){return Promise.resolve({status:'unavailable'});}
  function controller(root){var c=root.NemoNativeProjectSettingsController;return c&&typeof c.edit==='function'?c:null;}
  function say(root,key,suffix){try{if(typeof root.showToast!=='function')return;
    var text=root.SM&&typeof root.SM.t==='function'?root.SM.t(key):key;root.showToast(suffix===undefined?text:text+suffix);}catch(_){}}
  // The inputs show what the document holds: a clamped or refused entry reverts to the projected length.
  function syncFields(root){try{['proj-frames','tl-total'].forEach(function(id){
    var el=root.document&&root.document.getElementById(id);if(el&&root.state&&Number.isFinite(root.state.totalFrames))el.value=root.state.totalFrames;});}catch(_){}}
  function setTotalFrames(root,value){
    var c=controller(root),frames=Math.max(1,Math.min(999,Math.floor(Number(value))));
    if(!c||!Number.isFinite(frames))return unavailable();
    return c.edit({kind:'setTotalFrames',frames:frames}).then(function(result){syncFields(root);return result;});
  }
  function trimToWorkArea(root){
    var s=root.state;if(!s)return unavailable();
    if(s.activeSymbolId){say(root,'toastCloseComponentFirst');return unavailable();}
    if(s.activeMontageViewId){say(root,'toastCloseMontageFirst');return unavailable();}
    var begin=Math.max(0,Math.floor(s.waIn||0)),end=Math.min(s.totalFrames-1,Math.floor(s.waOut!=null?s.waOut:s.totalFrames-1));
    if(end<=begin){say(root,'hsWorkAreaTooShort');return unavailable();}
    if(begin===0&&end===s.totalFrames-1){say(root,'toastWorkAreaAlreadyCoversAll');return unavailable();}
    var c=controller(root);if(!c)return unavailable();
    var length=end-begin+1,frame=s.currentFrame;
    return c.edit({kind:'trimToWorkArea'}).then(function(result){
      if(!result||result.status!=='applied-presented'){syncFields(root);return result;}
      // The projection already clamped the playhead into the shorter timeline; the baseline re-bases it on the new start.
      var target=Math.max(0,Math.min(length-1,frame-begin)),go=Promise.resolve();
      if(s.currentFrame!==target&&typeof root.goToFrame==='function'){go=Promise.resolve().then(function(){return root.goToFrame(target);}).catch(function(){});}
      return go.then(function(){say(root,'toastCompReducedToWorkArea',length+' frames)');syncFields(root);return result;});
    });
  }
  return Object.freeze({setTotalFrames:setTotalFrames,trimToWorkArea:trimToWorkArea});
}));
