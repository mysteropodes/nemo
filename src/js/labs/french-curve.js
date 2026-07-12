// ---- LABS PROTOTYPE — French curve / ellipse guide snap (SketchBook) ----
// feature-scouting.md #5 flagged this as needing to intercept the stroke
// DURING the drag — normally that means editing draw-bridge.js's own
// pointermove loop, the most perf-sensitive code in the app (§5). Sidestep
// entirely: this is a SEPARATE self-contained tool armed only while F is
// held (same document-capture-before-the-bridges pattern vector-sculpt
// uses), so draw-bridge.js is never touched and never even runs during a
// snapped stroke.
//
//   SMLabs.setCurveGuide('ellipse', {cx,cy,rx,ry})   — default: canvas
//   SMLabs.setCurveGuide('line', {x1,y1,x2,y2})         center, big oval
//   held F + drag   — draws a real stroke, sampled every ~6 screen px,
//                      each point snapped to the NEAREST point on the
//                      guide curve — release commits it like any other
//                      stroke (pushUndo/ensureKeyframe/tagOwner)
//
// The committed Path is ordinary (current stroke/fill/width), so it's
// indistinguishable from hand-drawn ink to every consumer.
(function () {
  var GUIDE_KEY = 'nemo-labs-curveguide';
  var armed = false, dragging = false, guidePath = null, pts = [], lastScreen = null;

  function defaultGuide() {
    var cx = (state.canvasW || 1920) / 2, cy = (state.canvasH || 1080) / 2;
    return { type: 'ellipse', cx: cx, cy: cy, rx: (state.canvasW || 1920) * 0.35, ry: (state.canvasH || 1080) * 0.35 };
  }
  function loadGuide() { try { return JSON.parse(localStorage.getItem(GUIDE_KEY)) || defaultGuide(); } catch (e) { return defaultGuide(); } }
  window.SMLabs.setCurveGuide = function (type, opts) {
    opts = opts || {};
    var g = type === 'line'
      ? { type: 'line', x1: +opts.x1 || 0, y1: +opts.y1 || 0, x2: opts.x2 !== undefined ? +opts.x2 : (state.canvasW || 1920), y2: opts.y2 !== undefined ? +opts.y2 : (state.canvasH || 1080) }
      : { type: 'ellipse', cx: opts.cx !== undefined ? +opts.cx : (state.canvasW || 1920) / 2, cy: opts.cy !== undefined ? +opts.cy : (state.canvasH || 1080) / 2, rx: +opts.rx || (state.canvasW || 1920) * 0.35, ry: +opts.ry || (state.canvasH || 1080) * 0.35 };
    localStorage.setItem(GUIDE_KEY, JSON.stringify(g));
    if (typeof showToast === 'function') showToast('Labs — gabarit ' + g.type + ' positionné');
    return g;
  };
  window.SMLabs.getCurveGuide = loadGuide;

  function buildGuidePath() {
    var g = loadGuide();
    var p = new Path({ insert: false });
    if (g.type === 'line') { p.moveTo(new Point(g.x1, g.y1)); p.lineTo(new Point(g.x2, g.y2)); }
    else { var e = new Path.Ellipse({ center: [g.cx, g.cy], radius: [g.rx, g.ry], insert: false }); p.addSegments(e.segments); p.closed = true; e.remove(); }
    return p;
  }
  function snap(worldPt) {
    if (!guidePath) guidePath = buildGuidePath();
    var loc = guidePath.getNearestLocation(worldPt);
    return loc ? loc.point : worldPt;
  }

  function commit() {
    if (pts.length >= 2) {
      pushUndo();
      ensureKeyframe();
      var layer = userLayers[state.activeLayerIdx];
      layer.activate();
      var path = new Path();
      path.strokeColor = state.strokeEnabled ? state.strokeColor : null;
      path.strokeWidth = state.brushSize;
      path.strokeCap = state.strokeCap; path.strokeJoin = state.strokeJoin;
      path.fillColor = state.fillEnabled ? state.fillColor : null;
      path.opacity = state.opacity / 100;
      pts.forEach(function (p) { path.add(p); });
      path.simplify(state.smoothing || 1);
      if (typeof tagOwner === 'function') tagOwner(path);
      saveActiveLayerFrame(); updateUI();
      if (window.SMEngineBridge) SMEngineBridge.renderNow();
    }
    pts = []; lastScreen = null;
  }

  function onDown(e) {
    if (!armed || !window.SMEngineBridge) return;
    var ld = state.layers[state.activeLayerIdx];
    if (!ld || ld.locked) return;
    e.stopImmediatePropagation(); e.preventDefault();
    dragging = true; pts = []; guidePath = buildGuidePath();
    var w = SMEngineBridge.screenToWorld(e.clientX, e.clientY);
    pts.push(snap(new Point(w[0], w[1])));
    lastScreen = [e.clientX, e.clientY];
  }
  function onMove(e) {
    if (!armed || !dragging) return;
    e.stopImmediatePropagation(); e.preventDefault();
    if (lastScreen) {
      var dx = e.clientX - lastScreen[0], dy = e.clientY - lastScreen[1];
      if (dx * dx + dy * dy < 36) return; // ~6 screen px sampling floor
    }
    lastScreen = [e.clientX, e.clientY];
    var w = SMEngineBridge.screenToWorld(e.clientX, e.clientY);
    pts.push(snap(new Point(w[0], w[1])));
    // Live preview via the overlay path, same mechanism the shape bridges use.
    if (window.SMEngineBridge.renderWithOverlayItem) {
      var prev = new Path({ insert: false });
      pts.forEach(function (p) { prev.add(p); });
      prev.strokeColor = state.strokeColor || '#000000'; prev.strokeWidth = Math.max(1, state.brushSize);
      prev.data = { previewOnly: true };
      window.SMEngineBridge.renderWithOverlayItem(prev);
    }
  }
  function onUp(e) {
    if (!armed || !dragging) return;
    e.stopImmediatePropagation(); e.preventDefault();
    dragging = false;
    if (guidePath) { guidePath.remove(); guidePath = null; }
    commit();
  }

  document.addEventListener('keydown', function (e) {
    if (!window.SMLabs.isOn('french-curve')) return;
    var tag = (e.target && e.target.tagName) || '';
    if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT' || (e.target && e.target.isContentEditable)) return;
    if (e.key !== 'f' && e.key !== 'F') return;
    if (e.metaKey || e.ctrlKey || e.altKey) return;
    e.preventDefault();
    armed = true;
  }, true);
  document.addEventListener('keyup', function (e) { if (e.key === 'f' || e.key === 'F') { armed = false; if (dragging) onUp(e); } }, true);
  window.addEventListener('blur', function () { armed = false; dragging = false; pts = []; });

  document.addEventListener('pointerdown', onDown, true);
  document.addEventListener('pointermove', onMove, true);
  document.addEventListener('pointerup', onUp, true);
  document.addEventListener('pointercancel', onUp, true);

  window.SMLabs.register('french-curve', {
    flag: 'nemo-labs-frenchcurve',
    describe: 'Gabarit courbe/ellipse aimanté (SketchBook) : maintenir F + glisser dessine un trait collé au gabarit (SMLabs.setCurveGuide(\'ellipse\'|\'line\', opts))',
    onDisable: function () { armed = false; dragging = false; pts = []; if (guidePath) { guidePath.remove(); guidePath = null; } },
  });
})();
