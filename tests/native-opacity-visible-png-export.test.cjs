'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const workflow = require('../src/js/application/native-opacity-export-workflow.js');

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
    console, Date, Math, Promise, Error, AbortController, parseFloat, setTimeout() {},
    state: { totalFrames: 21, waIn: 0, waOut: 20, fps: 24, canvasW: 320, canvasH: 180 },
    rangeSel: { value: 'all' }, fmtSel: { value: 'png' },
    ALPHA_FORMATS: ['png'], currentExportScale: () => options.scale || 1,
    progEl: { style: {}, textContent: '' }, cancelBtn: { style: {}, disabled: false,
      addEventListener: (_event, fn) => { ctx.cancel = fn; } },
    modal: { style: {} }, SM: { t: (key) => key === 'exportError' ? 'exportError: {e}' : key }, showToast() {},
    saveAllLayerFrames() { calls.push('legacy-save'); },
    document: { getElementById: (id) => elements[id] },
    exportTauriAvailable: () => options.browser !== true,
    exportFrameRange: (opts) => ({ start: opts.start, end: opts.end }),
    exportPickDir: async () => { calls.push('picker'); return '/out'; },
    runBtn: { disabled: false, addEventListener: (_event, fn) => { ctx.click = fn; } },
  };
  ctx.window = ctx;
  elements['exp-cancel'] = ctx.cancelBtn;
  if (native) ctx.NemoNativeOpacityCutover = {
    blocksLegacy: () => true,
    isActive: () => options.active !== false,
    releaseCurrent: async () => { calls.push('release'); },
    exportPng: async (_destination, frames, onProgress, signal) => {
      calls.push(['native-export', Array.from(frames)]);
      if (options.fail) throw new Error('native job failed');
      if (options.pending) return new Promise((resolve) => signal.addEventListener('abort',
        () => resolve({ status: 'cancelled' }), { once: true }));
      if (onProgress) onProgress(frames.length, frames.length);
      return { status: 'succeeded' };
    },
  };
  vm.createContext(ctx);
  vm.runInContext(
    between(exportSource, 'function exportNativeOpacity(){', '\nasync function exportReleaseNative(') + '\n' +
    between(exportSource, 'async function exportRenderPNGsToDir(', '\n// ---- Browser-compatible video export') + '\n' +
    'window.SMExport={' + between(exportSource, '  exportPNGSequence:async function(opts){', '\n  exportTIFFSequence:') + '\n};',
    ctx, { filename: 'visible-export.js' });
  vm.runInContext(between(timelineSource, "  var cancelBtn=document.getElementById('exp-cancel')", '\n})();'),
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

test('visible native cancel waits for terminal cancellation without success or legacy fallback', async () => {
  const { ctx, calls } = harness(true, { pending: true });
  const running = ctx.click();
  await new Promise(setImmediate);
  assert.equal(ctx.cancelBtn.style.display, '');
  ctx.cancel();
  await running;
  assert.deepEqual(calls.map((call) => Array.isArray(call) ? call[0] : call), ['picker', 'native-export']);
  assert.equal(ctx.progEl.textContent, 'exportCancelled');
  assert.equal(ctx.cancelBtn.style.display, 'none');
});

test('native workflow binds one pinned job and observes Rust cancellation', async () => {
  const operations = [];
  const abort = new AbortController();
  const identity = { instanceId: 'instance-a', documentId: 'document-a', contentRevision: 7 };
  const receipt = (status) => ({ jobId: 'job-a', status, progress: 0 });
  const target = { exporter: {
    begin: () => ({ operation: 'job.export.png.begin' }),
    status: () => ({ operation: 'job.export.png.status' }),
    cancel: () => ({ operation: 'job.export.png.cancel' }),
    observe: (_request, response) => ({ receipt: response }),
  } };
  const ports = { bindOutput: async (binding) => { operations.push(['bind', binding.expectedRevision]); },
    sleep: async () => { operations.push('sleep'); } };
  const application = { dispatch: async (request) => {
    operations.push(request.operation);
    if (request.operation === 'query.document.snapshot.acquire') return { snapshot: true };
    return receipt(request.operation === 'job.export.png.cancel' ? 'cancelled' : 'running');
  } };
  const result = await workflow.run(ports, { successful: (value) => value }, application,
    target, identity, { frames: [{ sourceFrame: 0 }] },
    (_current, operation) => ({ operation }), (prefix) => prefix, '/tmp/native-out', [0],
    () => abort.abort(), abort.signal);
  assert.equal(result.status, 'cancelled');
  assert.deepEqual(operations, [['bind', 7], 'query.document.snapshot.acquire',
    'job.export.png.begin', 'job.export.png.cancel']);

  const before = [];
  const alreadyAborted = new AbortController(); alreadyAborted.abort();
  const preflight = await workflow.run({ bindOutput: () => before.push('bind') }, {}, {}, {}, identity,
    { frames: [] }, () => ({}), () => 'unused', '/tmp/native-out', [], null, alreadyAborted.signal);
  assert.equal(preflight.status, 'cancelled');
  assert.deepEqual(before, []);
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
