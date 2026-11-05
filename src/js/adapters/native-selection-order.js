/* Native Bring to front / Send to back of the Select selection (#2372, A-select-order; baseline 1065e9d8 select-bridge.js onContext, rows
 * 'Premier plan' and 'Arriere-plan'). With items selected on a native object project the two menu rows send ONE `object.vector.batch` of `order`
 * steps (one revision, one history entry, a held frame keyed with a copy, a locked layer or one deformed by a path effect refused with nothing
 * written) instead of the legacy bringToFront / sendToBack over Paper items, which a native project does not have. The steps follow the baseline loops:
 * Premier plan sends one step per selected item in selection order (each goes to the top, so the last selected ends up on top); Arriere-plan
 * sends them in REVERSE selection order (each goes to the bottom, so the first selected ends up lowest) - either way the selected items keep
 * the order they were selected in. The engine moves a ribbon's linked fill and a texture anchor's dabs with their owner. The selection is read
 * again at the presented revision first, so the steps name what the owner shows now against the document the click found; a document that
 * moved on writes nothing. It holds no document state, has no fallback writer and resolves to a status ('applied', 'noop', 'stale', 'refused',
 * 'failed'); a refusal is a notice, never a throw. Baseline runs both rows in any mode: in Motion the key a held frame inherits is restacked in place
 * (Animation 2D keys the held cell with a copy first). */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeSelectionOrder = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  var PINS = ['instanceId', 'documentId', 'lifecycleGeneration', 'contentRevision', 'documentSnapshotId'], MAX_STEPS = 64, MAX_BYTES = 60000, busy = new WeakSet();
  function samePin(a, b) { return !!a && !!b && PINS.every(function (k) { return a[k] === b[k]; }); }
  function notice(host, text) { if (typeof host.showToast === 'function') try { host.showToast(text); } catch (_) { /* presentation only */ } }
  // The selected native items the rows would reorder (those a presented listing named).
  function picked(host) {
    return (host.selectedPaths || []).filter(function (p) { return p && p.data && p.data.nativeStableTarget && Number.isInteger(p.data.nativeLayerIndex) && p.data.strokeId; });
  }
  // The batch steps for `mode` ('front' | 'back') over fresh presented paths in selection order. Pure.
  function stepsOf(paths, mode) {
    var list = mode === 'back' ? paths.slice().reverse() : paths;
    return list.map(function (p) { return { order: { stableTarget: p.data.nativeStableTarget, mode: mode } }; });
  }
  // The committed items as fresh presented paths (the old ones describe the stack from before); the selection stays the same items.
  async function reselect(host, sel) {
    var I = host.NemoSelectCanvasIntent, fresh = null;
    for (var tries = 0; tries < 2 && !fresh; tries++) {
      if (tries) await new Promise(function (done) { (host.setTimeout || setTimeout)(done, 120); });
      try { fresh = await I.reread(host, sel, false); } catch (_) { fresh = null; }
    }
    host.selectedPaths = fresh ? fresh.paths : [];
    if (host.state) host.state.selectedStrokeIndices = [];
  }
  async function send(host, mode, sel) {
    var I = host.NemoSelectCanvasIntent, Labs = host.NemoNativeObjectLabs, route = Labs && typeof Labs.desktop === 'function' ? Labs.desktop(host) : null;
    if (!route || !I) { notice(host, 'Reordering is not available right now.'); return 'refused'; }
    if (sel.length > MAX_STEPS) { notice(host, 'Too many items to reorder at once (' + MAX_STEPS + ' at most).'); return 'refused'; }
    if (host.NemoNativeSelectionStyle && typeof host.NemoNativeSelectionStyle.flush === 'function') host.NemoNativeSelectionStyle.flush(host);
    var before = I.presentedState(host) || await I.presentedSettled(host), layers = before && await I.reread(host, sel, false);
    var again = I.presentedState(host) || await I.presentedSettled(host);
    if (!layers || !layers.complete || !again || !samePin(again.pin, before.pin)) { notice(host, 'The document changed — nothing was reordered.'); return 'stale'; }
    var steps = stepsOf(layers.paths, mode);
    if (JSON.stringify(steps).length > MAX_BYTES) { notice(host, 'Too much to reorder at once — select fewer items.'); return 'refused'; }
    var result;
    try { result = await route.batchStrokes(layers.layerUid, layers.frame, steps, { select: true, inherit: !!host.state && host.state.appMode === 'motion' }); } catch (_) { result = null; } // Motion edits the key a held frame inherits
    if (!result || (result.status !== 'applied' && result.status !== 'noop')) { notice(host, 'The selection could not be reordered (locked, hidden or unsupported items are refused).'); return 'failed'; }
    if (result.status === 'applied') {
      await reselect(host, sel);
      if (host.NemoNativeSelectOverlay && typeof host.NemoNativeSelectOverlay.show === 'function') host.NemoNativeSelectOverlay.show(host);
      if (typeof host.updateUI === 'function') host.updateUI();
      if (host.SMEngineBridge && typeof host.SMEngineBridge.renderNow === 'function') host.SMEngineBridge.renderNow();
    }
    return result.status;
  }
  // Bring the selection to the front ('front') or send it to the back ('back'). Resolves to null when nothing native is selected.
  function order(host, mode) {
    if (mode !== 'front' && mode !== 'back') return Promise.resolve('refused');
    var sel = picked(host);
    if (!sel.length) return Promise.resolve(null);
    if (busy.has(host)) return Promise.resolve('busy');
    busy.add(host);
    return send(host, mode, sel).catch(function () { notice(host, 'The selection could not be reordered.'); return 'failed'; }).then(function (outcome) { busy.delete(host); return outcome; });
  }
  return Object.freeze({ front: function (host) { return order(host, 'front'); }, back: function (host) { return order(host, 'back'); }, order: order, picked: picked, stepsOf: stepsOf });
}));
