'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const script = fs.readFileSync(path.join(__dirname, 'object-export-probe-bootstrap.js'), 'utf8');
const runId = '11111111-1111-4111-8111-111111111111';
const destinationDirectory = '/private/tmp/nemo-runtime/tasks/owned/reports/object-export-' + runId;
const config = { runId, pid: 42, taskKey: 'a'.repeat(64), sourceSha: 'b'.repeat(40), destinationDirectory };
async function scenario(options = {}) {
  const calls = [], phases = [], exports = [];
  let resume, released = false;
  const invoke = async (name, args = {}) => {
    calls.push({ name, args });
    if (name === 'nemo_task_runtime') return { pid: options.foreign ? 43 : 42, taskKey: config.taskKey, windows: ['main'] };
    if (name === 'nemo_test_object_window_probe') {
      const packet = JSON.parse(args.payloadJson); phases.push(packet);
      if (packet.phase === options.blockAt) await new Promise(resolve => { resume = resolve; });
      return;
    }
    if (name === 'nemo_mcp_identity') return { instanceId: 'owned-instance' };
    if (name === 'nemo_native_object_bootstrap') {
      const request = JSON.parse(args.requestJson);
      assert.equal(request.instanceId, 'owned-instance');
      assert.deepEqual(JSON.parse(request.documentJson).objects[0].fill,
        { kind: 'solid', r: 0, g: 0, b: 1, a: 1 });
      return { instanceId: 'owned-instance', documentId: 'owned-document', contentRevision: 0 };
    }
    if (name === 'nemo_native_object_client_status') return { available: true, instanceId: 'owned-instance',
      documentId: 'owned-document', contentRevision: 0, lifecycleGeneration: 7 };
    if (name === 'nemo_native_object_dispatch') {
      const request = JSON.parse(args.requestJson);
      assert.equal(request.operation, 'query.document.object');
      assert.equal(request.payload.atRevision, 0);
      return { ok: true, instanceId: 'owned-instance', documentId: 'owned-document',
        result: { documentSnapshotId: 'owned-snapshot' } };
    }
    if (name === 'nemo_native_object_preview') {
      const request = JSON.parse(args.requestJson);
      assert.equal(request.documentSnapshotId, 'owned-snapshot');
      assert.equal(request.contentRevision, 0);
      return { status: options.failPreview ? 'failed' : 'presented', workId: 'native-work-1' };
    }
    if (name === 'nemo_native_object_export') {
      const request = JSON.parse(args.requestJson); exports.push(request);
      assert.equal(request.destinationDirectory, destinationDirectory);
      assert.equal(request.contentRevision, 0);
      assert.equal(request.lifecycleGeneration, 7);
      assert.deepEqual(request.outputSpec, { kind: 'frame', format: 'rgba8', width: 320,
        height: 180, colorInterpretation: 'srgb', alphaMode: 'straight' });
      if (request.documentSnapshotId.endsWith('-wrong')) throw new Error('snapshot refused');
      if (options.failExport) throw new Error('export failed');
      return { apiVersion: 2, requestId: request.requestId, instanceId: request.instanceId,
        documentId: request.documentId, contentRevision: 0, documentSnapshotId: 'owned-snapshot',
        status: 'succeeded', cleanupStatus: 'complete', externalEffectDisposition: 'committed',
        errorCode: null, artifactFiles: ['frame_0001.png'], published: !options.falsePublish,
        cleanupComplete: true };
    }
    if (name === 'nemo_native_release') {
      assert.equal(args.request.expectedRevision, 0);
      if (options.failRelease) throw new Error('release failed');
      released = true; return { status: 'succeeded', reentryAvailable: true };
    }
    throw new Error('unexpected invoke ' + name);
  };
  const window = { __NEMO_OBJECT_EXPORT_PROBE_CONFIG__: { ...config, ...options.config },
    __TAURI__: { core: { invoke } }, devicePixelRatio: 2 };
  vm.runInNewContext(script, { window, setTimeout: fn => { Promise.resolve().then(fn); return 1; } });
  for (let tick = 0; tick < 300 && !phases.some(p => p.phase === 'released' || p.phase === 'failed'); tick++) await Promise.resolve();
  return { calls, phases, exports, released, resume };
}
test('fixed owned original export uses presented pins, refuses stale snapshot, then publishes one artifact', async () => {
  const result = await scenario();
  assert.deepEqual(result.phases.map(p => p.phase), ['ready', 'blue', 'refused', 'exported', 'released']);
  assert.equal(result.phases[1].workId, 'native-work-1');
  assert.equal(result.phases[3].export.published, true);
  assert.deepEqual(result.phases[3].export.artifactFiles, ['frame_0001.png']);
  assert.equal(result.exports.length, 2);
  assert.equal(result.exports[0].documentSnapshotId, 'owned-snapshot-wrong');
  assert.equal(result.exports[1].documentSnapshotId, 'owned-snapshot');
  assert.equal(result.exports[0].destinationDirectory, result.exports[1].destinationDirectory);
  assert.equal(result.released, true);
});
test('blue checkpoint blocks both export invocations until external owned ACK', async () => {
  const result = await scenario({ blockAt: 'blue' });
  assert.deepEqual(result.phases.map(p => p.phase), ['ready', 'blue']);
  assert.equal(result.exports.length, 0);
  assert.equal(typeof result.resume, 'function'); result.resume();
  for (let tick = 0; tick < 150 && !result.phases.some(p => p.phase === 'released'); tick++) await Promise.resolve();
  assert.equal(result.exports.length, 2);
  assert.equal(result.phases.at(-1).phase, 'released');
});
test('foreign runtime, arbitrary config and failed export never claim publication', async () => {
  assert.equal((await scenario({ foreign: true })).calls.some(c => c.name === 'nemo_native_object_bootstrap'), false);
  for (const bad of [{ eval: 'arbitrary' }, { sourceSha: 'wrong' }, { destinationDirectory: '/tmp/foreign' }]) {
    assert.equal((await scenario({ config: bad })).calls.length, 0);
  }
  for (const options of [{ failPreview: true }, { failExport: true }, { falsePublish: true }]) {
    const result = await scenario(options);
    assert.equal(result.phases.at(-1).phase, 'failed');
    assert.equal(result.phases.some(p => p.phase === 'exported'), false);
    assert.equal(result.released, true);
  }
  const failedRelease = await scenario({ failExport: true, failRelease: true });
  assert.equal(failedRelease.phases.at(-1).cleanupSucceeded, false);
});
