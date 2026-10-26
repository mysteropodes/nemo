// ---- LABS PROTOTYPE — Speed/stream lines generator (Clip Studio Paint) ----
// Manga-style radial speed lines ("lignes de vitesse") in one call:
//
//   SMLabs.speedLines()                          — defaults, canvas center
//   SMLabs.speedLines({cx, cy, count, r0, r1, jitter})
//
// Generates `count` radial line Paths from radius r0 to r1 around
// (cx,cy), with per-line jitter on angle/length so it reads hand-ruled,
// using the current stroke color/width. Ordinary Paths, one undo removes
// the whole burst (single pushUndo before the batch).
(function () {
  window.SMLabs.register('speed-lines', {
    flag: 'nemo-labs-speedlines',
    describe: 'labsDescribeSpeedLines',
  });

  function nativeSpeedLines(native, opts) {
    var layerUid = native.layerUid(state.activeLayerIdx);
    if (!layerUid) { if (typeof showToast === 'function') showToast(SM.t('labsToastLayerInvalidLocked')); return Promise.resolve(0); }
    var w = state.canvasW || 1920, h = state.canvasH || 1080;
    var count = Math.max(3, Math.min(400, opts.count || 60));
    var r1 = opts.r1 !== undefined ? +opts.r1 : Math.max(w, h) * 0.75;
    return native.speedLines({ layerUid: layerUid, frame: state.currentFrame,
      cx: opts.cx !== undefined ? +opts.cx : w / 2, cy: opts.cy !== undefined ? +opts.cy : h / 2, count: count,
      r0: opts.r0 !== undefined ? +opts.r0 : r1 * 0.45, r1: r1,
      jitter: opts.jitter !== undefined ? Math.max(0, Math.min(1, +opts.jitter)) : 0.35,
      // The baseline draws every line from Math.random; one seed drawn here names the whole run.
      seed: opts.seed !== undefined ? opts.seed >>> 0 : Math.floor(Math.random() * 4294967296),
      color: NemoNativeObjectLabs.rgba(state.strokeEnabled ? state.strokeColor : '#000000'),
      brushSize: state.brushSize, opacity: state.opacity / 100 }).then(function () {
      if (typeof showToast === 'function') showToast(count + SM.t('labsToastSpeedLinesGeneratedSuffix'));
      return count;
    }, function () { if (typeof showToast === 'function') showToast(SM.t('labsToastLayerInvalidLocked')); return 0; });
  }

  window.SMLabs.speedLines = function (opts) {
    opts = opts || {};
    // Native authority (U36, #2362): the engine runs this generator over the shown cell as one history entry and
    // the call returns a Promise of the same count; the code below is the baseline path.
    var native = window.NemoNativeObjectLabs && NemoNativeObjectLabs.desktop(window);
    if (native) return nativeSpeedLines(native, opts);
    var ld = state.layers[state.activeLayerIdx];
    if (!ld || ld.locked || ld.symbolId) { if (typeof showToast === 'function') showToast(SM.t('labsToastLayerInvalidLocked')); return 0; }
    var cx = opts.cx !== undefined ? +opts.cx : (state.canvasW || 1920) / 2;
    var cy = opts.cy !== undefined ? +opts.cy : (state.canvasH || 1080) / 2;
    var count = Math.max(3, Math.min(400, opts.count || 60));
    var r1 = opts.r1 !== undefined ? +opts.r1 : Math.max(state.canvasW || 1920, state.canvasH || 1080) * 0.75;
    var r0 = opts.r0 !== undefined ? +opts.r0 : r1 * 0.45;
    var jitter = opts.jitter !== undefined ? Math.max(0, Math.min(1, +opts.jitter)) : 0.35;

    pushUndo();
    ensureKeyframe();
    var layer = userLayers[state.activeLayerIdx];
    layer.activate();
    for (var i = 0; i < count; i++) {
      var a = i / count * 2 * Math.PI + (Math.random() - 0.5) * jitter * (2 * Math.PI / count);
      var rr0 = r0 * (1 + (Math.random() - 0.5) * jitter);
      var rr1 = r1 * (1 + (Math.random() - 0.5) * jitter * 0.5);
      var p = new Path();
      p.strokeColor = state.strokeEnabled ? state.strokeColor : '#000000';
      p.strokeWidth = Math.max(0.5, state.brushSize * (0.6 + Math.random() * 0.8));
      p.strokeCap = 'round';
      p.fillColor = null;
      p.opacity = state.opacity / 100;
      p.add(new Point(cx + rr0 * Math.cos(a), cy + rr0 * Math.sin(a)));
      p.add(new Point(cx + rr1 * Math.cos(a), cy + rr1 * Math.sin(a)));
      if (typeof tagOwner === 'function') tagOwner(p);
    }
    saveActiveLayerFrame(); updateUI();
    if (window.SMEngineBridge) SMEngineBridge.renderNow();
    if (typeof showToast === 'function') showToast(count + SM.t('labsToastSpeedLinesGeneratedSuffix'));
    return count;
  };
})();
