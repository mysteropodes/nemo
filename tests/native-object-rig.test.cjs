'use strict';
// L03-T rigs as native commands: every rig action is ONE closed typed request through the serialized native session, the pose
// reset and commit are one reversible transaction each, and the live pose / IK drag is a typed unavailability. No `ld.rig`
// writer, no fallback. Panel rules come from the baseline oracle (tools/gen_rules.js at fbe4b8a3).
const test = require('node:test'), assert = require('node:assert/strict'), fs = require('node:fs'), path = require('node:path');
const Rig = require('../src/js/adapters/native-object-rig.js');
const { createFixture, forbidLegacyWriters, refused, LEGACY } = require('./native-object-rig-support.cjs');

const rules = JSON.parse(fs.readFileSync(path.join(__dirname, 'native-object-rig-fixtures/baseline-rules.json'), 'utf8'));
const L = 'native-layer:8:0';
const EDIT = 'command.document.object.rig.edit', POSE = 'command.document.object.rig.pose';
const SEGS = [{ point: [0, 0], handleIn: [0, 0], handleOut: [10, 0] }, { point: [100, 0], handleIn: [-10, 0], handleOut: [0, 0] }];
const stroke = { strokeId: 'native-stroke:3:0' }, mesh = { meshId: 'im_1' };
const edit = (id, revision, action) => ({ apiVersion: 2, requestId: id, instanceId: 'instance-A', documentId: 'document-A', expectedRevision: revision, operation: EDIT,
  payload: { command: 'object.rig.edit', layerUid: L, action } });
const pose = (id, revision, action) => ({ apiVersion: 2, requestId: id, instanceId: 'instance-A', documentId: 'document-A', expectedRevision: revision, operation: POSE,
  payload: { command: 'object.rig.pose', layerUid: L, action } });

// [call, expected action]: the wire shape of every rig edit, spelled out independently of the contract table.
const CALLS = [
  [(r) => r.drawBone(L, SEGS, false), { kind: 'drawBone', segments: SEGS, closed: false }],
  [(r) => r.drawBone(L, SEGS, true, 'bone1'), { kind: 'drawBone', boneId: 'bone1', segments: SEGS, closed: true }],
  [(r) => r.moveSegment(L, 'bone1', 1, 'handleOut', [4, 5]), { kind: 'moveSegment', boneId: 'bone1', vi: 1, part: 'handleOut', to: [4, 5] }],
  [(r) => r.moveSegment(L, 'bone1', 1, 'point', [4, 5], true), { kind: 'moveSegment', boneId: 'bone1', vi: 1, part: 'point', to: [4, 5], breakTangent: true }],
  [(r) => r.setBoneRadius(L, 'bone1', 120), { kind: 'setBoneRadius', boneId: 'bone1', radius: 120 }],
  [(r) => r.bindStroke(L, stroke.strokeId, 0), { kind: 'bindStroke', strokeId: 'native-stroke:3:0', frame: 0 }],
  [(r) => r.bindStroke(L, stroke.strokeId, 0, { boneIds: ['bone1'], radius: 200, rotate: false, softness: 0.25 }),
    { kind: 'bindStroke', strokeId: 'native-stroke:3:0', frame: 0, boneIds: ['bone1'], radius: 200, rotate: false, softness: 0.25 }],
  [(r) => r.bindMesh(L, 'im_1'), { kind: 'bindMesh', meshId: 'im_1' }],
  [(r) => r.autoAssign(L, 4, { defaultRadius: 200, rotate: true, softness: 0, paths: true, meshes: false }),
    { kind: 'autoAssign', frame: 4, defaultRadius: 200, rotate: true, softness: 0, paths: true, meshes: false }],
  [(r) => r.setWeight(L, stroke, 2, 0, 75), { kind: 'setWeight', target: stroke, vertex: 2, entry: 0, percent: 75 }],
  [(r) => r.addWeight(L, mesh, 4, 'bone2'), { kind: 'addWeight', target: mesh, vertex: 4, boneId: 'bone2' }],
  [(r) => r.removeWeight(L, stroke, 2, 1), { kind: 'removeWeight', target: stroke, vertex: 2, entry: 1 }],
  [(r) => r.resetWeights(L, mesh, 4, { radius: 150, softness: 0.5 }), { kind: 'resetWeights', target: mesh, vertex: 4, radius: 150, softness: 0.5 }],
  [(r) => r.resetWeights(L, stroke, 0), { kind: 'resetWeights', target: stroke, vertex: 0 }],
  [(r) => r.setIk(L, { boneId: 'bone1', vi: 0 }, { boneId: 'bone1', vi: 1 }, { boneId: 'bone2', vi: 1 }, true),
    { kind: 'setIk', root: { boneId: 'bone1', vi: 0 }, joint: { boneId: 'bone1', vi: 1 }, end: { boneId: 'bone2', vi: 1 }, flip: true }],
  [(r) => r.setIk(L, { boneId: 'bone1', vi: 0 }, { boneId: 'bone1', vi: 1 }, { boneId: 'bone2', vi: 1 }),
    { kind: 'setIk', root: { boneId: 'bone1', vi: 0 }, joint: { boneId: 'bone1', vi: 1 }, end: { boneId: 'bone2', vi: 1 } }],
  [(r) => r.removeIk(L, 'bone2:1'), { kind: 'removeIk', endKey: 'bone2:1' }],
  [(r) => r.removeBone(L, 'bone2'), { kind: 'removeBone', boneId: 'bone2' }],
  [(r) => r.removeBind(L, stroke), { kind: 'removeBind', target: stroke }],
];

test('every rig edit sends exactly its closed members, one request each, pinned to the moving revision', async () => {
  const fx = createFixture({});
  const rig = Rig.create(fx.session);
  for (const [call] of CALLS) await call(rig);
  assert.equal(fx.owner.log.received.length, CALLS.length);
  CALLS.forEach(([, action], i) => assert.deepEqual(fx.owner.log.received[i], edit('project-rig-' + (i + 1), 7 + i, action), 'call ' + i));
});

test('reset and commit are one pose command each with their receipts', async () => {
  const fx = createFixture({});
  const rig = Rig.create(fx.session);
  const reset = await rig.resetPose(L);
  const commit = await rig.commitPose(L, 12);
  assert.deepEqual(fx.owner.log.received, [pose('project-rig-1', 7, { kind: 'reset' }), pose('project-rig-2', 8, { kind: 'commit', frame: 12 })]);
  assert.deepEqual(reset, { requestId: 'project-rig-1', contentRevision: 8, applied: true, historyEntriesAdded: 1 });
  assert.deepEqual(commit, { requestId: 'project-rig-2', contentRevision: 9, applied: true, historyEntriesAdded: 1 });
  const rest = createFixture({ applied: () => false });
  assert.deepEqual(await Rig.create(rest.session).resetPose(L), { requestId: 'project-rig-1', contentRevision: 7, applied: false, historyEntriesAdded: 0 });
});

test('the owner\'s typed pose refusals are passed through with their reason and never retried', async () => {
  for (const reason of ['layer_locked', 'no_rigged_stroke', 'frame_out_of_range', 'keyframe_unavailable', 'pose_unavailable', 'frame_not_exposed', 'deform_failed']) {
    const fx = createFixture({ refuse: () => ({ code: 'invalid_request', reason }) });
    const error = await refused(Rig.create(fx.session).commitPose(L, 4));
    assert.deepEqual([error.code, error.reason, error.requestId], ['invalid_request', reason, 'project-rig-1']);
    assert.equal(fx.owner.log.received.length, 1);
  }
  const mesh = createFixture({ refuse: (r) => (r.payload.action.kind === 'bindMesh' ? { code: 'unavailable', reason: 'mesh_bind_unavailable' } : null) });
  const error = await refused(Rig.create(mesh.session).bindMesh(L, 'im_1'));
  assert.deepEqual([error.code, error.reason], ['unavailable', 'mesh_bind_unavailable']);
});

test('the Auto-assign panel arguments are the baseline ones (oracle rows)', () => {
  assert.equal(rules.autoAssign.length, 6);
  for (const row of rules.autoAssign) {
    const form = { radius: row.form.radius, rotate: row.form.rotate, softness: row.form.softness, driveShapes: row.form.shapes, driveMeshes: row.form.meshes };
    // The baseline's positional `radius` argument is the wire option `defaultRadius`; every other field keeps its name.
    const { radius, ...rest } = row.args;
    assert.deepEqual(Rig.autoAssignArgs(form), { defaultRadius: radius, ...rest }, JSON.stringify(row.form));
  }
  assert.deepEqual(Rig.autoAssignArgs(), { defaultRadius: 200, rotate: false, softness: 0, paths: true, meshes: true });
});

test('the IK chain key and the flip coercion are the baseline ones (oracle rows)', async () => {
  assert.equal(rules.ik.length, 21);
  for (const row of rules.ik) {
    assert.equal(Rig.ikKey(row.end), row.key);
    const fx = createFixture({});
    await Rig.create(fx.session).setIk(L, { boneId: 'r', vi: 0 }, { boneId: 'j', vi: 0 }, row.end, row.flip === 'undefined' ? undefined : row.flip);
    const sent = fx.owner.log.received[0].payload.action;
    if (row.flip === 'undefined') assert.equal('flip' in sent, false);
    else assert.equal(sent.flip, row.storedFlip, JSON.stringify(row));
  }
});

test('panel arguments flow into one auto-assign request', async () => {
  const fx = createFixture({});
  const args = Rig.autoAssignArgs({ radius: '150', rotate: true, softness: '40', driveShapes: { checked: true }, driveMeshes: { checked: false } });
  // The arguments go to `autoAssign` as they are: the panel radius must reach the wire as `defaultRadius` (it was dropped).
  await Rig.create(fx.session).autoAssign(L, 0, args);
  assert.deepEqual(fx.owner.log.received[0].payload.action, { kind: 'autoAssign', frame: 0, defaultRadius: 150, rotate: true, softness: 0.4, paths: true, meshes: false });
});

test('local refusals happen before any request is minted', async () => {
  const fx = createFixture({});
  const rig = Rig.create(fx.session);
  const bad = [
    () => rig.drawBone(L, SEGS.slice(0, 1), false), () => rig.drawBone(L, SEGS, 'no'), () => rig.drawBone('', SEGS, false), () => rig.drawBone(L, SEGS, false, ''),
    () => rig.moveSegment(L, 'b', -1, 'point', [0, 0]), () => rig.moveSegment(L, 'b', 0, 'tangent', [0, 0]), () => rig.moveSegment(L, 'b', 0, 'point', [0]),
    () => rig.setBoneRadius(L, 'b', Infinity), () => rig.bindStroke(L, 's', 0, { radius: -1 }), () => rig.bindStroke(L, 's', 4294967296),
    () => rig.autoAssign(L, 0, { defaultRadius: -5 }), () => rig.setWeight(L, { strokeId: 's', meshId: 'm' }, 0, 0, 50), () => rig.setWeight(L, {}, 0, 0, 50),
    () => rig.setWeight(L, stroke, 0, 0, '50'), () => rig.resetWeights(L, stroke, 0, { softness: 1.5 }), () => rig.removeIk(L, ''), () => rig.removeIk(L, 'k'.repeat(161)),
    () => rig.setIk(L, { boneId: 'a' }, { boneId: 'b', vi: 0 }, { boneId: 'c', vi: 0 }), () => rig.removeBind(L, null), () => rig.commitPose(L, -1), () => rig.commitPose(L, 1.5),
    () => rig.commitPose('x'.repeat(129), 0),
  ];
  for (const call of bad) {
    const error = await refused(call());
    assert.deepEqual([error.code, error.reason], ['invalid_request', 'local_validation']);
  }
  assert.equal(fx.owner.log.received.length, 0);
  assert.equal(fx.events.ids.length, 0);
});

test('a bone with too many anchors for the public 4096-byte bound is refused locally and never split', async () => {
  const fx = createFixture({});
  const many = Array.from({ length: 100 }, (_, i) => ({ point: [i, i], handleIn: [0, 0], handleOut: [1, 1] }));
  const error = await refused(Rig.create(fx.session).drawBone(L, many, false));
  assert.deepEqual([error.code, error.reason], ['invalid_request', 'request_size']);
  assert.equal(fx.owner.log.received.length, 0);
});

test('the live pose and IK drag is evaluation, not an operation: typed unavailability, zero requests', async () => {
  const fx = createFixture({});
  const rig = Rig.create(fx.session);
  assert.deepEqual(Rig.UNAVAILABLE, { dragPose: 'rig_pose_drag_unavailable', dragIkEnd: 'rig_pose_drag_unavailable' });
  for (const name of Object.keys(Rig.UNAVAILABLE)) {
    const error = await refused(rig[name](L, 'bone2:1', 10, 10));
    assert.deepEqual([error.code, error.reason], ['unavailable', 'rig_pose_drag_unavailable']);
  }
  assert.equal(fx.owner.log.received.length, 0);
});

test('queued edits keep their order and an ambiguous transport outcome is replayed with the same id', async () => {
  const fx = createFixture({});
  const rig = Rig.create(fx.session);
  await Promise.all([rig.setBoneRadius(L, 'b', 10), rig.setBoneRadius(L, 'b', 20), rig.resetPose(L)]);
  assert.deepEqual(fx.owner.log.received.map((r) => [r.requestId, r.expectedRevision]), [['project-rig-1', 7], ['project-rig-2', 8], ['project-rig-3', 9]]);
  const lost = createFixture({ faults: ['after'] });
  const receipt = await Rig.create(lost.session).commitPose(L, 2);
  assert.equal(receipt.applied, true);
  assert.deepEqual(lost.owner.log.executed, ['project-rig-1']);
  assert.deepEqual(lost.owner.log.received[0], lost.owner.log.received[1]);
  const unknown = createFixture({ faults: ['after', 'after'] });
  const error = await refused(Rig.create(unknown.session).resetPose(L));
  assert.equal(error.code, 'rig_command_indeterminate');
  assert.equal((await error.replay()).contentRevision, 8);
  assert.deepEqual(unknown.owner.log.executed, ['project-rig-1']);
});

test('receipts are correlated and absent native support is typed unavailability', async () => {
  for (const mutate of [(r) => { r.contentRevision = 9; }, (r) => { r.result.applied = false; }, (r) => { r.result.layerUid = 'x'; }, (r) => { r.instanceId = 'foreign'; }]) {
    const fx = createFixture({ respond: (response) => { mutate(response); return response; } });
    assert.equal((await refused(Rig.create(fx.session).resetPose(L))).code, 'uncorrelated');
  }
  const none = createFixture({ route: 'none' });
  assert.deepEqual([(await refused(Rig.create(none.session).resetPose(L))).reason], ['resource_route_unavailable']);
  const closed = createFixture({});
  closed.state.active = false;
  assert.equal((await refused(Rig.create(closed.session).commitPose(L, 0))).reason, 'no_native_project');
});

test('no legacy rig writer is reachable and the rig bridge keeps its capture pointer listeners', async () => {
  const guard = forbidLegacyWriters(LEGACY.concat(['applyRigDeform', 'rigDragIKEnd', 'rigAnchorPoint', 'SMEngineBridge']));
  try {
    const fx = createFixture({});
    const rig = Rig.create(fx.session);
    await rig.resetPose(L);
    await rig.commitPose(L, 0);
    await rig.setIk(L, { boneId: 'a', vi: 0 }, { boneId: 'a', vi: 1 }, { boneId: 'b', vi: 1 }, true);
    await refused(rig.dragIkEnd());
  } finally { guard.restore(); }
  const source = fs.readFileSync(path.join(__dirname, '../src/js/adapters/native-object-rig.js'), 'utf8').replace(/\/\*[\s\S]*?\*\//g, '').replace(/\/\/.*$/gm, '');
  for (const name of ['ld.rig', 'ensureLayerRig', 'rigResetPose', 'rigCommitFrame', 'applyRigDeform', 'paper', 'userLayers', 'localStorage', 'addEventListener', 'n20AllowLegacyWrite']) assert.equal(source.includes(name), false, name);
  const bridge = fs.readFileSync(path.join(__dirname, '../src/js/rig-bridge.js'), 'utf8');
  for (const type of ['pointerdown', 'pointermove', 'pointerup']) assert.match(bridge, new RegExp("addEventListener\\('" + type + "', on[A-Za-z]+, \\{ capture: true \\}"), type);
});

test('desktop() is absent unless the native project blocks the legacy writers, then one client per host', () => {
  const host = (session) => ({ NemoNativeObjectProject: { desktop: () => session } });
  assert.equal(Rig.desktop({}), null);
  assert.equal(Rig.desktop(host({ blocksLegacy: () => false })), null);
  const h = host({ blocksLegacy: () => true, isActive: () => true, info: () => null, requestId: () => 'x' });
  assert.equal(Rig.desktop(h), Rig.desktop(h));
  assert.equal(typeof Rig.desktop(h).commitPose, 'function');
});
