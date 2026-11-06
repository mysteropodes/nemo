/* B-draw-perspective-snap: the perspective guide's pull on Draw / Fill Brush and its snap on the Line tool, over the vanishing points the
 * NATIVE perspective read presents (items [{ordinal, x, y}], the effective list: stored or default). Pure functions of those points and of
 * document-space pointer coordinates, no Paper, no state of its own.
 * Baseline 1065e9d8: perspective-bridge.js axisCandidates / snapToVP (SNAP_DEG 6, 4 px of drag before a direction exists) /
 * findVPRayByAngle / projectOnRay; draw-bridge.js guideConstrain (GUIDE_PULL_MAX_DEG 30, GUIDE_PULL_STRENGTH 0.55: every sample past 4 px of
 * drag is blended PART-WAY toward its projection on the gizmo axis best matching the direction from the press, never snapped onto it);
 * shape-bridge.js maybeSnap (the Line end point snaps, then Shift constrains). With the guide off nothing changes. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativePerspectiveSnap = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  var SNAP_DEG = 6, PULL_MAX_DEG = 30, PULL_STRENGTH = 0.55, FLOOR = 4;

  // The guide's vanishing points when the projection says the guide is on, else null.
  function points(projection) {
    var p = projection && projection.perspective, s = p && p.settings;
    if (!s || s.perspectiveEnabled !== true || !Array.isArray(p.items)) return null;
    var out = p.items.filter(function (v) { return v && Number.isFinite(v.x) && Number.isFinite(v.y); });
    return out.length ? out : null;
  }
  // The exact axis set the cursor gizmo draws from `from`: one unit direction per vanishing point (none when sitting on it), then the vertical.
  function axes(vps, from) {
    var out = [];
    vps.forEach(function (vp) {
      var vx = vp.x - from[0], vy = vp.y - from[1], len = Math.hypot(vx, vy);
      if (len < 1) return;
      out.push({ x: vx / len, y: vy / len });
    });
    out.push({ x: 0, y: 1 });
    return out;
  }
  function gap(a, b) { return Math.abs(Math.atan2(Math.sin(a - b), Math.cos(a - b))); }
  // The axis closest in angle to the direction start -> cur within `tolDeg`, as a ray {px, py, dx, dy}, or null (findVPRayByAngle).
  function findRay(vps, start, cur, tolDeg) {
    var angle = Math.atan2(cur[1] - start[1], cur[0] - start[0]), best = null, bestDiff = (tolDeg || SNAP_DEG) * Math.PI / 180;
    axes(vps, start).forEach(function (ax) {
      var diff = gap(angle, Math.atan2(ax.y, ax.x));
      if (diff < bestDiff) { bestDiff = diff; best = { px: start[0], py: start[1], dx: ax.x, dy: ax.y }; }
    });
    return best;
  }
  function project(pt, ray) {
    var t = (pt[0] - ray.px) * ray.dx + (pt[1] - ray.py) * ray.dy;
    return [ray.px + ray.dx * t, ray.py + ray.dy * t];
  }
  // snapToVP: `end` moved onto the nearest axis in angle, keeping the drawn length; unchanged when no axis is within SNAP_DEG or the drag is under 4 px.
  function snapPoint(vps, start, end) {
    var len = Math.hypot(end[0] - start[0], end[1] - start[1]);
    if (len < FLOOR) return end;
    var ray = findRay(vps, start, end, SNAP_DEG);
    return ray ? [start[0] + ray.dx * len, start[1] + ray.dy * len] : end;
  }
  // guideConstrain: a sample blended toward its projection, the weight easing from PULL_STRENGTH at a perfect aim to 0 at PULL_MAX_DEG.
  function pullPoint(vps, start, w) {
    if (Math.hypot(w[0] - start[0], w[1] - start[1]) < FLOOR) return w;
    var ray = findRay(vps, start, w, PULL_MAX_DEG);
    if (!ray) return w;
    var diff = gap(Math.atan2(w[1] - start[1], w[0] - start[0]), Math.atan2(ray.dy, ray.dx)) * 180 / Math.PI;
    var t = Math.max(0, 1 - diff / PULL_MAX_DEG) * PULL_STRENGTH;
    if (t <= 0) return w;
    var proj = project(w, ray);
    return [w[0] + (proj[0] - w[0]) * t, w[1] + (proj[1] - w[1]) * t];
  }
  // The gesture entry points: `projection` is the perspective read the gesture began under (NemoNativePerspectiveController.read()).
  function line(projection, start, end) { var v = points(projection); return v ? snapPoint(v, start, end) : end; }
  function pull(projection, start, w) { var v = points(projection); return v ? pullPoint(v, start, w) : w; }
  return Object.freeze({ SNAP_DEG: SNAP_DEG, PULL_MAX_DEG: PULL_MAX_DEG, PULL_STRENGTH: PULL_STRENGTH, points: points, axes: axes, findRay: findRay, project: project,
    snapPoint: snapPoint, pullPoint: pullPoint, line: line, pull: pull });
}));
