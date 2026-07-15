// ---- LABS PROTOTYPE — Tween motion path + spacing ticks ----
// (2026-07, "j'aime bien les poignées du motion path caméra, est-ce qu'on
// peut appliquer ça au motion path de tween ?" + follow-up: "regarde le
// motion path de caméra à des poignées pour le in et out alors que le
// motion path de tween c'est juste une poignée pour les 2 donc moins
// réglable"): camera.js and motion.js both draw a dashed CUBIC bezier
// (independent hOut on the earlier key, hIn on the later key — two knobs,
// asymmetric control) for a SINGLE tracked point. 2D Animation's shape
// TWEENING (tweens.js/generateTweens) has no equivalent concept at all: a
// tweened stroke's shape is a per-vertex correspondence between two drawn
// poses, not one x/y value — there is no real "position" for the actual
// interpolation to bend along a spatial bezier the way camera/motion do.
//
// Still gave this its own real hOut/hIn pair per keyframe, matching
// camera/motion's exact interaction (draggable, independent in/out), NOT a
// single shared handle — stored Labs-side only (nemo-labs-tween-handles,
// keyed by strokeId+frame), never touching the project file, so it can't
// silently corrupt anything tweens.js reads. Honest about what it is: a
// PLANNING GUIDE, not a live control of the real tween. The dashed curve
// bends when you drag its handles; the spacing TICKS do not follow it —
// they stay at each in-between frame's ACTUAL baked centroid (read
// straight from ld.frames[f].strokes, generateTweens()'s real computed
// result), so the guide and the ticks together answer two different
// questions at once: "what arc am I planning?" (the curve) vs "what does
// the tween actually do right now?" (the ticks) — useful precisely because
// they're allowed to disagree until the keyframes themselves are redrawn
// to match.
//
//   SMLabs.enable('tween-motion-path')
//   Select exactly one stroke (Select tool) that has a strokeId — the
//   path/ticks track that stroke across the layer's keyframes. Drag the
//   small handle dots to bend the guide curve.
(function () {
  var HANDLE_KEY = 'nemo-labs-tween-handles';
  function loadHandles() { try { return JSON.parse(localStorage.getItem(HANDLE_KEY)) || {}; } catch (e) { return {}; } }
  function saveHandles(m) { try { localStorage.setItem(HANDLE_KEY, JSON.stringify(m)); } catch (e) {} }
  function hKey(strokeId, frame) { return strokeId + ':' + frame; }
  function getHandles(strokeId, frame) {
    var m = loadHandles(), h = m[hKey(strokeId, frame)];
    return { hOut: (h && h.hOut) || [0, 0], hIn: (h && h.hIn) || [0, 0] };
  }
  function setHandle(strokeId, frame, which, val) {
    var m = loadHandles(), k = hKey(strokeId, frame);
    if (!m[k]) m[k] = { hOut: [0, 0], hIn: [0, 0] };
    m[k][which] = val;
    saveHandles(m);
  }

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

  // Shared by onOverlay (drawing) and the hit-test (dragging) — the exact
  // set of consecutive-keyframe segments for the currently selected
  // stroke, each with both keyframes' real centroids.
  function activeSegments() {
    if (typeof selectedPaths === 'undefined' || selectedPaths.length !== 1) return null;
    var sel = selectedPaths[0];
    var strokeId = sel && sel.data && sel.data.strokeId;
    if (!strokeId) return null;
    var li = state.activeLayerIdx, ld = state.layers[li];
    if (!ld || ld.symbolId) return null;
    var frames = ld.frames;
    var keyIdx = [];
    for (var f = 0; f < frames.length; f++) {
      if (frames[f].isKeyframe && findByStrokeId(frames[f].strokes, strokeId)) keyIdx.push(f);
    }
    if (keyIdx.length < 2) return null;
    var segs = [];
    for (var i = 0; i < keyIdx.length - 1; i++) {
      var fA = keyIdx[i], fB = keyIdx[i + 1];
      var a = centroidOf(findByStrokeId(frames[fA].strokes, strokeId));
      var b = centroidOf(findByStrokeId(frames[fB].strokes, strokeId));
      if (!a || !b) continue;
      var isRun = true;
      for (var mf = fA + 1; mf < fB; mf++) {
        if (!frames[mf].isInterpolated || !findByStrokeId(frames[mf].strokes, strokeId)) { isRun = false; break; }
      }
      segs.push({ strokeId: strokeId, fA: fA, fB: fB, a: a, b: b, isRun: isRun, frames: frames });
    }
    return segs.length ? segs : null;
  }

  function onOverlay() {
    var segs = activeSegments();
    if (!segs) return null;
    var zs = 1 / Math.max(0.0001, view.zoom);
    var pathCol = [230, 110, 170, 190]; // soft pink — distinct from camera's blue and Motion's accent
    var tickCol = [255, 150, 50, 200]; // muted orange, discreet but visible
    var handleCol = [255, 170, 40, 220]; // same handle color camera.js/motion.js use
    var items = [];

    segs.forEach(function (seg) {
      var a = seg.a, b = seg.b, fA = seg.fA, fB = seg.fB, strokeId = seg.strokeId, frames = seg.frames;
      var h = getHandles(strokeId, fA), hOut = h.hOut;
      var hEnd = getHandles(strokeId, fB), hIn = hEnd.hIn;
      // Cubic bezier through the two centroids — same construction as
      // camera.js/motion.js's own motion path, straight line when both
      // handles are still [0,0] (the default, matching their behavior).
      var pts = [];
      var steps = 24;
      for (var s = 0; s <= steps; s++) {
        var t = s / steps, v = 1 - t;
        pts.push({ point: [
          v * v * v * a[0] + 3 * v * v * t * (a[0] + hOut[0]) + 3 * v * t * t * (b[0] + hIn[0]) + t * t * t * b[0],
          v * v * v * a[1] + 3 * v * v * t * (a[1] + hOut[1]) + 3 * v * t * t * (b[1] + hIn[1]) + t * t * t * b[1],
        ] });
      }
      items.push({ segments: pts, closed: false, fillColor: null, strokeColor: pathCol, strokeWidth: 1.4 * zs, dashPattern: [5 * zs, 4 * zs] });
      var isCurA = fA === state.currentFrame, isCurB = fB === state.currentFrame;
      items.push({ segments: circleSegs(a[0], a[1], (isCurA ? 5.5 : 4) * zs), closed: true, fillColor: isCurA ? [255, 170, 40, 255] : [230, 230, 230, 255], strokeColor: [30, 30, 30, 255], strokeWidth: 1 * zs });
      items.push({ segments: circleSegs(b[0], b[1], (isCurB ? 5.5 : 4) * zs), closed: true, fillColor: isCurB ? [255, 170, 40, 255] : [230, 230, 230, 255], strokeColor: [30, 30, 30, 255], strokeWidth: 1 * zs });
      // Independent in/out handles — one dot+line off EACH keyframe, not a
      // single shared knob, matching camera/motion's own two-handle rig.
      var hx1 = a[0] + hOut[0], hy1 = a[1] + hOut[1];
      items.push({ segments: [{ point: a }, { point: [hx1, hy1] }], closed: false, fillColor: null, strokeColor: handleCol, strokeWidth: 1 * zs });
      items.push({ segments: circleSegs(hx1, hy1, 4 * zs), closed: true, fillColor: handleCol, strokeColor: [30, 30, 30, 255], strokeWidth: 1 * zs });
      var hx2 = b[0] + hIn[0], hy2 = b[1] + hIn[1];
      items.push({ segments: [{ point: b }, { point: [hx2, hy2] }], closed: false, fillColor: null, strokeColor: handleCol, strokeWidth: 1 * zs });
      items.push({ segments: circleSegs(hx2, hy2, 4 * zs), closed: true, fillColor: handleCol, strokeColor: [30, 30, 30, 255], strokeWidth: 1 * zs });
      if (!seg.isRun) return;
      // Spacing ticks — real in-between centroids, NOT projected onto the
      // (possibly bent) guide curve above — see file header for why they're
      // deliberately allowed to diverge from it.
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
    });
    return items;
  }

  // -- dragging the hOut/hIn handles ---------------------------------------
  // Document-capture, same pattern as vector-sculpt.js/french-curve.js:
  // these listeners are on `document` (an ANCESTOR of #canvas-area), so in
  // the capture phase they fire BEFORE select-bridge.js's own capture
  // listener on #canvas-area ever does — no way for a descendant's
  // listener to out-race an ancestor's regardless of registration order
  // (confirmed the hard way building the Labs floating panel this same
  // session). Armed only when this prototype is on, the Select tool is
  // active, and the pointer actually starts on a handle — anything else
  // falls through untouched to select-bridge.js's own marquee/move/scale.
  var dragging = null; // {strokeId, frame, which}
  function hitHandle(pt) {
    var segs = activeSegments();
    if (!segs) return null;
    var tol = 10 / view.zoom;
    for (var i = 0; i < segs.length; i++) {
      var seg = segs[i];
      var hOut = getHandles(seg.strokeId, seg.fA).hOut;
      var hx1 = seg.a[0] + hOut[0], hy1 = seg.a[1] + hOut[1];
      if (Math.hypot(pt[0] - hx1, pt[1] - hy1) < tol) return { strokeId: seg.strokeId, frame: seg.fA, which: 'hOut', anchor: seg.a };
      var hIn = getHandles(seg.strokeId, seg.fB).hIn;
      var hx2 = seg.b[0] + hIn[0], hy2 = seg.b[1] + hIn[1];
      if (Math.hypot(pt[0] - hx2, pt[1] - hy2) < tol) return { strokeId: seg.strokeId, frame: seg.fB, which: 'hIn', anchor: seg.b };
    }
    return null;
  }
  function armed() {
    return window.SMLabs && window.SMLabs.isOn('tween-motion-path') && window.state && state.tool === 'select' && window.SMEngineBridge && window.SMEngineBridge.isEnabled();
  }
  document.addEventListener('pointerdown', function (e) {
    if (!armed()) return;
    var w = window.SMEngineBridge.screenToWorld(e.clientX, e.clientY);
    var hit = hitHandle(w);
    if (!hit) return;
    e.stopImmediatePropagation();
    e.preventDefault();
    dragging = hit;
  }, true);
  document.addEventListener('pointermove', function (e) {
    if (!dragging) return;
    e.stopImmediatePropagation();
    e.preventDefault();
    var w = window.SMEngineBridge.screenToWorld(e.clientX, e.clientY);
    setHandle(dragging.strokeId, dragging.frame, dragging.which, [w[0] - dragging.anchor[0], w[1] - dragging.anchor[1]]);
    window.SMEngineBridge.renderNow();
  }, true);
  document.addEventListener('pointerup', function (e) {
    if (!dragging) return;
    e.stopImmediatePropagation();
    e.preventDefault();
    dragging = null;
  }, true);

  window.SMLabs.register('tween-motion-path', {
    flag: 'nemo-labs-tween-path',
    describe: 'Sélectionne un trait tweené (outil Sélection) : chemin entre keyframes avec poignées in/out indépendantes (comme la caméra) + une barre par frame intermédiaire à sa vraie position calculée',
    onOverlay: onOverlay,
  });
})();
