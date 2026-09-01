// ---- Image vectorization (2026-09) ----
// "un system puissant de vectorisation ça sera dans effet même si pas
// wgsl" — traces a raster image into real, editable vector paths
// (Illustrator "Image Trace" equivalent), discoverable from the Effects
// "Add Effect" menu even though it isn't a per-frame WGSL effect: it's a
// one-shot BAKE that materializes NEW Path/CompoundPath content into a
// fresh layer, not an entry in ld.effects[] (that array is pixel-space,
// re-run every frame by engine.rs — vectorization produces document
// GEOMETRY once, the exact same "materialize, don't mutate the source"
// shape as the mograph Duplicator and Combine/Flatten).
//
// Runs in Rust (`vtracer` crate, MIT OR Apache-2.0) compiled to
// wasm32-unknown-unknown (vectorize-wasm/, loaded lazily via
// vectorize-wasm-loader.js — only the first time this dialog opens, not
// on every app boot) — works identically in the desktop app's Tauri
// webview AND a plain browser tab AND the web public beta, one code path
// everywhere (2026-09 revision: originally shipped as a Tauri-only native
// command, changed after Cyril pointed out a plain wasm build works fine
// too and the desktop-only gate made it untestable/unusable from a
// browser). The vtracer pipeline itself is shared with the still-present
// native Tauri command (src-tauri/src/vectorize.rs) via ../vectorize-core
// — see that crate's own doc comment for why neither reimplements it.
// This file only builds Paper.js geometry from the returned JSON and
// never touches an SVG string (see vectorize-core's own comment).
(function () {
  function activeLayer() { return state.layers[state.activeLayerIdx]; }

  // Finds the raster to trace: the first isRaster stroke in the active
  // layer's CURRENT frame — matches what the user is actually looking at,
  // independent of the effects-target/adjustment-layer machinery
  // (effects-panel.js's effectsTarget()), which is about pixel-effect
  // isolation and has nothing to do with picking a source image.
  function findActiveRasterStroke() {
    // getEffectiveStrokes (app.js) is the canonical accessor — it already
    // resolves a held (non-keyframe) frame to its inherited keyframe's
    // stored array, so this works whether or not the current frame is
    // itself a keyframe (CLAUDE.md §5quater).
    var strokes = getEffectiveStrokes(state.activeLayerIdx, state.currentFrame);
    if (!strokes) return null;
    for (var i = 0; i < strokes.length; i++) {
      if (strokes[i] && strokes[i].isRaster && strokes[i].src) return strokes[i];
    }
    return null;
  }

  // vtracer's own three built-in presets (cmdapp/src/config.rs, verified
  // against the exact published source of the pinned 0.6.5) — same
  // Bw/Poster/Photo vocabulary VTracer's own CLI/webapp uses, EXCEPT
  // hierarchical, deliberately overridden to 'cutout' here (VTracer's own
  // presets all default to 'stacked').
  //
  // Found live (2026-09, tracing a plain circle): in 'stacked' mode, each
  // shape's geometry is only correct WHEN COMPOSITED IN PAINT ORDER — the
  // "blue circle" shape traced out as the FULL CANVAS RECTANGLE (color
  // blue, geometry = the whole square), with the actual tight circle
  // instead living as a HOLE cut into the white background shape drawn on
  // top. The final on-screen composite still looked correct (which is why
  // Phase 0's own visual verification didn't catch this), but each
  // individual shape is meaningless on its own — exactly the opposite of
  // what Nemo needs, since every traced shape becomes its own independent,
  // separately-selectable document object, and Phase 1's paramShape fit
  // needs a shape's OWN geometry to match what it visually represents.
  // 'cutout' runs vtracer's second reclustering pass (config.rs's own
  // Hierarchical::Cutout branch) specifically so every shape is a real,
  // self-contained region — confirmed on the same circle: the blue shape's
  // own contour is the tight circle, not the canvas rect.
  var PRESETS = {
    bw: { colorMode: 'binary', hierarchical: 'cutout', filterSpeckle: 4, colorPrecision: 6, layerDifference: 16, cornerThreshold: 60 },
    poster: { colorMode: 'color', hierarchical: 'cutout', filterSpeckle: 4, colorPrecision: 8, layerDifference: 16, cornerThreshold: 60 },
    photo: { colorMode: 'color', hierarchical: 'cutout', filterSpeckle: 10, colorPrecision: 8, layerDifference: 48, cornerThreshold: 180 },
  };

  // Converts a vtracer "1+3n" absolute cubic-bezier control-point chain
  // (point 0 = contour start, then repeating groups of 3 = control1,
  // control2, next anchor) into Paper.js Segments — handles are RELATIVE
  // offsets from their own point in Paper.js, unlike vtracer's absolute
  // coordinates, so each handle is the control point minus its anchor.
  function splineToSegments(flatPts, mapPt) {
    var n = (flatPts.length / 2) | 0;
    if (n < 1) return [];
    var P = [];
    for (var i = 0; i < n; i++) P.push(mapPt(flatPts[i * 2], flatPts[i * 2 + 1]));
    var segs = [new Segment(P[0])];
    var i2 = 1;
    while (i2 + 2 < P.length) {
      var c1 = P[i2], c2 = P[i2 + 1], anchor = P[i2 + 2];
      var prevSeg = segs[segs.length - 1];
      prevSeg.handleOut = c1.subtract(prevSeg.point);
      var seg = new Segment(anchor);
      seg.handleIn = c2.subtract(anchor);
      segs.push(seg);
      i2 += 3;
    }
    return segs;
  }

  // Polygon contours (vtracer's non-spline output, kept for completeness
  // even though this bridge always requests PathSimplifyMode::Spline on
  // the Rust side) — plain vertices, straight edges, no handles.
  function polygonToSegments(flatPts, mapPt) {
    var segs = [];
    for (var i = 0; i < flatPts.length; i += 2) segs.push(new Segment(mapPt(flatPts[i], flatPts[i + 1])));
    return segs;
  }

  function contourToSegments(contour, mapPt) {
    return contour.kind === 'spline' ? splineToSegments(contour.points, mapPt) : polygonToSegments(contour.points, mapPt);
  }

  // ---- Phase 1: parametric shape recognition (2026-09) ----
  // "des forme qui peuvent être paramétrique" — when a traced contour is
  // well-approximated by a primitive Nemo already knows how to edit
  // natively (rounded rect, ellipse — see tools.js's buildRoundRectPath/
  // buildArcEllipsePath/stampParamShapeBox, the same machinery the
  // Rectangle/Ellipse tools themselves use), emit a REAL editable
  // paramShape instead of a frozen bezier blob: corner-radius handles,
  // the Motion cornerTL..BL/arcStart/arcSweep properties, the Coins panel
  // — all of it comes for free, because the shape IS indistinguishable
  // from one drawn by hand with those tools.
  //
  // Deliberately axis-aligned only in v1 — Nemo's own ellipse paramShape
  // has no rotation concept at all (applyParamShapeEllipse never calls
  // reapplyParamShapeAngle, unlike rect/star), and detecting a ROTATED
  // rect's angle from a traced contour is extra work with its own
  // failure modes; a rotated shape just falls through to the plain-path
  // path below, unchanged from Phase 0. Star/polygon fitting (needs
  // rotational-symmetry detection, a harder problem) is not attempted
  // here either — circle/ellipse and rect/rounded-rect cover the large
  // majority of real flat graphics (logos, icons, UI chrome) on their
  // own.
  //
  // Fit against the contour's PAPER anchor points (segment.point, handles
  // ignored) — a reasonable point-cloud sample of the boundary, cheap,
  // and already in the exact world-space paramShape's own box will use.
  var ELLIPSE_FIT_RMSE_MAX = 0.05; // normalized (x/rx)²+(y/ry)² residual
  var RECT_AREA_RATIO_MIN = 0.90;  // traced-area / bbox-area

  function fitEllipse(pts, bounds) {
    var cx = bounds.center.x, cy = bounds.center.y, rx = bounds.width / 2, ry = bounds.height / 2;
    if (rx < 2 || ry < 2 || pts.length < 5) return null;
    var sumSq = 0;
    pts.forEach(function (p) {
      var u = (p.x - cx) / rx, v = (p.y - cy) / ry;
      var e = u * u + v * v - 1;
      sumSq += e * e;
    });
    var rmse = Math.sqrt(sumSq / pts.length);
    return rmse <= ELLIPSE_FIT_RMSE_MAX ? { cx: cx, cy: cy, rx: rx, ry: ry } : null;
  }

  function shoelaceArea(pts) {
    var a = 0;
    for (var i = 0; i < pts.length; i++) {
      var p1 = pts[i], p2 = pts[(i + 1) % pts.length];
      a += p1.x * p2.y - p2.x * p1.y;
    }
    return Math.abs(a) / 2;
  }

  function fitRoundedRect(pts, bounds) {
    var w = bounds.width, h = bounds.height;
    if (w < 4 || h < 4 || pts.length < 4) return null;
    if (shoelaceArea(pts) / (w * h) < RECT_AREA_RATIO_MIN) return null;
    var x1 = bounds.left, y1 = bounds.top, x2 = bounds.right, y2 = bounds.bottom;
    var corners = { tl: [x1, y1], tr: [x2, y1], br: [x2, y2], bl: [x1, y2] };
    var maxRadius = Math.min(w, h) / 2;
    var radii = {}, ok = true;
    // The traced point closest to an exact sharp corner sits on the
    // rounding arc itself (or right at the corner if radius≈0) — for the
    // standard quarter-circle corner buildRoundRectPath draws, that
    // closest point is exactly radius·(√2−1) away from the sharp corner
    // (distance corner→arc-center is radius·√2, minus the radius itself).
    // An exact closed-form recovery of the radius, not a guess.
    Object.keys(corners).forEach(function (k) {
      var cx = corners[k][0], cy = corners[k][1], dMin = Infinity;
      pts.forEach(function (p) {
        var d = Math.hypot(p.x - cx, p.y - cy);
        if (d < dMin) dMin = d;
      });
      var r = dMin / (Math.SQRT2 - 1);
      if (r > maxRadius * 1.15) ok = false;
      radii[k] = Math.max(0, Math.min(maxRadius, r));
    });
    if (!ok) return null;
    return { x1: x1, y1: y1, x2: x2, y2: y2, tl: radii.tl, tr: radii.tr, br: radii.br, bl: radii.bl };
  }

  // ---- Phase 2: occlusion completion (2026-09) ----
  // "quand les élément sont les uns devant les autres [garder] en vrai
  // forme non cassé par la forme de devant. Deviner la forme complète."
  // (Cyril). The plan agreed on: never guess blind — derive. A shape
  // whose visible fragment fits a primitive is completed by REFITTING
  // that primitive on just the non-occluded points, never by hallucinating
  // the hidden part.
  //
  // Scoped to CIRCLES only in v1 (not general ellipses or rects): a
  // circle's bounding box is unusable here — the VISIBLE fragment's bbox
  // is smaller/offset from the true full circle's, so fitEllipse's
  // "derive center/radii from bounds" approach (fine for a COMPLETE shape,
  // Phase 1) is wrong on a cut one. A circle alone has a closed-form
  // algebraic least-squares fit that needs no bounding box at all — the
  // Kåsa method: minimize Σ(xi²+yi²+D·xi+E·yi+F)² by solving the 3×3
  // normal-equations system directly (this is genuinely a DERIVATION from
  // the visible points, not a guess). Ellipse/rect completion is real
  // future work, not attempted here — flagged in the roadmap, not silently
  // dropped.
  function fitCircleKasa(pts) {
    if (pts.length < 8) return null; // too few visible points to trust a fit
    // Normal equations for Σ(x²+y²) = -D·Σx - E·Σy - F·n (and the two
    // moment equations) — solved by Cramer's rule on the 3×3 system
    // [Σx² Σxy Σx; Σxy Σy² Σy; Σx Σy n] · [D;E;F] = [-Σx(x²+y²); -Σy(x²+y²); -Σ(x²+y²)]
    var n = pts.length, Sx = 0, Sy = 0, Sxx = 0, Syy = 0, Sxy = 0, Sxz = 0, Syz = 0, Sz = 0;
    pts.forEach(function (p) {
      var z = p.x * p.x + p.y * p.y;
      Sx += p.x; Sy += p.y; Sxx += p.x * p.x; Syy += p.y * p.y; Sxy += p.x * p.y;
      Sxz += p.x * z; Syz += p.y * z; Sz += z;
    });
    var a11 = Sxx, a12 = Sxy, a13 = Sx, b1 = -Sxz;
    var a21 = Sxy, a22 = Syy, a23 = Sy, b2 = -Syz;
    var a31 = Sx, a32 = Sy, a33 = n, b3 = -Sz;
    var det = a11 * (a22 * a33 - a23 * a32) - a12 * (a21 * a33 - a23 * a31) + a13 * (a21 * a32 - a22 * a31);
    if (Math.abs(det) < 1e-9) return null; // degenerate (collinear points)
    function detWithCol(col) {
      var m = [[a11, a12, a13], [a21, a22, a23], [a31, a32, a33]];
      for (var r = 0; r < 3; r++) m[r][col] = [b1, b2, b3][r];
      return m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1]) - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0]) + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
    }
    var D = detWithCol(0) / det, E = detWithCol(1) / det, F = detWithCol(2) / det;
    var cx = -D / 2, cy = -E / 2, r2 = cx * cx + cy * cy - F;
    if (r2 <= 0) return null;
    var r = Math.sqrt(r2);
    if (r < 2) return null;
    var sumSq = 0;
    pts.forEach(function (p) { var d = Math.hypot(p.x - cx, p.y - cy) - r; sumSq += d * d; });
    var rmse = Math.sqrt(sumSq / n);
    // Relative to the radius — an absolute pixel tolerance would be too
    // strict for a large circle and too loose for a tiny one.
    return rmse / r <= 0.04 ? { cx: cx, cy: cy, r: r } : null;
  }

  // For a shape that did NOT fit as a complete primitive (Phase 1), looks
  // for a NEIGHBORING shape (any other traced shape, regardless of paint
  // order — see below) and strips the points that sit on the SEAM shared
  // with it, then retries the fit on what's left.
  //
  // Found live testing a red circle occluded by a blue rectangle: this
  // does NOT filter by "is the point inside the neighbor's fill". In
  // vtracer's 'cutout' mode (the default since Phase 1's own fix), traced
  // regions are already non-overlapping — a occluded shape's own contour
  // never actually dips INSIDE the occluder's fill, it's already the cut
  // fragment. What needs excluding is the STRAIGHT SEAM edge where the
  // fragment was clipped, which is not "inside" anything — it's ON the
  // neighbor's own boundary. hitTest(point, {stroke:true}) against the
  // neighbor's outline catches exactly that, cleanly, without needing to
  // classify curved-vs-straight contour segments by hand. Paint order is
  // therefore irrelevant here too (unlike a hypothetical 'stacked'-mode
  // version of this) — every OTHER shape is a valid candidate neighbor.
  function tryOccludedCircleFit(pts, otherItems, fill) {
    // Tolerance scaled to the point cloud's own size (not a fixed pixel
    // count) — stays correct whether the traced image is 50px or 5000px.
    var minX = Infinity, maxX = -Infinity, minY = Infinity, maxY = -Infinity;
    pts.forEach(function (p) { if (p.x < minX) minX = p.x; if (p.x > maxX) maxX = p.x; if (p.y < minY) minY = p.y; if (p.y > maxY) maxY = p.y; });
    var tol = Math.max(1, Math.hypot(maxX - minX, maxY - minY) * 0.02);
    for (var i = 0; i < otherItems.length; i++) {
      var neighbor = otherItems[i];
      if (!neighbor || !neighbor.bounds) continue;
      var remaining = pts.filter(function (p) { return !neighbor.hitTest(p, { stroke: true, fill: false, tolerance: tol }); });
      // Found live: do NOT also require "some point got filtered" here —
      // a real occluded circle's own anchor points (including the couple
      // sitting right on the cut seam) can already sit close enough to
      // the true circle for the algebraic fit to succeed on the FULL,
      // unfiltered point set (a chord across a short cut doesn't deviate
      // far from the arc it replaces). Demanding a seam be found first
      // rejected an already-correct fit outright. fitCircleKasa's own
      // RMSE/r gate is what actually guards result quality — filtering
      // only helps it MORE, never gates whether to accept.
      if (remaining.length < pts.length * 0.35) continue; // too little left to trust
      var fit = fitCircleKasa(remaining);
      if (!fit) continue;
      var built = window.buildArcEllipsePath(fit.cx, fit.cy, fit.r, fit.r, 0, 359.9, 0);
      built.data.paramShape = { kind: 'ellipse', startAngle: 0, sweep: 359.9, innerRadius: 0 };
      if (window.stampParamShapeBox) window.stampParamShapeBox(built);
      built.fillColor = fill; built.strokeColor = null;
      return built;
    }
    return null;
  }

  // Tries ellipse first (a circle/donut/pie IS an ellipse fit in Nemo's
  // model — no separate "circle" kind exists), then rounded-rect. Returns
  // a real, already-inserted-ready paper.Path with data.paramShape
  // stamped, or null if neither fit well enough — the caller falls back
  // to the plain traced path in that case.
  function tryParamShapeFit(anchorPts, bounds, fill) {
    var e = fitEllipse(anchorPts, bounds);
    if (e && window.buildArcEllipsePath) {
      var ep = window.buildArcEllipsePath(e.cx, e.cy, e.rx, e.ry, 0, 359.9, 0);
      ep.data.paramShape = { kind: 'ellipse', startAngle: 0, sweep: 359.9, innerRadius: 0 };
      if (window.stampParamShapeBox) window.stampParamShapeBox(ep);
      ep.fillColor = fill; ep.strokeColor = null;
      return ep;
    }
    var r = fitRoundedRect(anchorPts, bounds);
    if (r && window.buildRoundRectPath) {
      var rp = window.buildRoundRectPath(r.x1, r.y1, r.x2, r.y2, r.tl, r.tr, r.br, r.bl);
      rp.data.paramShape = { kind: 'rect', tl: r.tl, tr: r.tr, br: r.br, bl: r.bl };
      if (window.stampParamShapeBox) window.stampParamShapeBox(rp);
      rp.fillColor = fill; rp.strokeColor = null;
      return rp;
    }
    return null;
  }

  // Builds ONE Paper item per traced shape — a plain Path for a single
  // contour, a CompoundPath (CLAUDE.md §1's own convention for anything
  // with holes) when vtracer emitted more than one, e.g. a donut. Fill
  // only (VTracer's Color has no alpha — transparency was already
  // consumed upstream to decide which regions to keep, see vectorize.rs).
  //
  // Returns {item, fixed, rawPts, fill} — `fixed:true` means item is
  // final (either a clean Phase 1 paramShape fit, or a multi-contour
  // CompoundPath, which occlusion completion below never attempts).
  // `fixed:false` means item is the raw, uncompleted single-contour path
  // — kept around (with its own point cloud) so the occlusion-completion
  // pass in runVectorize can still rescue it against a shape drawn later
  // in paint order.
  function buildShapeItem(shape, mapPt) {
    var fill = new Color(shape.r / 255, shape.g / 255, shape.b / 255, 1);
    // insert:false on every Path — Paper.js auto-inserts a plain
    // `new Path(segments)` into the CURRENTLY ACTIVE layer the instant
    // it's constructed. Left on, that pre-inserts each contour as its own
    // top-level layer child; wrapping siblings into a CompoundPath
    // afterward relies on the object-literal `{children:[...]}` form
    // actually reparenting already-inserted items, which it does NOT do
    // reliably (confirmed live: a 2-contour donut test case landed as two
    // separate solid Paths, no hole, no cut punched through). Building
    // detached, then reparenting explicitly via addChildren (a real,
    // documented Item method) makes this correct regardless of that
    // constructor-option ambiguity — the caller does the ONE actual
    // layer insertion once this function returns.
    var childPaths = shape.contours.map(function (c) {
      var p = new Path({ segments: contourToSegments(c, mapPt), closed: true, insert: false });
      return p;
    }).filter(function (p) { return p.segments.length >= 2; });
    if (!childPaths.length) return null;
    if (childPaths.length === 1) {
      // Try recognizing a parametric primitive BEFORE settling for the
      // raw traced path — single-contour shapes only (a donut/shape-with-
      // a-hole is a CompoundPath below; ellipse's own innerRadius COULD
      // represent a donut too, but that's a real fit case for later, not
      // attempted here).
      var pts = childPaths[0].segments.map(function (s) { return s.point; });
      var fitted = tryParamShapeFit(pts, childPaths[0].bounds, fill);
      if (fitted) { childPaths[0].remove(); return { item: fitted, fixed: true }; }
      childPaths[0].fillColor = fill; childPaths[0].strokeColor = null;
      return { item: childPaths[0], fixed: false, rawPts: pts, fill: fill };
    }
    // A real, multi-contour CompoundPath — correct here, and left alone.
    // Nemo's own save pipeline (_flattenCompoundChildren, app.js) already
    // walks every layer for exactly this case before every
    // saveActiveLayerFrame() — _collectLayerStrokes only knows how to
    // persist `instanceof Path`/Raster, so it keyhole-merges any
    // CompoundPath into the same island/hole representation every other
    // consumer expects (CLAUDE.md §1). Don't pre-flatten here too — that
    // safety net is the established, single place this already happens.
    var item = new CompoundPath({ insert: false });
    item.addChildren(childPaths);
    item.fillColor = fill;
    item.strokeColor = null;
    return { item: item, fixed: true };
  }

  // Browser-native base64 -> Uint8Array (atob + charCodeAt) — the wasm
  // binding takes raw bytes, not a base64 string; no library needed for
  // this one conversion.
  function base64ToBytes(b64) {
    var bin = atob(b64);
    var bytes = new Uint8Array(bin.length);
    for (var i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
    return bytes;
  }

  async function runVectorize(cfg) {
    var raster = findActiveRasterStroke();
    if (!raster) {
      showToast(SM && SM.t ? SM.t('vectorizeNoImage') : 'Select a layer with an image first.');
      return;
    }
    var m = /^data:image\/(\w+);base64,(.+)$/.exec(raster.src);
    if (!m) {
      showToast(SM && SM.t ? SM.t('vectorizeNoImage') : 'Select a layer with an image first.');
      return;
    }
    var result;
    try {
      var configJson = JSON.stringify({
        colorMode: cfg.colorMode,
        hierarchical: cfg.hierarchical,
        filterSpeckle: cfg.filterSpeckle,
        colorPrecision: cfg.colorPrecision,
        layerDifference: cfg.layerDifference,
        cornerThreshold: cfg.cornerThreshold,
      });
      // Runs inside vectorize-worker.js (a real Web Worker, not just an
      // async-wrapped call) — color clustering + spline fitting on a real
      // photo can take real seconds, and a synchronous wasm call blocks
      // whichever thread runs it for its whole duration no matter how the
      // JS around it is structured; only a worker keeps the editor
      // responsive and the progress bar below actually animating while it
      // runs (2026-09, "que le rastérize se fasse en arrière-plan").
      var resultJson = await window.VectorizeWasm.vectorize(base64ToBytes(m[2]), configJson);
      result = JSON.parse(resultJson);
    } catch (e) {
      showToast((SM && SM.t ? SM.t('vectorizeFailed') : 'Vectorization failed: ') + (e && e.message ? e.message : e));
      return;
    }
    if (!result || !result.shapes || !result.shapes.length) {
      showToast(SM && SM.t ? SM.t('vectorizeEmpty') : 'No shapes traced — try lowering the speckle filter.');
      return;
    }
    // Maps a pixel coordinate in the SOURCE image's native decoded size
    // (result.width/height — the raw PNG/JPEG dimensions, NOT the raster
    // item's on-canvas size) onto the raster's own world placement, so
    // the traced layer lands exactly on top of the original regardless of
    // how the image has been scaled/positioned in the document.
    // r.position is the raster's CENTER (Paper.js convention, confirmed
    // against desR/app.js), so the top-left corner is center - size/2.
    var left = raster.x - raster.width / 2, top = raster.y - raster.height / 2;
    var sx = raster.width / result.width, sy = raster.height / result.height;
    function mapPt(px, py) { return new Point(left + px * sx, top + py * sy); }
    // Captured BEFORE addLayer() switches state.activeLayerIdx to the new
    // (still-unnamed) layer — reading it after would name the result after
    // itself instead of its source.
    var sourceName = (activeLayer() && activeLayer().name) || 'Image';

    pushUndo();
    window.SM.addLayer();
    var li = state.activeLayerIdx;
    var newLayer = userLayers[li];
    state.layers[li].name = sourceName + ' traced';

    // Pass 1: build every shape's geometry (Phase 1's own paramShape fit
    // already tried inline) — nothing inserted into the layer yet, so
    // Pass 2 below can still swap a raw fragment out for a completed
    // primitive before anything becomes permanent document content.
    var built = result.shapes.map(function (shape) { return buildShapeItem(shape, mapPt); }).filter(Boolean);

    // Pass 2 (occlusion completion): for every shape Phase 1 couldn't fit
    // as a complete primitive, check every OTHER traced shape as a
    // possible neighbor sharing a cut seam (see tryOccludedCircleFit's own
    // comment for why paint order doesn't matter in 'cutout' mode — array
    // order here is just "not itself", nothing more).
    for (var bi = 0; bi < built.length; bi++) {
      if (built[bi].fixed) continue;
      var others = built.filter(function (b, idx) { return idx !== bi; }).map(function (b) { return b.item; });
      var completed = tryOccludedCircleFit(built[bi].rawPts, others, built[bi].fill);
      if (completed) { built[bi].item.remove(); built[bi].item = completed; }
    }

    built.forEach(function (b) { newLayer.addChild(b.item); });
    saveActiveLayerFrame();
    renderArcs(); updateUI();
    if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
    showToast((SM && SM.t ? SM.t('vectorizeDone') : 'Vectorized: ') + built.length + (SM && SM.t ? '' : ' shapes'));
  }


  // ---- Modal (mirrors custom-effects.js's own modal-overlay pattern) ----
  var modalEl = null;
  var _preset = 'poster';
  function ensureModal() {
    if (modalEl) return modalEl;
    modalEl = document.createElement('div');
    modalEl.className = 'modal-overlay';
    modalEl.style.display = 'none';
    modalEl.innerHTML =
      '<div class="modal-box" style="max-width:360px">' +
      '<div class="modal-hdr"><span data-i18n="vectorizeTitle">Vectorize Image</span><button class="modal-x" id="vt-close">&times;</button></div>' +
      '<div class="modal-bdy" style="display:flex;flex-direction:column;gap:10px">' +
      '<div style="font-size:11px;color:var(--text-dim)" data-i18n="vectorizeIntro">Traces the current layer\'s image into flat-color vector shapes on a new layer.</div>' +
      '<div class="pr" style="gap:6px" id="vt-presets">' +
      '<button class="pbtn" data-preset="bw" data-i18n="vectorizePresetBw">B&amp;W</button>' +
      '<button class="pbtn" data-preset="poster" data-i18n="vectorizePresetPoster">Poster</button>' +
      '<button class="pbtn" data-preset="photo" data-i18n="vectorizePresetPhoto">Photo</button>' +
      '</div>' +
      '<label style="font-size:11px;color:var(--text-dim)"><span data-i18n="vectorizeFilterSpeckle">Filter speckle</span><br><input type="number" id="vt-speckle" class="pi scrub" data-step="1" style="width:100%"></label>' +
      '<label style="font-size:11px;color:var(--text-dim)"><span data-i18n="vectorizeColorPrecision">Color precision</span><br><input type="number" id="vt-precision" class="pi scrub" data-step="1" style="width:100%"></label>' +
      '<label style="font-size:11px;color:var(--text-dim)"><span data-i18n="vectorizeCornerThreshold">Corner threshold</span><br><input type="number" id="vt-corner" class="pi scrub" data-step="1" style="width:100%"></label>' +
      '<div id="vt-error" style="display:none;font-size:10px;color:#ff8080"></div>' +
      // Same visual language as the existing background-optimize progress
      // row (media-library.js) — an INDETERMINATE bar, not a percentage:
      // vtracer's own API doesn't report incremental progress, so a real
      // percentage isn't available; a moving bar honestly says "still
      // working", not "X% done". Runs in vectorize-worker.js (a real Web
      // Worker — see runVectorize's own comment for why), so the editor
      // stays responsive and this bar keeps animating smoothly the whole
      // time, and the dialog can be dismissed (Cancel/×) without
      // cancelling the job — it keeps tracing in the background and the
      // result still lands (toast + new layer) whenever it finishes.
      '<div id="vt-progress" style="display:none">' +
      '<div style="font-size:11px;color:var(--text-dim);margin-bottom:4px" data-i18n="vectorizeWorking">Vectorizing…</div>' +
      '<div class="media-row-progress"><div class="media-row-progress-bar"></div></div>' +
      '</div>' +
      '<div class="pr" style="gap:6px;justify-content:flex-end">' +
      '<button class="pbtn" id="vt-cancel" data-i18n="btnCancel">Cancel</button>' +
      '<button class="pbtn ac" id="vt-run" data-i18n="vectorizeRun">Vectorize</button>' +
      '</div>' +
      '</div></div>';
    document.body.appendChild(modalEl);
    if (window.SM && window.SM.applyI18n) window.SM.applyI18n(modalEl);
    modalEl.querySelector('#vt-close').addEventListener('click', close);
    modalEl.querySelector('#vt-cancel').addEventListener('click', close);
    modalEl.addEventListener('click', function (e) { if (e.target === modalEl) close(); });
    modalEl.querySelectorAll('#vt-presets [data-preset]').forEach(function (btn) {
      btn.addEventListener('click', function () { applyPreset(btn.getAttribute('data-preset')); });
    });
    modalEl.querySelector('#vt-run').addEventListener('click', function () {
      var btn = modalEl.querySelector('#vt-run');
      var progress = modalEl.querySelector('#vt-progress');
      var presets = modalEl.querySelector('#vt-presets');
      btn.disabled = true;
      progress.style.display = '';
      presets.style.display = 'none';
      ['#vt-speckle', '#vt-precision', '#vt-corner'].forEach(function (sel) {
        modalEl.querySelector(sel).closest('label').style.display = 'none';
      });
      var cfg = readForm();
      // Deliberately NOT disabling #vt-close/#vt-cancel — the whole point
      // of running this in a Worker (see runVectorize's own comment) is
      // that the job survives the dialog closing: dismiss it and keep
      // working, the result (toast + new layer) still lands whenever
      // tracing finishes, exactly like any other background task.
      runVectorize(cfg).finally(function () {
        btn.disabled = false;
        progress.style.display = 'none';
        presets.style.display = '';
        ['#vt-speckle', '#vt-precision', '#vt-corner'].forEach(function (sel) {
          modalEl.querySelector(sel).closest('label').style.display = '';
        });
        close();
      });
    });
    return modalEl;
  }
  function close() { if (modalEl) modalEl.style.display = 'none'; }
  function applyPreset(name) {
    _preset = name;
    var p = PRESETS[name] || PRESETS.poster;
    modalEl.querySelector('#vt-speckle').value = p.filterSpeckle;
    modalEl.querySelector('#vt-precision').value = p.colorPrecision;
    modalEl.querySelector('#vt-corner').value = p.cornerThreshold;
    modalEl.querySelectorAll('#vt-presets [data-preset]').forEach(function (b) {
      b.classList.toggle('ac', b.getAttribute('data-preset') === name);
    });
  }
  function readForm() {
    var p = PRESETS[_preset] || PRESETS.poster;
    return {
      colorMode: p.colorMode,
      hierarchical: p.hierarchical,
      filterSpeckle: parseInt(modalEl.querySelector('#vt-speckle').value, 10) || p.filterSpeckle,
      colorPrecision: parseInt(modalEl.querySelector('#vt-precision').value, 10) || p.colorPrecision,
      layerDifference: p.layerDifference,
      cornerThreshold: parseInt(modalEl.querySelector('#vt-corner').value, 10) || p.cornerThreshold,
    };
  }
  function openVectorizeDialog() {
    ensureModal();
    modalEl.querySelector('#vt-error').style.display = 'none';
    applyPreset('poster');
    modalEl.style.display = 'flex';
  }
  window.openVectorizeDialog = openVectorizeDialog;
})();
