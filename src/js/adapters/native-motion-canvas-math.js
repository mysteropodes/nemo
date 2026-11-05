/* Native Motion canvas geometry (#2520/#2525): the pure maths of the baseline whole-layer Motion gizmo (motion.js@1065e9d8 motionBoxGeom,
 * motionHandlePositions, hitMotionBoxHandle, onDrag), with no document, DOM or native access. A layer's own bounds BEFORE its Motion (the native
 * `layerBounds` read) and its current Position / Anchor / Rotation / Scale fold into one mapper: Rotation and Scale turn about the pivot (bounds
 * centre + Anchor) and Position is a plain translation added last. Coordinates are world units; hit-testing is done in client pixels by the caller
 * through `toClient`. Every drag value is recomputed from the values the gesture started with, never accumulated. */
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

  /* g: { bounds: [x0, y0, x1, y1], position: [x, y], anchor: [x, y], rotation: deg, scale: [sx, sy] percent }; pxPerUnit: client pixels per world unit
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
    const b = { left: x0, top: y0, right: x1, bottom: y1, width: x1 - x0, height: y1 - y0 }, mx = (x0 + x1) / 2, my = (y0 + y1) / 2;
    const corners = { nw: fwd(x0, y0), ne: fwd(x1, y0), se: fwd(x1, y1), sw: fwd(x0, y1), n: fwd(mx, y0), s: fwd(mx, y1), e: fwd(x1, my), w: fwd(x0, my) };
    // Baseline ring: at most 36 screen px, and no more than 30% of the scaled larger half-dimension of the box.
    const ringRadius = Math.min(36 / pxPerUnit, Math.max(b.width * (scl[0] / 100), b.height * (scl[1] / 100)) * 0.3);
    return { isNull, bounds: b, bc, pos, anchor: anc, rotation: rot, scale: scl, pivot: fwd(px, py), pv: { x: px, y: py }, fwd, inv, corners, ringRadius };
  }
  const key = (k) => ({ frame: k.frame, v: num2(k.v, [0, 0]), hIn: num2(k.hIn, [0, 0]), hOut: num2(k.hOut, [0, 0]) });
  // The key dots and spatial segments of the Position path: a key's delta sits at the pivot-with-anchor point plus its value (baseline pvx / pvy).
  function path(l, keys) {
    const ks = (keys || []).map(key).sort((a, b) => a.frame - b.frame);
    const at = (k) => ({ x: l.pv.x + k.v[0], y: l.pv.y + k.v[1] });
    const dots = ks.map((k) => Object.assign({ key: k }, at(k)));
    const segments = [];
    for (let i = 0; i + 1 < ks.length; i++) {
      const a = at(ks[i]), b = at(ks[i + 1]);
      segments.push({ from: a, c1: { x: a.x + ks[i].hOut[0], y: a.y + ks[i].hOut[1] }, c2: { x: b.x + ks[i + 1].hIn[0], y: b.y + ks[i + 1].hIn[1] }, to: b });
    }
    return { dots, segments };
  }
  const near = (a, b, tol) => Math.hypot(a.x - b.x, a.y - b.y) < tol;
  /* Hit-test in CLIENT pixels. `client(worldPoint)` maps a world point to the screen; `p` is the pointer in client pixels. Order of the baseline onDown:
   * the Rotate ring and the scale handles, the Position key dot of the current frame, the Alt-held anchor, then (Select tool) the body of the box.
   * Handles are not offered for a Null, whose marker is its box. */
  function hit(l, p, o) {
    const px = o.client, tolBox = 9, ringTol = 7;
    if (!l.isNull) {
      const ring = px(l.pivot), rr = (l.ringRadius * o.pxPerUnit);
      if (Math.abs(Math.hypot(p.x - ring.x, p.y - ring.y) - rr) < ringTol) return { type: 'rotate' };
      let best = null, bestD = tolBox;
      EDGES.forEach((k) => { const c = px(l.corners[k]), d = Math.hypot(p.x - c.x, p.y - c.y); if (d < bestD) { bestD = d; best = { type: 'scale', dir: k }; } });
      if (best) return best;
    }
    const dot = (o.dots || []).find((d) => d.key.frame === o.frame && near(px(d), p, 8));
    if (dot) return { type: 'key', key: dot.key };
    if (o.alt && near(px(l.pivot), p, 9)) return { type: 'anchor' };
    if (o.body) {
      const q = ['nw', 'ne', 'se', 'sw'].map((k) => px(l.corners[k])); // point in the (possibly rotated) box: same side of every edge
      const side = q.map((a, i) => { const b = q[(i + 1) % 4]; return (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x); });
      if (side.every((v) => v >= 0) || side.every((v) => v <= 0)) return { type: 'move' };
    }
    return null;
  }
  const lockAxis = (shift, dx, dy) => (!shift ? [dx, dy] : Math.abs(dx) >= Math.abs(dy) ? [dx, 0] : [0, dy]);
  /* The value a drag means, from the values it began with (`start`: the layout when the pointer went down, `from`: the world point) and the pointer now
   * (`to`). Returns { property, values } or null when it changes nothing. A refused combination returns { refuse: text }. */
  function drag(mode, start, from, to, o) {
    const l = start, same = (a, b) => a.length === b.length && a.every((v, i) => v === b[i]);
    let out = null;
    if (mode.type === 'move') {
      const d = lockAxis(o && o.shift, to.x - from.x, to.y - from.y);
      out = { property: 'position', values: [l.pos[0] + d[0], l.pos[1] + d[1]], was: l.pos };
    } else if (mode.type === 'rotate') {
      const a0 = Math.atan2(from.y - l.pivot.y, from.x - l.pivot.x), a1 = Math.atan2(to.y - l.pivot.y, to.x - l.pivot.x);
      out = { property: 'rotation', values: [l.rotation + (a1 - a0) / RAD], was: [l.rotation] };
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
      out = { property: 'position', values: [to.x - l.pv.x, to.y - l.pv.y], was: mode.key.v, frame: mode.key.frame };
    } else if (mode.type === 'anchor') {
      // Moving the pivot also moves every other point unless Position compensates; that is one more property, so only the case where the
      // compensation is zero (Rotation 0, Scale 100 %) is a single edit.
      if (l.rotation !== 0 || l.scale[0] !== 100 || l.scale[1] !== 100) return { refuse: 'anchor_compensation' };
      const at = l.inv(to.x, to.y);
      out = { property: 'anchor', values: [at.x - l.bc.x, at.y - l.bc.y], was: l.anchor };
    }
    if (!out || !out.values.every(finite) || same(out.values, out.was)) return null;
    return out;
  }
  return Object.freeze({ layout, path, hit, drag, lockAxis, EDGES });
}));
