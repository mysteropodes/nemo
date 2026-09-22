'use strict';
// P19/#1021 — the export UI and an MCP client drive the SAME job.
//
// The claim under test is not "both paths export SVG" (they already did); it
// is that there is ONE session object, so the two paths cannot drift. What
// makes that checkable rather than a matter of reading the code: cross-path
// effects. An MCP `start` while the export dialog is exporting is refused
// `busy`, and an MCP `cancel` carrying the DIALOG's jobId really stops the
// dialog's export and removes its partial files. Two exporters could not do
// either.
//
// Everything below runs the real sources — application/export-job.js and
// adapters/export-svg-sequence.js as modules, the export.js binding and the
// timeline.js dialog block sliced out of their files, render-manager.js and
// bootstrap/opacity-application.js loaded whole into a vm context. The only
// stubs are the frame evaluator, the filesystem and the DOM.

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const root = path.resolve(__dirname, '..');
const src = (...p) => path.join(root, 'src/js', ...p);
const read = (p) => fs.readFileSync(p, 'utf8');

const jobSource = read(src('application/export-job.js'));
const adapterSource = read(src('adapters/export-svg-sequence.js'));
const exportSource = read(src('export.js'));
const timelineSource = read(src('timeline.js'));
const renderManagerSource = read(src('render-manager.js'));
const CAPABILITY_JSON = JSON.parse(read(path.join(root, 'engineering/application/capabilities/export-job.json')));

function slice(source, startMarker, endMarker, label) {
  const start = source.indexOf(startMarker);
  assert.notEqual(start, -1, `${label}: start marker not found — the source moved`);
  const end = source.indexOf(endMarker, start + startMarker.length);
  assert.notEqual(end, -1, `${label}: end marker not found — the source moved`);
  return source.slice(start, end);
}

// Objects born inside a vm context carry that context's prototypes; compare
// their JSON shape rather than their identity.
function plain(x) { return JSON.parse(JSON.stringify(x)); }

const IDENTITY = { instanceId: 'inst-1', documentId: 'doc-1', revision: 3 };

// ---- one context: the real job, the real adapter, the real export.js binding

function sessionContext(stubs) {
  const files = new Map();
  const removed = [];
  const ctx = Object.assign({
    console, JSON, Math, Date, Object, Array, String, Number, Promise, Error, Boolean,
    state: { waIn: 0, waOut: 2, canvasW: 320, canvasH: 180, layers: [{}], activeSymbolId: null, activeMontageViewId: null },
    _sceneVersion: 7,
    saveAllLayerFrames() { ctx.saved = (ctx.saved || 0) + 1; },
    NemoOpacityApplication: { meta() { return { documentId: 'doc-1', revision: 3 }; } },
    exportTauriAvailable() { return true; },
    exportFrameSVGString(f) { return '<svg f="' + f + '"/>'; },
    exportTauri() { return { fs: { async exists() { return false; }, async remove(p) { removed.push(p); files.delete(p); } } }; },
    async exportMkdir() {},
    async exportRemoveDir(dir) { removed.push(dir + '/'); },
    async exportWriteText(p, text) { files.set(p, text); },
  }, stubs);
  ctx.window = ctx;
  ctx.files = files;
  ctx.removed = removed;
  vm.createContext(ctx);
  vm.runInContext(jobSource, ctx, { filename: 'export-job.js' });
  vm.runInContext(adapterSource, ctx, { filename: 'export-svg-sequence.js' });
  vm.runInContext(
    slice(exportSource, 'var _svgSequenceJob=null;', '\nasync function exportGIFToPath(', 'P18 binding') + '\n'
    + slice(exportSource, 'function exportFrameRange(opts){', '\n// ---- Tauri fs/dialog/shell helpers ----', 'exportFrameRange') + '\n'
    + slice(exportSource, 'function pad4(n){', '\n', 'pad4'),
    ctx, { filename: 'export-binding.js' });
  ctx.session = ctx.exportSvgSequenceJob();
  ctx.mcp = ctx.NemoExportSvgSequence.capability({ session: () => ctx.session, meta: () => IDENTITY });
  return ctx;
}

function request(operation, payload, extra) {
  return Object.assign({ apiVersion: 1, requestId: 'req-' + operation, operation, payload }, extra || {});
}

// ---------------------------------------------------------------------------
// A. The MCP stage contract, served by the real session
// ---------------------------------------------------------------------------

test('A1: start/status return the declared output shape and the frame the evaluator produced', async () => {
  const ctx = sessionContext();
  const started = plain(ctx.mcp.handler(request('start', { frameIdx: 4 })));
  assert.equal(started.ok, true, JSON.stringify(started));
  assert.equal(started.apiVersion, 1);
  assert.equal(started.requestId, 'req-start');
  // The envelope carries the application's identity, not a second one invented
  // by the export capability.
  assert.equal(started.instanceId, IDENTITY.instanceId);
  assert.equal(started.documentId, IDENTITY.documentId);
  assert.equal(started.revision, IDENTITY.revision);
  assert.equal(started.result.status, 'running');
  assert.equal(started.result.progress, 0);
  assert.equal(typeof started.result.jobId, 'string');
  // `output.additionalProperties:false` in the descriptor is a real promise:
  // the job's own view also carries evaluated/written/total, which must not leak.
  assert.deepEqual(Object.keys(started.result).sort(), ['jobId', 'progress', 'status']);
  assert.equal(ctx.saved, 1, 'the MCP path syncs the document exactly like the UI path');

  await ctx.session.done(started.result.jobId);
  const done = plain(ctx.mcp.handler(request('status', { jobId: started.result.jobId })));
  assert.equal(done.result.status, 'succeeded');
  assert.equal(done.result.progress, 1);
  assert.deepEqual(done.result.artifact, { mimeType: 'image/svg+xml', data: '<svg f="4"/>' });
  assert.deepEqual(Object.keys(done.result).sort(), ['artifact', 'jobId', 'progress', 'status']);
  // `effects.scope: "none"` — the inline stage writes nothing to the filesystem.
  assert.equal(ctx.files.size, 0);
  assert.deepEqual(ctx.removed, []);
});

test('A2: cancel is a request whose terminal state lands on the next status, and yields no artifact', async () => {
  const ctx = sessionContext();
  const started = plain(ctx.mcp.handler(request('start', { frameIdx: 1 })));
  const jobId = started.result.jobId;
  const cancelled = plain(ctx.mcp.handler(request('cancel', { jobId })));
  assert.equal(cancelled.result.jobId, jobId);
  // Deliberately still `running`: export-job.js applies cancellation at the
  // next batch boundary, and freeing the single-tenant `running` slot here
  // would let a second export start while this one is mid-write. This is the
  // contract the descriptor's own examples were corrected to (P19/#1021) —
  // they previously claimed `cancelled` outright, written before there was an
  // implementation to check against.
  assert.equal(cancelled.result.status, 'running');
  assert.equal(cancelled.result.artifact, undefined);

  await ctx.session.done(jobId);
  const after = plain(ctx.mcp.handler(request('status', { jobId })));
  assert.equal(after.result.status, 'cancelled');
  assert.equal(after.result.artifact, undefined, 'a cancelled job never reports an artifact');
  assert.equal(after.result.progress, 0);
  // The examples in the descriptor say exactly this sequence, so they cannot
  // drift back to the un-implementable version without this failing.
  const labels = CAPABILITY_JSON.examples.map((e) => e.output.status);
  assert.deepEqual(labels, ['running', 'running', 'succeeded', 'running', 'cancelled']);
});

test('A3: malformed stages are typed errors, not exceptions or silent successes', () => {
  const ctx = sessionContext();
  const bad = (req) => plain(ctx.mcp.handler(req));
  assert.equal(bad(request('start', {})).error.code, 'invalid_request');
  assert.equal(bad(request('start', { frameIdx: -1 })).error.code, 'invalid_request');
  assert.equal(bad(request('start', { frameIdx: 1.5 })).error.code, 'invalid_request');
  assert.equal(bad(request('status', {})).error.code, 'invalid_request');
  assert.equal(bad(request('cancel', {})).error.code, 'invalid_request');
  assert.equal(bad(request('status', { jobId: 'nope' })).error.code, 'unknown_job');
  assert.equal(bad(request('property.get', {})).error.code, 'invalid_request', 'an operation outside the lifecycle is refused');
  assert.equal(bad(request('start', { frameIdx: 0 }, { cancelled: true })).error.code, 'cancelled');
  assert.equal(bad(request('start', { frameIdx: 0 }, { documentId: 'other' })).error.code, 'wrong_document');
  assert.equal(bad(request('start', { frameIdx: 0 }, { instanceId: 'other' })).error.code, 'wrong_document');
  assert.equal(bad({ apiVersion: 2, requestId: 'r', operation: 'start', payload: {} }).error.code, 'invalid_request');
  assert.equal(bad(null).error.code, 'invalid_request');
  // Every rejection still answers in the transport envelope.
  assert.equal(bad(request('status', {})).apiVersion, 1);
});

// ---------------------------------------------------------------------------
// B. THE test: one session, proven by cross-path effects
// ---------------------------------------------------------------------------

test('B1: an MCP start during a dialog export is refused busy — there is one session, not two', async () => {
  let release;
  const gate = new Promise((resolve) => { release = resolve; });
  let seen = null;
  const ctx = sessionContext({
    async exportWriteText(p, text) {
      ctx.files.set(p, text);
      if (ctx.files.size === 1) { seen = plain(ctx.mcp.handler(request('start', { frameIdx: 0 }))); release(); }
    },
  });
  const running = ctx.exportSVGSequenceToDir('/seq', { start: 0, end: 2 });
  await gate;
  assert.equal(seen.ok, false);
  assert.equal(seen.error.code, 'busy', 'the MCP stage sees the dialog job through the shared running slot');
  assert.equal(seen.result, undefined, 'a refused start yields no jobId a client could poll');
  assert.deepEqual(plain(await running), { ok: true, dir: '/seq' });
  // And once the dialog job is done the slot is free again.
  const after = plain(ctx.mcp.handler(request('start', { frameIdx: 0 })));
  assert.equal(after.ok, true);
});

test('B2: an MCP cancel of the DIALOG\'s jobId stops the dialog export and removes its partial files', async () => {
  const ctx = sessionContext({
    async exportWriteText(p, text) {
      ctx.files.set(p, text);
      if (ctx.files.size === 1) {
        // Exactly what an MCP client holds: the jobId, nothing else.
        const jobId = ctx.session.meta().running;
        const answer = plain(ctx.mcp.handler(request('cancel', { jobId })));
        assert.equal(answer.ok, true);
        assert.equal(answer.result.jobId, jobId);
      }
    },
  });
  const res = plain(await ctx.exportSVGSequenceToDir('/seq', { start: 0, end: 2 }));
  assert.deepEqual(res, { cancelled: true }, 'the dialog caller sees its own existing cancelled shape');
  assert.equal(ctx.files.size, 0, 'the one written file was cleaned up by the job, not by a second cleanup path');
  assert.ok(ctx.removed.includes('/seq/frame_0001.svg'));
});

// The real timeline.js dialog block, sliced from source and given only the
// handful of names it closes over.
const DIALOG_CANCEL = slice(timelineSource,
  '  // P19/#1021 — Cancel calls exactly the method',
  '\n  runBtn.addEventListener(\'click\',async function(){', 'timeline export cancel');

function dialogCancelHandler(session) {
  const clicks = [];
  vm.runInContext(DIALOG_CANCEL, vm.createContext({
    console, Object, JSON, String,
    document: { getElementById: () => ({ style: {}, addEventListener(type, fn) { clicks.push(fn); } }) },
    SM: { t: (k) => k },
    window: { SMExport: { isAvailable: () => true, svgSequenceJob: () => session } },
    fmtSel: { value: 'svg' }, progEl: { style: {} },
  }), { filename: 'timeline-export-cancel.js' });
  assert.equal(clicks.length, 1, 'the sliced block registers exactly one click handler');
  return clicks[0];
}

test('B3: the dialog cancel button and the MCP cancel stage call the same method on the same object', async () => {
  const idle = sessionContext();
  // No job yet (the directory picker is still open): the handler must not
  // throw and must not invent one.
  dialogCancelHandler(idle.session)();
  assert.equal(idle.session.meta().running, null);
  assert.equal(idle.session.meta().jobs, 0);

  // With a dialog export in flight, the button cancels it — the same effect B2
  // got through the MCP stage, because it is the same call on the same session.
  const ctx = sessionContext({
    async exportWriteText(p, text) { ctx.files.set(p, text); if (ctx.files.size === 1) click(); },
  });
  const click = dialogCancelHandler(ctx.session);
  assert.deepEqual(plain(await ctx.exportSVGSequenceToDir('/seq', { start: 0, end: 2 })), { cancelled: true });
  assert.equal(ctx.files.size, 0);
});

test('B4: the cancel button markup and its label exist, so the sliced handler has a real target', () => {
  const html = read(path.join(root, 'src/index.html'));
  assert.match(html, /id="exp-cancel"[^>]*data-i18n="btnCancel"/);
  // btnCancel is an existing key in every locale — no new i18n entry was needed.
  const i18n = read(src('i18n.js'));
  assert.equal(i18n.split('btnCancel:').length - 1, 4, 'btnCancel is defined in all four locales');
});

// ---------------------------------------------------------------------------
// C. Availability: one typed oracle, read by both paths
// ---------------------------------------------------------------------------

test('C1: with the evaluator gone, the UI path and the MCP path report the SAME typed reason and no job', async () => {
  const ctx = sessionContext();
  // The backend the fixture disables: the frame evaluator both sinks need.
  const dead = ctx.NemoExportSvgSequence.create({
    frameName: (i) => 'frame_' + i + '.svg',
    tauri: ctx.exportTauri, mkdir: ctx.exportMkdir, removeDir: ctx.exportRemoveDir,
    writeText: ctx.exportWriteText, saveAllLayerFrames() {},
  });
  const deadMcp = ctx.NemoExportSvgSequence.capability({ session: () => dead, meta: () => IDENTITY });

  const ui = plain(await dead.run({ dir: '/seq', start: 0, end: 2 }));
  const api = plain(deadMcp.handler(request('start', { frameIdx: 0 })));
  assert.equal(ui.reason, 'missing-dependency');
  assert.equal(api.error.code, 'missing-dependency', 'the same code, not a second vocabulary');
  assert.equal(ui.ok, false);
  assert.equal(api.ok, false);
  assert.equal(api.result, undefined, 'nothing a client could read as a started job');
  assert.equal(dead.meta().jobs, 0, 'no job was created on either path');
  assert.equal(ctx.files.size, 0, 'and no partial artifact exists to be mistaken for success');
});

test('C2: without the Tauri filesystem the directory sink is platform-unsupported; the inline sink is not', async () => {
  const ctx = sessionContext();
  const browser = ctx.NemoExportSvgSequence.create({
    evaluateFrame: ctx.exportFrameSVGString, frameName: (i) => 'frame_' + i + '.svg',
    tauri() { return undefined; }, mkdir: ctx.exportMkdir, removeDir: ctx.exportRemoveDir,
    writeText: ctx.exportWriteText, saveAllLayerFrames() {},
  });
  assert.deepEqual(plain(browser.availability('directory')), { state: 'unavailable', reason: 'platform-unsupported' });
  assert.deepEqual(plain(browser.availability('inline')), { state: 'available', reason: null });
  const ui = plain(await browser.run({ dir: '/seq', start: 0, end: 2 }));
  assert.equal(ui.ok, false);
  assert.equal(ui.reason, 'platform-unsupported');
  assert.equal(browser.meta().jobs, 0);
  // A single inline frame needs no filesystem, so refusing it here would be
  // inventing a limitation: it still runs.
  const mcp = ctx.NemoExportSvgSequence.capability({ session: () => browser, meta: () => IDENTITY });
  const started = plain(mcp.handler(request('start', { frameIdx: 2 })));
  assert.equal(started.ok, true);
  await browser.done(started.result.jobId);
  assert.equal(plain(mcp.handler(request('status', { jobId: started.result.jobId }))).result.artifact.data, '<svg f="2"/>');
});

test('C3: every availability reason this code can emit is in capability-v1\'s closed enum', () => {
  const schema = JSON.parse(read(path.join(root, 'engineering/application/capability-v1.schema.json')));
  const allowed = schema.$defs.Availability.properties.reason.anyOf[0].enum;
  const emitted = [...adapterSource.matchAll(/reason: '([a-z-]+)'/g)].map((m) => m[1]);
  assert.ok(emitted.length >= 2, 'the oracle emits at least the two reasons it documents');
  for (const reason of emitted) assert.ok(allowed.includes(reason), `"${reason}" is not a capability-v1 reason`);
});

// ---------------------------------------------------------------------------
// D. The runtime descriptor and the canonical JSON do not drift
// ---------------------------------------------------------------------------

test('D1: the runtime descriptor equals engineering/application/capabilities/export-job.json', () => {
  assert.deepEqual(plain(require(src('adapters/export-svg-sequence.js')).DESCRIPTOR), CAPABILITY_JSON);
});

test('D2: the descriptor now claims to be available, and the handler is what makes that true', () => {
  const registry = require(src('application/capability-registry.js')).create();
  const adapter = require(src('adapters/export-svg-sequence.js'));
  // Registration refuses an `available` claim with nothing behind it, so this
  // passing IS the evidence that the capability is bound.
  assert.equal(CAPABILITY_JSON.availability.state, 'available');
  assert.equal(registry.register(adapter.DESCRIPTOR, () => ({ ok: true })), 'export.svg.frame');
  assert.equal(typeof registry.handlerFor('export.svg.frame'), 'function');
  assert.deepEqual(CAPABILITY_JSON.effects.lifecycle, ['start', 'status', 'cancel']);
});

// ---------------------------------------------------------------------------
// E. The real bootstrap routes by capability instead of assuming one
// ---------------------------------------------------------------------------

function bootstrapContext(exportSession) {
  const noop = () => {};
  let sequence = 0;
  const ctx = {
    console, crypto: { randomUUID: () => `test-${++sequence}` },
    document: { readyState: 'loading', addEventListener: noop,
      getElementById: () => null, querySelector: () => null, querySelectorAll: () => [] },
    localStorage: { getItem: () => null },
    state: { currentFrame: 0, totalFrames: 24, fps: 24, appMode: 'motion',
      activeLayerIdx: 0, waOut: 23, maxUndo: 50, symbols: {},
      layers: [{ name: 'A', layerUid: 'a', frames: { 0: { strokes: [] } }, motionStatic: { opacity: [100] } }],
      undoStack: [], redoStack: [], undoLabels: [], redoLabels: [] },
    userLayers: [], _layerSel: [], SM: { t: (v) => v, setActiveLayer: noop },
    saveAllLayerFrames: noop, activateUL: noop, loadFrame: noop,
    renderOS: noop, renderArcs: noop, updateUI: noop, showToast: noop,
    renderLayerList: noop, renderTimeline: noop,
    SMExport: { svgSequenceJob: () => exportSession },
    createUserLayer(name) { ctx.state.layers.push({ name }); return ctx.state.layers.length - 1; },
  };
  ctx.window = ctx;
  vm.createContext(ctx);
  for (const file of ['animation/curve.js', 'domain/animation/opacity.js', 'motion.js',
    'domain/document/folder-codec.js', 'domain/tween/assignment.js',
    'application/history/frame-entry.js', 'tweens.js', 'domain/diagnostics/opacity-diagnostics.js', 'application/opacity-application.js',
    'application/capability-registry.js', 'application/opacity-capability.js',
    'application/export-job.js', 'adapters/export-svg-sequence.js',
    'bootstrap/opacity-application.js']) {
    const filename = src(file);
    vm.runInContext(read(filename), ctx, { filename });
    if (file === 'tweens.js') { ctx.renderOS = noop; ctx.renderArcs = noop; }
  }
  return ctx;
}

test('E1: NemoApplication.handle routes export operations to the export capability and everything else to opacity', () => {
  const inner = sessionContext();
  const ctx = bootstrapContext(inner.session);

  // Both capabilities are discoverable, and both are bound.
  const listed = ctx.NemoApplication.capabilities().map((e) => [e.id, e.bound]);
  assert.deepEqual(plain(listed), [['export.svg.frame', true], ['opacity', true]]);

  // An export operation reaches the export handler over the shared session.
  const identity = ctx.NemoOpacityApplication.meta();
  const started = plain(ctx.NemoApplication.handle({
    apiVersion: 1, requestId: 'r1', operation: 'start', payload: { frameIdx: 3 },
    instanceId: identity.instanceId, documentId: identity.documentId,
  }));
  assert.equal(started.ok, true, JSON.stringify(started));
  assert.equal(started.result.status, 'running');
  assert.equal(inner.session.meta().running, started.result.jobId, 'it is the session export.js owns');

  // Opacity's own operations are untouched by the new routing.
  const read1 = plain(ctx.NemoApplication.handle({
    apiVersion: 1, requestId: 'r2', operation: 'property.get',
    payload: { layerId: 'a', property: 'opacity' },
    instanceId: identity.instanceId, documentId: identity.documentId,
  }));
  assert.equal(read1.ok, true, JSON.stringify(read1));
  assert.equal(read1.result.value, 100);

  // And so are the service-wide operations that belong to no single feature —
  // they are deliberately absent from opacity's own effects.lifecycle, which is
  // exactly why opacity has to stay the default rather than be routed by name.
  const caps = plain(ctx.NemoApplication.handle({
    apiVersion: 1, requestId: 'r3', operation: 'capabilities', payload: {},
    instanceId: identity.instanceId, documentId: identity.documentId,
  }));
  assert.equal(caps.ok, true, JSON.stringify(caps));
  assert.deepEqual(caps.result.descriptors.map((d) => d.id), ['export.svg.frame', 'opacity']);
  const snap = plain(ctx.NemoApplication.handle({
    apiVersion: 1, requestId: 'r4', operation: 'snapshot', payload: {},
    instanceId: identity.instanceId, documentId: identity.documentId,
  }));
  assert.equal(snap.ok, true, JSON.stringify(snap));
});

test('E2: two capabilities claiming one operation is a loud failure, not a silent mis-route', () => {
  const inner = sessionContext();
  const ctx = bootstrapContext(inner.session);
  const clash = JSON.parse(JSON.stringify(CAPABILITY_JSON));
  clash.id = 'export.svg.frame.clone';
  clash.handlerKey = 'application.export.svgFrame.clone';
  ctx.NemoCapabilities.register(clash, () => ({ ok: true }));
  assert.throws(() => ctx.NemoApplication.handle({
    apiVersion: 1, requestId: 'r5', operation: 'start', payload: { frameIdx: 0 },
  }), /both claim the operation "start"/);
});

// ---------------------------------------------------------------------------
// F. The render queue's Stop reaches the same job
// ---------------------------------------------------------------------------

function renderManagerContext(exportCtx) {
  const handlers = {};
  const element = (id) => ({ id, style: {}, disabled: false, textContent: '', innerHTML: '',
    setAttribute() {}, removeAttribute() {}, appendChild() {},
    addEventListener(type, fn) { (handlers[id] = handlers[id] || {})[type] = fn; } });
  const known = ['render-manager-modal', 'rm-close', 'rm-add-current', 'rm-delete-all',
    'rm-render-all', 'rm-dyn-index', 'rm-dyn-offset'];
  const ctx = {
    console, JSON, Math, Date, Object, Array, String, Number, Promise, Error, parseInt, parseFloat,
    state: { fps: 24, totalFrames: 24, waIn: 0, waOut: 2 },
    SM: { t: (k) => k },
    showToast(m) { ctx.toasts = (ctx.toasts || []).concat(m); },
    saveAllLayerFrames() {},
    document: { getElementById: (id) => (known.includes(id) ? element(id) : null), createElement: () => element('x') },
    SMExport: {
      isAvailable: () => true,
      svgSequenceJob: () => exportCtx.session,
      exportSVGSequenceToDir: (dir, opts) => exportCtx.exportSVGSequenceToDir(dir, opts),
    },
  };
  ctx.window = ctx;
  vm.createContext(ctx);
  vm.runInContext(renderManagerSource, ctx, { filename: 'render-manager.js' });
  ctx.handlers = handlers;
  return ctx;
}

test('F1: Render All doubles as Stop, and Stop cancels the running export job itself', async () => {
  let stop;
  const gate = new Promise((resolve) => { stop = resolve; });
  const exportCtx = sessionContext({
    async exportWriteText(p, text) { exportCtx.files.set(p, text); if (exportCtx.files.size === 1) stop(); },
  });
  const rm = renderManagerContext(exportCtx);
  rm.SMRenderManager._queue().push({
    id: 'rq1', enabled: true, name: 'A', format: 'svg', frameRangeMode: 'custom',
    rangeStart: 0, rangeEnd: 2, resolutionScalePct: 100, quality: 'high',
    outputPath: '/seq', padding: 4, alpha: false, status: 'idle', progress: 0, error: null, outPathResolved: null,
  });
  rm.SMRenderManager.open();          // binds the real toolbar handlers
  const click = rm.handlers['rm-render-all'].click;
  assert.equal(typeof click, 'function');

  const batch = click();               // first click renders
  await gate;
  assert.equal(rm.SMRenderManager.isRendering(), true);
  const jobId = exportCtx.session.meta().running;
  assert.ok(jobId, 'the queue item really started the shared job');
  click();                             // second click is Stop, not a second batch
  await batch;

  assert.equal(plain(exportCtx.session.status(jobId)).job.status, 'cancelled',
    'Stop cancelled the job object, it did not merely abandon the loop');
  assert.equal(exportCtx.files.size, 0, 'and the job removed its own partial files');
  assert.equal(rm.SMRenderManager._queue()[0].status, 'error');
  assert.equal(rm.SMRenderManager._queue()[0].error, 'rmErrCancelled');
  assert.equal(rm.SMRenderManager.isRendering(), false);
});

test('F2: Stop outside a batch does nothing, and a queued batch stops before its next item', async () => {
  // The write hook has to be installed through sessionContext: the session's
  // ports are captured by value when export.js first builds it, so reassigning
  // the context global afterwards would be silently ignored.
  const exportCtx = sessionContext({
    async exportWriteText(p, text) { exportCtx.files.set(p, text); if (exportCtx.hook) exportCtx.hook(); },
  });
  const rm = renderManagerContext(exportCtx);
  assert.equal(rm.SMRenderManager.cancelAll(), false, 'nothing to stop when idle');

  const item = (id) => ({ id, enabled: true, name: id, format: 'svg', frameRangeMode: 'custom',
    rangeStart: 0, rangeEnd: 0, resolutionScalePct: 100, quality: 'high', outputPath: '/seq-' + id,
    padding: 4, alpha: false, status: 'idle', progress: 0, error: null, outPathResolved: null });
  rm.SMRenderManager._queue().push(item('a'), item('b'), item('c'));
  rm.SMRenderManager.open();
  let stopped = false;
  exportCtx.hook = () => { if (!stopped) { stopped = true; rm.SMRenderManager.cancelAll(); } };
  await rm.handlers['rm-render-all'].click();
  // plain(): the array is born in the vm context, so it carries that context's
  // Array prototype and deepStrictEqual would reject an otherwise equal value.
  const statuses = plain(rm.SMRenderManager._queue().map((it) => it.status));
  assert.deepEqual(statuses, ['error', 'idle', 'idle'], 'the two later items were never started');
  assert.equal(rm.SMRenderManager._queue()[0].error, 'rmErrCancelled');
});
