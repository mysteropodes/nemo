'use strict';
// L03-T widgets as native commands: create the widget layer with its controls, write a drag through the control edits, edit the
// presentation settings. One closed typed request per call; the drag mapping is the owner's; no JS value writer, no fallback.
const test = require('node:test'), assert = require('node:assert/strict'), fs = require('node:fs'), path = require('node:path');
const Widget = require('../src/js/adapters/native-object-widget.js');
const { createFixture, forbidLegacyWriters, refused, LEGACY } = require('./native-object-rig-support.cjs');

const rules = JSON.parse(fs.readFileSync(path.join(__dirname, 'native-object-rig-fixtures/baseline-rules.json'), 'utf8'));
const OP = 'command.document.object.widget.';
const UID = 'native-layer:8:0';
const request = (id, revision, suffix, payload) => ({ apiVersion: 2, requestId: id, instanceId: 'instance-A', documentId: 'document-A', expectedRevision: revision,
  operation: OP + suffix, payload: Object.assign({ command: 'object.widget.' + suffix }, payload) });

test('create sends the kind, the name and the position and returns the minted layer', async () => {
  const fx = createFixture({});
  const widget = Widget.create(fx.session, { nextName: () => 'Layer 3' });
  const receipt = await widget.create({ kind: 'joystick', pos: [320, 180] });
  assert.deepEqual(fx.owner.log.received, [request('project-widget-1', 7, 'create', { kind: 'joystick', name: 'Joystick 3', pos: [320, 180] })]);
  assert.deepEqual(receipt, { requestId: 'project-widget-1', contentRevision: 8, applied: true, historyEntriesAdded: 1, layerUid: 'native-layer:8:0' });
  const slider = await widget.create({ kind: 'slider', name: 'Fader', pos: [0, 0] });
  assert.equal(fx.owner.log.received[1].payload.name, 'Fader', 'an authored name is kept verbatim');
  assert.equal(slider.layerUid, 'native-layer:9:0');
});

test('kind and default layer name are the baseline rules (oracle rows of tools/gen_rules.js at fbe4b8a3)', () => {
  assert.equal(rules.widget.length, 30);
  for (const row of rules.widget) {
    assert.equal(Widget.kindOf(row.kindIn === null ? undefined : row.kindIn), row.kind, JSON.stringify(row));
    assert.equal(Widget.layerName(row.kindIn, row.nextName), row.name, JSON.stringify(row));
  }
  assert.equal(Widget.layerName('slider', 'Layer 3'), 'Slider 3');
  assert.equal(Widget.layerName('pad', 'Layer'), 'Joystick');
  assert.equal(Widget.layerName('joystick'), 'Joystick');
});

test('an unknown kind is a joystick and the position falls back to the canvas centre port, otherwise it is required', async () => {
  const fx = createFixture({});
  const widget = Widget.create(fx.session, { center: () => [960, 540] });
  await widget.create({ kind: 'pad' });
  assert.deepEqual([fx.owner.log.received[0].payload.kind, fx.owner.log.received[0].payload.name, fx.owner.log.received[0].payload.pos], ['joystick', 'Joystick', [960, 540]]);
  const bare = createFixture({});
  const error = await refused(Widget.create(bare.session).create({ kind: 'slider' }));
  assert.deepEqual([error.code, error.reason], ['invalid_request', 'pos_required']);
  assert.equal(bare.owner.log.received.length, 0);
});

test('write sends local, world and rest drags as one command each, with the frame as given', async () => {
  const fx = createFixture({});
  const widget = Widget.create(fx.session);
  await widget.writeLocal(UID, 12.5, [40, -25]);
  await widget.writeWorld(UID, 0, [420, 200], [320, 180], 30);
  await widget.writeRest(UID, 3);
  assert.deepEqual(fx.owner.log.received, [
    request('project-widget-1', 7, 'write', { layerUid: UID, frame: 12.5, to: { kind: 'local', point: [40, -25] } }),
    request('project-widget-2', 8, 'write', { layerUid: UID, frame: 0, to: { kind: 'world', point: [420, 200], anchor: [320, 180], angleDegrees: 30 } }),
    request('project-widget-3', 9, 'write', { layerUid: UID, frame: 3, to: { kind: 'rest' } }),
  ]);
});

test('settings sends only the members given and refuses an empty edit locally', async () => {
  const fx = createFixture({});
  const widget = Widget.create(fx.session);
  await widget.settings(UID, { size: [200, 180] });
  await widget.settings(UID, { pos: [10, 20], x: { min: -50, max: 50, rest: 0, label: 'Turn' }, y: { rest: 5 } });
  assert.deepEqual(fx.owner.log.received.map((r) => r.payload.set), [{ size: [200, 180] }, { pos: [10, 20], x: { min: -50, max: 50, rest: 0, label: 'Turn' }, y: { rest: 5 } }]);
  for (const bad of [{}, { x: {} }, { size: [200] }, { size: [200, 'x'] }, { z: 1 }, { x: { min: null } }, { pos: [1e10, 0] }, null]) {
    const error = await refused(widget.settings(UID, bad));
    assert.deepEqual([error.code, error.reason], ['invalid_request', 'local_validation'], JSON.stringify(bad));
  }
  assert.equal(fx.owner.log.received.length, 2);
});

test('local refusals happen before any request is minted', async () => {
  const fx = createFixture({});
  const widget = Widget.create(fx.session, { center: () => [0, 0] });
  const bad = [
    () => widget.writeLocal('', 0, [0, 0]), () => widget.writeLocal(UID, NaN, [0, 0]), () => widget.writeLocal(UID, 0, [0]), () => widget.writeLocal(UID, 0, [1e10, 0]),
    () => widget.writeWorld(UID, 0, [0, 0], [0, 0]), () => widget.write(UID, 0, { kind: 'drag' }), () => widget.write(UID, 0, { kind: 'rest', point: [0, 0] }),
    () => widget.create({ kind: 'joystick', name: 5 }), () => widget.create({ kind: 'joystick', name: null }), () => widget.create({ kind: 'joystick', pos: [0, 'x'] }), () => widget.create({ kind: 'joystick', pos: [0, NaN] }),
  ];
  for (const call of bad) {
    const error = await refused(call());
    assert.equal(error.code, 'invalid_request');
  }
  assert.equal(fx.owner.log.received.length, 0);
  assert.equal(fx.events.ids.length, 0);
});

test('a missing control or layer is the owner\'s typed answer, passed through and not retried', async () => {
  const fx = createFixture({ refuse: (r) => (r.operation === OP + 'write' ? { code: 'not_found' } : r.operation === OP + 'settings' ? { code: 'unavailable', reason: 'widget_axis_unresolved' } : null) });
  const widget = Widget.create(fx.session);
  assert.equal((await refused(widget.writeRest('missing', 0))).code, 'not_found');
  const unresolved = await refused(widget.settings(UID, { size: [20, 20] }));
  assert.deepEqual([unresolved.code, unresolved.reason], ['unavailable', 'widget_axis_unresolved']);
  assert.equal(fx.owner.log.received.length, 2);
});

test('the creation receipt must name the minted layer and the exact revision advance', async () => {
  for (const mutate of [(r) => { delete r.result.layerUid; }, (r) => { r.result.layerUid = ''; }, (r) => { r.contentRevision = 7; }, (r) => { r.result.applied = false; r.result.historyEntriesAdded = 0; }]) {
    const fx = createFixture({ respond: (response) => { mutate(response); return response; } });
    const error = await refused(Widget.create(fx.session, { center: () => [0, 0] }).create({ kind: 'slider' }));
    assert.equal(error.code, 'uncorrelated');
  }
});

test('a lost reply after delivery is replayed with the same id and creates exactly one layer', async () => {
  const fx = createFixture({ faults: ['after'] });
  const receipt = await Widget.create(fx.session, { center: () => [1, 2] }).create({ kind: 'joystick', name: 'J' });
  assert.equal(receipt.layerUid, 'native-layer:8:0');
  assert.deepEqual(fx.owner.log.executed, ['project-widget-1']);
  assert.deepEqual(fx.owner.log.received[0], fx.owner.log.received[1]);
});

test('no legacy writer is reachable and the widget bridge keeps its document-capture pointer listeners', async () => {
  const guard = forbidLegacyWriters(LEGACY.concat(['SMEngineBridge', 'createUserLayer', 'addExprControl']));
  try {
    const fx = createFixture({});
    const widget = Widget.create(fx.session, { center: () => [0, 0] });
    await widget.create({ kind: 'joystick' });
    await widget.writeRest(UID, 0);
    await widget.settings(UID, { pos: [1, 1] });
  } finally { guard.restore(); }
  const source = fs.readFileSync(path.join(__dirname, '../src/js/adapters/native-object-widget.js'), 'utf8').replace(/\/\*[\s\S]*?\*\//g, '').replace(/\/\/.*$/gm, '');
  for (const name of ['SMMotion', 'SMEngineBridge', 'paper', 'userLayers', 'localStorage', 'addEventListener', 'document.', 'motionStatic']) assert.equal(source.includes(name), false, name);
  const bridge = fs.readFileSync(path.join(__dirname, '../src/js/rig-widget.js'), 'utf8');
  for (const type of ['pointerdown', 'pointermove', 'pointerup']) {
    assert.match(bridge, new RegExp("document\\.addEventListener\\('" + type + "'[^\\n]*\\{ capture: true \\}"), type + ' stays a document capture listener');
  }
});

test('desktop() is absent unless the native project blocks the legacy writers, then one client per host', () => {
  const host = (session) => ({ NemoNativeObjectProject: { desktop: () => session } });
  assert.equal(Widget.desktop({}), null);
  assert.equal(Widget.desktop(host({ blocksLegacy: () => false })), null);
  const h = host({ blocksLegacy: () => true, isActive: () => true, info: () => null, requestId: () => 'x' });
  assert.equal(Widget.desktop(h), Widget.desktop(h));
  assert.equal(typeof Widget.desktop(h).writeLocal, 'function');
});
