/* Native Copy / Cut / Paste / Duplicate of the Select selection (#2372/#2256, X-shape-clipboard-duplicate). Baseline (1065e9d8 tools.js
 * duplicateSelection / copySelection / cutSelection / pasteSelection; timeline.js Cmd+C / Cmd+X / Cmd+V / Cmd+D; select-bridge.js menu rows
 * Copier, Couper, Coller, Dupliquer). On a native object project each gesture is ONE request through the layer lifecycle intent
 * (native-object-layer-lifecycle-intent.js): object.selection.duplicate, object.clipboard.copy / cut / paste. One request is one sealed owner
 * transaction, so one revision and one history entry (copy writes nothing). The baseline rules are decided by the engine owner: the 12 px
 * nudge of a duplicate and of a paste onto the cell it was copied from (a paste elsewhere lands in place), fresh stroke / linked-fill /
 * brush-group / combine-group identities, a pressure ribbon's linked fill and a bitmap brush's raster travelling with their primary, and
 * the held-frame rule (Animation 2D promotes the cell, Motion edits the key it inherits). No Paper / ld.groups writer is reached from here.
 * The detached clip lives here between a copy and a paste (the owner returns it, the owner re-validates it on paste); it is data, never
 * document state. The selection is read again at the presented revision before every request, and a refusal, a stale selection or an
 * absent owner is a notice and a resolved status, never a throw. After a duplicate or a paste the new shapes are selected (baseline).
 * duplicateAt (A-select-alt-drag-duplicate): the same ONE duplicate request with the distance of an Alt-drag in place of the nudge; `guard`
 * ({ pin, cancelled, release }) is the drag's own proof: a document that moved on since the press, or an Escape, sends nothing. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeSelectionClipboard = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  var PINS = ['instanceId', 'documentId', 'lifecycleGeneration', 'contentRevision', 'documentSnapshotId'];
  var jobs = new WeakMap(), intents = new WeakMap(), clips = new WeakMap();
  function notice(host, text) { if (text && typeof host.showToast === 'function') try { host.showToast(text); } catch (_) { /* presentation only */ } }
  // The app's own wording where it has one, English otherwise.
  function said(host, key, fallback) {
    try { var t = host.SM && typeof host.SM.t === 'function' ? host.SM.t(key) : null; return t && t !== key ? t : fallback; } catch (_) { return fallback; }
  }
  function session(host) {
    try { var P = host.NemoNativeObjectProject, s = P && typeof P.desktop === 'function' ? P.desktop(host) : null; return s && s.blocksLegacy() === true ? s : null; } catch (_) { return null; }
  }
  // True while a native object project owns the document (the legacy Paper clone / clipboard writers must not run then).
  function native(host) { return !!session(host); }
  function samePin(a, b) { return !!a && !!b && PINS.every(function (k) { return a[k] === b[k]; }); }
  function livePin(s) { try { return s.info(); } catch (_) { return null; } }
  function same(a, b) { return a.length === b.length && a.every(function (id, at) { return id === b[at]; }); }
  function selected(host) {
    return (host.selectedPaths || []).filter(function (p) { return p && p.data && p.data.nativeStableTarget && Number.isInteger(p.data.nativeLayerIndex) && p.data.strokeId; });
  }
  function idsOf(paths) { return paths.map(function (p) { return p.data.strokeId; }); }
  function modeOf(host) { return host.state && host.state.appMode === 'motion' ? 'motion' : 'animation2d'; }
  function hasClip(host) { var c = clips.get(host); return !!(c && c.clip && Array.isArray(c.clip.records) && c.clip.records.length); }
  // The lifecycle intent over this session's ports (pin / requestId / submit are the session's resource route), bound once per session.
  function intentFor(host, s) {
    if (intents.has(s)) return intents.get(s);
    var Life = host.NemoNativeObjectLayerLifecycleIntent, Component = host.NemoNativeObjectComponentIntent, Contract = host.NemoNativeObjectEditContract;
    if (!Life || !Component || !Contract || typeof Component.portsFromSession !== 'function') return null;
    var base = Component.portsFromSession(s);
    var made = Life.createLayerLifecycleIntent({ pin: base.pin, requestId: base.requestId, submit: base.submit,
      declared: function (operation) { return Contract.admitted(operation) && base.readDeclared() === true; } });
    intents.set(s, made);
    return made;
  }
  // Why the owner did not apply the edit, in words (the owner's classification, never a guess about geometry).
  function refusal(out, what) {
    if (out.code === 'stale_revision' || out.code === 'stale_selection') return 'The document changed — nothing was changed.';
    // The owner refuses a clip whose complete reply would pass 64 KiB as invalid_request, before any cut publishes; a paste over 64 KiB never leaves the SDK.
    if (out.code === 'request_too_large' || (out.code === 'invalid_request' && (what === 'Copy' || what === 'Cut'))) return 'Too much to ' + what.toLowerCase() + ' at once — select fewer shapes.';
    if (out.code === 'unavailable' || out.code === 'not_found') return what + ' is not available for this selection (locked, hidden or unsupported shapes are refused).';
    return what + ' could not be applied' + (out.message ? ': ' + out.message : '.');
  }
  function fail(host, status, text) { notice(host, text); return { failed: true, status: status }; }

  // ---- reads ---------------------------------------------------------------------------------------------------------------------------------
  // The selection read again at the presented revision: fresh paths in paint order and the whole layer's listing (the "before" of a duplicate).
  async function prepare(host, what) {
    var s = session(host), I = host.NemoSelectCanvasIntent, shown = host.selectedPaths || [];
    if (!s || !I) return fail(host, 'unavailable', what + ' is not available right now.');
    if (!shown.length) return { failed: true, status: 'noop' };
    var paths = selected(host);
    if (paths.length !== shown.length) return fail(host, 'refused', 'Select shapes on the canvas first.');
    if (paths.some(function (p) { return p.data.nativeLayerIndex !== paths[0].data.nativeLayerIndex; })) return fail(host, 'refused', 'Select shapes on one layer to ' + what.toLowerCase() + ' them.');
    // An edit still waiting in the style panel's quiet window commits to the cell it was made on before this request reads the shapes.
    if (host.NemoNativeSelectionStyle && typeof host.NemoNativeSelectionStyle.flush === 'function') host.NemoNativeSelectionStyle.flush(host);
    var intent = intentFor(host, s), pin = livePin(s), want = idsOf(paths), layers = null;
    if (!intent) return fail(host, 'unavailable', what + ' is not available right now.');
    try {
      await I.presentedSettled(host);
      layers = await I.layerPaths(host, function (item) { return want.indexOf(item.stableTarget.strokeId) >= 0; }, false, paths[0].data.nativeLayerIndex);
    } catch (_) { layers = null; }
    if (!layers || layers.paths.length !== want.length || !samePin(layers.pin, pin) || !samePin(livePin(s), pin)) return fail(host, 'stale', 'The selection changed — nothing was changed.');
    return { I: I, intent: intent, original: paths, layers: layers, ids: idsOf(layers.paths), context: I.heldId(host), idx: paths[0].data.nativeLayerIndex };
  }
  function target(host, c, offset) {
    var out = { contextId: c.context, layerUid: c.layers.layerUid, frame: c.layers.frame, mode: modeOf(host), strokeIds: c.ids };
    if (offset) out.offset = [offset[0], offset[1]];
    return out;
  }
  async function commit(host, call, what) {
    var out;
    try { out = await call(); } catch (_) { out = { ok: false, code: 'unavailable' }; }
    if (!out || !out.ok) return fail(host, out && out.code || 'unavailable', refusal(out || {}, what));
    return out;
  }
  // The records the layer did not list before the edit (a linked fill and a texture dab follow their owner, they are never their own selection).
  function listedIds(items) { var seen = {}; (items || []).forEach(function (item) { seen[item.stableTarget.strokeId] = true; }); return seen; }
  async function addedPaths(host, I, idx, before) {
    var fresh = null;
    for (var tries = 0; tries < 2 && !fresh; tries++) {
      try {
        fresh = await I.layerPaths(host, function (item) {
          return !before[item.stableTarget.strokeId] && !(item.role === 'fill' && item.owner) && item.brush !== 'copy'; }, false, idx);
      } catch (_) { fresh = null; }
    }
    return fresh;
  }
  function show(host) {
    var O = host.NemoNativeSelectOverlay;
    if (O && typeof O.show === 'function') try { O.show(host); } catch (_) { /* the outline is a nicety; the edit stands */ }
    if (typeof host.updateUI === 'function') host.updateUI();
    if (host.SMEngineBridge && typeof host.SMEngineBridge.renderNow === 'function') host.SMEngineBridge.renderNow();
  }
  // The new shapes become the selection (baseline); a selection the user changed while the request ran stands.
  function select(host, original, fresh) {
    if (original === null || same(idsOf(selected(host)), idsOf(original))) {
      host.selectedPaths = fresh && fresh.paths ? fresh.paths : [];
      if (host.state) host.state.selectedStrokeIndices = [];
    }
    show(host);
  }

  // ---- the four gestures ---------------------------------------------------------------------------------------------------------------------
  // Cmd+D / "Dupliquer": the selection cloned in place with the baseline 12 px nudge, the clones selected. With an `offset` ([dx, dy], record space) the
  // copies land that far away instead (an Alt-drag); `guard` is that drag's proof, checked after the re-read and before the one request.
  async function doDuplicate(host, offset, guard) {
    var c = await prepare(host, 'Duplicate');
    if (c.failed) return c;
    if (guard) {
      if (guard.pin && !samePin(c.layers.pin, guard.pin)) return fail(host, 'stale', 'The document changed during the drag \u2014 nothing was changed.');
      if (guard.cancelled && guard.cancelled()) return { failed: true, status: 'cancelled' };
      if (guard.release) guard.release();
    }
    var before = listedIds(c.layers.items);
    var out = await commit(host, function () { return c.intent.duplicateShapes(target(host, c, offset)); }, 'Duplicate');
    if (out.failed) return out;
    if (!out.applied) return fail(host, 'noop', 'Nothing changed.');
    select(host, c.original, await addedPaths(host, c.I, c.idx, before));
    return { status: 'applied', revision: out.revision };
  }
  async function takeClip(host, c, what) {
    var call = what === 'Cut' ? function () { return c.intent.cutShapes(target(host, c)); } : function () { return c.intent.copyShapes(target(host, c)); };
    var out = await commit(host, call, what);
    if (out.failed) return out;
    if (!out.clipboard || !Array.isArray(out.clipboard.records) || !out.clipboard.records.length) return fail(host, 'noop', '');
    clips.set(host, { clip: out.clipboard, count: c.ids.length });
    host._lastClipKind = 'canvas'; // the keyboard arbiter pastes the clipboard filled last
    return out;
  }
  // Cmd+C / "Copier": the selection becomes the detached clip; the document is untouched.
  async function doCopy(host) {
    var c = await prepare(host, 'Copy');
    if (c.failed) return c;
    var out = await takeClip(host, c, 'Copy');
    if (out.failed) return out;
    notice(host, said(host, 'toastCopied', 'Copied (') + c.ids.length + ')');
    return { status: 'copied', count: c.ids.length };
  }
  // Cmd+X / "Couper": the clip and the deletion of the same snapshot, one revision.
  async function doCut(host) {
    var c = await prepare(host, 'Cut');
    if (c.failed) return c;
    var out = await takeClip(host, c, 'Cut');
    if (out.failed) return out;
    if (!out.applied) return fail(host, 'noop', 'Nothing changed.');
    select(host, c.original, null);
    notice(host, said(host, 'toastCut', 'Cut (') + c.ids.length + ')');
    return { status: 'applied', revision: out.revision, count: c.ids.length };
  }
  // Cmd+V / "Coller": the clip lands on the active layer at the presented frame (offset by the owner when it is the cell it came from).
  async function doPaste(host) {
    var held = clips.get(host);
    if (!hasClip(host)) { notice(host, said(host, 'toastNothingToPaste', 'Nothing to paste')); return { status: 'noop' }; }
    var s = session(host), I = host.NemoSelectCanvasIntent, intent = s && intentFor(host, s);
    if (!s || !I || !intent) return fail(host, 'unavailable', 'Paste is not available right now.');
    if (host.NemoNativeSelectionStyle && typeof host.NemoNativeSelectionStyle.flush === 'function') host.NemoNativeSelectionStyle.flush(host);
    var idx = host.state.activeLayerIdx, layers = null, pin = livePin(s);
    try {
      await I.presentedSettled(host);
      layers = await I.layerPaths(host, function () { return false; }, false, idx);
    } catch (_) { layers = null; }
    if (!layers) return fail(host, 'unavailable', 'Paste is not available for this layer (locked, hidden or missing).');
    if (!samePin(layers.pin, pin) || !samePin(livePin(s), pin)) return fail(host, 'stale', 'The document changed — nothing was pasted.');
    var before = listedIds(layers.items);
    var out = await commit(host, function () {
      return intent.pasteShapes({ contextId: I.heldId(host), layerUid: layers.layerUid, frame: layers.frame, mode: modeOf(host), clip: held.clip }); }, 'Paste');
    if (out.failed) return out;
    if (!out.applied) return fail(host, 'noop', 'Nothing changed.');
    var fresh = await addedPaths(host, I, idx, before);
    select(host, null, fresh);
    notice(host, said(host, 'toastPasted', 'Pasted (') + (fresh ? fresh.paths.length : held.count) + ')');
    return { status: 'applied', revision: out.revision };
  }

  // One edit at a time: a second gesture waits for the first and reads the selection it left.
  function run(host, job) {
    if (!jobs.has(host)) jobs.set(host, Promise.resolve());
    var next = jobs.get(host).then(function () { return job(host); }).catch(function () { notice(host, 'Copy and paste are not available right now.'); return { status: 'unavailable' }; });
    jobs.set(host, next);
    return next;
  }
  function forget(host) { clips.delete(host); }

  return Object.freeze({
    native: native, hasClip: hasClip, forget: forget,
    duplicate: function (host) { return run(host, function (h) { return doDuplicate(h); }); },
    duplicateAt: function (host, offset, guard) { return run(host, function (h) { return doDuplicate(h, offset, guard); }); }, copy: function (host) { return run(host, doCopy); },
    cut: function (host) { return run(host, doCut); }, paste: function (host) { return run(host, doPaste); } });
}));
