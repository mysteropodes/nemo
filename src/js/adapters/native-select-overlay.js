/* Native Select presentation (#2372): the selection outline, oriented box with its 8 handles and the hover box, drawn as a
 * disposable SVG over the canvas. A native object project builds no Paper items and the native renderer has no Select overlay
 * producer, so a successful pick was invisible. This mirrors subselect-bridge's draw(): position:fixed, pointer-events:none,
 * viewBox canvasW x canvasH, world points mapped through NemoNativeViewportUnderlay.canvasMapping. Presentation only: it reads
 * root.selectedPaths (transient, never-inserted Paper paths) and owns no document state and no writer. Baseline look (1065e9d8
 * engine-bridge buildTransformBoxItems / buildHoverBoxItems): blue [74,158,255] box 1px at 0.8 alpha, 8 white handles with a
 * blue 1.2px edge and a 3.5px half-size, hover box 1.5px at 0.86 alpha, all sizes in screen pixels. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeSelectOverlay = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  var NS = 'http://www.w3.org/2000/svg', BLUE = 'rgb(74,158,255)', HANDLE = 3.5;
  var states = new WeakMap();
  function state(root) {
    var s = states.get(root);
    if (!s) { s = { svg: null, sel: null, hov: null, prev: null, sig: '', watching: false, busy: false, next: null }; states.set(root, s); }
    return s;
  }
  // What the overlay maps through, or null when no native presentation is current. `ctx` is the pick context (tool, mode,
  // frame, layer, document revision): a part drawn under another context is stale and is dropped.
  function capture(root) {
    try {
      var I = root.NemoSelectCanvasIntent, M = root.NemoNativeViewportUnderlay && root.NemoNativeViewportUnderlay.canvasMapping;
      var shown = I && M && I.presentedState(root), el = shown && root.document.getElementById('drawing-canvas');
      if (!el) return null;
      var r = el.getBoundingClientRect();
      if (!(r.width > 0 && r.height > 0) || !(shown.pin.canvasW > 0) || !(shown.pin.canvasH > 0)) return null;
      return { ctx: I.pickContext(root), pin: shown.pin, bounds: { left: r.left, top: r.top, width: r.width, height: r.height },
        V: M.transform({ presentation: shown.presentation }, false) };
    } catch (_) { return null; }
  }
  function at(V, x, y) { return [V[0] * x + V[2] * y + V[4], V[1] * x + V[3] * y + V[5]]; }
  function node(root, tag, attrs) {
    var n = root.document.createElementNS(NS, tag);
    Object.keys(attrs).forEach(function (k) { n.setAttribute(k, attrs[k]); });
    return n;
  }
  function line(root, extra) {
    return Object.assign({ fill: 'none', 'vector-effect': 'non-scaling-stroke' }, extra);
  }
  // The selection's oriented box (tools.js orientedSelBox): the union of the paths' bounds, or, when every path carries the
  // same data.boxAngle, the tight box in the de-rotated space mapped back to world around the union's centre. A mixed
  // selection falls back to the axis-aligned union, as in the baseline. Returns the four world corners and the 8 handles.
  function orientedBox(root, paths) {
    var list = (paths || []).filter(function (p) { return p && p.bounds; });
    if (!list.length) return null;
    var union = function (rects) {
      return rects.reduce(function (u, r) {
        return u ? { left: Math.min(u.left, r.left), top: Math.min(u.top, r.top), right: Math.max(u.right, r.right), bottom: Math.max(u.bottom, r.bottom) } : { left: r.left, top: r.top, right: r.right, bottom: r.bottom };
      }, null);
    };
    var b = union(list.map(function (p) { return p.bounds; }));
    var angle = (list[0].data && list[0].data.boxAngle) || 0;
    var pivot = { x: (b.left + b.right) / 2, y: (b.top + b.bottom) / 2 };
    var mixed = list.some(function (p) { return Math.abs(((p.data && p.data.boxAngle) || 0) - angle) > 0.01; });
    var Point = root.Point || (root.paper && root.paper.Point);
    if (angle && !mixed && Point && typeof list[0].clone === 'function') {
      b = union(list.map(function (p) {
        var c = p.clone({ insert: false }), r;
        c.rotate(-angle, new Point(pivot.x, pivot.y)); r = c.bounds; c.remove();
        return r;
      }));
    } else angle = 0;
    var rad = angle * Math.PI / 180, cos = Math.cos(rad), sin = Math.sin(rad);
    var pt = function (x, y) {
      var dx = x - pivot.x, dy = y - pivot.y;
      return angle ? [pivot.x + dx * cos - dy * sin, pivot.y + dx * sin + dy * cos] : [x, y];
    };
    var mx = (b.left + b.right) / 2, my = (b.top + b.bottom) / 2;
    var mid = pt(mx, my);
    return { center: { x: mid[0], y: mid[1] }, width: b.right - b.left, height: b.bottom - b.top, angle: angle,
      corners: [pt(b.left, b.top), pt(b.right, b.top), pt(b.right, b.bottom), pt(b.left, b.bottom)],
      handles: [pt(b.left, b.top), pt(mx, b.top), pt(b.right, b.top), pt(b.right, my), pt(b.right, b.bottom), pt(mx, b.bottom), pt(b.left, b.bottom), pt(b.left, my)] };
  }
  function outline(V, p) {
    var segs = p && p.segments;
    if (!segs || segs.length < 2) return '';
    var w = function (x, y) { return at(V, x, y).join(' '); }, start = segs[0].point, d = 'M ' + w(start.x, start.y);
    var curve = function (a, c) {
      d += ' C ' + w(a.point.x + a.handleOut.x, a.point.y + a.handleOut.y) + ' ' + w(c.point.x + c.handleIn.x, c.point.y + c.handleIn.y) + ' ' + w(c.point.x, c.point.y);
    };
    for (var i = 1; i < segs.length; i++) curve(segs[i - 1], segs[i]);
    if (p.closed) { curve(segs[segs.length - 1], segs[0]); d += ' Z'; }
    return d;
  }
  function polygon(root, V, box, extra) {
    return node(root, 'polygon', line(root, Object.assign({ points: box.corners.map(function (c) { return at(V, c[0], c[1]).join(','); }).join(' ') }, extra)));
  }
  function remove(s) { if (s.svg) { s.svg.remove(); s.svg = null; } s.sig = ''; }
  // The single (re)draw. A part whose context no longer holds is dropped here, so show/hover/the watcher cannot disagree.
  function refresh(root, force) {
    var s = state(root), c = capture(root), paths = root.selectedPaths;
    if (s.sel && (!c || s.sel.ctx !== c.ctx || s.sel.paths !== paths || s.sel.count !== paths.length)) { s.sel = null; force = true; }
    if (s.hov && (!c || s.hov.ctx !== c.ctx)) { s.hov = null; force = true; }
    if (s.prev && (!c || s.prev.ctx !== c.ctx)) { s.prev = null; force = true; }
    if (!s.sel && !s.hov && !s.prev) { remove(s); return; }
    var sig = JSON.stringify([c.V, c.bounds, c.pin.canvasW, c.pin.canvasH]);
    if (!force && s.svg && sig === s.sig) { watch(root); return; }
    remove(s); s.sig = sig;
    var svg = node(root, 'svg', { viewBox: '0 0 ' + c.pin.canvasW + ' ' + c.pin.canvasH, preserveAspectRatio: 'none', 'aria-hidden': 'true' });
    Object.assign(svg.style, { position: 'fixed', pointerEvents: 'none', zIndex: '20', left: c.bounds.left + 'px', top: c.bounds.top + 'px', width: c.bounds.width + 'px', height: c.bounds.height + 'px' });
    if (s.hov) {
      var hover = orientedBox(root, [s.hov.path]);
      if (hover) svg.appendChild(polygon(root, c.V, hover, { stroke: BLUE, 'stroke-opacity': '0.86', 'stroke-width': '1.5' }));
    }
    var shown = s.prev && s.prev.paths ? s.prev.paths : s.sel && s.sel.paths;
    if (shown) {
      shown.forEach(function (p) {
        var d = outline(c.V, p);
        if (d) svg.appendChild(node(root, 'path', line(root, { d: d, stroke: BLUE, 'stroke-opacity': '0.8', 'stroke-width': '1' })));
      });
      var box = orientedBox(root, shown), r = HANDLE * c.pin.canvasW / c.bounds.width;
      if (box) {
        // The rotate ring (baseline buildTransformBoxItems): centred on the box, 36 screen px at most, 30% of the box otherwise.
        var mid = at(c.V, box.center.x, box.center.y);
        svg.appendChild(node(root, 'circle', line(root, { cx: mid[0], cy: mid[1], stroke: BLUE, 'stroke-opacity': '0.63', 'stroke-width': '1',
          r: Math.min(36 * c.pin.canvasW / c.bounds.width, Math.max(box.width, box.height) * 0.3 * Math.hypot(c.V[0], c.V[1])) })));
        svg.appendChild(polygon(root, c.V, box, { stroke: BLUE, 'stroke-opacity': '0.8', 'stroke-width': '1' }));
        box.handles.forEach(function (h) {
          var p = at(c.V, h[0], h[1]);
          svg.appendChild(node(root, 'rect', line(root, { x: p[0] - r, y: p[1] - r, width: 2 * r, height: 2 * r, fill: '#ffffff', stroke: BLUE, 'stroke-width': '1.2' })));
        });
      }
    }
    if (s.prev && s.prev.marquee) {
      var m = s.prev.marquee;
      svg.appendChild(polygon(root, c.V, { corners: [[m[0], m[1]], [m[2], m[1]], [m[2], m[3]], [m[0], m[3]]] }, { fill: 'rgba(74,158,255,0.08)', stroke: BLUE, 'stroke-opacity': '0.9', 'stroke-width': '1' }));
    }
    root.document.body.appendChild(svg); s.svg = svg;
    watch(root);
  }
  // Pan, zoom and resize redraw in place; a changed context or selection removes the overlay. No-op once nothing is shown.
  function watch(root) {
    var s = state(root);
    if (s.watching || !root.requestAnimationFrame) return;
    s.watching = true;
    root.requestAnimationFrame(function () { s.watching = false; if (s.svg) refresh(root, false); });
  }
  // The selection changed (applyPick): redraw from root.selectedPaths; an empty selection draws nothing.
  function show(root) {
    var s = state(root), c = capture(root), paths = root.selectedPaths;
    s.sel = c && Array.isArray(paths) && paths.length ? { paths: paths, count: paths.length, ctx: c.ctx } : null;
    s.hov = null;
    refresh(root, true);
  }
  function clear(root) { var s = state(root); s.sel = null; s.hov = null; s.prev = null; s.next = null; remove(s); }
  // A drag in flight (#2372): `spec` is { paths } (the transformed copies of the selection, drawn with their box and handles)
  // and/or { marquee: [x0, y0, x1, y1] } in world space; null ends it. Nothing here writes the document.
  function preview(root, spec) {
    var s = state(root), c = capture(root);
    s.prev = spec && c ? { paths: spec.paths || null, marquee: spec.marquee || null, ctx: c.ctx } : null;
    refresh(root, true);
  }
  // Hover box of one picked path (null clears). A path already selected shows its selection box instead.
  function hover(root, path) {
    var s = state(root), sid = path && path.data && path.data.strokeId;
    if (!path && !s.hov) return;
    var c = capture(root);
    var held = sid && (root.selectedPaths || []).some(function (p) { return p && p.data && p.data.strokeId === sid; });
    var next = c && path && !held ? { path: path, ctx: c.ctx } : null;
    if (next && s.hov && sid && s.hov.path.data.strokeId === sid && s.hov.ctx === next.ctx) return;
    s.hov = next; refresh(root, true);
  }
  // Pointer hover over the active layer's presented strokes. Picks are coalesced: one read at a time, the latest position wins,
  // and a read whose context changed before it returned is discarded.
  function hoverAt(root, e) {
    var s = state(root), I = root.NemoSelectCanvasIntent, st = root.state;
    if (!e || e.buttons || !I || !st || st.tool !== 'select' || st.appMode === 'motion' || !I.objectProject(root)) { s.next = null; hover(root, null); return; }
    s.next = { x: e.clientX, y: e.clientY };
    if (s.busy) return;
    s.busy = true;
    (async function () {
      while (s.next) {
        var job = s.next, ctx = I.pickContext(root), item;
        s.next = null;
        try { item = await I.pick(root, job.x, job.y); } catch (_) { item = undefined; }
        if (item !== undefined && !s.next && I.pickContext(root) === ctx) hover(root, item);
      }
      s.busy = false;
    }());
  }
  return Object.freeze({ show: show, clear: clear, hover: hover, hoverAt: hoverAt, preview: preview, boxOf: orientedBox, refresh: function (root) { refresh(root, false); } });
}));
