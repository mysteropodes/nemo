'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const Export = require('../src/js/adapters/native-object-project-export.js');

function fixture(options = {}) {
  const pin = Object.freeze({ instanceId: 'instance', documentId: 'document',
    contentRevision: 7, documentSnapshotId: 'snapshot-7', lifecycleGeneration: 3, totalFrames: 10050 });
  const requests = [];
  const progress = [];
  const ports = {
    enqueue: (action) => options.enqueue ? options.enqueue(() => action(pin)) : Promise.resolve().then(() => action(pin)),
    frameRequest: (frame, actualPin) => {
      assert.strictEqual(actualPin, pin);
      return { apiVersion: 2, requestId: 'sequence-request', instanceId: pin.instanceId,
        documentId: pin.documentId, contentRevision: pin.contentRevision,
        documentSnapshotId: pin.documentSnapshotId, lifecycleGeneration: pin.lifecycleGeneration, frame };
    },
    invoke: async (request) => {
      requests.push(request);
      if (options.failure) throw options.failure;
      const count = request.frameRange ? request.frameRange.end - request.frameRange.start + 1 :
        request.frames ? request.frames.length : 1;
      return { apiVersion: 2, requestId: request.requestId, instanceId: request.instanceId,
        documentId: request.documentId, contentRevision: request.contentRevision,
        documentSnapshotId: request.documentSnapshotId,
        status: 'succeeded', cleanupStatus: 'complete', externalEffectDisposition: 'committed',
        errorCode: null, artifactFiles: Array.from({ length: count }, (_, index) =>
          `frame_${String(index + 1).padStart(4, '0')}.${request.format || 'png'}`),
        published: true, cleanupComplete: true, ...options.receipt };
    },
  };
  return { pin, requests, progress, run: (frames, format = 'png', signal) =>
    Export.run(ports, '/native-sequence', frames, (...args) => progress.push(args), signal, format) };
}

test('a long PNG work area carries the exact inclusive range and all five original owner pins once', async () => {
  const f = fixture();
  const result = await f.run(Array.from({ length: 120 }, (_, index) => index + 13));
  assert.equal(f.requests.length, 1);
  const request = f.requests[0];
  assert.deepEqual(request.frameRange, { start: 13, end: 132 });
  assert.equal(request.frame, 13);
  assert.equal(Object.hasOwn(request, 'frames'), false);
  for (const key of ['instanceId', 'documentId', 'contentRevision', 'documentSnapshotId', 'lifecycleGeneration']) {
    assert.equal(request[key], f.pin[key]);
  }
  assert.equal(result.artifactFiles.length, 120);
  assert.equal(result.artifactFiles.at(-1), 'frame_0120.png');
  assert.deepEqual(f.progress, [[120, 120]]);
});

test('bounded sparse PNG and SVG lists retain the original private request contract', async () => {
  for (const format of ['png', 'svg']) {
    const f = fixture();
    await f.run([0, 13, 119], format);
    assert.deepEqual(f.requests[0].frames, [0, 13, 119]);
    assert.equal(Object.hasOwn(f.requests[0], 'frameRange'), false);
    assert.equal(f.requests[0].format, format === 'svg' ? 'svg' : undefined);
    assert.deepEqual(f.progress, [[3, 3]]);
  }
});

test('unsupported long selections refuse before native dispatch or any successful progress', async () => {
  const sparse = Array.from({ length: 120 }, (_, index) => index);
  delete sparse[80];
  for (const [frames, format] of [
    [Array.from({ length: 33 }, (_, index) => index * 2), 'png'],
    [Array.from({ length: 33 }, (_, index) => index), 'svg'],
    [Array.from({ length: 10000 }, (_, index) => index), 'png'],
    [sparse, 'png'],
  ]) {
    const f = fixture();
    await assert.rejects(f.run(frames, format), /frame range is invalid/);
    assert.deepEqual(f.requests, []);
    assert.deepEqual(f.progress, []);
  }
});

test('queued long ranges are detached from later array mutation and pre-dispatch cancel does no work', async () => {
  let enter;
  const f = fixture({ enqueue: (action) => new Promise((resolve) => { enter = () => resolve(action()); }) });
  const frames = Array.from({ length: 120 }, (_, index) => index);
  const running = f.run(frames);
  frames[119] = 9000;
  enter();
  await running;
  assert.deepEqual(f.requests[0].frameRange, { start: 0, end: 119 });
  const cancelled = fixture();
  assert.deepEqual(await cancelled.run(frames.slice(0, 100), 'png', { aborted: true }), { status: 'cancelled' });
  assert.deepEqual(cancelled.requests, []);
  assert.deepEqual(cancelled.progress, []);
});

test('a native sequence must confirm the full exact manifest, original pin and final cleanup before progress', async () => {
  for (const receipt of [
    { artifactFiles: ['frame_0001.png'] }, { artifactFiles: Array(120).fill('frame_0001.png') },
    { contentRevision: 8 }, { documentSnapshotId: 'newer-snapshot' }, { published: false },
    { status: 'running' }, { cleanupComplete: false }, { cleanupStatus: 'incomplete' },
    { externalEffectDisposition: 'notCommitted' }, { errorCode: 'cleanup_failed' },
  ]) {
    const f = fixture({ receipt });
    await assert.rejects(f.run(Array.from({ length: 120 }, (_, index) => index)), /publication is unconfirmed/);
    assert.equal(f.requests.length, 1);
    assert.deepEqual(f.progress, []);
  }
  const failure = new Error('native child export refused');
  const f = fixture({ failure });
  await assert.rejects(f.run(Array.from({ length: 120 }, (_, index) => index)), (error) => error === failure);
  assert.equal(f.requests.length, 1);
  assert.deepEqual(f.progress, []);
});
