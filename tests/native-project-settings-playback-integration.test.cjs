'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const Project = require('../src/js/adapters/native-object-project.js');
const Files = require('../src/js/adapters/native-object-project-file.js');
const Motion = require('../src/js/adapters/native-opacity-motion-surface.js');
const Desktop = require('../src/js/adapters/native-project-settings-desktop.js');
const Intent = require('../src/js/adapters/native-project-settings-intent.js');
const TextRead = require('../src/js/adapters/native-text-read.js');
const turn = () => new Promise(setImmediate);
function deferred() {let resolve; const promise = new Promise(r => {resolve = r;}); return {promise, resolve};}
function shipped(file, name) {
  const source = fs.readFileSync(path.join(__dirname, '../src/js', file), 'utf8');
  const start = source.indexOf('function ' + name + '('), end = source.indexOf('\n}', start);
  assert.ok(start >= 0 && end > start, name);
  return source.slice(start, end + 2);
}
async function fixture() {
  let installed = false, revision = 0, fps = 31.5, now = 0, serial = 0, frameWork = 0, previewGate = null, writeGate = null, readGate = null;
  const callbacks = new Map(), writes = [], notices = [];
  const info = () => ({apiVersion: 2, instanceId: 'instance', documentId: 'document', lifecycleGeneration: 1,
    contentRevision: revision, documentSnapshotId: 'snapshot-' + revision, documentDigest: 'a'.repeat(64), byteCount: 1000,
    canvasW: 320, canvasH: 180, fps, totalFrames: 24, reference: null, layers: [{layerUid: 'drawing'}]});
  const fields = new Map();
  const document = {getElementById(id) {
    if (!fields.has(id)) fields.set(id, {value: '', innerHTML: '', classList: {add() {}, remove() {}}});
    return fields.get(id);
  }};
  const root = {state: {}, userLayers: [], document, notices, NemoNativeProjectSettingsIntent: Intent,
    NemoNativeObjectProjectFile: Files, showToast: message => notices.push(message),
    requestAnimationFrame: callback => {callbacks.set(++serial, callback); return serial;},
    cancelAnimationFrame: id => callbacks.delete(id)};
  let session, controller;
  const invoke = async (command, args) => {
    const request = args && JSON.parse(args.requestJson);
    if (command === 'nemo_native_object_client_status') return {...info(), available: installed};
    if (command === 'nemo_native_object_bootstrap') {
      installed = true; return {...info(), requestId: request.requestId, resourceCount: 0, viewportAvailable: false};
    }
    if (command === 'nemo_native_object_reference') return {...info(), requestId: request.requestId};
    if (command === 'nemo_native_object_preview') {
      if (previewGate) {const gate = previewGate; previewGate = null; gate.entered.resolve(); await gate.release.promise;}
      return {status: 'presented', workId: 'work-' + (++frameWork), viewGeneration: frameWork};
    }
    throw Error('Unexpected native boundary: ' + command);
  };
  session = Project.create({invoke, frame: () => root.state.currentFrame,
    projectionReady: value => Files.projectionReadyForUI(root, value),
    mapping: () => ({cssBounds: {x: 0, y: 0, width: 320, height: 180}, physicalExtent: {width: 320, height: 180},
      compositionExtent: {width: 320, height: 180}, reportedDpr: 1}),
    subscribeRevisions: async () => ({...info(), subscriptionId: 'subscription'}),
    subscriptionActive: () => true,
    publishRevision(value) {controller.willProject(value); return Files.refreshLayerTable(root, value);},
    changed(value) {controller.willProject(value); assert.equal(Files.refreshLayerTable(root, value), true);},
    presented() {if (controller) setImmediate(() => controller.presented());},
    dispatchApplication: async request => {
      if (request.operation === 'query.document.object.text') {
        if (readGate) {const gate = readGate; readGate = null; gate.entered.resolve(); await gate.release.promise;}
        return {apiVersion: 2, requestId: request.requestId, instanceId: request.instanceId, documentId: request.documentId,
          contentRevision: revision, ok: true, result: {instanceId: request.instanceId, documentId: request.documentId,
            atRevision: revision, documentSnapshotId: info().documentSnapshotId, kind: 'targets',
            scope: request.payload.view.scope, offset: 0, total: 0, items: [], nextOffset: null}};
      }
      assert.equal(request.operation, 'command.document.object.project.settings.edit');
      assert.equal(request.expectedRevision, revision); writes.push(request);
      if (writeGate) {writeGate.entered.resolve(); await writeGate.release.promise;}
      fps = request.payload.action.fps; revision++;
      return {apiVersion: 2, requestId: request.requestId, instanceId: 'instance', documentId: 'document',
        contentRevision: revision, ok: true, result: {applied: true, historyEntriesAdded: 1}};
    }});
  root.NemoNativeObjectProject = {desktop: () => session};
  root.NemoNativeOpacityCutover = session;
  const context = vm.createContext({window: root, state: root.state, document, NemoNativeOpacityMotionSurface: Motion,
    performance: {now: () => now}, showToast: root.showToast,
    // The ordinary Text inspector refreshes its authored-frame list after paint.
    // Use that production read path and Project queue, without synthetic latency.
    updateUI() {TextRead.create(session).listPage({contextId: 'scene-root', layerUid: 'drawing',
      frameScope: {kind: 'authored', frame: root.state.currentFrame}}, 0, 8, session.info()).catch(() => {});},
    renderOS() {}, renderArcs() {},
    cancelAnimationFrame: root.cancelAnimationFrame, clearInterval});
  vm.runInContext('var playInt=null,playRaf=null,nativePlayScheduler=null,_nativeFrameNavigator=null;\n' +
    shipped('app.js', 'goToFrame') + '\n' +
    ['startNativePlay', 'advancePlayFrame', 'startPlay', 'stopPlay', 'togglePlay'].map(name => shipped('timeline.js', name)).join('\n'), context);
  root.startPlay = context.startPlay; root.stopPlay = context.stopPlay; root.togglePlay = context.togglePlay;
  controller = Desktop.install(root);
  await session.openPath('/synthetic-settings.json', {reveal(value) {Files.projectView(root, value);}});
  return {root, session, controller, writes, notices,
    async tick(time, settle = true) {now = time; const entry = [...callbacks].at(-1); assert.ok(entry); callbacks.delete(entry[0]); entry[1](now); if (settle) await turn();},
    holdPreview() {previewGate = {entered: deferred(), release: deferred()}; return previewGate;},
    holdWrite() {writeGate = {entered: deferred(), release: deferred()}; return writeGate;},
    holdRead() {readGate = {entered: deferred(), release: deferred()}; return readGate;},
    navigate: context.goToFrame};
}
test('FPS change joins actual native frame restoration before settings dispatch and resumes playback', async () => {
  const f = await fixture(); f.root.startPlay(); await f.tick(100);
  assert.equal(f.root.state.currentFrame, 3); assert.equal(f.root.state.playing, true);
  const result = await f.controller.edit({kind: 'setFps', fps: 48});
  assert.equal(result.status, 'applied-presented', JSON.stringify(f.notices));
  assert.equal(f.writes.length, 1); assert.equal(f.session.info().contentRevision, 1);
  assert.equal(f.root.state.currentFrame, 3); assert.equal(f.root.state.fps, 48);
  assert.equal(f.root.state.playing, true); await f.tick(200); assert.equal(f.root.state.currentFrame, 7);
});

for (const change of ['none', 'owner', 'selection', 'frame']) test('FPS edit waits for painted text reads and preserves ' + change + ' context guard', {timeout: 2000}, async () => {
  const f = await fixture(); f.root.startPlay(); await f.tick(100);
  const gate = f.holdRead(), edit = f.controller.edit({kind: 'setFps', fps: 48});
  await gate.entered.promise;
  assert.equal(f.writes.length, 0, 'settings cannot overtake the inspector read');
  if (change === 'owner') f.root.NemoNativeObjectProject.desktop = () => ({...f.session});
  if (change === 'selection') f.root._layerSel = [];
  const navigation = change === 'frame' ? f.navigate(9) : null;
  gate.release.resolve();
  const result = await edit;
  assert.equal(result.status, change === 'none' ? 'applied-presented' : 'unavailable');
  assert.equal(f.writes.length, change === 'none' ? 1 : 0);
  if (navigation) assert.equal(await navigation, true);
});
for (let continuation = 0; continuation < 16; continuation++) test('FPS edit preserves a completing native playback frame, continuation ' + continuation, async () => {
  const f = await fixture(); f.root.startPlay(); const gate = f.holdPreview();
  await f.tick(100, false); await gate.entered.promise; gate.release.resolve();
  for (let i = 0; i < continuation; i++) await Promise.resolve();
  const result = await f.controller.edit({kind: 'setFps', fps: 48});
  assert.equal(result.status, 'applied-presented', JSON.stringify(f.notices));
  assert.equal(f.writes.length, 1); assert.equal(f.root.state.fps, 48); assert.equal(f.root.state.playing, true);
});
test('explicit Stop during the real pending settings dispatch prevents automatic resume', async () => {
  const f = await fixture(); f.root.startPlay(); await f.tick(100);
  const gate = f.holdWrite(), edit = f.controller.edit({kind: 'setFps', fps: 48});
  await gate.entered.promise; f.root.stopPlay(); gate.release.resolve();
  assert.equal((await edit).status, 'applied-presented');
  assert.equal(f.writes.length, 1); assert.equal(f.root.state.playing, false);
});
for (const change of ['owner', 'selection', 'frame']) test('pending FPS restoration refuses changed ' + change, async () => {
  const f = await fixture(); f.root.startPlay(); await f.tick(100);
  const gate = f.holdPreview(), edit = f.controller.edit({kind: 'setFps', fps: 48});
  await gate.entered.promise;
  if (change === 'owner') f.root.NemoNativeObjectProject.desktop = () => ({...f.session});
  if (change === 'selection') f.root._layerSel = [];
  const navigation = change === 'frame' ? f.navigate(9) : null;
  gate.release.resolve();
  assert.equal((await edit).status, 'unavailable'); assert.equal(f.writes.length, 0);
  if (navigation) assert.equal(await navigation, true);
  assert.equal(f.root.state.playing, false);
});
test('a synchronous Stop failure releases all preparing intents without a write', async () => {
  const f = await fixture(); f.root.startPlay(); await f.tick(100);
  f.root.stopPlay = () => {throw Error('Stop failed');};
  const first = f.controller.edit({kind: 'setFps', fps: 48});
  const latest = f.controller.edit({kind: 'setFps', fps: 30});
  assert.equal((await first).status, 'superseded'); assert.equal((await latest).status, 'unavailable');
  assert.equal(f.controller.isPending(), false); assert.equal(f.writes.length, 0);
});

for (const phase of ['restoration', 'dispatch']) test('ordinary Stop during pending FPS ' + phase + ' cancels resume and later Play works', async () => {
  const f = await fixture(); f.root.startPlay(); await f.tick(100);
  const gate = phase === 'restoration' ? f.holdPreview() : f.holdWrite();
  const edit = f.controller.edit({kind: 'setFps', fps: 48});
  await gate.entered.promise;
  assert.equal(f.root.state.playing, false);
  assert.ok(f.root.document.getElementById('btn-play').innerHTML.includes('\u{e034}'), 'pending playback still offers Stop');
  f.root.togglePlay();
  assert.equal(f.root.state.playing, false, 'ordinary Stop must not restart the old clock');
  assert.ok(f.root.document.getElementById('btn-play').innerHTML.includes('\u{e037}'));
  gate.release.resolve();
  assert.equal((await edit).status, 'applied-presented', JSON.stringify(f.notices));
  assert.equal(f.writes.length, 1); assert.equal(f.root.state.playing, false);
  assert.equal(f.root.state.fps, 48); assert.equal(f.root.state.currentFrame, 3);
  f.root.togglePlay(); assert.equal(f.root.state.playing, true);
  await f.tick(200); assert.equal(f.root.state.currentFrame, 7);
});
