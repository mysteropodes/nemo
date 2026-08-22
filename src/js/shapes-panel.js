// ---- ELEMENTS PANEL (2026-08, Animation 2D + Motion) ----
// Figma/Rive-style shape/group tree, surfaced as its own right-panel
// section (#shapes-sec, index.html) rather than copying Motion's own
// left-side "Éléments" list — Cyril: "surtout motion, en anim 2D peut-être
// plutôt un panel droit". Deliberately NOT a reimplementation: every piece
// of tree-building/selection/group logic is reused as-is from motion.js
// (SMMotion.buildShapeTree/layerElements/selectShapesByStrokeIds/
// elementLabel/liveItemByStrokeId), which was already written mode-
// agnostic for exactly this ("the same function backs both Motion's
// renderElementsList AND Animation 2D's forthcoming layer-row shape list"
// — buildShapeTree's own comment, written 2026-07-31, well before this
// file existed). Only the RENDERING is new here — a right-panel row
// instead of a left-panel one — so the two can never diverge in which
// shapes/groups exist or in what order (CLAUDE.md §3).
//
// Hierarchy pass (2026-08, "graphiquement tu peux te rapprocher de figma
// ou rive pour la hiérarchie") — a real expand/collapse chevron (.larrow,
// same ▸/▾ glyph-toggle idiom as every other collapsible group in this
// codebase) revealing indented member rows, and a selection-highlight
// (.lrow.act) so the panel reflects canvas selection state both ways.
//
// "Shape layer" pass (2026-08, "Elements doit agir un peu comme pour les
// shape layer" — AE's Shape Layer > Contents tree) — three additions,
// each reusing an existing mechanism rather than inventing a parallel one:
//  - Drag-to-reorder: manual mouse drag (NOT HTML5 draggable — same reason
//    timeline.js's own layer-row drag avoids it, inconsistent inside the
//    Tauri webview), moving the LIVE Paper item(s) via insertAbove, the
//    same primitive every other z-order operation in this codebase uses
//    (tools.js/draw-bridge.js/symmetry-bridge.js all call it directly).
//  - Combined Shape modes on a group's context menu: same
//    SMGroup.setGroupCombineMode the existing toolbar buttons already call
//    (timeline.js's updateCombinePanel) — no new boolean-op code.
//  - "Open a shape" for separate Fill/Stroke: this app's data model has no
//    separate Fill/Stroke sub-objects (one Path carries both paint fields
//    at once). Clicking "Fill" or "Stroke" under an expanded shape selects
//    the shape the ORDINARY way (state.tool stays 'select' — Cyril: "la
//    sélection... toujours en select") and scrolls the right panel to
//    that section; Select's own hasSel branch already shows Fill AND
//    Stroke together the instant a shape is selected, so there was no
//    need for a second, fsselect-tool-based selection mode after all.
(function () {
  // Session-only UI state — never persisted, same "not part of the
  // document" precedent as window._motionExpandedLayer and friends
  // (motion.js) or window._anchorGridOpenFor.
  var expandedGroups = {};
  var expandedShapes = {};
  function currentLayer() {
    var li = state.activeLayerIdx;
    return { li: li, ld: state.layers[li] };
  }
  function isStrokeSelected(li, strokeId) {
    if (!window.selectedPaths || !window.selectedPaths.length) return false;
    var item = window.SMMotion.liveItemByStrokeId(li, strokeId);
    return !!item && window.selectedPaths.indexOf(item) >= 0;
  }
  // Multi-select (2026-08, "impossible d'avoir le multiselect shift ou
  // alt dans le panel") — SMMotion.selectShapesByStrokeIds always REPLACES
  // the whole selection, same as every click in this panel used before
  // this fix; there was no additive path at all (confirmed: Motion's own
  // left-panel Éléments list has the identical gap, so there was no
  // existing helper to reuse here). additive=true toggles the clicked
  // strokeId(s) in/out of the CURRENT selection instead of replacing it —
  // same toggle semantics select-bridge.js's own canvas Shift-click
  // already uses (add if entirely absent, remove if already present), so
  // canvas and panel behave identically under Shift/Alt. Alt is treated
  // the same as Shift here — Cyril asked for "shift ou alt", and neither
  // this panel nor Motion's own list has a distinct meaning to give Alt
  // beyond "also multi-select", so inventing one would be undirected
  // scope, not a fix.
  function applySelection(li, strokeIds, additive) {
    if (!additive) { window.SMMotion.selectShapesByStrokeIds(li, strokeIds); return; }
    var items = strokeIds.map(function (sid) { return window.SMMotion.liveItemByStrokeId(li, sid); }).filter(Boolean);
    if (!items.length) return;
    // setActiveLayer(li) unconditionally calls clearSel() (timeline.js),
    // even when li is ALREADY the active layer — which it always is here
    // (this panel only ever lists the current layer's own shapes, see
    // currentLayer()). Calling it anyway wiped whatever was already in
    // selectedPaths a split second before reading it below, so every
    // additive click ever ended up with just the ONE just-clicked item —
    // found live (two Shift-clicks on different shapes left selCount at 1,
    // not 2). Guarded, not removed outright, in case this ever runs
    // against a different layer than the one currently active.
    if (window.state && window.state.activeLayerIdx !== li) window.SM.setActiveLayer(li);
    var sel = window.selectedPaths;
    var allIn = items.every(function (it) { return sel.indexOf(it) >= 0; });
    if (allIn) items.forEach(function (it) { var ix = sel.indexOf(it); if (ix >= 0) sel.splice(ix, 1); });
    else items.forEach(function (it) { if (sel.indexOf(it) < 0) sel.push(it); });
    if (window.state) state.selectedStrokeIndices = sel.map(function (it) { return typeof getSI === 'function' ? getSI(it) : -1; }).filter(function (i2) { return i2 >= 0; });
    if (window.renderArcs) renderArcs();
    if (window.updateUI) updateUI();
    if (window.SMEngineBridge) SMEngineBridge.renderNow();
  }
  // "Open a shape" for separate Fill/Stroke (2026-08, revised — Cyril:
  // "la sélection quand on clic sur un layer... stroke ou fill, toujours
  // en select"). First cut routed through the fs-select tool (_fsSel,
  // tools.js), which works but SWITCHES state.tool away from 'select' —
  // losing the canvas transform box and, since setTool('fsselect') also
  // runs clearSel(), the normal canvas selection. Not needed anyway: the
  // Select tool's own hasSel branch (timeline.js updatePropsContext)
  // already shows Fill AND Stroke sections TOGETHER the moment a shape is
  // selected normally — so "open Fill" / "open Stroke" now just selects
  // the shape the ordinary way (state.tool stays 'select', transform box
  // stays) and scrolls the right panel to bring that specific section
  // into view, rather than pretending there's a fill-only vs stroke-only
  // selection mode that the panel doesn't actually have.
  function selectPaintAspect(li, strokeId, kind) {
    if (window.SM && window.SM.setTool) window.SM.setTool('select');
    window.SMMotion.selectShapesByStrokeIds(li, [strokeId]); // triggers updateUI -> renderShapesPanel itself
    var sec = document.getElementById(kind === 'fill' ? 'fill-sec' : 'stroke-sec');
    if (sec) sec.scrollIntoView({ block: 'nearest' });
  }
  // A brush-drawn stroke with Fill enabled is really TWO live Paths (the
  // stroke itself + a separate linkedFill backdrop, draw-bridge.js) folded
  // into one Elements row by layerElements (motion.js) — see that
  // function's own comment. The Fill aspect must resolve to the REAL
  // companion item, not the stroke's own strokeId, or clicking/dragging
  // "Fill" would silently operate on the wrong Path.
  function paintStrokeIdFor(entry, kind) {
    if (kind === 'fill' && entry.sd.__linkedFillStrokeId) return entry.sd.__linkedFillStrokeId;
    return entry.strokeId;
  }
  // Front-most-first order for the two paint sub-rows ("le stroke et
  // fill... pouvoir se distinguer" + "pourquoi pas possible de select fill
  // et stroke individuellement pour les réorganiser") — top of the pair
  // reads as "in front", matching the drag-swap below. Two real different
  // cases: a linked-fill pair is genuinely two separate Paths with a real
  // z-position (compared via their index in layer.children); a plain
  // shape has ONE Path with both paint fields, ordered by its
  // data.paintOrder flag instead (the SAME 'fillFirst'/'strokeFirst' the
  // existing right-panel Paint Order toggle already writes — see
  // timeline.js's setPaintOrder).
  function paintRowOrder(c, entry) {
    if (entry.sd.__linkedFillStrokeId) {
      var layer = window.userLayers[c.li];
      var fillItem = window.SMMotion.liveItemByStrokeId(c.li, entry.sd.__linkedFillStrokeId);
      var strokeItem = window.SMMotion.liveItemByStrokeId(c.li, entry.strokeId);
      var fi = fillItem ? layer.children.indexOf(fillItem) : -1;
      var si = strokeItem ? layer.children.indexOf(strokeItem) : -1;
      return fi > si ? ['fill', 'stroke'] : ['stroke', 'fill'];
    }
    return entry.sd.paintOrder === 'strokeFirst' ? ['stroke', 'fill'] : ['fill', 'stroke'];
  }
  // Same inline input-swap idiom as timeline.js's startLayerRename and
  // motion.js's startShapeTreeRename — a third small, stable copy rather
  // than exporting/reusing motion.js's version, which calls Motion's OWN
  // renderLayerList/renderTimeline on finish (this panel needs to call
  // renderShapesPanel instead). CLAUDE.md §3 cares about duplicating logic
  // that could DRIFT (curve math, boolean ops); this is ~10 lines of DOM
  // wiring already intentionally copied twice before in this exact
  // codebase for this exact micro-pattern.
  function startRename(rowEl, currentName, commit) {
    var nm = rowEl.querySelector('.lnm'); if (!nm) return;
    var input = document.createElement('input'); input.type = 'text'; input.value = currentName;
    input.style.cssText = 'width:100%;background:var(--bg);border:1px solid var(--accent);color:var(--text);font-size:11px;border-radius:4px;padding:1px 4px;outline:none;';
    nm.innerHTML = ''; nm.appendChild(input); input.focus(); input.select();
    var done = false;
    function finish() { if (done) return; done = true; var v = input.value.trim(); if (v) commit(v); else renderShapesPanel(); }
    input.addEventListener('keydown', function (e) { e.stopPropagation(); if (e.key === 'Enter') finish(); else if (e.key === 'Escape') { done = true; renderShapesPanel(); } });
    input.addEventListener('blur', finish);
    input.addEventListener('mousedown', function (e) { e.stopPropagation(); });
    input.addEventListener('dblclick', function (e) { e.stopPropagation(); });
  }
  // ---- Drag-to-reorder (2026-08, "on ne peut pas réarranger l'ordre") ----
  // Manual mouse drag mirroring timeline.js's own _layerDrag exactly (see
  // that block's comment for why not HTML5 draggable) — mousedown arms it,
  // a 4px move threshold before it's a "real" drag (so a plain click still
  // reaches the row's click handler), global mousemove tracks the
  // drag-over target, global mouseup performs the move. Only TOP-LEVEL
  // rows (data-toplevel) are valid drag sources/targets in v1 — reordering
  // a group's own members, or dragging a shape into/out of a group, is a
  // separate, bigger feature (dropping ONTO a group here just moves the
  // dragged item to sit right in front of that whole group block).
  // "paint" kind (2026-08, "pourquoi il est pas possible de select fill et
  // stroke individuellement pour les réorganiser") — id is a composite
  // "<shapeStrokeId>:<fill|stroke>" key; the only valid drop target is the
  // SAME shape's OTHER paint sub-row (see performPaintSwap), so this never
  // needs to match against data-toplevel rows at all.
  var _elDrag = { active: false, kind: null, id: null, startY: 0, moved: false };
  function dragSelectorFor(kind, id) {
    if (kind === 'group') return '.lrow[data-gid="' + id + '"]';
    if (kind === 'paint') return '.lrow[data-paintid="' + id + '"]';
    return '.lrow[data-strokeid="' + id + '"]';
  }
  // Floating drag-ghost (2026-08, "quand on drag la ligne doit suivre la
  // souris pour mieux voir ce que l'on prend") — the row being dragged
  // used to just dim in place (.dragging{opacity:.4}, same as timeline.js's
  // own layer-row drag) with no feedback attached to the cursor itself.
  // A cloned copy of the row, position:fixed, its own top re-set on every
  // mousemove tick, gives a real "picked up" affordance — removed on
  // mouseup regardless of whether the drop lands anywhere.
  var _dragGhost = null;
  function startDragGhost(rowEl, e) {
    var rect = rowEl.getBoundingClientRect();
    _dragGhost = rowEl.cloneNode(true);
    _dragGhost.classList.remove('dragging', 'act', 'drag-over');
    _dragGhost.style.position = 'fixed';
    _dragGhost.style.left = rect.left + 'px';
    _dragGhost.style.width = rect.width + 'px';
    _dragGhost.style.top = (e.clientY - rect.height / 2) + 'px';
    _dragGhost.style.pointerEvents = 'none';
    _dragGhost.style.zIndex = 99999;
    _dragGhost.style.opacity = '0.92';
    _dragGhost.style.boxShadow = '0 6px 16px rgba(0,0,0,.5)';
    _dragGhost.style.background = 'var(--panel3, #2a2a33)';
    _dragGhost.style.borderRadius = '4px';
    _dragGhost.style.display = 'flex';
    _dragGhost.style.alignItems = 'center';
    document.body.appendChild(_dragGhost);
  }
  function moveDragGhost(e) {
    if (_dragGhost) _dragGhost.style.top = (e.clientY - _dragGhost.offsetHeight / 2) + 'px';
  }
  function stopDragGhost() {
    if (_dragGhost) { _dragGhost.remove(); _dragGhost = null; }
  }
  window.addEventListener('mousemove', function (e) {
    if (!_elDrag.active) return;
    if (!_elDrag.moved) {
      if (Math.abs(e.clientY - _elDrag.startY) < 4) return;
      _elDrag.moved = true;
      var src = document.querySelector(dragSelectorFor(_elDrag.kind, _elDrag.id));
      if (src) { src.classList.add('dragging'); startDragGhost(src, e); }
    }
    moveDragGhost(e);
    var list = document.getElementById('shapes-list'); if (!list) return;
    Array.prototype.forEach.call(list.querySelectorAll('.lrow'), function (r) { r.classList.remove('drag-over'); });
    var el = document.elementFromPoint(e.clientX, e.clientY);
    var row = _elDrag.kind === 'paint'
      ? el && el.closest('#shapes-list .lrow[data-paintid]')
      : el && el.closest('#shapes-list .lrow[data-toplevel]');
    if (row) row.classList.add('drag-over');
  });
  window.addEventListener('mouseup', function () {
    if (!_elDrag.active) return;
    if (_elDrag.moved) {
      window._elDragJustEnded = true;
      var list = document.getElementById('shapes-list');
      var overRow = list && list.querySelector('.lrow.drag-over');
      if (overRow) performReorder(overRow);
    }
    stopDragGhost();
    var list2 = document.getElementById('shapes-list');
    if (list2) Array.prototype.forEach.call(list2.querySelectorAll('.lrow'), function (r) { r.classList.remove('dragging', 'drag-over'); });
    _elDrag.active = false; _elDrag.moved = false;
  });
  // Fill<->Stroke swap (2026-08, "pourquoi il est pas possible de select
  // fill et stroke individuellement pour les réorganiser") — the only
  // valid drop target for a 'paint' drag is the SAME shape's other paint
  // sub-row (data-paintid's shapeStrokeId half must match); anything else
  // is a no-op. Two real cases, matching paintRowOrder's own split: a
  // linked-fill pair is two separate live Paths, reordered via the SAME
  // insertAbove primitive every other z-order operation in this codebase
  // uses; a plain shape is ONE Path with both paint fields, so there is no
  // real z-order to change — the drag instead toggles data.paintOrder
  // (the SAME 'fillFirst'/'strokeFirst' flag the right-panel Paint Order
  // toggle already writes, engine-bridge.js:1606), scoped to just this
  // one shape rather than routed through setPaintOrder (which also
  // overwrites state.paintOrder, the default for the NEXT shape drawn —
  // an unwanted global side effect for what should be a per-shape swap).
  function performPaintSwap(overRow) {
    var destPaintId = overRow.dataset.paintid;
    if (!destPaintId || destPaintId === _elDrag.id) return;
    var srcParts = _elDrag.id.split(':'), destParts = destPaintId.split(':');
    if (srcParts[0] !== destParts[0] || srcParts[1] === destParts[1]) return;
    var shapeStrokeId = srcParts[0], srcKind = srcParts[1];
    var c = currentLayer(); if (!c.ld) return;
    var entry = window.SMMotion.layerElements(c.li, c.ld).filter(function (e) { return e.strokeId === shapeStrokeId; })[0];
    if (!entry) return;
    if (entry.sd.__linkedFillStrokeId) {
      var fillItem = window.SMMotion.liveItemByStrokeId(c.li, entry.sd.__linkedFillStrokeId);
      var strokeItem = window.SMMotion.liveItemByStrokeId(c.li, entry.strokeId);
      if (!fillItem || !strokeItem) return;
      pushUndo();
      if (srcKind === 'fill') fillItem.insertAbove(strokeItem); else strokeItem.insertAbove(fillItem);
    } else {
      var item = window.SMMotion.liveItemByStrokeId(c.li, entry.strokeId);
      if (!item) return;
      pushUndo();
      item.data = item.data || {};
      item.data.paintOrder = srcKind === 'fill' ? 'fillFirst' : 'strokeFirst';
    }
    saveActiveLayerFrame();
    renderShapesPanel();
    if (window.renderArcs) renderArcs();
    if (window.SMEngineBridge) SMEngineBridge.renderNow();
  }
  function performReorder(overRow) {
    if (_elDrag.kind === 'paint') { performPaintSwap(overRow); return; }
    var c = currentLayer(); if (!c.ld) return;
    var destGid = overRow.dataset.gid, destStrokeId = overRow.dataset.strokeid;
    if (_elDrag.kind === 'group' && destGid === _elDrag.id) return; // dropped on itself
    if (_elDrag.kind === 'shape' && destStrokeId === _elDrag.id) return;
    var destItem;
    if (destGid) {
      var destMembers = window.SMMotion.layerElements(c.li, c.ld).filter(function (e) { return e.sd.groupId === destGid; });
      if (!destMembers.length) return;
      // Front-most (last) member — the dragged block lands adjacent to the
      // WHOLE group rather than injected into its middle.
      destItem = window.SMMotion.liveItemByStrokeId(c.li, destMembers[destMembers.length - 1].strokeId);
    } else if (destStrokeId) {
      destItem = window.SMMotion.liveItemByStrokeId(c.li, destStrokeId);
    }
    if (!destItem) return;
    var srcItems;
    if (_elDrag.kind === 'group') {
      var srcMembers = window.SMMotion.layerElements(c.li, c.ld).filter(function (e) { return e.sd.groupId === _elDrag.id; });
      srcItems = srcMembers.map(function (e) { return window.SMMotion.liveItemByStrokeId(c.li, e.strokeId); }).filter(Boolean);
    } else {
      var it = window.SMMotion.liveItemByStrokeId(c.li, _elDrag.id);
      srcItems = it ? [it] : [];
    }
    if (!srcItems.length) return;
    pushUndo();
    var anchor = destItem;
    srcItems.forEach(function (it) { it.insertAbove(anchor); anchor = it; });
    saveActiveLayerFrame();
    renderShapesPanel();
    if (window.renderArcs) renderArcs();
    if (window.SMEngineBridge) SMEngineBridge.renderNow();
  }
  function armDrag(e, kind, id) {
    if (e.button !== 0 || e.target.closest('.lico')) return;
    _elDrag.active = true; _elDrag.kind = kind; _elDrag.id = id; _elDrag.startY = e.clientY; _elDrag.moved = false;
  }
  // Fill/Stroke sub-row under an expanded shape — reuses selectPaintAspect
  // above, no boolean-op or paint logic of its own.
  //
  // Alignment fix (2026-08, "fais attention à la hiérarchie décalée
  // toujours quand on ouvre une shape") — a shape row's own swatch sits
  // AFTER its chevron/spacer .lico (20px + gap), but this sub-row had NO
  // such leading element, only padding-left — at indent+20 that put its
  // swatch to the LEFT of the shape's own swatch above it (44px vs the
  // parent's 50px), reading as mis-nested rather than nested further in.
  // Same leading spacer as the parent's own non-openable-row case fixes
  // it: both start their swatch at an identical spacer-width offset, so
  // the +20 padding-left difference is the ONLY thing separating them.
  function buildPaintSubRow(list, c, entry, kind, indent) {
    var row = document.createElement('div'); row.className = 'lrow motion-elem-row motion-elem-subrow';
    row.style.paddingLeft = (24 + indent) + 'px';
    row.dataset.paintid = entry.strokeId + ':' + kind;
    if (isStrokeSelected(c.li, paintStrokeIdFor(entry, kind))) row.classList.add('act');
    var spacer = document.createElement('span'); spacer.className = 'lico larrow-spacer';
    row.appendChild(spacer);
    var swatch = document.createElement('div'); swatch.className = 'motion-elem-swatch';
    swatch.style.background = (kind === 'fill' ? entry.sd.fillColor : entry.sd.strokeColor) || 'transparent';
    var nm = document.createElement('div'); nm.className = 'lnm';
    nm.textContent = SM.t(kind === 'fill' ? 'elementsFill' : 'elementsStroke');
    row.appendChild(swatch); row.appendChild(nm);
    row.addEventListener('click', function (e) {
      e.stopPropagation();
      if (window._elDragJustEnded) { window._elDragJustEnded = false; return; }
      selectPaintAspect(c.li, paintStrokeIdFor(entry, kind), kind);
    });
    row.addEventListener('mousedown', function (e) { e.stopPropagation(); armDrag(e, 'paint', entry.strokeId + ':' + kind); });
    list.appendChild(row);
  }
  // Builds one shape row — used both at top level and, indented, as a
  // group's expanded member row, so the two never visually drift apart.
  function buildShapeRow(list, c, node, idx, indent, topLevel) {
    var entry = { strokeId: node.strokeId, sd: node.sd };
    var row = document.createElement('div'); row.className = 'lrow motion-elem-row';
    if (indent) row.style.paddingLeft = (24 + indent) + 'px';
    if (topLevel) row.dataset.toplevel = '1';
    row.dataset.strokeid = entry.strokeId;
    if (isStrokeSelected(c.li, entry.strokeId)) row.classList.add('act');
    // "Open a shape" (2026-08) — a chevron only when there's something to
    // open (a raster has no fill/stroke paint fields at all).
    var canOpen = !entry.sd.isRaster && (entry.sd.fillColor || entry.sd.strokeColor);
    var expanded = canOpen && !!expandedShapes[entry.strokeId];
    if (canOpen) {
      var arrow = document.createElement('span'); arrow.className = 'lico larrow'; arrow.textContent = expanded ? '▾' : '▸';
      arrow.addEventListener('click', function (e) {
        e.stopPropagation();
        expandedShapes[entry.strokeId] = !expanded;
        renderShapesPanel();
      });
      row.appendChild(arrow);
    } else {
      var spacer = document.createElement('span'); spacer.className = 'lico larrow-spacer';
      row.appendChild(spacer);
    }
    var swatch = document.createElement('div'); swatch.className = 'motion-elem-swatch';
    if (entry.sd.isRaster) { swatch.classList.add('icon'); swatch.innerHTML = ICO_IMAGE; }
    else {
      // Fill + Stroke, distinguishable on one swatch (2026-08, "le stroke
      // et fill devrait être dans le même groupe et pouvoir se distinguer
      // l'un de l'autre visuellement") — this app's data model has no
      // separate Fill/Stroke sub-items to list (one Path carries both
      // paint fields at once, confirmed against engine-bridge.js's own
      // per-item construction), so "same group, visually distinct" means
      // both painted on the ONE existing swatch rather than the old
      // `fillColor || strokeColor` fallback, which silently dropped
      // whichever one lost. Center = fill (or empty if none), ring =
      // stroke color when the shape has a real stroke — same "filled
      // square with an outline in a different color" convention
      // Illustrator/Figma's own swatches use.
      swatch.style.background = entry.sd.fillColor || 'transparent';
      if (entry.sd.strokeColor) { swatch.style.borderColor = entry.sd.strokeColor; swatch.style.borderWidth = '2px'; }
    }
    var nm = document.createElement('div'); nm.className = 'lnm';
    nm.textContent = window.SMMotion.elementLabel(entry, idx, c.ld);
    row.appendChild(swatch); row.appendChild(nm);
    row.addEventListener('click', function (e) {
      if (window._elDragJustEnded) { window._elDragJustEnded = false; return; } // trailing click after a drag-drop, see armDrag/mouseup
      applySelection(c.li, [entry.strokeId], e.shiftKey || e.altKey);
    });
    if (topLevel) row.addEventListener('mousedown', function (e) { armDrag(e, 'shape', entry.strokeId); });
    function commitShapeRename(v) {
      pushUndo();
      if (!c.ld.shapeNames) c.ld.shapeNames = {};
      c.ld.shapeNames[entry.strokeId] = v;
      saveActiveLayerFrame(); renderShapesPanel();
    }
    row.addEventListener('dblclick', function (e) { e.stopPropagation(); startRename(row, nm.textContent, commitShapeRename); });
    row.addEventListener('contextmenu', function (e) {
      e.preventDefault(); e.stopPropagation();
      if (!window.showContextMenu) return;
      window.showContextMenu(e.clientX, e.clientY, [
        { label: SM.t('elementsRename'), action: function () { startRename(row, nm.textContent, commitShapeRename); } },
        { label: SM.t('elementsSelect'), action: function () { window.SMMotion.selectShapesByStrokeIds(c.li, [entry.strokeId]); } },
      ]);
    });
    list.appendChild(row);
    if (expanded) {
      // Front-most first (paintRowOrder) so the row ORDER itself already
      // reads as "what's on top" before the user ever drags anything.
      paintRowOrder(c, entry).forEach(function (kind) {
        if (kind === 'fill' && entry.sd.fillColor) buildPaintSubRow(list, c, entry, 'fill', indent + 20);
        if (kind === 'stroke' && entry.sd.strokeColor) buildPaintSubRow(list, c, entry, 'stroke', indent + 20);
      });
    }
  }
  function renderShapesPanel() {
    var list = document.getElementById('shapes-list');
    if (!list) return;
    list.innerHTML = '';
    if (!window.SMMotion || !window.SMMotion.buildShapeTree) return;
    var c = currentLayer();
    if (!c.ld) return;
    var tree = window.SMMotion.buildShapeTree(c.li, c.ld);
    if (!tree.length) {
      var empty = document.createElement('div'); empty.className = 'pr';
      empty.style.cssText = 'color:var(--text-dim);font-size:10.5px';
      empty.textContent = SM && SM.t ? SM.t('elementsPanelEmpty') : 'No elements';
      list.appendChild(empty);
      return;
    }
    var shapeIdx = 0;
    tree.forEach(function (node) {
      if (node.type === 'group') {
        // Recompute this group's own member strokeIds from the flat list
        // (layerElements), not the already-collapsed tree — same reason
        // motion.js's own renderElementsList does this (click-select,
        // rename and now expand-render all need the full membership, not
        // just "a group exists here").
        var memberEntries = window.SMMotion.layerElements(c.li, c.ld).filter(function (e) { return e.sd.groupId === node.gid; });
        var memberIds = memberEntries.map(function (e) { return e.strokeId; });
        var expanded = !!expandedGroups[node.gid];
        var anySelected = memberIds.some(function (sid) { return isStrokeSelected(c.li, sid); });

        var grow = document.createElement('div'); grow.className = 'lrow motion-elem-row motion-elem-group';
        grow.dataset.toplevel = '1'; grow.dataset.gid = node.gid;
        if (anySelected) grow.classList.add('act');
        var arrow = document.createElement('span'); arrow.className = 'lico larrow'; arrow.textContent = expanded ? '▾' : '▸';
        arrow.addEventListener('click', function (e) {
          e.stopPropagation();
          expandedGroups[node.gid] = !expanded;
          renderShapesPanel();
        });
        var gswatch = document.createElement('div'); gswatch.className = 'motion-elem-swatch icon'; gswatch.innerHTML = ICO_GROUP;
        var gnm = document.createElement('div'); gnm.className = 'lnm'; gnm.textContent = node.name;
        grow.appendChild(arrow); grow.appendChild(gswatch); grow.appendChild(gnm);
        function commitGroupRename(v) {
          pushUndo();
          if (window.SMGroup && SMGroup.renameGroup) SMGroup.renameGroup(node.gid, c.ld, v, memberIds);
          saveActiveLayerFrame(); renderShapesPanel();
        }
        grow.addEventListener('click', function (e) {
          if (window._elDragJustEnded) { window._elDragJustEnded = false; return; }
          applySelection(c.li, memberIds, e.shiftKey || e.altKey);
        });
        grow.addEventListener('mousedown', function (e) { armDrag(e, 'group', node.gid); });
        grow.addEventListener('dblclick', function (e) { e.stopPropagation(); startRename(grow, node.name, commitGroupRename); });
        grow.addEventListener('contextmenu', function (e) {
          e.preventDefault(); e.stopPropagation();
          if (!window.showContextMenu) return;
          // Combined Shape (2026-08, "peut être avoir les combined shape
          // option pour le groupe") — same SMGroup.setGroupCombineMode the
          // existing toolbar buttons call (timeline.js's updateCombinePanel/
          // COMBINE_MODE_BTN_IDS), just reachable from this row's own menu
          // instead of only via canvas selection + the right-panel toolbar.
          // groupSelection() already seeds ld.groups[gid].combineMode:'none'
          // at creation time, so this never hits setGroupCombineMode's own
          // "group doesn't exist yet" early-return.
          var curMode = (c.ld.groups && c.ld.groups[node.gid] && c.ld.groups[node.gid].combineMode) || 'none';
          function combineItem(mode, key) {
            return { label: SM.t(key) + (curMode === mode ? ' ✓' : ''), action: function () {
              if (window.SMGroup && SMGroup.setGroupCombineMode) SMGroup.setGroupCombineMode(node.gid, c.ld, mode);
              renderShapesPanel();
            } };
          }
          window.showContextMenu(e.clientX, e.clientY, [
            { label: SM.t('elementsRename'), action: function () { startRename(grow, node.name, commitGroupRename); } },
            { label: SM.t('elementsSelectMembers'), action: function () { window.SMMotion.selectShapesByStrokeIds(c.li, memberIds); } },
            { sep: true },
            combineItem('unite', 'combineUnion'),
            combineItem('subtract', 'combineSubtract'),
            combineItem('intersect', 'combineIntersect'),
            combineItem('exclude', 'combineExclude'),
            combineItem('none', 'combineNone'),
            { sep: true },
            { label: SM.t('elementsUngroup'), action: function () {
              pushUndo();
              memberIds.forEach(function (sid) {
                var it = window.SMMotion.liveItemByStrokeId(c.li, sid);
                if (it && it.data) delete it.data.groupId;
              });
              if (c.ld.groups) delete c.ld.groups[node.gid];
              delete expandedGroups[node.gid];
              saveActiveLayerFrame(); renderShapesPanel();
              if (window.SMEngineBridge) SMEngineBridge.renderNow();
            } },
          ]);
        });
        list.appendChild(grow);
        if (expanded) {
          memberEntries.forEach(function (me) {
            buildShapeRow(list, c, { strokeId: me.strokeId, sd: me.sd }, shapeIdx++, 20, false);
          });
        }
      } else {
        buildShapeRow(list, c, node, shapeIdx++, 0, true);
      }
    });
  }
  window.renderShapesPanel = renderShapesPanel;
})();
