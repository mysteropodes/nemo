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
    draw: ['symmetry', 'radial-symmetry', 'predictive-stroke', 'multiframe-draw', 'canvas-grid', 'view-filter'],
    fillbrush: ['symmetry', 'radial-symmetry', 'canvas-grid', 'view-filter'],
    pen: ['french-curve', 'canvas-grid', 'view-filter'],
    line: ['french-curve', 'canvas-grid'],
    rect: ['canvas-grid', 'view-filter'],
    ellipse: ['french-curve', 'canvas-grid'],
    eraser: ['out-of-pegs', 'canvas-grid'],
    select: ['vector-sculpt', 'tween-motion-path', 'canvas-grid'],
    subselect: ['vector-sculpt'],
    hand: ['canvas-grid'],
  };
  // Tools armed by a held key regardless of the active tool (flip-roll=R,
  // mirror-check=M, lagoon-menu=Q) — always available, shown after a
  // divider so they read as a separate "always on hand" group rather than
  // being confused for the current tool's own context.
  var ALWAYS = ['flip-roll', 'mirror-check', 'lagoon-menu'];

  var SHORT_NAME = {
    'symmetry': 'Miroir vertical',
    'radial-symmetry': 'Miroir radial (mandala)',
    'predictive-stroke': 'Trait prédictif',
    'multiframe-draw': 'Dessin multi-frames',
    'canvas-grid': 'Grille',
    'view-filter': 'Contrôle des valeurs',
    'french-curve': 'Gabarit courbe (maintenir F)',
    'vector-sculpt': 'Sculpt vectoriel (maintenir W)',
    'tween-motion-path': 'Chemin de tween + barres d\'espacement',
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
    symmetry: '<svg viewBox="0 0 24 24" width="15" height="15" fill="currentColor"><rect x="11" y="2" width="2" height="20" rx="1"/><path d="M9 6L3 12L9 18Z"/><path d="M15 6L21 12L15 18Z"/></svg>',
    'radial-symmetry': '<svg viewBox="0 0 24 24" width="15" height="15" fill="currentColor"><circle cx="12" cy="12" r="2.6"/><rect x="10.8" y="1.5" width="2.4" height="7" rx="1.2"/><rect x="10.8" y="1.5" width="2.4" height="7" rx="1.2" transform="rotate(60 12 12)"/><rect x="10.8" y="1.5" width="2.4" height="7" rx="1.2" transform="rotate(120 12 12)"/><rect x="10.8" y="1.5" width="2.4" height="7" rx="1.2" transform="rotate(180 12 12)"/><rect x="10.8" y="1.5" width="2.4" height="7" rx="1.2" transform="rotate(240 12 12)"/><rect x="10.8" y="1.5" width="2.4" height="7" rx="1.2" transform="rotate(300 12 12)"/></svg>',
    'predictive-stroke': '<svg viewBox="0 0 24 24" width="15" height="15" fill="currentColor"><path d="M12 2l1.2 3.8L17 7l-3.8 1.2L12 12l-1.2-3.8L7 7l3.8-1.2L12 2z"/><path d="M19 13l.6 1.9L21.5 15.5l-1.9.6L19 18l-.6-1.9L16.5 15.5l1.9-.6L19 13z"/><path d="M5 15l.5 1.5L7 17l-1.5.5L5 19l-.5-1.5L3 17l1.5-.5L5 15z"/></svg>',
    'multiframe-draw': '<svg viewBox="0 0 24 24" width="15" height="15" fill="currentColor"><path d="M12 2L22 7L12 12L2 7Z"/><path d="M2 12L12 17L22 12L22 14L12 19L2 14Z" opacity=".55"/></svg>',
    'canvas-grid': '<svg viewBox="0 0 24 24" width="15" height="15" fill="currentColor"><rect x="3" y="3" width="7" height="7" rx="1"/><rect x="14" y="3" width="7" height="7" rx="1"/><rect x="3" y="14" width="7" height="7" rx="1"/><rect x="14" y="14" width="7" height="7" rx="1"/></svg>',
    'view-filter': '<svg viewBox="0 0 24 24" width="15" height="15" fill="currentColor"><path fill-rule="evenodd" clip-rule="evenodd" d="M12 21a9 9 0 100-18 9 9 0 000 18zm0-2V5a7 7 0 010 14z"/></svg>',
    'french-curve': '<svg viewBox="0 0 24 24" width="15" height="15" fill="currentColor"><path fill-rule="evenodd" clip-rule="evenodd" d="M12 6c6 0 10 2.7 10 6s-4 6-10 6S2 15.3 2 12s4-6 10-6zm0 2.2c-4.9 0-7.8 1.8-7.8 3.8s2.9 3.8 7.8 3.8 7.8-1.8 7.8-3.8-2.9-3.8-7.8-3.8z"/></svg>',
    'vector-sculpt': '<svg viewBox="0 0 24 24" width="15" height="15" fill="currentColor"><path d="M2.5 20.5c5.5.5 7-8 10.3-13.6l2.7 1.8c-4 5.4-5.7 12.6-11.4 12.6z"/><path d="M14.2 4.6l6.3 1-2 6.3z"/></svg>',
    'out-of-pegs': '<svg viewBox="0 0 24 24" width="15" height="15" fill="currentColor"><circle cx="9" cy="12" r="6" opacity=".45"/><circle cx="15" cy="12" r="6"/></svg>',
    'tween-motion-path': '<svg viewBox="0 0 24 24" width="15" height="15" fill="currentColor"><circle cx="3.5" cy="20.5" r="2.2"/><circle cx="20.5" cy="3.5" r="2.2"/><path d="M4.5 19.3L8 15.6M9.6 13.9L12.3 11.1M13.9 9.6L16.6 6.8M18.2 5.1L19.4 4" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"/><path d="M6.5 14.4l2.2 2.2M11 9.9l2.2 2.2M15.5 5.4l2.2 2.2" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"/></svg>',
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
    'radial-symmetry': [{ key: 'nemo-labs-radial-sectors', label: 'Secteurs', def: 6, min: 2, max: 16, step: 1, set: function (v) { window.SMLabs.setRadialSectors(v); } }],
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

  function buildGroup(names, registered) {
    return names.filter(function (n) { return registered.hasOwnProperty(n); });
  }

  function renderLabsFloatPanel() {
    var panel = document.getElementById('labs-float-panel');
    var iconsWrap = document.getElementById('labs-float-icons');
    var paramsWrap = document.getElementById('labs-float-params');
    if (!panel || !iconsWrap || !paramsWrap || !window.SMLabs || !window.state) return;
    var registered = {};
    SMLabs.list().forEach(function (p) { registered[p.name] = p.on; });
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
        window.SMLabs.toggle(n);
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
    // shown tool that's both ON and has a PARAMS entry.
    paramsWrap.innerHTML = '';
    var anyParams = false;
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
