// ---- LOTTIE JSON PREVIEW ----
// Reads back the JSON export.js just produced and renders it frame-by-frame
// into its own separate Paper.js project/canvas — a round-trip sanity check
// ("does what we emitted actually reconstruct into what the timeline showed")
// rather than a pixel-perfect lottie-web replica. Only interprets the exact
// shape of JSON lottieBuild() itself emits (single 'sh'/'fl'/'st'/'tr' group
// per ty:4 layer, one keyframe per baked frame, ks.a always 1) — this is not
// a general-purpose Lottie player.
(function () {
  var mainProject = null, previewProject = null, currentJson = null;
  var playing = false, playTimer = null;

  function ensurePreviewProject() {
    if (previewProject) return previewProject;
    var canvas = document.getElementById('lottie-preview-canvas');
    mainProject = paper.project; // whichever project is active right now (the live app)
    previewProject = new paper.Project(canvas);
    mainProject.activate(); // don't leave the preview project active — nothing else in the app expects that
    return previewProject;
  }

  // Finds the keyframe active at `frame` in a Lottie animated property
  // (ks.a===1, k = array of {t,s:[...]}) — every frame in our own export has
  // its own literal keyframe (baked, no re-derived easing), so this is a
  // hold-to-nearest-past-keyframe lookup, not real bezier interpolation.
  function shapeValueAt(shItem, frame) {
    var k = shItem.ks.k;
    if (!Array.isArray(k)) return k; // static (shouldn't happen for our own export, kept for safety)
    var chosen = k[0];
    for (var i = 0; i < k.length; i++) { if (k[i].t <= frame) chosen = k[i]; else break; }
    return chosen.s[0];
  }
  var LOTTIE_BM_REVERSE = { 1: 'multiply', 2: 'screen', 3: 'overlay', 4: 'darken', 5: 'lighten', 6: 'colorDodge', 7: 'colorBurn', 8: 'hardLight', 9: 'softLight', 10: 'difference', 11: 'exclusion', 12: 'hue', 13: 'saturation', 14: 'color', 15: 'luminosity' };

  function renderFrame(json, frameIdx) {
    var proj = ensurePreviewProject();
    var canvas = document.getElementById('lottie-preview-canvas');
    if (canvas.width !== json.w || canvas.height !== json.h) {
      canvas.width = json.w; canvas.height = json.h;
    }
    proj.activate();
    proj.view.viewSize = new Size(json.w, json.h);
    proj.clear();
    // Lottie layers are first-entry-on-top; Paper.js paints later-added
    // children on top, so walk the array back-to-front to reproduce the
    // same stacking (see lottieBuild's own reverse in export.js for why).
    json.layers.slice().reverse().forEach(function (L) {
      if (frameIdx < L.ip || frameIdx >= L.op) return;
      if (L.ty === 1) {
        new Path.Rectangle({ point: [0, 0], size: [L.sw, L.sh], fillColor: L.sc, insert: true });
        return;
      }
      if (L.ty !== 4) return;
      var items = (L.shapes && L.shapes[0] && L.shapes[0].it) || [];
      var shItem = null, flItem = null, stItem = null;
      items.forEach(function (it) {
        if (it.ty === 'sh') shItem = it;
        else if (it.ty === 'fl') flItem = it;
        else if (it.ty === 'st') stItem = it;
      });
      if (!shItem) return;
      var val = shapeValueAt(shItem, frameIdx);
      if (!val || !val.v || !val.v.length) return;
      var path = new Path();
      for (var i = 0; i < val.v.length; i++) {
        var pt = val.v[i], hi = val.i[i], ho = val.o[i];
        path.add(new Segment(new Point(pt[0], pt[1]), new Point(hi[0], hi[1]), new Point(ho[0], ho[1])));
      }
      if (val.c) path.closed = true;
      if (stItem) {
        var sc = stItem.c.k;
        path.strokeColor = new Color(sc[0], sc[1], sc[2], (stItem.o.k !== undefined ? stItem.o.k : 100) / 100);
        path.strokeWidth = stItem.w.k;
        path.strokeCap = stItem.lc === 2 ? 'round' : (stItem.lc === 3 ? 'square' : 'butt');
        path.strokeJoin = stItem.lj === 2 ? 'round' : (stItem.lj === 3 ? 'bevel' : 'miter');
      }
      if (flItem) {
        var fc = flItem.c.k;
        path.fillColor = new Color(fc[0], fc[1], fc[2], (flItem.o.k !== undefined ? flItem.o.k : 100) / 100);
      }
      if (L.bm && LOTTIE_BM_REVERSE[L.bm]) path.blendMode = LOTTIE_BM_REVERSE[L.bm];
    });
    proj.view.update();
    mainProject.activate();
  }

  function stopPlayback() {
    playing = false;
    if (playTimer) { clearInterval(playTimer); playTimer = null; }
    var btn = document.getElementById('lottie-preview-play');
    if (btn) btn.innerHTML = '&#9654;';
  }

  function goToFrame(f) {
    if (!currentJson) return;
    f = Math.max(currentJson.ip, Math.min(currentJson.op - 1, f));
    renderFrame(currentJson, f);
    var scrub = document.getElementById('lottie-preview-scrub');
    if (scrub) scrub.value = f;
    var lbl = document.getElementById('lottie-preview-frame');
    if (lbl) lbl.textContent = (f - currentJson.ip + 1) + ' / ' + (currentJson.op - currentJson.ip);
  }

  function initControls() {
    var modal = document.getElementById('lottie-preview-modal');
    var closeBtn = document.getElementById('lottie-preview-close');
    var scrub = document.getElementById('lottie-preview-scrub');
    var playBtn = document.getElementById('lottie-preview-play');
    if (closeBtn) closeBtn.addEventListener('click', function () { stopPlayback(); modal.style.display = 'none'; });
    modal.addEventListener('click', function (e) { if (e.target === modal) { stopPlayback(); modal.style.display = 'none'; } });
    if (scrub) scrub.addEventListener('input', function () { stopPlayback(); goToFrame(parseInt(scrub.value, 10)); });
    if (playBtn) playBtn.addEventListener('click', function () {
      if (playing) { stopPlayback(); return; }
      playing = true; playBtn.innerHTML = '&#10074;&#10074;';
      var cur = parseInt(scrub.value, 10);
      playTimer = setInterval(function () {
        cur++;
        if (cur > currentJson.op - 1) cur = currentJson.ip;
        goToFrame(cur);
      }, 1000 / (currentJson.fr || 24));
    });
  }
  var controlsReady = false;

  window.SMLottiePreview = {
    open: function (json) {
      if (!controlsReady) { initControls(); controlsReady = true; }
      currentJson = json;
      stopPlayback();
      var scrub = document.getElementById('lottie-preview-scrub');
      if (scrub) { scrub.min = json.ip; scrub.max = json.op - 1; scrub.value = json.ip; }
      var modal = document.getElementById('lottie-preview-modal');
      modal.style.display = 'flex';
      goToFrame(json.ip);
    }
  };
})();
