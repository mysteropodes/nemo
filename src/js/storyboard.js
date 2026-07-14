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
    if (!on) {
      stopMontagePlay();
      if (previewLayer) previewLayer.removeChildren();
    }
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

  // ---- montage time model ----
  // item = { symbolId, trimIn, trimOut, duration } (all frames):
  //   trimIn/trimOut  = the SOURCE range played from the component
  //   duration        = how many montage frames the item occupies —
  //                     equal to the source range for plain trims, different
  //                     after a STRETCH (Alt+drag: same source range squeezed
  //                     into fewer/more frames = speed change).
  var FPP = 1.5; // montage lane: pixels per frame
  function symbolDuration(symbolId) {
    var sym = state.symbols[symbolId];
    return sym ? (sym.totalFrames || (sym.layers[0] && sym.layers[0].frames.length) || 24) : 24;
  }
  function montageTotal(m) { return m.items.reduce(function (a, it) { return a + it.duration; }, 0); }
  function montageStrokesAt(m, f) {
    var acc = 0;
    for (var i = 0; i < m.items.length; i++) {
      var it = m.items[i];
      if (f < acc + it.duration) {
        var local = f - acc;
        var srcLen = it.trimOut - it.trimIn + 1;
        var srcFrame = it.trimIn + Math.min(srcLen - 1, Math.floor(local * srcLen / it.duration));
        return symbolStrokesAt(it.symbolId, srcFrame);
      }
      acc += it.duration;
    }
    return [];
  }
  // All layers of the symbol at frame fi, with the same previous-keyframe
  // backfill getLFSSubStrokes uses (a non-key frame shows the last key).
  function symbolStrokesAt(symbolId, fi) {
    var sym = state.symbols[symbolId];
    if (!sym) return [];
    var out = [];
    sym.layers.forEach(function (sl) {
      var idx = Math.min(fi, sl.frames.length - 1);
      var fr = sl.frames[idx];
      if (fr && (fr.isKeyframe || fr.isInterpolated)) { out = out.concat(fr.strokes || []); return; }
      for (var k = idx - 1; k >= 0; k--) {
        var f2 = sl.frames[k];
        if (f2 && f2.isKeyframe) { out = out.concat(f2.strokes || []); return; }
      }
    });
    return out;
  }

  // ---- canvas preview (service Paper layer, ghostAllLayer pattern —
  // engine-bridge's buildSceneJson swaps the document layers for this one
  // in storyboard mode, reading it through onionLayerItems) ----
  var previewLayer = null;
  function getPreviewLayer() {
    var s = sb();
    var m = s.modules.find(function (x) { return x.id === s.activeMontageId; });
    if (!m || !m.items || !m.items.length) return null;
    return previewLayer;
  }
  function updatePreview() {
    var s = sb();
    var m = s.modules.find(function (x) { return x.id === s.activeMontageId; });
    if (!previewLayer && window.project) {
      var prev = project.activeLayer;
      previewLayer = new Layer({ insert: true });
      previewLayer.visible = false; // engine-only — Paper's own (hidden) canvas never draws it
      if (prev) prev.activate();
    }
    if (!previewLayer) return;
    previewLayer.removeChildren();
    if (m && m.items.length) {
      var strokes = montageStrokesAt(m, m.playhead || 0);
      strokes.forEach(function (sd) { desP(sd, previewLayer); });
    }
    window._sceneVersion++;
    if (window.SMEngineBridge && SMEngineBridge.isEnabled()) SMEngineBridge.renderNow();
  }

  // ---- montage playback (self-contained wall-clock rAF, the same
  // frame-dropping principle timeline.js's startPlay uses — deliberately
  // NOT the global transport: a montage plays inside the node space
  // without touching state.currentFrame or the 2D timeline) ----
  var _playRaf = null, _playingMontageId = null;
  function stopMontagePlay() {
    if (_playRaf) cancelAnimationFrame(_playRaf);
    _playRaf = null; _playingMontageId = null;
  }
  function toggleMontagePlay(m) {
    if (_playingMontageId === m.id) { stopMontagePlay(); render(); return; }
    stopMontagePlay();
    _playingMontageId = m.id;
    var fps = state.fps || 24, frameMs = 1000 / fps;
    var clock = performance.now();
    function step(now) {
      if (_playingMontageId !== m.id) return;
      var steps = Math.floor((now - clock) / frameMs);
      if (steps > 0) {
        clock += steps * frameMs;
        var total = montageTotal(m);
        if (!total) { stopMontagePlay(); render(); return; }
        m.playhead = ((m.playhead || 0) + steps) % total; // loop
        positionPlayhead(m);
        updatePreview();
      }
      _playRaf = requestAnimationFrame(step);
    }
    _playRaf = requestAnimationFrame(step);
    render();
  }
  function positionPlayhead(m) {
    var el = world && world.querySelector('[data-sb-id="' + m.id + '"] .sb-ph');
    if (el) el.style.left = ((m.playhead || 0) * FPP) + 'px';
  }

  // Montage strip: ordered chips (width = duration), trim handles, stretch
  // (Alt+drag right handle), drag-to-reorder, scrubbable playhead, play.
  function renderMontage(m) {
    var el = document.createElement('div');
    el.className = 'sb-module sb-montage' + (sb().activeMontageId === m.id ? ' active' : '');
    var head = document.createElement('div');
    head.className = 'sb-montage-head';
    head.innerHTML = '<svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.8"><rect x="3" y="5" width="18" height="14" rx="2"/><path d="M7 5v14M17 5v14M3 10h4M3 14h4M17 10h4M17 14h4"/></svg>';
    var nm = document.createElement('span');
    nm.textContent = m.name + ' — ' + montageTotal(m) + ' f';
    head.appendChild(nm);
    var play = document.createElement('span');
    play.className = 'sb-play';
    play.title = 'Lire / arrêter le montage';
    play.textContent = _playingMontageId === m.id ? '◼' : '▶';
    play.addEventListener('pointerdown', function (e) { e.stopPropagation(); });
    play.addEventListener('click', function (e) { e.stopPropagation(); sb().activeMontageId = m.id; toggleMontagePlay(m); updatePreview(); });
    head.appendChild(play);
    el.appendChild(head);
    // The montage moves by its HEAD only — the lane's own pointer events
    // are scrub/retime gestures, they must never drag the whole module.
    wireModuleDrag(head, m);

    var lane = document.createElement('div');
    lane.className = 'sb-montage-lane';
    lane.style.width = Math.max(240, montageTotal(m) * FPP + 8) + 'px';
    if (!m.items.length) {
      var hint = document.createElement('span');
      hint.className = 'sb-hint';
      hint.textContent = 'Glisser des instances ici';
      lane.appendChild(hint);
    }
    m.items.forEach(function (it, idx) { lane.appendChild(renderChip(m, it, idx)); });
    // playhead
    var ph = document.createElement('div');
    ph.className = 'sb-ph';
    ph.style.left = ((m.playhead || 0) * FPP) + 'px';
    lane.appendChild(ph);
    // scrub: drag on the lane background (not a chip)
    lane.addEventListener('pointerdown', function (e) {
      if (e.target.closest('.sb-chip')) return;
      e.stopPropagation();
      sb().activeMontageId = m.id;
      function scrub(ev) {
        var r = lane.getBoundingClientRect();
        var f = Math.round((ev.clientX - r.left) / (FPP * sb().zoom));
        m.playhead = Math.max(0, Math.min(Math.max(0, montageTotal(m) - 1), f));
        positionPlayhead(m);
        updatePreview();
      }
      scrub(e);
      function up() { document.removeEventListener('pointermove', scrub); document.removeEventListener('pointerup', up); render(); }
      document.addEventListener('pointermove', scrub);
      document.addEventListener('pointerup', up);
    });
    el.appendChild(lane);
    el.addEventListener('click', function () {
      if (sb().activeMontageId !== m.id) { sb().activeMontageId = m.id; render(); updatePreview(); }
    });
    wireModuleMenu(el, m);
    return el;
  }

  function renderChip(m, it, idx) {
    var chip = document.createElement('div');
    chip.className = 'sb-chip';
    chip.style.width = (it.duration * FPP) + 'px';
    chip.style.background = symbolColor(it.symbolId);
    var sym = state.symbols[it.symbolId];
    var srcLen = it.trimOut - it.trimIn + 1;
    var stretched = it.duration !== srcLen;
    chip.title = (sym ? sym.name : '?') + ' — ' + it.duration + ' f' + (stretched ? ' (vitesse ×' + (srcLen / it.duration).toFixed(2) + ')' : '');
    var lbl = document.createElement('span');
    lbl.textContent = it.duration + (stretched ? '×' : '');
    chip.appendChild(lbl);

    // Trim handles (edges). Plain drag = TRIM (source range and duration
    // move together — speed unchanged); Alt+drag on the RIGHT handle =
    // STRETCH (duration alone changes — the same source range plays
    // faster/slower). "Les deux" per the spec Q&A.
    ['left', 'right'].forEach(function (side) {
      var h = document.createElement('div');
      h.className = 'sb-trim ' + side;
      h.title = side === 'right' ? 'Trim — Alt+glisser : étirer (vitesse)' : 'Trim';
      h.addEventListener('pointerdown', function (e) {
        e.stopPropagation(); e.preventDefault();
        var startX = e.clientX, o = { trimIn: it.trimIn, trimOut: it.trimOut, duration: it.duration };
        var stretch = side === 'right' && e.altKey;
        var maxLen = symbolDuration(it.symbolId);
        function mv(ev) {
          var df = Math.round((ev.clientX - startX) / (FPP * sb().zoom));
          if (stretch) {
            it.duration = Math.max(1, o.duration + df);
          } else if (side === 'left') {
            var ti = Math.max(0, Math.min(o.trimOut, o.trimIn + df));
            var d = ti - o.trimIn;
            it.trimIn = ti;
            it.duration = Math.max(1, o.duration - d);
          } else {
            var to = Math.min(maxLen - 1, Math.max(o.trimIn, o.trimOut + df));
            var d2 = to - o.trimOut;
            it.trimOut = to;
            it.duration = Math.max(1, o.duration + d2);
          }
          chip.style.width = (it.duration * FPP) + 'px';
        }
        function up() {
          document.removeEventListener('pointermove', mv);
          document.removeEventListener('pointerup', up);
          render(); updatePreview();
        }
        document.addEventListener('pointermove', mv);
        document.addEventListener('pointerup', up);
      });
      chip.appendChild(h);
    });

    // Body drag = reorder within the lane (swap when crossing a neighbor's
    // midpoint — the montage list stays the single source of order).
    chip.addEventListener('pointerdown', function (e) {
      if (e.target.classList.contains('sb-trim')) return;
      e.stopPropagation();
      var curIdx = m.items.indexOf(it);
      function mv(ev) {
        var siblings = Array.from(chip.parentElement.querySelectorAll('.sb-chip'));
        var over = siblings.findIndex(function (c) {
          var r = c.getBoundingClientRect();
          return ev.clientX >= r.left && ev.clientX <= r.right;
        });
        if (over >= 0 && over !== curIdx) {
          m.items.splice(curIdx, 1);
          m.items.splice(over, 0, it);
          curIdx = over;
          render();
        }
      }
      function up() { document.removeEventListener('pointermove', mv); document.removeEventListener('pointerup', up); updatePreview(); }
      document.addEventListener('pointermove', mv);
      document.addEventListener('pointerup', up);
    });

    chip.addEventListener('contextmenu', function (e) {
      e.preventDefault(); e.stopPropagation();
      window.showContextMenu(e.clientX, e.clientY, [
        { label: 'Détacher (redevient un module libre)', action: function () { m.items.splice(m.items.indexOf(it), 1); addInstance(it.symbolId, m.x, m.y + 120); updatePreview(); } },
        { label: 'Réinitialiser le retiming', action: function () { it.trimIn = 0; it.trimOut = symbolDuration(it.symbolId) - 1; it.duration = it.trimOut + 1; render(); updatePreview(); } },
        { label: 'Retirer du montage', action: function () { m.items.splice(m.items.indexOf(it), 1); render(); updatePreview(); } },
      ]);
    });
    return chip;
  }

  // Dropping a free instance module onto a montage lane absorbs it into
  // the sequence at the pointer's position — the free module is consumed
  // (detaching from the chip's context menu recreates one).
  function tryDropIntoMontage(m, clientX, clientY) {
    if (m.type !== 'instance') return false;
    var lanes = world.querySelectorAll('.sb-montage .sb-montage-lane');
    for (var i = 0; i < lanes.length; i++) {
      var r = lanes[i].getBoundingClientRect();
      if (clientX >= r.left && clientX <= r.right && clientY >= r.top && clientY <= r.bottom) {
        var mid = lanes[i].closest('.sb-module').dataset.sbId;
        var montage = sb().modules.find(function (x) { return x.id === mid; });
        if (!montage) return false;
        var dur = symbolDuration(m.symbolId);
        var item = { symbolId: m.symbolId, trimIn: 0, trimOut: dur - 1, duration: dur };
        // insertion index from pointer x against existing chips
        var chips = lanes[i].querySelectorAll('.sb-chip');
        var at = montage.items.length;
        for (var c = 0; c < chips.length; c++) {
          var cr = chips[c].getBoundingClientRect();
          if (clientX < cr.left + cr.width / 2) { at = c; break; }
        }
        montage.items.splice(at, 0, item);
        var s = sb();
        s.modules.splice(s.modules.indexOf(m), 1);
        s.activeMontageId = montage.id;
        render(); updatePreview();
        return true;
      }
    }
    return false;
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
      var moved = false, lastX = e.clientX, lastY = e.clientY;
      function mv(ev) {
        lastX = ev.clientX; lastY = ev.clientY;
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
        if (!moved) return;
        // Released over a montage lane? The instance is absorbed into the
        // sequence there instead of staying a free module.
        if (tryDropIntoMontage(m, lastX, lastY)) return;
        render(); // settle snapped position for everyone
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
        // The link between the three modes: a montage becomes a LAYER in
        // Animation 2D/Motion (spec: "apparaîtra comme un layer dans les
        // autres timelines"). The montage stays the source of truth —
        // getEffectiveStrokes resolves the layer's content from it live
        // (app.js ld.montageId branch), like a precomp.
        menu.push({ label: 'Placer comme calque dans Animation 2D', action: function () { placeMontageAsLayer(m); } });
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

  function montageById(id) {
    return sb().modules.find(function (x) { return x.type === 'montage' && x.id === id; }) || null;
  }
  function placeMontageAsLayer(m) {
    if (!m.items.length) { showToast('Le montage est vide — glissez-y des instances d\u2019abord'); return; }
    // One layer per montage: re-placing focuses the existing one instead
    // of stacking duplicates.
    for (var i = 0; i < state.layers.length; i++) {
      if (state.layers[i].montageId === m.id) {
        if (window.SMMotion) SMMotion.setAppMode('anim2d');
        state.activeLayerIdx = i; activateUL(i); updateUI();
        showToast('Ce montage est déjà placé — calque « ' + state.layers[i].name + ' »');
        return;
      }
    }
    if (window.saveAllLayerFrames) saveAllLayerFrames();
    if (window.pushUndoLayers) pushUndoLayers();
    var idx = createUserLayer(m.name);
    state.layers[idx].montageId = m.id;
    if (window.SMMotion) SMMotion.setAppMode('anim2d');
    state.activeLayerIdx = idx; activateUL(idx);
    loadFrame(state.currentFrame); updateUI();
    showToast('Montage placé comme calque « ' + m.name + ' » (' + montageTotal(m) + ' images, en boucle)');
  }

  window.SMStoryboard = {
    setVisible: setVisible,
    render: render,
    addInstance: addInstance,
    addMontage: addMontage,
    getPreviewLayer: getPreviewLayer,
    updatePreview: updatePreview,
    montageStrokesAt: montageStrokesAt,
    montageTotal: montageTotal,
    montageById: montageById,
  };
})();
