// ---- PERSPECTIVE DRAWING GUIDES (Sketchbook-style) ----
// A pure drawing AID, not document content: vanishing points + a radiating
// guide grid, rendered as an overlay (same "extra layer pushed into
// buildSceneJson()'s scene, never saved to frame data" pattern as the
// marquee/transform-box/eraser-cursor overlays already use) plus optional
// snapping so a straight line drawn roughly toward a vanishing point locks
// onto it exactly — same idea as Sketchbook's perspective guide cursor,
// which rotates to point at the VP and snaps a stroke drawn toward it.
//
// The guide persists across tool switches once state.perspectiveEnabled is
// true (matches Sketchbook: you set it up once, then draw with whatever
// tool you like) — only DRAGGING a vanishing point requires the dedicated
// 'perspective' tool to be active, same as every other tool-specific
// interaction in this app.
(function () {
  function engineOn() { return window.SMEngineBridge && window.SMEngineBridge.isEnabled() && !state.playing; }

  // Default VP layout per mode — centered on the canvas, spaced wide enough
  // that the guide lines read as a real perspective fan rather than a tight
  // cluster. Re-seeded whenever the mode changes (switching 1pt<->2pt<->3pt
  // needs a different VP COUNT, so old positions can't just be reused/
  // truncated without looking arbitrary).
  function defaultVPs(mode) {
    var w = state.canvasW, h = state.canvasH;
    var cx = w / 2, cy = h / 2;
    // Fisheye (2026-07, Sketchbook-style barrel-lens grid): a single eye
    // point, same as 1pt — buildPerspectiveGuideItems below adds concentric
    // rings around it instead of just straight rays, and every existing
    // snap function (both angle-snap and magnetic) already works unchanged
    // since they only ever care about "rays from a VP", which a fisheye VP
    // still has.
    if (mode === '1pt' || mode === 'fisheye') return [{ x: cx, y: cy, locked: false }];
    if (mode === '3pt') return [
      { x: cx - w * 0.6, y: cy - h * 0.15, locked: false },
      { x: cx + w * 0.6, y: cy - h * 0.15, locked: false },
      { x: cx, y: cy + h * 1.3, locked: false },
    ];
    return [ // 2pt
      { x: cx - w * 0.65, y: cy, locked: false },
      { x: cx + w * 0.65, y: cy, locked: false },
    ];
  }
  function ensurePerspectiveVPs() {
    if (!state.perspectiveVPs || !state.perspectiveVPs.length) state.perspectiveVPs = defaultVPs(state.perspectiveMode);
    return state.perspectiveVPs;
  }
  window.ensurePerspectiveVPs = ensurePerspectiveVPs;
  window.resetPerspectiveVPs = function () { state.perspectiveVPs = defaultVPs(state.perspectiveMode); if (window.SMEngineBridge) window.SMEngineBridge.renderNow(); };
  window.setPerspectiveMode = function (mode) {
    state.perspectiveMode = mode;
    state.perspectiveVPs = defaultVPs(mode);
    if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
  };

  // Builds the overlay items: a small diamond marker + N radiating lines
  // per vanishing point, plus a horizon line through the first two VPs
  // (the classic "eye-level" reference line, only meaningful with >=2
  // points). Lines are drawn far enough past the VP to cross the whole
  // canvas regardless of zoom/pan — the renderer clips to the visible
  // viewport on its own, same as every other overlay.
  function buildPerspectiveGuideItems() {
    if (!state.perspectiveEnabled) return [];
    var vps = ensurePerspectiveVPs();
    var items = [];
    var reach = Math.max(state.canvasW, state.canvasH) * 4; // long enough to always cross the canvas from any VP position, even one placed off-canvas
    var col = [120, 170, 255, 90];
    var n = Math.max(4, state.perspectiveDensity | 0);
    vps.forEach(function (vp) {
      for (var i = 0; i < n; i++) {
        var a = (i / n) * Math.PI * 2;
        items.push({
          segments: [{ point: [vp.x, vp.y] }, { point: [vp.x + Math.cos(a) * reach, vp.y + Math.sin(a) * reach] }],
          closed: false, fillColor: null, strokeColor: col, strokeWidth: 1, strokeCap: 'butt',
        });
      }
    });
    // Fisheye's extra rings — a handful of concentric circles around the
    // single eye point, approximating the "radial + concentric" read of a
    // barrel-lens guide (not true spherical-projection math, just the same
    // visual language: straight rays + rings bowing around a center).
    if (state.perspectiveMode === 'fisheye' && vps.length) {
      var eye = vps[0];
      var maxR = Math.max(state.canvasW, state.canvasH) * 0.75;
      var ringCount = 5;
      for (var ri = 1; ri <= ringCount; ri++) {
        var r = (ri / ringCount) * maxR;
        var segs = [];
        var ringPts = 32;
        for (var pi = 0; pi <= ringPts; pi++) {
          var pa = (pi / ringPts) * Math.PI * 2;
          segs.push({ point: [eye.x + Math.cos(pa) * r, eye.y + Math.sin(pa) * r] });
        }
        items.push({ segments: segs, closed: true, fillColor: null, strokeColor: col, strokeWidth: 1, strokeCap: 'butt' });
      }
    }
    // Horizon (eye-level line) through the first two VPs, 2pt/3pt only —
    // a single VP has no second point to define a line through.
    if (vps.length >= 2) {
      var dx = vps[1].x - vps[0].x, dy = vps[1].y - vps[0].y;
      var len = Math.hypot(dx, dy) || 1;
      var ux = dx / len, uy = dy / len;
      items.push({
        segments: [{ point: [vps[0].x - ux * reach, vps[0].y - uy * reach] }, { point: [vps[1].x + ux * reach, vps[1].y + uy * reach] }],
        closed: false, fillColor: null, strokeColor: [255, 200, 80, 130], strokeWidth: 1.4, strokeCap: 'butt',
      });
    }
    // VP markers themselves — small diamonds, filled if locked (matches
    // Sketchbook's locked/unlocked visual distinction) so it's obvious at a
    // glance which points can still be dragged.
    vps.forEach(function (vp) {
      var s = 6;
      items.push({
        segments: [{ point: [vp.x, vp.y - s] }, { point: [vp.x + s, vp.y] }, { point: [vp.x, vp.y + s] }, { point: [vp.x - s, vp.y] }],
        closed: true,
        fillColor: vp.locked ? [255, 120, 80, 220] : null,
        strokeColor: [255, 150, 100, 255], strokeWidth: 1.5,
      });
    });
    return items;
  }
  window.buildPerspectiveGuideItems = buildPerspectiveGuideItems;

  // Snaps `end` onto whichever vanishing point's ray from `start` is
  // closest in ANGLE (within SNAP_DEG) — preserves the drawn length (the
  // point is projected onto the ray, not replaced by the VP itself), so a
  // short stroke drawn toward a far-off vanishing point still comes out
  // short, just perfectly straight toward it. Returns `end` unchanged if
  // guides are off, no VP is close enough in angle, or the drag is too
  // short to have a meaningful direction yet.
  var SNAP_DEG = 6;
  function snapToVP(start, end) {
    if (!state.perspectiveEnabled) return end;
    var dx = end.x - start.x, dy = end.y - start.y;
    var dragLen = Math.hypot(dx, dy);
    if (dragLen < 4) return end;
    var dragAngle = Math.atan2(dy, dx);
    var vps = ensurePerspectiveVPs();
    var best = null, bestDiff = SNAP_DEG * Math.PI / 180;
    vps.forEach(function (vp) {
      var vx = vp.x - start.x, vy = vp.y - start.y;
      if (Math.hypot(vx, vy) < 1) return; // start point sits ON the VP — no meaningful direction to snap to
      var vAngle = Math.atan2(vy, vx);
      var diff = Math.abs(Math.atan2(Math.sin(dragAngle - vAngle), Math.cos(dragAngle - vAngle)));
      if (diff < bestDiff) { bestDiff = diff; best = { x: vx, y: vy }; }
    });
    if (!best) return end;
    var uLen = Math.hypot(best.x, best.y);
    var ux = best.x / uLen, uy = best.y / uLen;
    return new Point(start.x + ux * dragLen, start.y + uy * dragLen);
  }
  window.perspectiveSnapPoint = snapToVP;

  // Magnetic per-point snapping used by freehand/pressure-brush strokes
  // (draw-bridge.js) — unlike snapToVP above (which locks a whole straight
  // Line-tool drag onto a single ray from its start point), this projects
  // an ARBITRARY point onto the closest guide line (any VP's radiating ray,
  // or the horizon) if it's within a small screen-pixel tolerance, and
  // returns the point UNCHANGED otherwise. Called per sample as a freehand
  // stroke is drawn, so the brush sticks to a guide line when the hand
  // wanders near one and draws normally everywhere else — same "magnetic"
  // assist Sketchbook's Perspective Guide gives to any brush, not just a
  // ruler tool.
  var MAGNET_PX = 14;
  function nearestGuideProjection(worldPt) {
    if (!state.perspectiveEnabled) return null;
    var vps = ensurePerspectiveVPs();
    if (!vps.length) return null;
    var tol = MAGNET_PX / view.zoom;
    var best = null, bestDist = tol;
    function tryLine(px, py, dx, dy) {
      var vx = worldPt.x - px, vy = worldPt.y - py;
      var t = vx * dx + vy * dy;
      var projx = px + dx * t, projy = py + dy * t;
      var dist = Math.hypot(worldPt.x - projx, worldPt.y - projy);
      if (dist < bestDist) { bestDist = dist; best = new Point(projx, projy); }
    }
    var n = Math.max(4, state.perspectiveDensity | 0);
    vps.forEach(function (vp) {
      for (var i = 0; i < n; i++) {
        var a = (i / n) * Math.PI * 2;
        tryLine(vp.x, vp.y, Math.cos(a), Math.sin(a));
      }
    });
    if (vps.length >= 2) {
      var dx = vps[1].x - vps[0].x, dy = vps[1].y - vps[0].y;
      var len = Math.hypot(dx, dy) || 1;
      tryLine(vps[0].x, vps[0].y, dx / len, dy / len);
    }
    return best;
  }
  window.perspectiveSnapPointMagnetic = nearestGuideProjection;

  // ---- Tool interaction: drag a vanishing point, OR drag the horizon line
  // to move the whole guide at once (Sketchbook lets you reposition the
  // entire fan by dragging the eye-level line, not just one VP at a time).
  // Locked VPs stay put even during a whole-guide drag.
  var dragging = null; // the VP object currently being dragged, or null
  var draggingWhole = null; // {startX,startY,orig:[{x,y}...]} or null
  function shouldEdit() { return engineOn() && state.tool === 'perspective'; }
  function hitVP(worldPt) {
    var vps = ensurePerspectiveVPs();
    var tol = 12 / view.zoom;
    for (var i = 0; i < vps.length; i++) {
      if (vps[i].locked) continue;
      if (Math.hypot(vps[i].x - worldPt.x, vps[i].y - worldPt.y) < tol) return vps[i];
    }
    return null;
  }
  function hitHorizon(worldPt) {
    var vps = ensurePerspectiveVPs();
    if (vps.length < 2) return false;
    var tol = 10 / view.zoom;
    var p0 = vps[0], p1 = vps[1];
    var dx = p1.x - p0.x, dy = p1.y - p0.y;
    var len = Math.hypot(dx, dy) || 1;
    var ux = dx / len, uy = dy / len;
    var vx = worldPt.x - p0.x, vy = worldPt.y - p0.y;
    var t = vx * ux + vy * uy;
    var projx = p0.x + ux * t, projy = p0.y + uy * t;
    return Math.hypot(worldPt.x - projx, worldPt.y - projy) < tol;
  }
  function onDown(e) {
    if (!shouldEdit()) return;
    e.stopImmediatePropagation(); e.preventDefault();
    var w = window.SMEngineBridge.screenToWorld(e.clientX, e.clientY);
    var wp = new Point(w[0], w[1]);
    dragging = hitVP(wp);
    if (dragging) { window.SMEngineBridge.suspend(); return; }
    if (hitHorizon(wp)) {
      draggingWhole = { startX: wp.x, startY: wp.y, orig: ensurePerspectiveVPs().map(function (v) { return { x: v.x, y: v.y }; }) };
      window.SMEngineBridge.suspend();
    }
  }
  function onMove(e) {
    if (!shouldEdit() || (!dragging && !draggingWhole)) return;
    e.stopImmediatePropagation(); e.preventDefault();
    var w = window.SMEngineBridge.screenToWorld(e.clientX, e.clientY);
    if (dragging) {
      dragging.x = w[0]; dragging.y = w[1];
    } else {
      var dx = w[0] - draggingWhole.startX, dy = w[1] - draggingWhole.startY;
      var vps = ensurePerspectiveVPs();
      draggingWhole.orig.forEach(function (o, i) {
        if (vps[i].locked) return;
        vps[i].x = o.x + dx; vps[i].y = o.y + dy;
      });
    }
    window.SMEngineBridge.renderNow();
  }
  function onUp(e) {
    if (!shouldEdit() || (!dragging && !draggingWhole)) return;
    e.stopImmediatePropagation(); e.preventDefault();
    dragging = null; draggingWhole = null;
    window.SMEngineBridge.resume();
    window.SMEngineBridge.renderNow();
  }
  function init() {
    var target = document.getElementById('canvas-area') || document.getElementById('drawing-canvas');
    target.addEventListener('pointerdown', onDown, { capture: true });
    target.addEventListener('pointermove', onMove, { capture: true });
    target.addEventListener('pointerup', onUp, { capture: true });
    target.addEventListener('pointercancel', onUp, { capture: true });
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', init);
  else init();
})();
