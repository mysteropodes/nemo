'use strict';
// U35/U36 (#2362): the public declaration of the seven Labs operations. The contract table (shared with the Rust
// validator), the closed transport schema and the common SDK accept the same payloads and refuse the same flaws; the
// descriptor examples are copyable requests and receipts.
const test = require('node:test'), assert = require('node:assert/strict'), fs = require('node:fs'), path = require('node:path');
const contract = require('../src/js/adapters/native-object-rig-contract.js');
const SDK = require('../src/js/adapters/native-application.js');
const { validator } = require('./native-object-rig-schema.cjs');

const ROOT = path.resolve(__dirname, '..');
const schema = JSON.parse(fs.readFileSync(path.join(ROOT, 'engineering/application/native-transport-v2.schema.json'), 'utf8'));
const descriptor = JSON.parse(fs.readFileSync(path.join(ROOT, 'engineering/application/capabilities-v2/native-object-labs.json'), 'utf8'));
const OPS = [contract.LABS_SPEED_LINES, contract.LABS_BOIL, contract.LABS_FOLLOW_PATH, contract.LABS_REFERENCE_FILL,
  contract.LABS_PINGPONG, contract.LABS_RETIME, contract.LABS_INTERVAL];
const defOf = (operation) => schema.$defs.Request.oneOf.find((b) => b.allOf[1].properties.operation.const === operation)
  .allOf[1].properties.payload.$ref.replace('#/$defs/', '');
const valid = {
  [contract.LABS_SPEED_LINES]: descriptor.examples[0].request.payload,
  [contract.LABS_BOIL]: descriptor.examples[1].request.payload,
  [contract.LABS_FOLLOW_PATH]: descriptor.examples[2].request.payload,
  [contract.LABS_REFERENCE_FILL]: descriptor.examples[3].request.payload,
  [contract.LABS_PINGPONG]: descriptor.examples[4].request.payload,
  [contract.LABS_RETIME]: descriptor.examples[5].request.payload,
  [contract.LABS_INTERVAL]: descriptor.examples[6].request.payload,
};
const flawsOf = (payload) => {
  const out = [['extra member', { ...payload, extra: 1 }], ['command', { ...payload, command: 'object.labs.other' }]];
  for (const key of Object.keys(payload)) if (key !== 'command') out.push(['missing ' + key, Object.fromEntries(Object.entries(payload).filter(([k]) => k !== key))]);
  return out;
};
const bounds = {
  [contract.LABS_SPEED_LINES]: [['count', 2], ['count', 401], ['count', 3.5], ['jitter', 1.1], ['jitter', -0.1], ['seed', -1], ['seed', 4294967296],
    ['color', [0, 0, 0]], ['color', [0, 0, 0, 1, 1]], ['color', [0, 0, 1.1, 1]], ['opacity', 2], ['brushSize', -1], ['cx', 1e8], ['frame', -1],
    ['layerUid', ''], ['layerUid', 'x'.repeat(257)]],
  [contract.LABS_BOIL]: [['frames', 0], ['frames', 25], ['amplitude', 0.1], ['amplitude', '2'], ['seedStep', 1.5], ['seedStep', 4294967296]],
  [contract.LABS_FOLLOW_PATH]: [['frames', 1], ['frames', 121], ['ease', 'bounce'], ['removeTrajectory', 'yes'], ['strokeIds', []],
    ['strokeIds', Array.from({ length: 257 }, (_, i) => 's' + i)], ['strokeIds', ['']], ['pathId', 5]],
  [contract.LABS_REFERENCE_FILL]: [['point', [1]], ['point', [0, 2e12]], ['point', ['0', 0]], ['color', '#336699'], ['color', 7], ['opacity', 1.5], ['frame', 0.5]],
  [contract.LABS_PINGPONG]: [['times', 0], ['times', 51], ['fromFrame', -1], ['toLayer', '']],
  [contract.LABS_RETIME]: [['spacing', 0], ['spacing', 13], ['toFrame', 1.5]],
  [contract.LABS_INTERVAL]: [['count', 0], ['count', 1000], ['ease', 'bounce'], ['layerUid', 5]],
};

test('the contract and the closed schema accept each valid payload and refuse every flaw alike', () => {
  const v = validator(schema);
  for (const op of OPS) {
    assert.equal(contract.admitted(op), true); assert.equal(contract.requiresRevision(op), true);
    const def = { $ref: '#/$defs/' + defOf(op) };
    contract.payload(op, valid[op]);
    assert.equal(v.ok(def, valid[op]), true, op);
    const flaws = flawsOf(valid[op]).concat(bounds[op].map(([key, value]) => [key + '=' + JSON.stringify(value).slice(0, 20), { ...valid[op], [key]: value }]));
    for (const [why, payload] of flaws) {
      assert.throws(() => contract.payload(op, payload), TypeError, op + ' contract ' + why);
      assert.equal(v.ok(def, payload), false, op + ' schema ' + why);
    }
  }
});

test('the descriptor declares the seven operations with one copyable example each and the SDK correlates the receipts', () => {
  assert.equal(descriptor.id, 'native.object.labs');
  assert.deepEqual(descriptor.operations, OPS);
  assert.deepEqual(descriptor.examples.map((e) => e.request.operation), OPS);
  for (const example of descriptor.examples) {
    SDK.validateRequest(example.request);
    SDK.validateResponse(example.response, example.request.requestId, example.request.operation);
    assert.equal(example.response.contentRevision, example.request.expectedRevision + 1);
    assert.equal(contract.commandOf(example.request.operation), example.request.payload.command);
  }
  const noop = { ...descriptor.examples[1].response, contentRevision: 7, result: { applied: false, historyEntriesAdded: 0 } };
  SDK.validateResponse(noop, noop.requestId, contract.LABS_BOIL);
  assert.throws(() => SDK.validateResponse({ ...noop, result: { applied: true, historyEntriesAdded: 0 } }, noop.requestId, contract.LABS_BOIL), TypeError);
});
