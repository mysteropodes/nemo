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
