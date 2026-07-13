// ---- ROTOSCOPY REFERENCE (video / image sequence / single image) ----
// One reference at a time (state.refMedia), drawn INSIDE the engine scene as
// an image item appended to the synthetic background layer (engine-bridge
// buildSceneJson) — above the white artboard rect, below every drawing
// layer, exactly where tracing reference belongs. Never part of frame data,
// never exported with the artwork.
//
// state.refMedia = {
//   type: 'video' | 'imageseq' | 'image',
//   name, opacity (0..1), visible, offsetFrames,
//   src: dataURL            (video/image)
//   frames: [dataURL, ...]  (imageseq — 1 image per timeline frame)
// }
// Runtime-only fields (_video/_seekBusy/_imgCache/_dims) are stripped by
// exportJSON's explicit mapping, same convention as audio tracks.
//
// Video sync: an HTMLVideoElement seeked to (frame - offsetFrames)/fps on
// every loadFrame; 'seeked' fires async, the decoded picture is drawn to an
// offscreen canvas and (re)uploaded to the engine under ONE fixed image id
// ('ref:video') — engine-side the upload replaces the cached texture, so GPU
// memory stays a single frame regardless of video length. Playback rides the
// same hook (loadFrame runs per played frame); if decoding can't keep up the
// reference simply lags a frame or two behind — the drawing layers never do.
(function () {
  var REF_VIDEO_ID = 'ref:video';

  function ref() { return state.refMedia || null; }

  // ---- media loading ----
  function ensureVideo(r) {
    if (r._video) return r._video;
    var v = document.createElement('video');
    v.src = r.src;
    v.muted = true;
    v.preload = 'auto';
    v.addEventListener('loadedmetadata', function () {
      r._dims = { w: v.videoWidth, h: v.videoHeight };
      syncToFrame(state.currentFrame, true);
    });
    r._video = v;
    r._seekCanvas = document.createElement('canvas');
    return v;
  }
  function ensureSeqImage(r, idx) {
    if (!r._imgCache) r._imgCache = {};
    if (r._imgCache[idx]) return r._imgCache[idx];
    if (idx < 0 || idx >= r.frames.length) return null;
    var img = new Image();
    img.onload = function () {
      r._dims = r._dims || { w: img.naturalWidth, h: img.naturalHeight };
      bumpScene();
    };
    img.src = r.frames[idx];
    r._imgCache[idx] = img;
    return img;
  }

  function bumpScene() {
    window._sceneVersion++;
    if (window.SMEngineBridge && window.SMEngineBridge.isEnabled()) window.SMEngineBridge.renderNow();
  }

  // ---- per-frame sync (hooked from loadFrame) ----
  // Bug found 2026-07 ("le lecteur de footage vidéo... sauter des images"):
  // `_seekBusy` was ONLY ever cleared inside the 'seeked' handler, with no
  // fallback. Per the HTML5 spec, setting `currentTime` to a value the
  // browser resolves to the SAME decoded picture it's already showing
  // (common when two different requested times round to the same frame at
  // a video's own fps, or during fast scrubbing) fires NO 'seeked' event at
  // all. That left `_seekBusy` stuck `true` forever the first time it
  // happened — every later syncToFrame call would just silently overwrite
  // `_pendingT` without ever applying it, permanently freezing the
  // reference on a stale frame instead of the documented "lags a frame or
  // two" tolerance. Fix: a watchdog timeout races the 'seeked' event; if it
  // fires first, force-unstick exactly as if 'seeked' had arrived, so a
  // silently-skipped event degrades to a brief stall instead of a
  // permanent one.
  var SEEK_WATCHDOG_MS = 300;
  function syncToFrame(frame, force) {
    var r = ref();
    if (!r || !r.visible) return;
    if (r.type === 'video') {
      var v = ensureVideo(r);
      if (!v.videoWidth) return; // metadata not in yet — loadedmetadata re-syncs
      var t = Math.max(0, Math.min(v.duration || 0, (frame - (r.offsetFrames || 0)) / state.fps));
      if (!force && Math.abs(v.currentTime - t) < 0.5 / state.fps) return;
      if (r._seekBusy) { r._pendingT = t; return; } // collapse seek bursts — only the latest target matters
      startSeek(r, v, t);
    } else if (r.type === 'imageseq') {
      ensureSeqImage(r, frameToSeqIndex(r, frame)); // pre-decode; onload bumps
    }
    // single image: nothing frame-dependent
  }
  function startSeek(r, v, t) {
    r._seekBusy = true;
    var settled = false;
    function finish() {
      if (settled) return; // whichever of 'seeked'/watchdog fires first wins; the other is a no-op
      settled = true;
      v.removeEventListener('seeked', onSeeked);
      clearTimeout(watchdog);
      // try/finally: r._seekBusy=false MUST run even if drawImage/upload
      // throws (a corrupt/mid-decode video frame, a canvas security error,
      // engine-bridge erroring) — otherwise a throw here would leave the
      // flag stuck exactly like the original unguarded bug, just via a
      // different trigger. The whole point of this rewrite is that NOTHING
      // can leave _seekBusy permanently true.
      try {
        var cv = r._seekCanvas;
        cv.width = v.videoWidth; cv.height = v.videoHeight;
        cv.getContext('2d').drawImage(v, 0, 0);
        if (window.SMEngineBridge) window.SMEngineBridge.registerImagePixels(REF_VIDEO_ID, cv);
      } finally {
        r._seekBusy = false;
      }
      bumpScene();
      if (r._pendingT !== undefined) { var pt = r._pendingT; delete r._pendingT; startSeek(r, v, pt); }
    }
    function onSeeked() { finish(); }
    v.addEventListener('seeked', onSeeked);
    var watchdog = setTimeout(finish, SEEK_WATCHDOG_MS);
    v.currentTime = t;
  }
  function frameToSeqIndex(r, frame) {
    return Math.max(0, Math.min(r.frames.length - 1, frame - (r.offsetFrames || 0)));
  }

  // ---- scene item (called by engine-bridge buildSceneJson) ----
  // Returns the image item for the CURRENT frame, or null when hidden /
  // not decoded yet / outside the sequence. registerFn(id, source) uploads
  // pixels; engine-bridge passes its own cached-registration helper for
  // static ids and the forced re-upload path is only used by the video seek.
  function buildRefItem(registerCachedFn) {
    var r = ref();
    if (!r || !r.visible) return null;
    var imageId = null, dims = r._dims;
    if (r.type === 'video') {
      if (!r._video || !r._video.videoWidth) { ensureVideo(r); return null; }
      imageId = REF_VIDEO_ID; // pixels uploaded by syncToFrame's seeked handler
      if (!window.SMEngineBridge.hasImage(REF_VIDEO_ID)) return null;
    } else if (r.type === 'imageseq') {
      var idx = frameToSeqIndex(r, state.currentFrame);
      var img = ensureSeqImage(r, idx);
      if (!img || !img.complete || !img.naturalWidth) return null;
      imageId = 'ref:seq:' + idx;
      registerCachedFn(imageId, img);
      dims = { w: img.naturalWidth, h: img.naturalHeight };
    } else {
      var im = ensureSeqImageSingle(r);
      if (!im || !im.complete || !im.naturalWidth) return null;
      imageId = 'ref:img';
      registerCachedFn(imageId, im);
      dims = { w: im.naturalWidth, h: im.naturalHeight };
    }
    if (!dims) return null;
    // fit-to-artboard, centered — same framing every reference viewer uses
    var s = Math.min(state.canvasW / dims.w, state.canvasH / dims.h);
    var w = dims.w * s, h = dims.h * s;
    return { image: { imageId: imageId, x: (state.canvasW - w) / 2, y: (state.canvasH - h) / 2, width: w, height: h, opacity: r.opacity !== undefined ? r.opacity : 0.5 } };
  }
  function ensureSeqImageSingle(r) {
    if (r._img) return r._img;
    var img = new Image();
    img.onload = function () { r._dims = { w: img.naturalWidth, h: img.naturalHeight }; bumpScene(); };
    img.src = r.src;
    r._img = img;
    return img;
  }

  // ---- import ----
  function setRef(r) {
    state.refMedia = r;
    syncUI();
    if (r) syncToFrame(state.currentFrame, true);
    bumpScene();
  }
  function importFiles(files) {
    if (!files.length) return;
    var f0 = files[0];
    if (f0.type.indexOf('video/') === 0) {
      var rd = new FileReader();
      rd.onload = function (ev) {
        setRef({ type: 'video', name: f0.name, src: ev.target.result, opacity: 0.5, visible: true, offsetFrames: 0 });
        if (ev.target.result.length > 12 * 1024 * 1024) showToast('Vidéo volumineuse : sera incluse dans le fichier projet');
      };
      rd.readAsDataURL(f0);
      return;
    }
    // images: 1 file = static image, several = sequence (sorted by name —
    // the universal convention for numbered image sequences)
    var imgs = Array.prototype.filter.call(files, function (f) { return f.type.indexOf('image/') === 0; });
    if (!imgs.length) { showToast('Format non reconnu (vidéo ou images)'); return; }
    imgs.sort(function (a, b) { return a.name.localeCompare(b.name, undefined, { numeric: true }); });
    var readers = imgs.map(function (f) {
      return new Promise(function (res) {
        var rd = new FileReader();
        rd.onload = function (ev) { res(ev.target.result); };
        rd.readAsDataURL(f);
      });
    });
    Promise.all(readers).then(function (urls) {
      if (urls.length === 1) setRef({ type: 'image', name: imgs[0].name, src: urls[0], opacity: 0.5, visible: true, offsetFrames: 0 });
      else setRef({ type: 'imageseq', name: imgs[0].name + ' (+' + (urls.length - 1) + ')', frames: urls, opacity: 0.5, visible: true, offsetFrames: 0 });
    });
  }

  // ---- panel UI sync ----
  function syncUI() {
    var r = ref();
    var name = document.getElementById('p-ref-name');
    if (name) name.textContent = r ? (r.name + (r.type === 'imageseq' ? ' — séquence' : r.type === 'video' ? ' — vidéo' : '')) : 'Aucune référence';
    var on = document.getElementById('p-ref-on'); if (on) on.checked = !!(r && r.visible);
    var op = document.getElementById('p-ref-opacity'); if (op && r) op.value = Math.round((r.opacity !== undefined ? r.opacity : 0.5) * 100);
    var off = document.getElementById('p-ref-offset'); if (off && r) off.value = r.offsetFrames || 0;
    var rm = document.getElementById('btn-ref-remove'); if (rm) rm.disabled = !r;
  }

  window.SMReference = {
    buildRefItem: buildRefItem,
    onFrameChanged: syncToFrame,
    importFiles: importFiles,
    // after importJSON/newProject swapped state.refMedia wholesale
    reload: function () { syncUI(); var r = ref(); if (r) syncToFrame(state.currentFrame, true); bumpScene(); },
  };

  function init() {
    var btn = document.getElementById('btn-ref-import');
    var input = document.getElementById('ref-file-input');
    if (btn && input) {
      btn.addEventListener('click', function () { input.click(); });
      input.addEventListener('change', function (e) { importFiles(e.target.files); e.target.value = ''; });
    }
    var on = document.getElementById('p-ref-on');
    if (on) on.addEventListener('change', function () { var r = ref(); if (r) { r.visible = on.checked; if (r.visible) syncToFrame(state.currentFrame, true); bumpScene(); } });
    var op = document.getElementById('p-ref-opacity');
    if (op) op.addEventListener('input', function () { var r = ref(); if (r) { r.opacity = op.value / 100; bumpScene(); } });
    var off = document.getElementById('p-ref-offset');
    if (off) off.addEventListener('change', function () { var r = ref(); if (r) { r.offsetFrames = parseInt(off.value) || 0; syncToFrame(state.currentFrame, true); bumpScene(); } });
    var rm = document.getElementById('btn-ref-remove');
    if (rm) rm.addEventListener('click', function () { setRef(null); });
    syncUI();
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', init);
  else init();
})();
