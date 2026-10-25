'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const path = require('node:path');
const script = fs.readFileSync(path.join(__dirname, 'object-window-probe-bootstrap.js'), 'utf8');
const config = { runId: '11111111-1111-4111-8111-111111111111', pid: 42, taskKey: 'a'.repeat(64), sourceSha: 'b'.repeat(40) };
async function scenario(options = {}) {
  const calls = [], phases = []; let revision = 0, previews = 0, release = false, resume, statusCalls = 0, sourceDocument;
  const invoke = async (name, args) => {
    calls.push({ name, args });
    if (options.failInvoke === name) throw (Object.hasOwn(options, 'rejection') ? options.rejection : new Error('private arbitrary error must not escape'));
    if (name === 'nemo_task_runtime') return { pid: options.foreign ? 43 : 42, taskKey: config.taskKey, windows: ['main'] };
    if (name === 'nemo_test_object_window_probe') { const payload = JSON.parse(args.payloadJson); phases.push(payload); if (payload.phase === options.failCheckpoint) throw new Error('checkpoint refused'); if (payload.phase === options.blockAt) await new Promise(resolve => { resume = resolve; }); return; }
    if (name === 'nemo_mcp_identity') return { instanceId: options.badIdentity ? null : 'owned-instance' };
    if (name === 'nemo_native_object_bootstrap') { const request = JSON.parse(args.requestJson); assert.equal(request.instanceId, 'owned-instance'); sourceDocument = JSON.parse(request.documentJson); return { instanceId: 'owned-instance', documentId: options.badBootstrap ? null : 'owned-doc', contentRevision: 0 }; }
    if (name === 'nemo_native_object_client_status') { statusCalls++; if (options.failStatusAlways || (options.failFirstStatus && statusCalls === 1) || (options.failStatusAfterEdit && revision > 0)) throw new Error('status unavailable'); return { available: !options.badStatus, instanceId: 'owned-instance', documentId: 'owned-doc', contentRevision: revision, lifecycleGeneration: 7 }; }
    if (name === 'nemo_native_object_dispatch') {
      const request = JSON.parse(args.requestJson);
      assert.equal(request.instanceId, 'owned-instance'); assert.equal(request.documentId, 'owned-doc');
      if ((options.failRead || options.failQuery === request.payload.stableTarget?.strokeId) && request.operation === 'query.document.object') throw new Error('query unavailable');
      if (options.failMutation && options.failMutation === request.payload.command) throw new Error('command unavailable');
      if (request.operation.startsWith('command.')) { assert.equal(request.expectedRevision, revision); revision++; }
      const result = { documentSnapshotId: 'snapshot-' + revision };
      if (request.operation === 'query.document.object') {
        const record = sourceDocument.objects.find(object => object.target.strokeId === request.payload.stableTarget.strokeId);
        result.object = structuredClone(record);
        if (record.target.strokeId === 'a' && (revision === 1 || revision === 3)) result.object.fill = { kind: 'solid', r: 1, g: 0, b: 0, a: 1 };
        if ((options.corruptRecord === record.target.strokeId && revision === 1) || options.badQueryRecord === record.target.strokeId) result.object.geometry.segments[0].point.x += 1;
      }
      if (options.badQuerySnapshot && options.badQuerySnapshot === request.payload.stableTarget?.strokeId) result.documentSnapshotId = '';
      if (options.foreignSiblingSnapshot && request.payload.stableTarget?.strokeId === 'b') result.documentSnapshotId = 'foreign-snapshot';
      const response = { apiVersion: 2, requestId: request.requestId, ok: true, instanceId: 'owned-instance', documentId: 'owned-doc', contentRevision: revision, result };
      if (options.badQueryCorrelation && options.badQueryCorrelation === request.payload.stableTarget?.strokeId) response.requestId = 'foreign';
      if (options.corruptCorrelation && revision === 1) response[options.corruptCorrelation] = options.corruptCorrelation === 'contentRevision' ? revision + 1 : 'foreign';
      return response;
    }
    if (name === 'nemo_native_object_preview') {
      const request = JSON.parse(args.requestJson); assert.equal(request.contentRevision, revision); assert.equal(request.documentSnapshotId, 'snapshot-' + revision);
      assert.deepEqual(request.outputSpec, { kind: 'frame', format: 'rgba8', width: 320, height: 180, colorInterpretation: 'srgb', alphaMode: 'straight' });
      previews++; if (options.failPreview) throw new Error('not-presented');
      return { status: options.badPreviewReceipt ? 'failed-validation' : options.defer && previews === 1 ? 'deferred-timeout' : 'presented', workId: 'work-' + revision, viewGeneration: 1 };
    }
    if (name === 'nemo_native_release') { assert.equal(args.request.instanceId, 'owned-instance'); assert.equal(args.request.documentId, 'owned-doc'); assert.equal(args.request.expectedRevision, revision); if (options.failRelease) throw new Error('release unavailable'); release = true; return { status: options.badReleaseReceipt ? 'failed' : 'succeeded', reentryAvailable: true }; }
    throw new Error('unknown invoke');
  };
  const window = { __NEMO_OBJECT_WINDOW_PROBE_CONFIG__: { ...config, ...options.config }, __TAURI__: { core: { invoke } }, devicePixelRatio: options.dpr ?? 2 };
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
  assert.ok(result.phases.every(p => !Object.hasOwn(p, 'failureStage')));
  const bootstrap = JSON.parse(result.calls.find(c => c.name === 'nemo_native_object_bootstrap').args.requestJson);
  const document = JSON.parse(bootstrap.documentJson);
  assert.equal(document.formatVersion, 2); assert.deepEqual(document.objects[0].fill, { kind: 'solid', r: 0, g: 0, b: 1, a: 1 });
  assert.equal(document.objects.length, 2); assert.equal(document.objects[1].target.strokeId, 'b');
  assert.deepEqual(document.objects[1].fill, { kind: 'solid', r: 0, g: 1, b: 0, a: 1 });
  const reads = result.calls.filter(c => c.name === 'nemo_native_object_dispatch').map(c => JSON.parse(c.args.requestJson)).filter(r => r.operation === 'query.document.object');
  assert.deepEqual(reads.map(r => [r.payload.atRevision, r.payload.stableTarget.strokeId]), [[0, 'a'], [0, 'b'], [1, 'a'], [1, 'b'], [2, 'a'], [2, 'b'], [3, 'a'], [3, 'b']]);
  const changes = result.calls.filter(c => c.name === 'nemo_native_object_dispatch').map(c => JSON.parse(c.args.requestJson)).filter(r => r.operation.startsWith('command.'));
  assert.deepEqual(changes.map(r => r.operation), ['command.document.object.fill.set', 'command.document.object.undo', 'command.document.object.redo']);
  assert.deepEqual(changes[0].payload.fill, { kind: 'solid', r: 1, g: 0, b: 0, a: 1 });
  assert.equal(result.calls.filter(c => c.name === 'nemo_native_object_bootstrap').length, 1);
});
test('corrupt target or unchanged sibling geometry cannot earn the next visual checkpoint', async () => {
  for (const corruptRecord of ['a', 'b']) {
    const result = await scenario({ corruptRecord });
    assert.deepEqual(result.phases.map(p => p.phase), ['ready', 'blue', 'failed']);
    assert.equal(result.release, true); assert.equal(result.previews, 1);
    assert.equal(result.phases.at(-1).failureStage, (corruptRecord === 'a' ? 'primary' : 'sibling') + '-query-record');
  }
});
test('every response identity and exact revision must correlate before a checkpoint', async () => {
  for (const corruptCorrelation of ['apiVersion', 'requestId', 'instanceId', 'documentId', 'contentRevision']) {
    const result = await scenario({ corruptCorrelation });
    assert.deepEqual(result.phases.map(p => p.phase), ['ready', 'blue', 'failed']);
    assert.equal(result.release, true); assert.equal(result.previews, 1);
    assert.equal(result.phases.at(-1).failureStage, 'fill-correlation');
  }
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

test('bootstrap commit is released even when status or first object read fails before pins', async () => {
  for (const options of [{ failFirstStatus: true }, { failStatusAlways: true }, { failRead: true }]) {
    const result = await scenario(options);
    assert.deepEqual(result.phases.map(p => p.phase), ['ready', 'failed']);
    assert.equal(result.previews, 0); assert.equal(result.release, true);
    assert.equal(result.phases.at(-1).cleanupSucceeded, true);
    const releases = result.calls.filter(c => c.name === 'nemo_native_release');
    assert.equal(releases.length, 1); assert.equal(releases[0].args.request.expectedRevision, 0);
    assert.equal(releases[0].args.request.documentId, 'owned-doc');
  }
});
test('early status failure with failed cleanup remains failed and never claims pixels or release', async () => {
  const result = await scenario({ failStatusAlways: true, failRelease: true });
  assert.deepEqual(result.phases.map(p => p.phase), ['ready', 'failed']);
  assert.equal(result.previews, 0); assert.equal(result.release, false);
  assert.equal(result.phases.at(-1).cleanupSucceeded, false);
  assert.equal(result.phases.at(-1).failureStage, 'status-invoke');
  assert.equal(result.calls.filter(c => c.name === 'nemo_native_release').length, 1);
});

test('committed edit revision remains cleanup authority when the subsequent status is unavailable', async () => {
  const result = await scenario({ failStatusAfterEdit: true });
  assert.deepEqual(result.phases.map(p => p.phase), ['ready', 'blue', 'failed']);
  assert.equal(result.release, true); assert.equal(result.phases.at(-1).cleanupSucceeded, true);
  const release = result.calls.find(c => c.name === 'nemo_native_release');
  assert.equal(release.args.request.documentId, 'owned-doc'); assert.equal(release.args.request.expectedRevision, 1);
});

// Inject only test-stub failures; the production scenario/config remains fixed.
test('fixed failure stages distinguish invokes, pins, records, snapshots, preview and mutations', async () => {
  const cases = [
    [{ failInvoke: 'nemo_task_runtime' }, 'runtime-invoke'],
    [{ foreign: true }, 'runtime-pins'],
    [{ failCheckpoint: 'ready' }, 'ready-checkpoint'],
    [{ failInvoke: 'nemo_mcp_identity' }, 'identity-invoke'],
    [{ badIdentity: true }, 'identity-pins'],
    [{ failInvoke: 'nemo_native_object_bootstrap' }, 'bootstrap-invoke'],
    [{ badBootstrap: true }, 'bootstrap-receipt'],
    [{ failFirstStatus: true }, 'status-invoke'], [{ badStatus: true }, 'status-pins'],
    [{ failQuery: 'a' }, 'primary-query-invoke'], [{ failQuery: 'b' }, 'sibling-query-invoke'],
    [{ badQueryCorrelation: 'a' }, 'primary-query-correlation'], [{ badQueryCorrelation: 'b' }, 'sibling-query-correlation'],
    [{ badQueryRecord: 'a' }, 'primary-query-record'], [{ badQueryRecord: 'b' }, 'sibling-query-record'],
    [{ badQuerySnapshot: 'a' }, 'primary-query-snapshot'], [{ badQuerySnapshot: 'b' }, 'sibling-query-snapshot'],
    [{ foreignSiblingSnapshot: true }, 'sibling-query-snapshot'],
    [{ dpr: 5 }, 'preview-dpr'], [{ failPreview: true }, 'preview-invoke'],
    [{ badPreviewReceipt: true }, 'preview-receipt'],
    [{ failMutation: 'object.fill.set' }, 'fill-invoke'],
    [{ failMutation: 'object.undo' }, 'undo-invoke'], [{ failMutation: 'object.redo' }, 'redo-invoke'],
    [{ failRelease: true }, 'cleanup-invoke'], [{ badReleaseReceipt: true }, 'cleanup-receipt'],
  ];
  for (const [options, stage] of cases) {
    const result = await scenario(options);
    assert.equal(result.phases.at(-1)?.phase, 'failed', stage);
    assert.equal(result.phases.at(-1).failureStage, stage);
    assert.ok(!JSON.stringify(result.phases).includes('private arbitrary error'));
  }
});
test('cleanup refusal cannot replace the original primary or sibling failure stage', async () => {
  for (const target of ['a', 'b']) {
    const result = await scenario({ badQueryRecord: target, failRelease: true });
    assert.equal(result.phases.at(-1).cleanupSucceeded, false);
    assert.equal(result.phases.at(-1).failureStage, (target === 'a' ? 'primary' : 'sibling') + '-query-record');
  }
});
test('only actual recognized HostResult code fields suffix invoke failures and survive cleanup', async () => {
  for (const code of ['invalid_request', 'unavailable', 'wrong_instance', 'wrong_document',
    'stale_revision', 'stale_owner', 'cancelled_before_dispatch', 'cleanup_failed', 'revision_unacknowledged']) {
    const result = await scenario({ failInvoke: 'nemo_native_object_preview',
      rejection: { code, message: 'private native message', details: { private: 'body' } }, failRelease: true });
    assert.equal(result.phases.at(-1).failureStage, 'preview-invoke:' + code);
    assert.equal(result.phases.at(-1).cleanupSucceeded, false);
    assert.ok(!JSON.stringify(result.phases).includes('private'));
  }
  for (const rejection of ['invalid_request', '{"code":"invalid_request"}', null,
    { code: 'arbitrary-private' }, { code: 'invalid_request:unavailable' },
    { error: { code: 'invalid_request' } }, { get code() { throw Error('private getter'); } }]) {
    const result = await scenario({ failInvoke: 'nemo_native_object_preview', rejection });
    assert.equal(result.phases.at(-1).failureStage, 'preview-invoke');
  }
});
