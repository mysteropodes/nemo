'use strict';
// CS4/#2343 — on the desktop remediation host a legacy JavaScript/Paper write is
// never implicit, and an export never releases native authority to the legacy
// writer (EXECUTION_PLAN §0). Browser behaviour is unchanged (R24/#1619).
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');
const vm = require('node:vm');

const LegacySurface = require('../src/js/adapters/native-opacity-legacy-surface.js');

const ROOT = path.join(__dirname, '..');
const KINDS = ['create-layer', 'document-import', 'frame-history-checkpoint', 'layer-reorder', 'script-run'];

function desktopRoot(extra) {
  return Object.assign({ __TAURI__: { core: { invoke() {} } } }, extra);
}

test('desktop host: no admission and no central policy denies every legacy write kind', () => {
  const root = desktopRoot();
  for (const kind of KINDS) {
    assert.equal(LegacySurface.allowLegacyWrite(root, kind), false, kind);
    assert.throws(() => LegacySurface.requireLegacyWrite(root, kind), { name: 'NemoNativeReleaseRequired' }, kind);
  }
});

test('browser host keeps its read-side import admission and denies edits', () => {
  const root = {};
  assert.equal(LegacySurface.allowLegacyWrite(root, 'document-import'), true);
  assert.equal(LegacySurface.allowLegacyWrite(root, 'frame-history-checkpoint'), false);
});

test('an installed admission or central policy still decides first', () => {
  const allowAll = desktopRoot({ NemoNativeOpacityLegacyAdmission: { allow: () => true } });
  assert.equal(LegacySurface.allowLegacyWrite(allowAll, 'create-layer'), true);
  assert.equal(LegacySurface.allowLegacyWrite(desktopRoot(), 'create-layer', () => false), false);
});

test('desktop surface.allow denies even when the edit guard has seen no native identity', () => {
  const guardCalls = [];
  const guard = { allow(kind) { guardCalls.push(kind); return true; } };
  // desktopPorts is the desktop-only port set; its surface.allow gates every
  // wrapped writer (native-opacity-operations allowLegacy).
  const surface = LegacySurface.desktopPorts(desktopRoot({ SM: { importJSON() {} }, SMEngineBridge: { nativeEditGuard: guard } }), {}).surface;
  assert.equal(surface.allow('script-run'), false);
  assert.deepEqual(guardCalls, ['script-run'], 'the guard is still consulted first');
  const denying = { allow() { return false; } };
  assert.equal(LegacySurface.desktopPorts(desktopRoot({ SM: { importJSON() {} }, SMEngineBridge: { nativeEditGuard: denying } }), {})
    .surface.allow('script-run'), false);
});

function exportSandbox(blocksLegacy) {
  const source = fs.readFileSync(path.join(ROOT, 'src/js/export.js'), 'utf8');
  const start = source.indexOf('function exportNativeOpacity(){');
  const end = source.indexOf('\nfunction exportEnsureLayer(');
  const releases = [];
  const window = { NemoNativeOpacityCutover: {
    blocksLegacy: () => blocksLegacy, releaseCurrent: (kind) => { releases.push(kind); return null; } } };
  const context = vm.createContext({ window });
  vm.runInContext(source.slice(start, end), context);
  return { context, releases };
}

test('Lottie refuses with a typed unavailable result instead of releasing native authority', () => {
  const { context, releases } = exportSandbox(true);
  const result = context.exportNativeUnavailable('Lottie');
  assert.equal(result.ok, false);
  assert.equal(result.unavailable, true);
  assert.match(result.error, /Lottie export is not available for native projects yet/);
  assert.deepEqual(releases, []);
  assert.equal(exportSandbox(false).context.exportNativeUnavailable('Lottie'), null);
});

test('the SVG workflow release port refuses rather than releases under native authority', async () => {
  const { context, releases } = exportSandbox(true);
  await assert.rejects(context.exportRefuseRelease('svg-export'), /cannot release native authority/);
  assert.deepEqual(releases, []);
  await exportSandbox(false).context.exportRefuseRelease('svg-export');
});

test('export.js no longer calls releaseCurrent on behalf of an export', () => {
  const source = fs.readFileSync(path.join(ROOT, 'src/js/export.js'), 'utf8');
  assert.equal(source.includes('releaseCurrent('), false);
  assert.equal(source.includes('exportReleaseNative'), false);
  assert.equal(source.split("exportNativeUnavailable('Lottie')").length - 1, 2, 'both Lottie fronts refuse');
});

// Boot level: the production surface script and app.js's own scaffold creation
// run in a desktop realm before any admission exists (index.html load order).
function desktopBootRealm() {
  const realm = vm.createContext({ __TAURI__: { core: { invoke() {} } } });
  realm.window = realm;
  vm.runInContext(fs.readFileSync(path.join(ROOT, 'src/js/adapters/native-opacity-legacy-surface.js'), 'utf8'), realm);
  const app = fs.readFileSync(path.join(ROOT, 'src/js/app.js'), 'utf8');
  const start = app.indexOf('function createUserLayer(name,kind){');
  const boot = "createUserLayer('Layer 1','boot-scaffold');";
  const end = app.indexOf(boot, start) + boot.length;
  assert.ok(start > 0 && end > start, 'app.js keeps the scaffold function and its boot call together');
  Object.assign(realm, {
    arcLayer: { activate() {} }, userLayers: [],
    state: { layers: [], totalFrames: 3 }, nextLayerColor: () => '#5FA875',
    Layer: class { constructor(opts) { this.name = opts.name; } insertBelow() {} },
  });
  vm.runInContext(app.slice(start, end), realm, { filename: 'app.js#boot-scaffold' });
  return realm;
}

test('desktop boot creates its single scaffold layer before any admission is installed', () => {
  const realm = desktopBootRealm();
  assert.equal(realm.userLayers.length, 1);
  assert.equal(realm.state.layers.length, 1);
  assert.equal(realm.state.layers[0].name, 'Layer 1');
});

test('after the boot scaffold, desktop layer creation and a second scaffold both refuse', () => {
  const realm = desktopBootRealm();
  for (const call of ["createUserLayer('Layer 2')", "createUserLayer('Layer 2','boot-scaffold')"]) {
    assert.throws(() => vm.runInContext(call, realm), (error) => error.name === 'NemoNativeReleaseRequired', call);
  }
  assert.equal(realm.userLayers.length, 1, 'no second Paper group');
  assert.equal(realm.state.layers.length, 1, 'no second layer row');
});

test('browser boot keeps creating its scaffold layer (R24 behaviour unchanged)', () => {
  assert.equal(LegacySurface.allowLegacyWrite({}, 'boot-scaffold'), true);
  assert.equal(LegacySurface.allowLegacyWrite({}, 'create-layer'), true);
});
