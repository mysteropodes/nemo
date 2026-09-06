// @ts-check
// Pure writers for the persisted opacity track shared by layers and elements.
// This module owns no document copy; callers retain holder lookup and rendering.
var NemoOpacityDomain = (function () {
  'use strict';

  var DEFAULT_CURVE = [{ x: 0, y: 0 }, { x: 0.25, y: 0.156 }, { x: 0.5, y: 0.5 }, { x: 0.75, y: 0.844 }, { x: 1, y: 1 }];

  function number(value) { return typeof value === 'number' && isFinite(value); }
  function copyCurvePoints(points) {
    return points.map(function (point) {
      var copy = { x: point.x, y: point.y };
      if (typeof point.tx === 'number') { copy.tx = point.tx; copy.ty = point.ty || 0; }
      return copy;
    });
  }
  function defaultCurve() { return copyCurvePoints(DEFAULT_CURVE); }
  function findLayer(state, id) {
    var layers = state && state.layers || [];
    for (var i = 0; i < layers.length; i++) if (layers[i] && layers[i].layerUid === id) return layers[i];
    return null;
  }
  function snapshot(state, motion) {
    var layers = state && state.layers || [], frame = state && state.currentFrame;
    return {
      layers: layers.map(function (layer) {
        return { id: layer.layerUid, name: layer.name || '', opacity: motion.valueAtFrame(layer, 'opacity', frame)[0] };
      }),
      frame: frame
    };
  }
  function trackFor(holder) { return holder && holder.motion && holder.motion.opacity || null; }
  function isAnimated(holder) { var track = trackFor(holder); return !!(track && track.keys && track.keys.length); }
  function ensureTrack(holder) {
    if (!holder.motion) holder.motion = {};
    if (!holder.motion.opacity) holder.motion.opacity = { keys: [] };
    return holder.motion.opacity;
  }
  function keyAt(track, frame) {
    if (!track || !track.keys) return null;
    for (var i = 0; i < track.keys.length; i++) if (track.keys[i].frame === frame) return track.keys[i];
    return null;
  }
  function setKeyAtFrame(holder, frame, values, curvePoints, defaults) {
    var track = ensureTrack(holder), key = keyAt(track, frame);
    if (key) {
      key.v = values.slice();
      if (curvePoints) key.curvePoints = curvePoints;
      return key;
    }
    var curve = curvePoints || (defaults && defaults.defaultCurve ? defaults.defaultCurve() : defaultCurve());
    key = { frame: frame, v: values.slice(), curvePoints: curve, hOut: [0, 0], hIn: [0, 0] };
    track.keys.push(key);
    track.keys.sort(function (a, b) { return a.frame - b.frame; });
    return key;
  }
  function setValue(holder, values, frame, defaults) {
    if (isAnimated(holder)) { setKeyAtFrame(holder, frame, values, null, defaults); return; }
    if (!holder.motionStatic) holder.motionStatic = {};
    holder.motionStatic.opacity = values.slice();
  }
  function removeKeyAtFrame(holder, frame) {
    var track = trackFor(holder), key = keyAt(track, frame);
    if (!key) return;
    track.keys.splice(track.keys.indexOf(key), 1);
  }
  function setAnimated(holder, animated, frame, effectiveValue, defaults) {
    if (isAnimated(holder) === animated) return;
    if (!animated) {
      ensureTrack(holder).keys = [];
      if (!holder.motionStatic) holder.motionStatic = {};
      holder.motionStatic.opacity = effectiveValue.slice();
      return;
    }
    setKeyAtFrame(holder, frame, effectiveValue, null, defaults);
  }

  return {
    DEFAULT_CURVE: DEFAULT_CURVE,
    number: number,
    copyCurvePoints: copyCurvePoints,
    defaultCurve: defaultCurve,
    findLayer: findLayer,
    snapshot: snapshot,
    setValue: setValue,
    setKeyAtFrame: setKeyAtFrame,
    removeKeyAtFrame: removeKeyAtFrame,
    setAnimated: setAnimated
  };
})();
if (typeof module !== 'undefined' && module.exports) module.exports = NemoOpacityDomain;
