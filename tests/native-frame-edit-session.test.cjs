'use strict';
// L05-K end to end up to the host boundary: the REAL native project session (editFrames), the REAL shared application client
// (contract joined) and the REAL intent adapter. Only the Tauri invoke boundary is simulated, as in the repository's other
// session tests ("adapter protocol oracle only"); the Rust owners are exercised by L05-C/D/E/I/J1 and the nemo-mcp contract tests.
const test = require('node:test');
const assert = require('node:assert/strict');
const Project = require('../src/js/adapters/native-object-project.js');
const SDK = require('../src/js/adapters/native-application.js');
const Intent = require('../src/js/adapters/native-frame-edit-intent.js');
const Contract = require('../src/js/adapters/native-frame-edit-contract.js');
const P = 'command.document.object.';
const OPS = Contract.OPERATIONS;

function scenario(o) {
  o = o || {};
  let active = false, revision = 0, total = 21, failure = o.failure || null;
  const uids = ['layer-alpha', 'layer-beta', 'layer-gamma'], calls = [], ui = { frame: 5, active: 1, layerSel: [], cells: [] };
  const info = () => ({ apiVersion: 2, instanceId: 'frame-instance', documentId: 'frame-document', lifecycleGeneration: 1, documentSnapshotId: 'frame-snapshot-' + revision,
    contentRevision: revision, documentDigest: String(revision % 10).repeat(64), byteCount: 1000, canvasW: 320, canvasH: 180, fps: 24, totalFrames: total,
    layers: uids.map((layerUid) => ({ layerUid })), reference: null, undoDepth: revision, redoDepth: 0 });
  const lengthAfter = (op, p) => (op === P + 'frames.insert' || op === P + 'frames.duplicate' ? total + 1 : op === P + 'frames.extend' ? total + p.count
    : op === P + 'frames.remove' ? total - (p.to - p.from + 1) : total);
  async function invoke(command, args) {
    const r = args ? (args.request || JSON.parse(args.requestJson)) : null;
    calls.push({ command, r });
    if (command === 'nemo_native_object_client_status') return Object.assign(info(), { available: active });
    if (command === 'nemo_native_object_bootstrap') { active = true; return Object.assign(info(), { requestId: r.requestId, resourceCount: 0, viewportAvailable: false }); }
    if (command === 'nemo_native_object_reference') return Object.assign(info(), { requestId: r.requestId });
    if (command === 'nemo_native_object_preview') return { status: 'presented', workId: 'frame-work-' + revision, viewGeneration: 1 };
    assert.equal(command, 'nemo_native_dispatch');
    assert.equal(r.instanceId, 'frame-instance'); assert.equal(r.documentId, 'frame-document'); assert.equal(r.expectedRevision, revision);
    const base = { apiVersion: 2, requestId: r.requestId, instanceId: r.instanceId, documentId: r.documentId };
    if (failure === 'stale') return Object.assign(base, { contentRevision: revision, ok: false, error: { code: 'stale_revision', message: 'Stale frame edit' } });
    const applied = failure !== 'noop';
    if (applied) { total = lengthAfter(r.operation, r.payload); revision += 1; }
    return Object.assign(base, { contentRevision: revision, ok: true, result: Object.assign({ applied, historyEntriesAdded: applied ? 1 : 0 }, r.operation === P + 'frames.repeat' ? { cycledLayers: 3, skippedComponentLayers: 0 } : {}) });
  }
  const ports = {
    invoke, frame: () => ui.frame, projectionReady: () => true, publishRevision: () => true,
    mapping: (i) => ({ cssBounds: { x: 0, y: 0, width: 320, height: 180 }, physicalExtent: { width: 320, height: 180 }, compositionExtent: { width: i.canvasW, height: i.canvasH }, reportedDpr: 1 }),
    subscribeRevisions: async () => ({ instanceId: 'frame-instance', documentId: 'frame-document', lifecycleGeneration: 1, contentRevision: revision, subscriptionId: 'subscription-1' }),
    changed: () => {}
  };
  if (!o.noClient) ports.dispatchApplication = (request) => SDK.createNativeApplicationAdapter('ui', { dispatch: (value) => invoke('nemo_native_dispatch', { request: value }) }).dispatch(request);
  const session = Project.create(ports);
  const intent = Intent.create({ currentSession: () => session, application: SDK, frame: () => ui.frame, activeLayer: () => ui.active,
    layerSelection: () => ui.layerSel, cellSelection: () => ui.cells, available: () => true });
  return { session, intent, ui, calls, commands: () => calls.filter((c) => c.command === 'nemo_native_dispatch'), open: async () => { await session.openPath('/frames.json'); await session.presentPreview(ui.frame); } };
}

test('the real session exposes the frame edit only while it is presented, subscribed and has a public client', async () => {
  const h = scenario(); assert.equal(h.session.editFrames, undefined, 'not before Open');
  await h.open(); assert.equal(typeof h.session.editFrames, 'function');
  const bare = scenario({ noClient: true }); await bare.open();
  assert.equal(bare.session.editFrames, undefined, 'no public application client, no private fallback');
  assert.deepEqual({ ...await bare.intent.run({ kind: 'insertFrame' }) }, { status: 'unavailable', code: 'frame_edit_session_unavailable' });
  assert.equal(bare.commands().length, 0);
});

test('F5 through the real chain: pinned revision, shared client, correlated receipt, one new revision', async () => {
  const h = scenario(); await h.open();
  const out = await h.intent.run({ kind: 'insertFrame' });
  assert.deepEqual({ ...out }, { status: 'applied-presented', applied: true, playhead: 6 });
  assert.equal(h.commands().length, 1);
  const { r } = h.commands()[0];
  assert.match(r.requestId, /^project-frame-edit-\d+$/);
  assert.deepEqual({ ...r, requestId: 'x' }, { apiVersion: 2, requestId: 'x', instanceId: 'frame-instance', documentId: 'frame-document', operation: P + 'frames.insert',
    expectedRevision: 0, payload: { command: 'object.frames.insert', cell: 5 } });
  assert.equal(h.session.info().contentRevision, 1); assert.equal(h.session.info().totalFrames, 22);
  assert.equal(h.session.readPresentedObjectSession().presentation.status, 'presented');
  assert.equal(h.calls.at(-1).command, 'nemo_native_object_preview', 'the new revision is presented after the receipt');
});

test('every declared operation reaches the host with exactly the contract payload', async () => {
  const h = scenario(); await h.open();
  for (const op of OPS) {
    const before = h.session.info();
    const out = await h.session.editFrames(op, Contract.example(op), before);
    assert.equal(out.ok, true, op);
    const { r } = h.commands().at(-1);
    assert.equal(r.operation, op); assert.deepEqual(r.payload, Contract.example(op)); assert.equal(r.expectedRevision, before.contentRevision);
    assert.equal(h.session.info().contentRevision, before.contentRevision + 1, op);
  }
  assert.equal(h.commands().length, OPS.length);
});

test('a no-op adds no revision and a native refusal changes nothing and is reported with its code', async () => {
  let h = scenario({ failure: 'noop' }); await h.open();
  assert.deepEqual({ ...await h.intent.run({ kind: 'clearKey' }) }, { status: 'unchanged', applied: false });
  assert.equal(h.session.info().contentRevision, 0);
  h = scenario({ failure: 'stale' }); await h.open();
  assert.deepEqual({ ...await h.intent.run({ kind: 'clearKey' }) }, { status: 'refused', code: 'stale_revision' });
  assert.equal(h.session.info().contentRevision, 0); assert.equal(h.session.readPresentedObjectSession().contentRevision, 0);
});

test('a stale pin or an undeclared operation is refused inside the session before anything is sent', async () => {
  const h = scenario(); await h.open();
  const old = h.session.info();
  await h.session.editFrames(P + 'frames.insert', { command: 'object.frames.insert', cell: 0 }, old);
  assert.equal(h.commands().length, 1);
  await assert.rejects(h.session.editFrames(P + 'frames.insert', { command: 'object.frames.insert', cell: 0 }, old), /intent is stale/);
  const fresh = h.session.info();
  for (const [op, payload] of [[P + 'frames.clipboard.paste', { command: 'object.frames.clipboard.paste' }], [P + 'layer.policy.edit', { command: 'object.layer.policy.edit', layerUid: 'x', flag: 'locked', value: true }],
    [P + 'frames.insert', { command: 'object.frames.insert', cell: -1 }], [P + 'frames.insert', { command: 'object.frames.extend', cell: 1 }]]) {
    await assert.rejects(h.session.editFrames(op, payload, fresh), /intent is invalid/, op);
  }
  assert.equal(h.commands().length, 1, 'nothing past the first edit reached the host');
});

test('selection-driven actions use the real session with document-order layer UIDs', async () => {
  const h = scenario(); await h.open();
  h.ui.cells = [{ layer: 2, frame: 3 }, { layer: 0, frame: 4 }]; h.ui.layerSel = [2, 0];
  assert.equal((await h.intent.run({ kind: 'repeatSelection', times: 3 })).status, 'applied-presented');
  assert.equal((await h.intent.run({ kind: 'duplicateSelection' })).status, 'applied-presented');
  assert.equal((await h.intent.run({ kind: 'removeFrameSpan' })).status, 'applied-presented');
  assert.deepEqual(h.commands().map((c) => c.r.payload), [
    { command: 'object.frames.repeat', fromLayerUid: 'layer-alpha', toLayerUid: 'layer-gamma', fromFrame: 3, toFrame: 4, times: 3 },
    { command: 'object.frames.clipboard.duplicate', layerUid: 'layer-beta', selection: [{ layerUid: 'layer-gamma', frame: 3 }, { layerUid: 'layer-alpha', frame: 4 }] },
    { command: 'object.frames.remove', from: 3, to: 4 }
  ]);
  assert.deepEqual(h.commands().map((c) => c.r.expectedRevision), [0, 1, 2]);
  assert.equal(h.session.info().totalFrames, 19, 'repeat/duplicate keep the length in this oracle; the span removal takes two frames');
});

test('the shared client loads in a bare vm context without the contract and keeps the font-only behaviour', () => {
  // The classic page and bare-context loaders (tests/native-diagnostics-query.test.cjs) run the SDK with whatever globals exist.
  const vm = require('node:vm'), fs = require('node:fs'), path = require('node:path');
  const source = (name) => fs.readFileSync(path.join(__dirname, '..', 'src/js/adapters', name), 'utf8');
  // Objects are built inside the target context: the SDK's plain-object check is per realm.
  const request = (context, operation) => vm.runInContext('(' + JSON.stringify({ apiVersion: 2, requestId: 'r-1', instanceId: 'i-1', documentId: 'd-1', expectedRevision: 1, operation, payload: Contract.example(operation) }) + ')', context);
  const bare = {}; vm.createContext(bare);
  vm.runInContext(source('native-application.js'), bare, { filename: 'native-application.js' });
  assert.equal(typeof bare.NemoNativeApplicationAdapter.validateRequest, 'function', 'loads without throwing');
  for (const op of OPS) assert.throws(() => bare.NemoNativeApplicationAdapter.validateRequest(request(bare, op)), /not admitted/, op);
  const classic = {}; vm.createContext(classic);
  vm.runInContext(source('native-frame-edit-contract.js'), classic, { filename: 'native-frame-edit-contract.js' });
  vm.runInContext(source('native-application.js'), classic, { filename: 'native-application.js' });
  for (const op of OPS) assert.doesNotThrow(() => classic.NemoNativeApplicationAdapter.validateRequest(request(classic, op)), op);
});


test('F7 real Project/SDK chain uses active UID, one ACK/inspect/presentation per checkpoint, including repeated blank', async () => {
  const h = scenario(); await h.open();
  h.ui.layerSel = [2, 0]; h.ui.cells = [{ layer: 0, frame: 3 }];
  for (let revision = 0; revision < 2; revision++) {
    assert.deepEqual({ ...await h.intent.run({ kind: 'insertBlankKey' }) }, { status: 'applied-presented', applied: true });
    const { r } = h.commands().at(-1);
    assert.equal(r.operation, P + 'layer.key.blank'); assert.equal(r.expectedRevision, revision);
    assert.deepEqual(r.payload, { command: 'object.layer.key.blank', layerUid: 'layer-beta', cell: 5 });
    assert.equal(h.session.info().contentRevision, revision + 1);
    assert.equal(h.session.info().undoDepth, revision + 1, 'modeled native metadata, not a real undo execution');
    assert.equal(h.session.readPresentedObjectSession().contentRevision, revision + 1);
    assert.equal(h.calls.at(-1).command, 'nemo_native_object_preview');
  }
  assert.equal(h.commands().length, 2); assert.equal(h.ui.frame, 5); assert.equal(h.session.info().totalFrames, 21);
  assert.deepEqual(h.ui.layerSel, [2, 0]); assert.deepEqual(h.ui.cells, [{ layer: 0, frame: 3 }]);
  const bare = scenario({ noClient: true }); await bare.open();
  assert.equal((await bare.intent.run({ kind: 'insertBlankKey' })).code, 'frame_edit_session_unavailable');
  assert.equal(bare.commands().length, 0);
  const stale = scenario({ failure: 'stale' }); await stale.open();
  assert.deepEqual({ ...await stale.intent.run({ kind: 'insertBlankKey' }) }, { status: 'refused', code: 'stale_revision' });
  assert.equal(stale.session.info().contentRevision, 0);
  const noop = scenario({ failure: 'noop' }); await noop.open();
  assert.equal((await noop.intent.run({ kind: 'insertBlankKey' })).status, 'unconfirmed');
  assert.equal(noop.session.info().contentRevision, 0);
});
