// ---- LABS PROTOTYPE — Timeline zoom (every pro 2D app has this) ----
// TVPaint/Harmony/Premiere-style: Ctrl+scroll over the timeline (or
// SMLabs.setTimelineZoom(px)) widens/narrows every frame cell, so a long
// scene can be scrubbed at a glance or a tight beat can be spread out for
// frame-precise work.
//
// The entire timeline already keys its pixel math off TWO things that
// happen to already be a live-read global var and a live-read CSS custom
// property, never captured/baked at load time:
//   - JS:  `FC` (app.js, `var FC=16` — module-global, not const, read
//     fresh by every hit-test/drag/position calculation across
//     timeline.js, ui.js, camera.js, audio-bridge.js)
//   - CSS: `--fc` (style.css :root, drives .fhc/.fc/#playhead widths)
// Zooming is therefore just: mutate both, then ask the existing render
// functions to re-lay-out with the new value — no core file edit, no new
// DOM structure, every existing drag/hit-test keeps working unmodified
// because it was already reading FC live, not from a closure.
//
//   SMLabs.setTimelineZoom(px)      — direct cell width (clamped 4..64)
//   SMLabs.zoomTimelineIn()/Out()   — ×1.25 / ÷1.25 around current value
//   SMLabs.resetTimelineZoom()      — back to the app's default (16px)
//   Ctrl/Cmd + wheel over the timeline (while flag on) also zooms
(function () {
  var KEY = 'nemo-labs-fc';
  var DEFAULT_FC = 16;

  function clamp(n) { return Math.max(4, Math.min(64, Math.round(n))); }
  function refresh() {
    if (typeof renderTimeline === 'function') renderTimeline();
    if (typeof updatePlayhead === 'function') updatePlayhead();
    if (window.updateWaBar) window.updateWaBar();
    if (window.SMAudio && SMAudio.renderStrip) SMAudio.renderStrip();
  }
  function apply(px) {
    px = clamp(px);
    window.FC = px;
    document.documentElement.style.setProperty('--fc', px + 'px');
    localStorage.setItem(KEY, String(px));
    refresh();
    return px;
  }

  window.SMLabs.setTimelineZoom = function (px) {
    var v = apply(px);
    if (typeof showToast === 'function') showToast('Zoom timeline : ' + v + 'px/frame');
    return v;
  };
  window.SMLabs.zoomTimelineIn = function () { return window.SMLabs.setTimelineZoom(window.FC * 1.25); };
  window.SMLabs.zoomTimelineOut = function () { return window.SMLabs.setTimelineZoom(window.FC / 1.25); };
  window.SMLabs.resetTimelineZoom = function () { return window.SMLabs.setTimelineZoom(DEFAULT_FC); };
  window.SMLabs.getTimelineZoom = function () { return window.FC; };

  function inTimeline(el) {
    return !!(el && el.closest && (el.closest('#frame-grid') || el.closest('#frame-hdr') || el.closest('#fg-wrap')));
  }
  document.addEventListener('wheel', function (e) {
    if (!window.SMLabs.isOn('timeline-zoom')) return;
    if (!(e.ctrlKey || e.metaKey)) return;
    if (!inTimeline(e.target)) return;
    e.preventDefault();
    // deltaY < 0 (scroll up / pinch out) = zoom in, matches every app's convention.
    apply(window.FC * (e.deltaY < 0 ? 1.08 : 1 / 1.08));
  }, { passive: false });

  window.SMLabs.register('timeline-zoom', {
    flag: 'nemo-labs-tlzoom',
    describe: 'Zoom de la timeline (TVPaint/Harmony/Premiere) : Ctrl/Cmd+molette au-dessus de la grille, ou SMLabs.setTimelineZoom(px)/zoomTimelineIn/Out/resetTimelineZoom — mute FC (JS) + --fc (CSS), les deux déjà lus en direct partout, aucun fichier core touché',
    onEnable: function () {
      var saved = parseInt(localStorage.getItem(KEY), 10);
      if (!isNaN(saved) && saved !== window.FC) apply(saved);
    },
    onDisable: function () { apply(DEFAULT_FC); },
  });
  // Resume the saved zoom level after a page reload if the flag was left on.
  window.addEventListener('load', function () {
    if (!window.SMLabs.isOn('timeline-zoom')) return;
    var saved = parseInt(localStorage.getItem(KEY), 10);
    if (!isNaN(saved)) apply(saved);
  });
})();
