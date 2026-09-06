// The layer-opacity aggregate deliberately owns no copy of a document.  It
// adapts the persisted layer record and the long-standing SMMotion track
// shape, so save/load, the renderer, and exporters keep reading one value.
(function (root) {
  'use strict';
  function number(value) {
    return typeof value === 'number' && isFinite(value);
  }
  function layerId(layer, index) {
    if (!layer.layerUid) layer.layerUid = 'layer-' + (index + 1);
    return layer.layerUid;
  }
  function findLayer(state, id) {
    for (var i = 0; i < state.layers.length; i++) {
      if (layerId(state.layers[i], i) === id) return state.layers[i];
    }
    return null;
  }
  function snapshot(state, motion) {
    return {
      layers: state.layers.map(function (layer, index) {
        var id = layerId(layer, index);
        return { id: id, name: layer.name || '', opacity: motion.valueAtFrame(layer, 'opacity', state.currentFrame)[0] };
      }),
      frame: state.currentFrame
    };
  }
  root.NemoOpacityDomain = { number: number, findLayer: findLayer, snapshot: snapshot };
})(window);
