/* The 3D layer gizmo of the native Motion canvas (#2520/#2525): the maths, drawing geometry, hit tests and drag values of motion.js@1065e9d8
 * (`rotate3D`, `project3DToScreen`, `gizmo3DPose`, `gizmo3DAxisScreenPoints`, `gizmo3DRingScreenPoints`, `hit3DGizmoAxis`, `hit3DGizmoRing` and the
 * `axis3d` / `ring3d` drags), without `state.layers`. Pure functions of the native reads (the layer's rest bounds, the raw Position / Anchor / Rotation /
 * Position Z / Rotation X / Rotation Y at the playhead, the canvas size and the ancestors' affine); nothing here reads or writes the document.
 * Three coloured arrows (X red, Y green, Z blue) leave the layer's origin along its LOCAL axes, and three rings lie in the planes perpendicular to
 * them. An arrow drag writes ONE Position component or Position Z (the pointer projected on the arrow's own screen direction); a ring drag writes
 * Rotation X, Rotation Y or Rotation (the pointer's turn about the origin). Whichever of the two is numerically closer to the pointer is grabbed. */
(function (root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeMotionGizmo3d = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  const CAMERA_DISTANCE = 2000, AXIS_LEN = 180, RING_RADIUS = 130, RING_STEPS = 32, AXIS_TOL = 8, RING_TOL = 7, RAD = Math.PI / 180;
  const AXES = ['x', 'y', 'z'], COLORS = { x: [235, 70, 70, 255], y: [90, 200, 100, 255], z: [70, 130, 235, 255] };
  const PROPERTY = { x: 'rotationX', y: 'rotationY', z: 'rotation' };
  const finite = (v) => typeof v === 'number' && Number.isFinite(v), pair = (v) => Array.isArray(v) && v.length === 2 && v.every(finite);

  // X, then Y, then Z (the Z turn is the ordinary 2D Rotation), each about the origin: rotate3D.
  function rotate(x, y, z, rx, ry, rz) {
    let a, c, s, n;
    if (rx) { a = rx * RAD; c = Math.cos(a); s = Math.sin(a); n = y * c - z * s; z = y * s + z * c; y = n; }
    if (ry) { a = ry * RAD; c = Math.cos(a); s = Math.sin(a); n = x * c + z * s; z = -x * s + z * c; x = n; }
    if (rz) { a = rz * RAD; c = Math.cos(a); s = Math.sin(a); n = x * c - y * s; y = x * s + y * c; x = n; }
    return [x, y, z];
  }
  // The default camera: CAMERA_DISTANCE is chosen so z = 0 projects to itself. A point nearer than 5% of the depth is clamped, as the baseline does.
  function project(wx, wy, wz, canvasW, canvasH) {
    const depth = Math.max(0.05, (CAMERA_DISTANCE + wz) / CAMERA_DISTANCE);
    return { x: canvasW / 2 + wx / depth, y: canvasH / 2 + wy / depth };
  }
  /* The layer's pose now: `read` carries bounds [x0, y0, x1, y1], position, anchor, rotation, positionZ, rotationX, rotationY, canvasW, canvasH. The origin
   * is the anchor in world space (bounds centre + anchor + position), canvas-centre relative like the projector's. Null when a value is missing. */
  function pose(read) {
    if (!read || !Array.isArray(read.bounds) || read.bounds.length !== 4 || !read.bounds.every(finite) || !pair(read.position) || !pair(read.anchor)) return null;
    if (![read.rotation, read.positionZ, read.rotationX, read.rotationY, read.canvasW, read.canvasH].every(finite)) return null;
    const b = read.bounds, px = (b[0] + b[2]) / 2 + read.anchor[0], py = (b[1] + b[3]) / 2 + read.anchor[1];
    return { pos: read.position.slice(), rot: read.rotation, posZ: read.positionZ, rotX: read.rotationX, rotY: read.rotationY, canvasW: read.canvasW, canvasH: read.canvasH,
      originWX: px + read.position[0] - read.canvasW / 2, originWY: py + read.position[1] - read.canvasH / 2, originWZ: read.positionZ };
  }
  // The pose with the property a drag is previewing replaced (Position moves the origin by the same delta the projector would).
  function preview(p, change) {
    if (!p || !change || !change.property || !Array.isArray(change.values)) return p;
    const v = change.values, next = Object.assign({}, p);
    if (change.property === 'position' && pair(v)) { next.originWX += v[0] - p.pos[0]; next.originWY += v[1] - p.pos[1]; next.pos = v.slice(); }
    else if (change.property === 'positionZ' && finite(v[0])) { next.posZ = v[0]; next.originWZ = v[0]; }
    else if (change.property === 'rotationX' && finite(v[0])) next.rotX = v[0];
    else if (change.property === 'rotationY' && finite(v[0])) next.rotY = v[0];
    else if (change.property === 'rotation' && finite(v[0])) next.rot = v[0];
    return next;
  }
  const origin = (p) => project(p.originWX, p.originWY, p.originWZ, p.canvasW, p.canvasH);
  const at = (p, d) => { const r = rotate(d[0], d[1], d[2], p.rotX, p.rotY, p.rot); return project(p.originWX + r[0], p.originWY + r[1], p.originWZ + r[2], p.canvasW, p.canvasH); };
  // Local axes: the arrows tilt with the layer's own rotation, as the baseline's "local axis mode" does.
  function arrows(p) {
    const o = origin(p), dirs = { x: [1, 0, 0], y: [0, 1, 0], z: [0, 0, 1] }, out = {};
    AXES.forEach((k) => { out[k] = { origin: o, tip: at(p, dirs[k].map((c) => c * AXIS_LEN)) }; });
    return out;
  }
  // Each ring lies in the plane perpendicular to its own axis, sampled as RING_STEPS + 1 points of a closed polyline.
  function rings(p) {
    const plane = { x: (a) => [0, Math.cos(a), Math.sin(a)], y: (a) => [Math.sin(a), 0, Math.cos(a)], z: (a) => [Math.cos(a), Math.sin(a), 0] }, out = {};
    AXES.forEach((k) => {
      out[k] = [];
      for (let i = 0; i <= RING_STEPS; i++) out[k].push(at(p, plane[k](i / RING_STEPS * Math.PI * 2).map((c) => c * RING_RADIUS)));
    });
    return out;
  }
  // The ancestors' affine [a, b, c, d, e, f] (x' = a x + c y + e): the same map the layer's box is drawn through (`outerWorldPoint`).
  function through(outer, q) {
    if (!Array.isArray(outer) || outer.length !== 6) return q;
    return { x: outer[0] * q.x + outer[2] * q.y + outer[4], y: outer[1] * q.x + outer[3] * q.y + outer[5] };
  }
  function toSegment(pt, a, b) {
    const dx = b.x - a.x, dy = b.y - a.y, len2 = dx * dx + dy * dy, t = len2 ? Math.max(0, Math.min(1, ((pt.x - a.x) * dx + (pt.y - a.y) * dy) / len2)) : 0;
    return Math.hypot(pt.x - (a.x + t * dx), pt.y - (a.y + t * dy));
  }
  /* The control under `pointer` (client pixels): { type: 'axis3d' | 'ring3d', axis, dist } or null. `client` maps a canvas point to client pixels. */
  function hit(p, outer, client, pointer) {
    if (!p) return null;
    const shown = (q) => client(through(outer, q));
    let bestAxis = null, bestRing = null;
    const a = arrows(p);
    AXES.forEach((k) => { const d = toSegment(pointer, shown(a[k].origin), shown(a[k].tip)); if (d < AXIS_TOL && (!bestAxis || d < bestAxis.dist)) bestAxis = { type: 'axis3d', axis: k, dist: d }; });
    const r = rings(p);
    AXES.forEach((k) => {
      for (let i = 0; i < r[k].length - 1; i++) {
        const d = toSegment(pointer, shown(r[k][i]), shown(r[k][i + 1]));
        if (d < RING_TOL && (!bestRing || d < bestRing.dist)) bestRing = { type: 'ring3d', axis: k, dist: d };
      }
    });
    return bestAxis && (!bestRing || bestAxis.dist <= bestRing.dist) ? bestAxis : bestRing;
  }
  /* The grab: what a drag needs from the moment the pointer went down (`from` is the pointer in canvas coordinates). */
  function begin(found, p, outer, from) {
    if (found.type === 'axis3d') {
      const a = arrows(p)[found.axis], o = through(outer, a.origin), t = through(outer, a.tip), dx = t.x - o.x, dy = t.y - o.y, len = Math.hypot(dx, dy) || 1;
      return { type: 'axis3d', axis: found.axis, dirX: dx / len, dirY: dy / len, from, pos: p.pos.slice(), baseline: found.axis === 'z' ? p.posZ : p.pos[found.axis === 'x' ? 0 : 1] };
    }
    const c = through(outer, origin(p)), base = found.axis === 'x' ? p.rotX : found.axis === 'y' ? p.rotY : p.rot;
    return { type: 'ring3d', axis: found.axis, center: c, startAngle: Math.atan2(from.y - c.y, from.x - c.x) / RAD, baseline: base };
  }
  // The value a drag means from the grab and the pointer now (`to`, canvas coordinates); null when nothing changes.
  function drag(g, to) {
    let out;
    if (g.type === 'axis3d') {
      const v = g.baseline + (to.x - g.from.x) * g.dirX + (to.y - g.from.y) * g.dirY;
      out = g.axis === 'z' ? { property: 'positionZ', values: [v], was: [g.baseline] }
        : { property: 'position', values: [g.axis === 'x' ? v : g.pos[0], g.axis === 'y' ? v : g.pos[1]], was: g.pos };
    } else {
      const v = g.baseline + (Math.atan2(to.y - g.center.y, to.x - g.center.x) / RAD - g.startAngle);
      out = { property: PROPERTY[g.axis], values: [v], was: [g.baseline] };
    }
    return out.values.every(finite) && out.values.some((x, i) => x !== out.was[i]) ? out : null;
  }
  // The arrow head: two points behind the tip, `size` pixels long and `half` wide, as the baseline's filled triangle.
  function head(tip, from, size, half) {
    const dx = tip.x - from.x, dy = tip.y - from.y, len = Math.hypot(dx, dy) || 1, ux = dx / len, uy = dy / len, nx = -uy, ny = ux;
    return [tip, { x: tip.x - ux * size + nx * half, y: tip.y - uy * size + ny * half }, { x: tip.x - ux * size - nx * half, y: tip.y - uy * size - ny * half }];
  }
  return Object.freeze({ CAMERA_DISTANCE, AXIS_LEN, RING_RADIUS, COLORS, AXES, rotate, project, pose, preview, arrows, rings, through, hit, begin, drag, head });
}));
