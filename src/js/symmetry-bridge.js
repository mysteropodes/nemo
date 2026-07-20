// ---- SYMMETRY DRAWING GUIDE (Sketchbook-style) ----
// A pure drawing AID that also COPIES: unlike perspective-bridge.js (which
// only guides/snaps, never adds geometry), enabling Symmetry makes every
// committed stroke also produce a mirrored (Y/X/Free) or rotated (Radial/
// mandala) copy — same idea as Sketchbook's Symmetry tool. The axis/center
// itself is rendered as an overlay (same "extra layer pushed into
// buildSceneJson()'s scene, never saved to frame data" convention as the
// perspective guide, marquee, transform-box, etc.) and persists across tool
// switches once state.symmetryEnabled is true — only DRAGGING the axis (or
// its endpoints, or the radial center) requires the dedicated 'symmetry'
// tool to be active, matching every other tool-specific interaction here.
//
// Promoted from the earlier Labs prototype (src/js/labs/symmetry-mirror.js,
// removed) — same underlying "copy is a plain untagged Path, no data.*
// field of its own" principle (CLAUDE.md §1: the safest way to add
// something new without auditing every layer.children consumer is to not be
// structurally new at all), generalized from a canvas-center-only vertical
// mirror to an arbitrary draggable axis line, plus a real toggleable Free
// (arbitrary angle) mode and an "extend/stop at axis" option.
//
// Scope note (matches the promoted prototype's own limitation): only wired
// into draw-bridge.js's freehand/pressure-brush commit + live-preview path,
// not into tools.js's Pen/Line/Rect/Ellipse tool commits — mirroring those
// too is a separate, larger follow-up (each has its own commit function).
(function () {
  function engineOn() { return window.SMEngineBridge && window.SMEngineBridge.isEnabled() && !state.playing; }

  // ---- geometry helpers -------------------------------------------------
  function axisDir(axis) {
    var dx = axis.x2 - axis.x1, dy = axis.y2 - axis.y1;
    var len = Math.hypot(dx, dy) || 1;
    return { ux: dx / len, uy: dy / len };
  }
  // Reflects a WORLD POINT across the axis line. General line-reflection
  // (works for any angle, not just vertical/horizontal) — project the point
  // onto the axis, then mirror it through that projection.
  function reflectPoint(px, py, axis) {
    var d = axisDir(axis);
    var vx = px - axis.x1, vy = py - axis.y1;
    var t = vx * d.ux + vy * d.uy;
    var projx = axis.x1 + t * d.ux, projy = axis.y1 + t * d.uy;
    return { x: 2 * projx - px, y: 2 * projy - py };
  }
  // Reflects a RELATIVE VECTOR (Paper segment handles, which are offsets,
  // not points — no translation involved, only direction) across the axis.
  function reflectVector(vx, vy, axis) {
    var d = axisDir(axis);
    var t = vx * d.ux + vy * d.uy;
    return { x: 2 * t * d.ux - vx, y: 2 * t * d.uy - vy };
  }
  // Signed area (cross product) of (point - axis.x1/y1) against the axis
  // direction — sign tells you which side of the line the point is on.
  function sideOfXY(x, y, axis) {
    var dx = axis.x2 - axis.x1, dy = axis.y2 - axis.y1;
    var vx = x - axis.x1, vy = y - axis.y1;
    return dx * vy - dy * vx;
  }
  // "Stop at axis" mode (state.symmetryExtend === false): truncates a
  // samples array (or Paper segments, see clipPathAtAxis below) the instant
  // it crosses to the far side of the axis, so a stroke drawn across the
  // fold never overlaps its own mirror there. Simple side-flip cut, not an
  // interpolated exact intersection point — good enough for a drawing aid,
  // matches how a hand-drawn line wobbling near the axis actually behaves.
  function clipSamplesAtAxis(samples, axis) {
    if (!samples.length) return samples;
    var side0 = sideOfXY(samples[0][0], samples[0][1], axis) >= 0;
    var out = [samples[0]];
    for (var i = 1; i < samples.length; i++) {
      var s = sideOfXY(samples[i][0], samples[i][1], axis) >= 0;
      if (s !== side0) break;
      out.push(samples[i]);
    }
    return out;
  }
  function clipPathAtAxis(path, axis) {
    if (!path.segments.length) return;
    var side0 = sideOfXY(path.segments[0].point.x, path.segments[0].point.y, axis) >= 0;
    var cut = path.segments.length;
    for (var i = 1; i < path.segments.length; i++) {
      var s = sideOfXY(path.segments[i].point.x, path.segments[i].point.y, axis) >= 0;
      if (s !== side0) { cut = i; break; }
    }
    if (cut < path.segments.length) path.removeSegments(cut);
  }

  // ---- default geometry / lazy seeding ----------------------------------
  function defaultAxis(mode) {
    var w = state.canvasW, h = state.canvasH, cx = w / 2, cy = h / 2;
    if (mode === 'x') return { x1: 0, y1: cy, x2: w, y2: cy };
    if (mode === 'free') {
      var r = Math.min(w, h) * 0.4;
      return { x1: cx - r, y1: cy - r, x2: cx + r, y2: cy + r };
    }
    return { x1: cx, y1: 0, x2: cx, y2: h }; // 'y' default
  }
  function defaultRadialCenter() { return { x: state.canvasW / 2, y: state.canvasH / 2 }; }
  function ensureSymmetryAxis() {
    if (!state.symmetryAxis) state.symmetryAxis = defaultAxis(state.symmetryMode);
    return state.symmetryAxis;
  }
  function ensureSymmetryRadialCenter() {
    if (!state.symmetryRadialCenter) state.symmetryRadialCenter = defaultRadialCenter();
    return state.symmetryRadialCenter;
  }
  window.ensureSymmetryAxis = ensureSymmetryAxis;
  window.ensureSymmetryRadialCenter = ensureSymmetryRadialCenter;
  window.resetSymmetryGuide = function () {
    if (state.symmetryMode === 'radial') state.symmetryRadialCenter = defaultRadialCenter();
    else state.symmetryAxis = defaultAxis(state.symmetryMode);
    if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
  };
  window.setSymmetryMode = function (mode) {
    state.symmetryMode = mode;
    if (mode === 'radial') state.symmetryRadialCenter = defaultRadialCenter();
    else state.symmetryAxis = defaultAxis(mode);
    if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
  };
  function sectorCount() { return Math.max(2, Math.min(24, state.symmetryRadialSectors | 0 || 6)); }

  // ---- overlay: the axis line (+ endpoint handles) or the radial spokes
  // (+ center handle) — same "diamond marker" visual language as
  // perspective-bridge.js's VP markers, distinct color so the two guides
  // never get confused when both happen to be on at once.
  function buildSymmetryGuideItems() {
    if (!state.symmetryEnabled) return [];
    var items = [];
    var reach = Math.max(state.canvasW, state.canvasH) * 4;
    var col = [230, 90, 200, 130];
    var handleCol = [230, 120, 210, 255];
    // Draggable-point handle, Sketchbook-style (feedback: "dans sketchbook
    // on a des points que l'on peut déplacer... regarde bien sketchbook") —
    // a ring + filled center dot, not the small diamond this used to be.
    // Bigger and rounder reads unambiguously as "a grabbable point", the
    // exact same visual language Sketchbook's own guide handles use.
    function ringHandle(x, y, r) {
      var outer = [], inner = [];
      var innerR = r * 0.4;
      for (var i = 0; i < 16; i++) {
        var a = (i / 16) * Math.PI * 2;
        outer.push({ point: [x + Math.cos(a) * r, y + Math.sin(a) * r] });
        inner.push({ point: [x + Math.cos(a) * innerR, y + Math.sin(a) * innerR] });
      }
      items.push({ segments: outer, closed: true, fillColor: [15, 20, 30, 160], strokeColor: handleCol, strokeWidth: 2 });
      items.push({ segments: inner, closed: true, fillColor: handleCol, strokeColor: null });
    }
    if (state.symmetryMode === 'radial') {
      var c = ensureSymmetryRadialCenter();
      var n = sectorCount();
      for (var i = 0; i < n; i++) {
        var a = (i / n) * Math.PI * 2;
        items.push({
          segments: [{ point: [c.x, c.y] }, { point: [c.x + Math.cos(a) * reach, c.y + Math.sin(a) * reach] }],
          closed: false, fillColor: null, strokeColor: col, strokeWidth: 1, strokeCap: 'butt',
        });
      }
      ringHandle(c.x, c.y, 9);
    } else {
      var axis = ensureSymmetryAxis();
      var d = axisDir(axis);
      var mx = (axis.x1 + axis.x2) / 2, my = (axis.y1 + axis.y2) / 2;
      items.push({
        segments: [{ point: [mx - d.ux * reach, my - d.uy * reach] }, { point: [mx + d.ux * reach, my + d.uy * reach] }],
        closed: false, fillColor: null, strokeColor: col, strokeWidth: 1.4, strokeCap: 'butt',
      });
      // One handle per draggable point: both endpoints in Free mode (each
      // independently grabbable), but Y/X only really has ONE meaningful
      // drag point — the whole-axis translate — so a single handle at the
      // visible midpoint reads more like Sketchbook's own Y/X guide (one
      // ring on the line, not two at its off-screen-reaching ends).
      if (state.symmetryMode === 'free') {
        ringHandle(axis.x1, axis.y1, 7);
        ringHandle(axis.x2, axis.y2, 7);
      } else {
        ringHandle(mx, my, 9);
      }
    }
    return items;
  }
  window.buildSymmetryGuideItems = buildSymmetryGuideItems;

  // ---- draw-bridge.js hooks: same (samples, overlayItemFor) / (path,
  // layer) signatures the old Labs prototype used, so draw-bridge.js's two
  // call sites barely changed when this was promoted out of Labs.
  function onPreview(samples, overlayItemFor) {
    if (!state.symmetryEnabled || !samples || samples.length < 2) return [];
    if (state.symmetryMode === 'radial') {
      var c = ensureSymmetryRadialCenter();
      var n = sectorCount();
      var out = [];
      for (var k = 1; k < n; k++) {
        var ang = k * 2 * Math.PI / n, cos = Math.cos(ang), sin = Math.sin(ang);
        var rotated = samples.map(function (s) {
          var dx = s[0] - c.x, dy = s[1] - c.y;
          return [c.x + dx * cos - dy * sin, c.y + dx * sin + dy * cos, s[2]];
        });
        out = out.concat(overlayItemFor(rotated));
      }
      return out;
    }
    var axis = ensureSymmetryAxis();
    var src = state.symmetryExtend ? samples : clipSamplesAtAxis(samples, axis);
    var mirrored = src.map(function (s) {
      var r = reflectPoint(s[0], s[1], axis);
      return [r.x, r.y, s[2]];
    });
    return overlayItemFor(mirrored);
  }
  function onStrokeCommitted(path, layer) {
    if (!state.symmetryEnabled || !path.segments || !path.segments.length) return;
    if (state.symmetryMode === 'radial') {
      var c = ensureSymmetryRadialCenter();
      var n = sectorCount();
      var center = new Point(c.x, c.y);
      for (var k = 1; k < n; k++) {
        var copy = path.clone({ insert: false });
        copy.rotate(k * 360 / n, center);
        copy.insertAbove(path);
        if (typeof tagOwner === 'function') tagOwner(copy);
      }
      return;
    }
    var axis = ensureSymmetryAxis();
    if (!state.symmetryExtend) clipPathAtAxis(path, axis);
    var mirrored = path.clone({ insert: false });
    mirrored.segments.forEach(function (seg) {
      var rp = reflectPoint(seg.point.x, seg.point.y, axis);
      var rhi = reflectVector(seg.handleIn.x, seg.handleIn.y, axis);
      var rho = reflectVector(seg.handleOut.x, seg.handleOut.y, axis);
      seg.point = new Point(rp.x, rp.y);
      seg.handleIn = new Point(rhi.x, rhi.y);
      seg.handleOut = new Point(rho.x, rho.y);
    });
    // Reflecting across ANY line flips handedness (a mirror's determinant
    // is -1) — re-orient closed shapes so fills read the same way as the
    // original, not inside-out. Same fix the promoted prototype already had
    // for the vertical-only case; still correct for an arbitrary angle.
    if (mirrored.closed) mirrored.reverse();
    mirrored.insertAbove(path);
    if (typeof tagOwner === 'function') tagOwner(mirrored);
  }
  // Same mirror/rotate math as onPreview/onStrokeCommitted above, but for a
  // single already-built scene-item object ({segments,closed,fillColor,
  // strokeColor,strokeWidth} — the Rust ItemIn wire shape) instead of a
  // [x,y,width] samples array or a live Paper Path. shape-bridge.js's Line/
  // Rect/Ellipse live preview builds exactly this kind of item directly
  // (no samples array, no Paper Path to clone), so it needs its own entry
  // point rather than reusing onPreview/onStrokeCommitted verbatim.
  // "Stop at axis" clipping intentionally doesn't apply here — clipping a
  // whole closed Rect/Ellipse mid-shape isn't a meaningful operation the
  // way it is for a freehand line; skipped for scene items on purpose.
  function transformSceneItemPoints(segments, fn) {
    return segments.map(function (s) {
      var p = fn(s.point[0], s.point[1]);
      var out = { point: [p.x, p.y] };
      if (s.handleIn) { var hi = fn(s.handleIn[0], s.handleIn[1], true); out.handleIn = [hi.x, hi.y]; }
      if (s.handleOut) { var ho = fn(s.handleOut[0], s.handleOut[1], true); out.handleOut = [ho.x, ho.y]; }
      return out;
    });
  }
  function mirrorSceneItem(item) {
    if (!state.symmetryEnabled || !item || !item.segments) return [];
    if (state.symmetryMode === 'radial') {
      var c = ensureSymmetryRadialCenter();
      var n = sectorCount();
      var out = [];
      for (var k = 1; k < n; k++) {
        var ang = k * 2 * Math.PI / n, cos = Math.cos(ang), sin = Math.sin(ang);
        var segs = transformSceneItemPoints(item.segments, function (x, y, isVec) {
          if (isVec) return { x: x * cos - y * sin, y: x * sin + y * cos };
          var dx = x - c.x, dy = y - c.y;
          return { x: c.x + dx * cos - dy * sin, y: c.y + dx * sin + dy * cos };
        });
        out.push({ segments: segs, closed: item.closed, fillColor: item.fillColor, strokeColor: item.strokeColor, strokeWidth: item.strokeWidth });
      }
      return out;
    }
    var axis = ensureSymmetryAxis();
    var segs2 = transformSceneItemPoints(item.segments, function (x, y, isVec) {
      return isVec ? reflectVector(x, y, axis) : reflectPoint(x, y, axis);
    });
    return [{ segments: segs2, closed: item.closed, fillColor: item.fillColor, strokeColor: item.strokeColor, strokeWidth: item.strokeWidth }];
  }
  window.SMSymmetry = { onPreview: onPreview, onStrokeCommitted: onStrokeCommitted, mirrorSceneItem: mirrorSceneItem };

  // ---- tool interaction: drag an axis endpoint (Free mode), drag the
  // whole axis (Y/X translate it perpendicular to itself, Free translates
  // freely), or drag the radial center — same capture-phase pointerdown/
  // move/up pattern as perspective-bridge.js's VP/horizon dragging.
  var draggingEndpoint = null; // 'p1' | 'p2' | null
  var draggingWhole = null; // {startX,startY,orig:{x1,y1,x2,y2}} | null
  var draggingCenter = false;
  // Broadened 2026-07 (feedback: "quand j'utilise brush et clic sur les
  // boutons de guides je perds la sélection de l'outil brush... il faudrait
  // garder l'outil brush sélectionné et pouvoir modifier les guides dans le
  // canvas quand même") — dragging the axis/endpoints/center no longer
  // requires actually switching into the dedicated 'symmetry' tool, just
  // the guide being enabled. Safe regardless of which tool is active: this
  // file's onDown/onMove/onUp only ever act on an explicit hit-test hit
  // (hitEndpoint/hitAxisLine/hitCenter) and fall through untouched on a
  // miss, and load BEFORE every drawing-tool bridge (draw-bridge.js,
  // pen-bridge.js, shape-bridge.js — see index.html's script order), so a
  // real hit is always claimed here first, before that tool's own onDown
  // ever sees the event.
  function shouldEdit() { return engineOn() && (state.tool === 'symmetry' || state.symmetryEnabled); }
  function hitEndpoint(worldPt, axis) {
    var tol = 12 / view.zoom;
    if (Math.hypot(axis.x1 - worldPt.x, axis.y1 - worldPt.y) < tol) return 'p1';
    if (Math.hypot(axis.x2 - worldPt.x, axis.y2 - worldPt.y) < tol) return 'p2';
    return null;
  }
  function hitAxisLine(worldPt, axis) {
    var tol = 10 / view.zoom;
    var d = axisDir(axis);
    var vx = worldPt.x - axis.x1, vy = worldPt.y - axis.y1;
    var t = vx * d.ux + vy * d.uy;
    var projx = axis.x1 + t * d.ux, projy = axis.y1 + t * d.uy;
    return Math.hypot(worldPt.x - projx, worldPt.y - projy) < tol;
  }
  function hitCenter(worldPt, c) {
    var tol = 12 / view.zoom;
    return Math.hypot(c.x - worldPt.x, c.y - worldPt.y) < tol;
  }
  function onDown(e) {
    if (!shouldEdit()) return;
    // stopImmediatePropagation/preventDefault only on an ACTUAL hit, not on
    // every shouldEdit()-true pointerdown — 2026-07 regression found while
    // verifying the broadened shouldEdit() above (see its own comment):
    // this used to fire unconditionally right here, which was harmless
    // when shouldEdit() required actually BEING in the dedicated 'symmetry'
    // tool (nothing else needed the event then anyway), but now that
    // shouldEdit() can be true while Draw/Fill Brush is active, doing this
    // before the hit-test swallowed EVERY click on the canvas the instant
    // the guide was enabled — not just clicks on the guide itself — which
    // silently broke ordinary drawing. Confirmed live: drawing off-axis
    // committed 0 strokes with the guide on vs 1 with it off, same drag.
    var w = window.SMEngineBridge.screenToWorld(e.clientX, e.clientY);
    var wp = new Point(w[0], w[1]);
    if (state.symmetryMode === 'radial') {
      var c = ensureSymmetryRadialCenter();
      if (hitCenter(wp, c)) { e.stopImmediatePropagation(); e.preventDefault(); draggingCenter = true; window.SMEngineBridge.suspend(); }
      return;
    }
    var axis = ensureSymmetryAxis();
    if (state.symmetryMode === 'free') {
      var hit = hitEndpoint(wp, axis);
      if (hit) { e.stopImmediatePropagation(); e.preventDefault(); draggingEndpoint = hit; window.SMEngineBridge.suspend(); return; }
    }
    if (hitAxisLine(wp, axis)) {
      e.stopImmediatePropagation(); e.preventDefault();
      draggingWhole = { startX: wp.x, startY: wp.y, orig: { x1: axis.x1, y1: axis.y1, x2: axis.x2, y2: axis.y2 } };
      window.SMEngineBridge.suspend();
    }
  }
  function onMove(e) {
    if (!shouldEdit() || (!draggingEndpoint && !draggingWhole && !draggingCenter)) return;
    e.stopImmediatePropagation(); e.preventDefault();
    var w = window.SMEngineBridge.screenToWorld(e.clientX, e.clientY);
    if (draggingCenter) {
      var c = ensureSymmetryRadialCenter();
      c.x = w[0]; c.y = w[1];
    } else if (draggingEndpoint) {
      var axis = ensureSymmetryAxis();
      if (draggingEndpoint === 'p1') { axis.x1 = w[0]; axis.y1 = w[1]; }
      else { axis.x2 = w[0]; axis.y2 = w[1]; }
    } else {
      var dx = w[0] - draggingWhole.startX, dy = w[1] - draggingWhole.startY;
      var axis2 = ensureSymmetryAxis();
      // Y/X modes stay constrained to a pure vertical/horizontal line —
      // only the perpendicular offset moves, so it can never accidentally
      // drift into a tilted line while dragging the whole guide.
      if (state.symmetryMode === 'y') {
        axis2.x1 = draggingWhole.orig.x1 + dx; axis2.x2 = draggingWhole.orig.x2 + dx;
      } else if (state.symmetryMode === 'x') {
        axis2.y1 = draggingWhole.orig.y1 + dy; axis2.y2 = draggingWhole.orig.y2 + dy;
      } else {
        axis2.x1 = draggingWhole.orig.x1 + dx; axis2.y1 = draggingWhole.orig.y1 + dy;
        axis2.x2 = draggingWhole.orig.x2 + dx; axis2.y2 = draggingWhole.orig.y2 + dy;
      }
    }
    window.SMEngineBridge.renderNow();
  }
  function onUp(e) {
    if (!shouldEdit() || (!draggingEndpoint && !draggingWhole && !draggingCenter)) return;
    e.stopImmediatePropagation(); e.preventDefault();
    draggingEndpoint = null; draggingWhole = null; draggingCenter = false;
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
