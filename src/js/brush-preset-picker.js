// Dedicated brush-preset picker popover — replaces the plain <select> with
// a swatch button (mirrors color-picker.js's anchored-popover pattern) that
// opens a scrollable grid of small canvas-drawn previews, one per preset,
// so the user can actually SEE the texture before picking it instead of
// reading a name off a dropdown list.
//
// The preview is NOT a separate lookalike approximation — it builds a
// throwaway Paper.js demo path ({insert:false}, never added to any layer)
// and runs the EXACT SAME buildBrushDabs() (tools.js) the real brush commit
// uses, then traces each returned dab's real segments onto the 2D canvas.
// This guarantees "what you see in the picker" can never silently drift
// from "what you actually get" on a real stroke — the previous version drew
// its own hand-rolled jittered-line approximation, which is exactly the
// kind of second implementation of the same idea that drifts out of sync
// the next time the real engine's parameters change (see
// strokemotion/CLAUDE.md's guidance on keeping duplicated logic in sync).
(function () {
  var popover = null, closeHandlers = null;
  var demoPath = null; // built lazily once Paper.js is ready, reused for every preview render

  // Hover-preview on the CANVAS (2026-07 feedback: "on ne voit pas les
  // brush en direct... il faut le même [que le menu flottant]") — same
  // apply-live-on-mouseenter/revert-on-mouseleave pattern as the floating
  // Brush Menu's own preview (brush-menu-bridge.js), reusing the same
  // truly-global helpers (stripAnyBrushTexture/applyBrushTexture, both
  // declared outside any IIFE) rather than a second copy of that logic.
  var _hoverSnapshot = null;
  function eligibleHoverPaths() {
    if (!(window.state && (state.tool === 'select' || state.tool === 'subselect')) || !window.selectedPaths || !selectedPaths.length) return [];
    return selectedPaths.filter(function (p) { return p instanceof Path && !(p.data && (p.data.isVectorBrush || p.data.isFillShape)); });
  }
  function beginHoverPreview() {
    if (_hoverSnapshot) return;
    _hoverSnapshot = eligibleHoverPaths().map(function (p) { return { p: p, preset: (p.data && p.data.brushTexturePreset) || null }; });
  }
  function hoverPreview(key) {
    beginHoverPreview();
    if (!_hoverSnapshot.length) return;
    _hoverSnapshot.forEach(function (rec) {
      stripAnyBrushTexture(rec.p);
      if (key && key !== 'none') applyBrushTexture(rec.p, key);
    });
    if (window.SMEngineBridge) SMEngineBridge.renderNow();
  }
  function revertHoverPreview() {
    if (!_hoverSnapshot) return;
    _hoverSnapshot.forEach(function (rec) {
      stripAnyBrushTexture(rec.p);
      if (rec.preset) applyBrushTexture(rec.p, rec.preset);
    });
    _hoverSnapshot = null;
    if (window.SMEngineBridge) SMEngineBridge.renderNow();
  }

  function closePopover() {
    if (!popover) return;
    revertHoverPreview();
    popover.remove();
    popover = null;
    if (closeHandlers) { closeHandlers(); closeHandlers = null; }
  }

  function getDemoPath(w, h) {
    if (demoPath) return demoPath;
    demoPath = new Path({ insert: false });
    var n = 20;
    for (var i = 0; i <= n; i++) {
      var t = i / n;
      demoPath.add(new Point(6 + t * (w - 12), h / 2 + Math.sin(t * Math.PI * 2.2) * (h * 0.22)));
    }
    demoPath.smooth();
    return demoPath;
  }

  // Traces a live Paper.js Path's actual bezier segments onto a 2D canvas
  // context — works for any shape/rotation, since it reads real geometry
  // rather than assuming a simple ellipse.
  function traceOnCanvas2D(ctx, path) {
    var segs = path.segments;
    if (!segs.length) return;
    ctx.beginPath();
    ctx.moveTo(segs[0].point.x, segs[0].point.y);
    var n = segs.length;
    for (var i = 1; i <= n; i++) {
      var prev = segs[i - 1], cur = segs[i % n];
      var c1 = prev.point.add(prev.handleOut), c2 = cur.point.add(cur.handleIn);
      ctx.bezierCurveTo(c1.x, c1.y, c2.x, c2.y, cur.point.x, cur.point.y);
      if (i === n && !path.closed) break;
    }
    if (path.closed) ctx.closePath();
  }

  function drawPreview(canvas, presetKey) {
    var ctx = canvas.getContext('2d');
    var w = canvas.width, h = canvas.height;
    ctx.clearRect(0, 0, w, h);
    var textColor = getComputedStyle(document.documentElement).getPropertyValue('--text').trim() || '#eee';
    var preset = window.resolveBrushPreset ? window.resolveBrushPreset(presetKey) : null;
    if (!preset) {
      ctx.globalAlpha = 1; ctx.strokeStyle = textColor; ctx.lineWidth = 2; ctx.setLineDash([]);
      ctx.beginPath(); ctx.moveTo(6, h / 2); ctx.lineTo(w - 6, h / 2); ctx.stroke();
      return;
    }
    var demo = getDemoPath(w, h);
    var baseWidth = Math.max(2, h * 0.16); // representative stroke width for the thumbnail's own scale
    var dabs = window.buildBrushDabs(demo, preset, baseWidth);
    dabs.forEach(function (dab) {
      ctx.globalAlpha = dab.data.dabOpacity;
      ctx.fillStyle = textColor;
      traceOnCanvas2D(ctx, dab);
      ctx.fill();
      dab.remove();
    });
    ctx.globalAlpha = 1;
  }

  function builtinGroups() {
    return [
      { label: 'None', keys: ['none'] },
      { label: 'Chalk', keys: ['chalk-blunt', 'chalk-round', 'chalk-scribble'] },
      { label: 'Charcoal', keys: ['charcoal-feather', 'charcoal-pencil', 'charcoal-rough', 'charcoal-rounded', 'charcoal-smooth', 'charcoal-soft', 'charcoal-tapered', 'charcoal-thick', 'charcoal-thin', 'charcoal-varied'] },
      { label: 'Pencil', keys: ['pencil-feather', 'pencil-thick', 'pencil-thin'] },
      { label: 'Formes de pointe', keys: ['marker-flat', 'ink-chisel', 'pastel-chip', 'chalk-facet', 'ink-splatter', 'drybrush-bristle', 'watercolor-edge'] },
      { label: 'Distribution (rnote)', keys: ['airbrush-soft', 'marker-dry'] },
      { label: 'Scribble-fill', keys: ['graphite-scribble'] },
    ];
  }
  function customKeys() {
    return Object.keys((window.state && state.customBrushPresets) || {});
  }
  function labelFor(key) {
    if (key === 'none') return 'None (solid)';
    if (window.state && state.customBrushPresets && state.customBrushPresets[key] && state.customBrushPresets[key].label) return state.customBrushPresets[key].label;
    return key.split('-').map(function (w) { return w[0].toUpperCase() + w.slice(1); }).join(' - ');
  }

  function paintButton(presetKey) {
    var canvas = document.getElementById('p-brushpreset-preview');
    var label = document.getElementById('p-brushpreset-label');
    if (canvas) drawPreview(canvas, presetKey);
    if (label) label.textContent = labelFor(presetKey || 'none');
  }
  // Sets state.brushPreset directly rather than round-tripping through the
  // hidden legacy <select id="p-brushpreset"> — that select's static
  // <option> list only ever knew about the 16 built-ins, so `sel.value =
  // customKey` silently no-opped (the browser leaves .value unchanged when
  // asked for a value with no matching <option>) the moment custom presets
  // existed. The <select> still exists in the DOM for anything that reads
  // its markup, but is no longer the source of truth.
  function selectPreset(key) {
    // setBrushPreset (timeline.js, SM.setBrushPreset) ALREADY retroactively
    // re-textures the current selection itself (own pushUndo/
    // stripAnyBrushTexture+applyBrushTexture/saveActiveLayerFrame, added
    // 2026-07-17 — "les changement de brush dynamique ne s'applique pas au
    // stroke de la selection") — this function used to ALSO do its own,
    // essentially identical, retroactive-apply block on top of that,
    // silently double-applying (two undo entries, two re-stamps with a
    // fresh random seed each) every time a preset was picked from this
    // popover with an eligible selection active. Found via the floating
    // Brush panel's own preset grid (brush-menu-bridge.js, 2026-07
    // harmonization pass) calling this same function — caught by checking
    // state.undoStack.length after a single click, not just eyeballing the
    // result, which is why it went unnoticed this long.
    if (window.SM) window.SM.setBrushPreset(key);
    paintButton(key);
  }

  function open(anchorEl, currentKey, onSelect) {
    closePopover();
    var el = document.createElement('div');
    el.className = 'ctx-menu bp-picker-pop';
    var html = '';
    builtinGroups().forEach(function (g) {
      html += '<div class="bp-group-label">' + g.label + '</div><div class="bp-grid">';
      g.keys.forEach(function (k) {
        html += '<button class="bp-item' + (k === currentKey ? ' active' : '') + '" data-key="' + k + '" title="' + labelFor(k) + '">' +
          '<canvas width="150" height="20"></canvas><span>' + labelFor(k) + '</span></button>';
      });
      html += '</div>';
    });
    var custom = customKeys();
    if (custom.length) {
      html += '<div class="bp-group-label">Mes brushes</div><div class="bp-grid">';
      custom.forEach(function (k) {
        html += '<button class="bp-item bp-item-custom' + (k === currentKey ? ' active' : '') + '" data-key="' + k + '" title="' + labelFor(k) + '">' +
          '<canvas width="150" height="20"></canvas><span>' + labelFor(k) + '</span>' +
          '<span class="bp-item-del" data-del="' + k + '" title="Supprimer">&times;</span></button>';
      });
      html += '</div>';
    }
    html += '<button class="bp-edit-btn" id="bp-open-editor">Éditer / créer un brush…</button>';
    el.innerHTML = html;
    document.body.appendChild(el);
    popover = el;

    el.querySelectorAll('.bp-item').forEach(function (btn) {
      drawPreview(btn.querySelector('canvas'), btn.dataset.key);
      btn.addEventListener('mouseenter', function () { hoverPreview(btn.dataset.key); });
      btn.addEventListener('mouseleave', function () { revertHoverPreview(); });
      btn.addEventListener('click', function (e) {
        if (e.target.classList.contains('bp-item-del')) return;
        // Revert the hover preview BEFORE the real commit — onSelect's own
        // pushUndo (via setBrushPreset) must snapshot the true pre-hover
        // state, not the already-mutated preview, or Ctrl+Z would restore
        // to the hovered-but-not-picked brush instead of the original.
        revertHoverPreview();
        onSelect(btn.dataset.key);
        // Stay open (2026-07 feedback: "il faut le même [que le menu
        // flottant]... celui-ci disparait une fois que l'on a select une
        // brush") — same convention as the floating Brush Menu's own grid,
        // which only re-highlights the active item instead of closing, so
        // several presets can be tried/compared in one popover visit.
        el.querySelectorAll('.bp-item').forEach(function (b) { b.classList.toggle('active', b.dataset.key === btn.dataset.key); });
      });
    });
    el.querySelectorAll('.bp-item-del').forEach(function (delBtn) {
      delBtn.addEventListener('click', function (e) {
        e.preventDefault(); e.stopPropagation();
        var key = delBtn.dataset.del;
        if (window.state && state.customBrushPresets) delete state.customBrushPresets[key];
        if (state.brushPreset === key) { state.brushPreset = 'none'; paintButton('none'); }
        closePopover();
        open(anchorEl, currentKey, onSelect);
      });
    });
    var editBtn = el.querySelector('#bp-open-editor');
    if (editBtn) editBtn.addEventListener('click', function () {
      closePopover();
      if (window.BrushEditor) window.BrushEditor.open(currentKey && currentKey !== 'none' ? currentKey : null, onSelect);
    });

    var ar = anchorEl.getBoundingClientRect();
    el.style.visibility = 'hidden'; el.style.display = 'block';
    var ew = el.offsetWidth, eh = el.offsetHeight;
    var left = Math.min(ar.left, window.innerWidth - ew - 8);
    var top = Math.min(ar.bottom + 6, window.innerHeight - eh - 8);
    el.style.left = Math.max(4, left) + 'px'; el.style.top = Math.max(4, top) + 'px';
    el.style.visibility = '';

    function onOutside(e) { if (!el.contains(e.target) && e.target !== anchorEl) closePopover(); }
    function onKey(e) { if (e.key === 'Escape') closePopover(); }
    document.addEventListener('mousedown', onOutside, true);
    document.addEventListener('keydown', onKey);
    closeHandlers = function () {
      document.removeEventListener('mousedown', onOutside, true);
      document.removeEventListener('keydown', onKey);
    };
  }

  window.BrushPresetPicker = {
    open: open, paintButton: paintButton, drawPreview: drawPreview, labelFor: labelFor, selectPreset: selectPreset,
    // Exposed for brush-menu-bridge.js's own big two-tab picker (2026-07) —
    // reuses this file's exact catalog/preview/select logic rather than
    // keeping a second copy that could drift out of sync.
    groups: builtinGroups, customKeys: customKeys,
  };

  function init() {
    var btn = document.getElementById('p-brushpreset-btn');
    if (!btn) return;
    btn.addEventListener('click', function (e) {
      e.preventDefault(); e.stopPropagation();
      open(btn, state.brushPreset || 'none', selectPreset);
    });
    paintButton(state.brushPreset || 'none');
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', init);
  else init();
})();
