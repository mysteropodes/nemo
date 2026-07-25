// ---- GROUPS (2026-07, Cmd+G) ----
// Feedback: "mettre en place les groupes, avec command+g, select les
// éléments et ça permet d'avoir un avec une seul boite de transformation."
//
// Deliberately NOT a Paper.js `Group` (a real wrapper item that would
// become a THIRD kind of `strokes`/`layer.children` entry alongside Path/
// Raster) — select-bridge.js's transform box already computes one shared
// box + shared pivot/delta for however many items are in `selectedPaths`
// today (xformSelBounds/orientedSelBox, tools.js), with zero Group needed
// for the "one transform box" mechanic itself. The only thing actually
// missing was PERSISTENCE: `selectedPaths` is rebuilt from scratch on every
// click/marquee, so a multi-selection never survived a deselect, a save/
// reload, or a frame navigation.
//
// So a group here is just a shared `data.groupId` tag (same stable-id
// pattern as `data.strokeId`/`data.brushGroupId` elsewhere in this file)
// on each member Path/Raster — they stay perfectly ordinary flat items to
// every existing consumer (buildSceneJson, tween matching, export.js,
// serP/desP...), which is exactly why this is safe: none of those loops
// need to change at all, unlike introducing a real new item type would
// require (see CLAUDE.md §1's "famille de bug n°1"). select-bridge.js's
// click-select is the ONE place that needs to know about groupId — it
// expands a click on any one member into the whole group via membersOf()
// below, and from there the EXISTING shared-transform-box code just works
// unmodified on whatever `selectedPaths` now contains.
(function () {
  function groupSelection() {
    if ((state.tool !== 'select' && state.tool !== 'subselect') || !window.selectedPaths || selectedPaths.length < 2) {
      if (window.showToast) showToast('Sélectionnez au moins 2 éléments pour créer un groupe');
      return;
    }
    pushUndo();
    var gid = 'grp_' + Date.now().toString(36) + '_' + Math.floor(Math.random() * 1e6);
    selectedPaths.forEach(function (p) { if (!p.data) p.data = {}; p.data.groupId = gid; });
    saveActiveLayerFrame(); updateUI();
    if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
    if (window.showToast) showToast('Groupe créé (' + selectedPaths.length + ' éléments)');
  }
  function ungroupSelection() {
    // Both refusals were silent while groupSelection's own ("Sélectionnez au
    // moins 2 calques") is not — so Cmd+Shift+G read as a broken shortcut
    // rather than an inapplicable one (2026-07-25 UX audit).
    if (!window.selectedPaths || !selectedPaths.length) {
      if (window.showToast) showToast('Sélectionnez d\'abord un groupe à dissocier');
      return;
    }
    var hasGroup = selectedPaths.some(function (p) { return p.data && p.data.groupId; });
    if (!hasGroup) {
      if (window.showToast) showToast('La sélection ne contient aucun groupe');
      return;
    }
    pushUndo();
    selectedPaths.forEach(function (p) { if (p.data) delete p.data.groupId; });
    saveActiveLayerFrame(); updateUI();
    if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
    if (window.showToast) showToast('Groupe dissocié');
  }
  // Every sibling in `layer` sharing `p`'s groupId, INCLUDING `p` itself —
  // `[p]` (a one-item "group") when `p` has no groupId, so every caller can
  // unconditionally use this instead of branching on "is this grouped".
  function membersOf(p, layer) {
    var gid = p.data && p.data.groupId;
    if (!gid || !layer) return [p];
    return layer.children.filter(function (c) { return c.data && c.data.groupId === gid; });
  }
  window.SMGroup = { groupSelection: groupSelection, ungroupSelection: ungroupSelection, membersOf: membersOf };
})();
