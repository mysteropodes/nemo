'use strict';
// CS8-D08 I44 (#2381): opening a Kitsu shot natively. Duration travels in the one New Project intent; the legacy
// state.layers / createUserLayer / totalFrames writers are never reached, and a refused open records nothing.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const Project = require('../src/js/adapters/native-object-project.js');
const Files = require('../src/js/adapters/native-object-project-file.js');
const Workflow = require('../src/js/application/native-object-save-as-workflow.js');
const Ack = require('../src/js/application/native-object-saved-acknowledgement.js');
const Service = require('../src/js/adapters/kitsu-service.js');

function host(skew = 0) {
  let installed = false, number = 0, meta = null;
  const calls = [];
  const h = { calls };
  h.invoke = async (command, args) => {
    const r = args ? JSON.parse(args.requestJson) : null;
    calls.push({ command, r });
    if (command === 'nemo_native_object_client_status') return { apiVersion: 2, instanceId: 'instance', available: installed, ...meta };
    if (command === 'nemo_native_object_bootstrap') {
      const cfg = r.newProject;
      installed = true; number++;
      meta = { apiVersion: 2, instanceId: 'instance', documentId: 'document-' + number, contentRevision: 0,
        documentSnapshotId: 'snapshot-' + number, lifecycleGeneration: number, documentDigest: String(number).repeat(64), byteCount: 1000,
        canvasW: cfg.canvasW, canvasH: cfg.canvasH, fps: cfg.fps, totalFrames: (cfg.totalFrames ?? Math.round(cfg.fps * 5)) + skew,
        layers: [{ layerUid: 'layer-1', name: 'Layer 1', visible: true }], reference: null, undoDepth: 0, redoDepth: 0 };
      return { apiVersion: 2, requestId: r.requestId, instanceId: 'instance', documentId: meta.documentId, contentRevision: 0, resourceCount: 0, viewportAvailable: false };
    }
    if (command === 'nemo_native_object_reference') return { ...meta, requestId: r.requestId };
    if (command === 'nemo_native_object_preview') return { status: 'presented', workId: 'work-' + calls.length, viewGeneration: number };
    throw Error('Unexpected native route: ' + command);
  };
  return h;
}
const session = (h) => Project.create({ invoke: h.invoke,
  mapping: (info) => ({ cssBounds: { width: 640, height: 360 }, physicalExtent: { width: 640, height: 360 }, compositionExtent: { width: info.canvasW, height: info.canvasH } }) });
const bootstraps = (h) => h.calls.filter((c) => c.command === 'nemo_native_object_bootstrap');

test('the New Project intent carries an optional totalFrames to the bootstrap request and checks the receipt', async () => {
  const h = host(), s = session(h);
  const info = await s.newProject({ canvasW: 640, canvasH: 360, fps: 25, totalFrames: 37 });
  assert.equal(info.totalFrames, 37);
  assert.deepEqual(bootstraps(h)[0].r.newProject, { canvasW: 640, canvasH: 360, fps: 25, totalFrames: 37 });
  const plain = host(), p = session(plain);
  await p.newProject({ canvasW: 640, canvasH: 360, fps: 25 });
  assert.deepEqual(Object.keys(bootstraps(plain)[0].r.newProject).sort(), ['canvasH', 'canvasW', 'fps']);
  const skewed = host(1);
  await assert.rejects(session(skewed).newProject({ canvasW: 64, canvasH: 48, fps: 24, totalFrames: 10 }), /uncorrelated/);
});

test('an out-of-range or non-integer duration is refused before any native request', async () => {
  for (const totalFrames of [0, -1, 1.5, '12', null, NaN, 4294967296, Infinity]) {
    const h = host(), s = session(h);
    await assert.rejects(s.newProject({ canvasW: 64, canvasH: 48, fps: 24, totalFrames }), { message: /scalar intent is invalid/ }, String(totalFrames));
    assert.equal(h.calls.length, 0);
  }
  const h = host(), s = session(h);
  await assert.rejects(s.newProject({ canvasW: 64, canvasH: 48, fps: 24, totalFrames: 5, extra: 1 }), { message: /scalar intent is invalid/ });
  assert.equal((await s.newProject({ canvasW: 64, canvasH: 48, fps: 24, totalFrames: 4294967295 })).totalFrames, 4294967295);
});

test('the project file form forwards the duration from the legacy-shaped New config', async () => {
  const h = host(), s = session(h);
  const ui = Files.create(s, { readUI: () => ({ handle: 'tab', generation: 1 }), confirmReplace: async () => true, occlude() {}, reveal() {}, restore() {},
    opened: () => true, created: () => true, commit: () => true, repaint() {}, pick: async () => '/n.json', notice() {}, failed() {}, refused() {} }, Workflow, Ack);
  assert.equal(await ui.newProject({ w: 1280, h: 720, fps: 24, name: 'Shot', totalFrames: 48 }), true);
  assert.deepEqual(bootstraps(h)[0].r.newProject, { canvasW: 1280, canvasH: 720, fps: 24, totalFrames: 48 });
  assert.equal(s.info().totalFrames, 48);
});

// ---- openShot against a mocked Kitsu server ----
const source = fs.readFileSync(path.join(__dirname, '../src/js/kitsu.js'), 'utf8');
const kitsuSession = { serverUrl: 'https://kitsu.invalid', accessToken: 'fixture-token' };
const reply = (value) => ({ ok: true, status: 200, json: async () => value, text: async () => '' });
function nativeRoot(newProjectResult, structure) {
  const legacy = [], created = [], built = [];
  const trap = (name) => (...args) => { legacy.push(name); };
  const state = new Proxy({ layers: [{ frames: [] }] }, { set(target, key, value) { if (!String(key).startsWith('kitsu')) legacy.push('state.' + String(key)); return Reflect.set(target, key, value); } });
  const root = { console, state, URLSearchParams, NemoKitsuService: Service,
    document: { readyState: 'loading', addEventListener() {} },
    sessionStorage: { getItem: () => null, setItem() {}, removeItem() {} },
    localStorage: { setItem() { throw Error('unexpected persistent credential write'); } },
    createUserLayer: trap('createUserLayer'), activateUL: trap('activateUL'), loadFrame: trap('loadFrame'), renderOS: trap('renderOS'),
    renderArcs: trap('renderArcs'), updateUI: trap('updateUI'),
    SMProject: { nativeObjectProject: () => ({}), async newProject(cfg) { created.push(cfg); return newProjectResult; } },
    NemoKitsuNativeStructure: { async build(rt) { built.push(rt.state === state); if (structure) throw structure; return { layers: [{ name: 'Rough' }, { name: 'Clean' }, { name: 'Color' }] }; } },
    __TAURI__: { http: { fetch: async () => reply([]) } } };
  root.window = root;
  vm.runInNewContext(source, root, { filename: 'src/js/kitsu.js' });
  return { root, legacy, created, state, built };
}
const shot = { id: 's1', name: 'SH010', data: { fps: '25', frame_in: 10, frame_out: 57 } };

test('openShot creates the project natively with width, height, fps and duration and performs no legacy write', async () => {
  const { root, legacy, created, state, built } = nativeRoot(true);
  const result = await root.SMKitsu.openShot(kitsuSession, { id: 'p1', name: 'Proj' }, null, shot);
  assert.deepEqual(JSON.parse(JSON.stringify(created)), [{ w: 1920, h: 1080, fps: 25, name: 'Proj — SH010', totalFrames: 48 }]);
  assert.equal(result.meta.frameCount, 48);
  assert.deepEqual(Array.from(result.meta.layerStructure), ['Rough', 'Clean', 'Color']);
  assert.deepEqual(built, [true], 'the layer structure is built once, after the project, through the native module');
  assert.deepEqual(legacy, []);
  assert.equal(state.layers.length, 1);
  assert.equal(state.layers[0].name, undefined);
  assert.equal(root.state.kitsuShot.shotId, 's1');
});

test('a refused native creation throws, records no Kitsu shot and writes nothing legacy', async () => {
  for (const outcome of [false, null, undefined]) {
    const { root, legacy, created, state } = nativeRoot(outcome);
    await assert.rejects(root.SMKitsu.openShot(kitsuSession, { id: 'p1', name: 'Proj' }, null, shot), /natif/);
    assert.equal(created.length, 1);
    assert.deepEqual(legacy, []);
    assert.ok(!state.kitsuShot);
  }
});

test('an unreadable duration never reaches native creation', async () => {
  const { root, legacy, created } = nativeRoot(true);
  await assert.rejects(root.SMKitsu.openShot(kitsuSession, {}, null, { name: 'S', data: { fps: 'abc' } }), /Durée/);
  assert.equal(created.length, 0);
  assert.deepEqual(legacy, []);
});

test('a refused layer step throws, records no Kitsu shot, and still performs no legacy write', async () => {
  const { root, legacy, created, state, built } = nativeRoot(true, new Error('Structure des calques Kitsu : étape « create Clean » refusée'));
  await assert.rejects(root.SMKitsu.openShot(kitsuSession, { id: 'p1', name: 'Proj' }, null, shot), /create Clean/);
  assert.equal(created.length, 1);
  assert.equal(built.length, 1);
  assert.deepEqual(legacy, []);
  assert.ok(!state.kitsuShot);
  assert.equal(state.layers.length, 1);
});

test('a refused project creation never reaches the layer structure', async () => {
  const { root, built } = nativeRoot(false);
  await assert.rejects(root.SMKitsu.openShot(kitsuSession, { id: 'p1', name: 'Proj' }, null, shot), /natif/);
  assert.deepEqual(built, []);
});
