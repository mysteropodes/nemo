'use strict';
const assert = require('node:assert/strict');
const test = require('node:test');
const { createNativeMediaExportAdapter, AUDIO_DISPOSITION } = require('../src/js/adapters/native-media-export-adapter.js');
const { createNativeVectorExportAdapter } = require('../src/js/adapters/native-vector-export-adapter.js');
const core = require('../src/js/adapters/native-media-export-core.js');
const { oracle, identity, snapshotResponse, SNAPSHOT, names, receipts, response, failure } = require('./native-media-export-support.cjs');

const live = () => createNativeMediaExportAdapter({ route: true });
const SIZE = { width: 64, height: 36 };
const spec = { kind: 'frame', format: 'rgba8', width: 64, height: 36, colorInterpretation: 'srgb', alphaMode: 'straight' };
// An override of `undefined` removes the member: absent is not the same as an own member holding undefined.
const input = (over) => {
  const value = Object.assign({ outputHandle: 'output/a', kind: 'sequence', frameFormat: 'png', range: { start: 2, end: 5 }, size: SIZE }, over);
  for (const key of Object.keys(value)) if (value[key] === undefined) delete value[key];
  return value;
};
const begin = (adapter, id, over) => adapter.begin(identity(), snapshotResponse(), id, input(over));
const deepFrozen = (value) => value === null || typeof value !== 'object' || (Object.isFrozen(value) && Object.values(value).every(deepFrozen));
function running(adapter, request) {
  adapter.observe(request, response(request, receipts.running()));
  return adapter.plan('native-export-job-1');
}
let statusSerial = 0;
function finish(adapter, files) {
  const statusRequest = adapter.status(identity(), `status-${statusSerial++}`, 'native-export-job-1');
  return adapter.observe(statusRequest, response(statusRequest, receipts.succeeded(files)));
}

test('begin mints one frozen, exact v2 request for a PNG sequence, an SVG still and a video', () => {
  const adapter = live();
  const sequence = begin(adapter, 'media-1');
  assert.deepEqual(sequence, {
    apiVersion: 2, requestId: 'media-1', instanceId: 'instance-a', documentId: 'native-document-1', expectedRevision: 7, operation: 'job.export.media.begin',
    payload: {
      documentSnapshotId: SNAPSHOT, contextId: 'scene-root', outputHandle: 'output/a', output: { kind: 'sequence', frameFormat: 'png' },
      range: { start: 2, end: 5 }, outputSpec: spec,
    },
  });
  assert.ok(deepFrozen(sequence));
  const still = begin(adapter, 'media-2', { kind: 'still', frameFormat: 'svg', range: { start: 3, end: 3 }, size: undefined });
  assert.deepEqual(still.payload, {
    documentSnapshotId: SNAPSHOT, contextId: 'scene-root', outputHandle: 'output/a', output: { kind: 'still', frameFormat: 'svg' }, range: { start: 3, end: 3 },
  });
  const movie = begin(adapter, 'media-3', { kind: 'video', range: {}, video: { fps: 29.97, quality: 'best' } });
  assert.deepEqual(movie.payload.range, {});
  assert.deepEqual(movie.payload.video, { fps: 29.97, quality: 'best' });
  assert.deepEqual(movie.payload.output, { kind: 'video', frameFormat: 'png' });
  assert.deepEqual(begin(adapter, 'media-4', { kind: 'video', range: { start: 0 } }).payload.video, undefined);
  assert.deepEqual(begin(adapter, 'media-5', { kind: 'video', range: {}, video: {} }).payload.video, {});
});

test('absent and zero stay distinct (baseline exportFrameRange): an explicit 0 is a value, not the work-area fallback', () => {
  const adapter = live();
  const r = (range, id) => begin(adapter, id, { range, kind: 'video' }).payload.range;
  assert.deepEqual(r({ start: 0 }, 'z-1'), { start: 0 });
  assert.deepEqual(r({ end: 0 }, 'z-2'), { end: 0 });
  assert.deepEqual(r({ start: 0, end: 0 }, 'z-3'), { start: 0, end: 0 });
  assert.deepEqual(r({}, 'z-4'), {});
  assert.equal(Object.keys(r({ end: 9 }, 'z-5')).includes('start'), false);
  // every oracle row: the presence of each member is kept, and a fully explicit range fixes the file count.
  for (const row of oracle.range) {
    const range = {};
    if (row.start !== null) range.start = row.start;
    if (row.end !== null) range.end = row.end;
    const both = row.start !== null && row.end !== null;
    const label = JSON.stringify(row);
    if (both && row.end < row.start) { assert.throws(() => begin(live(), 'o-1', { range }), TypeError, label); continue; }
    const fresh = live();
    const request = begin(fresh, 'o-1', { range });
    assert.deepEqual(request.payload.range, range, label);
    const plan = running(fresh, request);
    if (both) {
      assert.equal(plan.expectation.count, row.outEnd - row.outStart + 1, label);
      assert.equal(plan.expectation.count, row.end - row.start + 1, label);
    } else assert.equal(plan.expectation.count, null, label);
  }
});

test('input is closed and every kind / format / range / size / video rule is enforced locally', () => {
  const adapter = live();
  let n = 0;
  const refuse = (over, label) => assert.throws(() => begin(adapter, `bad-${n++}`, over), TypeError, label);
  refuse({ kind: 'movie' }, 'unknown kind');
  refuse({ kind: undefined }, 'missing kind');
  refuse({ frameFormat: 'gif' }, 'unknown format');
  refuse({ kind: 'video', frameFormat: 'svg', size: undefined }, 'a video of SVG frames');
  refuse({ kind: 'still', range: {} }, 'a still without a frame');
  refuse({ kind: 'still', range: { start: 3 } }, 'a still with one member');
  refuse({ kind: 'still', range: { start: 3, end: 4 } }, 'a still of two frames');
  refuse({ range: { start: 5, end: 4 } }, 'sequence end before start');
  refuse({ kind: 'video', range: { start: 5, end: 4 } }, 'video end before start');
  refuse({ range: { start: 0, end: 120 } }, '121 frames in one receipt');
  refuse({ range: { start: -1 } }, 'negative start');
  refuse({ range: { end: 1.5 } }, 'fractional end');
  refuse({ range: { start: 4294967296 } }, 'start above u32');
  refuse({ range: { first: 1 } }, 'unknown range member');
  refuse({ range: { start: '1' } }, 'string start');
  refuse({ range: undefined }, 'missing range');
  refuse({ size: undefined }, 'PNG frames without a size');
  refuse({ frameFormat: 'svg' }, 'SVG frames with a size');
  for (const size of [{ width: 0, height: 36 }, { width: 64, height: 4097 }, { width: 64.5, height: 36 }, { width: '64', height: 36 }, { width: 64 }, { width: 64, height: 36, depth: 1 }, null]) refuse({ size }, 'size ' + JSON.stringify(size));
  refuse({ video: { fps: 24 } }, 'video options on a sequence');
  for (const video of [{ fps: -1 }, { fps: NaN }, { fps: Infinity }, { fps: '24' }, { quality: 'a b' }, { quality: 'x'.repeat(33) }, { quality: 7 }, { codec: 'x' }, null]) refuse({ kind: 'video', video }, 'video ' + String(JSON.stringify(video)));
  refuse({ audio: true }, 'audio member');
  refuse({ documentSnapshotId: 'other' }, 'snapshot override');
  refuse({ outputSpec: spec }, 'outputSpec passed through');
  refuse({ outputHandle: 'data:image/png;base64,AA' }, 'data handle');
  refuse({ outputHandle: '' }, 'empty handle');
  refuse({ onProgress() {} }, 'callback');
  assert.throws(() => adapter.begin(identity(), snapshotResponse(), 'undef-1', { ...input(), size: undefined }), TypeError, 'an own member holding undefined is not absence');
  assert.equal(begin(adapter, 'edge-1', { range: { start: 0, end: 119 } }).payload.range.end, 119);
  assert.equal(begin(adapter, 'edge-2', { size: { width: 4096, height: 1 } }).payload.outputSpec.width, 4096);
  assert.equal(begin(adapter, 'edge-3', { kind: 'video', range: { start: 0, end: 4294967295 } }).payload.range.end, 4294967295);
  assert.equal(begin(adapter, 'edge-4', { kind: 'video', video: { fps: 0, quality: '' } }).payload.video.quality, '', 'fps 0 and an empty quality fall back to the project values natively');
});

test('without a registered host route every operation is typed unavailability and mints nothing', () => {
  const adapter = createNativeMediaExportAdapter();
  assert.deepEqual(adapter.availability(), { state: 'unavailable', reason: 'export_route_unavailable' });
  for (const call of [() => begin(adapter, 'm-1'), () => adapter.status(identity(), 'm-2', 'job-1'), () => adapter.cancel(identity(), 'm-3', 'job-1')]) {
    assert.throws(call, (error) => error.name === 'NativeExportUnavailable' && error.code === 'unavailable' && error.reason === 'export_route_unavailable');
  }
  assert.throws(() => createNativeMediaExportAdapter({ route: 1 }), TypeError);
  assert.equal(AUDIO_DISPOSITION, 'silent_baseline');
});

test('a sequence receipt must list exactly the planned frames; an open range accepts only a contiguous manifest', () => {
  const known = live();
  running(known, begin(known, 'k-1'));
  assert.throws(() => finish(known, names(3, 'png')), /count/);
  assert.throws(() => finish(known, names(4, 'svg')), /names/);
  assert.throws(() => finish(known, ['frame_0001.png', 'frame_0002.png', 'frame_0004.png', 'frame_0005.png']), /names/);
  assert.deepEqual(finish(known, names(4, 'png')).receipt.artifact.files, names(4, 'png'));
  const open = live();
  running(open, begin(open, 'k-2', { range: {} }));
  assert.throws(() => finish(open, []), /1..120/);
  assert.throws(() => finish(open, names(121, 'png')), /1..120/);
  assert.throws(() => finish(open, ['frame_0002.png']), /names/);
  assert.equal(finish(open, names(48, 'png')).receipt.artifact.files.length, 48);
  const svg = live();
  running(svg, begin(svg, 'k-3', { frameFormat: 'svg', size: undefined, range: { start: 0, end: 1 } }));
  assert.throws(() => finish(svg, names(2, 'png')), /names/);
  assert.equal(finish(svg, names(2, 'svg')).expectation.ext, 'svg');
});

test('a still publishes frame_0001 in its format; a video publishes one file', () => {
  const still = live();
  running(still, begin(still, 's-1', { kind: 'still', range: { start: 9, end: 9 } }));
  assert.throws(() => finish(still, names(2, 'png')), /count/);
  assert.throws(() => finish(still, ['frame_0009.png']), /names/);
  assert.deepEqual(finish(still, ['frame_0001.png']).receipt.artifact.files, ['frame_0001.png']);
  const movie = live();
  const request = begin(movie, 'v-1', { kind: 'video', range: { start: 0, end: 4294967295 } });
  assert.deepEqual(running(movie, request).expectation, { kind: 'video' });
  assert.throws(() => finish(movie, ['a.mp4', 'b.mp4']), /exactly one/);
  assert.throws(() => finish(movie, []), /exactly one/);
  assert.deepEqual(finish(movie, ['movie.mp4']).receipt.artifact, { target: 'output/a', files: ['movie.mp4'] });
});

test('missing links, missing decoders and refused placements are typed refusals, never a frame', () => {
  const cases = [
    ['not_found', 'link_missing', 5], ['unavailable', 'link_permission', 0], ['unavailable', 'media_facts_unavailable', 12], ['invalid_request', 'media_placement_refused', 3],
    ['invalid_request', 'media_plan_invalid', undefined], ['invalid_request', 'media_source_frame_invalid', 4294967295], ['unavailable', 'media_decoder_unavailable', 7],
  ];
  assert.deepEqual(cases.map((c) => c[1]).sort(), [...core.REASONS].sort());
  for (const [code, reason, frame] of cases) {
    const adapter = live();
    const request = begin(adapter, 'x-1');
    const details = frame === undefined ? { reason } : { reason, frame };
    const refused = adapter.observe(request, failure(request, { code, message: 'refused before staging', details }));
    assert.equal(refused.ok, false);
    assert.deepEqual(refused.error, { code, message: 'refused before staging', details });
    assert.equal(adapter.plan('native-export-job-1'), null, 'a refusal retains no job');
    assert.throws(() => adapter.observe(request, response(request, receipts.succeeded(names(4, 'png')))), /no retained export plan/, 'and a later success for that request is not believed');
  }
});

test('exports are silent: no audio member exists on the wire and none is accepted', () => {
  const adapter = live();
  const request = begin(adapter, 'a-1', { kind: 'video', range: {} });
  assert.equal(JSON.stringify(request).includes('audio'), false);
  assert.throws(() => begin(adapter, 'a-2', { kind: 'video', range: {}, audio: 'mute' }), TypeError);
  assert.throws(() => begin(adapter, 'a-3', { kind: 'video', range: {}, video: { audio: true } }), TypeError);
});

test('status and cancel mint job.export.media.* requests and reject another adapter\'s requests', () => {
  const adapter = live();
  const request = begin(adapter, 'c-1');
  running(adapter, request);
  const statusRequest = adapter.status(identity(), 'c-2', 'native-export-job-1');
  assert.equal(statusRequest.operation, 'job.export.media.status');
  assert.deepEqual(statusRequest.payload, { jobId: 'native-export-job-1' });
  const cancelRequest = adapter.cancel(identity(), 'c-3', 'native-export-job-1');
  assert.equal(cancelRequest.operation, 'job.export.media.cancel');
  const cancelled = adapter.observe(cancelRequest, response(cancelRequest, receipts.cancelled()));
  assert.equal(cancelled.receipt.status, 'cancelled');
  assert.throws(() => adapter.observe(statusRequest, response(statusRequest, receipts.succeeded(names(4, 'png')))), /conflicts/);
  const vector = createNativeVectorExportAdapter({ route: true });
  assert.throws(() => vector.observe(request, response(request, receipts.running())), /frozen request/);
  const vectorRequest = vector.begin(identity(), snapshotResponse(), 'v-9', { outputHandle: 'output/a', frames: [0] });
  assert.throws(() => adapter.observe(vectorRequest, response(vectorRequest, receipts.running())), /frozen request/);
  assert.notEqual(vectorRequest.operation, request.operation);
});

test('the widest legal media begin stays inside the staged request bound', () => {
  const widest = { instanceId: 'i'.repeat(128), documentId: 'd'.repeat(128), contentRevision: 9007199254740991 };
  const snap = snapshotResponse({ instanceId: widest.instanceId, documentId: widest.documentId, contentRevision: widest.contentRevision, result: { atRevision: widest.contentRevision, documentSnapshotId: 's'.repeat(128) } });
  const request = live().begin(widest, snap, 'r'.repeat(128), {
    outputHandle: 'o'.repeat(128), kind: 'video', frameFormat: 'png', range: { start: 4294967294, end: 4294967295 }, size: { width: 4096, height: 4096 },
    video: { fps: 0.30000000000000004, quality: 'q'.repeat(32) },
  });
  assert.ok(Buffer.byteLength(JSON.stringify(request)) < 4096);
});
