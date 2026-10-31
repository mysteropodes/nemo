/* Native Boolean buttons of the Select selection (#2372, insp-boolean-ops). On a native object project the Unite / Subtract / Intersect /
 * Exclude buttons and the Alt-click "combine" send ONE native request through the existing group intent (native-object-group-intent.js):
 * `object.boolean.apply {mode, layerUid, contextId, frame, strokeIds}` or `object.group.combine {layerUid, contextId, groupId, mode, strokeIds}`.
 * One request is one sealed owner transaction, so it is one revision and one history entry. Baseline (1065e9d8 tools.js booleanOp /
 * group-bridge.js combineSelection): the operands are the selection in selection order and the LAST one is the style source (its stroke and
 * opacity, its linked fill's colour); the result is split into one flat path per island (insertBooleanResult), the first island keeping the
 * style source's id - all decided by the engine owner, so no Paper / WASM boolean path is reached from here. Alt-click makes the selection a
 * non-destructive combine group (a selection that already IS one group's membership only has its mode changed, also by the engine).
 * This module holds no document state: the selection is read again at the presented revision before the request, and a refusal, a stale
 * selection or an absent owner is a notice and a resolved status, never a throw. After the edit the style source (or, for a combine, the
 * whole selection) is selected again. */
(function (root, factory) {
  var commonjs = typeof module === 'object' && module.exports;
  // The intent modules load after this one in the page, so a browser resolves them from the host at call time; node has them here.
  var api = factory(commonjs ? { group: require('./native-object-group-intent.js'), component: require('./native-object-component-intent.js'), contract: require('./native-object-edit-contract.js') } : {});
  if (commonjs) module.exports = api;
  else root.NemoNativeSelectionBoolean = api;
}(typeof globalThis === 'object' ? globalThis : this, function (modules) {
  'use strict';
  var MODES = ['unite', 'subtract', 'intersect', 'exclude'], PINS = ['instanceId', 'documentId', 'lifecycleGeneration', 'contentRevision', 'documentSnapshotId'];
  var VECTOR_ONLY = { raster: 'images', text: 'text' };
  var jobs = new WeakMap(), intents = new WeakMap();
  function lib(host, name, key) { return host[name] || modules[key] || null; }
  function notice(host, text) { if (typeof host.showToast === 'function') try { host.showToast(text); } catch (_) {} }
  // The app's own wording where it has one, English otherwise.
  function said(host, key, fallback) {
    try { var t = host.SM && typeof host.SM.t === 'function' ? host.SM.t(key) : null; return t && t !== key ? t : fallback; } catch (_) { return fallback; }
  }
  function session(host) {
    try { var P = host.NemoNativeObjectProject, s = P && typeof P.desktop === 'function' ? P.desktop(host) : null; return s && s.blocksLegacy() === true ? s : null; } catch (_) { return null; }
  }
  // True while a native object project owns the document (the legacy Paper boolean writers must not run then).
  function native(host) { return !!session(host); }
  function samePin(a, b) { return !!a && !!b && PINS.every(function (k) { return a[k] === b[k]; }); }
  function livePin(s) { try { return s.info(); } catch (_) { return null; } }
  function same(a, b) { return a.length === b.length && a.every(function (id, at) { return id === b[at]; }); }
  function selected(host) {
    return (host.selectedPaths || []).filter(function (p) { return p && p.data && p.data.nativeStableTarget && Number.isInteger(p.data.nativeLayerIndex) && p.data.strokeId; });
  }
  function idsOf(paths) { return paths.map(function (p) { return p.data.strokeId; }); }
  // The group intent over this session's ports: pin / requestId / submit are the session's resource route (one dispatch, then a reconcile),
  // `declared` is the native edit contract's. Bound once per session.
  function intentFor(host, s) {
    if (intents.has(s)) return intents.get(s);
    var Group = lib(host, 'NemoNativeObjectGroupIntent', 'group'), Component = lib(host, 'NemoNativeObjectComponentIntent', 'component');
    var Contract = lib(host, 'NemoNativeObjectEditContract', 'contract');
    if (!Group || !Component || !Contract || typeof Component.portsFromSession !== 'function') return null;
    var base = Component.portsFromSession(s);
    var made = Group.createGroupIntent({
      pin: base.pin, requestId: base.requestId, submit: base.submit,
      declared: function (operation) { return Contract.admitted(operation) && base.readDeclared() === true; } });
    intents.set(s, made);
    return made;
  }
  function groupId() { return 'grp_' + Date.now().toString(36) + '_' + Math.floor(Math.random() * 1e6); }
  // Why the owner did not apply the edit, in words (the owner's classification, never a guess about geometry).
  function refusal(out, combine) {
    var what = combine ? 'Combine' : 'Boolean operation';
    if (out.code === 'stale_revision' || out.code === 'stale_selection') return 'The document changed — nothing was ' + (combine ? 'combined.' : 'changed.');
    if (out.code === 'request_too_large') return 'Too many shapes selected — select fewer shapes.';
    if (out.code === 'unavailable' || out.code === 'not_found') return what + ' is not available for these shapes (curved, unfilled, locked or animated shapes are refused).';
    return what + ' could not be applied' + (out.message ? ': ' + out.message : '.');
  }
  function reselect(host, original, fresh) {
    var O = host.NemoNativeSelectOverlay;
    // The user may have picked something else while the request ran: their selection stands.
    if (same(idsOf(selected(host)), idsOf(original))) {
      host.selectedPaths = fresh && fresh.complete ? fresh.paths : [];
      if (host.state) host.state.selectedStrokeIndices = [];
    }
    if (O && typeof O.show === 'function') O.show(host);
    if (typeof host.updateUI === 'function') host.updateUI();
    if (host.SMEngineBridge && typeof host.SMEngineBridge.renderNow === 'function') host.SMEngineBridge.renderNow();
  }
  async function execute(host, combine, mode) {
    var s = session(host), I = host.NemoSelectCanvasIntent, shown = host.selectedPaths || [];
    if (!s || !I) { notice(host, 'Boolean operations are not available right now.'); return { status: 'unavailable' }; }
    if (MODES.indexOf(mode) < 0) return { status: 'invalid' };
    if (shown.length < 2) {
      notice(host, combine ? said(host, 'toastSelectAtLeast2ShapesToCombine', 'Select at least 2 shapes to combine') : said(host, 'toastSelectAtLeast2Shapes', 'Select at least 2 shapes'));
      return { status: 'noop' };
    }
    var paths = selected(host);
    if (paths.length !== shown.length) { notice(host, 'Select shapes on the canvas first.'); return { status: 'refused' }; }
    var other = paths.find(function (p) { return VECTOR_ONLY[p.data.nativeRole]; });
    if (other) { notice(host, 'Boolean operations need vector shapes — ' + VECTOR_ONLY[other.data.nativeRole] + ' cannot be combined.'); return { status: 'refused' }; }
    if (paths.some(function (p) { return p.data.nativeLayerIndex !== paths[0].data.nativeLayerIndex; })) {
      notice(host, 'Select shapes on one layer to combine them.');
      return { status: 'refused' };
    }
    var intent = intentFor(host, s), gone = 'The selection changed — nothing was ' + (combine ? 'combined.' : 'changed.'), layers = null;
    if (!intent) { notice(host, 'Boolean operations are not available right now.'); return { status: 'unavailable' }; }
    var pin = livePin(s);
    try { layers = await I.reread(host, paths, false); } catch (_) { layers = null; }
    if (!layers || !layers.complete || !samePin(layers.pin, pin) || !samePin(livePin(s), pin)) { notice(host, gone); return { status: 'stale' }; }
    var ids = idsOf(layers.paths), out;
    var input = combine ? { layerUid: layers.layerUid, contextId: 'scene-root', groupId: groupId(), mode: mode, strokeIds: ids }
      : { mode: mode, layerUid: layers.layerUid, contextId: 'scene-root', frame: layers.frame, strokeIds: ids };
    try { out = await (combine ? intent.combine(input) : intent.booleanApply(input)); } catch (_) { out = { ok: false, code: 'unavailable' }; }
    if (!out || !out.ok) { notice(host, refusal(out || {}, combine)); return { status: out && out.code || 'unavailable' }; }
    if (!out.applied) { notice(host, 'Nothing changed.'); return { status: 'noop' }; }
    var fresh = null, again = combine ? layers.paths : layers.paths.slice(-1);
    // Baseline selects the result: the first island keeps the style source's id, so that one is read back (a combine keeps every member).
    for (var tries = 0; tries < 2 && !fresh; tries++) {
      try { fresh = await I.reread(host, again, false); } catch (_) { fresh = null; }
    }
    reselect(host, paths, fresh);
    notice(host, combine ? said(host, 'toastCombined', 'Combined') : said(host, 'toastBooleanOpApplied', 'Boolean operation applied'));
    return { status: 'applied', revision: out.revision };
  }
  // One edit at a time: a second click waits for the first and reads the selection it left.
  function run(host, combine, mode) {
    if (!jobs.has(host)) jobs.set(host, Promise.resolve());
    var next = jobs.get(host).then(function () { return execute(host, combine, mode); }).catch(function () { notice(host, 'Boolean operations are not available right now.'); return { status: 'unavailable' }; });
    jobs.set(host, next);
    return next;
  }
  // Unite / Subtract / Intersect / Exclude of the selection (the destructive baseline booleanOp).
  function apply(host, mode) { return run(host, false, mode); }
  // Alt-click: the selection as a non-destructive combine group (the baseline combineSelection).
  function combine(host, mode) { return run(host, true, mode); }
  return Object.freeze({ native: native, apply: apply, combine: combine, MODES: MODES.slice() });
}));
