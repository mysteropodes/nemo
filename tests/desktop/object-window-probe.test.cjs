'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const path = require('node:path');
const script = fs.readFileSync(path.join(__dirname, 'object-window-probe-bootstrap.js'), 'utf8');
const config = { runId: '11111111-1111-4111-8111-111111111111', pid: 42, taskKey: 'a'.repeat(64), sourceSha: 'b'.repeat(40) };
async function scenario(options = {}) {
  const calls = [], phases = []; let revision = 0, previews = 0, release = false, resume;
  const invoke = async (name, args) => {
    calls.push({ name, args });
    if (name === 'nemo_task_runtime') return { pid: options.foreign ? 43 : 42, taskKey: config.taskKey, windows: ['main'] };
    if (name === 'nemo_test_object_window_probe') { const payload = JSON.parse(args.payloadJson); phases.push(payload); if (payload.phase === options.blockAt) await new Promise(resolve => { resume = resolve; }); return; }
    if (name === 'nemo_mcp_identity') return { instanceId: 'owned-instance' };
    if (name === 'nemo_native_object_bootstrap') { const request = JSON.parse(args.requestJson); assert.equal(request.instanceId, 'owned-instance'); return {}; }
    if (name === 'nemo_native_object_client_status') return { available: true, instanceId: 'owned-instance', documentId: 'owned-doc', contentRevision: revision, lifecycleGeneration: 7 };
    if (name === 'nemo_native_object_dispatch') {
      const request = JSON.parse(args.requestJson);
      assert.equal(request.instanceId, 'owned-instance'); assert.equal(request.documentId, 'owned-doc');
      if (request.operation.startsWith('command.')) { assert.equal(request.expectedRevision, revision); revision++; }
      return { ok: true, instanceId: 'owned-instance', documentId: 'owned-doc', result: { documentSnapshotId: 'snapshot-' + revision } };
    }
    if (name === 'nemo_native_object_preview') {
      const request = JSON.parse(args.requestJson); assert.equal(request.contentRevision, revision); assert.equal(request.documentSnapshotId, 'snapshot-' + revision);
      assert.deepEqual(request.outputSpec, { kind: 'frame', format: 'rgba8', width: 320, height: 180, colorInterpretation: 'srgb', alphaMode: 'straight' });
      previews++; if (options.failPreview) throw new Error('not-presented');
      return { status: options.defer && previews === 1 ? 'deferred-timeout' : 'presented', workId: 'work-' + revision, viewGeneration: 1 };
    }
    if (name === 'nemo_native_release') { assert.equal(args.request.expectedRevision, revision); release = true; return { status: 'succeeded', reentryAvailable: true }; }
    throw new Error('unknown invoke');
  };
  const window = { __NEMO_OBJECT_WINDOW_PROBE_CONFIG__: { ...config, ...options.config }, __TAURI__: { core: { invoke } }, devicePixelRatio: 2 };
  vm.runInNewContext(script, { window, setTimeout: fn => { Promise.resolve().then(fn); return 1; } });
  for (let tick = 0; tick < 200; tick++) {
    await Promise.resolve();
    if (phases.some(p => p.phase === 'released' || p.phase === 'failed')) break;
  }
  return { calls, phases, previews, release, window, resume };
}
test('fixed original-window scenario uses actual pins and exact blue/red/history/release order', async () => {
  const result = await scenario();
  assert.deepEqual(result.phases.map(p => p.phase), ['ready', 'blue', 'red', 'undo', 'redo', 'released']);
  assert.deepEqual(result.phases.filter(p => p.workId).map(p => p.revision), [0, 1, 2, 3]);
  assert.equal(result.release, true); assert.equal(result.previews, 4);
  const bootstrap = JSON.parse(result.calls.find(c => c.name === 'nemo_native_object_bootstrap').args.requestJson);
  const document = JSON.parse(bootstrap.documentJson);
  assert.equal(document.formatVersion, 2); assert.deepEqual(document.objects[0].fill, { kind: 'solid', r: 0, g: 0, b: 1, a: 1 });
  const changes = result.calls.filter(c => c.name === 'nemo_native_object_dispatch').map(c => JSON.parse(c.args.requestJson)).filter(r => r.operation.startsWith('command.'));
  assert.deepEqual(changes.map(r => r.operation), ['command.document.object.fill.set', 'command.document.object.undo', 'command.document.object.redo']);
  assert.deepEqual(changes[0].payload.fill, { kind: 'solid', r: 1, g: 0, b: 0, a: 1 });
  assert.equal(result.calls.filter(c => c.name === 'nemo_native_object_bootstrap').length, 1);
});
test('deferred frame retries use the same source pins and never reinstall owner', async () => {
  const result = await scenario({ defer: true }); assert.equal(result.previews, 5);
  const previews = result.calls.filter(c => c.name === 'nemo_native_object_preview');
  assert.deepEqual(previews[0].args, previews[1].args);
  assert.equal(result.calls.filter(c => c.name === 'nemo_native_object_bootstrap').length, 1);
});
test('failure never reports paint success and performs common native cleanup', async () => {
  const result = await scenario({ failPreview: true });
  assert.deepEqual(result.phases.map(p => p.phase), ['ready', 'failed']); assert.equal(result.release, true);
  assert.equal(result.phases[1].status, 'operation_failed'); assert.equal(result.phases[1].cleanupSucceeded, true);
});
test('foreign runtime and arbitrary or malformed configuration cannot invoke document operations', async () => {
  const foreign = await scenario({ foreign: true });
  assert.equal(foreign.calls.some(c => c.name === 'nemo_native_object_bootstrap'), false);
  for (const config of [{ eval: 'arbitrary' }, { sourceSha: 'invalid' }, { pid: 0 }, { runId: 'invalid' }]) {
    const result = await scenario({ config }); assert.equal(result.calls.length, 0);
  }
});

test('a visual checkpoint prevents the next mutation until its owned acknowledgment', async () => {
  const result = await scenario({ blockAt: 'blue' });
  assert.deepEqual(result.phases.map(p => p.phase), ['ready', 'blue']);
  const mutations = () => result.calls.filter(c => c.name === 'nemo_native_object_dispatch' && JSON.parse(c.args.requestJson).operation.startsWith('command.'));
  assert.equal(mutations().length, 0); assert.equal(typeof result.resume, 'function');
  result.resume();
  for (let tick = 0; tick < 100 && !result.phases.some(p => p.phase === 'released'); tick++) await Promise.resolve();
  assert.equal(mutations().length, 3); assert.equal(result.phases.at(-1).phase, 'released');
});
