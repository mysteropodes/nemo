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
  // ---- NESTING (2026-09, issue #738 — Cyril: "si on a un groupe avec
  // merge qui est dans un sous groupe alors celui-ci est considéré comme
  // un seul élément pour le merge du groupe parent") ----
  // The DATA model already allowed it: a group's `order` may hold child
  // GIDs as well as leaf strokeIds, and collectGroupStrokeIds/
  // resolveGroupMembers below have always recursed through them. What was
  // missing was (a) any UI that creates the nesting, (b) the upward link
  // needed to answer "which group is the outermost one for this stroke",
  // and (c) a combine that treats a combined CHILD as one operand.
  // The upward link is `parent` on the CHILD's own ld.groups entry — the
  // per-stroke `data.groupId` tag keeps meaning the stroke's INNERMOST
  // group, so every existing consumer of that tag is untouched.
  function groupMeta(gid, ld) { return (ld && ld.groups && ld.groups[gid]) || null; }
  function parentGroupOf(gid, ld) { var g = groupMeta(gid, ld); return (g && g.parent && groupMeta(g.parent, ld)) ? g.parent : null; }
  // Outermost ancestor of `gid` (itself when it has no parent). Depth-capped
  // like resolveGroupMembers, so a corrupt parent cycle can never hang.
  function rootGroupOf(gid, ld) {
    var cur = gid, seen = {};
    for (var i = 0; i < 32 && cur; i++) {
      if (seen[cur]) { console.warn('[SMGroup] cyclic group parent, stopping at', cur); return cur; }
      seen[cur] = true;
      var up = parentGroupOf(cur, ld);
      if (!up) return cur;
      cur = up;
    }
    return cur;
  }
  function rootGroupOfItem(p, ld) {
    var gid = p && p.data && p.data.groupId;
    return gid && groupMeta(gid, ld) ? rootGroupOf(gid, ld) : (gid || null);
  }
  // Detaches one leaf strokeId from whatever group entry currently lists
  // it, dissolving that group if it drops below 2 entries — the shared
  // core of removeMemberFromGroup and of re-grouping a shape that already
  // belonged somewhere else.
  function detachLeaf(sid, ld, layer) {
    if (!sid || !ld || !ld.groups) return;
    Object.keys(ld.groups).forEach(function (gid) {
      var grp = ld.groups[gid]; if (!grp || !grp.order) return;
      var ix = grp.order.indexOf(sid); if (ix === -1) return;
      grp.order.splice(ix, 1);
      if (grp.order.length < 2) dissolveGroup(gid, ld, layer);
    });
  }
  // Dissolves ONE level: leaf members lose their tag, child groups are
  // promoted to wherever this group sat (its own parent, or top level).
  function dissolveGroup(gid, ld, layer) {
    var grp = groupMeta(gid, ld); if (!grp) return;
    var up = grp.parent && groupMeta(grp.parent, ld) ? grp.parent : null;
    (grp.order || []).forEach(function (entry) {
      if (groupMeta(entry, ld)) {
        if (up) ld.groups[entry].parent = up; else delete ld.groups[entry].parent;
      } else {
        var item = findByStrokeId(layer, entry);
        if (item && item.data) { if (up) item.data.groupId = up; else delete item.data.groupId; }
      }
    });
    if (up) {
      var pOrder = ld.groups[up].order || [];
      var ix = pOrder.indexOf(gid);
      if (ix !== -1) pOrder.splice.apply(pOrder, [ix, 1].concat(grp.order || []));
    }
    delete ld.groups[gid];
  }
  function groupSelection() {
    if ((state.tool !== 'select' && state.tool !== 'subselect') || !window.selectedPaths || selectedPaths.length < 2) {
      if (window.showToast) showToast(SM.t('toastSelectAtLeast2ElementsToGroup'));
      return;
    }
    var li = state.activeLayerIdx, ld = state.layers[li], layer = window.userLayers ? userLayers[li] : null;
    if (!ld) return;
    ensureLayerGroups(ld);
    // UNITS, not shapes (2026-09, #738): a selection that contains every
    // member of an existing group nests that WHOLE group as one entry
    // instead of dissolving it into loose members — which is what made
    // "group a group with a shape" flatten before. A group only PARTLY
    // selected still contributes its selected shapes as loose leaves
    // (they leave their old group on the way, which is what the old code
    // did implicitly, minus the dangling `order` entries it left behind).
    var ordered = selectedPaths.slice();
    if (layer) ordered.sort(function (a, b) { return layer.children.indexOf(a) - layer.children.indexOf(b); });
    var selSet = ordered;
    var units = [], seenGid = {};
    ordered.forEach(function (p) {
      var root = rootGroupOfItem(p, ld);
      if (root && groupMeta(root, ld)) {
        if (seenGid[root]) return;
        var leaves = resolveGroupMembers(root, ld, layer);
        var whole = leaves.length > 0 && leaves.every(function (m) { return selSet.indexOf(m) !== -1; });
        if (whole) { seenGid[root] = true; units.push({ kind: 'group', gid: root }); return; }
      }
      units.push({ kind: 'leaf', p: p });
    });
    if (units.length < 2) {
      // Everything selected already belongs to one and the same group —
      // grouping it again would just wrap a single unit, which is a no-op
      // the user would read as "Cmd+G does nothing".
      if (window.showToast) showToast(SM.t('toastSelectAtLeast2ElementsToGroup'));
      return;
    }
    pushUndo();
    var gid = 'grp_' + Date.now().toString(36) + '_' + Math.floor(Math.random() * 1e6);
    var order = [];
    units.forEach(function (u) {
      if (u.kind === 'group') { ld.groups[u.gid].parent = gid; order.push(u.gid); return; }
      var p = u.p;
      if (!p.data) p.data = {};
      var sid = ensureStrokeId(p);
      detachLeaf(sid, ld, layer);
      p.data.groupId = gid;
      order.push(sid);
    });
    // 2026-08 fix: hardcoded French default name, shown regardless of locale.
    ld.groups[gid] = { name: SM.t('autoNameGroup'), combineMode: 'none', order: order };
    saveActiveLayerFrame(); updateUI();
    if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
    if (window.showToast) showToast(SM.t('toastGroupCreatedSuffix') + units.length + SM.t('toastElementsCloseParen'));
  }
  function ungroupSelection() {
    // Both refusals were silent while groupSelection's own ("Sélectionnez au
    // moins 2 calques") is not — so Cmd+Shift+G read as a broken shortcut
    // rather than an inapplicable one (2026-07-25 UX audit).
    if (!window.selectedPaths || !selectedPaths.length) {
      if (window.showToast) showToast(SM.t('toastSelectGroupToUngroupFirst'));
      return;
    }
    var hasGroup = selectedPaths.some(function (p) { return p.data && p.data.groupId; });
    if (!hasGroup) {
      if (window.showToast) showToast(SM.t('toastSelectionHasNoGroup'));
      return;
    }
    pushUndo();
    // One LEVEL at a time (2026-09, #738): dissolve the outermost group of
    // each selected shape, which promotes its child groups instead of
    // erasing the whole nested structure in one keystroke — Cmd+Shift+G
    // pressed again then peels the next level, matching Illustrator/Figma.
    var uli = state.activeLayerIdx, uld = state.layers[uli], ulayer = window.userLayers ? userLayers[uli] : null;
    var roots = {};
    selectedPaths.forEach(function (p) { var r = rootGroupOfItem(p, uld); if (r && groupMeta(r, uld)) roots[r] = true; });
    var rootKeys = Object.keys(roots);
    if (rootKeys.length) rootKeys.forEach(function (r) { dissolveGroup(r, uld, ulayer); });
    else selectedPaths.forEach(function (p) { if (p.data) delete p.data.groupId; });
    saveActiveLayerFrame(); updateUI();
    if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
    if (window.showToast) showToast(SM.t('toastGroupUngrouped'));
  }
  // Every sibling in `layer` sharing `p`'s groupId, INCLUDING `p` itself —
  // `[p]` (a one-item "group") when `p` has no groupId, so every caller can
  // unconditionally use this instead of branching on "is this grouped".
  function membersOf(p, layer) {
    var gid = p.data && p.data.groupId;
    if (!gid || !layer) return [p];
    // Nested groups (2026-09, #738): clicking any shape selects its
    // OUTERMOST group, the same "click selects the top-level group, you
    // step inside from the tree" convention Figma/Illustrator use — with
    // the flat model these two were always the same thing, so this is the
    // one behavioural change nesting brings to canvas selection.
    var li = window.userLayers ? userLayers.indexOf(layer) : -1;
    var ld = li >= 0 ? state.layers[li] : null;
    var root = ld ? rootGroupOfItem(p, ld) : gid;
    if (ld && root && groupMeta(root, ld)) {
      var leaves = resolveGroupMembers(root, ld, layer);
      if (leaves.length) return leaves;
    }
    return layer.children.filter(function (c) { return c.data && c.data.groupId === gid; });
  }

  // ---- Non-destructive COMBINE groups (2026-07-29) ----
  // Extends the plain Cmd+G tag above with group-LEVEL metadata (combine
  // mode + explicit z-order) living on the layer descriptor, sibling of
  // ld.duplicator/ld.rig — see the "Non-destructive shape combine" plan.
  // Membership itself stays the SAME data.groupId tag (no new per-stroke
  // tag): a combine-group is just a plain Cmd+G group that additionally has
  // an entry in ld.groups with combineMode!=='none'. This is deliberate —
  // it means groupSelection/ungroupSelection/membersOf above, and every
  // existing consumer of data.groupId (select-bridge.js's click-expansion,
  // the "Grouper"/"Dissocier" context-menu items), work on combine-groups
  // with ZERO changes.
  function ensureLayerGroups(ld) {
    if (!ld.groups) ld.groups = {};
    return ld.groups;
  }
  // dupIndex (2026-07-29 fix, QA-confirmed): a mograph duplicator clones
  // every child N times (app.js applyLayerDuplicator), and every copy keeps
  // the ORIGINAL strokeId/groupId unchanged — only `data.dupIndex` (0..N-1,
  // stamped on every copy, always present the instant a duplicator is on)
  // tells them apart. A plain strokeId lookup used to always resolve to
  // whichever copy happened to come first, so only ONE of the N duplicated
  // instances of a combine-group ever got combined; the rest rendered raw.
  // `dupIndex` is left undefined/null for an ordinary, non-duplicated layer
  // (`(x||0)` below normalizes that to the same bucket as an explicit 0).
  function findByStrokeId(layer, strokeId, dupIndex) {
    if (!layer) return null;
    var kids = layer.children;
    for (var i = 0; i < kids.length; i++) {
      var d = kids[i].data;
      if (d && d.strokeId === strokeId && (d.dupIndex || 0) === (dupIndex || 0)) return kids[i];
    }
    return null;
  }
  // Flat list of the raw strokeId leaves a group's `order` references,
  // recursing through nested sub-groups — pure id-level traversal, no
  // live-item resolution, used to discover which dupIndex copies of a group
  // actually exist among a layer's children/dicts before resolving each one.
  function collectGroupStrokeIds(gid, ld, ancestors, depth) {
    depth = depth || 0; ancestors = ancestors || [];
    if (depth > 16 || ancestors.indexOf(gid) !== -1) return [];
    var grp = ld.groups && ld.groups[gid];
    if (!grp) return [];
    var nextAncestors = ancestors.concat([gid]);
    var ids = [];
    (grp.order || []).forEach(function (entry) {
      if (ld.groups[entry]) ids = ids.concat(collectGroupStrokeIds(entry, ld, nextAncestors, depth + 1));
      else ids.push(entry);
    });
    return ids;
  }
  // Resolves a group's `order` (strokeId leaves + nested gid sub-groups)
  // into live Paper items, bottom-up/depth-first, all belonging to ONE
  // duplicator copy (`dupIndex`, see findByStrokeId above — undefined for a
  // non-duplicated layer). `ancestors` is a fresh array per branch (not
  // shared/mutated) so the same sub-group id can legitimately appear in two
  // different branches without tripping the cycle guard — only a GENUINE
  // cycle (a group nested inside itself) trips it. Depth capped at 16
  // (mirrors _registerCap's "never unbounded" philosophy, engine-bridge.js)
  // — a cap-exceeded or cyclic branch just warns and contributes nothing,
  // never hangs/crashes the render.
  function resolveGroupMembers(gid, ld, layer, ancestors, depth, dupIndex) {
    depth = depth || 0; ancestors = ancestors || [];
    if (depth > 16) { console.warn('[SMGroup] group nesting too deep, skipping', gid); return []; }
    if (ancestors.indexOf(gid) !== -1) { console.warn('[SMGroup] cyclic group reference detected, skipping', gid); return []; }
    var grp = ld.groups && ld.groups[gid];
    if (!grp) return [];
    var nextAncestors = ancestors.concat([gid]);
    var out = [];
    (grp.order || []).forEach(function (entry) {
      if (ld.groups[entry]) out = out.concat(resolveGroupMembers(entry, ld, layer, nextAncestors, depth + 1, dupIndex));
      else { var item = findByStrokeId(layer, entry, dupIndex); if (item) out.push(item); }
    });
    return out;
  }
  // New "Combiner" action — creates a non-destructive combine-group from the
  // current selection, or (if the selection already IS one existing group's
  // full membership) just updates that group's combine mode. mode is one of
  // 'unite'/'subtract'/'intersect'/'exclude'.
  function combineSelection(mode) {
    if ((state.tool !== 'select' && state.tool !== 'subselect') || !window.selectedPaths || selectedPaths.length < 2) {
      if (window.showToast) showToast(SM.t('toastSelectAtLeast2ShapesToCombine'));
      return;
    }
    var li = state.activeLayerIdx, ld = state.layers[li], layer = userLayers[li];
    if (!ld || !layer) return;
    ensureLayerGroups(ld);
    var firstGid = selectedPaths[0].data && selectedPaths[0].data.groupId;
    var existingGid = null;
    if (firstGid && ld.groups[firstGid]) {
      var members = resolveGroupMembers(firstGid, ld, layer);
      var sameSet = members.length === selectedPaths.length && members.every(function (m) { return selectedPaths.indexOf(m) !== -1; });
      if (sameSet) existingGid = firstGid;
    }
    pushUndo();
    if (existingGid) {
      ld.groups[existingGid].combineMode = mode;
    } else {
      var gid = 'grp_' + Date.now().toString(36) + '_' + Math.floor(Math.random() * 1e6);
      // Bottom-to-top z-order, not selection/click order.
      var ordered = selectedPaths.slice().sort(function (a, b) { return layer.children.indexOf(a) - layer.children.indexOf(b); });
      var order = ordered.map(function (p) { if (!p.data) p.data = {}; p.data.groupId = gid; return ensureStrokeId(p); });
      ld.groups[gid] = { combineMode: mode, order: order };
    }
    saveActiveLayerFrame(); updateUI();
    if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
    if (window.showToast) showToast(SM.t('toastCombined'));
  }
  function setGroupCombineMode(gid, ld, mode) {
    if (!ld || !ld.groups || !ld.groups[gid]) return;
    pushUndo();
    ld.groups[gid].combineMode = mode;
    saveActiveLayerFrame(); updateUI();
    if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
  }
  // Named groups (2026-07-31, group/shape tree panel) — a plain Cmd+G group
  // (groupSelection above) only ever stamped data.groupId with no metadata
  // object at all; a combine-group had one but no `name` field. Called by
  // motion.js's tree-panel rename UI whenever a group without a ld.groups
  // entry yet gets renamed for the first time — creates one on demand
  // (combineMode:'none', matching a plain group's actual behavior) rather
  // than requiring every group to go through combineSelection first.
  function renameGroup(gid, ld, name, memberStrokeIds) {
    if (!ld || !gid) return;
    ensureLayerGroups(ld);
    if (!ld.groups[gid]) ld.groups[gid] = { combineMode: 'none', order: (memberStrokeIds || []).slice() };
    ld.groups[gid].name = name;
  }
  // "Sortir du groupe" — removes ONE shape from its combine-group, leaving
  // the rest intact. Distinct from ungroupSelection (whole-group dissolve)
  // since that function's shape doesn't fit a single-member branch cleanly.
  // opts.skipUndo/opts.silent let internal callers (a destructive booleanOp
  // ending a source shape's group membership before removing it) fold this
  // into their OWN undo snapshot/save cycle instead of adding a second one.
  function removeMemberFromGroup(p, ld, layer, opts) {
    opts = opts || {};
    var gid = p.data && p.data.groupId;
    if (!gid || !ld || !ld.groups || !ld.groups[gid]) return;
    if (!opts.skipUndo) pushUndo();
    var grp = ld.groups[gid];
    var sid = p.data.strokeId;
    grp.order = (grp.order || []).filter(function (e) { return e !== sid; });
    delete p.data.groupId;
    // Mirrors groupSelection's own ">=2 entries" invariant — one entry left
    // isn't a group anymore. dissolveGroup (not an inline loop) since an
    // entry may now be a child GROUP, which has to be promoted rather than
    // untagged (2026-09, #738).
    if (grp.order.length < 2) dissolveGroup(gid, ld, layer);
    if (!opts.silent) {
      saveActiveLayerFrame(); updateUI();
      if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
    }
  }
  // "Aplatir" — the one explicit, opt-in DESTRUCTIVE action: bakes the
  // current combined outline into one real shape and discards the group,
  // reusing the existing destructive insertBooleanResult (tools.js) for its
  // styling/z-position handling, same as booleanOp() itself does.
  function flattenGroup(gid, ld, layer) {
    var grp = ld && ld.groups && ld.groups[gid];
    if (!grp) return;
    var members = resolveGroupMembers(gid, ld, layer);
    if (members.length < 2) return;
    pushUndo();
    var style = members[members.length - 1];
    var mode = grp.combineMode === 'none' ? 'unite' : grp.combineMode;
    // Bake per-shape element Motion into a disposable clone before the
    // boolean op sees it (2026-09 fix, see elementMotionBakedClone's own
    // comment in tools.js) — this call used to go straight to foldBooleanOp
    // with the members' raw, un-animated segments, silently snapping any
    // Motion-transformed member back to its un-transformed position the
    // moment the group was flattened.
    var li = window.userLayers ? userLayers.indexOf(layer) : -1;
    var motionMembers = li >= 0 ? members.map(function (m) { return elementMotionBakedClone(li, m, state.currentFrame); }) : members;
    var folded = foldBooleanOp(mode, motionMembers, layer);
    var disposable = motionMembers === members ? [] : motionMembers.filter(function (m, idx) { return m !== members[idx]; });
    var insertAt = layer.children.indexOf(members[0]);
    // Stroke-only style source (2026-07-29 fix, same as booleanOp's own —
    // insertBooleanResult always drops the stroke on its islands, so a
    // stroke-only member's null fillColor used to leave the flattened
    // result fully invisible; fall back to its strokeColor, the closest
    // match to "what the merged stroke looked like" this fill-only result
    // model can give).
    // Visual-fill style source (2026-07-30 fix, same as booleanOp's own in
    // tools.js — see its comment there): a vector-brush ribbon's OWN
    // fillColor is its ink color, not the fill the user actually sees —
    // that lives on its separate linked-fill companion. Prefer it (and
    // its .data, for fillGradient via BOOL_KEEP_DATA_ALL) when present.
    var styleCompanion = findLinkedFillCompanion(layer, style);
    var fillSource = styleCompanion || style;
    var flattenFill = fillSource.fillColor || style.strokeColor;
    var islands = insertBooleanResult(layer, insertAt, folded.result, flattenFill, style.opacity, null, fillSource.data);
    members.forEach(function (m) { m.remove(); });
    folded.companions.forEach(function (c) { if (!c.removed) c.remove(); });
    disposable.forEach(function (c) { if (!c.removed) c.remove(); });
    // Descendant group entries go with it (2026-09, #738): flatten bakes
    // every leaf underneath into one shape, so a surviving child entry
    // would reference strokeIds that no longer exist and keep a `parent`
    // pointing at a group that is gone.
    (function removeSubtree(g, depth) {
      var meta = ld.groups[g]; if (!meta || depth > 16) return;
      (meta.order || []).forEach(function (entry) { if (ld.groups[entry]) removeSubtree(entry, depth + 1); });
      delete ld.groups[g];
    })(gid, 0);
    selectedPaths = islands; state.selectedStrokeIndices = [];
    fillRegenerateLinked(layer, islands[0]);
    saveActiveLayerFrame(); updateUI();
    if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
    if (window.showToast) showToast('Groupe aplati');
  }
  // ---- Render adapters — the actual "combine" happens here, as a
  // POST-PROCESS on the renderer's own output arrays. The live document
  // (layer.children / getEffectiveStrokes) is NEVER touched — see the plan's
  // "why not touch the live document" section: nulling a live member's own
  // fillColor/strokeColor would make it unclickable (Paper.js gates fill hit-
  // testing on hasFill()), silently breaking "each shape stays independently
  // selectable". Both adapters share the one computeGroupCombine (tools.js)
  // core — no duplicated boolean-op logic.

  // For buildSceneJson/onionLayerItems (engine-bridge.js): resolves LIVE
  // members (real Paper objects, so foldBooleanOp's linked-fill-companion
  // pre-step works unmodified), returns which live items to suppress paint
  // on (in the JSON item only, never on the item itself) plus the extra
  // combined-result path(s) to append, styled from the topmost member
  // ("what's on top wins", matching AE/Illustrator/Figma's own convention).
  function renderCombinesFromChildren(layer, ld, frameIdx) {
    var suppress = [], extra = [];
    if (!ld || !ld.groups) return { suppress: suppress, extra: extra };
    // NESTED COMBINE (2026-09, #738 — "un groupe avec merge dans un sous
    // groupe est considéré comme un seul élément pour le merge du parent").
    // The walk is now top-down from ROOT groups instead of a flat pass over
    // every group id, because the two questions a nested combine asks can
    // only be answered with the parent in hand:
    //  - what are my operands? a child group that combines contributes ONE
    //    operand (its own result); a child group that doesn't contributes
    //    its leaves, exactly as if it weren't there;
    //  - do I emit my result, or is my parent about to consume it?
    // A child's result is therefore computed once and either handed up or
    // drawn, never both — which is what stops the old flat pass from
    // drawing the inner merge and the outer merge on top of each other.
    function emit(gid, dupIndex, parentCombines, ancestors, depth, temps) {
      var grp = ld.groups[gid];
      if (!grp || depth > 16 || ancestors.indexOf(gid) !== -1) return [];
      var next = ancestors.concat([gid]);
      var mode = grp.combineMode || 'none';
      var operands = [];
      (grp.order || []).forEach(function (entry) {
        if (ld.groups[entry]) {
          operands = operands.concat(emit(entry, dupIndex, mode !== 'none', next, depth + 1, temps));
        } else {
          var item = findByStrokeId(layer, entry, dupIndex);
          if (item) operands.push(item);
        }
      });
      if (mode === 'none' || operands.length < 2) return operands;
      var leaves = resolveGroupMembers(gid, ld, layer, null, 0, dupIndex);
      // Consumed by a combining parent: hand up the UN-flattened result, so
      // the parent sees ONE shape (holes included) — that is the whole
      // point of #738. Emitted for real: flatten into islands, which is
      // what the renderer's item list expects.
      if (parentCombines) {
        var raw;
        try { raw = computeGroupCombineRaw(operands, mode, layer, frameIdx); }
        catch (e2) { console.warn('[SMGroup] nested combine failed for group', gid, e2); return operands; }
        if (!raw) return operands;
        leaves.forEach(function (m) { if (suppress.indexOf(m) === -1) suppress.push(m); });
        operands.forEach(function (o) { if (temps.indexOf(o) === -1 && layer && o.parent !== layer) temps.push(o); });
        // Style is irrelevant upstream (the outermost group paints the
        // final result from its own topmost leaf), but a fill-less operand
        // would be mistaken for a stroke-only shape and expanded.
        if (!raw.fillColor) raw.fillColor = '#000000';
        raw.strokeColor = null;
        return [raw];
      }
      var islands;
      try { islands = computeGroupCombine(operands, mode, layer, frameIdx); }
      catch (e) { console.warn('[SMGroup] combine failed for group', gid, e); return operands; }
      leaves.forEach(function (m) { if (suppress.indexOf(m) === -1) suppress.push(m); });
      // Operands that were themselves computed (a child's result) are
      // scratch geometry: keep them alive until the parent's boolean has
      // consumed them, then drop whatever the result doesn't reuse.
      operands.forEach(function (o) { if (temps.indexOf(o) === -1 && layer && o.parent !== layer) temps.push(o); });
      if (parentCombines) return islands;
      var styleSource = leaves[leaves.length - 1] || operands[operands.length - 1];
      // Vector-brush ribbon style source (2026-08 fix, feedback: "les
      // combine gère mal stroke plus fill" for Pressure-brush shapes) —
      // a ribbon's OWN fillColor is its ink/stroke color, never a real
      // fill (isVectorBrush ribbons paint through fillColor — see
      // app.js's serP comment); any real Fill the user set lives on a
      // completely separate companion Path.
      var srcIsVB = !!(styleSource.data && styleSource.data.isVectorBrush);
      var isl_fill, isl_stroke;
      if (srcIsVB) {
        var srcCompanion = findLinkedFillCompanion(layer, styleSource);
        isl_fill = srcCompanion ? srcCompanion.fillColor : null;
        isl_stroke = styleSource.fillColor;
      } else {
        // Stroke-only style source (2026-07-29 fix, QA-confirmed live):
        // computeGroupCombine expands a stroke-only member into its real
        // filled ribbon geometry, so painting that geometry with ONLY a
        // strokeColor drew just its outline.
        isl_fill = styleSource.fillColor || styleSource.strokeColor;
        isl_stroke = styleSource.fillColor ? styleSource.strokeColor : null;
      }
      islands.forEach(function (isl) {
        isl.fillColor = isl_fill;
        isl.strokeColor = isl_stroke;
        isl.strokeWidth = styleSource.strokeWidth;
        isl.opacity = styleSource.opacity;
        extra.push({ path: isl, groupCombineOf: gid });
      });
      return islands;
    }
    Object.keys(ld.groups).forEach(function (gid) {
      var grp = ld.groups[gid];
      if (!grp) return;
      // Roots only — a child is reached through its parent's own walk, and
      // reaching it twice is exactly the double-draw this replaces.
      if (grp.parent && ld.groups[grp.parent]) return;
      var strokeIds = collectGroupStrokeIds(gid, ld);
      if (!strokeIds.length) return;
      // One combine per duplicator COPY (see findByStrokeId's comment) —
      // discover which dupIndex values actually exist among this group's
      // own live children (an ordinary, non-duplicated layer has exactly
      // one bucket: undefined/0) instead of assuming a single global set of
      // members.
      var dupIndices = {};
      layer.children.forEach(function (c) {
        if (c.data && c.data.strokeId && strokeIds.indexOf(c.data.strokeId) !== -1) dupIndices[c.data.dupIndex || 0] = true;
      });
      Object.keys(dupIndices).forEach(function (dupIndexKey) {
        var temps = [];
        var result = emit(gid, Number(dupIndexKey), false, [], 0, temps);
        temps.forEach(function (t) {
          if (t.removed || result.indexOf(t) !== -1) return;
          if (extra.some(function (ex) { return ex.path === t; })) return;
          t.remove();
        });
      });
    });
    return { suppress: suppress, extra: extra };
  }
  // The dict-based twin, for export.js's two frame builders (operate on
  // plain stroke dicts post-getEffectiveStrokesRendered, not live Paper
  // objects). Builds transient {insert:false} Paths from member dicts'
  // segments (same construction _resolveDuplicatorPath/_boneSegsToPath
  // already use in app.js) — including a manual companion pre-merge for a
  // linked-fill ribbon member, since these transient paths carry no live
  // .data for foldBooleanOp's own companion lookup to find.
  //
  // li/frameIdx (2026-09, twin of the #723 fix): when the caller knows which
  // state.layers index and frame these dicts belong to, each member's
  // per-element Motion (elementMotionAt) is baked into its transient Path
  // BEFORE the boolean op — exactly what computeGroupCombine does on the
  // live side via elementMotionBakedClone (tools.js). Without it the
  // combined outline (which carries no strokeId, so the caller's own
  // per-stroke elMat pass never reaches it) rendered at the member's REST
  // pose in every export/onion frame while the canvas showed it animated.
  // Both optional: a caller without a frame context (StoryBoard reading a
  // symbol's inner layer) keeps the un-baked behavior.
  function applyCombinesToStrokes(strokes, ld, li, frameIdx) {
    if (!ld || !ld.groups || !strokes || !strokes.length) return strokes;
    var canBake = li != null && frameIdx != null && !!window.SMMotion;
    function bakeElementMotion(p, sd) {
      if (!canBake || !sd.strokeId) return p;
      var m = SMMotion.elementMotionAt(li, sd.strokeId, frameIdx, sd);
      if (!m || (m.dx === 0 && m.dy === 0 && m.rot === 0 && m.sx === 1 && m.sy === 1)) return p;
      var pivot = new Point(p.bounds.center.x + (m.ax || 0), p.bounds.center.y + (m.ay || 0));
      p.scale(m.sx, m.sy, pivot);
      p.rotate(m.rot, pivot);
      p.translate(m.dx, m.dy);
      return p;
    }
    function dictToPath(sd) {
      var p = new Path({ insert: false });
      sd.segments.forEach(function (s) { p.add(new Segment(new Point(s.point[0], s.point[1]), new Point(s.handleIn[0], s.handleIn[1]), new Point(s.handleOut[0], s.handleOut[1]))); });
      if (sd.closed) p.closed = true;
      // Paint carried over too (2026-07-29 fix) — foldBooleanOp's own
      // stroke-only-shape expansion (eraseExpandStrokeToFill) reads
      // p.fillColor/p.strokeColor off the live Path, and this transient one
      // used to have neither set at all (pure geometry), so a stroke-only
      // dict never got detected/expanded here even though the exact same
      // shapes DO get expanded via the live-Paper path (renderCombinesFromChildren).
      // hasRealStroke!==false mirrors CLAUDE.md §2's own desP contract — the
      // historical '#ffffff' fallback on a genuinely strokeless dict must
      // NOT be read as "this has a real stroke".
      if (sd.fillColor) p.fillColor = sd.fillColor;
      if (sd.strokeColor && sd.hasRealStroke !== false) { p.strokeColor = sd.strokeColor; p.strokeWidth = sd.strokeWidth || 1; }
      return p;
    }
    // Suppression is tracked by DICT IDENTITY (array of object references),
    // not by strokeId string (2026-07-29 fix, QA-confirmed): a mograph
    // duplicator's N copies all share the same strokeId (see
    // findByStrokeId's comment above), so a strokeId-keyed suppress map
    // used to blank out EVERY copy's paint while only ONE copy's worth of
    // members ever got resolved/combined — 5 of 6 grid positions rendered
    // fully blank in export. Bucketing member dicts by dupIndex below and
    // suppressing only the exact dicts in each bucket fixes both at once.
    var suppressed = [], extra = [];
    // Same top-down, parent-aware walk as renderCombinesFromChildren above
    // (2026-09, #738) — see its comment for why a nested combine can't be
    // resolved by a flat pass. Operands here are transient Paths built from
    // dicts; a child group that combines hands its result Path up as ONE
    // operand, a child that doesn't hands up its members' Paths.
    function dictOperands(gid, dicts, dupIndex, parentCombines, ancestors, depth, temps, out) {
      var grp = ld.groups[gid];
      if (!grp || depth > 16 || ancestors.indexOf(gid) !== -1) return [];
      var next = ancestors.concat([gid]);
      var mode = grp.combineMode || 'none';
      var operands = [];
      (grp.order || []).forEach(function (entry) {
        if (ld.groups[entry]) {
          operands = operands.concat(dictOperands(entry, dicts, dupIndex, mode !== 'none', next, depth + 1, temps, out));
        } else {
          var sd = dicts[entry];
          if (!sd || sd.isRaster || !sd.segments || !sd.segments.length) return;
          operands.push(dictPathFor(sd, temps));
        }
      });
      if (mode === 'none' || operands.length < 2) return operands;
      var leafIds = collectGroupStrokeIds(gid, ld);
      var memberDicts = leafIds.map(function (id) { return dicts[id]; }).filter(function (sd) { return sd && !sd.isRaster && sd.segments && sd.segments.length; });
      if (memberDicts.length < 2) return operands;
      operands.forEach(function (o) { if (temps.indexOf(o) === -1) temps.push(o); });
      // Same single-operand rule as the live twin above (#738).
      if (parentCombines) {
        var raw;
        try { raw = computeGroupCombineRaw(operands, mode, null); }
        catch (e2) { console.warn('[SMGroup] nested combine failed for group', gid, e2); return operands; }
        if (!raw) return operands;
        memberDicts.forEach(function (sd) { if (suppressed.indexOf(sd) === -1) suppressed.push(sd); });
        if (!raw.fillColor) raw.fillColor = '#000000';
        raw.strokeColor = null;
        return [raw];
      }
      var islands;
      try { islands = computeGroupCombine(operands, mode, null); }
      catch (e) { console.warn('[SMGroup] combine failed for group', gid, e); return operands; }
      memberDicts.forEach(function (sd) { if (suppressed.indexOf(sd) === -1) suppressed.push(sd); });
      // z-position of the merged result: right after the group's
      // front-most member dict, not appended after every other stroke
      // (feedback #735 — twin of buildSceneJson's own fix).
      var afterIdx = -1;
      memberDicts.forEach(function (sd) { var ix = strokes.indexOf(sd); if (ix > afterIdx) afterIdx = ix; });
      var styleSource = memberDicts[memberDicts.length - 1];
      var thisDi = memberDicts[0].dupIndex;
      var combFill, combStroke;
      if (styleSource.isVectorBrush) {
        // Same fix as renderCombinesFromChildren's twin above, dict form: a
        // ribbon dict's own fillColor is its ink/stroke color, never a real
        // fill — the real Fill (if any) is a separate companion dict.
        var styleCompanion = styleSource.linkedFillId ? strokes.filter(function (d) {
          return d.isLinkedFillCompanion && d.linkedFillId === styleSource.linkedFillId && (d.dupIndex || 0) === (styleSource.dupIndex || 0);
        })[0] : null;
        combFill = styleCompanion ? styleCompanion.fillColor : null;
        combStroke = styleSource.fillColor;
      } else {
        // Stroke-only style source (2026-07-29 fix) — computeGroupCombine
        // just expanded a stroke-only member into a real filled ribbon, so
        // the exported dict must flip to filled-with-the-stroke's-color too.
        var srcHasRealStroke = !!styleSource.hasRealStroke;
        combFill = styleSource.fillColor || (srcHasRealStroke ? styleSource.strokeColor : null);
        combStroke = (styleSource.fillColor && srcHasRealStroke) ? styleSource.strokeColor : null;
      }
      islands.forEach(function (isl) {
        extra.push({
          __afterIdx: afterIdx, // consumed (and removed) by the splice at the end
          segments: isl.segments.map(function (sg) { return { point: [sg.point.x, sg.point.y], handleIn: [sg.handleIn.x, sg.handleIn.y], handleOut: [sg.handleOut.x, sg.handleOut.y] }; }),
          closed: isl.closed,
          fillColor: combFill, strokeColor: combStroke, strokeWidth: styleSource.strokeWidth, opacity: styleSource.opacity,
          // hasRealStroke (app.js desP / export.js lottieBuild) is
          // AUTHORITATIVE, checked as !!sd.hasRealStroke — derived, not
          // copied straight from styleSource (2026-07-29 QA-confirmed).
          hasRealStroke: !!combStroke,
          isGroupCombineResult: true, groupCombineOf: gid,
          isDuplicatorCopy: memberDicts[0].isDuplicatorCopy, dupIndex: thisDi,
        });
      });
      return islands;
    }
    // A member's transient Path, companion pre-merged and element Motion
    // baked — extracted from the old inline loop so the recursion above can
    // build one operand at a time.
    function dictPathFor(sd, temps) {
      var p = dictToPath(sd);
      if (sd.linkedFillId) {
        var companionDict = strokes.filter(function (d) {
          return d.isLinkedFillCompanion && d.linkedFillId === sd.linkedFillId && (d.dupIndex || 0) === (sd.dupIndex || 0);
        })[0];
        if (companionDict && companionDict.segments && companionDict.segments.length) {
          var cp = dictToPath(companionDict);
          // Same ring-vs-boolean problem as foldBooleanOp's own pre-step
          // (tools.js) — p.unite(cp) on a closed vector-brush ribbon (a
          // self-touching "sliced ring" Path) can come back empty. The
          // companion already fills the ring's own hole by construction, so
          // recovering the ring's outer boundary alone sidesteps it.
          var unsliced = _unsliceRingPath(p);
          if (unsliced) {
            var outerP = new Path({ insert: false, closed: true });
            unsliced.exterior.forEach(function (seg) { outerP.add(seg); });
            p.remove(); p = outerP;
          } else {
            try { var merged = p.unite(cp, { insert: false }); if (merged) { p.remove(); p = merged; } } catch (e) {}
          }
          cp.remove();
          if (suppressed.indexOf(companionDict) === -1) suppressed.push(companionDict);
        }
      }
      // After the companion merge, so the whole visible shape (ribbon + its
      // fill) moves as one — a companion has no elementMotion entry of its
      // own, it follows its anchor.
      p = bakeElementMotion(p, sd);
      temps.push(p);
      return p;
    }
    Object.keys(ld.groups).forEach(function (gid) {
      var grp = ld.groups[gid];
      if (!grp) return;
      if (grp.parent && ld.groups[grp.parent]) return; // roots only
      var strokeIds = collectGroupStrokeIds(gid, ld);
      if (!strokeIds.length) return;
      var byDupIndex = {};
      strokes.forEach(function (sd) {
        if (!sd.strokeId || strokeIds.indexOf(sd.strokeId) === -1) return;
        var di = sd.dupIndex || 0;
        (byDupIndex[di] || (byDupIndex[di] = [])).push(sd);
      });
      Object.keys(byDupIndex).forEach(function (diKey) {
        var dicts = {};
        byDupIndex[diKey].forEach(function (sd) { dicts[sd.strokeId] = sd; });
        var temps = [];
        var result = dictOperands(gid, dicts, Number(diKey), false, [], 0, temps, extra);
        temps.forEach(function (t) { if (!t.removed && result.indexOf(t) === -1) t.remove(); });
      });
    });
    if (!extra.length) return strokes;
    var out = strokes.map(function (sd) {
      if (suppressed.indexOf(sd) !== -1) {
        var sd2 = JSON.parse(JSON.stringify(sd));
        sd2.fillColor = null; sd2.strokeColor = null;
        // hasRealStroke:false is AUTHORITATIVE for desP (app.js) — without
        // it, desP's own serP-fallback restores a phantom white stroke on
        // any strokeColor:null it doesn't otherwise recognize as genuinely
        // strokeless (CLAUDE.md §2's documented desP contract).
        sd2.hasRealStroke = false;
        return sd2;
      }
      return sd;
    });
    // Splice each merged result right after its group's front-most member
    // (same depth rule as buildSceneJson's fix) — back-most target first
    // so earlier splices never shift a later one; islands keep their order.
    extra.sort(function (a, b) { return b.__afterIdx - a.__afterIdx; });
    var lastAfter = null, cursor = 0;
    extra.forEach(function (ex) {
      if (ex.__afterIdx !== lastAfter) { lastAfter = ex.__afterIdx; cursor = ex.__afterIdx + 1; }
      delete ex.__afterIdx;
      out.splice(cursor++, 0, ex);
    });
    return out;
  }

  window.SMGroup = {
    groupSelection: groupSelection, ungroupSelection: ungroupSelection, membersOf: membersOf,
    ensureLayerGroups: ensureLayerGroups, resolveGroupMembers: resolveGroupMembers,
    collectGroupStrokeIds: collectGroupStrokeIds, rootGroupOf: rootGroupOf, rootGroupOfItem: rootGroupOfItem,
    parentGroupOf: parentGroupOf, dissolveGroup: dissolveGroup,
    combineSelection: combineSelection, setGroupCombineMode: setGroupCombineMode,
    removeMemberFromGroup: removeMemberFromGroup, flattenGroup: flattenGroup, renameGroup: renameGroup,
    renderCombinesFromChildren: renderCombinesFromChildren, applyCombinesToStrokes: applyCombinesToStrokes,
  };
})();
