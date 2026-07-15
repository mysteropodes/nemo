// ---- LABS PROTOTYPE — Reference-layer fill (Clip Studio / ink & paint) ----
// THE traditional ink-and-paint workflow: line art on one layer, colors on
// another — the paint bucket on the COLOR layer must read the LINE ART
// layer's strokes as walls, which Nemo's fill (walls = active layer only)
// can't do. Action:
//
//   SMLabs.referenceFill(worldX, worldY)     — fill at a point
//   SMLabs.referenceFillAtPointer()          — same, at the pointer
//
// Mechanics: every OTHER visible layer's wall-capable strokes are cloned
// into the active layer as disposable walls for exactly one
// fillVectorFind call, then removed — the precedent is the fill tool's
// own Alt-drawn closing strokes (fillMaterializeTempCloseStrokes,
// tools.js): inserted and removed synchronously, so they never reach any
// persistent layer.children consumer. The resulting fill is an ordinary
// path in the ACTIVE layer, with the same fillSeed metadata a normal
// bucket click stores.
(function () {
  var lastPtr = null;
  document.addEventListener('pointermove', function (e) { lastPtr = [e.clientX, e.clientY]; }, true);

  window.SMLabs.referenceFillAtPointer = function () {
    if (!lastPtr || !window.SMEngineBridge) return false;
    var w = SMEngineBridge.screenToWorld(lastPtr[0], lastPtr[1]);
    return window.SMLabs.referenceFill(w[0], w[1]);
  };

  window.SMLabs.referenceFill = function (wx, wy) {
    var li = state.activeLayerIdx;
    var ld = state.layers[li];
    if (!ld || ld.locked || ld.symbolId) { if (typeof showToast === 'function') showToast('Calque actif invalide/verrouillé'); return false; }
    var layer = userLayers[li];
    var pt = new Point(wx, wy);

    // Clone every other visible layer's wall strokes in, temporarily.
    var temps = [];
    for (var i = 0; i < userLayers.length; i++) {
      if (i === li) continue;
      if (typeof layerIsEffectivelyVisible === 'function' && !layerIsEffectivelyVisible(i)) continue;
      if (state.layers[i] && state.layers[i].symbolId) continue;
      userLayers[i].children.forEach(function (c) {
        if (!(c instanceof Path) || c.segments.length < 2) return;
        if (c.data && (c.data.isLinkedFillCompanion || c.data.isBrushTextureCopy || c.data.ghostFrame !== undefined)) return;
        if (!(c.strokeColor || c.fillColor || (c.data && c.data.isVectorBrush))) return;
        var clone = c.clone({ insert: false });
        clone.data = clone.data || {};
        clone.data.isFillTempClose = true; // same disposable-wall tag as Alt closing strokes
        layer.addChild(clone);
        temps.push(clone);
      });
    }

    var res = null;
    try { res = fillVectorFind(pt, layer, null); }
    finally { temps.forEach(function (p) { p.remove(); }); }

    if (!res) { if (typeof showToast === 'function') showToast('Aucune zone fermée ici (toutes couches confondues)'); return false; }
    pushUndo();
    // A 'closedWall' result IS a wall copy derived from one of our temp
    // clones — it inherits the clone's data wholesale, including the
    // isFillTempClose tag (found live: the final fill persisted carrying
    // the disposable-wall marker). Strip it and give the fill its own id.
    if (res.path.data) { delete res.path.data.isFillTempClose; delete res.path.data.strokeId; }
    layer.insertChild(fillInsertIndexFor(layer, pt, res.path), res.path);
    res.path.fillColor = state.fillColor;
    res.path.strokeColor = null;
    res.path.opacity = state.opacity / 100;
    res.path.data.fillSeed = [pt.x, pt.y];
    res.path.data.fillGapPx = res.gapPx;
    // NOTE: no fillWalls ids — the walls came from other layers and their
    // clones are gone; fillRegenerateLinked would find nothing to re-trace
    // against, so this fill simply doesn't auto-regenerate (a real
    // adoption would persist cross-layer wall refs).
    if (typeof tagOwner === 'function') tagOwner(res.path);
    saveActiveLayerFrame(); updateUI();
    if (window.SMEngineBridge) SMEngineBridge.renderNow();
    if (typeof showToast === 'function') showToast('Fill posé (murs : toutes les couches visibles)');
    return true;
  };

  window.SMLabs.register('reference-fill', {
    flag: 'nemo-labs-reffill',
    describe: 'Ink & paint (CSP reference layer) : SMLabs.referenceFill(x,y) remplit sur le calque actif en lisant les traits de TOUS les calques visibles comme murs',
  });
})();
