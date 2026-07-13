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
  }

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
  // Maps a bar's row element to/from its layer index — buildBar populates
  // both below. The marquee only has DOM hit-test info (needs row->li); a
  // group-drag needs the reverse (li->row) to update each member's bar.
  var _rowLayerIdx = new WeakMap();
  var _liToRow = {};

  // Drag state is a single module-level singleton (same idiom as ui.js's
  // initWaDrag: window-level mousemove/mouseup, not a per-drag add/remove
  // pair) — only one bar (or group) can be dragged at a time anyway.
  var _drag = null; // {li, row, type:'in'|'out'|'both', startX, origIn, origOut} | {group:true, startX, members:[{li,row,origIn,origOut}]}
  function onDown(li, row, type, e) {
    e.stopPropagation(); e.preventDefault();
    var ld = state.layers[li]; if (!ld) return;
    if (window.pushUndo) pushUndo(); // one undo step for the whole drag, not one per mousemove
    // Grabbing the BODY of an already-selected bar (part of a multi-select)
    // moves the whole group together; grabbing an edge handle always trims
    // just that one bar, even inside a selection (matches motion.js's own
    // keyframe group-drag: only a body/point drag groups, not a handle).
    if (type === 'both' && isBarSelected(li) && _barSel.length > 1) {
      var members = _barSel.map(function (mli) {
        var mld = state.layers[mli], mrow = _liToRow[mli];
        if (!mld || !mrow) return null;
        return { li: mli, row: mrow, origIn: inPointOf(mld), origOut: outPointOf(mld) };
      }).filter(Boolean);
      _drag = { group: true, startX: e.clientX, members: members };
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
      window.showContextMenu(e.clientX, e.clientY, [
        { label: 'Réinitialiser (pleine durée)', action: function () { if (window.pushUndo) pushUndo(); delete state.layers[li].inPoint; delete state.layers[li].outPoint; if (window.renderTimeline) renderTimeline(); if (window.loadFrame) loadFrame(state.currentFrame); if (window.SMEngineBridge) SMEngineBridge.renderNow(); } },
      ]);
    });
    // Marquee starts on a mousedown that lands on the ROW's own empty
    // background — this row's only real content is the bar itself, so
    // e.target===row means the click missed the bar entirely.
    row.addEventListener('mousedown', function (e) { if (e.target === row) startMarquee(e); });
  }

  window.SMLayerInOut = { inPointOf: inPointOf, outPointOf: outPointOf, hasCustomRange: hasCustomRange, buildBar: buildBar, updateBar: updateBar };
})();
