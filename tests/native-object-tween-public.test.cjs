'use strict';
// D20-D22 (#2358): the public declaration of the four tween commands. The contract table (shared with the Rust
// validator), the closed transport schema and the common SDK accept the same payloads and refuse the same flaws; the
// descriptor examples are copyable requests and receipts.
const test = require('node:test'), assert = require('node:assert/strict'), fs = require('node:fs'), path = require('node:path');
const contract = require('../src/js/adapters/native-object-rig-contract.js');
const SDK = require('../src/js/adapters/native-application.js');
const { validator } = require('./native-object-rig-schema.cjs');

const ROOT = path.resolve(__dirname, '..');
const schema = JSON.parse(fs.readFileSync(path.join(ROOT, 'engineering/application/native-transport-v2.schema.json'), 'utf8'));
const descriptor = JSON.parse(fs.readFileSync(path.join(ROOT, 'engineering/application/capabilities-v2/native-object-tween.json'), 'utf8'));
const OPS = [contract.TWEEN_GENERATE, contract.TWEEN_REGENERATE, contract.TWEEN_SPLIT, contract.TWEEN_REASSIGN];
const defOf = (operation) => schema.$defs.Request.oneOf.find((b) => b.allOf[1].properties.operation.const === operation)
  .allOf[1].properties.payload.$ref.replace('#/$defs/', '');
const options = { resamplePoints: 50, step: 1, skipManual: true, easing: [{ x: 0, y: 0 }, { x: 0.5, y: 0.2, tx: 1, ty: 0 }, { x: 1, y: 1 }] };
const span = { from: 0, to: 4, easing: [{ x: 0, y: 0 }, { x: 1, y: 1 }], overrides: [{ aId: 'a', bId: 'b' }, { aIds: ['c', 'd'], bId: 'e' }], manualStrokes: ['a', 'c'] };
const valid = {
  [contract.TWEEN_GENERATE]: [{ command: 'object.tween.generate', layerUid: 'L', options, selection: [1, 6] }, { command: 'object.tween.generate', layerUid: 'L' },
    { command: 'object.tween.generate', layerUid: 'L', starts: [] }],
  [contract.TWEEN_REGENERATE]: [{ command: 'object.tween.regenerate', layerUid: 'L', options, starts: [0, 4] }, { command: 'object.tween.regenerate', layerUid: 'L' }],
  [contract.TWEEN_SPLIT]: [{ command: 'object.tween.split', layerUid: 'L', frame: 2, options }, { command: 'object.tween.split', layerUid: 'L', frame: 2 }],
  [contract.TWEEN_REASSIGN]: [{ command: 'object.tween.reassign', layerUid: 'L', span, options }, { command: 'object.tween.reassign', layerUid: 'L', span }],
};
const clone = (v) => JSON.parse(JSON.stringify(v));
const withAt = (payload, pointer, value) => {
  const copy = clone(payload), keys = pointer.split('/').slice(1);
  let at = copy;
  for (const key of keys.slice(0, -1)) at = at[key];
  if (value === undefined) delete at[keys[keys.length - 1]]; else at[keys[keys.length - 1]] = value;
  return copy;
};
const flaws = {
  [contract.TWEEN_GENERATE]: [['/layerUid', ''], ['/layerUid', 'x'.repeat(256)], ['/layerUid', 5], ['/extra', 1], ['/command', 'object.tween.split'], ['/layerUid', undefined],
    ['/selection', [-1]], ['/selection', [4294967296]], ['/selection', [1.5]], ['/selection', new Array(4097).fill(0)], ['/starts', [1]],
    ['/options/resamplePoints', 1], ['/options/resamplePoints', 257], ['/options/step', 0], ['/options/step', 1000], ['/options/skipManual', 'yes'],
    ['/options/easing', new Array(257).fill({ x: 0, y: 0 })], ['/options/easing/0/x', -0.1], ['/options/easing/0/x', 1.1], ['/options/easing/0/z', 1],
    ['/options/easing/1/tx', '1'], ['/options/resamplePoints', undefined], ['/options/extra', true]],
  [contract.TWEEN_REGENERATE]: [['/options/step', 0], ['/selection', '1'], ['/selection', [1]]],
  [contract.TWEEN_SPLIT]: [['/frame', -1], ['/frame', undefined], ['/frame', '2'], ['/selection', [1]], ['/span', span], ['/options/step', 0]],
  [contract.TWEEN_REASSIGN]: [['/span', undefined], ['/span/from', -1], ['/span/to', 1.5], ['/span/overrides', new Array(129).fill({ aId: 'a', bId: 'b' })],
    ['/span/overrides/0/bId', ''], ['/span/overrides/0/aIds', ['a']], ['/span/overrides/1/aIds', []], ['/span/overrides/1/aId', 'a'], ['/span/overrides/0/aId', undefined],
    ['/span/manualStrokes', new Array(129).fill('m')], ['/span/manualStrokes/0', ''], ['/span/easing/0/y', '0'], ['/span/extra', 1], ['/frame', 1], ['/starts', [0]]],
};

test('the contract and the closed schema accept each valid payload and refuse every flaw alike', () => {
  const v = validator(schema);
  for (const op of OPS) {
    assert.equal(contract.admitted(op), true); assert.equal(contract.requiresRevision(op), true);
    const def = { $ref: '#/$defs/' + defOf(op) };
    for (const payload of valid[op]) { contract.payload(op, payload); assert.equal(v.ok(def, payload), true, op + ' ' + JSON.stringify(payload).slice(0, 60)); }
    const base = valid[op][0];
    for (const [pointer, value] of flaws[op]) {
      const payload = withAt(base, pointer, value);
      assert.throws(() => contract.payload(op, payload), TypeError, op + ' contract ' + pointer);
      assert.equal(v.ok(def, payload), false, op + ' schema ' + pointer);
    }
  }
});

test('another command of the family is refused by the command member', () => {
  for (const op of OPS) for (const other of OPS) {
    if (op === other) continue;
    assert.throws(() => contract.payload(op, valid[other][0]), TypeError, op + ' <- ' + other);
  }
});

test('the descriptor declares the four operations with one copyable example each and the SDK correlates the receipts', () => {
  assert.equal(descriptor.id, 'native.object.tween');
  assert.deepEqual(descriptor.operations, OPS);
  assert.deepEqual(descriptor.examples.map((e) => e.request.operation), OPS);
  assert.equal(descriptor.resourceBoundary.maxRequestBytes, 65536);
  assert.equal(descriptor.authority.fallback, 'none'); assert.equal(descriptor.authority.adapterOwnsWritableMirror, false);
  const v = validator(schema);
  for (const example of descriptor.examples) {
    SDK.validateRequest(example.request);
    SDK.validateResponse(example.response, example.request.requestId, example.request.operation);
    assert.equal(example.response.contentRevision, example.request.expectedRevision + 1);
    assert.equal(contract.commandOf(example.request.operation), example.request.payload.command);
    contract.payload(example.request.operation, example.request.payload);
    assert.equal(v.ok({ $ref: '#/$defs/' + defOf(example.request.operation) }, example.request.payload), true);
  }
});

test('the receipts are the compact command result and the revision advances only when an edit applied', () => {
  for (const op of OPS) {
    contract.result(op, { applied: true, historyEntriesAdded: 1 });
    contract.result(op, { applied: false, historyEntriesAdded: 0 });
    for (const bad of [{ applied: true, historyEntriesAdded: 0 }, { applied: false, historyEntriesAdded: 1 }, { applied: true, historyEntriesAdded: 1, layerUid: 'x' }]) {
      assert.throws(() => contract.result(op, bad), TypeError, JSON.stringify(bad));
    }
  }
});
