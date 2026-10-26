/* Rigs as native commands (lane 3, I11/I12/I13): bones, influence radii, stroke binds, auto-assign, per-vertex weights and
 * two-bone IK chains (`command.document.object.rig.edit`, one sealed owner transaction per action) and the reversible reset
 * and commit of the pose (`command.document.object.rig.pose`). A thin client of the native project session: no rig mirror,
 * no `ld.rig` writer; the legacy rigResetPose / rigCommitFrame / rigSetIK stay behind n20AllowLegacyWrite and are never
 * called. The live pose and IK drag is evaluation (a detached deformation of an immutable snapshot), not an operation: it is
 * a typed unavailability here. Hit-testing and the pose drag preview stay in rig-bridge.js (presentation only). */
(function (root, factory) {
  const commonjs = typeof module === 'object' && module.exports;
  const command = commonjs ? require('./native-object-rig-command.js') : root.NemoNativeObjectRigCommand;
  const contract = commonjs ? require('./native-object-rig-contract.js') : root.NemoNativeObjectRigContract;
  const api = factory(command, contract);
  if (commonjs) module.exports = api;
  else root.NemoNativeObjectRig = api;
}(typeof globalThis === 'object' ? globalThis : this, function (Command, Contract) {
  'use strict';
  const UNAVAILABLE = Object.freeze({ dragPose: 'rig_pose_drag_unavailable', dragIkEnd: 'rig_pose_drag_unavailable' });
  function unavailable(name) { return () => Promise.reject(Command.fault('unavailable', 'Native rig ' + name + ' is unavailable', { reason: UNAVAILABLE[name] })); }

  // timeline.js btn-rig-auto-assign at the baseline: the panel fields to the options of `autoAssign`. The baseline's positional
  // `radius` is the wire's `defaultRadius`; the result is passed to `autoAssign(layerUid, frame, options)` as it is.
  function autoAssignArgs(form) {
    const f = form || {};
    return {
      defaultRadius: parseFloat(f.radius) || 200,
      rotate: !!f.rotate,
      softness: (parseFloat(f.softness) || 0) / 100,
      paths: !f.driveShapes || f.driveShapes.checked,
      meshes: !f.driveMeshes || f.driveMeshes.checked,
    };
  }
  // app.js rigSetIK at the baseline: the chain is filed under `boneId:vi` of its end.
  function ikKey(end) { return end.boneId + ':' + end.vi; }

  function create(session) {
    const commands = Command.create(session, 'rig');
    const edit = (layerUid, action) => commands.execute(Contract.RIG_EDIT, () => ({ command: 'object.rig.edit', layerUid, action }));
    const pose = (layerUid, action) => commands.execute(Contract.RIG_POSE, () => ({ command: 'object.rig.pose', layerUid, action }));
    // Members the caller leaves undefined are absent from the wire: absent and explicit values stay distinct.
    const kind = (name, members) => {
      const action = { kind: name };
      Object.keys(members).forEach((key) => { if (members[key] !== undefined) action[key] = members[key]; });
      return action;
    };
    const api = {
      drawBone: (layerUid, segments, closed, boneId) => edit(layerUid, kind('drawBone', { boneId, segments, closed })),
      moveSegment: (layerUid, boneId, vi, part, to, breakTangent) => edit(layerUid, kind('moveSegment', { boneId, vi, part, to, breakTangent })),
      setBoneRadius: (layerUid, boneId, radius) => edit(layerUid, kind('setBoneRadius', { boneId, radius })),
      bindStroke: (layerUid, strokeId, frame, options) => edit(layerUid, kind('bindStroke', Object.assign({ strokeId, frame }, pick(options, ['boneIds', 'radius', 'rotate', 'softness'])))),
      bindMesh: (layerUid, meshId, options) => edit(layerUid, kind('bindMesh', Object.assign({ meshId }, pick(options, ['frame', 'boneIds', 'radius', 'rotate', 'softness'])))),
      autoAssign: (layerUid, frame, options) => edit(layerUid, kind('autoAssign', Object.assign({ frame }, pick(options, ['defaultRadius', 'rotate', 'softness', 'paths', 'meshes'])))),
      setWeight: (layerUid, target, vertex, entry, percent) => edit(layerUid, kind('setWeight', { target, vertex, entry, percent })),
      addWeight: (layerUid, target, vertex, boneId) => edit(layerUid, kind('addWeight', { target, vertex, boneId })),
      removeWeight: (layerUid, target, vertex, entry) => edit(layerUid, kind('removeWeight', { target, vertex, entry })),
      resetWeights: (layerUid, target, vertex, options) => edit(layerUid, kind('resetWeights', Object.assign({ target, vertex }, pick(options, ['radius', 'softness'])))),
      setIk: (layerUid, root, joint, end, flip) => edit(layerUid, kind('setIk', { root, joint, end, flip: flip === undefined ? undefined : !!flip })),
      removeIk: (layerUid, endKey) => edit(layerUid, kind('removeIk', { endKey })),
      removeBone: (layerUid, boneId) => edit(layerUid, kind('removeBone', { boneId })),
      removeBind: (layerUid, target) => edit(layerUid, kind('removeBind', { target })),
      resetPose: (layerUid) => pose(layerUid, { kind: 'reset' }),
      commitPose: (layerUid, frame) => pose(layerUid, { kind: 'commit', frame }),
    };
    Object.keys(UNAVAILABLE).forEach((name) => { api[name] = unavailable(name); });
    return Object.freeze(api);
  }
  function pick(options, names) {
    const out = {};
    const source = options || {};
    names.forEach((name) => { if (source[name] !== undefined) out[name] = source[name]; });
    return out;
  }
  const bindings = new WeakMap();
  function desktop(host) {
    const module = host.NemoNativeObjectProject, session = module && module.desktop(host);
    if (!session || !session.blocksLegacy()) return null;
    if (!bindings.has(host)) bindings.set(host, create(session));
    return bindings.get(host);
  }
  return Object.freeze({ create, desktop, autoAssignArgs, ikKey, UNAVAILABLE });
}));
