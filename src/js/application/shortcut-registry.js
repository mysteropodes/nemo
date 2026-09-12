// Keyboard-shortcut binding data and persistence (P30/#1032), extracted from
// timeline.js. Dispatch (runToolShortcut/runCommandShortcut/onKeyDown) and the
// settings-modal DOM rendering (renderShortcutsList) stay in timeline.js as
// callers — this module owns only the tables, the override/persistence state,
// and the pure lookups both dispatch and the modal read through.
//
// COMMAND_SHORTCUTS' own `run` closures still reference timeline.js globals
// (goToFrame, state, window.SM, ...) directly, same as before the move —
// classic-script globals are shared across files, so this is unaffected by
// which file defines the table, only by script tag order relative to when a
// bound key is actually pressed (well after those globals exist).
var NemoShortcutRegistry = (function () {
  'use strict';

  var STORAGE_KEY = 'nemo-shortcuts';

  var TOOL_SHORTCUTS = [
    {action:'draw',key:'b',label:'Draw'},
    {action:'select',key:'v',label:'Select'},
    {action:'subselect',key:'a',label:'Subselect (node edit)'},
    {action:'fsselect',key:'m',label:'Fill/Stroke Select'},
    {action:'comment',key:'c',label:'Comment'},
    {action:'pen',key:'p',label:'Pen'},
    {action:'line',key:'u',label:'Line'},
    {action:'rect',key:'r',label:'Rectangle'},
    {action:'ellipse',key:'l',label:'Ellipse'},
    {action:'speechbubble',key:'d',label:'Bulle de dialogue'},
    {action:'star',key:'q',label:'Étoile / Polygone'},
    {action:'eraser',key:'e',label:'Eraser'},
    {action:'fill',key:'g',label:'Fill'},
    {action:'fillbrush',key:'n',label:'Fill Brush'},
    {action:'eyedropper',key:'i',label:'Eyedropper'},
    {action:'hand',key:'h',label:'Hand (pan)'},
    {action:'zoom',key:'z',label:'Zoom'},
    {action:'toggleOnion',key:'o',label:'Toggle Onion Skin'},
    // UI/UX audit (2026-07): these tools had NO letter shortcut at all —
    // every other tool button does, so their absence read as an
    // inconsistency rather than a deliberate omission. The alphabet is
    // nearly exhausted by the bindings above (only q/s/w/y were free); no
    // mnemonic reads as cleanly as the existing ones (v=select, b=brush,
    // p=pen...) so these are arbitrary placeholders, not a claimed "right"
    // answer — rebindable via the existing Réglages > Raccourcis UI
    // (shortcutOverrides/localStorage) like any other entry here.
    {action:'text',key:'y',label:'Texte'},
    {action:'rotate',key:'w',label:'Rotation du canevas'},
    {action:'rig',key:'s',label:'Rig (Skeleton)'},
    // Deliberately NOT bound to 'q' (or anything): the Perspective rail
    // button was removed on purpose (see the comment above the button
    // markup in index.html) — perspective is reachable ONLY via the Labs
    // floating panel now. A live 'q' binding with no matching rail button
    // used to switch state.tool to 'perspective' silently: every .tool-btn
    // lost its .active class (none has data-tool="perspective" to match),
    // so the whole rail went dark with zero explanation while the cursor
    // quietly became a crosshair — found by the same audit, fixed by
    // deleting the binding rather than re-adding a button the UI review
    // that removed it explicitly didn't want back.
  ];
  // COMMANDES remappables (2026-09) — jusqu'ici seuls les 21 OUTILS étaient
  // reconfigurables dans Réglages ▸ Raccourcis, alors que 63 autres touches
  // étaient câblées en dur dans onKeyDown : un animateur venu de TVPaint ou
  // d'After Effects ne pouvait déplacer aucune commande. Ce tableau est la
  // liste de celles qui passent par la même table d'overrides que les outils
  // (même stockage, même détection de conflit) ; leurs anciennes branches en
  // dur ont été retirées du gestionnaire pour qu'une touche réassignée ne
  // déclenche pas les deux. Le reste des touches est exposé en LECTURE SEULE
  // dans le panneau (READONLY_SHORTCUTS plus bas) plutôt que d'être passé
  // sous silence — le panneau devient la carte complète du clavier.
  var COMMAND_SHORTCUTS = [
    {action:'cmdPrevKey',key:'j',cat:'nav',label:'shortcutCmdPrevKey',run:function(){if(state.playing)stopPlay();goToFrame(prevKeyframeFrame(state.activeLayerIdx,state.currentFrame));}},
    {action:'cmdNextKey',key:'k',cat:'nav',label:'shortcutCmdNextKey',run:function(){if(state.playing)stopPlay();goToFrame(nextKeyframeFrame(state.activeLayerIdx,state.currentFrame));}},
    {action:'cmdPrevFrame',key:',',cat:'nav',label:'shortcutCmdPrevFrame',run:function(){if(state.playing)stopPlay();goToFrame(state.currentFrame-1);}},
    {action:'cmdNextFrame',key:'.',cat:'nav',label:'shortcutCmdNextFrame',run:function(){if(state.playing)stopPlay();goToFrame(state.currentFrame+1);}},
    {action:'cmdGoStart',key:'Home',cat:'nav',label:'shortcutCmdGoStart',run:function(){if(state.playing)stopPlay();goToFrame(0);}},
    {action:'cmdGoEnd',key:'End',cat:'nav',label:'shortcutCmdGoEnd',run:function(){if(state.playing)stopPlay();goToFrame(state.totalFrames-1);}},
    {action:'cmdInsertFrame',key:'F5',cat:'frames',label:'shortcutCmdInsertFrame',run:function(e){e.preventDefault();insertFrame();}},
    {action:'cmdInsertKey',key:'F6',cat:'frames',label:'shortcutCmdInsertKey',run:function(e){e.preventDefault();insertKeyframe();}},
    {action:'cmdInsertBlankKey',key:'F7',cat:'frames',label:'shortcutCmdInsertBlankKey',run:function(e){e.preventDefault();insertBlankKeyframe();}},
    {action:'cmdDuplicateKey',key:'',cat:'frames',label:'shortcutCmdDuplicateKey',run:function(){window.SM.duplicateKeyframe();}},
    {action:'cmdExtendExposure',key:'+',cat:'frames',label:'shortcutCmdExtendExposure',run:function(){window.SM.extendExposure(1);}},
    {action:'cmdTween',key:'t',cat:'frames',label:'shortcutCmdTween',run:function(){window.SM.generateTweens();}},
    {action:'cmdFlipPreview',key:'f',cat:'view',label:'shortcutCmdFlipPreview',run:function(e){if(!e.shiftKey)window.SM.flipPreview();}},
    {action:'cmdResetView',key:'/',cat:'view',label:'shortcutCmdResetView',run:function(e){e.preventDefault();window.SM.resetView();}},
    {action:'cmdRenameLayer',key:'F2',cat:'layers',label:'shortcutCmdRenameLayer',run:function(e){e.preventDefault();if(state.layers[state.activeLayerIdx])startLayerRename(state.activeLayerIdx);}},
  ];
  // Touches câblées ailleurs (playback, presse-papier, modes, gestes) : listées
  // pour que le panneau soit exhaustif, marquées non réassignables.
  var READONLY_SHORTCUTS = [
    {keys:'Espace',label:'shortcutRoPlay',cat:'nav'},
    {keys:'←  →',label:'shortcutRoStepFrame',cat:'nav'},
    {keys:'⇧ Page',label:'shortcutRoStepLayer',cat:'layers'},
    {keys:'⌘Z / ⇧⌘Z',label:'shortcutRoUndo',cat:'edit'},
    {keys:'⌘C / ⌘V / ⌘X',label:'shortcutRoClipboard',cat:'edit'},
    {keys:'⌘D',label:'shortcutRoDuplicate',cat:'edit'},
    {keys:'⌘G / ⇧⌘G',label:'shortcutRoGroup',cat:'edit'},
    {keys:'⌘A',label:'shortcutRoSelectAll',cat:'edit'},
    {keys:'⌫',label:'shortcutRoDelete',cat:'edit'},
    {keys:'B / N',label:'shortcutRoWorkArea',cat:'nav'},
    {keys:'F9',label:'shortcutRoEasyEase',cat:'frames'},
    {keys:'⌥ ← →',label:'shortcutRoNudgeKeys',cat:'frames'},
    {keys:'⇧⌘D',label:'shortcutRoSplitLayer',cat:'layers'},
    {keys:'Échap',label:'shortcutRoEscape',cat:'edit'},
  ];
  var SHORTCUT_CATS = [{id:'tools',label:'shortcutCatTools'},{id:'nav',label:'shortcutCatNav'},{id:'frames',label:'shortcutCatFrames'},{id:'layers',label:'shortcutCatLayers'},{id:'view',label:'shortcutCatView'},{id:'edit',label:'shortcutCatEdit'}];

  function isObject(value) { return !!value && typeof value === 'object' && !Array.isArray(value); }

  // Pure: given only the two rebindable tables, independent of any override
  // state — same behavior as the original shortcutDefFor.
  function defFor(action) {
    return TOOL_SHORTCUTS.find(function (s) { return s.action === action; })
      || COMMAND_SHORTCUTS.find(function (s) { return s.action === action; })
      || null;
  }

  // Pure: overrides passed explicitly rather than read from module state.
  function keyFor(overrides, action) {
    if (overrides && overrides[action]) return overrides[action];
    var d = defFor(action);
    return d ? d.key : null;
  }

  // Pure. Only checks TOOL_SHORTCUTS/COMMAND_SHORTCUTS against each other —
  // READONLY_SHORTCUTS was never included in the original clash set either,
  // preserved as-is rather than widened.
  function clashFor(overrides, action, key) {
    var lk = (key || '').toLowerCase();
    if (!lk) return null;
    var all = TOOL_SHORTCUTS.concat(COMMAND_SHORTCUTS);
    for (var i = 0; i < all.length; i++) {
      if (all[i].action === action) continue;
      if ((keyFor(overrides, all[i].action) || '').toLowerCase() === lk) return all[i];
    }
    return null;
  }

  // Pure: returns a NEW overrides object rather than mutating the one passed in.
  function withOverride(overrides, action, key) {
    var next = Object.assign({}, overrides);
    if (key) next[action] = key.toLowerCase(); else delete next[action];
    return next;
  }

  function loadOverrides(storage) {
    var raw = null;
    try { raw = storage.getItem(STORAGE_KEY); } catch (e) { return {}; }
    if (!raw) return {};
    try {
      var parsed = JSON.parse(raw);
      return isObject(parsed) ? parsed : {};
    } catch (e) { return {}; }
  }

  // One instance per preference context (production: one for the app's
  // lifetime; a test builds a fresh one per case) — never a module-level
  // singleton, so "a fresh preference context" is an actual new object, not
  // residual state from a previous test or reload.
  function create(ports) {
    var storage = ports && ports.storage;
    var onOverrideChanged = ports && ports.onOverrideChanged;
    var overrides = loadOverrides(storage);

    function setKey(action, key) {
      overrides = withOverride(overrides, action, key);
      try { storage.setItem(STORAGE_KEY, JSON.stringify(overrides)); } catch (e) { /* in-memory override still applies */ }
      if (typeof onOverrideChanged === 'function') onOverrideChanged(action);
    }

    // Matches the original reset button handler: clears state and storage
    // only. It never called the badge-refresh port either — the modal list
    // re-render was the caller's own job then, and stays so now.
    function reset() {
      overrides = {};
      try { storage.removeItem(STORAGE_KEY); } catch (e) { /* ignored, same as before */ }
    }

    return {
      defFor: defFor,
      keyFor: function (action) { return keyFor(overrides, action); },
      clashFor: function (action, key) { return clashFor(overrides, action, key); },
      setKey: setKey,
      reset: reset,
      toolShortcuts: function () { return TOOL_SHORTCUTS; },
      commandShortcuts: function () { return COMMAND_SHORTCUTS; },
      readonlyShortcuts: function () { return READONLY_SHORTCUTS; },
      categories: function () { return SHORTCUT_CATS; },
    };
  }

  return {
    create: create,
    defFor: defFor,
    keyFor: keyFor,
    clashFor: clashFor,
    withOverride: withOverride,
    TOOL_SHORTCUTS: TOOL_SHORTCUTS,
    COMMAND_SHORTCUTS: COMMAND_SHORTCUTS,
    READONLY_SHORTCUTS: READONLY_SHORTCUTS,
    SHORTCUT_CATS: SHORTCUT_CATS,
  };
})();
if (typeof module !== 'undefined' && module.exports) module.exports = NemoShortcutRegistry;
