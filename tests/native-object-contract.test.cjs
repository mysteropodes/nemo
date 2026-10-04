'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');
const { validate } = require('../scripts/nemo/lib/capability-schema.cjs');
const { validateRequest, validateResponse } = require('../src/js/adapters/native-application.js');
const root = path.resolve(__dirname, '..');
const load = (file) => JSON.parse(fs.readFileSync(path.join(root, file), 'utf8'));
const schema = load('engineering/application/native-object-v1.schema.json');
const cases = load('engineering/application/examples/native-object-v1/cases.json');
const check = (name, value) => {
  const errors = validate(schema.$defs[name], value, schema.$defs);
  const record = name === 'ObjectRecord' ? value
    : name === 'ReadResult' ? value?.object : name === 'ReadResponse' ? value?.result?.object : null;
  // Explicit contract-oracle supplement: the shipped subset validator has no
  // maxItems support. This does not implement admission or modify that validator.
  const segments = record?.geometry?.segments;
  if (Array.isArray(segments) && segments.length > schema.$defs.Geometry.properties.segments.maxItems) {
    errors.push('geometry.segments: exceeds declared maxItems 256');
  }
  return errors;
};

function changed(value, change) {
  const copy = structuredClone(value);
  const parent = change.path.slice(0, -1).reduce((node, key) => node[key], copy);
  const key = change.path.at(-1);
  if (change.delete) delete parent[key];
  else parent[key] = 'jsonValue' in change ? JSON.parse(change.jsonValue) : change.value;
  return copy;
}

test('schema uses shared constraints plus one explicitly checked maxItems boundary', () => {
  // Do not silently add unsupported keywords; only the one declared segment
  // maxItems is permitted and supplemented above, with a negative control below.
  const supported = new Set(['$schema', '$id', 'title', 'description', '$defs', '$ref', 'type',
    'const', 'enum', 'minLength', 'pattern', 'minimum', 'maximum', 'minItems', 'items',
    'required', 'properties', 'additionalProperties']);
  function visit(node) {
    assert.equal(typeof node, 'object');
    for (const [key, value] of Object.entries(node)) {
      assert.ok(supported.has(key) || (key === 'maxItems' && node === schema.$defs.Geometry.properties.segments),
        `unsupported schema keyword ${key}`);
      if (key === '$defs' || key === 'properties') Object.values(value).forEach(visit);
      if (key === 'items') visit(value);
      if (key === '$ref') assert.ok(schema.$defs[value.replace('#/$defs/', '')], value);
    }
  }
  visit(schema);
  assert.equal(schema.$schema, 'https://json-schema.org/draft/2020-12/schema');
  assert.equal(cases.oracleVersion, 'nemo.native-object-cases/1');
  assert.equal(cases.status, 'prospective-contract-only');
  assert.equal(schema.$defs.Geometry.properties.segments.minItems, 2);
  assert.equal(schema.$defs.Geometry.properties.segments.maxItems, 256);
});

test('fixed authored/reference records preserve opaque IDs and relative cubic geometry', () => {
  assert.equal(cases.records.length, 2);
  for (const record of cases.records) {
    assert.deepEqual(check('ObjectRecord', record), []);
    assert.deepEqual(JSON.parse(JSON.stringify(record)), record);
  }
  const authored = cases.records[0];
  assert.equal(authored.target.layerUid, 'legacy layer:alpha');
  assert.equal(authored.target.strokeId, 'ink/path_A');
  assert.equal(cases.records[1].target.frameScope.kind, 'reference');
  assert.equal(cases.records[1].target.strokeId, '0001', 'opaque numeric-looking strings retain their bytes');
  const [a, b] = authored.geometry.segments;
  assert.deepEqual([a.point.x + a.handleOut.x, a.point.y + a.handleOut.y], [13.375, 19]);
  assert.deepEqual([b.point.x + b.handleIn.x, b.point.y + b.handleIn.y], [37, 22]);
});

test('every fixed negative record mutation is rejected', () => {
  assert.ok(cases.invalidRecords.length >= 25);
  for (const change of cases.invalidRecords) {
    assert.notDeepEqual(check('ObjectRecord', changed(cases.records[0], change)), [], change.label);
  }
  for (const family of ['compound-path', 'component', 'brush', 'gradient', 'text', 'raster', 'mesh', 'media']) {
    assert.notDeepEqual(check('ObjectRecord', { ...cases.records[0], family }), [], family);
  }
});

test('JSON numeric overflow is rejected by supported bounds for every coordinate and handle', () => {
  for (const jsonValue of ['1e400', '-1e400']) {
    assert.equal(Number.isFinite(JSON.parse(jsonValue)), false);
    for (const field of ['point', 'handleIn', 'handleOut']) {
      for (const axis of ['x', 'y']) {
        const record = changed(cases.records[0], { path: ['geometry', 'segments', 0, field, axis], jsonValue });
        assert.notDeepEqual(validate(schema, record), [], `${field}.${axis}: ${jsonValue}`);
      }
    }
  }
});

test('segment counts match 2..256 with an explicit oracle maximum supplement', () => {
  assert.deepEqual(cases.segmentCountControls.map(({ count }) => count), [1, 2, 256, 257]);
  for (const { count, valid } of cases.segmentCountControls) {
    const record = structuredClone(cases.records[0]);
    record.geometry.segments = Array.from({ length: count }, (_, i) => structuredClone(cases.records[0].geometry.segments[i % 3]));
    assert.equal(check('ObjectRecord', record).length === 0, valid, `contract count ${count}`);
    if (count === 257) assert.deepEqual(validate(schema, record), [], 'shared validator alone does not check maxItems');
  }
});

function assertReadPair(request, response) {
  assert.deepEqual(check('ReadRequest', request), []);
  assert.deepEqual(check('ReadResponse', response), []);
  for (const key of ['requestId', 'instanceId', 'documentId']) assert.equal(response[key], request[key]);
  assert.equal(response.result.atRevision, request.payload.atRevision);
  assert.ok(response.contentRevision >= response.result.atRevision);
  assert.deepEqual(response.result.object.target, request.payload.stableTarget);
}

test('prospective pinned read has an independently fixed record and accepts a later envelope head', () => {
  const { request, response } = cases.read;
  assertReadPair(request, response);
  assert.deepEqual(response.result.object, cases.records[0]);
  assert.equal(response.contentRevision, 9);
  assert.equal(response.result.atRevision, 7);
  for (const change of [
    { path: ['requestId'], value: 'other' },
    { path: ['documentId'], value: 'other' },
    { path: ['result', 'atRevision'], value: 8 },
    { path: ['result', 'object', 'target', 'strokeId'], value: 'renumbered' },
    { path: ['result', 'object', 'target', 'frameScope', 'frame'], value: 8 },
  ]) assert.throws(() => assertReadPair(request, changed(response, change)), change.path.join('.'));
});

function assertPreservation(before, after, oracle) {
  for (const key of oracle.unchanged) assert.deepEqual(after[key], before[key]);
  assert.notDeepEqual(after[oracle.changed], before[oracle.changed]);
}

test('prospective single changed-fill command retains the fixed target and geometry oracle', () => {
  const { request, response, preservationOracle, afterRecord: after } = cases.command;
  assert.deepEqual(check('CommandRequest', request), []);
  assert.deepEqual(check('CommandResponse', response), []);
  assert.equal(response.contentRevision, request.expectedRevision + 1);
  for (const key of ['requestId', 'instanceId', 'documentId']) assert.equal(response[key], request[key]);
  assert.deepEqual(request.payload.stableTarget, cases.records[0].target);
  const before = cases.records[preservationOracle.beforeRecordIndex];
  assert.deepEqual(check('ObjectRecord', after), []);
  assertPreservation(before, after, preservationOracle);
  assert.deepEqual(after.fill, request.payload.fill);
  // This is a fixed contract oracle, not dispatch or history implementation.
  const renumbered = structuredClone(after); renumbered.target.strokeId = 'replacement-id';
  assert.deepEqual(check('ObjectRecord', renumbered), [], 'schema alone cannot prove identity retention');
  assert.throws(() => assertPreservation(before, renumbered, preservationOracle));
  for (const change of [
    { path: ['target', 'layerUid'], value: 'new-layer' },
    { path: ['target', 'frameScope', 'frame'], value: 8 },
    { path: ['geometry', 'segments', 0, 'handleOut', 'x'], value: 13.375 },
    { path: ['fill'], value: before.fill },
  ]) assert.throws(() => assertPreservation(before, changed(after, change), preservationOracle));
});

test('prospective failures have closed envelopes and unchanged current revision', () => {
  assert.deepEqual(cases.failures.map((entry) => entry.response.error.code),
    ['wrong_document', 'not_found', 'unavailable', 'stale_revision']);
  for (const entry of cases.failures) {
    const request = entry.requestChanges.reduce(changed, cases[entry.requestKind].request);
    assert.deepEqual(check(entry.requestKind === 'read' ? 'ReadRequest' : 'CommandRequest', request), []);
    const response = entry.response;
    assert.deepEqual(check('ErrorResponse', response), []);
    assert.equal(response.requestId, request.requestId);
    assert.equal(response.contentRevision, 9);
    assert.equal('result' in response, false);
    if (response.error.code === 'wrong_document') {
      assert.notEqual(response.documentId, request.documentId);
      assert.equal(response.error.details.requestedDocumentId, request.documentId);
    } else assert.equal(response.documentId, request.documentId);
    assert.notDeepEqual(check('ErrorResponse', { ...response, result: {} }), []);
  }
});

test('protocol mutation controls separate schema/API/storage versions and require revision selectors', () => {
  for (const [kind, name, changes] of [
    ['read', 'ReadRequest', [
      { path: ['apiVersion'], value: 1 }, { path: ['payload', 'atRevision'], delete: true },
      { path: ['payload', 'atRevision'], value: -1 }, { path: ['payload', 'stableTarget', 'frameScope'], delete: true },
      { path: ['expectedRevision'], value: 7 }, { path: ['payload', 'allFrames'], value: true },
    ]],
    ['command', 'CommandRequest', [
      { path: ['expectedRevision'], delete: true }, { path: ['expectedRevision'], value: 9.5 },
      { path: ['payload', 'command'], value: 'object.renumber' }, { path: ['payload', 'stableTarget', 'strokeIndex'], value: 0 },
      { path: ['payload', 'fill', 'kind'], value: 'gradient' }, { path: ['schemaVersion'], value: 1 },
    ]],
  ]) for (const change of changes) assert.notDeepEqual(check(name, changed(cases[kind].request, change)), [], `${kind}: ${change.path}`);
});

test('active opacity adapter and stored schema reject staged object admission and commands', () => {
  assert.throws(() => validateRequest(cases.read.request));
  assert.throws(() => validateRequest(cases.command.request));
  const transport = load('engineering/application/native-transport-v2.schema.json');
  const opacitySchema = transport.$defs.NativeOpacityDocument;
  const opacity = load('native-engine/tests/fixtures/opacity-v2/project.json');
  assert.deepEqual(validate(opacitySchema, opacity, transport.$defs), []);
  assert.notDeepEqual(validate(opacitySchema, { ...opacity, objects: cases.records }, transport.$defs), []);
  assert.equal(transport.$defs.NativeOpacityLayer.additionalProperties, false);
  const extra = structuredClone(opacity); extra.layers[0].objects = cases.records;
  // The subset validator returns early for anyOf; the active layer schema uses
  // sibling constraints. Use the production codec boundary for this control.
  const response = { apiVersion: 2, requestId: 'opacity-serialize', instanceId: 'instance-A',
    documentId: 'document-A', contentRevision: 9, ok: true,
    result: { atRevision: 9, documentSnapshotId: 'snapshot-A-nine', document: extra } };
  assert.throws(() => validateResponse(response, response.requestId, 'query.document.serialize'));
});
