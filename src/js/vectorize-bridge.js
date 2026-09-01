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
  // Bw/Poster/Photo vocabulary VTracer's own CLI/webapp uses, so a preset
  // picked here behaves identically to picking it anywhere else VTracer
  // ships.
  var PRESETS = {
    bw: { colorMode: 'binary', hierarchical: 'stacked', filterSpeckle: 4, colorPrecision: 6, layerDifference: 16, cornerThreshold: 60 },
    poster: { colorMode: 'color', hierarchical: 'stacked', filterSpeckle: 4, colorPrecision: 8, layerDifference: 16, cornerThreshold: 60 },
    photo: { colorMode: 'color', hierarchical: 'stacked', filterSpeckle: 10, colorPrecision: 8, layerDifference: 48, cornerThreshold: 180 },
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

  // Builds ONE Paper item per traced shape — a plain Path for a single
  // contour, a CompoundPath (CLAUDE.md §1's own convention for anything
  // with holes) when vtracer emitted more than one, e.g. a donut. Fill
  // only (VTracer's Color has no alpha — transparency was already
  // consumed upstream to decide which regions to keep, see vectorize.rs).
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
    var item;
    if (childPaths.length === 1) {
      item = childPaths[0];
    } else {
      // A real, multi-contour CompoundPath — correct here, and left alone.
      // Nemo's own save pipeline (_flattenCompoundChildren, app.js) already
      // walks every layer for exactly this case before every
      // saveActiveLayerFrame() — _collectLayerStrokes only knows how to
      // persist `instanceof Path`/Raster, so it keyhole-merges any
      // CompoundPath into the same island/hole representation every other
      // consumer expects (CLAUDE.md §1). Don't pre-flatten here too — that
      // safety net is the established, single place this already happens.
      item = new CompoundPath({ insert: false });
      item.addChildren(childPaths);
    }
    item.fillColor = fill;
    item.strokeColor = null;
    return item;
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
      await window.VectorizeWasm.load();
      var configJson = JSON.stringify({
        colorMode: cfg.colorMode,
        hierarchical: cfg.hierarchical,
        filterSpeckle: cfg.filterSpeckle,
        colorPrecision: cfg.colorPrecision,
        layerDifference: cfg.layerDifference,
        cornerThreshold: cfg.cornerThreshold,
      });
      var resultJson = window.VectorizeWasm.vectorize_image(base64ToBytes(m[2]), configJson);
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
    var built = 0;
    result.shapes.forEach(function (shape) {
      var item = buildShapeItem(shape, mapPt);
      if (item) { newLayer.addChild(item); built++; }
    });
    saveActiveLayerFrame();
    renderArcs(); updateUI();
    if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
    showToast((SM && SM.t ? SM.t('vectorizeDone') : 'Vectorized: ') + built + (SM && SM.t ? '' : ' shapes'));
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
      '<label style="font-size:11px;color:var(--text-dim);display:flex;align-items:center;gap:6px"><input type="checkbox" id="vt-cutout"><span data-i18n="vectorizeCutout">Cutout (punch holes instead of stacking)</span></label>' +
      '<div id="vt-error" style="display:none;font-size:10px;color:#ff8080"></div>' +
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
      btn.disabled = true;
      var cfg = readForm();
      runVectorize(cfg).finally(function () { btn.disabled = false; close(); });
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
      hierarchical: modalEl.querySelector('#vt-cutout').checked ? 'cutout' : 'stacked',
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
    modalEl.querySelector('#vt-cutout').checked = false;
    modalEl.style.display = 'flex';
  }
  window.openVectorizeDialog = openVectorizeDialog;
})();
