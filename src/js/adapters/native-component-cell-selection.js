/* #2256 A-comp-edit-timeline: the header "Select frames" mode INSIDE an entered native component.
 * Baseline: enterSymbol swaps state.layers / state.totalFrames for the component's, so the frame-header selection (timeline.js) works on the
 * component's own rows and frames unchanged (1065e9d8:src/js/app.js:4130). Native: the scene module (native-frame-cell-selection.js) is mounted on the
 * component grid's caption with a component-scoped info: the session's five owner pins plus {contextId, totalFrames, layers} of the held context.
 * Its presentation proof is the context controller's own projection of that very component (NemoNativeComponentContext.projectionReady), since the
 * viewport presents the component, not the scene lease. Selection only: it writes no document and no history entry; the cells it publishes
 * (_sel.frames) feed the same Delete / Move / Repeat / Copy / Duplicate routes the grid's delegated cell clicks already feed. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeComponentCellSelection = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  var PINS = ['instanceId', 'documentId', 'lifecycleGeneration', 'contentRevision', 'documentSnapshotId'];
  function session(host) { try { var p = host.NemoNativeObjectProject; return p ? p.desktop(host) : null; } catch (_) { return null; } }
  // The component-scoped info: the owner pins of the session and the held component's rows and length (rows carry only what the module reads: layerUid).
  function infoOf(host, st) {
    var s = session(host), pin = s && s.isActive() && s.info();
    if (!pin || !st || !Array.isArray(st.rows)) return null;
    var out = { contextId: st.contextId, totalFrames: st.totalFrames, layers: st.rows.map(function (row) { return { layerUid: row.layerUid }; }) };
    PINS.forEach(function (key) { out[key] = pin[key]; });
    return out;
  }
  function ports(host, st) {
    return {
      currentSession: function () { return session(host); },
      // The very component this info names is still the one the controller holds, at this length, and its rows/revision are the settled ones.
      projectionReady: function (info) {
        var c = host.NemoNativeComponentContext, here = c && c.presented(host);
        return !!here && here.contextId === info.contextId && here.totalFrames === info.totalFrames && c.projectionReady(host, info) === true;
      },
      frameEditPending: function () { var c = host.NemoNativeFrameEditController; return !!(c && c.isPending()); },
    };
  }
  // Mount (or re-mount after a repaint) the selection strip on the grid's header caption. `frameOnly` is the playback / scrub tick the grid moved in place.
  function mount(host, st, caption, frameOnly) {
    var helper = host.NemoNativeFrameCellSelection, info = infoOf(host, st);
    if (!helper || !caption || !info) return false;
    return helper.mount(host, caption, info, Object.assign(ports(host, st), { frameOnly: frameOnly === true })) === true;
  }
  return Object.freeze({ mount: mount, infoOf: infoOf });
}));
