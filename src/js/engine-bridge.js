// ---- C7 CUTOVER STEP 1: Rust-engine bridge (beta toggle) ----
// Binds a VelloEngine to a second canvas stacked exactly over the Paper.js
// one. Toggle ON: the Rust canvas becomes visible on top and re-renders the
// CURRENT scene state every animation frame (rebuilt from the same
// state.layers/getEffectiveStrokes data Paper.js draws from, so both
// engines always show the same document); Paper.js keeps running underneath
// untouched — all tools still work, their edits show up in the Rust render
// on the next rAF tick. This is deliberately a *mirror*, not a replacement
// yet: input still goes to Paper.js, only the pixels come from Rust. Later
// C7 steps move input handling over tool by tool.
(function () {
  var enabled = false;
  var engine = null;
  var rustCanvas = null;
  var rafId = 0;
  var lastSceneJson = '';
  var engineW = 0, engineH = 0;
  // Set by draw-bridge.js (or any future intercepted tool) for the duration
  // of an active drag: tick()'s own rAF loop keeps running the whole time
  // (it's unconditional, not started per-drag), and renderWithOverlayItem
  // deliberately resets lastSceneJson so the FIRST tick after the drag ends
  // re-syncs cleanly — but while a drag is in progress, that same reset made
  // tick()'s very next frame notice "the scene changed" and re-render the
  // scene WITHOUT the live overlay, erasing the in-progress stroke a frame
  // after renderWithOverlayItem drew it. The two loops raced every single
  // pointermove, and the overlay only "won" often enough to be visible under
  // certain timing (e.g. happened to be more visible with DevTools open,
  // which perturbs rAF/JS timing enough to change who wins the race) —
  // never because the underlying logic was actually reliable. Suspending
  // tick() entirely for the drag's duration removes the race: only
  // renderWithOverlayItem drives the picture until pointerup, exactly the
  // single-writer behavior a live preview during an intercepted drag needs.
  var suspended = false;

  function cssColorToRgba(css, opacity) {
    if (!css) return null;
    var h = String(css).replace('#', '');
    if (h.length === 3) h = h.split('').map(function (c) { return c + c; }).join('');
    if (h.length !== 6 && h.length !== 8) return null;
    var r = parseInt(h.substr(0, 2), 16), g = parseInt(h.substr(2, 2), 16), b = parseInt(h.substr(4, 2), 16);
    if (isNaN(r) || isNaN(g) || isNaN(b)) return null;
    // An 8-digit hex (#rrggbbaa) carries its own alpha byte — multiply it
    // with the object's separate opacity, don't let one silently win.
    var hexA = h.length === 8 ? parseInt(h.substr(6, 2), 16) / 255 : 1;
    // opacity can arrive >1 or <0 here: tween easing curves (evalCurve,
    // ui.js) clamp their x (time) input to [0,1] but not their y (value)
    // output — overshoot-style curve handles are draggable outside the unit
    // square on purpose — and that overshot value multiplies straight into
    // a stroke's opacity through interpStroke/pushFade (tweens.js). Rust's
    // ItemIn color channels are u8, so an unclamped alpha above 255 (or
    // below 0) fails serde deserialization and takes the whole render down
    // (see the r2() comment above for why one bad value kills everything).
    var a = Math.max(0, Math.min(255, Math.round(255 * (opacity !== undefined ? opacity : 1) * hexA)));
    return [r, g, b, a];
  }

  // LIVE Paper.js layer children -> engine scene JSON. Reading the live
  // items (via serP) instead of the saved frame records is what makes an
  // in-progress brush stroke visible WHILE dragging — the frame record only
  // gets the stroke at mouseup (saveActiveLayerFrame), which made the first
  // version of this bridge feel broken ("je ne vois pas le trait pendant
  // que je dessine"). loadFrame keeps live children in sync with the
  // current frame, so live-only is also correct across frame changes.
  // `closed:true` whenever a fill is present: serP never stored Paper's
  // .closed flag (pre-existing), and Paper.js fills open paths as if closed
  // anyway — mirroring that with an explicit close keeps the last curve
  // segment of circles/fills instead of dropping it.
  // Raster (imported bitmap) support: the engine caches uploaded pixels by
  // a stable id (see VelloEngine::register_image's own comment) and only
  // wants a reference per frame, not the raw bytes again — registeredIds
  // avoids even the cheap has_image() wasm-boundary round trip for images
  // already known-registered this session. Reading pixels straight from
  // Paper's own Raster#canvas (see registerRasterIfNeeded below) is the
  // simplest reliable way to get raw RGBA8 out without needing any
  // image-decoding logic on the Rust side at all.
  var registeredImageIds = {};

  // Render-only coordinate rounding (2 decimals — 0.01 world units is
  // sub-pixel until 100× zoom): the scene JSON crosses the JS→wasm boundary
  // as TEXT and gets re-parsed by serde on every render. Raw jittered dab
  // coordinates serialize at full double precision (~17 chars per number),
  // which made a dab-heavy document's scene JSON approach a megabyte —
  // most of that pure fractional noise no renderer can show. Rounding here
  // touches ONLY what's sent to the renderer; persistence (serP into frame
  // records) keeps full precision, so document data never degrades.
  // NaN/Infinity (a degenerate anchor, a zero-length normalize, a stray
  // divide-by-zero anywhere upstream) silently becomes JSON `null` under
  // JSON.stringify — and SegIn.point on the Rust side is a plain, non-
  // Option [f64;2] with no #[serde(default)] (unlike every other ItemIn
  // field, see the file-level comment above), so ONE bad coordinate
  // anywhere in the scene throws serde's "invalid type: null, expected
  // f64" and takes the WHOLE render (every layer, not just the offending
  // item) down with it — tick()'s catch then disables the engine for the
  // rest of the session. Falling back to 0 here is a strictly better
  // failure mode: a single item renders in the wrong place instead of the
  // entire GPU-accelerated engine going dark.
  function r2(v) { return isFinite(v) ? Math.round(v * 100) / 100 : 0; }
  function roundSegs(segs) {
    for (var i = 0; i < segs.length; i++) {
      var s = segs[i];
      s.point = [r2(s.point[0]), r2(s.point[1])];
      if (s.handleIn) s.handleIn = [r2(s.handleIn[0]), r2(s.handleIn[1])];
      if (s.handleOut) s.handleOut = [r2(s.handleOut[0]), r2(s.handleOut[1])];
    }
    return segs;
  }
  function registerRasterIfNeeded(raster) {
    // Paper.js's Raster keeps its OWN internal <canvas> representation
    // (`.canvas`, already decoded/ready once `.loaded` is true) rather than
    // exposing the source as a plain `.image` HTMLImageElement — reading
    // pixels straight from that is simpler and more reliable than trying to
    // re-decode the original dataURL ourselves. Known simplification: this
    // canvas reflects the raster's CURRENT `.size` at registration time, so
    // if the raster is later resized, the cached texture keeps the
    // resolution it had when first registered — acceptable for how rasters
    // are used today (imported once via images.js, not interactively
    // resized), revisit if that changes.
    if (!raster.loaded || !raster.canvas) return null; // not decoded yet — try again next tick
    var id = raster.data && raster.data.src;
    if (!id) return null;
    if (registeredImageIds[id]) return id;
    var cv = raster.canvas;
    var ctx = cv.getContext('2d');
    var pixels = ctx.getImageData(0, 0, cv.width, cv.height).data;
    engine.register_image(id, pixels, cv.width, cv.height);
    registeredImageIds[id] = true;
    return id;
  }

  // `skipVolatile` (renderWithOverlayItem only): leaves out the three
  // per-pointermove cursor overlays (pressure/eraser/pen-preview) so the
  // rest of the scene can be CACHED across the moves of one drag — those
  // three are the only parts of the scene that legitimately change between
  // two moves of a suspended drag (the document itself is untouched until
  // commit, by draw-bridge's own design), and renderWithOverlayItem
  // re-appends them fresh on every call. Without this split, drawing on a
  // document full of brush-texture dabs (up to 180 small Paths PER textured
  // stroke) re-serialized every dab on every single pointermove — the
  // reported "ça rame avec les brush custom/preset" lag.
  // Upload helpers for reference-bridge.js: registerCachedImage uploads an
  // <img>/canvas ONCE per id (image / image-sequence frames, which never
  // change); registerImagePixels force-re-uploads under the same id (the
  // video reference re-uses one id and replaces its pixels on every seek,
  // keeping GPU memory at a single frame regardless of video length).
  function drawableToPixels(source) {
    var cv;
    if (source instanceof HTMLCanvasElement) cv = source;
    else {
      cv = document.createElement('canvas');
      cv.width = source.naturalWidth || source.width;
      cv.height = source.naturalHeight || source.height;
      cv.getContext('2d').drawImage(source, 0, 0);
    }
    return { pixels: cv.getContext('2d').getImageData(0, 0, cv.width, cv.height).data, w: cv.width, h: cv.height };
  }
  function registerCachedImage(id, source) {
    if (!engine || registeredImageIds[id]) return;
    var p = drawableToPixels(source);
    engine.register_image(id, p.pixels, p.w, p.h);
    registeredImageIds[id] = true;
  }
  function registerImagePixels(id, source) {
    if (!engine) return;
    var p = drawableToPixels(source);
    engine.register_image(id, p.pixels, p.w, p.h);
    registeredImageIds[id] = true;
  }
  // Raw-bytes variant (EXPERIMENTAL, native-video-decode branch): the
  // native decoder already produces tightly-packed RGBA8 — going through
  // registerImagePixels above would draw those bytes onto a canvas just
  // to getImageData them straight back out (two full-frame copies + a
  // premultiply/unpremultiply round-trip that can subtly alter pixel
  // values). Feed engine.register_image directly instead; `pixels` must
  // be a Uint8Array of exactly w*h*4 bytes.
  function registerImageRaw(id, pixels, w, h) {
    if (!engine) return false;
    engine.register_image(id, pixels, w, h);
    registeredImageIds[id] = true;
    return true;
  }

  function buildSceneJson(skipVolatile) {
    var layers = [];
    // StoryBoard montage preview (storyboard.js, 2026-07): when the node
    // space has an active montage, the canvas shows THAT montage's frame
    // at its own playhead instead of the document's layers — the preview
    // strokes live in storyboard.js's dedicated service Paper layer (same
    // pattern as ghostAllLayer), read through the SAME onionLayerItems
    // item builder every other stroke-data consumer uses (CLAUDE.md §1:
    // no new parallel serialization path).
    var sbPreview = (state.appMode === 'storyboard' && window.SMStoryboard) ? SMStoryboard.getPreviewLayer() : null;
    if (sbPreview) {
      layers.push({ items: onionLayerItems(sbPreview) });
    } else
    for (var i = 0; i < state.layers.length; i++) {
      if (!layerIsEffectivelyVisible(i) || !userLayers[i]) { layers.push({ items: [] }); continue; }
      var children = userLayers[i].children;
      var items = [];
      // Motion mode (motion.js): a keyed position/rotation/scale/opacity
      // transform for this layer at the CURRENT frame, applied ONLY to the
      // JSON items below — never to userLayers[i] itself (see motion.js's
      // header comment on why: mutating the live Paper.js layer would get
      // baked into the next saveActiveLayerFrame() permanently). Null (the
      // overwhelmingly common case — no motion on this layer) skips the
      // per-item transform pass entirely below.
      var motionMat = (window.SMMotion && children.length) ? SMMotion.layerMotionAt(i, state.currentFrame) : null;
      // Pivot = auto bounds center + the layer's Anchor Point offset
      // (motionMat.ax/ay) — see motion.js's layerMotionAt header comment.
      var motionPivot = motionMat ? { x: userLayers[i].bounds.center.x + motionMat.ax, y: userLayers[i].bounds.center.y + motionMat.ay } : null;
      // Brush-texture companions (isBrushTextureCopy — bitmap raster or
      // vector dab group) don't get their own Elements row in Motion
      // (motion.js's layerElements folds them into their anchor's, "merge
      // trait et fond quand ils font partie d'une même shape") — so at
      // render time their per-element Motion transform must resolve
      // through the ANCHOR's strokeId, not their own (which the companion
      // may not even have, having never been individually keyed). One
      // pass over children building brushGroupId -> anchor strokeId,
      // reused below instead of a lookup per companion.
      var brushAnchorStrokeId = null;
      for (var bi = 0; bi < children.length; bi++) {
        var bc = children[bi];
        if (bc.data && bc.data.brushGroupId && !bc.data.isBrushTextureCopy && bc.data.strokeId) {
          if (!brushAnchorStrokeId) brushAnchorStrokeId = {};
          brushAnchorStrokeId[bc.data.brushGroupId] = bc.data.strokeId;
        }
      }
      // EXPERIMENTAL (native-video-decode branch): a natively-decoded video
      // layer has NO Paper children — its picture is one image item under a
      // per-layer fixed id ('nv:<i>'), pixels pushed by native-video-bridge's
      // onFrameChanged sync (same replaced-GPU-texture pattern as the
      // rotoscopy reference). Emitted inside the normal per-layer loop so
      // z-order and layer visibility behave exactly like any other layer;
      // Motion-mode transforms are computed independently of the
      // children.length gate above (an empty Paper layer has no usable
      // bounds, so the pivot is the video rect's own center).
      if (state.layers[i].nativeVideo && window.SMEngineBridge && registeredImageIds['nv:' + i]) {
        var nv = state.layers[i].nativeVideo;
        var inF = window.layerInPoint ? layerInPoint(state.layers[i]) : 0;
        var outF = window.layerOutPoint ? layerOutPoint(state.layers[i]) : state.totalFrames - 1;
        if (state.currentFrame >= inF && state.currentFrame <= outF) {
          var nvS = Math.min(state.canvasW / nv.width, state.canvasH / nv.height);
          var nvW = nv.width * nvS, nvH = nv.height * nvS;
          var nvRect = { x: (state.canvasW - nvW) / 2, y: (state.canvasH - nvH) / 2, width: nvW, height: nvH };
          var nvOp = 1;
          var nvMat = window.SMMotion ? SMMotion.layerMotionAt(i, state.currentFrame) : null;
          if (nvMat) {
            var nvPivot = { x: nvRect.x + nvRect.width / 2 + nvMat.ax, y: nvRect.y + nvRect.height / 2 + nvMat.ay };
            nvRect = SMMotion.transformImageRect(nvRect, nvPivot, nvMat);
            nvOp *= nvMat.op;
          }
          items.push({ image: { imageId: 'nv:' + i, x: nvRect.x, y: nvRect.y, width: nvRect.width, height: nvRect.height, opacity: nvOp } });
        }
      }
      for (var s = 0; s < children.length; s++) {
        var c = children[s];
        // Team review view filter — 'mine' hides everyone else's content
        // (ghosts included, since a ghost's ownerId is the ORIGINAL
        // author, never you); 'revisions' hides plain uncontested content
        // so review attention lands only on ghosts + active corrections.
        // Geometry is untouched either way — this only decides what goes
        // into THIS render, never mutates the document.
        if (state.revisionView === 'mine') {
          if (c.data && c.data.ownerId && state.userProfile && c.data.ownerId !== state.userProfile.id) continue;
        } else if (state.revisionView === 'revisions') {
          if (!(c.data && (c.data.isRevisionGhost || c.data.revisionParentId))) continue;
        }
        // Element-level Motion target (2026-07): a strokeId-scoped transform
        // nested INSIDE the layer's own — applied FIRST below, pivoted
        // around this item's OWN bounds (never the whole layer's), matching
        // AE's shape-group-inside-a-layer composition. null in the common
        // case (this item has no per-element motion of its own).
        var cStrokeId = c.data && ((c.data.isBrushTextureCopy && brushAnchorStrokeId && brushAnchorStrokeId[c.data.brushGroupId]) || c.data.strokeId);
        var elMat = (window.SMMotion && cStrokeId) ? SMMotion.elementMotionAt(i, cStrokeId, state.currentFrame) : null;
        var elPivot = elMat ? { x: c.bounds.center.x + elMat.ax, y: c.bounds.center.y + elMat.ay } : null;
        if (c instanceof Raster) {
          var imageId = registerRasterIfNeeded(c);
          if (!imageId) continue;
          var rb = c.bounds; // display rect — Paper's Raster is center-positioned, bounds gives top-left directly
          var imgOp = c.opacity !== undefined ? c.opacity : 1;
          if (elMat) { rb = SMMotion.transformImageRect(rb, elPivot, elMat); imgOp *= elMat.op; }
          if (motionMat) { rb = SMMotion.transformImageRect(rb, motionPivot, motionMat); imgOp *= motionMat.op; }
          items.push({
            image: { imageId: imageId, x: rb.x, y: rb.y, width: rb.width, height: rb.height, opacity: imgOp },
          });
          continue;
        }
        // The eraser (tools.js eraseAtPoint) produces a CompoundPath the
        // moment a bite severs a shape into disjoint islands or leaves a
        // hole — `c instanceof Path` is false for a CompoundPath (siblings
        // in Paper's class hierarchy, not parent/child), so this loop used
        // to silently drop the WHOLE shape from the rendered scene the
        // instant an erase touch produced one, even though it was still
        // sitting right there in the layer's data — which is exactly what
        // read as "one eraser touch deletes the whole thing", when really
        // only the picture stopped updating, not the geometry. Flattening a
        // CompoundPath into one item per sub-path (styled from the
        // CompoundPath's own fillColor/strokeColor/opacity, since individual
        // children never carry their own — Paper.js style cascades from the
        // parent) renders every island; a true enclosed hole won't be cut
        // out this way (each island paints solid), but that's a much better
        // fallback than the shape vanishing outright, and is the rare case
        // (most erase bites are edge nibbles that split a shape, not punch
        // a fully-enclosed hole through its middle).
        var subPaths;
        if (c instanceof CompoundPath) subPaths = c.children.filter(function (ch) { return ch instanceof Path && ch.segments.length >= 2; });
        else if (c instanceof Path && c.segments.length >= 2) subPaths = [c];
        else continue;
        subPaths.forEach(function (sub) {
          var sd = serP(sub);
          if (elMat) sd.segments = SMMotion.transformSegments(sd.segments, elPivot, elMat);
          if (motionMat) sd.segments = SMMotion.transformSegments(sd.segments, motionPivot, motionMat);
          var op = c.opacity !== undefined ? c.opacity : 1;
          if (elMat) op *= elMat.op;
          if (motionMat) op *= motionMat.op;
          var strokeScale = 1;
          if (elMat) strokeScale *= (Math.abs(elMat.sx) + Math.abs(elMat.sy)) / 2;
          if (motionMat) strokeScale *= (Math.abs(motionMat.sx) + Math.abs(motionMat.sy)) / 2;
          // The path's OWN closed flag (now correctly carried by serP(),
          // see app.js), NOT "has a fillColor" — that heuristic sent an
          // unwanted closing stroke segment across any OPEN path that also
          // happened to have a fill (the norm since Draw-tool strokes get
          // fillColor by default), and conversely dropped the closure on a
          // genuinely closed STROKE-only shape (no fill) with no visible
          // symptom for filled shapes (fill rendering closes the boundary
          // regardless of this flag) but a real one for stroke-only paths.
          //
          // Stroke sub-fields are only emitted when there IS a stroke, and
          // dash fields only when there IS a dash — every ItemIn field is an
          // Option with a default on the Rust side, and blindly repeating
          // `"dashPattern":[],"dashOffset":0,"miterLimit":10,...` on every
          // one of a dab-heavy document's thousands of fill-only items was
          // ~200 bytes/item of dead weight re-parsed by serde every render.
          var item = {
            segments: roundSegs(sd.segments),
            closed: !!sd.closed,
            fillColor: cssColorToRgba(c.fillColor ? c.fillColor.toCSS(true) : null, op),
          };
          var sc = cssColorToRgba(c.strokeColor ? c.strokeColor.toCSS(true) : null, op);
          if (sc) {
            item.strokeColor = sc;
            item.strokeWidth = (c.strokeWidth || 1) * strokeScale;
            item.strokeCap = c.strokeCap;
            item.strokeJoin = c.strokeJoin;
            item.miterLimit = c.miterLimit;
            if (c.dashArray && c.dashArray.length) {
              item.dashPattern = c.dashArray;
              item.dashOffset = c.dashOffset;
            }
          }
          if (c.data && c.data.paintOrder) item.paintOrder = c.data.paintOrder;
          items.push(item);
        });
      }
      var bm = state.layers[i].blendMode;
      // Track matte (2026-07, scouted from Caddis's Layer.matteMode):
      // AE convention — the matte SOURCE is implicitly the layer directly
      // above this one (i+1), never referenced by id. Same wire shape as
      // blendMode (a plain per-layer string, undefined = no matte), read
      // by engine.rs's composite_scene which also SKIPS painting the
      // source layer as its own visible content once it's consumed.
      var mm = state.layers[i].matteMode;
      layers.push({ items: items, blendMode: (bm && bm !== 'normal') ? bm : undefined, matteMode: (mm && mm !== 'none') ? mm : undefined });
    }
    // artboard background as the bottom item of a synthetic bottom layer,
    // mirroring drawStage()'s background rect
    var bgItems = [{
      segments: [
        { point: [0, 0] }, { point: [state.canvasW, 0] },
        { point: [state.canvasW, state.canvasH] }, { point: [0, state.canvasH] },
      ],
      closed: true,
      fillColor: cssColorToRgba(state.canvasBg, 1),
      strokeColor: null,
      strokeWidth: 1,
    }];
    // Rotoscopy reference (reference-bridge.js) — above the artboard rect,
    // below every drawing layer, exactly where tracing reference belongs.
    if (window.SMReference) {
      var refItem = window.SMReference.buildRefItem(registerCachedImage);
      if (refItem) bgItems.push(refItem);
    }
    layers.unshift({ items: bgItems });
    // "Clip to canvas" (Document panel, off by default — off-canvas artwork
    // is visible by default, matching every vector tool's normal behavior)
    // is a mask, not a real GPU clip: four opaque bands, colored to match
    // #canvas-area's own CSS background (the color that's already visible
    // through the transparent WebGPU surface outside the artboard), painted
    // over whatever content sticks out past the canvas rect. Simpler and
    // lower-risk than plumbing a real vello Scene::push_layer clip through
    // engine.rs for a purely cosmetic "hide the overflow" toggle.
    if (state.canvasClip) layers.push({ items: buildClipMaskItems() });
    // Onion ghosts sit right above the background but BELOW the current
    // frame's real artwork (layers[1..]) — a faint reference, never
    // obscuring what's actually being drawn on top of it.
    var onionItems = buildOnionSkinItems();
    if (onionItems.length) layers.splice(1, 0, { items: onionItems });
    var ghostAllItems = buildGhostAllItems();
    if (ghostAllItems.length) layers.splice(1, 0, { items: ghostAllItems });
    var nodeItems = buildNodeHandleItems();
    if (nodeItems.length) layers.push({ items: nodeItems });
    var xformItems = buildTransformBoxItems();
    if (xformItems.length) layers.push({ items: xformItems });
    var marqueeItems = buildMarqueeItems();
    if (marqueeItems.length) layers.push({ items: marqueeItems });
    var fsSelItems = buildFSSelectionItems();
    if (fsSelItems.length) layers.push({ items: fsSelItems });
    var revisionItems = buildRevisionOutlineItems();
    if (revisionItems.length) layers.push({ items: revisionItems });
    var commentItems = buildCommentPinItems();
    if (commentItems.length) layers.push({ items: commentItems });
    if (window.SMCamera) {
      var cameraItems = SMCamera.buildOverlayItems();
      if (cameraItems.length) layers.push({ items: cameraItems.map(function (it) { it.segments = roundSegs(it.segments); return it; }) });
    }
    if (window.SMMotion) {
      var motionItems = SMMotion.buildOverlayItems();
      if (motionItems.length) layers.push({ items: motionItems.map(function (it) { it.segments = roundSegs(it.segments); return it; }) });
    }
    if (typeof fillCloseStrokesOverlayItems === 'function') {
      var fillCloseItems = fillCloseStrokesOverlayItems();
      if (fillCloseItems.length) layers.push({ items: fillCloseItems.map(function (it) { it.segments = roundSegs(it.segments); return it; }) });
    }
    if (!skipVolatile) {
      var eraserItems = buildEraserCursorItems();
      if (eraserItems.length) layers.push({ items: eraserItems });
      var pressureItems = buildPressureCursorItems();
      if (pressureItems.length) layers.push({ items: pressureItems });
      var penItems = buildPenPreviewItems();
      if (penItems.length) layers.push({ items: penItems });
    }
    var arcItems = buildArcHandleItems();
    if (arcItems.length) layers.push({ items: arcItems });
    var safetyItems = buildSafetyZoneItems();
    if (safetyItems.length) layers.push({ items: safetyItems });
    var perspectiveItems = window.buildPerspectiveGuideItems ? window.buildPerspectiveGuideItems() : [];
    if (perspectiveItems.length) layers.push({ items: perspectiveItems });
    return JSON.stringify({ layers: layers });
  }

  // Onion skin ghosts (tweens.js's renderOS() keeps onionPrevLayer/
  // onionNextLayer populated with tinted-color/reduced-opacity Paper.js
  // paths for the surrounding frames, exactly like it always has) — but
  // buildSceneJson() above only ever walked state.layers/userLayers, so
  // once the opaque Rust canvas became the default renderer sitting on top
  // of Paper's own (now invisible) canvas, these ghosts kept being computed
  // correctly and just never made it into the picture the user actually
  // sees. Same conversion as the main per-layer loop (serP() + rgba), just
  // reading these two dedicated onion layers instead.
  function onionLayerItems(layer) {
    var items = [];
    layer.children.forEach(function (c) {
      // Same Raster handling as buildSceneJson() above — an imported image/
      // video-frame ghosted onto the onion-skin layer (desR, tweens.js
      // renderOS) was silently dropped here, same "new item type not
      // handled everywhere" gap as the CompoundPath case right below.
      if (c instanceof Raster) {
        var imageId = registerRasterIfNeeded(c);
        if (imageId) {
          var rb = c.bounds;
          items.push({ image: { imageId: imageId, x: rb.x, y: rb.y, width: rb.width, height: rb.height, opacity: c.opacity !== undefined ? c.opacity : 1 } });
        }
        return;
      }
      // Same CompoundPath flattening as buildSceneJson() above — an erased
      // (possibly multi-island) shape ghosted onto the onion-skin layer hit
      // the exact same "silently dropped, not just this frame's real
      // artwork" gap.
      var subPaths;
      if (c instanceof CompoundPath) subPaths = c.children.filter(function (ch) { return ch instanceof Path && ch.segments.length >= 2; });
      else if (c instanceof Path && c.segments.length >= 2) subPaths = [c];
      else return;
      subPaths.forEach(function (sub) {
        var sd = serP(sub);
        var op = c.opacity !== undefined ? c.opacity : 1;
        items.push({
          segments: roundSegs(sd.segments),
          // The path's OWN closed flag, not "has a fillColor" — that
          // heuristic broke Outline onion-skin mode, which intentionally
          // nulls fillColor on the ghost (renderOS in tweens.js) to show a
          // silhouette instead of a solid tint: a closed shape traced in
          // outline mode would otherwise report closed:false and render with
          // a gap where the fill used to close the loop.
          closed: !!sub.closed,
          fillColor: cssColorToRgba(c.fillColor ? c.fillColor.toCSS(true) : null, op),
          strokeColor: cssColorToRgba(c.strokeColor ? c.strokeColor.toCSS(true) : null, op),
          strokeWidth: c.strokeWidth || 1,
          strokeCap: c.strokeCap,
          strokeJoin: c.strokeJoin,
        });
      });
    });
    return items;
  }
  function buildOnionSkinItems() {
    if (!state.onionSkin || state.playing) return [];
    return onionLayerItems(onionPrevLayer).concat(onionLayerItems(onionNextLayer));
  }
  // "Ghost all keyframes" reuses the exact same onionLayerItems() reader —
  // ghostAllLayer (tweens.js's renderGhostAll) is populated the same way
  // onionPrevLayer/onionNextLayer are, just for every keyframe instead of
  // only the immediate neighbors.
  function buildGhostAllItems() {
    if (!state.ghostAllFrames || state.playing) return [];
    return onionLayerItems(ghostAllLayer);
  }

  // ---- Subselection node/tangent handle overlay ----
  // The rust canvas sits opaquely on top of Paper's own canvas, so once the
  // beta engine is on, Paper's own renderNodeHandles() overlay (in
  // tools.js — circles for anchors, squares for bezier handles, thin guide
  // lines between them) is drawn but invisible underneath. Rebuilt here as
  // plain scene items (using the same JSON format everything else in the
  // bridge uses) from the exact same source data renderNodeHandles() reads
  // (nodeEditTargetPath/nodeEditSegmentsData/_nodeSel, all globals from
  // tools.js), so the Rust mirror shows the identical picture.
  var KAPPA = 0.5522847498;
  function circleItem(cx, cy, r, fillColor, strokeColor, strokeWidth) {
    var k = r * KAPPA;
    var pts = [[r, 0], [0, r], [-r, 0], [0, -r]];
    var tangents = [[0, 1], [-1, 0], [0, -1], [1, 0]];
    var segments = pts.map(function (p, i) {
      var t = tangents[i];
      return {
        point: [cx + p[0], cy + p[1]],
        handleIn: [-t[0] * k, -t[1] * k],
        handleOut: [t[0] * k, t[1] * k],
      };
    });
    return { segments: roundSegs(segments), closed: true, fillColor: fillColor, strokeColor: strokeColor, strokeWidth: strokeWidth };
  }
  function rectItem(cx, cy, halfSize, fillColor, strokeColor, strokeWidth) {
    var segments = [
      { point: [cx - halfSize, cy - halfSize] },
      { point: [cx + halfSize, cy - halfSize] },
      { point: [cx + halfSize, cy + halfSize] },
      { point: [cx - halfSize, cy + halfSize] },
    ];
    return { segments: roundSegs(segments), closed: true, fillColor: fillColor, strokeColor: strokeColor, strokeWidth: strokeWidth };
  }
  function lineItem(fromPt, toPt, strokeColor, strokeWidth) {
    return {
      segments: roundSegs([{ point: fromPt }, { point: toPt }]),
      closed: false,
      fillColor: null,
      strokeColor: strokeColor,
      strokeWidth: strokeWidth,
    };
  }
  function boundsRectItem(left, top, right, bottom, fillColor, strokeColor, strokeWidth) {
    var segments = [
      { point: [left, top] }, { point: [right, top] },
      { point: [right, bottom] }, { point: [left, bottom] },
    ];
    return { segments: roundSegs(segments), closed: true, fillColor: fillColor, strokeColor: strokeColor, strokeWidth: strokeWidth };
  }

  // ---- "Clip to canvas" mask (see the call site's own comment) ----
  var CLIP_MASK_COLOR = [35, 35, 48, 255]; // #canvas-area's CSS background
  var CLIP_MASK_SPAN = 100000; // world units — comfortably past any realistic off-canvas content
  function buildClipMaskItems() {
    var w = state.canvasW, h = state.canvasH;
    return [
      boundsRectItem(-CLIP_MASK_SPAN, -CLIP_MASK_SPAN, w + CLIP_MASK_SPAN, 0, CLIP_MASK_COLOR, null, 1), // top
      boundsRectItem(-CLIP_MASK_SPAN, h, w + CLIP_MASK_SPAN, h + CLIP_MASK_SPAN, CLIP_MASK_COLOR, null, 1), // bottom
      boundsRectItem(-CLIP_MASK_SPAN, 0, 0, h, CLIP_MASK_COLOR, null, 1), // left
      boundsRectItem(w, 0, w + CLIP_MASK_SPAN, h, CLIP_MASK_COLOR, null, 1), // right
    ];
  }

  // ---- Safety zones (Document panel toggle) ----
  // Standard broadcast/film safe-area convention: action-safe = inset 5% of
  // each dimension (90% of canvas visible), title-safe = inset 10% (80%
  // visible) — the same percentages used across TV/animation delivery specs
  // (e.g. the long-standing SMPTE recommendation), not something specific
  // to this app to invent. Pure overlay guide, never part of the actual
  // rendered/exported frame.
  function buildSafetyZoneItems() {
    if (!state.safetyZones) return [];
    var w = state.canvasW, h = state.canvasH, zs = 1 / view.zoom;
    var actionColor = [255, 210, 0, 200], titleColor = [255, 90, 90, 200];
    function inset(pct) {
      var mx = w * pct, my = h * pct;
      return boundsRectItem(mx, my, w - mx, h - my, null, actionColor, zs);
    }
    var action = inset(0.05);
    var title = inset(0.10);
    title.strokeColor = titleColor;
    return [action, title];
  }

  // ---- Select-tool transform box + marquee overlay ----
  // Same reasoning as the node-handle overlay above: renderTransformHandles()
  // and the marquee-rectangle drawing in tools.js both go through Paper's own
  // canvas, invisible under the opaque rust canvas — the user caught this
  // directly too ("il manque la transform box, la selection par lot avec
  // rectangle de selection"). Rebuilt from the exact same source
  // (xformSelBounds()/selectedPaths for the box, the module-global _marquee
  // for the rubber-band rect — both plain tools.js globals), matching colors
  // and screen-space sizes 1:1 (see tools.js's renderTransformHandles/the
  // marquee Path.Rectangle literal for the reference values). One
  // simplification: dashed strokes aren't supported by the engine's Stroke
  // type yet, so the dashed outline/marquee border render as solid instead —
  // a cosmetic gap, not a functional one, until dashing is added engine-side.
  function buildTransformBoxItems() {
    if (state.tool !== 'select' || !selectedPaths.length || typeof orientedSelBox !== 'function') return [];
    // Oriented box (tools.js orientedSelBox — "les boîtes de transformation
    // ne tournent pas avec l'objet quand on rotate"): everything below is
    // computed in the box's de-rotated space then mapped to world through
    // selBoxPt, so the whole gizmo — outline, 8 handles, rotate grip,
    // anchor crosshair — rotates rigidly with the selection.
    var box = orientedSelBox();
    if (!box) return [];
    var b = box.b;
    var zs = 1 / view.zoom;
    var items = [];
    // Compose the layer's Motion transform on top of the stroke's own
    // boxAngle — the gizmo must sit on the object where it RENDERS
    // ("si on rotate la propriété dans le panel la box tourne pas avec
    // l'objet"), and the panel's position/scale/rotation all flow
    // through the same map.
    var nvMap = (window.SMMotion && SMMotion.layerMotionPointMap) ? SMMotion.layerMotionPointMap(state.activeLayerIdx) : null;
    function W(x, y) { var p = selBoxPt(x, y, box); if (nvMap) { var w = nvMap.fwd(p.x, p.y); return w; } return [p.x, p.y]; }
    if (!box.angle && !(nvMap && nvMap.mat.rot)) {
      // no rotation anywhere — but position/scale from Motion still apply
      var tl = W(b.left, b.top), br = W(b.right, b.bottom);
      items.push(boundsRectItem(tl[0], tl[1], br[0], br[1], null, [74, 158, 255, 204], 1 * zs));
    } else {
      // rotated outline = four explicit edges (boundsRectItem is AABB-only)
      var c1 = W(b.left, b.top), c2 = W(b.right, b.top), c3 = W(b.right, b.bottom), c4 = W(b.left, b.bottom);
      items.push(lineItem(c1, c2, [74, 158, 255, 204], 1 * zs));
      items.push(lineItem(c2, c3, [74, 158, 255, 204], 1 * zs));
      items.push(lineItem(c3, c4, [74, 158, 255, 204], 1 * zs));
      items.push(lineItem(c4, c1, [74, 158, 255, 204], 1 * zs));
    }
    var midX = b.left + b.width / 2, midY = b.top + b.height / 2;
    var corners = {
      nw: W(b.left, b.top), n: W(midX, b.top), ne: W(b.right, b.top),
      e: W(b.right, midY), se: W(b.right, b.bottom), s: W(midX, b.bottom),
      sw: W(b.left, b.bottom), w: W(b.left, midY),
    };
    Object.keys(corners).forEach(function (k) {
      var p = corners[k];
      items.push(rectItem(p[0], p[1], 3.5 * zs, [255, 255, 255, 255], [74, 158, 255, 255], 1.2 * zs));
    });
    // Anchor/pivot marker (redesign 2026-07-09, AE-style anchor point) — a
    // small ringed crosshair AT the point rotation actually pivots around
    // (tools.js xformAnchorPoint), so a non-center anchor is visible on the
    // shape itself, not just as an abstract dot in the side panel widget.
    // Computed BEFORE the rotate ring below since the ring is centered here.
    var ap = { x: corners.n[0], y: b.top }; // fallback if xformAnchorPoint is unavailable
    if (typeof xformAnchorPoint === 'function') {
      var ap0 = xformAnchorPoint(b);
      // A custom pivot (Alt+click) is ALREADY a world point — only the
      // bounds-derived anchors live in the box's de-rotated space.
      var apArr = state.xformAnchorCustom ? [ap0.x, ap0.y] : W(ap0.x, ap0.y);
      ap = { x: apArr[0], y: apArr[1] };
      // Light hover scale (2026-07, live feedback) — select-bridge.js's
      // onMove sets state.xformAnchorHovered on a passive hover-only pass
      // (not an active drag). +25% radius reads as a clear but subtle
      // "you can grab this" without a jarring size jump.
      var ar = (state.xformAnchorHovered ? 10 : 8) * zs;
      items.push(circleItem(ap.x, ap.y, ar, null, [74, 158, 255, 255], 1.2 * zs));
      items.push(lineItem([ap.x - ar, ap.y], [ap.x + ar, ap.y], [74, 158, 255, 255], 1 * zs));
      items.push(lineItem([ap.x, ap.y - ar], [ap.x, ap.y + ar], [74, 158, 255, 255], 1 * zs));
    }
    // Rotate RING (2026-07, replaces the old tiny offset stem+dot handle —
    // live feedback: hard to notice, only grabbable from one exact spot).
    // Centered on the anchor/pivot. Small and mostly size-INDEPENDENT (per
    // user mockup — a small ring near the pivot, not one that grows to
    // enclose the whole selection); must match select-bridge.js's
    // computeHandles ringRadius formula exactly, or the drawn ring and the
    // hit-testable one would disagree.
    var ringRadius = Math.min(36 * zs, Math.max(b.width, b.height) * 0.3);
    // Light hover grow (2026-07, same "you can grab this" pattern as the
    // anchor crosshair's own hover scale just above) — state.xformRingHovered
    // set by select-bridge.js's passive hover-only pointermove pass; only
    // the DRAWN radius grows, not the hit-test one (computeHandles), same
    // treatment as the anchor dot.
    var ringDrawRadius = ringRadius + (state.xformRingHovered ? 4 : 0) * zs;
    items.push(circleItem(ap.x, ap.y, ringDrawRadius, null, [74, 158, 255, 160], 1 * zs));
    return items;
  }
  function buildMarqueeItems() {
    if (!_marquee.active || !_marquee.rect) return [];
    var b = _marquee.rect.bounds;
    return [boundsRectItem(b.left, b.top, b.right, b.bottom, [74, 158, 255, 20], [74, 158, 255, 230], 1 / view.zoom)];
  }
  // Fill/Stroke Select tool (v18, tools.js _fsSel) — a fill selection gets
  // a filled orange highlight tracing the SAME geometry (Animate's dotted-
  // pattern convention, simplified to a flat translucent tint since this
  // engine has no tiled-pattern brush); a stroke/segment selection gets a
  // thick dashed line traced along fsHighlightPath()'s (non-destructive)
  // extracted arc — the real geometry never mutates just to show this.
  // Approximates a tiled crosshatch pattern (the Japanese-animation-layout
  // convention for marking a designated-but-unresolved area) by clipping a
  // family of parallel diagonal lines to the region's actual shape — this
  // engine has no tiled-pattern brush (see buildFSSelectionItems), so the
  // hatch is just literal short line items computed fresh each call. Works
  // for concave shapes too: each candidate line's crossings with `region`
  // are found via getIntersections and paired up even-odd (offset[0]-
  // offset[1] is "inside", offset[2]-offset[3] is "inside", etc.), same
  // rule a scanline fill uses — no boolean ops needed.
  function fsHatchClipLines(region, spacingWorld, angleDeg) {
    var b = region.bounds;
    if (!b || b.width <= 0 || b.height <= 0) return [];
    var diag = Math.sqrt(b.width * b.width + b.height * b.height) + spacingWorld * 2;
    var cx = b.center.x, cy = b.center.y;
    var rad = angleDeg * Math.PI / 180;
    var dx = Math.cos(rad), dy = Math.sin(rad);
    var nx = -dy, ny = dx;
    var lines = [];
    var count = Math.ceil(diag / spacingWorld);
    for (var i = -count; i <= count; i++) {
      var off = i * spacingWorld;
      var ox = cx + nx * off, oy = cy + ny * off;
      var p1 = new Point(ox - dx * diag / 2, oy - dy * diag / 2);
      var p2 = new Point(ox + dx * diag / 2, oy + dy * diag / 2);
      var testLine = new Path({ segments: [p1, p2], insert: false });
      var ix;
      try { ix = region.getIntersections(testLine); } catch (e) { ix = []; }
      if (ix.length >= 2) {
        var offsets = ix.map(function (loc) { return loc.intersection.offset; }).sort(function (a, c) { return a - c; });
        for (var j = 0; j + 1 < offsets.length; j += 2) {
          var pa = testLine.getPointAt(offsets[j]);
          var pb = testLine.getPointAt(offsets[j + 1]);
          if (pa && pb) lines.push([pa.x, pa.y, pb.x, pb.y]);
        }
      }
      testLine.remove();
    }
    return lines;
  }
  // Team review (Phase 1): a dashed outline in the AUTHOR's own profile
  // color over every active (non-ghost) foreign-owned revision currently
  // in the rendered scene — the ghost itself already reads as "not current
  // work" via its own reduced opacity (set directly on the item at fork
  // time, tools.js forkIfForeignOwner), so this only needs to flag the
  // LIVE correction sitting on top of it. Non-destructive overlay, same
  // pattern as buildFSSelectionItems below — never touches the real paint.
  // Team review: one small pin per comment on the CURRENT frame, in the
  // author's profile color — a resolved comment renders faded (same
  // reduced-opacity convention as a revision ghost) so unresolved notes
  // stay the ones that visually grab attention. Never exported (export.js
  // renders straight from Paper's document, which never sees this overlay
  // layer at all — same reasoning as every other cursor/highlight builder
  // in this file).
  function buildCommentPinItems() {
    if (!state.comments || !state.comments.length) return [];
    var zs = 1 / view.zoom;
    var items = [];
    state.comments.forEach(function (cm) {
      if (cm.frame !== state.currentFrame) return;
      var col = cssColorToRgba(cm.authorColor || '#4a9eff', cm.resolved ? 0.35 : 1) || [74, 158, 255, 230];
      items.push(circleItem(cm.x, cm.y, 9 * zs, col, [255, 255, 255, cm.resolved ? 90 : 230], 1.5 * zs));
    });
    return items;
  }
  function buildRevisionOutlineItems() {
    if (!state.userProfile) return [];
    var out = [];
    var zs = 1 / view.zoom;
    for (var i = 0; i < state.layers.length; i++) {
      if (!layerIsEffectivelyVisible(i) || !userLayers[i]) continue;
      var children = userLayers[i].children;
      for (var s = 0; s < children.length; s++) {
        var c = children[s];
        if (!(c instanceof Path) || c.segments.length < 2) continue;
        if (!(c.data && c.data.revisionParentId && !c.data.isRevisionGhost)) continue;
        if (c.data.ownerId === state.userProfile.id) continue; // your own revisions don't need flagging to you
        if (state.revisionView === 'mine') continue; // filtered out of the scene above anyway, but be explicit
        var col = cssColorToRgba(c.data.ownerColor || '#ff8800', 1) || [255, 136, 0, 230];
        var segs = c.segments.map(function (seg) { return { point: [seg.point.x, seg.point.y], handleIn: [seg.handleIn.x, seg.handleIn.y], handleOut: [seg.handleOut.x, seg.handleOut.y] }; });
        out.push({
          segments: roundSegs(segs), closed: !!c.closed, fillColor: null,
          strokeColor: col, strokeWidth: ((c.strokeWidth || 2) + 3) * zs, dashPattern: [5 * zs, 4 * zs],
        });
      }
    }
    return out;
  }
  function buildFSSelectionItems() {
    if (state.tool !== 'fsselect' || !_fsSel || typeof fsHighlightPath !== 'function') return [];
    var hl = fsHighlightPath(_fsSel);
    if (!hl || !hl.segments || !hl.segments.length) { if (hl) hl.remove(); return []; }
    var segs = hl.segments.map(function (s) { return { point: [s.point.x, s.point.y], handleIn: [s.handleIn.x, s.handleIn.y], handleOut: [s.handleOut.x, s.handleOut.y] }; });
    var closed = !!hl.closed;
    var zs = 1 / view.zoom;
    if (_fsSel.kind === 'fill' || _fsSel.kind === 'fillregion') {
      var hatchLines = closed ? fsHatchClipLines(hl, 9 * zs, 45) : [];
      hl.remove();
      var items = [{ segments: roundSegs(segs), closed: closed, fillColor: [255, 152, 0, 45], strokeColor: [255, 152, 0, 230], strokeWidth: 2 * zs, dashPattern: [4 * zs, 3 * zs] }];
      hatchLines.forEach(function (ln) {
        items.push({
          segments: [{ point: [ln[0], ln[1]], handleIn: [0, 0], handleOut: [0, 0] }, { point: [ln[2], ln[3]], handleIn: [0, 0], handleOut: [0, 0] }],
          closed: false, fillColor: null, strokeColor: [255, 152, 0, 150], strokeWidth: 1 * zs,
        });
      });
      return items;
    }
    hl.remove();
    return [{ segments: roundSegs(segs), closed: closed, fillColor: null, strokeColor: [255, 152, 0, 230], strokeWidth: (( _fsSel.path.strokeWidth || 2) + 4) * zs, dashPattern: [5 * zs, 4 * zs] }];
  }

  // Set by eraser-bridge.js on every pointermove while the Eraser tool is
  // active (hover included, not just while actively erasing) — mirrors
  // eraseUpdateCursor()'s always-on cursor circle in tools.js, which is
  // likewise invisible under the opaque rust canvas once the beta engine is
  // on.
  var eraserCursorWorld = null;
  var eraserCursorRadius = null;
  // radius is optional — omitted on plain hover (no pressure sample yet),
  // defaulting to the nominal size; eraser-bridge.js passes the real
  // pressure-scaled radius while actively erasing so the cursor always
  // shows the width that's ACTUALLY about to be cut, matching the pressure
  // brush's own cursor convention (setPressureCursor below).
  function setEraserCursor(worldPt, radius) { eraserCursorWorld = worldPt; eraserCursorRadius = radius; }
  function buildEraserCursorItems() {
    if (state.tool !== 'eraser' || !eraserCursorWorld) return [];
    var r = eraserCursorRadius != null ? eraserCursorRadius : state.eraserSize / 2;
    // Rim was pure white — invisible against a white canvas background
    // (reported). The app's own blue accent stays visible on any bg.
    return [circleItem(eraserCursorWorld[0], eraserCursorWorld[1], r, [255, 255, 255, 31], [74, 158, 255, 230], 1 / view.zoom)];
  }

  // Set by draw-bridge.js on every pointermove while the Draw tool has
  // Pressure brush enabled (hover AND active drag alike, matching the
  // eraser cursor's always-on convention) — a small circle at the true
  // radius the NEXT sample would paint at, growing/shrinking live with
  // pressure so the user can feel the brush's dynamic range before/while
  // committing ink, the way Procreate/Photoshop's brush cursor does.
  var pressureCursorWorld = null, pressureCursorRadius = 0, pressureCursorForced = false;
  // `forced` (draw-bridge.js's alt+drag brush-size resize) bypasses the
  // vectorBrush gate below: that gate exists for the ALWAYS-ON hover
  // indicator, which is deliberately pressure-brush-only (a fixed-width
  // brush has no "next sample radius" to preview continuously) — but the
  // alt-drag gesture is a DIFFERENT, explicit "show me the new size while I
  // resize" preview that applies to every brush mode. Reported: alt-drag
  // resize on the plain (non-pressure) brush showed no circle at all, since
  // this function silently swallowed it via the same gate.
  function setPressureCursor(worldPt, radius, forced) { pressureCursorWorld = worldPt; pressureCursorRadius = radius; pressureCursorForced = !!forced; }
  function buildPressureCursorItems() {
    if (state.tool !== 'draw' || !pressureCursorWorld || !(state.vectorBrush || pressureCursorForced)) return [];
    // Rim was pure white — invisible against a white canvas background
    // (reported). The app's own blue accent stays visible on any bg.
    return [circleItem(pressureCursorWorld[0], pressureCursorWorld[1], pressureCursorRadius, [255, 255, 255, 40], [74, 158, 255, 220], 1 / view.zoom)];
  }

  // Set by pen-bridge.js on every pointermove while the Pen tool has an
  // in-progress path — mirrors the dashed rubber-band preview line from the
  // last placed anchor to the cursor in tools.js's onMouseMoveTool.
  var penPreviewWorld = null;
  function setPenPreview(worldPt) { penPreviewWorld = worldPt; }
  function buildPenPreviewItems() {
    if (state.tool !== 'pen' || typeof _pen === 'undefined' || !_pen.path) return [];
    var zs = 1 / view.zoom;
    var items = [];
    if (penPreviewWorld) {
      var last = _pen.path.lastSegment.point;
      items.push(lineItem([last.x, last.y], penPreviewWorld, [120, 170, 255, 153], 1 * zs));
    }
    // Anchors + tangent handles of the in-progress pen path (feedback #19)
    // — same visual language as the Subselection tool's node handles
    // (buildNodeHandleItems below): circles for anchors, thin guide line +
    // small square for each non-zero handle, all sized in 1/zoom so they
    // stay constant on screen.
    _pen.path.segments.forEach(function (s) {
      var pt = [s.point.x, s.point.y];
      var hi = [s.handleIn.x, s.handleIn.y], ho = [s.handleOut.x, s.handleOut.y];
      if (Math.hypot(hi[0], hi[1]) > 0.5) {
        var hiPt = [pt[0] + hi[0], pt[1] + hi[1]];
        items.push(lineItem(pt, hiPt, [120, 170, 255, 178], 1 * zs));
        items.push(rectItem(hiPt[0], hiPt[1], 3 * zs, [255, 255, 255, 255], [74, 158, 255, 255], 1 * zs));
      }
      if (Math.hypot(ho[0], ho[1]) > 0.5) {
        var hoPt = [pt[0] + ho[0], pt[1] + ho[1]];
        items.push(lineItem(pt, hoPt, [120, 170, 255, 178], 1 * zs));
        items.push(rectItem(hoPt[0], hoPt[1], 3 * zs, [255, 255, 255, 255], [74, 158, 255, 255], 1 * zs));
      }
      items.push(circleItem(pt[0], pt[1], 3.5 * zs, [255, 255, 255, 255], [74, 158, 255, 255], 1.2 * zs));
    });
    return items;
  }

  // Tween motion-arc handles: renderArcs() in tweens.js draws these into a
  // real Paper `arcLayer` (dashed CUBIC-bezier curve between two matched
  // strokes' centroids + two independent OUT/IN draggable handles, upgraded
  // 2026-07 from a single shared quadratic control point — camera.js's own
  // motion-path rig), same invisible-under-the-rust-canvas problem as every
  // other overlay above. Rebuilt here directly from the already-populated
  // `arcHandles` array (a tweens.js global — populated by the SAME
  // renderArcs() call select-bridge.js already triggers after any arc drag/
  // selection change), using the same 24-sample polyline approximation of
  // the cubic curve the original does (`cubicBez`, also a tweens.js
  // global). arcHandles now carries TWO entries per matched pair (one
  // `which:'out'`, one `which:'in'`) instead of one — grouped back into a
  // single curve+both-handles item set here so the curve draws once, not
  // twice.
  var ARC_COLORS = [
    [255, 107, 107], [78, 205, 196], [255, 230, 109],
    [162, 155, 254], [253, 121, 168], [0, 206, 201],
  ];
  function buildArcHandleItems() {
    if (typeof arcHandles === 'undefined' || !arcHandles.length) return [];
    var zs = 1 / view.zoom;
    var items = [];
    var byPair = {}, order = [];
    arcHandles.forEach(function (ah) {
      var k = ah.fA + '-' + ah.fB + '-' + ah.matchIdx;
      if (!byPair[k]) { byPair[k] = { ptA: ah.ptA, ptB: ah.ptB }; order.push(k); }
      byPair[k][ah.which] = [ah.handle.position.x, ah.handle.position.y];
    });
    order.forEach(function (k, i) {
      var p = byPair[k];
      var col = ARC_COLORS[i % ARC_COLORS.length];
      var out = p.out, inn = p['in'];
      var pts = [];
      for (var s = 0; s <= 24; s++) {
        var t = s / 24;
        pts.push({ point: [cubicBez(p.ptA[0], out[0], inn[0], p.ptB[0], t), cubicBez(p.ptA[1], out[1], inn[1], p.ptB[1], t)] });
      }
      items.push({ segments: roundSegs(pts), closed: false, fillColor: null, strokeColor: col.concat([153]), strokeWidth: 2 * zs });
      items.push(circleItem(p.ptA[0], p.ptA[1], 4 * zs, col.concat([204]), null, 0));
      items.push(circleItem(p.ptB[0], p.ptB[1], 4 * zs, col.concat([204]), null, 0));
      items.push(lineItem(p.ptA, out, col.concat([140]), 1 * zs));
      items.push(circleItem(out[0], out[1], 6 * zs, [255, 255, 255, 242], col.concat([255]), 2 * zs));
      items.push(lineItem(p.ptB, inn, col.concat([140]), 1 * zs));
      items.push(circleItem(inn[0], inn[1], 6 * zs, [255, 255, 255, 242], col.concat([255]), 2 * zs));
    });
    return items;
  }

  function buildNodeHandleItems() {
    if (typeof nodeEditTargetPath !== 'function') return [];
    var path = nodeEditTargetPath();
    if (!path) return [];
    var segs = nodeEditSegmentsData(path);
    var zs = 1 / view.zoom;
    var items = [];
    segs.forEach(function (s, i) {
      var pt = s.point, hi = s.handleIn, ho = s.handleOut;
      var hiLen = Math.hypot(hi[0], hi[1]), hoLen = Math.hypot(ho[0], ho[1]);
      if (hiLen > 0.5) {
        var hiPt = [pt[0] + hi[0], pt[1] + hi[1]];
        items.push(lineItem(pt, hiPt, [120, 170, 255, 178], 1 * zs));
        items.push(rectItem(hiPt[0], hiPt[1], 3 * zs, [255, 255, 255, 255], [74, 158, 255, 255], 1 * zs));
      }
      if (hoLen > 0.5) {
        var hoPt = [pt[0] + ho[0], pt[1] + ho[1]];
        items.push(lineItem(pt, hoPt, [120, 170, 255, 178], 1 * zs));
        items.push(rectItem(hoPt[0], hoPt[1], 3 * zs, [255, 255, 255, 255], [74, 158, 255, 255], 1 * zs));
      }
      var isSel = _nodeSel.indexOf(i) >= 0;
      var fc = isSel ? [255, 184, 108, 255] : [74, 158, 255, 255];
      items.push(circleItem(pt[0], pt[1], (isSel ? 5 : 4) * zs, fc, [255, 255, 255, 255], 1 * zs));
    });
    return items;
  }

  function syncViewport() {
    // Paper.js maps world->CSS pixels: css = (world - view.center) * zoom
    // + viewSize/2. The engine renders in DEVICE pixels (the canvas's
    // width/height attributes), so on HiDPI/Retina displays there's an
    // extra devicePixelRatio-style factor between the two — derived from
    // the actual canvas-vs-viewSize ratio rather than window.devicePixelRatio
    // so it stays correct if Paper is configured differently.
    var scale = engineW / view.viewSize.width;
    var z = view.zoom * scale;
    var panX = engineW / 2 - view.center.x * z;
    var panY = engineH / 2 - view.center.y * z;
    // Rotation/pivot were always plumbed through on the Rust side
    // (Viewport already has rotation/pivot fields, screen_to_world already
    // accounts for them) — this was the only remaining piece hardcoding
    // them to 0, so `state.canvasRotation` never had any visible effect.
    //
    // Viewport::transform() (engine.rs) composes:
    //   screen = pan + pivot + rotate(zoom*(world - pivot))
    // — i.e. `pivot` is read in WORLD space (matches its own doc comment:
    // "pass canvasW/2, canvasH/2"), and is ADDED BACK untouched after the
    // rotate+scale, not scaled itself. Feeding it the plain panX/panY above
    // is wrong the moment zoom != 1: expanding the formula at rotation=0
    // gives `pan + pivot*(1-z) + z*world`, not the `pan + z*world` those
    // panX/panY were actually derived for — a visible jump the instant
    // rotation went non-zero (confirmed live: world point at screen center
    // came back wildly off pivot after a 90° rotate before this fix).
    // Solving for the pan that keeps rotation=0 pixel-identical to before
    // (`pan_adjusted + pivot*(1-z) + z*world == pan + z*world`) gives
    // `pan_adjusted = pan + pivot*(z-1)`, applied below — this is exact
    // algebra, not a heuristic, so rotation=0 stays byte-for-byte the same
    // scene as before this feature existed, and rotation!=0 now genuinely
    // spins around the artboard center.
    var pivotWX = state.canvasW / 2, pivotWY = state.canvasH / 2;
    var panAdjX = panX + pivotWX * (z - 1);
    var panAdjY = panY + pivotWY * (z - 1);
    engine.set_viewport(panAdjX, panAdjY, z, state.canvasRotation || 0, pivotWX, pivotWY);
  }

  // Shared with renderWithOverlayItem/renderNow so all three call sites stay
  // in sync — canvasRotation was missing here entirely at first, so rotating
  // the stage via canvasRotation alone (no zoom/pan change alongside it)
  // never tripped the dirty-check and silently never re-rendered.
  function viewportKeyNow() { return view.zoom + '|' + view.center.x + ',' + view.center.y + '|' + (state.canvasRotation || 0); }
  var lastViewportKey = '';
  var lastSceneVersion = -1; // forces the very first tick to build+render regardless
  function tick() {
    if (!enabled || !engine) return;
    if (suspended) { rafId = requestAnimationFrame(tick); return; }
    try {
      // Dirty-check both the scene content and the viewport before paying
      // for a render — the first version re-rendered unconditionally on
      // EVERY rAF tick (60/s) even while completely idle (cursor hovering,
      // no drawing, no pan/zoom), which combined with Paper.js's own render
      // loop running underneath made the whole app feel laggy ("ça rame").
      // Only the two things that can actually change the picture — scene
      // content or viewport — are checked; everything else is a no-op skip.
      var viewportKey = viewportKeyNow();
      var viewportChanged = viewportKey !== lastViewportKey;
      // buildSceneJson() itself isn't free — it walks every live Paper.js
      // item on every layer and re-serializes them all, every single call.
      // window._sceneVersion (app.js: bumped by saveActiveLayerFrame/
      // loadFrame, which together bracket every tool action and frame/layer
      // navigation outside of an active drag — see their own comments) is a
      // near-free proxy for "might the scene have changed" — skip the
      // expensive rebuild entirely unless that counter moved or the
      // viewport did, rather than rebuilding on every idle tick just to
      // string-diff it away.
      var versionChanged = window._sceneVersion !== lastSceneVersion;
      if (viewportChanged || versionChanged) {
        var json = buildSceneJson();
        var sceneChanged = json !== lastSceneJson;
        if (viewportChanged || sceneChanged) {
          if (viewportChanged) { syncViewport(); lastViewportKey = viewportKey; }
          lastSceneJson = json;
          window.__lastSceneJson = json;
          engine.render(json);
        }
        lastSceneVersion = window._sceneVersion;
      }
    } catch (e) {
      console.error('[engine-bridge] render failed, disabling', e);
      setEnabled(false);
      return;
    }
    rafId = requestAnimationFrame(tick);
  }

  var resizeObserver = null;
  var paperCanvasEl = null;

  // Paper.js's own canvas is resized by app.js whenever the window/panel
  // layout changes (its width/height attributes track the container in
  // device pixels), but the engine's WebGPU surface + offscreen render
  // target were only ever sized ONCE, at ensureEngine() time — this is what
  // made the app-window resize bug: the rust canvas kept presenting at its
  // creation-time resolution while its CSS box stretched to fill the new
  // container size (blurry/stretched picture), AND screen_to_world (used by
  // draw-bridge.js) computed world coordinates using the now-stale
  // engineW/engineH against the canvas's current (already-changed)
  // getBoundingClientRect(), silently producing wrong coordinates — which is
  // also why an intercepted tool's live preview could render off-screen
  // after any resize. A ResizeObserver on the Paper canvas itself (the
  // authoritative source of the current device-pixel size) keeps both the
  // rust canvas's own width/height attributes AND the wasm-side surface in
  // sync any time it changes.
  function handleResize() {
    if (!engine || !paperCanvasEl) return;
    var w = paperCanvasEl.width, h = paperCanvasEl.height;
    // #canvas-area uses flex-basis:0% — mid-reflow (e.g. the instant the
    // DevTools panel is docked/undocked, or any other layout change that
    // spans more than one paint), the browser can report a transient 0×N (or
    // N×0) box before the flex layout settles on its final size. Rust's own
    // resize() also guards against 0, but calling it there anyway would
    // still burn a real WebGPU surface.configure() at an invalid size —
    // Dawn/WebGPU treats that as a hard validation error that poisons the
    // surface for the rest of the session (confirmed: "Could not create a
    // swapchain texture of size 0", reproduced by toggling DevTools). Bail
    // out entirely on a zero-sized read; the ResizeObserver fires again as
    // soon as layout finishes settling, with the real final size.
    if (w <= 0 || h <= 0) return;
    if (w === engineW && h === engineH) return;
    engineW = w; engineH = h;
    rustCanvas.width = w;
    rustCanvas.height = h;
    try {
      engine.resize(w, h);
    } catch (e) {
      console.error('[engine-bridge] resize failed, disabling', e);
      setEnabled(false);
      return;
    }
    lastSceneJson = ''; // force a full re-render at the new size
    lastViewportKey = '';
    lastSceneVersion = -1; // force tick() to actually rebuild, not just skip on an unchanged version
    invalidateOverlayBase();
  }

  async function ensureEngine() {
    if (engine) return true;
    if (!window.GeometryWasm || !window.GeometryWasm.ready) {
      showToast('Moteur Rust indisponible (wasm non chargé)');
      return false;
    }
    var paperCanvas = document.getElementById('drawing-canvas');
    paperCanvasEl = paperCanvas;
    // Same 0-size hazard as handleResize() below, but at creation time: a
    // 0×0 (or 0×N) surface.configure() during create_engine() is just as
    // fatal to the WebGPU surface as during a later resize.
    if (paperCanvas.width <= 0 || paperCanvas.height <= 0) {
      showToast('Moteur Rust: canvas pas encore prêt, réessaie');
      return false;
    }
    rustCanvas = document.createElement('canvas');
    rustCanvas.id = 'rust-canvas';
    // device-pixel size copied from the Paper canvas so world coordinates
    // line up 1:1; CSS position stacked directly over it
    engineW = paperCanvas.width;
    engineH = paperCanvas.height;
    rustCanvas.width = engineW;
    rustCanvas.height = engineH;
    rustCanvas.style.cssText = 'position:absolute;inset:0;width:100%;height:100%;pointer-events:none;display:none;';
    paperCanvas.parentNode.insertBefore(rustCanvas, paperCanvas.nextSibling);
    try {
      engine = await window.GeometryWasm.create_engine(rustCanvas, engineW, engineH);
      if (window.ResizeObserver) {
        resizeObserver = new ResizeObserver(handleResize);
        resizeObserver.observe(paperCanvas);
      }
      window.addEventListener('resize', handleResize);
      return true;
    } catch (e) {
      console.error('[engine-bridge] engine creation failed', e);
      showToast('Moteur Rust: échec WebGPU — ' + e);
      rustCanvas.remove();
      rustCanvas = null;
      return false;
    }
  }

  async function setEnabled(on, silent) {
    if (on && !(await ensureEngine())) return;
    enabled = on;
    if (rustCanvas) rustCanvas.style.display = on ? 'block' : 'none';
    // Paper's own canvas is fully hidden under the opaque rust canvas while
    // the engine is on, yet Paper kept RASTERIZING it on every view
    // invalidation (any pan/zoom/center change, any item mutation) — pure
    // duplicate work that scales with item count, and the actual dominant
    // cost on dab-heavy documents (measured: ~200ms per pan move at ~2600
    // items, vs ~6ms for the rust render of the same scene). The scene
    // graph stays fully live (hit-testing, bounds, exportSVG never needed
    // the raster), only the invisible repaint is disabled. Restored when
    // the engine is toggled off, where Paper's canvas is the visible one.
    if (typeof view !== 'undefined' && view) view.autoUpdate = !on;
    if (on) {
      if (!silent) showToast('Rendu: moteur Rust (vello/WebGPU)');
      tick();
    } else {
      cancelAnimationFrame(rafId);
      if (typeof view !== 'undefined' && view) view.update();
      if (engine && !silent) showToast('Rendu: Paper.js');
    }
  }

  // Screen (real client) coordinates -> world, via the SAME viewport the
  // bridge keeps synced — used by draw-bridge.js so an intercepted tool's
  // pointer events land at the exact spot the Rust canvas is showing.
  function screenToWorld(clientX, clientY) {
    var rect = rustCanvas.getBoundingClientRect();
    var sx = (clientX - rect.left) * (engineW / rect.width);
    var sy = (clientY - rect.top) * (engineH / rect.height);
    // engine.screen_to_world (Rust `Vec<f64>` via wasm-bindgen) comes back as
    // a Float64Array, not a plain Array — every caller here only ever
    // indexes it (w[0]/w[1], works fine on both), but JSON.stringify()
    // serializes a Float64Array as {"0":x,"1":y} instead of [x,y]. Any
    // overlay item built directly from this raw result (e.g. pen-bridge's
    // live rubber-band preview) silently sent malformed `point` fields to
    // the Rust renderer, which rejected the whole scene as a deserialize
    // error — caught by tick()'s try/catch, which then permanently disabled
    // the ENTIRE engine for the rest of the session (setEnabled(false)),
    // not just that one overlay. Converting to a real Array here fixes every
    // call site at once with no behavior change for existing w[0]/w[1] use.
    return Array.from(engine.screen_to_world(sx, sy));
  }

  // Renders the current persisted scene (same as a normal tick) plus one
  // extra "live" item appended on top — the in-progress stroke an
  // intercepted tool is building, which deliberately never touches Paper's
  // own scene graph until it's committed (see draw-bridge.js) so Paper.js
  // does zero work — no re-render — for the whole duration of the drag.
  //
  // The persisted part of the scene is CACHED as a pre-serialized JSON
  // prefix for the duration of the drag: since the document is untouched
  // until commit and the viewport doesn't move mid-draw, rebuilding +
  // re-serializing every path on every layer on EVERY pointermove was pure
  // waste — and stopped being merely wasteful once brush-texture presets
  // meant a single stroke can carry ~180 dab Paths (the reported "ça rame
  // avec les brush custom" — thousands of serP calls per mousemove). Keyed
  // on (_sceneVersion, viewportKey); invalidated by renderNow (the "scene
  // actually mutated mid-drag" path, e.g. the eraser) and resume().
  // String-splicing the overlay into the cached prefix (instead of
  // JSON.parse → push → re-stringify of the whole scene) keeps the per-move
  // cost proportional to the overlay alone, not the document.
  var overlayBasePrefix = null, overlayBaseVersion = -1, overlayBaseViewKey = '';
  function invalidateOverlayBase() { overlayBasePrefix = null; }
  // rAF coalescing: a stylus fires pointermove at 120-240Hz — far beyond
  // the 60Hz the display can show — and every one of those used to pay a
  // full render. Only the LAST overlay state per animation frame can ever
  // reach the screen anyway, so intermediate ones are stored and dropped.
  // resume() cancels any still-pending render so a stale overlay can't
  // paint over the freshly-committed stroke after the drag ends.
  var pendingOverlayItem = null, overlayRafId = 0;
  function renderWithOverlayItem(item) {
    pendingOverlayItem = item;
    if (overlayRafId) return;
    overlayRafId = requestAnimationFrame(function () {
      overlayRafId = 0;
      var it = pendingOverlayItem;
      pendingOverlayItem = null;
      if (it != null) renderOverlayNow(it);
    });
  }
  function renderOverlayNow(item) {
    if (!engine) return;
    syncViewport();
    var vk = viewportKeyNow();
    lastViewportKey = vk;
    if (overlayBasePrefix === null || overlayBaseVersion !== window._sceneVersion || overlayBaseViewKey !== vk) {
      var base = buildSceneJson(true); // '{"layers":[...]}' — always ends in ']}'
      overlayBasePrefix = base.slice(0, base.length - 2);
      overlayBaseVersion = window._sceneVersion;
      overlayBaseViewKey = vk;
    }
    // `item` may be a single item (all pre-existing callers) or an array —
    // the wire format (LayerIn.items: Vec<ItemIn>) always supported a list,
    // this just stopped hardcoding it to exactly one so callers like
    // draw-bridge.js can overlay extra items (e.g. endpoint markers)
    // alongside the in-progress stroke in the same render call.
    var overlayItems = Array.isArray(item) ? item : [item];
    // The volatile cursor overlays excluded from the cached base (see
    // buildSceneJson's skipVolatile) — re-collected fresh on every move so
    // the pressure/eraser cursor keeps tracking the pointer instead of
    // freezing at its drag-start position inside the cache.
    var volatileItems = buildEraserCursorItems().concat(buildPressureCursorItems(), buildPenPreviewItems());
    var json = overlayBasePrefix + ',' + JSON.stringify({ items: overlayItems })
      + (volatileItems.length ? ',' + JSON.stringify({ items: volatileItems }) : '') + ']}';
    lastSceneJson = ''; // force the next normal tick to re-diff post-commit
    lastSceneVersion = -1; // ditto — don't let an unchanged version skip that rebuild
    engine.render(json);
  }

  // Renders the current live scene immediately, no extra overlay item and
  // no dirty-check skip — used by select-bridge.js after mutating
  // Paper.js selection/geometry data directly (move/scale/rotate/marquee),
  // since buildSceneJson() already picks up the transform box/marquee/node
  // handles from that same live data on its own (see
  // buildTransformBoxItems/buildMarqueeItems above); unlike draw-bridge's
  // in-progress stroke, there's no separate overlay item to append here.
  // `viewportOnly` (opt-in per call site, viewtools-bridge pan/rotate): the
  // caller guarantees it changed NOTHING but the viewport since the last
  // render — the scene JSON is re-used verbatim and only the viewport
  // uniform is re-synced, skipping the full walk+serialize entirely. Safe
  // for pan/rotate (no scene item reads view.center or rotation; the
  // 1/view.zoom-sized overlay handles depend on ZOOM only), deliberately
  // NOT used by the zoom drag, whose zs-sized overlays genuinely change.
  // Never inferred automatically: select-bridge/eraser-bridge mutate
  // geometry without bumping _sceneVersion, so only the call site knows.
  var viewportRafId = 0;
  function renderNow(viewportOnly) {
    if (!engine) return;
    if (viewportOnly) {
      // Same rAF coalescing rationale as renderWithOverlayItem — pan/rotate
      // pointermoves outrun the display. The callback reads lastSceneJson at
      // FIRE time (not schedule time), so a full render landing in between
      // is never overwritten with something staler than itself.
      if (viewportRafId) return;
      viewportRafId = requestAnimationFrame(function () {
        viewportRafId = 0;
        if (!engine) return;
        syncViewport();
        lastViewportKey = viewportKeyNow();
        if (lastSceneJson) { engine.render(lastSceneJson); return; }
        var json2 = buildSceneJson();
        lastSceneJson = json2;
        window.__lastSceneJson = json2;
        engine.render(json2);
      });
      return;
    }
    if (viewportRafId) { cancelAnimationFrame(viewportRafId); viewportRafId = 0; } // a full render supersedes any pending viewport-only one
    syncViewport();
    lastViewportKey = viewportKeyNow();
    invalidateOverlayBase(); // scene may have mutated without a version bump (eraser/select drags) — never let a stale drag-cache survive this
    var json = buildSceneJson();
    lastSceneJson = json;
    window.__lastSceneJson = json;
    engine.render(json);
    lastSceneVersion = window._sceneVersion; // tick() must not redo this render on the next rAF
  }

  // For native video LAYERS/reference only (native-video-bridge.js):
  // registering fresh pixels under a STABLE image id ('nv:<i>' or
  // 'ref:native') changes what's on screen WITHOUT changing the scene
  // JSON one byte — the JSON only carries the id string. Two problems
  // this caused before this function existed:
  //   1. tick()'s own dirty-check string-diffs the rebuilt JSON against
  //      lastSceneJson and finds NO difference for a video-only frame
  //      change, so it SKIPS engine.render() entirely — the new pixels
  //      sit in the Rust-side image HashMap but the canvas never redraws
  //      them until some unrelated real scene edit finally forces a
  //      render. This is why video frames sometimes appeared to freeze.
  //   2. Calling the general renderNow() to work around that pays for a
  //      full buildSceneJson() walk of every Paper.js item on every
  //      layer, every video frame (30x/s) — pure waste when the item
  //      list provably didn't change.
  // Fix: skip buildSceneJson() entirely, reuse the last built JSON
  // verbatim, and render unconditionally (bytes changed even though the
  // JSON string didn't) — then sync tick()'s own version bookkeeping so
  // it doesn't redundantly rebuild+diff on the very next rAF.
  function renderImageOnly() {
    if (!engine) return;
    if (!lastSceneJson) { renderNow(); return; } // nothing built yet — first frame
    engine.render(lastSceneJson);
    lastSceneVersion = window._sceneVersion;
  }

  window.SMEngineBridge = {
    setEnabled: setEnabled,
    isEnabled: function () { return enabled; },
    screenToWorld: screenToWorld,
    renderWithOverlayItem: renderWithOverlayItem,
    renderNow: renderNow,
    renderImageOnly: renderImageOnly,
    setEraserCursor: setEraserCursor,
    setPressureCursor: setPressureCursor,
    setPenPreview: setPenPreview,
    registerImagePixels: registerImagePixels,
    registerImageRaw: registerImageRaw,
    hasImage: function (id) { return !!registeredImageIds[id]; },
    // Call suspend() at the start of an intercepted drag and resume() at the
    // end — see the `suspended` var above for why: without this, tick()'s
    // own unconditional rAF loop races renderWithOverlayItem and erases the
    // live overlay a frame after every pointermove draws it.
    suspend: function () { suspended = true; },
    resume: function () {
      suspended = false;
      invalidateOverlayBase();
      if (overlayRafId) { cancelAnimationFrame(overlayRafId); overlayRafId = 0; pendingOverlayItem = null; }
    },
  };

  // Rust/vello is now the default renderer (no more opt-in checkbox) — per
  // the C7 architecture split (Rust: rendering + heavy geometry, JS: data
  // model/events/UI/persistence), Paper.js stays the live document/source
  // of truth underneath (see every *-bridge.js), this just decides which
  // canvas actually paints the picture. Auto-enables on load with a bounded
  // retry: the Paper canvas may not have a real device-pixel size yet on
  // the very first attempt (still 0×0 mid-layout), which ensureEngine()
  // rejects rather than risk a poisoned zero-size WebGPU surface — a few
  // retries a beat apart reliably lands after layout settles, silently (no
  // toast) since this isn't a user-initiated action.
  function autoEnable(attemptsLeft) {
    setEnabled(true, true).then(function () {
      if (!enabled && attemptsLeft > 0) setTimeout(function () { autoEnable(attemptsLeft - 1); }, 200);
    });
  }
  function init() {
    autoEnable(15);
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', init);
  else init();
})();
