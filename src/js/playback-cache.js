// ---- PLAYBACK BAKE CACHE ("Cache de lecture") ----
// Direction decided 2026-07-28 (see project-nemo-playback-architecture
// research): pre-render timeline frames to small in-memory bitmaps once, at
// reduced resolution, so replaying an already-baked range blits instantly
// instead of re-materializing Paper.js objects (loadFrame/desP) and
// re-running the GPU scene render every tick. This is a FALLBACK for when
// real-time playback genuinely can't keep up, not a step every project
// pays — see timeline.js's playStep for the rAF-interval monitor that
// triggers bakeRange() automatically, and the small manual button
// (#btn-bake-cache) that calls the same function on demand.
//
// Reuses, rather than duplicates, the exact machinery the effects-export
// path already proved safe for this: beginEffectsExport()/endEffectsExport()
// (engine-bridge.js) already suspend the live tick() render loop and
// snapshot/restore state.currentFrame + the engine's native render size
// around a whole batch of loadFrame() calls — a bake pass is structurally
// the same "batch-render frames without racing the live document" operation
// export already does, just writing bitmaps to a Map instead of PNGs to
// disk, and at a smaller resolution.
(function () {
  var _cache = new Map(); // frameIndex -> ImageBitmap
  var _bytes = 0;
  var _bakeW = 0, _bakeH = 0; // resolution the CURRENT cache contents were baked at
  var _scale = 0.5; // accepted degradation: half native resolution by default
  var _budgetBytes = 256 * 1024 * 1024; // hardcoded for this pass — Réglages exposure is future work (same deferral CLAUDE.md §5quinquies already used for the image store)
  var _baking = false;
  var _cancelRequested = false;

  // Identity stamp taken at bake time — cheaper and far more robust than
  // hooking every place that swaps state.layers/canvas size/fps (enterSymbol,
  // exitToScene, montage view, StoryBoard mode, project load...). Any of
  // those already replace state.layers with a DIFFERENT array reference or
  // change canvasW/canvasH/totalFrames, so comparing identity here catches
  // ALL of them automatically — including future code paths that swap
  // state.layers this file's author never enumerated (the "family of bug
  // #1" trap, CLAUDE.md §1). In-place edits to the SAME state.layers array
  // don't change this identity, which is exactly why pushUndoLayers
  // (tweens.js) still needs its own explicit invalidateAll() call below.
  var _stampLayers = null, _stampCanvasW = 0, _stampCanvasH = 0, _stampTotalFrames = 0;

  function _stampMatches() {
    return _stampLayers === state.layers &&
      _stampCanvasW === state.canvasW && _stampCanvasH === state.canvasH &&
      _stampTotalFrames === state.totalFrames;
  }

  function invalidateAll() {
    _cache.forEach(function (bmp) { if (bmp && bmp.close) bmp.close(); });
    _cache = new Map();
    _bytes = 0;
    _bakeW = 0; _bakeH = 0;
    _stampLayers = null;
  }

  function hasFrame(f) {
    if (!_stampMatches()) { if (_cache.size) invalidateAll(); return false; }
    return _cache.has(f);
  }

  // ---- On-screen blit (stacked exactly like #rust-canvas — same "canvas
  // stacked over the Paper one" convention engine-bridge.js already uses) ----
  var _previewCanvas = null;
  function _ensurePreviewCanvas() {
    if (_previewCanvas && document.body.contains(_previewCanvas)) return _previewCanvas;
    var rustCanvas = document.getElementById('rust-canvas');
    if (!rustCanvas) return null;
    _previewCanvas = document.createElement('canvas');
    _previewCanvas.id = 'bake-preview-canvas';
    _previewCanvas.style.cssText = 'position:absolute;inset:0;width:100%;height:100%;pointer-events:none;display:none;';
    rustCanvas.parentNode.insertBefore(_previewCanvas, rustCanvas.nextSibling);
    return _previewCanvas;
  }
  function blitFrame(f) {
    if (!hasFrame(f)) return false;
    var canvas = _ensurePreviewCanvas();
    if (!canvas) return false;
    var bmp = _cache.get(f);
    // Assigning width/height resets the whole backing store, even when the
    // value did not change. Cached playback calls this once per displayed
    // frame, so the unconditional assignments used to discard and recreate
    // a full-canvas buffer on every cache hit. Resize only when the document
    // size actually changed; clearRect below remains the per-frame reset.
    if (canvas.width !== state.canvasW) canvas.width = state.canvasW;
    if (canvas.height !== state.canvasH) canvas.height = state.canvasH;
    var ctx = canvas.getContext('2d');
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    ctx.drawImage(bmp, 0, 0, canvas.width, canvas.height);
    canvas.style.display = 'block';
    var rustCanvas = document.getElementById('rust-canvas');
    if (rustCanvas) rustCanvas.style.display = 'none';
    return true;
  }
  // Restores #rust-canvas's own visibility — mirrors setEnabled's own
  // `rustCanvas.style.display = on ? 'block' : 'none'` (engine-bridge.js)
  // rather than guessing a different value, and defers to isEnabled() so
  // this never fights the engine being off for an unrelated reason.
  function hideBakePreview() {
    if (_previewCanvas) _previewCanvas.style.display = 'none';
    var rustCanvas = document.getElementById('rust-canvas');
    if (rustCanvas && window.SMEngineBridge && window.SMEngineBridge.isEnabled()) rustCanvas.style.display = 'block';
  }

  // ---- Bake pass ----
  function cancelBake() { _cancelRequested = true; }
  function isBaking() { return _baking; }

  async function bakeRange(startF, endF, opts) {
    opts = opts || {};
    if (_baking) return { started: false, reason: 'already-baking' };
    if (!window.SMEngineBridge || !window.SMEngineBridge.isEnabled()) return { started: false, reason: 'engine-disabled' };
    startF = Math.max(0, startF); endF = Math.min(state.totalFrames - 1, endF);
    if (endF < startF) return { started: false, reason: 'empty-range' };

    var cw = state.canvasW, ch = state.canvasH;
    var scale = opts.scale || _scale;
    var bakeW = Math.max(1, Math.round(cw * scale)), bakeH = Math.max(1, Math.round(ch * scale));
    // A resolution or document change invalidates whatever was baked before —
    // upscaling a mismatched-size cache would just be silently wrong content
    // once state.layers has moved on, and mixing two bake resolutions in one
    // cache is needless complexity for zero benefit.
    if (!_stampMatches() || _bakeW !== bakeW || _bakeH !== bakeH) invalidateAll();
    _bakeW = bakeW; _bakeH = bakeH;
    _stampLayers = state.layers; _stampCanvasW = cw; _stampCanvasH = ch; _stampTotalFrames = state.totalFrames;

    _baking = true; _cancelRequested = false;
    var cached = 0, total = endF - startF + 1, budgetHit = false;
    window.SMEngineBridge.beginEffectsExport();
    window.SMEngineBridge.resizeEngineOffscreen(bakeW, bakeH);
    try {
      for (var f = startF; f <= endF; f++) {
        if (_cancelRequested) break;
        if (_cache.has(f)) { cached++; continue; }
        var frameBytes = bakeW * bakeH * 4;
        if (_bytes + frameBytes > _budgetBytes) { budgetHit = true; break; }
        var pixels = await window.SMEngineBridge.renderFrameRawPixels(f);
        var imgData = new ImageData(pixels, bakeW, bakeH);
        var bitmap = await createImageBitmap(imgData);
        _cache.set(f, bitmap);
        _bytes += frameBytes;
        cached++;
        if (opts.onProgress) opts.onProgress(cached, total);
      }
    } finally {
      window.SMEngineBridge.endEffectsExport();
      _baking = false;
    }
    return { started: true, cached: cached, total: total, budgetHit: budgetHit, cancelled: _cancelRequested };
  }

  window.SMPlaybackCache = {
    invalidateAll: invalidateAll,
    hasFrame: hasFrame,
    blitFrame: blitFrame,
    hideBakePreview: hideBakePreview,
    bakeRange: bakeRange,
    cancelBake: cancelBake,
    isBaking: isBaking,
    cacheSize: function () { return _cache.size; },
    cacheBytes: function () { return _bytes; },
    getBakeSize: function () { return { w: _bakeW, h: _bakeH }; },
    setBudgetBytes: function (n) { _budgetBytes = Math.max(1, Math.floor(n)); },
    setScale: function (s) { _scale = s; },
  };
})();
