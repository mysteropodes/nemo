/* U04 (#2370): the desktop target of every rebindable shortcut (shortcut-registry.js), consulted by runToolShortcut and
 * runCommandShortcut in timeline.js. 'native' reaches a native route, 'presentation' changes only tool or view state (the
 * same as its toolbar button), and 'unavailable' refuses on desktop with its reason instead of a silent legacy refusal.
 * A shortcut without a declared target is unavailable on desktop: a new registry action must be classified here. The
 * read-only keys onKeyDown handles itself are not in this table. Outside the desktop host every shortcut runs as before. */
(function(root,factory){var api=factory();if(typeof module==='object'&&module.exports)module.exports=api;
  else root.NemoShortcutTargets=api;
}(typeof globalThis==='object'?globalThis:this,function(){
  'use strict';
  function native(root){return !!(root.__TAURI__&&root.__TAURI__.core);}
  function off(reason){return {kind:'unavailable',reason:reason};}
  var NATIVE={kind:'native'},UI={kind:'presentation'};
  var TARGETS=Object.freeze({
    // SM.setTool selects the tool (timeline.js); the tool's own gesture routes natively or refuses, as from the toolbar.
    draw:UI,select:UI,subselect:UI,fsselect:UI,comment:UI,pen:UI,line:UI,rect:UI,ellipse:UI,speechbubble:UI,star:UI,eraser:UI,
    fill:UI,fillbrush:UI,eyedropper:UI,hand:UI,zoom:UI,text:UI,rotate:UI,rig:UI,
    // Onion skin is a view preference, never written to the project file (app.js).
    toggleOnion:UI,
    // goToFrame follows the native frame navigator while a native document is open (app.js).
    cmdPrevKey:UI,cmdNextKey:UI,cmdPrevFrame:UI,cmdNextFrame:UI,cmdGoStart:UI,cmdGoEnd:UI,
    // native-frame-edit-desktop.js shortcut() owns these frame edits.
    cmdInsertFrame:NATIVE,cmdInsertKey:NATIVE,cmdInsertBlankKey:NATIVE,cmdDuplicateKey:NATIVE,cmdExtendExposure:NATIVE,
    // generateTweens is a legacy writer; tween generation has no native route yet.
    cmdTween:off('Generating tweens is unavailable in native Nemo.'),
    // flipPreview steps goToFrame; resetView uses the native viewport navigation first.
    cmdFlipPreview:UI,cmdResetView:UI,
    // The rename field commits through the legacy SM.renameLayer, which refuses; no native rename field exists yet.
    cmdRenameLayer:off('Renaming a layer from the keyboard is unavailable in native Nemo.')
  });
  function target(root,action){var t=Object.prototype.hasOwnProperty.call(TARGETS,action)?TARGETS[action]:null;return typeof t==='function'?t(root):t;}
  function notice(root,reason){try{if(root.showToast)root.showToast(reason);}catch(_){}}
  // Whether the shortcut may run here; an unavailable one shows its reason instead.
  function allow(root,action){
    if(!native(root))return true;
    var t=target(root,action)||off('This shortcut is unavailable in native Nemo.');
    if(t.kind!=='unavailable')return true;
    notice(root,t.reason);return false;
  }
  return Object.freeze({targets:TARGETS,target:target,allow:allow});
}));
