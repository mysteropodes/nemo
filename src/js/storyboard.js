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
      var el = m.type === 'instance' ? renderInstance(m) : m.type === 'montage' ? renderMontage(m) : m.type === 'sound' ? renderSound(m) : null;
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
    stopAllAudio();
  }
  // Start every montage audio entry relative to playhead p (frames):
  // already-underway entries start immediately at the right position in
  // their range; not-yet-reached ones are scheduled ahead (WebAudio's own
  // clock keeps them sample-accurate against the visual rAF loop).
  function startMontageAudio(m, p) {
    var fps = state.fps || 24;
    (m.audio || []).forEach(function (a) {
      var key = 'a' + a.aid;
      var relSec = (p - a.offsetFrames) / fps;
      if (relSec >= 0) playRange(key, a.dataB64, a.inSec, a.outSec, 0, relSec);
      else playRange(key, a.dataB64, a.inSec, a.outSec, -relSec, 0);
    });
  }
  function toggleMontagePlay(m) {
    if (_playingMontageId === m.id) { stopMontagePlay(); render(); return; }
    stopMontagePlay();
    _playingMontageId = m.id;
    startMontageAudio(m, m.playhead || 0);
    var fps = state.fps || 24, frameMs = 1000 / fps;
    var clock = performance.now();
    function step(now) {
      if (_playingMontageId !== m.id) return;
      var steps = Math.floor((now - clock) / frameMs);
      if (steps > 0) {
        clock += steps * frameMs;
        var total = montageTotal(m);
        if (!total) { stopMontagePlay(); render(); return; }
        var prevPh = m.playhead || 0;
        m.playhead = (prevPh + steps) % total; // loop
        if (m.playhead < prevPh) { stopAllAudio(); startMontageAudio(m, m.playhead); } // wrapped
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
    // The whole montage body drags (head, padding, edges) — the lane,
    // chips and play button own their gestures and are excluded inside
    // wireModuleDrag itself, so no head-only rule to discover anymore.
    wireModuleDrag(el, m);

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
    // Embedded audio track (spec Q&A: "piste audio DANS le module
    // montage") — one row under the lane, each entry a waveform block at
    // offsetFrames*FPP, horizontally draggable to re-offset, right-click
    // to detach (back to a free sound module) or remove.
    if (m.audio && m.audio.length) {
      var arow = document.createElement('div');
      arow.className = 'sb-audio-row';
      arow.style.width = lane.style.width;
      m.audio.forEach(function (a) {
        var blk = document.createElement('div');
        blk.className = 'sb-audio-blk';
        var durF = Math.max(1, Math.round(((a.outSec - a.inSec) * (state.fps || 24))));
        blk.style.left = (a.offsetFrames * FPP) + 'px';
        blk.style.width = (durF * FPP) + 'px';
        blk.title = a.name + ' — décaler en glissant';
        var acv = document.createElement('canvas');
        acv.width = Math.max(20, durF * FPP); acv.height = 22;
        blk.appendChild(acv);
        decodeAudio('a' + a.aid, a.dataB64, function (buf) { drawWave(acv, buf, a.inSec, a.outSec); });
        blk.addEventListener('pointerdown', function (e) {
          e.stopPropagation(); e.preventDefault();
          var startX = e.clientX, o = a.offsetFrames;
          function mv(ev) {
            a.offsetFrames = Math.max(0, o + Math.round((ev.clientX - startX) / (FPP * sb().zoom)));
            blk.style.left = (a.offsetFrames * FPP) + 'px';
          }
          function up() { document.removeEventListener('pointermove', mv); document.removeEventListener('pointerup', up); }
          document.addEventListener('pointermove', mv);
          document.addEventListener('pointerup', up);
        });
        blk.addEventListener('contextmenu', function (e) {
          e.preventDefault(); e.stopPropagation();
          window.showContextMenu(e.clientX, e.clientY, [
            { label: 'Détacher (redevient un module son)', action: function () { m.audio.splice(m.audio.indexOf(a), 1); sb().modules.push({ id: newId(), type: 'sound', name: a.name, dataB64: a.dataB64, x: m.x, y: m.y + 160, inSec: a.inSec, outSec: a.outSec, cursorSec: null }); render(); } },
            { label: 'Retirer du montage', action: function () { m.audio.splice(m.audio.indexOf(a), 1); render(); } },
          ]);
        });
        arow.appendChild(blk);
      });
      el.appendChild(arow);
    }
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
  // Rewritten after real-use feedback ("le drag des modules est complexe
  // et pas fluide"). What was wrong the first time, in order of felt
  // impact:
  //   1. snapToNeighbors re-queried the DOM (querySelector + offsetWidth)
  //      for EVERY module on EVERY pointermove — forced-layout thrash in
  //      the middle of the gesture, the stutter itself.
  //   2. renderMontage passed its HEAD element as the drag target, so
  //      dragging a montage slid the header INSIDE the module instead of
  //      moving the module. Real bug, never caught because tests only
  //      dragged instances.
  //   3. No pointer capture (a fast drag exiting the window lost its
  //      pointerup), no rAF coalescing (a 240Hz pen outruns the display —
  //      same fix as every other drag in this codebase), no drop-target
  //      feedback (dropping into a montage was invisible until release),
  //      and a full render() blink after every plain move.
  // Now: geometry is cached ONCE at pointerdown (neighbor rects, lane
  // rects in screen space), moves are rAF-coalesced pure math + two style
  // writes, the montage lane under the pointer highlights live
  // (.drop-hint), pointer capture guarantees the release, and a plain
  // move ends with NO re-render (the style already matches the model —
  // only an actual drop rebuilds).
  function wireModuleDrag(el, m) {
    el.addEventListener('pointerdown', function (e) {
      if (e.button !== 0) return;
      // Interactive sub-parts own their gestures — never start a module
      // drag from them. (This is also what makes the WHOLE module bodies
      // safely draggable now, instead of the head-only montage rule.)
      if (e.target.closest('.sb-montage-lane, .sb-chip, .sb-play, .sb-edit, .sb-wave, .sb-audio-blk, .sb-trim')) return;
      e.stopPropagation();
      var start = toWorld(e.clientX, e.clientY);
      var ox = start.x - m.x, oy = start.y - m.y;
      var w = el.offsetWidth, h = el.offsetHeight;
      // ---- one-time geometry snapshot ----
      var neighbors = [];
      sb().modules.forEach(function (o) {
        if (o.id === m.id) return;
        var oe = world.querySelector('[data-sb-id="' + o.id + '"]');
        if (oe) neighbors.push({ x: o.x, y: o.y, w: oe.offsetWidth, h: oe.offsetHeight });
      });
      var lanes = (m.type === 'instance' || m.type === 'sound')
        ? Array.from(world.querySelectorAll('.sb-montage .sb-montage-lane')).filter(function (l) { return !l.closest('[data-sb-id="' + m.id + '"]'); }).map(function (l) { return { el: l, r: l.getBoundingClientRect() }; })
        : [];
      var moved = false, lastX = e.clientX, lastY = e.clientY, raf = 0, hintLane = null;
      // Capture guarantees the pointerup even when a fast drag exits the
      // window — but it can throw (already-released pointer, synthetic
      // events); the drag must survive without it, falling back to the
      // element-scoped listeners below.
      try { el.setPointerCapture(e.pointerId); } catch (err) {}
      el.style.zIndex = 30; // above siblings while in flight
      function apply() {
        raf = 0;
        var p = toWorld(lastX, lastY);
        m.x = p.x - ox; m.y = p.y - oy;
        // edge snap against the cached snapshot — pure math, no DOM reads
        neighbors.forEach(function (o) {
          var vOverlap = m.y < o.y + o.h && m.y + h > o.y;
          var hOverlap = m.x < o.x + o.w && m.x + w > o.x;
          if (vOverlap) {
            if (Math.abs(m.x - (o.x + o.w)) < SNAP_PX) m.x = o.x + o.w;
            else if (Math.abs((m.x + w) - o.x) < SNAP_PX) m.x = o.x - w;
            if (Math.abs(m.y - o.y) < SNAP_PX) m.y = o.y;
          }
          if (hOverlap) {
            if (Math.abs(m.y - (o.y + o.h)) < SNAP_PX) m.y = o.y + o.h;
            else if (Math.abs((m.y + h) - o.y) < SNAP_PX) m.y = o.y - h;
          }
        });
        el.style.left = m.x + 'px';
        el.style.top = m.y + 'px';
        // live drop-target feedback
        var over = null;
        for (var i = 0; i < lanes.length; i++) {
          var r = lanes[i].r;
          if (lastX >= r.left && lastX <= r.right && lastY >= r.top - 20 && lastY <= r.bottom + 40) { over = lanes[i].el; break; }
        }
        if (over !== hintLane) {
          if (hintLane) hintLane.classList.remove('drop-hint');
          if (over) over.classList.add('drop-hint');
          hintLane = over;
        }
      }
      function mv(ev) {
        lastX = ev.clientX; lastY = ev.clientY;
        moved = true;
        if (!raf) raf = requestAnimationFrame(apply);
      }
      function up() {
        document.removeEventListener('pointermove', mv);
        document.removeEventListener('pointerup', up);
        document.removeEventListener('pointercancel', up);
        if (raf) { cancelAnimationFrame(raf); apply(); }
        el.style.zIndex = '';
        if (hintLane) hintLane.classList.remove('drop-hint');
        if (!moved) return;
        // Released over a montage lane? The instance is absorbed into the
        // sequence there — a sound module becomes the montage's audio track.
        if (tryDropIntoMontage(m, lastX, lastY)) return;
        if (tryDropSoundIntoMontage(m, lastX, lastY)) return;
        // plain move: the style already matches the model — nothing to redraw
      }
      // document-scoped: works whether capture engaged or not (captured
      // events retarget to el but still bubble through document).
      document.addEventListener('pointermove', mv);
      document.addEventListener('pointerup', up);
      document.addEventListener('pointercancel', up);
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
      menu.push({ label: 'Importer un son…', action: function () { importSoundAt(p.x, p.y); } });
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

  // ---- sound modules ----
  // { id, type:'sound', name, dataB64, x, y, inSec, outSec, cursorSec }
  // inSec/outSec = the played RANGE — a split produces two modules sharing
  // the same dataB64 with complementary ranges (no audio data duplicated
  // beyond the base64 string reference). Decoded AudioBuffers are runtime
  // state and live in _audioBuffers keyed by module/audio-entry id — the
  // persisted model stays plain JSON by construction.
  var _audioBuffers = {};
  var _audioCtx = null;
  var _playingSources = [];
  function audioCtx() {
    if (!_audioCtx) { var AC = window.AudioContext || window.webkitAudioContext; if (AC) _audioCtx = new AC(); }
    return _audioCtx;
  }
  function decodeAudio(key, dataB64, cb) {
    if (_audioBuffers[key]) { cb(_audioBuffers[key]); return; }
    var c = audioCtx();
    if (!c) return;
    fetch(dataB64).then(function (r) { return r.arrayBuffer(); }).then(function (ab) {
      return c.decodeAudioData(ab);
    }).then(function (buf) {
      _audioBuffers[key] = buf;
      cb(buf);
    }).catch(function (e) { console.warn('[storyboard] audio decode failed', e); });
  }
  // Minimal own peaks renderer (NOT SMAudio's peaksCanvasFor — that one is
  // welded to 2D audio-track objects and caches onto them; this draws any
  // buffer range into any canvas, which split views need).
  function drawWave(cv, buf, inSec, outSec) {
    var ctx = cv.getContext('2d');
    ctx.clearRect(0, 0, cv.width, cv.height);
    ctx.fillStyle = 'rgba(140,150,255,.75)';
    var ch = buf.getChannelData(0);
    var sr = buf.sampleRate;
    var s0 = Math.floor(inSec * sr), s1 = Math.min(ch.length, Math.floor(outSec * sr));
    var mid = cv.height / 2;
    for (var x = 0; x < cv.width; x++) {
      var a = s0 + Math.floor((s1 - s0) * x / cv.width);
      var b = s0 + Math.floor((s1 - s0) * (x + 1) / cv.width);
      var peak = 0;
      for (var i = a; i < b; i += 16) { var v = Math.abs(ch[i] || 0); if (v > peak) peak = v; }
      var h = Math.max(1, peak * mid);
      ctx.fillRect(x, mid - h, 1, h * 2);
    }
  }
  function stopAllAudio() {
    _playingSources.forEach(function (s) { try { s.stop(); } catch (e) {} });
    _playingSources = [];
  }
  function playRange(key, dataB64, inSec, outSec, when, offsetIntoRange) {
    decodeAudio(key, dataB64, function (buf) {
      var c = audioCtx();
      var src = c.createBufferSource();
      src.buffer = buf;
      src.connect(c.destination);
      var start = inSec + (offsetIntoRange || 0);
      if (start >= outSec) return;
      src.start(c.currentTime + (when || 0), start, outSec - start);
      _playingSources.push(src);
    });
  }

  var _soundInput = null;
  function importSoundAt(x, y) {
    if (!_soundInput) {
      _soundInput = document.createElement('input');
      _soundInput.type = 'file';
      _soundInput.accept = 'audio/*';
      _soundInput.style.display = 'none';
      document.body.appendChild(_soundInput);
    }
    _soundInput.onchange = function () {
      var f = _soundInput.files && _soundInput.files[0];
      _soundInput.value = '';
      if (!f) return;
      var rd = new FileReader();
      rd.onload = function () {
        sb().modules.push({ id: newId(), type: 'sound', name: f.name.replace(/\.[^.]+$/, ''), dataB64: rd.result, x: Math.round(x), y: Math.round(y), inSec: 0, outSec: null, cursorSec: null });
        render();
      };
      rd.readAsDataURL(f);
    };
    _soundInput.click();
  }

  function renderSound(m) {
    var el = document.createElement('div');
    el.className = 'sb-module sb-sound';
    var cv = document.createElement('canvas');
    cv.width = 220; cv.height = 44;
    cv.className = 'sb-wave';
    el.appendChild(cv);
    decodeAudio(m.id, m.dataB64, function (buf) {
      if (m.outSec == null) { m.outSec = buf.duration; render(); return; }
      drawWave(cv, buf, m.inSec || 0, m.outSec);
    });
    // split cursor: click on the wave places it (visual line), context
    // menu "Scinder ici" cuts at that point — the mock's "scindé".
    if (m.cursorSec != null && m.outSec != null) {
      var cur = document.createElement('div');
      cur.className = 'sb-wave-cursor';
      cur.style.left = (6 + 220 * ((m.cursorSec - (m.inSec || 0)) / (m.outSec - (m.inSec || 0)))) + 'px';
      el.appendChild(cur);
    }
    cv.addEventListener('pointerdown', function (e) {
      // plain click = place the split cursor; the module still drags via
      // anywhere else on its body (the padding/name area).
      e.stopPropagation();
      var r = cv.getBoundingClientRect();
      var t = (e.clientX - r.left) / r.width;
      m.cursorSec = (m.inSec || 0) + t * ((m.outSec || 0) - (m.inSec || 0));
      render();
    });
    var nm = document.createElement('div');
    nm.className = 'sb-name';
    nm.textContent = m.name + (m.outSec != null ? ' — ' + ((m.outSec - (m.inSec || 0)).toFixed(1)) + 's' : '');
    el.appendChild(nm);
    var play = document.createElement('span');
    play.className = 'sb-play sb-sound-play';
    play.textContent = '▶';
    play.title = 'Écouter';
    play.addEventListener('pointerdown', function (e) { e.stopPropagation(); });
    play.addEventListener('click', function (e) {
      e.stopPropagation();
      stopAllAudio();
      playRange(m.id, m.dataB64, m.inSec || 0, m.outSec || 0, 0, 0);
    });
    el.appendChild(play);
    wireModuleDrag(el, m);
    el.addEventListener('contextmenu', function (e) {
      e.preventDefault(); e.stopPropagation();
      var menu = [];
      if (m.cursorSec != null && m.cursorSec > (m.inSec || 0) + 0.05 && m.cursorSec < (m.outSec || 0) - 0.05) {
        menu.push({ label: 'Scinder ici', action: function () { splitSound(m); } });
      } else {
        menu.push({ label: 'Scinder (cliquer la forme d’onde pour placer le point)', disabled: true, action: function () {} });
      }
      menu.push({ label: 'Supprimer le module', action: function () { var s = sb(); s.modules.splice(s.modules.indexOf(m), 1); render(); } });
      window.showContextMenu(e.clientX, e.clientY, menu);
    });
    return el;
  }
  function splitSound(m) {
    var s = sb();
    var right = { id: newId(), type: 'sound', name: m.name, dataB64: m.dataB64, x: m.x + 250, y: m.y, inSec: m.cursorSec, outSec: m.outSec, cursorSec: null };
    m.outSec = m.cursorSec;
    m.cursorSec = null;
    // the split halves share the decoded buffer — register under the new id too
    if (_audioBuffers[m.id]) _audioBuffers[right.id] = _audioBuffers[m.id];
    s.modules.push(right);
    render();
  }

  // Dropping a sound module onto a montage lane embeds it as the montage's
  // audio track (spec Q&A: "piste audio DANS le module montage"), starting
  // at the montage frame under the pointer.
  function tryDropSoundIntoMontage(m, clientX, clientY) {
    if (m.type !== 'sound') return false;
    var lanes = world.querySelectorAll('.sb-montage .sb-montage-lane');
    for (var i = 0; i < lanes.length; i++) {
      var r = lanes[i].getBoundingClientRect();
      if (clientX >= r.left && clientX <= r.right && clientY >= r.top - 20 && clientY <= r.bottom + 40) {
        var mid = lanes[i].closest('.sb-module').dataset.sbId;
        var montage = sb().modules.find(function (x) { return x.id === mid; });
        if (!montage) return false;
        var atFrame = Math.max(0, Math.round((clientX - r.left) / (FPP * sb().zoom)));
        montage.audio.push({ aid: newId(), name: m.name, dataB64: m.dataB64, inSec: m.inSec || 0, outSec: m.outSec || 0, offsetFrames: atFrame });
        if (_audioBuffers[m.id]) _audioBuffers['a' + montage.audio[montage.audio.length - 1].aid] = _audioBuffers[m.id];
        var s = sb();
        s.modules.splice(s.modules.indexOf(m), 1);
        s.activeMontageId = montage.id;
        render();
        return true;
      }
    }
    return false;
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
