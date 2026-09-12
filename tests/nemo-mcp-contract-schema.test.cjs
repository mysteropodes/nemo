'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');

const root = path.join(__dirname, '..');
const load = (...parts) => JSON.parse(fs.readFileSync(path.join(root, ...parts), 'utf8'));
const schema = load('engineering', 'application', 'transport-v1.schema.json');
const descriptors = [
  load('engineering', 'application', 'capabilities', 'opacity.json'),
  load('engineering', 'application', 'capabilities', 'export-job.json'),
];

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
