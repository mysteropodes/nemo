// ---- LAYER IN/OUT POINT (v1, 2026-07) ----
// After-Effects-style per-layer visibility range on the main timeline: a
// layer with no explicit ld.inPoint/outPoint spans the whole project (unset
// = full range — every existing project keeps its exact old behavior with
// zero migration, see the header comment on getEffectiveStrokes, app.js).
// The actual render gate lives there (single shared choke point for both
// the live path via loadFrame() and the export path). This file is UI-only:
// the draggable bar + two resize handles, one instance per layer row, in
// BOTH Animation 2D's frame grid (.frow, timeline.js) and Motion mode's
// collapsed layer row (motion.js) — a layer's visible range is a layer-
// level concept, not specific to either timeline view.
(function () {
  // Delegates to the shared globals in app.js (layerInPoint/layerOutPoint) —
  // layerOutPoint's own default now auto-detects a blank-keyframe tail
  // (see its header comment there) instead of always the full timeline;
  // keeping ONE definition avoids the two drifting apart.
  function inPointOf(ld) { return window.layerInPoint ? layerInPoint(ld) : (ld.inPoint != null ? ld.inPoint : 0); }
  function outPointOf(ld) { return window.layerOutPoint ? layerOutPoint(ld) : (ld.outPoint != null ? ld.outPoint : state.totalFrames - 1); }
  // Parent-in-Time makes ld.inPoint/outPoint completely inert on a linked
  // edge — layerInPoint/layerOutPoint (app.js) check resolveLinkedTime
  // FIRST and return its result unconditionally when the edge is linked,
  // never even reading ld.inPoint/outPoint. Every drag handler below used
  // to write straight to those fields regardless, so dragging a linked
  // child's bar visibly did nothing (found live, Cyril: "les enfants du
  // parent in time... on ne peut pas les drag"). This redirects the SAME
  // drag math to timeLinkInOffset/timeLinkOutOffset instead — the ACTUAL
  // value resolveLinkedTime reads — so the bar genuinely follows the
  // cursor. `effectiveValue` is what the caller wants this edge to END UP
  // AT (identical to what it would have assigned to ld.inPoint/outPoint);
  // this converts that into the offset from the link source's CURRENT
  // in/out that produces the same effective result. Only the edge(s) the
  // link's own mode actually covers are redirected — mirrors
  // resolveLinkedTime's own per-edge mode check exactly, so (e.g.) an
  // 'in'-only link still lets the OUT handle drag normally. Returns true
  // when it handled the write (caller skips its own ld.inPoint/outPoint
  // assignment for that edge); false means the edge isn't linked and the
  // caller's normal write should proceed unchanged.
  function trySetLinkedEdge(ld, which, effectiveValue) {
    if (!ld.timeLink) return false;
    var mode = ld.timeLink.mode || 'both';
    if (which === 'in' && mode === 'out') return false;
    if (which === 'out' && mode === 'in') return false;
    var src = window.timeLinkSourceOf ? timeLinkSourceOf(ld) : null;
    if (!src) return false;
    if (!window.SMMotion || !window.SMMotion.setLayerValue) return false;
    // Cross-type source (2026-08-16) — same srcAnchor fallback as
    // resolveLinkedTime (app.js) and setLayerTimeLink (motion.js); absent
    // for every pre-existing/same-type link, so this changes nothing for
    // those.
    var srcWhich = (ld.timeLink.srcAnchor === 'in' || ld.timeLink.srcAnchor === 'out') ? ld.timeLink.srcAnchor : which;
    var base = srcWhich === 'in' ? inPointOf(src) : outPointOf(src);
    var prop = which === 'in' ? 'timeLinkInOffset' : 'timeLinkOutOffset';
    var li = state.layers.indexOf(ld);
    SMMotion.setLayerValue(li, prop, [effectiveValue - base]);
    return true;
  }
  // Live keyframe-diamond preview during a drag (2026-07-30, Cyril: "les clé
  // ne bouge pas en temp réel avec calque ou in/outpoint") — mirrors (a
  // deliberately simplified, "good enough for a preview" copy of) the
  // retime decision the mouseup handler makes for real a few hundred lines
  // down, but only calls into motion.js's transform-only preview, never the
  // actual data commit. `type==='out'` never retimes the layer-wide default
  // (mirrors mouseup's own toast: "Rogner la fin ne déplace jamais les
  // keyframes"), so Alt on an out-drag must NOT invert it — same guard
  // shape as mouseup's `d.type !== 'out'` inside the ternary condition.
  function livePreviewLayerKeys(li, type, ld, origIn, altKey) {
    if (!window.SMMotion || !SMMotion.previewKeyframeShift) return;
    var defaultRetimes = type !== 'out' && (type === 'both' || !!state.activeSymbolId);
    var retimes = (altKey && type !== 'out') ? !defaultRetimes : defaultRetimes;
    if (!retimes) return;
    SMMotion.previewKeyframeShift(li, inPointOf(ld) - origIn, 'layer');
  }
  function livePreviewSelectedKeys(dxFrames) {
    if (!window.SMMotion || !SMMotion.previewKeyframeShift) return;
    SMMotion.previewKeyframeShift(null, dxFrames, 'selected');
  }
  // A manually-dragged range OR an auto-detected blank-keyframe trim both
  // count as "not full range" for styling — a naturally-shortened bar
  // (layer stops drawing partway through) should read as visually distinct
  // from the full-timeline default too, not just a manual drag.
  // `!=null` on ld.inPoint (not `||`, the old check): an explicit drag of
  // the in-point handle back to exactly frame 0 (overriding a non-zero
  // auto-detected default) is falsy but still a genuine customization —
  // the old truthy check silently treated it as "no custom range", so the
  // bar lost its color tint the instant a user dragged the in-point to 0
  // (same class of bug as layerInPoint's own `!=null` fix, app.js).
  function hasCustomRange(ld) { return !!(ld.inPoint != null || ld.outPoint != null || outPointOf(ld) < state.totalFrames - 1); }

  // Every layer bar is colored with that layer's OWN assigned color, not
  // one uniform accent tint — makes a busy timeline with many layers
  // scannable at a glance. `ld.color` is already assigned to every layer
  // at creation (app.js nextLayerColor).
  function hexToRgba(hex, alpha) {
    hex = (hex || '#3F6BF5').replace('#', '');
    if (hex.length === 3) hex = hex.split('').map(function (c) { return c + c; }).join('');
    var r = parseInt(hex.slice(0, 2), 16), g = parseInt(hex.slice(2, 4), 16), b = parseInt(hex.slice(4, 6), 16);
    return 'rgba(' + r + ',' + g + ',' + b + ',' + alpha + ')';
  }
  function updateBar(row, li) {
    var ld = state.layers[li]; if (!ld) return;
    var bar = row.querySelector('.layer-inout-bar'); if (!bar) return;
    var inF = inPointOf(ld), outF = outPointOf(ld);
    bar.style.left = (inF * FC) + 'px';
    bar.style.width = Math.max(FC, (outF - inF + 1) * FC) + 'px';
    var custom = hasCustomRange(ld);
    bar.classList.toggle('full-range', !custom);
    bar.classList.toggle('has-timelink', !!ld.timeLink);
    // Layer-color tint ALWAYS applied now (2026-07-17, explicit request:
    // "il faut que ça couleur soit déjà appliqué quoi qu'il arrive" — a
    // layer previously only showed its own color once trimmed, reading as
    // "you have to move the layer to see its color"). Reverses the
    // 2026-07 decision below the full-range case used to plainly re-tell:
    // a fully neutral bar was chosen back then so a busy timeline didn't
    // tint EVERY row at a glance — kept here as a WEAKER tint (0.22 vs
    // 0.55 alpha) rather than dropped outright, so the "stands out because
    // it's trimmed" signal survives alongside the always-on color.
    bar.style.background = hexToRgba(ld.color, custom ? 0.55 : 0.22);
    bar.style.borderColor = hexToRgba(ld.color, custom ? 0.95 : 0.5);
    // Handle is a uniform white pill now regardless of layer color
    // (2026-08-16 restyle, nemo-timeline-inout-spec.html) — the bar itself
    // still carries the per-layer tint above; the handle's per-layer
    // --io-handle-color override (previously needed to win the CSS cascade
    // over :hover/.sel on a trimmed bar) is retired along with it, since the
    // handle no longer varies by layer color at all.
    // Animation 2D's per-frame .fc cells (Motion mode's collapsed spacer row
    // has none — nothing to dim there, the bar alone is enough context).
    var cells = row.querySelectorAll('.fc');
    if (cells.length) {
      cells.forEach(function (c) {
        var f = parseInt(c.dataset.frame, 10);
        c.classList.toggle('io-dim', f < inF || f > outF);
      });
    }
    renderKeyTicks(bar, row, li, ld, inF, outF);
    renderContentGaps(bar, ld, inF, outF);
    // Every renderTimeline() rebuilds row/bar/handle DOM from scratch
    // (buildBar runs again), so the edge-specific 'sel' highlight needs
    // reapplying here on every updateBar call too, not just once at the
    // initial build — otherwise a re-render after a drag/selection change
    // would silently drop it.
    applySelClasses(bar, selPartOf(li));
  }

  // ---- gaps for genuinely blank stretches inside the bar (2026-07:
  // "une timeline dans animation 2D avec des keyframe vide doivent
  // s'afficher comme ça dans motion" — Animation 2D's own frame grid marks
  // a blank keyframe with a HOLLOW dot and visibly stops drawing until the
  // next real one; Motion's bar used to render that whole stretch as one
  // solid, continuous block, hiding that a real gap existed in there) ----
  // Purely a rendering addition — inPoint/outPoint (the actual visibility
  // gate, getEffectiveStrokes) and the bar's drag hit-area are UNCHANGED,
  // this only draws dimmed overlays + hollow boundary dots over stretches
  // with no content, same "blank" test as autoIn/OutPointFromBlankKeyframe
  // (app.js): a keyframe is blank iff isKeyframe && !strokes.length, held/
  // tween frames between real keyframes don't affect the running state.
  function renderContentGaps(bar, ld, inF, outF) {
    var old = bar.querySelectorAll('.layer-inout-seg-gap, .layer-inout-segdot');
    for (var o = 0; o < old.length; o++) old[o].remove();
    if (!ld.frames) return;
    var segs = [], curStart = null, curBlank = true;
    for (var f = inF; f <= outF; f++) {
      var fr = ld.frames[f];
      if (!fr || !fr.isKeyframe) continue;
      var hasContent = !!(fr.strokes && fr.strokes.length);
      if (hasContent) {
        if (curBlank) curStart = f;
        curBlank = false;
      } else {
        if (!curBlank && curStart != null) { segs.push({ start: curStart, end: f }); curStart = null; }
        curBlank = true;
      }
    }
    if (!curBlank && curStart != null) segs.push({ start: curStart, end: outF });
    if (segs.length < 2) return; // one continuous stretch (or none) — the plain bar already reads correctly

    function addGap(gf0, gf1) {
      var gap = document.createElement('div'); gap.className = 'layer-inout-seg-gap';
      gap.style.left = ((gf0 - inF) * FC) + 'px';
      gap.style.width = Math.max(1, (gf1 - gf0) * FC) + 'px';
      bar.appendChild(gap);
    }
    var cursor = inF;
    segs.forEach(function (seg) {
      if (seg.start > cursor) addGap(cursor, seg.start);
      cursor = seg.end;
    });
    if (cursor < outF) addGap(cursor, outF);
    // Hollow dot at each segment's END (the blank keyframe that closes it)
    // — its START already gets the normal filled tick from renderKeyTicks
    // above, matching Animation 2D's own full/hollow dot convention
    // instead of drawing a redundant second marker there.
    segs.forEach(function (seg) {
      var dot = document.createElement('div'); dot.className = 'layer-inout-segdot';
      dot.style.left = ((seg.end - inF) * FC) + 'px';
      dot.title = window.SM.t('layerInoutEmptyKeyTitle').replace('{n}', seg.end + 1);
      bar.appendChild(dot);
    });
  }

  // ---- per-keyframe tick marks inside the bar (the "chantier", 2026-07:
  // "les traits violet au milieu... correspondent à d'autres keyframes
  // pleines que l'on peut modifier dans le temps de ce calque" / linking
  // Animation 2D's frame content to Motion's bar) ----
  // Animation 2D's `ld.frames[i]` (isKeyframe + real strokes = a "full"
  // keyframe, per CLAUDE.md's frame model) was completely invisible from
  // Motion's collapsed bar until now — the bar only ever showed the
  // in/out VISIBILITY range, with zero connection to where the layer's
  // actual drawn keyframes sit in time. One tick per full keyframe within
  // the visible [inF,outF] range, positioned relative to the bar's own
  // left edge (bar is itself the positioning context for its handles
  // already, see style.css) so it stays aligned as the bar gets trimmed.
  // Rebuilt on every updateBar call, same "rebuild from scratch" idiom as
  // everything else here — cheap at realistic keyframe counts and avoids
  // any stale-tick bookkeeping.
  function renderKeyTicks(bar, row, li, ld, inF, outF) {
    var old = bar.querySelectorAll('.layer-inout-key');
    for (var o = 0; o < old.length; o++) old[o].remove();
    if (!ld.frames) return;
    for (var f = inF; f <= outF; f++) {
      var fr = ld.frames[f];
      if (!fr || !fr.isKeyframe || !fr.strokes || !fr.strokes.length) continue;
      (function (frameIdx) {
        var tick = document.createElement('div');
        tick.className = 'layer-inout-key';
        tick.style.left = ((frameIdx - inF) * FC) + 'px';
        tick.title = window.SM.t('layerInoutKeyTitle').replace('{n}', frameIdx + 1);
        bar.appendChild(tick);
        tick.addEventListener('mousedown', function (e) { onKeyDown(li, row, bar, tick, frameIdx, e); });
      })(f);
    }
  }
  // Dragging a tick retimes ONE keyframe via window.SM.moveKeyframe
  // (timeline.js) — the SAME data mutation Animation 2D's own frame
  // content lives in (ld.frames), not a Motion-only copy, so the change
  // is visible back in Animation 2D's frame grid the moment you switch
  // modes (both views render straight off ld.frames, nothing to sync).
  // Deliberately a SEPARATE drag singleton from _drag above (not folded
  // into its group/in/out state machine): a keyframe retime is always a
  // single tick, never a group operation, and keeping it decoupled avoids
  // any risk of regressing the in/out drag logic that was just fixed.
  var _keyDrag = null; // {li, row, bar, tickEl, origFrame, startX, previewFrame}
  function onKeyDown(li, row, bar, tickEl, frame, e) {
    e.stopPropagation(); e.preventDefault();
    var ld = state.layers[li]; if (!ld || ld.locked) return;
    // undo step is taken inside moveKeyframe itself, once, on drop — not per mousemove
    _keyDrag = { li: li, row: row, bar: bar, tickEl: tickEl, origFrame: frame, startX: e.clientX, previewFrame: frame };
  }
  document.addEventListener('mousemove', function (e) {
    if (!_keyDrag) return;
    var ld = state.layers[_keyDrag.li]; if (!ld) { _keyDrag = null; return; }
    var dx = Math.round((e.clientX - _keyDrag.startX) / FC);
    var inF = inPointOf(ld), outF = outPointOf(ld);
    var nf = Math.max(inF, Math.min(outF, _keyDrag.origFrame + dx));
    _keyDrag.previewFrame = nf;
    _keyDrag.tickEl.style.left = ((nf - inF) * FC) + 'px';
  });
  document.addEventListener('mouseup', function () {
    if (!_keyDrag) return;
    var kd = _keyDrag; _keyDrag = null;
    if (kd.previewFrame !== kd.origFrame && window.SM && window.SM.moveKeyframe) {
      window.SM.moveKeyframe(kd.li, kd.origFrame, kd.previewFrame);
    }
    updateBar(kd.row, kd.li); // re-render ticks even if the move was a no-op (snaps back)
  });

  // ---- multi-select (marquee rectangle) across several layers' bars ----
  // Feedback: "impossible de rect + drag sélection sur les inpoint et
  // outpoint pour les drag ensemble" — same marquee-then-group-drag pattern
  // motion.js already has for keyframes, applied here to layer bars: drag a
  // rectangle down the layer column to select several layers' bars at
  // once, then drag any ONE of them to shift the whole group's in/out
  // range together (relative spacing preserved).
  var _barSel = []; // [{li, part:'in'|'out'|'both'}] — which EDGE(S) of each layer's bar are selected
  function findSelEntry(li) { for (var i = 0; i < _barSel.length; i++) if (_barSel[i].li === li) return _barSel[i]; return null; }
  function isBarSelected(li) { return !!findSelEntry(li); }
  function selPartOf(li) { var s = findSelEntry(li); return s ? s.part : null; }
  // 'both' highlights the whole bar (existing white outline); 'in'/'out'
  // highlight ONLY that specific handle — feedback: "si on drag sur une
  // partie du calque alors ça select le in ou out point seulement", a
  // marquee that only touches one edge of a bar shouldn't visually claim
  // the whole thing is selected.
  function applySelClasses(bar, part) {
    bar.classList.toggle('sel', part === 'both');
    var hleft = bar.querySelector('.layer-inout-handle.left');
    var hright = bar.querySelector('.layer-inout-handle.right');
    // 2026-08-16 fix (Cyril, live: "les in out/point des layers... disparaissent
    // si je les select avec le rec box, il faudrait que celui ci s'assombrisse")
    // — a whole-bar marquee pick (part==='both') used to leave BOTH handles in
    // their default light/white pill colour while the bar itself grew a white
    // .sel box-shadow ring right up against them — a light pill sitting flush
    // against a white ring reads as gone, not merely unselected. Darkening
    // both handles whenever the WHOLE bar is selected (in addition to the
    // existing single-edge darkening) keeps them visible against that ring,
    // and is the more honest signal anyway: a whole-bar selection DOES cover
    // both edges.
    if (hleft) hleft.classList.toggle('sel', part === 'in' || part === 'both');
    if (hright) hright.classList.toggle('sel', part === 'out' || part === 'both');
  }
  var _marquee = null; // {startX, startY, rectEl, moved}
  function startMarquee(e) {
    var rect = document.createElement('div'); rect.className = 'layer-inout-marquee-rect';
    document.body.appendChild(rect);
    _marquee = { startX: e.clientX, startY: e.clientY, rectEl: rect, moved: false };
  }
  // Bug found 2026-07 ("le drag + rect de selection prend toute la largeur
  // faut vraiment que ça suivent la souris de manière classique"): this
  // used to be a Y-ONLY band — left/right pinned to the viewport edges
  // regardless of where the cursor actually was, X never even read. A real
  // rectangle now tracks both axes and hit-tests against each BAR's own
  // rendered rect (not the row's, which is full-width) — true 2D overlap,
  // exactly the classic click-drag box every other timeline/DAW uses.
  //
  // Follow-up ("si on drag sur une partie du calque alors ça select le in
  // ou out point seulement, comme sur le calque violet maquette que j'ai
  // fait"): a bar hit by the marquee is now tagged with WHICH part it was
  // hit on — only the left half touched -> 'in', only the right half ->
  // 'out', spanning across (or the whole bar) -> 'both'. Judged against
  // the OVERLAP region's own midpoint, not the marquee rect's own bounds,
  // so a wide marquee that only grazes one edge of a narrow bar still
  // reads as a single-edge selection.
  function partForOverlap(barRect, ox0, ox1) {
    var mid = (barRect.left + barRect.right) / 2;
    var touchesLeft = ox0 <= mid, touchesRight = ox1 >= mid;
    if (touchesLeft && !touchesRight) return 'in';
    if (touchesRight && !touchesLeft) return 'out';
    return 'both';
  }
  function applyMarqueeSelection(x0, y0, x1, y1) {
    var sel = [];
    document.querySelectorAll('.layer-inout-bar').forEach(function (bar) {
      var row = bar.parentElement; if (!row) return;
      var li = _rowLayerIdx.get(row); if (li == null) return;
      var b = bar.getBoundingClientRect();
      var hit = b.left <= x1 && b.right >= x0 && b.top <= y1 && b.bottom >= y0;
      var part = null;
      if (hit) {
        part = partForOverlap(b, Math.max(x0, b.left), Math.min(x1, b.right));
        sel.push({ li: li, part: part });
      }
      applySelClasses(bar, part);
    });
    _barSel = sel;
  }
  function updateMarquee(e) {
    if (!_marquee) return;
    if (Math.abs(e.clientX - _marquee.startX) > 3 || Math.abs(e.clientY - _marquee.startY) > 3) _marquee.moved = true;
    var x0 = Math.min(_marquee.startX, e.clientX), x1 = Math.max(_marquee.startX, e.clientX);
    var y0 = Math.min(_marquee.startY, e.clientY), y1 = Math.max(_marquee.startY, e.clientY);
    var r = _marquee.rectEl;
    r.style.left = x0 + 'px'; r.style.top = y0 + 'px'; r.style.width = (x1 - x0) + 'px'; r.style.height = (y1 - y0) + 'px';
    if (_marquee.moved) {
      applyMarqueeSelection(x0, y0, x1, y1);
      // Motion mode: the same rectangle ALSO selects property keyframes
      // ("c'est possible d'avoir le drag de sélection avant de select les
      // clés à partir de n'importe où dans la timeline") — this marquee is
      // the one that starts from bar rows and the empty grid space, so
      // without the forward a key-selection drag only worked when started
      // inside a property track's own cells (motion.js's own marquee).
      if (state.appMode === 'motion' && window.SMMotion && SMMotion.marqueeSelect) SMMotion.marqueeSelect(x0, y0, x1, y1);
    }
  }
  function endMarquee() {
    if (!_marquee) return;
    var moved = _marquee.moved;
    _marquee.rectEl.remove();
    _marquee = null;
    if (!moved) {
      clearBarSel(); // plain click on empty space clears selection
      if (state.appMode === 'motion' && window.SMMotion && SMMotion.clearKeySelection) { SMMotion.clearKeySelection(); if (window.renderTimeline) renderTimeline(); }
    }
  }
  function clearBarSel() {
    _barSel = [];
    document.querySelectorAll('.layer-inout-bar').forEach(function (bar) { applySelClasses(bar, null); });
  }
  function refreshBarSelClasses() {
    document.querySelectorAll('.layer-inout-bar').forEach(function (bar) {
      var row = bar.parentElement, rli = row && _rowLayerIdx.get(row);
      applySelClasses(bar, rli != null ? selPartOf(rli) : null);
    });
  }
  // Shift/Cmd/Ctrl-click toggles one layer's bar in/out of the selection —
  // feedback: "je ne peux pas select 2 inpoint de calques différents ou
  // plus". The rectangle marquee above only starts from a row's EMPTY
  // background (`e.target===row`), but a layer's bar defaults to the FULL
  // timeline range (unset inPoint/outPoint = full range, see header
  // comment) — an untrimmed bar covers the entire row width with zero
  // empty pixels left to click, so the marquee could never even start on
  // the common case. A modifier-click toggle works regardless of bar
  // width AND lets non-adjacent layers join one selection (a rectangle is
  // inherently contiguous), matching the standard multi-select convention
  // every other timeline/keyframe editor uses alongside rectangle-select.
  // Always tags 'both' (a modifier-click is a whole-bar toggle, not tied
  // to any rectangle geometry) — only the marquee produces edge-scoped
  // 'in'/'out' selections.
  function toggleBarSel(li) {
    var idx = -1;
    for (var i = 0; i < _barSel.length; i++) if (_barSel[i].li === li) { idx = i; break; }
    if (idx >= 0) _barSel.splice(idx, 1); else _barSel.push({ li: li, part: 'both' });
    refreshBarSelClasses();
  }
  // Maps a bar's row element to/from its layer index — buildBar populates
  // both below. The marquee only has DOM hit-test info (needs row->li); a
  // group-drag needs the reverse (li->row) to update each member's bar.
  var _rowLayerIdx = new WeakMap();
  var _liToRow = {};

  // Drag state is a single module-level singleton (same idiom as ui.js's
  // initWaDrag: window-level mousemove/mouseup, not a per-drag add/remove
  // pair) — only one bar (or group) can be dragged at a time anyway.
  var _drag = null; // {li, row, type:'in'|'out'|'both', startX, origIn, origOut} | {group:true, type:'in'|'out'|'both', startX, members:[{li,row,origIn,origOut}]}
  // Keyframes selected when a bar drag STARTS travel with it — the same
  // rectangle now selects bars and keys together (see layer-inout's own
  // marquee forward), so "move the in point and take these keys along" is
  // the natural next gesture. Captured at mousedown because the drag
  // re-renders the grid, and a re-render rebuilds the key diamonds.
  function keySelNow() {
    return (window.SMMotion && SMMotion.getKeySelection) ? SMMotion.getKeySelection() : [];
  }
  // Shift-range vs Ctrl/Cmd-toggle (2026-07-30 fix, Cyril: "avec shift on
  // doit pouvoir select plusieurs calques, in out point, et avec ctrl aussi
  // pour sauter des calque") — previously shiftKey/metaKey/ctrlKey all did
  // the exact same thing (toggleBarSel, a plain membership flip), so Shift
  // could only ever build up a selection one bar at a time same as Ctrl,
  // never grab a contiguous run in one click like the layer-list's own
  // Shift-click (motion.js row handler) already does. _barAnchorLi tracks
  // the last EXPLICITLY bar-clicked layer — set on every plain click/toggle,
  // left untouched by a Shift-click itself so a run of Shift-clicks grows
  // or shrinks the range from the SAME fixed end rather than drifting.
  var _barAnchorLi = null;
  function onDown(li, row, type, e) {
    e.stopPropagation(); e.preventDefault();
    var ld = state.layers[li]; if (!ld) return;
    if (e.metaKey || e.ctrlKey) { // toggle one, no drag
      toggleBarSel(li); _barAnchorLi = li;
      // Mirror into _layerSel — without this the bar-built selection never
      // reached the layer list's highlight or its _layerSel-gated menu items
      // (2026-07-31 fix; the reverse direction, syncBarSelToLayerSel, was
      // wired one commit before these modifier branches existed).
      if (window.SMMotion && SMMotion.syncLayerSelFromBarSel) SMMotion.syncLayerSelFromBarSel(_barSel, li);
      return;
    }
    if (e.shiftKey) { // contiguous range from the anchor to this bar, no drag
      var anchorLi = (_barAnchorLi != null && state.layers[_barAnchorLi]) ? _barAnchorLi : li;
      var lo = Math.min(anchorLi, li), hi = Math.max(anchorLi, li);
      var rangeSel = [];
      for (var rk = lo; rk <= hi; rk++) rangeSel.push({ li: rk, part: 'both' });
      _barSel = rangeSel;
      refreshBarSelClasses();
      if (window.SMMotion && SMMotion.syncLayerSelFromBarSel) SMMotion.syncLayerSelFromBarSel(_barSel, anchorLi);
      return;
    }
    if (window.SMMotion && SMMotion.clearKeyframeShiftPreview) SMMotion.clearKeyframeShiftPreview(); // never inherit a prior drag's leftover offset
    if (window.pushUndo) pushUndo(); // one undo step for the whole drag, not one per mousemove
    // Flush any live canvas content into ld.frames BEFORE this drag starts
    // touching ld.inPoint/outPoint — mousemove below updates those live,
    // per frame (so the visible range shrinks WHILE dragging), which makes
    // getEffectiveStrokes' in/out gate hide the source frame's content
    // from the canvas for the REST of the drag. Saving now, while the
    // canvas still faithfully reflects the undragged state, is what lets
    // shiftLayerFrames (app.js/timeline.js, called at drop for a retiming
    // drag) safely skip re-deriving from a canvas the drag itself has
    // since made unreliable (bug found live: content was vanishing
    // entirely instead of moving — see shiftLayerFrames' own comment).
    if (window.saveAllLayerFrames) saveAllLayerFrames();
    // Drag state (2026-08-16, nemo-timeline-inout-spec.html): "conserve
    // l'état survol tant que le bouton est enfoncé, même hors zone" — the
    // handle actually grabbed (e.target for a handle mousedown, since the
    // listener is bound directly to hleft/hright) gets a .hot class the
    // :hover CSS rule also matches, so it stays lit even once the cursor
    // drags past its small hitbox. Cleared at mouseup below. null for a
    // bar-BODY drag (type==='both' from the bar itself, not a handle) —
    // nothing to highlight there.
    var handleEl = (type === 'in' || type === 'out') ? e.target : null;
    if (handleEl) handleEl.classList.add('hot');
    // Grabbing ANY part of an already-selected bar (body OR an edge handle)
    // moves/trims the whole group together — feedback: "je ne peux pas
    // select les inpoint ou outpoint avec le rect de select + drag" (an
    // earlier version restricted grouping to body-drags only, on the
    // mistaken assumption edge handles should always stay single-bar; that
    // was wrong, group-trimming in/out points together is exactly the
    // point of the marquee select here).
    if (isBarSelected(li) && _barSel.length > 1) {
      var members = _barSel.map(function (s) {
        var mld = state.layers[s.li], mrow = _liToRow[s.li];
        if (!mld || !mrow) return null;
        // part travels with each member (2026-07-30 fix, Cyril: "pas
        // possible de faire bouger un in point d'un calque avec un
        // outpoint d'un autre... quand ils sont tous les 2 select") — a
        // marquee can tag different bars with different parts ('in' on
        // one, 'out' on another, 'both' on a third); the drag below now
        // moves each member according to ITS OWN part instead of forcing
        // every member to follow whichever single handle was physically
        // grabbed to start the gesture.
        return { li: s.li, row: mrow, origIn: inPointOf(mld), origOut: outPointOf(mld), part: s.part || 'both' };
      }).filter(Boolean);
      // pressLi: which bar was actually clicked, kept alongside the group so
      // a plain (no-modifier, no-move) click can narrow the selection down
      // to just this one layer at mouseup — see that check's own comment.
      _drag = { group: true, type: type, startX: e.clientX, members: members, alt: !!e.altKey, keySel: keySelNow(), pressLi: li, hotEl: handleEl };
      return;
    }
    _drag = { li: li, row: row, type: type, startX: e.clientX, origIn: inPointOf(ld), origOut: outPointOf(ld), alt: !!e.altKey, keySel: keySelNow(), hotEl: handleEl };
  }
  // Parent in Time (2026-07-30 on-timeline connector) — a dragged bar's OWN
  // position is kept live via updateBar (cheap, see its neighboring comment
  // below), but a layer TIME-LINKED to whatever's being dragged resolves
  // its in/out FROM the dragged layer's CURRENT position (resolveLinkedTime,
  // app.js) — without an equally live update here, its bar visibly froze
  // until the drag ended and the one full renderTimeline() below finally
  // ran. Found live: "l'autre calque ne bouge pas en temps réel pendant le
  // drag du parent." Walks transitively (a chain of links), not just direct
  // children, same small guard bound resolveLinkedTime itself uses for
  // cycle-safety — still just a handful of cheap updateBar calls, not a
  // full rebuild.
  function updateLinkedChildrenBars(sourceLi) {
    var srcLd = state.layers[sourceLi];
    var srcUid = (srcLd && window.SMMotion && window.SMMotion.ensureLayerUid) ? SMMotion.ensureLayerUid(srcLd) : null;
    if (!srcUid) return;
    state.layers.forEach(function (l2, li2) {
      if (!l2.timeLink || !l2.timeLink.uid) return;
      var cur = l2, guard = 0, found = false;
      while (cur && cur.timeLink && cur.timeLink.uid && guard++ < 16) {
        if (cur.timeLink.uid === srcUid) { found = true; break; }
        var next = null;
        state.layers.forEach(function (o) { if (o.layerUid === cur.timeLink.uid) next = o; });
        cur = next;
      }
      if (!found) return;
      var row = _liToRow[li2];
      if (row) updateBar(row, li2);
    });
  }
  document.addEventListener('mousemove', function (e) {
    updateMarquee(e);
    if (!_drag) return;
    var total = state.totalFrames;
    if (_drag.group) {
      var dx = Math.round((e.clientX - _drag.startX) / FC);
      if (!dx) return;
      // Bug found 2026-07: this branch used to ALWAYS shift both in AND out
      // by dx for every member, even when the drag started on a single IN
      // or OUT handle — so a group in-point trim silently shifted the
      // whole range (out point included) instead of trimming just that
      // edge. Each handle type now only touches the field it owns, exactly
      // like the single-bar (non-group) branch below.
      //
      // 2026-07-30 fix (Cyril: "pas possible de faire bouger un in point
      // d'un calque avec un outpoint d'un autre et inversement quand ils
      // sont tous les 2 select") — the type of the ONE handle physically
      // grabbed to start the drag used to apply to EVERY member uniformly,
      // so a marquee that tagged layer A's IN and layer B's OUT separately
      // still moved both the same way once dragged together. Partition by
      // each member's OWN part instead (copied onto m.part in onDown,
      // straight from _barSel's marquee-assigned in/out/both tag): 'in'/
      // 'out' members clamp independently per-member (trimming one can't
      // break another), 'both' members keep the original group-wide bounds
      // check so a rigid whole-bar group still stays aligned or doesn't
      // move at all, never partially clamping some members but not others.
      var inMembers = [], outMembers = [], bothMembersM = [];
      _drag.members.forEach(function (m) {
        var p = m.part || 'both';
        if (p === 'in') inMembers.push(m); else if (p === 'out') outMembers.push(m); else bothMembersM.push(m);
      });
      inMembers.forEach(function (m) {
        var mld = state.layers[m.li]; if (!mld) return;
        var mNewIn = Math.max(0, Math.min(m.origIn + dx, m.origOut - 1));
        if (!trySetLinkedEdge(mld, 'in', mNewIn)) mld.inPoint = mNewIn;
        updateBar(m.row, m.li);
        updateLinkedChildrenBars(m.li);
      });
      outMembers.forEach(function (m) {
        var mld = state.layers[m.li]; if (!mld) return;
        var mNewOut = Math.min(total - 1, Math.max(m.origOut + dx, m.origIn + 1));
        if (!trySetLinkedEdge(mld, 'out', mNewOut)) mld.outPoint = mNewOut;
        updateBar(m.row, m.li);
        updateLinkedChildrenBars(m.li);
      });
      if (bothMembersM.length) {
        var ok = bothMembersM.every(function (m) {
          var ni = m.origIn + dx, no = m.origOut + dx;
          return ni >= 0 && no <= total - 1;
        });
        if (ok) bothMembersM.forEach(function (m) {
          var mld = state.layers[m.li]; if (!mld) return;
          var mInHandled = trySetLinkedEdge(mld, 'in', m.origIn + dx);
          var mOutHandled = trySetLinkedEdge(mld, 'out', m.origOut + dx);
          if (!mInHandled) mld.inPoint = m.origIn + dx;
          if (!mOutHandled) mld.outPoint = m.origOut + dx;
          updateBar(m.row, m.li);
          updateLinkedChildrenBars(m.li);
        });
      }
      // Live keyframe preview — one shared selDx read off the first member
      // that actually moved (matches mouseup's own reconciliation a few
      // hundred lines down: a group drag shares ONE keySel, not one per
      // member), otherwise each member previews its own layer-wide shift.
      if (_drag.keySel && _drag.keySel.length) {
        if (!e.altKey) {
          var selDxLive = 0;
          _drag.members.some(function (m) {
            var mld = state.layers[m.li]; if (!mld) return false;
            selDxLive = (_drag.type === 'out' ? outPointOf(mld) - m.origOut : inPointOf(mld) - m.origIn);
            return !!selDxLive;
          });
          livePreviewSelectedKeys(selDxLive);
        }
      } else {
        _drag.members.forEach(function (m) {
          var mld = state.layers[m.li]; if (!mld) return;
          // m.part, not _drag.type — this now matches the per-member edge
          // this specific bar actually moved a few lines up, instead of
          // whichever handle was grabbed to start the whole gesture.
          livePreviewLayerKeys(m.li, m.part || 'both', mld, m.origIn, e.altKey);
        });
      }
      if (window.loadFrame) loadFrame(state.currentFrame);
      if (window.SMEngineBridge) SMEngineBridge.renderNow();
      return;
    }
    var ld = state.layers[_drag.li]; if (!ld) { _drag = null; return; }
    var dx = Math.round((e.clientX - _drag.startX) / FC);
    if (_drag.type === 'in') {
      var newIn = Math.max(0, Math.min(_drag.origIn + dx, _drag.origOut - 1));
      if (!trySetLinkedEdge(ld, 'in', newIn)) ld.inPoint = newIn;
    } else if (_drag.type === 'out') {
      var newOut = Math.min(total - 1, Math.max(_drag.origOut + dx, _drag.origIn + 1));
      if (!trySetLinkedEdge(ld, 'out', newOut)) ld.outPoint = newOut;
    } else {
      var w = _drag.origOut - _drag.origIn;
      var ni = Math.max(0, _drag.origIn + dx);
      if (ni + w >= total) ni = total - 1 - w;
      var inHandled = trySetLinkedEdge(ld, 'in', ni);
      var outHandled = trySetLinkedEdge(ld, 'out', ni + w);
      if (!inHandled) ld.inPoint = ni;
      if (!outHandled) ld.outPoint = ni + w;
    }
    updateBar(_drag.row, _drag.li);
    updateLinkedChildrenBars(_drag.li);
    if (_drag.keySel && _drag.keySel.length) {
      if (!e.altKey) {
        var selDxLive2 = _drag.type === 'out' ? (outPointOf(ld) - _drag.origOut) : (inPointOf(ld) - _drag.origIn);
        livePreviewSelectedKeys(selDxLive2);
      }
    } else {
      livePreviewLayerKeys(_drag.li, _drag.type, ld, _drag.origIn, e.altKey);
    }
    // Content visibility for the CURRENT frame must reflect the new range
    // live (dragging the out point below the playhead should hide the
    // layer immediately) — loadFrame() re-derives userLayers[i] from
    // getEffectiveStrokes(), which is the actual gate.
    //
    // But only when the range ACTUALLY moved. This is a raw mousemove
    // handler with no rAF latch, and in/out points are quantised to whole
    // frames (dx is Math.round'ed above), so a pointer sliding across one
    // frame's width fired a full Paper scene rebuild + engine render on
    // every event — dozens of identical rebuilds per frame of travel. The
    // bar itself still follows the pointer (updateBar, above) because it is
    // cheap; only the scene rebuild is gated. 2026-07-28.
    if (ld.inPoint !== _drag.lastIn || ld.outPoint !== _drag.lastOut) {
      _drag.lastIn = ld.inPoint; _drag.lastOut = ld.outPoint;
      if (window.loadFrame) loadFrame(state.currentFrame);
      if (window.SMEngineBridge) SMEngineBridge.renderNow();
    }
  });
  document.addEventListener('mouseup', function (upEv) {
    endMarquee();
    if (!_drag) return;
    var d = _drag; _drag = null;
    // Unconditional, before any branch below (including the early-return
    // "plain click" ones a few lines down, which don't otherwise touch this
    // element) — a stale .hot surviving past mouseup would leave the handle
    // looking permanently "grabbed".
    if (d.hotEl) d.hotEl.classList.remove('hot');
    // The live preview is a transform on whatever was rendered at drag
    // start — every branch below either commits data + calls
    // renderLayerList/renderTimeline (which rebuilds these nodes from
    // scratch, transform and all) or does nothing further; clearing here
    // unconditionally means a branch that DOESN'T re-render (e.g. the
    // click-to-select return just below) can never leave a stale offset
    // behind either.
    if (window.SMMotion && SMMotion.clearKeyframeShiftPreview) SMMotion.clearKeyframeShiftPreview();
    // A press on a bar that never turned into a retime drag is a plain CLICK,
    // and the bar covers most of the grid half of a layer's row — so without
    // this that whole strip was unselectable (2026-07-27: "impossible de
    // select un layer en clicquand de ce côté de la timeline").
    if (!d.group && d.li != null && Math.abs(upEv.clientX - d.startX) < 3 &&
        state.appMode === 'motion' && window.SMMotion && SMMotion.selectLayerFromGrid) {
      _barAnchorLi = d.li;
      SMMotion.selectLayerFromGrid(d.li);
      // selectLayerFromGrid always syncs _barSel with part:'both' (whole-bar
      // ring) since it has no notion of which handle was actually pressed —
      // right for a body click, wrong for a plain click ON a handle, which
      // should darken THAT handle instead (spec: "sélectionné -> la poignée
      // s'assombrit" — found live, Cyril: "quand on select ça ne devient pas
      // foncé"). Override to the handle-specific selection afterward rather
      // than duplicate selectLayerFromGrid's layer-activation side effects.
      if (d.type === 'in' || d.type === 'out') {
        _barSel = [{ li: d.li, part: d.type }];
        refreshBarSelClasses();
      }
      return;
    }
    // A group drag that never actually moved is ALSO a plain click — onDown
    // only takes this branch because the pressed bar happened to already be
    // part of a 2+ selection, but the user's gesture is indistinguishable
    // from any other plain click. Previously this fell straight through to
    // reconcileTimeLinks/shiftLayerFrames below and did nothing visible, so
    // a plain click on an already-multi-selected bar silently stopped
    // selecting anything at all — found live (Cyril: "la selection des
    // calques... pas hyper bonne"): Shift-click 2 bars, then plain-click one
    // of them again, expecting it to narrow to just that layer like every
    // other selection tool in this app. Same convention as a fresh
    // (non-group) click a few lines up — narrow to the one bar actually
    // pressed, which also resyncs _barSel via selectLayerFromGrid.
    if (d.group && d.pressLi != null && Math.abs(upEv.clientX - d.startX) < 3 &&
        state.appMode === 'motion' && window.SMMotion && SMMotion.selectLayerFromGrid) {
      _barAnchorLi = d.pressLi;
      SMMotion.selectLayerFromGrid(d.pressLi);
      return;
    }
    // A time-linked layer (Parent in Time) resolves its in/out from its
    // SOURCE, so the ld.inPoint/outPoint this drag just wrote would be
    // ignored — the bar would snap back and the drag would read as broken.
    // Convert the movement into a change of OFFSET instead: the layer keeps
    // following its source, now at the distance you just dragged it to.
    // Same principle as spatial parenting, where moving a child changes the
    // child's own transform rather than detaching it.
    (function reconcileTimeLinks() {
      var members = d.group ? d.members : [{ li: d.li, origIn: d.origIn, origOut: d.origOut }];
      members.forEach(function (m) {
        var ld = state.layers[m.li];
        if (!ld || !ld.timeLink) return;
        var mode = ld.timeLink.mode || 'both';
        // What the user dragged the edge TO. trySetLinkedEdge (mousemove,
        // above) already writes the offset LIVE for any edge this link
        // covers and never touches ld.inPoint/outPoint while doing so — so
        // when they're still null here, the drag's real result is the
        // CURRENT resolved position, not m.origIn (the pre-drag value).
        // Falling back to origIn silently overwrote every live-linked drag
        // back to a zero offset on mouseup (bug found live: onDown fires,
        // setLayerValue fires mid-drag with the right numbers, then this
        // function stomped them back to the start). ld.inPoint/outPoint are
        // only ever non-null here for a pre-existing hard value that
        // predates this link (or an edge the mode doesn't cover, handled by
        // the mode guards below) — that legacy value is still the right
        // thing to migrate into an offset once.
        var wantIn = ld.inPoint != null ? ld.inPoint : inPointOf(ld);
        var wantOut = ld.outPoint != null ? ld.outPoint : outPointOf(ld);
        var srcIn = null, srcOut = null;
        state.layers.forEach(function (o) {
          if (o !== ld && o.layerUid === ld.timeLink.uid) { srcIn = inPointOf(o); srcOut = outPointOf(o); }
        });
        if (srcIn == null) return; // source gone — leave the hard values alone
        // Cross-type source (2026-08-16): srcAnchor overrides which of the
        // SOURCE's edges an 'in'/'out'-mode link reads from — same fallback
        // as resolveLinkedTime (app.js), setLayerTimeLink (motion.js) and
        // trySetLinkedEdge (mousemove, above). Absent for every pre-existing
        // / same-type link, so this changes nothing for those. 'both' mode
        // never sets srcAnchor (no meaning for "my whole range follows your
        // single point"), so it always reads the same-type srcIn/srcOut.
        var srcWhich = (ld.timeLink.srcAnchor === 'in' || ld.timeLink.srcAnchor === 'out') ? ld.timeLink.srcAnchor : null;
        var srcForIn = srcWhich ? (srcWhich === 'in' ? srcIn : srcOut) : srcIn;
        var srcForOut = srcWhich ? (srcWhich === 'in' ? srcIn : srcOut) : srcOut;
        // Offsets are Motion properties now (timeLinkInOffset/Out,
        // 2026-07-30) — write through the same public setter the side-panel
        // field and the pickwhip use, not the raw legacy field.
        if (window.SMMotion) {
          if (mode !== 'out') SMMotion.setLayerValue(m.li, 'timeLinkInOffset', [wantIn - srcForIn]);
          if (mode !== 'in') SMMotion.setLayerValue(m.li, 'timeLinkOutOffset', [wantOut - srcForOut]);
        }
        // The hard values are dead weight on a linked layer; dropping them
        // keeps a later unlink from resurrecting a stale range. Only for
        // edges the link actually covers — a partial-mode link's other edge
        // is a real trim value, just written fresh by this same drag.
        if (mode !== 'out') delete ld.inPoint;
        if (mode !== 'in') delete ld.outPoint;
      });
    })();
    // Feedback: "il faudrait pouvoir select des in et/out point de calque
    // avec keyframe pour les déplacer ensemble" — a whole-bar BODY move
    // (type:'both') retimes the layer's actual keyframe content along with
    // the visibility window, via window.SM.shiftLayerFrames (timeline.js).
    // Applied ONCE here at drop, not live per mousemove.
    //
    // INSIDE a component (state.activeSymbolId — 2026-07-17, "je suis dans
    // le component, ça m'affiche le montage de toute ces formes... si je
    // modifie les calque en drag... ça doit modifier les montage de ceux
    // si, c'est comme un precomp"): the IN handle ALSO retimes (shifts
    // where the shape's content starts — a single split-out shape layer is
    // just one held span, not pre-recorded footage with earlier frames
    // hidden past the edge, so "trim the in point" and "retime the start"
    // are the same operation here). The OUT handle deliberately stays a
    // pure clip/trim (ld.outPoint only, no shiftLayerFrames): trimming the
    // tail shorter never needs to MOVE anything — the existing visibility
    // window already does exactly that — and re-deriving from the live
    // canvas after an out-drag hit the same content-loss bug an in-drag
    // did (see shiftLayerFrames' own comment) for zero benefit, since
    // there's nothing to shift.
    // Whether the keyframes travel with the trim used to be entirely
    // hardcoded by handle type, with no way to ask for the other one —
    // 2026-07-25: "la sélection multiple de inpoint ou outpoint AVEC SANS
    // les keyframes". Alt inverts the default, held at mousedown or still
    // held at drop (either reads as intent), and applies to the WHOLE
    // multi-bar selection since the group branch shares this code path.
    //
    // Not offered on the OUT handle: shortening the tail has nothing to
    // move — the visibility window already hides what's past it, and the
    // shift computed below is derived from the IN point, so an out-drag
    // would compute dx=0 and do nothing anyway. Said out loud rather than
    // silently ignored.
    var defaultRetimes = d.type !== 'out' && (d.type === 'both' || !!state.activeSymbolId);
    var altHeld = !!(d.alt || (upEv && upEv.altKey));
    // Alt has ONE meaning at a time, decided by whether keys were picked:
    //   - no selection -> it inverts the layer-wide default above
    //   - a selection  -> it means "leave those keys where they are", and
    //     must NOT also flip the layer-wide default, or the keys the user
    //     just asked to leave alone would be moved by that pass instead
    //     (measured: an Alt in-drag with 2 keys selected moved them -11
    //     anyway, through shiftLayerMotionKeys).
    var hasKeySel = !!(d.keySel && d.keySel.length);
    if (altHeld && d.type === 'out') {
      if (window.showToast) showToast('Rogner la fin ne déplace jamais les keyframes — utilise le point d\'entrée ou le corps de la barre');
    }
    var retimes = (altHeld && !hasKeySel && d.type !== 'out') ? !defaultRetimes : defaultRetimes;
    if (altHeld && !hasKeySel && d.type !== 'out' && window.showToast) {
      showToast(retimes ? 'Keyframes déplacées avec le calque' : 'Fenêtre de visibilité seule — keyframes laissées en place');
    }
    if (altHeld && hasKeySel && window.showToast) showToast('Keyframes sélectionnées laissées en place');
    // An explicit keyframe selection travels with the handle, whatever the
    // handle's own retime default is (2026-07-25: "il faut pouvoir bouger
    // les in/out point de calque avec les keyframes selectionnées aussi").
    // Deliberately NOT gated on `retimes`: that flag encodes what should
    // happen to the layer's OWN content when nothing was picked — an IN
    // trim outside a component leaves it alone — but picking keys is an
    // instruction, and it would be silently ignored otherwise (measured:
    // the in point moved 29 -> 40 while the selected key stayed at 22).
    // Alt still means "leave the keyframes where they are", so it opts out.
    //
    // A layer can also carry a STANDING lock (ld.keyLock === 'in' | 'out' |
    // 'layer'), Sander van Dijk's "Lock Keyframes to In and Out Points":
    // with it set, the layer's keys follow that edge every time, with no
    // selection needed and no modifier to remember. The selection path below
    // still wins when there IS one — an explicit pick beats a standing rule.
    var lockDx = 0;
    if (!hasKeySel && !altHeld && window.SMMotion && SMMotion.shiftLayerMotionKeys) {
      var lockOne = function (li2, origIn, origOut) {
        var l2 = state.layers[li2]; if (!l2 || !l2.keyLock) return;
        var moved = l2.keyLock === 'out' ? outPointOf(l2) - origOut : inPointOf(l2) - origIn;
        // 'layer' locks to the whole block, so only a body move counts;
        // 'in'/'out' follow their own edge whichever handle was dragged.
        if (l2.keyLock === 'layer' && d.type !== 'both') return;
        if (!moved) return;
        SMMotion.shiftLayerMotionKeys(li2, moved);
        lockDx = moved;
      };
      if (d.group) d.members.forEach(function (m) { lockOne(m.li, m.origIn, m.origOut); });
      else lockOne(d.li, d.origIn, d.origOut);
    }
    var selDx = 0;
    if (hasKeySel && !altHeld) {
      if (d.group) {
        // Every member moved by the same delta — read it off the first one
        // that actually moved.
        d.members.some(function (m) {
          var mld = state.layers[m.li]; if (!mld) return false;
          selDx = (d.type === 'out' ? outPointOf(mld) - m.origOut : inPointOf(mld) - m.origIn);
          return !!selDx;
        });
      } else {
        var sld = state.layers[d.li];
        if (sld) selDx = (d.type === 'out' ? outPointOf(sld) - d.origOut : inPointOf(sld) - d.origIn);
      }
      if (selDx && window.SMMotion && SMMotion.shiftKeySelection) {
        SMMotion.shiftKeySelection(d.keySel, selDx);
        if (window.showToast) showToast(d.keySel.length + ' keyframe(s) déplacée(s) avec le calque');
      }
    }
    if (retimes && window.SM && window.SM.shiftLayerFrames) {
      var dxOf = function (ld, orig) { return inPointOf(ld) - orig.origIn; };
      // shiftLayerFrames moves the DRAWN content (ld.frames) only —
      // SMMotion.shiftLayerMotionKeys moves the property/effect keyframes,
      // which used to stay put (2026-07-25): the layer landed on new frames
      // still animating on its old schedule. Both, or the retime is only
      // half done.
      // An explicit keyframe selection WINS over the layer-wide shift: if
      // the user picked keys, those are the ones that move, and moving them
      // twice (once here, once via the layer-wide pass) would double the
      // offset. Layers with no selected key still shift wholesale.
      var selLayers = {};
      (d.keySel || []).forEach(function (s) {
        var i = state.layers.indexOf(s.holder);
        if (i >= 0) selLayers[i] = 1;
        else state.layers.forEach(function (ld2, li2) {
          // per-element holder: find the layer that owns it
          if (ld2.elementMotion) Object.keys(ld2.elementMotion).forEach(function (k) { if (ld2.elementMotion[k] === s.holder) selLayers[li2] = 1; });
        });
      });
      var retimeOne = function (li, dx) {
        if (!dx) return;
        window.SM.shiftLayerFrames(li, dx);
        // With an explicit selection, the layer-wide key pass is off entirely:
        // the picked keys are the ones that move (or, under Alt, the ones that
        // deliberately don't) — sweeping the rest along would contradict both.
        if (hasKeySel) return;
        var lk = state.layers[li] && state.layers[li].keyLock;
        if (lk && lockDx) return; // the standing lock above already moved this layer's keys
        if (window.SMMotion && SMMotion.shiftLayerMotionKeys) SMMotion.shiftLayerMotionKeys(li, dx);
      };
      if (d.group) {
        d.members.forEach(function (m) {
          var mld = state.layers[m.li]; if (!mld) return;
          retimeOne(m.li, dxOf(mld, m));
        });
      } else {
        var ld = state.layers[d.li];
        if (ld) retimeOne(d.li, dxOf(ld, d));
      }
      if (window.loadFrame) loadFrame(state.currentFrame);
      if (window.SMEngineBridge) SMEngineBridge.renderNow();
    }
    // Full rebuild only once, at drag END: other rows' bars don't move
    // during the drag so a per-move renderTimeline() would be pure waste
    // (CLAUDE.md §5 — avoid unnecessary per-move rebuild work). Now always
    // renderTimeline() (not just renderLayerList()) since a body-move drag
    // may have just shifted keyframe content — Animation 2D's frame-grid
    // dots and this bar's own tick marks (renderKeyTicks) both need to
    // rebuild to reflect the new positions.
    if (window.renderTimeline) renderTimeline();
    else if (window.renderLayerList) renderLayerList();
  });

  // ---- batch operations on the current bar selection (Align/Distribute/
  // Flip/Select Every/Invert Selection) — all require >=2 selected bars
  // except Invert, which works off whatever's currently selected (possibly
  // zero). One pushUndo() per operation.
  function selectedLayers() {
    return _barSel.map(function (s) { return { li: s.li, ld: state.layers[s.li] }; }).filter(function (x) { return x.ld; });
  }
  function applyBatch(fn) {
    var items = selectedLayers();
    if (items.length < 2) { if (window.showToast) showToast('Sélectionne au moins 2 calques'); return; }
    if (window.pushUndo) pushUndo();
    fn(items);
    if (window.renderTimeline) renderTimeline();
    if (window.loadFrame) loadFrame(state.currentFrame);
    if (window.SMEngineBridge) SMEngineBridge.renderNow();
  }
  // Shared write path for every batch op below (2026-07-30 fix — found
  // still outstanding by the broad QA audit, deliberately deferred twice
  // earlier this session). Every one of these ops used to write straight
  // to ld.inPoint/outPoint, same bug trySetLinkedEdge's own header comment
  // already fixed once for the single-bar drag handlers: a linked child's
  // in/out is completely inert (layerInPoint/layerOutPoint check
  // resolveLinkedTime FIRST and never read ld.inPoint/outPoint once an edge
  // is linked), so Align/Distribute/Flip/Stagger silently did nothing to any
  // Parent-in-Time child caught in the selection — no error, the bar just
  // never moved. Redirects through the same helper the manual drag path
  // uses; `w` (each bar's own duration) is always computed from the CURRENT
  // effective in/out before either write, so the two edges never see a
  // half-updated width mid-iteration.
  function setBarEdges(ld, newIn, newOut) {
    if (!trySetLinkedEdge(ld, 'in', newIn)) ld.inPoint = newIn;
    if (!trySetLinkedEdge(ld, 'out', newOut)) ld.outPoint = newOut;
  }
  function alignBars(mode) {
    applyBatch(function (items) {
      if (mode === 'left') {
        var minIn = Math.min.apply(null, items.map(function (x) { return inPointOf(x.ld); }));
        items.forEach(function (x) { var w = outPointOf(x.ld) - inPointOf(x.ld); setBarEdges(x.ld, minIn, minIn + w); });
      } else if (mode === 'right') {
        var maxOut = Math.max.apply(null, items.map(function (x) { return outPointOf(x.ld); }));
        items.forEach(function (x) { var w = outPointOf(x.ld) - inPointOf(x.ld); setBarEdges(x.ld, maxOut - w, maxOut); });
      } else {
        var avgCenter = items.reduce(function (s, x) { return s + (inPointOf(x.ld) + outPointOf(x.ld)) / 2; }, 0) / items.length;
        items.forEach(function (x) {
          var w = outPointOf(x.ld) - inPointOf(x.ld);
          var ni = Math.max(0, Math.round(avgCenter - w / 2));
          setBarEdges(x.ld, ni, ni + w);
        });
      }
    });
  }
  // Evenly spaces the selected bars' START points across the group's own
  // min-in..max-out span, preserving each bar's own duration.
  function distributeBars() {
    applyBatch(function (items) {
      var sorted = items.slice().sort(function (a, b) { return inPointOf(a.ld) - inPointOf(b.ld); });
      var first = inPointOf(sorted[0].ld), last = inPointOf(sorted[sorted.length - 1].ld);
      var step = sorted.length > 1 ? (last - first) / (sorted.length - 1) : 0;
      sorted.forEach(function (x, i) {
        var w = outPointOf(x.ld) - inPointOf(x.ld);
        var ni = Math.round(first + step * i);
        setBarEdges(x.ld, ni, ni + w);
      });
    });
  }
  // Reverses the temporal ORDER of the selected bars' start points (each
  // bar keeps its own duration, just swaps which "slot" it occupies).
  function flipBars() {
    applyBatch(function (items) {
      var sorted = items.slice().sort(function (a, b) { return inPointOf(a.ld) - inPointOf(b.ld); });
      var slots = sorted.map(function (x) { return inPointOf(x.ld); });
      var reversed = slots.slice().reverse();
      sorted.forEach(function (x, i) {
        var w = outPointOf(x.ld) - inPointOf(x.ld);
        setBarEdges(x.ld, reversed[i], reversed[i] + w);
      });
    });
  }
  // "Offset"/stagger: each selected bar's start shifts by an increasing
  // multiple of `step` frames (ordered by
  // current in-point, so the leftmost bar anchors the group and stays put),
  // preserving each bar's own duration — the classic cascading-entrance
  // rig ("layer 1 at frame 0, layer 2 at frame +step, layer 3 at +2*step…").
  // Distinct from distributeBars (which compresses/expands the WHOLE group
  // into an even spread across its own existing span) — stagger instead
  // grows the group's total span outward from its first bar by a fixed,
  // user-chosen amount per layer.
  function staggerBars(step) {
    applyBatch(function (items) {
      var sorted = items.slice().sort(function (a, b) { return inPointOf(a.ld) - inPointOf(b.ld); });
      var base = inPointOf(sorted[0].ld);
      sorted.forEach(function (x, i) {
        var w = outPointOf(x.ld) - inPointOf(x.ld);
        var ni = Math.max(0, base + step * i);
        setBarEdges(x.ld, ni, ni + w);
      });
    });
  }
  function selectEveryNth(n) {
    n = Math.max(2, parseInt(n, 10) || 2);
    var sorted = _barSel.slice().sort(function (a, b) { return inPointOf(state.layers[a.li]) - inPointOf(state.layers[b.li]); });
    _barSel = sorted.filter(function (_s, i) { return i % n === 0; });
    refreshBarSelClasses();
    if (window.showToast) showToast(_barSel.length + ' calque(s) sélectionné(s)');
  }
  // Selects every layer that HAS a bar row currently rendered and is NOT
  // already selected — inverts within the same universe Box Select draws
  // its marquee over. Always lands on 'both' (whole-bar), same as
  // toggleBarSel — inversion isn't tied to any rectangle geometry either.
  function invertBarSelection() {
    var all = [];
    document.querySelectorAll('.layer-inout-bar').forEach(function (bar) {
      var row = bar.parentElement, li = row && _rowLayerIdx.get(row);
      if (li != null) all.push(li);
    });
    _barSel = all.filter(function (li) { return !isBarSelected(li); }).map(function (li) { return { li: li, part: 'both' }; });
    refreshBarSelClasses();
  }

  // Builds the bar + its two handles into `row` (a .frow — Animation 2D's
  // per-layer frame row, or Motion mode's collapsed layer spacer row) and
  // wires its drag handlers. Idempotent-safe to call once per row per
  // render pass (renderTimeline/renderTimelineMotion rebuild rows from
  // scratch every time, same as every other overlay in this codebase).
  // Parent-in-Time anchor role helpers (2026-08-16 restyle,
  // nemo-timeline-inout-spec.html). Nemo's ld.timeLink={uid,mode} links the
  // WHOLE layer to one source by uid, with mode picking which of the
  // CHILD's own edges are driven — the source is always resolved via the
  // SAME edge type (trySetLinkedEdge above), so unlike the spec's own
  // {c:{r,a},p:{r,a}} shape, neither side stores a separate anchor role.
  // The anchor a link visually belongs to is therefore DERIVED, not
  // stored: it's whichever on-bar anchor the pickwhip gesture that created
  // it was dragged FROM — mode:'in' -> the in anchor, 'out' -> the out
  // anchor, 'both' -> the whole-layer anchor, exactly mirroring
  // startTimeLinkPickwhip's own mode mapping a few lines below.
  function timeLinkChildAnchor(ld) {
    if (!ld || !ld.timeLink) return null;
    var mode = ld.timeLink.mode || 'both';
    return mode === 'in' ? 'in' : mode === 'out' ? 'out' : 'whole';
  }
  // A layer is a PARENT on anchor `anchorType` when some OTHER layer's link
  // targets it AND that other layer's own derived child anchor is the SAME
  // type — symmetric with timeLinkChildAnchor since today's model only ever
  // resolves same-type-to-same-type (in follows in, out follows out; cross-
  // type "in follows out" is a separate, larger data-model change, not yet
  // built).
  function isTimeLinkParentAnchor(li, anchorType) {
    var ld = state.layers[li];
    var uid = ld && ((window.SMMotion && SMMotion.ensureLayerUid) ? SMMotion.ensureLayerUid(ld) : ld.layerUid);
    if (!uid) return false;
    return state.layers.some(function (other) {
      return other !== ld && other.timeLink && other.timeLink.uid === uid && timeLinkChildAnchor(other) === anchorType;
    });
  }
  function buildBar(row, li) {
    row.style.position = 'relative';
    var bar = document.createElement('div'); bar.className = 'layer-inout-bar' + (selPartOf(li) === 'both' ? ' sel' : '');
    var hleft = document.createElement('div'); hleft.className = 'layer-inout-handle left';
    var hright = document.createElement('div'); hright.className = 'layer-inout-handle right';
    // The Alt modifier is the only affordance for "with / without the
    // keyframes" (2026-07-25), so it has to be written down somewhere the
    // user will actually meet it — on the handle itself.
    hleft.title = 'Point d\'entrée — glisser pour rogner. Alt+glisser : emmener aussi les keyframes.\nAvec plusieurs barres sélectionnées, toute la sélection suit.';
    hright.title = 'Point de sortie — glisser pour rogner (les keyframes ne bougent pas : rogner la fin ne déplace rien).';
    bar.title = 'Glisser le corps : déplace le calque ET ses keyframes. Alt+glisser : déplacer la fenêtre de visibilité seule, keyframes en place.';
    bar.appendChild(hleft); bar.appendChild(hright);
    // Parent in Time — 3 on-timeline connection points (2026-07-30, Van
    // Dijk 2.1). Glisser directement depuis la barre plutôt que par le
    // panel latéral "Temps" — même lien (ld.timeLink), juste l'endroit du
    // geste qui change. Un seul lien par geste (confirmé avec Cyril, pas
    // de multi-calques en un coup) ; réutilise startTimeLinkPickwhip
    // (motion.js, exposé via SMMotion) pour le drag/le anti-cycle/la
    // création du lien, pas une seconde implémentation.
    var childAnchor = timeLinkChildAnchor(state.layers[li]);
    ['in', 'whole', 'out'].forEach(function (mode) {
      var a = document.createElement('div');
      a.className = 'timelink-anchor ' + mode;
      // Engaged-anchor coloring (2026-08-16) — "Lecture du lien: rond
      // foncé = enfant, rond clair = parent... ces deux-là sont TOUJOURS
      // visibles, Alt ne révèle que les points encore libres" (CSS below
      // gates the free/default state's visibility on an Alt-held class;
      // .is-child/.is-parent opt back out of that gate unconditionally).
      if (childAnchor === mode) a.classList.add('is-child');
      if (isTimeLinkParentAnchor(li, mode)) a.classList.add('is-parent');
      a.title = (mode === 'in' ? 'Glisser vers un autre calque : lie le point d’entrée de ce calque à son temps'
        : mode === 'out' ? 'Glisser vers un autre calque : lie le point de sortie de ce calque à son temps'
        : 'Glisser vers un autre calque : lie tout le calque (entrée + sortie) à son temps') + ' — clic droit pour délier';
      a.addEventListener('mousedown', function (e) {
        if (!window.SMMotion || !window.SMMotion.startTimeLinkPickwhip) return;
        window.SMMotion.startTimeLinkPickwhip(li, a, e, mode === 'whole' ? 'both' : mode);
      });
      // Right-click = instant unlink (2026-07-30, Cyril: "il était
      // impossible de désactiver le parent in time... ça peut être un
      // raccourci ou clic droit sur les boutons de parent") — stops
      // propagation so it doesn't ALSO trigger the bar's own contextmenu
      // (reset in/out points) a few lines below. A no-op when this layer
      // isn't linked, so right-clicking a plain anchor before any drag
      // stays inert rather than erroring.
      a.addEventListener('contextmenu', function (e) {
        e.preventDefault(); e.stopPropagation();
        var ld2 = state.layers[li];
        if (!ld2 || !ld2.timeLink) return;
        if (window.pushUndo) pushUndo();
        if (window.unlinkTimeLinkPreserveRange) unlinkTimeLinkPreserveRange(ld2); else delete ld2.timeLink;
        if (window.renderLayerList) renderLayerList();
        if (window.renderTimeline) renderTimeline();
        if (window.loadFrame) loadFrame(state.currentFrame);
        if (window.SMEngineBridge) SMEngineBridge.renderNow();
        if (window.showToast) showToast('Lien temporel retiré');
      });
      bar.appendChild(a);
    });
    row.appendChild(bar);
    updateBar(row, li);
    _rowLayerIdx.set(row, li);
    _liToRow[li] = row;
    hleft.addEventListener('mousedown', function (e) { onDown(li, row, 'in', e); });
    hright.addEventListener('mousedown', function (e) { onDown(li, row, 'out', e); });
    bar.addEventListener('mousedown', function (e) { if (e.target === bar) onDown(li, row, 'both', e); });
    bar.addEventListener('contextmenu', function (e) {
      e.preventDefault(); e.stopPropagation();
      if (!window.showContextMenu) return;
      var menu = [
        { label: 'Réinitialiser (pleine durée)', action: function () { if (window.pushUndo) pushUndo(); delete state.layers[li].inPoint; delete state.layers[li].outPoint; if (window.renderTimeline) renderTimeline(); if (window.loadFrame) loadFrame(state.currentFrame); if (window.SMEngineBridge) SMEngineBridge.renderNow(); } },
      ];
      // Menu-based Parent-in-Time (2026-07-31, Cyril: "clic droit pour
      // parent in time sur ... keyframe + in/out point") — target layer is
      // this bar's own; reachable whatever else is selected. Same shared
      // builder as the Motion layer-row menu (timeline.js).
      if (window.buildTimeLinkMenuItems) {
        menu.push({ label: 'Parent in Time — lier le temps à…', action: function () {
          window.showContextMenu(e.clientX + 8, e.clientY + 8, window.buildTimeLinkMenuItems(li, state.layers[li], function () { if (window.renderLayerList) renderLayerList(); if (window.renderTimeline) renderTimeline(); }));
        } });
      }
      // Batch ops act on the WHOLE current selection, not just the bar
      // that was right-clicked — offered once >=2 bars are selected,
      // regardless of which one you right-click.
      if (_barSel.length >= 2) {
        menu.push({ sep: true });
        menu.push({ label: 'Aligner à gauche (in)', action: function () { alignBars('left'); } });
        menu.push({ label: 'Aligner au centre', action: function () { alignBars('center'); } });
        menu.push({ label: 'Aligner à droite (out)', action: function () { alignBars('right'); } });
        menu.push({ label: 'Distribuer uniformément', action: function () { distributeBars(); } });
        menu.push({ label: 'Échelonner (stagger)…', action: function () { var v = prompt('Décalage entre calques (frames)', '2'); var step = parseInt(v, 10); if (!isNaN(step) && step !== 0) staggerBars(step); } });
        menu.push({ label: 'Inverser l’ordre (flip)', action: function () { flipBars(); } });
        menu.push({ label: 'Sélectionner 1 sur 2', action: function () { selectEveryNth(2); } });
      }
      if (_barSel.length >= 1) menu.push({ label: 'Inverser la sélection', action: invertBarSelection });
      window.showContextMenu(e.clientX, e.clientY, menu);
    });
    // Marquee starts on a mousedown that lands on the ROW's own empty
    // background — this row's only real content is the bar itself, so
    // e.target===row means the click missed the bar entirely.
    row.addEventListener('mousedown', function (e) { if (e.target === row) startMarquee(e); });
  }

  // Alt-reveal for free (unlinked) timelink-anchor dots (2026-08-16, spec:
  // "maintiens Alt pour révéler les points libres") — a class on #frame-grid
  // itself (not on each row, which renderTimeline wipes and rebuilds
  // constantly) so it survives across re-renders with zero re-registration.
  // Deliberately its OWN independent Alt-tracking rather than reading
  // state.altDown (set by timeline.js's onKeyDown/onKeyUp for the UNRELATED
  // Alt+drag "move visibility window only" gesture on the handles) — reusing
  // that flag would mean this dot-reveal and that other gesture's meaning
  // are coupled for no reason, and drift the moment either one's own
  // key-handling logic changes.
  function setTimeLinkAltReveal(v) {
    var grid = document.getElementById('frame-grid');
    if (grid) grid.classList.toggle('timelink-alt', v);
  }
  document.addEventListener('keydown', function (e) { if (e.key === 'Alt') setTimeLinkAltReveal(true); });
  document.addEventListener('keyup', function (e) { if (e.key === 'Alt') setTimeLinkAltReveal(false); });
  window.addEventListener('blur', function () { setTimeLinkAltReveal(false); });

  // Feedback: "on doit pouvoir drag + rect en dessous les calques même là
  // où y en pas dans la timeline". The per-row listener above only ever
  // sees a mousedown that lands on an actual `.frow`'s own background —
  // below the LAST rendered row there's no row element at all. `#frame-grid`
  // itself sizes to its CONTENT height (confirmed: with 2 layer rows it's
  // ~44px tall), so it doesn't even cover that empty area — `#fg-wrap` is
  // the actual scroll VIEWPORT underneath it and is what really extends
  // down to the panel's bottom edge, so that's the element a mousedown in
  // the empty space actually lands on. Bound once at load — renders clear
  // `#frame-grid`'s innerHTML but never replace `#fg-wrap` itself, so this
  // listener survives across re-renders same as the row-level ones would
  // need rebuilding but this one doesn't.
  function initEmptyGridMarquee() {
    var wrap = document.getElementById('fg-wrap');
    if (!wrap) return;
    wrap.addEventListener('mousedown', function (e) { if (e.target === wrap) startMarquee(e); });
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', initEmptyGridMarquee);
  else initEmptyGridMarquee();

  window.SMLayerInOut = {
    inPointOf: inPointOf, outPointOf: outPointOf, hasCustomRange: hasCustomRange, buildBar: buildBar, updateBar: updateBar,
    alignBars: alignBars, distributeBars: distributeBars, flipBars: flipBars, staggerBars: staggerBars, selectEveryNth: selectEveryNth, invertBarSelection: invertBarSelection,
    // Exposed so Motion's own grid marquee can drive bar selection too —
    // it intercepts the mousedown in capture phase before this module's
    // listeners ever see it (2026-07-25), so the two marquees have to share
    // one gesture rather than race for it. Mirror of the SMMotion.marqueeSelect
    // call this module already makes in the other direction.
    marqueeSelect: applyMarqueeSelection,
    clearSelection: clearBarSel,
    // Retime layers by a per-layer frame delta, content and property keys
    // included — the same two calls a bar drag makes at drop, exposed so
    // motion.js's box edges can stagger a whole selection without
    // reimplementing (and drifting from) that pair. Callers own the in/out
    // values themselves; this moves what LIVES at those frames.
    retimeLayers: function (plan) {
      (plan || []).forEach(function (p) {
        if (!p || !p.dx) return;
        if (window.SM && window.SM.shiftLayerFrames) window.SM.shiftLayerFrames(p.li, p.dx);
        if (window.SMMotion && SMMotion.shiftLayerMotionKeys) SMMotion.shiftLayerMotionKeys(p.li, p.dx);
      });
    },
    getBarSelection: function () { return _barSel.slice(); },
    // Optional anchorLi keeps _barAnchorLi in step with the layer-list's own
    // frozen anchor (motion.js syncBarSelToLayerSel passes it) so Shift-
    // ranges continue from the user's last click whichever side it was on.
    setBarSelection: function (sel, anchorLi) { _barSel = sel; if (anchorLi != null && state.layers[anchorLi]) _barAnchorLi = anchorLi; refreshBarSelClasses(); },
  };
})();
