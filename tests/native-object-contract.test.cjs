'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');
const { validate } = require('../scripts/nemo/lib/capability-schema.cjs');
const { createNativeApplicationAdapter, validateRequest, validateResponse } = require('../src/js/adapters/native-application.js');
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

test('shared SDK admits declared object reads while preserving opacity schema and undeclared command denial', () => {
  assert.strictEqual(validateRequest(cases.read.request), cases.read.request);
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

const descriptor = load('engineering/application/capabilities-v2/native-object.json');
const transport = load('engineering/application/native-transport-v2.schema.json');

test('registered object read has an active source route with pending installed ports and a complete-result byte policy', () => {
  assert.equal(descriptor.id, 'native.object');
  assert.equal(descriptor.status, 'active');
  assert.deepEqual(descriptor.operations, [
    'query.document.object', 'command.document.object.layer.visibility.set',
    'command.document.object.undo', 'command.document.object.redo',
  ]);
  assert.equal(descriptor.availability.state, 'available');
  assert.equal(descriptor.availability.reason, null);
  assert.deepEqual(descriptor.ports, [{ label: 'ui', state: 'pending' }, { label: 'mcp', state: 'pending' }]);
  assert.equal(descriptor.resourceBoundary.maxReadResponseBytes, 4096);
  assert.match(descriptor.resourceBoundary.policy, /never truncate or simplify/);
  assert.match(descriptor.typedValueBoundary, /duplicate raw JSON keys/);
  assert.deepEqual(descriptor.examples[0].request, cases.read.request);
  assert.equal(descriptor.examples[0].error, undefined);
  assert.strictEqual(validateRequest(descriptor.examples[0].request), descriptor.examples[0].request,
    'pure SDK shape admission leaves live-owner execution and installed client acceptance separate');
  assert.equal(load('engineering/application/capabilities-v2/native-opacity.json').availability.state, 'available');
});

test('public visibility command declares a required revision and exact opaque-UID boolean payload', () => {
  const request = {
    apiVersion: 2, requestId: 'visibility-contract-7', instanceId: 'instance-A',
    documentId: 'document-A', expectedRevision: 7,
    operation: 'command.document.object.layer.visibility.set',
    payload: { command: 'object.layer.visibility.set', layerUid: 'legacy layer:alpha', visible: false },
  };
  assert.deepEqual(descriptor.examples[1].request.operation, request.operation);
  assert.equal(descriptor.examples[1].error, undefined);
  const branch = transport.$defs.Request.oneOf.find((entry) =>
    entry.allOf?.some((part) => part.properties?.operation?.const === request.operation));
  assert.ok(branch);
  assert.equal(branch.allOf[0].$ref, '#/$defs/RequiredRevisionRequest');
  assert.equal(branch.allOf[1].properties.payload.$ref, '#/$defs/ObjectLayerVisibilityPayload');
  const payload = transport.$defs.ObjectLayerVisibilityPayload;
  assert.equal(payload.additionalProperties, false);
  assert.deepEqual(payload.required, ['command', 'layerUid', 'visible']);
  assert.equal(payload.properties.layerUid.$ref, '#/$defs/ObjectOpaqueId');
  assert.deepEqual(validate(payload, request.payload, transport.$defs), []);
  for (const change of [
    { path: ['command'], value: 'object.fill.set' },
    { path: ['layerUid'], value: '' },
    { path: ['layerUid'], value: 9 },
    { path: ['visible'], value: 0 },
    { path: ['visible'], delete: true },
  ]) assert.notDeepEqual(validate(payload, changed(request.payload, change), transport.$defs), [], change.path.join('.'));
  assert.notDeepEqual(validate(payload, { ...request.payload, extra: true }, transport.$defs), []);
  assert.strictEqual(validateRequest(request), request, 'SDK shape admission is separate from live-owner and subscriber acceptance');
});

test('transport object selector carries the frozen closed scope and bounded envelope contract', () => {
  const branch = transport.$defs.Request.oneOf.find((entry) =>
    entry.allOf?.some((part) => part.properties?.operation?.const === 'query.document.object'));
  assert.ok(branch);
  assert.equal(branch.allOf[0].$ref, '#/$defs/ForbiddenRevisionRequest');
  assert.equal(branch.allOf[1].properties.payload.$ref, '#/$defs/ObjectQueryPayload');
  const payload = transport.$defs.ObjectQueryPayload;
  assert.equal(payload.additionalProperties, false);
  assert.deepEqual(payload.required, ['atRevision', 'stableTarget']);
  assert.deepEqual(validate(payload, cases.read.request.payload, transport.$defs), []);
  for (const change of [
    { path: ['atRevision'], delete: true }, { path: ['atRevision'], value: -1 },
    { path: ['stableTarget', 'strokeId'], value: 1 },
    { path: ['stableTarget', 'frameScope', 'kind'], value: { authored: true } },
    { path: ['stableTarget', 'frameScope', 'frame'], value: 4294967296 },
    { path: ['stableTarget', 'contextId'], value: 'other' },
    { path: ['stableTarget', 'allFrames'], value: true },
  ]) assert.notDeepEqual(validate(payload, changed(cases.read.request.payload, change), transport.$defs), [], change.path.join('.'));
  assert.equal(transport.$defs.Identifier.pattern, '^[A-Za-z0-9][A-Za-z0-9._:/-]{0,127}$');
  assert.equal(transport.$defs.ObjectOpaqueId.minLength, 1);
  assert.equal('pattern' in transport.$defs.ObjectOpaqueId, false);
});

test('transport complete-record definitions preserve each frozen field constraint', () => {
  function renamed(value) {
    if (Array.isArray(value)) return value.map(renamed);
    if (value && typeof value === 'object') return Object.fromEntries(Object.entries(value).map(([key, entry]) => [key, renamed(entry)]));
    if (typeof value === 'string' && value.startsWith('#/$defs/')) {
      const name = value.slice(8);
      if (['OpaqueId', 'Frame', 'FrameScope', 'Target', 'Point', 'Segment', 'Geometry', 'SolidFill'].includes(name)) return `#/$defs/Object${name}`;
    }
    return value;
  }
  for (const name of ['OpaqueId', 'Frame', 'FrameScope', 'Target', 'Point', 'Segment', 'Geometry', 'SolidFill', 'ObjectRecord']) {
    assert.deepEqual(transport.$defs[name === 'ObjectRecord' ? name : `Object${name}`], renamed(schema.$defs[name]), name);
  }
  for (const record of cases.records) assert.deepEqual(validate(transport.$defs.ObjectRecord, record, transport.$defs), []);
});

test('layer creation uses its single available descriptor with strict payload and receipt contracts', () => {
  const create = load('engineering/application/capabilities-v2/native-object-layer-create.json');
  assert.equal(create.id, 'native.object.layer-create');
  assert.deepEqual(create.operations, ['command.document.object.layer.create']);
  assert.equal(create.status, 'active');
  assert.equal(create.availability.state, 'available');
  assert.equal(create.availability.reason, null);
  assert.equal(create.authority.fallback, 'none');
  assert.equal(create.authority.adapterOwnsWritableMirror, false);
  assert.equal(descriptor.availability.state, 'available', 'existing object query and visibility remain available');
  const { request, response } = create.examples[0];
  assert.strictEqual(validateRequest(request), request);
  assert.strictEqual(validateResponse(response, request.requestId, request.operation), response);
  const branch = transport.$defs.Request.oneOf.find(entry =>
    entry.allOf?.some(part => part.properties?.operation?.const === request.operation));
  assert.equal(branch.allOf[0].$ref, '#/$defs/RequiredRevisionRequest');
  assert.equal(branch.allOf[1].properties.payload.$ref, '#/$defs/ObjectLayerCreatePayload');
  const payload = transport.$defs.ObjectLayerCreatePayload;
  const result = transport.$defs.ObjectLayerCreateResult;
  assert.deepEqual(validate(payload, request.payload, transport.$defs), []);
  assert.deepEqual(validate(result, response.result, transport.$defs), []);
  assert.ok(transport.$defs.ResponseBase.properties.result.oneOf.some(value => value.$ref === '#/$defs/ObjectLayerCreateResult'));
  for (const invalid of [[], null, {}, { command: 'object.layer.create', name: null },
    { command: 'object.layer.create', name: 'x', layerUid: 'caller' },
    { command: 'object.layer.create', name: 0 }]) assert.ok(validate(payload, invalid, transport.$defs).length);
  for (const invalid of [{ applied: false, historyEntriesAdded: 0, layerUid: 'x' },
    { applied: true, historyEntriesAdded: 1 }, { applied: true, historyEntriesAdded: 1, layerUid: '' },
    { applied: true, historyEntriesAdded: 1, layerUid: 'x', extra: true }]) assert.ok(validate(result, invalid, transport.$defs).length);
});

test('timing declaration and closed transport definitions retain the accepted selector/read-result contract', () => {
  const timing = load('engineering/application/capabilities-v2/native-object-layer-timing.json');
  const accepted = load('engineering/application/native-layer-timing-v1.schema.json').$defs;
  const branch = transport.$defs.Request.oneOf.find(entry => entry.allOf?.some(part => part.properties?.operation?.const === 'query.document.object.layer.timing'));
  assert.equal(timing.id, 'native.object.layer-timing'); assert.deepEqual(timing.operations, ['query.document.object.layer.timing']);
  assert.equal(timing.availability.state, 'available'); assert.ok(timing.ports.every(port => port.state === 'pending'));
  assert.equal(branch.allOf[0].$ref, '#/$defs/ForbiddenRevisionRequest'); assert.equal(branch.allOf[1].properties.payload.$ref, '#/$defs/ObjectLayerTimingPayload');
  const timingReferences = Object.fromEntries([
    ['ObjectLayerTimingFrame','Frame'], ['ObjectLayerTimingCellPayload','CellSelector'],
    ['ObjectLayerRangePayload','RangeSelector'], ['ObjectLayerTimingCellResult','CellReadResult'],
    ['ObjectLayerRangeResult','RangeReadResult'], ['ObjectLayerPlacementPayload','PlacementSelector'],
    ['ObjectLayerPlacementResult','PlacementReadResult'], ['ObjectLayerRowCellsPayload','RowCellsSelector'],
    ['ObjectLayerRowCellsResult','RowCellsReadResult']
  ].map(([from,to])=>['#/$defs/'+from,'#/$defs/'+to]));
  const normalize = value => JSON.parse(JSON.stringify(value),(key,entry)=>key==='$ref'&&Object.hasOwn(timingReferences,entry)?timingReferences[entry]:entry);
  // Preserve both original cell and admitted range contracts down to their closed leaf definitions.
  for(const [from,to] of [['ObjectLayerTimingCellPayload','CellSelector'],['ObjectLayerRangePayload','RangeSelector'],
    ['ObjectLayerTimingCellResult','CellReadResult'],['ObjectLayerRangeResult','RangeReadResult'],
    ['ObjectLayerPlacementPayload','PlacementSelector'],['ObjectLayerPlacementResult','PlacementReadResult'],
    ['ObjectLayerRowCellsPayload','RowCellsSelector'],['ObjectLayerRowCellsResult','RowCellsReadResult']])assert.deepEqual(normalize(transport.$defs[from]),accepted[to]);
  assert.deepEqual(normalize(transport.$defs.ObjectLayerTimingPayload), accepted.Selector);
  assert.deepEqual(normalize(transport.$defs.ObjectLayerTimingResult), accepted.ReadResult);
  assert.deepEqual(transport.$defs.ObjectLayerTimingFrame, accepted.Frame);
  assert.ok(transport.$defs.ResponseBase.properties.result.oneOf.some(value => value.$ref === '#/$defs/ObjectLayerTimingResult'));
  // This subset validator has no oneOf: check the same declared exclusive branches.
  const validateSelector=value=>{const branches=transport.$defs.ObjectLayerTimingPayload.oneOf.map(branch=>validate(branch,value,transport.$defs));return branches.filter(errors=>errors.length===0).length===1?[]:['selector must match exactly one declared branch'];};
  const request = timing.examples[0].request; assert.equal(validateRequest(request), request);
  assert.deepEqual(validateSelector(request.payload), []);
  const rangeRequest={...request,payload:{...request.payload,view:'range'}};
  assert.equal(validateRequest(rangeRequest),rangeRequest);
  assert.deepEqual(validateSelector(rangeRequest.payload),[]);
  // T12 placement view: the same selector with its own view constant.
  const placementRequest={...request,payload:{...request.payload,view:'placement'}};
  assert.equal(validateRequest(placementRequest),placementRequest);
  assert.deepEqual(validateSelector(placementRequest.payload),[]);
  const placement=timing.examples.find(e=>e.request.payload.view==='placement');
  assert.equal(validateRequest(placement.request),placement.request);
  assert.doesNotThrow(()=>validateResponse(placement.response,placement.request.requestId,placement.request.operation));
  assert.deepEqual(validate(transport.$defs.ObjectLayerPlacementResult,placement.response.result,transport.$defs),[]);
  for(const bad of [{assetKind:'image'},{offsetFrames:'0'},{interpMode:''},{frameBlend:1},{sourceFrameCount:0},{sourceFrameCount:1.5},{timeRemap:{keyCount:-1,keyAtFrame:true}},{timeRemap:{keyCount:1}},{timeRemap:{keyCount:2,keyAtFrame:true}},{timeRemap:{keyCount:1,keyAtFrame:false,value:1,keys:[]}},{timeRemap:{keyCount:1,keyAtFrame:false,value:'1',keys:null}},{extra:1}]){
    const result={...placement.response.result,placement:{...placement.response.result.placement,...bad}};
    assert.throws(()=>validateResponse({...placement.response,result},placement.request.requestId,placement.request.operation),TypeError,JSON.stringify(bad));
  }
  const counted={...placement.response,result:{...placement.response.result,placement:{...placement.response.result.placement,sourceFrameCount:48}}};
  assert.doesNotThrow(()=>validateResponse(counted,placement.request.requestId,placement.request.operation));
  // U31 cells view (#2374): the same selector with view "cells" and its run-encoded row.
  const cells=timing.examples.find(e=>e.request.payload.view==='cells');
  assert.equal(validateRequest(cells.request),cells.request);
  assert.deepEqual(validateSelector(cells.request.payload),[]);
  assert.doesNotThrow(()=>validateResponse(cells.response,cells.request.requestId,cells.request.operation));
  assert.deepEqual(validate(transport.$defs.ObjectLayerRowCellsResult,cells.response.result,transport.$defs),[]);
  for(const cellsBad of [{runs:[[0,1,'blank',0,true,false]],nextFrame:null},{runs:[[1,1,'key',1,true,false]],nextFrame:null},{runs:[[0,1,'key',0,true,false]],nextFrame:2},{runs:[],nextFrame:null}])
    assert.throws(()=>validateResponse({...cells.response,result:{...cells.response.result,cells:cellsBad}},cells.request.requestId,cells.request.operation),TypeError,JSON.stringify(cellsBad));
  const empty={...placement.response,result:{...placement.response.result,placement:null}};
  assert.doesNotThrow(()=>validateResponse(empty,placement.request.requestId,placement.request.operation));
  for (const bad of [null, [], {}, { ...request.payload, lifecycleGeneration: 1 }, { ...request.payload, frame: 4294967296 }, { ...request.payload, atRevision: -1 }, {...rangeRequest.payload,view:'other'}]) {
    assert.ok(validateSelector(bad).length);
  }
  assert.equal(load('engineering/application/capabilities-v2/native-object-layer-create.json').availability.state, 'available');
});


test('public object Undo and Redo declare closed revision-pinned commands and exact compact receipts', () => {
  for (const action of ['undo', 'redo']) {
    const request = { apiVersion: 2, requestId: `object-${action}`, instanceId: 'instance-A',
      documentId: 'document-A', expectedRevision: 7, operation: `command.document.object.${action}`,
      payload: { command: `object.${action}` } };
    const branch = transport.$defs.Request.oneOf.find(entry =>
      entry.allOf?.some(part => part.properties?.operation?.const === request.operation));
    assert.equal(branch.allOf[0].$ref, '#/$defs/RequiredRevisionRequest');
    const payloadRef = `#/$defs/Object${action === 'undo' ? 'Undo' : 'Redo'}Payload`;
    assert.equal(branch.allOf[1].properties.payload.$ref, payloadRef);
    const payload = transport.$defs[payloadRef.slice('#/$defs/'.length)];
    assert.equal(payload.additionalProperties, false); assert.deepEqual(payload.required, ['command']);
    assert.deepEqual(validate(payload, request.payload, transport.$defs), []);
    assert.strictEqual(validateRequest(request), request);
    for (const invalid of [null, [], {}, { command: `object.${action}`, extra: true },
      { command: `object.${action === 'undo' ? 'redo' : 'undo'}` }, { command: null }]) {
      assert.ok(validate(payload, invalid, transport.$defs).length);
      assert.throws(() => validateRequest({ ...request, payload: invalid }));
    }
    const { expectedRevision, ...missing } = request;
    assert.throws(() => validateRequest(missing));
    for (const invalid of [null, -1, 1.5, Number.MAX_SAFE_INTEGER + 1])
      assert.throws(() => validateRequest({ ...request, expectedRevision: invalid }));
    assert.throws(() => validateRequest({ ...request, extra: true }));
    assert.deepEqual(transport.$defs.RequiredRevisionRequest.allOf[1].required, ['expectedRevision']);
    const response = { apiVersion: 2, requestId: request.requestId, instanceId: request.instanceId,
      documentId: request.documentId, contentRevision: 8, ok: true,
      result: { applied: true, historyEntriesAdded: 0 } };
    assert.strictEqual(validateResponse(response, request.requestId, request.operation), response);
    assert.deepEqual(validate(transport.$defs.ObjectHistoryResult, response.result, transport.$defs), []);
    assert.deepEqual(validate(transport.$defs.CommandResult, response.result, transport.$defs), []);
    for (const result of [null, {}, { applied: false, historyEntriesAdded: 0 },
      { applied: true, historyEntriesAdded: 1 }, { applied: true, historyEntriesAdded: null },
      { applied: true, historyEntriesAdded: Number.MAX_SAFE_INTEGER + 1 },
      { applied: true, historyEntriesAdded: 0, extra: true }]) {
      assert.ok(validate(transport.$defs.ObjectHistoryResult, result, transport.$defs).length);
      assert.throws(() => validateResponse({ ...response, result }, request.requestId, request.operation));
    }
    const { result, ...missingResult } = response;
    assert.throws(() => validateResponse(missingResult, request.requestId, request.operation));
    assert.throws(() => validateResponse(response, 'other-request', request.operation));
    const oversized = { ...missingResult, ok: false, error: { code: 'busy_conflict', message: 'x'.repeat(4096) } };
    assert.throws(() => validateResponse(oversized, request.requestId, request.operation), /4096/);
    // The legacy opacity domain still requires its original empty payload.
    assert.deepEqual(validateRequest({ ...request, operation: `history.${action}`, payload: {} }).payload, {});
    assert.throws(() => validateRequest({ ...request, operation: `history.${action}` }));
  }
});

test('public object history dispatch preserves owner and advanced revision correlation', async () => {
  for (const action of ['undo', 'redo']) {
    const request = { apiVersion: 2, requestId: `dispatch-${action}`, instanceId: 'instance-A',
      documentId: 'document-A', expectedRevision: 7, operation: `command.document.object.${action}`,
      payload: { command: `object.${action}` } };
    const response = { apiVersion: 2, requestId: request.requestId, instanceId: request.instanceId,
      documentId: request.documentId, contentRevision: 8, ok: true,
      result: { applied: true, historyEntriesAdded: 0 } };
    let sent;
    const adapter = createNativeApplicationAdapter('ui', { dispatch: value => { sent = value; return response; } });
    assert.strictEqual(await adapter.dispatch(request), response);
    assert.deepEqual(sent, request); assert.notStrictEqual(sent, request);
    for (const invalid of [{ ...response, documentId: 'foreign' }, { ...response, instanceId: 'foreign' },
      { ...response, contentRevision: 7 }, { ...response, contentRevision: 9 },
      { ...response, result: { applied: true, historyEntriesAdded: 1 } }]) {
      const bad = createNativeApplicationAdapter('ui', { dispatch: () => invalid });
      await assert.rejects(bad.dispatch(request));
    }
    const failed = { ...response, ok: false, contentRevision: 7,
      error: { code: 'busy_conflict', message: 'Object history could not move.' } };
    delete failed.result;
    assert.strictEqual(await createNativeApplicationAdapter('ui', { dispatch: () => failed }).dispatch(request), failed);
  }
});
