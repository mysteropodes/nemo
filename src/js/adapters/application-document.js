/* Browser adapter. This is the only v1 module allowed to know legacy globals. */
(function (root) {
  'use strict';
  function create() {
    var incarnation = 'doc_' + Date.now().toString(36) + '_' + Math.random().toString(36).slice(2, 8);
    function state() { return root.state; }
    function find(id) {
      var layers = state().layers || [];
      for (var i = 0; i < layers.length; i++) if (layers[i].layerUid === id) return layers[i];
      return null;
    }
    function keys(layer) {
      var track = root.SMMotion && root.SMMotion.trackFor(layer, 'opacity');
      return track && track.keys ? track.keys.map(function (key) { return { frame: key.frame, value: key.v[0] }; }) : [];
    }
    return {
      documentId: function () { return incarnation; },
      totalFrames: function () { return state().totalFrames; },
      layer: find,
      isAnimated: function (layer) { return keys(layer).length > 0; },
      snapshot: function (revision) {
        var s = state();
        return {
          documentId: incarnation, revision: revision, frame: s.currentFrame, fps: s.fps,
          totalFrames: s.totalFrames, context: { symbolId: s.activeSymbolId || null, montageViewId: s.activeMontageViewId || null },
          layers: (s.layers || []).map(function (layer) {
            return { id: layer.layerUid, name: layer.name, opacity: root.SMMotion.valueAtFrame(layer, 'opacity', s.currentFrame)[0], animated: keys(layer).length > 0, keys: keys(layer) };
          })
        };
      },
      beginHistory: function () { root.pushUndo(); },
      wouldSetOpacity: function (layer, value, frame, forceKeyframe) {
        var targetFrame = frame === undefined ? state().currentFrame : frame;
        var current = root.SMMotion.valueAtFrame(layer, 'opacity', targetFrame)[0];
        return current !== value || (forceKeyframe && !root.SMMotion.keyAt(root.SMMotion.trackFor(layer, 'opacity'), targetFrame));
      },
      setOpacity: function (layer, value, frame, forceKeyframe) {
        var targetFrame = frame === undefined ? state().currentFrame : frame;
        var current = root.SMMotion.valueAtFrame(layer, 'opacity', targetFrame)[0];
        if (!this.wouldSetOpacity(layer, value, targetFrame, forceKeyframe)) return false;
        if (forceKeyframe || this.isAnimated(layer)) root.SMMotion.setKeyAtFrame(layer, 'opacity', targetFrame, [value]);
        else root.SMMotion.setValue(layer, 'opacity', [value]);
        return true;
      },
      refresh: function () {
        if (root.renderLayerList) root.renderLayerList();
        if (root.renderTimeline) root.renderTimeline();
        if (root.SMEngineBridge) root.SMEngineBridge.renderNow();
      },
      undo: function () {
        var s = state(), before = s.undoStack.length; root.undo();
        return { changed: s.undoStack.length !== before, blocked: s.undoStack.length === before && before > 0 };
      },
      redo: function () {
        var s = state(), before = s.redoStack.length; root.redo();
        return { changed: s.redoStack.length !== before, blocked: s.redoStack.length === before && before > 0 };
      }
    };
  }
  root.SMApplicationDocument = { create: create };
}(typeof window !== 'undefined' ? window : globalThis));
