/* Reference UI commands join the project owner's serialized native history. */
(function(root,factory){
  var api=factory();
  if(typeof module==='object'&&module.exports)module.exports=api;
  else root.NemoNativeObjectReference=api;
}(typeof globalThis==='object'?globalThis:this,function(){
  'use strict';
  function create(session,ports){
    async function importPath(){
      var path=await ports.pick();if(path==null)return false;
      if(typeof path!=='string'||!path.toLowerCase().endsWith('.png'))throw new Error('This native reference workflow currently accepts one PNG image');
      // Replacement preserves the stable reference identity and settings.
      return session.reference(function(current){return {kind:'attach',id:current?current.id:session.requestId('ref'),sourcePath:path};});
    }
    function settings(patch){
      if(!patch||Object.keys(patch).some(function(k){return !['visible','opacity','offsetFrames'].includes(k);}))throw new Error('Invalid reference control');
      // Read at execution, so queued slider/toggle events cannot overwrite a
      // preceding change with values captured before its native commit.
      return session.reference(function(current){
        if(!current)throw new Error('No reference is attached');
        return Object.assign({kind:'settings',name:current.name,visible:current.visible,opacity:current.opacity,offsetFrames:current.offsetFrames},patch);
      });
    }
    return Object.freeze({importPath:importPath,settings:settings,remove:function(){return session.reference({kind:'remove'});},
      read:function(){var info=session.info();return info?info.reference:null;}});
  }
  var bindings=new WeakMap();
  function desktop(root){
    var module=root.NemoNativeObjectProject,session=module&&module.desktop(root);
    if(!session||!session.blocksLegacy())return null;
    if(!bindings.has(root))bindings.set(root,create(session,{pick:function(){return root.__TAURI__.dialog.open({title:'Import Reference Image',multiple:false,filters:[{name:'PNG image',extensions:['png']}]});}}));
    return bindings.get(root);
  }
  return Object.freeze({create:create,desktop:desktop});
}));
