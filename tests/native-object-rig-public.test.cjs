'use strict';
// L03-T public declaration of the six lane-3 object operations and the seven U35/U36 Labs operations: the contract, the Rust validator data, the common SDK, the
// capability descriptors and the closed transport schema agree on one table and on one corpus. Run on the COMPOSED candidate
// (SDK seam patch + schema patch applied).
const test = require('node:test'), assert = require('node:assert/strict'), fs = require('node:fs'), path = require('node:path');
const contract = require('../src/js/adapters/native-object-rig-contract.js');
const seam = require('../src/js/adapters/native-object-rig-seam.js');
const SDK = require('../src/js/adapters/native-application.js');
const { validator } = require('./native-object-rig-schema.cjs');

const ROOT = path.resolve(__dirname, '..');
const read = (file) => fs.readFileSync(path.join(ROOT, file), 'utf8');
const corpus = JSON.parse(read('tests/native-object-rig-fixtures/public-corpus.json'));
const schema = JSON.parse(read('engineering/application/native-transport-v2.schema.json'));
const P = 'command.document.object.';
const SUFFIXES = ['image.mesh.edit', 'widget.create', 'widget.write', 'widget.settings', 'rig.edit', 'rig.pose', 'labs.speedLines', 'labs.boil', 'labs.followPath', 'labs.referenceFill', 'labs.pingpong', 'labs.retime', 'labs.interval'];
const COMMANDS = { 'image.mesh.edit': 'object.image.mesh.edit', 'widget.create': 'object.widget.create', 'widget.write': 'object.widget.write', 'widget.settings': 'object.widget.settings', 'rig.edit': 'object.rig.edit', 'rig.pose': 'object.rig.pose' , 'labs.speedLines': 'object.labs.speedLines', 'labs.boil': 'object.labs.boil', 'labs.followPath': 'object.labs.followPath', 'labs.referenceFill': 'object.labs.referenceFill', 'labs.pingpong': 'object.labs.pingpong', 'labs.retime': 'object.labs.retime', 'labs.interval': 'object.labs.interval' };
// The original agreement corpus covers the six lane-3 operations; the U36 Labs rows are tests/native-object-labs-public.test.cjs.
const ORIGINAL = SUFFIXES.slice(0, 6).map((s) => P + s);
const envelope = (operation, payload, extra) => Object.assign({ apiVersion: 2, requestId: 'edit-1', instanceId: 'instance-A', documentId: 'document-A', expectedRevision: 7, operation, payload }, extra);
const defOf = (operation) => {
  const branches = schema.$defs.Request.oneOf.filter((b) => b.allOf[1].properties.operation.const === operation);
  assert.equal(branches.length, 1, operation + ' has exactly one request branch');
  assert.equal(branches[0].allOf[0].$ref, '#/$defs/RequiredRevisionRequest');
  return branches[0].allOf[1].properties.payload.$ref.replace('#/$defs/', '');
};

test('the contract admits exactly the thirteen declared operations, all revisioned commands, and nothing else', () => {
  assert.deepEqual(contract.OPERATIONS, SUFFIXES.map((s) => P + s));
  for (const op of contract.OPERATIONS) {
    assert.equal(contract.admitted(op), true); assert.equal(contract.requiresRevision(op), true); assert.equal(seam.admitted(op), true);
  }
  for (const op of ['command.document.object.layer.create', 'command.document.object.layer.policy.edit', P + 'image.mesh', P + 'rig', P + 'toString', P + '__proto__',
    P + 'widget.create ', 'query.document.object', '', null, undefined, 5]) assert.equal(contract.admitted(op), false, String(op));
  for (const suffix of SUFFIXES) assert.equal(contract.commandOf(P + suffix), COMMANDS[suffix]);
});

test('the agreement corpus: every valid payload is accepted and every single flaw is refused by the contract', () => {
  assert.equal(corpus.valid.length, 28);
  assert.ok(corpus.flaws.length >= 700);
  for (const row of corpus.valid) contract.payload(row.operation, row.payload);
  const seen = new Set(corpus.valid.map((row) => row.operation));
  assert.deepEqual([...seen].sort(), ORIGINAL.slice().sort(), 'every original operation has a valid row');
  for (const row of corpus.flaws) assert.throws(() => contract.payload(row.operation, row.payload), TypeError, row.operation + ' ' + row.path + ' ' + row.why);
  const byOperation = new Map();
  corpus.flaws.forEach((row) => byOperation.set(row.operation, (byOperation.get(row.operation) || 0) + 1));
  for (const op of ORIGINAL) assert.ok(byOperation.get(op) >= 20, op + ' has its own flaws');
});

test('the closed schema accepts the same payloads and refuses the same flaws (except the owner-computed perimeter)', () => {
  const v = validator(schema);
  for (const row of corpus.valid) assert.equal(v.ok({ $ref: '#/$defs/' + defOf(row.operation) }, row.payload), true, row.operation + ' ' + JSON.stringify(row.payload).slice(0, 100));
  let notStated = 0;
  for (const row of corpus.flaws) {
    const accepted = v.ok({ $ref: '#/$defs/' + defOf(row.operation) }, row.payload);
    if (row.schema === false) { notStated += 1; assert.equal(accepted, true, 'the schema cannot state: ' + row.why); } else assert.equal(accepted, false, row.operation + ' ' + row.path + ' ' + row.why);
  }
  assert.equal(notStated, 1, 'only the outline perimeter is beyond the schema');
});

test('array bounds the corpus leaves out are enforced by the contract and the schema alike', () => {
  const v = validator(schema);
  const outline = (n) => ({ command: 'object.image.mesh.edit', meshId: 'im_1', action: { kind: 'setOutline', outline: Array.from({ length: n }, () => [0.5, 0.5]) } });
  const bone = (n) => ({ command: 'object.rig.edit', layerUid: 'layer-a', action: { kind: 'drawBone', closed: false, segments: Array.from({ length: n }, () => ({ point: [0, 0], handleIn: [0, 0], handleOut: [0, 0] })) } });
  for (const [op, make, max, min] of [[P + 'image.mesh.edit', outline, 512, 3], [P + 'rig.edit', bone, 256, 2]]) {
    const def = { $ref: '#/$defs/' + defOf(op) };
    for (const [n, accepted] of [[max, true], [max + 1, false], [min, true], [min - 1, false]]) {
      assert.equal(v.ok(def, make(n)), accepted, op + ' schema ' + n);
      if (accepted) contract.payload(op, make(n)); else assert.throws(() => contract.payload(op, make(n)), TypeError, op + ' contract ' + n);
    }
  }
});

test('the outline perimeter is bounded at 16 normalized units like the owner', () => {
  const op = P + 'image.mesh.edit';
  const body = (outline) => ({ command: 'object.image.mesh.edit', meshId: 'im_1', action: { kind: 'setOutline', outline } });
  contract.payload(op, body([[-4, 0], [4, 0], [-4, 0]]));
  assert.throws(() => contract.payload(op, body([[-4, 0], [4, 0], [-4, 0.5]])), TypeError);
  assert.throws(() => contract.payload(op, body([[-4, -4], [4, -4], [4, 4]])), TypeError);
});

test('identities are bounded in UTF-8 bytes: layer ids of the rig 128, widget layer ids any non-empty string', () => {
  const pose = (uid) => ({ command: 'object.rig.pose', layerUid: uid, action: { kind: 'reset' } });
  const write = (uid) => ({ command: 'object.widget.write', layerUid: uid, frame: 0, to: { kind: 'rest' } });
  contract.payload(P + 'rig.pose', pose('g'.repeat(128))); contract.payload(P + 'rig.pose', pose('é'.repeat(64)));
  assert.throws(() => contract.payload(P + 'rig.pose', pose('g'.repeat(129))), TypeError);
  assert.throws(() => contract.payload(P + 'rig.pose', pose('é'.repeat(65))), TypeError);
  contract.payload(P + 'widget.write', write('g'.repeat(1000))); contract.payload(P + 'widget.write', write('data:opaque authored layer'));
  assert.throws(() => contract.payload(P + 'widget.write', write('')), TypeError);
});

test('results: command receipts correlate applied and the history count; a widget creation must name its minted layer', () => {
  for (const suffix of SUFFIXES.filter((s) => s !== 'widget.create')) {
    contract.result(P + suffix, { applied: true, historyEntriesAdded: 1 });
    contract.result(P + suffix, { applied: false, historyEntriesAdded: 0 });
    for (const bad of [{ applied: true, historyEntriesAdded: 0 }, { applied: false, historyEntriesAdded: 1 }, { applied: true, historyEntriesAdded: 1, layerUid: 'x' },
      { applied: 1, historyEntriesAdded: 1 }, { applied: true }, null, []]) assert.throws(() => contract.result(P + suffix, bad), TypeError, suffix + JSON.stringify(bad));
  }
  contract.result(P + 'widget.create', { applied: true, historyEntriesAdded: 1, layerUid: 'native-layer:8:0' });
  for (const bad of [{ applied: true, historyEntriesAdded: 1 }, { applied: false, historyEntriesAdded: 0, layerUid: 'x' }, { applied: true, historyEntriesAdded: 1, layerUid: '' }]) {
    assert.throws(() => contract.result(P + 'widget.create', bad), TypeError);
  }
});

test('the common SDK admits each operation, requires expectedRevision, accepts opaque data: identities and refuses binary payloads', () => {
  for (const row of corpus.valid) {
    SDK.validateRequest(envelope(row.operation, row.payload));
    SDK.validateRequest(envelope(row.operation, row.payload, { cancelledBeforeDispatch: true }));
    const without = envelope(row.operation, row.payload); delete without.expectedRevision;
    assert.throws(() => SDK.validateRequest(without), /expectedRevision presence/, row.operation);
    assert.throws(() => SDK.validateRequest(envelope(row.operation, row.payload, { expectedRevision: -1 })), TypeError);
    assert.throws(() => SDK.validateRequest(envelope(row.operation, Object.assign({}, row.payload, { extra: true }))), TypeError);
  }
  SDK.validateRequest(envelope(P + 'widget.write', { command: 'object.widget.write', layerUid: 'data:opaque authored layer', frame: 0, to: { kind: 'rest' } }));
  assert.throws(() => SDK.validateRequest(envelope(P + 'rig.pose', { command: 'object.rig.pose', layerUid: new Uint8Array(4), action: { kind: 'reset' } })), TypeError);
  assert.throws(() => SDK.validateRequest(envelope(P + 'rig.poses', { command: 'object.rig.pose', layerUid: 'l', action: { kind: 'reset' } })), /not admitted/);
  const big = { command: 'object.rig.edit', layerUid: 'l', action: { kind: 'drawBone', closed: false, segments: Array.from({ length: 100 }, (_, i) => ({ point: [i, i], handleIn: [0, 0], handleOut: [1, 1] })) } };
  assert.throws(() => SDK.validateRequest(envelope(P + 'rig.edit', big)), /4096/);
});

test('the SDK correlates each receipt: identity, document replacement and the exact revision advance', async () => {
  for (const row of corpus.valid.filter((r, i, all) => all.findIndex((o) => o.operation === r.operation) === i)) {
    const q = envelope(row.operation, row.payload);
    const created = row.operation === P + 'widget.create';
    const result = (applied) => (created ? { applied: true, historyEntriesAdded: 1, layerUid: 'native-layer:8:0' } : { applied, historyEntriesAdded: applied ? 1 : 0 });
    const reply = (applied, mutate) => () => {
      const r = { apiVersion: 2, requestId: 'edit-1', instanceId: 'instance-A', documentId: 'document-A', contentRevision: 7 + (applied ? 1 : 0), ok: true, result: result(applied) };
      if (mutate) mutate(r);
      return r;
    };
    const send = (dispatch) => SDK.createNativeApplicationAdapter('c', { dispatch }).dispatch(q);
    assert.equal((await send(reply(true))).contentRevision, 8);
    if (!created) assert.equal((await send(reply(false))).contentRevision, 7);
    for (const mutate of [(r) => { r.contentRevision = created ? 7 : 9; }, (r) => { r.instanceId = 'foreign'; }, (r) => { r.documentId = 'foreign'; }, (r) => { r.requestId = 'foreign'; }]) {
      await assert.rejects(send(reply(true, mutate)), TypeError, row.operation);
    }
    if (!created) await assert.rejects(send(reply(true, (r) => { r.contentRevision = 7; })), /revision mismatch/);
    const replaced = await send(() => ({ apiVersion: 2, requestId: 'edit-1', instanceId: 'instance-A', documentId: 'document-B', contentRevision: 3, ok: false, error: { code: 'wrong_document', message: 'replaced' } }));
    assert.equal(replaced.error.code, 'wrong_document');
    await assert.rejects(send(() => ({ apiVersion: 2, requestId: 'edit-1', instanceId: 'instance-A', documentId: 'document-B', contentRevision: 3, ok: false, error: { code: 'not_found', message: 'x' } })), /document mismatch/);
  }
});

test('the injected seam is the font contract composed with the lane-3 contract and declares nothing else', () => {
  assert.equal(seam.admitted('command.document.object.asset.font.import'), true);
  assert.equal(contract.admitted('command.document.object.asset.font.import'), false);
  assert.equal(seam.admitted(P + 'rig.edit'), true);
  assert.equal(seam.admitted('command.document.object.layer.policy.edit'), false, 'the F06 policy command stays a plain SDK operation');
  assert.equal(seam.requiresRevision('query.document.object.asset.font'), false);
  assert.equal(seam.requiresRevision(P + 'rig.edit'), true);
  assert.equal(seam.paths('query.document.object'), null);
  assert.equal(seam.paths(P + 'rig.edit').has('payload.action.target.strokeId'), true);
  assert.equal(seam.paths(P + 'widget.create').has('result.layerUid'), true);
  assert.equal(seam.paths(P + 'widget.create').has('result.something'), false);
  assert.equal(typeof seam.compose, 'function', 'seams of other lanes chain through compose');
  assert.equal(contract.compose(null).admitted('command.document.object.asset.font.import'), false);
  const chained = contract.compose({ admitted: (op) => op === 'x', requiresRevision: () => true, payload() {}, result() {}, correlate() {}, paths: () => null });
  assert.equal(chained.admitted('x'), true); assert.equal(chained.admitted(P + 'rig.pose'), true);
  const fontRequest = { apiVersion: 2, requestId: 'font-1', instanceId: 'instance-A', documentId: 'document-A', expectedRevision: 7, operation: 'command.document.object.asset.font.import',
    payload: { command: 'object.asset.font.import', documentSnapshotId: 'native-object:document-A:7', lifecycleGeneration: 1, source: { kind: 'bundled', fontKey: 'Roboto-Regular' } } };
  SDK.validateRequest(fontRequest);
  assert.throws(() => SDK.validateRequest(Object.assign({}, fontRequest, { payload: Object.assign({}, fontRequest.payload, { source: { kind: 'bundled', fontKey: 'Comic' } }) })), TypeError);
});

test('the Rust validator data is the same table as the JS contract', () => {
  const rust = read('nemo-mcp/src/native_object_rig_contract.rs');
  const match = /const SPEC: &str = r####"([\s\S]*?)"####;/.exec(rust);
  assert.ok(match, 'the generated table is a raw string constant');
  assert.deepEqual(JSON.parse(match[1]), JSON.parse(JSON.stringify(contract.SPEC)));
});

test('capability descriptors and the closed transport schema declare exactly the contract operations', () => {
  const dir = path.join(ROOT, 'engineering/application/capabilities-v2');
  const ids = new Set(), declared = [];
  for (const family of ['image-mesh', 'widget', 'rig', 'labs']) {
    const d = JSON.parse(fs.readFileSync(path.join(dir, 'native-object-' + family + '.json'), 'utf8'));
    assert.equal(d.schemaVersion, 2); assert.equal(d.apiVersion, 2); assert.equal(d.id, 'native.object.' + family);
    assert.deepEqual(d.availability, { state: 'available', reason: null });
    assert.equal(d.authority.adapterOwnsWritableMirror, false); assert.equal(d.authority.fallback, 'none'); assert.equal(d.resourceBoundary.maxRequestBytes, 4096);
    assert.equal(ids.has(d.id), false); ids.add(d.id);
    assert.equal(d.input.oneOf.length, d.operations.length);
    d.input.oneOf.forEach((entry, i) => assert.equal(entry.$ref, '../native-transport-v2.schema.json#/$defs/' + defOf(d.operations[i])));
    for (const example of d.examples) {
      SDK.validateRequest(example.request);
      SDK.validateResponse(example.response, example.request.requestId, example.request.operation);
      assert.equal(d.operations.includes(example.request.operation), true);
      assert.equal(example.response.contentRevision, example.request.expectedRevision + (example.response.result.applied ? 1 : 0));
      assert.equal(validator(schema).ok({ $ref: '#/$defs/' + defOf(example.request.operation) }, example.request.payload), true);
    }
    assert.deepEqual(d.examples.map((e) => e.request.operation), d.operations, 'one copyable example per operation, in declaration order');
    declared.push(...d.operations);
  }
  assert.deepEqual(declared.slice().sort(), contract.OPERATIONS.slice().sort());
  for (const suffix of SUFFIXES) {
    const def = schema.$defs[defOf(P + suffix)];
    assert.equal(def.additionalProperties, false);
    assert.equal(def.properties.command.const, COMMANDS[suffix]);
    assert.ok(def.required.includes('command'));
  }
  assert.equal(schema.$defs.Request.oneOf.filter((b) => b.allOf[1].properties.operation.const === 'command.document.object.layer.policy.edit').length, 1, 'F06 branch is untouched');
  assert.equal(schema.$defs.Request.oneOf.filter((b) => b.allOf[1].properties.operation.const === 'command.document.object.layer.create').length, 1, 'layer create branch is untouched');
});
