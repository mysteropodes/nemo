'use strict';

const assert = require('node:assert/strict');
const test = require('node:test');

const watched = ['__TAURI__', 'NemoNativeOpacityPreviewAdapter',
  'NemoNativeOpacityExportAdapter', 'NemoApplication', 'SMMotion'];
const before = new Map(watched.map((name) => [name, globalThis[name]]));
const preview = require('../src/js/adapters/native-opacity-preview.js');
const exporter = require('../src/js/adapters/native-opacity-export.js');
const { validateRequest } = require('../src/js/adapters/native-application.js');

const FRAME = Object.freeze({ width: 320, height: 180 });
const OUTPUT_SPEC = Object.freeze({
  kind: 'frame', format: 'rgba8', width: 320, height: 180,
  colorInterpretation: 'srgb', alphaMode: 'straight',
});
const GEOMETRY = Object.freeze({ resourceId: 'geometry-1', resourceVersion: 'v1' });
const RECTANGLES = Object.freeze([
  Object.freeze({ bounds: Object.freeze({ x: 20, y: 60, width: 20, height: 20 }), rgba: Object.freeze([255, 204, 204, 255]) }),
  Object.freeze({ bounds: Object.freeze({ x: 84, y: 60, width: 20, height: 20 }), rgba: Object.freeze([255, 128, 128, 255]) }),
  Object.freeze({ bounds: Object.freeze({ x: 148, y: 60, width: 20, height: 20 }), rgba: Object.freeze([255, 51, 51, 255]) }),
]);
const FIXTURE_FRAMES = Object.freeze([0, 10, 20].map((frame) => Object.freeze({ frame, coloredPixels: 400 })));
const PREVIEW_METADATA = Object.freeze({
  documentSnapshotId: 'snapshot-0', documentId: 'opacity-document', contentRevision: 0,
  contextId: 'scene-root', frame: 0, quality: 'final', outputSpec: OUTPUT_SPEC,
  geometryHandle: GEOMETRY,
});

function pick(mod, names) {
  const name = names.find((key) => typeof mod[key] === 'function');
  assert.ok(name, `adapter must export one of ${names.join(', ')}`);
  return mod[name];
}
function frozen(value) {
  if (value && typeof value === 'object') {
    assert.equal(Object.isFrozen(value), true);
    Object.values(value).forEach(frozen);
  }
}
test('preview/export modules are dormant CommonJS modules with no legacy activation', () => {
  for (const [name, value] of before) assert.strictEqual(globalThis[name], value, name);
  frozen(preview); frozen(exporter);
});

test('fixture-level F0/F10/F20 compositional oracle is explicit and independent', () => {
  assert.deepEqual(FRAME, { width: 320, height: 180 });
  assert.deepEqual(FIXTURE_FRAMES, [{ frame: 0, coloredPixels: 400 }, { frame: 10, coloredPixels: 400 }, { frame: 20, coloredPixels: 400 }]);
  assert.deepEqual(RECTANGLES.map((item) => [item.bounds.x, item.bounds.y, item.bounds.x + item.bounds.width, item.bounds.y + item.bounds.height]),
    [[20, 60, 40, 80], [84, 60, 104, 80], [148, 60, 168, 80]]);
  assert.deepEqual(RECTANGLES.map((item) => item.rgba), [[255, 204, 204, 255], [255, 128, 128, 255], [255, 51, 51, 255]]);
  frozen(FRAME); frozen(RECTANGLES); frozen(OUTPUT_SPEC);
});

test('preview binds immutable metadata, discards stale work, presents newest, and is idempotent', () => {
  const create = pick(preview, ['createNativeOpacityPreviewAdapter', 'createPreviewAdapter']);
  const adapter = create();
  const one = adapter.register(PREVIEW_METADATA, { workId: 'work-1', viewGeneration: 1 });
  const two = adapter.register(PREVIEW_METADATA, { workId: 'work-2', viewGeneration: 2 });
  frozen(one); frozen(two);
  assert.equal(one.workId, 'work-1'); assert.equal(two.viewGeneration, 2);
  assert.throws(() => adapter.receive({ workId: one.workId, viewGeneration: one.viewGeneration, status: 'presented' }), /stale|older|generation/i);
  assert.deepEqual(adapter.receive({ workId: one.workId, viewGeneration: one.viewGeneration, status: 'stale-discarded' }).status, 'stale-discarded');
  const latest = adapter.receive({ workId: two.workId, viewGeneration: two.viewGeneration, status: 'presented' });
  assert.strictEqual(adapter.receive({ workId: two.workId, viewGeneration: two.viewGeneration, status: 'presented' }), latest);
  assert.throws(() => adapter.receive({ workId: two.workId, viewGeneration: two.viewGeneration, status: 'failed-validation' }), /conflict|terminal/i);
  assert.throws(() => adapter.receive({ workId: two.workId, viewGeneration: two.viewGeneration, status: 'unknown' }), /unknown|status/i);
  assert.throws(() => adapter.register({ ...PREVIEW_METADATA, documentSnapshotId: 'other' }, { workId: 'work-2', viewGeneration: 2 }), /rebound|generation/i);
  adapter.dispose(); adapter.dispose();
  assert.throws(() => adapter.register(PREVIEW_METADATA, { workId: 'work-3', viewGeneration: 3 }), /disposed/i);
});

test('preview/export inputs reject recursive pixels and never traffic full-frame JS data', () => {
  const createPreview = pick(preview, ['createNativeOpacityPreviewAdapter', 'createPreviewAdapter']);
  const adapter = createPreview();
  const reject = (value) => assert.throws(() => adapter.register({ ...PREVIEW_METADATA, payload: value }, { workId: 'bad', viewGeneration: 1 }), /forbidden|pixel|binary|payload|canvas|data/i);
  reject(Buffer.from([1])); reject(new Uint8Array([1])); reject({ nested: { blob: 'blob:n18' } });
  reject({ imageData: { data: [1, 2, 3] } }); reject('data:image/png;base64,AA==');
  const createExport = pick(exporter, ['createNativeOpacityExportAdapter', 'createExportAdapter']);
  const exportAdapter = createExport();
  const identity = { instanceId: 'n18-fixture', documentId: 'opacity-document', contentRevision: 0 };
  const snapshot = { apiVersion: 2, requestId: 'snapshot-raw', instanceId: 'n18-fixture', documentId: 'opacity-document', contentRevision: 0, ok: true,
    result: { atRevision: 0, documentSnapshotId: 'snapshot-0' } };
  assert.throws(() => exportAdapter.begin(identity, snapshot, 'raw-buffer', 'out', [{ sourceFrame: 0,
    geometryHandle: { resourceId: 'geometry-1', resourceVersion: 'v1', bytes: Buffer.from([1]) } }]), /pixel|binary|payload|unknown/i);
  assert.throws(() => exportAdapter.begin(identity, snapshot, 'raw-canvas', 'out', [{ sourceFrame: 0,
    geometryHandle: { resourceId: 'geometry-1', resourceVersion: 'v1', canvas: {} } }]), /pixel|canvas|payload|unknown/i);
  assert.throws(() => exportAdapter.begin(identity, snapshot, 'raw-url', 'data:image/png;base64,AA==', [{ sourceFrame: 0,
    geometryHandle: GEOMETRY }]), /binary URL|outputHandle|identifier/i);
  assert.equal(Object.keys(exporter).some((key) => /pixel|rgba|imageData|canvas/i.test(key)), false);
});

test('export begin/status/cancel use v2 validation, pin snapshot/revision, and retain the plan after UI edits', () => {
  const create = pick(exporter, ['createNativeOpacityExportAdapter', 'createExportAdapter']);
  const adapter = create();
  const identity = { instanceId: 'n18-fixture', documentId: 'opacity-document', contentRevision: 0 };
  const snapshotResponse = { apiVersion: 2, requestId: 'snapshot-req', instanceId: 'n18-fixture', documentId: 'opacity-document', contentRevision: 0, ok: true,
    result: { atRevision: 0, documentSnapshotId: 'snapshot-0' } };
  const frames = [0, 10, 20].map((sourceFrame) => ({ sourceFrame, geometryHandle: { resourceId: 'geometry-1', resourceVersion: 'v1' } }));
  const begin = adapter.begin(identity, snapshotResponse, 'begin-1', 'output-1', frames);
  frozen(begin); assert.equal(begin.expectedRevision, 0); assert.equal(begin.payload.operation, undefined);
  assert.doesNotThrow(() => validateRequest(begin));
  const retained = JSON.stringify(begin);
  frames[0].sourceFrame = 99; identity.contentRevision = 9; snapshotResponse.result.atRevision = 9;
  assert.equal(JSON.stringify(begin), retained, 'export plan is immutable after simulated UI edits');
  const runningResponse = { apiVersion: 2, requestId: begin.requestId, instanceId: 'n18-fixture', documentId: 'opacity-document', contentRevision: 0, ok: true,
    result: { jobId: 'job-1', status: 'running', pinnedRevision: 0, documentSnapshotId: 'snapshot-0', progress: 0, artifact: null, cleanup: { status: 'pending' }, externalEffectDisposition: 'none' } };
  const session = adapter.observe(begin, runningResponse);
  frozen(session); assert.equal(session.snapshot.documentSnapshotId, 'snapshot-0');
  assert.deepEqual(session.outputSpec, OUTPUT_SPEC);
  const status = adapter.status(identity, 'status-1', 'job-1');
  const cancel = adapter.cancel(identity, 'cancel-1', 'job-1');
  assert.deepEqual([status.operation, cancel.operation], ['job.export.png.status', 'job.export.png.cancel']);
  assert.deepEqual([begin.operation, status.operation, cancel.operation], ['job.export.png.begin', 'job.export.png.status', 'job.export.png.cancel']);
  assert.doesNotThrow(() => validateRequest(status));
  assert.doesNotThrow(() => validateRequest(cancel));
  assert.strictEqual(adapter.plan('job-1'), session);
  const cleanIdentity = { instanceId: 'n18-fixture', documentId: 'opacity-document', contentRevision: 0 };
  const cleanSnapshot = { ...snapshotResponse, result: { ...snapshotResponse.result, atRevision: 0 } };
  assert.throws(() => adapter.begin(cleanIdentity, cleanSnapshot, 'bad', 'output-1', [{ sourceFrame: 0, geometryHandle: { resourceId: 'g', resourceVersion: 'v' }, extra: true }]), /unknown|fields|validation/i);
});

test('oversized encoded begin envelopes are rejected before an invalid v2 request can escape', () => {
  const create = pick(exporter, ['createNativeOpacityExportAdapter', 'createExportAdapter']);
  const adapter = create();
  const identity = { instanceId: 'n18-fixture', documentId: 'opacity-document', contentRevision: 0 };
  const snapshot = { apiVersion: 2, requestId: 'snapshot-size', instanceId: 'n18-fixture', documentId: 'opacity-document', contentRevision: 0, ok: true,
    result: { atRevision: 0, documentSnapshotId: 'snapshot-0' } };
  const oversizedFrames = Array.from({ length: 120 }, (_, sourceFrame) => ({
    sourceFrame,
    geometryHandle: { resourceId: 'g'.repeat(128), resourceVersion: 'v'.repeat(128) },
  }));
  assert.throws(() => adapter.begin(identity, snapshot, 'begin-oversized', 'out', oversizedFrames), /4096|encoded|size|bounded/i);
});

test('a running jobId cannot rebind to a different pending immutable export plan', () => {
  const create = pick(exporter, ['createNativeOpacityExportAdapter', 'createExportAdapter']);
  const adapter = create();
  const identity = { instanceId: 'n18-fixture', documentId: 'opacity-document', contentRevision: 0 };
  const snapshot = { apiVersion: 2, requestId: 'snapshot-rebind', instanceId: 'n18-fixture', documentId: 'opacity-document', contentRevision: 0, ok: true,
    result: { atRevision: 0, documentSnapshotId: 'snapshot-0' } };
  const first = adapter.begin(identity, snapshot, 'begin-rebind-1', 'out-1', [{ sourceFrame: 0, geometryHandle: { resourceId: 'g-1', resourceVersion: 'v1' } }]);
  const firstReceipt = { jobId: 'job-rebind', status: 'running', pinnedRevision: 0, documentSnapshotId: 'snapshot-0',
    progress: 0.25, artifact: null, cleanup: { status: 'pending' }, externalEffectDisposition: 'none' };
  const firstSession = adapter.observe(first, { apiVersion: 2, requestId: first.requestId, instanceId: 'n18-fixture', documentId: 'opacity-document', contentRevision: 0, ok: true,
    result: firstReceipt });
  const duplicate = adapter.observe(first, { apiVersion: 2, requestId: first.requestId, instanceId: 'n18-fixture', documentId: 'opacity-document', contentRevision: 0, ok: true,
    result: firstReceipt });
  assert.deepEqual(duplicate, firstSession, 'identical observations retain the same immutable session value');

  const second = adapter.begin(identity, snapshot, 'begin-rebind-2', 'out-2', [{ sourceFrame: 10, geometryHandle: { resourceId: 'g-2', resourceVersion: 'v2' } }]);
  assert.throws(() => adapter.observe(second, { apiVersion: 2, requestId: second.requestId, instanceId: 'n18-fixture', documentId: 'opacity-document', contentRevision: 0, ok: true,
    result: firstReceipt }), /rebind|retained|immutable|job/i);
  assert.strictEqual(adapter.plan('job-rebind'), firstSession, 'the original plan remains authoritative');
});

test('running receipts cannot carry cleanup errors while cleanup is pending', () => {
  const create = pick(exporter, ['createNativeOpacityExportAdapter', 'createExportAdapter']);
  const adapter = create();
  const identity = { instanceId: 'n18-fixture', documentId: 'opacity-document', contentRevision: 0 };
  const snapshot = { apiVersion: 2, requestId: 'snapshot-cleanup-error', instanceId: 'n18-fixture', documentId: 'opacity-document', contentRevision: 0, ok: true,
    result: { atRevision: 0, documentSnapshotId: 'snapshot-0' } };
  const begin = adapter.begin(identity, snapshot, 'begin-cleanup-error', 'out', [{ sourceFrame: 0, geometryHandle: { resourceId: 'g', resourceVersion: 'v' } }]);
  assert.throws(() => adapter.observe(begin, { apiVersion: 2, requestId: begin.requestId, instanceId: 'n18-fixture', documentId: 'opacity-document', contentRevision: 0, ok: true,
    result: { jobId: 'job-cleanup-error', status: 'running', pinnedRevision: 0, documentSnapshotId: 'snapshot-0', progress: 0.1,
      artifact: null, cleanup: { status: 'pending', error: { code: 'cleanup_failed', message: 'cleanup is not terminal' } }, externalEffectDisposition: 'none' } }), /running|cleanup|pending|error/i);
});

test('job progress cannot regress when a running receipt advances into a terminal state', () => {
  const create = pick(exporter, ['createNativeOpacityExportAdapter', 'createExportAdapter']);
  const identity = { instanceId: 'n18-fixture', documentId: 'opacity-document', contentRevision: 0 };
  const snapshot = { apiVersion: 2, requestId: 'snapshot-progress', instanceId: 'n18-fixture', documentId: 'opacity-document', contentRevision: 0, ok: true,
    result: { atRevision: 0, documentSnapshotId: 'snapshot-0' } };
  for (const [status, terminal] of [
    ['failed', { cleanup: { status: 'complete' }, externalEffectDisposition: 'none', error: { code: 'frame_failed', message: 'frame failed' } }],
    ['cancelled', { cleanup: { status: 'complete' }, externalEffectDisposition: 'none' }],
  ]) {
    const adapter = create();
    const begin = adapter.begin(identity, snapshot, `begin-progress-${status}`, 'out', [{ sourceFrame: 0, geometryHandle: { resourceId: 'g', resourceVersion: 'v' } }]);
    adapter.observe(begin, { apiVersion: 2, requestId: begin.requestId, instanceId: 'n18-fixture', documentId: 'opacity-document', contentRevision: 0, ok: true,
      result: { jobId: `job-progress-${status}`, status: 'running', pinnedRevision: 0, documentSnapshotId: 'snapshot-0', progress: 0.75,
        artifact: null, cleanup: { status: 'pending' }, externalEffectDisposition: 'none' } });
    assert.throws(() => adapter.observe(begin, { apiVersion: 2, requestId: begin.requestId, instanceId: 'n18-fixture', documentId: 'opacity-document', contentRevision: 0, ok: true,
      result: { jobId: `job-progress-${status}`, status, pinnedRevision: 0, documentSnapshotId: 'snapshot-0', progress: 0.5,
        artifact: null, ...terminal } }), /progress|regress|receipt|terminal/i);
  }
});

test('job receipts cover running/succeeded/cancelled/failed cleanup and external-effect invariants without retry ownership', () => {
  const create = pick(exporter, ['createNativeOpacityExportAdapter', 'createExportAdapter']);
  const identity = { instanceId: 'n18-fixture', documentId: 'opacity-document', contentRevision: 0 };
  const snapshot = { apiVersion: 2, requestId: 'snapshot', instanceId: 'n18-fixture', documentId: 'opacity-document', contentRevision: 0, ok: true,
    result: { atRevision: 0, documentSnapshotId: 'snapshot-0' } };
  const frames = [{ sourceFrame: 0, geometryHandle: { resourceId: 'g', resourceVersion: 'v' } }];
  for (const [status, cleanup, artifact, effect, error] of [
    ['running', { status: 'pending' }, null, 'none'],
    ['succeeded', { status: 'complete' }, { target: 'out', files: ['frame.png'] }, 'committed'],
    ['cancelled', { status: 'complete' }, null, 'none'],
    ['failed', { status: 'failed', error: { code: 'cleanup_failed', message: 'cleanup failed' } }, null, 'indeterminate', { code: 'cleanup_failed', message: 'cleanup failed' }],
  ]) {
    const adapter = create();
    const begin = adapter.begin(identity, snapshot, `begin-${status}`, 'out', frames);
    const receipt = { jobId: `job-${status}`, status, pinnedRevision: 0, documentSnapshotId: 'snapshot-0',
      progress: status === 'succeeded' ? 1 : 0, artifact, cleanup, externalEffectDisposition: effect, ...(error ? { error } : {}) };
    const observed = adapter.observe(begin, { apiVersion: 2, requestId: begin.requestId, instanceId: 'n18-fixture', documentId: 'opacity-document', contentRevision: 0, ok: true, result: receipt });
    assert.equal(observed.receipt.status, status); frozen(observed.receipt);
    assert.equal(observed.snapshot.documentSnapshotId, 'snapshot-0');
  }
  const adapter = create();
  const begin = adapter.begin(identity, snapshot, 'invalid-begin', 'out', frames);
  const invalid = { jobId: 'bad-job', status: 'failed', pinnedRevision: 0, documentSnapshotId: 'snapshot-0', progress: 1,
    artifact: null, cleanup: { status: 'failed' }, externalEffectDisposition: 'none' };
  assert.throws(() => adapter.observe(begin, { apiVersion: 2, requestId: begin.requestId, instanceId: 'n18-fixture', documentId: 'opacity-document', contentRevision: 0, ok: true, result: invalid }), /cleanup|indeterminate|error/i);

  const retryAdapter = create();
  const retryBegin = retryAdapter.begin(identity, snapshot, 'no-retry-begin', 'out', frames);
  const failed = retryAdapter.observe(retryBegin, { apiVersion: 2, requestId: retryBegin.requestId,
    instanceId: 'n18-fixture', documentId: 'opacity-document', contentRevision: 1, ok: false,
    error: { code: 'stale_revision', message: 'current revision changed', details: { currentRevision: 1 } } });
  frozen(failed); assert.equal(failed.error.code, 'stale_revision');
  assert.throws(() => retryAdapter.observe(retryBegin, { apiVersion: 2, requestId: retryBegin.requestId,
    instanceId: 'n18-fixture', documentId: 'opacity-document', contentRevision: 1, ok: true,
    result: { jobId: 'late-job', status: 'running', pinnedRevision: 0, documentSnapshotId: 'snapshot-0',
      progress: 0, artifact: null, cleanup: { status: 'pending' }, externalEffectDisposition: 'none' } }), /retained export plan/);
});

test('preview metadata and export frame plan share identity but do not claim end-to-end attestation', () => {
  const previewAdapter = pick(preview, ['createNativeOpacityPreviewAdapter', 'createPreviewAdapter'])();
  const exportAdapter = pick(exporter, ['createNativeOpacityExportAdapter', 'createExportAdapter'])();
  const bindings = [0, 10, 20].map((frame, index) => previewAdapter.register(
    { ...PREVIEW_METADATA, frame }, { workId: `work-${index + 1}`, viewGeneration: index + 1 }));
  const identity = { instanceId: 'n18-fixture', documentId: 'opacity-document', contentRevision: 0 };
  const snapshot = { apiVersion: 2, requestId: 'snapshot-parity', instanceId: 'n18-fixture', documentId: 'opacity-document', contentRevision: 0, ok: true,
    result: { atRevision: 0, documentSnapshotId: 'snapshot-0' } };
  const frames = [0, 10, 20].map((sourceFrame) => ({ sourceFrame, geometryHandle: GEOMETRY }));
  const begin = exportAdapter.begin(identity, snapshot, 'begin-parity', 'out', frames);
  const session = exportAdapter.observe(begin, { apiVersion: 2, requestId: begin.requestId,
    instanceId: 'n18-fixture', documentId: 'opacity-document', contentRevision: 0, ok: true,
    result: { jobId: 'job-parity', status: 'running', pinnedRevision: 0, documentSnapshotId: 'snapshot-0',
      progress: 0, artifact: null, cleanup: { status: 'pending' }, externalEffectDisposition: 'none' } });
  for (const [index, binding] of bindings.entries()) {
    assert.equal(binding.documentSnapshotId, session.snapshot.documentSnapshotId);
    assert.equal(binding.contentRevision, session.snapshot.atRevision);
    assert.equal(binding.contextId, session.request.payload.contextId);
    assert.equal(binding.frame, session.request.payload.frames[index].sourceFrame);
    assert.deepEqual(binding.geometryHandle, session.request.payload.frames[index].geometryHandle);
    assert.deepEqual(binding.outputSpec, session.outputSpec);
    assert.equal(binding.quality, session.request.payload.quality);
    assert.equal(Object.hasOwn(binding, 'pixelAttestation'), false);
  }
  assert.equal(Object.hasOwn(session, 'pixelAttestation'), false);
  assert.deepEqual(FIXTURE_FRAMES.map((item) => item.frame), session.request.payload.frames.map((item) => item.sourceFrame));
});
