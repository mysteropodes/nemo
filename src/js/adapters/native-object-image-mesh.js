/* Image mesh edits as native commands (lane 3, F17/F18): offset, reset, outline and density, each ONE sealed owner transaction
 * (`command.document.object.image.mesh.edit`). A thin client of the native project session: it owns no mesh store, no mirror of
 * `state.imageMeshes` and no document writer; the legacy SMImageMesh writers are never called. What has no native operation
 * (video-layer binding and animated per-vertex keys) is a typed unavailability, never a fallback.
 * Hit-testing and the drag preview stay in image-mesh-bridge.js (document-capture pointer listeners, presentation only). */
(function (root, factory) {
  const commonjs = typeof module === 'object' && module.exports;
  const command = commonjs ? require('./native-object-rig-command.js') : root.NemoNativeObjectRigCommand;
  const contract = commonjs ? require('./native-object-rig-contract.js') : root.NemoNativeObjectRigContract;
  const api = factory(command, contract);
  if (commonjs) module.exports = api;
  else root.NemoNativeObjectImageMesh = api;
}(typeof globalThis === 'object' ? globalThis : this, function (Command, Contract) {
  'use strict';
  const UNAVAILABLE = Object.freeze({
    attachToVideoLayer: 'mesh_binding_unavailable', detachFromVideoLayer: 'mesh_binding_unavailable',
    setVertexKey: 'mesh_vertex_animation_unavailable',
  });
  function unavailable(name) { return () => Promise.reject(Command.fault('unavailable', 'Native image mesh ' + name + ' is unavailable', { reason: UNAVAILABLE[name] })); }

  // image-mesh-bridge.js setDensity at the baseline: the panel fields clamp to 1..32 and fall back to 4.
  function densityOf(raw) { return Math.max(1, Math.min(32, parseInt(raw, 10) || 4)); }
  // image-mesh-bridge.js onMove at the baseline: the offset written for a vertex dragged to `uv` is measured from rest.
  function dragOffset(rest, uv) { return [uv[0] - rest[0], uv[1] - rest[1]]; }

  function create(session) {
    const commands = Command.create(session, 'mesh');
    const guarded = (build, current) => (pin, context) => {
      if (current !== undefined && (typeof current !== 'function' || current(pin, context) !== true)) {
        throw Command.fault('stale_revision', 'Native mesh selection changed', { reason: 'mesh_selection_stale' });
      }
      return build();
    };
    const edit = (meshId, action, current) => commands.execute(Contract.MESH_EDIT,
      guarded(() => ({ command: 'object.image.mesh.edit', meshId, action }), current));
    const binding = (action, current) => commands.execute(Contract.MESH_EDIT,
      guarded(() => ({ command: 'object.image.mesh.edit', action }), current));
    // NS.setOutline ignores a falsy density option (the baseline keeps the existing grid): only truthy values are sent.
    function setOutline(meshId, outline, options, current) {
      const action = { kind: 'setOutline', outline };
      const o = options || {};
      if (o.cols) action.cols = o.cols;
      if (o.rows) action.rows = o.rows;
      return edit(meshId, action, current);
    }
    const api = {
      attach: (target, mesh, current) => binding({ kind: 'attach', target, mesh }, current),
      detach: (target, current) => binding({ kind: 'detach', target }, current),
      propagate: (target, meshId, current) => binding({ kind: 'propagate', target, meshId }, current),
      releaseIfUnused: (meshId, current) => binding({ kind: 'releaseIfUnused', meshId }, current),
      duplicate: (contextId, layerUid, current) => binding({ kind: 'duplicate', contextId, layerUid }, current),
      setOffset: (meshId, index, du, dv, current) => edit(meshId, { kind: 'setOffset', index, du, dv }, current),
      resetOffsets: (meshId, current) => edit(meshId, { kind: 'resetOffsets' }, current),
      setOutline,
      setDensity: (meshId, cols, rows, current) => edit(meshId, { kind: 'setDensity', cols, rows }, current),
    };
    Object.keys(UNAVAILABLE).forEach((name) => { api[name] = unavailable(name); });
    return Object.freeze(api);
  }
  const bindings = new WeakMap();
  function desktop(host) {
    const module = host.NemoNativeObjectProject, session = module && module.desktop(host);
    if (!session || !session.blocksLegacy()) return null;
    if (!bindings.has(host)) bindings.set(host, create(session));
    return bindings.get(host);
  }
  return Object.freeze({ create, desktop, densityOf, dragOffset, UNAVAILABLE });
}));
