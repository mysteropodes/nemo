/* Native canvas grouping of the Select selection (#2372, X-group-canvas-ops): Cmd+G (group), Cmd+Shift+G (ungroup), the combine panel's
 * "Sortir du groupe" (remove from group) and "Aplatir" (flatten). On a native object project each one is ONE request through the existing
 * group intent (native-object-group-intent.js): object.group.create / object.group.ungroup / object.group.member.remove / object.boolean.flatten.
 * One request is one sealed owner transaction, so one revision and one history entry; the baseline rules (1065e9d8 group-bridge.js
 * groupSelection / ungroupSelection / removeMemberFromGroup / flattenGroup) are decided by the engine owner - units not shapes, the outermost
 * level dissolves, a group left below two entries dissolves, a flatten bakes the combined outline and drops the subtree - so no Paper / ld.groups
 * writer is reached from here. The group state of the selection is READ from the native listing (the groupPath of each listed record).
 * A click on any member of a vector group selects the whole group (baseline select-bridge.js :1655 SMGroup.membersOf of the clicked item; a Shift click on a
 * selected member removes the whole group): `members` reads the root group's members from the layer listing, `adopt` applies the rule to the pick that
 * just happened, and `widen` does both for a click. A press that becomes a drag widens before the drag starts (native-select-gesture.js).
 * This module holds no document state: the selection is read again at the presented revision before every request, and a refusal, a stale
 * selection or an absent owner is a notice and a resolved status, never a throw. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeSelectGroup = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  var PINS = ['instanceId', 'documentId', 'lifecycleGeneration', 'contentRevision', 'documentSnapshotId'];
  var MODE_IDS = { unite: 'btn-combine-unite', subtract: 'btn-combine-subtract', intersect: 'btn-combine-intersect', exclude: 'btn-combine-exclude' };
  var jobs = new WeakMap(), intents = new WeakMap(), panels = new WeakMap();
  function notice(host, text) { if (text && typeof host.showToast === 'function') try { host.showToast(text); } catch (_) {} }
  // The app's own wording where it has one, English otherwise.
  function said(host, key, fallback) {
    try { var t = host.SM && typeof host.SM.t === 'function' ? host.SM.t(key) : null; return t && t !== key ? t : fallback; } catch (_) { return fallback; }
  }
  function session(host) {
    try { var P = host.NemoNativeObjectProject, s = P && typeof P.desktop === 'function' ? P.desktop(host) : null; return s && s.blocksLegacy() === true ? s : null; } catch (_) { return null; }
  }
  // True while a native object project owns the document (the legacy group writers must not run then).
  function native(host) { return !!session(host); }
  function samePin(a, b) { return !!a && !!b && PINS.every(function (k) { return a[k] === b[k]; }); }
  function livePin(s) { try { return s.info(); } catch (_) { return null; } }
  function same(a, b) { return a.length === b.length && a.every(function (id, at) { return id === b[at]; }); }
  function selected(host) {
    return (host.selectedPaths || []).filter(function (p) { return p && p.data && p.data.nativeStableTarget && Number.isInteger(p.data.nativeLayerIndex) && p.data.strokeId; });
  }
  function idsOf(paths) { return paths.map(function (p) { return p.data.strokeId; }); }
  // The group intent over this session's ports (the shape of native-selection-boolean's), bound once per session.
  function intentFor(host, s) {
    if (intents.has(s)) return intents.get(s);
    var Group = host.NemoNativeObjectGroupIntent, Component = host.NemoNativeObjectComponentIntent, Contract = host.NemoNativeObjectEditContract;
    if (!Group || !Component || !Contract || typeof Component.portsFromSession !== 'function') return null;
    var base = Component.portsFromSession(s);
    var made = Group.createGroupIntent({ pin: base.pin, requestId: base.requestId, submit: base.submit,
      declared: function (operation) { return Contract.admitted(operation) && base.readDeclared() === true; } });
    intents.set(s, made);
    return made;
  }
  function groupId() { return 'grp_' + Date.now().toString(36) + '_' + Math.floor(Math.random() * 1e6); }
  // Why the owner did not apply the edit, in words (the owner's classification, never a guess about geometry).
  function refusal(out, what) {
    if (out.code === 'stale_revision' || out.code === 'stale_selection') return 'The document changed — nothing was changed.';
    if (out.code === 'unavailable' || out.code === 'not_found') return what + ' is not available for this selection (locked, hidden or animated shapes are refused).';
    return what + ' could not be applied' + (out.message ? ': ' + out.message : '.');
  }

  // ---- the group state of a listing ---------------------------------------------------------------------------------------------------------
  // items: the layer's listing (paint order, bottom first). A linked fill and a texture dab are never their own selection (they follow their
  // ribbon / anchor), so they are not members here. groupPath is outermost first: its last entry is the shape's immediate group.
  function groupsInfo(items) {
    var members = {}, modes = {}, pathOf = {};
    (items || []).forEach(function (item) {
      if ((item.role === 'fill' && item.owner) || item.brush === 'copy') return;
      var id = item.stableTarget.strokeId, path = Array.isArray(item.groupPath) ? item.groupPath : [];
      pathOf[id] = path;
      path.forEach(function (g) { (members[g.id] = members[g.id] || []).push(id); modes[g.id] = typeof g.mode === 'string' ? g.mode : 'none'; });
    });
    return { members: members, modes: modes, pathOf: pathOf };
  }
  // The baseline rule (groupSelection): the LARGEST group whose every leaf is selected counts as one unit, any other shape as one.
  function unitCount(info, ids) {
    var seen = {}, n = 0;
    ids.forEach(function (id) {
      var path = info.pathOf[id] || [], key = id;
      for (var k = path.length - 1; k >= 0; k--) {
        if (!info.members[path[k].id].every(function (m) { return ids.indexOf(m) >= 0; })) break;
        key = 'g:' + path[k].id;
      }
      if (!seen[key]) { seen[key] = true; n++; }
    });
    return n;
  }
  // The baseline updateCombinePanel rule: the first selected shape's immediate group, when it is a combine group (mode other than none) and
  // the selection is that one shape or exactly the group's leaves.
  function activeGroup(info, ids) {
    var path = ids.length ? info.pathOf[ids[0]] || [] : [], gid = path.length ? path[path.length - 1].id : null;
    if (!gid || info.modes[gid] === 'none') return null;
    if (ids.length === 1) return gid;
    var all = info.members[gid];
    return all.length === ids.length && all.every(function (id) { return ids.indexOf(id) >= 0; }) ? gid : null;
  }

  // ---- reads and edits -------------------------------------------------------------------------------------------------------------------------
  function fail(host, status, text) { notice(host, text); return { failed: true, status: status }; }
  // The selection read again at the presented revision: fresh paths in paint order and the whole layer's listing for the group state.
  async function prepare(host, minimum, few) {
    var s = session(host), I = host.NemoSelectCanvasIntent, shown = host.selectedPaths || [];
    if (!s || !I) return fail(host, 'unavailable', 'Grouping is not available right now.');
    if (shown.length < minimum) return fail(host, 'noop', few);
    var paths = selected(host);
    if (paths.length !== shown.length) return fail(host, 'refused', 'Select shapes on the canvas first.');
    if (paths.some(function (p) { return p.data.nativeLayerIndex !== paths[0].data.nativeLayerIndex; })) return fail(host, 'refused', 'Select shapes on one layer to group them.');
    var intent = intentFor(host, s), pin = livePin(s), want = idsOf(paths), layers = null;
    if (!intent) return fail(host, 'unavailable', 'Grouping is not available right now.');
    try {
      await I.presentedSettled(host);
      layers = await I.layerPaths(host, function (item) { return want.indexOf(item.stableTarget.strokeId) >= 0; }, false, paths[0].data.nativeLayerIndex);
    } catch (_) { layers = null; }
    if (!layers || layers.paths.length !== want.length || !samePin(layers.pin, pin) || !samePin(livePin(s), pin)) return fail(host, 'stale', 'The selection changed — nothing was changed.');
    return { I: I, intent: intent, original: paths, layers: layers, ids: idsOf(layers.paths), info: groupsInfo(layers.items), context: I.heldId(host), layerIdx: paths[0].data.nativeLayerIndex };
  }
  async function commit(host, call, what) {
    var out;
    try { out = await call(); } catch (_) { out = { ok: false, code: 'unavailable' }; }
    if (!out || !out.ok) return fail(host, out && out.code || 'unavailable', refusal(out || {}, what));
    if (!out.applied) return fail(host, 'noop', 'Nothing changed.');
    return out;
  }
  function reselect(host, original, fresh) {
    var O = host.NemoNativeSelectOverlay;
    // The user may have picked something else while the request ran: their selection stands.
    if (same(idsOf(selected(host)), idsOf(original))) {
      host.selectedPaths = fresh && fresh.complete ? fresh.paths : [];
      if (host.state) host.state.selectedStrokeIndices = [];
    }
    panels.delete(host);
    if (O && typeof O.show === 'function') try { O.show(host); } catch (_) { /* the outline is a nicety; the edit stands */ }
    if (typeof host.updateUI === 'function') host.updateUI();
    if (host.SMEngineBridge && typeof host.SMEngineBridge.renderNow === 'function') host.SMEngineBridge.renderNow();
  }
  // The same shapes as fresh presented paths after the edit (grouping changes no stroke id).
  async function again(host, c) {
    var fresh = null;
    for (var tries = 0; tries < 2 && !fresh; tries++) {
      try { fresh = await c.I.reread(host, c.original, false); } catch (_) { fresh = null; }
    }
    return fresh;
  }

  // Cmd+G: the selection becomes one group (the units rule above decides what counts as an element).
  async function doGroup(host) {
    var few = said(host, 'toastSelectAtLeast2ElementsToGroup', 'Select at least 2 elements to create a group');
    if (host.state && host.state.tool !== 'select' && host.state.tool !== 'subselect') return fail(host, 'noop', few);
    var c = await prepare(host, 2, few);
    if (c.failed) return c;
    var units = unitCount(c.info, c.ids);
    // Everything selected already belongs to one and the same group: grouping it again would wrap a single unit.
    if (units < 2) return fail(host, 'noop', few);
    var out = await commit(host, function () {
      return c.intent.group({ layerUid: c.layers.layerUid, contextId: c.context, groupId: groupId(), name: said(host, 'autoNameGroup', 'Group'), strokeIds: c.ids }); }, 'Group');
    if (out.failed) return out;
    reselect(host, c.original, await again(host, c));
    notice(host, said(host, 'toastGroupCreatedSuffix', 'Group created (') + units + said(host, 'toastElementsCloseParen', ' elements)'));
    return { status: 'applied', revision: out.revision };
  }
  // Cmd+Shift+G: the outermost group of each selected shape dissolves (one level at a time).
  async function doUngroup(host) {
    var c = await prepare(host, 1, said(host, 'toastSelectGroupToUngroupFirst', 'Select a group to ungroup first'));
    if (c.failed) return c;
    var grouped = c.ids.filter(function (id) { return (c.info.pathOf[id] || []).length > 0; });
    if (!grouped.length) return fail(host, 'noop', said(host, 'toastSelectionHasNoGroup', 'The selection contains no group'));
    var out = await commit(host, function () { return c.intent.ungroup({ layerUid: c.layers.layerUid, contextId: c.context, strokeIds: grouped }); }, 'Ungroup');
    if (out.failed) return out;
    reselect(host, c.original, await again(host, c));
    notice(host, said(host, 'toastGroupUngrouped', 'Group ungrouped'));
    return { status: 'applied', revision: out.revision };
  }
  // "Sortir du groupe": ONE shape leaves its group, the rest stays (a group left below two entries dissolves, in the owner).
  async function doRemove(host) {
    var c = await prepare(host, 1, '');
    if (c.failed) return c;
    if (c.ids.length !== 1 || !(c.info.pathOf[c.ids[0]] || []).length) return { status: 'noop' };
    var out = await commit(host, function () { return c.intent.removeMember({ layerUid: c.layers.layerUid, contextId: c.context, strokeId: c.ids[0] }); }, 'Remove from group');
    if (out.failed) return out;
    reselect(host, c.original, await again(host, c));
    return { status: 'applied', revision: out.revision };
  }
  // "Aplatir": the combine group the selection IS becomes one real shape (destructive, explicit); the result islands are selected.
  async function doFlatten(host) {
    var c = await prepare(host, 1, '');
    if (c.failed) return c;
    var gid = activeGroup(c.info, c.ids);
    if (!gid || c.info.members[gid].length < 2) return { status: 'noop' };
    var before = {};
    c.layers.items.forEach(function (item) { before[item.stableTarget.strokeId] = true; });
    var out = await commit(host, function () {
      return c.intent.flatten({ layerUid: c.layers.layerUid, contextId: c.context, frame: c.layers.frame, groupId: gid }); }, 'Flatten');
    if (out.failed) return out;
    var fresh = null;
    for (var tries = 0; tries < 2 && !fresh; tries++) {
      try {
        // The islands are the records the layer did not list before (the owner puts them at the first member's slot).
        fresh = await c.I.layerPaths(host, function (item) {
          return !before[item.stableTarget.strokeId] && !(item.role === 'fill' && item.owner) && item.brush !== 'copy'; }, false, c.layerIdx);
      } catch (_) { fresh = null; }
    }
    reselect(host, c.original, fresh && { complete: true, paths: fresh.paths });
    notice(host, 'Groupe aplati');
    return { status: 'applied', revision: out.revision };
  }

  // The combine mode of the group the selection IS (baseline SMGroup.setGroupCombineMode from the group row of the right-click menu or a panel mode button).
  async function doMode(host, mode) {
    var c = await prepare(host, 1, '');
    if (c.failed) return c;
    var gid = activeGroup(c.info, c.ids);
    if (!gid || ['unite', 'subtract', 'intersect', 'exclude'].indexOf(mode) < 0) return { status: 'noop' };
    if (c.info.modes[gid] === mode) return { status: 'noop' };
    var out = await commit(host, function () { return c.intent.setMode({ layerUid: c.layers.layerUid, contextId: c.context, groupId: gid, mode: mode }); }, 'Combine mode');
    if (out.failed) return out;
    reselect(host, c.original, await again(host, c));
    return { status: 'applied', revision: out.revision };
  }

  // One edit at a time: a second gesture waits for the first and reads the selection it left.
  function run(host, job) {
    if (!jobs.has(host)) jobs.set(host, Promise.resolve());
    var next = jobs.get(host).then(function () { return job(host); }).catch(function () { notice(host, 'Grouping is not available right now.'); return { status: 'unavailable' }; });
    jobs.set(host, next);
    return next;
  }

  // ---- the combine panel -----------------------------------------------------------------------------------------------------------------------
  // The panel reads the group of the selection from the native listing (a read per selection / revision / frame, kept until one of them changes).
  function panelKey(s, host, paths) {
    var pin = livePin(s);
    return JSON.stringify([pin && PINS.map(function (k) { return pin[k]; }), host.state && host.state.currentFrame, paths[0].data.nativeLayerIndex, idsOf(paths)]);
  }
  async function readPanel(host, s, paths) {
    var I = host.NemoSelectCanvasIntent, ids = idsOf(paths), pin = livePin(s);
    if (!I || !(await I.presentedSettled(host))) return null;
    var layers = await I.layerPaths(host, function () { return false; }, false, paths[0].data.nativeLayerIndex);
    if (!layers || !samePin(layers.pin, pin)) return null;
    var info = groupsInfo(layers.items), inLayer = ids.filter(function (id) { return info.pathOf[id]; });
    if (inLayer.length !== ids.length) return null;
    var gid = activeGroup(info, ids);
    return { gid: gid, mode: gid ? info.modes[gid] : null };
  }
  function paintPanel(host, count, gid, mode) {
    var doc = host.document, row = doc && doc.getElementById('combine-existing-row');
    var buttons = Object.keys(MODE_IDS).map(function (m) { return doc.getElementById(MODE_IDS[m]); });
    if (!row || buttons.some(function (b) { return !b; })) return;
    var active = gid ? mode : 'unite';
    Object.keys(MODE_IDS).forEach(function (m, at) {
      buttons[at].classList.toggle('ac', m === active);
      var mirror = doc.getElementById('btn-bool-' + m);
      if (mirror) mirror.classList.toggle('ac', !!gid && m === active);
    });
    row.style.display = gid ? 'flex' : 'none';
    buttons.forEach(function (b) { b.disabled = !(gid || count >= 2); });
  }
  // Called by updateCombinePanel on every selection change; the row (Sortir du groupe / Aplatir) and the mode buttons follow the native read.
  function syncPanel(host) {
    var s = session(host), shown = host.selectedPaths || [], paths = selected(host), st = panels.get(host);
    if (!st) panels.set(host, st = { key: null, token: 0, done: false, gid: null, mode: null });
    if (!s || !paths.length || paths.length !== shown.length) { st.key = null; st.token++; paintPanel(host, shown.length, null, null); return; }
    var key = panelKey(s, host, paths);
    if (st.key === key && st.done) { paintPanel(host, paths.length, st.gid, st.mode); return; }
    if (st.key === key) return;
    st.key = key; st.done = false; var token = ++st.token;
    paintPanel(host, paths.length, null, null);
    readPanel(host, s, paths).then(function (r) {
      if (token !== st.token) return;
      if (!r) { st.key = null; return; }
      st.done = true; st.gid = r.gid; st.mode = r.mode;
      paintPanel(host, paths.length, r.gid, r.mode);
    }, function () { if (token === st.token) st.key = null; });
  }

  // ---- a click on a member selects the whole group (baseline membersOf) ----------------------------------------------------------------------
  // The members of the OUTERMOST group `item` sits in (the layer's own listing, paint order), as fresh presented paths; null when it is in no group or the read failed.
  async function members(host, item) {
    var I = host.NemoSelectCanvasIntent, top = item && item.data && Array.isArray(item.data.nativeGroupPath) && item.data.nativeGroupPath[0];
    if (!I || !top || !Number.isInteger(item.data.nativeLayerIndex)) return null;
    var found = null;
    try {
      found = await I.layerPaths(host, function (it) {
        var gp = it.groupPath; return !(it.role === 'fill' && it.owner) && it.brush !== 'copy' && Array.isArray(gp) && !!gp[0] && gp[0].id === top.id;
      }, false, item.data.nativeLayerIndex);
    } catch (_) { found = null; }
    return found && found.paths.length > 1 ? found.paths : null;
  }
  // The pick of `item` (already applied by applyPick) widened to its group: a pick that selected it brings its siblings, a Shift pick that removed it takes them away.
  function adopt(host, item, shift, group) {
    var ids = group.map(function (p) { return p.data.strokeId; }), now = host.selectedPaths || [], kept = shift ? now.filter(function (p) { return ids.indexOf(p.data.strokeId) < 0; }) : [];
    var held = now.some(function (p) { return p.data.strokeId === item.data.strokeId; });
    if (shift && !held) host.selectedPaths = kept; // the Shift pick removed the member: the whole group goes
    else host.selectedPaths = kept.concat(group);
    host.state.selectedStrokeIndices = [];
    if (host.NemoNativeSelectOverlay) host.NemoNativeSelectOverlay.show(host);
    if (typeof host.updateUI === 'function') host.updateUI();
    if (host.SMEngineBridge && typeof host.SMEngineBridge.renderNow === 'function') host.SMEngineBridge.renderNow();
  }
  // A click: read the group, and adopt it only while the pick it belongs to still holds (the tool, frame, layer and revision did not move on, the selection is still that pick's).
  async function widen(host, item, shift) {
    var I = host.NemoSelectCanvasIntent, ctx = I && I.pickContext(host), stamp = host.selectedPaths;
    var group = await members(host, item);
    if (!group || !I || I.pickContext(host) !== ctx || host.selectedPaths !== stamp) return false;
    adopt(host, item, shift, group);
    return true;
  }
  return Object.freeze({ members: members, adopt: adopt, widen: widen,
    native: native, group: function (host) { return run(host, doGroup); }, ungroup: function (host) { return run(host, doUngroup); },
    removeMember: function (host) { return run(host, doRemove); }, setMode: function (host, mode) { return run(host, function (h) { return doMode(h, mode); }); }, flatten: function (host) { return run(host, doFlatten); },
    syncPanel: syncPanel, groupsInfo: groupsInfo, unitCount: unitCount, activeGroup: activeGroup });
}));
