/* Cmd/Ctrl+S and Shift+Cmd/Ctrl+S run the project's own Save and Save As, from the page key or the File menu item. */
(function(root,factory){
  var api=factory();
  if(typeof module==='object'&&module.exports)module.exports=api;
  else{root.NemoProjectSaveShortcut=api;if(root.document)api.install(root);}
}(typeof globalThis==='object'?globalThis:this,function(){
  'use strict';
  var SAVE_EVENT='menu-save-project',SAVE_AS_EVENT='menu-save-project-as',DEDUPE_MS=400;
  function isSaveKey(e){
    return !!e&&(e.metaKey||e.ctrlKey)&&!e.altKey&&(typeof e.key==='string'&&e.key.toLowerCase()==='s'||e.code==='KeyS');
  }
  function install(root){
    var last=-Infinity,lastKind=null;
    // The menu accelerator and the page keydown can both report one press; honour the first only.
    function run(kind){
      var now=Date.now();if(kind===lastKind&&now-last<DEDUPE_MS)return false;last=now;lastKind=kind;
      var project=root.SMProject;if(!project||typeof project[kind]!=='function')return false;
      try{var result=project[kind]();if(result&&typeof result.catch==='function')result.catch(function(){});}catch(_){}
      return true;
    }
    // Capture phase: the arbiter's own Save case never also runs, and no field or tool can swallow the chord.
    root.document.addEventListener('keydown',function(e){
      if(!isSaveKey(e))return;e.preventDefault();e.stopImmediatePropagation();run(e.shiftKey?'saveAs':'save');
    },true);
    var tauri=root.__TAURI__;
    if(tauri&&tauri.event&&typeof tauri.event.listen==='function'){
      tauri.event.listen(SAVE_EVENT,function(){run('save');});
      tauri.event.listen(SAVE_AS_EVENT,function(){run('saveAs');});
    }
    return Object.freeze({run:run});
  }
  return Object.freeze({install:install,isSaveKey:isSaveKey,SAVE_EVENT:SAVE_EVENT,SAVE_AS_EVENT:SAVE_AS_EVENT});
}));
