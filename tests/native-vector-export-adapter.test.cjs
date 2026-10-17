'use strict';
const assert = require('node:assert/strict');
const test = require('node:test');
const { createNativeVectorExportAdapter } = require('../src/js/adapters/native-vector-export-adapter.js');
const { oracle, identity, snapshotResponse, SNAPSHOT, names, receipts, response, failure } = require('./native-media-export-support.cjs');

const live = () => createNativeVectorExportAdapter({ route: true });
const begin = (adapter, id, frames, handle) => adapter.begin(identity(), snapshotResponse(), id, { outputHandle: handle || 'output/a', frames });
const deepFrozen = (value) => value === null || typeof value !== 'object' || (Object.isFrozen(value) && Object.values(value).every(deepFrozen));

test('begin mints one frozen, exact v2 request pinned to the snapshot', () => {
  const adapter = live();
  const request = begin(adapter, 'svg-1', [0, 1, 2]);
  assert.deepEqual(request, {
    apiVersion: 2, requestId: 'svg-1', instanceId: 'instance-a', documentId: 'native-document-1', expectedRevision: 7,
    operation: 'job.export.svg.begin',
    payload: { documentSnapshotId: SNAPSHOT, contextId: 'scene-root', outputHandle: 'output/a', frames: [0, 1, 2] },
  });
  assert.ok(deepFrozen(request));
  assert.deepEqual(Object.keys(request), ['apiVersion', 'requestId', 'instanceId', 'documentId', 'expectedRevision', 'operation', 'payload']);
  const frames = [4, 9];
  const copy = begin(adapter, 'svg-2', frames);
  frames.push(10); frames[0] = 99;
  assert.deepEqual(copy.payload.frames, [4, 9], 'the request never aliases the caller array');
});

test('without a registered host route every operation is typed unavailability and mints nothing', () => {
  for (const adapter of [createNativeVectorExportAdapter(), createNativeVectorExportAdapter({}), createNativeVectorExportAdapter({ route: false })]) {
    assert.deepEqual(adapter.availability(), { state: 'unavailable', reason: 'export_route_unavailable' });
    for (const call of [() => begin(adapter, 'svg-1', [0]), () => adapter.status(identity(), 'svg-2', 'job-1'), () => adapter.cancel(identity(), 'svg-3', 'job-1')]) {
      assert.throws(call, (error) => error.name === 'NativeExportUnavailable' && error.code === 'unavailable' && error.reason === 'export_route_unavailable');
    }
    assert.equal(adapter.plan('job-1'), null);
  }
  assert.deepEqual(live().availability(), { state: 'available', reason: null });
  assert.throws(() => createNativeVectorExportAdapter({ route: 'yes' }), TypeError);
  assert.throws(() => createNativeVectorExportAdapter({ route: true, extra: 1 }), TypeError);
  assert.throws(() => createNativeVectorExportAdapter(null), TypeError);
});

test('input is closed, bounded and free of raw payloads', () => {
  const adapter = live();
  const cyc = []; cyc.push(cyc);
  const accessor = { outputHandle: 'output/a' }; Object.defineProperty(accessor, 'frames', { enumerable: true, get: () => [0] });
  const tooMany = Array.from({ length: 33 }, (_, i) => i);
  let n = 0;
  const refuse = (input, label) => assert.throws(() => adapter.begin(identity(), snapshotResponse(), `bad-${n++}`, input), TypeError, label);
  for (const [label, frames] of [
    ['empty', []], ['33 frames', tooMany], ['descending', [3, 2]], ['duplicate', [1, 1]], ['negative', [-1]], ['fraction', [1.5]], ['above u32', [4294967296]],
    ['not an array', 'abc'], ['string item', ['1']], ['null item', [null]], ['NaN', [NaN]], ['cyclic', cyc],
  ]) refuse({ outputHandle: 'output/a', frames }, label);
  for (const [label, input] of [
    ['unknown member', { outputHandle: 'output/a', frames: [0], extra: 1 }], ['missing frames', { outputHandle: 'output/a' }], ['missing handle', { frames: [0] }],
    ['bad handle', { outputHandle: 'a b', frames: [0] }], ['data handle', { outputHandle: 'data:image/svg+xml,x', frames: [0] }], ['null', null], ['array', [0]],
    ['accessor', accessor], ['callback', { outputHandle: 'output/a', frames: [0], onProgress() {} }], ['pixels', { outputHandle: 'output/a', frames: new Uint8Array(2) }],
  ]) refuse(input, label);
  assert.equal(adapter.begin(identity(), snapshotResponse(), 'ok-32', { outputHandle: 'output/a', frames: Array.from({ length: 32 }, (_, i) => 1000 + i * 3) }).payload.frames.length, 32);
  assert.equal(adapter.begin(identity(), snapshotResponse(), 'ok-max', { outputHandle: 'output/a', frames: [4294967295] }).payload.frames[0], 4294967295);
});

test('the pinned identity and snapshot must agree before anything is minted', () => {
  const adapter = live();
  const run = (id, ident, snap) => adapter.begin(ident, snap, id, { outputHandle: 'output/a', frames: [0] });
  assert.throws(() => run('s-1', identity(), snapshotResponse({ documentId: 'other' })), /does not match/);
  assert.throws(() => run('s-2', identity(), snapshotResponse({ instanceId: 'other' })), /does not match/);
  assert.throws(() => run('s-3', identity(), snapshotResponse({ contentRevision: 8 })), /does not match/);
  assert.throws(() => run('s-4', identity(), snapshotResponse({ ok: false })), /does not match/);
  assert.throws(() => run('s-5', identity(), snapshotResponse({ apiVersion: 1 })), /does not match/);
  assert.throws(() => run('s-6', identity(), snapshotResponse({ result: { atRevision: 6, documentSnapshotId: SNAPSHOT } })), /snapshot revision/);
  assert.throws(() => run('s-7', identity(), snapshotResponse({ result: { atRevision: 7, documentSnapshotId: SNAPSHOT, extra: 1 } })), TypeError);
  assert.throws(() => run('s-8', identity(), snapshotResponse({ result: { atRevision: 7, documentSnapshotId: 'a b' } })), TypeError);
  assert.throws(() => run('s-9', { ...identity(), contentRevision: -1 }, snapshotResponse()), TypeError);
  assert.throws(() => run('s-10', { instanceId: 'instance-a', documentId: 'native-document-1' }, snapshotResponse()), TypeError);
  assert.throws(() => run('s 11', identity(), snapshotResponse()), TypeError);
  assert.equal(run('s-12', identity(), snapshotResponse()).requestId, 's-12');
});

test('a request id is used once, and a refused begin does not consume it', () => {
  const adapter = live();
  begin(adapter, 'svg-1', [0]);
  assert.throws(() => begin(adapter, 'svg-1', [0]), /reuse/);
  assert.throws(() => adapter.begin(identity(), snapshotResponse(), 'svg-2', { outputHandle: 'a b', frames: [0] }), TypeError);
  assert.equal(begin(adapter, 'svg-2', [0]).requestId, 'svg-2', 'the refused id was not remembered');
  const widest = { instanceId: 'i'.repeat(128), documentId: 'd'.repeat(128), contentRevision: 9007199254740991 };
  const snap = snapshotResponse({ instanceId: widest.instanceId, documentId: widest.documentId, contentRevision: widest.contentRevision, result: { atRevision: widest.contentRevision, documentSnapshotId: 's'.repeat(128) } });
  const request = adapter.begin(widest, snap, 'r'.repeat(128), { outputHandle: 'o'.repeat(128), frames: Array.from({ length: 32 }, (_, i) => 4294967263 + i) });
  assert.ok(JSON.stringify(request).length < 4096, 'the widest legal SVG begin stays inside the staged bound');
});

test('running then succeeded: the retained plan is the pinned snapshot and the independent file manifest', () => {
  const adapter = live();
  const request = begin(adapter, 'svg-1', [3, 4, 5]);
  const running = adapter.observe(request, response(request, receipts.running()));
  assert.equal(running.receipt.status, 'running');
  assert.deepEqual(running.expectation, { kind: 'frames', ext: 'svg', count: 3 });
  assert.deepEqual(running.snapshot, { documentId: 'native-document-1', atRevision: 7, documentSnapshotId: SNAPSHOT });
  const statusRequest = adapter.status(identity(), 'svg-2', 'native-export-job-1');
  assert.deepEqual(statusRequest, {
    apiVersion: 2, requestId: 'svg-2', instanceId: 'instance-a', documentId: 'native-document-1', operation: 'job.export.svg.status', payload: { jobId: 'native-export-job-1' },
  });
  assert.ok(deepFrozen(statusRequest) && !('expectedRevision' in statusRequest));
  const done = adapter.observe(statusRequest, response(statusRequest, receipts.succeeded(names(3, 'svg'))));
  assert.equal(done.receipt.status, 'succeeded');
  assert.deepEqual(done.receipt.artifact, { target: 'output/a', files: ['frame_0001.svg', 'frame_0002.svg', 'frame_0003.svg'] });
  assert.equal(adapter.plan('native-export-job-1'), done);
  assert.ok(deepFrozen(done));
  assert.equal(adapter.observe(statusRequest, response(statusRequest, receipts.succeeded(names(3, 'svg')))), done, 'an identical receipt is idempotent');
});

test('a succeeded receipt that does not match the plan is refused and the retained receipt stays', () => {
  for (const [label, files, over, pattern] of [
    ['one file missing', names(2, 'svg'), {}, /count/], ['one file extra', names(4, 'svg'), {}, /count/], ['PNG names', names(3, 'png'), {}, /names/],
    ['swapped order', ['frame_0002.svg', 'frame_0001.svg', 'frame_0003.svg'], {}, /names/], ['zero based', ['frame_0000.svg', 'frame_0001.svg', 'frame_0002.svg'], {}, /names/],
    ['other target', names(3, 'svg'), { artifact: { target: 'output/b', files: names(3, 'svg') } }, /output handle/],
    ['other snapshot', names(3, 'svg'), { documentSnapshotId: 'native-object:native-document-1:6' }, /pinned snapshot/],
    ['other revision', names(3, 'svg'), { pinnedRevision: 6 }, /pinned snapshot/],
  ]) {
    const adapter = live();
    const request = begin(adapter, 'svg-1', [3, 4, 5]);
    adapter.observe(request, response(request, receipts.running()));
    const statusRequest = adapter.status(identity(), 'svg-2', 'native-export-job-1');
    assert.throws(() => adapter.observe(statusRequest, response(statusRequest, receipts.succeeded(files, over))), pattern, label);
    assert.equal(adapter.plan('native-export-job-1').receipt.status, 'running', label);
  }
});

test('lifecycle: progress never regresses, a terminal receipt is final, a job id cannot be rebound', () => {
  const adapter = live();
  const request = begin(adapter, 'svg-1', [0, 1]);
  adapter.observe(request, response(request, receipts.running({ progress: 0.5 })));
  const s1 = adapter.status(identity(), 'svg-2', 'native-export-job-1');
  assert.throws(() => adapter.observe(s1, response(s1, receipts.running({ progress: 0.25 }))), /regress/);
  const s2 = adapter.status(identity(), 'svg-3', 'native-export-job-1');
  adapter.observe(s2, response(s2, receipts.failed()));
  const s3 = adapter.status(identity(), 'svg-4', 'native-export-job-1');
  assert.throws(() => adapter.observe(s3, response(s3, receipts.succeeded(names(2, 'svg')))), /terminal job receipt conflicts/);
  assert.throws(() => adapter.observe(s3, response(s3, receipts.running())), /conflicts/);
  const other = begin(adapter, 'svg-5', [0, 1]);
  assert.throws(() => adapter.observe(other, response(other, receipts.running())), /rebound/);
  const mismatch = adapter.status(identity(), 'svg-6', 'native-export-job-1');
  assert.throws(() => adapter.observe(mismatch, response(mismatch, receipts.running({ jobId: 'native-export-job-2' }))), /observed jobId/);
});

test('cancel: contained cancellation, document replacement and cleanup debt keep their exact dispositions', () => {
  const adapter = live();
  const request = begin(adapter, 'svg-1', [0, 1]);
  adapter.observe(request, response(request, receipts.running()));
  const cancel = adapter.cancel(identity(), 'svg-2', 'native-export-job-1');
  assert.equal(cancel.operation, 'job.export.svg.cancel');
  const cancelled = adapter.observe(cancel, response(cancel, receipts.cancelled({ error: { code: 'document_replaced', message: 'replaced' } })));
  assert.equal(cancelled.receipt.status, 'cancelled');
  assert.equal(cancelled.receipt.artifact, null);
  assert.equal(cancelled.receipt.externalEffectDisposition, 'none');
  const debt = { code: 'cleanup_failed', message: 'debt' };
  const second = live();
  const r2 = begin(second, 'svg-1', [0]);
  const withDebt = second.observe(r2, response(r2, receipts.failed({ cleanup: { status: 'failed', error: debt }, externalEffectDisposition: 'indeterminate', error: debt })));
  assert.equal(withDebt.receipt.externalEffectDisposition, 'indeterminate');
  assert.equal(withDebt.receipt.cleanup.status, 'failed');
  const third = live();
  const r3 = begin(third, 'svg-1', [0]);
  third.observe(r3, response(r3, receipts.running()));
  const c3 = third.cancel(identity(), 'svg-2', 'native-export-job-1');
  assert.throws(() => third.observe(c3, response(c3, receipts.cancelled({ artifact: { target: 'output/a', files: names(1, 'svg') } }))), /cannot expose an artifact/);
  assert.throws(() => third.observe(c3, response(c3, receipts.cancelled({ externalEffectDisposition: 'committed' }))), /cancelled job requires/);
});

test('status and cancel are bound to the identity that started the job', () => {
  const adapter = live();
  const request = begin(adapter, 'svg-1', [0]);
  assert.throws(() => adapter.status(identity(), 'svg-2', 'native-export-job-1'), /not bound/, 'no receipt has named the job yet');
  adapter.observe(request, response(request, receipts.running()));
  assert.throws(() => adapter.status({ ...identity(), documentId: 'other' }, 'svg-3', 'native-export-job-1'), /not bound/);
  assert.throws(() => adapter.status({ ...identity(), instanceId: 'other' }, 'svg-4', 'native-export-job-1'), /not bound/);
  assert.throws(() => adapter.status(identity(), 'svg-5', 'unknown-job'), /not bound/);
  assert.throws(() => adapter.cancel(identity(), 'svg-6', 'a b'), TypeError);
  assert.equal(adapter.plan('unknown-job'), null);
  assert.equal(adapter.status({ ...identity(), contentRevision: 99 }, 'svg-7', 'native-export-job-1').requestId, 'svg-7', 'a job survives later revisions');
});

test('failed dispatch responses are typed; a refused begin leaves no retained plan', () => {
  const adapter = live();
  const request = begin(adapter, 'svg-1', [0]);
  const refusal = adapter.observe(request, failure(request, { code: 'unavailable', message: 'no decoder', details: { reason: 'media_decoder_unavailable', frame: 4 } }));
  assert.equal(refusal.ok, false);
  assert.deepEqual(refusal.error, { code: 'unavailable', message: 'no decoder', details: { reason: 'media_decoder_unavailable', frame: 4 } });
  assert.ok(deepFrozen(refusal));
  assert.throws(() => adapter.observe(request, response(request, receipts.running())), /no retained export plan/);
  for (const [label, error] of [
    ['job code', { code: 'frame_failed', message: 'm' }], ['unknown code', { code: 'teapot', message: 'm' }], ['unknown reason', { code: 'not_found', message: 'm', details: { reason: 'nope' } }],
    ['extra member', { code: 'not_found', message: 'm', path: '/x' }], ['no message', { code: 'not_found' }],
  ]) {
    const r = begin(adapter, `f-${label.replace(/ /g, '-')}`, [0]);
    assert.throws(() => adapter.observe(r, failure(r, error)), TypeError, label);
  }
  const both = begin(adapter, 'both', [0]);
  assert.throws(() => adapter.observe(both, { ...failure(both, { code: 'internal', message: 'm' }), result: {} }), /only error/);
  const none = begin(adapter, 'none', [0]);
  assert.throws(() => adapter.observe(none, { ...response(none, receipts.running()), error: { code: 'internal', message: 'm' } }), /only result/);
});

test('only this adapter\'s own frozen request can be observed, against its own identity', () => {
  const adapter = live();
  const request = begin(adapter, 'svg-1', [0]);
  const forged = { ...request };
  assert.throws(() => adapter.observe(forged, response(forged, receipts.running())), /frozen request/);
  const alien = live();
  assert.throws(() => alien.observe(request, response(request, receipts.running())), /frozen request/);
  for (const over of [{ requestId: 'svg-9' }, { instanceId: 'other' }, { documentId: 'other' }, { apiVersion: 3 }, { contentRevision: -1 }, { contentRevision: 1.5 }]) {
    assert.throws(() => adapter.observe(request, response(request, receipts.running(), over)), /response identity/, JSON.stringify(over));
  }
  assert.throws(() => adapter.observe(request, { ...response(request, receipts.running()), extra: 1 }), TypeError);
  assert.throws(() => adapter.observe(request, response(request, receipts.running(), { ok: 'yes' })), /successful response/);
});

test('no pixel, URL or handle payload crosses either direction', () => {
  const adapter = live();
  const request = begin(adapter, 'svg-1', [0]);
  for (const [label, result] of [
    ['data URL file', receipts.succeeded(['data:image/svg+xml;base64,AAAA'])], ['blob target', receipts.succeeded(names(1, 'svg'), { artifact: { target: 'blob:x', files: names(1, 'svg') } })],
    ['bytes', { ...receipts.running(), artifact: new Uint8Array(4) }], ['canvas', { ...receipts.running(), canvas: {} }],
  ]) assert.throws(() => adapter.observe(request, response(request, result)), TypeError, label);
  assert.throws(() => adapter.observe(request, response(request, receipts.running(), { requestId: 'svg-1', extraBlob: new Blob(['x']) })), TypeError);
  assert.equal(adapter.plan('native-export-job-1'), null, 'a refused receipt retains nothing');
});

test('the frame count and names follow the baseline numbering for every oracle range (G43 manifest)', () => {
  for (const row of oracle.png) {
    const count = row.names.length;
    if (count < 1 || count > 32) continue;
    const adapter = live();
    const frames = Array.from({ length: count }, (_, i) => row.start + i);
    const request = begin(adapter, 'o-' + row.start, frames);
    adapter.observe(request, response(request, receipts.running()));
    const statusRequest = adapter.status(identity(), 's-' + row.start, 'native-export-job-1');
    const svgNames = row.names.map((name) => name.replace(/\.png$/, '.svg'));
    const bent = svgNames.slice(); bent[count - 1] = 'frame_9999.svg';
    if (count > 1) assert.throws(() => adapter.observe(statusRequest, response(statusRequest, receipts.succeeded(bent))), /names/, row.start + '..' + row.end);
    const done = adapter.observe(statusRequest, response(statusRequest, receipts.succeeded(svgNames)));
    assert.deepEqual(done.receipt.artifact.files, svgNames, row.start + '..' + row.end);
    assert.equal(done.expectation.count, count);
  }
});
