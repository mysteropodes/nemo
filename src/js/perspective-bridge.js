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

  // Builds the overlay items — the VP markers only. Used to also draw N
  // static radiating lines per VP (+ a horizon line, + fisheye's concentric
  // rings) spanning the whole canvas at all times.
  //
  // 2026-09 removed (Cyril, live: "j'ai toujours les guides bleu, le but
  // c'est de pouvoir dessiner partout dans les axes avec le gizmo plus les
  // guides d'avant sauf les points de perspective que l'on peut bouger") —
  // that permanent fan was exactly the clutter he wanted gone: the moving
  // cursor crosshair (buildPerspectiveCursorGuideItems below, "le gizmo")
  // is now the ONLY axis indicator while drawing, always centered on where
  // you're actually about to draw instead of a fixed grid burned across the
  // whole canvas. The VPs themselves stay — draggable, visible, exactly as
  // before — since repositioning them is still how you set the guide up.
  // `state.perspectiveDensity` and the reach/candidate-ray math it used to
  // feed here are UNCHANGED and still live in nearestGuideProjection/
  // findGuideRayNear/buildPerspectiveGuideItems's siblings below — density
  // still controls how many candidate rays the snap logic considers, it
  // just no longer controls how many get drawn (nothing does, anymore).
  function buildPerspectiveGuideItems() {
    if (!state.perspectiveEnabled) return [];
    var vps = ensurePerspectiveVPs();
    var items = [];
    // VP markers themselves — draggable-point handles, Sketchbook-style
    // (feedback: "dans sketchbook on a des points que l'on peut
    // déplacer... regarde bien sketchbook" — a ring + filled center dot
    // reads unambiguously as "grab me", replacing the small diamond this
    // used to be). Locked VPs swap the center dot for a solid fill of the
    // ring itself, keeping Sketchbook's own locked/unlocked distinction.
    vps.forEach(function (vp) {
      var r = 9, innerR = r * 0.4, outer = [], inner = [];
      for (var i = 0; i < 16; i++) {
        var a = (i / 16) * Math.PI * 2;
        outer.push({ point: [vp.x + Math.cos(a) * r, vp.y + Math.sin(a) * r] });
        inner.push({ point: [vp.x + Math.cos(a) * innerR, vp.y + Math.sin(a) * innerR] });
      }
      items.push({
        segments: outer, closed: true,
        fillColor: vp.locked ? [255, 120, 80, 200] : [15, 20, 30, 160],
        strokeColor: [255, 150, 100, 255], strokeWidth: 2,
      });
      items.push({ segments: inner, closed: true, fillColor: [255, 150, 100, 255], strokeColor: null });
    });
    return items;
  }
  window.buildPerspectiveGuideItems = buildPerspectiveGuideItems;

  // ---- Live cursor guide (2026-08-31, Sketchbook's own "rotating compass"
  // — Cyril, with a reference video: "on a un guide sur 3 axes qui suit la
  // souris > 1 axe qui va vers point 1 de perspective > axe 2 vers l'autre
  // point > et un axe vertical [...] quand on dessine ça suit la direction
  // de là où l'on va") ----
  //
  // The static fan above radiates from each VP and never moves; this is
  // the OTHER half Sketchbook shows — a short dashed crosshair centered on
  // the CURSOR itself, pivoting as the cursor moves so its own axes always
  // point at the VPs from wherever the cursor currently is, plus one fixed
  // vertical axis. One axis per configured VP (so 1pt draws VP+vertical,
  // 2pt draws VP1+VP2+vertical — the exact 3-axis case Cyril described and
  // the video shows, 3pt draws all three VPs+vertical) rather than
  // hardcoding "exactly 2 VPs" — this is purely a drawing AID, so a 4th
  // reference line in 3pt mode costs nothing and never needs a special
  // case. This is a VISUAL INDICATOR ONLY: the actual snap-while-drawing
  // behavior it's showing you already exists (findVPRayByAngle/snapToVP
  // above, wired into draw-bridge.js) — short axes, not full-canvas rays,
  // so it doesn't compete with the static fan for attention.
  var CURSOR_GUIDE_REACH_PX = 46;
  function buildPerspectiveCursorGuideItems() {
    if (!state.perspectiveEnabled || !cursorWorld) return [];
    var vps = ensurePerspectiveVPs();
    var items = [];
    var zs = 1 / Math.max(0.0001, view.zoom);
    var reach = CURSOR_GUIDE_REACH_PX * zs;
    // 2026-09 fix (Cyril, live: "va sur brush perspective guide" showed
    // nothing) — this was pure white [255,255,255,210], invisible against
    // the default white canvas (state.canvasBg) that the overwhelming
    // majority of projects start on. Every OTHER element this same file
    // draws (the static fan, the horizon, the VP markers below) already
    // uses a saturated, opaque-ish color for exactly this reason — matched
    // to the VP markers' own orange/coral (they're both "live, interactive"
    // elements: the point you're aiming at and the guide following your
    // cursor toward it, as opposed to the fan's soft passive blue).
    var col = [255, 150, 100, 220];
    var cx = cursorWorld.x, cy = cursorWorld.y;
    function addAxis(dx, dy) {
      var len = Math.hypot(dx, dy);
      if (len < 1e-6) return; // cursor sits ON a VP — no direction to draw
      var ux = dx / len, uy = dy / len;
      items.push({
        segments: [{ point: [cx - ux * reach, cy - uy * reach] }, { point: [cx + ux * reach, cy + uy * reach] }],
        closed: false, fillColor: null, strokeColor: col, strokeWidth: 1 * zs, strokeCap: 'butt',
        dashPattern: [4 * zs, 3 * zs],
      });
    }
    vps.forEach(function (vp) { addAxis(vp.x - cx, vp.y - cy); });
    addAxis(0, 1); // vertical — screen-space up/down regardless of VP layout
    // Small center dot so the pivot point itself reads clearly against
    // three crossing dashed lines.
    items.push({
      segments: [{ point: [cx - 1.5 * zs, cy] }, { point: [cx + 1.5 * zs, cy] }],
      closed: false, fillColor: null, strokeColor: col, strokeWidth: 1.5 * zs, strokeCap: 'round',
    });
    return items;
  }
  window.buildPerspectiveCursorGuideItems = buildPerspectiveCursorGuideItems;

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

  // Hard directional lock (2026-07 — feedback: "la contrainte par rapport
  // aux guides est légère... ça dépend comment on fait notre trait mais il
  // peut partir dans une autre direction que le guide"). nearestGuideProjection
  // above is a per-POINT proximity magnet: it only pulls a sample onto a
  // guide line while that exact sample is within MAGNET_PX, so a stroke that
  // starts near a ray but then wanders away keeps drawing free-hand instead
  // of staying on the ray — not how Sketchbook's Perspective Guide actually
  // behaves once a stroke commits to a vanishing point. These two functions
  // let draw-bridge.js instead LOCK a whole stroke onto whichever ray is
  // closest at pointerdown (generous LOCK_TOLERANCE_PX, wider than the
  // per-point magnet's own, since committing to a direction should be easy
  // to trigger deliberately) and hard-project every subsequent point onto
  // that SAME ray for the rest of the gesture, regardless of how far the
  // hand later strays from it — matching a ruler-against-the-guide feel.
  // A stroke that starts far from every ray still draws completely free, by
  // design (not "near the guide" at all is a real, common intent).
  var LOCK_TOLERANCE_PX = 40;
  function findGuideRayNear(worldPt) {
    if (!state.perspectiveEnabled) return null;
    var vps = ensurePerspectiveVPs();
    if (!vps.length) return null;
    var tol = LOCK_TOLERANCE_PX / view.zoom;
    var best = null, bestDist = tol;
    function tryLine(px, py, dx, dy) {
      var vx = worldPt.x - px, vy = worldPt.y - py;
      var t = vx * dx + vy * dy;
      var projx = px + dx * t, projy = py + dy * t;
      var dist = Math.hypot(worldPt.x - projx, worldPt.y - projy);
      if (dist < bestDist) { bestDist = dist; best = { px: px, py: py, dx: dx, dy: dy }; }
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
  window.perspectiveFindGuideRayNear = findGuideRayNear;
  // Angle-based freehand lock (2026-07 — feedback: "dans sketchbook tu fais
  // tes traits n'importe où ils vont suivre la perspective" — the PROXIMITY
  // lock above (findGuideRayNear) only fires when the stroke STARTS within
  // LOCK_TOLERANCE_PX of an already-drawn guide ray, which is exactly the
  // opposite of Sketchbook's own behavior: there, the guide's ANGLE from a
  // vanishing point is what matters, not the stroke's distance from any
  // drawn line — you can start a stroke anywhere on the canvas and, once
  // its direction reads as "aimed at a VP," it locks onto that VP's ray.
  // Mirrors snapToVP's angle math above (same SNAP_DEG, same "closest VP
  // direction wins" logic) but returns a {px,py,dx,dy} ray struct usable by
  // projectOnRay/draw-bridge.js's guideConstrain, instead of a single
  // projected point — draw-bridge.js calls this once dragLen has crossed a
  // few pixels (see its own guideLockDecided comment for why the decision
  // can't happen at pointerdown, before any direction is known at all).
  function findVPRayByAngle(start, currentPt) {
    if (!state.perspectiveEnabled) return null;
    var dx = currentPt.x - start.x, dy = currentPt.y - start.y;
    var dragAngle = Math.atan2(dy, dx);
    var vps = ensurePerspectiveVPs();
    var best = null, bestDiff = SNAP_DEG * Math.PI / 180;
    vps.forEach(function (vp) {
      var vx = vp.x - start.x, vy = vp.y - start.y;
      var vlen = Math.hypot(vx, vy);
      if (vlen < 1) return; // start point sits ON the VP — no meaningful direction to compare against
      var vAngle = Math.atan2(vy, vx);
      var diff = Math.abs(Math.atan2(Math.sin(dragAngle - vAngle), Math.cos(dragAngle - vAngle)));
      if (diff < bestDiff) { bestDiff = diff; best = { px: start.x, py: start.y, dx: vx / vlen, dy: vy / vlen }; }
    });
    return best;
  }
  window.perspectiveFindVPRayByAngle = findVPRayByAngle;
  function projectOnRay(worldPt, ray) {
    var vx = worldPt.x - ray.px, vy = worldPt.y - ray.py;
    var t = vx * ray.dx + vy * ray.dy;
    return new Point(ray.px + ray.dx * t, ray.py + ray.dy * t);
  }
  window.perspectiveProjectOnRay = projectOnRay;

  // ---- Tool interaction: drag a vanishing point, OR drag the horizon line
  // to move the whole guide at once (Sketchbook lets you reposition the
  // entire fan by dragging the eye-level line, not just one VP at a time).
  // Locked VPs stay put even during a whole-guide drag.
  var dragging = null; // the VP object currently being dragged, or null
  var draggingWhole = null; // {startX,startY,orig:[{x,y}...]} or null
  // Live cursor position for the 3-axis follow guide below — updated
  // UNGATED, on every pointermove over the canvas, unlike dragging/
  // draggingWhole's own onMove body which only runs mid-drag. Read by
  // buildPerspectiveCursorGuideItems (this file) from engine-bridge.js's
  // VOLATILE items block — same "cheap live visual, expensive commit at
  // drop" split as eraserCursorWorld (engine-bridge.js's own pointer-
  // following overlay) already uses, and for the same reason: this file's
  // OWN buildPerspectiveGuideItems (the static VP fan) is built OUTSIDE
  // that block, so it's part of the cached scene prefix during a drag —
  // fine for a fan that never moves, but a cursor-following crosshair
  // bundled in there would freeze at wherever the cursor was when the
  // current drag/cache started, exactly the "grease pencil onion ghost
  // never refreshed" class of bug CLAUDE.md already warns about elsewhere.
  var cursorWorld = null;
  function trackCursor(e) {
    if (!engineOn() || !state.perspectiveEnabled) { cursorWorld = null; return; }
    var w = window.SMEngineBridge.screenToWorld(e.clientX, e.clientY);
    cursorWorld = { x: w[0], y: w[1] };
  }
  // Broadened 2026-07 — same reasoning as symmetry-bridge.js's own
  // shouldEdit (feedback: "garder l'outil brush sélectionné et pouvoir
  // modifier les guides dans le canvas quand même"): dragging a VP/horizon
  // handle no longer requires switching into the dedicated 'perspective'
  // tool, just the guide being enabled — safe since onDown only claims an
  // explicit hit-test hit and this file loads before every drawing-tool
  // bridge.
  function shouldEdit() { return engineOn() && (state.tool === 'perspective' || state.perspectiveEnabled); }
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
    // Only claim the event on an ACTUAL hit — same fix as
    // symmetry-bridge.js's own onDown (see its comment): calling
    // stopImmediatePropagation/preventDefault unconditionally here was safe
    // while shouldEdit() required actually being in the dedicated
    // 'perspective' tool, but broke ordinary drawing entirely once
    // shouldEdit() could also be true during Draw/Fill Brush.
    var w = window.SMEngineBridge.screenToWorld(e.clientX, e.clientY);
    var wp = new Point(w[0], w[1]);
    dragging = hitVP(wp);
    if (dragging) { e.stopImmediatePropagation(); e.preventDefault(); window.SMEngineBridge.suspend(); return; }
    if (hitHorizon(wp)) {
      e.stopImmediatePropagation(); e.preventDefault();
      draggingWhole = { startX: wp.x, startY: wp.y, orig: ensurePerspectiveVPs().map(function (v) { return { x: v.x, y: v.y }; }) };
      window.SMEngineBridge.suspend();
    }
  }
  function onMove(e) {
    // Ungated — see trackCursor's own comment. Runs even when this same
    // move goes on to be ignored (no VP drag active) or later stopped by a
    // DIFFERENT tool's own capture-phase listener further down the chain:
    // this file loads before every drawing-tool bridge (index.html script
    // order), so its capture listener always sees the event first.
    trackCursor(e);
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
    // Clears the live cursor guide the instant the pointer leaves the
    // canvas — pointermove alone never fires again once outside, so
    // without this the crosshair would freeze at its last position
    // instead of disappearing, the exact "ghost" class of bug a stale
    // volatile cursor overlay always risks.
    target.addEventListener('pointerleave', function () { cursorWorld = null; window.SMEngineBridge && window.SMEngineBridge.renderNow(); }, { capture: true });
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', init);
  else init();
})();
