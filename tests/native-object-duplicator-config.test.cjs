'use strict';
// #2256 Q4: the duplicator.config command and the duplicator read selector: JS contract table, the intent that builds ONE request, the
// shipped schema/descriptor/Rust table agreement, and the hook that hands updateDuplicatorPanel to the native panel.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const contract = require('../src/js/adapters/native-object-component-contract.js');
const reads = require('../src/js/adapters/native-object-component-read-contract.js');
const Duplicator = require('../src/js/adapters/native-object-duplicator.js');

const ROOT = path.resolve(__dirname, '..');
const CONFIG = 'command.document.object.duplicator.config';
const payload = (extra) => Object.assign({ command: 'object.duplicator.config', layerUid: 'layer-a' }, extra);

test('every config member is optional and judged by its kind; unknown members and wrong types are refused', () => {
  for (const ok of [{}, { mode: 'grid' }, { mode: 'path', rows: 1, cols: 30.5, spacingX: -3, count: 500, radius: 0, startAngle: -90 }, { radialOrient: false, pathAlignTangent: true, seed: 0 },
    { staggerRandom: {} }, { staggerRandom: { position: true, rotation: false, scale: true, opacity: false, hue: true } }, { timeOffset: {} },
    { timeOffset: { enabled: true, offsetFrames: -2, direction: 'random' } }]) {
    assert.doesNotThrow(() => contract.payload(CONFIG, payload(ok)), JSON.stringify(ok));
  }
  for (const bad of [{ mode: 'spiral' }, { mode: null }, { rows: '3' }, { rows: null }, { count: Infinity }, { seed: -1 }, { seed: 1.5 }, { seed: 4294967296 }, { radialOrient: 1 },
    { pathAlignTangent: 'true' }, { pathLayerUid: 'layer-b' }, { endAngle: 90 }, { staggerRandom: [] }, { staggerRandom: null }, { staggerRandom: { depth: true } },
    { staggerRandom: { hue: 1 } }, { timeOffset: { direction: 'sideways' } }, { timeOffset: { enabled: 'yes' } }, { timeOffset: { offsetFrames: '1' } }, { timeOffset: { extra: 1 } }, { timeOffset: 7 }]) {
    assert.throws(() => contract.payload(CONFIG, payload(bad)), TypeError, JSON.stringify(bad));
  }
  assert.throws(() => contract.payload(CONFIG, { command: 'object.duplicator.config' }), TypeError, 'the layer is required');
  assert.throws(() => contract.payload(CONFIG, { command: 'object.duplicator.set', layerUid: 'a' }), TypeError);
  assert.equal(contract.receiptOf(CONFIG), 'command');
  assert.ok(contract.OPERATIONS.includes(CONFIG));
});

test('the JS table, the shipped schema, the capability descriptor and the Rust table name the same config members', () => {
  const schema = JSON.parse(fs.readFileSync(path.join(ROOT, 'engineering/application/native-transport-v2.schema.json'), 'utf8'));
  const def = schema.$defs.ObjectDuplicatorConfigPayload;
  assert.equal(def.additionalProperties, false);
  assert.deepEqual(def.required, ['command', 'layerUid']);
  assert.deepEqual(Object.keys(def.properties).sort(), ['command'].concat(contract.members(CONFIG)).sort());
  const branches = schema.$defs.Request.oneOf.filter((b) => b.allOf[1].properties.operation && b.allOf[1].properties.operation.const === CONFIG);
  assert.equal(branches.length, 1);
  const descriptor = JSON.parse(fs.readFileSync(path.join(ROOT, 'engineering/application/capabilities-v2/native-object-duplicator.json'), 'utf8'));
  assert.ok(descriptor.operations.includes(CONFIG));
  assert.ok(descriptor.input.oneOf.some((entry) => entry.$ref.endsWith('/ObjectDuplicatorConfigPayload')));
  const example = descriptor.examples.find((entry) => entry.request.operation === CONFIG);
  assert.doesNotThrow(() => contract.payload(CONFIG, example.request.payload));
  assert.doesNotThrow(() => contract.result(CONFIG, example.response.result));
  const rust = fs.readFileSync(path.join(ROOT, 'nemo-mcp/src/native_object_component_contract.rs'), 'utf8');
  const row = /"duplicator\.config",\s*"object\.duplicator\.config",\s*Receipt::Command,\s*"([^"]+)"/.exec(rust);
  assert.ok(row, 'the Rust table declares the row');
  assert.deepEqual(row[1].split(' ').map((member) => member.split(':')[0]), contract.members(CONFIG), 'same members in the same wire order');
  assert.deepEqual(row[1].split(' ').filter((member) => member.endsWith('?')).map((member) => member.split(':')[0]), contract.optionalMembers(CONFIG));
});

const OPERATION_READ = 'query.document.object.component.read';
test('the duplicator read selector names one layer and the view is judged member by member', () => {
  const pin = { atRevision: 7, documentSnapshotId: 'native-object:document:7' };
  const request = Object.assign({}, pin, { selector: { kind: 'duplicator', layerUid: 'ly_dup' } });
  assert.doesNotThrow(() => reads.payload(OPERATION_READ, request));
  for (const selector of [{ kind: 'duplicator' }, { kind: 'duplicator', layerUid: '' }, { kind: 'duplicator', layerUid: 'x'.repeat(129) }, { kind: 'duplicator', layerUid: 'a', cursor: 0 },
    { kind: 'duplicator', contextId: 'a' }]) {
    assert.throws(() => reads.payload(OPERATION_READ, Object.assign({}, pin, { selector })), TypeError, JSON.stringify(selector));
  }
  const view = Object.assign({}, pin, { kind: 'duplicator', layerUid: 'ly_dup', mode: 'grid', rows: 2, cols: 3, spacingX: 150, spacingY: 150, count: 8, radius: 200, startAngle: 0,
    radialOrient: false, pathAlignTangent: true, pathLayerUid: 'ly_path', seed: 17, sourceMode: 'shape',
    staggerRandom: { position: false, rotation: false, scale: false, opacity: false, hue: false }, timeOffset: { enabled: false, offsetFrames: 1, direction: 'forward' } });
  assert.doesNotThrow(() => reads.result(OPERATION_READ, view, request));
  const patch = (key, value) => Object.assign({}, view, { [key]: value });
  for (const bad of [patch('layerUid', 'other'), patch('mode', 'spiral'), patch('rows', '2'), patch('seed', -1), patch('radialOrient', 0), patch('pathLayerUid', ''), patch('sourceMode', 'pool'),
    patch('staggerRandom', { position: true }), patch('timeOffset', { enabled: false, offsetFrames: 1, direction: 'sideways' }), patch('kind', 'context'), patch('atRevision', 8), patch('extra', 1)]) {
    assert.throws(() => reads.result(OPERATION_READ, bad, request), TypeError, JSON.stringify(bad));
  }
});

function intentHarness(layers) {
  const sent = [];
  const pin = { instanceId: 'instance-A', documentId: 'document-A', contentRevision: 7, documentSnapshotId: 'snap-7', layers: layers.map((layerUid) => ({ layerUid })) };
  const ports = { pin: () => pin, declared: () => true, requestId: (label) => label + '-1',
    async submit(request) { sent.push(request); return { apiVersion: 2, requestId: request.requestId, instanceId: 'instance-A', documentId: 'document-A', contentRevision: 8, ok: true, result: { applied: true, historyEntriesAdded: 1 } }; } };
  return { sent, intent: Duplicator.createDuplicatorIntent(ports, { random: () => 0.123456 }) };
}
test('configure sends ONE closed request pinned to the presented revision and refuses unknown members before sending', async () => {
  const h = intentHarness(['layer-a']);
  const outcome = await h.intent.configure('layer-a', { rows: 4, staggerRandom: { hue: true } });
  assert.equal(outcome.ok, true);
  assert.equal(h.sent.length, 1);
  assert.deepEqual(h.sent[0].payload, { command: 'object.duplicator.config', layerUid: 'layer-a', rows: 4, staggerRandom: { hue: true } });
  assert.equal(h.sent[0].operation, CONFIG);
  assert.equal(h.sent[0].expectedRevision, 7);
  for (const bad of [{ pathLayerUid: 'x' }, { endAngle: 1 }, { rows: 'a' }, null, [], 7]) {
    const refused = await h.intent.configure('layer-a', bad);
    assert.equal(refused.ok, false, JSON.stringify(bad));
  }
  assert.equal((await h.intent.configure('gone', { rows: 1 })).code, 'stale_selection');
  assert.equal((await h.intent.configure('', { rows: 1 })).code, 'invalid_request');
  assert.equal(h.sent.length, 1, 'refused intents send nothing');
});
test('reseed draws Math.floor(random() * 1e6) as one config request', async () => {
  const h = intentHarness(['layer-a']);
  assert.equal((await h.intent.reseed('layer-a')).ok, true);
  assert.deepEqual(h.sent.map((r) => r.payload), [{ command: 'object.duplicator.config', layerUid: 'layer-a', seed: 123456 }]);
  assert.ok(Duplicator.OPERATIONS.includes(CONFIG));
});

test('updateDuplicatorPanel hands the panel to the native owner first and runs the legacy body only when it declines', () => {
  const source = fs.readFileSync(path.join(ROOT, 'src/js/timeline.js'), 'utf8');
  const start = source.indexOf('function updateDuplicatorPanel(){'), end = source.indexOf('function renderDuplicatorSourcePool(', start);
  assert.ok(start >= 0 && end > start);
  const run = (owned) => {
    const touched = [];
    const context = { document: { getElementById: (id) => { touched.push(id); return null; } }, state: { layers: [], activeLayerIdx: 0 },
      NemoNativeObjectDuplicatorPanel: { sync: () => owned } };
    context.window = context;
    vm.createContext(context);
    vm.runInContext(source.slice(start, end), context);
    context.updateDuplicatorPanel();
    return touched;
  };
  assert.deepEqual(run(true), [], 'a native project reads no legacy state and touches no field');
  assert.deepEqual(run(false), ['duplicator-sec'], 'a legacy project runs its own body');
});
