// ---- SVG import as real editable vectors ----
// Promoted out of Labs 2026-07-27: this is the only importer in Nemo that
// produces EDITABLE geometry rather than a flat Raster, which makes it the
// obvious front door for a logo, an icon or a character turnaround — and it
// was behind a prototype flag plus a floating button in the corner. The
// parsing work below is unchanged; only the way in is.
// Every other asset importer in Nemo (images.js) drops a flat Raster onto
// the layer — fine for a photo reference, dead weight for a logo/icon/
// character turnaround that was ALREADY vector data before it got dragged
// in. This prototype parses the SVG with Paper.js's own importSVG (real
// parser, not a guess) and turns the result into genuine Nemo strokes:
// selectable, editable with the node tool, tweenable, exportable to Rive —
// indistinguishable from hand-drawn ink to every consumer in CLAUDE.md §1's
// checklist, same target `insertBooleanResult` already hits for boolean-op
// results.
//
// Two problems that made this NOT a five-minute job:
//   1. `<g transform>` nesting — importSVG keeps the real Paper.js Group
//      hierarchy with per-group matrices. serP()/desP() only understand a
//      flat Path with a `segments` array (CLAUDE.md family-of-bug #1) — a
//      raw Group as a layer child would vanish from saveActiveLayerFrame's
//      serialization silently. Fixed by walking the tree BEFORE detaching
//      anything, computing each leaf's accumulated world matrix from its
//      still-live `.parent` chain, cloning + baking that matrix in, THEN
//      inserting only flat Path/CompoundPath leaves into the layer.
//   2. Holes (donut shapes, letterforms with counters) import as a real
//      CompoundPath — also not layer-child-safe (no `.segments`, same bug
//      class). Nemo's whole rendering model doesn't do true holes anywhere
//      (confirmed: `insertBooleanResult`'s multi-hole branch keyhole-merges
//      every hole into its exterior via `_mergeHoleIntoExterior` rather
//      than keeping a CompoundPath) — reused that exact helper here, only
//      difference is preserving each shape's OWN SVG fill/stroke instead
//      of insertBooleanResult's caller-supplied override style, and
//      passing real bezier handles through instead of insertBooleanResult's
//      flattened-to-polyline [0,0] handles (its WASM boolean source is
//      already a polygon; ours is real SVG curve data worth keeping).
//
// Scope, stated honestly: `<path>/<rect>/<circle>/<ellipse>/<polygon>/
// <polyline>/<line>` (via `expandShapes:true`, converts primitives to real
// Path segments) and `<g>` nesting are supported. `<text>`, `<image>`,
// gradients/patterns, clip-paths and filters are NOT — Nemo's own item
// model has nowhere to put them (no PointText/Raster/gradient consumer in
// the CLAUDE.md checklist), so those elements are silently counted and
// reported as skipped rather than silently dropped with no explanation.
//
//   SMLabs.importSVGString(svgText)  — from a string (console/testing)
//   SMLabs.importSVGFile()           — file picker (Tauri dialog or <input>)
(function () {
  function worldMatrix(item) {
    var m = new Matrix();
    var chain = [];
    var cur = item;
    while (cur) { chain.unshift(cur); cur = cur.parent; }
    chain.forEach(function (n) { if (n.matrix) m.append(n.matrix); });
    return m;
  }

  // CompoundPath is checked BEFORE the generic `.children` fallback — it
  // also has `.children` (its sub-paths), but those must stay grouped
  // under their own compound (holes belong to the exterior they were cut
  // from), never scattered as independent leaves.
  function collectLeaves(item, out, skipped) {
    if (item instanceof Path || item instanceof CompoundPath) { out.push(item); return; }
    if (item.children && item.children.length) {
      item.children.slice().forEach(function (c) { collectLeaves(c, out, skipped); });
      return;
    }
    skipped.push(item.className || 'inconnu');
  }

  function segsOf(path) {
    return path.segments.map(function (s) {
      return { point: [s.point.x, s.point.y], handleIn: [s.handleIn.x, s.handleIn.y], handleOut: [s.handleOut.x, s.handleOut.y] };
    });
  }

  // Explodes one baked (world-transformed, still detached) leaf into flat
  // Path children ready for `layer.insertChild` — mirrors
  // insertBooleanResult's exterior/hole classification (tools.js) but
  // keeps the leaf's own style instead of a caller-supplied override.
  function explodeLeaf(leaf, layer, insertAt) {
    var inserted = [];
    if (!(leaf instanceof CompoundPath)) {
      layer.insertChild(insertAt, leaf);
      inserted.push(leaf);
      return inserted;
    }
    var fillColor = leaf.fillColor, strokeColor = leaf.strokeColor, strokeWidth = leaf.strokeWidth, opacity = leaf.opacity;
    var children = leaf.children.slice();
    var exteriors = children.filter(function (c) { return c.clockwise; });
    var holes = children.filter(function (c) { return !c.clockwise; });
    if (!holes.length) {
      (exteriors.length ? exteriors : children).forEach(function (isl) {
        isl.remove();
        isl.fillColor = fillColor; isl.strokeColor = strokeColor; isl.strokeWidth = strokeWidth; isl.opacity = opacity;
        layer.insertChild(insertAt + inserted.length, isl);
        inserted.push(isl);
      });
    } else {
      exteriors.forEach(function (ext) {
        var extSegs = segsOf(ext);
        var myHoles = holes.filter(function (h) { return ext.bounds.contains(h.bounds); });
        myHoles.forEach(function (h) { extSegs = _mergeHoleIntoExterior(extSegs, segsOf(h)); });
        var merged = new Path({ insert: false });
        extSegs.forEach(function (s) {
          merged.add(new Segment(new Point(s.point[0], s.point[1]), new Point(s.handleIn[0], s.handleIn[1]), new Point(s.handleOut[0], s.handleOut[1])));
        });
        merged.closed = true;
        merged.fillColor = fillColor; merged.strokeColor = strokeColor; merged.strokeWidth = strokeWidth; merged.opacity = opacity;
        layer.insertChild(insertAt + inserted.length, merged);
        inserted.push(merged);
      });
    }
    leaf.remove();
    return inserted;
  }

  // `opts.noFit` (2026-08, added for figma-import.js): skips the shrink-to-
  // fit + recenter step below. That step exists for the icon/logo/
  // turnaround case this importer was built for — dropped-in artwork of
  // unknown size, better centered on the canvas than pinned at (0,0). A
  // Figma frame import wants the OPPOSITE: shapes already carry their
  // exact absolute-canvas coordinates (baked in by figma-import.js to
  // match the frame's own layout 1:1) and must land EXACTLY there —
  // recentering the whole group would silently shift every shape by
  // however far its combined bbox center sits from the canvas center,
  // defeating "non destructif niveau mise en page". Omitted/false
  // preserves the exact prior behavior for every existing caller
  // (SMLabs/menu SVG import) — zero change to that already-shipped path.
  function importSVGString(svgText, opts) {
    opts = opts || {};
    var root;
    try { root = project.importSVG(svgText, { insert: false, expandShapes: true }); }
    catch (e) { if (typeof showToast === 'function') showToast('SVG invalide : ' + e.message); return 0; }
    if (!root) { if (typeof showToast === 'function') showToast('SVG invalide ou vide'); return 0; }

    var leaves = [], skipped = [];
    collectLeaves(root, leaves, skipped);
    if (!leaves.length) {
      if (typeof showToast === 'function') showToast('Aucune forme vectorielle importable dans ce SVG');
      root.remove();
      return 0;
    }

    // Bake each leaf's accumulated parent transform while the tree (and
    // its `.parent` chain) is still intact, THEN detach the import root.
    var baked = leaves.map(function (leaf) {
      var wm = worldMatrix(leaf);
      var clone = leaf.clone({ insert: false });
      clone.transform(wm);
      return clone;
    });
    root.remove();

    // Fit-to-canvas: shrink-only (never upscale a small icon into a giant
    // blob) uniform scale + center — same shrink-only rule images.js's
    // fitSize() already uses for bitmap import, kept consistent.
    var b = baked[0].bounds.clone();
    for (var i = 1; i < baked.length; i++) b = b.unite(baked[i].bounds);
    var s = opts.noFit ? 1 : ((b.width > 0 && b.height > 0) ? Math.min(1, state.canvasW / b.width, state.canvasH / b.height) : 1);
    var targetCx = state.canvasW / 2, targetCy = state.canvasH / 2;
    baked.forEach(function (c) {
      if (opts.noFit) return; // coordinates are already final absolute canvas positions
      c.scale(s, b.center);
      c.translate(targetCx - b.center.x, targetCy - b.center.y);
    });

    var ld = state.layers[state.activeLayerIdx];
    if (!ld || ld.locked || ld.symbolId) {
      if (typeof showToast === 'function') showToast(SM.t('toastInvalidLockedLayer'));
      baked.forEach(function (c) { c.remove(); });
      return 0;
    }

    pushUndo();
    ensureKeyframe();
    var layer = userLayers[state.activeLayerIdx];
    layer.activate();
    var insertAt = layer.children.length;
    var inserted = [];
    baked.forEach(function (leaf) {
      var items = explodeLeaf(leaf, layer, insertAt + inserted.length);
      inserted = inserted.concat(items);
    });
    inserted.forEach(function (p) {
      ensureStrokeId(p);
      if (typeof tagOwner === 'function') tagOwner(p);
    });
    saveActiveLayerFrame(); updateUI();
    if (window.SMEngineBridge) SMEngineBridge.renderNow();

    var msg = inserted.length + ' forme(s) importée(s) du SVG';
    if (skipped.length) msg += ' (' + skipped.length + ' élément(s) non-vectoriel(s) ignoré(s) : ' + skipped.join(', ') + ')';
    if (typeof showToast === 'function') showToast(msg);
    return inserted.length;
  };

  function tauriOk() { return typeof window.__TAURI__ !== 'undefined'; }
  var fileInput = null;
  function ensureFileInput() {
    if (fileInput) return fileInput;
    fileInput = document.createElement('input');
    fileInput.type = 'file';
    fileInput.accept = '.svg,image/svg+xml';
    fileInput.style.display = 'none';
    document.body.appendChild(fileInput);
    fileInput.addEventListener('change', function (e) {
      var file = e.target.files && e.target.files[0];
      e.target.value = '';
      if (!file) return;
      var r = new FileReader();
      r.onload = function () { importSVGString(r.result); };
      r.readAsText(file);
    });
    return fileInput;
  }

  async function importSVGFile() {
    if (tauriOk()) {
      var path = await window.__TAURI__.dialog.open({ title: 'Importer un SVG', multiple: false, filters: [{ name: 'SVG', extensions: ['svg'] }] });
      if (!path) return;
      var text = await window.__TAURI__.fs.readTextFile(path);
      importSVGString(text);
      return;
    }
    ensureFileInput().click();
  };


  // Public entry point. The SMLabs.* names above are kept as aliases so any
  // script or plugin written against the prototype keeps working.
  window.SMSvgImport = { openFile: importSVGFile, importString: importSVGString };
  // Aliases for anything written against the prototype (a user script, a
  // plugin) — set only if Labs is present, so this module no longer depends
  // on it to load.
  if (window.SMLabs) {
    window.SMLabs.importSVGFile = importSVGFile;
    window.SMLabs.importSVGString = importSVGString;
  }
})();
