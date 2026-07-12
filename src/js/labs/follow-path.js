// ---- LABS PROTOTYPE — Follow path baking (Moho) ----
// Moho animates a layer along a drawn path; Nemo's Labs version BAKES it:
// select the strokes to move (Select tool), then call with the strokeId
// of the trajectory stroke — the selection is stamped onto each frame of
// the range, translated along the path (eased), flipbook-style:
//
//   1. Draw a trajectory stroke, note its id via SMLabs.lastStrokeIdAt(x,y)
//      (or just select it alone and call SMLabs.selectedStrokeId())
//   2. Select the strokes to animate
//   3. SMLabs.followPath({ pathId, frames: 12, ease: 'inout',
//                          removeTrajectory: true })
//
// Pure data-level baking: per frame, deep-copied serP snapshots translated
// by (pathPoint(t) - pathPoint(0)) with fresh strokeIds — every frame is
// an ordinary keyframe, one pushUndoLayers reverts the whole bake. The
// trajectory stroke itself is removed from the drawing when
// removeTrajectory is true (it's a guide, not artwork).
(function () {
  var EASES = {
    linear: function (t) { return t; },
    'in': function (t) { return t * t; },
    out: function (t) { return 1 - (1 - t) * (1 - t); },
    inout: function (t) { return t < 0.5 ? 2 * t * t : 1 - 2 * (1 - t) * (1 - t); },
  };
  function freshId() { return 'labs_' + Date.now().toString(36) + '_' + Math.floor(Math.random() * 1e6); }

  window.SMLabs.selectedStrokeId = function () {
    if (typeof selectedPaths === 'undefined' || selectedPaths.length !== 1) return null;
    var p = selectedPaths[0];
    return p && p.data ? p.data.strokeId || null : null;
  };
  window.SMLabs.lastStrokeIdAt = function (wx, wy) {
    var layer = userLayers[state.activeLayerIdx];
    var hit = layer.hitTest(new Point(wx, wy), { stroke: true, fill: true, tolerance: 10 / view.zoom });
    return hit && hit.item && hit.item.data ? hit.item.data.strokeId || null : null;
  };

  window.SMLabs.followPath = function (opts) {
    opts = opts || {};
    var li = state.activeLayerIdx;
    var ld = state.layers[li];
    if (!ld || ld.locked || ld.symbolId) { if (typeof showToast === 'function') showToast('Calque invalide/verrouillé'); return 0; }
    var layer = userLayers[li];
    var traj = null;
    layer.children.forEach(function (c) { if (c instanceof Path && c.data && c.data.strokeId === opts.pathId) traj = c; });
    if (!traj) { if (typeof showToast === 'function') showToast('Trajectoire introuvable (pathId)'); return 0; }
    if (typeof selectedPaths === 'undefined' || !selectedPaths.length) { if (typeof showToast === 'function') showToast('Sélectionne les traits à animer (outil Sélection)'); return 0; }
    var targets = selectedPaths.filter(function (p) { return p && p.parent && p !== traj; });
    if (!targets.length) { if (typeof showToast === 'function') showToast('Sélection vide (la trajectoire ne compte pas)'); return 0; }
    var nFrames = Math.max(2, Math.min(120, opts.frames || 12));
    var fn = EASES[opts.ease || 'inout'] || EASES.inout;
    var cf = state.currentFrame;
    var L = traj.length;
    var p0 = traj.getPointAt(0);

    // Snapshot targets (and their linked fills) BEFORE any mutation.
    var snaps = [];
    targets.forEach(function (p) {
      snaps.push(serP(p));
      if (p.data && p.data.linkedFill && p.data.linkedFill.parent) snaps.push(serP(p.data.linkedFill));
    });
    var baseFrame = JSON.parse(JSON.stringify(getEffectiveStrokes(li, cf)));
    // The moving copies replace the originals along the way — strip the
    // originals (and the guide) from the baked background so each frame
    // shows ONE character on the path, not a frozen twin at the start.
    var movingIds = {};
    targets.forEach(function (p) {
      if (p.data && p.data.strokeId) movingIds[p.data.strokeId] = true;
      if (p.data && p.data.linkedFill && p.data.linkedFill.data && p.data.linkedFill.data.strokeId) movingIds[p.data.linkedFill.data.strokeId] = true;
    });
    var backdrop = baseFrame.filter(function (sd) {
      if (sd.strokeId && movingIds[sd.strokeId]) return false;
      if (opts.removeTrajectory !== false && sd.strokeId === opts.pathId) return false;
      return true;
    });

    pushUndoLayers();
    saveAllLayerFrames();
    var needed = cf + nFrames;
    if (needed > state.totalFrames) {
      var add = needed - state.totalFrames;
      for (var l = 0; l < state.layers.length; l++) { for (var a = 0; a < add; a++) state.layers[l].frames.push({ strokes: [], isKeyframe: false, isInterpolated: false }); }
      state.totalFrames = needed; window._totalF = needed;
      if (state.waOut < needed - 1) { state.waOut = needed - 1; window._waOut = state.waOut; }
    }
    for (var k = 0; k < nFrames; k++) {
      var t = fn(k / (nFrames - 1));
      var pt = traj.getPointAt(Math.min(L, t * L));
      var dx = pt.x - p0.x, dy = pt.y - p0.y;
      var frameStrokes = JSON.parse(JSON.stringify(backdrop));
      snaps.forEach(function (sd) {
        var copy = JSON.parse(JSON.stringify(sd));
        if (copy.strokeId) copy.strokeId = freshId();
        delete copy.brushGroupId;
        if (copy.segments) copy.segments.forEach(function (seg) {
          seg.point[0] += dx; seg.point[1] += dy;
          // handles are relative in serP form — only points move.
        });
        frameStrokes.push(copy);
      });
      ld.frames[cf + k] = { strokes: frameStrokes, isKeyframe: true, isInterpolated: false };
    }
    if (typeof clearSel === 'function') clearSel();
    loadFrame(state.currentFrame); renderOS(); renderArcs(); updateUI();
    if (typeof renderTimeline === 'function') renderTimeline();
    if (typeof showToast === 'function') showToast('Follow path : ' + nFrames + ' frames bakées le long de la trajectoire');
    return nFrames;
  };

  window.SMLabs.register('follow-path', {
    flag: 'nemo-labs-followpath',
    describe: 'Follow path (Moho) : SMLabs.followPath({pathId,frames,ease}) bake la sélection le long d\'un trait-trajectoire en keyframes successives',
  });
})();
