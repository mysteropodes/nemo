'use strict';
// P18 — bounded export job around the SVG sequence exporter (H01 port).
//
// Two halves. The job half drives application/export-job.js through fake
// ports: a fake document whose fingerprint and content the tests mutate at
// chosen moments (inside a write await, between batches, never during a
// batch — the single-threaded runtime cannot), a recording filesystem, and
// the frame evaluator reading the fake document. The binding half slices the
// REAL export.js binding (`exportSVGSequenceToDir` and the helpers it calls)
// out of the source and runs it beside the real job module, so the
// production caller is exercised, not a reimplementation.

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const root = path.resolve(__dirname, '..');
const jobPath = path.join(root, 'src/js/application/export-job.js');
const NemoExportJob = require(jobPath);
const jobSource = fs.readFileSync(jobPath, 'utf8');
const exportSource = fs.readFileSync(path.join(root, 'src/js/export.js'), 'utf8');
const adapterPath = path.join(root, 'src/js/adapters/export-svg-sequence.js');
const adapterSource = fs.readFileSync(adapterPath, 'utf8');

// ---- fake document + filesystem ------------------------------------------

function harness(opts) {
  opts = opts || {};
  const doc = { documentId: 'doc-1', revision: 0, content: 'A' };
  const fsLog = { mkdir: [], writes: [], removed: [], removedDirs: [], files: new Map() };
  const existing = new Set(opts.existingDirs || []);
  let ids = 0;
  const hooks = { onWrite: null, onEvaluate: null };
  const ports = {
    newId() { return 'job-' + (++ids); },
    fingerprint() { return { documentId: doc.documentId, key: doc.revision + '|' + doc.content }; },
    beforeCapture() { fsLog.captured = (fsLog.captured || 0) + 1; },
    evaluateFrame(f) {
      if (hooks.onEvaluate) hooks.onEvaluate(f);
      return '<svg frame="' + f + '" content="' + doc.content + '"/>';
    },
    frameName(i) { return 'frame_' + String(i).padStart(4, '0') + '.svg'; },
    async mkdir(dir) { fsLog.mkdir.push(dir); const created = !existing.has(dir); existing.add(dir); return { created }; },
    async write(p, text) {
      if (hooks.onWrite) await hooks.onWrite(p, text);
      fsLog.writes.push(p); fsLog.files.set(p, text);
    },
    async remove(names, dir, createdDir) {
      for (const n of names) { fsLog.removed.push(dir + '/' + n); fsLog.files.delete(dir + '/' + n); }
      if (createdDir) fsLog.removedDirs.push(dir);
    },
  };
  return { doc, fsLog, hooks, ports, job: NemoExportJob.create(ports) };
}

function expected(f, content) { return '<svg frame="' + f + '" content="' + content + '"/>'; }

// ---- T1: identity, artifact, bounded progress, monotonic terminal ----------

test('T1: a three-frame export succeeds with a stable jobId, exact artifact files and bounded progress', async () => {
  const h = harness();
  const progress = [];
  const begun = h.job.begin({ dir: '/out', start: 0, end: 2, requestId: 'req-1', onProgress: (i, n) => progress.push([i, n]) });
  assert.equal(begun.ok, true);
  assert.equal(begun.job.status, 'running');
  assert.equal(begun.job.evaluated, 3, 'the whole first batch is evaluated synchronously inside begin()');
  assert.equal(h.fsLog.captured, 1, 'beforeCapture runs once, before evaluation');
  assert.equal(h.job.status(begun.jobId).job.jobId, begun.jobId);
  const terminal = await h.job.done(begun.jobId);
  assert.equal(terminal.status, 'succeeded');
  assert.deepEqual(terminal.artifact, { dir: '/out', files: ['frame_0001.svg', 'frame_0002.svg', 'frame_0003.svg'] });
  assert.equal(terminal.progress, 1);
  assert.deepEqual(progress, [[1, 3], [2, 3], [3, 3]]);
  for (const [i, f] of [[1, 0], [2, 1], [3, 2]]) assert.equal(h.fsLog.files.get('/out/frame_000' + i + '.svg'), expected(f, 'A'));
  assert.deepEqual(h.fsLog.mkdir, ['/out']);
  // Terminal is monotonic: status is stable and cancel after the end changes nothing.
  assert.equal(h.job.status(begun.jobId).job.status, 'succeeded');
  assert.equal(h.job.cancel(begun.jobId).job.status, 'succeeded');
  assert.equal(h.job.status(begun.jobId).job.status, 'succeeded');
  assert.equal(h.job.meta().running, null);
});

test('T1b: invalid inputs are rejected before anything is captured or written', () => {
  const h = harness();
  assert.equal(h.job.begin({ start: 0, end: 1 }).error.code, 'invalid_input');
  assert.equal(h.job.begin({ dir: '/out', start: 2, end: 1 }).error.code, 'invalid_range');
  assert.equal(h.job.begin({ dir: '/out', start: -1, end: 1 }).error.code, 'invalid_range');
  assert.equal(h.job.begin({ dir: '/out', start: 0.5, end: 1 }).error.code, 'invalid_range');
  assert.equal(h.job.status('nope').error.code, 'unknown_job');
  assert.equal(h.job.cancel('nope').error.code, 'unknown_job');
  assert.equal(h.fsLog.captured, undefined);
  assert.deepEqual(h.fsLog.writes, []);
});

// ---- T2: revision isolation, document change / replacement, cleanup --------

test('T2a: an edit that lands during a write cannot reach frames already evaluated in the batch', async () => {
  const h = harness();
  // Default batching: all three frames are evaluated before the first await.
  h.hooks.onWrite = async () => { h.doc.content = 'B'; h.doc.revision++; };
  const begun = h.job.begin({ dir: '/out', start: 0, end: 2 });
  const terminal = await h.job.done(begun.jobId);
  assert.equal(terminal.status, 'succeeded');
  for (const [i, f] of [[1, 0], [2, 1], [3, 2]]) assert.equal(h.fsLog.files.get('/out/frame_000' + i + '.svg'), expected(f, 'A'), 'every file reflects the begin-time document');
});

test('T2b: a same-document edit between batches ends the job failed{document_changed} and removes the partial files', async () => {
  const h = harness();
  // Force one frame per batch so the between-batch check is exercised.
  let writes = 0;
  h.hooks.onWrite = async () => { if (++writes === 1) { h.doc.content = 'B'; h.doc.revision++; } };
  const begun = h.job.begin({ dir: '/out', start: 0, end: 2, batchFrames: 1 });
  assert.equal(begun.job.evaluated, 1);
  const terminal = await h.job.done(begun.jobId);
  assert.equal(terminal.status, 'failed');
  assert.equal(terminal.error.code, 'document_changed');
  assert.deepEqual(terminal.partial, { written: ['frame_0001.svg'], kept: false });
  assert.equal(terminal.artifact, undefined, 'partial output is never labelled an artifact');
  assert.deepEqual(h.fsLog.removed, ['/out/frame_0001.svg']);
  assert.deepEqual(h.fsLog.removedDirs, ['/out'], 'the job created /out, so it removes it');
  assert.deepEqual(terminal.cleanup, { removed: 1, dir: true });
  assert.equal(h.fsLog.files.size, 0);
});

test('T2c: a document replacement between batches ends the job cancelled{document_replaced}; a pre-existing directory is kept', async () => {
  const h = harness({ existingDirs: ['/out'] });
  let writes = 0;
  h.hooks.onWrite = async () => { if (++writes === 1) { h.doc.documentId = 'doc-2'; h.doc.revision = 0; } };
  const begun = h.job.begin({ dir: '/out', start: 0, end: 2, batchFrames: 1 });
  const terminal = await h.job.done(begun.jobId);
  assert.equal(terminal.status, 'cancelled');
  assert.equal(terminal.error.code, 'document_replaced');
  assert.deepEqual(h.fsLog.removed, ['/out/frame_0001.svg']);
  assert.deepEqual(h.fsLog.removedDirs, [], 'a directory the job did not create is never removed');
  assert.deepEqual(terminal.cleanup, { removed: 1, dir: false });
});

test('T2d: a replacement during the LAST write cannot commit a late result', async () => {
  const h = harness();
  let writes = 0;
  h.hooks.onWrite = async () => { if (++writes === 3) h.doc.documentId = 'doc-2'; };
  const begun = h.job.begin({ dir: '/out', start: 0, end: 2 });
  const terminal = await h.job.done(begun.jobId);
  assert.equal(terminal.status, 'cancelled');
  assert.equal(terminal.error.code, 'document_replaced');
  assert.equal(h.fsLog.files.size, 0, 'all three written files are removed');
});

test('T2e: keepPartial leaves the partial files in place and reports them', async () => {
  const h = harness();
  let writes = 0;
  h.hooks.onWrite = async () => { if (++writes === 1) { h.doc.revision++; } };
  const begun = h.job.begin({ dir: '/out', start: 0, end: 2, batchFrames: 1, keepPartial: true });
  const terminal = await h.job.done(begun.jobId);
  assert.equal(terminal.status, 'failed');
  assert.deepEqual(terminal.partial, { written: ['frame_0001.svg'], kept: true });
  assert.deepEqual(h.fsLog.removed, []);
  assert.equal(h.fsLog.files.size, 1);
});

// ---- T3: cancel, busy, retry, failures -------------------------------------

test('T3a: cancel after the first write ends cancelled with the partial file removed', async () => {
  const h = harness();
  let writes = 0;
  const begun = h.job.begin({ dir: '/out', start: 0, end: 2, batchFrames: 1 });
  h.hooks.onWrite = async () => { if (++writes === 1) { const c = h.job.cancel(begun.jobId); assert.equal(c.job.status, 'running', 'cancel is a request; the status flips when the job observes it'); } };
  const terminal = await h.job.done(begun.jobId);
  assert.equal(terminal.status, 'cancelled');
  assert.equal(terminal.error, undefined);
  assert.deepEqual(terminal.partial, { written: ['frame_0001.svg'], kept: false });
  assert.deepEqual(h.fsLog.removed, ['/out/frame_0001.svg']);
  assert.equal(h.job.meta().running, null);
});

test('T3b: a second begin while a job runs is refused with busy; the same requestId returns the existing job instead of a duplicate', async () => {
  const h = harness();
  const a = h.job.begin({ dir: '/out', start: 0, end: 2, requestId: 'r1' });
  const b = h.job.begin({ dir: '/other', start: 0, end: 2 });
  assert.equal(b.ok, false); assert.equal(b.error.code, 'busy');
  const retry = h.job.begin({ dir: '/out', start: 0, end: 2, requestId: 'r1' });
  assert.equal(retry.ok, true); assert.equal(retry.jobId, a.jobId); assert.equal(retry.retried, true);
  const terminal = await h.job.done(a.jobId);
  assert.equal(terminal.status, 'succeeded');
  // After the terminal state a retry with the same requestId still returns the same job: no second artifact.
  const again = h.job.begin({ dir: '/out', start: 0, end: 2, requestId: 'r1' });
  assert.equal(again.jobId, a.jobId); assert.equal(again.job.status, 'succeeded');
  assert.equal(h.fsLog.writes.length, 3, 'exactly one set of files was written');
  assert.equal(h.job.meta().jobs, 1);
});

test('T3c: a write failure ends failed{write_failed} with cleanup; an evaluator failure in begin is terminal at once', async () => {
  const h = harness();
  let writes = 0;
  h.hooks.onWrite = async () => { if (++writes === 2) throw new Error('disk full'); };
  const begun = h.job.begin({ dir: '/out', start: 0, end: 2 });
  const terminal = await h.job.done(begun.jobId);
  assert.equal(terminal.status, 'failed');
  assert.deepEqual(terminal.error, { code: 'write_failed', message: 'disk full' });
  assert.deepEqual(h.fsLog.removed, ['/out/frame_0001.svg']);

  const h2 = harness();
  h2.hooks.onEvaluate = () => { throw new Error('no such frame'); };
  const b2 = h2.job.begin({ dir: '/out', start: 0, end: 2 });
  assert.equal(b2.ok, true);
  assert.equal(b2.job.status, 'failed');
  assert.deepEqual(b2.job.error, { code: 'evaluate_failed', message: 'no such frame' });
  assert.deepEqual(h2.fsLog.mkdir, [], 'nothing touched the filesystem');
  assert.equal((await h2.job.done(b2.jobId)).status, 'failed');
  assert.equal(h2.job.meta().running, null, 'the slot is released');
});

test('T3d: a remove port that throws synchronously still settles the job as a terminal failure with the cleanup error recorded', async () => {
  const h = harness();
  let writes = 0;
  h.hooks.onWrite = async () => { if (++writes === 2) throw new Error('disk full'); };
  h.ports.remove = () => { throw new Error('rm exploded'); };
  const begun = h.job.begin({ dir: '/out', start: 0, end: 2 });
  const terminal = await h.job.done(begun.jobId);
  assert.equal(terminal.status, 'failed');
  assert.equal(terminal.error.code, 'write_failed');
  assert.deepEqual(terminal.cleanup, { removed: 0, dir: true, error: 'rm exploded' });
  assert.equal(h.job.meta().running, null);
});

// ---- T4: the real export.js binding ------------------------------------------

function slice(source, startMarker, endMarker, label) {
  const start = source.indexOf(startMarker);
  assert.notEqual(start, -1, `${label}: start marker not found — the source moved`);
  const end = source.indexOf(endMarker, start + startMarker.length);
  assert.notEqual(end, -1, `${label}: end marker not found — the source moved`);
  return source.slice(start, end);
}

// The real adapter (fingerprint, tracked mkdir, remove, result mapping) plus
// the real export.js binding sliced from source: `exportSvgSequenceJob` and
// `exportSVGSequenceToDir`, with the real exportFrameRange and pad4 helpers.
// Objects born inside the vm context carry that context's Object prototype;
// compare their JSON shape.
function plain(x) { return JSON.parse(JSON.stringify(x)); }

function bindingContext(stubs) {
  const context = Object.assign({
    console, JSON, Math, Date, Object, Array, String, Number, Promise, Error,
    state: { waIn: 0, waOut: 2, canvasW: 320, canvasH: 180, layers: [{}], activeSymbolId: null, activeMontageViewId: null },
    _sceneVersion: 7,
    saveAllLayerFrames() { context.saved = (context.saved || 0) + 1; },
    NemoOpacityApplication: { meta() { return { documentId: 'doc-1', revision: 3 }; } },
    exportTauriAvailable() { return true; },
    exportFrameSVGString(f) { return '<svg f="' + f + '"/>'; },
  }, stubs);
  context.window = context;
  vm.createContext(context);
  vm.runInContext(jobSource, context, { filename: 'export-job.js' });
  vm.runInContext(adapterSource, context, { filename: 'export-svg-sequence.js' });
  const binding = slice(exportSource, 'var _svgSequenceJob=null;', '\nasync function exportGIFToPath(', 'P18 binding');
  const range = slice(exportSource, 'function exportFrameRange(opts){', '\n// ---- Tauri fs/dialog/shell helpers ----', 'exportFrameRange');
  const pad = slice(exportSource, 'function pad4(n){', '\n', 'pad4');
  vm.runInContext(binding + '\n' + range + '\n' + pad, context, { filename: 'export-binding.js' });
  return context;
}

test('T4a: the production exportSVGSequenceToDir runs through the job and keeps its result shape', async () => {
  const files = new Map(); const removed = [];
  const ctx = bindingContext({
    exportTauri() { return { fs: {
      async exists(p) { return p === '/existing'; },
      async remove(p) { removed.push(p); files.delete(p); },
    } }; },
    async exportMkdir(dir) { ctx.made = (ctx.made || []).concat(dir); },
    async exportRemoveDir(dir) { removed.push(dir + '/'); },
    async exportWriteText(p, text) { files.set(p, text); },
  });
  const progress = [];
  const res = await ctx.exportSVGSequenceToDir('/seq', { start: 0, end: 2, onProgress: (i, n) => progress.push([i, n]) });
  assert.deepEqual(plain(res), { ok: true, dir: '/seq' });
  assert.equal(ctx.saved, 1, 'saveAllLayerFrames runs once at begin');
  assert.deepEqual(ctx.made, ['/seq']);
  assert.deepEqual([...files.keys()], ['/seq/frame_0001.svg', '/seq/frame_0002.svg', '/seq/frame_0003.svg']);
  assert.equal(files.get('/seq/frame_0002.svg'), '<svg f="1"/>');
  assert.deepEqual(progress, [[1, 3], [2, 3], [3, 3]]);
  assert.deepEqual(removed, []);
  // The same service object is exposed for P19 and reports the finished job.
  const job = ctx.exportSvgSequenceJob();
  assert.equal(job.meta().jobs, 1); assert.equal(job.meta().running, null);
  // Range fallback: no start/end → state.waIn/waOut (unchanged behaviour).
  const res2 = await ctx.exportSVGSequenceToDir('/seq2', {});
  assert.deepEqual(plain(res2), { ok: true, dir: '/seq2' });
  assert.equal(files.size, 6);
});

test('T4b: the production binding maps document changes and cancellation to the callers\' existing shapes and cleans up', async () => {
  const files = new Map(); const removed = [];
  let writes = 0;
  const ctx = bindingContext({
    exportTauri() { return { fs: { async exists() { return false; }, async remove(p) { removed.push(p); files.delete(p); } } }; },
    async exportMkdir() {},
    async exportRemoveDir(dir) { removed.push(dir + '/'); },
    async exportWriteText(p, text) { files.set(p, text); if (++writes === 1) ctx._sceneVersion++; },
  });
  // batchFrames is not exposed by the binding (callers get the default 24), so
  // drive the document change through the default path with a range longer
  // than one batch: 25 frames → the between-batch check runs once.
  const res = await ctx.exportSVGSequenceToDir('/seq', { start: 0, end: 24 });
  assert.deepEqual(plain(res), { ok: false, error: 'The document changed during the export (between batches); partial output is not a valid sequence.' });
  assert.equal(files.size, 0, 'the 24 partial files are removed');
  assert.equal(removed.filter((p) => p.endsWith('/')).length, 1, 'the created directory is removed');

  // Cancellation through the exposed job maps to {cancelled:true}.
  const files2 = new Map();
  const ctx2 = bindingContext({
    exportTauri() { return { fs: { async exists() { return false; }, async remove(p) { files2.delete(p); } } }; },
    async exportMkdir() {}, async exportRemoveDir() {},
    async exportWriteText(p, text) { files2.set(p, text); if (files2.size === 1) ctx2.exportSvgSequenceJob().cancel(ctx2.exportSvgSequenceJob().meta().running); },
  });
  const res2 = await ctx2.exportSVGSequenceToDir('/seq', { start: 0, end: 2 });
  assert.deepEqual(plain(res2), { cancelled: true });
  assert.equal(files2.size, 0);

  // Outside Tauri the binding keeps today's desktop-only refusal, untouched.
  const ctx3 = bindingContext({ exportTauriAvailable() { return false; } });
  const res3 = await ctx3.exportSVGSequenceToDir('/seq', { start: 0, end: 2 });
  assert.equal(res3.ok, false); assert.match(res3.error, /app Nemo/);
});

test('routing guard: export.js has exactly one SVG-sequence loop, inside the job binding', () => {
  const dialog = slice(exportSource, '  exportSVGSequence:async function(opts){', '\n  exportPNGSequence:', 'SMExport.exportSVGSequence');
  assert.match(dialog, /return await exportSVGSequenceToDir\(dir,opts\);/);
  assert.doesNotMatch(dialog, /for\(var f=r\.start/, 'the dialog path no longer has its own frame loop');
  assert.equal(exportSource.split("'frame_'+pad4(i)+'.svg'").length, 2, 'one place names the sequence files');
  assert.doesNotMatch(exportSource, /NemoExportJob\b/, 'export.js reaches the job only through the adapter');
  assert.doesNotMatch(exportSource, /_sceneVersion|documentFingerprint/, 'the fingerprint lives in the adapter, not the legacy file');
  const jobCode = jobSource.split('\n').filter((l) => !/^\s*\/\//.test(l)).join('\n');
  assert.doesNotMatch(jobCode, /\bwindow\.|\bstate\.|\bdocument\.|\bSM[A-Z]/, 'the job module reads no globals');
});
