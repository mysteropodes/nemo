// ---- ELEMENTS PANEL (2026-08, Animation 2D) ----
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
// ou rive pour la hiérarchie") — a group used to be a dead-end row: click
// selected its members but there was no way to actually SEE them nested
// underneath, the one thing that makes a "hierarchy" panel read as a
// hierarchy rather than a flat list with a folder icon. Two additions,
// both reusing existing app conventions rather than inventing new ones:
// a real expand/collapse chevron (.larrow, same ▸/▾ glyph-toggle idiom as
// every other collapsible group in this codebase — Trim Paths, Path,
// Duplicator) revealing indented member rows, and a selection-highlight
// (.lrow.act, the SAME class/style timeline.js's own layer rows use for
// "this is the active one") so the panel reflects canvas selection state
// instead of only ever driving it one-way.
(function () {
  // Session-only UI state (which groups are expanded) — never persisted,
  // same "not part of the document" precedent as window._motionExpandedLayer
  // and friends (motion.js) or window._anchorGridOpenFor.
  var expandedGroups = {};
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
  // Builds one shape row — used both at top level and, indented, as a
  // group's expanded member row, so the two never visually drift apart.
  function buildShapeRow(list, c, node, idx, indent) {
    var entry = { strokeId: node.strokeId, sd: node.sd };
    var row = document.createElement('div'); row.className = 'lrow motion-elem-row';
    if (indent) row.style.paddingLeft = (24 + indent) + 'px';
    if (isStrokeSelected(c.li, entry.strokeId)) row.classList.add('act');
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
    row.addEventListener('click', function (e) { applySelection(c.li, [entry.strokeId], e.shiftKey || e.altKey); });
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
        grow.addEventListener('click', function (e) { applySelection(c.li, memberIds, e.shiftKey || e.altKey); });
        grow.addEventListener('dblclick', function (e) { e.stopPropagation(); startRename(grow, node.name, commitGroupRename); });
        grow.addEventListener('contextmenu', function (e) {
          e.preventDefault(); e.stopPropagation();
          if (!window.showContextMenu) return;
          window.showContextMenu(e.clientX, e.clientY, [
            { label: SM.t('elementsRename'), action: function () { startRename(grow, node.name, commitGroupRename); } },
            { label: SM.t('elementsSelectMembers'), action: function () { window.SMMotion.selectShapesByStrokeIds(c.li, memberIds); } },
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
            buildShapeRow(list, c, { strokeId: me.strokeId, sd: me.sd }, shapeIdx++, 20);
          });
        }
      } else {
        buildShapeRow(list, c, node, shapeIdx++, 0);
      }
    });
  }
  window.renderShapesPanel = renderShapesPanel;
})();
