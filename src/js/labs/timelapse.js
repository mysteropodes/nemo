// ---- LABS PROTOTYPE — Timelapse recording (Clip Studio Paint) ----
// CSP records your strokes as you work and renders the session as a
// video. Here: MediaRecorder on the visible (Rust/WebGPU) canvas's
// captureStream — the exact pixels on screen, camera moves and all —
// saved as a .webm download on stop.
//
//   SMLabs.timelapseStart(fps?)   — begin (default 30fps capture)
//   SMLabs.timelapseStop()        — stop + download nemo-timelapse.webm
//
// Zero interaction with the drawing pipeline: captureStream reads the
// canvas compositor output, it never touches Paper/engine state. VP9
// with VP8/default fallback depending on platform support.
(function () {
  var rec = null, chunks = [], t0 = 0, comp = null, compTimer = null;

  function findCanvas() {
    // The Rust engine canvas is the visible one; Paper's #drawing-canvas
    // sits under it and is only visible when the engine is off. Prefer
    // whichever is actually rendering.
    var cands = Array.prototype.slice.call(document.querySelectorAll('#canvas-area canvas, canvas'));
    var vis = cands.filter(function (c) {
      var r = c.getBoundingClientRect();
      return r.width > 50 && r.height > 50 && getComputedStyle(c).display !== 'none';
    });
    // Topmost in DOM paint order ≈ last visible one.
    return vis.length ? vis[vis.length - 1] : null;
  }

  window.SMLabs.timelapseStart = function (fps) {
    if (!window.SMLabs.isOn('timelapse')) { console.warn('[labs] enable(\'timelapse\') d\'abord'); return false; }
    if (rec) { console.warn('[labs] déjà en cours'); return false; }
    var cv = findCanvas();
    if (!cv) { console.warn('[labs] pas de canvas capturable'); return false; }
    fps = Math.max(1, Math.min(60, fps || 30));
    // captureStream() directly on the WebGPU canvas records ~nothing
    // (found live: 3s → a 652-byte webm; WebGPU canvases don't reliably
    // surface frames to captureStream). Composite the source into a
    // hidden 2D canvas on our own clock via drawImage (a WebGPU canvas IS
    // a valid CanvasImageSource) and record THAT stream instead — the 2D
    // canvas repaints every interval, so the recorder always has frames.
    comp = document.createElement('canvas');
    comp.width = cv.width; comp.height = cv.height;
    var cctx = comp.getContext('2d');
    compTimer = setInterval(function () {
      if (comp.width !== cv.width || comp.height !== cv.height) { comp.width = cv.width; comp.height = cv.height; }
      cctx.drawImage(cv, 0, 0);
    }, 1000 / fps);
    var stream = comp.captureStream(fps);
    var mime = ['video/webm;codecs=vp9', 'video/webm;codecs=vp8', 'video/webm']
      .find(function (m) { return window.MediaRecorder && MediaRecorder.isTypeSupported(m); });
    if (!mime) { console.warn('[labs] MediaRecorder/webm non supporté ici'); clearInterval(compTimer); comp = null; return false; }
    chunks = [];
    rec = new MediaRecorder(stream, { mimeType: mime, videoBitsPerSecond: 6e6 });
    rec.ondataavailable = function (e) { if (e.data && e.data.size) chunks.push(e.data); };
    rec.start(1000); // periodic chunks — a crash mid-session keeps prior data
    t0 = Date.now();
    if (typeof showToast === 'function') showToast(SM.t('labsToastTimelapseRecording'));
    return true;
  };

  window.SMLabs.timelapseStop = function () {
    if (!rec) { console.warn('[labs] rien à arrêter'); return null; }
    return new Promise(function (resolve) {
      rec.onstop = function () {
        var blob = new Blob(chunks, { type: 'video/webm' });
        rec = null; chunks = [];
        if (compTimer) { clearInterval(compTimer); compTimer = null; }
        comp = null;
        var secs = Math.round((Date.now() - t0) / 1000);
        var u = URL.createObjectURL(blob);
        var a = document.createElement('a');
        a.href = u; a.download = 'nemo-timelapse.webm'; a.click();
        URL.revokeObjectURL(u);
        if (typeof showToast === 'function') showToast(SM.t('labsToastTimelapseDonePrefix') + secs + SM.t('labsToastTimelapseDoneSuffix'));
        resolve({ seconds: secs, bytes: blob.size });
      };
      rec.stop();
    });
  };

  window.SMLabs.register('timelapse', {
    flag: 'nemo-labs-timelapse',
    describe: 'labsDescribeTimelapse',
    onDisable: function () {
      if (rec) { try { rec.stop(); } catch (e) {} rec = null; chunks = []; }
      if (compTimer) { clearInterval(compTimer); compTimer = null; }
      comp = null;
    },
  });
})();
