'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');
const {
  createNativeApplicationAdapter,
  validateRequest,
  validateResponse,
} = require('../src/js/adapters/native-application.js');

const ROOT = path.resolve(__dirname, '..');
const schema = JSON.parse(fs.readFileSync(path.join(ROOT, 'engineering/application/native-transport-v2.schema.json'), 'utf8'));
const declaration = JSON.parse(fs.readFileSync(path.join(ROOT, 'engineering/application/capabilities-v2/native-opacity.json'), 'utf8'));

function query(id = 'fixture-query') {
  return {
    apiVersion: 2,
    requestId: id,
    instanceId: 'instance-a',
    documentId: 'native-document-1',
    operation: 'query.document.opacity',
    payload: { stableTarget: { layerUid: 'r08_curve_layer' } },
  };
}

function response(request) {
  return {
    apiVersion: 2,
    requestId: request.requestId,
    instanceId: request.instanceId,
    documentId: request.documentId,
    contentRevision: 0,
    ok: true,
    result: { atRevision: 0, layerUid: 'r08_curve_layer', value: 25 },
  };
}

function capturingPort(seen) {
  return { dispatch(request) { seen.push(request); return response(request); } };
}

test('UI and MCP fixture ports dispatch byte-identical v2 requests and validate one response shape', async () => {
  const uiSeen = [];
  const mcpSeen = [];
  const ui = createNativeApplicationAdapter('ui', capturingPort(uiSeen));
  const mcp = createNativeApplicationAdapter('mcp', capturingPort(mcpSeen));
  const fixture = declaration.examples[0].request;
  assert.deepEqual(await ui.dispatch(fixture), declaration.examples[0].response);
  assert.deepEqual(await mcp.dispatch(fixture), declaration.examples[0].response);
  assert.deepEqual(uiSeen, mcpSeen);
  assert.equal(JSON.stringify(uiSeen[0]), JSON.stringify(fixture));
  assert.equal(ui.label, 'ui');
  assert.equal(mcp.label, 'mcp');
});

test('expectedRevision is required only for frozen mutation and begin operations', () => {
  const required = [
    ['command.document.apply', { command: 'layer.opacity.set', stableTarget: { layerUid: 'r08_curve_layer' }, value: 40 }],
    ['transaction.begin', { stableTarget: { layerUid: 'r08_curve_layer' } }],
    ['history.undo', {}], ['history.redo', {}],
    ['job.export.png.begin', declaration.examples[1].request.payload],
  ];
  for (const [operation, payload] of required) {
    const request = { ...query(`required-${operation}`), operation, payload };
    assert.throws(() => validateRequest(request), /expectedRevision/);
    request.expectedRevision = 0;
    assert.strictEqual(validateRequest(request), request);
  }

  const forbidden = [
    ['query.document.serialize', { atRevision: 0 }],
    ['query.document.evaluate', { atRevision: 0, contextId: 'scene-root', frame: 10 }],
    ['query.document.opacity', query().payload], ['query.document.revision', {}],
    ['query.document.snapshot.acquire', {}],
    ['transaction.update', { transactionId: 'transaction-1', value: 40 }],
    ['transaction.commit', { transactionId: 'transaction-1' }],
    ['transaction.cancel', { transactionId: 'transaction-1' }],
    ['transaction.status', { transactionId: 'transaction-1' }],
    ['job.export.png.status', { jobId: 'native-export-job-1' }],
    ['job.export.png.cancel', { jobId: 'native-export-job-1' }],
  ];
  for (const [operation, payload] of forbidden) {
    const request = { ...query(`forbidden-${operation}`), operation, payload, expectedRevision: 0 };
    assert.throws(() => validateRequest(request), /expectedRevision/);
    delete request.expectedRevision;
    assert.strictEqual(validateRequest(request), request);
  }
});

test('pinned reads require exact selectors and preserve the complete bounded result', async () => {
  const document = JSON.parse(fs.readFileSync(path.join(ROOT, 'native-engine/tests/fixtures/opacity-v2/project.json'), 'utf8'));
  for (const operation of ['query.document.serialize', 'query.document.evaluate']) {
    const evaluate = operation === 'query.document.evaluate';
    const request = { ...query('pinned-read'), operation,
      payload: evaluate ? { atRevision: 0, contextId: 'scene-root', frame: 10 } : { atRevision: 0 } };
    const result = evaluate ? {
      documentSnapshotId: 'native-opacity:native-document-1:0', documentId: request.documentId,
      contentRevision: 0, contextId: 'scene-root', frame: 10,
      layers: [{ layerUid: 'r08_curve_layer', value: 50 }],
    } : { atRevision: 0, documentSnapshotId: 'native-opacity:native-document-1:0', document };
    const reply = { ...response(request), contentRevision: 2, result };
    const seen = [];
    const adapter = createNativeApplicationAdapter('read', { dispatch(value) { seen.push(value); return reply; } });
    assert.deepEqual(await adapter.dispatch(request), reply, 'envelope head may be later than pinned content');
    assert.deepEqual(seen, [request]);
    for (const payload of [{}, { ...request.payload, extra: true }, { ...request.payload, atRevision: -1 }]) {
      assert.throws(() => validateRequest({ ...request, payload }), /missing|unknown|atRevision/);
    }
    for (const mutation of [
      (copy) => { copy.result.extra = true; },
      (copy) => { copy.result[evaluate ? 'contentRevision' : 'atRevision'] = 1; },
      (copy) => { copy.documentId = 'other'; },
    ]) {
      const copy = structuredClone(reply); mutation(copy);
      const bad = createNativeApplicationAdapter('read', { dispatch: () => copy });
      await assert.rejects(bad.dispatch(request), /unknown|mismatch/);
    }
    const oversized = structuredClone(reply);
    if (evaluate) oversized.result.layers = Array.from({ length: 160 }, (_, i) => ({ layerUid: `layer-${i}`, value: 50 }));
    else oversized.result.document.layers = Array.from({ length: 160 }, (_, i) => ({ layerUid: `layer-${i}`, motionStatic: { opacity: [25] } }));
    assert.throws(() => validateResponse(oversized, request.requestId, operation), /4096 encoded bytes/);
    const malformed = structuredClone(reply);
    if (evaluate) malformed.result.layers[0].layerUid = 'bad id';
    else malformed.result.document = {};
    assert.throws(() => validateResponse(malformed, request.requestId, operation), /bounded identifier|missing|format/);
    if (!evaluate) {
      const unsupportedCurve = structuredClone(reply);
      unsupportedCurve.result.document.layers[0].motion.opacity.keys[0].curvePoints[1].y = 0.2;
      assert.throws(() => validateResponse(unsupportedCurve, request.requestId, operation), /curve/);
      const reorderedCurve = structuredClone(reply);
      reorderedCurve.result.document.layers[0].motion.opacity.keys.forEach((key) => {
        key.curvePoints = key.curvePoints.map(({ x, y }) => ({ y, x }));
      });
      assert.equal(validateResponse(reorderedCurve, request.requestId, operation), reorderedCurve,
        'JSON object member order is not contract data');
      const duplicate = structuredClone(reply);
      duplicate.result.document.layers.push(structuredClone(duplicate.result.document.layers[0]));
      assert.throws(() => validateResponse(duplicate, request.requestId, operation), /identity/);
      const legacy = structuredClone(reply);
      legacy.result.document = { version: 13, layers: [] };
      assert.throws(() => validateResponse(legacy, request.requestId, operation), /missing|format/);
    }
  }
  for (const frame of [-1, 0.5, 4294967296]) {
    assert.throws(() => validateRequest({ ...query(), operation: 'query.document.evaluate',
      payload: { atRevision: 0, contextId: 'scene-root', frame } }), /frame/);
  }
});

test('read response size counts the entire UTF-8 envelope at the exact 4096-byte boundary', () => {
  const request = { ...query(), operation: 'query.document.serialize', payload: { atRevision: 0 } };
  const reply = { ...response(request), result: {
    atRevision: 0, documentSnapshotId: 'native-opacity:native-document-1:0',
    document: { format: 'nemo.native-opacity-document', formatVersion: 1, totalFrames: 21, layers: [] },
  } };
  let fitted = false;
  for (let i = 0; i < 100; i++) {
    reply.result.document.layers.push({ layerUid: `layer-${i}`, motionStatic: { opacity: [25] } });
    const gap = 4096 - Buffer.byteLength(JSON.stringify(reply));
    if (gap >= 0 && gap <= 100) {
      reply.result.document.layers.at(-1).layerUid += 'x'.repeat(gap);
      fitted = true; break;
    }
  }
  assert.equal(fitted, true, 'valid native document fixture reaches the exact response boundary');
  assert.equal(Buffer.byteLength(JSON.stringify(reply)), 4096);
  assert.equal(validateResponse(reply, request.requestId, request.operation), reply);
  reply.result.document.layers.at(-1).layerUid += 'x';
  assert.ok(Buffer.byteLength(JSON.stringify(reply.result)) < 4096);
  assert.throws(() => validateResponse(reply, request.requestId, request.operation), /4096 encoded bytes/);
});

test('PNG begin accepts bounded opaque handles and rejects raw, binary, canvas and geometry payloads', () => {
  const valid = declaration.examples[1].request;
  assert.strictEqual(validateRequest(valid), valid);
  for (const mutation of [
    (copy) => { copy.payload.frames[0].pixels = [0, 0, 0, 255]; },
    (copy) => { copy.payload.frames[0].geometry = { layers: [] }; },
    (copy) => { copy.payload.frames[0].geometryHandle.bytes = Buffer.from([1]); },
    (copy) => { copy.payload.frames[0].geometryHandle.canvas = { width: 1 }; },
    (copy) => { copy.payload.frames[0].geometryHandle.resourceId = 'data:application/octet-stream,x'; },
  ]) {
    const copy = structuredClone(valid);
    mutation(copy);
    assert.throws(() => validateRequest(copy), /forbidden|unknown fields|geometryHandle/);
  }
  const tooMany = structuredClone(valid);
  tooMany.payload.frames = Array.from({ length: 121 }, (_, sourceFrame) => ({
    sourceFrame,
    geometryHandle: { resourceId: 'geometry/r08', resourceVersion: 'v1' },
  }));
  assert.throws(() => validateRequest(tooMany), /bounded|4096/);
  const unordered = structuredClone(valid);
  unordered.payload.frames.push(structuredClone(unordered.payload.frames[0]));
  assert.throws(() => validateRequest(unordered), /ascending/);
});

test('response validator enforces exact envelopes, closed common errors and JobReceipt fields', () => {
  const request = query();
  assert.strictEqual(validateResponse(response(request), request.requestId).ok, true);
  const failed = {
    apiVersion: 2, requestId: request.requestId, instanceId: request.instanceId,
    documentId: request.documentId, contentRevision: 0, ok: false,
    error: { code: 'wrong_document', message: 'replaced', details: { requestedDocumentId: 'old' } },
  };
  assert.strictEqual(validateResponse(failed, request.requestId).ok, false);
  assert.throws(() => validateResponse({ ...failed, error: { code: 'arbitrary', message: 'x' } }, request.requestId), /dispatch error/);
  assert.throws(() => validateResponse({ ...response(request), error: failed.error }, request.requestId), /unknown fields|only result/);
  assert.strictEqual(validateResponse(declaration.examples[1].response, 'fixture-export', 'job.export.png.begin').result.status, 'running');
});

test('schema and staged declaration carry the same frozen operations, examples and no activation', () => {
  assert.equal(schema.$schema, 'https://json-schema.org/draft/2020-12/schema');
  assert.ok(schema.$defs.Request);
  assert.ok(schema.$defs.Response);
  assert.ok(schema.$defs.JobReceipt);
  const schemaOperations = new Set();
  for (const branch of schema.$defs.Request.oneOf) {
    const operation = branch.allOf[1].properties.operation;
    for (const value of operation.enum || [operation.const]) schemaOperations.add(value);
  }
  assert.deepEqual([...schemaOperations].sort(), [...declaration.operations].sort());
  assert.equal(declaration.schemaVersion, 2);
  assert.equal(declaration.apiVersion, 2);
  assert.equal(declaration.status, 'staged');
  assert.equal(declaration.availability.state, 'unavailable');
  assert.equal(declaration.authority.adapterOwnsWritableMirror, false);
  assert.equal(declaration.authority.fallback, 'none');
  assert.deepEqual(declaration.ports, [
    { label: 'ui', state: 'fixture-only' },
    { label: 'mcp', state: 'fixture-only' },
  ]);
  for (const example of declaration.examples) {
    validateRequest(example.request);
    validateResponse(example.response, example.request.requestId, example.request.operation);
  }
  for (const example of schema.$defs.Request.examples) validateRequest(example);
  for (const example of schema.$defs.Response.examples) validateResponse(example, example.requestId);
  assert.equal(fs.existsSync(path.join(ROOT, 'engineering/application/capabilities/native-opacity.json')), false);
});

test('module load and adapter construction do not discover or activate host, MCP or legacy fallbacks', async () => {
  const tauri = globalThis.__TAURI__;
  const mcp = globalThis.NemoApplicationMcpAdapter;
  let dispatches = 0;
  const adapter = createNativeApplicationAdapter('fixture', {
    dispatch(request) { dispatches += 1; return response(request); },
  });
  assert.equal(dispatches, 0);
  assert.equal(globalThis.__TAURI__, tauri);
  assert.equal(globalThis.NemoApplicationMcpAdapter, mcp);
  await adapter.dispatch(query('explicit-only'));
  assert.equal(dispatches, 1);
  assert.equal(globalThis.__TAURI__, tauri);
  assert.equal(globalThis.NemoApplicationMcpAdapter, mcp);
});
