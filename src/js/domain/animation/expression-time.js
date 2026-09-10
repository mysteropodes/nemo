// @ts-check
// Pure time-conversion math for Motion expression evaluation (frame<->second,
// step-time snapping). Callers own the mutable evaluation context (_ectx in
// motion.js) and fps resolution; this module only computes.
var NemoExpressionTimeDomain = (function () {
  'use strict';

  function stepTime(frame, everyNFrames) {
    var n = Number(everyNFrames);
    if (!isFinite(n) || n <= 0) return frame;
    return Math.floor(frame / n) * n;
  }
  function toFrames(seconds, fps) {
    return seconds * fps;
  }
  function toSeconds(frames, fps) {
    return fps === 0 ? 0 : frames / fps;
  }

  return { stepTime: stepTime, toFrames: toFrames, toSeconds: toSeconds };
})();
if (typeof module !== 'undefined' && module.exports) module.exports = NemoExpressionTimeDomain;
