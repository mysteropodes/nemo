// ---- LABS PROTOTYPE — Screentone / halftone fill (Clip Studio, scoped) ----
// feature-scouting.md #8 explains why naive vectorized screentones blow
// the scene budget (§5) — thousands of tiny dot Paths per flat area. This
// bakes the SAME real boolean-intersection pipeline clip-mask-bake
// already uses (booleanOpWasm, the exact function the Boolean Ops tool
// itself calls) to clip a generated dot/line grid down to one shape's
// silhouette — a real, moderate-count vector result, not one Path per
// dot naively unioned.
//
//   SMLabs.applyScreentone(pathOrNull, {
//     pattern:'dots'|'lines', cellPx:14, dotRatio:0.55, angle:45, color:'#000000'
//   })
// `pathOrNull` = a specific Path, or null/undefined to use the single
// selected path (Select tool). The grid is generated only across the
// target's own bounds (not the whole canvas — bounded cost), unioned via
// booleanOpWasm('unite', cells) into ONE compound shape, then intersected
// with the target via the same helper. The result REPLACES the target's
// fill with the tone pattern; the target's own outline is kept as a
// separate stroke-only Path on top so the silhouette edge stays clean —
// same "outline vs. fill are different objects" pattern the fill tool's
// own strokeColor/fillColor split already uses elsewhere in this file.
(function () {
  function targetPath(explicit) {
    if (explicit) return explicit;
    if (typeof selectedPaths !== 'undefined' && selectedPaths.length === 1) return selectedPaths[0];
    return null;
  }
  // Live-found bug: booleanOpWasm's pairwise fold (tools.js, shared with
  // the real Boolean Ops tool) is correct for its actual use case — 2-3
  // OPERANDS folded together — but for a dot GRID (700+ disjoint same-
  // size shapes) it silently collapses almost everything: per its own
  // comment, each fold step keeps only the SINGLE LARGEST-BY-AREA result
  // as the accumulator carried into the next union, so every dot already
  // unioned in that wasn't part of that largest piece gets dropped at the
  // very next step. Measured live: 761 dots (raw area 55098) unioned down
  // to a 2-piece, 130-area CompoundPath — essentially nothing survived.
  //
  // Correct algorithm for a tone anyway (not a workaround): classify each
  // cell against the target's own silhouette via a simple point-in-path
  // test — a fully-inside dot needs no boolean op at all (just keep the
  // plain circle), a fully-outside one is dropped for free, and only the
  // relatively few EDGE cells (straddling the boundary) need a real
  // intersect — one pairwise operation each, never a giant N-way fold.
  function intersectOne(path, clip) {
    if (window.GeometryWasm && window.GeometryWasm.ready) {
      try { var r = booleanOpWasm('intersect', [path, clip]); if (r) return r; } catch (e) { console.warn('[labs] intersect trame WASM échoué, repli Paper.js', e); }
    }
    return path.intersect(clip, { insert: false });
  }
  function clipCellsToTarget(cells, target) {
    var kept = [];
    cells.forEach(function (c) {
      var b = c.bounds;
      var corners = [b.topLeft, b.topRight, b.bottomLeft, b.bottomRight];
      var insideCount = corners.filter(function (p) { return target.contains(p); }).length;
      if (insideCount === corners.length && target.contains(c.position)) {
        kept.push(c); // fully inside — no boolean op needed
      } else if (insideCount === 0 && !target.contains(c.position)) {
        c.remove(); // fully outside — dropped for free
      } else {
        var clipped = intersectOne(c, target);
        c.remove();
        if (!clipped) { /* nothing */ }
        else if (clipped instanceof Path) {
          if (clipped.segments.length) kept.push(clipped); else clipped.remove();
        } else {
          // CompoundPath (rare for a dot, more likely for a long line cell
          // straddling a concave silhouette) — CLAUDE.md §1: never insert a
          // raw boolean result, explode into flat Paths first.
          clipped.children.slice().forEach(function (isl) { isl.remove(); if (isl.segments.length) kept.push(isl); });
          clipped.remove();
        }
      }
    });
    return kept;
  }

  function dotGrid(bounds, cellPx, dotRatio, angleDeg) {
    var cells = [];
    var r = cellPx * dotRatio / 2;
    var diag = Math.sqrt(bounds.width * bounds.width + bounds.height * bounds.height);
    var n = Math.ceil(diag / cellPx) + 2;
    var cx = bounds.center.x, cy = bounds.center.y;
    for (var gy = -n; gy <= n; gy++) {
      for (var gx = -n; gx <= n; gx++) {
        var lx = gx * cellPx, ly = gy * cellPx;
        var rad = angleDeg * Math.PI / 180;
        var wx = cx + lx * Math.cos(rad) - ly * Math.sin(rad);
        var wy = cy + lx * Math.sin(rad) + ly * Math.cos(rad);
        if (wx < bounds.x - cellPx || wx > bounds.x + bounds.width + cellPx) continue;
        if (wy < bounds.y - cellPx || wy > bounds.y + bounds.height + cellPx) continue;
        var c = new Path.Circle({ center: [wx, wy], radius: r, insert: false });
        cells.push(c);
      }
    }
    return cells;
  }
  function lineGrid(bounds, cellPx, thicknessRatio, angleDeg) {
    var cells = [];
    var thick = cellPx * thicknessRatio;
    var diag = Math.sqrt(bounds.width * bounds.width + bounds.height * bounds.height);
    var n = Math.ceil(diag / cellPx) + 2;
    var cx = bounds.center.x, cy = bounds.center.y;
    var rad = angleDeg * Math.PI / 180;
    for (var gy = -n; gy <= n; gy++) {
      var ly = gy * cellPx;
      var rect = new Path.Rectangle({ point: [-diag, ly - thick / 2], size: [diag * 2, thick], insert: false });
      rect.rotate(angleDeg, [0, 0]);
      rect.translate(new Point(cx, cy));
      cells.push(rect);
    }
    return cells;
  }

  window.SMLabs.applyScreentone = function (path, opts) {
    opts = opts || {};
    var target = targetPath(path);
    if (!target || !(target instanceof Path) || !target.fillColor) { if (typeof showToast === 'function') showToast('Sélectionne une forme remplie (outil Sélection)'); return false; }
    var ld = state.layers[state.activeLayerIdx];
    if (ld.locked) { if (typeof showToast === 'function') showToast('Calque verrouillé'); return false; }
    var pattern = opts.pattern === 'lines' ? 'lines' : 'dots';
    var cellPx = Math.max(3, opts.cellPx || 14);
    var angle = opts.angle !== undefined ? opts.angle : 45;
    var color = opts.color || '#000000';
    var bounds = target.bounds;
    var layer = target.layer || userLayers[state.activeLayerIdx];

    var cells = pattern === 'dots'
      ? dotGrid(bounds, cellPx, opts.dotRatio !== undefined ? opts.dotRatio : 0.55, angle)
      : lineGrid(bounds, cellPx, opts.dotRatio !== undefined ? opts.dotRatio : 0.35, angle);
    if (!cells.length) { if (typeof showToast === 'function') showToast('Zone trop petite pour ce pas de trame'); return false; }

    pushUndo();
    var clipSrc = target.clone({ insert: false });
    var kept = clipCellsToTarget(cells, clipSrc);
    clipSrc.remove();
    if (!kept.length) {
      if (typeof showToast === 'function') showToast('Trame vide sur cette forme (pas trop grand ?)');
      return false;
    }
    var insertAt = layer.children.indexOf(target) + 1;
    var islands = [];
    kept.forEach(function (c) {
      c.fillColor = color; c.strokeColor = null; c.opacity = target.opacity;
      layer.insertChild(insertAt, c);
      islands.push(c);
    });
    // Keep the target's own outline as a clean stroke-only silhouette on
    // top of the dropped-in tone fill, per the header's "outline vs fill
    // are different objects" note — remove its own fill so the tone shows.
    if (target.strokeColor) { var outline = target.clone({ insert: true }); outline.fillColor = null; layer.addChild(outline); }
    target.fillColor = null;
    if (!target.strokeColor) target.remove();
    saveActiveLayerFrame(); updateUI();
    if (window.SMEngineBridge) SMEngineBridge.renderNow();
    if (typeof showToast === 'function') showToast('Trame ' + pattern + ' appliquée (' + islands.length + ' île(s))');
    return true;
  };

  window.SMLabs.register('screentone-bake', {
    flag: 'nemo-labs-screentone',
    describe: 'Trame manga BAKÉE (CSP screentones, scope réaliste — voir feature-scouting #8) : SMLabs.applyScreentone(null, {pattern,cellPx,angle,color}) clippe une grille de points/lignes sur la forme sélectionnée via le pipeline Boolean Ops existant',
  });
})();
