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
(function () {
  function currentLayer() {
    var li = state.activeLayerIdx;
    return { li: li, ld: state.layers[li] };
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
        var grow = document.createElement('div'); grow.className = 'lrow motion-elem-row motion-elem-group';
        var gswatch = document.createElement('div'); gswatch.className = 'motion-elem-swatch'; gswatch.textContent = '▤'; gswatch.style.background = 'transparent';
        var gnm = document.createElement('div'); gnm.className = 'lnm'; gnm.textContent = node.name;
        grow.appendChild(gswatch); grow.appendChild(gnm);
        // Recompute this group's own member strokeIds from the flat list
        // (layerElements), not the already-collapsed tree — same reason
        // motion.js's own renderElementsList does this (click-select and
        // rename both need the full membership, not just "a group exists
        // here").
        var memberIds = window.SMMotion.layerElements(c.li, c.ld).filter(function (e) { return e.sd.groupId === node.gid; }).map(function (e) { return e.strokeId; });
        function commitGroupRename(v) {
          pushUndo();
          if (window.SMGroup && SMGroup.renameGroup) SMGroup.renameGroup(node.gid, c.ld, v, memberIds);
          saveActiveLayerFrame(); renderShapesPanel();
        }
        grow.addEventListener('click', function () { window.SMMotion.selectShapesByStrokeIds(c.li, memberIds); });
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
              saveActiveLayerFrame(); renderShapesPanel();
              if (window.SMEngineBridge) SMEngineBridge.renderNow();
            } },
          ]);
        });
        list.appendChild(grow);
      } else {
        var entry = { strokeId: node.strokeId, sd: node.sd };
        var idx = shapeIdx++;
        var row = document.createElement('div'); row.className = 'lrow motion-elem-row';
        var swatch = document.createElement('div'); swatch.className = 'motion-elem-swatch';
        swatch.style.background = entry.sd.fillColor || entry.sd.strokeColor || 'transparent';
        var nm = document.createElement('div'); nm.className = 'lnm';
        nm.textContent = window.SMMotion.elementLabel(entry, idx, c.ld);
        row.appendChild(swatch); row.appendChild(nm);
        row.addEventListener('click', function () { window.SMMotion.selectShapesByStrokeIds(c.li, [entry.strokeId]); });
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
    });
  }
  window.renderShapesPanel = renderShapesPanel;
})();
