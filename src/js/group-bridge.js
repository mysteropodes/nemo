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
      if (window.showToast) showToast(SM.t('toastSelectAtLeast2ElementsToGroup'));
      return;
    }
    pushUndo();
    var gid = 'grp_' + Date.now().toString(36) + '_' + Math.floor(Math.random() * 1e6);
    var li = state.activeLayerIdx, ld = state.layers[li];
    selectedPaths.forEach(function (p) { if (!p.data) p.data = {}; p.data.groupId = gid; ensureStrokeId(p); });
    // Named from the moment it's created (2026-07-31, group/shape tree
    // panel — Cyril: "vrai panel de gestion de group") — a plain Cmd+G
    // group used to have NO ld.groups entry at all, only the members'
    // data.groupId tag; a combine-group (below) had metadata but no name.
    // Unified here so both kinds of group show up named in the tree panel
    // from day one, not just after an explicit rename.
    if (ld) {
      ensureLayerGroups(ld);
      // 2026-08 fix: hardcoded French default name, shown regardless of locale.
      ld.groups[gid] = { name: SM.t('autoNameGroup'), combineMode: 'none', order: selectedPaths.map(function (p) { return p.data.strokeId; }) };
    }
    saveActiveLayerFrame(); updateUI();
    if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
    if (window.showToast) showToast(SM.t('toastGroupCreatedSuffix') + selectedPaths.length + SM.t('toastElementsCloseParen'));
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
    selectedPaths.forEach(function (p) { if (p.data) delete p.data.groupId; });
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
    // Mirrors groupSelection's own "a group needs >=2 members" invariant —
    // one member left isn't a group anymore.
    if (grp.order.length < 2) {
      grp.order.forEach(function (e) {
        var item = findByStrokeId(layer, e);
        if (item && item.data) delete item.data.groupId;
      });
      delete ld.groups[gid];
    }
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
    delete ld.groups[gid];
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
    Object.keys(ld.groups).forEach(function (gid) {
      var grp = ld.groups[gid];
      if (!grp || grp.combineMode === 'none') return;
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
        var dupIndex = Number(dupIndexKey);
        var members = resolveGroupMembers(gid, ld, layer, null, 0, dupIndex);
        if (members.length < 2) return;
        members.forEach(function (m) { suppress.push(m); });
        var styleSource = members[members.length - 1];
        var islands;
        try { islands = computeGroupCombine(members, grp.combineMode, layer, frameIdx); }
        catch (e) { console.warn('[SMGroup] combine failed for group', gid, e); return; }
        // Vector-brush ribbon style source (2026-08 fix, feedback: "les
        // combine gère mal stroke plus fill" for Pressure-brush shapes) —
        // a ribbon's OWN fillColor is its ink/stroke color, never a real
        // fill (isVectorBrush ribbons paint through fillColor — see
        // app.js's serP comment); any real Fill the user set lives on a
        // completely separate companion Path. Reading styleSource.fillColor
        // unconditionally as "the fill" painted the combined shape solid
        // in the STROKE's own color with no fill at all — confirmed live.
        // Resolve both from the ribbon's own companion, same lookup
        // foldBooleanOp already uses to fold it into the boolean operand.
        var srcIsVB = !!(styleSource.data && styleSource.data.isVectorBrush);
        var isl_fill, isl_stroke;
        if (srcIsVB) {
          var srcCompanion = findLinkedFillCompanion(layer, styleSource);
          isl_fill = srcCompanion ? srcCompanion.fillColor : null;
          isl_stroke = styleSource.fillColor;
        } else {
          // Stroke-only style source (2026-07-29 fix, QA-confirmed live:
          // combining two stroke-only brush strokes rendered as a thin
          // outline tracing the merged silhouette instead of a solid merged
          // stroke) — computeGroupCombine already expands a stroke-only
          // member into its real filled ribbon geometry (foldBooleanOp), so
          // painting that geometry with ONLY strokeColor (no fill) drew just
          // its outline. Flip to filled-with-the-stroke's-own-color, same
          // convention as eraseExpandStrokeToFill/booleanOp's identical fix.
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
    Object.keys(ld.groups).forEach(function (gid) {
      var grp = ld.groups[gid];
      if (!grp || grp.combineMode === 'none') return;
      var strokeIds = collectGroupStrokeIds(gid, ld);
      if (!strokeIds.length) return;
      var byDupIndex = {};
      strokes.forEach(function (sd) {
        if (!sd.strokeId || strokeIds.indexOf(sd.strokeId) === -1) return;
        var di = sd.dupIndex || 0;
        (byDupIndex[di] || (byDupIndex[di] = [])).push(sd);
      });
      Object.keys(byDupIndex).forEach(function (diKey) {
        var memberDicts = byDupIndex[diKey].filter(function (sd) { return !sd.isRaster && sd.segments && sd.segments.length; });
        if (memberDicts.length < 2) return;
        var thisDi = memberDicts[0].dupIndex;
        var paths = memberDicts.map(function (sd) {
          var p = dictToPath(sd);
          if (sd.linkedFillId) {
            var companionDict = strokes.filter(function (d) {
              return d.isLinkedFillCompanion && d.linkedFillId === sd.linkedFillId && (d.dupIndex || 0) === (sd.dupIndex || 0);
            })[0];
            if (companionDict && companionDict.segments && companionDict.segments.length) {
              var cp = dictToPath(companionDict);
              // Same ring-vs-boolean problem as foldBooleanOp's own
              // pre-step (tools.js) — p.unite(cp) on a closed vector-brush
              // ribbon (a self-touching "sliced ring" Path, see
              // _findRingRevisit's comment there) can come back empty. The
              // companion already fills the ring's own hole by
              // construction, so recovering the ring's outer boundary
              // alone — no boolean call — sidesteps it here too.
              var unsliced = _unsliceRingPath(p);
              if (unsliced) {
                var outerP = new Path({ insert: false, closed: true });
                unsliced.exterior.forEach(function (seg) { outerP.add(seg); });
                p.remove(); p = outerP;
              } else {
                try { var merged = p.unite(cp, { insert: false }); if (merged) { p.remove(); p = merged; } } catch (e) {}
              }
              cp.remove();
              suppressed.push(companionDict);
            }
          }
          // After the companion merge, so the whole visible shape (ribbon +
          // its fill) moves as one — a companion has no elementMotion entry
          // of its own, it follows its anchor.
          return bakeElementMotion(p, sd);
        });
        memberDicts.forEach(function (sd) { suppressed.push(sd); });
        var styleSource = memberDicts[memberDicts.length - 1];
        var combFill, combStroke;
        if (styleSource.isVectorBrush) {
          // Same fix as renderCombinesFromChildren's twin above, dict
          // form: a ribbon dict's own fillColor is its ink/stroke color,
          // never a real fill — the real Fill (if any) is a separate
          // companion dict, same lookup used a few lines up to merge its
          // geometry in.
          var styleCompanion = styleSource.linkedFillId ? strokes.filter(function (d) {
            return d.isLinkedFillCompanion && d.linkedFillId === styleSource.linkedFillId && (d.dupIndex || 0) === (styleSource.dupIndex || 0);
          })[0] : null;
          combFill = styleCompanion ? styleCompanion.fillColor : null;
          combStroke = styleSource.fillColor;
        } else {
          // Stroke-only style source (2026-07-29 fix, same one as
          // renderCombinesFromChildren's twin above) — computeGroupCombine
          // just expanded a stroke-only member into a real filled ribbon
          // (dictToPath's new paint-carrying + foldBooleanOp's own
          // eraseExpandStrokeToFill step), so the exported dict must flip to
          // filled-with-the-stroke's-color too, or it round-trips as an
          // outline-only shape (or, pre-fix, nothing visible at all).
          var srcHasRealStroke = !!styleSource.hasRealStroke;
          combFill = styleSource.fillColor || (srcHasRealStroke ? styleSource.strokeColor : null);
          combStroke = (styleSource.fillColor && srcHasRealStroke) ? styleSource.strokeColor : null;
        }
        var islands;
        try { islands = computeGroupCombine(paths, grp.combineMode, null); }
        catch (e) { console.warn('[SMGroup] combine failed for group', gid, e); return; }
        islands.forEach(function (isl) {
          extra.push({
            segments: isl.segments.map(function (s) { return { point: [s.point.x, s.point.y], handleIn: [s.handleIn.x, s.handleIn.y], handleOut: [s.handleOut.x, s.handleOut.y] }; }),
            closed: isl.closed,
            fillColor: combFill, strokeColor: combStroke, strokeWidth: styleSource.strokeWidth, opacity: styleSource.opacity,
            // hasRealStroke (app.js desP / export.js lottieBuild) is
            // AUTHORITATIVE, checked as !!sd.hasRealStroke — see
            // combFill/combStroke above for why it's derived, not copied
            // straight from styleSource anymore (2026-07-29 QA-confirmed:
            // copying it straight resurrected a phantom white stroke
            // whenever the topmost member was one of the strokeless
            // members CLAUDE.md §2 documents).
            hasRealStroke: !!combStroke,
            isGroupCombineResult: true, groupCombineOf: gid,
            isDuplicatorCopy: memberDicts[0].isDuplicatorCopy, dupIndex: thisDi,
          });
        });
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
    return out.concat(extra);
  }

  window.SMGroup = {
    groupSelection: groupSelection, ungroupSelection: ungroupSelection, membersOf: membersOf,
    ensureLayerGroups: ensureLayerGroups, resolveGroupMembers: resolveGroupMembers,
    combineSelection: combineSelection, setGroupCombineMode: setGroupCombineMode,
    removeMemberFromGroup: removeMemberFromGroup, flattenGroup: flattenGroup, renameGroup: renameGroup,
    renderCombinesFromChildren: renderCombinesFromChildren, applyCombinesToStrokes: applyCombinesToStrokes,
  };
})();
