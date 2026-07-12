// ---- LABS PROTOTYPE — Vector trim at intersections (Clip Studio Paint) ----
// CSP's beloved vector eraser mode: touch a line and it vanishes exactly
// UP TO its intersections with other lines — instant cleanup of
// overshoots at corners, no careful scrubbing. Action form:
//
//   SMLabs.trimAtIntersections(worldX, worldY)
//   SMLabs.trimAtPointer()      — same, at the last known pointer position
//
// Finds the stroke under the point, collects every intersection offset
// with the other strokes of the layer (and its own self-crossings), and
// removes the span between the two intersections bracketing the touched
// spot (or to the line end when the touched span is terminal — same as
// CSP). Pure Paper splitAt/remove on ordinary Paths; the surviving tail
// gets a fresh strokeId so tween matching never sees two objects claiming
// the same identity.
(function () {
  var lastPtr = null;
  document.addEventListener('pointermove', function (e) { lastPtr = [e.clientX, e.clientY]; }, true);

  function freshId() { return 'labs_' + Date.now().toString(36) + '_' + Math.floor(Math.random() * 1e6); }

  window.SMLabs.trimAtPointer = function () {
    if (!lastPtr || !window.SMEngineBridge) return false;
    var w = SMEngineBridge.screenToWorld(lastPtr[0], lastPtr[1]);
    return window.SMLabs.trimAtIntersections(w[0], w[1]);
  };

  window.SMLabs.trimAtIntersections = function (wx, wy) {
    var layer = userLayers[state.activeLayerIdx];
    if (!layer || state.layers[state.activeLayerIdx].locked) return false;
    var pt = new Point(wx, wy);
    var hit = layer.hitTest(pt, { stroke: true, tolerance: 10 / view.zoom });
    if (!hit || !(hit.item instanceof Path) || !hit.item.segments || hit.item.segments.length < 2) {
      if (typeof showToast === 'function') showToast('Aucun trait ici');
      return false;
    }
    var path = hit.item;
    if (path.data && (path.data.isVectorBrush || path.data.isFillShape || path.data.isLinkedFillCompanion || path.data.isBrushTextureCopy)) {
      if (typeof showToast === 'function') showToast('Trim : traits simples seulement (pas les rubans/fills)');
      return false;
    }
    var clickOff = path.getNearestLocation(pt).offset;
    var L = path.length;

    // Every intersection offset along THIS path: vs the layer's other
    // simple strokes, plus its own self-crossings (both branches).
    var offs = [];
    layer.children.forEach(function (o) {
      if (!(o instanceof Path) || o.segments.length < 2) return;
      if (o.data && (o.data.isLinkedFillCompanion || o.data.isBrushTextureCopy)) return;
      var ix = path.getIntersections(o);
      ix.forEach(function (loc) {
        offs.push(loc.offset);
        if (o === path && loc.intersection) offs.push(loc.intersection.offset);
      });
    });
    offs = offs.filter(function (v) { return v > 1e-6 && v < L - 1e-6; }).sort(function (a, b) { return a - b; });

    pushUndo();
    if (!offs.length) {
      // No intersections anywhere — CSP removes the whole line.
      if (path.data && path.data.linkedFill) path.data.linkedFill.remove();
      path.remove();
      saveActiveLayerFrame(); updateUI();
      if (window.SMEngineBridge) SMEngineBridge.renderNow();
      if (typeof showToast === 'function') showToast('Trait supprimé (aucune intersection)');
      return true;
    }

    var prev = 0, next = L;
    for (var i = 0; i < offs.length; i++) {
      if (offs[i] < clickOff) prev = offs[i];
      else { next = offs[i]; break; }
    }

    if (path.closed) {
      // Opening a closed path at `prev` re-bases offsets so the doomed
      // span starts at 0 — then a single second split isolates it.
      var opened = path.splitAt(path.getLocationAt(prev)) || path;
      // After splitting a closed path Paper keeps ONE open path (start =
      // split point). The span to remove is now [0, (next-prev+L)%L].
      var span = (next - prev + L) % L;
      var tail = opened.splitAt(opened.getLocationAt(span));
      opened.remove(); // the doomed span
      if (tail && tail.data && tail.data.strokeId) tail.data.strokeId = freshId();
    } else {
      // Order matters: split the far bound first so the near offset stays
      // valid on the (mutated-in-place) head path.
      var tail2 = next < L - 1e-6 ? path.splitAt(path.getLocationAt(next)) : null;
      if (prev > 1e-6) {
        var doomed = path.splitAt(path.getLocationAt(prev));
        if (doomed) doomed.remove();
      } else {
        path.remove();
      }
      if (tail2 && tail2.data && tail2.data.strokeId) tail2.data.strokeId = freshId();
    }
    saveActiveLayerFrame(); updateUI();
    if (window.SMEngineBridge) SMEngineBridge.renderNow();
    if (typeof showToast === 'function') showToast('Segment supprimé (trim aux intersections)');
    return true;
  };

  window.SMLabs.register('vector-trim', {
    flag: 'nemo-labs-trim',
    describe: 'Gomme vectorielle CSP : SMLabs.trimAtIntersections(x,y) supprime le segment du trait touché entre ses intersections avec les autres traits',
  });
})();
