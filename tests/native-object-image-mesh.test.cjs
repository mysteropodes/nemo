'use strict';
// L03-T image mesh edits as native commands: the adapter sends ONE closed typed request per edit through the serialized native
// session, reconciles the receipt, and has no JS/Paper writer and no fallback. Literal wire values; the UI rules come from the
// baseline oracle (tests/native-object-rig-fixtures/baseline-rules.json, tools/gen_rules.js at fbe4b8a3).
const test = require('node:test'), assert = require('node:assert/strict'), fs = require('node:fs'), path = require('node:path');
const Mesh = require('../src/js/adapters/native-object-image-mesh.js');
const { createFixture, forbidLegacyWriters, refused, LEGACY } = require('./native-object-rig-support.cjs');

const rules = JSON.parse(fs.readFileSync(path.join(__dirname, 'native-object-rig-fixtures/baseline-rules.json'), 'utf8'));
const OP = 'command.document.object.image.mesh.edit';
const request = (id, revision, action, meshId = 'im_1') => ({ apiVersion: 2, requestId: id, instanceId: 'instance-A', documentId: 'document-A', expectedRevision: revision,
  operation: OP, payload: { command: 'object.image.mesh.edit', meshId, action } });
const OUTLINE = [[0, 0], [1, 0], [1, 1], [0, 1]];

test('setOffset sends one literal command pinned to the revision and reconciles the receipt', async () => {
  const fx = createFixture({});
  const receipt = await Mesh.create(fx.session).setOffset('im_1', 8, 0.1, 0);
  assert.deepEqual(fx.owner.log.received, [request('project-mesh-1', 7, { kind: 'setOffset', index: 8, du: 0.1, dv: 0 })]);
  assert.deepEqual(receipt, { requestId: 'project-mesh-1', contentRevision: 8, applied: true, historyEntriesAdded: 1 });
  assert.equal(Object.isFrozen(receipt), true);
  assert.deepEqual(fx.events.reconciled, [8]);
  assert.equal(fx.state.revision, 8);
});

test('every action sends exactly its closed members', async () => {
  const fx = createFixture({});
  const mesh = Mesh.create(fx.session);
  await mesh.resetOffsets('im_1');
  await mesh.setOutline('im_1', OUTLINE, { cols: 4, rows: 6 });
  await mesh.setOutline('im_1', OUTLINE);
  await mesh.setDensity('im_1', 8, 3);
  assert.deepEqual(fx.owner.log.received.map((r) => r.payload.action), [
    { kind: 'resetOffsets' },
    { kind: 'setOutline', outline: OUTLINE, cols: 4, rows: 6 },
    { kind: 'setOutline', outline: OUTLINE },
    { kind: 'setDensity', cols: 8, rows: 3 },
  ]);
  assert.deepEqual(fx.owner.log.received.map((r) => r.expectedRevision), [7, 8, 9, 10], 'each call reads the pin at execution');
});

test('binding methods send the admitted native action shapes through the existing serialized route', async () => {
  const fx = createFixture({});
  const adapter = Mesh.create(fx.session);
  const target = { contextId: 'scene-root', layerUid: 'L0', frameScope: { kind: 'authored', frame: 13 }, strokeId: 'raster-13' };
  const destination = { ...target, layerUid: 'L2', frameScope: { kind: 'authored', frame: 7 }, strokeId: 'copy-7' };
  const mesh = { outline: OUTLINE, verts: OUTLINE, tris: [0, 1, 2, 0, 2, 3],
    offsets: [[0, 0], [0, 0], [0, 0], [0, 0]], cols: 1, rows: 1 };
  await adapter.attach(target, mesh);
  await adapter.propagate(destination, 'im_1');
  await adapter.duplicate('scene-root', 'L2');
  await adapter.detach(target);
  await adapter.releaseIfUnused('im_1');
  assert.deepEqual(fx.owner.log.received.map((r) => r.payload), [
    { command: 'object.image.mesh.edit', action: { kind: 'attach', target, mesh } },
    { command: 'object.image.mesh.edit', action: { kind: 'propagate', target: destination, meshId: 'im_1' } },
    { command: 'object.image.mesh.edit', action: { kind: 'duplicate', contextId: 'scene-root', layerUid: 'L2' } },
    { command: 'object.image.mesh.edit', action: { kind: 'detach', target } },
    { command: 'object.image.mesh.edit', action: { kind: 'releaseIfUnused', meshId: 'im_1' } },
  ]);
  assert.deepEqual(fx.owner.log.received.map((r) => r.expectedRevision), [7, 8, 9, 10, 11]);
});

test('a falsy density option is left out exactly like the baseline setOutline ignores it', async () => {
  const fx = createFixture({});
  const mesh = Mesh.create(fx.session);
  await mesh.setOutline('im_1', OUTLINE, { cols: 0, rows: null });
  await mesh.setOutline('im_1', OUTLINE, { cols: undefined, rows: 5 });
  assert.deepEqual(fx.owner.log.received.map((r) => r.payload.action), [{ kind: 'setOutline', outline: OUTLINE }, { kind: 'setOutline', outline: OUTLINE, rows: 5 }]);
});

test('two calls queued back to back send revisions 7 then 8 (the pin is read at execution)', async () => {
  const fx = createFixture({});
  const mesh = Mesh.create(fx.session);
  await Promise.all([mesh.setOffset('im_1', 1, 0.5, 0), mesh.setOffset('im_1', 2, 0.25, 0)]);
  assert.deepEqual(fx.owner.log.received.map((r) => [r.requestId, r.expectedRevision]), [['project-mesh-1', 7], ['project-mesh-2', 8]]);
});

test('a queued binding from a stale native view mints no request id and sends no command', async () => {
  const fx = createFixture({});
  const mesh = Mesh.create(fx.session);
  const target = { contextId: 'scene-root', layerUid: 'L0', frameScope: { kind: 'authored', frame: 13 }, strokeId: 'raster-13' };
  const capturedRevision = fx.state.revision;
  const current = (pin) => pin.contentRevision === capturedRevision;
  const earlier = mesh.resetOffsets('im_1');
  const stale = mesh.detach(target, current);
  await earlier;
  const error = await refused(stale);
  assert.equal(error.code, 'stale_revision');
  assert.equal(error.reason, 'mesh_selection_stale');
  assert.deepEqual(fx.events.ids, ['project-mesh-1'], 'the rejected binding does not mint a request id');
  assert.deepEqual(fx.owner.log.received.map((r) => r.payload.action), [{ kind: 'resetOffsets' }], 'the rejected binding sends no native command');
});

test('a no-op edit is an unapplied receipt on the same revision', async () => {
  const fx = createFixture({ applied: () => false });
  const receipt = await Mesh.create(fx.session).resetOffsets('im_1');
  assert.deepEqual(receipt, { requestId: 'project-mesh-1', contentRevision: 7, applied: false, historyEntriesAdded: 0 });
  assert.equal(fx.state.revision, 7);
});

test('a native refusal is passed through typed and never retried', async () => {
  const fx = createFixture({ refuse: () => ({ code: 'unavailable', reason: 'topology_kernel_unavailable' }) });
  const error = await refused(Mesh.create(fx.session).setDensity('im_1', 8, 8));
  assert.equal(error.code, 'unavailable');
  assert.equal(error.reason, 'topology_kernel_unavailable');
  assert.equal(error.requestId, 'project-mesh-1');
  assert.equal(fx.owner.log.received.length, 1);
  assert.deepEqual(fx.events.reconciled, [7], 'the session is resynchronized at the refusal revision');
  const stale = createFixture({ refuse: () => ({ code: 'not_found' }) });
  assert.equal((await refused(Mesh.create(stale.session).setOffset('missing', 0, 0, 0))).code, 'not_found');
});

test('local refusals happen before any request is minted', async () => {
  const fx = createFixture({});
  const mesh = Mesh.create(fx.session);
  const bad = [
    () => mesh.setOffset('im_1', -1, 0, 0), () => mesh.setOffset('im_1', 1.5, 0, 0), () => mesh.setOffset('im_1', 0, NaN, 0), () => mesh.setOffset('im_1', 0, 0, 1e10),
    () => mesh.setOffset('im_1', 0, '0.1', 0), () => mesh.setOffset('', 0, 0, 0), () => mesh.setOffset('m'.repeat(129), 0, 0, 0), () => mesh.resetOffsets(7),
    () => mesh.setDensity('im_1', 0, 4), () => mesh.setDensity('im_1', 4, 65), () => mesh.setDensity('im_1', 4.5, 4), () => mesh.setDensity('im_1', 4),
    () => mesh.setOutline('im_1', OUTLINE.slice(0, 2)), () => mesh.setOutline('im_1', [[0, 0], [5, 0], [0, 1]]), () => mesh.setOutline('im_1', [[-4, -4], [4, -4], [4, 4]]),
    () => mesh.setOutline('im_1', OUTLINE, { cols: 65 }),
  ];
  for (const call of bad) {
    const error = await refused(call());
    assert.equal(error.code, 'invalid_request');
    assert.equal(error.reason, 'local_validation');
  }
  assert.equal(fx.owner.log.received.length, 0);
  assert.equal(fx.events.ids.length, 0, 'no request id was minted');
});

test('a request above the public 4096-byte bound is refused locally and never split', async () => {
  const fx = createFixture({});
  const outline = Array.from({ length: 200 }, (_, i) => [0.123456789 + i / 1000, 0.123456789]);
  const error = await refused(Mesh.create(fx.session).setOutline('im_1', outline));
  assert.equal(error.code, 'invalid_request');
  assert.equal(error.reason, 'request_size');
  assert.equal(fx.owner.log.received.length, 0);
});

test('video binding and animated vertex keys remain unavailable with zero requests', async () => {
  const fx = createFixture({});
  const mesh = Mesh.create(fx.session);
  assert.deepEqual(Mesh.UNAVAILABLE, { attachToVideoLayer: 'mesh_binding_unavailable',
    detachFromVideoLayer: 'mesh_binding_unavailable', setVertexKey: 'mesh_vertex_animation_unavailable' });
  for (const [name, reason] of Object.entries(Mesh.UNAVAILABLE)) {
    const error = await refused(mesh[name]({}));
    assert.equal(error.code, 'unavailable');
    assert.equal(error.reason, reason);
  }
  assert.equal(fx.owner.log.received.length, 0);
});

test('an ambiguous transport failure re-sends the SAME request and the owner executes once', async () => {
  const fx = createFixture({ faults: ['after'] });
  const receipt = await Mesh.create(fx.session).resetOffsets('im_1');
  assert.equal(receipt.applied, true);
  assert.equal(fx.owner.log.received.length, 2);
  assert.deepEqual(fx.owner.log.received[0], fx.owner.log.received[1], 'same id, same body');
  assert.deepEqual(fx.owner.log.executed, ['project-mesh-1']);
});

test('two failures with the document unchanged are "not delivered"; two after delivery are indeterminate and replayable', async () => {
  const lost = createFixture({ faults: ['before', 'before'] });
  const notDelivered = await refused(Mesh.create(lost.session).resetOffsets('im_1'));
  assert.equal(notDelivered.code, 'unavailable');
  assert.equal(notDelivered.reason, 'transport_failure');
  assert.equal(lost.state.revision, 7);
  const after = createFixture({ faults: ['after', 'after'] });
  const unknown = await refused(Mesh.create(after.session).resetOffsets('im_1'));
  assert.equal(unknown.code, 'mesh_command_indeterminate');
  assert.equal(typeof unknown.replay, 'function');
  const replayed = await unknown.replay();
  assert.deepEqual(replayed, { requestId: 'project-mesh-1', contentRevision: 8, applied: true, historyEntriesAdded: 1 });
  assert.deepEqual(after.owner.log.executed, ['project-mesh-1'], 'executed exactly once');
  after.state.document = 'document-B';
  const again = await refused(unknown.replay());
  assert.equal(again.code, 'stale_owner', 'a replay only goes to the same native project');
});

test('receipts are correlated before they are trusted', async () => {
  const wrong = [(r) => { r.contentRevision = 9; }, (r) => { r.result.applied = false; }, (r) => { r.result.extra = 1; }, (r) => { r.instanceId = 'foreign'; }, (r) => { r.documentId = 'foreign'; }];
  for (const mutate of wrong) {
    const fx = createFixture({ respond: (response) => { mutate(response); return response; } });
    const error = await refused(Mesh.create(fx.session).resetOffsets('im_1'));
    assert.equal(error.code, 'uncorrelated');
    assert.ok(fx.events.reconciled.length >= 1, 'the session is resynchronized');
  }
  const foreign = createFixture({ respond: (response) => Object.assign(response, { requestId: 'foreign' }) });
  assert.equal((await refused(Mesh.create(foreign.session).resetOffsets('im_1'))).code, 'uncorrelated');
});

test('absent native support is typed unavailability, never a fallback', async () => {
  const none = createFixture({ route: 'none' });
  const error = await refused(Mesh.create(none.session).resetOffsets('im_1'));
  assert.deepEqual([error.code, error.reason], ['unavailable', 'resource_route_unavailable']);
  const closed = createFixture({});
  closed.state.active = false;
  const inactive = await refused(Mesh.create(closed.session).resetOffsets('im_1'));
  assert.deepEqual([inactive.code, inactive.reason], ['unavailable', 'no_native_project']);
  const font = createFixture({ route: 'font' });
  assert.equal((await Mesh.create(font.session).resetOffsets('im_1')).applied, true, 'the original route name serves the same function');
});

test('the UI rules are the baseline ones (oracle rows of tools/gen_rules.js at fbe4b8a3)', () => {
  assert.equal(rules.baseline, 'fbe4b8a33e6d5a229f014154df2ba21988a9fabc');
  assert.ok(rules.density.length >= 14 && rules.drag.length >= 4);
  for (const row of rules.density) assert.equal(Mesh.densityOf(row.raw), row.value, JSON.stringify(row.raw));
  assert.deepEqual(rules.density.map((r) => r.value), [4, 4, 1, 4, 7, 32, 32, 32, 4, 3, 1, 12, 1, 4]);
  for (const row of rules.drag) assert.deepEqual(Mesh.dragOffset(row.rest, row.uv), [row.du, row.dv]);
  assert.deepEqual(Mesh.dragOffset([0.5, 0.5], [0.6, 0.5]), [0.09999999999999998, 0]);
});

test('no legacy writer is reachable and the presentation bridge keeps its document-capture pointer listeners', async () => {
  const guard = forbidLegacyWriters(LEGACY);
  try {
    const fx = createFixture({});
    const mesh = Mesh.create(fx.session);
    await mesh.setOffset('im_1', 0, 0.1, 0.1);
    await mesh.resetOffsets('im_1');
    for (const name of Object.keys(Mesh.UNAVAILABLE)) await refused(mesh[name]());
  } finally { guard.restore(); }
  const source = fs.readFileSync(path.join(__dirname, '../src/js/adapters/native-object-image-mesh.js'), 'utf8').replace(/\/\*[\s\S]*?\*\//g, '').replace(/\/\/.*$/gm, '');
  for (const name of ['SMImageMesh', 'SMMotion', 'paper', 'userLayers', 'localStorage', 'addEventListener', 'document.']) assert.equal(source.includes(name), false, name);
  const bridge = fs.readFileSync(path.join(__dirname, '../src/js/image-mesh-bridge.js'), 'utf8');
  for (const type of ['pointerdown', 'pointermove', 'pointerup']) {
    assert.match(bridge, new RegExp("document\\.addEventListener\\('" + type + "'[^\\n]*\\{ capture: true \\}"), type + ' stays a document capture listener');
  }
});

test('desktop() is the same thin client per host and absent unless the native project blocks the legacy writers', () => {
  const blocked = { blocksLegacy: () => true }, open = { blocksLegacy: () => false };
  const host = (session) => ({ NemoNativeObjectProject: { desktop: () => session } });
  assert.equal(Mesh.desktop({}), null);
  assert.equal(Mesh.desktop(host(null)), null);
  assert.equal(Mesh.desktop(host(open)), null);
  const h = host(Object.assign({ isActive: () => true, info: () => null, requestId: () => 'x' }, blocked));
  const first = Mesh.desktop(h);
  assert.equal(typeof first.setOffset, 'function');
  assert.equal(Mesh.desktop(h), first);
});
