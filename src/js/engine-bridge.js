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
    var a = Math.round(255 * (opacity !== undefined ? opacity : 1));
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

  function buildSceneJson() {
    var layers = [];
    for (var i = 0; i < state.layers.length; i++) {
      if (!state.layers[i].visible || !userLayers[i]) { layers.push({ items: [] }); continue; }
      var children = userLayers[i].children;
      var items = [];
      for (var s = 0; s < children.length; s++) {
        var c = children[s];
        if (c instanceof Raster) {
          var imageId = registerRasterIfNeeded(c);
          if (!imageId) continue;
          var rb = c.bounds; // display rect — Paper's Raster is center-positioned, bounds gives top-left directly
          items.push({
            image: { imageId: imageId, x: rb.x, y: rb.y, width: rb.width, height: rb.height, opacity: c.opacity !== undefined ? c.opacity : 1 },
          });
          continue;
        }
        if (!(c instanceof Path) || c.segments.length < 2) continue;
        var sd = serP(c);
        var op = sd.opacity !== undefined ? sd.opacity : 1;
        items.push({
          segments: sd.segments,
          closed: !!sd.fillColor,
          fillColor: cssColorToRgba(sd.fillColor, op),
          strokeColor: cssColorToRgba(sd.strokeColor, op),
          strokeWidth: sd.strokeWidth || 1,
          strokeCap: sd.strokeCap,
          strokeJoin: sd.strokeJoin,
          miterLimit: sd.miterLimit,
          dashPattern: sd.dashArray,
          dashOffset: sd.dashOffset,
          paintOrder: sd.paintOrder,
        });
      }
      layers.push({ items: items });
    }
    // artboard background as the bottom item of a synthetic bottom layer,
    // mirroring drawStage()'s background rect
    layers.unshift({
      items: [{
        segments: [
          { point: [0, 0] }, { point: [state.canvasW, 0] },
          { point: [state.canvasW, state.canvasH] }, { point: [0, state.canvasH] },
        ],
        closed: true,
        fillColor: cssColorToRgba(state.canvasBg, 1),
        strokeColor: null,
        strokeWidth: 1,
      }],
    });
    // Onion ghosts sit right above the background but BELOW the current
    // frame's real artwork (layers[1..]) — a faint reference, never
    // obscuring what's actually being drawn on top of it.
    var onionItems = buildOnionSkinItems();
    if (onionItems.length) layers.splice(1, 0, { items: onionItems });
    var nodeItems = buildNodeHandleItems();
    if (nodeItems.length) layers.push({ items: nodeItems });
    var xformItems = buildTransformBoxItems();
    if (xformItems.length) layers.push({ items: xformItems });
    var marqueeItems = buildMarqueeItems();
    if (marqueeItems.length) layers.push({ items: marqueeItems });
    var eraserItems = buildEraserCursorItems();
    if (eraserItems.length) layers.push({ items: eraserItems });
    var pressureItems = buildPressureCursorItems();
    if (pressureItems.length) layers.push({ items: pressureItems });
    var penItems = buildPenPreviewItems();
    if (penItems.length) layers.push({ items: penItems });
    var arcItems = buildArcHandleItems();
    if (arcItems.length) layers.push({ items: arcItems });
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
      if (!(c instanceof Path) || c.segments.length < 2) return;
      var sd = serP(c);
      var op = sd.opacity !== undefined ? sd.opacity : 1;
      items.push({
        segments: sd.segments,
        closed: !!sd.fillColor,
        fillColor: cssColorToRgba(sd.fillColor, op),
        strokeColor: cssColorToRgba(sd.strokeColor, op),
        strokeWidth: sd.strokeWidth || 1,
        strokeCap: sd.strokeCap,
        strokeJoin: sd.strokeJoin,
      });
    });
    return items;
  }
  function buildOnionSkinItems() {
    if (!state.onionSkin || state.playing) return [];
    return onionLayerItems(onionPrevLayer).concat(onionLayerItems(onionNextLayer));
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
    return { segments: segments, closed: true, fillColor: fillColor, strokeColor: strokeColor, strokeWidth: strokeWidth };
  }
  function rectItem(cx, cy, halfSize, fillColor, strokeColor, strokeWidth) {
    var segments = [
      { point: [cx - halfSize, cy - halfSize] },
      { point: [cx + halfSize, cy - halfSize] },
      { point: [cx + halfSize, cy + halfSize] },
      { point: [cx - halfSize, cy + halfSize] },
    ];
    return { segments: segments, closed: true, fillColor: fillColor, strokeColor: strokeColor, strokeWidth: strokeWidth };
  }
  function lineItem(fromPt, toPt, strokeColor, strokeWidth) {
    return {
      segments: [{ point: fromPt }, { point: toPt }],
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
    return { segments: segments, closed: true, fillColor: fillColor, strokeColor: strokeColor, strokeWidth: strokeWidth };
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
    if (state.tool !== 'select' || !selectedPaths.length || typeof xformSelBounds !== 'function') return [];
    var b = xformSelBounds();
    if (!b) return [];
    var zs = 1 / view.zoom;
    var items = [];
    items.push(boundsRectItem(b.left, b.top, b.right, b.bottom, null, [74, 158, 255, 204], 1 * zs));
    var midX = b.left + b.width / 2, midY = b.top + b.height / 2;
    var corners = {
      nw: [b.left, b.top], n: [midX, b.top], ne: [b.right, b.top],
      e: [b.right, midY], se: [b.right, b.bottom], s: [midX, b.bottom],
      sw: [b.left, b.bottom], w: [b.left, midY],
    };
    Object.keys(corners).forEach(function (k) {
      var p = corners[k];
      items.push(rectItem(p[0], p[1], 3.5 * zs, [255, 255, 255, 255], [74, 158, 255, 255], 1.2 * zs));
    });
    var rotOff = 20 * zs;
    var topCenter = [midX, b.top];
    var rotPos = [midX, b.top - rotOff];
    items.push(lineItem(topCenter, rotPos, [74, 158, 255, 204], 1 * zs));
    items.push(circleItem(rotPos[0], rotPos[1], 5 * zs, [255, 255, 255, 255], [74, 158, 255, 255], 1.2 * zs));
    return items;
  }
  function buildMarqueeItems() {
    if (!_marquee.active || !_marquee.rect) return [];
    var b = _marquee.rect.bounds;
    return [boundsRectItem(b.left, b.top, b.right, b.bottom, [74, 158, 255, 20], [74, 158, 255, 230], 1 / view.zoom)];
  }

  // Set by eraser-bridge.js on every pointermove while the Eraser tool is
  // active (hover included, not just while actively erasing) — mirrors
  // eraseUpdateCursor()'s always-on cursor circle in tools.js, which is
  // likewise invisible under the opaque rust canvas once the beta engine is
  // on.
  var eraserCursorWorld = null;
  function setEraserCursor(worldPt) { eraserCursorWorld = worldPt; }
  function buildEraserCursorItems() {
    if (state.tool !== 'eraser' || !eraserCursorWorld) return [];
    return [circleItem(eraserCursorWorld[0], eraserCursorWorld[1], state.eraserSize / 2, [255, 255, 255, 31], [255, 255, 255, 230], 1 / view.zoom)];
  }

  // Set by draw-bridge.js on every pointermove while the Draw tool has
  // Pressure brush enabled (hover AND active drag alike, matching the
  // eraser cursor's always-on convention) — a small circle at the true
  // radius the NEXT sample would paint at, growing/shrinking live with
  // pressure so the user can feel the brush's dynamic range before/while
  // committing ink, the way Procreate/Photoshop's brush cursor does.
  var pressureCursorWorld = null, pressureCursorRadius = 0;
  function setPressureCursor(worldPt, radius) { pressureCursorWorld = worldPt; pressureCursorRadius = radius; }
  function buildPressureCursorItems() {
    if (state.tool !== 'draw' || !state.vectorBrush || !pressureCursorWorld) return [];
    return [circleItem(pressureCursorWorld[0], pressureCursorWorld[1], pressureCursorRadius, [255, 255, 255, 40], [255, 255, 255, 220], 1 / view.zoom)];
  }

  // Set by pen-bridge.js on every pointermove while the Pen tool has an
  // in-progress path — mirrors the dashed rubber-band preview line from the
  // last placed anchor to the cursor in tools.js's onMouseMoveTool.
  var penPreviewWorld = null;
  function setPenPreview(worldPt) { penPreviewWorld = worldPt; }
  function buildPenPreviewItems() {
    if (state.tool !== 'pen' || !penPreviewWorld || typeof _pen === 'undefined' || !_pen.path) return [];
    var last = _pen.path.lastSegment.point;
    return [lineItem([last.x, last.y], penPreviewWorld, [120, 170, 255, 153], 1 / view.zoom)];
  }

  // Tween motion-arc handles: renderArcs() in tweens.js draws these into a
  // real Paper `arcLayer` (dashed quadratic-bezier curve between two
  // matched strokes' centroids + a draggable control-point handle), same
  // invisible-under-the-rust-canvas problem as every other overlay above.
  // Rebuilt here directly from the already-populated `arcHandles` array
  // (a tweens.js global — populated by the SAME renderArcs() call select-
  // bridge.js already triggers after any arc drag/selection change), using
  // the same 24-sample polyline approximation of the quadratic curve the
  // original does (`qBez`, also a tweens.js global).
  var ARC_COLORS = [
    [255, 107, 107], [78, 205, 196], [255, 230, 109],
    [162, 155, 254], [253, 121, 168], [0, 206, 201],
  ];
  function buildArcHandleItems() {
    if (typeof arcHandles === 'undefined' || !arcHandles.length) return [];
    var zs = 1 / view.zoom;
    var items = [];
    arcHandles.forEach(function (ah, i) {
      var col = ARC_COLORS[i % ARC_COLORS.length];
      var ac = [ah.handle.position.x, ah.handle.position.y];
      var pts = [];
      for (var s = 0; s <= 24; s++) {
        var t = s / 24;
        pts.push({ point: [qBez(ah.ptA[0], ac[0], ah.ptB[0], t), qBez(ah.ptA[1], ac[1], ah.ptB[1], t)] });
      }
      items.push({ segments: pts, closed: false, fillColor: null, strokeColor: col.concat([153]), strokeWidth: 2 * zs });
      items.push(circleItem(ah.ptA[0], ah.ptA[1], 4 * zs, col.concat([204]), null, 0));
      items.push(circleItem(ah.ptB[0], ah.ptB[1], 4 * zs, col.concat([204]), null, 0));
      items.push(circleItem(ac[0], ac[1], 7 * zs, [255, 255, 255, 242], col.concat([255]), 2 * zs));
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
    engine.set_viewport(panX, panY, z, 0, 0, 0);
  }

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
      var viewportKey = view.zoom + '|' + view.center.x + ',' + view.center.y;
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
    if (on) {
      if (!silent) showToast('Rendu: moteur Rust (vello/WebGPU)');
      tick();
    } else {
      cancelAnimationFrame(rafId);
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
    return engine.screen_to_world(sx, sy);
  }

  // Renders the current persisted scene (same as a normal tick) plus one
  // extra "live" item appended on top — the in-progress stroke an
  // intercepted tool is building, which deliberately never touches Paper's
  // own scene graph until it's committed (see draw-bridge.js) so Paper.js
  // does zero work — no re-render — for the whole duration of the drag.
  function renderWithOverlayItem(item) {
    if (!engine) return;
    syncViewport();
    lastViewportKey = view.zoom + '|' + view.center.x + ',' + view.center.y;
    var scene = JSON.parse(buildSceneJson());
    // `item` may be a single item (all pre-existing callers) or an array —
    // the wire format (LayerIn.items: Vec<ItemIn>) always supported a list,
    // this just stopped hardcoding it to exactly one so callers like
    // draw-bridge.js can overlay extra items (e.g. endpoint markers)
    // alongside the in-progress stroke in the same render call.
    scene.layers.push({ items: Array.isArray(item) ? item : [item] });
    var json = JSON.stringify(scene);
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
  function renderNow() {
    if (!engine) return;
    syncViewport();
    lastViewportKey = view.zoom + '|' + view.center.x + ',' + view.center.y;
    var json = buildSceneJson();
    lastSceneJson = json;
    engine.render(json);
  }

  window.SMEngineBridge = {
    setEnabled: setEnabled,
    isEnabled: function () { return enabled; },
    screenToWorld: screenToWorld,
    renderWithOverlayItem: renderWithOverlayItem,
    renderNow: renderNow,
    setEraserCursor: setEraserCursor,
    setPressureCursor: setPressureCursor,
    setPenPreview: setPenPreview,
    // Call suspend() at the start of an intercepted drag and resume() at the
    // end — see the `suspended` var above for why: without this, tick()'s
    // own unconditional rAF loop races renderWithOverlayItem and erases the
    // live overlay a frame after every pointermove draws it.
    suspend: function () { suspended = true; },
    resume: function () { suspended = false; },
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
