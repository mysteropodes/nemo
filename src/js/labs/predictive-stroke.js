// ---- LABS PROTOTYPE — Predictive Stroke (Autodesk SketchBook-style) ----
// Recognizes a freehand-drawn LINE, CIRCLE/ELLIPSE, or RECTANGLE at commit
// time and snaps the stroke to the perfect shape, in place. Flag
// 'predictive-stroke' via labs-core.js console API, off by default.
//
// MUST be loaded (and therefore registered) BEFORE symmetry-mirror.js in
// index.html — labs-core fans the commit hook out in registration order,
// and mirror/radial copies should reflect the CORRECTED shape, not the raw
// wobbly one.
//
// Mutates the already-committed Path's own segments (never replaces the
// object), so strokeId/owner/paint/undo/save all keep pointing at the same
// item — per CLAUDE.md §1 there is nothing structurally new here for any
// layer.children consumer to learn about. Skipped entirely for
// vector-brush/fill-shape strokes (their geometry is an outline rebuilt
// from centerSegments, not the drawn centerline — snapping the outline
// would fight rebuildVectorBrushOutline).
(function () {
  // -- geometry helpers ---------------------------------------------------
  // Sample the path's REAL curve geometry at even arc-length intervals
  // rather than reading segment anchors: by the time the commit hook runs,
  // the stroke has already been bezier-simplified (path.simplify /
  // buildCenterSegments), leaving few anchors whose straight-line polyline
  // cuts every corner — RDP corner detection on those anchors misses a
  // cleanly-drawn rectangle entirely (found live: 41-point drawn rect →
  // 15 anchors → no 4-corner match). 64 curve samples restore the actual
  // drawn geometry regardless of how simplify placed its anchors.
  function pts(path) {
    var L = path.length;
    if (!L || path.segments.length < 3) return path.segments.map(function (s) { return s.point; });
    var out = [], N = 64;
    for (var i = 0; i <= N; i++) out.push(path.getPointAt(Math.min(L, i / N * L)));
    return out;
  }
  function dist(a, b) { return a.getDistance(b); }

  function pathLen(P) { var L = 0; for (var i = 1; i < P.length; i++) L += dist(P[i - 1], P[i]); return L; }

  // Max perpendicular deviation from the first→last chord, as a fraction
  // of chord length. Straight-line test.
  function lineDeviation(P) {
    var a = P[0], b = P[P.length - 1], ab = b.subtract(a), L = ab.length;
    if (L < 1e-6) return Infinity;
    var maxD = 0;
    for (var i = 1; i < P.length - 1; i++) {
      var ap = P[i].subtract(a);
      var d = Math.abs(ap.x * ab.y - ap.y * ab.x) / L;
      if (d > maxD) maxD = d;
    }
    return maxD / L;
  }

  // Ellipse test: for a closed-ish stroke, normalize each point's offset
  // from the bbox center by the bbox radii — on a perfect ellipse every
  // normalized radius is exactly 1. Low variance = ellipse.
  function ellipseError(P, bounds) {
    var cx = bounds.center.x, cy = bounds.center.y;
    var rx = Math.max(1e-6, bounds.width / 2), ry = Math.max(1e-6, bounds.height / 2);
    var sum = 0, sum2 = 0, n = P.length;
    for (var i = 0; i < n; i++) {
      var r = Math.sqrt(Math.pow((P[i].x - cx) / rx, 2) + Math.pow((P[i].y - cy) / ry, 2));
      sum += r; sum2 += r * r;
    }
    var mean = sum / n;
    return Math.sqrt(Math.max(0, sum2 / n - mean * mean)); // stddev of normalized radius
  }

  // Rectangle test: Ramer-Douglas-Peucker down to dominant corners; a
  // rectangle simplifies to exactly 4 corners with ~90° turns and sides
  // roughly parallel to the drawn bbox.
  function rdp(P, eps) {
    if (P.length < 3) return P.slice();
    var a = P[0], b = P[P.length - 1], ab = b.subtract(a), L = Math.max(1e-6, ab.length);
    var maxD = -1, idx = -1;
    for (var i = 1; i < P.length - 1; i++) {
      var ap = P[i].subtract(a);
      var d = Math.abs(ap.x * ab.y - ap.y * ab.x) / L;
      if (d > maxD) { maxD = d; idx = i; }
    }
    if (maxD <= eps) return [a, b];
    var left = rdp(P.slice(0, idx + 1), eps), right = rdp(P.slice(idx), eps);
    return left.slice(0, -1).concat(right);
  }
  // RDP on a CLOSED loop directly is degenerate: with first==last the
  // baseline chord is zero-length, every deviation blows up against a
  // point, and the recursion collapses to a single vertex (found live:
  // a cleanly-drawn rectangle loop -> 1 "corner"). Standard fix: cut the
  // loop at the point farthest from its start (a guaranteed real extremity
  // of the shape), RDP the two halves against real nonzero chords, join.
  function loopCorners(P, eps) {
    var fi = 1, fd = -1;
    for (var i = 1; i < P.length; i++) { var d = P[i].getDistance(P[0]); if (d > fd) { fd = d; fi = i; } }
    if (fi <= 0 || fi >= P.length - 1) return rdp(P, eps);
    var h1 = rdp(P.slice(0, fi + 1), eps), h2 = rdp(P.slice(fi), eps);
    // h1 ends where h2 starts (P[fi]); h2 ends back at the start point —
    // drop both junction duplicates so each corner appears exactly once.
    return h1.slice(0, -1).concat(h2.slice(0, -1));
  }
  function isRectCorners(corners) {
    if (corners.length !== 4) return false;
    for (var i = 0; i < 4; i++) {
      var p0 = corners[(i + 3) % 4], p1 = corners[i], p2 = corners[(i + 1) % 4];
      var v1 = p1.subtract(p0), v2 = p2.subtract(p1);
      if (v1.length < 1e-6 || v2.length < 1e-6) return false;
      var cos = Math.abs(v1.dot(v2) / (v1.length * v2.length));
      if (cos > 0.35) return false; // ~ >70° off a right angle
    }
    return true;
  }

  // -- snap replacements (mutate segments in place) -----------------------
  function snapToLine(path) {
    var a = path.firstSegment.point, b = path.lastSegment.point;
    path.removeSegments();
    path.add(a); path.add(b);
    path.closed = false;
  }
  function snapToEllipse(path, bounds) {
    var tmp = new Path.Ellipse({ rectangle: bounds, insert: false });
    path.removeSegments();
    tmp.segments.forEach(function (s) {
      path.add(new Segment(s.point.clone(), s.handleIn.clone(), s.handleOut.clone()));
    });
    tmp.remove();
    path.closed = true;
  }
  function snapToRect(path, corners) {
    path.removeSegments();
    corners.forEach(function (p) { path.add(p); });
    path.closed = true;
  }

  window.SMLabs.register('predictive-stroke', {
    flag: 'nemo-labs-predictive',
    describe: 'labsDescribePredictiveStroke',
    onStroke: function (path) {
      if (!path.segments || path.segments.length < 3) return;
      // Vector-brush ribbons & fill shapes: geometry is a rebuilt outline,
      // not the drawn centerline — leave them alone.
      if (path.data && (path.data.isVectorBrush || path.data.isFillShape || path.data.centerSegments)) return;
      var P = pts(path);
      var L = pathLen(P);
      if (L < 40) return; // dot / tiny tick — never "correct" those

      // 1. Line: chord deviation under 4% of length.
      if (lineDeviation(P) < 0.04) { snapToLine(path); return; }

      var closedish = dist(P[0], P[P.length - 1]) < 0.18 * L;
      if (!closedish) return;
      var bounds = path.bounds;

      // 2. Ellipse: normalized-radius stddev under 0.09.
      if (ellipseError(P, bounds) < 0.09) { snapToEllipse(path, bounds); return; }

      // 3. Rectangle: simplifies to 4 near-right-angle corners.
      var closed = P.slice();
      closed[closed.length - 1] = closed[0]; // exact loop for the split-and-RDP below
      var corners = loopCorners(closed, Math.max(6, L * 0.03));
      if (isRectCorners(corners)) { snapToRect(path, corners); return; }
    },
  });
})();
