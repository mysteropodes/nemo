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
  // Height of the timeline's bottom chrome band, which this scrollbar is
  // centred in. Was 40px when the layer buttons lived down there too; they
  // moved into #layer-hdr (2026-07-27) so the band only has to clear the
  // bar, then both were halved on top of that ("le glisser pour zoomer la
  // timeline toute cette zone là tu peux réduire le heigth de 50%").
  // BOTTOM_BAND_PX MUST equal #layer-panel's and #fg-col's padding-bottom in
  // style.css — those two keep the panel and the grid ending at the same y,
  // and this centres the bar in whatever they leave free. BAR_H is the bar's
  // own height, used for both the CSS and that centring, so the two can't
  // drift the way a hardcoded 12 in the formula did.
  var BOTTOM_BAND_PX = 8;
  var BAR_H = 6;

  // Bug found 2026-07 ("impossible de l'amener jusqu'au bout"): with the
  // old 64px ceiling, closing the scrollbar-handle gap all the way down
  // to MIN_THUMB_PX needed an FC the cap didn't allow for any realistically
  // long project (e.g. ~145px/frame for a 120-frame timeline at a typical
  // track width — verified live, the handle stalled at ~63px thumbW,
  // exactly what FC=64 produces there) — not a math bug in the drag
  // itself, just a ceiling too low to ever let the gesture finish.
  //
  // Same bug, opposite edge (2026-08, "impossible d'étirer la barre de
  // zoom dézoom jusqu'au bout pour voir toute la timeline jusqu'à la
  // fin"): a FIXED 4px floor stalls the WIDEN-the-thumb gesture the exact
  // same way once totalFrames is long enough that even FC=4 doesn't fit
  // the whole timeline in the track — verified live at 999 frames on a
  // 529px track: FC hit the 4px floor but content stayed 3996px, thumb
  // stuck at ~70px (visRatio ~0.13) no matter how far past the bar's edge
  // the drag went. The floor now adapts to the CURRENT track width and
  // project length — low enough that "drag the handle as far as it goes"
  // can always reach thumbW===trackW (whole timeline visible, nothing
  // left to scroll) for any project length, not just ones short enough
  // for the old fixed 4 to happen to cover.
  function minFloor() {
    var wrap = wrapEl();
    var trackW = wrap ? wrap.clientWidth : 0;
    var total = Math.max(1, (state && state.totalFrames) || 1);
    if (!trackW) return 1;
    // Never ABOVE 4 (the previous fixed floor, still right for any
    // project short enough not to need going lower) — only ever relaxes
    // it further for a long one. Hard floor of 0.1: a project so long
    // that even sub-pixel-per-frame can't fit is a display limit, not
    // something to chase further (ruler labels are illegible long before
    // this anyway).
    return Math.max(0.1, Math.min(4, trackW / total));
  }
  function clamp(n) { return Math.max(minFloor(), Math.min(400, n)); }
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
    // Bug found 2026-07 ("le inpoint de la scroll bar... sort offset"):
    // `bar` used to be a plain child of #fg-wrap — the SAME element that
    // scrolls horizontally — so its own on-screen position silently
    // drifted along with the content instead of staying pinned at the
    // bottom. Harmless as long as nothing read the bar's actual screen
    // rect, but onMove below now needs an accurate one to solve the drag
    // geometry exactly. `bar` is position:fixed and appended to
    // document.body (see ensureScrollbar), so its left/top must be
    // re-synced to #fg-wrap's live bounding rect on every redraw instead
    // of being a static CSS left:0/bottom:0.
    var wrapRect = wrap.getBoundingClientRect();
    bar.style.left = wrapRect.left + 'px';
    // BELOW the grid, centred in the reserved bottom band (BOTTOM_BAND_PX,
    // matched by #layer-panel's and #fg-col's padding-bottom) — not
    // `wrapRect.bottom - 12`, which laid it over the
    // last row (2026-07-25: "double barre de scroll, une en haut avec les
    // poignées bleues alors qu'elle devrait aller à la place de celle en bas
    // en grise"). #fg-wrap stops at the separator line and both columns
    // leave that band free, so the bar reads as the timeline's bottom
    // chrome instead of covering content.
    bar.style.top = (wrapRect.bottom + Math.round((BOTTOM_BAND_PX - BAR_H) / 2)) + 'px';
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
      // The blue cap is drawn by ::after at HALF the element's width
      // (2026-07-27: "les poignée bleu… peuvent être réduite de 50% en
      // width — je parle bien visuellement des poignée bleu"). The
      // element itself keeps its full EDGE_PX so the grab zone is
      // unchanged; shrinking the element instead would have made a
      // already-6px-tall control genuinely hard to hit.
      '#tlzoom-scrollbar .tlzoom-sb-handle{background:none;}' +
      '#tlzoom-scrollbar .tlzoom-sb-handle::after{content:"";position:absolute;top:0;bottom:0;width:50%;background:rgba(74,158,255,.65);border-radius:inherit;transition:background .1s,transform .1s;}' +
      '#tlzoom-scrollbar .tlzoom-sb-handle.tlzoom-sb-left::after{left:0;}' +
      '#tlzoom-scrollbar .tlzoom-sb-handle.tlzoom-sb-right::after{right:0;}' +
      '#tlzoom-scrollbar .tlzoom-sb-handle:hover::after{background:#4ea9ff;transform:scaleX(1.15);}';
    document.head.appendChild(style);

    bar = document.createElement('div');
    bar.id = 'tlzoom-scrollbar';
    // position:fixed + appended to document.body (not `wrap`) — see
    // redrawScrollbar()'s comment: a plain absolutely-positioned child of
    // #fg-wrap scrolls away with the content it's meant to control.
    // left/top are resynced to #fg-wrap's live rect on every redraw.
    bar.style.cssText = 'position:fixed;left:0;top:0;height:' + BAR_H + 'px;z-index:60;background:rgba(255,255,255,.04);border-radius:' + (BAR_H / 2) + 'px;overflow:visible;';
    thumb = document.createElement('div');
    // Body = pan handle, neutral gray always (no color change on hover —
    // color means "this is a zoom knob", the body deliberately never uses
    // it, hover just gives the familiar grab->grabbing cursor).
    thumb.style.cssText = 'position:absolute;top:0;height:' + BAR_H + 'px;background:rgba(255,255,255,.14);border-radius:' + (BAR_H / 2) + 'px;cursor:grab;';
    var leftHandle = document.createElement('div'), rightHandle = document.createElement('div');
    [leftHandle, rightHandle].forEach(function (h, i) {
      h.className = 'tlzoom-sb-handle ' + (i === 0 ? 'tlzoom-sb-left' : 'tlzoom-sb-right');
      h.title = (window.SM && SM.t) ? SM.t('tlZoomDragTitle') : 'Drag to zoom the timeline in / out';
      h.style.cssText = 'position:absolute;top:0;width:' + EDGE_PX + 'px;height:' + BAR_H + 'px;cursor:ew-resize;border-radius:' + (i === 0 ? (BAR_H / 2) + 'px 2px 2px ' + (BAR_H / 2) + 'px' : '2px ' + (BAR_H / 2) + 'px ' + (BAR_H / 2) + 'px 2px') + ';' + (i === 0 ? 'left:0;' : 'right:0;');
      thumb.appendChild(h);
    });
    bar.appendChild(thumb);
    document.body.appendChild(bar);

    thumb.addEventListener('pointerenter', function () { if (!dragMode) thumb.style.cursor = 'grab'; });
    thumb.addEventListener('pointerdown', function () { thumb.style.cursor = 'grabbing'; });
    window.addEventListener('pointerup', function () { thumb.style.cursor = 'grab'; });

    var dragMode = null; // 'pan' | 'zoom-left' | 'zoom-right'
    var startX = 0, startScrollLeft = 0, startFC = 0, anchorTrackX = 0;

    // Current thumb geometry in TRACK px (0..trackW) — same formula
    // redrawScrollbar() uses, factored out so onDown/onMove can both
    // solve it exactly instead of approximating.
    function thumbGeometry(trackW, fc) {
      var contentW = Math.max(1, (state.totalFrames || 1) * fc);
      var visRatio = Math.min(1, trackW / contentW);
      var thumbW = Math.max(MIN_THUMB_PX, trackW * visRatio);
      var maxScroll = Math.max(1, contentW - trackW);
      var scrollRatio = Math.max(0, Math.min(1, wrapEl().scrollLeft / maxScroll));
      var thumbLeft = scrollRatio * (trackW - thumbW);
      return { thumbW: thumbW, thumbLeft: thumbLeft, thumbRight: thumbLeft + thumbW, contentW: contentW, maxScroll: maxScroll };
    }

    function onDown(e, mode) {
      e.stopPropagation(); e.preventDefault();
      dragMode = mode;
      startX = e.clientX;
      var wrap2 = wrapEl();
      startScrollLeft = wrap2.scrollLeft;
      startFC = window.FC;
      var trackW = wrap2.clientWidth;
      // Bug found 2026-07 ("le inpoint de la scroll bar... sort offset...
      // impossible de l'amener jusqu'au bout"): the old formula scaled FC
      // by an arbitrary `dx*4` heuristic with no real relationship to
      // where the handle actually rendered, so it visibly desynced from
      // the mouse and could stall well before the thumb ever reached the
      // OTHER handle. Solved exactly instead: whichever edge you're NOT
      // dragging stays pinned at its exact TRACK-space position
      // (anchorTrackX), and the dragged edge is set to match the mouse
      // pixel-for-pixel every move (onMove below) — standard resize-handle
      // behavior, so it tracks the cursor and can close the gap all the
      // way down to MIN_THUMB_PX.
      var g = thumbGeometry(trackW, startFC);
      anchorTrackX = mode === 'zoom-left' ? g.thumbRight : g.thumbLeft;
      window.addEventListener('pointermove', onMove);
      window.addEventListener('pointerup', onUp);
    }
    function onMove(e) {
      if (!dragMode) return;
      var wrap2 = wrapEl();
      var trackW = wrap2.clientWidth;
      if (dragMode === 'pan') {
        var dx = e.clientX - startX;
        var contentW = totalContentWidth();
        var maxScroll = Math.max(1, contentW - trackW);
        var thumbW = Math.max(MIN_THUMB_PX, trackW * Math.min(1, trackW / contentW));
        var scrollPerPx = maxScroll / Math.max(1, trackW - thumbW);
        wrap2.scrollLeft = Math.max(0, Math.min(maxScroll, startScrollLeft + dx * scrollPerPx));
        redrawScrollbar();
        return;
      }
      var total = Math.max(1, state.totalFrames || 1);
      var barRect = bar.getBoundingClientRect();
      var mx = Math.max(0, Math.min(trackW, e.clientX - barRect.left));
      var wantedThumbW = dragMode === 'zoom-right' ? (mx - anchorTrackX) : (anchorTrackX - mx);
      wantedThumbW = Math.max(MIN_THUMB_PX, wantedThumbW);
      var newFC = clamp((trackW * trackW) / (wantedThumbW * total));
      window.FC = newFC;
      document.documentElement.style.setProperty('--fc', newFC + 'px');
      // clamp() may have capped newFC short of what wantedThumbW asked
      // for — recompute the ACTUAL resulting thumbW from the final FC so
      // the anchor edge lands exactly right rather than drifting.
      var g = thumbGeometry(trackW, newFC);
      var newThumbLeft = dragMode === 'zoom-right' ? anchorTrackX : (anchorTrackX - g.thumbW);
      var scrollRatioNew = Math.max(0, Math.min(1, newThumbLeft / Math.max(1, trackW - g.thumbW)));
      wrap2.scrollLeft = scrollRatioNew * g.maxScroll;
      // refresh() -> renderTimeline(), which empties and recreates every
      // ruler cell and grid row (measured 27.7ms at 40 layers). onMove is
      // bound to raw window pointermove with no throttle, so a zoom-handle
      // drag queued one full rebuild per event — several per displayed
      // frame, all but the last one thrown away. Latched to one per
      // animation frame; onUp does a final unlatched refresh so the last
      // pointer position always lands. 2026-07-28.
      if (!refreshRaf) refreshRaf = requestAnimationFrame(function () { refreshRaf = 0; refresh(); });
    }
    var refreshRaf = 0;
    function onUp() {
      if (!dragMode) return;
      // Flush the latched rebuild — the drag's final position must not be
      // left sitting in a cancelled animation frame.
      if (refreshRaf) { cancelAnimationFrame(refreshRaf); refreshRaf = 0; }
      if (dragMode !== 'pan') refresh();
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
      if (typeof showToast === 'function') showToast('Zoom timeline : ' + (Math.round(v * 10) / 10) + 'px/frame');
      return v;
    },
    zoomIn: function () { return window.SMTimelineZoom.set(window.FC * 1.25); },
    zoomOut: function () { return window.SMTimelineZoom.set(window.FC / 1.25); },
    reset: function () { return window.SMTimelineZoom.set(DEFAULT_FC); },
    get: function () { return window.FC; },
    // Bug found 2026-07 ("la barre de scroll doit pouvoir aller d'un bout
    // à l'autre... pas d'offset derrière"): redrawScrollbar() sizes the
    // thumb off state.totalFrames (totalContentWidth()) but was only ever
    // called from this module's own zoom/pan/resize/scroll handlers —
    // any OTHER code path that changes totalFrames (insert/delete frames,
    // project load, etc., timeline.js/app.js) had no way to tell it the
    // content width just changed, leaving the thumb sized/positioned
    // against a stale width with a persistent gap at one edge. Exposed so
    // renderTimeline() (timeline.js) — the one function that already runs
    // after every such change — can resync it every time, same "single
    // choke point" fix as updateWaBar() above.
    redraw: function () { redrawScrollbar(); },
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
    // parseFloat, not parseInt (2026-08 fix) — the dynamic floor (minFloor)
    // can now persist a sub-1 value for a long project; truncating it back
    // to an integer on reload would silently re-zoom in past what the user
    // had actually set.
    var saved = parseFloat(localStorage.getItem(KEY));
    if (!isNaN(saved) && saved !== window.FC) apply(saved);
    else redrawScrollbar();
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', init); else init();
})();
