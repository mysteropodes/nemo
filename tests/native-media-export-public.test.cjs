'use strict';
// Composed-candidate test (needs patches 02 and 03): the public declaration of the native SVG / media export jobs, the
// schema <-> contract agreement and the real common SDK carrying the six operations.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');
const root = path.join(__dirname, '..');
const read = (...p) => fs.readFileSync(path.join(root, ...p), 'utf8');
const schema = JSON.parse(read('engineering', 'application', 'native-transport-v2.schema.json'));
const vectorDescriptor = JSON.parse(read('engineering', 'application', 'capabilities-v2', 'native-vector-export.json'));
const mediaDescriptor = JSON.parse(read('engineering', 'application', 'capabilities-v2', 'native-media-export.json'));
const contract = require('../src/js/adapters/native-media-export-contract.js');
const { createNativeApplicationAdapter, validateRequest, validateResponse } = require('../src/js/adapters/native-application.js');
const { createNativeVectorExportAdapter } = require('../src/js/adapters/native-vector-export-adapter.js');
const { createNativeMediaExportAdapter } = require('../src/js/adapters/native-media-export-adapter.js');
const { validator } = require('./native-media-export-schema.cjs');
const { identity, snapshotResponse, SNAPSHOT, names, receipts, response } = require('./native-media-export-support.cjs');

const v = validator(schema);
const inUnion = (request) => v.check({ $ref: '#/$defs/Request' }, request);
const OPS = ['job.export.svg.begin', 'job.export.svg.status', 'job.export.svg.cancel', 'job.export.media.begin', 'job.export.media.status', 'job.export.media.cancel'];
const branches = (operation) => schema.$defs.Request.oneOf.filter((b) => (b.allOf[1].properties.operation.const || '') === operation || (b.allOf[1].properties.operation.enum || []).includes(operation));

test('the six operations are declared once each in the v2 union and once each by a descriptor', () => {
  for (const operation of OPS) {
    const found = branches(operation);
    assert.equal(found.length, 1, operation);
    const begin = operation.endsWith('.begin');
    assert.equal(found[0].allOf[0].$ref, begin ? '#/$defs/RequiredRevisionRequest' : '#/$defs/ForbiddenRevisionRequest', operation);
    const payload = found[0].allOf[1].properties.payload.$ref;
    assert.equal(payload, begin ? (operation.includes('.svg.') ? '#/$defs/ExportSvgBeginPayload' : '#/$defs/ExportMediaBeginPayload') : '#/$defs/JobIdPayload');
  }
  assert.deepEqual(vectorDescriptor.operations, OPS.slice(0, 3));
  assert.deepEqual(mediaDescriptor.operations, OPS.slice(3));
  assert.equal(schema.$defs.ResponseBase.properties.result.oneOf.filter((e) => e.$ref === '#/$defs/JobReceipt').length, 1, 'the existing JobReceipt is the result; the union is unchanged');
  for (const d of [vectorDescriptor, mediaDescriptor]) {
    assert.equal(d.availability.state, 'unavailable');
    assert.match(d.availability.reason, /not registered$/);
    assert.equal(d.authority.adapterOwnsWritableMirror, false);
    assert.equal(d.authority.fallback, 'none');
    assert.deepEqual(d.ports, [{ label: 'ui', state: 'pending' }, { label: 'mcp', state: 'pending' }]);
    assert.equal(d.resourceBoundary.maxReadResponseBytes, 4096);
    assert.ok(d.resourceBoundary.forbidden.includes('raw pixels') && d.resourceBoundary.forbidden.includes('SVG text'));
    assert.equal(d.schemaVersion, 2); assert.equal(d.apiVersion, 2);
    assert.ok(d.examples.length >= 2);
  }
  assert.equal(vectorDescriptor.id, 'native.vector.export');
  assert.equal(mediaDescriptor.id, 'native.media.export');
});

test('the descriptors\' fixed pairs are valid union requests and run through the real SDK with a correlated receipt', async () => {
  for (const d of [vectorDescriptor, mediaDescriptor]) {
    for (const example of d.examples) {
      assert.ok(inUnion(example.request), example.label);
      assert.doesNotThrow(() => validateRequest(example.request), example.label);
      assert.doesNotThrow(() => validateResponse(example.response, example.request.requestId, example.request.operation), example.label);
      const adapter = createNativeApplicationAdapter('ui', { dispatch: async () => example.response });
      assert.deepEqual(await adapter.dispatch(example.request), example.response);
    }
  }
});

test('schema and contract agree on every structural flaw; the rules the schema cannot say are the contract\'s alone', () => {
  const svg = { documentSnapshotId: SNAPSHOT, contextId: 'scene-root', outputHandle: 'output/a', frames: [0, 5, 4294967295] };
  const spec = { kind: 'frame', format: 'rgba8', width: 64, height: 36, colorInterpretation: 'srgb', alphaMode: 'straight' };
  const media = (output, range, extra) => ({ documentSnapshotId: SNAPSHOT, contextId: 'scene-root', outputHandle: 'output/a', output, range, ...extra });
  const valid = [
    ['svg', svg], ['svg one frame', { ...svg, frames: [7] }],
    ['png sequence', media({ kind: 'sequence', frameFormat: 'png' }, { start: 0, end: 3 }, { outputSpec: spec })],
    ['png still', media({ kind: 'still', frameFormat: 'png' }, { start: 4, end: 4 }, { outputSpec: spec })],
    ['svg sequence', media({ kind: 'sequence', frameFormat: 'svg' }, {})],
    ['svg still', media({ kind: 'still', frameFormat: 'svg' }, { start: 0, end: 0 })],
    ['video', media({ kind: 'video', frameFormat: 'png' }, { end: 9 }, { outputSpec: spec })],
    ['video options', media({ kind: 'video', frameFormat: 'png' }, {}, { outputSpec: { ...spec, width: 4096, height: 1 }, video: { fps: 0, quality: '' } })],
    ['video fps only', media({ kind: 'video', frameFormat: 'png' }, { start: 2 }, { outputSpec: spec, video: { fps: 29.97 } })],
  ];
  const op = (name) => (name.startsWith('svg') && !name.includes('sequence') && !name.includes('still') ? 'job.export.svg.begin' : 'job.export.media.begin');
  const def = (name) => (op(name) === 'job.export.svg.begin' ? 'ExportSvgBeginPayload' : 'ExportMediaBeginPayload');
  for (const [name, payload] of valid) {
    assert.doesNotThrow(() => contract.payload(op(name), payload), name);
    assert.ok(v.def(def(name))(payload), name);
  }
  // Flaws both sides must refuse.
  const clone = (x) => JSON.parse(JSON.stringify(x));
  const flaws = [];
  for (const [name, payload] of valid) {
    for (const key of Object.keys(payload)) {
      if (key !== 'range' && key !== 'video' && key !== 'outputSpec') { const m = clone(payload); delete m[key]; flaws.push([`${name} without ${key}`, name, m]); }
      const n = clone(payload); n[key] = null; flaws.push([`${name} null ${key}`, name, n]);
    }
    flaws.push([`${name} extra member`, name, { ...clone(payload), extra: 1 }]);
    flaws.push([`${name} other context`, name, { ...clone(payload), contextId: 'component-1' }]);
    flaws.push([`${name} bad handle`, name, { ...clone(payload), outputHandle: 'a b' }]);
  }
  const png = valid[2][1];
  flaws.push(['png without spec', 'png sequence', (() => { const m = clone(png); delete m.outputSpec; return m; })()]);
  flaws.push(['svg with spec', 'svg sequence', { ...clone(valid[4][1]), outputSpec: spec }]);
  flaws.push(['svg video', 'video', { ...clone(valid[6][1]), output: { kind: 'video', frameFormat: 'svg' } }]);
  flaws.push(['video options on a sequence', 'png sequence', { ...clone(png), video: { fps: 24 } }]);
  flaws.push(['video options on svg', 'svg sequence', { ...clone(valid[4][1]), video: {} }]);
  flaws.push(['unknown kind', 'png sequence', { ...clone(png), output: { kind: 'movie', frameFormat: 'png' } }]);
  flaws.push(['unknown format', 'png sequence', { ...clone(png), output: { kind: 'sequence', frameFormat: 'gif' } }]);
  flaws.push(['output extra member', 'png sequence', { ...clone(png), output: { kind: 'sequence', frameFormat: 'png', audio: true } }]);
  flaws.push(['range extra member', 'png sequence', { ...clone(png), range: { start: 0, first: 1 } }]);
  flaws.push(['range string', 'png sequence', { ...clone(png), range: { start: '0' } }]);
  flaws.push(['range negative', 'png sequence', { ...clone(png), range: { start: -1 } }]);
  flaws.push(['range above u32', 'png sequence', { ...clone(png), range: { end: 4294967296 } }]);
  for (const [k, value] of [['width', 0], ['width', 4097], ['height', 0], ['height', 4097], ['width', 1.5], ['format', 'rgba16'], ['colorInterpretation', 'display-p3'], ['alphaMode', 'premultiplied'], ['kind', 'preview']]) {
    const m = clone(png); m.outputSpec[k] = value; flaws.push([`spec ${k}=${value}`, 'png sequence', m]);
  }
  { const m = clone(png); m.outputSpec.depth = 8; flaws.push(['spec extra member', 'png sequence', m]); }
  for (const video of [{ fps: -1 }, { fps: '24' }, { quality: 'a b' }, { quality: 'x'.repeat(33) }, { quality: 3 }, { codec: 'h264' }]) {
    flaws.push([`video ${JSON.stringify(video)}`, 'video', { ...clone(valid[6][1]), video }]);
  }
  for (const frames of [[], Array.from({ length: 33 }, (_, i) => i), [-1], [1.5], [4294967296], ['1'], 'abc', null]) flaws.push([`frames ${JSON.stringify(frames)}`, 'svg', { ...clone(svg), frames }]);
  flaws.push(['frames descending', 'svg', { ...clone(svg), frames: [3, 2] }]);
  let schemaOnly = 0;
  for (const [label, name, payload] of flaws) {
    const operation = op(name);
    assert.throws(() => contract.payload(operation, payload), TypeError, 'contract: ' + label);
    // JSON Schema cannot say "strictly ascending": only that case may pass the schema.
    const accepted = v.def(def(name))(payload);
    if (label === 'frames descending') { assert.equal(accepted, true, label); schemaOnly++; } else assert.equal(accepted, false, 'schema: ' + label);
  }
  assert.equal(schemaOnly, 1);
  assert.ok(flaws.length > 100, 'corpus size ' + flaws.length);
  // Contract-only rules.
  for (const [label, name, payload] of [
    ['still of two frames', 'png still', { ...clone(valid[3][1]), range: { start: 4, end: 5 } }],
    ['still without a frame', 'svg still', { ...clone(valid[5][1]), range: {} }],
    ['end before start', 'png sequence', { ...clone(png), range: { start: 5, end: 4 } }],
    ['sequence of 121 frames', 'png sequence', { ...clone(png), range: { start: 0, end: 120 } }],
  ]) {
    assert.throws(() => contract.payload(op(name), payload), TypeError, label);
    assert.equal(v.def(def(name))(payload), true, 'the schema cannot express: ' + label);
  }
});

test('the real SDK carries the six operations: revision rules, closed payloads, bounded correlated receipts', async () => {
  const begin = vectorDescriptor.examples[0].request;
  const status = vectorDescriptor.examples[1].request;
  assert.throws(() => validateRequest({ ...begin, expectedRevision: undefined }), TypeError);
  const noRevision = { ...begin }; delete noRevision.expectedRevision;
  assert.throws(() => validateRequest(noRevision), /expectedRevision/);
  assert.throws(() => validateRequest({ ...status, expectedRevision: 0 }), /expectedRevision/);
  assert.throws(() => validateRequest({ ...begin, payload: { ...begin.payload, extra: 1 } }), TypeError);
  assert.throws(() => validateRequest({ ...begin, payload: { ...begin.payload, outputHandle: 'data:image/svg+xml,x' } }), /forbidden/);
  assert.throws(() => validateRequest({ ...begin, payload: { ...begin.payload, frames: [2, 1] } }), /ascending/);
  assert.throws(() => validateRequest({ ...begin, operation: 'job.export.svg.begins' }), /not admitted/);
  assert.throws(() => validateRequest({ ...status, payload: { jobId: 'a b' } }), TypeError);
  // an SVG payload is not a PNG begin and a PNG begin is not an SVG begin
  const png = JSON.parse(read('engineering', 'application', 'capabilities-v2', 'native-opacity.json')).examples[1];
  assert.doesNotThrow(() => validateRequest(png.request));
  assert.throws(() => validateRequest({ ...png.request, operation: 'job.export.svg.begin' }), TypeError);
  assert.throws(() => validateRequest({ ...begin, operation: 'job.export.png.begin' }), TypeError);
  const answer = (over) => ({ ...vectorDescriptor.examples[0].response, ...over });
  const send = (request, reply) => createNativeApplicationAdapter('ui', { dispatch: async () => reply }).dispatch(request);
  const rejects = (request, reply, pattern, label) => assert.rejects(() => send(request, reply), pattern, label);
  await send(begin, answer({}));
  const running = vectorDescriptor.examples[0].response.result;
  await rejects(begin, answer({ result: { ...running, pinnedRevision: 1 } }), /snapshot/, 'other pinned revision');
  await rejects(begin, answer({ result: { ...running, documentSnapshotId: 'native-object:native-document-1:9' } }), /snapshot/, 'other snapshot');
  await rejects(begin, answer({ instanceId: 'other' }), /owner/, 'other instance');
  await rejects(begin, answer({ documentId: 'other' }), /document/, 'other document');
  await rejects(begin, answer({ result: { ...running, artifact: { target: 'output/fixture', files: [] } } }), /running job/, 'running with an artifact');
  await rejects(begin, answer({ result: { ...running, extra: 1 } }), TypeError, 'receipt extra member');
  await rejects(begin, answer({ result: { ...running, artifact: { target: 'output/fixture', files: ['data:image/svg+xml;base64,AAAA'] } } }), /forbidden/, 'bytes in a file name');
  await rejects(status, { ...answer({}), requestId: status.requestId, result: { ...running, jobId: 'native-export-job-2' } }, /another job/, 'status names another job');
  await rejects(begin, answer({ result: { ...running, artifact: { target: 'output/fixture', files: Array.from({ length: 400 }, (_, i) => `frame_${String(i).padStart(4, '0')}.svg`) }, status: 'succeeded', progress: 1, cleanup: { status: 'complete' }, externalEffectDisposition: 'committed' } }), /4096/, 'response over the bound');
  const refusal = { apiVersion: 2, requestId: begin.requestId, instanceId: begin.instanceId, documentId: begin.documentId, contentRevision: 0, ok: false, error: { code: 'unavailable', message: 'no decoder', details: { reason: 'media_decoder_unavailable', frame: 2 } } };
  assert.deepEqual((await send(begin, refusal)).error.details, { reason: 'media_decoder_unavailable', frame: 2 });
  await rejects(begin, { ...refusal, error: { code: 'frame_failed', message: 'm' } }, TypeError, 'a job code is not a dispatch error');
});

test('adapters and SDK agree: every request an adapter mints is a valid union request and passes the real SDK', async () => {
  const vector = createNativeVectorExportAdapter({ route: true });
  const media = createNativeMediaExportAdapter({ route: true });
  const mint = [
    vector.begin(identity(), snapshotResponse(), 'p-1', { outputHandle: 'output/a', frames: [0, 1, 2] }),
    media.begin(identity(), snapshotResponse(), 'p-2', { outputHandle: 'output/a', kind: 'sequence', frameFormat: 'png', range: { start: 0, end: 3 }, size: { width: 64, height: 36 } }),
    media.begin(identity(), snapshotResponse(), 'p-3', { outputHandle: 'output/a', kind: 'still', frameFormat: 'svg', range: { start: 2, end: 2 } }),
    media.begin(identity(), snapshotResponse(), 'p-4', { outputHandle: 'output/a', kind: 'video', frameFormat: 'png', range: {}, size: { width: 4096, height: 2160 }, video: { fps: 24, quality: 'best' } }),
  ];
  for (const request of mint) {
    assert.ok(inUnion(request), request.requestId);
    assert.doesNotThrow(() => validateRequest(request));
    const job = request.requestId === 'p-1' ? vector : media;
    const reply = response(request, receipts.running({ jobId: 'job-' + request.requestId }));
    const delivered = await createNativeApplicationAdapter('ui', { dispatch: async () => reply }).dispatch(JSON.parse(JSON.stringify(request)));
    assert.equal(job.observe(request, delivered).receipt.status, 'running');
  }
  const status = vector.status(identity(), 'p-5', 'job-p-1');
  assert.ok(inUnion(status)); assert.doesNotThrow(() => validateRequest(status));
  const cancel = media.cancel(identity(), 'p-6', 'job-p-2');
  assert.ok(inUnion(cancel)); assert.doesNotThrow(() => validateRequest(cancel));
  const done = await createNativeApplicationAdapter('ui', { dispatch: async () => response(status, receipts.succeeded(names(3, 'svg'), { jobId: 'job-p-1' })) }).dispatch(status);
  assert.equal(vector.observe(status, done).receipt.artifact.files.length, 3);
});

test('compose delegates every other operation to the existing contract, unchanged', () => {
  const base = { admitted: (op) => op === 'x.base', requiresRevision: () => 'base-rev', payload: () => 'base-payload', result: () => 'base-result', correlate: () => 'base-correlate', paths: () => 'base-paths' };
  const composed = contract.compose(base);
  assert.ok(Object.isFrozen(composed));
  assert.equal(composed.admitted('x.base'), true);
  assert.equal(composed.admitted('job.export.svg.begin'), true);
  assert.equal(composed.admitted('job.export.png.begin'), false);
  assert.equal(composed.requiresRevision('x.base'), 'base-rev');
  assert.equal(composed.requiresRevision('job.export.media.begin'), true);
  assert.equal(composed.requiresRevision('job.export.media.status'), false);
  assert.equal(composed.payload('x.base', {}), 'base-payload');
  assert.equal(composed.result('x.base', {}), 'base-result');
  assert.equal(composed.correlate({ operation: 'x.base' }, {}), 'base-correlate');
  assert.equal(composed.paths('x.base'), 'base-paths');
  assert.ok(composed.paths('job.export.svg.begin') instanceof Set);
  assert.equal(contract.compose(null).admitted('job.export.svg.begin'), true);
  assert.equal(contract.compose(undefined).admitted('x.base'), false);
});
