// ---- STORYBOARD MODE (2026-07) ----
// Node-space montage editor — the third app mode next to Animation 2D and
// Motion. The timeline grid is replaced by a free pan/zoom space holding
// draggable MODULES (the Rive-state-machine feel, but for editing):
//
//   - instance modules: one per placed COMPONENT instance (state.symbols —
//     the same symbol system component layers use; double-click opens the
//     symbol's own timeline via SM.enterSymbol, exactly like a component
//     layer's double-click). Several modules may instance the SAME symbol.
//   - montage modules (v1 scope, task 2/4): a horizontal strip instances
//     snap INTO, becoming an ordered, retimable editing sequence.
//   - sound modules (task 4/4): audio with waveform, splittable, snapping
//     onto a montage as its embedded track.
//
// Data model — plain JSON, persisted wholesale through exportJSON (no
// runtime-only fields in `modules`, by construction):
//   state.storyboard = {
//     pan: {x, y}, zoom: 1, nextId: 1, activeMontageId: null,
//     modules: [
//       { id, type:'instance', symbolId, x, y },
//       { id, type:'montage', name, x, y,
//         items: [{ symbolId, trimIn, trimOut, stretch }], audio: [], playhead: 0 },
//     ],
//   }
(function () {
  var SNAP_PX = 10; // world-space edge-snap distance while dragging

  function sb() {
    if (!state.storyboard) {
      state.storyboard = { pan: { x: 40, y: 40 }, zoom: 1, nextId: 1, activeMontageId: null, modules: [] };
    }
    return state.storyboard;
  }
  function newId() { var s = sb(); return 'sbm' + (s.nextId++); }

  // Stable per-symbol color (the mock shows plain colored rects as the
  // instance preview) — hash the symbol id onto a fixed pleasant palette
  // so every instance of the same component shares a color.
  var PALETTE = ['#c0504d', '#4f81bd', '#4bac7e', '#8064a2', '#d19a3f', '#5e6ad2', '#9f5fa8', '#3f9aa8'];
  function symbolColor(symbolId) {
    var h = 0;
    for (var i = 0; i < symbolId.length; i++) h = (h * 31 + symbolId.charCodeAt(i)) >>> 0;
    return PALETTE[h % PALETTE.length];
  }

  // ---- DOM ----
  var space = null, world = null;
  function ensureDom() {
    if (space) return;
    var tl = document.getElementById('tl-content');
    if (!tl) return;
    space = document.createElement('div');
    space.id = 'storyboard-space';
    world = document.createElement('div');
    world.id = 'sb-world';
    space.appendChild(world);
    tl.appendChild(space);
    wireSpace();
  }

  function applyView() {
    var s = sb();
    world.style.transform = 'translate(' + s.pan.x + 'px,' + s.pan.y + 'px) scale(' + s.zoom + ')';
  }
  function toWorld(clientX, clientY) {
    var s = sb();
    var r = space.getBoundingClientRect();
    return { x: (clientX - r.left - s.pan.x) / s.zoom, y: (clientY - r.top - s.pan.y) / s.zoom };
  }

  function setVisible(on) {
    ensureDom();
    if (!space) return;
    space.style.display = on ? 'block' : 'none';
    // The regular timeline chrome shares #tl-content — swap wholesale.
    var lp = document.getElementById('layer-panel');
    var lpr = document.getElementById('layer-panel-resize');
    var fg = document.getElementById('fg-wrap');
    [lp, lpr, fg].forEach(function (el) { if (el) el.style.display = on ? 'none' : ''; });
    if (on) { applyView(); render(); }
  }

  // ---- module rendering ----
  function render() {
    if (!world) return;
    world.innerHTML = '';
    sb().modules.forEach(function (m) {
      var el = m.type === 'instance' ? renderInstance(m) : m.type === 'montage' ? renderMontage(m) : null;
      if (!el) return;
      el.style.left = m.x + 'px';
      el.style.top = m.y + 'px';
      el.dataset.sbId = m.id;
      world.appendChild(el);
    });
  }

  function renderInstance(m) {
    var el = document.createElement('div');
    el.className = 'sb-module sb-instance';
    var sym = state.symbols[m.symbolId];
    var card = document.createElement('div');
    card.className = 'sb-thumb';
    card.style.background = symbolColor(m.symbolId);
    el.appendChild(card);
    var side = document.createElement('div');
    side.className = 'sb-side';
    var edit = document.createElement('div');
    edit.className = 'sb-edit';
    edit.title = 'Éditer l’animation du composant';
    edit.innerHTML = '<svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.8"><path d="M4 6h9M4 12h6M4 18h7"/><path d="m14.5 15.5 5-5 2 2-5 5-2.6.6z"/></svg>';
    edit.addEventListener('click', function (e) { e.stopPropagation(); openSymbol(m); });
    side.appendChild(edit);
    el.appendChild(side);
    var nm = document.createElement('div');
    nm.className = 'sb-name';
    nm.textContent = sym ? sym.name : '(composant supprimé)';
    el.appendChild(nm);
    el.addEventListener('dblclick', function (e) { e.stopPropagation(); openSymbol(m); });
    wireModuleDrag(el, m);
    wireModuleMenu(el, m);
    return el;
  }

  // Montage strip — v1 shell only in this task (an empty drop target with a
  // film icon + name); items/retiming/playback land in task 2/4.
  function renderMontage(m) {
    var el = document.createElement('div');
    el.className = 'sb-module sb-montage' + (sb().activeMontageId === m.id ? ' active' : '');
    var head = document.createElement('div');
    head.className = 'sb-montage-head';
    head.innerHTML = '<svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.8"><rect x="3" y="5" width="18" height="14" rx="2"/><path d="M7 5v14M17 5v14M3 10h4M3 14h4M17 10h4M17 14h4"/></svg>';
    var nm = document.createElement('span');
    nm.textContent = m.name;
    head.appendChild(nm);
    el.appendChild(head);
    var lane = document.createElement('div');
    lane.className = 'sb-montage-lane';
    if (!m.items.length) {
      var hint = document.createElement('span');
      hint.className = 'sb-hint';
      hint.textContent = 'Glisser des instances ici';
      lane.appendChild(hint);
    }
    el.appendChild(lane);
    el.addEventListener('click', function () {
      sb().activeMontageId = m.id;
      render();
    });
    wireModuleDrag(el, m);
    wireModuleMenu(el, m);
    return el;
  }

  function openSymbol(m) {
    if (!state.symbols[m.symbolId]) { showToast('Ce composant n’existe plus'); return; }
    // enterSymbol lives in the Animation 2D world — switch modes first so
    // the user lands in the symbol's editable timeline, matching the mock
    // ("double clic … ouvre l’onglet d’instance d’animation 2D").
    if (window.SMMotion) SMMotion.setAppMode('anim2d');
    window.SM.enterSymbol(m.symbolId);
  }

  // ---- dragging + edge snap ----
  function wireModuleDrag(el, m) {
    el.addEventListener('pointerdown', function (e) {
      if (e.button !== 0) return;
      e.stopPropagation();
      var start = toWorld(e.clientX, e.clientY);
      var ox = start.x - m.x, oy = start.y - m.y;
      var moved = false;
      function mv(ev) {
        var p = toWorld(ev.clientX, ev.clientY);
        m.x = p.x - ox; m.y = p.y - oy;
        moved = true;
        snapToNeighbors(m, el);
        el.style.left = m.x + 'px';
        el.style.top = m.y + 'px';
      }
      function up() {
        document.removeEventListener('pointermove', mv);
        document.removeEventListener('pointerup', up);
        if (moved) render(); // settle snapped position for everyone
      }
      document.addEventListener('pointermove', mv);
      document.addEventListener('pointerup', up);
    });
  }

  // Edge snap: while dragging, if one of this module's edges comes within
  // SNAP_PX (world units) of a neighbor's opposite edge — and they overlap
  // on the other axis — glue them flush. Left/right edges AND top/bottom,
  // so instances can line up in rows or stack, the mock's side-by-side look.
  function snapToNeighbors(m, el) {
    var w = el.offsetWidth, h = el.offsetHeight;
    sb().modules.forEach(function (o) {
      if (o.id === m.id) return;
      var oEl = world.querySelector('[data-sb-id="' + o.id + '"]');
      if (!oEl) return;
      var ow = oEl.offsetWidth, oh = oEl.offsetHeight;
      var vOverlap = m.y < o.y + oh && m.y + h > o.y;
      var hOverlap = m.x < o.x + ow && m.x + w > o.x;
      if (vOverlap) {
        if (Math.abs(m.x - (o.x + ow)) < SNAP_PX) m.x = o.x + ow;         // my left to their right
        else if (Math.abs((m.x + w) - o.x) < SNAP_PX) m.x = o.x - w;      // my right to their left
        if (Math.abs(m.y - o.y) < SNAP_PX) m.y = o.y;                     // top alignment while side-snapped
      }
      if (hOverlap) {
        if (Math.abs(m.y - (o.y + oh)) < SNAP_PX) m.y = o.y + oh;
        else if (Math.abs((m.y + h) - o.y) < SNAP_PX) m.y = o.y - h;
      }
    });
  }

  // ---- module / space context menus ----
  function wireModuleMenu(el, m) {
    el.addEventListener('contextmenu', function (e) {
      e.preventDefault(); e.stopPropagation();
      var menu = [];
      if (m.type === 'instance') {
        menu.push({ label: 'Éditer l’animation', action: function () { openSymbol(m); } });
      }
      if (m.type === 'montage') {
        menu.push({ label: 'Renommer', action: function () { var v = prompt('Nom du montage', m.name); if (v) { m.name = v; render(); } } });
      }
      menu.push({ label: 'Supprimer le module', action: function () { var s = sb(); s.modules.splice(s.modules.indexOf(m), 1); if (s.activeMontageId === m.id) s.activeMontageId = null; render(); } });
      window.showContextMenu(e.clientX, e.clientY, menu);
    });
  }

  function wireSpace() {
    // Pan: drag the empty background.
    space.addEventListener('pointerdown', function (e) {
      if (e.button !== 0 || e.target.closest('.sb-module')) return;
      var s = sb();
      var sx = e.clientX - s.pan.x, sy = e.clientY - s.pan.y;
      function mv(ev) { s.pan.x = ev.clientX - sx; s.pan.y = ev.clientY - sy; applyView(); }
      function up() { document.removeEventListener('pointermove', mv); document.removeEventListener('pointerup', up); }
      document.addEventListener('pointermove', mv);
      document.addEventListener('pointerup', up);
    });
    // Zoom: wheel, anchored on the cursor.
    space.addEventListener('wheel', function (e) {
      e.preventDefault();
      var s = sb();
      var before = toWorld(e.clientX, e.clientY);
      s.zoom = Math.max(0.25, Math.min(3, s.zoom * (e.deltaY < 0 ? 1.1 : 0.9)));
      var r = space.getBoundingClientRect();
      s.pan.x = e.clientX - r.left - before.x * s.zoom;
      s.pan.y = e.clientY - r.top - before.y * s.zoom;
      applyView();
    }, { passive: false });
    // Create modules: right-click empty space.
    space.addEventListener('contextmenu', function (e) {
      if (e.target.closest('.sb-module')) return;
      e.preventDefault();
      var p = toWorld(e.clientX, e.clientY);
      var menu = [];
      var symIds = Object.keys(state.symbols || {});
      symIds.forEach(function (id) {
        menu.push({ label: 'Instance : ' + state.symbols[id].name, action: function () { addInstance(id, p.x, p.y); } });
      });
      if (!symIds.length) menu.push({ label: '(Aucun composant — créez-en un en Animation 2D)', disabled: true, action: function () {} });
      menu.push({ sep: true });
      menu.push({ label: 'Nouveau montage', action: function () { addMontage(p.x, p.y); } });
      window.showContextMenu(e.clientX, e.clientY, menu);
    });
  }

  function addInstance(symbolId, x, y) {
    sb().modules.push({ id: newId(), type: 'instance', symbolId: symbolId, x: Math.round(x), y: Math.round(y) });
    render();
  }
  function addMontage(x, y) {
    var s = sb();
    var m = { id: newId(), type: 'montage', name: 'Montage ' + (s.modules.filter(function (x2) { return x2.type === 'montage'; }).length + 1), x: Math.round(x), y: Math.round(y), items: [], audio: [], playhead: 0 };
    s.modules.push(m);
    s.activeMontageId = m.id;
    render();
  }

  window.SMStoryboard = {
    setVisible: setVisible,
    render: render,
    addInstance: addInstance,
    addMontage: addMontage,
  };
})();
