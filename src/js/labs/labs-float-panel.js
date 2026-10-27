// ---- LABS FLOATING PANEL — contextual quick-access to Labs prototypes ----
// (2026-07, "un panel horizontal flottant dans les zones de canvas que
// l'on pourrait drop partout ... qui afficherait les outils Labs en
// fonction de l'outil sur lequel on clique"). Réglages > Labs (labs-
// panel.js) stays the full control surface (inline params, actions on the
// current selection); this is a fast, at-a-glance TOGGLE strip for the
// stay-on tools relevant to whatever tool is active right now — click
// Brush, see Miroir/Mandala; click Gomme, see Décalage onion; etc.
// Deliberately toggle-only (no inline params here — Réglages > Labs is
// where those live) to keep this small enough to float over the canvas
// without covering the drawing.
(function () {
  // Which stay-on Labs prototypes are relevant to which core tool. Only
  // tools with at least one entry ever show the panel for that tool.
  // Timeline-scoped prototypes (xsheet, timeline-markers, timeline-zoom)
  // aren't tied to a canvas tool at all, so they're deliberately absent
  // here — they stay Réglages-only.
  var TOOL_CONTEXT = {
    // 'symmetry' (promoted out of Labs, 2026-07) and 'perspective' (already
    // a real shipped feature, never in Labs at all) are BOTH permanently
    // reachable via their own toolbar button + right-panel section — but
    // "permanently reachable" turned out to mean "invisible unless you
    // already know to scroll the right panel" in practice (feedback:
    // "j'arrive pas à comprendre les outils que tu as mis en place... les
    // avoir dans la barre flottante quand on utilise l'outil brush"). Both
    // are real `state.*`-backed features, not Labs prototypes — see
    // REAL_FEATURES below for how this panel treats them as first-class
    // entries alongside the localStorage-flag-based Labs ones.
    // 'shadow-brush' added to every tool whose OWN commit code respects
    // state.shadowMode (tools.js/draw-bridge.js: Draw, Pen, Fill Brush,
    // Line/Rect/Ellipse all check it) — not just Draw. Before this, arming
    // Shadow Mode from Draw then switching to any of these silently kept
    // recoloring/tagging everything drawn there with ZERO visible
    // indicator in that tool's own context (2026-07: "pourquoi... ça change
    // de couleur au relâchement" — traced to exactly this: the mode was
    // still armed from earlier Draw-tool testing, with no way to notice
    // from Fill Brush's own panel). Line/Rect/Ellipse had it worst: those
    // three aren't even in TOOL_OPTS_TOOLS, so the right-panel checkbox
    // never showed for them either — this floating button was their ONLY
    // possible indicator, and it was missing too.
    draw: ['brush-menu', 'shadow-brush', 'symmetry', 'perspective', 'predictive-stroke', 'multiframe-draw', 'canvas-grid', 'view-filter'],
    fillbrush: ['brush-menu', 'shadow-brush', 'symmetry', 'perspective', 'canvas-grid', 'view-filter'],
    pen: ['shadow-brush', 'symmetry', 'perspective', 'french-curve', 'canvas-grid', 'view-filter'],
    line: ['shadow-brush', 'symmetry', 'perspective', 'french-curve', 'canvas-grid'],
    rect: ['shadow-brush', 'symmetry', 'canvas-grid', 'view-filter'],
    ellipse: ['shadow-brush', 'symmetry', 'french-curve', 'canvas-grid'],
    eraser: ['out-of-pegs', 'canvas-grid'],
    // 'brush-menu' added here (2026-07, Stroke-panel brush harmonization):
    // applying/converting a vector or bitmap brush texture on an EXISTING
    // selection (Select/Subselect tool, nothing being drawn) used to go
    // through the Stroke panel's own "Apply to selection" button/Bitmap
    // Brush checkbox — both removed in favor of the floating Brush panel
    // auto-applying on preset click (see brush-menu-bridge.js). Without an
    // entry here that panel's button never appears outside Draw/Fillbrush,
    // making that whole apply-to-selection path unreachable.
    select: ['brush-menu', 'vector-sculpt', 'canvas-grid'],
    subselect: ['brush-menu', 'vector-sculpt'],
    // 2026-07 feedback ("impossible de faire de faire des multiselection
    // avec fill/stroke select + shift... il faudrait afficher les outils de
    // lasso et rectangle de selection pour cette outil"): the marquee/lasso
    // drag itself was built (tools.js, Shift+click / Alt-drag) but only as a
    // hidden modifier-key convention — no VISIBLE way to pick a mode, which
    // is what was actually asked for. These two entries give it real toggle
    // buttons, same pattern as everything else in this panel.
    // 'fs-break-intersect' (2026-08, "il faudrait une option qui supprime
    // les stroke d'intersection... au drag... comme une brush") — the
    // classic animation cleanup "cassure": armed, a drag over a stroke
    // highlights every point where it crosses ANOTHER stroke, cutting a
    // small gap at each on release (tools.js's fsBreak* functions).
    fsselect: ['fs-select-rect', 'fs-select-lasso', 'fs-break-intersect'],
    hand: ['canvas-grid'],
  };
  // Real `state.*`-backed features (NOT Labs prototypes — no localStorage
  // flag, no SMLabs.register) shown in this same strip. isOn/toggle read
  // and write the actual app state directly and keep the right-panel
  // checkbox (#p-sym-on/#p-persp-on) in sync, so toggling from either place
  // never desyncs the other — same "two entry points, one truth" principle
  // the Labs toggle button already follows for renderLabsPanel().
  var REAL_FEATURES = {
    symmetry: {
      isOn: function () { return !!(window.state && state.symmetryEnabled); },
      toggle: function () {
        state.symmetryEnabled = !state.symmetryEnabled;
        if (state.symmetryEnabled) {
          if (window.ensureSymmetryAxis) window.ensureSymmetryAxis();
          if (window.ensureSymmetryRadialCenter) window.ensureSymmetryRadialCenter();
          // No longer force-switches into the 'symmetry' tool (2026-07
          // feedback: "je perds la sélection de l'outil brush... les
          // boutons du menu flottant disparaissent") — symmetry-bridge.js's
          // shouldEdit no longer requires actually BEING in that tool, just
          // the guide being enabled, so the axis is draggable immediately
          // from whatever tool (Brush, Fill Brush...) is already active,
          // and the floating panel (keyed off the CURRENT tool) stays put.
        }
        var cb = document.getElementById('p-sym-on'); if (cb) cb.checked = state.symmetryEnabled;
        if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
        var native = window.NemoNativeSymmetryController;
        if (native) native.route({ kind: 'setEnabled', enabled: state.symmetryEnabled });
      },
    },
    perspective: {
      isOn: function () {
        if (window.__TAURI__ && window.__TAURI__.core) return !!(window.perspectiveIsEnabled && window.perspectiveIsEnabled());
        return !!(window.state && state.perspectiveEnabled);
      },
      toggle: function (origin) {
        if (window.__TAURI__ && window.__TAURI__.core) {
          if (window.setPerspectiveEnabled) return window.setPerspectiveEnabled(!this.isOn(), origin);
          return;
        }
        state.perspectiveEnabled = !state.perspectiveEnabled;
        if (state.perspectiveEnabled) {
          if (window.ensurePerspectiveVPs) window.ensurePerspectiveVPs();
          // No longer force-switches tool — same reasoning as symmetry's
          // toggle above.
        }
        var cb = document.getElementById('p-persp-on'); if (cb) cb.checked = state.perspectiveEnabled;
        if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
      },
    },
  };
  // Non-toggle entries — a plain action button (opens a popover) rather
  // than an on/off switch, so `registered[n]`/`addBtn` below treat it
  // differently: `isActive` decides the `.active` highlight (whether the
  // popover happens to be open right now) and `onClick` just runs the
  // action instead of flipping a boolean. Currently only the Brush menu
  // (brush-menu-bridge.js, 2026-07 — "un menu... avec toute les brush et
  // des paramètre... 2 onglet vecto et bitmap").
  var ACTIONS = {
    'brush-menu': {
      isActive: function () { return !!(window.BrushMenu && window.BrushMenu.isOpen()); },
      onClick: function (btn) { if (window.BrushMenu) window.BrushMenu.toggle(btn); },
    },
    // Shadow Brush (2026-07, shadow-brush-bridge.js) — a guide-line brush
    // for the Stroke/Fill/Shadow layer-separation workflow, dedicated
    // colors+ids picked from its own small popover. `.active` reflects
    // state.shadowMode itself (armed), not just "popover happens to be
    // open" — the two ACTIONS above only ever mean the latter, but this one
    // needs to read as "on" for as long as the mode stays armed, since a
    // single click on an already-armed button disarms it immediately
    // instead of reopening the picker (see the bridge's own toggle()).
    'shadow-brush': {
      isActive: function () { return !!(window.SMShadowBrush && window.SMShadowBrush.isArmed()); },
      onClick: function (btn) { if (window.SMShadowBrush) window.SMShadowBrush.toggle(btn); },
    },
    // Fill/Stroke Select's marquee mode picker — mutually exclusive with
    // 'fs-select-lasso' below (isActive reflects state.fsSelectMode, default
    // 'rect'). tools.js reads state.fsSelectMode when starting a marquee on
    // empty-canvas click; Alt still works as a one-off override of whichever
    // mode is picked here (see the fsselect onMouseDown branch, tools.js).
    'fs-select-rect': {
      isActive: function () { return !(window.state && state.fsSelectMode === 'lasso'); },
      onClick: function () { if (window.state) state.fsSelectMode = 'rect'; renderLabsFloatPanel(); },
    },
    'fs-select-lasso': {
      isActive: function () { return !!(window.state && state.fsSelectMode === 'lasso'); },
      onClick: function () { if (window.state) state.fsSelectMode = 'lasso'; renderLabsFloatPanel(); },
    },
    // "Break at intersections" brush (2026-08) — stays armed like Shadow
    // Brush above (isActive reflects state.fsBreakMode itself, not just
    // "a popover happens to be open"), since the whole point is to drag
    // repeatedly across several crossings without re-clicking a mode button
    // between each one. Clears any live pending-cut markers on disarm so a
    // half-finished drag never leaves stale dots on screen.
    'fs-break-intersect': {
      isActive: function () { return !!(window.state && state.fsBreakMode); },
      onClick: function () {
        if (!window.state) return;
        state.fsBreakMode = !state.fsBreakMode;
        if (!state.fsBreakMode && window.SMEngineBridge) window.SMEngineBridge.setFSBreakMarks([]);
        renderLabsFloatPanel();
      },
    },
  };
  // Tools armed by a held key regardless of the active tool (flip-roll=R,
  // mirror-check=M, lagoon-menu=Q) — always available, shown after a
  // divider so they read as a separate "always on hand" group rather than
  // being confused for the current tool's own context.
  var ALWAYS = ['flip-roll', 'mirror-check', 'lagoon-menu'];

  // Values are i18n KEYS (not literal text), resolved via SM.t() in addBtn()
  // below — never here, at file-parse time: labs-float-panel.js loads
  // BEFORE timeline.js/i18n.js in index.html's script order (feedback #105:
  // this is exactly why the floating icon strip stayed French under an
  // English app language — SHORT_NAME held literal strings with no i18n
  // hookup at all).
  var SHORT_NAME = {
    'brush-menu': 'lfsShortBrushMenu',
    // UI/UX audit (2026-07-30): the only real explanation of this tool
    // lived in English on a checkbox buried in Export options, nowhere
    // near this icon — the actual discovery point for a first-time user.
    'shadow-brush': 'lfsShortShadowBrush',
    // UI/UX audit (2026-07-30): both of these restated their own name —
    // the ONLY tooltip either tool has anywhere (their sole discovery
    // point is this Labs icon), so a first-time user had no way to learn
    // what enabling them actually does before poking around. No After
    // Effects equivalent for either, unlike most of this panel's tools.
    'symmetry': 'lfsShortSymmetry',
    'perspective': 'lfsShortPerspective',
    'predictive-stroke': 'lfsShortPredictiveStroke',
    'multiframe-draw': 'lfsShortMultiframeDraw',
    'canvas-grid': 'lfsShortCanvasGrid',
    'view-filter': 'lfsShortViewFilter',
    'french-curve': 'lfsShortFrenchCurve',
    'vector-sculpt': 'lfsShortVectorSculpt',
    'out-of-pegs': 'lfsShortOutOfPegs',
    'flip-roll': 'lfsShortFlipRoll',
    'mirror-check': 'lfsShortMirrorCheck',
    'lagoon-menu': 'lfsShortLagoonMenu',
    'fs-select-rect': 'lfsShortFsSelectRect',
    'fs-select-lasso': 'lfsShortFsSelectLasso',
    'fs-break-intersect': 'lfsShortFsBreakIntersect',
  };

  // Solid/filled icons (2026-07, "les icônes ne sont pas trop flat design
  // comme les autres, parfois trait trop fin") — the app's own tool-btn
  // icons (index.html's .tool-btn buttons) are all solid fill="currentColor"
  // shapes with no stroke lines at all; the first pass here used thin
  // stroke-based line art, which read as a different, thinner icon
  // language. Rebuilt every icon as a solid silhouette to match.
  var ICONS = {
    // Reuses the EXACT same glyph as the main Brush/Draw tool button in the
    // left toolbar (index.html, data-tool="draw") — feedback 2026-07:
    // "certaines icônes comme les guides ou brush du panneau flottant ne
    // sont pas très claires". Same concept, same icon, so it reads as
    // recognizable on sight instead of a new abstract shape to learn.
    'brush-menu': '<svg viewBox="0 0 24 24" width="15" height="15" fill="currentColor"><path d="M7 14c-1.66 0-3 1.34-3 3 0 1.31-1.16 2-2 2 .92 1.22 2.49 2 4 2 2.21 0 4-1.79 4-4 0-1.66-1.34-3-3-3z"/><path d="M20.71 4.63l-1.34-1.34a1 1 0 00-1.41 0L9 12.25 11.75 15l8.96-8.96a1 1 0 000-1.41z"/></svg>',
    'shadow-brush': '<svg viewBox="0 0 24 24" width="15" height="15" fill="currentColor"><path d="M20.25 19.5h-9.44l7.724-7.715L21 9.31a1.49 1.49 0 0 0 0-2.119l-4.19-4.19a1.51 1.51 0 0 0-2.12 0L3.44 14.25A1.492 1.492 0 0 0 3 15.31v4.19A1.5 1.5 0 0 0 4.5 21h15.75a.75.75 0 1 0 0-1.5ZM4.5 15.31l9-9 4.19 4.19-9 9H4.5v-4.19Z"/></svg>',
    // Redrawn (2026-07, same "not very clear" feedback) as two solid
    // mirrored blocks either side of the axis instead of two thin
    // triangles — reads unambiguously as "mirror" even at 15px, where the
    // old triangle pair tended to blur into a single arrow-like shape.
    'symmetry': '<svg viewBox="0 0 24 24" width="15" height="15" fill="currentColor"><rect x="11" y="2" width="2" height="20" rx="1" opacity=".45"/><path d="M9 4H4a1 1 0 0 0-1 1v14a1 1 0 0 0 1 1h5z"/><path d="M15 4h5a1 1 0 0 1 1 1v14a1 1 0 0 1-1 1h-5z" opacity=".55"/></svg>',
    // Redrawn as a square frame with diagonals converging on a center
    // vanishing point — the standard "perspective grid" glyph used by
    // most design tools, clearer than the old horizon-line-plus-rays shape.
    'perspective': '<svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="3" width="18" height="18" rx="1"/><path d="M3 3l8.5 8.5M21 3l-8.5 8.5M3 21l8.5-8.5M21 21l-8.5-8.5"/><circle cx="12" cy="12" r="1.3" fill="currentColor" stroke="none"/></svg>',
    'predictive-stroke': '<svg viewBox="0 0 24 24" width="15" height="15" fill="currentColor"><path d="M12 2l1.2 3.8L17 7l-3.8 1.2L12 12l-1.2-3.8L7 7l3.8-1.2L12 2z"/><path d="M19 13l.6 1.9L21.5 15.5l-1.9.6L19 18l-.6-1.9L16.5 15.5l1.9-.6L19 13z"/><path d="M5 15l.5 1.5L7 17l-1.5.5L5 19l-.5-1.5L3 17l1.5-.5L5 15z"/></svg>',
    'multiframe-draw': '<svg viewBox="0 0 24 24" width="15" height="15" fill="currentColor"><path d="M12 2L22 7L12 12L2 7Z"/><path d="M2 12L12 17L22 12L22 14L12 19L2 14Z" opacity=".55"/></svg>',
    'canvas-grid': '<svg viewBox="0 0 24 24" width="15" height="15" fill="currentColor"><rect x="3" y="3" width="7" height="7" rx="1"/><rect x="14" y="3" width="7" height="7" rx="1"/><rect x="3" y="14" width="7" height="7" rx="1"/><rect x="14" y="14" width="7" height="7" rx="1"/></svg>',
    'view-filter': '<svg viewBox="0 0 24 24" width="15" height="15" fill="currentColor"><path fill-rule="evenodd" clip-rule="evenodd" d="M12 21a9 9 0 100-18 9 9 0 000 18zm0-2V5a7 7 0 010 14z"/></svg>',
    'french-curve': '<svg viewBox="0 0 24 24" width="15" height="15" fill="currentColor"><path fill-rule="evenodd" clip-rule="evenodd" d="M12 6c6 0 10 2.7 10 6s-4 6-10 6S2 15.3 2 12s4-6 10-6zm0 2.2c-4.9 0-7.8 1.8-7.8 3.8s2.9 3.8 7.8 3.8 7.8-1.8 7.8-3.8-2.9-3.8-7.8-3.8z"/></svg>',
    'vector-sculpt': '<svg viewBox="0 0 24 24" width="15" height="15" fill="currentColor"><path d="M2.5 20.5c5.5.5 7-8 10.3-13.6l2.7 1.8c-4 5.4-5.7 12.6-11.4 12.6z"/><path d="M14.2 4.6l6.3 1-2 6.3z"/></svg>',
    'out-of-pegs': '<svg viewBox="0 0 24 24" width="15" height="15" fill="currentColor"><circle cx="9" cy="12" r="6" opacity=".45"/><circle cx="15" cy="12" r="6"/></svg>',
    'flip-roll': '<svg viewBox="0 0 24 24" width="15" height="15" fill="currentColor"><path d="M17 1l4 4-4 4V6.5H8a4.5 4.5 0 00-4.5 4.5H1.5A7 7 0 018.5 4H17V1z"/><path d="M7 23l-4-4 4-4v3.5h9A4.5 4.5 0 0020.5 14H23a7 7 0 01-7 7H7v2z"/></svg>',
    'mirror-check': '<svg viewBox="0 0 24 24" width="15" height="15" fill="currentColor"><path d="M9 6L3 12L9 18V14H15V18L21 12L15 6V10H9Z"/></svg>',
    'lagoon-menu': '<svg viewBox="0 0 24 24" width="15" height="15" fill="currentColor"><path d="M12 12V3A9 9 0 0119.8 7.5Z"/><path d="M12 12L19.8 7.5A9 9 0 0119.8 16.5Z" opacity=".65"/><path d="M12 12L19.8 16.5A9 9 0 014.2 16.5Z" opacity=".4"/><path d="M12 12L4.2 16.5A9 9 0 0112 3V12Z" opacity=".2"/></svg>',
    // Dashed rectangle = the marquee shape itself, same visual language as
    // the app's own marquee overlay (dashed stroke, translucent fill).
    'fs-select-rect': '<svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="1.8"><rect x="4" y="5" width="16" height="14" rx="1" stroke-dasharray="3.2 2.4"/></svg>',
    // Freehand wavy loop = lasso, distinct silhouette from the rectangle.
    'fs-select-lasso': '<svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"><path d="M12 3c-4.5 0-8 2.7-8 6.5 0 3 2.3 5.3 5.6 6.1L8 20l2.2-1.1c.6.1 1.2.1 1.8.1 4.5 0 8-2.7 8-6.5S16.5 3 12 3z" stroke-dasharray="2.6 2.2"/></svg>',
    // Two crossing strokes, one drawn UNBROKEN (the one being crossed) and
    // the other with a small gap right at the crossing point (the one the
    // brush just cut) — reads directly as "break where lines cross"
    // without needing scissors iconography that'd be ambiguous next to the
    // Eraser tool's own icon.
    'fs-break-intersect': '<svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"><path d="M4 4l16 16"/><path d="M20 4l-6.2 6.2M9.8 14.2 4 20"/></svg>',
  };

  // Adjustable-parameter tools get a second row below the icon strip
  // (2026-07, "certaines fonctions ont besoin de paramètre, il faudrait
  // ... un panel de paramètres en dessous"), matching the reference
  // image's [-] value [+] stepper pill. Only tools with a SIMPLE numeric
  // knob and an existing SMLabs setter get one here — french-curve
  // (object-shaped guide position) and out-of-pegs (4 independent
  // numbers) stay Réglages/console-only, a stepper pair or quad would be
  // more UI than this compact strip should carry.
  // `labelKey` is an i18n key, resolved via SM.t() where the pill is built
  // (inside renderLabsFloatPanel, at render time — safe, unlike this
  // top-level object literal itself).
  var PARAMS = {
    'vector-sculpt': [{ key: 'nemo-labs-sculpt-radius', labelKey: 'lfsLabelRadius', def: 60, min: 8, max: 400, step: 10, set: function (v) { window.SMLabs.setSculptRadius(v); } }],
    'canvas-grid': [{ key: 'nemo-labs-grid-step', labelKey: 'lfsLabelStep', def: 100, min: 4, max: 500, step: 10, set: function (v) { window.SMLabs.setGridStep(v); } }],
    'flip-roll': [
      { key: 'nemo-labs-flip-speed', labelKey: 'lfsLabelSpeed', def: 6, min: 2, max: 24, step: 1, set: function (v) { window.SMLabs.setFlipSpeed(v); } },
      { key: 'nemo-labs-flip-span', labelKey: 'lfsLabelSpan', def: 2, min: 1, max: 6, step: 1, set: function (v) { window.SMLabs.setFlipSpan(v); } },
    ],
  };
  var SYM_MODES = ['y', 'x', 'free', 'radial'];
  var PERSP_MODES = ['1pt', '2pt', '3pt', 'fisheye'];
  // Values are i18n keys, resolved via SM.t() at every usage site below
  // (never here — see the SHORT_NAME comment above for why).
  var PERSP_MODE_LABEL = { '1pt': 'lfsPersp1pt', '2pt': 'lfsPersp2pt', '3pt': 'lfsPersp3pt', fisheye: 'lfsPerspFisheye' };
  var SYM_MODE_LABEL = { y: 'lfsSymY', x: 'lfsSymX', free: 'lfsSymFree', radial: 'lfsSymRadial' };

  var floatingUi=null;
  function floatingUiInstance(){
    if(!floatingUi)floatingUi=window.NemoLabsFloatingUI.create({
      labels:{lfsLabelMode:function(){return SM.t('lfsLabelMode');},lfsLabelSectors:function(){return SM.t('lfsLabelSectors');},lfsBtnExtend:function(){return SM.t('lfsBtnExtend');},lfsExtendTitle:function(){return SM.t('lfsExtendTitle');},lfsBtnReset:function(){return SM.t('lfsBtnReset');},lfsResetPositionTitle:function(){return SM.t('lfsResetPositionTitle');},lfsBtnLock:function(){return SM.t('lfsBtnLock');},lfsLockTitle:function(){return SM.t('lfsLockTitle');},lfsResetPositionsTitle:function(){return SM.t('lfsResetPositionsTitle');}},
      document:document,contexts:TOOL_CONTEXT,realFeatures:REAL_FEATURES,actions:ACTIONS,always:ALWAYS,names:SHORT_NAME,icons:ICONS,params:PARAMS,
      symModes:SYM_MODES,perspModes:PERSP_MODES,perspLabels:PERSP_MODE_LABEL,symLabels:SYM_MODE_LABEL,
      ready:function(){return window.SMLabs&&window.state;},list:function(){return SMLabs.list();},
      readField:function(key){
        if (key.indexOf('perspective') === 0 && window.__TAURI__ && window.__TAURI__.core) {
          var value = window.readPerspectiveSettings && window.readPerspectiveSettings();
          return value ? key === 'perspectiveMode' && value[key] === undefined ? '2pt' : value[key] : undefined;
        }
        return state[key];
      },
      perspectiveNative:function(){return !!(window.__TAURI__&&window.__TAURI__.core);},
      perspectiveToken:function(){var c=window.NemoNativePerspectiveController;return c&&c.read();},
      perspectiveReady:function(){var c=window.NemoNativePerspectiveController;return !!(c&&c.read()&&!c.isPending());},toggle:function(name){return window.SMLabs.toggle(name);},
      title:function(key){return (window.SM&&SM.t?SM.t(key):null);},translate:function(key){return SM.t(key);},
      refreshSettings:function(){if(window.renderLabsPanel)window.renderLabsPanel();},
      getItem:function(key){return localStorage.getItem(key);},setItem:function(key,value){return localStorage.setItem(key,value);},
      addEventListener:function(name,fn){return window.addEventListener(name,fn);},setTimeout:function(fn,delay){return setTimeout(fn,delay);},
      setSectors:function(value){state.symmetryRadialSectors=Math.max(2,Math.min(24,value));var c=window.NemoNativeSymmetryController;if(c)c.route({kind:'setSectors',sectors:state.symmetryRadialSectors});},
      toggleExtend:function(){state.symmetryExtend=!state.symmetryExtend;var c=window.NemoNativeSymmetryController;if(c)c.route({kind:'setExtend',extend:state.symmetryExtend});},
      perspectiveLocked:function(){var vps=window.ensurePerspectiveVPs?window.ensurePerspectiveVPs():[];return !!(vps.length&&vps.every(function(vp){return vp.locked;}));},
      lockPerspective:function(locked,origin){
        if(window.__TAURI__&&window.__TAURI__.core){if(window.lockPerspectiveVPs)return window.lockPerspectiveVPs(locked,origin);return;}
        (window.ensurePerspectiveVPs?window.ensurePerspectiveVPs():[]).forEach(function(vp){vp.locked=locked;});
      },
      renderNow:function(){if(window.SMEngineBridge)window.SMEngineBridge.renderNow();},
      setSymmetryMode:function(value){if(window.setSymmetryMode)return window.setSymmetryMode(value);},
      syncSymmetryPanelVisibility:function(){if(window.syncSymmetryPanelVisibility)return window.syncSymmetryPanelVisibility();},
      resetSymmetryGuide:function(){if(window.resetSymmetryGuide)return window.resetSymmetryGuide();},
      setPerspectiveMode:function(value,origin){if(window.setPerspectiveMode)return window.setPerspectiveMode(value,origin);},
      resetPerspectiveVPs:function(origin){if(window.resetPerspectiveVPs)return window.resetPerspectiveVPs(origin);},
    });
    return floatingUi;
  }
  function renderLabsFloatPanel(){return floatingUiInstance().render();}
  function setupDrag(){return floatingUiInstance().setupDrag();}
  function setupHover(){return floatingUiInstance().setupHover();}

  function init() {
    setupDrag();
    setupHover();
    renderLabsFloatPanel();
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', init);
  else init();

  window.renderLabsFloatPanel = renderLabsFloatPanel;
  // Repaint on language switch — same rationale/timing as labs-panel.js's
  // own afterI18n registration (see its comment): this file loads BEFORE
  // timeline.js, whose `window.SM={...}` is a full reassignment, so the
  // push is deferred to 'load' rather than done at parse time.
  window.addEventListener('load', function () {
    window.SM = window.SM || {};
    (window.SM.afterI18n = window.SM.afterI18n || []).push(renderLabsFloatPanel);
  });
})();
