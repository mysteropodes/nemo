// @ts-check
// Motion's pure on-curve easing kernel. Classic scripts and CommonJS execute
// the same source; only this explicit compatibility namespace is published.
// No document, UI, Paper, native bridge, timers, or writable module state.
/** @typedef {{x: number, y: number, tx?: number, ty?: number}} CurvePoint */
var SMAnimationCurve = (function () {
  'use strict';
  /** @param {number} t @param {number} a @param {number} b @param {number} c @param {number} d */
  function curveCubicAt(t, a, b, c, d) { var u = 1 - t; return u * u * u * a + 3 * u * u * t * b + 3 * u * t * t * c + t * t * t * d; }
  /** @param {number} t @param {number} a @param {number} b @param {number} c @param {number} d */
  function curveCubicDerivAt(t, a, b, c, d) { var u = 1 - t; return 3 * u * u * (b - a) + 6 * u * t * (c - b) + 3 * t * t * (d - c); }
  // Preserve the editor's manual tangents and automatic slope limiter.
  // ui.js owns a separate editor evaluator; this extraction changes no math.
  /** @param {readonly CurvePoint[]} pts @param {number} i */
  function curveTangentAt(pts, i) {
    var p = pts[i];
    if (typeof p.tx === 'number') return { x: p.tx, y: p.ty || 0 };
    var prev = pts[i - 1] || p, next = pts[i + 1] || p;
    var tx = (next.x - prev.x) / 2;
    var dx0 = p.x - prev.x, dx1 = next.x - p.x;
    var s0 = dx0 > 1e-9 ? (p.y - prev.y) / dx0 : 0, s1 = dx1 > 1e-9 ? (next.y - p.y) / dx1 : 0;
    var m;
    if (prev === p) m = s1;
    else if (next === p) m = s0;
    else if (s0 * s1 <= 0) m = 0;
    else {
      m = (s0 + s1) / 2;
      var lim = 3 * Math.min(Math.abs(s0), Math.abs(s1));
      if (Math.abs(m) > lim) m = (m > 0 ? 1 : -1) * lim;
    }
    return { x: tx, y: m * tx };
  }
  /** @param {readonly CurvePoint[]} pts @param {number} i */
  function curveSegCtrl(pts, i) {
    var p0 = pts[i], p3 = pts[i + 1];
    var t1 = curveTangentAt(pts, i), t2 = curveTangentAt(pts, i + 1);
    return { c1: { x: p0.x + t1.x / 3, y: p0.y + t1.y / 3 }, c2: { x: p3.x - t2.x / 3, y: p3.y - t2.y / 3 } };
  }
  /** @param {readonly CurvePoint[]} pts @param {number} x */
  function curveSegFor(pts, x) { var i = 0; while (i < pts.length - 2 && pts[i + 1].x < x) i++; return i; }
  /**
   * Evaluate on-curve waypoints without changing or caching caller-owned data.
   * Missing/short curves pass x through BEFORE clamping. Otherwise clamp x,
   * preserve unbounded y (manual overshoot), and use the existing eight-step
   * Newton solve with derivative early exit; there is no bisection fallback.
   * Normal editor input has finite coordinates, ordered x and endpoints 0/1.
   * Invalid imported geometry is not repaired here; legacy degeneracies remain.
   * @param {readonly CurvePoint[] | null | undefined} pts
   * @param {number} x
   * @returns {number}
   */
  function evalCurvePoints(pts, x) {
    if (!pts || pts.length < 2) return x;
    x = Math.max(0, Math.min(1, x));
    var i = curveSegFor(pts, x), p0 = pts[i], p3 = pts[i + 1], ctrl = curveSegCtrl(pts, i);
    var span = p3.x - p0.x, t = span > 1e-6 ? (x - p0.x) / span : 0;
    for (var k = 0; k < 8; k++) {
      var ex = curveCubicAt(t, p0.x, ctrl.c1.x, ctrl.c2.x, p3.x) - x;
      var dx = curveCubicDerivAt(t, p0.x, ctrl.c1.x, ctrl.c2.x, p3.x);
      if (Math.abs(dx) < 1e-6) break;
      t -= ex / dx; t = Math.max(0, Math.min(1, t));
    }
    return curveCubicAt(t, p0.y, ctrl.c1.y, ctrl.c2.y, p3.y);
  }

  return { evalCurvePoints: evalCurvePoints };
})();
if (typeof module !== 'undefined' && module.exports) module.exports = SMAnimationCurve;
