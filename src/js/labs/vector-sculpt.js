// ---- LABS PROTOTYPE — Vector sculpt: push & smooth (Toon Boom / Umoupen) ----
// The "finger" for vector ink: hold W and drag over the drawing —
// every stroke point inside the brush radius is PUSHED along the drag
// with a soft falloff (Toon Boom contour nudging / Umoupen warp,
// Illustrator's Warp tool). Hold W+Shift and scrub instead to SMOOTH
// (each point relaxes toward its neighbors' midpoint — Harmony's Smooth
// Editor).
//
//   held W          — push/warp brush
//   held W + Shift  — smooth/relax brush
//   SMLabs.setSculptRadius(px)   — SCREEN-px brush radius (default 60)
//
// Interception: document-level capture listeners (they fire BEFORE the
// tool bridges' own #canvas-area capture handlers), armed ONLY while W is
// physically held with the flag on — release W and the app never knows
// this file exists. No core file touched. Displaces segment POINTS only
// (handles follow their point, curve character survives); vector-brush
// ribbons and fill shapes are skipped (their outlines are rebuilt from
// centerSegments — sculpting the outline would desync, same reason
// predictive-stroke skips them). Fills bounded by sculpted walls are
// regenerated at gesture end via fillRegenerateLinked, like the eraser
// does per bite. One pushUndo per gesture.
(function () {
  var RAD_KEY = 'nemo-labs-sculpt-radius';
  var armed = false, dragging = false, lastW = null, undoPushed = false, touched = [];

  function radiusScreen() {
    var n = parseFloat(localStorage.getItem(RAD_KEY) || '60');
    return (isNaN(n) || n < 8) ? 60 : Math.min(400, n);
  }
  window.SMLabs.setSculptRadius = function (n) {
    localStorage.setItem(RAD_KEY, String(Math.max(8, Math.min(400, +n || 60))));
    if (typeof showToast === 'function') showToast('Labs — sculpt : rayon ' + radiusScreen() + 'px écran');
    return radiusScreen();
  };

  function sculptables(layer) {
    return layer.children.filter(function (c) {
      if (!(c instanceof Path) || !c.segments || c.segments.length < 2) return false;
      if (c.data && (c.data.isVectorBrush || c.data.isFillShape || c.data.isBrushTextureCopy || c.data.ghostFrame !== undefined)) return false;
      return true;
    });
  }

  // A long simplified stroke can pass straight through the brush with
  // both anchors far outside it (a ruler-straight line has exactly 2) —
  // pushing anchors alone then does nothing (found live). Toon Boom
  // subdivides under the brush: if the CURVE runs within the radius but
  // no anchor is inside, insert anchors at the nearest spot (and once
  // more each side for a bendable span) before displacing.
  function densifyUnderBrush(p, center, R) {
    for (var guard = 0; guard < 6; guard++) {
      var anyInside = p.segments.some(function (seg) { return seg.point.getDistance(center) < R * 0.8; });
      if (anyInside) return;
      var near = p.getNearestLocation(center);
      if (!near || near.point.getDistance(center) >= R) return;
      p.divideAt(near);
    }
  }
  function applyPush(layer, center, delta, R) {
    var moved = false;
    sculptables(layer).forEach(function (p) {
      // Cheap reject: brush circle vs stroke bbox.
      if (!p.bounds.expand(2 * R).contains(center)) return;
      densifyUnderBrush(p, center, R);
      var hit = false;
      p.segments.forEach(function (seg) {
        var d = seg.point.getDistance(center);
        if (d >= R) return;
        var w = 1 - d / R; w = w * w; // quadratic falloff
        seg.point = seg.point.add(delta.multiply(w));
        hit = true;
      });
      if (hit) { moved = true; if (touched.indexOf(p) < 0) touched.push(p); }
    });
    return moved;
  }

  function applySmooth(layer, center, R) {
    var moved = false;
    sculptables(layer).forEach(function (p) {
      if (!p.bounds.expand(2 * R).contains(center)) return;
      var segs = p.segments;
      var hit = false;
      // Relax interior points toward their neighbors' midpoint; endpoints
      // stay pinned so the stroke never shrinks off its anchors.
      for (var i = 1; i < segs.length - 1; i++) {
        var d = segs[i].point.getDistance(center);
        if (d >= R) continue;
        var w = (1 - d / R); w = w * w * 0.35; // gentler than push
        var mid = segs[i - 1].point.add(segs[i + 1].point).divide(2);
        segs[i].point = segs[i].point.add(mid.subtract(segs[i].point).multiply(w));
        hit = true;
      }
      if (hit) { moved = true; if (touched.indexOf(p) < 0) touched.push(p); }
    });
    return moved;
  }

  function onDown(e) {
    if (!armed) return;
    e.stopImmediatePropagation(); e.preventDefault();
    dragging = true; undoPushed = false; touched = [];
    var w = SMEngineBridge.screenToWorld(e.clientX, e.clientY);
    lastW = new Point(w[0], w[1]);
  }
  function onMove(e) {
    if (!armed || !dragging) return;
    e.stopImmediatePropagation(); e.preventDefault();
    var w = SMEngineBridge.screenToWorld(e.clientX, e.clientY);
    var cur = new Point(w[0], w[1]);
    var layer = userLayers[state.activeLayerIdx];
    if (!layer || state.layers[state.activeLayerIdx].locked) return;
    var R = radiusScreen() / view.zoom;
    if (!undoPushed) { pushUndo(); ensureKeyframe(); undoPushed = true; }
    var moved = e.shiftKey ? applySmooth(layer, cur, R) : applyPush(layer, cur, cur.subtract(lastW), R);
    lastW = cur;
    if (moved) { saveActiveLayerFrame(); SMEngineBridge.renderNow(); }
  }
  function onUp(e) {
    if (!armed || !dragging) return;
    e.stopImmediatePropagation(); e.preventDefault();
    dragging = false; lastW = null;
    var layer = userLayers[state.activeLayerIdx];
    // Fills whose walls were sculpted re-trace against the new geometry.
    touched.forEach(function (p) { if (typeof fillRegenerateLinked === 'function') fillRegenerateLinked(layer, p); });
    touched = [];
    saveActiveLayerFrame(); updateUI();
    SMEngineBridge.renderNow();
  }

  document.addEventListener('keydown', function (e) {
    if (!window.SMLabs.isOn('vector-sculpt')) return;
    var tag = (e.target && e.target.tagName) || '';
    if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT' || (e.target && e.target.isContentEditable)) return;
    if (e.key !== 'w' && e.key !== 'W') return;
    if (e.metaKey || e.ctrlKey || e.altKey) return;
    e.preventDefault();
    armed = true;
  }, true);
  document.addEventListener('keyup', function (e) {
    if (e.key === 'w' || e.key === 'W') { armed = false; dragging = false; lastW = null; }
  }, true);
  window.addEventListener('blur', function () { armed = false; dragging = false; lastW = null; });

  // Document-level capture: fires before the tool bridges' #canvas-area
  // capture handlers, so while W is held no bridge or Paper handler ever
  // sees the gesture.
  document.addEventListener('pointerdown', onDown, true);
  document.addEventListener('pointermove', onMove, true);
  document.addEventListener('pointerup', onUp, true);
  document.addEventListener('pointercancel', onUp, true);

  window.SMLabs.register('vector-sculpt', {
    flag: 'nemo-labs-sculpt',
    describe: 'Pousse-vecteurs (Toon Boom/Umoupen) : maintenir W + glisser pousse les points sous la brosse (falloff doux), W+Shift lisse ; SMLabs.setSculptRadius(px)',
    onDisable: function () { armed = false; dragging = false; lastW = null; touched = []; },
  });
})();
