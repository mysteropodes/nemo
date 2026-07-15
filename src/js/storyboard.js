// ---- STORYBOARD MODE (2026-07) ----
// Node-space montage editor — the third app mode next to Animation 2D and
// Motion. The timeline grid is replaced by a free pan/zoom space holding
// draggable MODULES (the Rive-state-machine feel, but for editing).
//
// ARCHITECTURE v2 (reworked after the user's annotated mock corrected v1's
// misreading): the montage is NOT a container that absorbs instances into
// internal chips. It is a small anchor BLOCK (film icon) that real modules
// SNAP TO in a chain, staying whole and visible (thumbnail + edit button);
// any member can be desnapped by dragging it out. A time RULER sits above
// the chain and grows/shrinks as modules join/leave; sounds snap BELOW the
// chain as visible waveform modules. ("On snap chaque component ou module
// à cet élément-là pour en faire un montage … on peut aussi desnap
// n'importe quel module … timeline de montage s'agrandit en fonction de
// l'ajout ou suppression de module.")
//
// Data model — plain JSON, persisted wholesale through exportJSON (no
// runtime-only fields in `modules`, by construction):
//   state.storyboard = {
//     pan: {x, y}, zoom: 1, nextId: 1, activeMontageId: null,
//     modules: [
//       { id, type:'instance', symbolId, x, y,
//         trimIn?, trimOut?, duration? },            // retiming, set when chained
//       { id, type:'montage', name, x, y, playhead,
//         chain: [instanceModuleId...],              // ORDER = the sequence
//         audio: [{ moduleId, offsetFrames }] },     // sound modules snapped below
//       { id, type:'sound', name, dataB64, x, y, inSec, outSec, cursorSec },
//     ],
//   }
(function () {
  var SNAP_PX = 12;      // world-space edge-snap distance while dragging
  var RULER_H = 16;      // ruler strip height above a chain
  var CHAIN_GAP_Y = 8;   // gap between chain and attached sounds

  function sb() {
    if (!state.storyboard) {
      state.storyboard = { pan: { x: 40, y: 40 }, zoom: 1, nextId: 1, activeMontageId: null, modules: [] };
    }
    migrateLegacy(state.storyboard);
    return state.storyboard;
  }
  // v1 montages stored their sequence as internal `items` chips and audio
  // with inline dataB64 — convert both to the v2 real-module model once.
  function migrateLegacy(s) {
    if (s._v2) return;
    s._v2 = true;
    (s.modules || []).forEach(function (m) {
      if (m.type !== 'montage') return;
      if (!m.chain) m.chain = [];
      if (m.items && m.items.length) {
        m.items.forEach(function (it) {
          var id = 'sbm' + (s.nextId++);
          s.modules.push({ id: id, type: 'instance', symbolId: it.symbolId, x: m.x, y: m.y, trimIn: it.trimIn, trimOut: it.trimOut, duration: it.duration });
          m.chain.push(id);
        });
      }
      delete m.items;
      if (m.audio && m.audio.length && m.audio[0] && m.audio[0].dataB64) {
        var conv = [];
        m.audio.forEach(function (a) {
          var id = 'sbm' + (s.nextId++);
          s.modules.push({ id: id, type: 'sound', name: a.name, dataB64: a.dataB64, x: m.x, y: m.y + 140, inSec: a.inSec, outSec: a.outSec, cursorSec: null });
          conv.push({ moduleId: id, offsetFrames: a.offsetFrames || 0 });
        });
        m.audio = conv;
      }
      if (!m.audio) m.audio = [];
    });
  }
  function newId() { var s = sb(); return 'sbm' + (s.nextId++); }
  function moduleById(id) { return sb().modules.find(function (x) { return x.id === id; }) || null; }

  // Stable per-symbol color (the mock shows plain colored rects as the
  // instance preview) — hash the symbol id onto a fixed pleasant palette
  // so every instance of the same component shares a color.
  var PALETTE = ['#c0504d', '#4f81bd', '#4bac7e', '#8064a2', '#d19a3f', '#5e6ad2', '#9f5fa8', '#3f9aa8'];
  function symbolColor(symbolId) {
    var h = 0;
    for (var i = 0; i < symbolId.length; i++) h = (h * 31 + symbolId.charCodeAt(i)) >>> 0;
    return PALETTE[h % PALETTE.length];
  }
  // Chained-instance thumb width scales with duration ("les poignées ne
  // changent pas... la taille du rectangle" — correct: the card was fixed
  // at 72px regardless of what trim/stretch did to the underlying data,
  // so retiming was invisible on the module itself, only readable in the
  // small text label). Free (unchained) instances keep the CSS default.
  var THUMB_PXF = 2.4, THUMB_MIN_W = 56;
  function thumbWidth(duration) { return Math.max(THUMB_MIN_W, Math.round(duration * THUMB_PXF)); }

  // ---- real instance thumbnails (replaces the flat colored rect) ----
  // Rendered via Paper's OWN CPU rasterizer (Layer.rasterize — the exact
  // technique export.js's exportFrameDataURL already uses for PNG export),
  // not the vello/WASM engine: a dedicated hidden service layer (own layer,
  // never exportLayer/ghostAllLayer — CLAUDE.md's "don't share a service
  // layer across unrelated consumers" lesson) filled via desP from the
  // symbol's resolved strokes, rasterized small, cached as a dataURL.
  //
  // Which frame: frame 0 for a free (unchained) instance — cheap, stable,
  // no re-render cost while scrubbing/playing. For a CHAINED member, the
  // trim-IN frame instead (the first frame that will actually play) — more
  // informative than always frame 0, and only changes when a trim/stretch
  // gesture ends (already a full render()), never on every playhead tick —
  // a literal "live playhead frame" thumbnail was considered and rejected:
  // re-rasterizing per rAF during playback for potentially many chained
  // members would be real, avoidable jank.
  var _thumbLayer = null, _thumbCache = {}; // "symbolId:frame" -> dataURL
  function invalidateThumb(symbolId) {
    Object.keys(_thumbCache).forEach(function (k) { if (k.indexOf(symbolId + ':') === 0) delete _thumbCache[k]; });
  }
  function thumbDataUrl(symbolId, frame) {
    var key = symbolId + ':' + frame;
    if (_thumbCache[key]) return _thumbCache[key];
    var sym = state.symbols[symbolId];
    if (!sym || !window.project) return null;
    if (!_thumbLayer) {
      var prevA = project.activeLayer;
      _thumbLayer = new Layer({ insert: true });
      _thumbLayer.visible = false; // rasterize() flips this true only for the instant of the call
      prevA.activate();
    }
    var prev = project.activeLayer;
    _thumbLayer.activate();
    _thumbLayer.removeChildren();
    var strokes = symbolStrokesAt(symbolId, frame);
    strokes.forEach(function (sd) { desP(sd, _thumbLayer); });
    var url = null;
    if (_thumbLayer.children.length) {
      _thumbLayer.visible = true;
      // Fit the symbol's own bounds into a small fixed thumbnail canvas —
      // resolution scaled so the LARGER dimension lands near 96px, capped
      // low (never above 1) since this is a tiny UI thumbnail, not export.
      var b = _thumbLayer.bounds;
      if (b.width > 0 && b.height > 0) {
        var res = Math.min(1, 96 / Math.max(b.width, b.height));
        var raster = _thumbLayer.rasterize({ resolution: 72 * res, insert: false });
        url = raster.canvas.toDataURL('image/png');
        raster.remove();
      }
      _thumbLayer.visible = false;
    }
    prev.activate();
    if (url) _thumbCache[key] = url;
    return url;
  }
  function applyThumb(card, m, sym) {
    if (!sym) return;
    var host = chainOf(m);
    var frame = host ? (m.trimIn || 0) : 0;
    var url = thumbDataUrl(m.symbolId, frame);
    if (url) {
      card.style.backgroundImage = 'url(' + url + ')';
      card.style.backgroundSize = 'contain';
      card.style.backgroundRepeat = 'no-repeat';
      card.style.backgroundPosition = 'center';
    }
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

  // ---- time model ----
  // A chained instance module carries its own retiming:
  //   trimIn/trimOut = SOURCE range played from the component
  //   duration       = montage frames occupied — equals the source range
  //                    for plain trims, differs after a STRETCH (Alt+drag:
  //                    same range squeezed into fewer/more frames = speed).
  function symbolDuration(symbolId) {
    var sym = state.symbols[symbolId];
    return sym ? (sym.totalFrames || (sym.layers[0] && sym.layers[0].frames.length) || 24) : 24;
  }
  function ensureRetime(mod) {
    if (mod.trimIn == null) mod.trimIn = 0;
    if (mod.trimOut == null) mod.trimOut = symbolDuration(mod.symbolId) - 1;
    if (mod.duration == null) mod.duration = mod.trimOut - mod.trimIn + 1;
  }
  // The montage's sequence, derived LIVE from its chain of real modules —
  // a dangling id (deleted module) is skipped, never crashes.
  function chainMods(m) {
    return (m.chain || []).map(moduleById).filter(function (x) { return x && x.type === 'instance'; });
  }
  function montageTotal(m) {
    return chainMods(m).reduce(function (a, mod) { ensureRetime(mod); return a + mod.duration; }, 0);
  }
  function montageStrokesAt(m, f) {
    var mods = chainMods(m), acc = 0;
    for (var i = 0; i < mods.length; i++) {
      var mod = mods[i];
      ensureRetime(mod);
      if (f < acc + mod.duration) {
        var local = f - acc;
        var srcLen = mod.trimOut - mod.trimIn + 1;
        var srcFrame = mod.trimIn + Math.min(srcLen - 1, Math.floor(local * srcLen / mod.duration));
        return symbolStrokesAt(mod.symbolId, srcFrame);
      }
      acc += mod.duration;
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

  // ---- chain layout + piecewise time<->pixels mapping ----
  // Members keep their natural module widths (the mock shows equal cards,
  // not duration-proportional strips), so the ruler's time axis is
  // PIECEWISE: member i spans [cumDur, cumDur+duration) in time and
  // [cumX, cumX+width) in pixels. Geometry is cached per montage at
  // layout time — pxToFrame/frameToPx read the cache, never the DOM.
  var _chainGeom = {}; // montageId -> {startX, y, totalW, totalDur, members:[{id,w,dur}]}
  function layoutChains() {
    if (!world) return;
    _chainGeom = {};
    sb().modules.forEach(function (m) {
      if (m.type !== 'montage') return;
      var blockEl = world.querySelector('[data-sb-id="' + m.id + '"]');
      if (!blockEl) return;
      var x = m.x + blockEl.offsetWidth; // chain starts flush at the block's right edge
      var geom = { startX: x, y: m.y, totalW: 0, totalDur: 0, members: [] };
      chainMods(m).forEach(function (mod) {
        ensureRetime(mod);
        var el = world.querySelector('[data-sb-id="' + mod.id + '"]');
        if (!el) return;
        mod.x = x; mod.y = m.y;
        el.style.left = x + 'px';
        el.style.top = m.y + 'px';
        el.classList.add('chained');
        geom.members.push({ id: mod.id, w: el.offsetWidth, dur: mod.duration });
        x += el.offsetWidth;
        geom.totalW += el.offsetWidth;
        geom.totalDur += mod.duration;
      });
      _chainGeom[m.id] = geom;
      // attached sounds sit UNDER the chain at their time offset
      var blockH = blockEl.offsetHeight;
      (m.audio || []).forEach(function (a) {
        var smod = moduleById(a.moduleId);
        var sEl = smod && world.querySelector('[data-sb-id="' + smod.id + '"]');
        if (!smod || !sEl) return;
        smod.x = frameToPx(m, a.offsetFrames);
        smod.y = m.y + blockH + CHAIN_GAP_Y;
        sEl.style.left = smod.x + 'px';
        sEl.style.top = smod.y + 'px';
        sEl.classList.add('chained');
      });
      positionRuler(m);
    });
  }
  function pxToFrame(m, px) {
    var g = _chainGeom[m.id];
    if (!g || !g.members.length) return 0;
    var rel = px - g.startX;
    if (rel <= 0) return 0;
    var cumX = 0, cumF = 0;
    for (var i = 0; i < g.members.length; i++) {
      var mb = g.members[i];
      if (rel < cumX + mb.w) return Math.round(cumF + (rel - cumX) / mb.w * mb.dur);
      cumX += mb.w; cumF += mb.dur;
    }
    return Math.max(0, g.totalDur - 1);
  }
  function frameToPx(m, f) {
    var g = _chainGeom[m.id];
    if (!g || !g.members.length) return (g ? g.startX : m.x);
    var cumX = 0, cumF = 0;
    for (var i = 0; i < g.members.length; i++) {
      var mb = g.members[i];
      if (f < cumF + mb.dur) return g.startX + cumX + (f - cumF) / mb.dur * mb.w;
      cumX += mb.w; cumF += mb.dur;
    }
    return g.startX + g.totalW;
  }

  // ---- ruler (the montage timeline above the chain — grows/shrinks with
  // membership, scrubbable, carries the playhead marker) ----
  function positionRuler(m) {
    var g = _chainGeom[m.id];
    var ruler = world.querySelector('[data-sb-ruler="' + m.id + '"]');
    if (!ruler) return;
    if (!g || !g.members.length) { ruler.style.display = 'none'; return; }
    ruler.style.display = 'block';
    ruler.style.left = g.startX + 'px';
    ruler.style.top = (m.y - RULER_H - 4) + 'px';
    ruler.style.width = g.totalW + 'px';
    var ph = ruler.querySelector('.sb-ph');
    // The playhead at frame f sits at f's LEFT boundary — mathematically
    // right but visually "ne va pas jusqu'au bout": on a stretched member
    // (5f over ~180px) the last frame leaves a huge dead zone at the far
    // right. Editor-pragmatic: the LAST frame pins the marker to the
    // ruler's extreme right edge.
    if (ph) {
      var phF = m.playhead || 0;
      var px = phF >= g.totalDur - 1 ? g.totalW : (frameToPx(m, phF) - g.startX);
      ph.style.left = px + 'px';
    }
    var lbl = ruler.querySelector('.sb-ruler-lbl');
    if (lbl) lbl.textContent = (m.playhead || 0) + ' / ' + g.totalDur + ' f';
  }
  function buildRuler(m) {
    var ruler = document.createElement('div');
    ruler.className = 'sb-ruler';
    ruler.dataset.sbRuler = m.id;
    var lbl = document.createElement('span');
    lbl.className = 'sb-ruler-lbl';
    ruler.appendChild(lbl);
    var ph = document.createElement('div');
    ph.className = 'sb-ph';
    ruler.appendChild(ph);
    ruler.addEventListener('pointerdown', function (e) {
      e.stopPropagation(); e.preventDefault();
      sb().activeMontageId = m.id;
      markActive();
      function scrub(ev) {
        var p = toWorld(ev.clientX, ev.clientY);
        var total = montageTotal(m);
        m.playhead = Math.max(0, Math.min(Math.max(0, total - 1), pxToFrame(m, p.x)));
        positionRuler(m);
        updatePreview();
      }
      scrub(e);
      function up() { document.removeEventListener('pointermove', scrub); document.removeEventListener('pointerup', up); }
      document.addEventListener('pointermove', scrub);
      document.addEventListener('pointerup', up);
    });
    return ruler;
  }
  function markActive() {
    var act = sb().activeMontageId;
    world.querySelectorAll('.sb-montageblock').forEach(function (b) {
      b.classList.toggle('active', b.dataset.sbId === act);
    });
  }

  // ---- module rendering ----
  function render() {
    if (!world) return;
    world.innerHTML = '';
    var s = sb();
    s.modules.forEach(function (m) {
      var el = m.type === 'instance' ? renderInstance(m) : m.type === 'montage' ? renderMontageBlock(m) : m.type === 'sound' ? renderSound(m) : null;
      if (!el) return;
      el.style.left = m.x + 'px';
      el.style.top = m.y + 'px';
      el.dataset.sbId = m.id;
      world.appendChild(el);
    });
    s.modules.forEach(function (m) { if (m.type === 'montage') world.appendChild(buildRuler(m)); });
    layoutChains();
    markActive();
  }

  function chainOf(mod) {
    return sb().modules.find(function (m) { return m.type === 'montage' && (m.chain || []).indexOf(mod.id) >= 0; }) || null;
  }
  function audioHostOf(mod) {
    return sb().modules.find(function (m) { return m.type === 'montage' && (m.audio || []).some(function (a) { return a.moduleId === mod.id; }); }) || null;
  }

  function renderInstance(m) {
    var el = document.createElement('div');
    el.className = 'sb-module sb-instance';
    var sym = state.symbols[m.symbolId];
    var card = document.createElement('div');
    card.className = 'sb-thumb';
    card.style.background = symbolColor(m.symbolId); // fallback shown until/unless a real render succeeds
    applyThumb(card, m, sym);
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
    var host = chainOf(m);
    if (host) { ensureRetime(m); card.style.width = thumbWidth(m.duration) + 'px'; }
    var nm = document.createElement('div');
    nm.className = 'sb-name';
    var dName = sym ? sym.name : '(composant supprimé)';
    if (host) {
      var srcLen = m.trimOut - m.trimIn + 1;
      dName += ' — ' + m.duration + 'f';
      if (m.duration !== srcLen) dName += ' (×' + (srcLen / m.duration).toFixed(2) + ')';
    }
    nm.textContent = dName;
    nm.title = dName; // full text on hover — the label itself still truncates at max-width
    el.appendChild(nm);
    // Retiming handles only exist while chained ("On y snap, retime les
    // component"): edges = trim (source range + duration together, speed
    // unchanged) — Alt+drag right = stretch (duration alone = speed).
    if (host) {
      ['left', 'right'].forEach(function (side2) {
        var hnd = document.createElement('div');
        hnd.className = 'sb-trim ' + side2;
        hnd.title = side2 === 'right' ? 'Trim — Alt+glisser : étirer (vitesse)' : 'Trim';
        hnd.addEventListener('pointerdown', function (e) {
          e.stopPropagation(); e.preventDefault();
          ensureRetime(m);
          var startX = e.clientX, o = { trimIn: m.trimIn, trimOut: m.trimOut, duration: m.duration };
          var stretch = side2 === 'right' && e.altKey;
          var maxLen = symbolDuration(m.symbolId);
          var PXF = 2; // trim gesture scale: 2px per frame — steady, zoom-independent feel
          var host2 = chainOf(m);
          function mv(ev) {
            var df = Math.round((ev.clientX - startX) / PXF);
            if (stretch) m.duration = Math.max(1, o.duration + df);
            else if (side2 === 'left') {
              var ti = Math.max(0, Math.min(o.trimOut, o.trimIn + df));
              m.trimIn = ti;
              m.duration = Math.max(1, o.duration - (ti - o.trimIn));
            } else {
              var to = Math.min(maxLen - 1, Math.max(o.trimIn, o.trimOut + df));
              m.trimOut = to;
              m.duration = Math.max(1, o.duration + (to - o.trimOut));
            }
            nm.textContent = (sym ? sym.name : '?') + ' — ' + m.duration + 'f';
            card.style.width = thumbWidth(m.duration) + 'px';
            // LIVE feedback ("les poignées devraient pouvoir bouger"):
            // durations feed the ruler's piecewise time map — refresh the
            // cached geometry and the ruler (total + playhead position)
            // while dragging, plus the canvas preview, not just at release.
            if (host2) {
              var g2 = _chainGeom[host2.id];
              if (g2) {
                var mb2 = g2.members.find(function (x) { return x.id === m.id; });
                if (mb2) { g2.totalDur += m.duration - mb2.dur; mb2.dur = m.duration; }
                var tot2 = g2.totalDur;
                if ((host2.playhead || 0) > Math.max(0, tot2 - 1)) host2.playhead = Math.max(0, tot2 - 1);
                positionRuler(host2);
              }
              updatePreview();
            }
          }
          function up() {
            document.removeEventListener('pointermove', mv);
            document.removeEventListener('pointerup', up);
            render(); updatePreview();
          }
          document.addEventListener('pointermove', mv);
          document.addEventListener('pointerup', up);
        });
        el.appendChild(hnd);
      });
    }
    el.addEventListener('dblclick', function (e) { e.stopPropagation(); openSymbol(m); });
    wireModuleDrag(el, m);
    wireModuleMenu(el, m);
    return el;
  }

  // The montage anchor BLOCK (film icon) — modules chain to its right.
  function renderMontageBlock(m) {
    var el = document.createElement('div');
    el.className = 'sb-module sb-montageblock' + (sb().activeMontageId === m.id ? ' active' : '');
    el.title = m.name + ' — glissez des instances contre son bord droit pour monter';
    el.innerHTML = '<svg viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor" stroke-width="1.6"><rect x="3" y="5" width="18" height="14" rx="2"/><path d="M7 5v14M17 5v14M3 10h4M3 14h4M17 10h4M17 14h4"/></svg>';
    var play = document.createElement('span');
    play.className = 'sb-play sb-block-play';
    play.textContent = _playingMontageId === m.id ? '◼' : '▶';
    play.title = 'Lire / arrêter le montage';
    play.addEventListener('pointerdown', function (e) { e.stopPropagation(); });
    play.addEventListener('click', function (e) {
      e.stopPropagation();
      sb().activeMontageId = m.id;
      markActive();
      toggleMontagePlay(m);
      updatePreview();
    });
    el.appendChild(play);
    el.addEventListener('click', function () {
      sb().activeMontageId = m.id;
      markActive();
      updatePreview();
    });
    wireModuleDrag(el, m);
    wireModuleMenu(el, m);
    return el;
  }

  function openSymbol(m) {
    if (!state.symbols[m.symbolId]) { showToast('Ce composant n’existe plus'); return; }
    if (window.SMMotion) SMMotion.setAppMode('anim2d');
    window.SM.enterSymbol(m.symbolId);
  }

  // ---- canvas preview (service Paper layer, ghostAllLayer pattern —
  // engine-bridge's buildSceneJson swaps the document layers for this one
  // in storyboard mode, reading it through onionLayerItems) ----
  var previewLayer = null;
  function getPreviewLayer() {
    var s = sb();
    var m = s.modules.find(function (x) { return x.id === s.activeMontageId; });
    if (!m || !chainMods(m).length) return null;
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
    if (m && chainMods(m).length) {
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
  // Start every attached sound relative to playhead p (frames): already-
  // underway entries start mid-range at the exact position; not-yet-reached
  // ones are scheduled ahead (WebAudio's own clock keeps them sample-
  // accurate against the visual rAF loop).
  function startMontageAudio(m, p) {
    var fps = state.fps || 24;
    (m.audio || []).forEach(function (a) {
      var smod = moduleById(a.moduleId);
      if (!smod) return;
      var relSec = (p - a.offsetFrames) / fps;
      if (relSec >= 0) playRange(smod.id, smod.dataB64, smod.inSec || 0, smod.outSec || 0, 0, relSec);
      else playRange(smod.id, smod.dataB64, smod.inSec || 0, smod.outSec || 0, -relSec, 0);
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
        positionRuler(m);
        updatePreview();
      }
      _playRaf = requestAnimationFrame(step);
    }
    _playRaf = requestAnimationFrame(step);
    render();
  }

  // ---- chain membership (join / leave / re-join by snapping) ----
  // Joining: an instance released with its LEFT edge flush against (within
  // SNAP_PX of) the montage block's right edge, a member's right edge, or
  // overlapping the chain row, inserts at the x-derived index. Leaving:
  // grabbing a chained member lifts it out immediately (the chain repacks
  // live), dropping it back in re-inserts, dropping elsewhere leaves it
  // free — the mock's "desnap n'importe quel module".
  function tryJoinChain(mod, relX, relY) {
    if (mod.type !== 'instance') return false;
    var el = world.querySelector('[data-sb-id="' + mod.id + '"]');
    var h = el ? el.offsetHeight : 80;
    var s = sb();
    for (var i = 0; i < s.modules.length; i++) {
      var m = s.modules[i];
      if (m.type !== 'montage') continue;
      var blockEl = world.querySelector('[data-sb-id="' + m.id + '"]');
      if (!blockEl) continue;
      var g = _chainGeom[m.id] || { startX: m.x + blockEl.offsetWidth, totalW: 0, members: [] };
      var rowY = m.y;
      // vertical proximity with the chain row
      if (Math.abs(mod.y - rowY) > h * 0.8) continue;
      var chainEnd = g.startX + g.totalW;
      // near the row horizontally: from a bit before the block to a bit past the chain end
      if (mod.x < m.x - SNAP_PX * 4 || mod.x > chainEnd + SNAP_PX * 6) continue;
      // insertion index from the module's x against member boundaries
      var at = 0, cumX = g.startX;
      for (var k = 0; k < g.members.length; k++) {
        if (mod.x > cumX + g.members[k].w / 2) at = k + 1;
        cumX += g.members[k].w;
      }
      ensureRetime(mod);
      m.chain.splice(at, 0, mod.id);
      s.activeMontageId = m.id;
      return true;
    }
    return false;
  }
  function tryJoinAudio(mod) {
    if (mod.type !== 'sound') return false;
    var s = sb();
    for (var i = 0; i < s.modules.length; i++) {
      var m = s.modules[i];
      if (m.type !== 'montage') continue;
      var blockEl = world.querySelector('[data-sb-id="' + m.id + '"]');
      if (!blockEl) continue;
      var g = _chainGeom[m.id];
      if (!g || !g.members.length) continue;
      var blockH = blockEl.offsetHeight;
      var laneY = m.y + blockH + CHAIN_GAP_Y;
      if (Math.abs(mod.y - laneY) > 40) continue;
      if (mod.x < g.startX - SNAP_PX * 6 || mod.x > g.startX + g.totalW + SNAP_PX * 6) continue;
      m.audio.push({ moduleId: mod.id, offsetFrames: Math.max(0, pxToFrame(m, mod.x)) });
      s.activeMontageId = m.id;
      return true;
    }
    return false;
  }
  function leaveAnyChain(mod) {
    var host = chainOf(mod);
    if (host) { host.chain.splice(host.chain.indexOf(mod.id), 1); return host; }
    var ah = audioHostOf(mod);
    if (ah) { ah.audio = ah.audio.filter(function (a) { return a.moduleId !== mod.id; }); return ah; }
    return null;
  }

  // ---- dragging (rAF-coalesced, geometry cached at pointerdown — see the
  // v1 rewrite's commit message for the jank post-mortem) ----
  function wireModuleDrag(el, m, opts) {
    el.addEventListener('pointerdown', function (e) {
      if (e.button !== 0) return;
      if (e.target.closest('.sb-play, .sb-edit, .sb-trim')) return;
      e.stopPropagation();
      var start = toWorld(e.clientX, e.clientY);
      var ox = start.x - m.x, oy = start.y - m.y;
      var w = el.offsetWidth, h = el.offsetHeight;
      var neighbors = [];
      sb().modules.forEach(function (o) {
        if (o.id === m.id) return;
        var oe = world.querySelector('[data-sb-id="' + o.id + '"]');
        if (oe) neighbors.push({ x: o.x, y: o.y, w: oe.offsetWidth, h: oe.offsetHeight });
      });
      var moved = false, lastX = e.clientX, lastY = e.clientY, raf = 0, left = false;
      // Dragging a montage BLOCK moves the WHOLE assembly ("quand on drag
      // un montage alors ça doit bouger tout l'ensemble") — snapshot every
      // dependent (chain members, attached sounds, the ruler) with its
      // offset relative to the block, and carry them in apply().
      var ensemble = [];
      if (m.type === 'montage') {
        var depIds = (m.chain || []).slice();
        (m.audio || []).forEach(function (a) { depIds.push(a.moduleId); });
        depIds.forEach(function (did) {
          var dm = moduleById(did);
          var de = dm && world.querySelector('[data-sb-id="' + did + '"]');
          if (dm && de) ensemble.push({ mod: dm, el: de, dx: dm.x - m.x, dy: dm.y - m.y });
        });
        var rulerEl = world.querySelector('[data-sb-ruler="' + m.id + '"]');
        if (rulerEl) ensemble.push({ el: rulerEl, rx: parseFloat(rulerEl.style.left || 0) - m.x, ry: parseFloat(rulerEl.style.top || 0) - m.y });
      }
      try { el.setPointerCapture(e.pointerId); } catch (err) {}
      el.style.zIndex = 30;
      function apply() {
        raf = 0;
        // First real movement of a chained/attached module DESNAPS it —
        // the chain repacks live underneath, the mock's core gesture.
        if (!left) {
          left = true;
          var wasHosted = leaveAnyChain(m);
          if (wasHosted) {
            // repack without rebuilding the dragged element itself
            render();
            var el2 = world.querySelector('[data-sb-id="' + m.id + '"]');
            if (el2) { el2.style.zIndex = 30; el = el2; }
            updatePreview();
          }
        }
        var p = toWorld(lastX, lastY);
        m.x = p.x - ox; m.y = p.y - oy;
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
        ensemble.forEach(function (d) {
          if (d.mod) {
            d.mod.x = m.x + d.dx; d.mod.y = m.y + d.dy;
            d.el.style.left = d.mod.x + 'px';
            d.el.style.top = d.mod.y + 'px';
          } else {
            d.el.style.left = (m.x + d.rx) + 'px';
            d.el.style.top = (m.y + d.ry) + 'px';
          }
        });
      }
      function mv(ev) {
        lastX = ev.clientX; lastY = ev.clientY;
        moved = true;
        if (!raf) raf = requestAnimationFrame(apply);
      }
      function up(ev) {
        document.removeEventListener('pointermove', mv);
        document.removeEventListener('pointerup', up);
        document.removeEventListener('pointercancel', up);
        if (raf) { cancelAnimationFrame(raf); apply(); }
        el.style.zIndex = '';
        if (!moved) {
          // plain click, no drag — the module's own tap action (sound
          // modules place their split cursor here; see renderSound)
          if (opts && opts.onTap) opts.onTap(ev || e);
          return;
        }
        // snap-join: instance → chain row; sound → under-chain audio lane;
        // montage blocks never join anything.
        var joined = tryJoinChain(m, 0, 0) || tryJoinAudio(m);
        render();
        if (joined) updatePreview();
      }
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
        if (chainOf(m)) {
          menu.push({ label: 'Réinitialiser le retiming', action: function () { m.trimIn = 0; m.trimOut = symbolDuration(m.symbolId) - 1; m.duration = m.trimOut + 1; render(); updatePreview(); } });
          menu.push({ label: 'Détacher du montage', action: function () { leaveAnyChain(m); m.y += 110; render(); updatePreview(); } });
        }
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
      if (m.type === 'sound' && audioHostOf(m)) {
        menu.push({ label: 'Détacher du montage', action: function () { leaveAnyChain(m); m.y += 60; render(); } });
      }
      menu.push({ label: 'Supprimer le module', action: function () { leaveAnyChain(m); var s = sb(); s.modules.splice(s.modules.indexOf(m), 1); if (s.activeMontageId === m.id) s.activeMontageId = null; render(); } });
      window.showContextMenu(e.clientX, e.clientY, menu);
    });
  }

  function wireSpace() {
    // Pan: drag the empty background.
    space.addEventListener('pointerdown', function (e) {
      if (e.button !== 0 || e.target.closest('.sb-module, .sb-ruler')) return;
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
      if (e.target.closest('.sb-module, .sb-ruler')) return;
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
    var m = { id: newId(), type: 'montage', name: 'Montage ' + (s.modules.filter(function (x2) { return x2.type === 'montage'; }).length + 1), x: Math.round(x), y: Math.round(y), chain: [], audio: [], playhead: 0 };
    s.modules.push(m);
    s.activeMontageId = m.id;
    render();
    return m;
  }

  // ---- sound modules ----
  // { id, type:'sound', name, dataB64, x, y, inSec, outSec, cursorSec }
  // inSec/outSec = the played RANGE — a split produces two modules sharing
  // the same dataB64 with complementary ranges. Decoded AudioBuffers are
  // runtime state in _audioBuffers keyed by module id — the persisted
  // model stays plain JSON by construction.
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
  // buffer RANGE into any canvas, which split views need).
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
      var hh = Math.max(1, peak * mid);
      ctx.fillRect(x, mid - hh, 1, hh * 2);
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
    if (m.cursorSec != null && m.outSec != null) {
      var cur = document.createElement('div');
      cur.className = 'sb-wave-cursor';
      cur.style.left = (6 + 220 * ((m.cursorSec - (m.inSec || 0)) / (m.outSec - (m.inSec || 0)))) + 'px';
      el.appendChild(cur);
    }
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
    // Click-vs-drag on the waveform ("le curseur de drag dans le son ne
    // marche pas bien"): the wave no longer hijacks pointerdown — the
    // whole module drags normally, and a plain CLICK (release without
    // movement, handled by wireModuleDrag's onTap) places the split
    // cursor at the clicked time.
    wireModuleDrag(el, m, {
      onTap: function (ev) {
        if (!ev || m.outSec == null) return;
        var r = cv.getBoundingClientRect();
        if (ev.clientX < r.left || ev.clientX > r.right || ev.clientY < r.top || ev.clientY > r.bottom) return;
        var t = (ev.clientX - r.left) / r.width;
        m.cursorSec = (m.inSec || 0) + t * (m.outSec - (m.inSec || 0));
        render();
      },
    });
    el.addEventListener('contextmenu', function (e) {
      e.preventDefault(); e.stopPropagation();
      var menu = [];
      if (m.cursorSec != null && m.cursorSec > (m.inSec || 0) + 0.05 && m.cursorSec < (m.outSec || 0) - 0.05) {
        menu.push({ label: 'Scinder ici', action: function () { splitSound(m); } });
      } else {
        menu.push({ label: 'Scinder (cliquer la forme d’onde pour placer le point)', disabled: true, action: function () {} });
      }
      if (audioHostOf(m)) menu.push({ label: 'Détacher du montage', action: function () { leaveAnyChain(m); m.y += 60; render(); } });
      menu.push({ label: 'Supprimer le module', action: function () { leaveAnyChain(m); var s = sb(); s.modules.splice(s.modules.indexOf(m), 1); render(); } });
      window.showContextMenu(e.clientX, e.clientY, menu);
    });
    return el;
  }
  function splitSound(m) {
    var s = sb();
    var right = { id: newId(), type: 'sound', name: m.name, dataB64: m.dataB64, x: m.x + 250, y: m.y, inSec: m.cursorSec, outSec: m.outSec, cursorSec: null };
    m.outSec = m.cursorSec;
    m.cursorSec = null;
    if (_audioBuffers[m.id]) _audioBuffers[right.id] = _audioBuffers[m.id];
    s.modules.push(right);
    render();
  }

  function montageById(id) {
    return sb().modules.find(function (x) { return x.type === 'montage' && x.id === id; }) || null;
  }
  function placeMontageAsLayer(m) {
    if (!chainMods(m).length) { showToast('Le montage est vide — snappez des instances contre son bloc d’abord'); return; }
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
    invalidateThumb: invalidateThumb,
    thumbDataUrl: thumbDataUrl, // exposed for debug/tests as well as internal reuse
  };
})();
