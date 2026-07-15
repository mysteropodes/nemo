// ---- LABS PROTOTYPE — Tween motion path + spacing ticks ----
// (2026-07, "j'aime bien les poignées du motion path caméra, est-ce qu'on
// peut appliquer ça au motion path de tween ? en plus d'avoir visuellement
// des barres d'espacement de keyframes tween sur le motion path"):
// camera.js and motion.js both already draw a dashed bezier "motion path"
// with draggable hOut/hIn handles for a SINGLE tracked point (the camera
// rect's center, or a Motion-mode position track) — but 2D Animation's
// shape TWEENING (tweens.js/generateTweens) has no equivalent concept at
// all: a tweened stroke's shape is a per-vertex correspondence between two
// drawn poses, not one x/y value, so there is no single "position" to
// attach a spatial bezier handle to the way camera/motion do.
//
// Scope of this v1, deliberately: a READ-ONLY visualization, not
// draggable handles. Making the handles genuinely bend the tween's spatial
// path would mean teaching tweens.js/interpStroke an entirely new spatial-
// offset concept it doesn't have today (camera/motion's handles work
// because they animate ONE point; a shape tween has none) — a real,
// separate engineering effort, not a Labs-sized addition. What IS shown
// here is 100% accurate and immediately useful on its own: the selected
// stroke's CENTROID at each keyframe (dashed line, camera-style dots), and
// a perpendicular tick mark at every actual in-between frame's ALREADY-
// COMPUTED position (read straight from ld.frames[f].strokes — the same
// baked interpolation result generateTweens() already produced, so
// whatever easing is really applied is exactly what the ticks show, no
// re-derivation of the easing math needed or risked).
//
//   SMLabs.enable('tween-motion-path')
//   Select exactly one stroke (Select tool) that has a strokeId — the
//   path/ticks track that stroke across the layer's keyframes.
(function () {
  function centroidOf(strokeData) {
    var segs = strokeData && strokeData.segments;
    if (!segs || !segs.length) return null;
    var sx = 0, sy = 0;
    segs.forEach(function (s) { sx += s.point[0]; sy += s.point[1]; });
    return [sx / segs.length, sy / segs.length];
  }
  function findByStrokeId(strokes, id) {
    if (!strokes) return null;
    for (var i = 0; i < strokes.length; i++) if (strokes[i].strokeId === id) return strokes[i];
    return null;
  }
  function circleSegs(cx, cy, r) {
    var k = r * 0.5523;
    return [
      { point: [cx + r, cy], handleIn: [0, k], handleOut: [0, -k] },
      { point: [cx, cy - r], handleIn: [k, 0], handleOut: [-k, 0] },
      { point: [cx - r, cy], handleIn: [0, -k], handleOut: [0, k] },
      { point: [cx, cy + r], handleIn: [-k, 0], handleOut: [k, 0] },
    ];
  }

  function onOverlay() {
    if (typeof selectedPaths === 'undefined' || selectedPaths.length !== 1) return null;
    var sel = selectedPaths[0];
    var strokeId = sel && sel.data && sel.data.strokeId;
    if (!strokeId) return null;
    var li = state.activeLayerIdx, ld = state.layers[li];
    if (!ld || ld.symbolId) return null;
    var frames = ld.frames;
    var zs = 1 / Math.max(0.0001, view.zoom);
    var pathCol = [230, 110, 170, 190]; // soft pink — distinct from camera's blue and Motion's accent
    var tickCol = [255, 150, 50, 200]; // muted orange, discreet but visible
    var items = [];

    // Keyframe indices where this stroke exists — connects consecutive
    // ones directly (no run-boundary logic needed: if the strokeId isn't
    // in a keyframe, there's simply nothing to draw across that gap).
    var keyIdx = [];
    for (var f = 0; f < frames.length; f++) {
      if (frames[f].isKeyframe && findByStrokeId(frames[f].strokes, strokeId)) keyIdx.push(f);
    }
    if (keyIdx.length < 2) return null;

    for (var i = 0; i < keyIdx.length - 1; i++) {
      var fA = keyIdx[i], fB = keyIdx[i + 1];
      var a = centroidOf(findByStrokeId(frames[fA].strokes, strokeId));
      var b = centroidOf(findByStrokeId(frames[fB].strokes, strokeId));
      if (!a || !b) continue;
      // Only draw across a contiguous tween run — a gap where the stroke
      // reappears in a LATER keyframe after being absent from in-between
      // frames isn't the same tween at all, just a coincidentally-reused
      // strokeId (rare, e.g. after Ctrl+Z history), and connecting it would
      // draw a misleading path across unrelated poses.
      var isRun = true;
      for (var mf = fA + 1; mf < fB; mf++) {
        if (!frames[mf].isInterpolated || !findByStrokeId(frames[mf].strokes, strokeId)) { isRun = false; break; }
      }
      items.push({ segments: [{ point: a }, { point: b }], closed: false, fillColor: null, strokeColor: pathCol, strokeWidth: 1.4 * zs, dashPattern: [5 * zs, 4 * zs] });
      var isCurA = fA === state.currentFrame, isCurB = fB === state.currentFrame;
      items.push({ segments: circleSegs(a[0], a[1], (isCurA ? 5.5 : 4) * zs), closed: true, fillColor: isCurA ? [255, 170, 40, 255] : [230, 230, 230, 255], strokeColor: [30, 30, 30, 255], strokeWidth: 1 * zs });
      items.push({ segments: circleSegs(b[0], b[1], (isCurB ? 5.5 : 4) * zs), closed: true, fillColor: isCurB ? [255, 170, 40, 255] : [230, 230, 230, 255], strokeColor: [30, 30, 30, 255], strokeWidth: 1 * zs });
      if (!isRun) continue;
      // Spacing ticks — one per real in-between frame, positioned at that
      // frame's ACTUAL baked centroid (not assumed-linear), oriented
      // perpendicular to the local A->B direction so bunched-up ticks near
      // an eased end read clearly as "close together" the way a spacing
      // chart does.
      var dx = b[0] - a[0], dy = b[1] - a[1];
      var len = Math.hypot(dx, dy) || 1;
      var px = -dy / len, py = dx / len;
      var tickLen = 6 * zs;
      for (var tf = fA + 1; tf < fB; tf++) {
        var c = centroidOf(findByStrokeId(frames[tf].strokes, strokeId));
        if (!c) continue;
        var isCurT = tf === state.currentFrame;
        items.push({
          segments: [{ point: [c[0] - px * tickLen, c[1] - py * tickLen] }, { point: [c[0] + px * tickLen, c[1] + py * tickLen] }],
          closed: false, fillColor: null, strokeColor: isCurT ? [255, 170, 40, 255] : tickCol, strokeWidth: (isCurT ? 2 : 1.3) * zs,
        });
      }
    }
    return items;
  }

  window.SMLabs.register('tween-motion-path', {
    flag: 'nemo-labs-tween-path',
    describe: 'Sélectionne un trait tweené (outil Sélection) : montre son chemin entre keyframes + une barre par frame intermédiaire, à sa vraie position calculée (lecture seule)',
    onOverlay: onOverlay,
  });
})();
