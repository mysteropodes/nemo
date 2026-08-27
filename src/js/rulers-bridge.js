// ---- Rulers + draggable guides (2026-08-27, "mettre en place les repères
// et rulers comme dans tout bon soft") ----
// Two thin <canvas> ruler bars overlaid on the canvas-area edges (never
// touches the Rust engine's own viewport/resize — see index.html's own
// comment on why overlay-not-shrink was chosen), plus draggable green
// guide lines (state.guides.h/.v, WORLD-space Y/X values so they stay put
// across pan/zoom — only their ON-SCREEN position is recomputed).
//
// Redraw strategy: a light rAF loop compares the last-seen
// zoom/center/canvas-size against the current one and only repaints when
// something actually moved — cheaper and far less invasive than threading
// an explicit render call into every pan/zoom/rotate call site across
// viewtools-bridge.js/timeline-zoom.js/tools.js's own wheel handler/
// resetView/fitCanvas (CLAUDE.md §5's own "don't do free work" principle:
// a few number comparisons at 60fps is negligible, and it's skipped
// entirely whenever state.rulersOn is off).
(function () {
  var RULER_SIZE = 18; // must match style.css's own copy of this constant
  var TICK_COLOR = 'rgba(236,234,231,.55)';
  var TEXT_COLOR = 'rgba(236,234,231,.65)';
  var hCanvas, vCanvas, area;
  var lastZoom = null, lastCenterX = null, lastCenterY = null, lastW = null, lastH = null;
  var guideEls = { h: [], v: [] }; // parallel to state.guides.h/.v, index-aligned

  function ensureGuides() {
    if (!state.guides) state.guides = { h: [], v: [] };
    if (!state.guides.h) state.guides.h = [];
    if (!state.guides.v) state.guides.v = [];
  }

  // ---- Ruler tick rendering ----
  // Picks a "nice" step (1/2/5 × a power of 10) so labeled ticks land on
  // round world-unit numbers regardless of zoom, same convention every
  // ruler in every design app uses.
  function niceStep(worldPerPx, minPxBetweenTicks) {
    var rough = worldPerPx * minPxBetweenTicks;
    var mag = Math.pow(10, Math.floor(Math.log10(rough)));
    var norm = rough / mag;
    var step = norm <= 1 ? 1 : norm <= 2 ? 2 : norm <= 5 ? 5 : 10;
    return step * mag;
  }

  function resizeCanvases() {
    if (!area) return;
    var w = area.clientWidth, h = area.clientHeight;
    var dpr = window.devicePixelRatio || 1;
    var hw = Math.max(0, w - RULER_SIZE), vh = Math.max(0, h - RULER_SIZE);
    if (hCanvas.width !== Math.round(hw * dpr)) { hCanvas.width = Math.round(hw * dpr); hCanvas.style.width = hw + 'px'; }
    if (hCanvas.height !== Math.round(RULER_SIZE * dpr)) { hCanvas.height = Math.round(RULER_SIZE * dpr); hCanvas.style.height = RULER_SIZE + 'px'; }
    if (vCanvas.height !== Math.round(vh * dpr)) { vCanvas.height = Math.round(vh * dpr); vCanvas.style.height = vh + 'px'; }
    if (vCanvas.width !== Math.round(RULER_SIZE * dpr)) { vCanvas.width = Math.round(RULER_SIZE * dpr); vCanvas.style.width = RULER_SIZE + 'px'; }
  }

  // World->view-local pixel, matching openCommentPopover's own use of
  // view.projectToView (timeline.js) — canvas-area-relative, exactly the
  // coordinate space these overlay elements live in.
  function w2v(wx, wy) { return view.projectToView(new Point(wx, wy)); }

  function drawRulers() {
    if (!state.rulersOn || !hCanvas || !vCanvas) return;
    var dpr = window.devicePixelRatio || 1;
    var zoom = view.zoom;
    var hctx = hCanvas.getContext('2d'), vctx = vCanvas.getContext('2d');
    hctx.setTransform(dpr, 0, 0, dpr, 0, 0); vctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    var hw = hCanvas.width / dpr, vh = vCanvas.height / dpr;
    hctx.clearRect(0, 0, hw, RULER_SIZE); vctx.clearRect(0, 0, RULER_SIZE, vh);
    hctx.fillStyle = getComputedStyle(document.documentElement).getPropertyValue('--panel2') || '#1a1a1e';
    hctx.fillRect(0, 0, hw, RULER_SIZE);
    vctx.fillStyle = hctx.fillStyle;
    vctx.fillRect(0, 0, RULER_SIZE, vh);
    var worldPerPx = 1 / zoom;
    var step = niceStep(worldPerPx, 55); // ~55px between labeled ticks, comfortable at any zoom
    hctx.font = '9px sans-serif'; vctx.font = '9px sans-serif';
    hctx.textBaseline = 'middle'; vctx.textBaseline = 'middle';
    // Horizontal ruler: iterate world X across the visible width
    var originH = w2v(0, 0);
    var wx0 = ((0 - originH.x) / zoom) - (((0 - originH.x) / zoom) % step) - step * 2;
    for (var wx = wx0; ; wx += step) {
      var sx = w2v(wx, 0).x;
      if (sx > hw + 40) break;
      if (sx < -40) continue;
      hctx.strokeStyle = TICK_COLOR;
      hctx.beginPath(); hctx.moveTo(sx + .5, RULER_SIZE); hctx.lineTo(sx + .5, RULER_SIZE - 8); hctx.stroke();
      hctx.fillStyle = TEXT_COLOR;
      hctx.fillText(Math.round(wx), sx + 3, RULER_SIZE - 10);
    }
    // Vertical ruler: iterate world Y across the visible height, label
    // drawn sideways (rotated) — standard ruler convention.
    var originV = w2v(0, 0);
    var wy0 = ((0 - originV.y) / zoom) - (((0 - originV.y) / zoom) % step) - step * 2;
    for (var wy = wy0; ; wy += step) {
      var sy = w2v(0, wy).y;
      if (sy > vh + 40) break;
      if (sy < -40) continue;
      vctx.strokeStyle = TICK_COLOR;
      vctx.beginPath(); vctx.moveTo(RULER_SIZE, sy + .5); vctx.lineTo(RULER_SIZE - 8, sy + .5); vctx.stroke();
      vctx.save();
      vctx.translate(RULER_SIZE - 10, sy);
      vctx.rotate(-Math.PI / 2);
      vctx.fillStyle = TEXT_COLOR;
      vctx.fillText(Math.round(wy), 0, 0);
      vctx.restore();
    }
  }

  // ---- Guide DOM elements (visible line + wider invisible hit strip) ----
  function guideContainer() {
    var c = document.getElementById('guide-layer');
    if (!c) { c = document.createElement('div'); c.id = 'guide-layer'; c.style.cssText = 'position:absolute;inset:0;pointer-events:none;'; area.appendChild(c); }
    return c;
  }
  function ensureGuideEl(axis, i) {
    var arr = guideEls[axis];
    if (arr[i]) return arr[i];
    var line = document.createElement('div'); line.className = 'guide-line ' + axis;
    var hit = document.createElement('div'); hit.className = 'guide-hit ' + axis; hit.style.pointerEvents = 'auto';
    guideContainer().appendChild(line); guideContainer().appendChild(hit);
    wireGuideDrag(hit, axis, i);
    var rec = { line: line, hit: hit };
    arr[i] = rec;
    return rec;
  }
  function pruneGuideEls(axis, count) {
    var arr = guideEls[axis];
    while (arr.length > count) { var r = arr.pop(); if (r) { r.line.remove(); r.hit.remove(); } }
  }
  function renderGuides() {
    ensureGuides();
    if (!state.rulersOn) { pruneGuideEls('h', 0); pruneGuideEls('v', 0); return; }
    state.guides.h.forEach(function (wy, i) {
      var r = ensureGuideEl('h', i);
      var sy = w2v(0, wy).y;
      r.line.style.top = sy + 'px'; r.hit.style.top = (sy - 3) + 'px';
      r.line.style.display = r.hit.style.display = (sy >= 0 && sy <= area.clientHeight) ? '' : 'none';
    });
    pruneGuideEls('h', state.guides.h.length);
    state.guides.v.forEach(function (wx, i) {
      var r = ensureGuideEl('v', i);
      var sx = w2v(wx, 0).x;
      r.line.style.left = sx + 'px'; r.hit.style.left = (sx - 3) + 'px';
      r.line.style.display = r.hit.style.display = (sx >= 0 && sx <= area.clientWidth) ? '' : 'none';
    });
    pruneGuideEls('v', state.guides.v.length);
  }

  // Dragging an EXISTING guide: reposition live; release back over its
  // own ruler (< RULER_SIZE screen px into the bar) deletes it — same
  // "drag back onto the ruler to remove" convention as every reference
  // app's guides.
  function wireGuideDrag(hitEl, axis, indexRef) {
    // Same document-level capture-phase fix as wireRulerDrag above — the
    // hit strip is also a descendant of #canvas-area.
    document.addEventListener('pointerdown', function (e) {
      if (e.target !== hitEl) return;
      if (state.guidesLocked) return;
      e.preventDefault(); e.stopPropagation(); e.stopImmediatePropagation();
      var pointerId = e.pointerId;
      var i = guideEls[axis].indexOf(guideEls[axis].filter(function (r) { return r && r.hit === hitEl; })[0]);
      function onMove(ev) {
        if (ev.pointerId !== pointerId) return;
        ev.stopImmediatePropagation();
        var rect = area.getBoundingClientRect();
        if (axis === 'h') {
          var localY = ev.clientY - rect.top;
          var wpt = view.viewToProject(new Point(0, localY));
          state.guides.h[i] = wpt.y;
        } else {
          var localX = ev.clientX - rect.left;
          var wpt2 = view.viewToProject(new Point(localX, 0));
          state.guides.v[i] = wpt2.x;
        }
        renderGuides();
      }
      function onUp(ev) {
        if (ev.pointerId !== pointerId) return;
        ev.stopImmediatePropagation();
        document.removeEventListener('pointermove', onMove, true);
        document.removeEventListener('pointerup', onUp, true);
        var rect = area.getBoundingClientRect();
        var backOnRuler = axis === 'h' ? (ev.clientY - rect.top) < RULER_SIZE : (ev.clientX - rect.left) < RULER_SIZE;
        if (backOnRuler) { state.guides[axis].splice(i, 1); renderGuides(); }
      }
      document.addEventListener('pointermove', onMove, true);
      document.addEventListener('pointerup', onUp, true);
    }, true);
  }

  // Dragging a NEW guide out from a ruler bar.
  // CAPTURE-phase on `document`, gated by e.target — every drawing-tool
  // bridge (draw-bridge.js/select-bridge.js/eraser-bridge.js/etc.) attaches
  // its OWN pointerdown at capture phase on #canvas-area and calls
  // e.stopImmediatePropagation() unconditionally whenever its tool is
  // active (e.g. the default Draw tool). Since the ruler/guide-hit
  // elements are DOM descendants of #canvas-area, a plain (bubble-phase)
  // listener attached directly to them never even fires — the ancestor's
  // capture-phase listener already halted the whole dispatch on its way
  // down. Reproduced live: a synthetic pointerdown dispatched straight on
  // #ruler-h came back with e.defaultPrevented===true from DRAW-bridge's
  // own handler, while this file's own listener never ran at all (traced
  // via a temporary counter). Registering on `document` at capture phase
  // runs BEFORE any listener on the descendant #canvas-area, same fix
  // tools.js's own fsselect marquee already uses for this identical class
  // of interception conflict.
  function wireRulerDrag(rulerEl, axis) {
    document.addEventListener('pointerdown', function (e) {
      if (e.target !== rulerEl) return;
      if (state.guidesLocked || !state.rulersOn) return;
      e.preventDefault(); e.stopPropagation(); e.stopImmediatePropagation();
      var pointerId = e.pointerId;
      var ghost = document.createElement('div');
      ghost.className = 'guide-line ' + axis; ghost.style.opacity = '.6';
      guideContainer().appendChild(ghost);
      function place(ev) {
        var rect = area.getBoundingClientRect();
        if (axis === 'h') ghost.style.top = Math.max(RULER_SIZE, ev.clientY - rect.top) + 'px';
        else ghost.style.left = Math.max(RULER_SIZE, ev.clientX - rect.left) + 'px';
      }
      place(e);
      function onMove(ev) { if (ev.pointerId === pointerId) { ev.stopImmediatePropagation(); place(ev); } }
      function onUp(ev) {
        if (ev.pointerId !== pointerId) return;
        ev.stopImmediatePropagation();
        document.removeEventListener('pointermove', onMove, true);
        document.removeEventListener('pointerup', onUp, true);
        ghost.remove();
        var rect = area.getBoundingClientRect();
        // Released back on the ruler (didn't actually drag out) — no-op,
        // same convention as every reference app: a click-without-drag on
        // a ruler doesn't create a stray guide at the bar's own edge.
        if (axis === 'h' && (ev.clientY - rect.top) < RULER_SIZE) return;
        if (axis === 'v' && (ev.clientX - rect.left) < RULER_SIZE) return;
        var local = axis === 'h' ? new Point(0, ev.clientY - rect.top) : new Point(ev.clientX - rect.left, 0);
        var wpt = view.viewToProject(local);
        ensureGuides();
        if (axis === 'h') state.guides.h.push(wpt.y); else state.guides.v.push(wpt.x);
        renderGuides();
      }
      document.addEventListener('pointermove', onMove, true);
      document.addEventListener('pointerup', onUp, true);
    }, true);
  }

  // ---- Snapping (opt-in helper for other tools) ----
  // Exposed for select-bridge.js's move drag — snaps a WORLD point to the
  // nearest guide within `tolPx` SCREEN pixels (so the snap distance feels
  // consistent regardless of zoom), returns {x,y,snappedX,snappedY} or the
  // original point unchanged if nothing's close enough / guides are off.
  function snapPoint(worldPt, tolPx) {
    if (!state.rulersOn || !state.guidesSnap) return worldPt;
    ensureGuides();
    tolPx = tolPx || 8;
    var sp = w2v(worldPt.x, worldPt.y);
    var bestVx = null, bestVdx = tolPx + 1;
    state.guides.v.forEach(function (wx) {
      var sx = w2v(wx, 0).x, d = Math.abs(sx - sp.x);
      if (d < bestVdx) { bestVdx = d; bestVx = wx; }
    });
    var bestHy = null, bestHdy = tolPx + 1;
    state.guides.h.forEach(function (wy) {
      var sy = w2v(0, wy).y, d = Math.abs(sy - sp.y);
      if (d < bestHdy) { bestHdy = d; bestHy = wy; }
    });
    return new Point(bestVx != null ? bestVx : worldPt.x, bestHy != null ? bestHy : worldPt.y);
  }

  function loop() {
    requestAnimationFrame(loop);
    if (!state.rulersOn || !area || document.hidden) return;
    var z = view.zoom, c = view.center, w = area.clientWidth, h = area.clientHeight;
    if (z === lastZoom && c.x === lastCenterX && c.y === lastCenterY && w === lastW && h === lastH) return;
    lastZoom = z; lastCenterX = c.x; lastCenterY = c.y; lastW = w; lastH = h;
    resizeCanvases();
    drawRulers();
    renderGuides();
  }

  function toggleOn(on) {
    state.rulersOn = on != null ? !!on : !state.rulersOn;
    document.body.classList.toggle('rulers-off', !state.rulersOn);
    if (state.rulersOn) { lastZoom = null; } // force a redraw on the next loop tick
    else { renderGuides(); } // collapses the DOM guide elements
  }

  function init() {
    area = document.getElementById('canvas-area');
    hCanvas = document.getElementById('ruler-h');
    vCanvas = document.getElementById('ruler-v');
    if (!area || !hCanvas || !vCanvas) return;
    document.body.classList.toggle('rulers-off', !state.rulersOn);
    wireRulerDrag(hCanvas, 'h');
    wireRulerDrag(vCanvas, 'v');
    requestAnimationFrame(loop);
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', init);
  else init();

  window.SMRulers = { toggleOn: toggleOn, snapPoint: snapPoint, render: function () { lastZoom = null; } };
})();
