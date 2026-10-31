'use strict';
/* Public declaration of the lane-7 brush resource operations: SDK contract, composition with the other contracts,
 * capability descriptor, closed schema and the v2 transport unions. Run on the composed candidate (it needs the
 * native-application.js seam, the transport schema branches and the descriptor registration). */
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const root = path.resolve(__dirname, '..');
const SDK = require('../src/js/adapters/native-application.js');
const Contract = require('../src/js/adapters/native-bitmap-brush-resources-contract.js');
const Font = require('../src/js/adapters/native-font-contract.js');
const read = (file) => JSON.parse(fs.readFileSync(path.join(root, 'engineering/application', file), 'utf8'));
const schema = read('native-bitmap-brush-resources-v1.schema.json');
const descriptor = read('capabilities-v2/native-bitmap-brush-resources.json');
const oracle = JSON.parse(fs.readFileSync(path.join(__dirname, 'native-bitmap-brush-resources-fixtures', 'canvas-oracle.json'), 'utf8'));
const PUBLISH = Contract.PUBLISH, QUERY = Contract.QUERY;

const publish = () => ({ command: 'object.brush.bitmap.publish', documentSnapshotId: 'snapshot-7', assetId: 'asset-project-asset-1', brushId: 'i1:abc', width: 241, height: 41 });
const inspect = () => ({ action: 'inspect', atRevision: 7, documentSnapshotId: 'snapshot-7', brushId: 'i1:abc' });
const scene = (offset = 0) => ({ action: 'scene', atRevision: 7, documentSnapshotId: 'snapshot-7', contextId: 'scene-root', frame: 12, offset, limit: 1 });
const ink = { version: '3', width: 241, height: 41, bytes: 39524 };
const inkEntry = () => ({ sourceObjectIndex: 5, layerUid: 'L1', strokeId: 'native-brush:1:0', brushId: 'i1:abc', ink: Object.assign({}, ink) });
const missingEntry = () => ({ sourceObjectIndex: 2, layerUid: 'L0', strokeId: 'native-brush:2:0', brushId: 'i1:m1', unavailable: { code: 'ink-missing' } });
const page = (offset, next, total, entries) => ({ atRevision: 7, documentSnapshotId: 'snapshot-7', contextId: 'scene-root', frame: 12, offset, nextOffset: next, totalEntries: total, entries });
const CASES = [
  { operation: PUBLISH, payload: publish(), result: { applied: true, historyEntriesAdded: 0, brushId: 'i1:abc', version: '1', width: 241, height: 41, bytes: 39524 } },
  { operation: PUBLISH, payload: publish(), result: { applied: false, historyEntriesAdded: 0, brushId: 'i1:abc', version: '18446744073709551615', width: 241, height: 41, bytes: 39524 } },
  { operation: QUERY, payload: inspect(), result: { atRevision: 7, documentSnapshotId: 'snapshot-7', brush: Object.assign({ brushId: 'i1:abc', resident: true }, ink) } },
  { operation: QUERY, payload: inspect(), result: { atRevision: 7, documentSnapshotId: 'snapshot-7', brush: { brushId: 'i1:abc', resident: false } } },
  { operation: QUERY, payload: scene(0), result: page(0, 1, 2, [inkEntry()]) },
  { operation: QUERY, payload: scene(1), result: page(1, null, 2, [missingEntry()]) },
  { operation: QUERY, payload: scene(2), result: page(2, null, 2, []) },
];
const request = (c, rev = 7) => {
  const q = { apiVersion: 2, requestId: 'brush-7', instanceId: 'instance-A', documentId: 'document-A', operation: c.operation, payload: structuredClone(c.payload) };
  if (c.operation === PUBLISH) q.expectedRevision = rev;
  return q;
};
const reply = (q, result, rev) => ({ apiVersion: 2, requestId: q.requestId, instanceId: q.instanceId, documentId: q.documentId, contentRevision: rev === undefined ? 7 : rev, ok: true, result });

/* A deliberately small validator for the schema subset the declaration uses (arrays included). */
function valid(def, value) {
  if (def.$ref) return valid(schema.$defs[def.$ref.replace('#/$defs/', '')], value);
  if (def.oneOf) return def.oneOf.filter((d) => valid(d, value)).length === 1;
  if (def.const !== undefined) return JSON.stringify(def.const) === JSON.stringify(value);
  if (def.enum) return def.enum.includes(value);
  const types = [].concat(def.type || []);
  const kind = value === null ? 'null' : Array.isArray(value) ? 'array' : Number.isInteger(value) && types.includes('integer') ? 'integer' : typeof value;
  if (types.length && !types.includes(kind)) return false;
  if (kind === 'string') return !(def.pattern && !new RegExp(def.pattern).test(value)) && !(def.minLength && value.length < def.minLength) && !(def.maxLength && Buffer.byteLength(value) > def.maxLength);
  if (kind === 'number' || kind === 'integer') return !(def.minimum !== undefined && value < def.minimum) && !(def.maximum !== undefined && value > def.maximum);
  if (kind === 'array') return !(def.maxItems !== undefined && value.length > def.maxItems) && value.every((v) => !def.items || valid(def.items, v));
  if (kind !== 'object') return true;
  const keys = Object.keys(value);
  if ((def.required || []).some((k) => !(k in value))) return false;
  return keys.every((k) => def.properties && k in def.properties ? valid(def.properties[k], value[k]) : def.additionalProperties !== false);
}
const flaws = (payload) => {
  const out = [];
  for (const key of Object.keys(payload)) {
    const missing = structuredClone(payload); delete missing[key]; out.push(['missing ' + key, missing]);
    const nulled = structuredClone(payload); nulled[key] = null; out.push(['null ' + key, nulled]);
  }
  const extra = structuredClone(payload); extra.extra = 1; out.push(['extra', extra]);
  return out;
};

test('the closed schema and the SDK contract accept the same corpus and refuse the same single-flaw variants', () => {
  for (const c of CASES) {
    assert.ok(valid(schema.$defs.Input, c.payload), 'schema accepts ' + (c.payload.command || c.payload.action));
    assert.doesNotThrow(() => Contract.payload(c.operation, c.payload));
    assert.ok(valid(schema.$defs.Output, c.result), 'schema accepts the result');
    assert.doesNotThrow(() => Contract.result(c.operation, c.result));
    for (const [label, bad] of flaws(c.payload)) {
      assert.equal(valid(schema.$defs.Input, bad), false, 'schema refuses ' + label);
      assert.throws(() => Contract.payload(c.operation, bad), TypeError, 'contract refuses ' + label);
    }
    for (const [label, bad] of flaws(c.result)) {
      // nextOffset is nullable (the last page): the schema cannot say WHEN null is right, the contract can (checked below).
      if (label !== 'null nextOffset') assert.equal(valid(schema.$defs.Output, bad), false, 'schema refuses result ' + label);
      if (label === 'null nextOffset' && c.result.nextOffset === null) continue;
      assert.throws(() => Contract.result(c.operation, bad), TypeError, 'contract refuses result ' + label);
    }
  }
  const nested = [
    ['zero width', PUBLISH, (p) => { p.width = 0; }, publish()], ['width over 4096', PUBLISH, (p) => { p.width = 4097; }, publish()],
    ['fractional height', PUBLISH, (p) => { p.height = 41.5; }, publish()], ['string width', PUBLISH, (p) => { p.width = '241'; }, publish()],
    ['blank brush id', PUBLISH, (p) => { p.brushId = '  '; }, publish()], ['blank asset id', PUBLISH, (p) => { p.assetId = '\t'; }, publish()],
    ['bad snapshot', PUBLISH, (p) => { p.documentSnapshotId = '-lead'; }, publish()], ['other command', PUBLISH, (p) => { p.command = 'object.brush.bitmap.erase'; }, publish()],
    ['carries bytes', PUBLISH, (p) => { p.bytes = 'QUJD'; }, publish()], ['carries a path', PUBLISH, (p) => { p.path = '/a.png'; }, publish()],
    ['scene limit 2', QUERY, (p) => { p.limit = 2; }, scene()], ['scene limit 0', QUERY, (p) => { p.limit = 0; }, scene()],
    ['scene offset beyond the bound', QUERY, (p) => { p.offset = 4097; }, scene()], ['negative offset', QUERY, (p) => { p.offset = -1; }, scene()],
    ['frame beyond u32', QUERY, (p) => { p.frame = 4294967296; }, scene()], ['context with a space', QUERY, (p) => { p.contextId = 'a b'; }, scene()],
    ['unknown action', QUERY, (p) => { p.action = 'list'; }, scene()], ['blank brush in inspect', QUERY, (p) => { p.brushId = ' '; }, inspect()],
  ];
  for (const [label, operation, mutate, original] of nested) {
    const bad = structuredClone(original);
    mutate(bad);
    assert.equal(valid(schema.$defs.Input, bad), false, 'schema refuses ' + label);
    assert.throws(() => Contract.payload(operation, bad), TypeError, 'contract refuses ' + label);
  }
  const results = [
    ['history entry claimed', (r) => { r.historyEntriesAdded = 1; }, CASES[0]], ['version 0', (r) => { r.version = '0'; }, CASES[0]],
    ['version with a leading zero', (r) => { r.version = '01'; }, CASES[0]], ['numeric version', (r) => { r.version = 1; }, CASES[0]],
    ['cost not width * height * 4', (r) => { r.bytes = 39520; }, CASES[0]], ['zero width', (r) => { r.width = 0; }, CASES[0]],
    ['resident row without a cost', (r) => { r.brush = { brushId: 'i1:abc', resident: true }; }, CASES[2]],
    ['absent row with a version', (r) => { r.brush = { brushId: 'i1:abc', resident: false, version: '1' }; }, CASES[3]],
    ['two entries in a page', (r) => { r.entries.push(inkEntry()); }, CASES[4]], ['entry with ink and a reason', (r) => { r.entries[0].unavailable = { code: 'x' }; }, CASES[4]],
    ['entry with neither', (r) => { delete r.entries[0].ink; }, CASES[4]], ['blank reason code', (r) => { r.entries[0].unavailable.code = ' '; }, CASES[5]],
    ['wrong next offset', (r) => { r.nextOffset = 2; }, CASES[4]], ['next offset at the end', (r) => { r.nextOffset = 3; }, CASES[5]],
    ['entry past the end', (r) => { r.entries.push(inkEntry()); }, CASES[6]], ['total beyond the bound', (r) => { r.totalEntries = 4097; }, CASES[4]],
  ];
  for (const [label, mutate, c] of results) {
    const bad = structuredClone(c.result);
    mutate(bad);
    assert.throws(() => Contract.result(c.operation, bad), TypeError, 'contract refuses ' + label);
  }
  // The cost product, the u64 version edge and the null-exactly-at-the-end rule are the contract's, not the schema's (JSON Schema cannot express them).
  const early = structuredClone(CASES[4].result); early.nextOffset = null;
  assert.equal(valid(schema.$defs.Output, early), true);
  assert.throws(() => Contract.result(QUERY, early), TypeError);
  const cost = structuredClone(CASES[0].result); cost.bytes = 39528;
  assert.equal(valid(schema.$defs.Output, cost), true);
  assert.throws(() => Contract.result(PUBLISH, cost), TypeError);
  const beyond = structuredClone(CASES[0].result); beyond.version = '18446744073709551616';
  assert.equal(valid(schema.$defs.Output, beyond), true);
  assert.throws(() => Contract.result(PUBLISH, beyond), TypeError);
  const opaqueEdge = publish();
  opaqueEdge.brushId = 'a'.repeat(255);
  assert.doesNotThrow(() => Contract.payload(PUBLISH, opaqueEdge));
  opaqueEdge.brushId = 'a'.repeat(256);
  assert.throws(() => Contract.payload(PUBLISH, opaqueEdge), TypeError);
  opaqueEdge.brushId = 'é'.repeat(128);
  assert.throws(() => Contract.payload(PUBLISH, opaqueEdge), TypeError, '128 two-byte characters are 256 bytes');
  opaqueEdge.brushId = 'é'.repeat(127);
  assert.doesNotThrow(() => Contract.payload(PUBLISH, opaqueEdge));
});

test('the declared size limit and cost are the baseline: every verbatim bake satisfies the schema and the contract', () => {
  assert.equal(schema.$defs.Side.maximum, oracle.stats.maxW);
  assert.equal(Contract.MAX_SIDE, 4096);
  assert.equal(schema.$defs.Cost.maximum, 4096 * 4096 * 4);
  for (const row of oracle.rows) {
    const p = Object.assign(publish(), { width: row.canvas.width, height: row.canvas.height });
    assert.doesNotThrow(() => Contract.payload(PUBLISH, p), row.name);
    assert.ok(valid(schema.$defs.Publish, p), row.name);
    const r = { applied: true, historyEntriesAdded: 0, brushId: 'i1:abc', version: '1', width: row.canvas.width, height: row.canvas.height, bytes: row.baselineBytes };
    assert.doesNotThrow(() => Contract.result(PUBLISH, r), row.name);
    assert.ok(valid(schema.$defs.PublishResult, r), row.name);
    assert.ok(row.baselineBytes <= schema.$defs.Cost.maximum, row.name);
  }
});

test('the contract composes in front of the other contracts without taking their operations', () => {
  const seen = [];
  const base = {
    admitted: (op) => op === 'x.base', requiresRevision: (op) => op === 'x.base',
    payload: (...a) => { seen.push(['payload', ...a]); }, result: (...a) => { seen.push(['result', ...a]); },
    correlate: (q) => { seen.push(['correlate', q.operation]); }, paths: (op) => { seen.push(['paths', op]); return null; },
  };
  const composed = Contract.compose(base);
  assert.deepEqual([composed.admitted(PUBLISH), composed.admitted(QUERY), composed.admitted('x.base'), composed.admitted('x.other')], [true, true, true, false]);
  assert.deepEqual([composed.requiresRevision(PUBLISH), composed.requiresRevision(QUERY), composed.requiresRevision('x.base')], [true, false, true]);
  assert.deepEqual([Contract.requiresRevision(PUBLISH), Contract.requiresRevision(QUERY), Contract.admitted('query.document.object')], [true, false, false]);
  composed.payload(PUBLISH, publish(), {});
  assert.deepEqual(seen, [], 'an owned operation never reaches the base');
  composed.payload('x.base', { a: 1 }, { env: 1 });
  composed.result('x.base', { r: 1 });
  composed.correlate({ operation: 'x.base' }, {});
  composed.paths('x.base');
  assert.deepEqual(seen, [['payload', 'x.base', { a: 1 }, { env: 1 }], ['result', 'x.base', { r: 1 }], ['correlate', 'x.base'], ['paths', 'x.base']]);
  assert.equal(composed.paths(PUBLISH).has('payload.brushId'), true);
  assert.equal(composed.paths(QUERY).has('result.entries.3.layerUid'), true);
  assert.equal(composed.paths(QUERY).has('payload.contextId'), false);
  assert.equal(Contract.compose(null).admitted(PUBLISH), true);
  const real = Contract.compose(Font);
  assert.equal(real.admitted('command.document.object.asset.font.import'), true);
  assert.equal(real.requiresRevision('query.document.object.asset.font'), false, 'a font read still forbids a revision');
  assert.equal(real.requiresRevision(PUBLISH), true);
  // Composing over a composed contract keeps every layer reachable (the lane-6 contract composes at the same seam).
  const layered = Contract.compose(Contract.compose(Font));
  assert.deepEqual([layered.admitted(PUBLISH), layered.admitted('query.document.object.asset.font')], [true, true]);
});

test('the SDK transports every owned operation once, validates it first and keeps the font and legacy operations', async () => {
  for (const c of CASES) {
    const q = request(c), sent = [];
    const adapter = SDK.createNativeApplicationAdapter('lane7', { dispatch(value) { sent.push(value); return reply(q, c.result); } });
    const response = await adapter.dispatch(q);
    assert.equal(sent.length, 1);
    assert.deepEqual(sent[0], q);
    assert.notEqual(sent[0], q, 'the transport receives a clone');
    assert.deepEqual(response, reply(q, c.result));
  }
  const bare = SDK.createNativeApplicationAdapter('lane7', { dispatch() { throw new Error('must not dispatch'); } });
  for (const c of CASES) {
    const q = request(c);
    if (c.operation === PUBLISH) delete q.expectedRevision; else q.expectedRevision = 7;
    assert.throws(() => bare.dispatch(q), TypeError, 'revision presence: ' + (c.payload.command || c.payload.action));
    assert.throws(() => bare.dispatch(Object.assign(request(c), { cancelledBeforeDispatch: 'x' })), TypeError);
    for (const [label, bad] of flaws(c.payload)) assert.throws(() => bare.dispatch(Object.assign(request(c), { payload: bad })), TypeError, label);
  }
  const font = { apiVersion: 2, requestId: 'font-1', instanceId: 'instance-A', documentId: 'document-A', expectedRevision: 7, operation: 'command.document.object.asset.font.import',
    payload: { command: 'object.asset.font.import', documentSnapshotId: 'snapshot-7', lifecycleGeneration: 1, source: { kind: 'bundled', fontKey: 'Roboto-Regular' } } };
  assert.doesNotThrow(() => SDK.validateRequest(font));
  const policy = { apiVersion: 2, requestId: 'p-1', instanceId: 'instance-A', documentId: 'document-A', expectedRevision: 7, operation: 'command.document.object.layer.policy.edit',
    payload: { command: 'object.layer.policy.edit', layerUid: 'L0', flag: 'locked', value: true } };
  assert.doesNotThrow(() => SDK.validateRequest(policy));
  assert.throws(() => SDK.validateRequest(Object.assign({}, font, { operation: 'command.document.object.brush.nope' })), TypeError);
});

test('no pixel, canvas or byte payload is accepted anywhere in an owned request; authored ids that look binary are', () => {
  const base = request(CASES[0]);
  const carriers = [new Uint8Array(4), new ArrayBuffer(4), new Blob(['x']), { getContext() {} }, { canvas: {} }, Buffer.from('abc')];
  for (const carrier of carriers) {
    for (const where of ['payload', 'top']) {
      const q = structuredClone(base);
      if (where === 'payload') q.payload.pixels = carrier; else q.pixels = carrier;
      assert.throws(() => SDK.validateRequest(q), TypeError, where + ' ' + Object.prototype.toString.call(carrier));
    }
  }
  const dataUrl = structuredClone(base);
  dataUrl.payload.dataUrl = 'data:image/png;base64,QUJD';
  assert.throws(() => SDK.validateRequest(dataUrl), TypeError, 'a data URL in any unlisted member is refused');
  const authored = structuredClone(base);
  authored.payload.brushId = 'data:looks-binary';
  authored.payload.assetId = 'blob:also-authored';
  assert.doesNotThrow(() => SDK.validateRequest(authored), 'brush and asset ids are authored text');
  const snapshot = structuredClone(base);
  snapshot.payload.documentSnapshotId = 'data:x';
  assert.throws(() => SDK.validateRequest(snapshot), TypeError);
  const q = request(CASES[4]);
  const r = reply(q, page(0, 1, 2, [Object.assign(inkEntry(), { brushId: 'data:authored', layerUid: 'blob:authored' })]));
  assert.doesNotThrow(() => SDK.validateResponse(r, q.requestId, QUERY), 'scene entry ids are authored text');
  const leaked = reply(q, page(0, 1, 2, [inkEntry()]));
  leaked.result.entries[0].ink.version = 'data:x';
  assert.throws(() => SDK.validateResponse(leaked, q.requestId, QUERY), TypeError);
});

test('the SDK refuses an uncorrelated answer of every owned operation', async () => {
  for (const c of CASES) {
    const q = request(c);
    const mutate = [
      (r) => { r.requestId = 'foreign'; }, (r) => { r.instanceId = 'foreign'; }, (r) => { r.documentId = 'foreign'; },
      (r) => { r.contentRevision = 8; }, (r) => { r.contentRevision = 6; }, (r) => { r.result.padding = 'x'.repeat(5000); }, (r) => { r.result.extra = 1; },
    ];
    if (c.operation === PUBLISH) mutate.push((r) => { r.result.brushId = 'other'; }, (r) => { r.result.width = 240; r.result.bytes = 240 * 41 * 4; }, (r) => { r.result.historyEntriesAdded = 1; });
    else mutate.push((r) => { r.result.documentSnapshotId = 'snapshot-6'; }, (r) => { r.result.atRevision = 6; });
    if (c.payload.action === 'inspect') mutate.push((r) => { r.result.brush.brushId = 'other'; });
    if (c.payload.action === 'scene') mutate.push((r) => { r.result.frame = 13; }, (r) => { r.result.contextId = 'other'; }, (r) => { r.result.offset += 1; });
    for (const flaw of mutate) {
      const response = reply(q, structuredClone(c.result));
      flaw(response);
      const adapter = SDK.createNativeApplicationAdapter('lane7', { dispatch() { return response; } });
      await assert.rejects(adapter.dispatch(q), `${c.operation} ${String(flaw)}`);
    }
  }
});

test('a refusal is a plain typed error and a no-op publication keeps its revision', async () => {
  const q = request(CASES[1]);
  const reused = reply(q, CASES[1].result);
  assert.equal(reused.contentRevision, 7);
  const ok = SDK.createNativeApplicationAdapter('lane7', { dispatch() { return reused; } });
  assert.deepEqual(await ok.dispatch(q), reused);
  const refused = { apiVersion: 2, requestId: q.requestId, instanceId: q.instanceId, documentId: q.documentId, contentRevision: 7, ok: false, error: { code: 'not_found', message: 'Native brush request refused.', details: { reason: 'missing_asset' } } };
  const adapter = SDK.createNativeApplicationAdapter('lane7', { dispatch() { return refused; } });
  assert.equal((await adapter.dispatch(q)).error.code, 'not_found');
});

test('descriptor: both operations are declared, available on the session-owned store, and neither has a fallback', () => {
  assert.deepEqual([descriptor.id, descriptor.operations, descriptor.availability.state], ['native.object.brush-resources', [PUBLISH, QUERY], 'available']);
  assert.equal(descriptor.availability.reason, null);
  assert.deepEqual([descriptor.schemaVersion, descriptor.apiVersion, descriptor.version, descriptor.status], [2, 2, 1, 'active']);
  assert.deepEqual(descriptor.authority, { document: 'native-engine::history::NativeObjectHistory', resources: 'native-engine::bitmap_brush_resources_store::BrushBitmapStore', adapterOwnsWritableMirror: false, fallback: 'none' });
  assert.equal(descriptor.resourceBoundary.maxRequestBytes, 4096);
  assert.match(descriptor.resourceBoundary.policy, /no pixel, canvas or byte payload/);
  assert.ok(descriptor.commonErrorCodes.includes('unavailable') && descriptor.commonErrorCodes.includes('stale_revision') && descriptor.commonErrorCodes.includes('busy_conflict'));
  assert.deepEqual(descriptor.ports, [{ label: 'ui', state: 'pending' }, { label: 'mcp', state: 'pending' }]);
  // Descriptor examples are validated against the whole advertised union by the MCP crate tests; the fixed pairs live in the schema file.
  assert.deepEqual(descriptor.examples, []);
  assert.equal(schema.examples.length, 3);
  for (const e of schema.examples) {
    assert.doesNotThrow(() => SDK.validateRequest(e.request), e.label);
    assert.doesNotThrow(() => SDK.validateResponse(e.response, e.request.requestId, e.request.operation), e.label);
    assert.ok(valid(schema.$defs.Input, e.request.payload), e.label);
    assert.ok(valid(schema.$defs.Output, e.response.result), e.label);
    assert.doesNotThrow(() => Contract.correlate(e.request, e.response), e.label);
    assert.doesNotThrow(() => Contract.result(e.request.operation, e.response.result), e.label);
  }
  assert.deepEqual(descriptor.input.oneOf.map((v) => v.$ref), ['../native-bitmap-brush-resources-v1.schema.json#/$defs/Publish', '../native-bitmap-brush-resources-v1.schema.json#/$defs/Query']);
  assert.equal(descriptor.output.$ref, '../native-bitmap-brush-resources-v1.schema.json#/$defs/Output');
});

test('the v2 request union declares each operation exactly once with its revision kind, and the result union stays exclusive', () => {
  const transport = read('native-transport-v2.schema.json');
  const branches = transport.$defs.Request.oneOf;
  for (const [operation, ref, kind] of [[PUBLISH, 'Publish', 'RequiredRevisionRequest'], [QUERY, 'Query', 'ForbiddenRevisionRequest']]) {
    const found = branches.filter((b) => b.allOf[1].properties.operation.const === operation);
    assert.equal(found.length, 1, operation);
    assert.equal(found[0].allOf[0].$ref, '#/$defs/' + kind, operation);
    assert.equal(found[0].allOf[1].properties.payload.$ref, 'native-bitmap-brush-resources-v1.schema.json#/$defs/' + ref, operation);
  }
  const all = [];
  for (const b of branches) for (const v of [].concat(b.allOf[1].properties.operation.enum || b.allOf[1].properties.operation.const)) all.push(v);
  assert.equal(new Set(all).size, all.length, 'every operation is declared once in the union');
  const results = transport.$defs.ResponseBase.properties.result.oneOf.map((entry) => entry.$ref);
  for (const name of ['PublishResult', 'InspectResult', 'SceneResult']) {
    assert.equal(results.filter((ref) => ref === 'native-bitmap-brush-resources-v1.schema.json#/$defs/' + name).length, 1, name);
  }
  // Exclusivity: a published receipt matches none of the neighbouring result shapes and vice versa.
  const neighbours = results.filter((ref) => ref.startsWith('#/$defs/')).map((ref) => transport.$defs[ref.slice('#/$defs/'.length)]);
  assert.ok(neighbours.length >= 10, 'the exclusivity check compares against the real neighbouring result shapes');
  const sample = [CASES[0].result, CASES[2].result, CASES[4].result];
  for (const result of sample) {
    for (const n of neighbours) {
      const closed = n.additionalProperties === false && (n.required || []).every((k) => k in result) && Object.keys(result).every((k) => k in (n.properties || {}));
      assert.equal(closed, false, 'a brush result must not also be a ' + JSON.stringify(Object.keys(n.properties || {}).slice(0, 3)));
    }
  }
  assert.equal(transport.$id.replace(/[^/]*$/, ''), schema.$id.replace(/[^/]*$/, ''), 'the relative reference resolves to the schema $id');
});

test('schema: every object is closed and the bounds are the contract constants', () => {
  const objects = [];
  (function walk(node, at) {
    if (Array.isArray(node)) return node.forEach((v, i) => walk(v, at + '/' + i));
    if (!node || typeof node !== 'object') return;
    if (node.type === 'object') objects.push([at, node]);
    Object.keys(node).forEach((k) => walk(node[k], at + '/' + k));
  }(schema.$defs, '$defs'));
  assert.ok(objects.length >= 12);
  for (const [at, node] of objects) assert.equal(node.additionalProperties, false, at);
  const d = schema.$defs;
  assert.deepEqual([d.Side.minimum, d.Side.maximum], [1, Contract.MAX_SIDE]);
  assert.equal(d.SceneQuery.properties.limit.const, 1);
  assert.deepEqual([d.SceneQuery.properties.offset.maximum, d.SceneResult.properties.totalEntries.maximum, d.SceneResult.properties.entries.maxItems], [Contract.MAX_ENTRIES, Contract.MAX_ENTRIES, 1]);
  assert.equal(d.PublishResult.properties.historyEntriesAdded.const, 0);
  assert.equal(d.Publish.properties.command.const, 'object.brush.bitmap.publish');
  assert.deepEqual(d.Input.oneOf.map((v) => v.$ref), ['#/$defs/Publish', '#/$defs/InspectQuery', '#/$defs/SceneQuery']);
  assert.deepEqual(d.Output.oneOf.map((v) => v.$ref), ['#/$defs/PublishResult', '#/$defs/InspectResult', '#/$defs/SceneResult']);
  assert.equal(new RegExp(d.Version.pattern).test('18446744073709551615'), true);
  assert.equal(new RegExp(d.Version.pattern).test('0'), false);
  assert.equal(schema.$id, 'https://github.com/mysteropodes/nemo/engineering/application/native-bitmap-brush-resources-v1.schema.json');
});
