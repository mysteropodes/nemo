'use strict';
// C-comp-owned-state: the markers (and camera keys) of the component the user is inside are the component's own, read from and written
// through the native owner. The real src/js/markers.js runs in a vm against the real adapter; the scene keeps the composition controller,
// an entered component never touches it, every intent is ONE command.document.object.component.state.edit and a refusal writes nothing.
const test = require('node:test'), assert = require('node:assert/strict'), fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const StateUI = require('../src/js/adapters/native-component-state-ui.js');
const SRC = path.join(__dirname, '..', 'src', 'js');

const settle = async () => { for (let i = 0; i < 12; i++) await new Promise((done) => setImmediate(done)); };

function fixture(options) {
  options = options || {};
  const edits = [], sceneEdits = [], toasts = [], reads = [];
  const store = { 'sym_x': [{ frame: 2, name: 'a', color: '#e8b64c' }, { frame: 9, name: 'b', color: '#ff0000' }], 'sym_y': [{ frame: 5, name: 'other', color: '#fff' }],
    keys: [{ frame: 0, x: 1, y: 2, w: 3, rot: null, hOut: null, hIn: null, ease: null }] };
  let revision = 4;
  const pin = () => ({ instanceId: 'i', documentId: 'd', lifecycleGeneration: 1, contentRevision: revision, documentSnapshotId: 's' + revision });
  const sceneProjection = { info: pin(), composition: { markers: [{ ordinal: 1, frame: 0, name: 'scene', color: '#000' }], effectiveBpm: 120, effectiveBpmOffset: 0, effectiveBpmShow: false } };
  const client = {
    async listMarkers(context) { reads.push(['markers', context]); return options.readFails ? { ok: false, code: 'unavailable' } : { ok: true, items: (store[context] || []).map((m) => ({ ...m })) }; },
    async listCameraKeys(context) { reads.push(['keys', context]); return { ok: true, items: store.keys }; },
    async editState(context, edit) {
      edits.push([context, JSON.parse(JSON.stringify(edit))]);
      if (options.refuse) return { ok: false, code: 'invalid_request', message: 'refused' };
      revision += 1;
      const list = store[context];
      if (edit.kind === 'addMarker') list.push({ frame: edit.frame, name: edit.name || '', color: edit.color }), list.sort((a, b) => a.frame - b.frame);
      if (edit.kind === 'removeMarker') list.splice(list.findIndex((m) => m.frame === edit.frame), 1);
      if (edit.kind === 'clearMarkers') list.length = 0;
      if (edit.kind === 'updateMarker') { const m = list.find((x) => x.frame === edit.frame); if (edit.name !== undefined) m.name = edit.name; if (edit.newFrame !== undefined) m.frame = edit.newFrame; list.sort((a, b) => a.frame - b.frame); }
      return { ok: true, result: { applied: true, historyEntriesAdded: 1 } };
    },
  };
  const window = {
    __TAURI__: { core: {} },
    state: { activeSymbolId: options.scene ? null : 'sym_x', totalFrames: 24, currentFrame: 3 },
    showToast: (text) => toasts.push(text),
    SM: { t: (key) => key },
    NemoNativeObjectProject: { desktop: () => ({ isActive: () => true, info: pin }) },
    NemoNativeObjectComponent: { desktop: () => client },
    NemoNativeCompositionController: { read: () => sceneProjection, edit: (action, origin) => { sceneEdits.push([action, origin === sceneProjection]); return Promise.resolve({ status: 'applied-presented' }); } },
    SMBpm: { openSettings: (...args) => { window.bpmArgs = args; } },
    renderTimeline: () => {},
  };
  window.window = window;
  window.document = { readyState: 'complete', getElementById: () => null, querySelectorAll: () => [], addEventListener: () => {} };
  const context = vm.createContext(window);
  vm.runInContext(fs.readFileSync(path.join(SRC, 'markers.js'), 'utf8'), context);
  const state = StateUI.create(window);
  return { window, state, edits, sceneEdits, toasts, reads, store, markers: window.SMMarkers, bump: () => { revision += 1; sceneProjection.info = pin(); } };
}

test('inside a component the marker strip reads the component markers, with their one-based ordinals, and never the scene composition', async () => {
  const f = fixture();
  assert.equal(f.state.active(), 'sym_x');
  assert.deepEqual(f.markers.compMarkers(), [], 'nothing is shown until the stored page was read');
  await settle();
  const shown = f.markers.compMarkers();
  assert.deepEqual(shown.map((m) => [m.ordinal, m.frame, m.name, m.color]), [[1, 2, 'a', '#e8b64c'], [2, 9, 'b', '#ff0000']]);
  assert.deepEqual(f.reads, [['markers', 'sym_x']]);
  f.markers.compMarkers(); f.markers.compMarkers();
  assert.equal(f.reads.length, 1, 'one read per presented revision');
  f.bump(); f.markers.compMarkers(); await settle();
  assert.equal(f.reads.length, 2, 'a new revision is read again (undo and redo change the stored markers)');
  assert.equal(f.sceneEdits.length, 0);
});

test('the scene keeps the composition controller and a component never sees another component markers', async () => {
  const scene = fixture({ scene: true });
  assert.equal(scene.state.active(), null);
  assert.deepEqual(scene.markers.compMarkers().map((m) => m.name), ['scene']);
  scene.markers.addCompMarker(7, 'x');
  assert.deepEqual(scene.sceneEdits.map((e) => e[0].kind), ['addMarker']);
  assert.equal(scene.edits.length, 0, 'the scene never writes a component record');
  const f = fixture();
  f.markers.compMarkers(); await settle();
  f.window.state.activeSymbolId = 'sym_y'; f.markers.compMarkers(); await settle();
  assert.deepEqual(f.markers.compMarkers().map((m) => m.name), ['other']);
  f.window.state.activeSymbolId = 'sym_x'; f.window.state.activeSymbolId = 'sym_z';
  assert.deepEqual(f.markers.compMarkers(), [], 'an unread component shows nothing, not the previous one');
});

test('adding a marker at the playhead sends ONE state.edit for the entered component and shows the stored result', async () => {
  const f = fixture();
  f.markers.compMarkers(); await settle();
  f.markers.addCompMarker(7, 'hi'); await settle();
  assert.deepEqual(f.edits, [['sym_x', { kind: 'addMarker', frame: 7, name: 'hi', color: '#e8b64c' }]]);
  assert.deepEqual(f.markers.compMarkers().map((m) => m.frame), [2, 7, 9]);
  assert.equal(f.sceneEdits.length, 0);
});

test('the strip intents are frame-addressed native edits: move, rename, remove and clear', async () => {
  const f = fixture();
  f.markers.compMarkers(); await settle();
  const origin = f.state.read();
  await f.state.edit({ kind: 'updateMarker', ordinal: 2, frame: 12 }, origin);
  await f.state.edit({ kind: 'updateMarker', ordinal: 1, name: 'renamed' }, f.state.read());
  await f.state.edit({ kind: 'removeMarker', ordinal: 1 }, f.state.read());
  await f.state.edit({ kind: 'clearMarkers' }, f.state.read());
  assert.deepEqual(f.edits.map((e) => e[1]), [
    { kind: 'updateMarker', frame: 9, newFrame: 12 },
    { kind: 'updateMarker', frame: 2, name: 'renamed' },
    { kind: 'removeMarker', frame: 2 },
    { kind: 'clearMarkers' },
  ]);
  // A marker the cache does not hold is refused before anything is sent.
  f.edits.length = 0;
  const outcome = await f.state.edit({ kind: 'removeMarker', ordinal: 9 }, f.state.read());
  assert.equal(outcome.status, 'unavailable');
  assert.equal(f.edits.length, 0);
});

test('a refusal says why, writes nothing and leaves the stored markers shown', async () => {
  const f = fixture({ refuse: true });
  f.markers.compMarkers(); await settle();
  f.markers.addCompMarker(7, 'nope'); await settle();
  assert.equal(f.toasts.length, 1);
  assert.match(f.toasts[0], /refused/);
  assert.deepEqual(f.markers.compMarkers().map((m) => m.frame), [2, 9]);
});

test('the BPM grid menu keeps the scene composition owner inside a component', async () => {
  const f = fixture();
  f.markers.compMarkers(); await settle();
  const origin = f.state.read();
  assert.ok(origin.context && origin.composition.effectiveBpm === 120, 'the projection carries the scene BPM facts');
});

test('the camera keys of the entered component are read and written as native component edits', async () => {
  const f = fixture();
  assert.deepEqual(await f.state.readCameraKeys(), f.store.keys);
  await f.state.setCameraKey(4, { x: 10, y: 20, w: 100, rot: 5 });
  await f.state.setCameraEase(4, [0.25, 0.1, 0.25, 1]);
  await f.state.removeCameraKey(4);
  await f.state.clearCamera();
  assert.deepEqual(f.edits.map((e) => e[1]), [
    { kind: 'setCameraKey', frame: 4, x: 10, y: 20, w: 100, rot: 5 },
    { kind: 'setCameraEase', frame: 4, ease: [0.25, 0.1, 0.25, 1] },
    { kind: 'removeCameraKey', frame: 4 },
    { kind: 'clearCamera' },
  ]);
  assert.ok(f.edits.every((e) => e[0] === 'sym_x'));
  const scene = fixture({ scene: true });
  assert.equal(await scene.state.readCameraKeys(), null, 'the scene camera has its own owner');
  const refused = await scene.state.setCameraKey(1, { x: 1, y: 2, w: 3 });
  assert.equal(refused.status, 'unavailable');
  assert.equal(scene.edits.length, 0);
});
