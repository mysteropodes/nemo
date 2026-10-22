'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const diagnostic = require('../src/js/adapters/native-import-diagnostic.js');

const PRIVATE = 'PRIVATE-document-path-token-content-error-stack';
const tick = () => new Promise((resolve) => setImmediate(resolve));
const deferred = () => {
  let resolve;
  const promise = new Promise((done) => { resolve = done; });
  return { promise, resolve };
};

function fixture() {
  const writes = [], renames = [], dirs = [];
  const calls = { imports: 0, pickers: 0, toasts: 0, paths: 0 };
  const info = {
    instanceId: PRIVATE + '-instance', documentId: PRIVATE + '-document', lifecycleGeneration: 1,
    contentRevision: 7, documentSnapshotId: PRIVATE + '-snapshot', totalFrames: 24,
    name: PRIVATE, path: PRIVATE, contents: PRIVATE, layers: [{ layerUid: PRIVATE + '-layer', name: PRIVATE }],
  };
  const session = {
    info: () => info, isActive: () => true, readSubscriptionGeneration: () => 3,
    readPresentedObjectSession: () => ({ ...info, available: true,
      presentation: { ...info, owner: 'native', status: 'presented', frame: 2 } }),
    importBatch: () => { calls.imports += 1; throw new Error(PRIVATE); },
  };
  const root = {
    state: { currentFrame: 2, privateData: PRIVATE },
    NemoNativeObjectProjectFile: { projectionReadyForUI: () => true },
    NemoApplicationMcpTransport: { createNativeTauriTransport() {} },
    NemoNativeObjectRasterAsset: { desktop() {} },
    showToast: () => { calls.toasts += 1; },
    __TAURI__: {
      event: { listen() {} },
      dialog: { open: () => { calls.pickers += 1; } },
      path: { appDataDir: async () => { calls.paths += 1; return '/private-test-app/'; } },
      fs: {
        mkdir: async (...args) => { dirs.push(args); },
        writeTextFile: async (path, text) => { writes.push({ path, text }); },
        rename: async (...args) => { renames.push(args); },
      },
    },
  };
  const state = { routeDeclared: true, routeRetired: false, imagePending: false };
  const ports = { root, current: () => session, selection: () => [info.layers[0].layerUid], state: () => state };
  return { root, session, info, state, ports, calls, writes, renames, dirs };
}
const read = (f, index = f.writes.length - 1) => JSON.parse(f.writes[index].text);

test('post-result diagnostics retain useful gate facts without private identity or import side effects', async () => {
  const f = fixture(), api = diagnostic.create(f.ports);
  const result = Object.freeze({ status: 'unavailable', code: 'unavailable', reason: PRIVATE,
    error: PRIVATE, stack: PRIVATE, payload: { contents: PRIVATE }, path: PRIVATE });
  assert.equal(api.observe(result), undefined, 'observation returns synchronously');
  await tick();
  assert.equal(f.writes.length, 1);
  const receipt = read(f);
  assert.equal(receipt.schemaVersion, 1);
  assert.equal(receipt.kind, 'native-import-unavailable');
  assert.equal(receipt.status, 'unavailable');
  assert.equal(receipt.reason, 'other');
  assert.equal(receipt.code, 'unavailable');
  assert.equal(receipt.imageCatchReason, 'other');
  assert.equal(f.writes[0].text.includes(PRIVATE), false);
  assert.deepEqual(Object.keys(receipt).sort(), ['code', 'imageCatchReason', 'kind', 'reason', 'schemaVersion', 'snapshot', 'status']);
  assert.equal(receipt.snapshot.phase, 'post-result');
  for (const key of ['routeDeclared', 'sessionPresent', 'active', 'infoValid', 'frameValid', 'selectionValid',
    'projectionReady', 'presentedAvailable', 'presentedPinsMatch', 'presentedOwnerNative', 'presentedStatusReady',
    'presentedFrameMatch', 'importBatchCallable', 'subscriptionIdentityCallable', 'subscriptionTicketPositive',
    'desktopEventListenPresent', 'transportModulePresent', 'rasterModulePresent']) assert.equal(receipt.snapshot[key], true, key);
  assert.equal(receipt.snapshot.routeRetired, false);
  assert.equal(receipt.snapshot.imagePending, false);
  assert.ok(Object.values(receipt.snapshot).every((value) => typeof value === 'boolean' || value === 'post-result' || value === 'error'));
  assert.deepEqual(f.dirs, [['/private-test-app/diagnostics', { recursive: true }]]);
  assert.equal(f.writes[0].path, '/private-test-app/diagnostics/native-import-gate.pending.json');
  assert.deepEqual(f.renames, [['/private-test-app/diagnostics/native-import-gate.pending.json', '/private-test-app/diagnostics/native-import-gate.json']]);
  assert.deepEqual([f.calls.imports, f.calls.pickers, f.calls.toasts], [0, 0, 0]);
});

test('the unavailable public result keeps a fixed private reason associated only with its exact object', async () => {
  const f = fixture(), api = diagnostic.create(f.ports);
  const error = { reason: 'image_codec_unavailable', message: PRIVATE, stack: PRIVATE };
  const result = api.unavailable(error);
  assert.deepEqual(result, { status: 'unavailable', code: 'unavailable' });
  assert.deepEqual(Reflect.ownKeys(result).sort(), ['code', 'status'], 'no private error field or symbol crosses the public boundary');
  error.reason = 'owner_reopened';
  api.observe(result);
  await tick();
  assert.equal(read(f).imageCatchReason, 'image_codec_unavailable');
  api.observe({ ...result });
  await tick();
  assert.equal(read(f).imageCatchReason, 'other', 'a lookalike result does not inherit another error');
  api.observe(api.unavailable({ reason: PRIVATE, message: PRIVATE }));
  await tick();
  assert.equal(read(f).imageCatchReason, 'other');
  assert.ok(f.writes.every(({ text }) => !text.includes(PRIVATE)));
});

test('ordinary outcomes do not trigger diagnostic reads or I/O', async () => {
  const f = fixture();
  const reads = { current: 0, selection: 0, state: 0 };
  for (const key of Object.keys(reads)) f.ports[key] = () => { reads[key] += 1; return null; };
  const api = diagnostic.create(f.ports);
  for (const result of [null, undefined, { status: 'imported' }, { status: 'cancelled' }, { status: 'unconfirmed' }]) {
    assert.doesNotThrow(() => api.observe(result));
  }
  await tick();
  assert.deepEqual(reads, { current: 0, selection: 0, state: 0 }, 'ordinary outcomes never inspect gate or session ports');
  assert.equal(f.calls.paths, 0);
  assert.equal(f.writes.length, 0);
});

test('observation contains failing read ports and optional desktop I/O failures', async () => {
  for (const fail of ['current', 'info', 'selection', 'state', 'appDataDir', 'mkdir', 'writeTextFile', 'rename', 'missingTauri']) {
    const f = fixture(), boom = () => { throw new Error(PRIVATE); };
    if (fail === 'current' || fail === 'selection' || fail === 'state') f.ports[fail] = boom;
    else if (fail === 'info') f.session.info = boom;
    else if (fail === 'missingTauri') delete f.root.__TAURI__;
    else if (fail === 'appDataDir') f.root.__TAURI__.path.appDataDir = async () => { throw new Error(PRIVATE); };
    else f.root.__TAURI__.fs[fail] = async () => { throw new Error(PRIVATE); };
    const api = diagnostic.create(f.ports), result = Object.freeze({ status: 'unavailable', reason: 'no_presented_native_session' });
    assert.doesNotThrow(() => api.observe(result), fail);
    await tick();
    assert.deepEqual(result, { status: 'unavailable', reason: 'no_presented_native_session' }, fail);
    assert.ok(f.writes.every(({ text }) => !text.includes(PRIVATE)), fail);
    assert.deepEqual([f.calls.imports, f.calls.pickers, f.calls.toasts], [0, 0, 0], fail);
  }
});

test('pending writes never delay callers and coalesce the latest observation behind one writer', async () => {
  const f = fixture(), gate = deferred();
  let active = 0, maximum = 0;
  f.root.__TAURI__.fs.writeTextFile = async (path, text) => {
    active += 1;
    maximum = Math.max(maximum, active);
    f.writes.push({ path, text });
    if (f.writes.length === 1) await gate.promise;
    active -= 1;
  };
  const api = diagnostic.create(f.ports);
  assert.equal(api.observe({ status: 'unavailable', reason: 'native_image_route_absent' }), undefined);
  await tick();
  assert.equal(f.writes.length, 1, 'the first writer is held in fake I/O');
  assert.equal(api.observe({ status: 'unavailable', reason: 'picker_unavailable' }), undefined);
  f.state.routeRetired = true;
  assert.equal(api.observe({ status: 'unavailable', reason: 'retired' }), undefined);
  await tick();
  assert.equal(f.writes.length, 1, 'no concurrent writer starts while the first is pending');
  gate.resolve();
  await tick();
  assert.equal(maximum, 1);
  assert.equal(f.calls.paths, 1);
  assert.equal(f.writes.length, 2, 'intermediate pending observations are coalesced');
  assert.deepEqual(f.writes.map(({ text }) => JSON.parse(text).reason), ['native_image_route_absent', 'retired']);
  assert.equal(read(f).snapshot.routeRetired, true);
  assert.equal(f.renames.length, 2);
});

test('the desktop image catch preserves public results and UI when optional diagnostics are present, absent or broken', async () => {
  const Desktop = require('../src/js/adapters/native-import-desktop.js');
  const Raster = require('../src/js/adapters/native-object-raster-asset.js');
  const { makeSession } = require('./helpers/native-import-session.cjs');
  for (const variant of ['present', 'absent', 'create', 'observe', 'unavailable']) {
    const f = fixture(), h = makeSession(), toasts = [], picks = [];
    const invoked = { create: 0, observe: 0, unavailable: 0, dispatch: 0 };
    h.state.instanceId = PRIVATE + '-instance';
    h.state.documentId = PRIVATE + '-document';
    h.state.documentSnapshotId = PRIVATE + '-snapshot';
    Object.assign(h.session, {
      isActive: () => true, blocksLegacy: () => true, readSubscriptionGeneration: () => 1,
      runResourceCommand: () => { invoked.dispatch += 1; assert.fail('codec refusal must precede native dispatch'); },
    });
    Object.assign(f.root, {
      document: null, paper: { project: {} }, state: { currentFrame: 0, activeLayerIdx: 0, layers: [{ layerUid: 'L0' }] }, _layerSel: [0],
      SM: { t: (key) => key }, showToast: (text) => toasts.push(text),
      NemoNativeObjectProject: { desktop: () => h.session, selectLayerUI: () => assert.fail('no selection on refusal') },
      NemoNativeObjectRasterAsset: Raster,
    });
    f.root.__TAURI__.dialog.open = async (options) => { picks.push(options); return '/' + PRIVATE + '.jpg'; };
    for (const [name, file] of Object.entries({ Core: 'core', Svg: 'svg', Drop: 'drop', Figma: 'figma', Psd: 'psd', Montage: 'montage' })) {
      f.root['NemoNativeImport' + name] = require('../src/js/adapters/native-import-' + file + '.js');
    }
    if (variant !== 'absent') f.root.NemoNativeImportDiagnostic = {
      create(ports) {
        invoked.create += 1;
        if (variant === 'create') throw new Error(PRIVATE);
        const real = diagnostic.create(ports);
        return {
          observe(result) {
            invoked.observe += 1;
            if (variant === 'observe') throw new Error(PRIVATE);
            return real.observe(result);
          },
          unavailable(error) {
            invoked.unavailable += 1;
            if (variant === 'unavailable') throw new Error(PRIVATE);
            return real.unavailable(error);
          },
        };
      },
    };
    const controller = Desktop.install(f.root);
    const result = await controller.image.openFile();
    assert.deepEqual(result, { status: 'unavailable', code: 'unavailable' }, variant);
    assert.deepEqual(Reflect.ownKeys(result).sort(), ['code', 'status'], variant);
    assert.equal(controller.surface(result), result, 'surface preserves the exact returned object: ' + variant);
    await tick();
    assert.deepEqual(picks, [{ title: 'Import Image', multiple: false, filters: [{ name: 'PNG Image', extensions: ['png'] }] }], variant);
    assert.deepEqual(toasts, ['Native import is unavailable.'], variant);
    assert.equal(invoked.dispatch, 0, variant);
    assert.equal(h.calls.length, 0, 'no vector import dispatch: ' + variant);
    assert.equal(h.state.contentRevision, 7, variant);
    assert.equal(controller.image.status(), null, 'known refusal creates no uncertain commit: ' + variant);
    const active = !['absent', 'create'].includes(variant);
    assert.deepEqual([invoked.create, invoked.unavailable, invoked.observe], [variant === 'absent' ? 0 : 1, active ? 1 : 0, active ? 1 : 0], variant);
    const emits = variant === 'present' || variant === 'unavailable';
    assert.equal(f.writes.length, emits ? 1 : 0, variant);
    if (emits) {
      assert.equal(read(f).imageCatchReason, variant === 'present' ? 'image_codec_unavailable' : 'other', variant);
      assert.equal(read(f).reason, 'other', 'the catch reason stays out of the public result');
      assert.equal(f.writes[0].text.includes(PRIVATE), false, variant);
    }
  }
});
