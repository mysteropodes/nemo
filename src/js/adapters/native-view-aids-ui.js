/* Native view aids (#2373): Safety zones, Clip to canvas and the alpha checker on a native OBJECT project, as pure view overlays.
 * The buttons #btn-safety, #btn-clip and #btn-alphabg only flipped state.* and asked the legacy scene builder (engine-bridge.js)
 * to redraw; a native object project builds no legacy scene, so the button lit up and nothing changed. The aids are now drawn as
 * two disposable SVGs over the canvas, mapped through the native presentation view (documentToComposition, like
 * native-select-overlay.js): they read the transient view flags only, send no engine command, never touch the document revision
 * and live in the DOM, so no export (native, a separate render path) can contain them.
 * Baseline look (1065e9d8 engine-bridge buildSafetyZoneItems / buildClipMaskItems / showAlphaChecker): safety rectangles inset 5%
 * (action, rgb 255,210,0) and 10% (title, rgb 255,90,90) at 200/255 alpha and 1 screen px; four #232330 bands outside the artboard
 * ("clip"); an 8 screen px checker of 222/255 tiles, capped at 4000 tiles. Limit: the native frame is already painted with the
 * document background underneath the DOM, so the checker can only DARKEN alternate tiles by the baseline step (255 -> 222); on a
 * white background that is the baseline pixel exactly, on another background it is a tint (a true replacement needs an engine
 * render option and is outside a view overlay). */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeViewAids = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  var NS = 'http://www.w3.org/2000/svg';
  var FLAGS = { clip: 'canvasClip', safety: 'safetyZones', alpha: 'previewAlphaBg' };
  var CLIP_RGB = 'rgb(35,35,48)', CLIP_SPAN = 100000;
  var ACTION_RGB = 'rgb(255,210,0)', TITLE_RGB = 'rgb(255,90,90)', ZONE_OPACITY = String(200 / 255);
  var CHECK_PX = 8, MAX_TILES = 4000, CHECK_DARKEN = String((255 - 222) / 255);
  var states = new WeakMap();
  function stateOf(root) {
    var s = states.get(root);
    if (!s) {
      s = { under: null, over: null, sig: '', scheduled: false }; states.set(root, s);
      if (typeof root.addEventListener === 'function') root.addEventListener('resize', function () { notify(root); });
    }
    return s;
  }
  function flags(root) {
    var st = root.state || {};
    return { clip: !!st.canvasClip, safety: !!st.safetyZones, alpha: !!st.previewAlphaBg };
  }
  // The current native presentation and the canvas rectangle, or null when no native object project is presented.
  function capture(root) {
    try {
      var I = root.NemoSelectCanvasIntent, M = root.NemoNativeViewportUnderlay && root.NemoNativeViewportUnderlay.canvasMapping;
      var shown = I && M && I.presentedState(root), el = shown && root.document.getElementById('drawing-canvas');
      if (!el) return null;
      var r = el.getBoundingClientRect();
      if (!(r.width > 0 && r.height > 0) || !(shown.pin.canvasW > 0) || !(shown.pin.canvasH > 0)) return null;
      return { pin: shown.pin, bounds: { left: r.left, top: r.top, width: r.width, height: r.height },
        V: M.transform({ presentation: shown.presentation }, false) };
    } catch (_) { return null; }
  }
  function node(root, tag, attrs) {
    var n = root.document.createElementNS(NS, tag);
    Object.keys(attrs).forEach(function (k) { n.setAttribute(k, attrs[k]); });
    return n;
  }
  function at(V, x, y) { return [V[0] * x + V[2] * y + V[4], V[1] * x + V[3] * y + V[5]]; }
  function rect(V, x0, y0, x1, y1) { return [at(V, x0, y0), at(V, x1, y0), at(V, x1, y1), at(V, x0, y1)]; }
  function points(list) { return list.map(function (p) { return p.join(','); }).join(' '); }
  function surface(root, c, z) {
    var svg = node(root, 'svg', { viewBox: '0 0 ' + c.pin.canvasW + ' ' + c.pin.canvasH, preserveAspectRatio: 'none', 'aria-hidden': 'true' });
    Object.assign(svg.style, { position: 'fixed', pointerEvents: 'none', zIndex: z, left: c.bounds.left + 'px', top: c.bounds.top + 'px',
      width: c.bounds.width + 'px', height: c.bounds.height + 'px' });
    return svg;
  }
  // Screen pixels per document unit (the baseline view.zoom).
  function zoomOf(c) { return Math.hypot(c.V[0], c.V[1]) * c.bounds.width / c.pin.canvasW; }
  // The baseline checker grid: an 8 screen px tile, grown past that only to stay within 4000 tiles.
  function checkerGrid(w, h, zoom) {
    var tile = CHECK_PX / Math.max(0.001, zoom), cols = w / tile, rows = h / tile;
    if (cols * rows > MAX_TILES) tile *= Math.sqrt((cols * rows) / MAX_TILES);
    var nx = Math.max(1, Math.round(w / tile)), ny = Math.max(1, Math.round(h / tile));
    return { cols: nx, rows: ny, tileW: w / nx, tileH: h / ny };
  }
  function checker(root, c) {
    var w = c.pin.canvasW, h = c.pin.canvasH, g = checkerGrid(w, h, zoomOf(c)), d = [];
    for (var cy = 0; cy < g.rows; cy++) for (var cx = 0; cx < g.cols; cx++) {
      if ((cx + cy) % 2 !== 0) continue; // the baseline's darker tile is (cx + cy) even
      d.push('M ' + cx * g.tileW + ' ' + cy * g.tileH + ' h ' + g.tileW + ' v ' + g.tileH + ' h ' + -g.tileW + ' Z');
    }
    var group = node(root, 'g', { transform: 'matrix(' + c.V.join(' ') + ')' });
    group.appendChild(node(root, 'path', { d: d.join(' '), fill: '#000000', 'fill-opacity': CHECK_DARKEN, stroke: 'none' }));
    return group;
  }
  function clip(root, c) {
    var w = c.pin.canvasW, h = c.pin.canvasH, outer = rect([1, 0, 0, 1, 0, 0], -CLIP_SPAN, -CLIP_SPAN, CLIP_SPAN, CLIP_SPAN), inner = rect(c.V, 0, 0, w, h);
    var d = function (p) { return 'M ' + p.map(function (q) { return q.join(' '); }).join(' L ') + ' Z'; };
    return node(root, 'path', { d: d(outer) + ' ' + d(inner), 'fill-rule': 'evenodd', fill: CLIP_RGB, stroke: 'none' });
  }
  function safety(root, c) {
    var w = c.pin.canvasW, h = c.pin.canvasH;
    return [[0.05, ACTION_RGB], [0.10, TITLE_RGB]].map(function (z) {
      var mx = w * z[0], my = h * z[0];
      return node(root, 'polygon', { points: points(rect(c.V, mx, my, w - mx, h - my)), fill: 'none', stroke: z[1], 'stroke-opacity': ZONE_OPACITY,
        'stroke-width': '1', 'vector-effect': 'non-scaling-stroke' });
    });
  }
  function drop(s) {
    ['under', 'over'].forEach(function (k) { if (s[k]) { s[k].remove(); s[k] = null; } });
    s.sig = '';
  }
  // The single (re)draw from the view flags and the current presentation; nothing is drawn when neither holds.
  function refresh(root, force) {
    var s = stateOf(root), f = flags(root), c = (f.clip || f.safety || f.alpha) ? capture(root) : null;
    if (!c) { drop(s); return; }
    var sig = JSON.stringify([f, c.V, c.bounds, c.pin.canvasW, c.pin.canvasH]);
    if (!force && sig === s.sig && (s.under || s.over)) return;
    drop(s); s.sig = sig;
    if (f.alpha || f.clip) {
      var under = surface(root, c, '10');
      if (f.alpha) under.appendChild(checker(root, c));
      if (f.clip) under.appendChild(clip(root, c));
      root.document.body.appendChild(under); s.under = under;
    }
    if (f.safety) {
      var over = surface(root, c, '21');
      safety(root, c).forEach(function (n) { over.appendChild(n); });
      root.document.body.appendChild(over); s.over = over;
    }
  }
  // The presentation, view or canvas rectangle may have changed: redraw once on the next frame (one-shot, only while an aid is on).
  function notify(root) {
    var s = stateOf(root), f = flags(root);
    if (s.scheduled || !(f.clip || f.safety || f.alpha || s.under || s.over) || !root.requestAnimationFrame) return;
    s.scheduled = true;
    root.requestAnimationFrame(function () { s.scheduled = false; refresh(root, false); });
  }
  function objectProject(root) {
    var I = root.NemoSelectCanvasIntent;
    return !!(I && typeof I.objectProject === 'function' && I.objectProject(root));
  }
  // A button click on a native object project: flip the view flag, mirror the button and redraw. Returns false when the
  // legacy path owns the click (no native object project), so the caller keeps its own behaviour.
  function toggle(root, kind, button) {
    var key = FLAGS[kind];
    if (!key || !root.state || !objectProject(root)) return false;
    root.state[key] = !root.state[key];
    if (button && button.classList) button.classList.toggle('active', !!root.state[key]);
    refresh(root, true);
    return true;
  }
  return Object.freeze({ toggle: toggle, notify: notify, refresh: function (root) { refresh(root, false); }, checkerGrid: checkerGrid });
}));
