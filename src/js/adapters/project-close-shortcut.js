/* Cmd/Ctrl+W closes the active project tab through the tab bar's own close path; it never closes the window. */
(function(root,factory){
  var api=factory();
  if(typeof module==='object'&&module.exports)module.exports=api;
  else{root.NemoProjectCloseShortcut=api;if(root.document)api.install(root);}
}(typeof globalThis==='object'?globalThis:this,function(){
  'use strict';
  var MENU_EVENT='menu-close-project',DEDUPE_MS=400;
  function isCloseKey(e){
    return !!e&&(e.metaKey||e.ctrlKey)&&!e.altKey&&!e.shiftKey&&typeof e.key==='string'&&e.key.toLowerCase()==='w';
  }
  function install(root){
    var last=-Infinity;
    // The native menu accelerator and the page keydown can both report one press; honour the first only.
    function close(){
      var now=Date.now();if(now-last<DEDUPE_MS)return false;last=now;
      var project=root.SMProject;if(!project||typeof project.closeActiveTab!=='function')return false;
      try{var result=project.closeActiveTab();if(result&&typeof result.catch==='function')result.catch(function(){});}catch(_){}
      return true;
    }
    root.document.addEventListener('keydown',function(e){
      if(!isCloseKey(e))return;e.preventDefault();close();
    },true);
    var tauri=root.__TAURI__;
    if(tauri&&tauri.event&&typeof tauri.event.listen==='function')tauri.event.listen(MENU_EVENT,close);
    return Object.freeze({close:close});
  }
  return Object.freeze({install:install,isCloseKey:isCloseKey,MENU_EVENT:MENU_EVENT});
}));
