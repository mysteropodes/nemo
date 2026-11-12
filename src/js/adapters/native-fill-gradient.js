/* Native fill gradient of the Select selection (#2372, native-fill-gradient). On a native object project the Fill row's gradient button / checkbox, the
 * stop editor and the on-canvas gizmo of gradient-bridge.js (baseline 1065e9d8: a Paper item's data.fillGradient, drawn and dragged in world space)
 * edit the ONE selected listed shape through the engine: the listing carries the record's stored gradient (`nativeStyle.fillGradient`, record space) and
 * every edit is a `fillGradient` member of ONE restyle step (NemoNativeSelectionStyle.apply), so a toggle, a stop edit and a drag are each one revision and
 * one history entry, pinned to the presented revision, waiting out a pending native command and refused with a notice - never thrown, never sent to a
 * Paper writer. The engine stores exactly two stops: a third stop is refused here with a notice.
 * Baseline rules kept: a single plain stroke or dynamic shape only (anything else explains itself through the button's toast); turning it on gives the
 * default gradient (the item's fill to blue across the bounds' half diagonal); turning it off returns to the flat fill; a stop colour keeps its alpha byte
 * (CLAUDE.md section 2); the stops stay in ascending offset (a renderer drops a stop that is out of order). Typed values commit on change, a gizmo drag
 * commits once at release from the state it began in. This module holds no document state: the gradient it shows is the listed one, or the edit in flight. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeFillGradient = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  var NS = 'http://www.w3.org/2000/svg', BLUE = 'rgb(90,180,255)';
  var states = new WeakMap();
  function st(root) {
    if (!states.has(root)) states.set(root, { id: null, grad: undefined, drag: null });
    return states.get(root);
  }
  function style(root) { return root.NemoNativeSelectionStyle; }
  function clone(g) { return JSON.parse(JSON.stringify(g)); }
  function notice(root, text) { if (typeof root.showToast === 'function') try { root.showToast(text); } catch (_) {} }
  function explain(root) {
    var text = 'Select a single shape with the Selection tool to edit its fill gradient.';
    try { if (root.SM && typeof root.SM.t === 'function') text = root.SM.t('toastSelectSingleShapeForGradient') || text; } catch (_) {}
    notice(root, text);
  }
  // True on a native object project: the Fill row's gradient controls then belong to this module whatever the selection is.
  function handles(root) { var S = style(root); return !!(S && typeof S.native === 'function' && S.native(root)); }
  // The one selected shape the gradient controls edit (a plain stroke or a dynamic shape, never a ribbon, brush, mask, image or text), else null.
  function pathOf(root) {
    var sel = root.selectedPaths || [], tool = root.state && root.state.tool, p = sel[0], d = p && p.data;
    if ((tool !== 'select' && tool !== 'subselect') || sel.length !== 1 || !d || !d.nativeStableTarget || !Number.isInteger(d.nativeLayerIndex)) return null;
    return (d.nativeRole === 'plain' || d.nativeRole === 'shape') && !d.nativeBrush && !d.nativeRibbon ? p : null;
  }
  function listed(p) { var g = p.data.nativeStyle && p.data.nativeStyle.fillGradient; return g ? clone(g) : null; }
  // The gradient the panel shows: the edit still in flight for this shape, else the listed one.
  function current(root, p) { var s = st(root); return s.grad !== undefined && s.id === p.data.strokeId ? s.grad : listed(p); }
  // What gradient-bridge's panel renders from: a read-only snapshot shaped like the Paper item it used to read (nothing here is ever written back).
  function target(root) {
    var p = pathOf(root);
    if (!p) return null;
    var g = current(root, p);
    return { data: g ? { fillGradient: clone(g) } : {} };
  }
  function opaque(c) { return c.length === 7 ? c + 'ff' : c; }
  // Baseline defaultGradientFor, in the record's own space: across the bounds' half diagonal at the vertical centre, the item's fill to blue.
  function defaultFor(p) {
    var b = p.data.nativeBounds, fill = p.data.nativeStyle && p.data.nativeStyle.fill;
    if (!Array.isArray(b) || b.length !== 4 || !b.every(Number.isFinite)) return null;
    var cx = (b[0] + b[2]) / 2, cy = (b[1] + b[3]) / 2, half = Math.hypot(b[2] - b[0], b[3] - b[1]) / 2 || 100;
    return { kind: 'linear', from: [cx - half, cy], to: [cx + half, cy], stops: [{ offset: 0, color: fill ? opaque(fill) : '#ff0000ff' }, { offset: 1, color: '#0000ffff' }] };
  }
  // ONE restyle for the edit; resolves with the style module's { status }. The panel is re-read afterwards so a refused edit puts the controls back.
  function write(root, p, g) {
    var S = style(root), s = st(root), sent = g;
    s.id = p.data.strokeId; s.grad = g;
    var done;
    try { done = S.apply(root, { fillGradient: g }); } catch (_) { done = Promise.resolve({ status: 'unavailable' }); }
    return Promise.resolve(done).then(function (r) { return r; }, function () { return { status: 'unavailable' }; }).then(function (r) {
      if (s.grad === sent) s.grad = undefined;
      if (typeof root.renderGradientPanel === 'function') root.renderGradientPanel();
      return r;
    });
  }
  function change(root, edit) {
    var p = pathOf(root), g = p && current(root, p);
    if (!p) { explain(root); return Promise.resolve({ status: 'unavailable' }); }
    if (!g) return Promise.resolve({ status: 'noop' });
    var next = clone(g);
    if (edit(next) === false) return Promise.resolve({ status: 'noop' });
    next.stops.sort(function (a, b) { return a.offset - b.offset; });
    return write(root, p, next);
  }
  // The checkbox / button: turning it on stores the default gradient, turning it off returns to the flat fill. One restyle each.
  function toggle(root, on) {
    var p = pathOf(root);
    if (!p) { explain(root); if (typeof root.renderGradientPanel === 'function') root.renderGradientPanel(); return Promise.resolve({ status: 'unavailable' }); }
    var g = current(root, p);
    if (!!g === !!on) return Promise.resolve({ status: 'noop' });
    var fresh = on ? defaultFor(p) : null;
    if (on && !fresh) { notice(root, 'Could not read the shape just now.'); if (typeof root.renderGradientPanel === 'function') root.renderGradientPanel(); return Promise.resolve({ status: 'unavailable' }); }
    return write(root, p, fresh);
  }
  // A stop's colour from the native colour input (#rrggbb): the stop keeps the alpha byte it had.
  function stopColor(root, index, hex) {
    return change(root, function (g) {
      var stop = g.stops[index];
      if (!stop || !/^#[0-9a-fA-F]{6}$/.test(hex)) return false;
      stop.color = hex + (stop.color.length === 9 ? stop.color.slice(7, 9) : 'ff');
      return true;
    });
  }
  function stopOffset(root, index, percent) {
    return change(root, function (g) {
      var stop = g.stops[index];
      if (!stop) return false;
      stop.offset = Math.max(0, Math.min(100, Number.isFinite(percent) ? percent : 0)) / 100;
      return true;
    });
  }
  function kind(root, value) {
    return change(root, function (g) { if (value !== 'linear' && value !== 'radial') return false; g.kind = value; return true; });
  }
  function addStop(root) {
    notice(root, 'Native gradients take two stops.');
    return Promise.resolve({ status: 'unavailable' });
  }

  // ---- the on-canvas gizmo: record space <-> world through the item's presented matrix ----
  function map(m, a) { return [m[0] * a[0] + m[2] * a[1] + m[4], m[1] * a[0] + m[3] * a[1] + m[5]]; }
  function unmap(m, w) {
    var det = m[0] * m[3] - m[1] * m[2];
    if (!Number.isFinite(det) || Math.abs(det) < 1e-12) return null;
    var x = w[0] - m[4], y = w[1] - m[5];
    return [(m[3] * x - m[2] * y) / det, (m[0] * y - m[1] * x) / det];
  }
  function lerp(a, b, t) { return [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]; }
  function matrixOf(p) { var m = p.data.nativeMatrix; return Array.isArray(m) && m.length === 6 && m.every(Number.isFinite) ? m : null; }
  function consume(e) { if (typeof e.stopImmediatePropagation === 'function') e.stopImmediatePropagation(); if (typeof e.preventDefault === 'function') e.preventDefault(); }
  function redraw(root, on) {
    var O = root.NemoNativeSelectOverlay;
    if (O && typeof O.preview === 'function') O.preview(root, on ? {} : null);
  }
  function pointer(root, phase, e) {
    var s = st(root), I = root.NemoSelectCanvasIntent, O = root.NemoNativeSelectOverlay, d = s.drag;
    if (phase === 'down') {
      var p = pathOf(root), g = p && current(root, p), shown = g && I && I.presentedState(root), m = p && matrixOf(p);
      if (!g || !shown || !m || (e.button !== undefined && e.button > 0)) return false;
      // The gizmo exists only while the overlay draws this selection for the current context (as the Select gesture's handles do).
      if (!(O && typeof O.selectionContext === 'function' && O.selectionContext(root) === I.pickContext(root))) return false;
      var world = function (x, y) { return I.worldPoint(root, x, y, shown.pin, shown.presentation); };
      var at = world(e.clientX, e.clientY), one = world(e.clientX + 1, e.clientY), wpp = Math.hypot(one.x - at.x, one.y - at.y) || at.perPixel || 1;
      var f = map(m, g.from), t = map(m, g.to), near = function (q, px) { return Math.hypot(at.x - q[0], at.y - q[1]) < px * wpp; }, grab = null;
      if (near(f, 12)) grab = 'from';
      else if (near(t, 12)) grab = 'to';
      else g.stops.forEach(function (stop) { if (!grab && near(lerp(f, t, stop.offset), 9)) grab = { stop: stop }; });
      if (!grab) return false;
      s.drag = { grab: grab, grad: clone(g), start: JSON.stringify(g), p: p, m: m, world: world, ctx: I.pickContext(root), pin: shown.pin };
      if (typeof grab === 'object') s.drag.grab = { stop: s.drag.grad.stops[g.stops.indexOf(grab.stop)] };
      consume(e);
      return true;
    }
    if (!d) return false;
    consume(e);
    if (phase === 'move') {
      var w = d.world(e.clientX, e.clientY), wp = [w.x, w.y], rec = unmap(d.m, wp);
      if (!rec) return true;
      if (d.grab === 'from') d.grad.from = rec;
      else if (d.grab === 'to') d.grad.to = rec;
      else {
        // The drag point is projected on the from-to line (in world space, which an affine keeps proportional) and clamped to the ends, as the baseline did.
        var f2 = map(d.m, d.grad.from), t2 = map(d.m, d.grad.to), dx = t2[0] - f2[0], dy = t2[1] - f2[1], len2 = dx * dx + dy * dy || 1;
        d.grab.stop.offset = Math.max(0, Math.min(1, ((wp[0] - f2[0]) * dx + (wp[1] - f2[1]) * dy) / len2));
        d.grad.stops.sort(function (a, b) { return a.offset - b.offset; });
      }
      redraw(root, true);
      return true;
    }
    s.drag = null;
    redraw(root, false);
    // Released: ONE edit from the state the drag began in. A cancelled or unmoved drag writes nothing; a selection that moved on meanwhile is not restyled.
    if (phase !== 'up' || JSON.stringify(d.grad) === d.start) return true;
    var live = pathOf(root);
    if (!live || live.data.strokeId !== d.p.data.strokeId || !I || I.pickContext(root) !== d.ctx) { notice(root, 'The selection changed — nothing was restyled.'); return true; }
    write(root, live, d.grad);
    return true;
  }
  // Drawn by native-select-overlay.js inside its svg: `c` = { V, pin, bounds } (world to canvas units), `shown` the paths it outlines.
  function draw(root, svg, c, shown) {
    var p = pathOf(root), s = st(root);
    if (!p || shown.length !== 1 || shown[0] !== p) return;
    var g = s.drag ? s.drag.grad : current(root, p), m = matrixOf(p);
    if (!g || !m || !Array.isArray(g.from) || !Array.isArray(g.to)) return;
    var k = c.pin.canvasW / c.bounds.width, V = c.V;
    var at = function (a) { var w = map(m, a); return [V[0] * w[0] + V[2] * w[1] + V[4], V[1] * w[0] + V[3] * w[1] + V[5]]; };
    var add = function (tag, attrs) {
      var n = root.document.createElementNS(NS, tag);
      Object.keys(attrs).forEach(function (key) { n.setAttribute(key, attrs[key]); });
      n.setAttribute('vector-effect', 'non-scaling-stroke');
      svg.appendChild(n);
    };
    var f = at(g.from), t = at(g.to), r = 6 * k, ds = 5 * k;
    add('line', { x1: f[0], y1: f[1], x2: t[0], y2: t[1], stroke: BLUE, 'stroke-width': '1.5', fill: 'none' });
    add('rect', { x: f[0] - r, y: f[1] - r, width: 2 * r, height: 2 * r, fill: '#ffffff', stroke: BLUE, 'stroke-width': '1.5' });
    add('circle', { cx: t[0], cy: t[1], r: r, fill: '#ffffff', stroke: BLUE, 'stroke-width': '1.5' });
    g.stops.forEach(function (stop) {
      var q = lerp(f, t, stop.offset), a = stop.color.length === 9 ? parseInt(stop.color.slice(7, 9), 16) / 255 : 1;
      add('polygon', { points: [[q[0], q[1] - ds], [q[0] + ds, q[1]], [q[0], q[1] + ds], [q[0] - ds, q[1]]].join(' '),
        fill: stop.color.slice(0, 7), 'fill-opacity': String(a), stroke: 'rgb(30,30,30)', 'stroke-width': '1.2' });
    });
  }
  return Object.freeze({ handles: handles, target: target, toggle: toggle, stopColor: stopColor, stopOffset: stopOffset, kind: kind, addStop: addStop, pointer: pointer, draw: draw });
}));
