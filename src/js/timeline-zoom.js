// ---- TIMELINE ZOOM (2026-07) ----
// Ported from the experimental/feature-scouting branch's Labs prototype
// (src/js/labs/timeline-zoom.js, 3 commits: initial Ctrl+wheel zoom, the
// custom drag-to-zoom scrollbar, then its visual polish) — validated there
// ("Verified live" in all three commit messages), promoted here as a normal
// always-on module instead of staying behind that branch's opt-in
// SMLabs flag system.
//
// TVPaint/Harmony/Premiere-style: Ctrl/Cmd+wheel over the timeline (or
// dragging the custom scrollbar's blue end-cap HANDLES) widens/narrows
// every frame cell, so a long scene can be scrubbed at a glance or a tight
// beat spread out for frame-precise work.
//
// The entire timeline already keys its pixel math off TWO things that are
// already live-read (never captured/baked at load time):
//   - JS:  `FC` (app.js, `var FC=16` — module-global, read fresh by every
//     hit-test/drag/position calculation across timeline.js, ui.js,
//     camera.js, audio-bridge.js, motion.js, layer-inout.js)
//   - CSS: `--fc` (style.css :root, drives .fhc/.fc/#playhead widths)
// Zooming is therefore just: mutate both, then ask the existing render
// functions to re-lay-out with the new value — no core file edit needed,
// every existing drag/hit-test keeps working unmodified.
//
//   SMTimelineZoom.set(px)      — direct cell width (clamped 4..64)
//   SMTimelineZoom.zoomIn()/Out() — ×1.25 / ÷1.25 around current value
//   SMTimelineZoom.reset()      — back to the app's default (30px)
//   Ctrl/Cmd + wheel over the timeline also zooms
//
// DEFAULT_FC bumped 16->30 (2026-07, "il faudrait réajuster le zoom... pour
// qu'il soit proche de celui de la capture") — 16px read as cramped next to
// the reference screenshot (~30px/frame measured from its ruler spacing).
// Only affects a session with no persisted localStorage value yet, or an
// explicit reset() — FC itself is a single shared global (confirmed live:
// switching Animation2D<->Motion never changes it), so there was never a
// per-mode divergence to fix, just a default worth raising for both.
//
// Custom scrollbar: drag the THUMB BODY (neutral gray) to pan, drag either
// blue end-cap HANDLE to resize the thumb, which zooms (a wider thumb =
// more of the timeline visible at once = smaller FC) — Premiere/Resolve
// convention. #fg-wrap's native scrollbar (`overflow:auto`) can't have
// custom handles injected into it, so it's hidden and replaced with a real
// DOM bar. The body never tints blue (color is reserved to mean "this is a
// zoom knob") — only the two end-cap handles do, always visible, not just
// on hover, per feedback that a whole-thumb color change hid where the
// zoom-vs-pan zones actually were.
(function () {
  var KEY = 'nemo-timeline-fc';
  var KEY_VER = 'nemo-timeline-fc-ver';
  var CUR_VER = '2'; // bump whenever DEFAULT_FC changes and existing persisted values should be migrated once
  var DEFAULT_FC = 30;

  function clamp(n) { return Math.max(4, Math.min(64, Math.round(n))); }
  function refresh() {
    if (typeof renderTimeline === 'function') renderTimeline();
    if (typeof updatePlayhead === 'function') updatePlayhead();
    if (window.updateWaBar) window.updateWaBar();
    if (window.SMAudio && SMAudio.renderStrip) SMAudio.renderStrip();
    redrawScrollbar();
  }
  function apply(px) {
    px = clamp(px);
    window.FC = px;
    document.documentElement.style.setProperty('--fc', px + 'px');
    localStorage.setItem(KEY, String(px));
    refresh();
    return px;
  }

  // ---- custom zoom scrollbar ----
  var bar = null, thumb = null;
  var EDGE_PX = 10, MIN_THUMB_PX = 28;

  function wrapEl() { return document.getElementById('fg-wrap'); }
  function totalContentWidth() { return Math.max(1, (state.totalFrames || 1) * window.FC); }

  function redrawScrollbar() {
    if (!bar) return;
    var wrap = wrapEl();
    if (!wrap) return;
    var trackW = wrap.clientWidth;
    var contentW = totalContentWidth();
    var visRatio = Math.min(1, trackW / contentW);
    var thumbW = Math.max(MIN_THUMB_PX, trackW * visRatio);
    var maxScroll = Math.max(1, contentW - trackW);
    var scrollRatio = Math.max(0, Math.min(1, wrap.scrollLeft / maxScroll));
    var thumbLeft = scrollRatio * (trackW - thumbW);
    bar.style.width = trackW + 'px';
    thumb.style.width = thumbW + 'px';
    thumb.style.left = thumbLeft + 'px';
    // Fully zoomed out (everything visible, nothing to scroll) — thumb
    // fills the track and dragging its body would have nothing to pan;
    // dim it to communicate that instead of leaving a dead control.
    thumb.classList.toggle('tlzoom-sb-full', visRatio >= 0.999);
  }

  function ensureScrollbar() {
    if (bar) return;
    var wrap = wrapEl();
    if (!wrap) return;
    var style = document.createElement('style');
    style.textContent =
      '#fg-wrap{scrollbar-width:none;-ms-overflow-style:none;}' +
      '#fg-wrap::-webkit-scrollbar{display:none;}' +
      '#tlzoom-scrollbar .tlzoom-sb-full{opacity:.5;}' +
      '#tlzoom-scrollbar .tlzoom-sb-handle{background:rgba(74,158,255,.65);transition:background .1s,transform .1s;}' +
      '#tlzoom-scrollbar .tlzoom-sb-handle:hover{background:#4ea9ff;transform:scaleX(1.15);}';
    document.head.appendChild(style);

    bar = document.createElement('div');
    bar.id = 'tlzoom-scrollbar';
    bar.style.cssText = 'position:absolute;left:0;bottom:0;height:12px;z-index:6;background:rgba(255,255,255,.04);border-radius:6px;overflow:visible;';
    thumb = document.createElement('div');
    // Body = pan handle, neutral gray always (no color change on hover —
    // color means "this is a zoom knob", the body deliberately never uses
    // it, hover just gives the familiar grab->grabbing cursor).
    thumb.style.cssText = 'position:absolute;top:0;height:12px;background:rgba(255,255,255,.14);border-radius:6px;cursor:grab;';
    var leftHandle = document.createElement('div'), rightHandle = document.createElement('div');
    [leftHandle, rightHandle].forEach(function (h, i) {
      h.className = 'tlzoom-sb-handle';
      h.title = 'Glisser pour zoomer / dézoomer la timeline';
      h.style.cssText = 'position:absolute;top:0;width:' + EDGE_PX + 'px;height:12px;cursor:ew-resize;border-radius:' + (i === 0 ? '6px 2px 2px 6px' : '2px 6px 6px 2px') + ';' + (i === 0 ? 'left:0;' : 'right:0;');
      thumb.appendChild(h);
    });
    bar.appendChild(thumb);
    wrap.appendChild(bar);

    thumb.addEventListener('pointerenter', function () { if (!dragMode) thumb.style.cursor = 'grab'; });
    thumb.addEventListener('pointerdown', function () { thumb.style.cursor = 'grabbing'; });
    window.addEventListener('pointerup', function () { thumb.style.cursor = 'grab'; });

    var dragMode = null; // 'pan' | 'zoom-left' | 'zoom-right'
    var startX = 0, startScrollLeft = 0, startFC = 0, anchorFrame = 0, anchorScreenX = 0;

    function onDown(e, mode) {
      e.stopPropagation(); e.preventDefault();
      dragMode = mode;
      startX = e.clientX;
      var wrap2 = wrapEl();
      startScrollLeft = wrap2.scrollLeft;
      startFC = window.FC;
      // Bug found 2026-07 ("la scrollbar doit pouvoir se réduire en
      // direction du curseur de temps des 2 côtés sinon perd le fil de là
      // où on est"): used to anchor whichever EDGE wasn't being dragged
      // (Premiere's own convention) — but that could scroll the playhead
      // straight out of view if it happened to sit near the edge you WERE
      // dragging. Anchoring to the PLAYHEAD's own screen position instead:
      // regardless of which handle you drag, the current frame stays
      // visually still, so you never lose track of where you are.
      var trackW = wrap2.clientWidth;
      anchorFrame = state.currentFrame;
      anchorScreenX = (anchorFrame * startFC) - startScrollLeft;
      window.addEventListener('pointermove', onMove);
      window.addEventListener('pointerup', onUp);
    }
    function onMove(e) {
      if (!dragMode) return;
      var wrap2 = wrapEl();
      var trackW = wrap2.clientWidth;
      var dx = e.clientX - startX;
      if (dragMode === 'pan') {
        var contentW = totalContentWidth();
        var maxScroll = Math.max(1, contentW - trackW);
        var thumbW = Math.max(MIN_THUMB_PX, trackW * Math.min(1, trackW / contentW));
        var scrollPerPx = maxScroll / Math.max(1, trackW - thumbW);
        wrap2.scrollLeft = Math.max(0, Math.min(maxScroll, startScrollLeft + dx * scrollPerPx));
        redrawScrollbar();
        return;
      }
      // Zoom: dragging the right edge OUTWARD (dx>0) widens the thumb ->
      // more content fits in the same track width -> ZOOM OUT (smaller
      // FC). Dragging it inward zooms in. The left edge is the mirror.
      var sign = dragMode === 'zoom-right' ? 1 : -1;
      var newFC = clamp(startFC * trackW / Math.max(20, trackW + sign * dx * 4));
      window.FC = newFC;
      document.documentElement.style.setProperty('--fc', newFC + 'px');
      // Re-anchor so the PLAYHEAD stays at the same screen position.
      var newContentW = totalContentWidth();
      var newAnchorContentX = anchorFrame * newFC;
      wrap2.scrollLeft = Math.max(0, Math.min(newContentW - trackW, newAnchorContentX - anchorScreenX));
      refresh();
    }
    function onUp() {
      if (!dragMode) return;
      dragMode = null;
      thumb.style.cursor = 'grab';
      localStorage.setItem(KEY, String(window.FC));
      window.removeEventListener('pointermove', onMove);
      window.removeEventListener('pointerup', onUp);
    }
    thumb.addEventListener('pointerdown', function (e) {
      if (e.target === leftHandle) onDown(e, 'zoom-left');
      else if (e.target === rightHandle) onDown(e, 'zoom-right');
      else onDown(e, 'pan');
    });
    wrap.addEventListener('scroll', function () { if (!dragMode) redrawScrollbar(); });
    window.addEventListener('resize', redrawScrollbar);
    redrawScrollbar();
  }

  window.SMTimelineZoom = {
    set: function (px) {
      var v = apply(px);
      if (typeof showToast === 'function') showToast('Zoom timeline : ' + v + 'px/frame');
      return v;
    },
    zoomIn: function () { return window.SMTimelineZoom.set(window.FC * 1.25); },
    zoomOut: function () { return window.SMTimelineZoom.set(window.FC / 1.25); },
    reset: function () { return window.SMTimelineZoom.set(DEFAULT_FC); },
    get: function () { return window.FC; },
  };

  function inTimeline(el) {
    return !!(el && el.closest && (el.closest('#frame-grid') || el.closest('#frame-hdr') || el.closest('#fg-wrap')));
  }
  document.addEventListener('wheel', function (e) {
    if (!(e.ctrlKey || e.metaKey)) return;
    if (!inTimeline(e.target)) return;
    e.preventDefault();
    // deltaY < 0 (scroll up / pinch out) = zoom in, matches every app's convention.
    apply(window.FC * (e.deltaY < 0 ? 1.08 : 1 / 1.08));
  }, { passive: false });

  function init() {
    ensureScrollbar();
    // Bug found 2026-07 ("tu n'as pas ajusté le zoom quand on repasse sur
    // l'animation 2D"): DEFAULT_FC alone only affects a session with NO
    // persisted value yet — an existing session already had its OLD
    // (cramped, 16px) FC saved, so raising the default silently changed
    // nothing for anyone who'd already used the app once. Version-stamp
    // the persisted key so this forces exactly ONE migration to the new
    // default; any zoom the user picks afterward is respected normally.
    if (localStorage.getItem(KEY_VER) !== CUR_VER) {
      localStorage.setItem(KEY_VER, CUR_VER);
      apply(DEFAULT_FC);
      return;
    }
    var saved = parseInt(localStorage.getItem(KEY), 10);
    if (!isNaN(saved) && saved !== window.FC) apply(saved);
    else redrawScrollbar();
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', init); else init();
})();
