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
    fsselect: ['fs-select-rect', 'fs-select-lasso'],
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
      },
    },
    perspective: {
      isOn: function () { return !!(window.state && state.perspectiveEnabled); },
      toggle: function () {
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
  };
  // Tools armed by a held key regardless of the active tool (flip-roll=R,
  // mirror-check=M, lagoon-menu=Q) — always available, shown after a
  // divider so they read as a separate "always on hand" group rather than
  // being confused for the current tool's own context.
  var ALWAYS = ['flip-roll', 'mirror-check', 'lagoon-menu'];

  var SHORT_NAME = {
    'brush-menu': 'Brosses (vecteur/bitmap)',
    'shadow-brush': 'Shadow Brush (lignes de délimitation, calque Shadow)',
    'symmetry': 'Guide de symétrie / mandala',
    'perspective': 'Guide de perspective',
    'predictive-stroke': 'Trait prédictif',
    'multiframe-draw': 'Dessin multi-frames',
    'canvas-grid': 'Grille',
    'view-filter': 'Contrôle des valeurs',
    'french-curve': 'Gabarit courbe (maintenir F)',
    'vector-sculpt': 'Sculpt vectoriel — glisser, Shift pour lisser, Alt+glisser pour le rayon',
    'out-of-pegs': 'Décalage des fantômes onion',
    'flip-roll': "Rouleau d'animateur (maintenir R)",
    'mirror-check': 'Miroir de contrôle (maintenir M)',
    'lagoon-menu': "Menu radial d'outils (maintenir Q)",
    'fs-select-rect': 'Sélection rectangle (Fill/Stroke Select)',
    'fs-select-lasso': 'Sélection lasso (Fill/Stroke Select) — Alt+glisser inverse temporairement',
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
  };

  // Adjustable-parameter tools get a second row below the icon strip
  // (2026-07, "certaines fonctions ont besoin de paramètre, il faudrait
  // ... un panel de paramètres en dessous"), matching the reference
  // image's [-] value [+] stepper pill. Only tools with a SIMPLE numeric
  // knob and an existing SMLabs setter get one here — french-curve
  // (object-shaped guide position) and out-of-pegs (4 independent
  // numbers) stay Réglages/console-only, a stepper pair or quad would be
  // more UI than this compact strip should carry.
  var PARAMS = {
    'vector-sculpt': [{ key: 'nemo-labs-sculpt-radius', label: 'Rayon', def: 60, min: 8, max: 400, step: 10, set: function (v) { window.SMLabs.setSculptRadius(v); } }],
    'canvas-grid': [{ key: 'nemo-labs-grid-step', label: 'Pas', def: 100, min: 4, max: 500, step: 10, set: function (v) { window.SMLabs.setGridStep(v); } }],
    'flip-roll': [
      { key: 'nemo-labs-flip-speed', label: 'Vitesse', def: 6, min: 2, max: 24, step: 1, set: function (v) { window.SMLabs.setFlipSpeed(v); } },
      { key: 'nemo-labs-flip-span', label: 'Portée', def: 2, min: 1, max: 6, step: 1, set: function (v) { window.SMLabs.setFlipSpan(v); } },
    ],
  };
  function paramValue(p) {
    var n = parseFloat(localStorage.getItem(p.key));
    return isNaN(n) ? p.def : n;
  }

  // A name is "available" (gets a button at all) if it's either a
  // registered Labs prototype, one of the real state-backed REAL_FEATURES,
  // or a plain ACTIONS entry — all three are otherwise treated identically
  // by everything below (same button, same active-state highlight).
  function buildGroup(names, registered) {
    return names.filter(function (n) { return registered.hasOwnProperty(n) || REAL_FEATURES.hasOwnProperty(n) || ACTIONS.hasOwnProperty(n); });
  }
  var SYM_MODES = ['y', 'x', 'free', 'radial'];
  var PERSP_MODES = ['1pt', '2pt', '3pt', 'fisheye'];
  var PERSP_MODE_LABEL = { '1pt': '1pt', '2pt': '2pt', '3pt': '3pt', fisheye: 'Fisheye' };
  var SYM_MODE_LABEL = { y: 'Y', x: 'X', free: 'Libre', radial: 'Radial' };

  function renderLabsFloatPanel() {
    var panel = document.getElementById('labs-float-panel');
    var iconsWrap = document.getElementById('labs-float-icons');
    var paramsWrap = document.getElementById('labs-float-params');
    if (!panel || !iconsWrap || !paramsWrap || !window.SMLabs || !window.state) return;
    var registered = {};
    SMLabs.list().forEach(function (p) { registered[p.name] = p.on; });
    // REAL_FEATURES entries override/augment the Labs `on` reading with
    // their own live state check — done here, once, rather than special-
    // casing every reader below.
    Object.keys(REAL_FEATURES).forEach(function (n) { registered[n] = REAL_FEATURES[n].isOn(); });
    Object.keys(ACTIONS).forEach(function (n) { registered[n] = ACTIONS[n].isActive(); });
    var ctx = buildGroup(TOOL_CONTEXT[state.tool] || [], registered);
    var always = buildGroup(ALWAYS.filter(function (n) { return ctx.indexOf(n) < 0; }), registered);
    if (!ctx.length && !always.length) { panel.style.display = 'none'; return; }
    panel.style.display = 'flex';
    iconsWrap.innerHTML = '';
    var shown = ctx.concat(always);
    function addBtn(n) {
      var btn = document.createElement('button');
      btn.className = 'labs-float-btn' + (registered[n] ? ' active' : '');
      btn.title = SHORT_NAME[n] || n;
      btn.innerHTML = ICONS[n] || '';
      btn.addEventListener('click', function () {
        // ACTIONS entries manage their own re-render (BrushMenu.toggle
        // calls renderLabsFloatPanel itself once the popover's open/closed
        // state is settled) — skip the generic renderLabsFloatPanel() call
        // below for those so a just-opened popover's own button doesn't
        // get redrawn (and re-bound) out from under an in-flight click.
        if (ACTIONS[n]) { ACTIONS[n].onClick(btn); return; }
        if (REAL_FEATURES[n]) REAL_FEATURES[n].toggle();
        else window.SMLabs.toggle(n);
        renderLabsFloatPanel();
        // Keep Réglages > Labs in sync if it happens to be open at the
        // same time — same checkbox state, two entry points.
        if (window.renderLabsPanel) window.renderLabsPanel();
      });
      iconsWrap.appendChild(btn);
    }
    ctx.forEach(addBtn);
    if (ctx.length && always.length) {
      var div = document.createElement('div');
      div.className = 'labs-float-divider';
      iconsWrap.appendChild(div);
    }
    always.forEach(addBtn);

    // Params row: one stepper pill per numeric knob, for every currently-
    // shown tool that's both ON and has a PARAMS entry — PLUS a hand-built
    // pair of pills for Symmetry (mode cycle + conditional sector count),
    // which don't fit PARAMS' "one localStorage-backed number" shape since
    // mode is a string cycle and both fields live on `state`, not
    // localStorage.
    // Every option row carries its owning tool's icon at the far left, greyed
    // (feedback 2026-07-26: "Un icon tout à gauche de la ligne de l'outil
    // grisé doit apparaître"). The PARAMS-driven rows further down already
    // built one; the hand-written Symmetry/Perspective pills never did, so
    // with several tools active at once those rows were the only ones giving
    // no clue which tool they belonged to — precisely the case the feedback
    // is about. One helper so a future hand-built pill can't forget again.
    function stepperPill(toolKey) {
      var pill = document.createElement('div');
      pill.className = 'labs-float-stepper';
      var ico = document.createElement('span');
      ico.className = 'lfs-tool-icon';
      ico.innerHTML = ICONS[toolKey] || '';
      pill.appendChild(ico);
      return pill;
    }
    paramsWrap.innerHTML = '';
    var anyParams = false;
    if (registered.symmetry) {
      anyParams = true;
      var modePill = stepperPill('symmetry');
      var modeLbl = document.createElement('span'); modeLbl.className = 'lfs-label'; modeLbl.textContent = 'Mode';
      var modePrev = document.createElement('button'); modePrev.className = 'lfs-btn'; modePrev.textContent = '−';
      var modeVal = document.createElement('span'); modeVal.className = 'lfs-val';
      var modeNext = document.createElement('button'); modeNext.className = 'lfs-btn'; modeNext.textContent = '+';
      function cycleMode(dir) {
        var i = SYM_MODES.indexOf(state.symmetryMode);
        var next = SYM_MODES[(i + dir + SYM_MODES.length) % SYM_MODES.length];
        if (window.setSymmetryMode) window.setSymmetryMode(next);
        var sel = document.getElementById('p-sym-mode'); if (sel) sel.value = next;
        if (window.syncSymmetryPanelVisibility) window.syncSymmetryPanelVisibility();
        modeVal.textContent = SYM_MODE_LABEL[next];
        renderLabsFloatPanel(); // sectors pill needs to appear/disappear when entering/leaving radial
      }
      modeVal.textContent = SYM_MODE_LABEL[state.symmetryMode] || state.symmetryMode;
      modePrev.addEventListener('click', function () { cycleMode(-1); });
      modeNext.addEventListener('click', function () { cycleMode(1); });
      modePill.appendChild(modeLbl); modePill.appendChild(modePrev); modePill.appendChild(modeVal); modePill.appendChild(modeNext);
      paramsWrap.appendChild(modePill);
      if (state.symmetryMode === 'radial') {
        var secPill = stepperPill('symmetry');
        var secLbl = document.createElement('span'); secLbl.className = 'lfs-label'; secLbl.textContent = 'Secteurs';
        var secMinus = document.createElement('button'); secMinus.className = 'lfs-btn'; secMinus.textContent = '−';
        var secVal = document.createElement('span'); secVal.className = 'lfs-val'; secVal.textContent = state.symmetryRadialSectors;
        var secPlus = document.createElement('button'); secPlus.className = 'lfs-btn'; secPlus.textContent = '+';
        function setSectors(v) {
          state.symmetryRadialSectors = Math.max(2, Math.min(24, v));
          secVal.textContent = state.symmetryRadialSectors;
          var inp = document.getElementById('p-sym-sectors'); if (inp) inp.value = state.symmetryRadialSectors;
          if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
        }
        secMinus.addEventListener('click', function () { setSectors(state.symmetryRadialSectors - 1); });
        secPlus.addEventListener('click', function () { setSectors(state.symmetryRadialSectors + 1); });
        secPill.appendChild(secLbl); secPill.appendChild(secMinus); secPill.appendChild(secVal); secPill.appendChild(secPlus);
        paramsWrap.appendChild(secPill);
      }
      // Extend/Reset (2026-07): moved here from the now-removed right-panel
      // "Symmetry Guide" section (feedback: "les onglet guide symétrie et
      // perspective n'ont plus lieu d'être, les options doivent être gérées
      // au niveau du panneau flottant") — same wide-toggle-pill/action-pill
      // shape as Perspective's own Lock/Reset just below.
      var extPill = stepperPill('symmetry');
      var extBtn = document.createElement('button'); extBtn.className = 'lfs-btn wide' + (state.symmetryExtend ? ' on' : '');
      extBtn.textContent = 'Extend'; extBtn.title = "Off: a stroke crossing the axis is cut right at it, so it and its mirror never overlap at the fold";
      extBtn.addEventListener('click', function () {
        state.symmetryExtend = !state.symmetryExtend;
        extBtn.classList.toggle('on', state.symmetryExtend);
      });
      extPill.appendChild(extBtn);
      paramsWrap.appendChild(extPill);
      var symResetPill = stepperPill('symmetry');
      var symResetBtn = document.createElement('button'); symResetBtn.className = 'lfs-btn wide';
      symResetBtn.textContent = 'Reset'; symResetBtn.title = 'Reset position';
      symResetBtn.addEventListener('click', function () { if (window.resetSymmetryGuide) window.resetSymmetryGuide(); });
      symResetPill.appendChild(symResetBtn);
      paramsWrap.appendChild(symResetPill);
    }
    if (registered.perspective) {
      // Mode/Density/Lock/Reset (2026-07): moved here from the removed
      // right-panel "Perspective Guide" section (same feedback as
      // Symmetry's Extend/Reset above) — only on/off was mirrored here
      // before, so this panel had no Mode control at all yet either.
      anyParams = true;
      var pModePill = stepperPill('perspective');
      var pModeLbl = document.createElement('span'); pModeLbl.className = 'lfs-label'; pModeLbl.textContent = 'Mode';
      var pModePrev = document.createElement('button'); pModePrev.className = 'lfs-btn'; pModePrev.textContent = '−';
      var pModeVal = document.createElement('span'); pModeVal.className = 'lfs-val';
      var pModeNext = document.createElement('button'); pModeNext.className = 'lfs-btn'; pModeNext.textContent = '+';
      function cyclePerspMode(dir) {
        var i = PERSP_MODES.indexOf(state.perspectiveMode);
        var next = PERSP_MODES[(i + dir + PERSP_MODES.length) % PERSP_MODES.length];
        if (window.setPerspectiveMode) window.setPerspectiveMode(next);
        pModeVal.textContent = PERSP_MODE_LABEL[next];
      }
      pModeVal.textContent = PERSP_MODE_LABEL[state.perspectiveMode] || state.perspectiveMode;
      pModePrev.addEventListener('click', function () { cyclePerspMode(-1); });
      pModeNext.addEventListener('click', function () { cyclePerspMode(1); });
      pModePill.appendChild(pModeLbl); pModePill.appendChild(pModePrev); pModePill.appendChild(pModeVal); pModePill.appendChild(pModeNext);
      paramsWrap.appendChild(pModePill);
      var densPill = stepperPill('perspective');
      var densLbl = document.createElement('span'); densLbl.className = 'lfs-label'; densLbl.textContent = 'Densité';
      var densMinus = document.createElement('button'); densMinus.className = 'lfs-btn'; densMinus.textContent = '−';
      var densVal = document.createElement('span'); densVal.className = 'lfs-val'; densVal.textContent = state.perspectiveDensity;
      var densPlus = document.createElement('button'); densPlus.className = 'lfs-btn'; densPlus.textContent = '+';
      function setDensity(v) {
        state.perspectiveDensity = Math.max(4, Math.min(72, v));
        densVal.textContent = state.perspectiveDensity;
        if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
      }
      densMinus.addEventListener('click', function () { setDensity(state.perspectiveDensity - 2); });
      densPlus.addEventListener('click', function () { setDensity(state.perspectiveDensity + 2); });
      densPill.appendChild(densLbl); densPill.appendChild(densMinus); densPill.appendChild(densVal); densPill.appendChild(densPlus);
      paramsWrap.appendChild(densPill);
      var lockPill = stepperPill('perspective');
      var lockBtn = document.createElement('button'); lockBtn.className = 'lfs-btn wide';
      lockBtn.textContent = 'Lock'; lockBtn.title = 'Prevents dragging any vanishing point with the Perspective tool';
      var vpsNow = window.ensurePerspectiveVPs ? window.ensurePerspectiveVPs() : [];
      lockBtn.classList.toggle('on', !!(vpsNow.length && vpsNow.every(function (vp) { return vp.locked; })));
      lockBtn.addEventListener('click', function () {
        var locked = !lockBtn.classList.contains('on');
        (window.ensurePerspectiveVPs ? window.ensurePerspectiveVPs() : []).forEach(function (vp) { vp.locked = locked; });
        lockBtn.classList.toggle('on', locked);
        if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
      });
      lockPill.appendChild(lockBtn);
      paramsWrap.appendChild(lockPill);
      var perspResetPill = stepperPill('perspective');
      var perspResetBtn = document.createElement('button'); perspResetBtn.className = 'lfs-btn wide';
      perspResetBtn.textContent = 'Reset'; perspResetBtn.title = 'Reset positions';
      perspResetBtn.addEventListener('click', function () { if (window.resetPerspectiveVPs) window.resetPerspectiveVPs(); });
      perspResetPill.appendChild(perspResetBtn);
      paramsWrap.appendChild(perspResetPill);
    }
    shown.forEach(function (n) {
      if (!registered[n] || !PARAMS[n]) return;
      PARAMS[n].forEach(function (p) {
        anyParams = true;
        var pill = document.createElement('div');
        pill.className = 'labs-float-stepper';
        var lbl = document.createElement('span'); lbl.className = 'lfs-label'; lbl.textContent = p.label;
        var minus = document.createElement('button'); minus.className = 'lfs-btn'; minus.textContent = '−';
        var val = document.createElement('input');
        val.type = 'number';
        val.className = 'lfs-val scrub';
        val.min = p.min; val.max = p.max; val.step = p.step;
        var plus = document.createElement('button'); plus.className = 'lfs-btn'; plus.textContent = '+';
        function refresh() { val.value = paramValue(p); }
        refresh();
        minus.addEventListener('click', function () { p.set(Math.max(p.min, paramValue(p) - p.step)); refresh(); });
        plus.addEventListener('click', function () { p.set(Math.min(p.max, paramValue(p) + p.step)); refresh(); });
        val.addEventListener('change', function () {
          var next = Math.max(p.min, Math.min(p.max, parseFloat(val.value)));
          if (isNaN(next)) next = p.def;
          p.set(next); refresh();
        });
        var toolIcon = document.createElement('span');
        toolIcon.className = 'lfs-tool-icon';
        toolIcon.innerHTML = ICONS[n] || '';
        pill.appendChild(toolIcon); pill.appendChild(lbl); pill.appendChild(minus); pill.appendChild(val); pill.appendChild(plus);
        paramsWrap.appendChild(pill);
      });
    });
    paramsWrap.style.display = anyParams ? 'flex' : 'none';
  }

  // -- drag anywhere within #canvas-area, position persisted -------------
  // Stored RELATIVE to #canvas-area's own rect (e.g. "20px from its left
  // edge"), not as raw viewport coordinates — the panel itself is
  // position:fixed (see its CSS comment for why: it's a DOM sibling of
  // #canvas-area, not a child, so it has no positioning ancestor of its
  // own), and #canvas-area's viewport position can shift (window resize,
  // side panels toggling). Storing relative-to-canvas-area keeps a saved
  // position meaningful across those changes instead of drifting off into
  // whatever now occupies those raw screen pixels.
  var POS_KEY = 'nemo-labs-float-pos';
  function loadPos() {
    try { return JSON.parse(localStorage.getItem(POS_KEY)) || { left: 20, top: 60 }; }
    catch (e) { return { left: 20, top: 60 }; }
  }
  function savePos(p) { try { localStorage.setItem(POS_KEY, JSON.stringify(p)); } catch (e) {} }

  var relPos = null; // {left, top} relative to #canvas-area's rect
  function applyFixedPos() {
    var panel = document.getElementById('labs-float-panel');
    var area = document.getElementById('canvas-area');
    if (!panel || !area || !relPos) return;
    var ar = area.getBoundingClientRect();
    panel.style.left = (ar.left + relPos.left) + 'px';
    panel.style.top = (ar.top + relPos.top) + 'px';
  }

  function setupDrag() {
    var panel = document.getElementById('labs-float-panel');
    var handle = document.getElementById('labs-float-handle');
    var area = document.getElementById('canvas-area');
    if (!panel || !handle || !area) return;
    relPos = loadPos();
    applyFixedPos();
    window.addEventListener('resize', applyFixedPos);
    var dragging = false, startX = 0, startY = 0, startLeft = 0, startTop = 0;
    handle.addEventListener('pointerdown', function (e) {
      dragging = true;
      startX = e.clientX; startY = e.clientY;
      startLeft = relPos.left; startTop = relPos.top;
      handle.setPointerCapture(e.pointerId);
      e.preventDefault();
    });
    handle.addEventListener('pointermove', function (e) {
      if (!dragging) return;
      var ar = area.getBoundingClientRect();
      var nl = startLeft + (e.clientX - startX);
      var nt = startTop + (e.clientY - startY);
      // Clamped to stay fully inside the canvas area — "drop partout" means
      // anywhere in the drawing zone, not off past its edges where it'd be
      // unreachable.
      nl = Math.max(2, Math.min(ar.width - panel.offsetWidth - 2, nl));
      nt = Math.max(2, Math.min(ar.height - panel.offsetHeight - 2, nt));
      relPos = { left: nl, top: nt };
      applyFixedPos();
    });
    handle.addEventListener('pointerup', function (e) {
      if (!dragging) return;
      dragging = false;
      try { handle.releasePointerCapture(e.pointerId); } catch (er) {}
      savePos(relPos);
    });
  }

  function init() {
    setupDrag();
    renderLabsFloatPanel();
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', init);
  else init();

  window.renderLabsFloatPanel = renderLabsFloatPanel;
})();
