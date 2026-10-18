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
    Blob: class { constructor(parts, meta) { calls.push(['browser-blob',parts[0],meta.type]); } },
    URL: { createObjectURL: () => 'blob:svg-fixture', revokeObjectURL: (url) => calls.push(['browser-revoke',url]) },
    state: { totalFrames: 21, waIn: 0, waOut: 20, fps: 24, canvasW: 320, canvasH: 180 },
    rangeSel: { value: 'all' }, fmtSel: { value: options.format || 'png' },
    ALPHA_FORMATS: ['png'], currentExportScale: () => options.scale || 1,
    progEl: { style: {}, textContent: '' }, cancelBtn: { style: {}, disabled: false,
      addEventListener: (_event, fn) => { ctx.cancel = fn; } },
    modal: { style: {} }, SM: { t: (key) => key === 'exportError' ? 'exportError: {e}' : key }, showToast() {},
    saveAllLayerFrames() { calls.push('legacy-save'); },
    document: { getElementById: (id) => elements[id], createElement: () => ({ click() { calls.push(['browser-download',this.download,this.href]); } }) },
    exportTauriAvailable: () => options.browser !== true,
    exportFrameRange: (opts) => ({ start: opts.start, end: opts.end }),
    exportFrameSVGString: (frame) => { calls.push(['legacy-svg-evaluate',frame]); return '<svg>legacy</svg>'; },
    exportSvgSequenceJob: () => ({ run: async (intent) => { calls.push(['legacy-svg-run',intent.start,intent.end]); return {ok:true}; } }),
    exportPickDir: async () => { calls.push('picker'); if (options.pick) return options.pick(); return options.cancelPicker ? null : '/out'; },
    runBtn: { disabled: false, addEventListener: (_event, fn) => { ctx.click = fn; } },
  };
  ctx.window = ctx;
  elements['exp-cancel'] = ctx.cancelBtn;
  if (native) ctx.NemoNativeOpacityCutover = {
    blocksLegacy: () => true,
    isActive: () => options.active !== false,
    identity: () => options.identity ? options.identity() : ({instanceId:'instance',documentId:'document',contentRevision:0,documentSnapshotId:'snapshot-0',lifecycleGeneration:1}),
    releaseCurrent: async () => { calls.push('release'); },
    exportSvg: options.olderOpacity ? undefined : async (destination, frames, onProgress, signal) => {
      calls.push(['native-svg', Array.from(frames), destination]);
      if (options.fail) throw new Error('native SVG job failed');
      if (options.pendingSvg) return options.pendingSvg();
      if (signal && signal.aborted) return { status: 'cancelled' };
      if (onProgress) onProgress(frames.length, frames.length);
      return options.svgReceipt || { status: 'succeeded', published: true, cleanupComplete: true };
    },
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
  vm.runInContext(fs.readFileSync(path.join(root, 'src/js/adapters/native-main-svg-export.js'), 'utf8'), ctx, {filename:'native-main-svg-export.js'});
  vm.runInContext(
    between(exportSource, 'function exportNativeOpacity(){', '\nfunction exportEnsureLayer(') + '\n' +
    between(exportSource, 'async function exportRenderPNGsToDir(', '\n// ---- Browser-compatible video export') + '\n' +
    between(exportSource, 'function exportMainSvgWorkflow(){', '\nasync function exportGIFToPath(') + '\n' +
    'window.SMExport={exportSVGSequenceToDir:exportSVGSequenceToDir,svgSequenceJob:function(){throw Error("legacy SVG job must not be read");},' +
    between(exportSource, '  exportSVGSequence:async function(opts){', '\n  exportTIFFSequence:') + '\n};',
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

test('visible native SVG exports the literal inclusive range without save, release or legacy job',async()=>{
  const {ctx,calls}=harness(true,{format:'svg'});await ctx.click();
  assert.deepEqual(calls.map(c=>Array.isArray(c)?c[0]:c),['picker','native-svg']);
  assert.deepEqual(calls[1][1],Array.from({length:21},(_,i)=>i));
  assert.match(calls[1][2],/^\/out\/nemo-native-svg-/);
  assert.equal(ctx.progEl.textContent,'exportDone');assert.equal(ctx.cancelBtn.style.display,'none');
  ctx.cancel();assert.equal(calls.length,2,'native SVG never reaches legacy cancellation');
});
test('native SVG render-manager ToDir uses the same native entry without a picker',async()=>{
  const {ctx,calls}=harness(true,{format:'svg'}),progress=[];
  const result=await ctx.SMExport.exportSVGSequenceToDir('/batch/',{start:2,end:4,scale:1,onProgress:(...v)=>progress.push(v)});
  assert.equal(result.ok,true);assert.match(result.dir,/^\/batch\/nemo-native-svg-/);
  assert.deepEqual(calls.map(c=>c[0]),['native-svg']);assert.deepEqual(calls[0][1],[2,3,4]);
  assert.deepEqual(progress,[[3,3]]);
});
test('native SVG picker cancellation and pre-aborted callers dispatch nothing',async()=>{
  const cancelled=harness(true,{format:'svg',cancelPicker:true});await cancelled.ctx.click();
  assert.deepEqual(cancelled.calls,['picker']);assert.equal(cancelled.ctx.progEl.textContent,'exportCancelled');
  const aborted=harness(true,{format:'svg'}),signal={aborted:true};
  assert.equal((await aborted.ctx.SMExport.exportSVGSequence({start:0,end:1,signal})).cancelled,true);
  assert.equal((await aborted.ctx.SMExport.exportSVGSequenceToDir('/batch',{start:0,end:1,signal})).cancelled,true);
  assert.deepEqual(aborted.calls,[]);
  const controller=new AbortController(),during=harness(true,{format:'svg',pick:()=>{controller.abort();return '/out';}});
  assert.equal((await during.ctx.SMExport.exportSVGSequence({start:0,end:1,signal:controller.signal})).cancelled,true);
  assert.deepEqual(during.calls,['picker']);
});
test('native SVG indeterminate or older opacity owner is explicitly unavailable before picker',async()=>{
  for(const options of [{active:false},{olderOpacity:true},{scale:2}]){
    const {ctx,calls}=harness(true,{format:'svg',...options});await ctx.click();
    assert.deepEqual(calls,[]);assert.match(ctx.progEl.textContent,/exportError/);
    ctx.cancel();assert.deepEqual(calls,[]);
  }
});
test('native SVG invalid bounded range is refused before picker or loop allocation',async()=>{
  for(const [start,end] of [[-1,1],[0,32],[2,1],[0.5,1],[0,Infinity],[null,1]]){
    const {ctx,calls}=harness(true,{format:'svg'});
    await assert.rejects(ctx.SMExport.exportSVGSequence({start,end}),/frame range is invalid/);
    assert.deepEqual(calls,[]);
  }
});
test('native SVG failures never report publication, release or fall back to legacy',async()=>{
  for(const options of [{fail:true},{svgReceipt:{status:'running'}},{svgReceipt:{status:'succeeded',published:false,cleanupComplete:true}}]){
    const {ctx,calls}=harness(true,{format:'svg',...options});await ctx.click();
    assert.deepEqual(calls.map(c=>Array.isArray(c)?c[0]:c),['picker','native-svg']);
    assert.match(ctx.progEl.textContent,/exportError/);
  }
});
test('native synchronous SVG hides live cancel and a stale cancel cannot enter the legacy job',async()=>{
  let resolve;const pending=new Promise(r=>{resolve=r;});
  const {ctx,calls}=harness(true,{format:'svg',pendingSvg:()=>pending}),running=ctx.click();
  await new Promise(setImmediate);assert.equal(ctx.cancelBtn.style.display,'none');
  ctx.cancel();resolve({status:'succeeded',published:true,cleanupComplete:true});await running;
  assert.equal(ctx.progEl.textContent,'exportDone');
  assert.deepEqual(calls.map(c=>Array.isArray(c)?c[0]:c),['picker','native-svg']);
});
test('legacy-owned SVG keeps its existing save path and browser native SVG cannot fall back',async()=>{
  const legacy=harness(false,{format:'svg'});await legacy.ctx.click();
  assert.deepEqual(legacy.calls,['legacy-save','picker',['legacy-svg-run',0,20]]);
  const browser=harness(true,{format:'svg',browser:true});await browser.ctx.click();
  assert.deepEqual(browser.calls,[]);assert.match(browser.ctx.progEl.textContent,/preview navigateur/);
});

test('native SVG picker intent cannot export a changed owner or revision',async()=>{
  for(const field of ['instanceId','documentId','contentRevision','documentSnapshotId','lifecycleGeneration']){
    const pin={instanceId:'instance',documentId:'document',contentRevision:0,documentSnapshotId:'snapshot-0',lifecycleGeneration:1};
    const {ctx,calls}=harness(true,{format:'svg',identity:()=>pin,pick:()=>{pin[field]=field==='contentRevision'||field==='lifecycleGeneration'?2:'changed';return '/out';}});
    await ctx.click();assert.deepEqual(calls,['picker']);assert.match(ctx.progEl.textContent,/owner changed/);
  }
});

test('legacy browser SVG keeps its evaluator and download contract only when native-inactive',async()=>{
  const {ctx,calls}=harness(false,{format:'svg',browser:true});
  const result=await ctx.SMExport.exportSVGSequence({start:3,end:9});
  assert.equal(result.ok,true);assert.equal(result.browserFallback,true);
  assert.deepEqual(calls,[['legacy-svg-evaluate',3],['browser-blob','<svg>legacy</svg>','image/svg+xml'],
    ['browser-download','frame_0001.svg','blob:svg-fixture'],['browser-revoke','blob:svg-fixture']]);
});
