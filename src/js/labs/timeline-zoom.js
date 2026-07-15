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
//
// Custom scrollbar (feedback: "j'arrive à zoomer mais pas dézoomer [au
// Ctrl+molette], la barre du bas... pourrait avoir un in/out point au
// hover bleu que si on les drag ça permet d'ajuster le zoom aussi") —
// same Premiere/Resolve pattern: drag the THUMB BODY to pan, drag either
// EDGE to resize the thumb, which zooms (a wider thumb = more of the
// timeline visible at once = smaller FC). #fg-wrap's native scrollbar
// (`overflow:auto`) can't have custom handles injected into it — it's
// hidden and replaced with a real DOM bar so the edges can be grabbed.
// This also sidesteps the wheel-gesture bug entirely: it's a direct
// mouse drag, not dependent on interpreting a trackpad/wheel deltaY sign
// (isolated wheel-event testing found the zoom math itself symmetric in
// both directions — the reported one-way failure looks gesture/OS-level,
// not a logic bug — so a mouse-drag control is the more robust fix
// regardless of that root cause).
(function () {
  var KEY = 'nemo-labs-fc';
  var DEFAULT_FC = 16;

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
  // Feedback: "la scroll bar compliqué à comprendre il faudrait que ça
  // soit les bouts qui soit des poignées bleu pour zoomer ou dézoomer
  // pas l'ensemble de la barre" — the first version only changed color on
  // hover of the WHOLE thumb, so the 8px zoom-edge zones were invisible
  // and indistinguishable from the pan-the-body zone. Fixed: the body
  // stays a neutral gray pan handle at all times; the two end caps are
  // now visually separate ALWAYS-blue pill-shaped knobs (not just on
  // hover), independently brightening only THEMSELVES on hover — the
  // body never tints blue anymore, only the knobs do.
  var hideNativeStyle = null, bar = null, thumb = null;
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
    thumb.classList.toggle('lbs-tlsb-full', visRatio >= 0.999);
  }

  function ensureScrollbar() {
    if (bar) return;
    var wrap = wrapEl();
    if (!wrap) return;
    hideNativeStyle = document.createElement('style');
    hideNativeStyle.textContent =
      '#fg-wrap{scrollbar-width:none;-ms-overflow-style:none;}' +
      '#fg-wrap::-webkit-scrollbar{display:none;}' +
      '#labs-tlscrollbar .lbs-tlsb-full{opacity:.5;}' +
      '#labs-tlscrollbar .lbs-tlsb-handle{background:rgba(74,158,255,.65);transition:background .1s,transform .1s;}' +
      '#labs-tlscrollbar .lbs-tlsb-handle:hover{background:#4ea9ff;transform:scaleX(1.15);}';
    document.head.appendChild(hideNativeStyle);

    bar = document.createElement('div');
    bar.id = 'labs-tlscrollbar';
    bar.style.cssText = 'position:absolute;left:0;bottom:0;height:12px;z-index:6;background:rgba(255,255,255,.04);border-radius:6px;overflow:visible;';
    thumb = document.createElement('div');
    // Body = pan handle, neutral gray always (no color change on hover —
    // color now means "this is a zoom knob", so the body deliberately
    // never uses it, hover just gives the familiar grab→grabbing cursor).
    thumb.style.cssText = 'position:absolute;top:0;height:12px;background:rgba(255,255,255,.14);border-radius:6px;cursor:grab;';
    var leftHandle = document.createElement('div'), rightHandle = document.createElement('div');
    [leftHandle, rightHandle].forEach(function (h, i) {
      h.className = 'lbs-tlsb-handle';
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
    var startX = 0, startScrollLeft = 0, startFC = 0, anchorContentX = 0;

    function onDown(e, mode) {
      e.stopPropagation(); e.preventDefault();
      dragMode = mode;
      startX = e.clientX;
      var wrap2 = wrapEl();
      startScrollLeft = wrap2.scrollLeft;
      startFC = window.FC;
      // The content-space x-coordinate under the EDGE NOT being dragged —
      // kept visually fixed while resizing, exactly like Premiere's
      // timeline zoom-by-scrollbar-edge behavior.
      var trackW = wrap2.clientWidth;
      if (mode === 'zoom-left') anchorContentX = startScrollLeft + trackW; // right edge stays put
      else if (mode === 'zoom-right') anchorContentX = startScrollLeft; // left edge stays put
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
      // Zoom: dragging the right edge OUTWARD (dx>0) widens the thumb →
      // more content fits in the same track width → ZOOM OUT (smaller
      // FC). Dragging it inward zooms in. The left edge is the mirror.
      var sign = dragMode === 'zoom-right' ? 1 : -1;
      var newFC = clamp(startFC * trackW / Math.max(20, trackW + sign * dx * 4));
      window.FC = newFC;
      document.documentElement.style.setProperty('--fc', newFC + 'px');
      // Re-anchor so the edge NOT being dragged stays visually still.
      var newContentW = totalContentWidth();
      if (dragMode === 'zoom-left') wrap2.scrollLeft = Math.max(0, Math.min(newContentW - trackW, anchorContentX - trackW));
      else wrap2.scrollLeft = Math.max(0, Math.min(newContentW - trackW, anchorContentX));
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
  function teardownScrollbar() {
    if (bar) { bar.remove(); bar = null; thumb = null; }
    if (hideNativeStyle) { hideNativeStyle.remove(); hideNativeStyle = null; }
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
    describe: 'Zoom de la timeline : Ctrl/Cmd+molette au-dessus de la grille, scrollbar custom en bas (glisser le corps = pan, glisser un bord = zoom), ou SMLabs.setTimelineZoom(px)/zoomTimelineIn/Out/resetTimelineZoom — mute FC (JS) + --fc (CSS), les deux déjà lus en direct partout, aucun fichier core touché',
    onEnable: function () {
      ensureScrollbar();
      var saved = parseInt(localStorage.getItem(KEY), 10);
      if (!isNaN(saved) && saved !== window.FC) apply(saved);
      else redrawScrollbar();
    },
    onDisable: function () { apply(DEFAULT_FC); teardownScrollbar(); },
  });
  // Resume the saved zoom level after a page reload if the flag was left on.
  window.addEventListener('load', function () {
    if (!window.SMLabs.isOn('timeline-zoom')) return;
    ensureScrollbar();
    var saved = parseInt(localStorage.getItem(KEY), 10);
    if (!isNaN(saved)) apply(saved);
  });
})();
