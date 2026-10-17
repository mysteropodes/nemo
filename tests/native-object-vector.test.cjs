'use strict';
// L01-J typed intents of the native vector operations. Real adapter, command and contract modules over a session double
// with the real route semantics (native-object-vector-support.cjs); the stroke dictionaries are the baseline oracle's.
// Nothing here establishes native or installed execution: the owner is a double that follows the documented receipt rules.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const Vector = require('../src/js/adapters/native-object-vector.js');
const Contract = require('../src/js/adapters/native-object-vector-contract.js');
const { createFixture, forbidLegacyWriters, clone } = require('./native-object-vector-support.cjs');

const oracle = JSON.parse(fs.readFileSync(path.join(__dirname, 'native-object-vector-fixtures', 'stroke-oracle.json'), 'utf8'));
const dict = (id) => clone(oracle.cases.find((c) => c.id === id).in);
const adapter = (options) => { const fixture = createFixture(options); return Object.assign(fixture, { vector: Vector.create(fixture.session) }); };
const B = (n) => n / 255;
const PIN7 = { instanceId: 'instance-A', documentId: 'document-A', contentRevision: 7, documentSnapshotId: 'snapshot-7' };
const LINE = [{ point: { x: 10, y: 20 }, handleIn: { x: 0, y: 0 }, handleOut: { x: 0, y: 0 } }, { point: { x: 110, y: 70 }, handleIn: { x: 0, y: 0 }, handleOut: { x: 0, y: 0 } }];
const LINE_RECORD = (strokeId, vector) => ({
  schemaVersion: 1, family: 'editable-vector-path', target: { contextId: 'scene-root', frameScope: { kind: 'authored', frame: 3 }, layerUid: 'L1', strokeId },
  geometry: { kind: 'cubic-path', closed: false, coordinateSpace: 'document-world', handleSpace: 'relative', segments: LINE },
  fill: null,
  vector: Object.assign({ stroke: { kind: 'solid', r: B(17), g: B(34), b: B(51), a: 1 }, hasRealStroke: true, strokeWidth: 4, strokeCap: 'round', strokeJoin: 'round', miterLimit: 10, dashOffset: 0, opacity: 1 }, vector),
});
const seeded = (fixture, strokeId) => { fixture.owner.records.set(strokeId, LINE_RECORD(strokeId)); };

test('create sends one closed request pinned to the presented revision and reports the owner-allocated stroke', async () => {
  const a = adapter();
  const result = await a.vector.createStroke('L1', 3, dict('open-line'));
  assert.deepEqual(a.owner.log.received, [{
    apiVersion: 2, requestId: 'project-vector-1', instanceId: 'instance-A', documentId: 'document-A', expectedRevision: 7,
    operation: 'command.document.object.vector.edit', payload: { command: 'object.vector.create', record: LINE_RECORD('') },
  }]);
  assert.equal(result.status, 'applied');
  assert.equal(result.operation, 'object.vector.create');
  assert.equal(result.strokeId, 'native-stroke:8:0');
  assert.equal(result.revision, 8);
  assert.equal(result.requestId, 'project-vector-1');
  assert.deepEqual(result.origin, PIN7, 'the sealed origin the request was pinned to');
  assert.deepEqual([result.authoredStrokeId, result.unmapped], [null, []]);
  assert.deepEqual(a.owner.records.get('native-stroke:8:0'), LINE_RECORD('native-stroke:8:0'));
  assert.deepEqual(a.events.reconciled, [8], 'the session is reconciled to the receipt revision');
  assert.equal(a.state.revision, 8);
});

test('a stroke with an authored id is created blank (the owner allocates) and the authored id is reported; the third argument overrides opacity', async () => {
  const a = adapter();
  const result = await a.vector.createStroke('L1', 3, dict('stroke-id-kept'), { opacity: 0.25 });
  assert.equal(a.owner.log.received[0].payload.record.target.strokeId, '');
  assert.equal(a.owner.log.received[0].payload.record.vector.opacity, 0.25);
  assert.deepEqual([result.strokeId, result.authoredStrokeId, result.unmapped], ['native-stroke:8:0', 'st_01', ['groupId']]);
});

test('replace is an exact no-op when nothing changed and one revision when something did', async () => {
  const a = adapter();
  seeded(a, 'native-stroke:3:0');
  const same = await a.vector.replaceStroke({ layerUid: 'L1', frame: 3, strokeId: 'native-stroke:3:0' }, dict('open-line'));
  assert.equal(same.status, 'noop'); assert.equal(same.revision, 7); assert.equal(same.receipt.historyEntriesAdded, 0); assert.equal(same.strokeId, 'native-stroke:3:0');
  assert.equal(a.state.revision, 7);
  assert.equal(a.owner.log.received[0].payload.command, 'object.vector.replace');
  const changed = Object.assign(dict('open-line'), { strokeWidth: 9 });
  const second = await a.vector.replaceStroke({ layerUid: 'L1', frame: 3, strokeId: 'native-stroke:3:0' }, changed);
  assert.equal(second.status, 'applied'); assert.equal(second.revision, 8); assert.equal(second.receipt.historyEntriesAdded, 1);
  assert.equal(a.owner.records.get('native-stroke:3:0').vector.strokeWidth, 9);
  assert.deepEqual(a.owner.log.received[1].payload.record, LINE_RECORD('native-stroke:3:0', { strokeWidth: 9 }));
  const missing = await a.vector.replaceStroke({ layerUid: 'L1', frame: 3, strokeId: 'native-stroke:1:0' }, dict('open-line'));
  assert.deepEqual([missing.status, missing.code], ['refused', 'not_found']);
});

test('delete names the stable target; a second delete is the owner not_found, not a silent success', async () => {
  const a = adapter();
  seeded(a, 'native-stroke:3:0');
  const first = await a.vector.deleteStroke({ layerUid: 'L1', frame: 3, strokeId: 'native-stroke:3:0' });
  assert.deepEqual(a.owner.log.received[0].payload, { command: 'object.vector.delete', stableTarget: { contextId: 'scene-root', frameScope: { kind: 'authored', frame: 3 }, layerUid: 'L1', strokeId: 'native-stroke:3:0' } });
  assert.deepEqual([first.status, first.strokeId, first.revision], ['applied', 'native-stroke:3:0', 8]);
  assert.equal(a.owner.records.size, 0);
  const again = await a.vector.deleteStroke({ layerUid: 'L1', frame: 3, strokeId: 'native-stroke:3:0' });
  assert.deepEqual([again.status, again.code], ['refused', 'not_found']);
  assert.equal(a.state.revision, 8);
});

test('parameter edits send the closed payloads; an identity translate is a no-op', async () => {
  const a = adapter();
  seeded(a, 'native-stroke:3:0');
  const at = { layerUid: 'L1', frame: 3, strokeId: 'native-stroke:3:0' };
  const stable = { contextId: 'scene-root', frameScope: { kind: 'authored', frame: 3 }, layerUid: 'L1', strokeId: 'native-stroke:3:0' };
  const set = await a.vector.setParametric(at, { kind: 'rect', tl: 5, bl: 10 });
  const move = await a.vector.translateParametric(at, [12, -4.5]);
  const identity = await a.vector.translateParametric(at, [0, 0]);
  const scale = await a.vector.scaleParametric(at, [2, -1], [10, 20]);
  assert.deepEqual(a.owner.log.received.map((r) => [r.operation, r.expectedRevision, r.payload]), [
    ['command.document.object.parametric.edit', 7, { command: 'object.parametric.set', stableTarget: stable, paramShape: { kind: 'rect', tl: 5, bl: 10 } }],
    ['command.document.object.parametric.edit', 8, { command: 'object.parametric.translate', stableTarget: stable, delta: [12, -4.5] }],
    ['command.document.object.parametric.edit', 9, { command: 'object.parametric.translate', stableTarget: stable, delta: [0, 0] }],
    ['command.document.object.parametric.edit', 9, { command: 'object.parametric.scale', stableTarget: stable, scale: [2, -1], anchor: [10, 20] }],
  ]);
  assert.deepEqual([set.status, move.status, identity.status, scale.status], ['applied', 'applied', 'noop', 'applied']);
  assert.deepEqual([set.revision, move.revision, identity.revision, scale.revision], [8, 9, 9, 10]);
  const unknown = await a.vector.translateParametric({ layerUid: 'L1', frame: 3, strokeId: 'native-stroke:9:9' }, [1, 1]);
  assert.deepEqual([unknown.status, unknown.code], ['refused', 'not_found']);
});

test('the layer stack is replaced as a whole, in evaluation order; the baseline ld.pathFx array is accepted as is', async () => {
  const a = adapter();
  const stack = [{ type: 'zigzag', enabled: true, p1: 12, p2: 4, p3: 0 }, { type: 'roughen', p1: null }];
  const first = await a.vector.setPathEffects('L1', stack);
  assert.deepEqual(a.owner.log.received[0].payload, { command: 'object.path.effects.set', layerUid: 'L1', stack });
  assert.deepEqual([first.status, first.revision], ['applied', 8]);
  const same = await a.vector.setPathEffects('L1', clone(stack));
  assert.deepEqual([same.status, same.revision], ['noop', 8]);
  const reordered = await a.vector.setPathEffects('L1', stack.slice().reverse());
  assert.deepEqual([reordered.status, reordered.revision], ['applied', 9], 'a reorder is a real change');
  const emptied = await a.vector.setPathEffects('L1', []);
  assert.deepEqual([emptied.status, emptied.revision], ['applied', 10], 'an authored empty stack is a value');
  assert.deepEqual(a.owner.layers.get('L1'), []);
  const added = await a.vector.setPathEffects('L1', [a.vector.defaultEffect('roughen')]);
  assert.deepEqual(a.owner.log.received[4].payload.stack, [{ type: 'roughen', enabled: true, p1: 8, p2: 6, p3: 0 }]);
  assert.equal(added.status, 'applied');
  const unknown = await a.vector.setPathEffects('L1', [{ type: 'zigzag' }, { type: 'wave' }]);
  assert.deepEqual([unknown.status, unknown.code, unknown.reason.includes('pathFx[1]')], ['unmapped', 'unsupported_effect', true]);
  assert.equal(a.owner.log.received.length, 5, 'nothing is sent for a stack the owner cannot store');
});

test('without a native session, route or active project every intent is typed unavailability and nothing is sent', async () => {
  const none = Vector.create(null);
  const input = [['createStroke', ['L1', 3, dict('open-line')]], ['deleteStroke', [{ layerUid: 'L1', frame: 3, strokeId: 's' }]], ['setPathEffects', ['L1', []]],
    ['translateParametric', [{ layerUid: 'L1', frame: 3, strokeId: 's' }, [1, 1]]]];
  for (const [method, args] of input) assert.deepEqual(await none[method](...args), { status: 'unavailable', reason: 'no_native_session' }, method);
  const noRoute = adapter({ noRoute: true });
  const inactive = adapter();
  inactive.state.active = false;
  for (const a of [noRoute, inactive]) {
    for (const [method, args] of input) {
      const result = await a.vector[method](...args);
      assert.equal(result.status, 'unavailable', method);
      assert.match(result.reason, /resource_route_unavailable|no_native_project/);
    }
    assert.deepEqual(a.owner.log.received, []);
  }
  assert.equal(Vector.desktop({}), null, 'a window without the native project module has no vector client');
  assert.equal(Vector.desktop({ NemoNativeObjectProject: { desktop: () => null } }), null);
  assert.equal(Vector.desktop({ NemoNativeObjectProject: { desktop: () => ({ blocksLegacy: () => false }) } }), null, 'a host that still runs the legacy writers does not get a native client');
});

test('desktop() binds one client per host window', () => {
  const session = createFixture().session;
  session.blocksLegacy = () => true;
  const host = { NemoNativeObjectProject: { desktop: () => session } };
  const client = Vector.desktop(host);
  assert.ok(client && typeof client.createStroke === 'function');
  assert.equal(Vector.desktop(host), client);
  assert.notEqual(Vector.desktop({ NemoNativeObjectProject: { desktop: () => session } }), client);
  assert.deepEqual(Object.keys(client).sort(), ['createStroke', 'defaultEffect', 'deleteStroke', 'replaceStroke', 'scaleParametric', 'setParametric', 'setPathEffects', 'translateParametric']);
});

test('a stroke the record cannot carry, or the owner could not store, is named and never sent', async () => {
  const a = adapter();
  const refused = await a.vector.createStroke('L1', 3, Object.assign(dict('open-line'), { fillGradient: { kind: 'linear' } }));
  assert.deepEqual([refused.status, refused.code, refused.members], ['unavailable', 'unavailable', ['fillGradient']]);
  const tooFew = await a.vector.createStroke('L1', 3, dict('single-anchor-open'));
  assert.deepEqual([tooFew.status, tooFew.code], ['unmapped', 'unsupported_geometry']);
  const colour = await a.vector.createStroke('L1', 3, Object.assign(dict('open-line'), { strokeColor: 'red' }));
  assert.deepEqual([colour.status, colour.code], ['unmapped', 'unsupported_color']);
  const unnamed = await a.vector.replaceStroke({ layerUid: 'L1', frame: 3, strokeId: '' }, dict('open-line'));
  assert.equal(unnamed.status, 'invalid');
  const reference = await a.vector.deleteStroke({ layerUid: 'L1', frame: -1, strokeId: 'x' });
  assert.equal(reference.status, 'invalid');
  const badDelta = await a.vector.translateParametric({ layerUid: 'L1', frame: 3, strokeId: 'x' }, [1, NaN]);
  assert.equal(badDelta.status, 'invalid');
  assert.deepEqual(a.owner.log.received, []);
  assert.equal(a.events.runs, 2, 'only the requests that reached the validator entered the route');
});

test('a request above the public 4096-byte bound is refused locally, never split or truncated', async () => {
  const a = adapter();
  const big = dict('open-line');
  big.segments = Array.from({ length: 256 }, (_, i) => ({ point: [i, i * 2], handleIn: [0, 0], handleOut: [0, 0] }));
  const result = await a.vector.createStroke('L1', 3, big);
  assert.equal(result.status, 'too_large'); assert.equal(result.limit, 4096); assert.ok(result.bytes > 4096);
  assert.deepEqual(a.owner.log.received, []);
  const fits = dict('open-line');
  fits.segments = Array.from({ length: 24 }, (_, i) => ({ point: [i, i * 2], handleIn: [0, 0], handleOut: [0, 0] }));
  assert.equal((await a.vector.createStroke('L1', 3, fits)).status, 'applied', 'a path within the bound is sent');
  assert.ok(Buffer.byteLength(JSON.stringify(a.owner.log.received[0])) <= 4096);
});

test('the origin the caller read must still be the presented one; a stale origin sends nothing', async () => {
  const a = adapter();
  const stale = Object.assign({}, PIN7, { contentRevision: 6 });
  for (const origin of [stale, Object.assign({}, PIN7, { documentSnapshotId: 'snapshot-6' }), Object.assign({}, PIN7, { documentId: 'document-B' }), Object.assign({}, PIN7, { instanceId: 'other' }), null, {}]) {
    assert.deepEqual(await a.vector.createStroke('L1', 3, dict('open-line'), { origin }), { status: 'stale', reason: 'origin_changed' }, JSON.stringify(origin));
  }
  assert.deepEqual(a.owner.log.received, []);
  const fresh = await a.vector.createStroke('L1', 3, dict('open-line'), { origin: PIN7 });
  assert.equal(fresh.status, 'applied'); assert.deepEqual(fresh.origin, PIN7);
  const aged = await a.vector.createStroke('L1', 3, dict('open-line'), { origin: PIN7 });
  assert.deepEqual(aged, { status: 'stale', reason: 'origin_changed' }, 'the same origin is stale once the revision advanced');
});

test('the owner refusals are typed and reconcile the session: stale revision, locked layer, guarded layer', async () => {
  const stale = adapter({ refuse: () => ({ code: 'stale_revision', message: 'refused' }) });
  const staleResult = await stale.vector.createStroke('L1', 3, dict('open-line'));
  assert.deepEqual([staleResult.status, staleResult.code, staleResult.requestId], ['refused', 'stale_revision', 'project-vector-1']);
  assert.deepEqual(stale.events.reconciled, [7]);
  assert.equal(stale.state.revision, 7);
  const locked = adapter({ refuse: () => ({ code: 'unavailable', message: 'refused', details: { reason: 'layer_locked' } }) });
  assert.deepEqual(await locked.vector.createStroke('L1', 3, dict('open-line')), { status: 'unavailable', reason: 'layer_locked' });
  const wrong = adapter({ refuse: () => ({ code: 'wrong_document', message: 'refused' }) });
  const wrongResult = await wrong.vector.setPathEffects('L1', []);
  assert.deepEqual([wrongResult.status, wrongResult.code], ['refused', 'wrong_document']);
});

test('an ambiguous transport outcome replays the SAME request id and body and the owner executes once', async () => {
  const before = adapter({ faults: ['before'] });
  const one = await before.vector.createStroke('L1', 3, dict('open-line'));
  assert.equal(one.status, 'applied');
  assert.equal(before.owner.log.received.length, 2);
  assert.deepEqual(before.owner.log.received[0], before.owner.log.received[1], 'identical id and body');
  assert.deepEqual(before.owner.log.executed, ['project-vector-1']);
  const after = adapter({ faults: ['after'] });
  const two = await after.vector.createStroke('L1', 3, dict('open-line'));
  assert.equal(two.status, 'applied'); assert.equal(two.strokeId, 'native-stroke:8:0');
  assert.deepEqual(after.owner.log.executed, ['project-vector-1'], 'the stored receipt answered the second send');
  assert.equal(after.state.revision, 8);
  const lost = adapter({ faults: ['before', 'before'] });
  assert.deepEqual(await lost.vector.createStroke('L1', 3, dict('open-line')), { status: 'unavailable', reason: 'transport_failure' });
  assert.deepEqual(lost.owner.log.executed, []);
  const unknown = adapter({ faults: ['after', 'after'] });
  const indeterminate = await unknown.vector.createStroke('L1', 3, dict('open-line'));
  assert.equal(indeterminate.status, 'indeterminate'); assert.equal(indeterminate.requestId, 'project-vector-1');
  assert.equal(typeof indeterminate.replay, 'function');
  assert.deepEqual(unknown.owner.log.executed, ['project-vector-1']);
  const replayed = await indeterminate.replay();
  assert.equal(replayed.status, 'applied'); assert.equal(replayed.strokeId, 'native-stroke:8:0');
  assert.deepEqual(unknown.owner.log.executed, ['project-vector-1'], 'the replay did not execute twice');
});

test('a receipt that does not belong to the request is unconfirmed, never applied', async () => {
  for (const tamper of [
    (response) => { response.result.strokeId = 'other:8:0'; },
    (response) => { response.contentRevision = 9; },
    (response) => { response.documentId = 'document-B'; },
    (response) => { response.result.operation = 'replace'; },
    (response) => { delete response.result.strokeId; },
  ]) {
    const a = adapter({ respond: (response) => { tamper(response); return response; } });
    const result = await a.vector.createStroke('L1', 3, dict('open-line'));
    assert.equal(result.status, 'unconfirmed');
    assert.equal(result.requestId, 'project-vector-1');
  }
});

test('intents are serialized: each one is pinned to the revision its predecessor left behind', async () => {
  const a = adapter();
  const results = await Promise.all([
    a.vector.createStroke('L1', 3, dict('open-line')),
    a.vector.setPathEffects('L1', [{ type: 'zigzag' }]),
    a.vector.createStroke('L1', 3, dict('closed-rect')),
  ]);
  assert.deepEqual(results.map((r) => [r.status, r.revision]), [['applied', 8], ['applied', 9], ['applied', 10]]);
  assert.deepEqual(a.owner.log.received.map((r) => r.expectedRevision), [7, 8, 9]);
  assert.deepEqual(a.owner.log.received.map((r) => r.requestId), ['project-vector-1', 'project-vector-2', 'project-vector-3']);
  assert.deepEqual([...a.owner.records.keys()], ['native-stroke:8:0', 'native-stroke:10:0']);
});

test('no legacy writer is reachable: Paper, the frame store, the shape bridge, the WASM boolean and path-fx stay untouched', async () => {
  const guard = forbidLegacyWriters(['paper', 'Paper', 'project', 'userLayers', 'state', 'SMPathFx', 'shapeGeom', 'commitShape', 'applyParamShapeRect', 'applyParamShapeEllipse',
    'applyParamShapeStar', 'serP', 'desP', 'insertBooleanResult', 'saveActiveLayerFrame', 'SMEngineBridge']);
  try {
    const a = adapter();
    assert.equal((await a.vector.createStroke('L1', 3, dict('vb-basic'))).status, 'applied');
    seeded(a, 'native-stroke:3:0');
    assert.equal((await a.vector.scaleParametric({ layerUid: 'L1', frame: 3, strokeId: 'native-stroke:3:0' }, [2, 2], [0, 0])).status, 'applied');
    assert.equal((await a.vector.setPathEffects('L1', [{ type: 'roughen' }])).status, 'applied');
    assert.equal((await a.vector.deleteStroke({ layerUid: 'L1', frame: 3, strokeId: 'native-stroke:3:0' })).status, 'applied');
  } finally { guard.restore(); }
});

test('read-back validator: the returned record equals what was sent, with the allocated id taken from the receipt', async () => {
  const a = adapter();
  const sent = a.vector.createStroke('L1', 3, dict('param-rect-with-box-angle'));
  const receipt = await sent;
  const request = a.owner.log.received[0].payload.record;
  const stored = a.owner.records.get(receipt.strokeId);
  assert.deepEqual(Vector.verifyReadBack(request, stored, receipt.receipt), { ok: true });
  const moved = clone(stored); moved.geometry.segments[1].point.x += 1e-9;
  assert.deepEqual(Vector.verifyReadBack(request, moved, receipt.receipt), { ok: false, reason: 'mismatch' });
  const lost = clone(stored); delete lost.vectorExt.boxAngle;
  assert.deepEqual(Vector.verifyReadBack(request, lost, receipt.receipt), { ok: false, reason: 'mismatch' });
  const invalid = clone(stored); invalid.vector.hasRealStroke = !invalid.vector.hasRealStroke;
  assert.equal(Vector.verifyReadBack(request, invalid, receipt.receipt).reason, 'invalid_record');
  assert.equal(Vector.verifyReadBack(request, stored, { strokeId: 'native-stroke:9:9' }).reason, 'mismatch', 'a different allocated id is a different record');
  assert.equal(Vector.stableTarget('L1', 3, 'x').frameScope.frame, 3);
  assert.deepEqual(Vector.OPERATIONS.slice(), Contract.OPERATIONS.slice());
});
