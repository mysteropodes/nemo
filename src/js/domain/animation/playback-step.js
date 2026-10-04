// @ts-check
// One logical playback transition. The caller owns the clock, document,
// audio and mutable direction; this module only decides the next frame.
var NemoPlaybackStep = (function () {
  'use strict';

  function advance(frame, direction, workIn, workOut, loop, pingPong) {
    var next = frame + direction;
    if (next > workOut) {
      if (loop && pingPong) {
        direction = -1;
        next = frame - 1;
        if (next < workIn) next = workIn;
      } else if (loop) {
        next = workIn;
        return { frame: next, direction: direction, audioLoop: true };
      } else {
        return { frame: null, direction: direction, audioLoop: false };
      }
    } else if (next < workIn) {
      if (loop && pingPong) {
        direction = 1;
        next = frame + 1;
        if (next > workOut) next = workOut;
      } else {
        return { frame: null, direction: direction, audioLoop: false };
      }
    }
    return { frame: next, direction: direction, audioLoop: false };
  }

  return { advance: advance };
})();
if (typeof module !== 'undefined' && module.exports) module.exports = NemoPlaybackStep;
