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
    draw: ['brush-menu', 'symmetry', 'perspective', 'predictive-stroke', 'multiframe-draw', 'canvas-grid', 'view-filter'],
    fillbrush: ['brush-menu', 'symmetry', 'perspective', 'canvas-grid', 'view-filter'],
    pen: ['symmetry', 'perspective', 'french-curve', 'canvas-grid', 'view-filter'],
    line: ['symmetry', 'perspective', 'french-curve', 'canvas-grid'],
    rect: ['symmetry', 'canvas-grid', 'view-filter'],
    ellipse: ['symmetry', 'french-curve', 'canvas-grid'],
    eraser: ['out-of-pegs', 'canvas-grid'],
    select: ['vector-sculpt', 'canvas-grid'],
    subselect: ['vector-sculpt'],
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
          // Feedback: "on ne peut pas changer l'emplacement des points/
          // guides" — turning the guide on from HERE used to leave
          // state.tool at whatever it was (almost always Draw/Fillbrush),
          // so the axis appeared but nothing you clicked on it did
          // anything: dragging is gated on state.tool==='symmetry' (see
          // symmetry-bridge.js's shouldEdit). Jumping into that tool the
          // instant the guide turns on means the very first thing the user
          // can do is drag it into place, no separate "now click the tiny
          // toolbar icon" step required.
          if (window.SM && window.SM.setTool) window.SM.setTool('symmetry');
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
          // Same reasoning as symmetry's toggle above.
          if (window.SM && window.SM.setTool) window.SM.setTool('perspective');
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
  };
  // Tools armed by a held key regardless of the active tool (flip-roll=R,
  // mirror-check=M, lagoon-menu=Q) — always available, shown after a
  // divider so they read as a separate "always on hand" group rather than
  // being confused for the current tool's own context.
  var ALWAYS = ['flip-roll', 'mirror-check', 'lagoon-menu'];

  var SHORT_NAME = {
    'brush-menu': 'Brosses (vecteur/bitmap)',
    'symmetry': 'Guide de symétrie / mandala',
    'perspective': 'Guide de perspective',
    'predictive-stroke': 'Trait prédictif',
    'multiframe-draw': 'Dessin multi-frames',
    'canvas-grid': 'Grille',
    'view-filter': 'Contrôle des valeurs',
    'french-curve': 'Gabarit courbe (maintenir F)',
    'vector-sculpt': 'Sculpt vectoriel (maintenir W)',
    'out-of-pegs': 'Décalage des fantômes onion',
    'flip-roll': "Rouleau d'animateur (maintenir R)",
    'mirror-check': 'Miroir de contrôle (maintenir M)',
    'lagoon-menu': "Menu radial d'outils (maintenir Q)",
  };

  // Solid/filled icons (2026-07, "les icônes ne sont pas trop flat design
  // comme les autres, parfois trait trop fin") — the app's own tool-btn
  // icons (index.html's .tool-btn buttons) are all solid fill="currentColor"
  // shapes with no stroke lines at all; the first pass here used thin
  // stroke-based line art, which read as a different, thinner icon
  // language. Rebuilt every icon as a solid silhouette to match.
  var ICONS = {
    'brush-menu': '<svg viewBox="0 0 24 24" width="15" height="15" fill="currentColor"><path d="M19.4 2.6c.8.8.8 2 0 2.8L9.5 15.3l-4-4L15.4 1.4c.8-.8 2-.8 2.8 0z" opacity=".85"/><path d="M8.3 12.4l3.3 3.3-1.4 1.4c-1.8 1.8-6.4 2-6.4 2s.2-4.6 2-6.4z"/></svg>',
    // Same "split by an axis" language as the toolbar button (index.html),
    // simplified/solid-filled to match this strip's icon set.
    'symmetry': '<svg viewBox="0 0 24 24" width="15" height="15" fill="currentColor"><rect x="11" y="2" width="2" height="20" rx="1"/><path d="M9 6L3 12L9 18Z"/><path d="M15 6L21 12L15 18Z"/></svg>',
    'perspective': '<svg viewBox="0 0 24 24" width="15" height="15" fill="currentColor"><circle cx="12" cy="6" r="2"/><path d="M12 6L2 22h4L12 10l6 12h4Z"/><path d="M2 22L12 6l10 16" fill="none" stroke="currentColor" stroke-width="1.4" opacity=".5"/></svg>',
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
    paramsWrap.innerHTML = '';
    var anyParams = false;
    if (registered.symmetry) {
      anyParams = true;
      var modePill = document.createElement('div'); modePill.className = 'labs-float-stepper';
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
        var secPill = document.createElement('div'); secPill.className = 'labs-float-stepper';
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
    }
    shown.forEach(function (n) {
      if (!registered[n] || !PARAMS[n]) return;
      PARAMS[n].forEach(function (p) {
        anyParams = true;
        var pill = document.createElement('div');
        pill.className = 'labs-float-stepper';
        var lbl = document.createElement('span'); lbl.className = 'lfs-label'; lbl.textContent = p.label;
        var minus = document.createElement('button'); minus.className = 'lfs-btn'; minus.textContent = '−';
        var val = document.createElement('span'); val.className = 'lfs-val';
        var plus = document.createElement('button'); plus.className = 'lfs-btn'; plus.textContent = '+';
        function refresh() { val.textContent = paramValue(p); }
        refresh();
        minus.addEventListener('click', function () { p.set(Math.max(p.min, paramValue(p) - p.step)); refresh(); });
        plus.addEventListener('click', function () { p.set(Math.min(p.max, paramValue(p) + p.step)); refresh(); });
        pill.appendChild(lbl); pill.appendChild(minus); pill.appendChild(val); pill.appendChild(plus);
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
