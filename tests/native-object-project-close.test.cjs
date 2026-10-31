'use strict';
// #2381: closing the last project returns to the home screen. Session release, file adapter, and the prompt-first orchestration.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const Project = require('../src/js/adapters/native-object-project.js');
const Files = require('../src/js/adapters/native-object-project-file.js');
const Close = require('../src/js/adapters/native-object-project-close.js');
const Workflow = require('../src/js/application/native-object-save-as-workflow.js');
const Ack = require('../src/js/application/native-object-saved-acknowledgement.js');

function host(options = {}) {
  const calls = []; let installed = false, serial = 0; const revision = 0;
  const info = () => ({ apiVersion: 2, instanceId: 'instance', documentId: 'document-' + serial, lifecycleGeneration: 1,
    documentSnapshotId: 'snapshot-' + revision, contentRevision: revision, canvasW: 320, canvasH: 180, fps: 24, totalFrames: 21,
    documentDigest: 'a'.repeat(64), byteCount: 10000, layers: [{ layerUid: 'drawing' }], reference: null, undoDepth: 0, redoDepth: 0 });
  async function invoke(command, args) {
    const r = args && args.requestJson ? JSON.parse(args.requestJson) : args && args.request || null; calls.push({ command, r });
    if (command === 'nemo_native_object_client_status') return { ...info(), available: installed };
    if (command === 'nemo_native_object_bootstrap') { installed = true; serial++; return { ...info(), requestId: r.requestId, resourceCount: 0, viewportAvailable: false }; }
    if (command === 'nemo_native_object_reference' && r.action.kind === 'inspect') return { ...info(), requestId: r.requestId };
    if (command === 'nemo_native_object_preview') return { status: 'presented', workId: 'work', viewGeneration: 1 };
    if (command === 'nemo_native_release') {
      if (options.releaseFails) throw new Error('release lost');
      if (!options.releaseKeepsProject) installed = false;
      return { status: options.releaseStatus || 'succeeded', reentryAvailable: true, requestId: r.requestId, instanceId: r.instanceId, documentId: r.documentId };
    }
    throw new Error('Unexpected command: ' + command);
  }
  return { invoke, calls, vacate() { installed = false; } };
}
function session(h, surfaces = []) {
  return Project.create({ invoke: h.invoke, frame: () => 0, surfaceChanged: (value) => surfaces.push(value === null ? null : 'shown'),
    mapping: (i) => ({ cssBounds: { x: 0, y: 0, width: 320, height: 180 }, physicalExtent: { width: 320, height: 180 },
      compositionExtent: { width: i.canvasW, height: i.canvasH }, reportedDpr: 1 }) });
}

test('Close releases the active project through the existing release route and the session returns to idle', async () => {
  const h = host(), surfaces = [], s = session(h, surfaces);
  await s.openPath('/project.json'); assert.equal(s.isActive(), true); assert.equal(s.blocksLegacy(), true);
  const mark = h.calls.length;
  assert.equal(await s.closeProject(), true);
  const sent = h.calls.slice(mark).map((c) => c.command);
  assert.deepEqual(sent, ['nemo_native_object_client_status', 'nemo_native_release', 'nemo_native_object_client_status']);
  const release = h.calls.find((c) => c.command === 'nemo_native_release').r;
  assert.deepEqual({ ...release, requestId: typeof release.requestId }, { apiVersion: 2, requestId: 'string', instanceId: 'instance', documentId: 'document-1', expectedRevision: 0 });
  assert.equal(s.isActive(), false); assert.equal(s.info(), null); assert.equal(s.blocksLegacy(), false);
  assert.equal(surfaces.at(-1), null, 'the native canvas is hidden');
  assert.equal(await s.newProject({ canvasW: 320, canvasH: 180, fps: 24 }, {}).then((i) => !!i), true, 'New works from the home screen afterwards');
});

test('Close is refused while idle, and a second Close while one runs is refused', async () => {
  const s = session(host());
  await assert.rejects(s.closeProject(), (e) => e.details.disposition === 'notDispatched');
  const h = host(), t = session(h); await t.openPath('/project.json');
  const first = t.closeProject();
  await assert.rejects(t.closeProject(), (e) => e.details.disposition === 'notDispatched');
  await first;
});

test('a project that is no longer the one on screen is never released blind', async () => {
  const h = host(), s = session(h); await s.openPath('/project.json');
  h.vacate();
  await assert.rejects(s.closeProject());
  assert.equal(h.calls.some((c) => c.command === 'nemo_native_release'), false);
  assert.equal(s.isIndeterminate(), true, 'an owner that cannot be confirmed is fenced, never released blind');
});

test('an unconfirmed release fences the session instead of pretending the project closed', async () => {
  for (const options of [{ releaseFails: true }, { releaseStatus: 'failed' }, { releaseKeepsProject: true }]) {
    const h = host(options), s = session(h); await s.openPath('/project.json');
    await assert.rejects(s.closeProject());
    assert.equal(s.isIndeterminate(), true, JSON.stringify(options)); assert.equal(s.isActive(), false);
  }
});

function files(s, extra = {}) {
  const log = [];
  const ports = { readUI: () => ({ handle: 'tab', generation: 1 }), reveal: async () => {}, opened: () => true, commit: () => true, pick: async () => '/n.json',
    repaint: () => log.push('repaint'), failed: (e) => log.push('failed:' + e.message), refused: (e) => log.push('refused:' + e.message),
    closeFailed: (e) => log.push('closeFailed:' + e.message), confirmReplace: async () => true, notice() {}, discardRecovery: (id) => log.push('discard:' + id), ...extra };
  return { ui: Files.create(s, ports, Workflow, Ack), log };
}

test('the file adapter closes the project, drops its crash-recovery slots and forgets the saved baseline', async () => {
  const h = host(), s = session(h), f = files(s); await f.ui.openPath('/project.json');
  assert.equal(f.ui.isDirty(), false);
  assert.equal(await f.ui.closeProject(), true);
  assert.ok(f.log.includes('discard:document-1')); assert.equal(f.ui.canOpen(), true);
  assert.equal(await f.ui.closeProject(), false, 'nothing left to close');
});

test('a refused or failed close reports a close message, never an Open one', async () => {
  const h = host({ releaseFails: true }), s = session(h), f = files(s); await f.ui.openPath('/project.json');
  assert.equal(await f.ui.closeProject(), false);
  assert.ok(f.log.some((x) => /^closeFailed:/.test(x)), f.log.join('|')); assert.ok(!f.log.some((x) => /^(refused|failed):/.test(x)));
});

function orchestration(over = {}) {
  const log = []; let dirty = false, pinned = 'one';
  const ports = { native: () => true, current: () => pinned, isDirty: () => dirty, notice: (m) => log.push('notice:' + m),
    confirmDiscard: async () => { log.push('prompt'); return true; }, closeNative: async () => { log.push('closeNative'); return true; },
    closeLegacy: async () => { log.push('closeLegacy'); return true; }, finish: () => log.push('finish'), ...over };
  return { log, close: Close.create(ports), set dirty(v) { dirty = v; }, set pinned(v) { pinned = v; } };
}

test('a clean project closes without a prompt and then shows the home screen', async () => {
  const o = orchestration(); assert.equal(await o.close(), true); assert.deepEqual(o.log, ['closeNative', 'finish']);
});
test('unsaved changes prompt FIRST; Cancel keeps the project and the tab', async () => {
  const cancel = orchestration({ confirmDiscard: async () => false }); cancel.dirty = true;
  assert.equal(await cancel.close(), false); assert.deepEqual(cancel.log, []);
  const yes = orchestration(); yes.dirty = true;
  assert.equal(await yes.close(), true); assert.deepEqual(yes.log, ['prompt', 'closeNative', 'finish']);
});
test('the browser project takes the same prompt-first path through its own close', async () => {
  const o = orchestration({ native: () => false }); o.dirty = true;
  assert.equal(await o.close(), true); assert.deepEqual(o.log, ['prompt', 'closeLegacy', 'finish']);
});
test('a project that changed while the prompt was open is not closed', async () => {
  let o; o = orchestration({ confirmDiscard: async () => { o.pinned = 'two'; return true; } }); o.dirty = true;
  assert.equal(await o.close(), false); assert.deepEqual(o.log, []);
});
test('a failed close keeps the tab, and a repeated Cmd+W while one runs is ignored', async () => {
  const failed = orchestration({ closeNative: async () => false }); assert.equal(await failed.close(), false); assert.ok(!failed.log.includes('finish'));
  let release; const slow = orchestration({ closeNative: () => new Promise((r) => { release = () => r(true); }) });
  const first = slow.close(); assert.equal(await slow.close(), false); release(); assert.equal(await first, true);
  const thrown = orchestration({ closeNative: async () => { throw new Error('boom'); } });
  assert.equal(await thrown.close(), false); assert.deepEqual(thrown.log, ['notice:Could not close the project: boom']);
  assert.equal(await thrown.close(), false, 'busy flag clears after a failure');
});

test('the shipped page loads the close module before the native project session', () => {
  const html = fs.readFileSync('src/index.html', 'utf8');
  const at = (name) => html.indexOf('js/adapters/' + name);
  assert.ok(at('native-object-project-close.js') > 0 && at('native-object-project-close.js') < at('native-object-project.js'));
});
