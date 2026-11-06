/* Native Select freehand lasso (#2372 A-select-skew-anchor-lasso). Baseline (1065e9d8 select-bridge.js :1773 and onUp ~2725): Alt+drag on empty canvas
 * (no selection to move a pivot of) draws a freehand path instead of a rectangle; on release, with more than 2 points, the path is closed and an item
 * is selected when its bounds meet the lasso's bounds AND its centre lies inside the lasso or (for anything but an image) the lasso crosses it, because
 * the lasso may wander and the bounds test alone is not enough. A lasso of 2 points or fewer is the plain bounds rectangle. The candidates are the same
 * ones the marquee reads (every visible unlocked layer, a linked fill and a texture dab never their own selection), committed through the marquee's
 * path by the gesture. Pure geometry here: it reads the presented paths it is given and holds no document state. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeSelectLasso = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  var FLATTEN = 8, MIN_STEP_PX = 1.5;
  // Even-odd point in polygon (`poly` is [[x, y], ...], implicitly closed).
  function contains(poly, x, y) {
    var inside = false;
    for (var i = 0, j = poly.length - 1; i < poly.length; j = i++) {
      var a = poly[i], b = poly[j];
      if ((a[1] > y) !== (b[1] > y) && x < (b[0] - a[0]) * (y - a[1]) / (b[1] - a[1]) + a[0]) inside = !inside;
    }
    return inside;
  }
  function cross(ax, ay, bx, by, cx, cy, dx, dy) {
    var d = (bx - ax) * (dy - cy) - (by - ay) * (dx - cx);
    if (d === 0) return false;
    var t = ((cx - ax) * (dy - cy) - (cy - ay) * (dx - cx)) / d, u = ((cx - ax) * (by - ay) - (cy - ay) * (bx - ax)) / d;
    return t >= 0 && t <= 1 && u >= 0 && u <= 1;
  }
  // A presented path as a polyline: its anchors with each cubic sampled (a cubic lies near its control polygon, FLATTEN steps is plenty for a hit).
  function polyline(path) {
    var segs = path.segments || [], out = [];
    for (var i = 0; i < segs.length; i++) {
      var a = segs[i], b = segs[i + 1] || (path.closed ? segs[0] : null);
      out.push([a.point.x, a.point.y]);
      if (!b) continue;
      var p0 = a.point, p1 = { x: a.point.x + a.handleOut.x, y: a.point.y + a.handleOut.y }, p3 = b.point, p2 = { x: b.point.x + b.handleIn.x, y: b.point.y + b.handleIn.y };
      for (var s = 1; s < FLATTEN; s++) {
        var t = s / FLATTEN, u = 1 - t;
        out.push([u * u * u * p0.x + 3 * u * u * t * p1.x + 3 * u * t * t * p2.x + t * t * t * p3.x, u * u * u * p0.y + 3 * u * u * t * p1.y + 3 * u * t * t * p2.y + t * t * t * p3.y]);
      }
    }
    if (path.closed && out.length) out.push(out[0]);
    return out;
  }
  // True when some edge of the lasso crosses some edge of the path (the lasso closed).
  function crosses(poly, path) {
    var line = polyline(path);
    for (var i = 0; i < poly.length; i++) {
      var a = poly[i], b = poly[(i + 1) % poly.length];
      for (var j = 0; j + 1 < line.length; j++) if (cross(a[0], a[1], b[0], b[1], line[j][0], line[j][1], line[j + 1][0], line[j + 1][1])) return true;
    }
    return false;
  }
  function boundsOf(poly) {
    return poly.reduce(function (b, p) { return [Math.min(b[0], p[0]), Math.min(b[1], p[1]), Math.max(b[2], p[0]), Math.max(b[3], p[1])]; }, [Infinity, Infinity, -Infinity, -Infinity]);
  }
  // The candidate paths (those whose bounds already meet the lasso's bounds) the lasso really takes. With 2 points or fewer the bounds test is all there is.
  function pick(poly, paths) {
    if (poly.length <= 2) return paths.slice();
    return paths.filter(function (p) {
      var b = p.bounds, c = [(b.left + b.right) / 2, (b.top + b.bottom) / 2], image = p.data && p.data.nativeRole === 'raster';
      return contains(poly, c[0], c[1]) || (!image && crosses(poly, p));
    });
  }
  // The point to add to a lasso being drawn: null while the pointer has moved less than a pixel (`wpp` world units per screen pixel).
  function extend(poly, at, wpp) {
    var last = poly[poly.length - 1];
    return last && Math.hypot(at.x - last[0], at.y - last[1]) < MIN_STEP_PX * wpp ? null : [at.x, at.y];
  }
  // The lasso's committed selection: candidates by its bounds (the marquee read), then the lasso's own test. null when the canvas could not be read.
  async function select(root, poly) {
    var I = root.NemoSelectCanvasIntent, found = await I.marquee(root, boundsOf(poly));
    return found && pick(poly, found);
  }
  return Object.freeze({ contains: contains, crosses: crosses, pick: pick, extend: extend, select: select, boundsOf: boundsOf });
}));
