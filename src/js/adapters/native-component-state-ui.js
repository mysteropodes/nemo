/* C-comp-owned-state: the markers and the camera keys of the component the user is inside, under the native owner.
 * Baseline `enterSymbol` / `exitToScene` (1065e9d8:src/js/app.js) swap `state.markers` and `state.cameraKeys` per symbol; natively the
 * component record owns both (`ComponentRecord.markers`, `.cameraKeys`) and ONE closed edit writes them
 * (`command.document.object.component.state.edit`, one revision and one history entry). This adapter keeps no copy of either: it reads the
 * stored pages (`query.document.object.component.read`, selectors markers / cameraKeys) into a cache keyed by the context and the presented
 * revision, offers that cache as the projection the ordinary marker strip (src/js/markers.js) already reads, and turns the strip's intents
 * (add, rename, move, remove, clear) into the native edit. It is a presentation port: when the project moves on the cache is re-read, a
 * refusal shows a notice and writes nothing, and there is no JS fallback writer.
 * `active()` is the context id while a native session is inside a component, else null (the scene keeps the composition controller).
 * The camera half is the same read/edit pair for the camera tool of an entered component (`readCameraKeys`, `setCameraKey`,
 * `removeCameraKey`, `setCameraEase`, `clearCamera`). */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else { root.NemoNativeComponentStateUI = api; api.create(root); }
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  var SCENE = 'scene-root';
  var installed = new WeakMap();

  function create(root) {
    if (installed.has(root)) return installed.get(root);
    var cache = null, loading = null, failedKey = null, retired = false, pending = false;

    function client() {
      try {
        var api = root.NemoNativeObjectComponent;
        return api && typeof api.desktop === 'function' ? api.desktop(root) : null;
      } catch (_) { return null; }
    }
    function session() {
      try {
        var project = root.NemoNativeObjectProject;
        return project && typeof project.desktop === 'function' ? project.desktop(root) : null;
      } catch (_) { return null; }
    }
    function native() { return !!(root.__TAURI__ && root.__TAURI__.core); }
    function toast(text) { try { if (typeof root.showToast === 'function') root.showToast(text); } catch (_) { /* presentation only */ } }
    /** The entered component's id, else null. */
    function active() {
      var id = native() && root.state ? root.state.activeSymbolId : null;
      return typeof id === 'string' && id.length && id !== SCENE ? id : null;
    }
    function pinKey(context) {
      var s = session(), info = null;
      try { info = s && s.isActive() ? s.info() : null; } catch (_) { info = null; }
      return info && Number.isSafeInteger(info.contentRevision) ? context + '@' + info.documentSnapshotId + ':' + info.contentRevision : null;
    }
    function repaint() {
      try { if (root.SMMarkers && typeof root.SMMarkers.render === 'function') root.SMMarkers.render(); } catch (_) { /* presentation only */ }
    }
    function refresh(context, key) {
      var component = client();
      if (!component || typeof component.listMarkers !== 'function') return Promise.resolve(null);
      loading = component.listMarkers(context).then(function (listed) {
        loading = null;
        if (retired || !listed || listed.ok !== true) { failedKey = key; return null; }
        cache = { context: context, key: key, markers: listed.items.map(function (item, at) {
          return { ordinal: at + 1, frame: item.frame, name: item.name, color: item.color };
        }) };
        failedKey = null;
        repaint();
        return cache;
      }, function () { loading = null; failedKey = key; return null; });
      return loading;
    }
    /** The projection the marker strip reads (`read().composition.markers`, each with its one-based ordinal). */
    function read() {
      var context = active(), key = context && pinKey(context);
      if (!context || !key) return null;
      if ((!cache || cache.key !== key) && !loading && failedKey !== key) refresh(context, key);
      var markers = cache && cache.context === context ? cache.markers : [];
      // The scene's BPM and display facts ride along unchanged: BPM stays a project setting (the baseline never swapped it).
      var scene = root.NemoNativeCompositionController && root.NemoNativeCompositionController.read();
      return Object.freeze({
        info: scene ? scene.info : null,
        context: context,
        composition: Object.freeze(Object.assign({}, scene ? scene.composition : {}, { markers: markers, markerCount: markers.length, markersPresent: true })),
      });
    }
    function send(edit) {
      var context = active(), component = client();
      if (!context || !component || typeof component.editState !== 'function') { toast('Native component markers are unavailable.'); return Promise.resolve({ status: 'unavailable' }); }
      if (pending) { toast('Composition changes are not confirmed. Wait for synchronization before trying again.'); return Promise.resolve({ status: 'unavailable' }); }
      pending = true;
      return Promise.resolve(component.editState(context, edit)).then(function (outcome) {
        if (!outcome || outcome.ok !== true) {
          toast('Component change refused: ' + (outcome && (outcome.message || outcome.reason || outcome.code) || 'unavailable'));
          return { status: 'refused' };
        }
        cache = null; failedKey = null;
        var key = pinKey(context);
        return (key ? refresh(context, key) : Promise.resolve(null)).then(function () { return { status: outcome.result && outcome.result.applied === false ? 'unchanged' : 'applied-presented' }; });
      }, function () { toast('Component change was not confirmed.'); return { status: 'unconfirmed' }; })
        .then(function (done) { pending = false; return done; });
    }
    function byOrdinal(origin, ordinal) {
      var list = origin && origin.composition && origin.composition.markers;
      return Array.isArray(list) ? list.find(function (m) { return m.ordinal === ordinal; }) || null : null;
    }
    /** The composition controller's marker intents (ordinal addressed), as native component edits (frame addressed). */
    function edit(action, origin) {
      if (!action || typeof action !== 'object') return Promise.resolve({ status: 'unavailable' });
      if (action.kind === 'addMarker') return send({ kind: 'addMarker', frame: action.frame, name: action.name || '', color: action.color });
      if (action.kind === 'clearMarkers') return send({ kind: 'clearMarkers' });
      var marker = byOrdinal(origin, action.ordinal);
      if (!marker) { toast('Native component markers are unavailable.'); return Promise.resolve({ status: 'unavailable' }); }
      if (action.kind === 'removeMarker') return send({ kind: 'removeMarker', frame: marker.frame });
      if (action.kind === 'updateMarker') {
        var patch = { kind: 'updateMarker', frame: marker.frame };
        if (action.frame !== undefined && action.frame !== marker.frame) patch.newFrame = action.frame;
        if (action.name !== undefined) patch.name = action.name;
        if (action.color !== undefined) patch.color = action.color;
        return send(patch);
      }
      return Promise.resolve({ status: 'unavailable' });
    }

    // ---- camera (sym.cameraKeys) ---------------------------------------------------------------------------------------------
    function readCameraKeys() {
      var context = active(), component = client();
      if (!context || !component || typeof component.listCameraKeys !== 'function') return Promise.resolve(null);
      return Promise.resolve(component.listCameraKeys(context)).then(function (listed) { return listed && listed.ok === true ? listed.items : null; }, function () { return null; });
    }
    function setCameraKey(frame, key) { return send(Object.assign({ kind: 'setCameraKey', frame: frame }, key || {})); }
    function removeCameraKey(frame) { return send({ kind: 'removeCameraKey', frame: frame }); }
    function setCameraEase(frame, ease) { return send({ kind: 'setCameraEase', frame: frame, ease: ease }); }
    function clearCamera() { return send({ kind: 'clearCamera' }); }

    var api = Object.freeze({
      active: active, read: read, edit: edit,
      readCameraKeys: readCameraKeys, setCameraKey: setCameraKey, removeCameraKey: removeCameraKey, setCameraEase: setCameraEase, clearCamera: clearCamera,
      invalidate: function () { cache = null; failedKey = null; },
      dispose: function () { retired = true; cache = null; installed.delete(root); if (root.NemoNativeComponentState === api) root.NemoNativeComponentState = undefined; },
    });
    installed.set(root, api);
    root.NemoNativeComponentState = api;
    return api;
  }
  return Object.freeze({ create: create });
}));
