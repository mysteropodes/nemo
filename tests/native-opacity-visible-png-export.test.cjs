'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const root = path.resolve(__dirname, '..');
const exportSource = fs.readFileSync(path.join(root, 'src/js/export.js'), 'utf8');
const timelineSource = fs.readFileSync(path.join(root, 'src/js/timeline.js'), 'utf8');

function between(source, first, last) {
  const start = source.indexOf(first);
  assert.notEqual(start, -1, `missing ${first}`);
  const end = source.indexOf(last, start + first.length);
  assert.notEqual(end, -1, `missing ${last}`);
  return source.slice(start, end);
}

function harness(native, options = {}) {
  const calls = [];
  const elements = { 'exp-alpha': { checked: !!options.alpha },
    'exp-include-shadow': { checked: false } };
  const ctx = {
    console, Date, Math, Promise, Error, parseFloat, setTimeout() {},
    state: { totalFrames: 21, waIn: 0, waOut: 20, fps: 24, canvasW: 320, canvasH: 180 },
    rangeSel: { value: 'all' }, fmtSel: { value: 'png' },
    ALPHA_FORMATS: ['png'], currentExportScale: () => options.scale || 1,
    progEl: { style: {}, textContent: '' }, cancelBtn: { style: {}, disabled: false },
    modal: { style: {} }, SM: { t: (key) => key === 'exportError' ? 'exportError: {e}' : key }, showToast() {},
    saveAllLayerFrames() { calls.push('legacy-save'); },
    document: { getElementById: (id) => elements[id] },
    exportTauriAvailable: () => options.browser !== true,
    exportFrameRange: (opts) => ({ start: opts.start, end: opts.end }),
    exportPickDir: async () => { calls.push('picker'); return '/out'; },
    runBtn: { disabled: false, addEventListener: (_event, fn) => { ctx.click = fn; } },
  };
  ctx.window = ctx;
  if (native) ctx.NemoNativeOpacityCutover = {
    blocksLegacy: () => true,
    isActive: () => options.active !== false,
    releaseCurrent: async () => { calls.push('release'); },
    exportPng: async (_destination, frames, onProgress) => {
      calls.push(['native-export', Array.from(frames)]);
      if (options.fail) throw new Error('native job failed');
      if (onProgress) onProgress(frames.length, frames.length);
    },
  };
  vm.createContext(ctx);
  vm.runInContext(
    between(exportSource, 'function exportNativeOpacity(){', '\nasync function exportReleaseNative(') + '\n' +
    between(exportSource, 'async function exportRenderPNGsToDir(', '\n// ---- Browser-compatible video export') + '\n' +
    'window.SMExport={' + between(exportSource, '  exportPNGSequence:async function(opts){', '\n  exportTIFFSequence:') + '\n};',
    ctx, { filename: 'visible-export.js' });
  vm.runInContext(between(timelineSource, "  runBtn.addEventListener('click',async function(){", '\n})();'),
    ctx, { filename: 'visible-export-dialog.js' });
  return { ctx, calls };
}

test('visible native PNG command exports all 21 frames without a Paper save or release', async () => {
  const { ctx, calls } = harness(true);
  await ctx.click();
  assert.deepEqual(calls.map((call) => Array.isArray(call) ? [call[0], call[1].length] : call),
    ['picker', ['native-export', 21]]);
  assert.deepEqual(calls[1][1], Array.from({ length: 21 }, (_, i) => i));
  assert.equal(ctx.progEl.textContent, 'exportDone');
});

test('native scaled and transparent PNG requests reject before picker or legacy release', async () => {
  for (const options of [{ scale: 2 }, { alpha: true }]) {
    const { ctx, calls } = harness(true, options);
    await ctx.click();
    assert.deepEqual(calls, []);
    assert.match(ctx.progEl.textContent, /supports only 1×/);
  }
});

test('native failure and indeterminate authority never start a second writer', async () => {
  for (const options of [{ fail: true }, { active: false }]) {
    const { ctx, calls } = harness(true, options);
    await ctx.click();
    assert.equal(calls.includes('legacy-save'), false);
    assert.equal(calls.includes('release'), false);
    assert.match(ctx.progEl.textContent, /exportError/);
  }
});

test('legacy-owned command retains its frame-save path; browser native export is unavailable', async () => {
  const legacy = harness(false);
  legacy.ctx.SMExport.exportPNGSequence = async () => ({ ok: true });
  await legacy.ctx.click();
  assert.deepEqual(legacy.calls, ['legacy-save']);

  const browser = harness(true, { browser: true });
  await browser.ctx.click();
  assert.deepEqual(browser.calls, []);
  assert.match(browser.ctx.progEl.textContent, /preview navigateur/);
});
