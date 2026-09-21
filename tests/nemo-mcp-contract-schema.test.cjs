'use strict';

const assert = require('node:assert/strict');
const { execFileSync } = require('node:child_process');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');

const root = path.join(__dirname, '..');
const load = (...parts) => JSON.parse(fs.readFileSync(path.join(root, ...parts), 'utf8'));
const schema = load('engineering', 'application', 'transport-v1.schema.json');
const nativeSchema = load('engineering', 'application', 'native-transport-v2.schema.json');
const descriptors = [
  load('engineering', 'application', 'capabilities', 'opacity.json'),
  load('engineering', 'application', 'capabilities', 'export-job.json'),
  load('engineering', 'application', 'capabilities', 'timelapse.json'),
];
const nativeDescriptors = [
  load('engineering', 'application', 'capabilities-v2', 'native-opacity.json'),
];

function compiledSchema(...args) {
  return execFileSync('cargo', [
    'run', '--locked', '--offline', '--quiet',
    '--manifest-path', path.join(root, 'nemo-mcp', 'Cargo.toml'),
    '--bin', 'nemo-mcp-schema', '--', ...args,
  ], { cwd: root });
}

test('generated MCP transport schema embeds every complete registered descriptor', () => {
  const payload = schema.request.properties.payload;
  assert.deepEqual(payload['x-nemo-registeredCapabilities'], descriptors);
  assert.equal(payload.type, 'object');
  assert.ok(Array.isArray(payload.anyOf));
});

test('generated property branch derives input fields and types from its descriptor', () => {
  const payload = schema.request.properties.payload;
  const opacity = payload.anyOf.find((branch) =>
    branch.properties && branch.properties.property && branch.properties.property.const === 'opacity');
  assert.ok(opacity, 'opacity is a registered property branch');
  assert.equal(opacity.properties.property.const, 'opacity');
  assert.equal(opacity.properties.value.type, 'number');
  assert.ok(opacity.required.includes('layerId'));
  assert.ok(opacity.required.includes('property'));
});

test('compiled schema selector preserves the committed v1 bytes and exact accepted v2 bytes', () => {
  assert.deepEqual(
    compiledSchema(),
    fs.readFileSync(path.join(root, 'engineering', 'application', 'transport-v1.schema.json')),
  );
  assert.deepEqual(
    compiledSchema('--api-version', '2'),
    fs.readFileSync(path.join(root, 'engineering', 'application', 'native-transport-v2.schema.json')),
  );
});

test('feature-owned native declarations cover exactly the v2 transport operation set', () => {
  const schemaOperations = nativeSchema.$defs.Request.oneOf.flatMap((branch) => {
    const operation = branch.allOf[1].properties.operation;
    return Object.hasOwn(operation, 'const') ? [operation.const] : operation.enum;
  });
  const declaredOperations = nativeDescriptors.flatMap((descriptor) => descriptor.operations);
  assert.equal(new Set(schemaOperations).size, schemaOperations.length, 'v2 schema repeats an operation');
  assert.equal(new Set(declaredOperations).size, declaredOperations.length, 'native declarations repeat an operation');
  assert.deepEqual([...new Set(declaredOperations)].sort(), [...new Set(schemaOperations)].sort());
});

test('pinned read schemas declare strict selectors and exact native result identities', () => {
  for (const [operation, payload, result, required] of [
    ['query.document.serialize', 'SerializeQueryPayload', 'SerializeQueryResult', ['atRevision']],
    ['query.document.evaluate', 'EvaluateQueryPayload', 'EvaluateQueryResult', ['atRevision', 'contextId', 'frame']],
  ]) {
    const branch = nativeSchema.$defs.Request.oneOf.find((entry) => entry.allOf[1].properties.operation.const === operation);
    assert.equal(branch.allOf[0].$ref, '#/$defs/ForbiddenRevisionRequest');
    assert.equal(branch.allOf[1].properties.payload.$ref, `#/$defs/${payload}`);
    assert.deepEqual(nativeSchema.$defs[payload].required, required);
    assert.equal(nativeSchema.$defs[payload].additionalProperties, false);
    assert.equal(nativeSchema.$defs[result].additionalProperties, false);
    assert.ok(nativeSchema.$defs.ResponseBase.properties.result.oneOf.some((entry) => entry.$ref === `#/$defs/${result}`));
  }
  assert.deepEqual(nativeSchema.$defs.SerializeQueryResult.required, ['atRevision', 'documentSnapshotId', 'document']);
  assert.equal(nativeSchema.$defs.SerializeQueryResult.properties.document.$ref, '#/$defs/NativeOpacityDocument');
  assert.equal(nativeSchema.$defs.NativeOpacityDocument.additionalProperties, false);
  assert.equal(nativeSchema.$defs.NativeOpacityLayer.additionalProperties, false);
  assert.equal(nativeSchema.$defs.OpacityKey.additionalProperties, false);
  assert.deepEqual(nativeSchema.$defs.EvaluateQueryResult.required,
    ['documentSnapshotId', 'documentId', 'contentRevision', 'contextId', 'frame', 'layers']);
  assert.equal(nativeDescriptors[0].resourceBoundary.maxReadResponseBytes, 4096);
  assert.equal(nativeDescriptors[0].availability.state, 'unavailable');
});
