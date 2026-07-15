// ---- LABS PROTOTYPE — Canvas grid overlay (Umoupen grids / SketchBook) ----
// A world-space grid drawn on a separate transparent <canvas> floated over
// the drawing canvas (pointer-events:none — the drawing pipeline never
// sees it, nothing in CLAUDE.md §1's consumer list is involved). Follows
// pan/zoom/rotation live by deriving the world→screen affine from three
// SMEngineBridge.screenToWorld probes each frame; redrawn only when that
// transform actually changes (matrix-key comparison), so the rAF loop is
// ~free while the viewport is idle.
//
//   SMLabs.enable('canvas-grid')      — toggle
//   SMLabs.setGridStep(px)            — world-space cell size (default 100)
(function () {
  var STEP_KEY = 'nemo-labs-grid-step';
  var overlay = null, rafId = null, lastKey = '';

  function gridStep() {
    var n = parseFloat(localStorage.getItem(STEP_KEY) || '100');
    return (isNaN(n) || n < 4) ? 100 : n;
  }
  window.SMLabs.setGridStep = function (n) {
    localStorage.setItem(STEP_KEY, String(Math.max(4, +n || 100)));
    lastKey = ''; // force redraw
    if (typeof showToast === 'function') showToast('Labs — grille : pas de ' + gridStep() + 'px monde');
    return gridStep();
  };

  function ensureOverlay(cv) {
    if (overlay && overlay.parentNode) return overlay;
    overlay = document.createElement('canvas');
    overlay.id = 'labs-grid-overlay';
    overlay.style.cssText = 'position:absolute;pointer-events:none;z-index:5;';
    cv.parentNode.appendChild(overlay);
    return overlay;
  }

  // world = A·screen + b, probed off the live engine viewport. Inverting
  // gives screen = A⁻¹·(world - b); ctx.setTransform takes that directly.
  function worldToScreenMatrix() {
    var o = SMEngineBridge.screenToWorld(0, 0);
    var x = SMEngineBridge.screenToWorld(1, 0);
    var y = SMEngineBridge.screenToWorld(0, 1);
    var a = x[0] - o[0], b = x[1] - o[1], c = y[0] - o[0], d = y[1] - o[1];
    var det = a * d - b * c;
    if (Math.abs(det) < 1e-12) return null;
    var ia = d / det, ib = -b / det, ic = -c / det, id = a / det;
    return { a: ia, b: ib, c: ic, d: id, e: -(ia * o[0] + ic * o[1]), f: -(ib * o[0] + id * o[1]) };
  }

  function draw() {
    rafId = null;
    if (!window.SMLabs.isOn('canvas-grid')) { if (overlay) overlay.remove(); overlay = null; return; }
    var cv = document.getElementById('drawing-canvas');
    if (!cv || !window.SMEngineBridge || !SMEngineBridge.isEnabled || !SMEngineBridge.isEnabled()) { schedule(); return; }
    var m = worldToScreenMatrix();
    if (!m) { schedule(); return; }
    var r = cv.getBoundingClientRect();
    var step = gridStep();
    var key = [m.a, m.b, m.c, m.d, m.e, m.f, r.width, r.height, step].join(',');
    if (key === lastKey) { schedule(); return; }
    lastKey = key;

    var ov = ensureOverlay(cv);
    // Track the drawing canvas's own box (it can move on panel resizes).
    ov.style.left = cv.offsetLeft + 'px';
    ov.style.top = cv.offsetTop + 'px';
    ov.style.width = r.width + 'px';
    ov.style.height = r.height + 'px';
    var dpr = window.devicePixelRatio || 1;
    if (ov.width !== Math.round(r.width * dpr)) ov.width = Math.round(r.width * dpr);
    if (ov.height !== Math.round(r.height * dpr)) ov.height = Math.round(r.height * dpr);

    var ctx = ov.getContext('2d');
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.clearRect(0, 0, ov.width, ov.height);
    // screenToWorld probes use CLIENT coords relative to the canvas rect —
    // matrix maps world→client-offset-from-rect; overlay sits exactly on
    // the rect, so no extra offset term, just the dpr scale.
    ctx.setTransform(m.a * dpr, m.b * dpr, m.c * dpr, m.d * dpr, m.e * dpr, m.f * dpr);

    // Visible world bbox = transformed screen corners.
    var cs = [SMEngineBridge.screenToWorld(r.left, r.top), SMEngineBridge.screenToWorld(r.right, r.top),
              SMEngineBridge.screenToWorld(r.left, r.bottom), SMEngineBridge.screenToWorld(r.right, r.bottom)];
    var minX = Infinity, maxX = -Infinity, minY = Infinity, maxY = -Infinity;
    cs.forEach(function (p) { minX = Math.min(minX, p[0]); maxX = Math.max(maxX, p[0]); minY = Math.min(minY, p[1]); maxY = Math.max(maxY, p[1]); });

    // Guard against pathological zoom-out flooding: cap line count.
    var nLines = (maxX - minX) / step + (maxY - minY) / step;
    if (nLines < 4000) {
      // 1 device px expressed in world units (the ctx transform maps world
      // to device px, scale·dpr per world px).
      var scale = Math.sqrt(Math.abs(m.a * m.d - m.b * m.c)); // screen px per world px
      ctx.lineWidth = scale > 1e-9 ? 1 / (scale * dpr) : 1;
      ctx.strokeStyle = 'rgba(120,140,255,0.18)';
      ctx.beginPath();
      for (var gx = Math.floor(minX / step) * step; gx <= maxX; gx += step) { ctx.moveTo(gx, minY); ctx.lineTo(gx, maxY); }
      for (var gy = Math.floor(minY / step) * step; gy <= maxY; gy += step) { ctx.moveTo(minX, gy); ctx.lineTo(maxX, gy); }
      ctx.stroke();
      // Canvas border (world 0,0 to canvasW,canvasH) slightly stronger.
      ctx.strokeStyle = 'rgba(120,140,255,0.35)';
      ctx.strokeRect(0, 0, state.canvasW || 1920, state.canvasH || 1080);
    }
    schedule();
  }
  function schedule() { if (!rafId) rafId = requestAnimationFrame(draw); }

  window.SMLabs.register('canvas-grid', {
    flag: 'nemo-labs-grid',
    describe: 'Grille monde superposée au canvas (pas réglable via SMLabs.setGridStep, défaut 100px), suit pan/zoom/rotation',
    onEnable: function () { lastKey = ''; schedule(); },
    onDisable: function () { if (overlay) { overlay.remove(); overlay = null; } },
  });
  // Resume after reload if the flag was already on.
  if (window.SMLabs.isOn('canvas-grid')) schedule();
})();
