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
    // Bug found 2026-07 ("si hover ou select avec rectangle le in ou
    // outpoint celui ci doit se bleuté"): this used to set the handle's
    // background as an INLINE style, which always wins over any CSS rule
    // regardless of selector specificity — .layer-inout-handle:hover and
    // .layer-inout-bar.sel .layer-inout-handle (style.css) existed but
    // could never actually show through on a trimmed (custom-color) bar.
    // Routing the layer color through a custom property instead lets
    // normal CSS cascade rules for hover/selected states win as expected.
    if (custom) bar.style.setProperty('--io-handle-color', hexToRgba(ld.color, 1));
    else bar.style.removeProperty('--io-handle-color');
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
    if (hleft) hleft.classList.toggle('sel', part === 'in');
    if (hright) hright.classList.toggle('sel', part === 'out');
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
  function onDown(li, row, type, e) {
    e.stopPropagation(); e.preventDefault();
    var ld = state.layers[li]; if (!ld) return;
    if (e.shiftKey || e.metaKey || e.ctrlKey) { toggleBarSel(li); return; } // select-only, no drag
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
        return { li: s.li, row: mrow, origIn: inPointOf(mld), origOut: outPointOf(mld) };
      }).filter(Boolean);
      _drag = { group: true, type: type, startX: e.clientX, members: members, alt: !!e.altKey, keySel: keySelNow() };
      return;
    }
    _drag = { li: li, row: row, type: type, startX: e.clientX, origIn: inPointOf(ld), origOut: outPointOf(ld), alt: !!e.altKey, keySel: keySelNow() };
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
      // like the single-bar (non-group) branch below — 'in'/'out' clamp
      // per-member independently (trimming can't break another member),
      // 'both' keeps the original whole-range shift with its group-wide
      // bounds check (shifting must stay valid for every member at once,
      // since duration is preserved).
      if (_drag.type === 'in') {
        _drag.members.forEach(function (m) {
          var mld = state.layers[m.li]; if (!mld) return;
          mld.inPoint = Math.max(0, Math.min(m.origIn + dx, m.origOut - 1));
          updateBar(m.row, m.li);
        });
      } else if (_drag.type === 'out') {
        _drag.members.forEach(function (m) {
          var mld = state.layers[m.li]; if (!mld) return;
          mld.outPoint = Math.min(total - 1, Math.max(m.origOut + dx, m.origIn + 1));
          updateBar(m.row, m.li);
        });
      } else {
        var ok = _drag.members.every(function (m) {
          var ni = m.origIn + dx, no = m.origOut + dx;
          return ni >= 0 && no <= total - 1;
        });
        if (!ok) return;
        _drag.members.forEach(function (m) {
          var mld = state.layers[m.li]; if (!mld) return;
          mld.inPoint = m.origIn + dx; mld.outPoint = m.origOut + dx;
          updateBar(m.row, m.li);
        });
      }
      if (window.loadFrame) loadFrame(state.currentFrame);
      if (window.SMEngineBridge) SMEngineBridge.renderNow();
      return;
    }
    var ld = state.layers[_drag.li]; if (!ld) { _drag = null; return; }
    var dx = Math.round((e.clientX - _drag.startX) / FC);
    if (_drag.type === 'in') ld.inPoint = Math.max(0, Math.min(_drag.origIn + dx, _drag.origOut - 1));
    else if (_drag.type === 'out') ld.outPoint = Math.min(total - 1, Math.max(_drag.origOut + dx, _drag.origIn + 1));
    else {
      var w = _drag.origOut - _drag.origIn;
      var ni = Math.max(0, _drag.origIn + dx);
      if (ni + w >= total) ni = total - 1 - w;
      ld.inPoint = ni; ld.outPoint = ni + w;
    }
    updateBar(_drag.row, _drag.li);
    // Content visibility for the CURRENT frame must reflect the new range
    // live (dragging the out point below the playhead should hide the
    // layer immediately) — loadFrame() re-derives userLayers[i] from
    // getEffectiveStrokes(), which is the actual gate.
    if (window.loadFrame) loadFrame(state.currentFrame);
    if (window.SMEngineBridge) SMEngineBridge.renderNow();
  });
  document.addEventListener('mouseup', function (upEv) {
    endMarquee();
    if (!_drag) return;
    var d = _drag; _drag = null;
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
  function alignBars(mode) {
    applyBatch(function (items) {
      if (mode === 'left') {
        var minIn = Math.min.apply(null, items.map(function (x) { return inPointOf(x.ld); }));
        items.forEach(function (x) { var w = outPointOf(x.ld) - inPointOf(x.ld); x.ld.inPoint = minIn; x.ld.outPoint = minIn + w; });
      } else if (mode === 'right') {
        var maxOut = Math.max.apply(null, items.map(function (x) { return outPointOf(x.ld); }));
        items.forEach(function (x) { var w = outPointOf(x.ld) - inPointOf(x.ld); x.ld.outPoint = maxOut; x.ld.inPoint = maxOut - w; });
      } else {
        var avgCenter = items.reduce(function (s, x) { return s + (inPointOf(x.ld) + outPointOf(x.ld)) / 2; }, 0) / items.length;
        items.forEach(function (x) {
          var w = outPointOf(x.ld) - inPointOf(x.ld);
          var ni = Math.max(0, Math.round(avgCenter - w / 2));
          x.ld.inPoint = ni; x.ld.outPoint = ni + w;
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
        x.ld.inPoint = ni; x.ld.outPoint = ni + w;
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
        x.ld.inPoint = reversed[i]; x.ld.outPoint = reversed[i] + w;
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
        x.ld.inPoint = ni; x.ld.outPoint = ni + w;
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
    getBarSelection: function () { return _barSel.slice(); },
    setBarSelection: function (sel) { _barSel = sel; refreshBarSelClasses(); },
  };
})();
