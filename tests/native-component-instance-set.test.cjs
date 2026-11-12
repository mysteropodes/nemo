'use strict';
// #2256 Q5: the component.instance.set command and the instance read selector: JS contract table, the intent that builds ONE request, the shipped
// schema/descriptor/Rust table agreement, and the hook that hands updateCompInstancePanel to the native panel.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const contract = require('../src/js/adapters/native-object-component-contract.js');
const reads = require('../src/js/adapters/native-object-component-read-contract.js');
const Component = require('../src/js/adapters/native-object-component.js');

const ROOT = path.resolve(__dirname, '..');
const SET = 'command.document.object.component.instance.set';
const payload = (extra) => Object.assign({ command: 'object.component.instance.set', layerUid: 'layer-a' }, extra);

test('every member is optional and judged by its kind; unknown members and wrong types are refused', () => {
  for (const ok of [{}, { mode: 'loop' }, { mode: 'single', singleFrame: 0, atFrame: 4294967295 }, { singleFrame: 4294967295 }, { placedAt: -2147483648 }, { placedAt: 2147483647 },
    { speed: 0.1 }, { speed: 8, mode: 'pingpong', placedAt: 3 }]) {
    assert.doesNotThrow(() => contract.payload(SET, payload(ok)), JSON.stringify(ok));
  }
  for (const bad of [{ mode: 'bounce' }, { mode: 'Loop' }, { mode: null }, { singleFrame: -1 }, { singleFrame: 1.5 }, { singleFrame: 4294967296 }, { singleFrame: '3' },
    { placedAt: 2147483648 }, { placedAt: -2147483649 }, { placedAt: 0.5 }, { speed: 0 }, { speed: -1 }, { speed: '2' }, { speed: null }, { speed: Infinity }, { atFrame: -1 },
    { symTrimIn: 1 }, { symbolId: 'sym' }]) {
    assert.throws(() => contract.payload(SET, payload(bad)), TypeError, JSON.stringify(bad));
  }
  assert.throws(() => contract.payload(SET, { command: 'object.component.instance.set' }), TypeError, 'the layer is required');
  assert.throws(() => contract.payload(SET, { command: 'object.component.place', layerUid: 'a' }), TypeError);
  assert.equal(contract.receiptOf(SET), 'command');
  assert.ok(contract.OPERATIONS.includes(SET));
});

test('the JS table, the shipped schema, the capability descriptor and the Rust table name the same members', () => {
  const schema = JSON.parse(fs.readFileSync(path.join(ROOT, 'engineering/application/native-transport-v2.schema.json'), 'utf8'));
  const def = schema.$defs.ObjectComponentInstanceSetPayload;
  assert.equal(def.additionalProperties, false);
  assert.deepEqual(def.required, ['command', 'layerUid']);
  assert.deepEqual(Object.keys(def.properties).sort(), ['command'].concat(contract.members(SET)).sort());
  const branches = schema.$defs.Request.oneOf.filter((b) => b.allOf[1].properties.operation && b.allOf[1].properties.operation.const === SET);
  assert.equal(branches.length, 1);
  const descriptor = JSON.parse(fs.readFileSync(path.join(ROOT, 'engineering/application/capabilities-v2/native-object-component.json'), 'utf8'));
  assert.ok(descriptor.operations.includes(SET));
  assert.ok(descriptor.input.oneOf.some((entry) => entry.$ref.endsWith('/ObjectComponentInstanceSetPayload')));
  const example = descriptor.examples.find((entry) => entry.request.operation === SET);
  assert.doesNotThrow(() => contract.payload(SET, example.request.payload));
  assert.doesNotThrow(() => contract.result(SET, example.response.result));
  const rust = fs.readFileSync(path.join(ROOT, 'nemo-mcp/src/native_object_component_contract.rs'), 'utf8');
  const row = /"component\.instance\.set",\s*"object\.component\.instance\.set",\s*Receipt::Command,\s*"([^"]+)"/.exec(rust);
  assert.ok(row, 'the Rust table declares the row');
  assert.deepEqual(row[1].split(' ').map((member) => member.split(':')[0]), contract.members(SET), 'same members in the same wire order');
  assert.deepEqual(row[1].split(' ').filter((member) => member.endsWith('?')).map((member) => member.split(':')[0]), contract.optionalMembers(SET));
});

const READ = 'query.document.object.component.read';
test('the instance read selector names one layer and one frame and the view is judged member by member', () => {
  const pin = { atRevision: 7, documentSnapshotId: 'native-object:document:7' };
  const request = Object.assign({}, pin, { selector: { kind: 'instance', layerUid: 'ly_inst', frame: 3 } });
  assert.doesNotThrow(() => reads.payload(READ, request));
  for (const selector of [{ kind: 'instance', layerUid: 'a' }, { kind: 'instance', frame: 0 }, { kind: 'instance', layerUid: '', frame: 0 }, { kind: 'instance', layerUid: 'x'.repeat(129), frame: 0 },
    { kind: 'instance', layerUid: 'a', frame: -1 }, { kind: 'instance', layerUid: 'a', frame: 1.5 }, { kind: 'instance', layerUid: 'a', frame: 0, cursor: 0 }]) {
    assert.throws(() => reads.payload(READ, Object.assign({}, pin, { selector })), TypeError, JSON.stringify(selector));
  }
  const view = Object.assign({}, pin, { kind: 'instance', layerUid: 'ly_inst', componentId: 'sym_a', mode: 'once', singleFrame: 0, speed: 1, placedAt: 0, totalFrames: 24, resolvedFrame: 3, explicit: false });
  assert.doesNotThrow(() => reads.result(READ, view, request));
  assert.doesNotThrow(() => reads.result(READ, Object.assign({}, view, { resolvedFrame: null }), request));
  const patch = (key, value) => Object.assign({}, view, { [key]: value });
  for (const bad of [patch('layerUid', 'other'), patch('componentId', ''), patch('mode', 'bounce'), patch('singleFrame', -1), patch('speed', 0), patch('speed', '1'), patch('placedAt', null),
    patch('totalFrames', -1), patch('resolvedFrame', 1.5), patch('resolvedFrame', -1), patch('explicit', 0), patch('kind', 'duplicator'), patch('atRevision', 8), patch('extra', 1)]) {
    assert.throws(() => reads.result(READ, bad, request), TypeError, JSON.stringify(bad));
  }
  const missing = Object.assign({}, view); delete missing.explicit;
  assert.throws(() => reads.result(READ, missing, request), TypeError);
});

function intentHarness(layers, options = {}) {
  const sent = [];
  const pin = { instanceId: 'instance-A', documentId: 'document-A', contentRevision: 7, documentSnapshotId: 'snap-7', layers: layers.map((layerUid) => ({ layerUid })) };
  const ports = { pin: () => pin, declared: () => options.declared !== false, requestId: (label) => label + '-1',
    async submit(request) { sent.push(request); return { apiVersion: 2, requestId: request.requestId, instanceId: 'instance-A', documentId: 'document-A', contentRevision: 8, ok: true, result: { applied: true, historyEntriesAdded: 1 } }; } };
  return { sent, intent: Component.createComponentIntent(ports, {}) };
}
test('setInstance sends ONE closed request pinned to the presented revision and refuses unknown members before sending', async () => {
  const h = intentHarness(['layer-a']);
  const outcome = await h.intent.setInstance('layer-a', { mode: 'pingpong', speed: 2 });
  assert.equal(outcome.ok, true);
  assert.equal(h.sent.length, 1);
  assert.deepEqual(h.sent[0].payload, { command: 'object.component.instance.set', layerUid: 'layer-a', mode: 'pingpong', speed: 2 });
  assert.equal(h.sent[0].operation, SET);
  assert.equal(h.sent[0].expectedRevision, 7);
  for (const bad of [{ symbolId: 'x' }, { symTrimIn: 1 }, { speed: 0 }, { mode: 'bounce' }, { singleFrame: 1.5 }, {}, null, [], 7]) {
    const refused = await h.intent.setInstance('layer-a', bad);
    assert.equal(refused.ok, false, JSON.stringify(bad));
  }
  assert.equal((await h.intent.setInstance('gone', { speed: 2 })).code, 'stale_selection');
  assert.equal((await h.intent.setInstance('', { speed: 2 })).code, 'invalid_request');
  assert.equal(h.sent.length, 1, 'refused intents send nothing');
  assert.ok(Component.OPERATIONS.includes(SET));
});
test('setInstance is typed unavailability while the native declaration is absent, never a legacy write', async () => {
  const h = intentHarness(['layer-a'], { declared: false });
  const out = await h.intent.setInstance('layer-a', { speed: 2 });
  assert.deepEqual([out.ok, out.code, out.reason], [false, 'unavailable', 'native_declaration_absent']);
  assert.equal(h.sent.length, 0);
});

test('updateCompInstancePanel hands the panel to the native owner first and runs the legacy body only when it declines', () => {
  const source = fs.readFileSync(path.join(ROOT, 'src/js/timeline.js'), 'utf8');
  const start = source.indexOf('function updateCompInstancePanel(){'), end = source.indexOf('// Frame-strip: one clickable cell per frame', start);
  assert.ok(start >= 0 && end > start);
  const run = (owned) => {
    const touched = [];
    const context = { document: { getElementById: (id) => { touched.push(id); return { style: {} }; } }, state: { layers: [], activeLayerIdx: 0, activeSymbolId: null },
      NemoNativeComponentInstancePanel: { sync: () => owned } };
    context.window = context;
    vm.createContext(context);
    vm.runInContext(source.slice(start, end), context);
    context.updateCompInstancePanel();
    return touched;
  };
  assert.deepEqual(run(true), [], 'a native project reads no legacy state and touches no field');
  assert.deepEqual(run(false), ['comp-instance-sec'], 'a legacy project runs its own body');
});
