/* U04 (#2370): the desktop target of every app-menu action (timeline.js initAppMenu). 'native' reaches a native route,
 * 'presentation' opens UI only (any document change it leads to goes through its own native route), and
 * 'unavailable' refuses on desktop with its reason instead of reaching a legacy writer or the JavaScript scene evaluator.
 * An action without a declared target is unavailable on desktop: a new menu item must be classified here. Outside the
 * desktop host the menu is unchanged. */
(function(root,factory){var api=factory();if(typeof module==='object'&&module.exports)module.exports=api;
  else root.NemoAppMenuTargets=api;
}(typeof globalThis==='object'?globalThis:this,function(){
  'use strict';
  function native(root){return !!(root.__TAURI__&&root.__TAURI__.core);}
  // The same test the native media router uses (native-import-drop.js desktopMedia): a native document is open.
  function nativeDocument(root){
    try{var s=root.NemoNativeObjectProject&&root.NemoNativeObjectProject.desktop(root);return !!(s&&s.blocksLegacy());}catch(_){return false;}
  }
  function off(reason){return {kind:'unavailable',reason:reason};}
  var NATIVE={kind:'native'},UI={kind:'presentation'};
  var NO_DOCUMENT='Open or create a native project first.';
  var TARGETS=Object.freeze({
    // project.js addTab -> start screen -> SMProject.newProject (native object New).
    'ctx-new-project':NATIVE,
    // project.js openDialog/openPath -> native object Open; a non-object file falls to the native opacity entry.
    'ctx-open':NATIVE,
    // project-native-save.js: the native object session saves; otherwise desktopJSON refuses without native persistence.
    'ctx-save':NATIVE,'ctx-save-as':NATIVE,
    // kitsu.js openShot rewrites state.layers after newProject; publish renders through the JavaScript exporter (#2381).
    'ctx-kitsu-open':off('Kitsu shots are unavailable in native Nemo.'),
    'ctx-kitsu-publish':off('Kitsu publishing is unavailable in native Nemo.'),
    // The legacy script API writes state.layers directly; the native script API (#2354) replaces it when loaded.
    'ctx-nemo-script':function(root){return root.NemoScriptNativeApi?NATIVE:off('Scripts are unavailable in native Nemo.');},
    // Plugins receive the legacy script API (nemo-plugin.js bridge) on every host.
    'ctx-nemo-plugin':off('Plugins are unavailable in native Nemo.'),
    // second-viewer.js renders SMEngineBridge.buildSceneJsonForFrame, the JavaScript scene evaluator.
    'ctx-new-view':off('A second viewer is unavailable in native Nemo.'),
    // The history modal; restore is the native version restore (project.js).
    'ctx-history':UI,
    // images.js -> NemoNativeImportDrop.desktopMedia, which is native only while a native document is open.
    'ctx-import-img':function(root){return nativeDocument(root)?NATIVE:off(NO_DOCUMENT);},
    'ctx-import-video':function(root){return nativeDocument(root)?NATIVE:off(NO_DOCUMENT);},
    // native-import-desktop.js: native PSD and SVG importers own these.
    'ctx-import-psd':NATIVE,'ctx-import-svg':NATIVE,
    // Settings, Figma tab: the import button there calls the native Figma importer.
    'ctx-import-figma':UI,
    // The OBJ reference overlay (labs/reference-3d.js), never exported or baked; disabled while frozen in dev.
    'ctx-import-3d':UI,
    // The export modal and the render queue; running a job is their own native route (#2375).
    'ctx-export':UI,'ctx-render-manager':UI,
    'ctx-settings':UI,'ctx-shortcuts':UI,'ctx-about':UI
  });
  function target(root,id){var t=Object.prototype.hasOwnProperty.call(TARGETS,id)?TARGETS[id]:null;return typeof t==='function'?t(root):t;}
  function notice(root,reason){try{if(root.showToast)root.showToast(reason);}catch(_){}}
  // The menu items as the desktop host may run them: an unavailable action shows its reason and calls nothing else.
  function guard(root,items){
    if(!native(root))return items;
    return items.map(function(item){
      if(!item||item.sep||item.disabled||typeof item.action!=='function')return item;
      var t=target(root,item.id)||off('This menu action is unavailable in native Nemo.');
      if(t.kind!=='unavailable')return item;
      return Object.assign({},item,{action:function(){notice(root,t.reason);}});
    });
  }
  return Object.freeze({targets:TARGETS,target:target,guard:guard});
}));
