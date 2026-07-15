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
    select: ['vector-sculpt', 'canvas-grid'],
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
    'out-of-pegs': 'Décalage des fantômes onion',
    'flip-roll': "Rouleau d'animateur (maintenir R)",
    'mirror-check': 'Miroir de contrôle (maintenir M)',
    'lagoon-menu': "Menu radial d'outils (maintenir Q)",
  };

  var ICONS = {
    symmetry: '<svg viewBox="0 0 24 24" width="15" height="15"><line x1="12" y1="2" x2="12" y2="22" stroke="currentColor" stroke-width="1.4" stroke-dasharray="2,2"/><path d="M10 6 L4 12 L10 18 Z" fill="currentColor"/><path d="M14 6 L20 12 L14 18 Z" fill="currentColor"/></svg>',
    'radial-symmetry': '<svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="1.4"><circle cx="12" cy="12" r="3"/><line x1="12" y1="12" x2="12" y2="2"/><line x1="12" y1="12" x2="20" y2="7"/><line x1="12" y1="12" x2="20" y2="17"/><line x1="12" y1="12" x2="12" y2="22"/><line x1="12" y1="12" x2="4" y2="17"/><line x1="12" y1="12" x2="4" y2="7"/></svg>',
    'predictive-stroke': '<svg viewBox="0 0 24 24" width="15" height="15" fill="currentColor"><path d="M12 2l1.2 3.8L17 7l-3.8 1.2L12 12l-1.2-3.8L7 7l3.8-1.2L12 2z"/><path d="M19 13l.6 1.9L21.5 15.5l-1.9.6L19 18l-.6-1.9L16.5 15.5l1.9-.6L19 13z"/><path d="M5 15l.5 1.5L7 17l-1.5.5L5 19l-.5-1.5L3 17l1.5-.5L5 15z"/></svg>',
    'multiframe-draw': '<svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="1.4"><rect x="3" y="7" width="12" height="10" rx="1"/><rect x="8" y="3" width="12" height="10" rx="1"/></svg>',
    'canvas-grid': '<svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="1.4"><rect x="3" y="3" width="18" height="18"/><line x1="9" y1="3" x2="9" y2="21"/><line x1="15" y1="3" x2="15" y2="21"/><line x1="3" y1="9" x2="21" y2="9"/><line x1="3" y1="15" x2="21" y2="15"/></svg>',
    'view-filter': '<svg viewBox="0 0 24 24" width="15" height="15"><circle cx="12" cy="12" r="9" fill="none" stroke="currentColor" stroke-width="1.4"/><path d="M12 3a9 9 0 010 18z" fill="currentColor"/></svg>',
    'french-curve': '<svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="1.4"><ellipse cx="12" cy="12" rx="9" ry="5"/></svg>',
    'vector-sculpt': '<svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"><path d="M3 18 Q9 18 12 10 Q15 4 21 4"/><path d="M12 10 L15 8 M12 10 L14 13"/></svg>',
    'out-of-pegs': '<svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="1.4"><circle cx="9" cy="12" r="6" opacity=".5"/><circle cx="15" cy="12" r="6"/></svg>',
    'flip-roll': '<svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"><path d="M4 12a8 8 0 0114-5.3M20 12a8 8 0 01-14 5.3"/><path d="M18 4v3h-3M6 20v-3h3"/></svg>',
    'mirror-check': '<svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="1.4"><rect x="3" y="5" width="18" height="14" rx="1"/><path d="M8 12h8M8 12l2-2M8 12l2 2M16 12l-2-2M16 12l-2 2"/></svg>',
    'lagoon-menu': '<svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="1.4"><circle cx="12" cy="12" r="8"/><line x1="12" y1="4" x2="12" y2="12"/><line x1="12" y1="12" x2="18.9" y2="15.5"/><line x1="12" y1="12" x2="5.1" y2="15.5"/></svg>',
  };

  function buildGroup(names, registered) {
    return names.filter(function (n) { return registered.hasOwnProperty(n); });
  }

  function renderLabsFloatPanel() {
    var panel = document.getElementById('labs-float-panel');
    var iconsWrap = document.getElementById('labs-float-icons');
    if (!panel || !iconsWrap || !window.SMLabs || !window.state) return;
    var registered = {};
    SMLabs.list().forEach(function (p) { registered[p.name] = p.on; });
    var ctx = buildGroup(TOOL_CONTEXT[state.tool] || [], registered);
    var always = buildGroup(ALWAYS.filter(function (n) { return ctx.indexOf(n) < 0; }), registered);
    if (!ctx.length && !always.length) { panel.style.display = 'none'; return; }
    panel.style.display = 'flex';
    iconsWrap.innerHTML = '';
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
