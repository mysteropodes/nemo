/* Motion on a native object project: the ELEMENT level of the canvas (#2520/#2525, C-motion-canvas-4). Baseline motion.js@1065e9d8 ~6258-6620 and
 * select-bridge.js@1065e9d8 ~803-960: a double-click on a shape of the active layer enters its group (per-object mode) and targets that shape; while a
 * shape is targeted the whole-layer gizmo gives way to the shape's own box, a click on a sibling hands the target over, a click elsewhere leaves the
 * group (the layer gizmo returns), and a drag of the targeted shape is `elementMove`: a Motion edit of the ELEMENT's own Position (a key at the playhead
 * when the stopwatch is on, the static value otherwise), never an edit of its geometry.
 * Here the picked shape is the presented native record (select-canvas-intent `pick`), its box is the select overlay's box (drawn from the hit-test
 * listing: `bounds` and `matrix`), and the move is ONE `object.property.edit` of `position` with the record's `strokeId`: one revision, one history entry.
 * The world drag delta is pulled back through the layer's pose (the hit-test listing's `layerMatrix`, own Motion and parent chain) because an element's
 * Position sits underneath the layer transform (baseline `layerMotionPointMap().invVec`). Isolation is only a flag on the layer the shape belongs to; the
 * selection itself is root.selectedPaths. This file holds no document state and never reads a JS layer record. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else {
    root.NemoNativeMotionElement = api;
    if (root.document && typeof root.document.addEventListener === 'function') { try { api.install(root); } catch (_) { /* nothing to listen on */ } }
  }
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  var PINS = ['instanceId', 'documentId', 'lifecycleGeneration', 'contentRevision', 'documentSnapshotId'];
  var GONE = 'The document changed during the drag — nothing was changed.';
  var round = function (v) { return Math.round(v * 1e6) / 1e6; };
  var flags = new WeakMap();
  function flag(win) { if (!flags.has(win)) flags.set(win, { layerUid: null, busy: false }); return flags.get(win); }
  function notice(win, text) { if (typeof win.showToast === 'function') try { win.showToast(text, 'warn'); } catch (_) { /* presentation only */ } }
  function inMotion(win) { var st = win.state; return !!st && st.appMode === 'motion' && st.playing !== true; }
  function selected(win) { return (win.selectedPaths || []).filter(function (p) { return p && p.data && p.data.nativeStableTarget; }); }
  // True while a shape of the ACTIVE layer is targeted inside its entered group: every selected item sits on that layer and the layer is the one entered.
  // The whole-layer gizmo of native-motion-canvas.js gives way to the shape's box then. Never true outside Motion, with nothing selected, or after the
  // active layer changed (the flag is read lazily, so every way out of the group is covered without a listener on each).
  function isolated(win) {
    var f = flag(win), st = win.state, sel = selected(win);
    if (!f.layerUid || !inMotion(win) || !st || st.tool !== 'select' || !sel.length) return false;
    return sel.every(function (p) { return p.data.nativeLayerUid === f.layerUid && p.data.nativeLayerIndex === st.activeLayerIdx; });
  }
  // Double-click on the canvas in Motion: the shape of the active layer under the pointer becomes the target (a group entry; baseline tools.js dblclick).
  async function enter(win, e) {
    var I = win.NemoSelectCanvasIntent, st = win.state, f = flag(win);
    if (!inMotion(win) || !I || st.tool !== 'select' || !I.objectProject(win) || f.busy) return false;
    var area = win.document.getElementById && win.document.getElementById('canvas-area');
    if (area && e.target && typeof area.contains === 'function' && !area.contains(e.target)) return false;
    var ctx = I.pickContext(win), item;
    f.busy = true;
    try { item = await I.pick(win, e.clientX, e.clientY, st.activeLayerIdx); } catch (_) { item = undefined; } finally { f.busy = false; }
    if (!item || I.pickContext(win) !== ctx || !item.data || !item.data.strokeId) return false;
    f.layerUid = item.data.nativeLayerUid;
    I.applyPick(win, item, false);
    return true;
  }
  function samePin(a, b) { return !!a && !!b && PINS.every(function (k) { return a[k] === b[k]; }); }
  function selectorOf(info, view) {
    return { instanceId: info.instanceId, documentId: info.documentId, atRevision: info.contentRevision, documentSnapshotId: info.documentSnapshotId, view: view };
  }
  // The delta of an element drag in the layer's own space: `world` pulled back through the inverse of the layer pose (own Motion and parent chain).
  // A layer whose row says it carries no Motion needs no pose read; one that does must be read, never guessed.
  async function localDelta(win, idx, pin, frame, world) {
    var P = win.NemoNativeLayerMotionPose, I = win.NemoSelectCanvasIntent, row = I.layerList(win, pin)[idx];
    if (!P || !P.needs(row)) return world.slice();
    var inv = await P.read(win, idx, pin, frame);
    return inv.vector(world);
  }
  // ONE `object.property.edit` of the targeted shape's Position. `sel` is the selection the drag started on, `matrix` the world move affine of the
  // gesture (a pure translation, Shift axis lock already applied), `o`: { pin, stale(), cancelled(), release() } as commitMatrix takes. Resolves to
  // 'applied' or a refusal ('refused' / 'stale' / 'failed' / 'cancelled'); a refusal has already toasted.
  async function commitMove(win, sel, matrix, o) {
    o = o || {};
    var I = win.NemoSelectCanvasIntent, W = win.NemoNativeObjectWidgetUI, Wait = win.NemoNativePresentationWait;
    if (!inMotion(win) || !I || !W) { notice(win, 'Moving this element is not available right now.'); return 'refused'; }
    if (sel.length !== 1) { notice(win, 'Move one element at a time in Motion.'); return 'refused'; }
    var p = sel[0], t = p.data.nativeStableTarget, idx = p.data.nativeLayerIndex;
    if (!Number.isInteger(idx) || !t.strokeId || !(p.data.nativeRole)) { notice(win, 'Select an element to move it.'); return 'refused'; }
    if (o.stale && o.stale()) { notice(win, GONE); return 'stale'; }
    var held = I.heldId(win), heldArg = held === 'scene-root' ? undefined : held;
    var session = W.session(win), proof = W.proofNow(win, 'readProperty', heldArg);
    if (!proof && session && Wait) {
      await Wait.until(win, session, function () { return !!W.proofNow(win, 'readProperty', heldArg); }, function () { return !(o.cancelled && o.cancelled()); });
      proof = W.proofNow(win, 'readProperty', heldArg);
    }
    if (!proof || !proof.current(true) || (o.pin && !samePin(proof.info, o.pin))) { notice(win, GONE); return 'stale'; }
    var target = { contextId: proof.selector.contextId, layerUid: t.layerUid, strokeId: t.strokeId }, frame = proof.selector.frame;
    try {
      var local = await localDelta(win, idx, o.pin || proof.info, frame, [matrix[4], matrix[5]]);
      var base = await proof.session.readProperty(selectorOf(proof.info, { kind: 'rawSample', target: target, property: 'position', frame: frame }));
      if (!base || !Array.isArray(base.value) || base.value.length !== 2 || !base.value.every(Number.isFinite)) throw new Error('The element position could not be read.');
      if (o.cancelled && o.cancelled()) return 'cancelled';
      var pin = {}; PINS.forEach(function (k) { pin[k] = proof.info[k]; });
      if (!proof.current(true)) { notice(win, GONE); return 'stale'; }
      if (o.release) o.release();
      var payload = { command: 'object.property.edit', target: target, property: 'position',
        action: { kind: 'setValue', frame: frame, values: [round(base.value[0] + local[0]), round(base.value[1] + local[1])] } };
      var response = await proof.session.editProperty(payload, pin, function () { return proof.current(); });
      if (!response || response.ok !== true || !response.result || typeof response.result.applied !== 'boolean') throw new Error('Native property edit was not confirmed.');
      if (!response.result.applied) return 'noop';
    } catch (error) {
      notice(win, (error && error.message || 'Native Motion edit is unavailable.') + (error && error.code ? ' (' + error.code + ')' : ''));
      return 'failed';
    }
    // The committed element as a fresh presented path (the old one holds the geometry from before the move), still targeted.
    var fresh = null;
    try { fresh = await I.reread(win, sel, false); } catch (_) { fresh = null; }
    win.selectedPaths = fresh && fresh.complete ? fresh.paths : [];
    win.state.selectedStrokeIndices = [];
    if (!win.selectedPaths.length) flag(win).layerUid = null;
    return 'applied';
  }
  function install(win) {
    var doc = win.document;
    doc.addEventListener('dblclick', function (e) {
      if (!inMotion(win) || !win.NemoSelectCanvasIntent || !win.state || win.state.tool !== 'select') return;
      enter(win, e).then(function (taken) { if (taken) { try { e.preventDefault(); } catch (_) { /* already dispatched */ } } }, function () { /* a failed read targets nothing */ });
    }, { capture: true });
    return api;
  }
  var api = Object.freeze({ isolated: isolated, enter: enter, commitMove: commitMove, install: install, localDelta: localDelta, GONE: GONE });
  return api;
}));
