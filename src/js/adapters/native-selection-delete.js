/* Native Select + Delete (#2256, baseline timeline.js onKeyDown -> SM.deleteSelStrokes @1065e9d8): Delete or Backspace with strokes picked by the Select tool removes
 * them as exactly ONE native `object.vector.batch` of deletes (one revision, one undo entry), on the scene root AND inside an entered component (the
 * batch rides the component command route on the context the workspace controller holds; the scene is untouched). Every selected record is deleted
 * with its linked fill; the families that have no stored delete here (texture and bitmap brush strokes, whose dabs would be orphaned) are refused with
 * a notice and nothing is written. The selection is re-read from the native listing first, so the batch names what the owner shows now, against the
 * document the key press found: a document that moved on writes nothing. Layer-row and frame-cell selections keep their own Delete on the scene root (this module
 * leaves the key alone for them; inside a component the key is refused with a notice, never handed to the legacy handler), and it owns no document state and has no fallback writer. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else { root.NemoNativeSelectionDelete = api; if (root.document) api.install(root); }
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  var PINS = ['instanceId', 'documentId', 'lifecycleGeneration', 'contentRevision', 'documentSnapshotId'], MAX_STEPS = 64, MAX_BYTES = 60000, busy = new WeakSet();
  function samePin(a, b) { return !!a && !!b && PINS.every(function (k) { return a[k] === b[k]; }); }
  function notice(root, text) { if (typeof root.showToast === 'function') try { root.showToast(text); } catch (_) { /* presentation only */ } }
  // The selected native strokes the key would delete: null when the key is not Select's to take (another selection kind owns it).
  function picked(root) {
    var st = root.state, I = root.NemoSelectCanvasIntent;
    if (!st || st.tool !== 'select' || st.appMode === 'motion' || !I || !I.objectProject(root) || root._layerActiveExplicit) return null;
    if (root._sel && Array.isArray(root._sel.frames) && root._sel.frames.length) return null;
    var sel = (root.selectedPaths || []).filter(function (p) { return p && p.data && p.data.nativeStableTarget; });
    return sel.length ? sel : null;
  }
  function refusal(sel) {
    var found = [];
    sel.forEach(function (p) {
      var d = p.data, label = d.nativeBrush === 'bitmap' ? 'bitmap brushes' : d.nativeBrush === 'copy' || d.nativeBrush === 'anchor' ? 'texture brushes' : null;
      if (label && found.indexOf(label) < 0) found.push(label);
    });
    return found.length ? found.join(', ') : null;
  }
  // A record's delete steps: the record, and its linked fill (the ribbon's companion the listing named).
  function stepsOf(paths) {
    var steps = [];
    paths.forEach(function (p) {
      var t = p.data.nativeStableTarget, link = p.data.linkedFill && p.data.linkedFill.data && p.data.linkedFill.data.strokeId;
      steps.push({ delete: t });
      if (typeof link === 'string' && link && link !== t.strokeId && !paths.some(function (q) { return q.data.strokeId === link; })) steps.push({ delete: Object.assign({}, t, { strokeId: link }) });
    });
    return steps;
  }
  async function remove(root, sel) {
    var I = root.NemoSelectCanvasIntent, Labs = root.NemoNativeObjectLabs, route = Labs && Labs.desktop(root), bad = refusal(sel);
    if (root.NemoNativeSelectionStyle && typeof root.NemoNativeSelectionStyle.flush === 'function') root.NemoNativeSelectionStyle.flush(root);
    if (bad) { notice(root, 'Deleting ' + bad + ' is not available on native projects yet.'); return 'refused'; }
    if (sel.length > MAX_STEPS / 2) { notice(root, 'Too many items to delete at once (' + MAX_STEPS / 2 + ' at most).'); return 'refused'; }
    if (!route || sel.some(function (p) { return !Number.isInteger(p.data.nativeLayerIndex); })) { notice(root, 'Deleting is not available right now.'); return 'refused'; }
    var before = I.presentedState(root) || await I.presentedSettled(root), layers = before && await I.reread(root, sel, false), again = I.presentedState(root) || await I.presentedSettled(root);
    if (!layers || !layers.complete || !again || !samePin(again.pin, before.pin)) { notice(root, 'The document changed — nothing was deleted.'); return 'stale'; }
    var steps = stepsOf(layers.paths);
    if (steps.length > MAX_STEPS || JSON.stringify(steps).length > MAX_BYTES) { notice(root, 'Too much to delete at once — select fewer items.'); return 'refused'; }
    var result;
    try { result = await route.batchStrokes(layers.layerUid, layers.frame, steps, { select: true }); } catch (_) { result = null; }
    if (!result || result.status !== 'applied') { notice(root, 'The selection could not be deleted.'); return 'failed'; }
    root.selectedPaths = []; root.state.selectedStrokeIndices = [];
    if (root.NemoNativeSelectOverlay && typeof root.NemoNativeSelectOverlay.show === 'function') root.NemoNativeSelectOverlay.show(root);
    if (typeof root.updateUI === 'function') root.updateUI();
    if (root.SMEngineBridge && typeof root.SMEngineBridge.renderNow === 'function') root.SMEngineBridge.renderNow();
    return 'applied';
  }
  // #2256 review: inside an entered component a Delete with nothing picked would reach the legacy timeline Delete, whose rows are the component's
  // placeholder rows (frame cells, layers, anchors of a scene it is not showing). Deleting those is not offered natively there yet, so the key is
  // taken and refused with a notice; it never reaches the legacy handler. Pen keeps its own last-anchor Delete (the pen tool consumes it first).
  function enteredRefusal(root, e) {
    var I = root.NemoSelectCanvasIntent, C = root.NemoNativeComponentContext, st = root.state;
    if (!I || !I.objectProject(root) || !C || typeof C.contextId !== 'function' || C.contextId(root) === 'scene-root') return null;
    if (st && st.appMode === 'motion') return null; // Motion deletes keys, not frames or layers
    if (st && st.tool === 'pen' && !(root._sel && Array.isArray(root._sel.frames) && root._sel.frames.length)) return null;
    e.preventDefault(); e.stopImmediatePropagation();
    notice(root, 'Deleting frames or layers inside a component is not available yet.');
    return Promise.resolve('refused');
  }
  // The keydown (document capture, so the timeline's legacy Delete never sees a key this module takes). Resolves to the outcome, or null when not taken.
  function key(root, e) {
    if (!e || (e.key !== 'Delete' && e.key !== 'Backspace') || e.defaultPrevented) return null;
    if (e.target && (/^(INPUT|TEXTAREA|SELECT)$/.test(e.target.tagName) || e.target.isContentEditable)) return null;
    var sel = picked(root);
    if (!sel) return enteredRefusal(root, e);
    e.preventDefault(); e.stopImmediatePropagation();
    if (busy.has(root)) return Promise.resolve('busy');
    busy.add(root);
    return remove(root, sel).catch(function () { notice(root, 'The selection could not be deleted.'); return 'failed'; }).then(function (outcome) { busy.delete(root); return outcome; });
  }
  function install(root) {
    var d = root.document;
    if (d && typeof d.addEventListener === 'function') d.addEventListener('keydown', function (e) { key(root, e); }, true);
  }
  return Object.freeze({ install: install, key: key, picked: picked, stepsOf: stepsOf });
}));
