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
  // Adaptive live-preview render resolution (2026-08, feedback #60 part 2 —
  // "l'échantillonnage pour la preview... temps réel mais de bonne qualité
  // en fonction de la config de l'ordinateur"). engineW/engineH are ALREADY
  // just "however many actual device pixels the WebGPU surface has" —
  // decoupled from the canvas element's CSS display size by construction
  // (screenToWorld's own `engineW / rect.width` ratio, syncViewport's
  // `scale = engineW / view.viewSize.width` — both comments explicitly call
  // out "an extra devicePixelRatio-style factor between the two"). Nothing
  // outside this file even reads engineW/engineH (grepped). So reducing
  // them by a `_previewRenderScale` factor below 1 — fewer texels for the
  // GPU to shade, same CSS box, same effect_zoom (deliberately EXCLUDES
  // this ratio already, exactly like it excludes real DPR, for the same
  // reason: an effect's radius must stay CSS/document-relative, not texel-
  // relative) — needs ZERO changes anywhere else: composite_scene,
  // screen_to_world, and every hit-test already work in whatever pixel
  // space engineW/engineH currently describe. Verified live: content
  // position and effect radii both stay visually correct at any scale,
  // same reasoning (and same pivot/pan algebra) already proven for
  // resizeEngineOffscreenAtScale's export-side supersampling above.
  var _previewRenderScale = 1.0;
  var PREVIEW_SCALE_FLOOR = 0.5;
  // One-shot startup calibration, not continuous monitoring: rAF-interval
  // probes are explicitly documented elsewhere in this codebase as
  // unreliable (aliasing against the engine's own coalesced render calls,
  // contradictory results run to run) — measuring the real render() JS-call
  // duration over the first several ACTUAL renders after engine creation
  // (tick()'s own dirty-check already means every call here is real GPU
  // work, never an idle no-op) is a steadier signal, and deciding ONCE
  // avoids fighting/oscillating quality up and down mid-session.
  var _calibSamples = [];
  var _calibDone = false;
  var CALIB_SAMPLE_COUNT = 12;
  var CALIB_SLOW_MS = 14; // ~60fps budget (16.6ms) minus headroom for other per-frame JS
  var CALIB_FAST_MS = 8;  // comfortably fast — no reduction needed at all
  function recordCalibSample(ms) {
    if (_calibDone) return;
    _calibSamples.push(ms);
    if (_calibSamples.length < CALIB_SAMPLE_COUNT) return;
    _calibDone = true;
    var avg = _calibSamples.reduce(function (a, b) { return a + b; }, 0) / _calibSamples.length;
    if (avg > CALIB_SLOW_MS) {
      // One step down is enough for the common case (a render that's ~2x
      // over budget needs roughly half the pixels, and area scales with
      // the square of the linear factor) — a hard-slow machine still gets
      // real relief; not iterating further keeps this predictable rather
      // than hunting for an exact fps target.
      setPreviewRenderScale(avg > CALIB_SLOW_MS * 1.8 ? PREVIEW_SCALE_FLOOR : 0.75);
    } else if (avg <= CALIB_FAST_MS) {
      // Already at 1.0 by default — nothing to do, just documents the
      // "comfortably fast, stay at full quality" branch explicitly.
    }
  }
  function setPreviewRenderScale(scale) {
    scale = Math.max(PREVIEW_SCALE_FLOOR, Math.min(1.0, scale));
    if (scale === _previewRenderScale) return;
    _previewRenderScale = scale;
    if (engine && paperCanvasEl) applyEngineSize(paperCanvasEl.width, paperCanvasEl.height);
  }
  // Shared by handleResize/ensureEngine so the renderScale multiply lives
  // in exactly one place (CLAUDE.md §3) — ALWAYS derived from the Paper
  // canvas's own current device-pixel size, never compounded onto a
  // previous already-scaled engineW/engineH.
  var _nativeEngineW = 0, _nativeEngineH = 0; // last paperCanvas size applyEngineSize saw, PRE-scale
  function applyEngineSize(nativeW, nativeH) {
    _nativeEngineW = nativeW; _nativeEngineH = nativeH;
    var w = Math.max(1, Math.round(nativeW * _previewRenderScale));
    var h = Math.max(1, Math.round(nativeH * _previewRenderScale));
    engineW = w; engineH = h;
    rustCanvas.width = w; rustCanvas.height = h;
    engine.resize(w, h);
    lastSceneJson = ''; lastViewportKey = ''; lastSceneVersion = -1;
    invalidateOverlayBase();
  }
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
  // ---- RETAINED PATH STORE (2026-07-28) -------------------------------
  // The `register_image`/imageId pattern, extended to vector geometry.
  //
  // Why: an item's segments were re-serialized into the scene JSON, re-parsed
  // by serde and rebuilt into a BezPath on EVERY render even when the
  // geometry had not changed since the previous frame. Measured: 2000 vector
  // strokes scrubbed at 31fps where 2000 rasters — six numbers and an id per
  // item — reached 61fps. 4000 strokes x 24 segments is ~1.15M coordinates
  // pushed across the JS/WASM boundary per frame.
  //
  // The identity trick is the same one that makes images work. For images it
  // is the `src` string; here it is the STORED STROKE DICT's object identity.
  // getEffectiveStrokes hands back the very same dict objects for a frame
  // until an edit replaces the whole array (f.strokes = ..., see
  // saveActiveLayerFrame) — so a dict that is `===` to the one we registered
  // from is a promise that the geometry is unchanged. desP stamps the dict it
  // built from onto `path.data.__engineSrcDict` (app.js).
  //
  // That covers reload-from-storage, but NOT live mutation: sculpt/erase/
  // drag mutate the Paper item in place without touching the stored dict.
  // Hence the _changed hook below, which nulls the stamp on any GEOMETRY
  // change so a mutated item falls back to inline serialization until the
  // next desP rebuild re-stamps it.
  //
  // Scope of v1, deliberately narrow: a pathRef is emitted only for items
  // whose geometry reaches the renderer untransformed (no per-vertex offset,
  // no element/layer/parent Motion matrix). Those transforms rewrite every
  // coordinate, so the stored path would be the wrong shape. That is exactly
  // the measured case (Animation 2D scrub / drawing-heavy documents). The
  // natural follow-up is an `Affine` alongside the ref for elMat/motionMat/
  // parentChain — they ARE affine around a pivot — leaving only per-vertex
  // offsets on the inline path. Not built here: one correctness surface at a
  // time.
  var _pathKeyByDict = new WeakMap();
  // THE assumption this whole mechanism rests on: a stored stroke dict's
  // object identity IS its geometry identity. That holds only for layer kinds
  // whose getEffectiveStrokes branch returns the STORED array. Three branches
  // synthesize instead — `symbolId` (a component instance with a camera or
  // symMatrix runs every stroke through cloneStrokeForTransform, minting
  // fresh dicts on EVERY call), `montageId` (StoryBoard-resolved strokes) and
  // `lfsGroup` (concatenated sub-layer channels). Registering from those grew
  // the store without bound: measured on the reference project, 0 -> 25 -> 50
  // -> 75 entries across identical scrub passes over the same frames.
  //
  // A "only register a dict seen twice" heuristic was tried first and is NOT
  // enough: seen-twice means "the scene was built twice while this dict was
  // alive", which a second render between two loadFrames satisfies trivially.
  // The gate below is explicit instead — and because CLAUDE.md §1 is exactly
  // about a new branch being missed by one reader, `_registerCap` is a hard
  // backstop so any future synthesizing branch degrades into "no speedup"
  // rather than "unbounded memory".
  var _registerCap = 250000;
  var _registerCount = 0;
  var _capWarned = false;
  var _pathRefSeq = 0;
  var _pathRefsEnabled = false;   // flipped on only by a PASSING self-test
  var _geomFlagMask = 0;
  var _retireQueue = [];
  var _pathRegistry = (typeof FinalizationRegistry === 'function')
    ? new FinalizationRegistry(function (key) { _retireQueue.push(key); })
    : null;

  // Paper's ChangeFlag bit values are version-specific and not part of its
  // public API, so they are DISCOVERED rather than hardcoded: mutate a
  // throwaway path's geometry and record the flags, then change only its
  // style and subtract those. If anything about that probe looks wrong the
  // whole feature stays off and the renderer keeps its existing behaviour —
  // a wrong mask would either null every stamp (slow but correct) or, far
  // worse, MISS a real geometry edit and paint stale geometry.
  function installGeometryHook() {
    if (_pathRefsEnabled || !window.paper || !paper.Path || !paper.Item) return;
    // TWO prototypes, established by probing rather than assumed: `Path` has
    // its OWN _changed, and Paper's class system captures `base` as a direct
    // function reference at definition time — so patching Item.prototype
    // alone never intercepts a Path's changes (measured: zero callbacks).
    // CompoundPath and Raster have no own _changed and inherit Item's, so
    // both prototypes must be wrapped to cover every item type a layer holds.
    var protos = [];
    if (Object.prototype.hasOwnProperty.call(paper.Path.prototype, '_changed')) protos.push(paper.Path.prototype);
    if (Object.prototype.hasOwnProperty.call(paper.Item.prototype, '_changed')) protos.push(paper.Item.prototype);
    if (!protos.length) return;
    // The probe MUST run on an INSERTED item: an `insert:false` path fires no
    // _changed at all (measured), which is exactly how the first version of
    // this probe silently produced a zero mask.
    var scratch = new paper.Layer();
    var probe = new paper.Path({ insert: true });
    var probeFlags = 0;
    var origs = protos.map(function (pr) { return pr._changed; });
    protos.forEach(function (pr, i) {
      pr._changed = function (flags) { if (this === probe) probeFlags |= flags; return origs[i].apply(this, arguments); };
    });
    var geomBits = 0, styleBits = 0;
    try {
      probeFlags = 0;
      probe.add(new paper.Point(0, 0));
      probe.add(new paper.Point(10, 10));
      probe.firstSegment.point.x = 5;
      geomBits = probeFlags;
      probeFlags = 0;
      probe.fillColor = '#ff0000';
      probe.opacity = 0.5;
      probe.strokeWidth = 3;     // carries its own STROKE bit on top of STYLE
      styleBits = probeFlags;
    } catch (e) {
      protos.forEach(function (pr, i) { pr._changed = origs[i]; });
      probe.remove(); scratch.remove();
      console.warn('[engine-bridge] retained-path probe threw, feature disabled', e);
      return;
    }
    protos.forEach(function (pr, i) { pr._changed = origs[i]; });
    probe.remove(); scratch.remove();
    // Style changes carry a shared "needs redraw" bit that geometry changes
    // carry too; subtracting the style set leaves only the geometry-only bits
    // (measured on this build: geometry 41, style 449 -> mask 40).
    _geomFlagMask = geomBits & ~styleBits;
    if (!_geomFlagMask) {
      console.warn('[engine-bridge] retained paths off: no geometry-only change flag found');
      return;
    }
    // Real hook. Hot path — runs on every mutation of every item — so the
    // cheap bitmask test comes first, and `_data` is read directly rather
    // than through Paper's `data` getter, which LAZILY CREATES an object on
    // every access (the same builder-not-accessor trap as raster.canvas,
    // CLAUDE.md §5bis).
    var liveOrigs = protos.map(function (pr) { return pr._changed; });
    protos.forEach(function (pr, i) {
      pr._changed = function (flags) {
        if (flags & _geomFlagMask) {
          if (this._data && this._data.__engineSrcDict) this._data.__engineSrcDict = null;
          // Second consumer of the same signal: loadFrame skips rebuilding a
          // layer whose stored strokes are unchanged, and must NOT do that if
          // the live items were edited without being saved back (a sculpt on
          // a non-keyframe, say). Marking the owning Paper layer here is what
          // makes that skip safe. `_parent` direct, not `.parent` — same
          // lazy-getter caution as `_data` above.
          var par = this._parent;
          if (par) par._smGeomDirty = true;
        }
        return liveOrigs[i].apply(this, arguments);
      };
    });
    _pathRefsEnabled = true;
    // Tells loadFrame (app.js) that the dirty signal it relies on is live.
    // Without the hook installed there is no way to know a layer was
    // edited, so loadFrame must keep rebuilding unconditionally.
    window.__smGeomDirtyHookInstalled = true;
  }

  // Returns an engine key for this item's geometry, or null to fall back to
  // inline segments. `segs` must already be the rounded array that WOULD have
  // been sent, so a registered path and an inline one are byte-identical
  // geometry (CLAUDE.md §3: the two paths must not drift).
  // ---- MOTION CHAIN AS ONE AFFINE (2026-07-28, retained-path v2) --------
  // Every Motion matrix in the item -> element -> layer -> parent chain is an
  // affine around a pivot, so the whole chain collapses into a single 2x3 the
  // engine can compose with its view transform. That lets an ANIMATED shape
  // keep using its registered path instead of falling back to re-serializing
  // every coordinate, which was v1's biggest gap (Motion was entirely
  // excluded). Per-vertex path offsets are the one non-affine piece and are
  // screened out by the caller.
  //
  // Convention: [a,b,c,d,e,f] as SVG/kurbo, x' = a*x + c*y + e, y' = b*x + d*y + f.
  // Mirrors transformSegments (motion.js) EXACTLY — scale in the pivot's local
  // frame, rotate around the pivot, translate last:
  //   A = T(dx,dy) . T(P) . R(rot) . S(sx,sy) . T(-P)
  // If those two ever drift the picture silently changes only for animated
  // shapes, so they are verified against each other by a pixel A/B, not by
  // reading (CLAUDE.md §3).
  function affineFromMotion(m, pivot) {
    var rad = (m.rot || 0) * Math.PI / 180, cs = Math.cos(rad), sn = Math.sin(rad);
    var a = cs * m.sx, b = sn * m.sx, c = -sn * m.sy, d = cs * m.sy;
    return [a, b, c, d,
      pivot.x + (m.dx || 0) - a * pivot.x - c * pivot.y,
      pivot.y + (m.dy || 0) - b * pivot.x - d * pivot.y];
  }
  // m2 applied AFTER m1.
  function affineMul(m2, m1) {
    return [
      m2[0] * m1[0] + m2[2] * m1[1],
      m2[1] * m1[0] + m2[3] * m1[1],
      m2[0] * m1[2] + m2[2] * m1[3],
      m2[1] * m1[2] + m2[3] * m1[3],
      m2[0] * m1[4] + m2[2] * m1[5] + m2[4],
      m2[1] * m1[4] + m2[3] * m1[5] + m2[5],
    ];
  }
  // Uniform scale only. The inline path scales stroke width by
  // (|sx|+|sy|)/2 while the engine strokes THROUGH the affine, and those two
  // agree exactly only when sx == sy — a non-uniform chain would draw a
  // subtly different (elliptical-pen) stroke, so it falls back to inline
  // rather than quietly changing the picture.
  function motionChainUniform(elMat, motionMat, parentChain) {
    var EPS = 1e-9;
    if (elMat && Math.abs(elMat.sx - elMat.sy) > EPS) return false;
    if (motionMat && Math.abs(motionMat.sx - motionMat.sy) > EPS) return false;
    for (var i = 0; i < parentChain.length; i++) {
      var pm = parentChain[i].mat;
      if (Math.abs(pm.sx - pm.sy) > EPS) return false;
    }
    return true;
  }

  // Lookup only — must stay free of any serialization, because the whole
  // point is to answer "already registered?" BEFORE paying for serP().
  // A first attempt kept serP+roundSegs unconditional and only skipped the
  // JSON bytes: measured 36fps -> 33fps, i.e. slightly WORSE, because serP
  // walking the Paper path is the dominant cost and the WeakMap lookup was
  // pure addition. The saving only exists if the serialization is skipped.
  function existingPathRef(item) {
    if (!_pathRefsEnabled || !engine) return null;
    var d = item._data && item._data.__engineSrcDict;
    return d ? (_pathKeyByDict.get(d) || null) : null;
  }
  function pathRefFor(item, segs, closed) {
    if (!_pathRefsEnabled || !engine || !engine.register_path) return null;
    var dict = item._data && item._data.__engineSrcDict;
    if (!dict) return null;
    var key = _pathKeyByDict.get(dict);
    if (key) return key;
    if (!segs.length) return null;
    if (_registerCount >= _registerCap) {
      if (!_capWarned) { _capWarned = true; console.warn('[engine-bridge] retained-path cap reached; new geometry stays inline'); }
      return null;
    }
    key = 'p' + (++_pathRefSeq);
    var flat = new Float64Array(segs.length * 6);
    for (var i = 0; i < segs.length; i++) {
      var s = segs[i], o = i * 6;
      flat[o] = s.point[0]; flat[o + 1] = s.point[1];
      if (s.handleIn) { flat[o + 2] = s.handleIn[0]; flat[o + 3] = s.handleIn[1]; }
      if (s.handleOut) { flat[o + 4] = s.handleOut[0]; flat[o + 5] = s.handleOut[1]; }
    }
    engine.register_path(key, flat, !!closed);
    _registerCount++;
    _pathKeyByDict.set(dict, key);
    if (_pathRegistry) _pathRegistry.register(dict, key);
    return key;
  }

  // Flushed once per render rather than per collection callback — retiring is
  // pure bookkeeping and a batched JSON array is one boundary crossing.
  function flushRetiredPaths() {
    if (!_retireQueue.length || !engine || !engine.retire_paths) return;
    var batch = _retireQueue;
    _retireQueue = [];
    try { engine.retire_paths(JSON.stringify(batch)); } catch (e) { /* non-fatal */ }
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
    // ORDER MATTERS. `raster.canvas` is not an accessor, it is a BUILDER:
    // Paper allocates a canvas and drawImages the source into it on first
    // access, per Raster OBJECT — and loadFrame rebuilds every Raster on
    // every frame. Touching it before the already-registered check meant a
    // scene of N rasters paid N canvas allocations + N drawImage calls per
    // frame to re-derive pixels the GPU had held since the first frame.
    // Measured (2026-07-28, 2000 bitmap items): 24fps target delivered at
    // 4.4fps, with 16 main-thread long tasks of 150-370ms across 3s — none
    // of it attributable to any function in the play loop, because the cost
    // was inside this one property read.
    //
    // The id comes from raster.data.src alone (engineIdFor, memoised per
    // source string), so it costs nothing and needs no decoded canvas.
    // Checking it first also removes a one-frame flash: a rebuilt Raster
    // whose image is already on the GPU no longer has to wait for its own
    // `loaded` flag before the engine can draw it.
    var id = engineIdFor(raster);
    if (!id) return null;
    if (registeredImageIds[id]) { _touchImage(id); return id; }
    if (!raster.loaded || !raster.canvas) return null; // not decoded yet — try again next tick
    var cv = raster.canvas;
    var ctx = cv.getContext('2d');
    var pixels = ctx.getImageData(0, 0, cv.width, cv.height).data;
    engine.register_image(id, pixels, cv.width, cv.height);
    registeredImageIds[id] = true;
    _noteImageRegistered(id, cv.width, cv.height);
    return id;
  }

  // ---- ENGINE IMAGE ID (2026-07-28) ----
  // The id handed to the engine used to BE the raster's data URL. The engine
  // only ever uses it as a HashMap key — it never reads it — but every scene
  // item carried the whole base64 string, so buildSceneJson emitted it once
  // per visible raster on EVERY render. Measured on a real project: 24
  // rasters produced a 4.6MB scene JSON and a 9.3ms full render, which is
  // what made a Motion drag (one full render per pointermove) unusable while
  // the same drag in Animation 2D stayed smooth on its cached base prefix.
  //
  // A short content-derived id keeps every property that mattered: identical
  // pixels still collapse to one key, so the GPU texture cache and
  // registeredImageIds keep deduplicating exactly as before. Namespaced
  // 'i1:' because the engine's id space is FLAT and already holds
  // 'nv:<layer>' and the reference-bridge's 'ref:*' — a bare hash could
  // collide with one of those and swap a video frame for artwork.
  //
  // Memoised per distinct source string, not per raster: loadFrame rebuilds
  // every Raster object each tick, so recomputing would hash megabytes of
  // base64 per frame — exactly the cost being removed. Collisions are
  // eliminated rather than made unlikely: on a hash hit the stored source is
  // compared and a suffix added if it differs, so two images can never
  // silently become one.
  var _engineIdBySrc = new Map(), _engineIdSrc = new Map();
  function _hashSrc(str) {
    var h1 = 0x811c9dc5, h2 = 0x01000193;
    for (var i = 0; i < str.length; i++) {
      var c = str.charCodeAt(i);
      h1 ^= c; h1 = (h1 * 0x01000193) >>> 0;
      h2 = (h2 ^ c) >>> 0; h2 = (h2 * 0x85ebca6b) >>> 0;
    }
    return str.length.toString(36) + '-' + h1.toString(36) + '-' + h2.toString(36);
  }
  function engineIdFor(raster) {
    var src = raster && raster.data && raster.data.src;
    if (!src) return null;
    var known = _engineIdBySrc.get(src);
    if (known) return known;
    var base = 'i1:' + _hashSrc(src), id = base, n = 1;
    while (_engineIdSrc.has(id) && _engineIdSrc.get(id) !== src) { n++; id = base + '#' + n; }
    _engineIdBySrc.set(src, id); _engineIdSrc.set(id, src);
    return id;
  }

  // ---- BITMAP-BRUSH TRIM (2026-08-21) ----
  // A bitmap-brush stroke's ink is ONE baked Raster covering the whole
  // stroke, so the per-dab reveal used for the VECTOR texture presets can't
  // trim it (see bakeTrimmed's own comment in bitmap-brush.js for the
  // measured before/after). Instead: trim the ANCHOR's centerline, re-bake
  // the texture along just that window, and hand the engine the new pixels.
  //
  // Cached on the window, because buildSceneJson runs on EVERY frame and a
  // bake is a full canvas stamp pass — without this an untouched static trim
  // would re-bake 60x/second. The key rounds the window to 0.1% so an
  // ANIMATED trim re-bakes only when the value actually moves, and the id is
  // derived from that key so identical windows reuse the GPU upload too
  // (registerImagePixels is idempotent per id, same contract liveRestamp
  // relies on). One entry per anchor — an animated trim overwrites its own
  // slot each time the window changes rather than growing without bound.
  var _bmbTrimCache = Object.create(null);
  function bitmapTrimmedImage(li, anchorItem, anchorStrokeId, frameIdx) {
    var spec = anchorItem && anchorItem.data && anchorItem.data.bitmapBrushSpec;
    if (!spec || !window.SMBitmapBrush || !SMBitmapBrush.bakeTrimmed || !window.SMMotion) return undefined;
    var win = SMMotion.trimWindowAt(li, anchorStrokeId, frameIdx);
    if (!win) return undefined; // no trim on this stroke — caller keeps the normal path
    var key = anchorStrokeId + '|' + Math.round(win.start * 10) + '|' + Math.round(win.end * 10) + '|' + Math.round((win.offset || 0) * 10);
    var hit = _bmbTrimCache[anchorStrokeId];
    if (hit && hit.key === key) { if (hit.id) _touchImage(hit.id); return hit.value; }
    // Trim the anchor's own geometry exactly like the vector path does, then
    // re-stamp along the result. serP is NOT used here: this needs the live
    // anchor's segments in the shape applyTrimFor expects, nothing else off
    // the serialized dict.
    var segs = anchorItem.segments.map(function (s) {
      return { point: [s.point.x, s.point.y], handleIn: [s.handleIn.x, s.handleIn.y], handleOut: [s.handleOut.x, s.handleOut.y] };
    });
    var trimmed = SMMotion.applyTrimFor(li, anchorStrokeId, segs, !!anchorItem.closed, frameIdx);
    var bake = (trimmed && trimmed.segments && trimmed.segments.length >= 2)
      ? SMBitmapBrush.bakeTrimmed(trimmed.segments, trimmed.closed, spec)
      : null;
    var value = null; // null = window collapsed, draw nothing
    var id = null;
    if (bake) {
      id = 'bmbtrim:' + _hashSrc(key);
      registerImagePixels(id, bake.canvas);
      value = { id: id, rect: { x: bake.minX, y: bake.minY, width: bake.w, height: bake.h, rotation: 0 } };
    }
    _bmbTrimCache[anchorStrokeId] = { key: key, value: value, id: id };
    return value;
  }

  // A Raster's DISPLAY rect + its own rotation, for the engine's image
  // item (2026-07 — image items now carry `rotation`, engine.rs ImageRef).
  // c.bounds is the AXIS-ALIGNED envelope: correct while the raster is
  // unrotated, but wrong (inflated) the moment the select tool's rotate
  // ring spins it — the un-rotated rect must come from position/size/
  // scaling instead, with the spin reported separately. Paper's own
  // matrix decomposition (`matrix.rotation`, degrees) is the source of
  // truth for the accumulated angle; scaling is abs()'d since a mirror
  // flip (negative scale) isn't representable in the rect+angle model —
  // known simplification, same class as registerRasterIfNeeded's own.
  function rasterImageRect(c) {
    var rot = (c.matrix && c.matrix.rotation) || 0;
    if (!rot) { var rb = c.bounds; return { x: rb.x, y: rb.y, width: rb.width, height: rb.height, rotation: 0 }; }
    var sw = Math.abs(c.scaling.x) * c.width, sh = Math.abs(c.scaling.y) * c.height;
    return { x: c.position.x - sw / 2, y: c.position.y - sh / 2, width: sw, height: sh, rotation: rot };
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
  // ---- BOUNDED IMAGE STORE (2026-07-28) --------------------------------
  // The engine's image map was unbounded by design — its own comment said
  // "cached for the engine's whole lifetime". That is fine for a drawing
  // document with a handful of imported rasters, and untenable for the
  // footage side of the app: a 1000-frame 1920x1080 sequence is 8.3GB of
  // decoded RGBA8, and nothing ever dropped a byte of it.
  //
  // Eviction is driven from JS, not the engine, for the same reason the
  // retained path store is: this side knows what the scene being rendered
  // actually references, and it can always re-upload (the pixels come back
  // from the Paper Raster's canvas, or from the video/reference bridge's own
  // per-frame push). An engine-side LRU would have to guess, and a wrong
  // guess makes an image silently vanish from the picture.
  //
  // Policy: least-recently-USED (as in, last emitted into a scene), never
  // touching anything the CURRENT build references, down to a byte budget.
  var _imgBytes = new Map();       // id -> decoded bytes held by the engine
  var _imgLastUsed = new Map();    // id -> build tick when last emitted
  var _imgUsedThisBuild = null;    // Set, non-null only during a scene build
  var _imgTick = 0;
  var _imgBudgetBytes = 384 * 1024 * 1024;
  var _imgEvictions = 0;

  function _noteImageRegistered(id, w, h) {
    _imgBytes.set(id, w * h * 4);
    // Counts as a USE, not merely a registration: an image is uploaded
    // because the frame being built needs it, so it must join
    // _imgUsedThisBuild or it becomes an eviction candidate the instant it
    // arrives — measured, it evicted everything including what was on screen.
    _touchImage(id);
  }
  // Called wherever an image id is emitted into the scene being built.
  function _touchImage(id) {
    _imgLastUsed.set(id, ++_imgTick);
    if (_imgUsedThisBuild) _imgUsedThisBuild.add(id);
  }
  function _imgTotalBytes() {
    var t = 0;
    _imgBytes.forEach(function (b) { t += b; });
    return t;
  }
  // Run at the END of a scene build, when `_imgUsedThisBuild` is exactly the
  // set of ids this frame draws. Anything else is a candidate, oldest first.
  function enforceImageBudget() {
    var total = _imgTotalBytes();
    if (total <= _imgBudgetBytes || !engine || !engine.retire_images) return;
    var cands = [];
    _imgBytes.forEach(function (bytes, id) {
      if (_imgUsedThisBuild && _imgUsedThisBuild.has(id)) return;   // on screen now
      cands.push({ id: id, t: _imgLastUsed.get(id) || 0, b: bytes });
    });
    cands.sort(function (a, b) { return a.t - b.t; });
    var drop = [];
    for (var i = 0; i < cands.length && total > _imgBudgetBytes; i++) {
      drop.push(cands[i].id); total -= cands[i].b;
    }
    if (!drop.length) return;
    try { engine.retire_images(JSON.stringify(drop)); } catch (e) { return; }
    for (var k = 0; k < drop.length; k++) {
      // Dropping the JS-side gate is what makes the next use re-upload —
      // registerRasterIfNeeded/registerCachedImage both early-out on it.
      delete registeredImageIds[drop[k]];
      _imgBytes.delete(drop[k]); _imgLastUsed.delete(drop[k]);
    }
    _imgEvictions += drop.length;
  }

  function registerCachedImage(id, source) {
    if (!engine || registeredImageIds[id]) return;
    var p = drawableToPixels(source);
    engine.register_image(id, p.pixels, p.w, p.h);
    registeredImageIds[id] = true;
    _noteImageRegistered(id, p.w, p.h);
  }
  function registerImagePixels(id, source) {
    if (!engine) return;
    var p = drawableToPixels(source);
    engine.register_image(id, p.pixels, p.w, p.h);
    registeredImageIds[id] = true;
    _noteImageRegistered(id, p.w, p.h);
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
    _noteImageRegistered(id, w, h);
    return true;
  }

  // Converts a layer's `effects` array (JS shape: {type, enabled, p1..p4},
  // built by effects-panel.js) into the Rust-facing shape (EffectIn,
  // engine.rs: {effectType, enabled, p1..p4} — camelCase via
  // #[serde(rename_all = "camelCase")]). Shared by both the ordinary-layer
  // and effect/adjustment-layer push sites below — same stack, same wire
  // shape, only the SOURCE texture composite_scene runs it on differs.
  // frameIdx (2026-07-25): effect parameters can be keyframed, so the value
  // sent to the renderer is the one AT THIS FRAME, not the effect's static
  // field. effectParamValueAt (effects-panel.js) returns the static value
  // untouched when a parameter isn't keyed, so an unanimated effect is
  // bit-identical to before. Defaults to the current frame, which is what the
  // live view wants; export passes the frame it is writing.
  // Export renders frame by frame through renderFrameToPixelsPNG, which calls
  // loadFrame(f) — and loadFrame does NOT move state.currentFrame. Reading the
  // current frame here would therefore have given EVERY exported image the
  // effect value of whatever frame the editor happened to be sitting on, i.e.
  // a frozen effect in the file and a moving one on screen. This override is
  // set around the export's own buildSceneJson call.
  var _fxFrameOverride = null;
  function sceneEffectsOf(ld, frameIdx, _guard) {
    var f = (frameIdx != null) ? frameIdx
          : (_fxFrameOverride != null ? _fxFrameOverride : state.currentFrame);
    var at = window.effectParamValueAt || function (e, k) { return e[k]; };
    // "Instance Effect" (Van Dijk 5.2): a layer can borrow another layer's
    // whole effects stack live, instead of copy-pasting it and then having
    // two copies to keep in sync. Resolved HERE rather than by duplicating
    // the data, so the source's own keyframed parameters drive the borrower
    // at the same frame — that is the entire point over a copy.
    //
    // The borrowed stack comes FIRST: its own effects then stack on top,
    // which matches how you'd read it in the panel (inherited base, local
    // additions). _guard stops a cycle (A borrows B, B borrows A) at one
    // hop instead of blowing the stack.
    var inherited = [];
    if (ld.effectsFrom && !_guard && window.SMMotion && SMMotion.ensureLayerUid) {
      for (var li = 0; li < state.layers.length; li++) {
        var src = state.layers[li];
        if (src !== ld && src.layerUid === ld.effectsFrom) {
          inherited = sceneEffectsOf(src, frameIdx, true);
          break;
        }
      }
    }
    return inherited.concat((ld.effects || []).map(function (e) {
      var out = { effectType: e.type, enabled: !!e.enabled,
               p1: at(e, 'p1', f), p2: at(e, 'p2', f), p3: at(e, 'p3', f), p4: at(e, 'p4', f),
               // p5..p8 (2026-08, "possibilité de sortir plus de
               // paramètres d'effets") — undefined on every effect entry
               // predating this (every built-in EFFECT_PARAM_CONFIG type,
               // any shader-library effect saved before its own p5..p8
               // param() calls existed) reads as `undefined` through
               // `at()` here, JSON-serializes to nothing, and
               // engine.rs's `#[serde(default)] p5..p8: Option<f32>`
               // deserializes that as None → 0.0 at the Rust side, the
               // same "missing field, not an error" contract p1..p4
               // already had before this.
               p5: at(e, 'p5', f), p6: at(e, 'p6', f), p7: at(e, 'p7', f), p8: at(e, 'p8', f) };
      // Zoom-compensate any "spatial" param of a shipped shader-library
      // effect (2026-07-29 fix, "un effet twirl qui bouge en fonction du
      // zoom du canvas") — engine.rs's run_one_effect explicitly skips this
      // for every "custom:" effect (it can't know a generic p1..p4's
      // meaning), but this library's own param() definitions DO know, via
      // the `spatial` flag (shader-effects-library.js) — same reasoning/
      // direction as run_one_effect's own `* z` for the built-in effects.
      if (typeof e.type === 'string' && e.type.indexOf('custom:') === 0 && window.SMSHADER_EFFECTS) {
        var libId = e.type.slice('custom:'.length);
        var def = SMSHADER_EFFECTS.filter(function (d) { return d.id === libId; })[0];
        if (def) {
          def.params.forEach(function (p) {
            if (p.spatial && out[p.key] != null) out[p.key] = out[p.key] * view.zoom;
          });
        }
      }
      return out;
    }));
  }

  // excludeGhosts (2026-07 audit): the live editing view legitimately wants
  // review ghosts visible under the default 'all' revisionView (a reviewer
  // needs to SEE a frozen ghost to click Accept/Reject on it) — this must
  // stay false for every normal render call. Export paths (renderFrameToPixelsPNG,
  // the effects-enabled export route) need the opposite: a rendered file
  // should never contain a ghost regardless of the current viewing mode,
  // same invariant export.js's own exportBuildFrame already enforces for
  // the plain (non-effects) export path — this closes the same gap for the
  // WGPU-effects one, which reuses this function instead of exportBuildFrame.
  function buildSceneJson(skipVolatile, excludeGhosts, renderContext) {
    renderContext = renderContext || {};
    // Opened here and closed at the return below: while a build is in flight
    // this collects every image id the frame actually draws, which is what
    // makes eviction safe (nothing on screen is ever a candidate).
    _imgUsedThisBuild = new Set();
    var renderFrame = renderContext.frame != null ? renderContext.frame
      : (_fxFrameOverride != null ? _fxFrameOverride : state.currentFrame);
    var includeEditorOverlays = renderContext.includeEditorOverlays !== false;
    // Transparent-background export (2026-08, feedback: "le halo n'a pas
    // d'alpha", twice — the literal alpha channel, not the look). The
    // Export panel's "Fond transparent (alpha)" checkbox was honored ONLY
    // by the Paper.js export path (exportBuildFrame's `if(!alpha)` skips
    // its bg rect); the ENGINE path — the one taken whenever any effect is
    // in play, i.e. exactly when you'd export a flare as an element —
    // never received the flag at all (exportRenderPNGsToDir passed `alpha`
    // to exportFrameDataURL but not to renderFrameToPixelsPNG), and the bg
    // rect below hardcoded its alpha to 1. So a flare exported for
    // compositing silently came out on an opaque canvas-colored plate.
    // Live preview toggle (2026-08-27, "il manque un bouton pour afficher
    // ou pas le fond en alpha") — explicit renderContext.alphaBg (export
    // call sites, which pass their own dialog checkbox value) always
    // wins; the ordinary live-render call sites never set this field at
    // all, so they fall through to the persistent live-preview flag.
    var alphaBg = renderContext.alphaBg !== undefined ? !!renderContext.alphaBg : !!state.previewAlphaBg;
    // Live "show transparency" checkerboard (2026-08-27 revision — the
    // original approach tried to make the on-screen <canvas> itself
    // composite against a CSS backdrop behind it, which turned out to be
    // blocked: the WebGPU surface isn't configured for alpha compositing
    // against the page (wgpu::CompositeAlphaMode, set in Rust's own
    // surface.configure(), not reachable from JS) — confirmed both in this
    // preview and live in the real app: the canvas stayed opaque white
    // either way. Drawing the checkerboard as real scene CONTENT instead
    // sidesteps that entirely — no Rust/canvas-config change needed, same
    // guaranteed-to-render path as every other shape in the scene.
    // Export must NEVER get this: a real transparent-PNG export
    // (renderContext.alphaBg explicitly passed) still needs true alpha=0
    // pixels, not a baked-in checkerboard — only the AMBIENT live-preview
    // fallback (no explicit renderContext.alphaBg at all) draws it.
    var showAlphaChecker = renderContext.alphaBg === undefined && !!state.previewAlphaBg;
    var layers = [];
    // StoryBoard montage preview (storyboard.js, 2026-07): when the node
    // space has an active montage, the canvas shows THAT montage's frame
    // at its own playhead instead of the document's layers — the preview
    // strokes live in storyboard.js's dedicated service Paper layer (same
    // pattern as ghostAllLayer), read through the SAME onionLayerItems
    // item builder every other stroke-data consumer uses (CLAUDE.md §1:
    // no new parallel serialization path).
    var sbPreview = (state.appMode === 'storyboard' && window.SMStoryboard) ? SMStoryboard.getPreviewLayer() : null;
    // Track matte by uid (2026-07-31): userLayerEntries[i] = the ONE wire
    // entry that IS state.layers[i]'s own main slot (never a motion-blur
    // ghost sample). Recorded at all four push sites of the loop below so
    // the final pass before JSON.stringify can resolve each layer's
    // matteSourceLayerUid to a concrete wire-array index via OBJECT
    // IDENTITY (layers.indexOf) — immune to the bg unshift, onion/ghost
    // splices, and any future overlay insertions, unlike any index math
    // done mid-loop.
    var userLayerEntries = [];
    // Order (2026-08) — cheap up-front check so an untouched document (the
    // overwhelming default) pays nothing for the stable-sort right after
    // this loop below.
    var _anyLayerOrder = window.SMMotion ? SMMotion.anyLayerHasOrder() : false;
    if (sbPreview) {
      layers.push({ items: onionLayerItems(sbPreview) });
    } else
    for (var i = 0; i < state.layers.length; i++) {
      if (!layerIsEffectivelyVisible(i) || !userLayers[i]) { layers.push(userLayerEntries[i] = { items: [] }); continue; }
      // Null layer (2026-07, Motion) — pure organizational/pivot layer,
      // never painted (AE's "Null Object"), same "no content, no paint"
      // shape as an invisible layer above, but still emitted as its OWN
      // stack slot (unlike an invisible layer it's never actually hidden —
      // other layers can still parent to it via SMMotion's existing
      // parentLayerUid/parentChainMats mechanism, which only needs the
      // layer to exist at some index, not to draw anything).
      if (state.layers[i].isNullLayer) { layers.push(userLayerEntries[i] = { items: [] }); continue; }
      // Guide layer (2026-08, AE feature audit 8.6) — a real layer object
      // (rotatable/parentable/keyable, unlike a classic ruler-drag guide),
      // same "no content, no paint" shape as Null right above. The actual
      // guide LINE is drawn separately, as an editor-only overlay
      // (buildGuideLayerItems, below) — never part of the real/exported
      // scene, same convention as safety zones/perspective guides.
      if (state.layers[i].isGuideLayer) { layers.push(userLayerEntries[i] = { items: [] }); continue; }
      // Effect (adjustment) layer (2026-07, Motion; effects stack rewrite
      // 2026-07) — never paints its own content either (ld.frames/strokes
      // are ignored on purpose, matching AE's "Adjustment Layer" toggle),
      // but DOES carry isEffectLayer + its `effects` stack so engine.rs's
      // composite_scene applies each enabled entry to everything already
      // composited below it — see that function's is_effect_layer branch.
      if (state.layers[i].isEffectLayer) {
        layers.push(userLayerEntries[i] = { items: [], isEffectLayer: true, effects: sceneEffectsOf(state.layers[i]) });
        continue;
      }
      var children = userLayers[i].children;
      var items = [];
      // Vector masks (2026-08, AE-style "Mask" — see geometry-wasm's
      // engine.rs LayerIn::masks doc comment for the combine algorithm).
      // Any Path tagged data.isMask never renders as normal content — it's
      // pulled out into this layer-scoped list instead, with a forced
      // solid-white/no-stroke item (only geometry matters engine-side) so
      // its actual on-canvas paint stays whatever the user set it to for
      // editing, without affecting the clip. maskFeather is a v1 shared-
      // per-layer value (max across this layer's own masks), not per-mask
      // — see the mask-feature audit's deliberate scope note.
      var layerMasks = [];
      var layerMaskFeather = 0;
      // Motion mode (motion.js): a keyed position/rotation/scale/opacity
      // transform for this layer at the CURRENT frame, applied ONLY to the
      // JSON items below — never to userLayers[i] itself (see motion.js's
      // header comment on why: mutating the live Paper.js layer would get
      // baked into the next saveActiveLayerFrame() permanently). Null (the
      // overwhelmingly common case — no motion on this layer) skips the
      // per-item transform pass entirely below.
      var motionMat = (window.SMMotion && children.length) ? SMMotion.layerMotionAt(i, renderFrame) : null;
      // Pivot = auto bounds center + the layer's Anchor Point offset
      // (motionMat.ax/ay) — see motion.js's layerMotionAt header comment.
      var motionPivot = motionMat ? { x: userLayers[i].bounds.center.x + motionMat.ax, y: userLayers[i].bounds.center.y + motionMat.ay } : null;
      // Layer parenting (motion.js's parentLayerUid/parentChainMats,
      // 2026-07): every ancestor's OWN layer-level transform, immediate
      // parent first — applied AFTER this layer's own motionMat, same
      // nesting order as elMat-then-motionMat above one level further out.
      // Empty array (the common case, no parent) makes the per-item loop
      // below a no-op cost.
      var parentChain = window.SMMotion ? SMMotion.parentChainMats(i, renderFrame) : [];
      // 3D layer (2026-07-28, Grease-Pencil-style — see motion.js's
      // make3DProjector/project3DSegments doc comment) — the layer-level
      // motionMat/parentChain above get REPLACED by a per-VERTEX 3D
      // projector (applied inside the per-item loop below, right where
      // motionMat's own transformSegments call already lived) rather than
      // a single affine matrix, so suppress them here to their neutral/
      // no-op state: every downstream per-item consumer already treats
      // null/[] as "no transform", so this is the one place that needs
      // changing rather than every scattered call site. Per-ELEMENT motion
      // (elMat) is untouched — a shape animating inside a 3D layer still
      // works, composed before the layer's own 3D projection. Parenting
      // INTO or OUT OF a 3D layer is explicitly out of scope for this pass
      // (composing a 2D affine parent transform with this layer's own
      // nonlinear 3D projection is exactly the "nested 3D" complexity the
      // plan flagged) — a parented 3D layer simply ignores its parent's
      // transform for now, not silently wrong in a way that's hard to
      // notice. Images (Raster items) are also out of scope this pass —
      // the projector below only ever applies to items with `.segments`
      // (vector paths); an image inside a 3D layer renders unprojected.
      var is3D = !!state.layers[i].threeD;
      var project3D = null;
      // Per-clone 3D projector cache (2026-07-30, "en 3D aussi avec ID de
      // chaque cloner") — a duplicator+3D layer's clones can each carry
      // their own extra positionZ/rotationX/rotationY delta on
      // data.dup3D (applyLayerDuplicator, app.js). Building a fresh
      // make3DProjector per VERTEX would be wasteful; this caches one per
      // unique dupIndex instead, reused by every stroke belonging to that
      // same clone (built lazily below, in the per-item loop, the first
      // time each dupIndex is actually seen). Stays null/unused for every
      // ordinary 3D layer (no duplicator) or 2D duplicator (no threeD) —
      // zero extra cost for the overwhelmingly common cases.
      var dup3DProjectorCache = null;
      if (is3D) {
        motionMat = null; motionPivot = null; parentChain = [];
        var q3dBounds = userLayers[i].bounds;
        // Multi-parent crossfade (2026-07-30) — a 3D layer ignores
        // parentChain entirely (just above), so this is the ONLY path a 3D
        // layer's 2 parents (if it has both) ever reach it through:
        // blendedParentContributionFor3D returns null for every OTHER
        // case (no parents, exactly one parent — same "3D layers don't
        // parent" boundary as before this feature), so this is a no-op
        // for the overwhelmingly common case.
        var parent3D = (window.SMMotion && SMMotion.blendedParentContributionFor3D) ? SMMotion.blendedParentContributionFor3D(i, renderFrame) : null;
        if (window.SMMotion && q3dBounds) project3D = SMMotion.make3DProjector(state.layers[i], q3dBounds, renderFrame, state.canvasW, state.canvasH, parent3D);
        if (state.layers[i].duplicator) dup3DProjectorCache = {};
      }
      // Brush-texture companions (isBrushTextureCopy — bitmap raster or
      // vector dab group) don't get their own Elements row in Motion
      // (motion.js's layerElements folds them into their anchor's, "merge
      // trait et fond quand ils font partie d'une même shape") — so at
      // render time their per-element Motion transform must resolve
      // through the ANCHOR's strokeId, not their own (which the companion
      // may not even have, having never been individually keyed). One
      // pass over children building brushGroupId -> anchor strokeId,
      // reused below instead of a lookup per companion.
      // Only layer kinds whose getEffectiveStrokes branch returns the STORED
      // stroke array can back a retained path — see the store's comment.
      // duplicator: 4th synthesizing case — getEffectiveStrokesRendered
      // (app.js) allocates fresh clone dicts per call, so dict identity
      // (the store's one invariant) is meaningless for these too.
      var layerRetainable = !state.layers[i].symbolId && !state.layers[i].montageId && !state.layers[i].lfsGroup && !state.layers[i].duplicator;
      var brushAnchorStrokeId = null;
      // Texture-brush dab reveal (2026-08-20) — a dab has no arc-length/
      // position field of its own (applyBrushTexture's dab.data is just
      // {isBrushTextureCopy,brushGroupId}, tools.js), but buildBrushDabs
      // walks the anchor's centerline strictly start-to-end and stamps them
      // in that same order — so a dab's position among its OWN group is
      // still a faithful (if coarse) proxy for "how far along the stroke"
      // without adding a new persisted field (CLAUDE.md §1's "new tag
      // needs checking everywhere" cost, avoided entirely). dabOrdinal maps
      // each dab item -> its 0..1 fraction within its group, read below to
      // decide whether Trim Paths should draw it at all.
      // ⚠️ layer.children order is the REVERSE of creation order, not a
      // match for it: every dab calls `dab.insertAbove(basePath)`
      // (applyBrushTexture, tools.js) against the SAME fixed basePath
      // reference rather than the previously-inserted dab, so each new dab
      // lands immediately after the anchor, ahead of every dab inserted
      // before it — the LAST dab stamped (end of stroke) ends up FIRST in
      // children right after the anchor. Confirmed live: an un-flipped
      // idx/(n-1) revealed the stroke's END first on trimStart=0/trimEnd=40
      // instead of its start. Flipped below (1 - idx/(n-1)) so ordinal 0
      // is the stroke's start and 1 is its end, matching trimWindowAt's
      // own start/end convention.
      var brushGroupDabs = null;
      var brushAnchorItem = null;
      for (var bi = 0; bi < children.length; bi++) {
        var bc = children[bi];
        if (bc.data && bc.data.brushGroupId) {
          if (bc.data.isBrushTextureCopy) {
            if (!brushGroupDabs) brushGroupDabs = {};
            var gid0 = bc.data.brushGroupId;
            (brushGroupDabs[gid0] = brushGroupDabs[gid0] || []).push(bc);
          } else if (bc.data.strokeId) {
            if (!brushAnchorStrokeId) brushAnchorStrokeId = {};
            brushAnchorStrokeId[bc.data.brushGroupId] = bc.data.strokeId;
            // The anchor ITEM too, not just its id (2026-08-21): the bitmap
            // -brush trim path below needs its bitmapBrushSpec AND its live
            // geometry to re-bake, and re-finding it by id would mean a
            // second scan of children per raster.
            if (!brushAnchorItem) brushAnchorItem = {};
            brushAnchorItem[bc.data.brushGroupId] = bc;
          }
        }
      }
      var dabOrdinal = null;
      if (brushGroupDabs) {
        dabOrdinal = new WeakMap();
        Object.keys(brushGroupDabs).forEach(function (gid) {
          var list = brushGroupDabs[gid];
          var n = list.length;
          list.forEach(function (dab, idx) { dabOrdinal.set(dab, n > 1 ? 1 - idx / (n - 1) : 0); });
        });
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
      // Placeholder while a native video's first frame hasn't decoded/
      // registered yet (2026-08 fix, feedback: "avant que une vidéo soit
      // ready un placeholder dans le canvas à la bonne taille") — nv.width/
      // height are known synchronously right after native-video-bridge.js's
      // open() resolves, well before any pixel is actually decoded (that
      // file's own importAsLayer already decodes frame 0 first specifically
      // to minimize this window, but it's still a real async gap — a piped
      // ffmpeg probe or a whole-file arrayBuffer() read on the web path).
      // Deliberately NOT the full render branch below (duplicator/3D/
      // parent-chain) — this is a transient state, a plain rect at the
      // right size/position/rotation covers it without duplicating that
      // whole pipeline for something that shows for a few hundred ms.
      if (state.layers[i].nativeVideo && window.SMEngineBridge && !registeredImageIds['nv:' + i]) {
        var nvPH = state.layers[i].nativeVideo;
        var inFPH = window.layerInPoint ? layerInPoint(state.layers[i]) : 0;
        var outFPH = window.layerOutPoint ? layerOutPoint(state.layers[i]) : state.totalFrames - 1;
        if (renderFrame >= inFPH && renderFrame <= outFPH && nvPH.width && nvPH.height) {
          var phS = Math.min(state.canvasW / nvPH.width, state.canvasH / nvPH.height);
          var phW = nvPH.width * phS, phH = nvPH.height * phS;
          var phRect = { x: (state.canvasW - phW) / 2, y: (state.canvasH - phH) / 2, width: phW, height: phH };
          var phMat = (!is3D && window.SMMotion) ? SMMotion.layerMotionAt(i, renderFrame) : null;
          var phOp = 1;
          if (phMat) {
            var phPivot = { x: phRect.x + phRect.width / 2 + phMat.ax, y: phRect.y + phRect.height / 2 + phMat.ay };
            phRect = SMMotion.transformImageRect(phRect, phPivot, phMat);
            phOp = phMat.op;
          }
          items.push(boundsRectItem(phRect.x, phRect.y, phRect.x + phRect.width, phRect.y + phRect.height, [38, 36, 42, Math.round(255 * phOp)], [110, 110, 122, Math.round(200 * phOp)], 1.5));
        }
      }
      if (state.layers[i].nativeVideo && window.SMEngineBridge && registeredImageIds['nv:' + i]) {
        var nv = state.layers[i].nativeVideo;
        var inF = window.layerInPoint ? layerInPoint(state.layers[i]) : 0;
        var outF = window.layerOutPoint ? layerOutPoint(state.layers[i]) : state.totalFrames - 1;
        if (renderFrame >= inF && renderFrame <= outF) {
          var nvS = Math.min(state.canvasW / nv.width, state.canvasH / nv.height);
          var nvW = nv.width * nvS, nvH = nv.height * nvS;
          var nvRectBase = { x: (state.canvasW - nvW) / 2, y: (state.canvasH - nvH) / 2, width: nvW, height: nvH };
          // Duplicator (2026-07-30): a native video layer has no strokes array
          // (getEffectiveStrokes short-circuits to [] for nativeVideo, app.js),
          // so it never reached applyLayerDuplicator/getEffectiveStrokesRendered
          // — enabling the Duplicator on a video layer used to do nothing at
          // all. Duplicated HERE, in the video's own local/content space
          // (nvRectBase, pivot = its own center), exactly mirroring the stroke
          // path's order: seed content gets duplicated first, THEN the
          // layer's own Motion transform + parent chain apply uniformly to
          // every resulting rect below — never the other way around, or a
          // rotated/scaled layer would duplicate along the wrong axes.
          // _duplicatorCount/_duplicatorModeOffset/_duplicatorClonePlacement
          // (app.js) are the SAME functions applyLayerDuplicator itself calls
          // — never re-derive this math here.
          var nvDup = state.layers[i].duplicator;
          var nvCount = nvDup ? _duplicatorCount(nvDup) : 1;
          var nvRects;
          if (nvDup && nvCount > 1) {
            var nvMode = nvDup.mode || 'grid';
            var nvCols = Math.max(1, nvDup.cols || 1);
            var nvPathInfo = nvMode === 'path' ? _resolveDuplicatorPath(nvDup, renderFrame) : null;
            if (nvMode === 'path' && !nvPathInfo) {
              nvRects = [{ rect: nvRectBase, opacityFactor: 1 }]; // unconfigured path ref: show the seed, not nothing
            } else {
              var nvPivot = { x: nvRectBase.x + nvRectBase.width / 2, y: nvRectBase.y + nvRectBase.height / 2 };
              // No per-clone content resampling exists for video (exactly one
              // decoded frame backs 'nv:'+i) — temporal stagger is a no-op
              // here, unlike the stroke path's tOffOn branch.
              nvRects = [];
              for (var nvk = 0; nvk < nvCount; nvk++) {
                var nvOff = _duplicatorModeOffset(nvDup, nvMode, nvk, nvCount, nvCols, nvPathInfo);
                var nvPlace = _duplicatorClonePlacement(nvDup, nvk, nvPivot, nvOff.baseDx, nvOff.baseDy, nvOff.baseRot, [0, 0], 0, [0, 0], 0, 0, 0, 0, nvDup.staggerRandom || {}, false);
                nvRects.push({ rect: SMMotion.transformImageRect(nvRectBase, nvPivot, nvPlace), opacityFactor: nvPlace.opacityFactor });
              }
              if (nvPathInfo) nvPathInfo.path.remove();
            }
          } else {
            nvRects = [{ rect: nvRectBase, opacityFactor: 1 }];
          }
          // 3D layer (2026-07-30, Cyril: "la 3D sur les footage... marche
          // pas" — confirmed and fixed, see motion.js's project3DImageRect
          // doc comment). NOT reusing the shared layer-wide `project3D`
          // here (unlike the raster branch above, which does) — that one's
          // pivot comes from userLayers[i].bounds, the layer's real Paper
          // CHILDREN bounds, which for a nativeVideo layer are ALWAYS
          // degenerate: a native video has no backing Paper item at all
          // (getEffectiveStrokes short-circuits to [] for it, app.js), so
          // the shared projector's pivot silently collapses to the anchor
          // point alone instead of the video's own visual center — found
          // live: a pure rotationY produced a WIDER, oddly cropped rect
          // instead of a foreshortened one. Built fresh here with the
          // video's own rect as its bounds instead, same "own visual
          // center is the default pivot" contract every other Motion
          // transform in this app already follows.
          var nvMat = (!is3D && window.SMMotion) ? SMMotion.layerMotionAt(i, renderFrame) : null;
          var nvChain = is3D ? [] : SMMotion.parentChainMats(i, renderFrame);
          var nvProject3D = (is3D && window.SMMotion) ? SMMotion.make3DProjector(state.layers[i], nvRectBase, renderFrame, state.canvasW, state.canvasH, parent3D) : null;
          _touchImage('nv:' + i); // one shared texture for every clone — touch once, not per clone
          for (var nvri = 0; nvri < nvRects.length; nvri++) {
            var nvRect = nvRects[nvri].rect, nvOp = nvRects[nvri].opacityFactor;
            if (nvMat) {
              var nvItemPivot = { x: nvRect.x + nvRect.width / 2 + nvMat.ax, y: nvRect.y + nvRect.height / 2 + nvMat.ay };
              nvRect = SMMotion.transformImageRect(nvRect, nvItemPivot, nvMat);
              nvOp *= nvMat.op;
            }
            for (var nvpc = 0; nvpc < nvChain.length; nvpc++) { nvRect = SMMotion.transformImageRect(nvRect, nvChain[nvpc].pivot, nvChain[nvpc].mat); nvOp *= nvChain[nvpc].mat.op; }
            if (nvProject3D) nvRect = SMMotion.project3DImageRect(nvRect, nvProject3D);
            items.push({ image: { imageId: 'nv:' + i, x: nvRect.x, y: nvRect.y, width: nvRect.width, height: nvRect.height, opacity: nvOp, rotation: nvRect.rotation || 0 } });
          }
        }
      }
      // Nested video (2026-07-30, "attaque le chantier pour les vidéo dans
      // des component"): a nativeVideo layer living inside a Component's
      // OWN sym.layers is invisible to the block above (keyed purely off
      // this top-level `i`) — native-video-bridge.js's _syncSymbolVideos
      // (called from the same onFrameChanged choke point as the top-level
      // sync) is what actually uploads its pixels, under image id
      // 'nvsym:<symbolId>:<subLayerIndex>'; this is the render-side match
      // for that upload. Mirrors the block above almost exactly, with one
      // extra stage: the symbol's OWN internal composition (its layer-level
      // Motion on the video sub-layer, its OWN camera, and the instance's
      // symMatrix placement) applies FIRST, in symbol-space — exactly the
      // same order getEffectiveStrokes' symbolId branch (app.js) already
      // bakes into stroke content — THEN this outer layer's own Motion/
      // parent chain applies on top, identical to the plain block above.
      if (state.layers[i].symbolId && window.SMEngineBridge && window.SMMotion) {
        var nvsymLd = state.layers[i];
        var nvsymSym = state.symbols[nvsymLd.symbolId];
        if (nvsymSym) {
          var nvsymIi = window.resolveSymbolFrameIdx ? resolveSymbolFrameIdx(nvsymSym, nvsymLd, renderFrame) : 0;
          for (var nvsymK = 0; nvsymK < nvsymSym.layers.length; nvsymK++) {
            var sl = nvsymSym.layers[nvsymK];
            if (!sl || !sl.nativeVideo || sl.visible === false) continue;
            var nvsymImageId = 'nvsym:' + nvsymLd.symbolId + ':' + nvsymK;
            if (!registeredImageIds[nvsymImageId]) continue;
            var nvsymNv = sl.nativeVideo;
            var nvsymInF = window.layerInPoint ? layerInPoint(sl) : 0;
            var nvsymOutF = window.layerOutPoint ? layerOutPoint(sl) : (nvsymSym.totalFrames - 1);
            if (nvsymIi < nvsymInF || nvsymIi > nvsymOutF) continue;
            var nvsymS = Math.min(state.canvasW / nvsymNv.width, state.canvasH / nvsymNv.height);
            var nvsymW = nvsymNv.width * nvsymS, nvsymH = nvsymNv.height * nvsymS;
            var nvsymRectBase = { x: (state.canvasW - nvsymW) / 2, y: (state.canvasH - nvsymH) / 2, width: nvsymW, height: nvsymH };
            // Duplicator on the video sub-layer itself — same seed-then-
            // placement order as the top-level block, in the video's own
            // local/content space, before any of the symbol/instance/outer
            // transform stages below.
            var nvsymDup = sl.duplicator;
            var nvsymCount = nvsymDup ? _duplicatorCount(nvsymDup) : 1;
            var nvsymRects;
            if (nvsymDup && nvsymCount > 1) {
              var nvsymMode = nvsymDup.mode || 'grid';
              var nvsymCols = Math.max(1, nvsymDup.cols || 1);
              var nvsymPathInfo = nvsymMode === 'path' ? _resolveDuplicatorPath(nvsymDup, nvsymIi) : null;
              if (nvsymMode === 'path' && !nvsymPathInfo) {
                nvsymRects = [{ rect: nvsymRectBase, opacityFactor: 1 }];
              } else {
                var nvsymPivot = { x: nvsymRectBase.x + nvsymRectBase.width / 2, y: nvsymRectBase.y + nvsymRectBase.height / 2 };
                nvsymRects = [];
                for (var nvsymK2 = 0; nvsymK2 < nvsymCount; nvsymK2++) {
                  var nvsymOff = _duplicatorModeOffset(nvsymDup, nvsymMode, nvsymK2, nvsymCount, nvsymCols, nvsymPathInfo);
                  var nvsymPlace = _duplicatorClonePlacement(nvsymDup, nvsymK2, nvsymPivot, nvsymOff.baseDx, nvsymOff.baseDy, nvsymOff.baseRot, [0, 0], 0, [0, 0], 0, 0, 0, 0, nvsymDup.staggerRandom || {}, false);
                  nvsymRects.push({ rect: SMMotion.transformImageRect(nvsymRectBase, nvsymPivot, nvsymPlace), opacityFactor: nvsymPlace.opacityFactor });
                }
                if (nvsymPathInfo) nvsymPathInfo.path.remove();
              }
            } else {
              nvsymRects = [{ rect: nvsymRectBase, opacityFactor: 1 }];
            }
            // Symbol-internal stage: the video sub-layer's OWN Motion
            // (computeMotionMatFor takes the layer OBJECT — sl has no
            // top-level state.layers index for layerMotionAt to use), then
            // the symbol's OWN camera, then the instance's symMatrix
            // placement — same three steps and order getEffectiveStrokes'
            // symbolId branch already applies to stroke content.
            var nvsymLayerMat = SMMotion.computeMotionMatFor(sl, nvsymIi);
            var nvsymOp2 = 1;
            for (var nvsymRi = 0; nvsymRi < nvsymRects.length; nvsymRi++) {
              var nvsymRect = nvsymRects[nvsymRi].rect, nvsymOp = nvsymRects[nvsymRi].opacityFactor;
              if (nvsymLayerMat) {
                var nvsymLayerPivot = { x: nvsymRect.x + nvsymRect.width / 2 + nvsymLayerMat.ax, y: nvsymRect.y + nvsymRect.height / 2 + nvsymLayerMat.ay };
                nvsymRect = SMMotion.transformImageRect(nvsymRect, nvsymLayerPivot, nvsymLayerMat);
                nvsymOp *= nvsymLayerMat.op;
              }
              if (nvsymSym.cameraKeys && nvsymSym.cameraKeys.length && window.SMCamera) {
                var nvsymCamM = SMCamera.cameraMatrixAtFrame(nvsymSym.cameraKeys, nvsymIi, state.canvasW, state.canvasH);
                if (nvsymCamM) nvsymRect = SMMotion.transformImageRectByMatrix(nvsymRect, nvsymCamM);
              }
              if (nvsymLd.symMatrix && window.symMatrixOf) {
                nvsymRect = SMMotion.transformImageRectByMatrix(nvsymRect, symMatrixOf(nvsymLd));
              }
              // Outer stage: the instance layer's OWN Motion + parent
              // chain — identical treatment to the plain top-level video
              // block above, and to how a symbolId layer's own baked
              // strokes get this same pair applied by the per-item loop
              // further down.
              var nvsymOuterMat = SMMotion.layerMotionAt(i, renderFrame);
              if (nvsymOuterMat) {
                var nvsymOuterPivot = { x: nvsymRect.x + nvsymRect.width / 2 + nvsymOuterMat.ax, y: nvsymRect.y + nvsymRect.height / 2 + nvsymOuterMat.ay };
                nvsymRect = SMMotion.transformImageRect(nvsymRect, nvsymOuterPivot, nvsymOuterMat);
                nvsymOp *= nvsymOuterMat.op;
              }
              var nvsymChain = SMMotion.parentChainMats(i, renderFrame);
              for (var nvsymPc = 0; nvsymPc < nvsymChain.length; nvsymPc++) { nvsymRect = SMMotion.transformImageRect(nvsymRect, nvsymChain[nvsymPc].pivot, nvsymChain[nvsymPc].mat); nvsymOp *= nvsymChain[nvsymPc].mat.op; }
              items.push({ image: { imageId: nvsymImageId, x: nvsymRect.x, y: nvsymRect.y, width: nvsymRect.width, height: nvsymRect.height, opacity: nvsymOp, rotation: nvsymRect.rotation || 0 } });
            }
            _touchImage(nvsymImageId);
          }
        }
      }
      // Order (2026-08) — same per-layer cheap guard as _anyLayerOrder
      // above, scoped to just this layer's own elementMotion so a document
      // where only ONE layer's shapes use Order doesn't pay the per-item
      // tag+sort cost on every other layer.
      var _anyElemOrder = window.SMMotion && SMMotion.layerElementsHaveOrder(state.layers[i]);
      for (var s = 0; s < children.length; s++) {
        var c = children[s];
        // Team review view filter — 'mine' hides everyone else's content
        // (ghosts included, since a ghost's ownerId is the ORIGINAL
        // author, never you); 'revisions' hides plain uncontested content
        // so review attention lands only on ghosts + active corrections.
        // Geometry is untouched either way — this only decides what goes
        // into THIS render, never mutates the document.
        if (excludeGhosts) {
          if (c.data && c.data.isRevisionGhost) continue;
        } else if (state.revisionView === 'mine') {
          if (c.data && c.data.ownerId && state.userProfile && c.data.ownerId !== state.userProfile.id) continue;
        } else if (state.revisionView === 'revisions') {
          if (!(c.data && (c.data.isRevisionGhost || c.data.revisionParentId))) continue;
        }
        // Shadow Brush guide-line visibility (timeline.js's
        // toggleShadowGuides) — a shadow-tagged item's whole purpose is to
        // disappear once it's done its job as a fill-boundary guide (see
        // shadow-brush-bridge.js's header comment), so this is a live
        // render-only filter, same shape as the revisionView one just
        // above: geometry/document data is untouched, only what goes into
        // THIS render is affected.
        if (!state.showShadowGuides && c.data && c.data.channelTag === 'shadow') continue;
        // Texture-brush dab reveal (2026-08-20) — "il faudrait mettre en
        // place le trim du coup": a dab (isBrushTextureCopy) isn't shaped
        // by segments/closed at all (see b7e9b29's own diagnosis of why
        // trimming the ANCHOR never touched them), so giving Trim Paths any
        // visible effect on a textured stroke means filtering which dabs
        // draw, not reshaping anything. dabOrdinal (built above) is each
        // dab's 0..1 position among its own group, in the same order
        // buildBrushDabs stamped them; win uses trimWindowAt's own
        // semantics (start/end/offset, 0-100, clipped — not wrapped, same
        // "open path" convention applyTrimSegments uses) so a textured
        // stroke's reveal matches a plain stroke's trim exactly.
        // NOT for a BITMAP-brush group (2026-08-21): its ink is one baked
        // Raster, so it has exactly one "dab" sitting at ordinal 0 — this
        // per-dab test then degenerates to all-or-nothing (kept whole for
        // any window starting at 0, dropped entirely for any window starting
        // past it), which is why trim looked like a complete no-op on a
        // bitmap stroke. Those groups are trimmed properly by re-baking the
        // texture along the trimmed centerline instead (bitmapTrimmedImage,
        // applied in the Raster branch below).
        if (c.data && c.data.isBrushTextureCopy && c.data.brushGroupId && dabOrdinal && window.SMMotion
            && !(brushAnchorItem && brushAnchorItem[c.data.brushGroupId]
                 && brushAnchorItem[c.data.brushGroupId].data
                 && brushAnchorItem[c.data.brushGroupId].data.bitmapBrushSpec)) {
          var dabAnchorId = brushAnchorStrokeId && brushAnchorStrokeId[c.data.brushGroupId];
          if (dabAnchorId) {
            var dabWin = SMMotion.trimWindowAt(i, dabAnchorId, renderFrame);
            if (dabWin) {
              var dabPct = (dabOrdinal.get(c) || 0) * 100;
              var dabS = dabWin.start + (dabWin.offset || 0), dabE = dabWin.end + (dabWin.offset || 0);
              if (dabPct < Math.max(0, Math.min(100, dabS)) || dabPct > Math.max(0, Math.min(100, dabE))) continue;
            }
          }
        }
        // Element-level Motion target (2026-07): a strokeId-scoped transform
        // nested INSIDE the layer's own — applied FIRST below, pivoted
        // around this item's OWN bounds (never the whole layer's), matching
        // AE's shape-group-inside-a-layer composition. null in the common
        // case (this item has no per-element motion of its own).
        var cStrokeId = c.data && ((c.data.isBrushTextureCopy && brushAnchorStrokeId && brushAnchorStrokeId[c.data.brushGroupId]) || c.data.strokeId);
        // Path-CONTENT mutations (Trim, per-vertex offsets, animated corner
        // radii, path-vertex-follow, text-bounds-follow) must never run on a
        // texture-copy dab's own tiny stamp geometry (isBrushTextureCopy,
        // typically a 4-point square). cStrokeId above is deliberately
        // aliased to the ANCHOR's strokeId for a dab so it inherits the
        // anchor's Motion TRANSFORM (elMat) and fillColor override — both
        // desired, both harmless to share. But arc-length-trimming (say)
        // 0-40% of a dab's own 4-point square, or running any other
        // per-vertex rebuild meant for the whole compound stroke, silently
        // collapses each dab to a near-zero-area sliver — this is exactly
        // what made a trimmed bitmap/texture-brush stroke's dabs vanish
        // (confirmed live: hasTrimMotionFor/applyTrimFor were firing on
        // every individual dab via the anchor-aliased id). Real strokeId
        // only (undefined for a dab, since a dab never carries its own
        // data.strokeId) restores "not applicable" for anything that
        // reshapes path content — only used below, never for elMat/
        // elementFillColorAt which should keep sharing the anchor's id.
        var cPathOpsStrokeId = (c.data && c.data.isBrushTextureCopy) ? undefined : cStrokeId;
        var elMat = (window.SMMotion && cStrokeId) ? SMMotion.elementMotionAt(i, cStrokeId, renderFrame) : null;
        var elPivot = elMat ? { x: c.bounds.center.x + elMat.ax, y: c.bounds.center.y + elMat.ay } : null;
        if (c instanceof Raster) {
          // Bitmap-brush Trim Paths (2026-08-21) — a trimmed bitmap stroke
          // draws a texture re-baked along the trimmed centerline instead of
          // its full-length one. undefined = this raster isn't a trimmed
          // bitmap-brush companion, take the normal path; null = the trim
          // window collapsed to nothing, draw no ink at all.
          var bmbTrim;
          if (c.data && c.data.isBrushTextureCopy && c.data.brushGroupId && brushAnchorItem) {
            var bmbAnchor = brushAnchorItem[c.data.brushGroupId];
            var bmbAnchorId = brushAnchorStrokeId && brushAnchorStrokeId[c.data.brushGroupId];
            if (bmbAnchor && bmbAnchorId) bmbTrim = bitmapTrimmedImage(i, bmbAnchor, bmbAnchorId, renderFrame);
          }
          if (bmbTrim === null) continue;
          var imageId = bmbTrim ? bmbTrim.id : registerRasterIfNeeded(c);
          if (!imageId) continue;
          // un-rotated display rect + the raster's own spin (see helper); a
          // re-baked trim carries its OWN rect (the bake's bounds shrink with
          // the window), so it must not reuse the full-length raster's.
          var rb = bmbTrim ? bmbTrim.rect : rasterImageRect(c);
          var imgOp = c.opacity !== undefined ? c.opacity : 1;
          if (elMat) { rb = SMMotion.transformImageRect(rb, elPivot, elMat); imgOp *= elMat.op; }
          if (motionMat) { rb = SMMotion.transformImageRect(rb, motionPivot, motionMat); imgOp *= motionMat.op; }
          for (var pc = 0; pc < parentChain.length; pc++) { rb = SMMotion.transformImageRect(rb, parentChain[pc].pivot, parentChain[pc].mat); imgOp *= parentChain[pc].mat.op; }
          // 3D layer (2026-07-30, Cyril: "la 3D sur les footage... marche
          // pas" — confirmed, see motion.js's project3DImageRect doc
          // comment for the full reasoning/approximation tradeoff).
          // motionMat/parentChain are already forced null/[] above when
          // is3D (elMat is untouched, same as the vector path below), so
          // this REPLACES what their contribution would have been rather
          // than stacking on top of it. Per-clone override mirrors the
          // vector path's own dup3D handling a few hundred lines down
          // (same cache, same merge-with-parent3D logic) — a duplicated
          // raster (imported image used as a Duplicator seed) gets its
          // own clone's positionZ/rotationX/rotationY delta instead of
          // sharing the layer-wide projector.
          if (project3D) {
            var rbProjector = project3D;
            if (c.data && c.data.dup3D && dup3DProjectorCache) {
              var rbDk3 = c.data.dupIndex;
              if (!dup3DProjectorCache[rbDk3]) {
                var rbCloneDelta = c.data.dup3D;
                if (parent3D) {
                  rbCloneDelta = {
                    dx: (parent3D.dx || 0), dy: (parent3D.dy || 0), drot: (parent3D.drot || 0),
                    dsxPct: (parent3D.dsxPct || 0), dsyPct: (parent3D.dsyPct || 0),
                    dz: (rbCloneDelta.dz || 0) + (parent3D.dz || 0),
                    drx: (rbCloneDelta.drx || 0) + (parent3D.drx || 0),
                    dry: (rbCloneDelta.dry || 0) + (parent3D.dry || 0),
                  };
                }
                dup3DProjectorCache[rbDk3] = SMMotion.make3DProjector(state.layers[i], q3dBounds, renderFrame, state.canvasW, state.canvasH, rbCloneDelta);
              }
              rbProjector = dup3DProjectorCache[rbDk3];
            }
            rb = SMMotion.project3DImageRect(rb, rbProjector);
          }
          items.push({
            image: { imageId: imageId, x: rb.x, y: rb.y, width: rb.width, height: rb.height, opacity: imgOp, rotation: rb.rotation || 0 },
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
          // FAST PATH: geometry provably untransformed AND already registered
          // -> emit the ref without touching serP/roundSegs at all. This is
          // the entire point of the retained store; everything below is the
          // cold path that pays serialization once per new geometry.
          // v2: the Motion chain no longer disqualifies a retained path — it
          // is folded into ONE affine sent alongside the ref. Four things
          // still do, each because it would change the picture:
          //   - per-vertex path offsets (the only non-affine piece)
          //   - a non-uniform scale anywhere in the chain (stroke-width
          //     semantics differ, see motionChainUniform)
          //   - a gradient fill (its anchors are pre-transformed inline)
          //   - the current-frame outline overlay (its stroke width is a
          //     screen-space constant that must NOT ride the affine)
          // 3D layer (2026-07-28): a 5th exclusion, same reasoning as the
          // other four — retained-path registration stores `segsBefore`
          // (the PRE-transform geometry) and relies on `pathTf` (a single
          // AFFINE matrix) to reproduce the posed result on the Rust side.
          // The 3D projector below is NONLINEAR (a real perspective
          // projection, not scale+rotate+translate) — pathTf can't
          // represent it, so a retained path would render flat/unprojected
          // whenever this fast path fired. Always take the cold (serP)
          // path for a 3D layer instead, exactly like symbolId/montageId/
          // lfsGroup already do for their own "synthesizes new geometry
          // every call" reasons (layerRetainable, above).
          var xformable = layerRetainable
            && !project3D
            && !(window.SMMotion && cPathOpsStrokeId && SMMotion.hasPathVertexMotionFor(i, cPathOpsStrokeId))
            && !(window.SMMotion && cPathOpsStrokeId && SMMotion.hasTrimMotionFor(i, cPathOpsStrokeId))
            && !(window.SMMotion && cPathOpsStrokeId && SMMotion.hasParamShapeMotionFor && SMMotion.hasParamShapeMotionFor(i, cPathOpsStrokeId))
            // Path-point parenting "drive" direction + rect-follows-text-
            // bounds (2026-08): both rebuild sd.segments from scratch every
            // frame in the cold (sd-populated) path below — a retained
            // pathRef+pathTransform is a single AFFINE on top of CACHED
            // geometry, which can't represent "this vertex jumped to
            // wherever layer X is now" or "this rect's whole outline
            // matches layer Y's current text bounds". Missing from this
            // exclusion list meant a vertex-follow/text-bounds-follow
            // target that happened to also be an otherwise-plain, uniformly
            // -scaled shape took the fast path and silently never re-shaped
            // — confirmed live: the SAME hooks below (applyPathVertexFollowFor/
            // applyTextBoundsFollowFor) never ran because `sd` stayed null.
            && !(window.SMMotion && cPathOpsStrokeId && SMMotion.hasPathVertexFollowMotionFor && SMMotion.hasPathVertexFollowMotionFor(i, cPathOpsStrokeId))
            && !(window.SMMotion && cPathOpsStrokeId && SMMotion.hasTextBoundsFollowMotionFor && SMMotion.hasTextBoundsFollowMotionFor(i, cPathOpsStrokeId))
            && !(c.data && c.data.fillGradient)
            && !(c.data && c.data.strokeGradientAlongPath)
            && !(includeEditorOverlays && state.currentFrameOutline)
            && motionChainUniform(elMat, motionMat, parentChain);
          var fastRef = xformable ? existingPathRef(sub) : null;
          var sd = fastRef ? null : serP(sub);
          // Identity, not deep-compare: applyPathVertexOffsetsFor returns the
          // SAME array when the shape has no per-vertex keys, so `!==` is an
          // exact "were the coordinates rewritten?" test.
          var segsBefore = sd ? sd.segments : null;
          // Path property, per-vertex (motion.js's applyPathVertexOffsetsFor,
          // 2026-07): innermost layer of the transform stack, applied to the
          // raw geometry BEFORE elMat — a vertex offset is authored in the
          // shape's OWN local space, same as elMat's own pivot is computed
          // from `c.bounds` (the pre-offset bounds), matching AE's model
          // where a path's own points are edited before any transform.
          if (sd && window.SMMotion && cPathOpsStrokeId) sd.segments = SMMotion.applyPathVertexOffsetsFor(i, cPathOpsStrokeId, sd.segments, renderFrame);
          // Trim Paths (2026-08) — same innermost-layer placement as vertex
          // offsets right above (authored in the shape's own local space,
          // before elMat/motionMat), applied right after so a trimmed
          // portion still rides any per-vertex sculpting done on top of it.
          if (sd && window.SMMotion && cPathOpsStrokeId && SMMotion.hasTrimMotionFor(i, cPathOpsStrokeId)) {
            // Vector-brush ribbon (2026-08-20): trim the CENTERLINE +
            // width profile and rebuild the outline from just that
            // window, instead of arc-length-slicing the already-baked
            // outline polygon (applyTrimFor below) — slicing the OUTLINE
            // cuts across the ribbon's own width rather than along its
            // length, the same wedge/sliver family as the fill-wedge bug
            // this whole feature started from (18b2d9a's own comment).
            // buildVariableWidthPath (tools.js) is the exact function
            // rebuildVectorBrushOutline already uses for the untrimmed
            // case, so a trimmed ribbon gets identical caps/smoothing.
            if (c.data && c.data.isVectorBrush && c.data.centerSegments && c.data.centerSegments.length >= 2) {
              var vbTrim = SMMotion.applyTrimToVectorBrush(i, cPathOpsStrokeId, c.data.centerSegments, c.data.widthProfile, renderFrame);
              var vbOutline = (vbTrim && vbTrim.pts && vbTrim.pts.length >= 2)
                ? buildVariableWidthPath(vbTrim.pts.map(function (p) { return new Point(p[0], p[1]); }), vbTrim.widths)
                : null;
              if (vbOutline) {
                sd.segments = vbOutline.segments.map(function (s) {
                  return { point: [s.point.x, s.point.y], handleIn: [s.handleIn.x, s.handleIn.y], handleOut: [s.handleOut.x, s.handleOut.y] };
                });
                sd.closed = true;
                vbOutline.remove();
              } else {
                sd.segments = []; sd.closed = false;
              }
            } else {
              var trimmed = SMMotion.applyTrimFor(i, cPathOpsStrokeId, sd.segments, sd.closed, renderFrame);
              sd.segments = trimmed.segments; sd.closed = trimmed.closed;
            }
          }
          // Brush size (2026-08) — same innermost-layer placement as Trim
          // right above, and deliberately `else if` with it: both rebuild
          // this same vector-brush's outline from its centerline+width
          // profile, and composing an animated Trim window with an
          // animated Brush Size on the SAME stroke at the same time needs
          // the trim's OWN pts/widths as Brush Size's input, not the
          // shape's un-trimmed static data — a real future case, not
          // attempted here. Trim wins when both are set.
          else if (sd && window.SMMotion && cPathOpsStrokeId && SMMotion.hasBrushSizeMotionFor && SMMotion.hasBrushSizeMotionFor(i, cPathOpsStrokeId)) {
            var bsSegs = SMMotion.applyBrushSizeFor(i, cPathOpsStrokeId, sd, renderFrame);
            if (bsSegs) { sd.segments = bsSegs; sd.closed = true; }
          }
          // Dynamic shapes phase 2 (2026-08-18) — animated corner radii,
          // same innermost-layer placement as Trim/vertex-offsets right
          // above (shape's own local space, before elMat/motionMat).
          if (sd && window.SMMotion && cPathOpsStrokeId && SMMotion.hasParamShapeMotionFor && SMMotion.hasParamShapeMotionFor(i, cPathOpsStrokeId)) {
            sd.segments = SMMotion.applyParamShapeFor(i, cPathOpsStrokeId, sd, renderFrame);
          }
          // Path-point parenting, "drive" direction (2026-08) — a vertex of
          // this path snapping onto another layer's world position. The
          // "follow" direction (this LAYER snapping onto a point on another
          // path) needs no extra call here — it's inside SMMotion.layerMotionAt
          // itself, already the source of motionMat above.
          if (sd && window.SMMotion && cPathOpsStrokeId && SMMotion.hasPathVertexFollowMotionFor && SMMotion.hasPathVertexFollowMotionFor(i, cPathOpsStrokeId)) {
            sd.segments = SMMotion.applyPathVertexFollowFor(i, cPathOpsStrokeId, sd, renderFrame);
          }
          // Rect-follows-text-bounds (2026-08) — whole-shape rebuild from
          // another layer's resolved bounds, same placement as the two
          // hooks above (shape's own local space, before elMat/motionMat).
          if (sd && window.SMMotion && cPathOpsStrokeId && SMMotion.hasTextBoundsFollowMotionFor && SMMotion.hasTextBoundsFollowMotionFor(i, cPathOpsStrokeId)) {
            sd.segments = SMMotion.applyTextBoundsFollowFor(i, cPathOpsStrokeId, sd, renderFrame);
          }
          // Dynamic shapes phase 2 (2026-08-18) — animated corner radii,
          // same innermost-layer placement as Trim/vertex-offsets right
          // above (shape's own local space, before elMat/motionMat).
          if (sd && window.SMMotion && cStrokeId && SMMotion.hasParamShapeMotionFor && SMMotion.hasParamShapeMotionFor(i, cStrokeId)) {
            sd.segments = SMMotion.applyParamShapeFor(i, cStrokeId, sd, renderFrame);
          }
          if (sd && elMat) sd.segments = SMMotion.transformSegments(sd.segments, elPivot, elMat);
          if (sd && motionMat) sd.segments = SMMotion.transformSegments(sd.segments, motionPivot, motionMat);
          // 3D layer (2026-07-28) — replaces motionMat's role for a 3D-
          // enabled layer (motionMat is forced null above): projects every
          // VERTEX through the layer's 3D transform + camera, leaving
          // strokeWidth/strokeScale (below) completely untouched — the
          // Grease-Pencil-style contract this feature was explicitly
          // corrected to (motion.js's make3DProjector doc comment).
          if (sd && project3D) {
            // Per-clone 3D override (2026-07-30) — a duplicator copy
            // carrying its own positionZ/rotationX/rotationY delta
            // (data.dup3D, app.js's applyLayerDuplicator) gets its OWN
            // projector instead of the layer-wide `project3D` every other
            // item here shares, cached per dupIndex so every stroke
            // belonging to the same clone reuses one build.
            var itemProjector = project3D;
            if (c.data && c.data.dup3D && dup3DProjectorCache) {
              var dk3 = c.data.dupIndex;
              if (!dup3DProjectorCache[dk3]) {
                // Merge with the layer-wide parent3D contribution (if any)
                // — without this, a layer that's BOTH a duplicator AND
                // multi-parented would apply the parent blend to its
                // shared `project3D` but silently drop it from every
                // per-clone override, since dup3D only ever carries the
                // clone's OWN delta.
                var cloneDelta = c.data.dup3D;
                if (parent3D) {
                  cloneDelta = {
                    dx: (parent3D.dx || 0), dy: (parent3D.dy || 0), drot: (parent3D.drot || 0),
                    dsxPct: (parent3D.dsxPct || 0), dsyPct: (parent3D.dsyPct || 0),
                    dz: (cloneDelta.dz || 0) + (parent3D.dz || 0),
                    drx: (cloneDelta.drx || 0) + (parent3D.drx || 0),
                    dry: (cloneDelta.dry || 0) + (parent3D.dry || 0),
                  };
                }
                dup3DProjectorCache[dk3] = SMMotion.make3DProjector(state.layers[i], q3dBounds, renderFrame, state.canvasW, state.canvasH, cloneDelta);
              }
              itemProjector = dup3DProjectorCache[dk3];
            }
            sd.segments = SMMotion.project3DSegments(sd.segments, itemProjector);
          }
          if (sd) for (var pc2 = 0; pc2 < parentChain.length; pc2++) sd.segments = SMMotion.transformSegments(sd.segments, parentChain[pc2].pivot, parentChain[pc2].mat);
          var op = c.opacity !== undefined ? c.opacity : 1;
          if (elMat) op *= elMat.op;
          if (motionMat) op *= motionMat.op;
          for (var pc3 = 0; pc3 < parentChain.length; pc3++) op *= parentChain[pc3].mat.op;
          var strokeScale = 1;
          if (elMat) strokeScale *= (Math.abs(elMat.sx) + Math.abs(elMat.sy)) / 2;
          if (motionMat) strokeScale *= (Math.abs(motionMat.sx) + Math.abs(motionMat.sy)) / 2;
          for (var pc4 = 0; pc4 < parentChain.length; pc4++) strokeScale *= (Math.abs(parentChain[pc4].mat.sx) + Math.abs(parentChain[pc4].mat.sy)) / 2;
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
          // Retained path (see the store's own comment above): only when the
          // geometry reaches here untouched by the per-vertex / element /
          // layer / parent transform chain — any of those rewrite every
          // coordinate, so the stored path would be the wrong shape.
          var rounded = sd ? roundSegs(sd.segments) : null;
          // A stored path must live in the shape's OWN space, never a posed
          // one — so registration reads `segsBefore`, the pre-transform array
          // (transformSegments returns a new array, it does not mutate). That
          // is what lets a permanently-animated layer register at all: v2's
          // first draft registered from the POSED geometry and so a Motion
          // layer, which never presents an untransformed frame, could never
          // seed its own store entry.
          var pathRef = fastRef;
          if (!pathRef && sd && xformable) pathRef = pathRefFor(sub, roundSegs(segsBefore), sd.closed);
          var pathTf = null;
          if (pathRef && (elMat || motionMat || parentChain.length)) {
            if (elMat) pathTf = affineFromMotion(elMat, elPivot);
            if (motionMat) { var mm = affineFromMotion(motionMat, motionPivot); pathTf = pathTf ? affineMul(mm, pathTf) : mm; }
            for (var pcf = 0; pcf < parentChain.length; pcf++) {
              var pmf = affineFromMotion(parentChain[pcf].mat, parentChain[pcf].pivot);
              pathTf = pathTf ? affineMul(pmf, pathTf) : pmf;
            }
          }
          var item;
          if (pathRef) {
            item = { pathRef: pathRef, fillColor: cssColorToRgba(c.fillColor ? c.fillColor.toCSS(true) : null, op) };
            if (pathTf) item.pathTransform = pathTf;
          } else {
            item = {
              segments: rounded,
              closed: !!sd.closed,
              fillColor: cssColorToRgba(c.fillColor ? c.fillColor.toCSS(true) : null, op),
            };
          }
          // Extended per-shape property: Fill color (2026-07) — overrides
          // the item's own painted color with its element holder's
          // 'fillColor' track, [r,g,b,a] 0-255, the exact shape
          // cssColorToRgba above already produces — never touched unless
          // the user actually keyed/set it (elementFillColorAt returns
          // null otherwise, see its own comment in motion.js).
          if (window.SMMotion && cStrokeId) {
            var fcOverride = SMMotion.elementFillColorAt(i, cStrokeId, renderFrame);
            if (fcOverride) item.fillColor = fcOverride;
          }
          // Gradient fill (2026-07) — takes priority over the flat fillColor
          // above on the Rust side (geometry-wasm's paint_fill), same
          // "richer field wins" precedent as centerline/image. Anchor points
          // are document coordinates and follow the exact same element,
          // layer and parent transform chain as the path geometry.
          // gradientGeomOk: a from/to-less gradient would send `undefined`
          // endpoints into the Rust side. Skipping it here leaves the flat
          // fillColor in place — the shape still draws, just without the
          // ramp, instead of rendering as garbage.
          if (c.data && c.data.fillGradient && (!window.gradientGeomOk || window.gradientGeomOk(c.data.fillGradient))) {
            var fg = c.data.fillGradient;
            function transformedGradientPoint(pt) {
              var one = [{ point: [pt[0], pt[1]], handleIn: [0, 0], handleOut: [0, 0] }];
              if (elMat) one = SMMotion.transformSegments(one, elPivot, elMat);
              if (motionMat) one = SMMotion.transformSegments(one, motionPivot, motionMat);
              for (var gpc = 0; gpc < parentChain.length; gpc++) one = SMMotion.transformSegments(one, parentChain[gpc].pivot, parentChain[gpc].mat);
              return one[0].point;
            }
            item.fillGradient = {
              kind: fg.kind, from: transformedGradientPoint(fg.from), to: transformedGradientPoint(fg.to),
              stops: fg.stops.map(function (s) { return { offset: s.offset, color: cssColorToRgba(s.color, op) || [0, 0, 0, 0] }; }),
            };
          }
          // Trim Paths (2026-08-19 fix) — feedback: "ça trim ça comme un
          // fill alors que ça devrait trim comme dans After Effects ou
          // Redgiant Stroke 3D". applyTrimFor above already replaces
          // sd.segments with a short OPEN sub-arc, but item.fillColor (and
          // fillGradient, just above) still carried whatever paint color
          // the ORIGINAL closed shape had — filling an open partial arc
          // implicitly closes it edge-to-edge, drawing the classic AE
          // "pac-man wedge" instead of a clean progressive line reveal.
          // AE/Stroke 3D's own convention: Trim Paths is a STROKE
          // operation — the standard workaround for the exact wedge
          // artifact this fixes is "don't fill a trimmed shape, use only
          // a stroke", so this makes that the enforced behavior rather
          // than a manual gotcha the artist has to already know about.
          // Deliberately unconditional on the trim window (even 0/100 =
          // "untrimmed") rather than only when partially trimmed: Trim
          // Paths rebuilds ANY trimmed shape (closed or not) as an open
          // polyline approximation (applyTrimSegments always returns
          // closed:false), so the wedge risk exists at any window, not
          // just a partial one.
          // EXCLUDED: vector-brush ribbons (c.data.isVectorBrush). Their
          // visible ink IS the fill (a filled ribbon built from
          // centerSegments/widthProfile) — a real drawn stroke's
          // strokeColor is either null or just a thin keyline hairline
          // (legacy applyBrushKeyline output), never the main paint. Nulling
          // fillColor here too would leave a trimmed brush stroke with NO
          // paint at all (confirmed live: renders fully blank — only the
          // hairline keyline would remain, if even that). Trim Paths on a
          // vector-brush stroke keeps the pre-fix wedge-style behavior for
          // now — a real "reveal the ribbon progressively" implementation
          // needs to trim the centerline/widthProfile before
          // rebuildVectorBrushOutline, not the baked outline segments,
          // which is a separate piece of work.
          // cPathOpsStrokeId (not cStrokeId) — a texture-copy dab must never
          // reach this branch at all: see its own comment above. The
          // isVectorBrush guard stays for the real anchor item, which IS
          // reached through cPathOpsStrokeId.
          if (window.SMMotion && cPathOpsStrokeId && SMMotion.hasTrimMotionFor(i, cPathOpsStrokeId)
              && !(c.data && c.data.isVectorBrush)) {
            item.fillColor = null;
            delete item.fillGradient;
          }
          var sc = cssColorToRgba(c.strokeColor ? c.strokeColor.toCSS(true) : null, op);
          if (sc) {
            item.strokeColor = sc;
            // With pathTransform the engine strokes THROUGH the affine, which
            // already scales the pen — pre-multiplying here too would square
            // the scale.
            item.strokeWidth = (c.strokeWidth || 1) * (item.pathTransform ? 1 : strokeScale);
            // typeof-guarded — same Option<String> boundary as blendMode/
            // matteMode above; a Path deserialized from a corrupted/older
            // save (desP, app.js) could otherwise hand a non-string value
            // straight through to serde and permanently disable the engine.
            item.strokeCap = typeof c.strokeCap === 'string' ? c.strokeCap : undefined;
            item.strokeJoin = typeof c.strokeJoin === 'string' ? c.strokeJoin : undefined;
            item.miterLimit = c.miterLimit;
            if (c.dashArray && c.dashArray.length) {
              item.dashPattern = c.dashArray;
              item.dashOffset = c.dashOffset;
            }
            // Extended per-shape properties: Stroke color/width (2026-08 —
            // second slice of the "propriétés étendues par forme" chantier,
            // same shape as Fill color's own override above). Only reachable
            // inside this `if(sc)` block — same "can't animate a stroke into
            // existence on a strokeless shape" scope Fill color's own
            // comment establishes for fills. Width gets the SAME strokeScale
            // treatment as the base width just above it (a keyed width is
            // the shape's own LOCAL value, composed with elMat/motionMat
            // exactly like c.strokeWidth already is — not a finished,
            // already-posed number).
            if (window.SMMotion && cStrokeId) {
              var scOverride = SMMotion.elementStrokeColorAt(i, cStrokeId, renderFrame);
              if (scOverride) item.strokeColor = scOverride;
              var swOverride = SMMotion.elementStrokeWidthAt(i, cStrokeId, renderFrame);
              if (swOverride !== null) item.strokeWidth = swOverride * (item.pathTransform ? 1 : strokeScale);
            }
          }
          if (includeEditorOverlays && state.currentFrameOutline) {
            delete item.fillGradient;
            item.fillColor = null;
            if (!item.strokeColor) {
              item.strokeColor = [235, 235, 240, Math.round(255 * op)];
              item.strokeWidth = Math.max(1 / view.zoom, 0.75);
            }
          }
          if (c.data && typeof c.data.paintOrder === 'string') item.paintOrder = c.data.paintOrder;
          // Per-element effects (2026-07, effects-panel.js — "possible de
          // différencié les effet par éléments sélectionné") — same
          // {effectType,enabled,p1..p4}[] shape sceneEffectsOf already
          // normalizes ld.effects into, just scoped to this one item. See
          // engine.rs's paint_layer_items for how an item carrying this is
          // isolated and effect-processed on its own within the layer.
          if (c.data && c.data.effects && c.data.effects.length) item.effects = sceneEffectsOf(c.data);
          // Stroke gradient along path (2026-08, "gradient qui tire du
          // début à la fin du trait" — distinct from fillGradient above,
          // which is a spatial 2-point ramp unrelated to the path's own
          // shape). No new WGSL/Rust needed: this engine draws ONE flat
          // color per stroked item, so a gradient becomes many small
          // straight sub-segments, each solid-colored at its own arc-length
          // position — the same "split into pieces along the path" idea
          // buildBitmapBrush's dab-stamping already uses for a different
          // purpose. v1: exactly 2 stops (from/to), piece count adaptive to
          // length (denser trait = more pieces, capped both ends so a tiny
          // trait or a huge one both stay cheap).
          if (item.segments && item.strokeColor && c.data && c.data.strokeGradientAlongPath && window.SMMotion) {
            var sg = c.data.strokeGradientAlongPath;
            var fromRgba = cssColorToRgba(sg.from, op), toRgba = cssColorToRgba(sg.to, op);
            if (fromRgba && toRgba) {
              var poly = SMMotion.flattenSegmentsToPolyline(item.segments, item.closed, 20);
              var cumL = [0];
              for (var pli = 1; pli < poly.length; pli++) {
                var pdx = poly[pli][0] - poly[pli - 1][0], pdy = poly[pli][1] - poly[pli - 1][1];
                cumL.push(cumL[pli - 1] + Math.sqrt(pdx * pdx + pdy * pdy));
              }
              var totalL = cumL[cumL.length - 1];
              if (totalL > 0) {
                var pieceCount = Math.max(6, Math.min(48, Math.round(totalL / 40)));
                var pieceLen = totalL / pieceCount;
                function pointAtLenGrad(len) {
                  for (var qi = 1; qi < cumL.length; qi++) {
                    if (cumL[qi] >= len) {
                      var qSegLen = cumL[qi] - cumL[qi - 1];
                      var qt = qSegLen > 0 ? (len - cumL[qi - 1]) / qSegLen : 0;
                      return [poly[qi - 1][0] + (poly[qi][0] - poly[qi - 1][0]) * qt, poly[qi - 1][1] + (poly[qi][1] - poly[qi - 1][1]) * qt];
                    }
                  }
                  return poly[poly.length - 1];
                }
                for (var pieceI = 0; pieceI < pieceCount; pieceI++) {
                  var pA = pointAtLenGrad(pieceI * pieceLen), pB = pointAtLenGrad((pieceI + 1) * pieceLen);
                  var tMid = (pieceI + 0.5) / pieceCount;
                  var pieceColor = [
                    Math.round(fromRgba[0] + (toRgba[0] - fromRgba[0]) * tMid),
                    Math.round(fromRgba[1] + (toRgba[1] - fromRgba[1]) * tMid),
                    Math.round(fromRgba[2] + (toRgba[2] - fromRgba[2]) * tMid),
                    Math.round(fromRgba[3] + (toRgba[3] - fromRgba[3]) * tMid),
                  ];
                  items.push({
                    segments: [{ point: pA, handleIn: [0, 0], handleOut: [0, 0] }, { point: pB, handleIn: [0, 0], handleOut: [0, 0] }],
                    closed: false, fillColor: null, strokeColor: pieceColor,
                    strokeWidth: item.strokeWidth, strokeCap: item.strokeCap, strokeJoin: item.strokeJoin,
                  });
                }
                return; // pieces pushed directly — skip the single combined item below
              }
            }
          }
          // Non-destructive combine groups (2026-07-29) need to find, after
          // this loop, which JSON item(s) came from which live source item —
          // stripped again right after use, never sent to the renderer.
          item.__srcC = sub;
          // Order (2026-08) — tagged here, consumed by the stable sort right
          // after this loop, stripped before that sort returns (never sent
          // to the renderer, same lifecycle as __srcC above).
          if (_anyElemOrder && window.SMMotion && cStrokeId) item.__ord = SMMotion.elementOrderAt(i, cStrokeId, renderFrame);
          // Vector mask (2026-08) — pulled out of `items` (never painted as
          // normal content) into `layerMasks` instead, geometry/transform
          // untouched (so it moves/animates exactly like any other Path)
          // but with a forced solid-white/no-stroke paint: only the SHAPE
          // matters to the engine's mask combine, never the real color the
          // user sees while editing it on canvas.
          if (c.data && c.data.isMask) {
            var maskItem = {};
            for (var mik in item) { if (Object.prototype.hasOwnProperty.call(item, mik)) maskItem[mik] = item[mik]; }
            maskItem.fillColor = [255, 255, 255, 255];
            delete maskItem.fillGradient;
            maskItem.strokeColor = null;
            delete maskItem.strokeWidth; delete maskItem.strokeCap; delete maskItem.strokeJoin; delete maskItem.miterLimit;
            delete maskItem.dashPattern; delete maskItem.dashOffset; delete maskItem.__srcC; delete maskItem.effects;
            layerMasks.push({ item: maskItem, mode: (c.data.maskMode === 'subtract' || c.data.maskMode === 'intersect') ? c.data.maskMode : 'add' });
            if (c.data.maskFeather > 0) layerMaskFeather = Math.max(layerMaskFeather, c.data.maskFeather);
            return;
          }
          items.push(item);
        });
      }
      // Order (2026-08, "système pour animer l'id index de calque ou de
      // shape/éléments") — element-level z-stacking within this one layer.
      // Every item pushed above by a shape carrying an Order track/static
      // got tagged with __ord; a STABLE sort here turns that into actual
      // draw position (ties — the default, __ord undefined/0 — keep
      // original document order, same neutral CSS-z-index meaning as the
      // layer-level sort in the main loop above). Runs BEFORE the combine-
      // groups block below so it only ever touches real per-shape items,
      // never the synthetic merged-outline item combine-groups appends
      // (which always lands last regardless — acceptable v1 scope, a
      // combined shape has no single per-member Order that would be
      // correct for it anyway).
      if (_anyElemOrder) {
        items.forEach(function (it, ix) { it.__ordIx = ix; });
        items.sort(function (a, b) { return ((a.__ord || 0) - (b.__ord || 0)) || (a.__ordIx - b.__ordIx); });
        items.forEach(function (it) { delete it.__ord; delete it.__ordIx; });
      }
      // Non-destructive combine groups (2026-07-29) — post-process on the
      // JSON items just built, never on the live document (see the "why not
      // touch the live document" rationale, group-bridge.js): suppress the
      // paint of any item whose source is a combine-group member (in the
      // JSON item only — the live Path keeps its real fill/stroke, still
      // fully hit-testable/editable), then append the combined outline
      // (styled from the topmost member). Scoped strictly to this layer's
      // own ld.groups, cheap no-op for the overwhelmingly common case of no
      // combine-groups at all.
      if (window.SMGroup && state.layers[i].groups && Object.keys(state.layers[i].groups).length) {
        var combineRes = SMGroup.renderCombinesFromChildren(userLayers[i], state.layers[i]);
        var suppressArr = combineRes.suppress;
        items.forEach(function (it) {
          if (suppressArr.length && it.__srcC && suppressArr.indexOf(it.__srcC) !== -1) {
            it.fillColor = null; it.strokeColor = null; delete it.strokeWidth; delete it.dashPattern; delete it.dashOffset; delete it.fillGradient;
          }
          delete it.__srcC;
        });
        combineRes.extra.forEach(function (ex) {
          var op2 = ex.path.opacity !== undefined ? ex.path.opacity : 1;
          var exSegs = ex.path.segments.map(function (s) { return { point: [s.point.x, s.point.y], handleIn: [s.handleIn.x, s.handleIn.y], handleOut: [s.handleOut.x, s.handleOut.y] }; });
          // Layer Motion transform (2026-07-29 fix, QA-confirmed "le gizmo/
          // combine se décale par rapport au calque") — computeGroupCombine
          // runs on the members' raw LIVE Paper geometry (un-posed, same
          // §5ter "stored path lives in the shape's own space" contract
          // every other item in this loop already follows), but unlike
          // those, this merged item was built straight from that geometry
          // with no pathTransform/pathRef of its own — every other item
          // gets motionMat+parentChain applied via pathTransform, so this
          // one must too, or it stays frozen at the pre-Motion position
          // while the rest of the layer (and its own now-suppressed
          // members) moves. No elMat here on purpose: the merge can draw
          // from several members that could each carry a DIFFERENT
          // per-element Motion, so there is no single element transform
          // that's correct for the combined shape — only the layer-level
          // chain, which is uniform across every item in this layer.
          if (motionMat) exSegs = SMMotion.transformSegments(exSegs, motionPivot, motionMat);
          for (var pcx = 0; pcx < parentChain.length; pcx++) exSegs = SMMotion.transformSegments(exSegs, parentChain[pcx].pivot, parentChain[pcx].mat);
          var extraItem = {
            segments: roundSegs(exSegs),
            closed: !!ex.path.closed,
            fillColor: cssColorToRgba(ex.path.fillColor ? ex.path.fillColor.toCSS(true) : null, op2),
          };
          var exSc = cssColorToRgba(ex.path.strokeColor ? ex.path.strokeColor.toCSS(true) : null, op2);
          if (exSc) { extraItem.strokeColor = exSc; extraItem.strokeWidth = ex.path.strokeWidth || 1; }
          items.push(extraItem);
        });
      } else {
        items.forEach(function (it) { delete it.__srcC; });
      }
      // typeof-guarded (not just truthy) — engine.rs's LayerIn::blend_mode is
      // Option<String>, and a non-string value here (e.g. a corrupted/older
      // project file that stored a boolean, or the stale localStorage
      // 'nemo-auto' autosave restored silently at boot, timeline.js's
      // importJSON) sails through the `bm && bm !== 'normal'` ternary below
      // UNCHANGED — Rust's serde then rejects the whole scene, which
      // tick()'s catch treats exactly like the screen_to_world Float64Array
      // bug above: setEnabled(false) for the rest of the session, not just
      // a skipped frame.
      var bm = state.layers[i].blendMode;
      if (typeof bm !== 'string') bm = undefined;
      // Track matte (2026-07, scouted from Caddis's Layer.matteMode):
      // AE convention — the matte SOURCE is implicitly the layer directly
      // above this one (i+1), never referenced by id. Same wire shape as
      // blendMode (a plain per-layer string, undefined = no matte), read
      // by engine.rs's composite_scene which also SKIPS painting the
      // source layer as its own visible content once it's consumed.
      var mm = state.layers[i].matteMode;
      if (typeof mm !== 'string') mm = undefined;
      // Effects stack (2026-07 rewrite — was separate blurRadius/gshadow_*
      // fields) — runs on THIS layer's own isolated alpha (see
      // geometry-wasm/src/engine.rs's LayerIn::effects doc comment).
      // ---- MOTION BLUR (2026-07-25) ----------------------------------
      // AE's per-layer switch, gated by a comp-wide one, sampled the way
      // every renderer without a real velocity buffer does it: N copies of
      // the layer along the shutter interval, each at 1/N opacity, drawn
      // UNDER the sharp current frame.
      //
      // The samples are built from the items ALREADY produced above rather
      // than by re-running that whole loop N times — the loop interleaves
      // item building with the transform pass, and duplicating it would be
      // the "two readers that must stay identical" trap CLAUDE.md §3 is
      // about. Instead each sample applies the DELTA between the matrix at
      // the current frame and the matrix at the sample time, around the same
      // pivot: exact for this transform model (scale, then rotate, then
      // translate about a fixed pivot), and layerMotionAt already accepts a
      // fractional frame (rawValueAtFrame interpolates on a float t).
      //
      // Samples the layer's OWN motion only, not its parent chain — a
      // parented layer blurs on its own movement, not the rig's. Noted
      // rather than silently approximated.
      var mbOn = state.motionBlurOn && state.layers[i].motionBlur && motionMat && items.length;
      // Computed once and shared with the ghosts below (2026-07 fix) — was
      // previously only ever called for the real layer's own push, so a
      // ghost sample always composited plain-Normal/un-effected regardless
      // of this layer's actual blendMode/effects stack. Re-evaluating per
      // ghost at its OWN sample time would be more "correct" for a keyed
      // effect param, but the geometry delta above already only APPROXIMATES
      // each sample (reusing the sharp frame's built items, not a full
      // re-render) — reusing the sharp frame's effects here matches that
      // same established approximation, not a new one.
      var mbEffects = sceneEffectsOf(state.layers[i]);
      if (mbOn) {
        var mbSamples = Math.max(2, Math.min(16, state.motionBlurSamples || 6));
        var mbShutter = Math.max(0.05, Math.min(2, state.motionBlurShutter || 0.5)); // in frames
        for (var s = 1; s <= mbSamples; s++) {
          var t = (s / mbSamples) * mbShutter;
          var ms = SMMotion.layerMotionAt(i, renderFrame - t);
          if (!ms) continue;
          var delta = {
            dx: ms.dx - motionMat.dx, dy: ms.dy - motionMat.dy,
            rot: ms.rot - motionMat.rot,
            sx: motionMat.sx ? ms.sx / motionMat.sx : 1,
            sy: motionMat.sy ? ms.sy / motionMat.sy : 1,
            op: 1, ax: 0, ay: 0,
          };
          // Nothing moved between these two instants — every remaining
          // sample would be an exact copy of the sharp layer, so stop
          // rather than pay for identical draws.
          if (!delta.dx && !delta.dy && !delta.rot && delta.sx === 1 && delta.sy === 1) continue;
          var fade = (1 - s / (mbSamples + 1)) / mbSamples * 2; // trail off toward the tail
          var sampleItems = items.map(function (it) {
            var c = {};
            for (var k in it) if (Object.prototype.hasOwnProperty.call(it, k)) c[k] = it[k];
            if (c.segments) c.segments = roundSegs(SMMotion.transformSegments(c.segments, motionPivot, delta));
            // Retained-path items (pathRef+pathTransform, no segments —
            // CLAUDE.md §5ter) fell through untouched here: c.pathTransform
            // got shallow-copied as-is, so every ghost sat at the EXACT
            // same position as the sharp frame, just faded — stacked
            // duplicates instead of a smear, for what's now the common case
            // (any ordinary animated shape layer once geometry retention is
            // on). Same fix as segments, composed as the OUTERMOST affine —
            // mirrors how motionMat's own affine gets folded into pathTf
            // above (affineMul(newer, older)).
            else if (c.pathTransform) c.pathTransform = affineMul(affineFromMotion(delta, motionPivot), c.pathTransform);
            c.opacity = (c.opacity != null ? c.opacity : 1) * fade;
            return c;
          });
          // blendMode/effects (2026-07 fix): a ghost previously always
          // composited plain Normal with no effects stack, regardless of
          // this layer's own settings — a Screen-blend or Glow-effect layer
          // showed a correctly blended/effected sharp frame trailed by
          // ghosts that ignored both. matteMode deliberately NOT carried:
          // engine.rs resolves a matte source by array adjacency (the layer
          // directly above), and duplicating matteMode across N ghost
          // entries would change which entries sit adjacent to what without
          // a way to verify the Rust-side consequence from here — left as a
          // separate, not-yet-addressed gap rather than guessed at.
          layers.push({ items: sampleItems, blendMode: (bm && bm !== 'normal') ? bm : undefined, effects: mbEffects });
        }
      }
      // 3D layer (2026-07-28) — no special envelope needed: each item's
      // segments were already projected in place (project3D, above), so
      // this layer's `items` push through the EXACT SAME path as any
      // ordinary layer. blendMode/matteMode/effects still apply normally.
      layers.push(userLayerEntries[i] = { items: items, blendMode: (bm && bm !== 'normal') ? bm : undefined, matteMode: (mm && mm !== 'none') ? mm : undefined,
        effects: mbEffects, masks: layerMasks.length ? layerMasks : undefined, maskFeather: layerMaskFeather || undefined });
    }
    // Order (2026-08, "système pour animer l'id index de calque ou de
    // shape/éléments") — layer-level z-stacking. At this exact point
    // `layers` holds EXACTLY one entry per state.layers[i], still in
    // original array order, nothing else pushed yet (background/onion/
    // overlays all come after) — the one safe moment to reorder it. A
    // STABLE sort by each layer's evaluated Order value (default 0 keeps
    // the layer at its natural position, same neutral meaning as CSS
    // z-index) — ties keep original index, so a document that never
    // touches this property renders through unchanged. Matte source
    // resolution further down finds its target via `layers.indexOf`
    // (object identity), which this comment block's own header already
    // documents as immune to exactly this kind of reordering.
    if (_anyLayerOrder) {
      var _orderPairs = layers.map(function (entry, idx) { return { entry: entry, idx: idx, ord: SMMotion.layerOrderAt(idx, renderFrame) }; });
      _orderPairs.sort(function (a, b) { return (a.ord - b.ord) || (a.idx - b.idx); });
      layers = _orderPairs.map(function (p) { return p.entry; });
    }
    // artboard background as the bottom item of a synthetic bottom layer,
    // mirroring drawStage()'s background rect
    var bgItems;
    if (showAlphaChecker) {
      // Fixed 20-column grid regardless of canvas size — a coarse but
      // instantly-recognizable checkerboard (Photoshop/After Effects
      // "show transparency" convention) at a bounded, predictable item
      // count (currently 20x~11 ≈ 220 rects for a 16:9 canvas) rather
      // than one item per fixed-pixel tile, which would scale into the
      // thousands on a large canvas and undercut the exact perf work
      // CLAUDE.md §5 documents for this same render path.
      var checkCols = 20;
      var checkTile = state.canvasW / checkCols;
      var checkRows = Math.max(1, Math.round(state.canvasH / checkTile));
      var checkTileH = state.canvasH / checkRows;
      var checkA = [222, 222, 222, 255], checkB = [255, 255, 255, 255];
      bgItems = [];
      for (var cy = 0; cy < checkRows; cy++) {
        for (var cx = 0; cx < checkCols; cx++) {
          var x0 = cx * checkTile, y0 = cy * checkTileH;
          bgItems.push({
            segments: [
              { point: [x0, y0] }, { point: [x0 + checkTile, y0] },
              { point: [x0 + checkTile, y0 + checkTileH] }, { point: [x0, y0 + checkTileH] },
            ],
            closed: true,
            fillColor: (cx + cy) % 2 === 0 ? checkA : checkB,
            strokeColor: null,
            strokeWidth: 1,
          });
        }
      }
    } else {
      bgItems = [{
        segments: [
          { point: [0, 0] }, { point: [state.canvasW, 0] },
          { point: [state.canvasW, state.canvasH] }, { point: [0, state.canvasH] },
        ],
        closed: true,
        // alpha 0 rather than omitting the rect entirely — mirrors
        // exportBuildFrame's own transparent-export branch (export.js) and
        // its reasoning, and keeps this array's shape identical for the
        // reference-item push just below.
        fillColor: cssColorToRgba(state.canvasBg, alphaBg ? 0 : 1),
        strokeColor: null,
        strokeWidth: 1,
      }];
    }
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
    if (includeEditorOverlays) {
      var onionItems = buildOnionSkinItems();
      if (onionItems.length) layers.splice(1, 0, { items: onionItems });
      var ghostAllItems = buildGhostAllItems();
      if (ghostAllItems.length) layers.splice(1, 0, { items: ghostAllItems });
      var nodeItems = buildNodeHandleItems();
      if (nodeItems.length) layers.push({ items: nodeItems });
      var xformItems = buildTransformBoxItems();
      if (xformItems.length) layers.push({ items: xformItems });
      var hoverBoxItems = buildHoverBoxItems();
      if (hoverBoxItems.length) layers.push({ items: hoverBoxItems });
      var textBoxItems = buildTextDragBoxItems();
      if (textBoxItems.length) layers.push({ items: textBoxItems });
      var marqueeItems = buildMarqueeItems();
      if (marqueeItems.length) layers.push({ items: marqueeItems });
      var fsSelItems = buildFSSelectionItems();
      if (fsSelItems.length) layers.push({ items: fsSelItems });
      var fsBreakItems = buildFSBreakMarkItems();
      if (fsBreakItems.length) layers.push({ items: fsBreakItems });
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
        var rigItems = buildRigPreviewItems();
        if (rigItems.length) layers.push({ items: rigItems });
      }
      var arcItems = buildArcHandleItems();
      if (arcItems.length) layers.push({ items: arcItems });
      var safetyItems = buildSafetyZoneItems();
      if (safetyItems.length) layers.push({ items: safetyItems });
      var guideItems = buildGuideLayerItems();
      if (guideItems.length) layers.push({ items: guideItems });
      var nullItems = buildNullLayerItems();
      if (nullItems.length) layers.push({ items: nullItems });
      var perspectiveItems = window.buildPerspectiveGuideItems ? window.buildPerspectiveGuideItems() : [];
      if (perspectiveItems.length) layers.push({ items: perspectiveItems });
      var symmetryItems = window.buildSymmetryGuideItems ? window.buildSymmetryGuideItems() : [];
      if (symmetryItems.length) layers.push({ items: symmetryItems });
      var gradientGizmoItems = window.buildGradientGizmoItems ? window.buildGradientGizmoItems() : [];
      if (gradientGizmoItems.length) layers.push({ items: gradientGizmoItems });
    }
    // Track matte source resolution (uid-based, 2026-07-31) — runs LAST,
    // after every unshift/splice above, so layers.indexOf gives the final
    // wire position of the source's main entry. A missing/dangling/self-
    // referencing uid leaves matteSourceIndex unset — engine.rs's
    // resolve_matte_source then applies the legacy implicit-i+1 fallback,
    // keeping pre-migration scenes rendering exactly as before.
    for (var mi = 0; mi < state.layers.length; mi++) {
      var mEntry = userLayerEntries[mi];
      if (!mEntry || !mEntry.matteMode) continue;
      var mUid = state.layers[mi].matteSourceLayerUid;
      if (!mUid) continue;
      var mSrcIdx = -1;
      for (var mj = 0; mj < state.layers.length; mj++) {
        if (mj !== mi && state.layers[mj].layerUid === mUid) { mSrcIdx = mj; break; }
      }
      if (mSrcIdx < 0 || !userLayerEntries[mSrcIdx]) continue;
      var mFinal = layers.indexOf(userLayerEntries[mSrcIdx]);
      if (mFinal >= 0) mEntry.matteSourceIndex = mFinal;
    }
    var frameForFx = renderFrame || 0;
    var fpsForFx = Math.max(1, state.fps || 24);
    var _sceneOut = JSON.stringify({ time: frameForFx / fpsForFx, layers: layers });
    // Closes the build: `_imgUsedThisBuild` is now exactly what this frame
    // draws, which is the only moment eviction can be decided safely.
    enforceImageBudget();
    _imgUsedThisBuild = null;
    return _sceneOut;
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
    // 3D layers (2026-07-30 fix, QA sweep) — onionPrevLayer/onionNextLayer/
    // ghostAllLayer are always a ghosted copy of the CURRENTLY ACTIVE layer
    // specifically (renderOS/renderGhostAll, tweens.js, both key off
    // state.activeLayerIdx), so unlike buildSceneJson's own per-item 3D
    // branch (which has to handle every layer), this only ever needs ONE
    // projector for the whole call. Without it, a 3D layer's onion-skin/
    // Ghost-All ghosts rendered flat/unprojected while the real current-
    // frame content next to them rendered correctly projected — visibly
    // misaligned. Exact same pattern as buildRigPreviewItems' own 3D branch
    // a few hundred lines down in this file (same "active layer only"
    // scope), reused rather than re-derived.
    var activeLd = state.layers[state.activeLayerIdx];
    var is3DActive = !!(activeLd && activeLd.threeD);
    var bounds3D = (is3DActive && userLayers[state.activeLayerIdx]) ? userLayers[state.activeLayerIdx].bounds : null;
    var onionProjector3D = (is3DActive && window.SMMotion && SMMotion.make3DProjector && bounds3D) ? SMMotion.make3DProjector(activeLd, bounds3D, state.currentFrame, state.canvasW, state.canvasH) : null;
    layer.children.forEach(function (c) {
      // Same Shadow Brush guide-line filter as buildSceneJson() above — an
      // onion-skin/Ghost-All ghost of a shadow-tagged guide line shouldn't
      // reappear just because the CURRENT frame's own copy got hidden.
      if (!state.showShadowGuides && c.data && c.data.channelTag === 'shadow') return;
      // Same Raster handling as buildSceneJson() above — an imported image/
      // video-frame ghosted onto the onion-skin layer (desR, tweens.js
      // renderOS) was silently dropped here, same "new item type not
      // handled everywhere" gap as the CompoundPath case right below.
      if (c instanceof Raster) {
        var imageId = registerRasterIfNeeded(c);
        if (imageId) {
          var rb = rasterImageRect(c); // same rotation-aware rect as buildSceneJson's own Raster branch
          if (onionProjector3D) rb = SMMotion.project3DImageRect(rb, onionProjector3D);
          items.push({ image: { imageId: imageId, x: rb.x, y: rb.y, width: rb.width, height: rb.height, opacity: c.opacity !== undefined ? c.opacity : 1, rotation: rb.rotation || 0 } });
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
        if (onionProjector3D) sd.segments = SMMotion.project3DSegments(sd.segments, onionProjector3D);
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
          // typeof-guarded — same Option<String> boundary as the main
          // per-item loop above (buildSceneJson).
          strokeCap: typeof c.strokeCap === 'string' ? c.strokeCap : undefined,
          strokeJoin: typeof c.strokeJoin === 'string' ? c.strokeJoin : undefined,
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

  // ---- Guide layers (2026-08, AE feature audit 8.6 — "guides as a real
  // layer object": rotatable, parentable, colored, unlike a classic
  // ruler-drag guide) ----
  // A guide layer has no content of its own (see the isGuideLayer skip in
  // the main per-layer loop) — its line is entirely derived from the SAME
  // Transform properties every other layer already has: ld.guidePos (a
  // WORLD anchor point, defaults to canvas center) is the base the layer's
  // own Position track offsets, and Rotation sets the line's angle
  // (0°/horizontal by default, +90° if guideOrientation is 'vertical') —
  // zero new keyframe machinery, reuses layerMotionAt/parentChainMats
  // exactly like an ordinary layer's own transform chain, so parenting a
  // guide to an animated layer (or keying the guide itself) just works.
  // Drawn as a single line spanning well past the canvas (SPAN, same
  // "comfortably past any realistic content" convention as
  // CLIP_MASK_SPAN) — never part of the real/exported scene.
  function buildGuideLayerItems() {
    if (!window.SMMotion) return [];
    var items = [], SPAN = 100000, frame = state.currentFrame;
    for (var i = 0; i < state.layers.length; i++) {
      var gld = state.layers[i];
      if (!gld.isGuideLayer || gld.visible === false) continue;
      var basePos = gld.guidePos || [state.canvasW / 2, state.canvasH / 2];
      // layerMotionAt just reads the layer's OWN motion/motionStatic dict
      // (computeMotionMat) — no bounds/pivot needed to call it, that's only
      // required by callers that go on to transform CONTENT around a
      // pivot (ordinary layers). A guide has none, so its own dx/dy/rot
      // apply directly as a flat offset/rotation on the anchor point.
      var ownMat = SMMotion.layerMotionAt(i, frame);
      var angleRad = ((gld.guideOrientation === 'vertical' ? 90 : 0) + (ownMat ? ownMat.rot : 0)) * Math.PI / 180;
      var pt = [{ point: [basePos[0] + (ownMat ? ownMat.dx : 0), basePos[1] + (ownMat ? ownMat.dy : 0)], handleIn: [0, 0], handleOut: [0, 0] }];
      var parentChain = SMMotion.parentChainMats(i, frame);
      for (var pc = 0; pc < parentChain.length; pc++) {
        pt = SMMotion.transformSegments(pt, parentChain[pc].pivot, parentChain[pc].mat);
        angleRad += (parentChain[pc].mat.rot || 0) * Math.PI / 180;
      }
      var cx = pt[0].point[0], cy = pt[0].point[1];
      var ex = Math.cos(angleRad) * SPAN, ey = Math.sin(angleRad) * SPAN;
      var col = cssColorToRgba(gld.color || '#00baff', 1) || [0, 186, 255, 255];
      items.push({
        segments: [{ point: [cx - ex, cy - ey], handleIn: [0, 0], handleOut: [0, 0] }, { point: [cx + ex, cy + ey], handleIn: [0, 0], handleOut: [0, 0] }],
        closed: false, fillColor: null, strokeColor: col, strokeWidth: Math.max(1, 1 / view.zoom),
      });
    }
    return items;
  }
  // ---- Null layers (2026-08, feedback #59 — a Null Object layer had zero
  // canvas presence: no marker to click/drag, so parenting or centering one
  // required editing raw fields). Same non-content-layer pattern as a guide
  // layer just above: position is ld.nullPos (a WORLD anchor, defaults to
  // canvas center — see addNullLayer, timeline.js, which also auto-centers
  // it on the layers pre-selected at creation time), composed through the
  // layer's OWN Motion transform + full parent chain exactly like any
  // ordinary layer's pivot (layerMotionAt/parentChainMats — a Null has no
  // bounds/pivot of its own to worry about, same simplification as a
  // guide's flat dx/dy offset). Drawn as a small SCREEN-CONSTANT-SIZE
  // marker (divided by view.zoom, same convention as circleItem's pressure
  // cursor above) — a Null has no real-world extent, so scaling its marker
  // with canvas zoom would be meaningless. Shape (ld.nullShape) is cosmetic
  // only, to help tell several Nulls apart at a glance.
  function buildNullLayerItems() {
    if (!window.SMMotion) return [];
    var items = [], frame = state.currentFrame, HS = 12 / view.zoom, sw = 1.5 / view.zoom;
    for (var i = 0; i < state.layers.length; i++) {
      var nld = state.layers[i];
      if (!nld.isNullLayer || nld.visible === false) continue;
      var basePos = nld.nullPos || [state.canvasW / 2, state.canvasH / 2];
      var ownMat = SMMotion.layerMotionAt(i, frame);
      var pt = [{ point: [basePos[0] + (ownMat ? ownMat.dx : 0), basePos[1] + (ownMat ? ownMat.dy : 0)], handleIn: [0, 0], handleOut: [0, 0] }];
      var parentChain = SMMotion.parentChainMats(i, frame);
      for (var pc = 0; pc < parentChain.length; pc++) {
        pt = SMMotion.transformSegments(pt, parentChain[pc].pivot, parentChain[pc].mat);
      }
      var cx = pt[0].point[0], cy = pt[0].point[1];
      var col = cssColorToRgba(nld.color || '#ff2d78', 1) || [255, 45, 120, 255];
      var shape = nld.nullShape || 'cross';
      if (shape === 'circle') {
        items.push(circleItem(cx, cy, HS, null, col, sw));
      } else if (shape === 'square') {
        items.push(rectItem(cx, cy, HS, null, col, sw));
      } else if (shape === 'diamond') {
        var dseg = [{ point: [cx, cy - HS] }, { point: [cx + HS, cy] }, { point: [cx, cy + HS] }, { point: [cx - HS, cy] }];
        items.push({ segments: roundSegs(dseg), closed: true, fillColor: null, strokeColor: col, strokeWidth: sw });
      } else {
        items.push(lineItem([cx - HS, cy], [cx + HS, cy], col, sw));
        items.push(lineItem([cx, cy - HS], [cx, cy + HS], col, sw));
      }
    }
    return items;
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
  // Animation 2D shape hover box (2026-08, feedback: "roll hover n'existe
  // pas sur animation 2D" — Motion mode's own hover box, SMMotion's
  // hoverOverlayItems, already did this; this is the same idea for a plain
  // shape hovered on the active/other layer). select-bridge.js owns the
  // hit-testing/hover state (onHoverMoveA2D/_hoverPathA2D — same split as
  // getMultiLayerBox above), this just turns its bounds into a draw item —
  // deliberately a SEPARATE function from buildTransformBoxItems below,
  // since a hover box must show even with nothing selected/tool idle.
  function buildHoverBoxItems() {
    if (state.appMode === 'motion' || !window.SMSelectBridge || !SMSelectBridge.getHoverBounds) return [];
    var hb = SMSelectBridge.getHoverBounds();
    if (!hb) return [];
    var hzs = 1 / view.zoom;
    // Same solid-blue color/weight as Motion's own hover box (hoverOverlayItems,
    // motion.js) — confirmed via a temporary oversized magenta stroke that the
    // pipeline draws correctly; 1.5px (vs Motion's 1px) since this box often
    // coincides exactly with the shape's own edge and needs to read over both
    // the shape's fill and the page background.
    var col = [74, 158, 255, 220], sw = 1.5 * hzs;
    var b = hb.b;
    // hb is now an ORIENTED box (orientedBoxForPath, tools.js): {b,angle,pivot}
    // in the shape's own de-rotated space — same shape as orientedSelBox's
    // return, so a rotated shape draws its true rotated outline instead of
    // an axis-aligned box that no longer matches its size/rotation (2026-08
    // fix, "la box du hover ne correspond pas à la forme du bounding box").
    if (!hb.angle) return [boundsRectItem(b.left, b.top, b.right, b.bottom, null, col, sw)];
    var c1 = selBoxPt(b.left, b.top, hb), c2 = selBoxPt(b.right, b.top, hb);
    var c3 = selBoxPt(b.right, b.bottom, hb), c4 = selBoxPt(b.left, b.bottom, hb);
    return [
      lineItem([c1.x, c1.y], [c2.x, c2.y], col, sw),
      lineItem([c2.x, c2.y], [c3.x, c3.y], col, sw),
      lineItem([c3.x, c3.y], [c4.x, c4.y], col, sw),
      lineItem([c4.x, c4.y], [c1.x, c1.y], col, sw),
    ];
  }
  // Text tool's live bounding box (2026-08, "quand on dessine le rectangle
  // du texte il faut lui faire apparaître le texte... avec un bounding box
  // comme dans tout éditeur de texte") — TWO sources, drawn identically:
  // _textDragRect (tools.js) while dragging out the initial placement box
  // (a real Paper Path in marqueeLayer — was already being built, but
  // NEVER reached the screen: it only ever lived in the Paper.js model,
  // invisible the moment the Rust engine took over rendering, CLAUDE.md §5),
  // and window._inplaceTextBoxBounds (timeline.js's openInPlaceTextEditor)
  // while the in-canvas textarea is open, tracking its live-growing size.
  // Solid, not dashed — same pre-existing engine limitation buildTransformBoxItems'
  // own comment already notes (the engine's Stroke type has no dash support yet).
  function buildTextDragBoxItems() {
    var items = [];
    var col = [255, 184, 108, 230], sw = 1 / view.zoom;
    if (window._textDragRect && !window._textDragRect.removed) {
      var b = window._textDragRect.bounds;
      items.push(boundsRectItem(b.left, b.top, b.right, b.bottom, [255, 184, 108, 15], col, sw));
    }
    if (window._inplaceTextBoxBounds) {
      var tb = window._inplaceTextBoxBounds;
      items.push(boundsRectItem(tb.left, tb.top, tb.right, tb.bottom, null, col, sw));
    }
    return items;
  }
  function buildTransformBoxItems() {
    if (state.tool !== 'select') return [];
    // Motion mode (2026-08-21 fix, "2 points d'ancrage et 2 rotation qui
    // s'affiche" on a Component with several elements): this function is
    // the Animation 2D transform box — corners/ring/anchor computed from
    // select-bridge.js's computeHandles(), which knows nothing about a
    // Motion layer's own Anchor Point offset (motion.js's separate anchor/
    // position/rotation/scale system). SMMotion.buildOverlayItems() below
    // (same caller, a few lines down) already draws motion.js's OWN
    // complete box/ring/anchor/position-dots/vertex/3D-gizmo overlay for
    // whatever's Motion-selected — nothing gated either one off from firing
    // together, and selectedPaths ends up populated in Motion mode too
    // (layer-row selection backs it the same as an Animation 2D pick), so
    // both fired at once: TWO rings and TWO anchor crosshairs, one pair
    // frozen at the plain geometric bounds center (this function, blind to
    // the Anchor Point offset) and one pair correctly following it
    // (motion.js's own). Confirmed live: dragging the Motion anchor moved
    // only ONE of the two crosshairs, leaving the other stranded at the
    // shape's un-offset center — exactly the reported symptom.
    if (state.appMode === 'motion') return [];
    // Timeline multi-layer selection in Animation 2D. The selection bridge
    // owns the exact same bounds/hit-test data; render it here ahead of the
    // ordinary selectedPaths box so the user sees one transform target for
    // all selected rows instead of a misleading box on the active layer.
    var ml = window.SMSelectBridge && SMSelectBridge.getMultiLayerBox ? SMSelectBridge.getMultiLayerBox() : null;
    if (ml) {
      var mb = ml.bounds, mzs = 1 / view.zoom, mItems = [];
      mItems.push(boundsRectItem(mb.left, mb.top, mb.right, mb.bottom, null, [74, 158, 255, 220], 1.2 * mzs));
      [mb.topLeft, mb.topRight, mb.bottomRight, mb.bottomLeft].forEach(function (p) {
        mItems.push(rectItem(p.x, p.y, 3.8 * mzs, [255, 255, 255, 255], [74, 158, 255, 255], 1.2 * mzs));
      });
      mItems.push(circleItem(ml.pivot.x, ml.pivot.y, ml.ringRadius, null, [74, 158, 255, 180], 1.1 * mzs));
      return mItems;
    }
    // Selected native-video layer (2026-07, "une vidéo est un objet comme
    // les autres") — same visual language as the path gizmo below (blue
    // outline, white corner squares, rotate ring), geometry from the ONE
    // shared SMNativeVideo.transformBox select-bridge also hit-tests, so
    // grabbed == drawn by construction. Videos have no edge-midpoint
    // handles (uniform corner scale only) and no anchor crosshair (their
    // pivot is the rect center by displayRect's own convention).
    if (window._nvSelectedLayer != null && window.SMNativeVideo && SMNativeVideo.transformBox) {
      var nvLd = state.layers[window._nvSelectedLayer];
      var nvTb = (nvLd && nvLd.nativeVideo) ? SMNativeVideo.transformBox(window._nvSelectedLayer) : null;
      if (nvTb) {
        var nzs = 1 / view.zoom;
        var nvItems = [];
        var nvC = [nvTb.corners.nw, nvTb.corners.ne, nvTb.corners.se, nvTb.corners.sw];
        for (var nvI = 0; nvI < 4; nvI++) {
          var pA = nvC[nvI], pB = nvC[(nvI + 1) % 4];
          nvItems.push(lineItem([pA.x, pA.y], [pB.x, pB.y], [74, 158, 255, 204], 1 * nzs));
        }
        nvC.forEach(function (p) {
          nvItems.push(rectItem(p.x, p.y, 3.5 * nzs, [255, 255, 255, 255], [74, 158, 255, 255], 1.2 * nzs));
        });
        nvItems.push(circleItem(nvTb.ringCenter.x, nvTb.ringCenter.y, nvTb.ringRadius, null, [74, 158, 255, 160], 1 * nzs));
        return nvItems;
      }
    }
    if (!selectedPaths.length || typeof orientedSelBox !== 'function') return [];
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
    // Live corner-pin distort (2026-07 feedback: "la bounding box ne
    // reflete pas cette transformation") — select-bridge.js's distortSrcQuad/
    // distortDstQuad are already past selBoxPt (geometry space, same as the
    // dragged segments themselves), so only the Motion map (not selBoxPt
    // again) still needs applying. Draws the ACTUAL warped quad + only its
    // 4 corners (no edge-midpoints/anchor/rotate-ring — none are meaningful
    // mid-perspective-warp) instead of the static pre-distort rectangle,
    // and highlights the corner actually being dragged.
    var liveDistort = window.SMSelectBridge && SMSelectBridge.getDistortState();
    if (liveDistort && liveDistort.quad) {
      function WG(pt) { return nvMap ? nvMap.fwd(pt.x, pt.y) : [pt.x, pt.y]; }
      var dq = liveDistort.quad;
      var dc = { nw: WG(dq.nw), ne: WG(dq.ne), se: WG(dq.se), sw: WG(dq.sw) };
      var dItems = [
        lineItem(dc.nw, dc.ne, [74, 158, 255, 204], 1 * zs),
        lineItem(dc.ne, dc.se, [74, 158, 255, 204], 1 * zs),
        lineItem(dc.se, dc.sw, [74, 158, 255, 204], 1 * zs),
        lineItem(dc.sw, dc.nw, [74, 158, 255, 204], 1 * zs),
      ];
      Object.keys(dc).forEach(function (k) {
        var isActive = k === liveDistort.dir;
        var p = dc[k];
        dItems.push(rectItem(p[0], p[1], (isActive ? 4.5 : 3.5) * zs, [255, 255, 255, 255], isActive ? [255, 159, 10, 255] : [74, 158, 255, 255], 1.2 * zs));
      });
      return dItems;
    }
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
      // Ctrl-hover corner-pin affordance (2026-07 feedback: "qd ctrl est
      // appuyé voir une différence visuelle sur les corner") — same accent
      // color as the active-drag highlight above, shown BEFORE any drag
      // starts so the user knows which corner Ctrl+drag would distort.
      var isDistortHover = state.xformDistortHoverDir === k;
      items.push(rectItem(p[0], p[1], (isDistortHover ? 4.5 : 3.5) * zs, [255, 255, 255, 255], isDistortHover ? [255, 159, 10, 255] : [74, 158, 255, 255], 1.2 * zs));
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
    // Dynamic shapes phase 3 (2026-08-18) — corner-radius drag handles for
    // a single selected rect with data.paramShape, orange to read as a
    // DIFFERENT kind of grip from the blue transform-box handles just
    // above (same color the mask/trim features already use for a
    // per-shape, non-transform control). Positions come from the SAME
    // select-bridge.js helper the hit-test uses (SMParamShapeHandles), so
    // drawn == grabbable by construction, never two independently
    // maintained copies of this math.
    if (window.SMParamShapeHandles) {
      var pshpSel = window.SMParamShapeHandles.paramShapeSelectionSingle();
      if (pshpSel) {
        var hpDraw = window.SMParamShapeHandles.cornerHandleWorldPositions(pshpSel);
        window.SMParamShapeHandles.handleNamesFor(pshpSel).forEach(function (c) {
          var hp = hpDraw[c];
          items.push(circleItem(hp.x, hp.y, 4 * zs, [255, 184, 108, 255], [255, 255, 255, 255], 1.2 * zs));
        });
      }
    }
    return items;
  }
  function buildMarqueeItems() {
    if (_marquee.active && _marquee.rect) {
      // 2026-07 ("le lasso... pendant le drag j'ai le rect de selection box
      // pas une selection visuelle libre même si après le relâchement j'ai
      // bien ma forme dessinée de select") — this always drew the
      // marquee's BOUNDING BOX (.bounds) regardless of its actual shape,
      // for BOTH select-bridge's own Alt-drag lasso and fsselect's
      // toggle-driven one (both share this one live-overlay chokepoint) —
      // never updated when lasso mode was added. The final hit-test
      // resolution (tools.js/select-bridge.js pointerup) already reads the
      // real freehand segments correctly; only this live preview collapsed
      // it to a rectangle. Serialize the actual traced points instead.
      if (_marquee.lasso && _marquee.rect.segments.length > 1) {
        var lassoPts = _marquee.rect.segments.map(function (s) { return { point: [s.point.x, s.point.y] }; });
        return [{ segments: roundSegs(lassoPts), closed: false, fillColor: [74, 158, 255, 20], strokeColor: [74, 158, 255, 230], strokeWidth: 1 / view.zoom }];
      }
      var b = _marquee.rect.bounds;
      return [boundsRectItem(b.left, b.top, b.right, b.bottom, [74, 158, 255, 20], [74, 158, 255, 230], 1 / view.zoom)];
    }
    // Subselect tool's own node-marquee (_nmq, subselect-bridge.js/tools.js)
    // was never drawn when the Rust engine is on (the default) — its rect IS
    // built in Paper.js (marqueeLayer) and the underlying vertex-selection
    // math works fine either way, but nothing here ever read _nmq to put it
    // in the rendered scene, so the drag box itself was completely invisible
    // on screen (found live, "le rec de sélection avec l'outil subselect
    // n'est pas présent" — reproduced by dispatching a real pointer drag and
    // confirming _nodeSel populated correctly with zero visual feedback).
    // Orange (matching _nmq.rect's own Paper-native styling, 'rgba(255,184,
    // 108,...)') rather than the Select tool's blue, so the two marquees
    // stay visually distinct.
    if (_nmq.active && _nmq.rect) {
      var nb = _nmq.rect.bounds;
      // 2026-07-29 fix: _nmq's own corners now live in raw document space
      // (subselect-bridge.js's toLocalPoint — same reasoning as
      // buildNodeHandleItems above), same as everything else this box is
      // compared against (_nodeSel's containment test). Map the 4 corners
      // through the active layer's Motion transform so the drawn box
      // matches where the drag visually happened. Only an axis-aligned
      // approximation (bounding box of the 4 mapped corners) when the
      // layer is also rotated — boundsRectItem has no rotated-quad form —
      // acceptable since the actual selection logic is exact regardless.
      var motionMap2 = (window.SMMotion && SMMotion.layerMotionPointMap) ? SMMotion.layerMotionPointMap(state.activeLayerIdx) : null;
      if (motionMap2) {
        var nCorners = [[nb.left, nb.top], [nb.right, nb.top], [nb.right, nb.bottom], [nb.left, nb.bottom]].map(function (c) { return motionMap2.fwd(c[0], c[1]); });
        var nxs = nCorners.map(function (c) { return c[0]; }), nys = nCorners.map(function (c) { return c[1]; });
        return [boundsRectItem(Math.min.apply(null, nxs), Math.min.apply(null, nys), Math.max.apply(null, nxs), Math.max.apply(null, nys), [255, 184, 108, 20], [255, 184, 108, 230], 1 / view.zoom)];
      }
      return [boundsRectItem(nb.left, nb.top, nb.right, nb.bottom, [255, 184, 108, 20], [255, 184, 108, 230], 1 / view.zoom)];
    }
    return [];
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
    if (state.tool !== 'fsselect' || !_fsSel || !_fsSel.length || typeof fsHighlightPath !== 'function') return [];
    // Multi-select (2026-07) — draw every selected entry's own highlight,
    // not just one.
    var out = [];
    _fsSel.forEach(function (sel) {
      var hl = fsHighlightPath(sel);
      if (!hl || !hl.segments || !hl.segments.length) { if (hl) hl.remove(); return; }
      var segs = hl.segments.map(function (s) { return { point: [s.point.x, s.point.y], handleIn: [s.handleIn.x, s.handleIn.y], handleOut: [s.handleOut.x, s.handleOut.y] }; });
      var closed = !!hl.closed;
      var zs = 1 / view.zoom;
      if (sel.kind === 'fill' || sel.kind === 'fillregion') {
        var hatchLines = closed ? fsHatchClipLines(hl, 9 * zs, 45) : [];
        hl.remove();
        out.push({ segments: roundSegs(segs), closed: closed, fillColor: [255, 152, 0, 45], strokeColor: [255, 152, 0, 230], strokeWidth: 2 * zs, dashPattern: [4 * zs, 3 * zs] });
        hatchLines.forEach(function (ln) {
          out.push({
            segments: [{ point: [ln[0], ln[1]], handleIn: [0, 0], handleOut: [0, 0] }, { point: [ln[2], ln[3]], handleIn: [0, 0], handleOut: [0, 0] }],
            closed: false, fillColor: null, strokeColor: [255, 152, 0, 150], strokeWidth: 1 * zs,
          });
        });
      } else {
        hl.remove();
        out.push({ segments: roundSegs(segs), closed: closed, fillColor: null, strokeColor: [255, 152, 0, 230], strokeWidth: ((sel.path.strokeWidth || 2) + 4) * zs, dashPattern: [5 * zs, 4 * zs] });
      }
    });
    return out;
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

  // Fill/Stroke Select — "break at intersections" brush (2026-08). While
  // dragging in this mode (tools.js), every crossing point currently under
  // the brush gets pushed here each pointermove so it renders live as a
  // small dot BEFORE the actual cut commits on pointerup — same "preview
  // now, commit later" shape as the eraser cursor above, just markers
  // instead of a single circle. Cleared (empty array) on pointerup/tool
  // change by the caller.
  var fsBreakMarksWorld = [];
  function setFSBreakMarks(points) { fsBreakMarksWorld = points || []; }
  function buildFSBreakMarkItems() {
    if (state.tool !== 'fsselect' || !fsBreakMarksWorld.length) return [];
    var zs = 1 / view.zoom;
    // Same orange as buildFSSelectionItems' own highlight color — reads as
    // "this tool's own accent" rather than a new unrelated hue.
    return fsBreakMarksWorld.map(function (pt) {
      return circleItem(pt[0], pt[1], 5 * zs, [255, 152, 0, 255], [255, 255, 255, 255], 1.2 * zs);
    });
  }
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
      var lastSeg = _pen.path.lastSegment, last = lastSeg.point;
      // Curved rubber-band, not a straight guess (feedback #38, "on voit
      // les vecteurs et tangentes... comme dans n'importe quel soft de
      // vecto") — Illustrator/AE preview the NEXT segment as it would
      // actually render if you clicked now: a real cubic curve carrying the
      // last anchor's own handleOut, ending flat into the cursor (handleIn
      // [0,0], since a plain hover has no next-anchor handle to show yet —
      // only a click-drag, live in onMove already via seg.handleOut, would
      // add one). When the last anchor has no handleOut (a plain corner
      // point) both handles are zero and this reduces to exactly the same
      // straight line the old lineItem drew, so this is a strict upgrade,
      // not a behavior change for the common straight-segment case.
      items.push({
        segments: [
          { point: [last.x, last.y], handleIn: [0, 0], handleOut: [lastSeg.handleOut.x, lastSeg.handleOut.y] },
          { point: [penPreviewWorld[0], penPreviewWorld[1]], handleIn: [0, 0], handleOut: [0, 0] },
        ],
        closed: false, fillColor: null, strokeColor: [120, 170, 255, 153], strokeWidth: 1 * zs,
      });
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

  // Rig tool (rig-bridge.js) — bones live as plain JSON on ld.rig.bones,
  // never inserted into the real Paper layer (that would make them ordinary
  // artwork geometry — see rig-bridge.js's own header comment), so unlike
  // everything else rendered here they can't be read off layer.children.
  // A bone's stored segment shape ({point,handleIn,handleOut}) already
  // matches an overlay item's own `segments` field one-for-one, so a
  // finished bone drops straight into an item literal with zero conversion;
  // only the anchor/tangent-handle markers need building, mirroring
  // buildPenPreviewItems' own loop above. `_rigDraw` (rig-bridge.js) is a
  // bare global exactly like tools.js's `_pen` — same reason: this function
  // needs to read the in-progress WIP path while it's still being drawn,
  // before it's copied into ld.rig.bones at finalize time.
  var rigPreviewWorld = null;
  function setRigPreview(worldPt) { rigPreviewWorld = worldPt; }
  function buildRigPreviewItems() {
    if (state.tool !== 'rig') return [];
    var ld = state.layers[state.activeLayerIdx];
    if (!ld || !ld.rig) return [];
    var zs = 1 / view.zoom;
    var items = [];
    var boneCol = [255, 200, 60, 235], handleCol = [255, 200, 60, 178];
    // 2026-07-29 fix — same story as buildNodeHandleItems' own toRendered a
    // bit below in this file (see its comment, and rig-bridge.js's
    // toLocalPoint): bone.segments/_rigDraw.path live in raw document space,
    // but the shape they deform renders through the active layer's own
    // Motion transform. This overlay used to draw the bone/handles/influence
    // circle straight from raw coordinates, so on any layer with an active
    // Motion Scale/Rotation/Position the rig guide visibly sat in the wrong
    // place relative to the shape it's actually editing. transformSegments
    // (motion.js) is the SAME point+handle-vector transform buildSceneJson's
    // own pathTransform composes elsewhere — reused here instead of hand-
    // rolling the math a second time. Radius uses the average of sx/sy: an
    // exact ellipse would need a different render primitive than a circle,
    // and Motion "zoom" is overwhelmingly a uniform scale in practice.
    // 3D layers (2026-07-29 fix) — the 2D affine map/transformSegments pair
    // above don't apply here at all (make3DProjector's forward map is a
    // true perspective projection); project3DSegments (motion.js, already
    // used by buildSceneJson's own 3D branch) is the correct per-vertex
    // equivalent of transformSegments for this case.
    var activeLd = state.layers[state.activeLayerIdx];
    var is3DActive = !!(activeLd && activeLd.threeD);
    var bounds3D = (is3DActive && userLayers[state.activeLayerIdx]) ? userLayers[state.activeLayerIdx].bounds : null;
    var projector3D = (is3DActive && window.SMMotion && SMMotion.make3DProjector && bounds3D) ? SMMotion.make3DProjector(activeLd, bounds3D, state.currentFrame, state.canvasW, state.canvasH) : null;
    var motionMap = (!projector3D && window.SMMotion && SMMotion.layerMotionPointMap) ? SMMotion.layerMotionPointMap(state.activeLayerIdx) : null;
    function toRenderedSegs(segs) {
      if (projector3D) return SMMotion.project3DSegments(segs, projector3D);
      return motionMap ? SMMotion.transformSegments(segs, motionMap.pivot, motionMap.mat) : segs;
    }
    function toRendered(x, y) {
      if (projector3D) { var p = projector3D(x, y); return [p.x, p.y]; }
      return motionMap ? motionMap.fwd(x, y) : [x, y];
    }
    // No single scale factor exists under perspective — approximated by
    // probing the actual projected distance from the circle's own (already-
    // rendered) center to one point on its rim. Fine for a rough visual
    // guide (never hit-tested against, never exported).
    function toRenderedRadius(r, cx, cy, renderedC) {
      if (projector3D) { var rim = projector3D(cx + r, cy); return Math.hypot(rim.x - renderedC[0], rim.y - renderedC[1]); }
      return motionMap ? r * (motionMap.mat.sx + motionMap.mat.sy) / 2 : r;
    }
    function pushHandles(pt, hi, ho) {
      if (Math.hypot(hi[0], hi[1]) > 0.5) {
        var hiPt = [pt[0] + hi[0], pt[1] + hi[1]];
        items.push(lineItem(pt, hiPt, handleCol, 1 * zs));
        items.push(rectItem(hiPt[0], hiPt[1], 3 * zs, [255, 255, 255, 255], boneCol, 1 * zs));
      }
      if (Math.hypot(ho[0], ho[1]) > 0.5) {
        var hoPt = [pt[0] + ho[0], pt[1] + ho[1]];
        items.push(lineItem(pt, hoPt, handleCol, 1 * zs));
        items.push(rectItem(hoPt[0], hoPt[1], 3 * zs, [255, 255, 255, 255], boneCol, 1 * zs));
      }
    }
    // Influence circles (2026-07-29, Assigner mode — "les box d'influences
    // comme dans Shapper sur les vecteurs que tu peux modifier si
    // l'assignement automatique ne fonctionne pas") — one dashed ring per
    // bone, centered on its own bounding-box center (rig-bridge.js's
    // boneCircleCenter), radius = bone.radius||panel default. Drag the ring
    // ITSELF (rig-bridge.js's hitRadiusHandle) to resize; release re-runs
    // auto-assign so the deformation reflects the new radius immediately.
    var showInfluence = state.rigSubMode === 'assign';
    var influenceCol = [255, 120, 220, 200];
    var panelRadiusEl = document.getElementById('rig-weight-radius');
    var panelDefault = (panelRadiusEl && parseFloat(panelRadiusEl.value)) || 200;
    Object.keys(ld.rig.bones).forEach(function (bid) {
      var bone = ld.rig.bones[bid];
      if (!bone.segments || bone.segments.length < 2) return;
      // The bone currently being (re-)drawn renders from the live Paper
      // Path below instead (it has a rubber-band cursor line the stored
      // JSON copy doesn't have yet) — skip its stored copy here to avoid
      // drawing it twice.
      if (typeof _rigDraw !== 'undefined' && _rigDraw.path && _rigDraw.boneId === bid) return;
      var renderedSegs = toRenderedSegs(bone.segments);
      items.push({ segments: roundSegs(renderedSegs), closed: !!bone.closed, fillColor: null, strokeColor: boneCol, strokeWidth: 2 * zs });
      renderedSegs.forEach(function (s) {
        pushHandles(s.point, s.handleIn || [0, 0], s.handleOut || [0, 0]);
        items.push(circleItem(s.point[0], s.point[1], 3.5 * zs, [255, 255, 255, 255], boneCol, 1.2 * zs));
      });
      if (showInfluence && window.SMRig) {
        var c0 = SMRig.boneCircleCenter(bone);
        var r0 = SMRig.boneRadiusOf(bone, panelDefault);
        var c = toRendered(c0.x, c0.y);
        var r = toRenderedRadius(r0, c0.x, c0.y, c);
        items.push({ segments: [{ point: [c[0] - 4 * zs, c[1]] }, { point: [c[0] + 4 * zs, c[1]] }], closed: false, fillColor: null, strokeColor: influenceCol, strokeWidth: 1.4 * zs });
        items.push({ segments: [{ point: [c[0], c[1] - 4 * zs] }, { point: [c[0], c[1] + 4 * zs] }], closed: false, fillColor: null, strokeColor: influenceCol, strokeWidth: 1.4 * zs });
        items.push(circleItem(c[0], c[1], r, null, influenceCol, 1.6 * zs));
      }
    });
    if (typeof _rigDraw !== 'undefined' && _rigDraw.path) {
      if (rigPreviewWorld) {
        var lastRaw = _rigDraw.path.lastSegment.point;
        var lastRendered = toRendered(lastRaw.x, lastRaw.y);
        items.push(lineItem(lastRendered, rigPreviewWorld, [255, 220, 130, 153], 1 * zs));
      }
      toRenderedSegs(_rigDraw.path.segments.map(function (s) { return { point: [s.point.x, s.point.y], handleIn: [s.handleIn.x, s.handleIn.y], handleOut: [s.handleOut.x, s.handleOut.y] }; })).forEach(function (s) {
        pushHandles(s.point, s.handleIn, s.handleOut);
        items.push(circleItem(s.point[0], s.point[1], 3.5 * zs, [255, 255, 255, 255], boneCol, 1.2 * zs));
      });
    }
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
    // 2026-07-29 fix — see subselect-bridge.js's toLocalPoint comment for
    // the full story: path.segments/nodeEditSegmentsData live in raw
    // document space, but the shape itself renders through the active
    // layer's own Motion transform (buildSceneJson's pathTransform, applied
    // to THIS layer's items a few hundred lines above in this same
    // function). Map every handle point through the SAME transform here so
    // the overlay actually sits on the shape it's editing instead of at its
    // pre-transform position.
    var motionMap = (window.SMMotion && SMMotion.layerMotionPointMap) ? SMMotion.layerMotionPointMap(state.activeLayerIdx) : null;
    // 3D layers (2026-07-29 fix) — layerMotionPointMap returns null for a
    // 3D-toggled layer even with real rotationX/rotationY set; layerMotion3DPointMap
    // is the dedicated perspective-correct counterpart (motion.js).
    if (!motionMap && window.SMMotion && SMMotion.layerMotion3DPointMap) motionMap = SMMotion.layerMotion3DPointMap(state.activeLayerIdx);
    function toRendered(x, y) { return motionMap ? motionMap.fwd(x, y) : [x, y]; }
    segs.forEach(function (s, i) {
      var pt = toRendered(s.point[0], s.point[1]);
      var hi = s.handleIn, ho = s.handleOut;
      var hiLen = Math.hypot(hi[0], hi[1]), hoLen = Math.hypot(ho[0], ho[1]);
      if (hiLen > 0.5) {
        var hiPt = toRendered(s.point[0] + hi[0], s.point[1] + hi[1]);
        items.push(lineItem(pt, hiPt, [120, 170, 255, 178], 1 * zs));
        items.push(rectItem(hiPt[0], hiPt[1], 3 * zs, [255, 255, 255, 255], [74, 158, 255, 255], 1 * zs));
      }
      if (hoLen > 0.5) {
        var hoPt = toRendered(s.point[0] + ho[0], s.point[1] + ho[1]);
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
    // effect_zoom deliberately excludes `scale` (DPR) so a pixel-space
    // effect stays the SAME apparent size on screen as document/CSS zoom
    // changes — but _previewRenderScale (2026-08, feedback #60 part 2) is a
    // DIFFERENT axis: unlike DPR, which only makes geometry crisper for the
    // SAME apparent content, reducing render scale makes composite_scene's
    // target texture genuinely SMALLER for the SAME CSS box. Leaving
    // effect_zoom unscaled would keep a blur's radius pinned to a FIXED
    // texel count while the canvas shrinks around it — the SAME setting
    // would visibly look blurrier at a lower render scale (measured live:
    // a 10-texel transition width is 0.88% of a 1134px-wide canvas but
    // 1.47% of a 680px one — the same texels covering more of a smaller
    // picture). Multiplying it in here keeps a blur's on-screen size
    // constant across any render scale, exactly like it already is
    // between the live preview and a native-resolution export.
    engine.set_viewport(panAdjX, panAdjY, z, state.canvasRotation || 0, pivotWX, pivotWY, view.zoom * _previewRenderScale);
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
          flushRetiredPaths();
          var _renderT0 = _calibDone ? 0 : performance.now();
          engine.render(json);
          if (!_calibDone) recordCalibSample(performance.now() - _renderT0);
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
    // Compared against the NATIVE size applyEngineSize last saw, not
    // engineW/engineH directly — those are the SCALED values once
    // _previewRenderScale < 1, which would never equal the native w/h
    // again and make this guard permanently false (re-resizing on every
    // spurious duplicate ResizeObserver firing instead of just real ones).
    if (w === _nativeEngineW && h === _nativeEngineH) return;
    try {
      applyEngineSize(w, h);
    } catch (e) {
      console.error('[engine-bridge] resize failed, disabling', e);
      setEnabled(false);
      return;
    }
  }

  async function ensureEngine() {
    if (engine) return true;
    if (!window.GeometryWasm || !window.GeometryWasm.ready) {
      showToast(SM.t('toastRustEngineUnavailableWasm'));
      return false;
    }
    var paperCanvas = document.getElementById('drawing-canvas');
    paperCanvasEl = paperCanvas;
    // Same 0-size hazard as handleResize() below, but at creation time: a
    // 0×0 (or 0×N) surface.configure() during create_engine() is just as
    // fatal to the WebGPU surface as during a later resize.
    if (paperCanvas.width <= 0 || paperCanvas.height <= 0) {
      showToast(SM.t('toastRustEngineCanvasNotReady'));
      return false;
    }
    rustCanvas = document.createElement('canvas');
    rustCanvas.id = 'rust-canvas';
    // device-pixel size copied from the Paper canvas so world coordinates
    // line up 1:1; CSS position stacked directly over it. renderScale is
    // always 1.0 here (calibration hasn't run yet — that needs a live
    // engine to measure against), so this is the native size verbatim.
    _nativeEngineW = paperCanvas.width; _nativeEngineH = paperCanvas.height;
    engineW = paperCanvas.width;
    engineH = paperCanvas.height;
    rustCanvas.width = engineW;
    rustCanvas.height = engineH;
    rustCanvas.style.cssText = 'position:absolute;inset:0;width:100%;height:100%;pointer-events:none;display:none;';
    paperCanvas.parentNode.insertBefore(rustCanvas, paperCanvas.nextSibling);
    try {
      engine = await window.GeometryWasm.create_engine(rustCanvas, engineW, engineH);
      // Custom WGSL effects (2026-07) — the wasm engine is a fresh instance
      // every time it's (re-)created, so any project-defined custom
      // effects already sitting in state.customEffects (e.g. "Resume Last
      // Session"/autosave restore, which may load data BEFORE this ever
      // runs) need to be re-registered now, or their pipelines simply
      // won't exist yet and run_one_effect's "custom:" branch would no-op.
      if (window.registerAllCustomEffects) window.registerAllCustomEffects();
      // Retained path store: probe Paper's change flags and, only if the
      // probe passes, start emitting pathRefs (see installGeometryHook).
      installGeometryHook();
      if (window.ResizeObserver) {
        resizeObserver = new ResizeObserver(handleResize);
        resizeObserver.observe(paperCanvas);
      }
      window.addEventListener('resize', handleResize);
      return true;
    } catch (e) {
      console.error('[engine-bridge] engine creation failed', e);
      showToast(SM.t('toastRustEngineWebgpuFailedSuffix') + e);
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
    // Mograph duplicator (2026-07-29): unlike every other Motion property,
    // which applies purely inside buildSceneJson's own per-item matrix
    // (computeMotionMat) and so is always fresh here, the duplicator's
    // dupOffset* properties are read by applyLayerDuplicator (app.js) ONLY
    // when getEffectiveStrokesRendered runs — which only happens inside
    // loadFrame. Any renderNow() call that isn't preceded by a loadFrame
    // (drag/type a Dup. field, drag a keyframe's value, paste a keyframe,
    // box-skew multiple keys…) would otherwise re-serialize the SAME stale
    // materialized copies — found live, "la duplication n'est pas en temps
    // réel quand on modifie les value". Patching every individual commit
    // path is exactly the whack-a-mole CLAUDE.md §1 warns about; a duplicator
    // layer's own loadFrame is already a no-reuse full rebuild by design
    // (§6's retained-path exclusion), so re-running it here is a correctness
    // fix, not a new cost, and every non-duplicator layer's existing
    // hold-frame reuse (_canReuseMaterialized) keeps this near-free.
    // dupOnly=true (2026-07-29): scopes the rebuild to duplicator layers
    // only — see loadFrame's own header comment for the live bug this fixes
    // (an unscoped call here silently reverted any OTHER layer's in-progress
    // live drag — Subselect, Eraser, Rig pose — on every render tick).
    if (state.layers.some(function (l) { return l.duplicator && !l._dupEditSource; })) loadFrame(state.currentFrame, true);
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

  // ---- Effects-aware frame export (2026-07) ----
  // Ordinary export (exportFrameDataURL, export.js) rasterizes straight
  // from Paper.js's own vector data — correct and fast, but bypasses the
  // WGPU effect stack entirely, so a project using any effect (blur/
  // vignette/glow/ground shadow/contour/threshold/halftone/...) never saw
  // them in its exported PNG/video (feedback 2026-07: "vérifie que le
  // rendu temps réel marche bien avec les effets" surfaced this — the live
  // preview worked fine, but effects turned out to be preview-only).
  //
  // This renders ONE frame through the SAME engine + buildSceneJson the
  // live preview already uses (reusing loadFrame/buildSceneJson keeps this
  // a single source of truth for scene content — no second parallel
  // per-stroke-transform implementation to drift out of sync with, the
  // exact family-of-bug-#1 risk this codebase explicitly warns against),
  // at the document's own resolution and zoom=1 — NOT whatever the
  // on-screen viewport happens to be sized/zoomed to, since effect pixel
  // params are now zoom-scaled (see run_one_effect, engine.rs) and must
  // match the true document size to look right in the export.
  //
  // Supersampled effects export (2026-08, feedback #60 — "il faudrait
  // augmenter la résolution [du] rendu final pour avoir une qualité pro"):
  // renderFrameToPixelsPNG now takes the same `scale` factor
  // exportFrameDataURL always supported, rendering at cw*scale × ch*scale
  // instead of native size. This is NOT a resize-then-blit upscale — the
  // whole scene (vector geometry AND effect params) renders natively at
  // the higher pixel density, via resizeEngineOffscreenAtScale below.
  var _fxExportSavedFrame = null, _fxExportSavedEngineW = 0, _fxExportSavedEngineH = 0;
  function beginEffectsExport() {
    if (!engine) return false;
    suspended = true; // same gate suspend() sets — see tick()'s own check
    _fxExportSavedFrame = state.currentFrame;
    _fxExportSavedEngineW = engineW; _fxExportSavedEngineH = engineH;
    return true;
  }
  // Resizes the engine's render target for an offscreen (render_to_pixels)
  // pass — decoupled from #rust-canvas's own on-screen width/height (that's
  // the whole point of render_to_pixels never touching the visible surface,
  // see engine.rs's own doc comment). Shared by renderFrameToPixelsPNG
  // (always native size) and the playback bake cache (playback-cache.js,
  // resized once to a REDUCED size for the whole bake pass) — one place
  // that pairs resize()+set_viewport() instead of two copies drifting out
  // of phase (CLAUDE.md §3).
  function resizeEngineOffscreen(w, h) {
    engine.resize(w, h);
    engine.set_viewport(0, 0, 1, 0, w / 2, h / 2, 1);
  }
  // Same idea as resizeEngineOffscreen, but for a render target that's a
  // uniform SCALE factor of the document's own native size rather than an
  // arbitrary independent w/h — the pivot must stay the NATIVE document
  // center (nativeW/2, nativeH/2), not half of the scaled target, or the
  // content renders at the wrong density/position the moment scale != 1.
  // Exact same pan-compensation algebra as syncViewport's panAdjX/panAdjY
  // (see that function's own derivation) with the "desired pan before
  // pivot compensation" fixed at 0 (no on-screen pan concept applies to an
  // offscreen export) and z fixed at `scale`: pan = pivot*(scale-1) is what
  // keeps world (0,0) landing on pixel (0,0) instead of drifting by the
  // pivot's own offset once zoom != 1. effect_zoom is set to the SAME
  // `scale` (not 1) so effect radii — already expressed as document pixels,
  // scaled by effect_zoom in engine.rs's run_one_effect — grow with the
  // render density instead of looking relatively thinner at higher scale.
  function resizeEngineOffscreenAtScale(nativeW, nativeH, scale) {
    var w = Math.max(1, Math.round(nativeW * scale)), h = Math.max(1, Math.round(nativeH * scale));
    engine.resize(w, h);
    var pivotWX = nativeW / 2, pivotWY = nativeH / 2;
    var panAdjX = pivotWX * (scale - 1), panAdjY = pivotWY * (scale - 1);
    engine.set_viewport(panAdjX, panAdjY, scale, 0, pivotWX, pivotWY, scale);
    return { w: w, h: h };
  }
  // Renders one frame and returns the raw RGBA8 pixels — the actual
  // GPU-readback call site, factored out so both the PNG-encoding export
  // path below and the playback bake cache call the exact same
  // loadFrame/buildSceneJson/render_to_pixels sequence instead of
  // duplicating it (CLAUDE.md §3). Caller must already have the engine at
  // the size it wants (resizeEngineOffscreen) before calling this.
  async function renderFrameRawPixels(frameIdx, alphaBg) {
    loadFrame(frameIdx);
    _fxFrameOverride = frameIdx;
    var json;
    try {
      json = buildSceneJson(true, true, {
        frame: frameIdx,
        forExport: true,
        includeEditorOverlays: false,
        alphaBg: !!alphaBg,
      });
    } finally { _fxFrameOverride = null; }
    var bytes = await engine.render_to_pixels(json);
    return new Uint8ClampedArray(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  }
  async function renderFrameToPixelsPNG(frameIdx, scale, alphaBg) {
    var cw = state.canvasW, ch = state.canvasH;
    var size = resizeEngineOffscreenAtScale(cw, ch, scale || 1);
    var pixels = await renderFrameRawPixels(frameIdx, alphaBg);
    var imgData = new ImageData(pixels, size.w, size.h);
    var off = document.createElement('canvas'); off.width = size.w; off.height = size.h;
    off.getContext('2d').putImageData(imgData, 0, 0);
    return off.toDataURL('image/png');
  }
  function endEffectsExport() {
    if (_fxExportSavedFrame === null) return;
    loadFrame(_fxExportSavedFrame);
    if (_fxExportSavedEngineW && _fxExportSavedEngineH) engine.resize(_fxExportSavedEngineW, _fxExportSavedEngineH);
    syncViewport();
    lastSceneJson = ''; // force a full re-render at the restored size/viewport
    suspended = false;
    invalidateOverlayBase();
    renderNow();
    _fxExportSavedFrame = null;
  }

  // Duplicate canvas viewer (2026-08, AE feature audit 8.4, "New Viewer" —
  // a second window/panel on the SAME comp, often locked to a different
  // frame than the main view). Deliberately narrow: just a frame-scoped
  // buildSceneJson call, not the whole raw function — second-viewer.js
  // owns its OWN VelloEngine instance/canvas/viewport entirely, this is
  // the one seam it needs into engine-bridge's closure. skipVolatile=true
  // (no drag-cursor previews — a second static viewer has no tool of its
  // own drawing into it) and includeEditorOverlays=false (no selection
  // handles/marquee/etc. — those belong to the tool driving the MAIN
  // canvas, not a read-only reference view).
  window.SMEngineBridge = {
    buildSceneJsonForFrame: function (frame) { return buildSceneJson(true, false, { frame: frame, includeEditorOverlays: false }); },
    setEnabled: setEnabled,
    isEnabled: function () { return enabled; },
    // Adaptive preview render scale (feedback #60 part 2) — read-only
    // status plus a manual override for a future Settings toggle/QA.
    getPreviewRenderScale: function () { return _previewRenderScale; },
    getPreviewCalibStatus: function () { return { done: _calibDone, samples: _calibSamples.slice() }; },
    setPreviewRenderScale: function (scale) { setPreviewRenderScale(scale); return _previewRenderScale; },
    forcePreviewCalibration: function () { _calibDone = false; _calibSamples = []; },
    screenToWorld: screenToWorld,
    renderWithOverlayItem: renderWithOverlayItem,
    renderNow: renderNow,
    renderImageOnly: renderImageOnly,
    setEraserCursor: setEraserCursor,
    setFSBreakMarks: setFSBreakMarks,
    setPressureCursor: setPressureCursor,
    setPenPreview: setPenPreview,
    setRigPreview: setRigPreview,
    registerImagePixels: registerImagePixels,
    // ONE definition of raster->engine image identity. bitmap-brush.js
    // re-uploads pixels under this id during erase and live restamp; if it
    // derived its own, the scene would ask for a key nobody wrote to and
    // the texture would silently stop updating (CLAUDE.md §1).
    engineIdFor: engineIdFor,
    // Project load replaces every stored stroke dict at once, so every
    // retained path is garbage immediately — dropping them eagerly beats
    // waiting for the GC to walk thousands of dead WeakMap entries.
    clearRetainedPaths: function () {
      _pathKeyByDict = new WeakMap(); _retireQueue = []; _registerCount = 0;
      if (engine && engine.clear_paths) { try { engine.clear_paths(); } catch (e) { /* non-fatal */ } }
    },
    // Kill switch. Retained paths are a deep change to what the renderer is
    // handed, so there is a way to turn them off at runtime without a
    // rebuild — both to A/B the output (the whole feature is only worth
    // shipping if the picture is byte-identical) and as a field escape hatch
    // if some item type ever turns out to mutate geometry without tripping
    // the change hook. Turning it OFF also drops every stamp, so the very
    // next scene build falls back to inline segments immediately rather than
    // waiting for a desP rebuild.
    setRetainedPathsEnabled: function (on) {
      _pathRefsEnabled = !!on && !!_geomFlagMask;
      if (!on) {
        for (var i = 0; i < userLayers.length; i++) {
          var ch = userLayers[i].children;
          for (var k = 0; k < ch.length; k++) if (ch[k]._data) ch[k]._data.__engineSrcDict = null;
        }
        // Dropping the WeakMap alone would ORPHAN every engine-side entry:
        // retirement fires when the stored DICT is collected, and the dicts
        // are still very much alive (they are the document). Without this the
        // engine store grew on every off/on cycle with nothing able to free
        // it. Disabling means forget everything, both sides.
        _pathKeyByDict = new WeakMap();
        _retireQueue = [];
        _registerCount = 0;
        if (engine && engine.clear_paths) { try { engine.clear_paths(); } catch (e) { /* non-fatal */ } }
      }
      return _pathRefsEnabled;
    },
    // Footage memory: the store is bounded now (see enforceImageBudget).
    // Exposed so a project can be sized against a machine, and so the
    // eviction policy can be observed rather than assumed.
    imageStoreStats: function () {
      return {
        jsBytes: _imgTotalBytes(),
        engineBytes: (engine && engine.image_store_bytes) ? engine.image_store_bytes() : -1,
        engineCount: (engine && engine.image_store_size) ? engine.image_store_size() : -1,
        budgetBytes: _imgBudgetBytes,
        evictions: _imgEvictions,
      };
    },
    // Math.floor, NOT `| 0`: bitwise coercion wraps at 2^31, so any budget
    // above ~2.1GB silently became a tiny number (a 4GB budget landed on 1
    // byte and evicted the whole store on the first frame).
    setImageBudgetBytes: function (n) { _imgBudgetBytes = Math.max(1, Math.floor(n)); return _imgBudgetBytes; },
    // Diagnostics: times the scene serialization in isolation. fps probes
    // aggregate loadFrame + Paper rebuild + engine render + browser paint,
    // which swamped the signal this change actually moves.
    timeSceneBuild: function (n) {
      n = n || 20;
      var t = performance.now(), bytes = 0;
      for (var i = 0; i < n; i++) bytes = buildSceneJson().length;
      return { msPerBuild: +((performance.now() - t) / n).toFixed(2), bytes: bytes };
    },
    // Diagnostics only (perf probes / tests) — not used by app code.
    retainedPathStats: function () {
      return { enabled: _pathRefsEnabled, geomFlagMask: _geomFlagMask,
        registered: (engine && engine.path_store_size) ? engine.path_store_size() : -1,
        pendingRetire: _retireQueue.length };
    },
    registerImageRaw: registerImageRaw,
    // Custom WGSL effects (2026-07) — see custom-effects.js's own
    // registerAllCustomEffects for why every definition gets re-sent here
    // on load/engine-(re)creation, not just when first authored.
    registerCustomEffect: function (key, fsBody) { if (engine) engine.register_custom_effect(key, fsBody); },
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
    // Effects-aware frame export (2026-07) — see the functions' own
    // comments above. beginEffectsExport/endEffectsExport bracket a whole
    // export run (called once each); renderFrameToPixelsPNG is called once
    // per exported frame in between.
    beginEffectsExport: beginEffectsExport,
    renderFrameToPixelsPNG: renderFrameToPixelsPNG,
    endEffectsExport: endEffectsExport,
    // Playback bake cache (playback-cache.js) — same offscreen
    // resize/readback pair the effects-export path above uses, exposed
    // separately so the bake pass can pick its OWN (reduced) resolution
    // instead of always native, without duplicating the resize+viewport or
    // render_to_pixels call sites.
    resizeEngineOffscreen: function (w, h) { if (!engine) return false; resizeEngineOffscreen(w, h); return true; },
    renderFrameRawPixels: function (frameIdx) { if (!engine) return Promise.resolve(null); return renderFrameRawPixels(frameIdx); },
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
