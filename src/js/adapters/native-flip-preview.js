/* D24 (#2358): native Flip preview ("Feuilletage", shortcut F) -- the baseline back-and-forth browse across the work area,
 * driven through the native frame navigator instead of the legacy setInterval + Paper loadFrame body.
 *
 * Kernel semantics kept from the baseline (timeline.js SM.flipPreview, 1065e9d8): one step per interval of 1000/fps ms,
 * moving away from the starting frame toward the out point, reflecting one frame inside the work-area edge (out-1, in+1),
 * and returning to the frame it started on when toggled off. What native adds, none of which the baseline needed:
 *  - Single flight: a tick that arrives while the previous frame is still being presented is dropped (the cursor does not
 *    advance), so a slow presentation never queues a burst of stale frames (the late-tick rule of the Play scheduler).
 *  - A generation fence: a result belonging to a stopped (or restarted) run is ignored, so no stale frame is published.
 *  - Ownership: only a native owner that is active with a stable identity may flip; a replaced, released or revised
 *    document, or a refused presentation, ends the flip without restoring an obsolete frame and never reaches legacy.
 * Stateless apart from the one run it owns; it never writes the document, only asks the navigator to present a frame. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeFlipPreview = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  var PORTS = ['frame', 'fps', 'waIn', 'waOut', 'controller', 'navigate', 'setInterval', 'clearInterval', 'notify'];
  // One flip step from `cur` heading `dir` over [wa, wo]. A work area of fewer than two frames has nothing to flip: it
  // stays on `wa` (the baseline stepped to wa+1, outside a one-frame range).
  function flipStep(cur, dir, wa, wo) {
    if (!(wo > wa)) return { frame: wa, dir: 1 };
    var frame = cur + dir;
    if (frame > wo) { dir = -1; frame = wo - 1; }
    if (frame < wa) { dir = 1; frame = wa + 1; }
    if (frame > wo) frame = wo;
    return { frame: frame, dir: dir };
  }
  function sameIdentity(a, b) {
    return !!a && !!b && a.instanceId === b.instanceId && a.documentId === b.documentId &&
      a.contentRevision === b.contentRevision;
  }
  function validIdentity(id) {
    return !!id && typeof id.instanceId === 'string' && !!id.instanceId &&
      typeof id.documentId === 'string' && !!id.documentId &&
      Number.isSafeInteger(id.contentRevision) && id.contentRevision >= 0;
  }
  function createFlipPreview(ports) {
    if (!ports || PORTS.some(function (name) { return typeof ports[name] !== 'function'; })) {
      throw new TypeError('native flip preview ports are unavailable');
    }
    var generation = 0, timer = null, owner = null, identity = null, origin = 0, cur = 0, dir = 1,
      wa = 0, wo = 0, pending = false;
    function ownerCurrent() {
      try { return !!owner && ports.controller() === owner && owner.isActive() === true &&
        sameIdentity(identity, owner.identity()); } catch (_) { return false; }
    }
    function release() {
      var id = timer;
      ++generation; timer = null; owner = null; identity = null; pending = false;
      if (id !== null) { try { ports.clearInterval(id); } catch (_) { /* The run is already fenced. */ } }
    }
    function tick(run) {
      if (run !== generation) return;
      if (!ownerCurrent()) { release(); return; }
      if (pending) return;
      var step = flipStep(cur, dir, wa, wo);
      cur = step.frame; dir = step.dir; pending = true;
      var presented;
      try { presented = Promise.resolve(ports.navigate(cur)); }
      catch (_) { pending = false; release(); return; }
      presented.then(function (ok) {
        if (run !== generation) return;
        pending = false;
        if (ok !== true) release();
      }, function () { if (run === generation) { pending = false; release(); } });
    }
    function start(candidate) {
      var initial, fps = ports.fps(), from = ports.waIn(), to = ports.waOut();
      try { initial = candidate && candidate.isActive() === true && candidate.identity(); } catch (_) { initial = null; }
      if (ports.controller() !== candidate || !validIdentity(initial) || !Number.isFinite(fps) || fps <= 0) {
        ports.notify('Native playback is unavailable');
        return false;
      }
      if (!Number.isInteger(from) || !Number.isInteger(to) || !(to > from)) return false; // Nothing to flip.
      // Flip and Play are mutually exclusive (optional ports): starting a flip stops a running Play first, so the Play
      // scheduler's stop-on-unpresented rule cannot end the flip and the flip never fights a running Play.
      try { if (typeof ports.playing === 'function' && ports.playing() === true) ports.stopPlay(); }
      catch (_) { /* Play's own stop path owns its failures. */ }
      owner = candidate; identity = initial; wa = from; wo = to; origin = cur = ports.frame(); dir = 1; pending = false;
      var run = ++generation;
      try { timer = ports.setInterval(function () { tick(run); }, 1000 / fps); }
      catch (_) { release(); ports.notify('Native playback is unavailable'); return false; }
      return true;
    }
    // Back to the frame the flip started on, unless the owner changed meanwhile (an obsolete frame is never restored).
    function stop() {
      var restore = ownerCurrent(), back = origin;
      release();
      return restore ? Promise.resolve(ports.navigate(back)) : Promise.resolve(false);
    }
    // Ends the run without presenting anything (a Play that starts next continues from the frame on screen; restoring the
    // origin here would race its first presentation). The generation fence drops any in-flight flip result.
    function cancel() { var was = timer !== null; release(); return was; }
    function toggle(candidate) { return timer !== null ? stop() : Promise.resolve(start(candidate)); }
    return Object.freeze({ toggle: toggle, start: start, stop: stop, cancel: cancel, active: function () { return timer !== null; } });
  }
  return Object.freeze({ flipStep: flipStep, createFlipPreview: createFlipPreview });
}));
