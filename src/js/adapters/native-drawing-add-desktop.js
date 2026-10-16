/* Desktop composition only: UI intent uses the existing native application owner. */
(function(root,factory){
  var api=factory();
  if(typeof module==='object'&&module.exports)module.exports=api;
  else {root.NemoNativeDrawingAddDesktop=api;api.install(root);}
}(typeof globalThis==='object'?globalThis:this,function(){
  'use strict';
  var installed=new WeakMap();
  function install(root){
    if(installed.has(root))return installed.get(root);
    function current(){
      return root.NemoNativeObjectProject&&root.NemoNativeObjectProject.desktop(root);
    }
    function selection(){
      var state=root.state,indices=root._layerSel;
      if(!state||!Array.isArray(state.layers)||!Array.isArray(indices))return null;
      var uids=indices.map(function(index){return Number.isSafeInteger(index)&&index>=0&&
        state.layers[index]&&state.layers[index].layerUid;});
      return uids.every(function(uid){return typeof uid==='string'&&uid;})&&
        new Set(uids).size===uids.length?uids:null;
    }
    var intent=root.NemoNativeDrawingAdd.create({
      currentSession:current,
      frame:function(){return root.state&&root.state.currentFrame;},
      selection:selection,
      available:function(session,info){return !!(session&&session===current()&&session.isActive()&&
        root.NemoNativeObjectProjectFile&&root.NemoNativeObjectProjectFile.projectionReadyForUI(root,info)===true);},
      select:function(uid,info,session){
        if(current()!==session)return false;
        var index=info.layers.findIndex(function(row){return row.layerUid===uid;});
        return index>=0&&root.NemoNativeObjectProject.selectLayerUI(root,index,false,uid,info)===true;
      }
    });
    function notice(message){try{if(typeof root.showToast==='function')root.showToast(message);}catch(_){} }
    var controller=Object.freeze({
      add:async function(){var result=await intent.add();
        if(result.status==='unavailable')notice('Native Drawing Add is unavailable.');
        else if(result.status==='unconfirmed')notice('Drawing creation was not confirmed. Do not retry until the document is synchronized.');
        else if(result.status==='created-unselected')notice('Drawing created; the current selection was left unchanged.');
        return result;},
      isPending:intent.isPending,
      dispose:intent.dispose
    });
    installed.set(root,controller);root.NemoNativeDrawingAddController=controller;
    return controller;
  }
  return Object.freeze({install:install});
}));
