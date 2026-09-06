'use strict';
// Independent expectations for the fixture corpus (R03).
//
// Nothing in this file imports application code. Every function is a plain
// restatement of a documented rule (CLAUDE.md, motion.js/app.js header
// comments, the After Effects conventions Nemo mirrors), written so that a
// fixture's expected values owe nothing to the implementation they judge —
// the same discipline tests/easing-reference.test.cjs already applies to the
// easing solvers. If a check here disagrees with the application, the test
// fails and a human decides which side is wrong; the reference is never
// "fixed" by copying the implementation's answer.

// --- keyframe interpolation -------------------------------------------------

// motion.js's DEFAULT_CURVE is a Catmull-Rom spline THROUGH the on-curve
// waypoints (0,0) (0.25,0.156) (0.5,0.5) (0.75,0.844) (1,1). A spline that
// interpolates its control points passes through them exactly, so at those
// five abscissae the eased fraction is known without evaluating any spline.
// Expectations are only ever emitted at these fractions.
const DEFAULT_EASE_WAYPOINTS = { 0: 0, 0.25: 0.156, 0.5: 0.5, 0.75: 0.844, 1: 1 };
function defaultEaseAt(t) {
  if (!(t in DEFAULT_EASE_WAYPOINTS)) throw new Error(`defaultEaseAt: no independent value at t=${t}; use a waypoint fraction`);
  return DEFAULT_EASE_WAYPOINTS[t];
}

// A two-point curve (0,0)→(1,1) has no interior knot: the eased fraction is
// the fraction itself.
const LINEAR_CURVE = [{ x: 0, y: 0 }, { x: 1, y: 1 }];
function linearEaseAt(t) { return t; }

function lerpVec(a, b, y) { return a.map((v, i) => v + (b[i] - v) * y); }

// Value of a track between two keys `a` and `b` at `frame`, given the eased
// fraction function of the segment (default or linear). Hold keys pin `a`
// (the flag lives on the OUTGOING key, AE convention). Outside the key range
// the value is clamped to the first/last key.
function trackValueAt(keys, frame, easeAt) {
  if (frame <= keys[0].frame) return keys[0].v.slice();
  const last = keys[keys.length - 1];
  if (frame >= last.frame) return last.v.slice();
  let i = 0;
  while (!(frame >= keys[i].frame && frame < keys[i + 1].frame)) i++;
  const a = keys[i], b = keys[i + 1];
  if (a.hold) return a.v.slice();
  const t = (frame - a.frame) / (b.frame - a.frame);
  return lerpVec(a.v, b.v, easeAt(t));
}

// Spatial bezier for `position` when a key carries hOut / hIn handles: a
// cubic from a.v through a.v+hOut and b.v+hIn to b.v, parameterised by the
// eased fraction.
function cubicBezier2(p0, p1, p2, p3, y) {
  const v = 1 - y;
  return [0, 1].map((d) => v * v * v * p0[d] + 3 * v * v * y * p1[d] + 3 * v * y * y * p2[d] + y * y * y * p3[d]);
}
function spatialPositionAt(a, b, frame, easeAt) {
  const t = (frame - a.frame) / (b.frame - a.frame);
  const y = easeAt(t);
  const ho = a.hOut || [0, 0], hi = b.hIn || [0, 0];
  return cubicBezier2(a.v, [a.v[0] + ho[0], a.v[1] + ho[1]], [b.v[0] + hi[0], b.v[1] + hi[1]], b.v, y);
}

// --- Animation 2D frame holds ------------------------------------------------

// A non-keyframe shows the most recent keyframe at or before it (traditional
// "on twos/threes" holds). Before the first keyframe there is nothing.
function heldKeyframeAt(keyframeFrames, frame) {
  let best = null;
  for (const k of keyframeFrames) if (k <= frame && (best === null || k > best)) best = k;
  return best;
}

// A layer with explicit inPoint/outPoint shows nothing outside [in, out].
function visibleInRange(inPoint, outPoint, frame, totalFrames) {
  const lo = inPoint == null ? 0 : inPoint, hi = outPoint == null ? totalFrames - 1 : outPoint;
  return frame >= lo && frame <= hi;
}

// --- component instances -----------------------------------------------------

// Internal frame shown by a component instance at main-timeline `frame`
// (resolveSymbolFrameIdx's documented modes; timeRemap/componentFrame not
// modelled here — fixtures that use them carry their own expectations).
function symbolFrameAt(mode, total, frame, { placedAt = 0, speed = 1, singleFrame = 0, trimIn = 0, trimOut = null } = {}) {
  const elapsed = Math.max(0, frame - placedAt) * speed;
  const n = Math.max(1, total);
  if (mode === 'single') return Math.min(n - 1, Math.max(0, Math.floor(singleFrame)));
  if (mode === 'once') { const out = trimOut == null ? n - 1 : trimOut; return Math.min(out, trimIn + Math.floor(elapsed)); }
  if (mode === 'pingpong') { if (n < 2) return 0; const cycle = (n - 1) * 2; const pos = Math.floor(elapsed) % cycle; return pos < n ? pos : cycle - pos; }
  return Math.floor(elapsed) % n; // loop
}

// --- colour and compositing --------------------------------------------------

// #rrggbb or #rrggbbaa → [r,g,b,a] with a in 0..1 (CLAUDE.md §2 hex8 rule).
function hexToRgba(hex) {
  const h = hex.replace('#', '');
  if (h.length !== 6 && h.length !== 8) throw new Error('hexToRgba: ' + hex);
  const n = (i) => parseInt(h.slice(i, i + 2), 16);
  return [n(0), n(2), n(4), h.length === 8 ? n(6) / 255 : 1];
}

// Source-over of an rgba colour (a in 0..1) on an opaque rgb background,
// rounded to 8-bit. `alphaScale` multiplies the source alpha (layer/item
// opacity).
function over(src, bg, alphaScale = 1) {
  const a = src[3] * alphaScale;
  return [0, 1, 2].map((i) => Math.round(src[i] * a + bg[i] * (1 - a)));
}

// --- geometry ----------------------------------------------------------------

function pointsBounds(points) {
  let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
  for (const [x, y] of points) { x0 = Math.min(x0, x); y0 = Math.min(y0, y); x1 = Math.max(x1, x); y1 = Math.max(y1, y); }
  return { x: x0, y: y0, width: x1 - x0, height: y1 - y0 };
}

// Even-odd point-in-polygon for the mesh invariants.
function pointInPolygon(poly, x, y) {
  let inside = false;
  for (let i = 0, j = poly.length - 1; i < poly.length; j = i++) {
    const xi = poly[i][0], yi = poly[i][1], xj = poly[j][0], yj = poly[j][1];
    if ((yi > y) !== (yj > y) && x < ((xj - xi) * (y - yi)) / (yj - yi) + xi) inside = !inside;
  }
  return inside;
}

// Normalised mesh vertex (u,v in 0..1 over the raster's display rect, image
// centre at x,y, unrotated) → world point.
function meshVertexWorld(rect, u, v) {
  return [rect.x - rect.width / 2 + u * rect.width, rect.y - rect.height / 2 + v * rect.height];
}

module.exports = {
  DEFAULT_EASE_WAYPOINTS, defaultEaseAt, LINEAR_CURVE, linearEaseAt, lerpVec, trackValueAt, cubicBezier2, spatialPositionAt,
  heldKeyframeAt, visibleInRange, symbolFrameAt, hexToRgba, over, pointsBounds, pointInPolygon, meshVertexWorld,
};
