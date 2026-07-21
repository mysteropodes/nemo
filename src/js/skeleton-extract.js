// ---- Shapper Intelligence — skeleton extraction (M1) ----
// Auto-extracts a branching centerline/skeleton from a selected filled
// shape, the same 3-step pipeline aescripts CenterLine uses:
//   1. Rasterize the shape to a binary mask (reusing the exact offscreen-
//      canvas pattern fillVectorFindRaster already established in tools.js —
//      bounds -> scaled canvas -> fill -> getImageData).
//   2. Zhang-Suen thinning: iteratively erode the mask down to a 1px-wide
//      skeleton. Nothing like this exists anywhere else in this codebase
//      (confirmed by search) — genuinely new code, unlike step 3 below.
//   3. Schneider curve-fitting + node reduction: Paper.js's own
//      `Path.simplify(tolerance)` IS Schneider's algorithm, already
//      vendored and used everywhere else in this codebase for exactly this
//      "raw point chain -> smooth minimal-node bezier" purpose (see
//      fillVectorFindRaster's own `rawPath.simplify(...)` call) — no new
//      curve-fitting math needed, just reused here.
//
// Unlike a single-spine skeletonizer, this walks the THINNED pixel graph to
// find junctions (3+ skeleton-neighbors) as well as endpoints (1 neighbor),
// so a branching shape (a hand, a star) produces a real graph — multiple
// branch curves meeting at shared junction nodes — not one flattened curve.
//
// Output is deliberately 100% plain JSON (no live Paper.js object
// references anywhere in the returned graph) — this is what lets M3
// persist it as `data.skeleton` via serP/desP without needing a
// relink-after-loadFrame() step the way rig-deform.js's live `binds[].path`
// reference currently has to (see CLAUDE.md §1's "live reference survives
// session only" trap, and rig-deform.js's own rebindAfterFrameChange()).
(function () {
  // ---- Step 1: rasterize a Path/CompoundPath to a binary mask -----------
  // Mirrors fillVectorFindRaster (tools.js) almost exactly: same bounds ->
  // scaled-canvas -> fill pattern, just filling the TARGET shape itself
  // (not tracing wall outlines around it) since we need its solid interior,
  // not an enclosed empty region.
  // Accepts either a single Path/CompoundPath or an ARRAY of them — a
  // multi-shape selection (2026-07: "je n'arrive pas à select un ensemble...
  // pour faire le skeleton") rasterizes as the UNION of every shape's fill
  // (each is filled into the same canvas in turn, so overlapping/adjacent
  // shapes merge into one silhouette), producing ONE shared skeleton that
  // every selected shape's vertices can then bind against.
  // Sample-based nearest point on `path` to document point `pt` — same
  // "walk getPointAt in N steps" pattern the fallback polygon rasterizer
  // below already uses, reused here so bridging needs no new Paper.js API.
  // `excludeArc` (optional): {center, span} in arc-length — samples whose
  // arc position falls within `span` of `center` are skipped. Used for
  // SELF-bridging (an endpoint reconnecting to its own stroke's body): the
  // stroke near the tip is trivially the nearest point, so that stretch
  // must be excluded for the search to find the far side it loops back to.
  function nearestPointOnPath(path, pt, samples, excludeArc) {
    samples = samples || 150;
    var len = path.length;
    if (len <= 0) return { point: path.firstSegment.point, dist: pt.getDistance(path.firstSegment.point) };
    var best = null, bestD2 = Infinity;
    for (var i = 0; i <= samples; i++) {
      var arc = Math.min(len, (i / samples) * len);
      if (excludeArc && Math.abs(arc - excludeArc.center) < excludeArc.span) continue;
      var p = path.getPointAt(arc);
      var dx = p.x - pt.x, dy = p.y - pt.y, d2 = dx * dx + dy * dy;
      if (d2 < bestD2) { bestD2 = d2; best = p; }
    }
    if (!best) return { point: path.firstSegment.point, dist: Infinity };
    return { point: best, dist: Math.sqrt(bestD2) };
  }

  // LOCAL fusion by endpoint proximity (2026-07 feedback: "il faudrait des
  // conditions... fusion locale par proximité d'extrémités" — a global
  // closing radius bridges ANY two strokes that pass near each other
  // anywhere in the sketch, including two lines that happen to run close
  // together well before their real convergence, not just the joint the
  // artist intends; tried and reverted, see git history). This instead
  // only bridges from a stroke's own ENDPOINT to the nearest point on
  // ANOTHER stroke — the actual artist gesture "this line's tip continues
  // that one" — leaving two strokes' interiors alone even when they pass
  // close together (a V's legs, a corridor's parallel walls), since
  // neither stroke actually TERMINATES there.
  // `selfRadiusDoc`: cap for SELF-bridges (a tip reconnecting to its own
  // stroke). `crossRadiusDoc`: cap for bridges to OTHER strokes — pass
  // Infinity to always weld the selection into one silhouette (2026-07
  // feedback: "une forme sans fill... il faudrait juste fermer
  // virtuellement les gaps pour le calcul de bone" — selecting several
  // strokes for one rig IS the statement that they form one shape, so
  // their tips always connect, however wide the gaps; the skeleton then
  // computes on the same solid silhouette a filled version would give).
  function computeEndpointBridges(paths, selfRadiusDoc, crossRadiusDoc) {
    var bridges = [];
    for (var i = 0; i < paths.length; i++) {
      var p = paths[i];
      if (p.fillColor || p.closed || !p.segments || p.segments.length < 2) continue;
      var ends = [p.firstSegment.point, p.lastSegment.point];
      for (var e = 0; e < ends.length; e++) {
        var ep = ends[e];
        var bestPt = null, bestDist = Infinity, bestW = p.strokeWidth || 2, bestLimit = 0;
        for (var j = 0; j < paths.length; j++) {
          var res, limit;
          if (j === i) {
            // SELF-bridge (2026-07 follow-up: a contour drawn as ONE stroke
            // whose ends nearly meet got no bridge at all — j===i was
            // skipped outright — so the ring never closed and the skeleton
            // hugged the band). The tip may reconnect to its own stroke's
            // body (closing the outline, or a hand loop returning to the
            // wrist) — exclude the 25% of arc adjacent to this endpoint so
            // the search finds the far side it loops back to, not itself.
            // Kept CAPPED (unlike cross-stroke bridges): an uncapped self-
            // bridge would close every plain V-shaped line into a triangle.
            var endArc = (e === 0) ? 0 : p.length;
            res = nearestPointOnPath(p, ep, 150, { center: endArc, span: p.length * 0.25 });
            limit = selfRadiusDoc;
          } else {
            res = nearestPointOnPath(paths[j], ep);
            limit = crossRadiusDoc;
          }
          if (res.dist < bestDist) { bestDist = res.dist; bestPt = res.point; bestW = Math.max(p.strokeWidth || 2, paths[j].strokeWidth || 2); bestLimit = limit; }
        }
        if (bestPt && bestDist > 1 && bestDist <= bestLimit) bridges.push({ from: ep, to: bestPt, width: bestW });
      }
    }
    return bridges;
  }

  // `closeRadiusDoc` (optional): endpoint-bridge cap in DOCUMENT px from
  // the Tool Options "Fusion" field. 0/undefined = automatic: cross-stroke
  // bridges UNLIMITED (a multi-stroke selection always welds into one
  // silhouette — the selection is the grouping intent), self-bridges
  // capped near the stroke's own width. A manual value caps BOTH (the
  // escape hatch when two selected strokes must NOT connect).
  function rasterizeShape(pathOrPaths, maxDim, closeRadiusDoc) {
    maxDim = maxDim || 512;
    var paths = Array.isArray(pathOrPaths) ? pathOrPaths : [pathOrPaths];
    var bounds = null;
    paths.forEach(function (p) { bounds = bounds ? bounds.unite(p.bounds) : p.bounds.clone(); });
    bounds = bounds.expand(4);
    var scale = Math.min(1, maxDim / Math.max(bounds.width, bounds.height, 1));
    var rw = Math.max(1, Math.round(bounds.width * scale));
    var rh = Math.max(1, Math.round(bounds.height * scale));
    var canvas = document.createElement('canvas');
    canvas.width = rw; canvas.height = rh;
    var ctx = canvas.getContext('2d');
    ctx.fillStyle = '#000';
    ctx.save();
    ctx.translate(-bounds.x * scale, -bounds.y * scale);
    ctx.scale(scale, scale);
    var strokeWidths = paths.filter(function (p) { return !p.fillColor; }).map(function (p) { return p.strokeWidth || 2; });
    var avgSW = strokeWidths.length ? strokeWidths.reduce(function (a, b) { return a + b; }, 0) / strokeWidths.length : 4;
    var manual = closeRadiusDoc && closeRadiusDoc > 0;
    var selfR = manual ? closeRadiusDoc : Math.max(10, avgSW * 3);
    var crossR = manual ? closeRadiusDoc : Infinity;
    var bridges = computeEndpointBridges(paths, selfR, crossR);
    paths.forEach(function (path) {
      ctx.beginPath();
      // A stroke-only shape (fillColor null — a plain hand-drawn line, e.g.
      // an arm/hand doodle with Stroke Only) has no interior to fill: filling
      // its raw path data treats it as an implicit closed polygon (a straight
      // chord from the last point back to the first), producing a degenerate
      // silhouette that has nothing to do with the visible ink. 2026-07
      // feedback (screenshot: a hand/arm outline where the desired skeleton
      // should run down the MIDDLE of the drawn line, not along some bogus
      // fill of its endpoints) — rasterize the STROKE'S OWN RIBBON instead,
      // as if the ink itself were the fill, by stroking with its real width/
      // cap/join. The thinning step below then finds the true centerline of
      // that ribbon, same as it would for any genuinely filled shape.
      var hasFill = !!path.fillColor;
      // VECTOR-driven closed-outline detection (2026-07 feedback: "pourquoi
      // tu ne prends pas en compte plutôt les vertices" — use the path's own
      // vertex data, which we have, instead of raster-side guessing): a
      // stroke whose two ENDPOINTS nearly meet is an outline the artist
      // drew around a region — its interior IS the intended shape, and the
      // implicit closing chord (first->last) is exactly the right bridge.
      // Filling such a path directly gives the delimited region with zero
      // heuristics. Only a genuinely OPEN line (endpoints far apart, e.g. a
      // single arm stroke) is rasterized as its ribbon alone, leaving the
      // morphological fusion below to merge it with its neighbors.
      var outlineClosed = false;
      if (!hasFill && path.segments && path.segments.length >= 3) {
        if (path.closed) outlineClosed = true;
        else {
          var endGap = path.firstSegment.point.getDistance(path.lastSegment.point);
          outlineClosed = endGap < Math.max((path.strokeWidth || 2) * 3, path.length * 0.1);
        }
        // Doubling-back detection (2026-07 feedback: "on voit que c'est une
        // forme non fermée, donc calculer ça avec la fermeture... avec les
        // coordonnées de vertices on pourrait vraiment définir la ligne
        // centrale dans la forme"): an OUTLINE stroke travels out along one
        // side of the shape and back along the other — most of its arc has
        // a far-in-arc-length counterpart running nearby in space (the two
        // sides of the limb, one body-thickness apart). A CENTERLINE stroke
        // (a plain V-shaped line) only self-approaches around its elbow.
        // Sample the path and measure what fraction of points have a
        // far-arc neighbor within ~8% of the shape's size: high fraction =
        // outline (close it via the implicit chord and fill, even when the
        // endpoint gap is large), low = genuine line (keep it a ribbon).
        if (!outlineClosed) {
          var NS = 48;
          var samples = [];
          for (var siP = 0; siP < NS; siP++) samples.push(path.getPointAt(path.length * (siP + 0.5) / NS));
          var sizeRef = Math.max(path.bounds.width, path.bounds.height);
          var nearD = Math.max((path.strokeWidth || 2) * 4, sizeRef * 0.08);
          var nearD2 = nearD * nearD;
          var minArcSep = Math.floor(NS * 0.3);
          var withNeighbor = 0;
          for (var ai = 0; ai < NS; ai++) {
            for (var aj = 0; aj < NS; aj++) {
              if (Math.abs(ai - aj) < minArcSep) continue;
              var ddx = samples[ai].x - samples[aj].x, ddy = samples[ai].y - samples[aj].y;
              if (ddx * ddx + ddy * ddy < nearD2) { withNeighbor++; break; }
            }
          }
          outlineClosed = withNeighbor / NS > 0.55;
        }
      }
      // Path.getPathData() gives an SVG-path-like string Paper.js itself can
      // produce; simplest robust route here is walking path.segments/curves
      // directly via Path2D from the exported SVG path data.
      var svgPath = path.exportSVG ? path.exportSVG().getAttribute('d') : null;
      if (svgPath) {
        var p2d = new Path2D(svgPath);
        if (hasFill) {
          ctx.fill(p2d, path.getFillRule ? path.getFillRule() : 'nonzero');
        } else {
          ctx.lineWidth = Math.max(1, path.strokeWidth || 2);
          ctx.lineCap = path.strokeCap || 'round';
          ctx.lineJoin = path.strokeJoin || 'round';
          ctx.stroke(p2d);
          // 'evenodd', not 'nonzero', for the implicit outline fill — a
          // hand-drawn outline routinely self-crosses (finger loops), and
          // nonzero winding can cancel those regions back to empty.
          if (outlineClosed) ctx.fill(p2d, 'evenodd');
        }
      } else {
        // Fallback: sample the path's own curves as a polygon (loses holes on
        // a CompoundPath, but never crashes).
        ctx.moveTo(path.firstSegment.point.x, path.firstSegment.point.y);
        var len = path.length, n = Math.max(8, Math.min(2000, Math.ceil(len / 2)));
        for (var i = 1; i <= n; i++) {
          var pt = path.getPointAt(Math.min(len, (i / n) * len));
          ctx.lineTo(pt.x, pt.y);
        }
        if (hasFill) {
          ctx.closePath();
          ctx.fill();
        } else {
          ctx.lineWidth = Math.max(1, path.strokeWidth || 2);
          ctx.lineCap = path.strokeCap || 'round';
          ctx.lineJoin = path.strokeJoin || 'round';
          ctx.stroke();
          if (outlineClosed) { ctx.closePath(); ctx.fill('evenodd'); }
        }
      }
    });
    // Draw the endpoint bridges as plain ink segments — once welded, any
    // ring they complete gets its interior flooded by fillEnclosedHoles
    // below, exactly like ink the artist drew themselves.
    ctx.lineCap = 'round';
    bridges.forEach(function (br) {
      ctx.beginPath();
      ctx.lineWidth = Math.max(3, br.width);
      ctx.moveTo(br.from.x, br.from.y);
      ctx.lineTo(br.to.x, br.to.y);
      ctx.stroke();
    });
    ctx.restore();
    var img = ctx.getImageData(0, 0, rw, rh);
    var mask = new Uint8Array(rw * rh);
    for (var px = 0; px < rw * rh; px++) mask[px] = img.data[px * 4 + 3] > 60 ? 1 : 0;
    // 2026-07 feedback (screenshot: a hand doodle's finger loops came back as
    // hollow RINGS — confirmed by directly sampling the raster mask, each
    // loop's interior pixel read 0/background). A single continuous pen
    // stroke that loops back near itself (a "finger") only paints its own
    // ribbon — the area it encloses stays empty, so thinning that ring finds
    // the ring's OWN centerline (a closed loop tracing the loop), not a
    // simple spine through a solid finger shape. Close every such hole: flood
    // -fill from the raster's OUTER border first (background genuinely
    // reachable from outside stays background), then anything still 0
    // afterward is enclosed by ink on all sides — promote it to foreground,
    // exactly matching "treat the doodle as if it were a solid filled shape."
    // Unconditional, not just for the stroke-only (anyStrokeOnly) branch —
    // this exact same hole can appear on an already-FILLED path too: the
    // default vector/pressure brush (draw-bridge.js, `state.vectorBrush`)
    // builds a proper filled ribbon, but a stroke that loops back near
    // itself can wind in a CANCELING direction under the 'nonzero' fill rule
    // (ctx.fill above), leaving the loop's interior unfilled the same way —
    // confirmed live: a filled self-crossing test path still needed this
    // pass, the anyStrokeOnly-gated version left it untouched. This tool's
    // whole purpose is "treat the drawing as one solid rig-able silhouette,"
    // so a rare deliberate donut/ring CompoundPath losing its hole here is
    // an acceptable trade — not a concern the fill-rule branch above needs
    // to special-case.
    mask = fillEnclosedHoles(mask, rw, rh);
    return { mask: mask, rw: rw, rh: rh, bounds: bounds, scale: scale };
  }

  function fillEnclosedHoles(mask, rw, rh) {
    var reachable = new Uint8Array(rw * rh);
    var stack = [];
    function seed(x, y) {
      var idx = y * rw + x;
      if (mask[idx] === 0 && !reachable[idx]) { reachable[idx] = 1; stack.push(idx); }
    }
    for (var x = 0; x < rw; x++) { seed(x, 0); seed(x, rh - 1); }
    for (var y = 0; y < rh; y++) { seed(0, y); seed(rw - 1, y); }
    while (stack.length) {
      var idx = stack.pop();
      var x = idx % rw, y = (idx / rw) | 0;
      if (x > 0) seed(x - 1, y);
      if (x < rw - 1) seed(x + 1, y);
      if (y > 0) seed(x, y - 1);
      if (y < rh - 1) seed(x, y + 1);
    }
    var result = new Uint8Array(mask);
    for (var i = 0; i < rw * rh; i++) if (mask[i] === 0 && !reachable[i]) result[i] = 1;
    return result;
  }

  // ---- Step 2: Zhang-Suen thinning --------------------------------------
  // Standard two-subiteration algorithm. `mask` is a flat Uint8Array (1 =
  // foreground). Returns a NEW Uint8Array (does not mutate the input) with
  // the shape eroded down to a 1px-wide skeleton.
  function zhangSuenThin(mask, rw, rh) {
    var img = new Uint8Array(mask);
    function at(x, y) { return (x < 0 || y < 0 || x >= rw || y >= rh) ? 0 : img[y * rw + x]; }
    var changed = true;
    var toRemove = [];
    while (changed) {
      changed = false;
      for (var sub = 0; sub < 2; sub++) {
        toRemove.length = 0;
        for (var y = 0; y < rh; y++) {
          for (var x = 0; x < rw; x++) {
            if (!img[y * rw + x]) continue;
            var p2 = at(x, y - 1), p3 = at(x + 1, y - 1), p4 = at(x + 1, y), p5 = at(x + 1, y + 1);
            var p6 = at(x, y + 1), p7 = at(x - 1, y + 1), p8 = at(x - 1, y), p9 = at(x - 1, y - 1);
            var b = p2 + p3 + p4 + p5 + p6 + p7 + p8 + p9;
            if (b < 2 || b > 6) continue;
            var seq = [p2, p3, p4, p5, p6, p7, p8, p9, p2];
            var a = 0;
            for (var k = 0; k < 8; k++) if (seq[k] === 0 && seq[k + 1] === 1) a++;
            if (a !== 1) continue;
            if (sub === 0) {
              if (p2 * p4 * p6 !== 0) continue;
              if (p4 * p6 * p8 !== 0) continue;
            } else {
              if (p2 * p4 * p8 !== 0) continue;
              if (p2 * p6 * p8 !== 0) continue;
            }
            toRemove.push(y * rw + x);
          }
        }
        if (toRemove.length) {
          changed = true;
          for (var ri = 0; ri < toRemove.length; ri++) img[toRemove[ri]] = 0;
        }
      }
    }
    return img;
  }

  // ---- Step 3: build a node/branch graph from the thinned skeleton ------
  // A confirmed-live Zhang-Suen artifact: a "staircase" step on any
  // non-45°/non-axis-aligned line leaves corner pixels with 3+ raw
  // 8-neighbors that are still topologically a simple curve bend, not a
  // real fork (e.g. E + S + SW all set, where S and SW are themselves
  // mutually adjacent — one incoming direction, one outgoing, just spread
  // across 2 physically-touching pixels instead of 1). Naive raw-neighbor-
  // COUNT classification flags every one of these as a junction, which
  // fragmented a plain 5-point star into 300+ false junctions (most
  // branches only ~2px long) before this fix.
  //
  // The correct fix is grouping: walk the 8 neighbors in a proper cyclic
  // ring (N,NE,E,SE,S,SW,W,NW) and count contiguous runs ("arcs") of set
  // pixels, not raw set-pixel count. A simple curve point has exactly 2
  // arcs (one incoming, one outgoing) no matter how many pixels wide each
  // arc's corner happens to be; a real fork has 3+ separate arcs. This is
  // the standard "crossing number" grouping used in real skeleton-pruning
  // implementations — confirmed against the actual failing star case
  // above (arc count 2 -> correctly 'regular') and against a synthetic
  // true 3-way fork (3 separate single-pixel arcs -> correctly 'junction').
  var RING_OFFSETS = [[0, -1], [1, -1], [1, 0], [1, 1], [0, 1], [-1, 1], [-1, 0], [-1, -1]]; // N,NE,E,SE,S,SW,W,NW
  function neighborArcs(skel, rw, rh, x, y) {
    function isSet(px, py) { return px >= 0 && py >= 0 && px < rw && py < rh && !!skel[py * rw + px]; }
    var bits = new Array(8), total = 0;
    for (var i = 0; i < 8; i++) {
      bits[i] = isSet(x + RING_OFFSETS[i][0], y + RING_OFFSETS[i][1]);
      if (bits[i]) total++;
    }
    if (total === 0) return { arcs: [], total: 0 };
    // Find a starting index that is NOT set (so a run wrapping around the
    // array boundary still gets grouped as one arc) — if every one of the
    // 8 neighbors is set there is trivially exactly one arc (the whole ring).
    var startIdx = -1;
    for (var s = 0; s < 8; s++) { if (!bits[s]) { startIdx = s; break; } }
    var arcs = [];
    if (startIdx === -1) {
      var whole = [];
      for (var w = 0; w < 8; w++) whole.push([x + RING_OFFSETS[w][0], y + RING_OFFSETS[w][1]]);
      arcs.push(whole);
      return { arcs: arcs, total: total };
    }
    var cur = null;
    for (var k = 1; k <= 8; k++) {
      var idx = (startIdx + k) % 8;
      if (bits[idx]) {
        if (!cur) { cur = []; arcs.push(cur); }
        cur.push([x + RING_OFFSETS[idx][0], y + RING_OFFSETS[idx][1]]);
      } else {
        cur = null;
      }
    }
    return { arcs: arcs, total: total };
  }

  // Walks the thinned pixel skeleton into a graph of nodes (endpoint /
  // junction / synthetic-loop-anchor) and branches (raw pixel chains
  // between two nodes). Isolated single-pixel specks (0 neighbors) are
  // dropped as noise — a real skeleton segment always has at least one
  // neighbor.
  function traceSkeletonGraph(skel, rw, rh) {
    var key = function (x, y) { return y * rw + x; };
    var kindOf = {}; // pixelKey -> 'endpoint'|'junction'|'regular'
    var arcsOf = {}; // pixelKey -> arcs (cached, reused for both classification and walking)
    var pixelList = [];
    for (var y = 0; y < rh; y++) {
      for (var x = 0; x < rw; x++) {
        if (!skel[key(x, y)]) continue;
        var info = neighborArcs(skel, rw, rh, x, y);
        if (info.total === 0) continue; // isolated speck, ignore
        pixelList.push([x, y]);
        arcsOf[key(x, y)] = info.arcs;
        kindOf[key(x, y)] = info.total === 1 ? 'endpoint' : (info.arcs.length >= 3 ? 'junction' : 'regular');
      }
    }
    // One representative pixel per arc — the arc closest (by simple
    // Euclidean distance) to `fromX,fromY` when given (used to keep the
    // walk continuing in a stable direction rather than an arbitrary one);
    // otherwise just the arc's first pixel.
    function arcRepresentative(arc, fromX, fromY) {
      if (fromX === undefined) return arc[0];
      var best = arc[0], bestD = Infinity;
      for (var i = 0; i < arc.length; i++) {
        var d = (arc[i][0] - fromX) * (arc[i][0] - fromX) + (arc[i][1] - fromY) * (arc[i][1] - fromY);
        if (d < bestD) { bestD = d; best = arc[i]; }
      }
      return best;
    }
    // Given the arcs at (x,y) and the pixel we just arrived FROM, return
    // representative next-hop pixel(s) for every OTHER arc (i.e. every
    // direction that isn't where we came from) — exactly 1 for a
    // 'regular' point, 2+ for a 'junction'.
    function otherArcRepresentatives(x, y, prevX, prevY) {
      var arcs = arcsOf[key(x, y)] || [];
      var reps = [];
      for (var i = 0; i < arcs.length; i++) {
        var arc = arcs[i];
        var containsPrev = arc.some(function (p) { return p[0] === prevX && p[1] === prevY; });
        if (containsPrev) continue;
        reps.push(arcRepresentative(arc, x, y));
      }
      return reps;
    }
    var nodes = []; // {id,x,y,kind}
    var nodeIdAt = {}; // pixelKey -> nodeId, for anchor pixels only
    var nextNodeId = 0;
    function ensureNode(x, y, kind) {
      var k = key(x, y);
      if (nodeIdAt[k] !== undefined) return nodeIdAt[k];
      var id = 'n' + (nextNodeId++);
      nodeIdAt[k] = id;
      nodes.push({ id: id, x: x, y: y, kind: kind });
      return id;
    }

    var branches = [];
    var visitedRegular = {}; // pixelKey -> true, once claimed by a branch
    var claimedFirstStep = {}; // "ax,ay->bx,by" -> true, prevents retracing an edge from its other end

    function stepKey(ax, ay, bx, by) { return ax + ',' + ay + '->' + bx + ',' + by; }

    function walkBranch(anchorX, anchorY, firstX, firstY) {
      var chain = [[anchorX, anchorY], [firstX, firstY]];
      var prevX = anchorX, prevY = anchorY, curX = firstX, curY = firstY;
      // Hard safety cap, not a normal exit path: a walk can never
      // legitimately need more steps than there are pixels in the whole
      // image. Guards against any future classification edge case
      // reintroducing an infinite cycle the way the closed-loop path once
      // did (kindOf not updated before the walk started) — better a
      // slightly-wrong truncated branch than a hung tab.
      var guard = rw * rh + 4;
      // eslint-disable-next-line no-constant-condition
      while (guard-- > 0) {
        var k = key(curX, curY);
        var kind = kindOf[k];
        if (kind !== 'regular') break; // hit another anchor (endpoint/junction) — branch complete
        visitedRegular[k] = true;
        var reps = otherArcRepresentatives(curX, curY, prevX, prevY);
        var next = reps.length ? reps[0] : null;
        if (!next) break; // dead end mid-chain (shouldn't happen for a true regular pixel, but stay safe)
        prevX = curX; prevY = curY; curX = next[0]; curY = next[1];
        chain.push([curX, curY]);
      }
      return chain;
    }

    for (var pi = 0; pi < pixelList.length; pi++) {
      var ax = pixelList[pi][0], ay = pixelList[pi][1];
      var akey = key(ax, ay);
      if (kindOf[akey] === 'regular') continue; // only start walks from anchors
      // One branch per ARC, not per raw neighbor pixel — a junction whose
      // fork happens to be several pixels wide in one direction (the same
      // staircase-corner effect as above) must still only spawn ONE
      // branch into that direction, not one per pixel in the corner.
      var aArcs = arcsOf[akey] || [];
      for (var ni = 0; ni < aArcs.length; ni++) {
        var rep = arcRepresentative(aArcs[ni], ax, ay);
        var fx = rep[0], fy = rep[1];
        var fkey = key(fx, fy);
        // Already claimed as part of another branch's interior, or this
        // exact anchor->neighbor step already traced from the other side.
        if (visitedRegular[fkey]) continue;
        if (kindOf[fkey] !== 'regular') {
          // Direct anchor-to-anchor adjacency (no interior pixels) — guard
          // against tracing the same short edge from both ends.
          var sk = stepKey(Math.min(ax, fx), Math.min(ay, fy), Math.max(ax, fx), Math.max(ay, fy));
          if (claimedFirstStep[sk]) continue;
          claimedFirstStep[sk] = true;
          var nodeA0 = ensureNode(ax, ay, kindOf[akey]);
          var nodeB0 = ensureNode(fx, fy, kindOf[fkey]);
          branches.push({ id: 'b' + branches.length, nodeIds: [nodeA0, nodeB0], pixels: [[ax, ay], [fx, fy]] });
          continue;
        }
        var chain = walkBranch(ax, ay, fx, fy);
        var endX = chain[chain.length - 1][0], endY = chain[chain.length - 1][1];
        var nodeA = ensureNode(ax, ay, kindOf[akey]);
        var nodeB = ensureNode(endX, endY, kindOf[key(endX, endY)]);
        branches.push({ id: 'b' + branches.length, nodeIds: [nodeA, nodeB], pixels: chain });
      }
    }

    // Pure closed loops (a ring/"O" shape) have ZERO anchor pixels — every
    // skeleton pixel classifies as 'regular' (exactly 2 neighbors each).
    // Pick one arbitrary pixel per unvisited loop as a synthetic anchor so
    // the loop still produces a usable (closed) branch instead of being
    // silently dropped.
    for (var lp = 0; lp < pixelList.length; lp++) {
      var lx = pixelList[lp][0], ly = pixelList[lp][1];
      var lkey = key(lx, ly);
      if (visitedRegular[lkey] || kindOf[lkey] !== 'regular') continue;
      var loopArcs = arcsOf[lkey] || [];
      if (loopArcs.length !== 2) continue; // shouldn't happen, stay safe
      var loopStart = arcRepresentative(loopArcs[0], lx, ly);
      var nodeLoop = ensureNode(lx, ly, 'endpoint'); // treat as a synthetic anchor
      // walkBranch only stops when it reaches a pixel whose kindOf isn't
      // 'regular' — since this pixel was never a REAL anchor (that's the
      // whole reason it needed a synthetic one), kindOf still says
      // 'regular' for it and the walk would otherwise cycle through the
      // loop forever once it arrives back here. Must be set BEFORE
      // walkBranch runs, not just registered via ensureNode above (which
      // only creates the `nodes[]` entry, it does not touch `kindOf`).
      kindOf[lkey] = 'endpoint';
      var chainA = walkBranch(lx, ly, loopStart[0], loopStart[1]);
      branches.push({ id: 'b' + branches.length, nodeIds: [nodeLoop, nodeLoop], pixels: chainA, closed: true });
    }

    return pruneSpurs(nodes, branches, Math.max(6, Math.round(Math.min(rw, rh) * 0.02)));
  }

  // Zhang-Suen thinning leaves a well-known artifact on diagonal lines:
  // "staircase" pixel patterns where a pixel touches several skeleton
  // neighbors both orthogonally and diagonally at once (confirmed by direct
  // pixel-grid inspection — a 3-neighbor "step corner" pattern, not a real
  // branch point), producing dozens of false junction pixels each spawning
  // a 1-3-pixel-long spur to a false endpoint. This is standard
  // "skeleton pruning"/"spur removal": iteratively strip any endpoint-
  // terminated branch shorter than `minLen` pixels, then collapse any node
  // whose degree drops to exactly 2 back into a single continuous branch
  // (splicing its two remaining branches together) — a real fork/junction
  // has 3+ substantial branches and survives; a staircase artifact's
  // spur does not.
  function pruneSpurs(nodes, branches, minLen) {
    var nodeById = {};
    nodes.forEach(function (n) { nodeById[n.id] = n; });
    var live = branches.slice();

    function degreeMap(list) {
      var deg = {};
      list.forEach(function (b) {
        deg[b.nodeIds[0]] = (deg[b.nodeIds[0]] || 0) + 1;
        if (b.nodeIds[1] !== b.nodeIds[0]) deg[b.nodeIds[1]] = (deg[b.nodeIds[1]] || 0) + 1;
        else deg[b.nodeIds[0]] += 1; // closed loop counts twice at its own anchor
      });
      return deg;
    }

    var changed = true;
    while (changed) {
      changed = false;
      var deg = degreeMap(live);
      for (var i = 0; i < live.length; i++) {
        var b = live[i];
        if (b.closed) continue; // never prune a whole standalone loop
        var a0 = b.nodeIds[0], a1 = b.nodeIds[1];
        if (a0 === a1) continue;
        var d0 = deg[a0] || 0, d1 = deg[a1] || 0;
        var endIsSpur = (d0 === 1 && d1 > 1) || (d1 === 1 && d0 > 1);
        if (!endIsSpur) continue;
        if (b.pixels.length > minLen) continue;
        live.splice(i, 1);
        changed = true;
        break;
      }
    }

    // Collapse degree-2 nodes: a former junction that lost enough spurs to
    // leave only two real branches is no longer a fork — splice its two
    // branches into one continuous branch and drop the node.
    changed = true;
    while (changed) {
      changed = false;
      var deg2 = degreeMap(live);
      for (var nid in deg2) {
        if (deg2[nid] !== 2) continue;
        var touching = [];
        for (var bi = 0; bi < live.length; bi++) {
          if (live[bi].closed) continue;
          if (live[bi].nodeIds[0] === nid || live[bi].nodeIds[1] === nid) touching.push(bi);
        }
        if (touching.length !== 2) continue; // shouldn't happen, stay safe
        var bA = live[touching[0]], bB = live[touching[1]];
        var otherA = bA.nodeIds[0] === nid ? bA.nodeIds[1] : bA.nodeIds[0];
        var otherB = bB.nodeIds[0] === nid ? bB.nodeIds[1] : bB.nodeIds[0];
        // Orient both pixel chains so nid's pixel is at the END of A and
        // the START of B, then concatenate (dropping B's duplicate first
        // pixel) into a single continuous chain from otherA to otherB.
        var pixA = bA.nodeIds[0] === nid ? bA.pixels.slice().reverse() : bA.pixels.slice();
        var pixB = bB.nodeIds[0] === nid ? bB.pixels.slice() : bB.pixels.slice().reverse();
        var merged = pixA.concat(pixB.slice(1));
        var newBranch = { id: bA.id, nodeIds: [otherA, otherB], pixels: merged, closed: false };
        var keep = live.filter(function (b, idx) { return idx !== touching[0] && idx !== touching[1]; });
        keep.push(newBranch);
        live = keep;
        changed = true;
        break;
      }
    }

    var finalDeg = degreeMap(live);
    var usedIds = {};
    live.forEach(function (b) { usedIds[b.nodeIds[0]] = true; usedIds[b.nodeIds[1]] = true; });
    var finalNodes = nodes.filter(function (n) { return usedIds[n.id]; });
    finalNodes.forEach(function (n) {
      var d = finalDeg[n.id] || 0;
      n.kind = d <= 1 ? 'endpoint' : (d === 2 ? 'regular-anchor' : 'junction');
    });
    return { nodes: finalNodes, branches: live };
  }

  // ---- Curve-fit each branch's raw pixel chain (Schneider via Paper.js's
  // own Path.simplify — already vendored, no new math) --------------------
  function fitBranchCurve(pixels, bounds, scale, tolerance) {
    var raw = new Path({ insert: false });
    for (var i = 0; i < pixels.length; i++) {
      var doc = new Point(bounds.x + pixels[i][0] / scale, bounds.y + pixels[i][1] / scale);
      if (i === 0) raw.moveTo(doc); else raw.lineTo(doc);
    }
    if (raw.segments.length > 2) raw.simplify(tolerance);
    var segs = raw.segments.map(function (s) {
      return {
        point: [s.point.x, s.point.y],
        handleIn: [s.handleIn.x, s.handleIn.y],
        handleOut: [s.handleOut.x, s.handleOut.y],
      };
    });
    raw.remove();
    return segs;
  }

  // ---- Public entry point ------------------------------------------------
  // Returns a plain-JSON skeleton graph:
  //   { nodes: [{id,x,y,kind}], branches: [{id,nodeIds:[a,b],segments:[...]}] }
  // `nodes[].x/y` are in DOCUMENT coordinates (already un-scaled from the
  // raster back through `bounds`/`scale`) so callers never need to touch
  // the rasterization internals.
  // pathOrPaths: a single Path/CompoundPath, or an array of them (a
  // multi-shape selection) — see rasterizeShape's comment above.
  function extractSkeleton(pathOrPaths, opts) {
    opts = opts || {};
    var paths = Array.isArray(pathOrPaths) ? pathOrPaths : [pathOrPaths];
    if (!paths.length) return null;
    var combined = null;
    for (var pi = 0; pi < paths.length; pi++) {
      if (!paths[pi] || !paths[pi].bounds) return null;
      combined = combined ? combined.unite(paths[pi].bounds) : paths[pi].bounds.clone();
    }
    if (combined.width < 2 || combined.height < 2) return null;
    var maxDim = opts.maxDim || 512;
    var tolerance = opts.tolerance !== undefined ? opts.tolerance : 2;
    var raster = rasterizeShape(paths, maxDim, opts.closeRadius);
    var thinned = zhangSuenThin(raster.mask, raster.rw, raster.rh);
    var graph = traceSkeletonGraph(thinned, raster.rw, raster.rh);
    if (!graph.nodes.length) return null;
    // Convert node pixel coords -> document coords now (branches still
    // reference nodes by id, only the node's own x/y needs the transform).
    graph.nodes.forEach(function (n) {
      n.x = raster.bounds.x + n.x / raster.scale;
      n.y = raster.bounds.y + n.y / raster.scale;
    });
    var branches = graph.branches.map(function (b) {
      return {
        id: b.id,
        nodeIds: b.nodeIds,
        closed: !!b.closed,
        segments: fitBranchCurve(b.pixels, raster.bounds, raster.scale, tolerance),
      };
    });
    return { nodes: graph.nodes, branches: branches };
  }

  window.SMSkeleton = {
    extractSkeleton: extractSkeleton,
    // Exposed for M1's own debug verification (draw branches as an
    // overlay) and for unit-style manual testing from the console — not
    // part of the public API surface other tools should call directly.
    _rasterizeShape: rasterizeShape,
    _zhangSuenThin: zhangSuenThin,
    _traceSkeletonGraph: traceSkeletonGraph,
  };
})();
