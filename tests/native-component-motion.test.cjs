'use strict';
// #2256 B-comp-edit-rest: Motion property reads and edits INSIDE an entered native component (baseline enterSymbol swapped state.layers for the
// component's layers, so the Motion panel, its stopwatch, keys and expressions worked on them). The real property port module and the real Motion
// binding run against scripted owners: a held component sends every read and edit with the held context id, the edit takes the component command
// lane (never the scene mutation lane), and a stale or foreign target is refused with nothing sent.
const test = require('node:test'), assert = require('node:assert/strict');
const Properties = require('../src/js/adapters/native-object-project-properties.js');
const Binding = require('../src/js/adapters/native-motion-property-binding.js');

const PIN = ['instanceId', 'documentId', 'lifecycleGeneration', 'contentRevision', 'documentSnapshotId'];
const same = (a, b) => !!a && !!b && PIN.every((k) => a[k] === b[k]);
function ports(options = {}) {
  const sent = [], lanes = [];
  const shown = {};
  let revision = 4, held = options.scene ? null : { contextId: 'comp-a' }, sceneRoot = !held;
  const info = () => ({ instanceId: 'i', documentId: 'd', lifecycleGeneration: 1, contentRevision: revision, documentSnapshotId: 'native-object:d:' + revision });
  const lane = (name) => (action) => { lanes.push(name); return Promise.resolve().then(() => action({
    readPresentedObjectSession: () => (options.stale ? null : { presentation: {} }),
    dispatch: async (request) => { sent.push(JSON.parse(JSON.stringify(request)));
      if (options.refuse) return { apiVersion: 2, requestId: request.requestId, instanceId: 'i', documentId: 'd', ok: false, contentRevision: revision, error: { code: 'not_found' } };
      if (!options.noop) revision += 1;
      return { apiVersion: 2, requestId: request.requestId, instanceId: 'i', documentId: 'd', ok: true, contentRevision: revision, result: { applied: !options.noop, historyEntriesAdded: options.noop ? 0 : 1 } }; },
    reconcile: async () => info() })); };
  const p = Properties.create({ same, freeze: Object.freeze, requestId: (label) => label + '-' + (sent.length + 1), mutation: (make) => { lanes.push('scene'); const request = make(info()); sent.push(request); return Promise.resolve({ ok: true, request }); }, held: () => held, runComponent: lane('component'),
    dispatchApplication: async (request) => { sent.push(JSON.parse(JSON.stringify(request))); return { apiVersion: 2, requestId: request.requestId, instanceId: 'i', documentId: 'd', contentRevision: revision, ok: true,
      result: { instanceId: 'i', documentId: 'd', atRevision: request.payload.atRevision, documentSnapshotId: request.payload.documentSnapshotId } }; },
    state: () => ({ info: info(), presentation: shown, ticket: 1, active: true, opening: false, pending: 0, synchronizing: false, subscribed: true, presentationCurrent: true, presented: {},
      dispatchAvailable: true, projectionReady: true, subscriptionActive: true, sceneRoot }) });
  return { p, sent, lanes, info, leave() { held = null; sceneRoot = true; }, move(id) { held = { contextId: id }; } };
}
const edit = (contextId) => ({ command: 'object.property.edit', target: { contextId, layerUid: 'in-1' }, property: 'opacity', action: { kind: 'setValue', frame: 2, values: [50] } });
const track = (contextId, property = 'opacity') => ({ command: 'object.property.track.edit', selections: [{ target: { contextId, layerUid: 'in-1' }, property, frame: 3 }], action: { kind: 'preset', preset: 'linear' } });

test('inside a component a property edit is ONE request on the component lane with the held context id, then reconciled', async () => {
  const f = ports();
  assert.equal(f.p.available(), true, 'the property port is available while a component is entered');
  const response = await f.p.editProperty(edit('comp-a'), f.info(), () => true);
  assert.deepEqual(f.lanes, ['component'], 'never the scene mutation lane');
  assert.equal(f.sent.length, 1);
  assert.equal(f.sent[0].operation, 'command.document.object.property.edit');
  assert.equal(f.sent[0].expectedRevision, 4);
  assert.equal(f.sent[0].payload.target.contextId, 'comp-a');
  assert.equal(response.ok, true);
  assert.deepEqual(response.result, { applied: true, historyEntriesAdded: 1 });
});
test('inside a component a key (track) edit takes the same lane with the track operation', async () => {
  const f = ports();
  const response = await f.p.editPropertyTrack(track('comp-a'), f.info(), () => true);
  assert.deepEqual(f.lanes, ['component']);
  assert.equal(f.sent[0].operation, 'command.document.object.property.track.edit');
  assert.equal(f.sent[0].payload.selections[0].target.contextId, 'comp-a');
  assert.equal(response.result.applied, true);
});
test('the scene root keeps the scene mutation lane, unchanged', async () => {
  const f = ports({ scene: true });
  await f.p.editProperty(edit('scene-root'), f.info(), () => true);
  assert.deepEqual(f.lanes, ['scene']);
  assert.equal(f.sent[0].operation, 'command.document.object.property.edit');
});
test('a target outside the held component, a stale intent and a moved pin send nothing', async () => {
  const f = ports();
  await assert.rejects(f.p.editProperty(edit('scene-root'), f.info(), () => true), /not in the entered component/);
  await assert.rejects(f.p.editPropertyTrack(track('comp-b'), f.info(), () => true), /not in the entered component/);
  await assert.rejects(f.p.editProperty(edit('comp-a'), f.info(), () => false), /unavailable/);
  await assert.rejects(f.p.editProperty(edit('comp-a'), Object.assign({}, f.info(), { contentRevision: 3 }), () => true), /stale/);
  const gone = ports({ stale: true });
  await assert.rejects(gone.p.editProperty(edit('comp-a'), gone.info(), () => true), /stale/);
  const other = ports(); const pending = other.p.editProperty(edit('comp-a'), other.info(), () => true); other.move('comp-b');
  await assert.rejects(pending, /stale/);
  assert.deepEqual([f.sent.length, gone.sent.length, other.sent.length], [0, 0, 0]);
});
test('inside a component only a layer Opacity edit is sent: other properties, element holders, expressions and controls are refused with a typed reason (the engine paints no more yet)', async () => {
  const f = ports();
  const refused = (error) => error.code === 'unavailable' && error.reason === 'component_property_not_rendered' && /only Opacity/.test(error.message);
  const position = Object.assign(edit('comp-a'), { property: 'position', action: { kind: 'setValue', frame: 2, values: [4, 5] } });
  const element = Object.assign(edit('comp-a'), { target: { contextId: 'comp-a', layerUid: 'in-1', strokeId: 's1' } });
  const expression = Object.assign(edit('comp-a'), { action: { kind: 'setExpression', code: 'value' } });
  const control = Object.assign(edit('comp-a'), { property: 'xc_1', action: { kind: 'addControl', controlType: 'slider', name: 'Turn' } });
  for (const payload of [position, element, expression, control]) await assert.rejects(f.p.editProperty(payload, f.info(), () => true), refused);
  const onPosition = track('comp-a', 'position');
  await assert.rejects(f.p.editPropertyTrack(onPosition, f.info(), () => true), refused);
  assert.equal(f.sent.length, 0, 'nothing was sent');
  const keyed = Object.assign(edit('comp-a'), { action: { kind: 'setKey', frame: 3, values: [10], curvePoints: [] } });
  assert.equal((await f.p.editProperty(keyed, f.info(), () => true)).ok, true, 'a keyed Opacity edit is sent');
  const ease = { command: 'object.property.track.edit', selections: [{ target: { contextId: 'comp-a', layerUid: 'in-1' }, property: 'opacity', frame: 3 }], action: { kind: 'preset', preset: 'linear' } };
  assert.equal((await f.p.editPropertyTrack(ease, f.info(), () => true)).ok, true, 'an Opacity key preset is sent');
});
test('a refused or unchanged edit is reconciled and reported exactly like the scene (typed refusal, a no-op is not a history entry)', async () => {
  const refused = ports({ refuse: true });
  await assert.rejects(refused.p.editProperty(edit('comp-a'), refused.info(), () => true), (error) => error.code === 'not_found');
  const none = ports({ noop: true });
  const response = await none.p.editProperty(edit('comp-a'), none.info(), () => true);
  assert.deepEqual(response.result, { applied: false, historyEntriesAdded: 0 });
});
test('property reads are also available inside a component and carry the held target', async () => {
  const f = ports();
  const selector = { instanceId: 'i', documentId: 'd', atRevision: 4, documentSnapshotId: 'native-object:d:4', view: { kind: 'rawSample', target: { contextId: 'comp-a', layerUid: 'in-1' }, property: 'opacity', frame: 2 } };
  await f.p.readProperty(selector);
  assert.equal(f.sent[0].operation, 'query.document.object.property.read');
  assert.equal(f.sent[0].payload.view.target.contextId, 'comp-a');
});

// The Motion binding judges an entered component by ITS rows, frame count and context id (the scene's layer table is not the table on screen).
function binding(options = {}) {
  const queries = [];
  const rows = [{ layerUid: 'in-1', name: 'Inner 1' }, { layerUid: 'in-2', name: 'Inner 2' }];
  const info = { instanceId: 'i', documentId: 'd', lifecycleGeneration: 1, contentRevision: 4, documentSnapshotId: 'native-object:d:4', totalFrames: 24, layers: [{ layerUid: 'S1', name: 'Scene 1' }] };
  const presented = Object.assign({ kind: 'native-object', available: true }, info);
  presented.presentation = Object.assign({ owner: 'native', status: 'presented', frame: 5 }, info);
  const session = { isActive: () => true, info: () => info, readPresentedObjectSession: () => presented };
  const root = { document: { createElement: () => ({ style: {}, appendChild() {}, addEventListener() {}, removeEventListener() {}, setAttribute() {} }) },
    state: { appMode: 'motion', currentFrame: 5, activeLayerIdx: options.scene ? 0 : 1, activeSymbolId: options.scene ? null : 'comp-a' }, _layerSel: [options.scene ? 0 : 1] };
  if (!options.scene) root.NemoNativeComponentContext = { presented: () => ({ contextId: 'comp-a', totalFrames: 12, fps: 12 }), layers: () => rows };
  const ui = { currentSession: () => session, projectionReady: () => true };
  const container = { appendChild() {}, removeChild() {}, replaceChildren() {}, children: [], childNodes: [], ownerDocument: root.document };
  const created = Binding.create(root, ui, async (selector) => { queries.push(selector); return {}; }, { container, readContextId: options.scene ? () => 'scene-root' : undefined });
  return { created, queries, root, info };
}
test('inside a component the binding reads the entered rows and the held context id, not the scene layer table', () => {
  const b = binding();
  assert.equal(b.created.readContextId(), 'comp-a');
  assert.deepEqual(b.created.readSelection(), { contextId: 'comp-a', layerUid: 'in-2' }, 'the active entered row');
  assert.deepEqual(b.created.readSelections(), [{ contextId: 'comp-a', layerUid: 'in-2' }]);
  assert.equal(b.created.readContext().frame, 5);
});
test('a playhead beyond the component frame count and a row index beyond the entered rows leave the binding without a scope', () => {
  const far = binding(); far.root.state.currentFrame = 12; far.root.NemoNativeComponentContext.presented = () => ({ contextId: 'comp-a', totalFrames: 12, fps: 12 });
  assert.equal(far.created.readContext(), null, 'frame 12 of a 12-frame component does not exist');
  const bad = binding(); bad.root._layerSel = [2]; bad.root.state.activeLayerIdx = 0;
  assert.equal(bad.created.readSelections(), null, 'row 2 does not exist in the component');
});
test('at the scene root the scene layer table and context are used as before', () => {
  const b = binding({ scene: true });
  assert.equal(b.created.readContextId(), 'scene-root');
  assert.deepEqual(b.created.readSelection(), { contextId: 'scene-root', layerUid: 'S1' }, 'the scene row');
});
