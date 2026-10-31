'use strict';
// #2256 leaf 3: the REAL native project session presents an entered component context. The viewport request names the context and
// bounds the frame by the component's own frame count, the presentation carries that context (so every scene editor that proves
// `presentation.contextId === 'scene-root'` declines), and an action that needs the presented scene lease (the resource route of
// draw, text, frame edits) is refused inside a component. The scene keeps its request exactly as before.
const test = require('node:test'), assert = require('node:assert/strict');
const Project = require('../src/js/adapters/native-object-project.js');

function host() {
  const calls = [];
  const info = () => ({ apiVersion: 2, instanceId: 'instance', documentId: 'document', lifecycleGeneration: 1, documentSnapshotId: 'snapshot-0', contentRevision: 0,
    canvasW: 320, canvasH: 180, fps: 24, totalFrames: 21, documentDigest: 'a'.repeat(64), byteCount: 10000, layers: [{ layerUid: 'drawing' }], reference: null, undoDepth: 0, redoDepth: 0 });
  let installed = false;
  async function invoke(command, args) {
    const r = args ? JSON.parse(args.requestJson) : null;
    calls.push({ command, r });
    if (command === 'nemo_native_object_client_status') return Object.assign(info(), { available: installed });
    if (command === 'nemo_native_object_bootstrap') { installed = true; return Object.assign(info(), { requestId: r.requestId, resourceCount: 0, viewportAvailable: false }); }
    if (command === 'nemo_native_object_reference') return Object.assign(info(), { requestId: r.requestId });
    if (command === 'nemo_native_object_preview') return { status: 'presented', workId: 'work-' + calls.length, viewGeneration: 1 };
    throw new Error('Unexpected command: ' + command);
  }
  return { invoke, calls, previews: () => calls.filter((c) => c.command === 'nemo_native_object_preview').map((c) => c.r) };
}
async function session(h, context) {
  const s = Project.create({ invoke: h.invoke, frame: () => 0, context, projectionReady: () => true,
    mapping: (pin) => ({ cssBounds: { x: 0, y: 0, width: 320, height: 180 }, physicalExtent: { width: 320, height: 180 }, compositionExtent: { width: pin.canvasW, height: pin.canvasH }, reportedDpr: 1 }) });
  await s.openPath('/project.json');
  return s;
}

test('with no entered context the viewport request is the scene-root request over the scene frame count, as before', async () => {
  const h = host(), s = await session(h, () => null);
  const shown = await s.presentPreview(20);
  const request = h.previews().pop();
  assert.equal(request.contextId, 'scene-root');
  assert.equal(request.frame, 20);
  assert.equal(shown.contextId, 'scene-root');
  assert.equal(s.readPresentedObjectSession().presentation.contextId, 'scene-root');
  await assert.rejects(() => s.presentPreview(21), /outside the native project/);
});

test('an entered component is presented by context and bounded by its own frame count', async () => {
  const h = host();
  let entered = null;
  const s = await session(h, () => entered);
  entered = { contextId: 'sym_a', totalFrames: 12, fps: 12 };
  const shown = await s.presentPreview(11);
  const request = h.previews().pop();
  assert.equal(request.contextId, 'sym_a');
  assert.equal(request.frame, 11);
  assert.equal(request.scopeKind, 'authored');
  assert.equal(shown.contextId, 'sym_a', 'the presentation says which context it shows');
  assert.equal(shown.frame, 11);
  const session0 = s.readPresentedObjectSession();
  assert.equal(session0.presentation.contextId, 'sym_a');
  const before = h.previews().length;
  await assert.rejects(() => s.presentPreview(12), /outside the native project/);
  assert.equal(h.previews().length, before, 'a frame outside the component sends nothing');
  entered = null;
  await s.presentPreview(20);
  assert.equal(h.previews().pop().contextId, 'scene-root', 'leaving the component presents the scene again');
});

test('an action that needs the presented scene lease is refused inside a component and runs in the scene', async () => {
  const h = host();
  let entered = null;
  const s = await session(h, () => entered);
  await s.presentPreview(0);
  const lease = () => s.runResourceCommand(async (ctx) => ctx.readPresentedObjectSession());
  const scene = await lease();
  assert.ok(scene && scene.presentation.contextId === 'scene-root', 'the scene lease is granted');
  entered = { contextId: 'sym_a', totalFrames: 12, fps: 12 };
  await s.presentPreview(0);
  assert.equal(await lease(), null, 'no scene lease inside a component: the editors report unavailable');
  entered = null;
  await s.presentPreview(0);
  assert.ok(await lease(), 'the lease is granted again once the scene is presented');
});
