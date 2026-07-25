// ---- MOTION GRAPH EDITOR (2026-07-25) ----
// After Effects' Graph Editor: the interpolation itself, drawn and edited as a
// curve over time, instead of inferred from diamonds on a track.
//
// Lives in its own file because motion.js is already 2900 lines and this needs
// none of its internals — it reads through the SMMotion API and writes back
// through the same key objects the track view already renders.
//
// WHY AN OVERLAY, NOT A ROW MODE. motion.js's panel/grid split rests on one
// invariant its comments return to over and over: every .lrow in #layer-list
// must line up with a .frow in #frame-grid, and any render path that emits a
// different number of rows on one side silently shifts every track below it.
// A graph editor built as rows would have to participate in that invariant on
// every zoom, expand and filter change. Instead this is one absolutely
// positioned box inside #fg-wrap — the scroller — so it inherits horizontal
// scroll and the live FC (frame column width) for free, and the row machinery
// never has to know it exists.
(function () {
  'use strict';

  var HDR_OFFSET = 42;      // #frame-hdr (20) + #bars-row (22), same constant index.html documents
  var PAD_T = 26, PAD_B = 22;
  var MIN_H = 180;
  var HANDLE_R = 4;

  var _on = false;
  var _mode = 'value';      // 'value' | 'speed'
  var _fit = null;          // {min,max} locked range, or null to auto-fit
  var _drag = null;
  var _el = null, _svg = null;

  // One colour per property/dimension. Position and Scale are 2D, so their two
  // dimensions must stay distinguishable from each other AND from the other
  // properties — otherwise a graph with four curves is unreadable.
  var COLORS = {
    'position.0': '#4fa3ff', 'position.1': '#7ee081',
    'anchor.0': '#b48ead', 'anchor.1': '#d0a3c8',
    'scale.0': '#ffb86c', 'scale.1': '#ff9f43',
    'rotation.0': '#ff6b81',
    'opacity.0': '#c9d1d9',
  };
  var DIM_SUFFIX = { position: ['X', 'Y'], anchor: ['X', 'Y'], scale: ['W', 'H'], rotation: [''], opacity: [''] };

  function M() { return window.SMMotion; }
  function fc() { return window.FC || 30; }
  function xForFrame(f) { return f * fc() + fc() / 2; }
  function frameForX(x) { return Math.round((x - fc() / 2) / fc()); }

  // Which curves to show. AE scopes the graph to the SELECTED properties and
  // falls back to everything animated on the layer — same rule here, because a
  // graph showing all five properties at once is exactly as unreadable in this
  // app as it is there.
  function tracks() {
    var out = [];
    var sel = M() && M().getKeySelection ? M().getKeySelection() : [];
    var wanted = null;
    if (sel && sel.length) {
      wanted = {};
      sel.forEach(function (s) { wanted[(state.layers.indexOf(s.holder)) + '|' + s.prop] = true; });
    }
    state.layers.forEach(function (ld, li) {
      if (!ld || !ld.motion) return;
      var expanded = (window._motionExpandedLayer === li) ||
        (window._motionRevealedLayers && window._motionRevealedLayers.indexOf(li) >= 0);
      Object.keys(ld.motion).forEach(function (prop) {
        var trk = ld.motion[prop];
        if (!trk || !trk.keys || trk.keys.length < 1) return;
        if (wanted) { if (!wanted[li + '|' + prop]) return; }
        else if (!expanded) return;
        var dims = (trk.keys[0].v || []).length || 1;
        for (var d = 0; d < dims; d++) out.push({ ld: ld, li: li, prop: prop, dim: d, track: trk });
      });
    });
    return out;
  }

  function sampleCurve(t) {
    var pts = [];
    var f0 = t.track.keys[0].frame, f1 = t.track.keys[t.track.keys.length - 1].frame;
    if (f1 <= f0) f1 = f0 + 1;
    for (var f = f0; f <= f1; f++) {
      var v = M().valueAtFrame(t.ld, t.prop, f);
      pts.push([f, Array.isArray(v) ? v[t.dim] : v]);
    }
    if (_mode === 'speed') {
      // Speed = |dv/dframe|, the derivative animators actually read: a flat
      // stretch is constant velocity, a dip to zero is a hold. Plotted at the
      // midpoint of each frame pair so it lines up with the motion it
      // describes rather than leading it by half a frame.
      var sp = [];
      for (var i = 1; i < pts.length; i++) sp.push([(pts[i][0] + pts[i - 1][0]) / 2, Math.abs(pts[i][1] - pts[i - 1][1])]);
      return sp;
    }
    return pts;
  }

  function rangeOf(all) {
    if (_fit) return _fit;
    var min = Infinity, max = -Infinity;
    all.forEach(function (c) { c.pts.forEach(function (p) { if (p[1] < min) min = p[1]; if (p[1] > max) max = p[1]; }); });
    if (!isFinite(min) || !isFinite(max)) { min = 0; max = 1; }
    if (Math.abs(max - min) < 1e-6) { min -= 1; max += 1; }
    var pad = (max - min) * 0.12;
    return { min: min - pad, max: max + pad };
  }

  function host() { return document.getElementById('fg-wrap'); }

  function ensureEl() {
    if (_el && _el.parentNode) return _el;
    var w = host(); if (!w) return null;
    _el = document.createElement('div');
    _el.id = 'motion-graph';
    _el.addEventListener('contextmenu', function (e) { e.preventDefault(); e.stopPropagation(); });
    w.appendChild(_el);
    return _el;
  }

  function height() {
    var w = host();
    var h = w ? w.clientHeight - HDR_OFFSET : 0;
    return Math.max(MIN_H, h);
  }

  function render() {
    if (!_on) { if (_el) _el.style.display = 'none'; return; }
    var el = ensureEl(); if (!el) return;
    el.style.display = 'block';
    var W = state.totalFrames * fc(), H = height();
    el.style.top = HDR_OFFSET + 'px';
    el.style.width = W + 'px';
    el.style.height = H + 'px';

    var all = tracks().map(function (t) { t.pts = sampleCurve(t); return t; });
    var rg = rangeOf(all);
    var innerH = H - PAD_T - PAD_B;
    function yFor(v) { return PAD_T + innerH - ((v - rg.min) / (rg.max - rg.min)) * innerH; }
    el._yFor = yFor; el._rg = rg; el._innerH = innerH;

    var svg = '<svg width="' + W + '" height="' + H + '" style="display:block">';
    // horizontal value gridlines, labelled — a graph without a scale is a
    // shape, not a measurement
    var STEPS = 4;
    for (var g = 0; g <= STEPS; g++) {
      var vv = rg.min + (rg.max - rg.min) * (g / STEPS), yy = yFor(vv);
      svg += '<line x1="0" y1="' + yy + '" x2="' + W + '" y2="' + yy + '" stroke="var(--border)" stroke-width="1" opacity="0.5"/>';
      svg += '<text x="4" y="' + (yy - 3) + '" font-size="9" fill="var(--text-dim)">' + (Math.round(vv * 10) / 10) + '</text>';
    }
    // playhead
    var px = xForFrame(state.currentFrame);
    svg += '<line x1="' + px + '" y1="0" x2="' + px + '" y2="' + H + '" stroke="var(--accent)" stroke-width="1" opacity="0.8"/>';

    all.forEach(function (t, ti) {
      var col = COLORS[t.prop + '.' + t.dim] || '#8ab4f8';
      var d = t.pts.map(function (p, i) { return (i ? 'L' : 'M') + xForFrame(p[0]).toFixed(1) + ' ' + yFor(p[1]).toFixed(1); }).join(' ');
      svg += '<path d="' + d + '" fill="none" stroke="' + col + '" stroke-width="1.8" stroke-linejoin="round"/>';
      if (_mode === 'value') {
        t.track.keys.forEach(function (k, ki) {
          var v = Array.isArray(k.v) ? k.v[t.dim] : k.v;
          var selNow = M().getKeySelection().some(function (s) { return s.key === k && s.prop === t.prop; });
          svg += '<circle class="mg-key" data-t="' + ti + '" data-k="' + ki + '" cx="' + xForFrame(k.frame).toFixed(1) + '" cy="' + yFor(v).toFixed(1) +
            '" r="' + (selNow ? HANDLE_R + 1.5 : HANDLE_R) + '" fill="' + (selNow ? '#fff' : col) + '" stroke="' + col + '" stroke-width="2"/>';
        });
        // Ease waypoints of each segment, placed ON the curve they shape —
        // dragging one is the whole point of a graph editor, and putting them
        // anywhere else would make the connection to the curve guesswork.
        for (var s2 = 0; s2 < t.track.keys.length - 1; s2++) {
          var a = t.track.keys[s2], b = t.track.keys[s2 + 1];
          if (a.hold) continue;
          var va = Array.isArray(a.v) ? a.v[t.dim] : a.v, vb = Array.isArray(b.v) ? b.v[t.dim] : b.v;
          var cps = a.curvePoints || [];
          for (var w2 = 1; w2 < cps.length - 1; w2++) {
            var cx = xForFrame(a.frame + (b.frame - a.frame) * cps[w2].x);
            var cy = yFor(va + (vb - va) * cps[w2].y);
            svg += '<circle class="mg-ease" data-t="' + ti + '" data-s="' + s2 + '" data-w="' + w2 + '" cx="' + cx.toFixed(1) + '" cy="' + cy.toFixed(1) +
              '" r="3" fill="var(--panel)" stroke="' + col + '" stroke-width="1.5" opacity="0.9"/>';
          }
        }
      }
    });
    svg += '</svg>';

    // legend + mode readout, pinned to the left of the visible scroll window so
    // it stays readable however far along the timeline you are
    var legend = all.map(function (t) {
      var col = COLORS[t.prop + '.' + t.dim] || '#8ab4f8';
      var suf = (DIM_SUFFIX[t.prop] || [''])[t.dim] || '';
      return '<span style="color:' + col + ';margin-right:12px">&#9632;&nbsp;' + t.prop + (suf ? '&nbsp;' + suf : '') + '</span>';
    }).join('');
    el.innerHTML = svg +
      '<div class="mg-legend" style="left:' + (host().scrollLeft + 8) + 'px">' + legend +
      '<span style="color:var(--text-dim);margin-left:6px">' + (_mode === 'speed' ? 'vitesse' : 'valeur') + (_fit ? ' · figé' : '') + '</span></div>';
    _svg = el.querySelector('svg');
    el._tracks = all;
  }

  // ---- interaction ----
  function localPt(e) {
    var r = _svg.getBoundingClientRect();
    return { x: e.clientX - r.left, y: e.clientY - r.top };
  }
  function onDown(e) {
    if (!_on) return;
    var tgt = e.target;
    if (!tgt || !tgt.classList) return;
    var all = _el._tracks || [];
    if (tgt.classList.contains('mg-key')) {
      var t = all[+tgt.dataset.t], k = t.track.keys[+tgt.dataset.k];
      if (window.pushUndo) pushUndo();
      // Share the track view's selection rather than keeping a private one:
      // clicking a point here lights up its diamond, and F9 / Delete / copy
      // then act on it exactly as they would from the track view. Shift adds,
      // matching the track view's own multi-select.
      if (M().selectKeys) {
        var cur = M().getKeySelection();
        var already = cur.some(function (s) { return s.key === k; });
        if (e.shiftKey) { if (!already) M().selectKeys(cur.concat([{ holder: t.ld, prop: t.prop, key: k }])); }
        else if (!already) M().selectKeys([{ holder: t.ld, prop: t.prop, key: k }]);
      }
      _drag = { kind: 'key', t: t, key: k, startFrame: k.frame, start: localPt(e), startV: (Array.isArray(k.v) ? k.v[t.dim] : k.v) };
      e.preventDefault(); e.stopPropagation();
    } else if (tgt.classList.contains('mg-ease')) {
      var t2 = all[+tgt.dataset.t], seg = +tgt.dataset.s, wi = +tgt.dataset.w;
      if (window.pushUndo) pushUndo();
      _drag = { kind: 'ease', t: t2, seg: seg, wi: wi, start: localPt(e) };
      e.preventDefault(); e.stopPropagation();
    }
  }
  function onMove(e) {
    if (!_drag) return;
    var p = localPt(e), rg = _el._rg, innerH = _el._innerH;
    var perPx = (rg.max - rg.min) / innerH;
    if (_drag.kind === 'key') {
      var t = _drag.t, k = _drag.key;
      // vertical = value
      var nv = _drag.startV - (p.y - _drag.start.y) * perPx;
      if (Array.isArray(k.v)) k.v[t.dim] = nv; else k.v = nv;
      // horizontal = time, refused rather than clamped when it would land on
      // another key — same rule as the track view's nudge, so a drag can never
      // silently merge two keyframes
      var nf = _drag.startFrame + Math.round((p.x - _drag.start.x) / fc());
      if (nf >= 0 && nf <= state.totalFrames - 1) {
        var clash = t.track.keys.some(function (o) { return o !== k && o.frame === nf; });
        if (!clash && nf !== k.frame) {
          k.frame = nf;
          t.track.keys.sort(function (a, b) { return a.frame - b.frame; });
        }
      }
    } else {
      var t3 = _drag.t, a = t3.track.keys[_drag.seg], b = t3.track.keys[_drag.seg + 1];
      var va = Array.isArray(a.v) ? a.v[t3.dim] : a.v, vb = Array.isArray(b.v) ? b.v[t3.dim] : b.v;
      var cps = a.curvePoints; if (!cps) return;
      var span = (b.frame - a.frame) * fc();
      var nx = (p.x - xForFrame(a.frame)) / (span || 1);
      var vy = rg.min + (innerH - (p.y - PAD_T)) * perPx;
      var ny = Math.abs(vb - va) > 1e-9 ? (vy - va) / (vb - va) : cps[_drag.wi].y;
      // Waypoints must stay ordered in x, or evalCurvePoints' segment search
      // walks past the point it wants and the ease inverts.
      var lo = cps[_drag.wi - 1].x + 0.02, hi = cps[_drag.wi + 1].x - 0.02;
      cps[_drag.wi].x = Math.max(lo, Math.min(hi, nx));
      cps[_drag.wi].y = Math.max(-1, Math.min(2, ny));
      delete cps[_drag.wi].tx; delete cps[_drag.wi].ty;
    }
    render();
    if (window.renderLayerList) renderLayerList();
    if (window.SMEngineBridge) SMEngineBridge.renderNow();
  }
  function onUp() {
    if (!_drag) return;
    _drag = null;
    if (window.renderTimeline) renderTimeline();
    render();
  }

  function toggle(on) {
    _on = (on === undefined) ? !_on : !!on;
    var grid = document.getElementById('frame-grid');
    // The track rows stay in the DOM (their geometry is what every other
    // module measures) — they are only hidden, so toggling back is free and
    // nothing downstream has to re-render.
    if (grid) grid.style.visibility = _on ? 'hidden' : '';
    var btn = document.getElementById('btn-mgraph');
    if (btn) btn.classList.toggle('on', _on);
    render();
    return _on;
  }
  function setMode(m) { _mode = m; _fit = null; render(); }
  function freezeRange() {
    // Lock the current auto-fit so editing a value doesn't make the whole graph
    // rescale under the cursor mid-drag — AE's own "lock zoom" affordance.
    if (_fit) _fit = null; else if (_el && _el._rg) _fit = { min: _el._rg.min, max: _el._rg.max };
    render();
  }

  // Toolbar button: plain click toggles, right-click swaps value/speed, Alt+
  // click freezes the vertical scale. Three actions on one button rather than
  // three buttons — the toolbar is already dense, and the two secondary ones
  // are modifiers of the first, not peers of it.
  document.addEventListener('DOMContentLoaded', function () {
    var b = document.getElementById('btn-mgraph');
    if (!b) return;
    b.addEventListener('click', function (e) {
      if (e.altKey) { if (_on) freezeRange(); return; }
      toggle();
    });
    b.addEventListener('contextmenu', function (e) {
      e.preventDefault();
      if (!_on) toggle(true);
      setMode(_mode === 'value' ? 'speed' : 'value');
    });
  });

  document.addEventListener('mousedown', onDown, true);
  document.addEventListener('mousemove', onMove);
  document.addEventListener('mouseup', onUp);
  window.addEventListener('resize', function () { if (_on) render(); });
  document.addEventListener('scroll', function (e) {
    if (_on && e.target && e.target.id === 'fg-wrap') {
      var lg = _el && _el.querySelector('.mg-legend');
      if (lg) lg.style.left = (e.target.scrollLeft + 8) + 'px';
    }
  }, true);

  window.SMMotionGraph = {
    isOn: function () { return _on; },
    toggle: toggle,
    render: render,
    setMode: setMode,
    mode: function () { return _mode; },
    freezeRange: freezeRange,
    isFrozen: function () { return !!_fit; },
  };
})();
