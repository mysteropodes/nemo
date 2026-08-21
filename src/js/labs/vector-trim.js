// ---- LABS PROTOTYPE — Vector trim at intersections (Clip Studio Paint) ----
// CSP's beloved vector eraser mode. 2026-08 rebuild (feedback #32 — "le but
// de cette outil c'est de drag [un] dessin temp sur une ligne et celle-ci
// s'efface jusqu'à un croisement avec une autre ligne"): the tool used to be
// a "position the cursor, click a button" single-point action. It's now a
// real knife-drag gesture, same capture-phase-interception architecture as
// draw-bridge.js/eraser-bridge.js — while armed, a click-drag anywhere on
// the canvas draws a TEMPORARY guide line (never inserted in the document)
// with a live highlight of every stroke it currently crosses; on release,
// every stroke the guide line crossed gets trimmed back to its own nearest
// intersections with OTHER strokes (or removed entirely if it has none) —
// CSP's "erase to intersection", but triggered by a cut across the line
// instead of a touch ON it, and able to cut several different lines in one
// drag.
//
// window.SMLabs.setKnifeTrimActive(bool) / isKnifeTrimActive() — arm/disarm.
// Pure Paper splitAt/remove on ordinary Paths; every surviving piece gets a
// fresh strokeId so tween matching never sees two objects claiming the same
// identity (CLAUDE.md §1).
(function () {
  var armed = false;
  var dragging = false;
  var pts = []; // world-space [x,y] samples of the temp knife line
  var target = null; // canvas element pointer listeners are bound to

  function freshId() { return 'labs_' + Date.now().toString(36) + '_' + Math.floor(Math.random() * 1e6); }

  function isEligible(o) {
    return o instanceof Path && o.segments.length >= 2 &&
      !(o.data && (o.data.isVectorBrush || o.data.isFillShape || o.data.isLinkedFillCompanion || o.data.isBrushTextureCopy));
  }

  function shouldIntercept() {
    return armed && window.SMEngineBridge && window.SMEngineBridge.isEnabled() && !state.playing &&
      !(window.editRefusalReason && window.editRefusalReason());
  }

  function dedupeSorted(arr) {
    var s = arr.slice().sort(function (a, b) { return a - b; });
    var out = [];
    s.forEach(function (v) { if (!out.length || v - out[out.length - 1] > 0.5) out.push(v); });
    return out;
  }

  // Every intersection offset of `t` (target) with the OTHER real strokes of
  // the layer (plus its own self-crossings, both branches) — same "bracket
  // candidates" computation the old single-point tool used, just renamed.
  function bracketOffsets(layer, t) {
    var offs = [];
    layer.children.forEach(function (o) {
      if (!isEligible(o)) return;
      var ix = t.getIntersections(o);
      ix.forEach(function (loc) {
        offs.push(loc.offset);
        if (o === t && loc.intersection) offs.push(loc.intersection.offset);
      });
    });
    var L = t.length;
    return offs.filter(function (v) { return v > 1e-6 && v < L - 1e-6; }).sort(function (a, b) { return a - b; });
  }

  // Builds the cut plan for every stroke the knife path crosses WITHOUT
  // mutating anything — so a gesture that cuts several different strokes in
  // one drag always brackets each of them against the layer's ORIGINAL
  // (pre-cut) geometry, not a half-mutated one.
  function buildPlan(layer, knifePath) {
    var plans = [];
    layer.children.slice().forEach(function (t) {
      if (!isEligible(t)) return;
      var ix = knifePath.getIntersections(t);
      if (!ix.length) return;
      var crossOffs = dedupeSorted(ix.map(function (loc) { return loc.intersection.offset; }));
      var offs = bracketOffsets(layer, t);
      if (!offs.length) { plans.push({ target: t, removeWhole: true }); return; }
      var L = t.length;
      var spans = crossOffs.map(function (co) {
        var prev = 0, next = L;
        for (var i = 0; i < offs.length; i++) { if (offs[i] < co) prev = offs[i]; else { next = offs[i]; break; } }
        return { start: prev, end: next };
      });
      spans.sort(function (a, b) { return a.start - b.start; });
      var merged = [];
      spans.forEach(function (s) {
        var last = merged[merged.length - 1];
        if (last && s.start <= last.end + 1e-6) last.end = Math.max(last.end, s.end);
        else merged.push({ start: s.start, end: s.end });
      });
      plans.push({ target: t, spans: merged, closed: t.closed, L: L });
    });
    return plans;
  }

  function applyPlan(p) {
    var t = p.target;
    if (p.removeWhole) {
      if (t.data && t.data.linkedFill) t.data.linkedFill.remove();
      t.remove();
      return;
    }
    if (p.closed) {
      // Multiple crossings on the same closed shape in one drag: cut the
      // first span only — opening a closed path at more than one point in
      // the same pass changes topology mid-loop, not worth the complexity
      // for a Labs prototype. Single crossing (the common case) is exact.
      var span = p.spans[0];
      var opened = t.splitAt(t.getLocationAt(span.start)) || t;
      var spanLen = (span.end - span.start + p.L) % p.L;
      var tail = opened.splitAt(opened.getLocationAt(spanLen));
      opened.remove();
      if (tail && tail.data && tail.data.strokeId) tail.data.strokeId = freshId();
      return;
    }
    // Open path, any number of spans: process farthest-start first so each
    // split only ever touches the still-untouched [0, start] prefix of the
    // ORIGINAL parameterization — same invariant the single-span version
    // relied on ("order matters"), just iterated.
    var spans = p.spans.slice().sort(function (a, b) { return b.start - a.start; });
    var head = t;
    spans.forEach(function (span) {
      if (!head) return;
      if (span.end < p.L - 1e-6) {
        var tailPiece = head.splitAt(head.getLocationAt(span.end));
        if (tailPiece && tailPiece.data && tailPiece.data.strokeId) tailPiece.data.strokeId = freshId();
      }
      if (span.start > 1e-6) {
        var doomed = head.splitAt(head.getLocationAt(span.start));
        if (doomed) doomed.remove();
      } else {
        head.remove();
        head = null;
      }
    });
  }

  function commitCut() {
    var layer = userLayers[state.activeLayerIdx];
    if (!layer || pts.length < 2) return false;
    // Reject a near-tap: a real cut needs an actual drag across a line, not
    // an accidental click.
    var dx = pts[pts.length - 1][0] - pts[0][0], dy = pts[pts.length - 1][1] - pts[0][1];
    var span = 0;
    for (var i = 1; i < pts.length; i++) span += Math.hypot(pts[i][0] - pts[i - 1][0], pts[i][1] - pts[i - 1][1]);
    if (span < 4 / view.zoom) return false;
    var knifePath = new Path({ insert: false, segments: pts.map(function (p) { return new Point(p[0], p[1]); }) });
    var plan = buildPlan(layer, knifePath);
    if (!plan.length) {
      if (typeof showToast === 'function') showToast('Aucun trait sous le couteau');
      return false;
    }
    pushUndo();
    plan.forEach(applyPlan);
    saveActiveLayerFrame(); updateUI();
    if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
    if (typeof showToast === 'function') {
      showToast(plan.length === 1 ? 'Trait coupé' : plan.length + ' traits coupés');
    }
    return true;
  }

  function knifeSegments() {
    return pts.map(function (p) { return { point: [p[0], p[1]], handleIn: [0, 0], handleOut: [0, 0] }; });
  }
  function pathOverlaySegments(p) {
    return p.segments.map(function (s) {
      return { point: [s.point.x, s.point.y], handleIn: [s.handleIn.x, s.handleIn.y], handleOut: [s.handleOut.x, s.handleOut.y] };
    });
  }

  function liveOverlay() {
    var items = [{
      segments: knifeSegments(), closed: false,
      strokeColor: [255, 90, 40, 235], strokeWidth: Math.max(1, 2 / view.zoom),
      dashPattern: [6 / view.zoom, 4 / view.zoom],
    }];
    var layer = userLayers[state.activeLayerIdx];
    if (layer && pts.length >= 2) {
      var knifePath = new Path({ insert: false, segments: pts.map(function (p) { return new Point(p[0], p[1]); }) });
      var kb = knifePath.bounds;
      layer.children.forEach(function (t) {
        if (!isEligible(t)) return;
        if (!kb.intersects(t.bounds)) return; // cheap pre-filter — getIntersections isn't free on a large scene
        if (!knifePath.getIntersections(t).length) return;
        items.push({
          segments: pathOverlaySegments(t), closed: t.closed,
          strokeColor: [255, 205, 60, 220],
          strokeWidth: (t.strokeWidth || 2) + 6 / view.zoom,
        });
      });
    }
    return items;
  }

  function onDown(e) {
    if (!shouldIntercept()) return;
    e.stopImmediatePropagation();
    e.preventDefault();
    dragging = true;
    pts = [];
    var w = window.SMEngineBridge.screenToWorld(e.clientX, e.clientY);
    pts.push(w);
    window.SMEngineBridge.suspend();
    window.SMEngineBridge.renderWithOverlayItem(liveOverlay());
  }
  function onMove(e) {
    if (!dragging) return;
    e.stopImmediatePropagation();
    e.preventDefault();
    var w = window.SMEngineBridge.screenToWorld(e.clientX, e.clientY);
    var last = pts[pts.length - 1];
    if (!last || Math.hypot(w[0] - last[0], w[1] - last[1]) >= 1 / view.zoom) pts.push(w);
    window.SMEngineBridge.renderWithOverlayItem(liveOverlay());
  }
  function onUp(e) {
    if (!dragging) return;
    e.stopImmediatePropagation();
    e.preventDefault();
    dragging = false;
    var w = window.SMEngineBridge.screenToWorld(e.clientX, e.clientY);
    pts.push(w);
    commitCut();
    window.SMEngineBridge.resume();
    window.SMEngineBridge.renderNow();
    pts = [];
  }
  function onKeyDown(e) {
    if (e.key !== 'Escape') return;
    if (dragging) {
      dragging = false;
      pts = [];
      window.SMEngineBridge.resume();
      window.SMEngineBridge.renderNow();
    } else if (armed) {
      window.SMLabs.setKnifeTrimActive(false);
    }
  }

  window.SMLabs.setKnifeTrimActive = function (on) {
    armed = !!on;
    if (!armed && dragging) {
      dragging = false; pts = [];
      if (window.SMEngineBridge) { window.SMEngineBridge.resume(); window.SMEngineBridge.renderNow(); }
    }
    if (target) target.style.cursor = armed ? 'crosshair' : '';
    if (window.renderLabsFloatPanel) window.renderLabsFloatPanel();
    if (typeof showToast === 'function' && armed) showToast('Couteau vectoriel : glisser à travers un trait pour le couper');
  };
  window.SMLabs.isKnifeTrimActive = function () { return armed; };

  function init() {
    target = document.getElementById('canvas-area') || document.getElementById('drawing-canvas');
    target.addEventListener('pointerdown', onDown, { capture: true });
    target.addEventListener('pointermove', onMove, { capture: true });
    target.addEventListener('pointerup', onUp, { capture: true });
    target.addEventListener('pointercancel', onUp, { capture: true });
    document.addEventListener('keydown', onKeyDown);
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', init);
  else init();

  window.SMLabs.register('vector-trim', {
    flag: 'nemo-labs-trim',
    describe: 'Couteau vectoriel CSP : glisser un trait-couteau à travers une ou plusieurs lignes pour les couper à leurs intersections les plus proches',
  });
})();
