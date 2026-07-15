// ---- LABS PROTOTYPE — Flip/Roll (TVPaint Flip panel) ----
// The traditional animator's "rolling": hold R and the playhead rapidly
// cycles through the frames around the current one (-2 -1 0 +1 +2 by
// default) so you FEEL the movement while drawing; release R and you're
// back exactly where you were. TVPaint exposes this as its Flip panel
// with a flips/sec setting — mirrored here:
//   SMLabs.setFlipSpeed(fps)   — cycles per second (default 6)
//   SMLabs.setFlipSpan(n)      — how many frames each side (default 2)
// Zero core touches: it only ever calls goToFrame(), the same function
// every transport button already goes through.
(function () {
  var SPEED_KEY = 'nemo-labs-flip-speed', SPAN_KEY = 'nemo-labs-flip-span';
  var rolling = false, origin = 0, seq = [], idx = 0, timer = null;

  function speed() { var n = parseFloat(localStorage.getItem(SPEED_KEY) || '6'); return (isNaN(n) || n <= 0) ? 6 : Math.min(24, n); }
  function span() { var n = parseInt(localStorage.getItem(SPAN_KEY) || '2', 10); return (isNaN(n) || n < 1) ? 2 : Math.min(6, n); }
  window.SMLabs.setFlipSpeed = function (n) { localStorage.setItem(SPEED_KEY, String(n)); return speed(); };
  window.SMLabs.setFlipSpan = function (n) { localStorage.setItem(SPAN_KEY, String(n)); return span(); };

  function start() {
    if (rolling || state.playing) return;
    rolling = true;
    origin = state.currentFrame;
    var s = span();
    // TVPaint-style roll order: sweep back over the priors, then through
    // to the followers, then repeat — reads as a little back-and-forth
    // around the pose being worked on.
    seq = [];
    for (var k = -s; k <= s; k++) {
      var f = origin + k;
      if (f >= 0 && f < state.totalFrames) seq.push(f);
    }
    if (seq.length < 2) { rolling = false; return; }
    idx = 0;
    timer = setInterval(function () {
      idx = (idx + 1) % seq.length;
      goToFrame(seq[idx]);
    }, 1000 / (speed() * 1));
  }
  function stop() {
    if (!rolling) return;
    rolling = false;
    clearInterval(timer); timer = null;
    goToFrame(origin);
  }

  document.addEventListener('keydown', function (e) {
    if (!window.SMLabs.isOn('flip-roll')) return;
    var tag = (e.target && e.target.tagName) || '';
    if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT' || (e.target && e.target.isContentEditable)) return;
    if (e.key !== 'r' && e.key !== 'R') return;
    if (e.metaKey || e.ctrlKey || e.altKey) return; // don't eat browser/app shortcuts
    // R is also the default Rectangle-tool shortcut (timeline.js
    // runToolShortcut) — without stopPropagation the core bubble-phase
    // onKeyDown still sees this same keydown and switches tools mid-roll.
    e.stopPropagation();
    if (e.repeat) { e.preventDefault(); return; } // key-repeat spam while held
    e.preventDefault();
    start();
  }, true);
  document.addEventListener('keyup', function (e) {
    if (e.key === 'r' || e.key === 'R') stop();
  }, true);
  // Safety: never leave the roll running if focus leaves the window mid-hold.
  window.addEventListener('blur', stop);

  window.SMLabs.register('flip-roll', {
    flag: 'nemo-labs-flip',
    describe: 'Rouleau d\'animateur : maintenir R fait défiler ±N frames autour de la pose, relâcher revient dessus (SMLabs.setFlipSpeed/Span)',
    onDisable: stop,
  });
})();
