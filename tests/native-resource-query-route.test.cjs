'use strict';
// #2256 review of #2668/#2669: the project-global READERS (palette, guides, shadow, perspective, symmetry, comments, composition, workspace,
// custom brushes) load while a component is entered (the baseline showed them), and every WRITE stays refused there. The route is the
// session's `runResourceQuery`: it runs inside a component, but only `query.*` operations reach the owner.
const test = require('node:test'), assert = require('node:assert/strict');
const Project = require('../src/js/adapters/native-object-project.js');

function host() {
  const info = () => ({ apiVersion: 2, instanceId: 'instance', documentId: 'document', lifecycleGeneration: 1, documentSnapshotId: 'snapshot-0', contentRevision: 0,
    canvasW: 320, canvasH: 180, fps: 24, totalFrames: 21, documentDigest: 'a'.repeat(64), byteCount: 10000, layers: [{ layerUid: 'drawing' }], reference: null, undoDepth: 0, redoDepth: 0 });
  let installed = false;
  async function invoke(command, args) {
    const r = args ? JSON.parse(args.requestJson) : null;
    if (command === 'nemo_native_object_client_status') return Object.assign(info(), { available: installed });
    if (command === 'nemo_native_object_bootstrap') { installed = true; return Object.assign(info(), { requestId: r.requestId, resourceCount: 0, viewportAvailable: false }); }
    if (command === 'nemo_native_object_reference') return Object.assign(info(), { requestId: r.requestId });
    if (command === 'nemo_native_object_preview') return { status: 'presented', workId: 'w', viewGeneration: 1 };
    throw new Error('Unexpected command: ' + command);
  }
  return { invoke };
}
async function session(context) {
  const sent = [], h = host();
  const s = Project.create({ invoke: h.invoke, frame: () => 0, context, projectionReady: () => true,
    dispatchApplication: async (request) => { sent.push(request.operation); return { ok: true, requestId: request.requestId }; },
    mapping: (pin) => ({ cssBounds: { x: 0, y: 0, width: 320, height: 180 }, physicalExtent: { width: 320, height: 180 }, compositionExtent: { width: pin.canvasW, height: pin.canvasH }, reportedDpr: 1 }) });
  await s.openPath('/project.json');
  return { s, sent };
}
const A = { contextId: 'sym_a', totalFrames: 12, fps: 12 };

test('inside a component the query route reads (project-global) and refuses every write; the command route is unchanged', async () => {
  let entered = null;
  const { s, sent } = await session(() => entered);
  await s.presentPreview(0);
  entered = A; await s.presentPreview(0);
  const read = await s.runResourceQuery(async (ctx) => {
    const lease = ctx.readPresentedObjectSession();
    const reply = await ctx.dispatch({ requestId: 'r1', operation: 'query.document.object.project.palette.read' });
    return { lease, reply };
  });
  assert.equal(read.lease.presentation.contextId, 'sym_a', 'the lease is the ENTERED presentation');
  assert.equal(read.reply.ok, true);
  assert.deepEqual(sent, ['query.document.object.project.palette.read'], 'the query reached the owner');
  await assert.rejects(() => s.runResourceQuery((ctx) => ctx.dispatch({ requestId: 'w1', operation: 'command.document.object.project.palette.edit' })), /queries cannot write/);
  await assert.rejects(() => s.runResourceQuery((ctx) => ctx.reconcile(1)), /cannot reconcile a write/);
  assert.deepEqual(sent, ['query.document.object.project.palette.read'], 'no write reached the owner');
  await assert.rejects(() => s.runResourceCommand(async () => 1), /not available inside a component/, 'the command route still refuses');
  await assert.rejects(() => s.runFontResource(async () => 1), /not available inside a component/);
});

test('a query queued in the scene does not run after a component is entered, and the scene keeps working', async () => {
  let entered = null;
  const { s } = await session(() => entered);
  await s.presentPreview(0);
  const scene = await s.runResourceQuery(async (ctx) => ctx.readPresentedObjectSession());
  assert.equal(scene.presentation.contextId, 'scene-root');
  let release;
  const blocker = s.runResourceQuery(() => new Promise((resolve) => { release = resolve; }));
  let ran = false;
  const queued = s.runResourceQuery(async () => { ran = true; }).then(() => null, (error) => error);
  for (let i = 0; i < 10 && !release; i++) await new Promise((done) => setImmediate(done));
  entered = A; release(); await blocker;
  assert.match(String((await queued).message), /not available inside a component/);
  assert.equal(ran, false);
  await assert.rejects(() => s.runResourceQuery(async () => 1), /not available inside a component/, 'submitted before the component presentation exists: refused, never a scene lease');
  await s.presentPreview(0);
  assert.equal((await s.runResourceQuery(async (ctx) => ctx.readPresentedObjectSession())).presentation.contextId, 'sym_a');
});

// Each reader's refresh goes through the query route (the command route is scene-only and would refuse inside a component).
const READERS = ['palette', 'guide', 'shadow', 'perspective', 'symmetry', 'comments', 'composition', 'workspace', 'custom-brush'];
for (const name of READERS) {
  test('the ' + name + ' reader loads through the query route while a component is entered', async () => {
    const Model = require('../src/js/adapters/native-' + name + '-desktop.js');
    const info = { instanceId: 'i', documentId: 'd', lifecycleGeneration: 1, contentRevision: 0, documentSnapshotId: 's', canvasW: 320, canvasH: 180 };
    const shown = () => ({ ...info, presentation: { ...info, owner: 'native', status: 'presented', contextId: 'sym_a' } });
    const calls = { query: 0, command: 0, dispatched: [] };
    const sentinel = new Error('ROUTE_REACHED');
    const stub = { info: () => info, isActive: () => true, requestId: () => 'r', readSubscriptionGeneration: () => 1, readPresentedObjectSession: shown,
      runResourceCommand() { calls.command += 1; return Promise.reject(new Error('Native edits are not available inside a component; return to the Scene first')); },
      runResourceQuery(action) { calls.query += 1; return action({ readPresentedObjectSession: shown, dispatch: (request) => { calls.dispatched.push(request.operation); return Promise.reject(sentinel); }, reconcile: () => Promise.reject(new Error('no')) }); } };
    const model = Model.create({ currentSession: () => stub, available: () => true, publish: () => true });
    await model.refresh().then(() => assert.fail('the sentinel stops the read'), (error) => assert.equal(error, sentinel));
    assert.equal(calls.query, 1, 'one query-route run');
    assert.equal(calls.command, 0, 'the refused command route is not used for the read');
    assert.equal(calls.dispatched.length, 1);
    assert.match(calls.dispatched[0], /^query\./, 'the reader sent a query operation');
  });
}
