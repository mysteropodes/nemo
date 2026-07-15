// ---- BITMAP BRUSH (v1 spike, 2026-07) ----
// A second, PARALLEL brush category alongside the existing all-vector
// brush-texture presets (applyBrushTexture, tools.js — jittered Path dab
// copies, ~170/stroke per CLAUDE.md's own perf numbers). Deliberately NOT a
// replacement: the vector presets stay untouched because some exports
// (Rive in particular) need real vector geometry per dab — see
// docs/raster-brush-texture-proposal.md for the full design discussion.
//
// This module produces a single Raster (real bitmap texture, stamped along
// the Draw tool's captured centerline) instead of hundreds of vector Path
// copies. Deliberately built on Paper's existing Raster item type rather
// than inventing a new one — Raster is ALREADY a first-class citizen
// everywhere a stroke needs to be (serR/desR, buildSceneJson, selectedPaths
// construction, save/load) confirmed by grep before writing this, so this
// file only needs to PRODUCE a Raster correctly; every consumer already
// handles it with zero changes.
//
// v1 scope, explicitly NOT done here (see the proposal doc's own "what it
// costs" section): tip shapes are procedural only (no ABR import yet — the
// tip-rendering functions below are the plug point for that later), no
// live drag preview (stamped once at commitStroke, same "cheap during
// drag, full quality on release" precedent as arc-handle dragging elsewhere
// in this codebase — except here v1 has literally no preview at all, a
// known limitation to revisit), and only wired into draw-bridge.js's plain
// constant-width commit path (NOT vector-brush/fill-brush, matching
// applyBrushTexture's own pre-existing scoping) and NOT yet mirrored into
// tools.js's Paper-native fallback (dead code when the Rust engine is on,
// which is the default — see this file's own TODO below for why that's an
// acceptable v1 gap, not an oversight).
(function () {
  var TIP_SIZE = 64; // procedural tip canvas resolution, px — independent of brush Size (that's a WORLD-space display size, this is just texture detail)

  // ---- procedural tips (v1 — plug point for ABR-imported tip masks later:
  // an ABR tip is already a grayscale bitmap mask, so a future importer just
  // needs to draw ITS pixels into a same-shaped canvas instead of these) ----
  function drawSoftTip(ctx, seed) {
    var g = ctx.createRadialGradient(TIP_SIZE / 2, TIP_SIZE / 2, 0, TIP_SIZE / 2, TIP_SIZE / 2, TIP_SIZE / 2);
    g.addColorStop(0, 'rgba(255,255,255,1)');
    g.addColorStop(0.7, 'rgba(255,255,255,.55)');
    g.addColorStop(1, 'rgba(255,255,255,0)');
    ctx.fillStyle = g;
    ctx.fillRect(0, 0, TIP_SIZE, TIP_SIZE);
  }
  function drawGrainTip(ctx, seed) {
    var rng = seededRng(seed);
    var cx = TIP_SIZE / 2, cy = TIP_SIZE / 2, r = TIP_SIZE / 2;
    // Soft falloff base so the grain has an overall round silhouette, not a square.
    drawSoftTip(ctx, seed);
    ctx.globalCompositeOperation = 'source-atop';
    var n = 220;
    for (var i = 0; i < n; i++) {
      var a = rng() * Math.PI * 2, d = Math.sqrt(rng()) * r;
      var x = cx + Math.cos(a) * d, y = cy + Math.sin(a) * d;
      var s = 0.6 + rng() * 1.6;
      ctx.fillStyle = 'rgba(0,0,0,' + (0.15 + rng() * 0.35) + ')';
      ctx.beginPath(); ctx.arc(x, y, s, 0, Math.PI * 2); ctx.fill();
    }
    ctx.globalCompositeOperation = 'source-over';
  }
  function drawSplatterTip(ctx, seed) {
    var rng = seededRng(seed);
    var cx = TIP_SIZE / 2, cy = TIP_SIZE / 2, r = TIP_SIZE / 2;
    var blobs = 5 + Math.floor(rng() * 4);
    for (var i = 0; i < blobs; i++) {
      var a = rng() * Math.PI * 2, d = rng() * r * 0.55;
      var x = cx + Math.cos(a) * d, y = cy + Math.sin(a) * d;
      var br = r * (0.35 + rng() * 0.5);
      var g = ctx.createRadialGradient(x, y, 0, x, y, br);
      g.addColorStop(0, 'rgba(255,255,255,.9)');
      g.addColorStop(1, 'rgba(255,255,255,0)');
      ctx.fillStyle = g;
      ctx.beginPath(); ctx.arc(x, y, br, 0, Math.PI * 2); ctx.fill();
    }
  }
  var TIP_DRAWERS = { soft: drawSoftTip, grain: drawGrainTip, splatter: drawSplatterTip };

  // Deterministic per-stroke RNG (same reasoning as tools.js's seededRng for
  // dab re-stamping on generated tween frames: a fresh Math.random() per
  // regeneration would make the texture visibly "boil" between saves/undos
  // touching the same stroke; not wired into tween regen yet in v1 since
  // Raster strokes don't currently participate in tween matching at all —
  // see the file-header TODO — but keeping this deterministic now avoids
  // having to revisit the stamping math later just to add determinism).
  function seededRng(seed) {
    var s = seed >>> 0;
    return function () { s = (s * 1664525 + 1013904223) >>> 0; return s / 4294967296; };
  }

  function buildTipCanvas(tipName, seed) {
    var c = document.createElement('canvas'); c.width = TIP_SIZE; c.height = TIP_SIZE;
    var ctx = c.getContext('2d');
    (TIP_DRAWERS[tipName] || drawSoftTip)(ctx, seed);
    return c;
  }

  // Walks the Draw tool's raw [x,y,width] samples at fixed arc-length
  // spacing, stamps the tip at each point (scattered/sized per settings),
  // and returns a Raster ready to insert — position/size already set, only
  // .data tagging + layer insertion are the caller's job.
  function stampBitmapBrush(samples, opts) {
    if (samples.length < 2) return null;
    var tipName = opts.tip, baseSize = opts.size, spacingPct = opts.spacing, scatterPct = opts.scatter, opacity = opts.opacity, color = opts.color, seed = opts.seed;
    var spacing = Math.max(1, baseSize * (spacingPct / 100));
    var scatterPx = baseSize * (scatterPct / 100);
    var rng = seededRng(seed);

    // Bounding box in world coords, padded for tip radius + scatter + a
    // pressure-width bump (samples' own width channel can widen the tip).
    var minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity, maxW = 0;
    samples.forEach(function (s) { minX = Math.min(minX, s[0]); maxX = Math.max(maxX, s[0]); minY = Math.min(minY, s[1]); maxY = Math.max(maxY, s[1]); maxW = Math.max(maxW, s[2] || 1); });
    var pad = baseSize * Math.max(1, maxW) / 2 + scatterPx + 4;
    minX -= pad; minY -= pad; maxX += pad; maxY += pad;
    var w = Math.max(1, maxX - minX), h = Math.max(1, maxY - minY);

    // Oversample the raster relative to world units for reasonable quality
    // regardless of current canvas zoom — a real "resample at export
    // resolution" step is future work (see proposal doc), this is a fixed,
    // cheap approximation for v1.
    var SCALE = 2;
    var canvas = document.createElement('canvas');
    canvas.width = Math.ceil(w * SCALE); canvas.height = Math.ceil(h * SCALE);
    var ctx = canvas.getContext('2d');
    ctx.fillStyle = color;
    var tip = buildTipCanvas(tipName, seed);

    // Arc-length walk with linear interpolation of position + pressure width.
    var acc = 0;
    var stampAt = function (x, y, wMul) {
      var jx = (rng() - 0.5) * 2 * scatterPx, jy = (rng() - 0.5) * 2 * scatterPx;
      var jScale = 0.75 + rng() * 0.5;
      var size = baseSize * wMul * jScale;
      var px = (x + jx - minX) * SCALE, py = (y + jy - minY) * SCALE, ps = size * SCALE;
      ctx.save();
      ctx.globalAlpha = opacity;
      ctx.globalCompositeOperation = 'source-over';
      // Tint the tip's white/alpha mask with the brush color: draw the tip,
      // then fill-composite the color through it — cheaper than re-drawing
      // colored procedural shapes per stamp.
      ctx.drawImage(tip, px - ps / 2, py - ps / 2, ps, ps);
      ctx.restore();
    };
    // Two-pass: draw all tip masks to an offscreen alpha buffer, then tint
    // once — avoids per-stamp compositing cost mattering much at typical
    // stroke lengths (dozens to low hundreds of stamps, not thousands).
    var maskCanvas = document.createElement('canvas');
    maskCanvas.width = canvas.width; maskCanvas.height = canvas.height;
    var mctx = maskCanvas.getContext('2d');
    var origCtx = ctx; ctx = mctx;
    for (var i = 0; i < samples.length - 1; i++) {
      var a = samples[i], b = samples[i + 1];
      var dx = b[0] - a[0], dy = b[1] - a[1];
      var segLen = Math.sqrt(dx * dx + dy * dy);
      if (segLen < 1e-6) continue;
      while (acc < segLen) {
        var t = acc / segLen;
        var x = a[0] + dx * t, y = a[1] + dy * t;
        var wv = (a[2] || 1) + ((b[2] || 1) - (a[2] || 1)) * t;
        stampAt(x, y, Math.max(0.2, wv));
        acc += spacing;
      }
      acc -= segLen;
    }
    ctx = origCtx;
    // Tint pass: fill the whole canvas with the brush color, clipped to the
    // mask's alpha via destination-in.
    ctx.fillStyle = color;
    ctx.fillRect(0, 0, canvas.width, canvas.height);
    ctx.globalCompositeOperation = 'destination-in';
    ctx.drawImage(maskCanvas, 0, 0);
    ctx.globalCompositeOperation = 'source-over';

    var dataUrl = canvas.toDataURL('image/png');
    var raster = new Raster(dataUrl);
    // Every other Raster-creation site in this app (desR, app.js; the SVG-
    // sequence import, timeline.js) sets .data.src right after construction
    // — serR()'s save-path reads THIS field first, falling back to
    // r.source only if it's unset. Skipping it (as this file did before a
    // live test caught it) meant the saved/onion-skin round-trip depended
    // on Paper's own .source getter instead, which produced a Raster whose
    // .canvas came back 0-width — engine-bridge.js's registerRasterIfNeeded
    // then threw on ctx.getImageData(0,0,0,0). Matching the established
    // convention fixes it at the source instead of special-casing the
    // reader.
    raster.data.src = dataUrl;
    raster.data.isBitmapBrush = true;
    raster.data.bitmapTip = tipName;
    raster.data.bitmapSize = baseSize;
    raster.data.bitmapSpacing = spacingPct;
    raster.data.bitmapScatter = scatterPct;
    raster.data.bitmapSeed = seed;
    var cx = minX + w / 2, cy = minY + h / 2;
    raster.onLoad = function () {
      raster.position = new Point(cx, cy);
      raster.size = new Size(w, h);
    };
    return raster;
  }

  // ---- panel wiring — plain state fields, same convention as every other
  // scrub/select input in this app (ui.js) ----
  function bindNum(id, key, def) {
    var el = document.getElementById(id); if (!el) return;
    state[key] = def;
    el.addEventListener('change', function () { state[key] = parseFloat(this.value) || def; });
    el.addEventListener('input', function () { state[key] = parseFloat(this.value) || def; });
  }
  function init() {
    var onEl = document.getElementById('p-bitmapbrush-on');
    if (onEl) { state.bitmapBrushOn = false; onEl.addEventListener('change', function () { state.bitmapBrushOn = this.checked; }); }
    var tipEl = document.getElementById('p-bitmap-tip');
    if (tipEl) { state.bitmapTip = tipEl.value; tipEl.addEventListener('change', function () { state.bitmapTip = this.value; }); }
    bindNum('p-bitmap-size', 'bitmapSize', 40);
    bindNum('p-bitmap-spacing', 'bitmapSpacing', 15);
    bindNum('p-bitmap-scatter', 'bitmapScatter', 20);
    bindNum('p-bitmap-opacity', 'bitmapOpacity', 100);
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', init); else init();

  window.SMBitmapBrush = {
    stamp: function (samples) {
      return stampBitmapBrush(samples, {
        tip: state.bitmapTip || 'soft',
        size: state.bitmapSize || 40,
        spacing: state.bitmapSpacing != null ? state.bitmapSpacing : 15,
        scatter: state.bitmapScatter != null ? state.bitmapScatter : 20,
        opacity: (state.bitmapOpacity != null ? state.bitmapOpacity : 100) / 100,
        // state.strokeColor is already a plain '#rrggbb'/'#rrggbbaa' CSS
        // string (set by the color pickers, see timeline.js setStrokeColor)
        // — NOT a Paper.js Color object, so no colorHex8() conversion here
        // (that function expects a Paper Color and would throw on a string).
        color: state.strokeColor || '#000000',
        seed: Math.floor(Math.random() * 0xffffffff),
      });
    },
  };
})();
