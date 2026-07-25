// ---- GRADIENT FILL (2026-07, Illustrator/Rive-style on-canvas control) ----
// Applies an N-stop linear or radial gradient to the fill of ONE selected
// shape at a time (Select/Subselect tool) — `p.data.fillGradient =
// {kind,from,to,stops:[{offset,color},...]}` (world coordinates,
// serP/desP-persisted, app.js; consumed by engine-bridge.js/geometry-wasm
// for the live GPU render and export.js's own Paper.js Gradient for
// export/preview). A plain data field, not a live Paper.js Gradient object,
// so any consumer that doesn't yet understand gradients (boolean ops, the
// eyedropper, thumbnails) still sees a flat `p.fillColor` fallback (stop 0's
// color) instead of breaking.
//
// Position (both endpoints AND each stop's offset along the line) is edited
// by DRAGGING an on-canvas gizmo — same overlay + own capture-phase
// pointerdown/move/up pattern as perspective-bridge.js's vanishing points and
// symmetry-bridge.js's axis, not a numeric angle field. The Effects panel
// only edits stop COLORS/count and Linear-vs-Radial, which don't have an
// obvious canvas gesture of their own.
(function () {
  function engineOn() { return window.SMEngineBridge && window.SMEngineBridge.isEnabled() && !state.playing; }
  function singleTarget() {
    if ((state.tool !== 'select' && state.tool !== 'subselect') || !window.selectedPaths || selectedPaths.length !== 1) return null;
    var p = selectedPaths[0];
    return (p instanceof Path || p instanceof CompoundPath) ? p : null;
  }
  function sortedStops(grad) {
    return grad.stops.slice().sort(function (a, b) { return a.offset - b.offset; });
  }
  function defaultGradientFor(p) {
    var b = p.bounds;
    var cx = b.center.x, cy = b.center.y;
    var halfDiag = Math.hypot(b.width, b.height) / 2 || 100;
    var c1 = p.fillColor ? colorHex8(p.fillColor) : '#ff0000';
    return {
      kind: 'linear',
      from: [cx - halfDiag, cy],
      to: [cx + halfDiag, cy],
      stops: [{ offset: 0, color: c1 }, { offset: 1, color: '#0000ff' }],
    };
  }

  // ---- Effects panel: stop list (color/offset/remove), kind, add-stop —
  // NOT position, which is drag-only (see file header). Rebuilt from
  // scratch on every call (small N, this app's own convention elsewhere —
  // e.g. renderElementsList in motion.js does the same) rather than
  // diffed DOM patching.
  function renderGradientPanel() {
    var onCb = document.getElementById('p-grad-on');
    if (!onCb) return;
    var kindRow = document.getElementById('p-grad-kind-row');
    var stopsList = document.getElementById('p-grad-stops-list');
    var addRow = document.getElementById('p-grad-addstop-row');
    var hintRow = document.getElementById('p-grad-hint-row');
    var target = singleTarget();
    if (!target) {
      onCb.checked = false; onCb.disabled = true;
      kindRow.style.display = stopsList.style.display = addRow.style.display = hintRow.style.display = 'none';
      if (window.syncFillGradientButton) syncFillGradientButton();
      return;
    }
    onCb.disabled = false;
    var grad = target.data.fillGradient;
    onCb.checked = !!grad;
    var show = !!grad ? 'flex' : 'none';
    kindRow.style.display = show; addRow.style.display = show; hintRow.style.display = grad ? 'block' : 'none';
    stopsList.style.display = grad ? 'block' : 'none';
    stopsList.innerHTML = '';
    // Le bouton dégradé de la ligne Fill (timeline.js) reflète cet état —
    // resynchronisé ici parce que c'est le seul endroit qui sait ce que porte
    // réellement la sélection courante.
    if (window.syncFillGradientButton) syncFillGradientButton();
    if (!grad) return;
    document.getElementById('p-grad-kind').value = grad.kind;
    var stops = sortedStops(grad);
    stops.forEach(function (stop) {
      var row = document.createElement('div');
      row.className = 'pr'; row.style.gap = '4px';
      var colorInp = document.createElement('input');
      colorInp.type = 'color'; colorInp.value = stop.color.slice(0, 7);
      colorInp.style.cssText = 'width:24px;height:22px;border:none;cursor:pointer;padding:0;';
      colorInp.addEventListener('change', function () {
        pushUndo(); stop.color = this.value;
        saveActiveLayerFrame(); if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
      });
      var offInp = document.createElement('input');
      offInp.type = 'number'; offInp.className = 'pi scrub'; offInp.min = 0; offInp.max = 100; offInp.step = 1; offInp.dataset.step = '1';
      offInp.style.width = '48px';
      offInp.value = Math.round(stop.offset * 100);
      offInp.addEventListener('change', function () {
        pushUndo(); stop.offset = Math.max(0, Math.min(100, parseFloat(this.value) || 0)) / 100;
        saveActiveLayerFrame(); renderGradientPanel(); if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
      });
      var pct = document.createElement('span'); pct.style.cssText = 'font-size:9px;color:var(--text-dim)'; pct.textContent = '%';
      var rmBtn = document.createElement('button');
      rmBtn.className = 'pbtn'; rmBtn.textContent = '×'; rmBtn.style.cssText = 'padding:0 6px;';
      rmBtn.disabled = grad.stops.length <= 2;
      rmBtn.addEventListener('click', function () {
        if (grad.stops.length <= 2) return;
        pushUndo();
        var idx = grad.stops.indexOf(stop);
        if (idx >= 0) grad.stops.splice(idx, 1);
        saveActiveLayerFrame(); renderGradientPanel(); if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
      });
      row.appendChild(colorInp); row.appendChild(offInp); row.appendChild(pct); row.appendChild(rmBtn);
      stopsList.appendChild(row);
    });
  }
  window.renderGradientPanel = renderGradientPanel;

  function toggleGradient(on) {
    var target = singleTarget();
    if (!target) return;
    pushUndo();
    if (on) {
      target.data.fillGradient = defaultGradientFor(target);
      target.fillColor = target.data.fillGradient.stops[0].color;
    } else {
      delete target.data.fillGradient;
    }
    saveActiveLayerFrame(); updateUI(); renderGradientPanel();
    if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
  }
  function addStop() {
    var target = singleTarget();
    if (!target || !target.data.fillGradient) return;
    pushUndo();
    var grad = target.data.fillGradient;
    var stops = sortedStops(grad);
    // Insert at the midpoint of the two stops with the largest gap — a
    // more useful default than always 0.5 once there are already 3+ stops.
    var bestGapIdx = 0, bestGap = -1;
    for (var i = 0; i < stops.length - 1; i++) {
      var gap = stops[i + 1].offset - stops[i].offset;
      if (gap > bestGap) { bestGap = gap; bestGapIdx = i; }
    }
    var a = stops[bestGapIdx], b = stops[bestGapIdx + 1] || stops[bestGapIdx];
    var mid = (a.offset + b.offset) / 2;
    grad.stops.push({ offset: mid, color: a.color });
    saveActiveLayerFrame(); renderGradientPanel();
    if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
  }

  // ---- On-canvas gizmo: line + endpoint handles (square=from, circle=to,
  // Illustrator's own start/end handle convention) + a small diamond per
  // stop positioned along the line at t=offset (colored to preview that
  // stop's own color at a glance). Same "extra layer pushed into
  // buildSceneJson()'s scene, never saved to frame data" convention as
  // every other overlay in this app.
  function stopWorldPos(grad, t) {
    var fx = grad.from[0], fy = grad.from[1], tx = grad.to[0], ty = grad.to[1];
    return { x: fx + (tx - fx) * t, y: fy + (ty - fy) * t };
  }
  function buildGradientGizmoItems() {
    var target = singleTarget();
    if (!target || !target.data.fillGradient) return [];
    var grad = target.data.fillGradient;
    var items = [];
    var col = [90, 180, 255, 255];
    items.push({
      segments: [{ point: grad.from }, { point: grad.to }],
      closed: false, fillColor: null, strokeColor: col, strokeWidth: 1.5, strokeCap: 'butt',
    });
    // "from" handle — small square.
    var s = 6, fx = grad.from[0], fy = grad.from[1];
    items.push({
      segments: [{ point: [fx - s, fy - s] }, { point: [fx + s, fy - s] }, { point: [fx + s, fy + s] }, { point: [fx - s, fy + s] }],
      closed: true, fillColor: [255, 255, 255, 255], strokeColor: col, strokeWidth: 1.5,
    });
    // "to" handle — small circle (approximated with a 12-gon, consistent
    // with this file's other overlay builders' style).
    var tx = grad.to[0], ty = grad.to[1], r = 6, segs = [];
    for (var i = 0; i < 12; i++) { var a = (i / 12) * Math.PI * 2; segs.push({ point: [tx + Math.cos(a) * r, ty + Math.sin(a) * r] }); }
    items.push({ segments: segs, closed: true, fillColor: [255, 255, 255, 255], strokeColor: col, strokeWidth: 1.5 });
    // Stop markers — diamonds, filled with the stop's OWN color so the
    // gizmo doubles as a live preview of where each color sits.
    grad.stops.forEach(function (stop) {
      var p = stopWorldPos(grad, stop.offset);
      var ds = 5;
      var h = stop.color.length === 9 ? [parseInt(stop.color.substr(1, 2), 16), parseInt(stop.color.substr(3, 2), 16), parseInt(stop.color.substr(5, 2), 16), parseInt(stop.color.substr(7, 2), 16)]
        : [parseInt(stop.color.substr(1, 2), 16), parseInt(stop.color.substr(3, 2), 16), parseInt(stop.color.substr(5, 2), 16), 255];
      items.push({
        segments: [{ point: [p.x, p.y - ds] }, { point: [p.x + ds, p.y] }, { point: [p.x, p.y + ds] }, { point: [p.x - ds, p.y] }],
        closed: true, fillColor: h, strokeColor: [30, 30, 30, 255], strokeWidth: 1.2,
      });
    });
    return items;
  }
  window.buildGradientGizmoItems = buildGradientGizmoItems;

  // ---- drag interaction ----
  var dragging = null; // 'from' | 'to' | {stopIndex} | null
  function shouldEdit() { return engineOn() && !!singleTarget() && !!singleTarget().data.fillGradient; }
  function hitPoint(worldPt, wx, wy, tolPx) {
    var tol = tolPx / view.zoom;
    return Math.hypot(worldPt.x - wx, worldPt.y - wy) < tol;
  }
  function onDown(e) {
    if (!shouldEdit()) return;
    var target = singleTarget();
    var grad = target.data.fillGradient;
    var w = window.SMEngineBridge.screenToWorld(e.clientX, e.clientY);
    var wp = new Point(w[0], w[1]);
    if (hitPoint(wp, grad.from[0], grad.from[1], 12)) { dragging = 'from'; }
    else if (hitPoint(wp, grad.to[0], grad.to[1], 12)) { dragging = 'to'; }
    else {
      for (var i = 0; i < grad.stops.length; i++) {
        var sp = stopWorldPos(grad, grad.stops[i].offset);
        if (hitPoint(wp, sp.x, sp.y, 9)) { dragging = { stopIndex: i }; break; }
      }
    }
    if (!dragging) return;
    e.stopImmediatePropagation(); e.preventDefault();
    pushUndo();
    window.SMEngineBridge.suspend();
  }
  function onMove(e) {
    if (!dragging || !shouldEdit()) return;
    e.stopImmediatePropagation(); e.preventDefault();
    var target = singleTarget();
    var grad = target.data.fillGradient;
    var w = window.SMEngineBridge.screenToWorld(e.clientX, e.clientY);
    if (dragging === 'from') grad.from = [w[0], w[1]];
    else if (dragging === 'to') grad.to = [w[0], w[1]];
    else {
      // Project the drag point onto the from→to line, clamped to [0,1] —
      // dragging a stop off either end just pins it at that end, matching
      // Illustrator's own gradient-stop clamping.
      var fx = grad.from[0], fy = grad.from[1], tx = grad.to[0], ty = grad.to[1];
      var dx = tx - fx, dy = ty - fy, len2 = dx * dx + dy * dy || 1;
      var t = ((w[0] - fx) * dx + (w[1] - fy) * dy) / len2;
      grad.stops[dragging.stopIndex].offset = Math.max(0, Math.min(1, t));
    }
    window.SMEngineBridge.renderNow();
  }
  function onUp(e) {
    if (!dragging) return;
    e.stopImmediatePropagation(); e.preventDefault();
    dragging = null;
    window.SMEngineBridge.resume();
    saveActiveLayerFrame(); renderGradientPanel();
    window.SMEngineBridge.renderNow();
  }

  function init() {
    var onCb = document.getElementById('p-grad-on');
    if (onCb) {
      onCb.addEventListener('change', function () { toggleGradient(this.checked); });
      document.getElementById('btn-grad-add-stop').addEventListener('click', addStop);
      document.getElementById('p-grad-kind').addEventListener('change', function () {
        var target = singleTarget();
        if (!target || !target.data.fillGradient) return;
        pushUndo(); target.data.fillGradient.kind = this.value;
        saveActiveLayerFrame(); if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
      });
    }
    var target = document.getElementById('canvas-area') || document.getElementById('drawing-canvas');
    if (target) {
      target.addEventListener('pointerdown', onDown, { capture: true });
      target.addEventListener('pointermove', onMove, { capture: true });
      target.addEventListener('pointerup', onUp, { capture: true });
      target.addEventListener('pointercancel', onUp, { capture: true });
    }
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', init);
  else init();
})();
