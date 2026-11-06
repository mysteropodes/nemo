'use strict';
// E-export-guard-rive-ae (#2357/#2375): on a native object project
// state.layers and state.cameraKeys are projections, so the Rive and
// After Effects camera exporters must refuse with the typed Lottie-style
// result instead of writing an empty or stale artboard/script.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');
const vm = require('node:vm');

const ROOT = path.join(__dirname, '..');
const read = (f) => fs.readFileSync(path.join(ROOT, f), 'utf8');

function sandbox(blocksLegacy, file) {
  const exportSrc = read('src/js/export.js');
  const guard = exportSrc.slice(exportSrc.indexOf('function exportNativeOpacity(){'), exportSrc.indexOf('\nfunction exportEnsureLayer('));
  const effects = [];
  const window = { NemoNativeOpacityCutover: { blocksLegacy: () => blocksLegacy } };
  const context = vm.createContext({
    window, effects, URL: { createObjectURL() { effects.push('blob'); return 'blob:x'; }, revokeObjectURL() {} },
    Blob: function Blob() {}, document: { createElement() { effects.push('anchor'); return { click() {} }; } },
    fetch() { effects.push('fetch'); throw new Error('network'); },
    state: { cameraLayerOn: true, cameraKeys: [{ f: 0 }], canvasW: 100, canvasH: 100 },
    exportFrameRange() { effects.push('range'); return { start: 0, end: 1 }; },
    exportPickSaveFile() { effects.push('picker'); return null; },
    exportWriteText() { effects.push('write'); },
    exportTauriAvailable() { return false; },
  });
  vm.runInContext(guard, context);
  vm.runInContext(read(file), context);
  return { exporters: window.SMExport, effects };
}

test('Rive export refuses typed on a native project without touching Rive Editor', async () => {
  const { exporters, effects } = sandbox(true, 'src/js/rive-export.js');
  const result = await exporters.exportRive({ start: 0, end: 1 });
  assert.deepEqual({ ok: result.ok, unavailable: result.unavailable }, { ok: false, unavailable: true });
  assert.equal(result.error, 'Rive export is not available for native projects yet.');
  assert.deepEqual(effects, []);
});

test('AE camera export refuses typed on a native project and writes nothing', async () => {
  const { exporters, effects } = sandbox(true, 'src/js/ae-camera-export.js');
  const result = await exporters.exportAECamera({});
  assert.deepEqual({ ok: result.ok, unavailable: result.unavailable }, { ok: false, unavailable: true });
  assert.equal(result.error, 'After Effects camera export is not available for native projects yet.');
  assert.deepEqual(effects, []);
});

test('legacy projects still reach the exporters past the guard', async () => {
  const ae = sandbox(false, 'src/js/ae-camera-export.js');
  const result = await ae.exporters.exportAECamera({});
  assert.notEqual(result.unavailable, true);
  const rive = sandbox(false, 'src/js/rive-export.js');
  await rive.exporters.exportRive({ start: 0, end: 1 }).catch(() => {});
  assert.ok(rive.effects.includes('range'), 'Rive export proceeds on a legacy project');
});

test('the export modal surfaces the refusal as the visible error line', () => {
  const t = read('src/js/timeline.js');
  assert.match(t, /else\{progEl\.textContent=SM\.t\('exportError'\)\.replace\('\{e\}',res\.error\|\|/);
});
