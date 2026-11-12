/* Native Motion canvas geometry (#2520/#2525): the pure maths of the baseline whole-layer Motion gizmo (motion.js@1065e9d8 motionBoxGeom,
 * motionHandlePositions, hitMotionBoxHandle, onDrag), with no document, DOM or native access. A layer's own bounds BEFORE its Motion (the native
 * `layerBounds` read) and its current Position / Anchor / Rotation / Scale fold into one mapper: Rotation and Scale turn about the pivot (bounds
 * centre + Anchor) and Position is a plain translation added last. Coordinates are world units; hit-testing is done in client pixels by the caller
 * through `toClient`. Every drag value is recomputed from the values the gesture started with, never accumulated. A parented layer carries the native
 * `parentAffine` (the map its ancestors apply to a point already moved by its own Motion, the baseline outerWorldPoint / outerLocalPoint): drawing
 * maps through it, a pointer maps back through its inverse (a Position delta through its linear part only, as invertVectorThroughParentChain). */
(function (root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeMotionCanvasMath = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  const RAD = Math.PI / 180;
  const NULL_HALF_PX = 12; // baseline buildNullLayerItems marker: HS = 12 / zoom
  const EDGES = ['nw', 'ne', 'se', 'sw', 'n', 's', 'e', 'w'];
  const finite = (v) => Number.isFinite(v);
  const num2 = (v, d) => (Array.isArray(v) && v.length >= 2 && finite(v[0]) && finite(v[1]) ? [v[0], v[1]] : d);

  /* g: { bounds: [x0, y0, x1, y1], position: [x, y], anchor: [x, y], rotation: deg, scale: [sx, sy] percent, outer?: the six-number ancestor map }; pxPerUnit: client pixels per world unit
   * (a Null is drawn as a fixed 24 px square). Returns the mapper and the handle geometry, or null when the input is not usable. */
  function layout(g, pxPerUnit) {
    if (!g || !Array.isArray(g.bounds) || g.bounds.length !== 4 || !g.bounds.every(finite) || !(pxPerUnit > 0)) return null;
    const pos = num2(g.position, [0, 0]), anc = num2(g.anchor, [0, 0]), scl = num2(g.scale, [100, 100]), rot = finite(g.rotation) ? g.rotation : 0;
    let [x0, y0, x1, y1] = g.bounds;
    const isNull = x1 - x0 === 0 && y1 - y0 === 0;
    const bc = { x: (x0 + x1) / 2, y: (y0 + y1) / 2 };
    if (isNull) { const h = NULL_HALF_PX / pxPerUnit; x0 = bc.x - h; y0 = bc.y - h; x1 = bc.x + h; y1 = bc.y + h; }
    const px = bc.x + anc[0], py = bc.y + anc[1], r = rot * RAD, c = Math.cos(r), s = Math.sin(r), sx = scl[0] / 100, sy = scl[1] / 100;
    const fwd = (x, y) => { const lx = (x - px) * sx, ly = (y - py) * sy; return { x: px + lx * c - ly * s + pos[0], y: py + lx * s + ly * c + pos[1] }; };
    const inv = (wx, wy) => {
      const ux = wx - pos[0] - px, uy = wy - pos[1] - py, lx = ux * c + uy * s, ly = -ux * s + uy * c;
      return { x: sx ? lx / sx + px : px, y: sy ? ly / sy + py : py };
    };
    // The outer (ancestor) map [a b c d e f]: x' = a x + c y + e, y' = b x + d y + f. A collapsed parent (det 0) has no gizmo.
    const m = Array.isArray(g.outer) && g.outer.length === 6 && g.outer.every(finite) ? g.outer : [1, 0, 0, 1, 0, 0], det = m[0] * m[3] - m[1] * m[2];
    if (!det) return null;
    const ow = (p) => ({ x: m[0] * p.x + m[2] * p.y + m[4], y: m[1] * p.x + m[3] * p.y + m[5] });
    const oinv = (p) => ({ x: (m[3] * (p.x - m[4]) - m[2] * (p.y - m[5])) / det, y: (-m[1] * (p.x - m[4]) + m[0] * (p.y - m[5])) / det });
    const ovec = (dx, dy) => [(m[3] * dx - m[2] * dy) / det, (-m[1] * dx + m[0] * dy) / det];
    const world = (x, y) => ow(fwd(x, y)), local = (p) => { const q = oinv(p); return inv(q.x, q.y); };
    const b = { left: x0, top: y0, right: x1, bottom: y1, width: x1 - x0, height: y1 - y0 }, mx = (x0 + x1) / 2, my = (y0 + y1) / 2;
    const corners = { nw: world(x0, y0), ne: world(x1, y0), se: world(x1, y1), sw: world(x0, y1), n: world(mx, y0), s: world(mx, y1), e: world(x1, my), w: world(x0, my) };
    // Baseline ring: at most 36 screen px, and no more than 30% of the scaled larger half-dimension of the box.
    const ringRadius = Math.min(36 / pxPerUnit, Math.max(b.width * (scl[0] / 100), b.height * (scl[1] / 100)) * 0.3);
    return { isNull, bounds: b, bc, pos, anchor: anc, rotation: rot, scale: scl, pivot: world(px, py), pv: { x: px, y: py }, fwd, inv, ow, oinv, ovec, flip: det < 0, local, corners, ringRadius };
  }
  const key = (k) => ({ frame: k.frame, v: num2(k.v, [0, 0]), hIn: num2(k.hIn, [0, 0]), hOut: num2(k.hOut, [0, 0]), hold: k.hold === true, curvePoints: Array.isArray(k.curvePoints) ? k.curvePoints : null });
  /* The key dots, spatial handles and Position path, all through the outer map. A key's delta sits at the pivot-with-anchor point plus its value (baseline
   * pvx / pvy). `evalCurve(curvePoints, t)` is the key's easing evaluator (SMAnimationCurve.evalCurvePoints): each segment is sampled once per FRAME through
   * the key's own easing, so the one-per-frame dots pack where the layer is slow and spread where it is fast (baseline buildOverlayItems); without it the
   * segment is the plain spatial curve and has no frame dots. */
  function path(l, keys, evalCurve) {
    const ks = (keys || []).map(key).sort((a, b) => a.frame - b.frame);
    const at = (k) => l.ow({ x: l.pv.x + k.v[0], y: l.pv.y + k.v[1] });
    const dots = ks.map((k) => Object.assign({ key: k }, at(k)));
    const handles = [];
    ks.forEach((k, i) => [i < ks.length - 1 && 'hOut', i > 0 && 'hIn'].forEach((which) => {
      if (!which) return;
      const h = k[which], p = l.ow({ x: l.pv.x + k.v[0] + h[0], y: l.pv.y + k.v[1] + h[1] });
      handles.push({ key: k, which, x: p.x, y: p.y, from: dots[i], zero: !h[0] && !h[1] });
    }));
    const segments = [], frames = [];
    for (let i = 0; i + 1 < ks.length; i++) {
      const a = ks[i], b = ks[i + 1], p0 = at(a), p3 = at(b);
      const c1 = l.ow({ x: l.pv.x + a.v[0] + a.hOut[0], y: l.pv.y + a.v[1] + a.hOut[1] }), c2 = l.ow({ x: l.pv.x + b.v[0] + b.hIn[0], y: l.pv.y + b.v[1] + b.hIn[1] });
      segments.push({ from: p0, c1, c2, to: p3 });
      if (typeof evalCurve !== 'function') continue;
      const n = Math.max(1, b.frame - a.frame), pts = [];
      for (let f = 0; f <= n; f++) {
        const bt = a.hold ? 0 : evalCurve(a.curvePoints || DEFAULT_CURVE, f / n), u = 1 - bt;
        pts.push({ x: u * u * u * p0.x + 3 * u * u * bt * c1.x + 3 * u * bt * bt * c2.x + bt * bt * bt * p3.x, y: u * u * u * p0.y + 3 * u * u * bt * c1.y + 3 * u * bt * bt * c2.y + bt * bt * bt * p3.y });
      }
      frames.push(pts);
    }
    return { dots, segments, handles, frames };
  }
  const DEFAULT_CURVE = [{ x: 0, y: 0 }, { x: 0.25, y: 0.156 }, { x: 0.5, y: 0.5 }, { x: 0.75, y: 0.844 }, { x: 1, y: 1 }];
  const near = (a, b, tol) => Math.hypot(a.x - b.x, a.y - b.y) < tol;
  /* Hit-test in CLIENT pixels. `client(worldPoint)` maps a world point to the screen; `p` is the pointer in client pixels. Order of the baseline onDown:
   * the Rotate ring and the scale handles, the spatial key handles (hitPositionHandle), the Position key dot of the current frame, the Alt-held anchor,
   * then (Select tool) the body of the box. The scale handles are not offered for a Null, whose marker is its box. A handle of zero length sits on its
   * key, so it is offered only where it does not hide the current frame's dot (the baseline drew it only when it had length). */
  function hit(l, p, o) {
    const px = o.client, tolBox = 9, ringTol = 7;
    if (!l.isNull && !o.noBox) { // a 3D layer has no box: its gizmo takes the ring and the handles' place
      const ring = px(l.pivot), rr = (l.ringRadius * o.pxPerUnit);
      if (Math.abs(Math.hypot(p.x - ring.x, p.y - ring.y) - rr) < ringTol) return { type: 'rotate' };
      let best = null, bestD = tolBox;
      EDGES.forEach((k) => { const c = px(l.corners[k]), d = Math.hypot(p.x - c.x, p.y - c.y); if (d < bestD) { bestD = d; best = { type: 'scale', dir: k }; } });
      if (best) return best;
    }
    const dot = (o.dots || []).find((d) => d.key.frame === o.frame && near(px(d), p, 8));
    const grab = (o.handles || []).find((h) => near(px(h), p, 10) && !(h.zero && dot));
    if (grab) return { type: 'handle', key: grab.key, which: grab.which };
    if (dot) return { type: 'key', key: dot.key };
    if (o.alt && near(px(l.pivot), p, 9)) return { type: 'anchor' };
    if (o.body && !o.noBox) {
      const q = ['nw', 'ne', 'se', 'sw'].map((k) => px(l.corners[k])); // point in the (possibly rotated) box: same side of every edge
      const side = q.map((a, i) => { const b = q[(i + 1) % 4]; return (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x); });
      if (side.every((v) => v >= 0) || side.every((v) => v <= 0)) return { type: 'move' };
    }
    return null;
  }
  /* The Position that keeps every rendered point still when the anchor moves to `anchor` (pos + (I - R S)(anchor_old - anchor_new)): what the engine writes
   * for a `keepPose` anchor edit, used here so the preview box does not swing while the anchor is dragged. */
  function follow(g, anchor) {
    const pos = num2(g.position, [0, 0]), old = num2(g.anchor, [0, 0]), scl = num2(g.scale, [100, 100]), r = (finite(g.rotation) ? g.rotation : 0) * RAD;
    const dx = old[0] - anchor[0], dy = old[1] - anchor[1], sx = scl[0] / 100, sy = scl[1] / 100, c = Math.cos(r), s = Math.sin(r);
    return [pos[0] + dx - (sx * c * dx - sy * s * dy), pos[1] + dy - (sx * s * dx + sy * c * dy)];
  }
  const lockAxis = (shift, dx, dy) => (!shift ? [dx, dy] : Math.abs(dx) >= Math.abs(dy) ? [dx, 0] : [0, dy]);
  /* The value a drag means, from the values it began with (`start`: the layout when the pointer went down, `from`: the world point) and the pointer now
   * (`to`). Returns { property, values[, frame, handle, keepPose] } or null when it changes nothing. */
  function drag(mode, start, from, to, o) {
    const l = start, same = (a, b) => a.length === b.length && a.every((v, i) => v === b[i]);
    let out = null;
    if (mode.type === 'move') { // a world delta, back through the ancestors' linear part: Position is a translation after the layer's own Motion
      const d = l.ovec(...lockAxis(o && o.shift, to.x - from.x, to.y - from.y));
      out = { property: 'position', values: [l.pos[0] + d[0], l.pos[1] + d[1]], was: l.pos };
    } else if (mode.type === 'rotate') {
      const a0 = Math.atan2(from.y - l.pivot.y, from.x - l.pivot.x), a1 = Math.atan2(to.y - l.pivot.y, to.x - l.pivot.x);
      out = { property: 'rotation', values: [l.rotation + (l.flip ? -1 : 1) * (a1 - a0) / RAD], was: [l.rotation] };
    } else if (mode.type === 'scale') {
      const corner = l.corners[mode.dir], orig = Math.hypot(corner.x - l.pivot.x, corner.y - l.pivot.y) || 1;
      if (mode.dir.length === 1) { // an edge handle scales the one axis, by the signed projection onto its own direction from the pivot
        const ux = (corner.x - l.pivot.x) / orig, uy = (corner.y - l.pivot.y) / orig;
        const ratio = ((to.x - l.pivot.x) * ux + (to.y - l.pivot.y) * uy) / orig, ns = mode.dir === 'n' || mode.dir === 's';
        out = { property: 'scale', values: ns ? [l.scale[0], l.scale[1] * ratio] : [l.scale[0] * ratio, l.scale[1]], was: l.scale };
      } else {
        const ratio = Math.hypot(to.x - l.pivot.x, to.y - l.pivot.y) / orig;
        out = { property: 'scale', values: [l.scale[0] * ratio, l.scale[1] * ratio], was: l.scale };
      }
    } else if (mode.type === 'key') {
      const q = l.oinv(to);
      out = { property: 'position', values: [q.x - l.pv.x, q.y - l.pv.y], was: mode.key.v, frame: mode.key.frame };
    } else if (mode.type === 'handle') { // a handle is stored relative to its key point
      const q = l.oinv(to), k = mode.key;
      out = { property: 'position', values: [q.x - l.pv.x - k.v[0], q.y - l.pv.y - k.v[1]], was: k[mode.which], frame: k.frame, handle: mode.which };
    } else if (mode.type === 'anchor') {
      // Moving the pivot also moves every other point unless Position compensates: with Rotation or Scale set the edit carries `keepPose`, and the
      // engine writes Position = pos + (I - R S)(anchor_old - anchor_new) in the same revision (the baseline anchor drag).
      const at = l.local(to);
      out = { property: 'anchor', values: [at.x - l.bc.x, at.y - l.bc.y], was: l.anchor };
      if (l.rotation !== 0 || l.scale[0] !== 100 || l.scale[1] !== 100) out.keepPose = true;
    }
    if (!out || !out.values.every(finite) || same(out.values, out.was)) return null;
    return out;
  }
  return Object.freeze({ layout, path, hit, drag, follow, lockAxis, EDGES });
}));
