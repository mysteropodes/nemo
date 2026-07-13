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
  function inPointOf(ld) { return window.layerInPoint ? layerInPoint(ld) : (ld.inPoint || 0); }
  function outPointOf(ld) { return window.layerOutPoint ? layerOutPoint(ld) : (ld.outPoint != null ? ld.outPoint : state.totalFrames - 1); }
  // A manually-dragged range OR an auto-detected blank-keyframe trim both
  // count as "not full range" for styling — a naturally-shortened bar
  // (layer stops drawing partway through) should read as visually distinct
  // from the full-timeline default too, not just a manual drag.
  function hasCustomRange(ld) { return !!(ld.inPoint || ld.outPoint != null || outPointOf(ld) < state.totalFrames - 1); }

  // Skew Pro's own timeline (the reference screenshot) colors every layer
  // bar with that layer's OWN assigned color, not one uniform accent tint —
  // makes a busy timeline with many layers scannable at a glance. `ld.color`
  // is already assigned to every layer at creation (app.js nextLayerColor).
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
    // Layer-color tint ONLY on a genuinely trimmed range — feedback: the
    // full-range (untouched, the overwhelming common case) bar tinting
    // EVERY layer's row with its own color made Animation 2D's whole
    // timeline "look nothing like before" at a glance, since that neutral
    // barely-visible strip is what every layer showed pre-in/out-point-
    // feature too. Reverting to the CSS .full-range default (plain white,
    // style.css) for that case — only a layer that's actually trimmed
    // (manually or via the blank-keyframe auto-detect) gets its own color,
    // exactly the "stands out because it's different" signal intended.
    if (custom) {
      bar.style.background = hexToRgba(ld.color, 0.55);
      bar.style.borderColor = hexToRgba(ld.color, 0.95);
    } else {
      bar.style.background = '';
      bar.style.borderColor = '';
    }
    var handles = bar.querySelectorAll('.layer-inout-handle');
    for (var h = 0; h < handles.length; h++) handles[h].style.background = custom ? (ld.color || '') : '';
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
        tick.title = 'Keyframe — frame ' + (frameIdx + 1) + ' (glisser pour retimer)';
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
  var _barSel = []; // layer indices whose bar is selected
  function isBarSelected(li) { return _barSel.indexOf(li) >= 0; }
  var _marquee = null; // {startY, rectEl, moved}
  function startMarquee(e) {
    var rect = document.createElement('div'); rect.className = 'layer-inout-marquee-rect';
    document.body.appendChild(rect);
    _marquee = { startY: e.clientY, rectEl: rect, moved: false };
  }
  function applyMarqueeSelection(y0, y1) {
    var sel = [];
    document.querySelectorAll('.layer-inout-bar').forEach(function (bar) {
      var row = bar.parentElement; if (!row) return;
      var li = _rowLayerIdx.get(row); if (li == null) return;
      var b = row.getBoundingClientRect();
      var cy = b.top + b.height / 2;
      var hit = cy >= y0 && cy <= y1;
      bar.classList.toggle('sel', hit);
      if (hit) sel.push(li);
    });
    _barSel = sel;
  }
  function updateMarquee(e) {
    if (!_marquee) return;
    if (Math.abs(e.clientY - _marquee.startY) > 3) _marquee.moved = true;
    var y0 = Math.min(_marquee.startY, e.clientY), y1 = Math.max(_marquee.startY, e.clientY);
    var r = _marquee.rectEl;
    r.style.top = y0 + 'px'; r.style.left = '0'; r.style.right = '0'; r.style.height = (y1 - y0) + 'px';
    if (_marquee.moved) applyMarqueeSelection(y0, y1);
  }
  function endMarquee() {
    if (!_marquee) return;
    var moved = _marquee.moved;
    _marquee.rectEl.remove();
    _marquee = null;
    if (!moved) { clearBarSel(); } // plain click on empty space clears selection
  }
  function clearBarSel() {
    _barSel = [];
    document.querySelectorAll('.layer-inout-bar.sel').forEach(function (b) { b.classList.remove('sel'); });
  }
  function refreshBarSelClasses() {
    document.querySelectorAll('.layer-inout-bar').forEach(function (bar) {
      var row = bar.parentElement, rli = row && _rowLayerIdx.get(row);
      bar.classList.toggle('sel', rli != null && isBarSelected(rli));
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
  function toggleBarSel(li) {
    var idx = _barSel.indexOf(li);
    if (idx >= 0) _barSel.splice(idx, 1); else _barSel.push(li);
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
  function onDown(li, row, type, e) {
    e.stopPropagation(); e.preventDefault();
    var ld = state.layers[li]; if (!ld) return;
    if (e.shiftKey || e.metaKey || e.ctrlKey) { toggleBarSel(li); return; } // select-only, no drag
    if (window.pushUndo) pushUndo(); // one undo step for the whole drag, not one per mousemove
    // Grabbing ANY part of an already-selected bar (body OR an edge handle)
    // moves/trims the whole group together — feedback: "je ne peux pas
    // select les inpoint ou outpoint avec le rect de select + drag" (an
    // earlier version restricted grouping to body-drags only, on the
    // mistaken assumption edge handles should always stay single-bar; that
    // was wrong, group-trimming in/out points together is exactly the
    // point of the marquee select here).
    if (isBarSelected(li) && _barSel.length > 1) {
      var members = _barSel.map(function (mli) {
        var mld = state.layers[mli], mrow = _liToRow[mli];
        if (!mld || !mrow) return null;
        return { li: mli, row: mrow, origIn: inPointOf(mld), origOut: outPointOf(mld) };
      }).filter(Boolean);
      _drag = { group: true, type: type, startX: e.clientX, members: members };
      return;
    }
    _drag = { li: li, row: row, type: type, startX: e.clientX, origIn: inPointOf(ld), origOut: outPointOf(ld) };
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
  document.addEventListener('mouseup', function () {
    endMarquee();
    if (!_drag) return;
    _drag = null;
    // Full rebuild only once, at drag END: other rows' bars don't move
    // during the drag so a per-move renderTimeline() would be pure waste
    // (CLAUDE.md §5 — avoid unnecessary per-move rebuild work); a final
    // renderLayerList() picks up anything the layer-panel might show that
    // depends on the range (e.g. a future disabled/dimmed state).
    if (window.renderLayerList) renderLayerList();
  });

  // ---- batch operations on the current bar selection (Skew Pro punch
  // list: Align/Distribute/Flip/Select Every/Invert Selection) — all
  // require >=2 selected bars except Invert, which works off whatever's
  // currently selected (possibly zero). One pushUndo() per operation.
  function selectedLayers() {
    return _barSel.map(function (li) { return { li: li, ld: state.layers[li] }; }).filter(function (x) { return x.ld; });
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
  // min-in..max-out span, preserving each bar's own duration — covers
  // Skew Pro's "Distribute" directly; its separate drag-the-selection-edge
  // "Space" tool is the same underlying idea (even spacing) via a
  // different gesture, folded into this one button for v1.
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
  // bar keeps its own duration, just swaps which "slot" it occupies) —
  // Skew Pro's "Flip Horizontally".
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
  function selectEveryNth(n) {
    n = Math.max(2, parseInt(n, 10) || 2);
    var sorted = _barSel.slice().sort(function (a, b) { return inPointOf(state.layers[a]) - inPointOf(state.layers[b]); });
    _barSel = sorted.filter(function (_li, i) { return i % n === 0; });
    document.querySelectorAll('.layer-inout-bar').forEach(function (bar) {
      var row = bar.parentElement, li = row && _rowLayerIdx.get(row);
      bar.classList.toggle('sel', li != null && isBarSelected(li));
    });
    if (window.showToast) showToast(_barSel.length + ' calque(s) sélectionné(s)');
  }
  // Selects every layer that HAS a bar row currently rendered and is NOT
  // already selected — inverts within the same universe Box Select draws
  // its marquee over, matching Skew Pro's own "I" shortcut.
  function invertBarSelection() {
    var all = [];
    document.querySelectorAll('.layer-inout-bar').forEach(function (bar) {
      var row = bar.parentElement, li = row && _rowLayerIdx.get(row);
      if (li != null) all.push(li);
    });
    _barSel = all.filter(function (li) { return !isBarSelected(li); });
    document.querySelectorAll('.layer-inout-bar').forEach(function (bar) {
      var row = bar.parentElement, li = row && _rowLayerIdx.get(row);
      bar.classList.toggle('sel', li != null && isBarSelected(li));
    });
  }

  // Builds the bar + its two handles into `row` (a .frow — Animation 2D's
  // per-layer frame row, or Motion mode's collapsed layer spacer row) and
  // wires its drag handlers. Idempotent-safe to call once per row per
  // render pass (renderTimeline/renderTimelineMotion rebuild rows from
  // scratch every time, same as every other overlay in this codebase).
  function buildBar(row, li) {
    row.style.position = 'relative';
    var bar = document.createElement('div'); bar.className = 'layer-inout-bar' + (isBarSelected(li) ? ' sel' : '');
    var hleft = document.createElement('div'); hleft.className = 'layer-inout-handle left';
    var hright = document.createElement('div'); hright.className = 'layer-inout-handle right';
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
      // Batch ops (Skew Pro punch list) act on the WHOLE current selection,
      // not just the bar that was right-clicked — offered once >=2 bars are
      // selected, regardless of which one you right-click.
      if (_barSel.length >= 2) {
        menu.push({ sep: true });
        menu.push({ label: 'Aligner à gauche (in)', action: function () { alignBars('left'); } });
        menu.push({ label: 'Aligner au centre', action: function () { alignBars('center'); } });
        menu.push({ label: 'Aligner à droite (out)', action: function () { alignBars('right'); } });
        menu.push({ label: 'Distribuer uniformément', action: function () { distributeBars(); } });
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

  window.SMLayerInOut = {
    inPointOf: inPointOf, outPointOf: outPointOf, hasCustomRange: hasCustomRange, buildBar: buildBar, updateBar: updateBar,
    alignBars: alignBars, distributeBars: distributeBars, flipBars: flipBars, selectEveryNth: selectEveryNth, invertBarSelection: invertBarSelection,
    getBarSelection: function () { return _barSel.slice(); },
  };
})();
