'use strict';
/* Public declaration of the lane-6 object operations: SDK contract, composition with the font contract, capability
 * descriptors and the closed schema. Run on the composed candidate (it needs the native-application.js header patch). */
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const root = path.resolve(__dirname, '..');
const SDK = require('../src/js/adapters/native-application.js');
const Contract = require('../src/js/adapters/native-object-item-contract.js');
const Font = require('../src/js/adapters/native-font-contract.js');
const read = (file) => JSON.parse(fs.readFileSync(path.join(root, 'engineering/application', file), 'utf8'));
const schema = read('native-object-item-v1.schema.json');
const item = read('capabilities-v2/native-object-item.json');
const asset = read('capabilities-v2/native-object-asset.json');
const assetRemove = read('capabilities-v2/native-object-asset-remove.json');
const ITEM = Contract.ITEM, ATTACH = Contract.ATTACH, REMOVE = Contract.REMOVE;

const stable = () => ({ contextId: 'scene-root', frameScope: { kind: 'authored', frame: 0 }, layerUid: 'legacy layer:alpha', strokeId: 'native-raster:3:0' });
const created = () => Object.assign(stable(), { strokeId: '' });
const block = () => ({ text: 'Hello world', font: { kind: 'bundled', fontKey: 'Roboto-Regular' }, size: 17, color: '#112233', align: 'left', fixedWidth: null, fixedHeight: null,
  bold: false, italic: false, underline: false, strike: false, letterSpacing: 0, wordSpacing: 0, lineHeightMult: 1.25, textCase: 'none', anchorTopLeft: { x: 10, y: 20 } });
const raster = () => ({ assetId: 'asset-project-asset-1', x: 10.5, y: 20, width: 64, height: 48, opacity: 1, rotation: -12.5, isBitmapBrush: false, brushGroupId: 'g1', isBrushTextureCopy: false, meshId: 'm1' });
const COMMANDS = [
  ['raster.create', { command: 'object.raster.create', target: created(), item: raster() }, 'native-raster:8:0'],
  ['raster.move', { command: 'object.raster.move', stableTarget: stable(), x: 1, y: 2.5 }, 'native-raster:3:0'],
  ['raster.resize', { command: 'object.raster.resize', stableTarget: stable(), width: 3, height: 4 }, 'native-raster:3:0'],
  ['raster.rotate', { command: 'object.raster.rotate', stableTarget: stable(), rotation: 190 }, 'native-raster:3:0'],
  ['raster.opacity', { command: 'object.raster.opacity', stableTarget: stable(), opacity: 0.5 }, 'native-raster:3:0'],
  ['raster.delete', { command: 'object.raster.delete', stableTarget: stable() }, 'native-raster:3:0'],
  ['text.create', { command: 'object.text.create', target: created(), block: block() }, 'native-text:8:0'],
  ['text.edit', { command: 'object.text.edit', stableTarget: stable(), patch: { fixedWidth: null, text: 'data:text kept opaque' } }, 'native-raster:3:0'],
  ['text.delete', { command: 'object.text.delete', stableTarget: stable() }, 'native-raster:3:0'],
  ['text.animator.upsert', { command: 'object.text.animator.upsert', layerUid: 'layer', animator: {id: 'ta', selector: {}} }, 'ta'],
  ['text.animator.remove', { command: 'object.text.animator.remove', layerUid: 'layer', animatorId: 'ta' }, 'ta'],
  ['text.animator.edit', { command: 'object.text.animator.edit', layerUid: 'layer', animatorId: 'ta', edit: {kind: 'setEnabled', enabled: false} }, 'ta'],
];
const ATTACH_PAYLOAD = () => ({ documentSnapshotId: 'snapshot-8', name: 'photo', mediaType: 'image/png', source: { kind: 'file', path: '/Users/a/Pictures/photo.png' } });
const REMOVE_PAYLOAD = () => ({ documentSnapshotId: 'snapshot-8', assetId: 'asset-project-asset-1' });
const CASES = COMMANDS.map(([name, payload, stroke]) => ({ operation: ITEM, payload, result: { applied: true, historyEntriesAdded: 1, operation: name, strokeId: stroke } }))
  .concat([{ operation: ATTACH, payload: ATTACH_PAYLOAD(), result: { applied: true, historyEntriesAdded: 1 } }, { operation: REMOVE, payload: REMOVE_PAYLOAD(), result: { applied: true, historyEntriesAdded: 1 } }]);
const request = (c, rev = 7) => ({ apiVersion: 2, requestId: 'item-7', instanceId: 'instance-A', documentId: 'document-A', expectedRevision: rev, operation: c.operation, payload: structuredClone(c.payload) });
const reply = (q, result, rev) => ({ apiVersion: 2, requestId: q.requestId, instanceId: q.instanceId, documentId: q.documentId, contentRevision: rev === undefined ? q.expectedRevision + (result.applied ? 1 : 0) : rev, ok: true, result });

/* A deliberately small validator for the schema subset the declaration uses. */
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
  if (kind !== 'object') return true;
  const keys = Object.keys(value);
  if (def.minProperties && keys.length < def.minProperties) return false;
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
    assert.ok(valid(schema.$defs.Input, c.payload), 'schema accepts ' + c.payload.command);
    assert.doesNotThrow(() => Contract.payload(c.operation, c.payload), c.payload.command);
    assert.ok(valid(schema.$defs.Output, c.result), 'schema accepts result');
    assert.doesNotThrow(() => Contract.result(c.operation, c.result));
    for (const [label, bad] of flaws(c.payload)) {
      assert.equal(valid(schema.$defs.Input, bad), false, 'schema refuses ' + label);
      assert.throws(() => Contract.payload(c.operation, bad), TypeError, 'contract refuses ' + label);
    }
  }
  const nested = [
    ['create target names a stroke', (p) => { p.target.strokeId = 'native-raster:3:0'; }, COMMANDS[0][1]],
    ['edit target without a stroke', (p) => { p.stableTarget.strokeId = ''; }, COMMANDS[1][1]],
    ['unknown frame scope', (p) => { p.stableTarget.frameScope.kind = 'nope'; }, COMMANDS[1][1]],
    ['frame beyond u32', (p) => { p.stableTarget.frameScope.frame = 4294967296; }, COMMANDS[1][1]],
    ['string coordinate', (p) => { p.x = '1'; }, COMMANDS[1][1]],
    ['raster extra member', (p) => { p.item.isText = true; }, COMMANDS[0][1]],
    ['null opacity', (p) => { p.item.opacity = null; }, COMMANDS[0][1]],
    ['string rotation', (p) => { p.item.rotation = '1'; }, COMMANDS[0][1]],
    ['unknown font family', (p) => { p.block.font = { kind: 'bundled', fontKey: 'Comic-Regular' }; }, COMMANDS[6][1]],
    ['google font', (p) => { p.block.font = { kind: 'google', family: 'Roboto' }; }, COMMANDS[6][1]],
    ['bad color', (p) => { p.block.color = '#12'; }, COMMANDS[6][1]],
    ['three-digit color', (p) => { p.block.color = '#123'; }, COMMANDS[6][1]],
    ['blank layer uid', (p) => { p.stableTarget.layerUid = '  '; }, COMMANDS[1][1]],
    ['blank stroke id', (p) => { p.stableTarget.strokeId = '\t'; }, COMMANDS[1][1]],
    ['blank asset id of an item', (p) => { p.item.assetId = ' '; }, COMMANDS[0][1]],
    ['blank brush group', (p) => { p.item.brushGroupId = '\n'; }, COMMANDS[0][1]],
    ['blank font asset id', (p) => { p.patch.font = { kind: 'asset', assetId: ' ' }; }, COMMANDS[7][1]],
    ['blank attach name', (p) => { p.name = ' '; }, ATTACH_PAYLOAD()],
    ['blank remove id', (p) => { p.assetId = '   '; }, REMOVE_PAYLOAD()],
    ['bad align', (p) => { p.block.align = 'justify'; }, COMMANDS[6][1]],
    ['bad case', (p) => { p.block.textCase = 'Upper'; }, COMMANDS[6][1]],
    ['empty text', (p) => { p.block.text = ''; }, COMMANDS[6][1]],
    ['derived member', (p) => { p.block.textOverflow = false; }, COMMANDS[6][1]],
    ['patch with the anchor', (p) => { p.patch.anchorTopLeft = { x: 1, y: 2 }; }, COMMANDS[7][1]],
    ['empty patch', (p) => { p.patch = {}; }, COMMANDS[7][1]],
    ['attach bytes', (p) => { p.source.bytes = 'QUJD'; }, ATTACH_PAYLOAD()],
    ['attach url source', (p) => { p.source.kind = 'url'; }, ATTACH_PAYLOAD()],
    ['attach unsupported type', (p) => { p.mediaType = 'application/pdf'; }, ATTACH_PAYLOAD()],
    ['attach empty path', (p) => { p.source.path = ''; }, ATTACH_PAYLOAD()],
    ['attach bad snapshot', (p) => { p.documentSnapshotId = '-lead'; }, ATTACH_PAYLOAD()],
    ['remove empty id', (p) => { p.assetId = ''; }, REMOVE_PAYLOAD()],
  ];
  for (const [label, mutate, original] of nested) {
    const bad = structuredClone(original);
    mutate(bad);
    const operation = bad.command ? ITEM : bad.name ? ATTACH : REMOVE;
    assert.equal(valid(schema.$defs.Input, bad), false, 'schema refuses ' + label);
    assert.throws(() => Contract.payload(operation, bad), TypeError, 'contract refuses ' + label);
  }
  // The byte bound of an opaque string is 255 UTF-8 bytes (the schema counts characters and is looser by design); a host path is only non-empty.
  const edge = structuredClone(COMMANDS[1][1]);
  edge.stableTarget.layerUid = 'a'.repeat(255);
  assert.doesNotThrow(() => Contract.payload(ITEM, edge));
  edge.stableTarget.layerUid = 'a'.repeat(256);
  assert.throws(() => Contract.payload(ITEM, edge), TypeError, '256 bytes');
  edge.stableTarget.layerUid = '\u00e9'.repeat(128);
  assert.throws(() => Contract.payload(ITEM, edge), TypeError, '128 two-byte characters are 256 bytes');
  edge.stableTarget.layerUid = '\u00e9'.repeat(127);
  assert.doesNotThrow(() => Contract.payload(ITEM, edge));
  const spaced = ATTACH_PAYLOAD();
  spaced.source.path = ' ';
  assert.doesNotThrow(() => Contract.payload(ATTACH, spaced), 'the native validator does not trim a host path');
  const optional = structuredClone(COMMANDS[7][1]);
  optional.patch = { fixedWidth: null, fixedHeight: 3, font: { kind: 'asset', assetId: 'font-1' }, textCase: 'capitalize', color: '#11223344' };
  assert.ok(valid(schema.$defs.Input, optional));
  assert.doesNotThrow(() => Contract.payload(ITEM, optional));
});

test('the contract composes in front of the font contract without taking its operations', () => {
  const seen = [];
  const base = {
    admitted: (op) => op === 'x.base', requiresRevision: (op) => op === 'x.base',
    payload: (...a) => { seen.push(['payload', ...a]); }, result: (...a) => { seen.push(['result', ...a]); },
    correlate: (q) => { seen.push(['correlate', q.operation]); }, paths: (op) => { seen.push(['paths', op]); return null; },
  };
  const composed = Contract.compose(base);
  for (const op of [ITEM, ATTACH, REMOVE, 'x.base']) { assert.equal(composed.admitted(op), true, op); assert.equal(composed.requiresRevision(op), true, op); }
  assert.equal(composed.admitted('x.other'), false);
  for (const op of [ITEM, ATTACH, REMOVE]) { assert.equal(Contract.admitted(op), true, op); assert.equal(Contract.requiresRevision(op), true, op); }
  for (const op of ['query.document.object', 'command.document.object.asset.font.import', 'x.other']) { assert.equal(Contract.admitted(op), false, op); assert.equal(Contract.requiresRevision(op), false, op); }
  composed.payload(ITEM, COMMANDS[1][1], {});
  assert.deepEqual(seen, [], 'an owned operation never reaches the base');
  composed.payload('x.base', { a: 1 }, { env: 1 });
  composed.result('x.base', { r: 1 });
  composed.correlate({ operation: 'x.base' }, {});
  composed.paths('x.base');
  assert.deepEqual(seen, [['payload', 'x.base', { a: 1 }, { env: 1 }], ['result', 'x.base', { r: 1 }], ['correlate', 'x.base'], ['paths', 'x.base']]);
  assert.ok(composed.paths(ITEM).has('payload.block.text'));
  assert.ok(composed.paths(ATTACH).has('payload.source.path'));
  assert.equal(Contract.compose(null).admitted(ITEM), true);
  const real = Contract.compose(Font);
  assert.equal(real.admitted('command.document.object.asset.font.import'), true);
  assert.equal(real.admitted('query.document.object.asset.font'), true);
  assert.equal(real.requiresRevision('query.document.object.asset.font'), false, 'a font read still forbids a revision');
});

test('the SDK transports every owned operation once, validates it first and keeps the font and legacy operations', async () => {
  for (const c of CASES) {
    const q = request(c), sent = [];
    const adapter = SDK.createNativeApplicationAdapter('lane6', { dispatch(value) { sent.push(value); return reply(value, c.result); } });
    const response = await adapter.dispatch(q);
    assert.equal(sent.length, 1);
    assert.deepEqual(sent[0], q);
    assert.notEqual(sent[0], q, 'the transport receives a clone');
    assert.deepEqual(response, reply(q, c.result));
  }
  const bare = SDK.createNativeApplicationAdapter('lane6', { dispatch() { throw new Error('must not dispatch'); } });
  for (const c of CASES) {
    const q = request(c);
    delete q.expectedRevision;
    assert.throws(() => bare.dispatch(q), TypeError, c.payload.command || c.operation);
    assert.throws(() => bare.dispatch(Object.assign(request(c), { cancelledBeforeDispatch: 'x' })), TypeError);
    for (const [label, bad] of flaws(c.payload)) assert.throws(() => bare.dispatch(Object.assign(request(c), { payload: bad })), TypeError, label);
  }
  const font = { apiVersion: 2, requestId: 'font-1', instanceId: 'instance-A', documentId: 'document-A', expectedRevision: 7, operation: 'command.document.object.asset.font.import',
    payload: { command: 'object.asset.font.import', documentSnapshotId: 'snapshot-7', lifecycleGeneration: 1, source: { kind: 'bundled', fontKey: 'Roboto-Regular' } } };
  assert.doesNotThrow(() => SDK.validateRequest(font));
  assert.throws(() => SDK.validateRequest(Object.assign({}, font, { expectedRevision: undefined })), TypeError);
  assert.throws(() => SDK.validateRequest(Object.assign({}, font, { operation: 'command.document.object.asset.nope' })), TypeError);
  const policy = { apiVersion: 2, requestId: 'p-1', instanceId: 'instance-A', documentId: 'document-A', expectedRevision: 7, operation: 'command.document.object.layer.policy.edit',
    payload: { command: 'object.layer.policy.edit', layerUid: 'L0', flag: 'locked', value: true } };
  assert.doesNotThrow(() => SDK.validateRequest(policy));
});

test('an authored string that merely looks binary is allowed only where the declaration says it is authored text', () => {
  const text = structuredClone(COMMANDS[6][1]);
  text.block.text = 'data:text/plain,hello';
  assert.doesNotThrow(() => SDK.validateRequest(request({ operation: ITEM, payload: text })));
  const patch = structuredClone(COMMANDS[7][1]);
  patch.patch.text = 'blob:kept as authored text';
  assert.doesNotThrow(() => SDK.validateRequest(request({ operation: ITEM, payload: patch })));
  const font = structuredClone(COMMANDS[6][1]);
  font.block.font.fontKey = 'data:x';
  assert.throws(() => SDK.validateRequest(request({ operation: ITEM, payload: font })), TypeError);
  const color = structuredClone(COMMANDS[6][1]);
  color.block.color = 'data:text/css,#fff';
  assert.throws(() => SDK.validateRequest(request({ operation: ITEM, payload: color })), TypeError);
  const attach = ATTACH_PAYLOAD();
  attach.source.bytes = 'data:image/png;base64,QUJD';
  assert.throws(() => SDK.validateRequest(request({ operation: ATTACH, payload: attach })), TypeError);
  const large = ATTACH_PAYLOAD();
  large.source.path = '/' + 'x'.repeat(4096);
  assert.throws(() => SDK.validateRequest(request({ operation: ATTACH, payload: large })), TypeError, 'the 4096-byte envelope still binds');
});

test('the SDK refuses an uncorrelated receipt of every owned operation', async () => {
  for (const c of CASES) {
    const q = request(c), mutate = [
      (r) => { r.requestId = 'foreign'; }, (r) => { r.instanceId = 'foreign'; }, (r) => { r.documentId = 'foreign'; },
      (r) => { r.contentRevision = q.expectedRevision; }, (r) => { r.contentRevision += 1; },
      (r) => { r.result.historyEntriesAdded = 0; }, (r) => { r.result.applied = false; }, (r) => { r.result.assetId = 'asset-x'; },
      (r) => { r.result.padding = 'x'.repeat(5000); },
    ];
    if (c.operation === ITEM) mutate.push((r) => { r.result.operation = r.result.operation === 'raster.move' ? 'raster.resize' : 'raster.move'; },
      (r) => { r.result.strokeId = 'other:1:1'; }, (r) => { delete r.result.strokeId; },
      (r) => { r.result.strokeId = r.result.strokeId.startsWith('native-raster:') ? 'native-text:9:0' : 'native-raster:9:0'; });
    for (const flaw of mutate) {
      const response = reply(q, structuredClone(c.result));
      flaw(response);
      const adapter = SDK.createNativeApplicationAdapter('lane6', { dispatch() { return response; } });
      await assert.rejects(adapter.dispatch(q), `${c.operation} ${String(flaw)}`);
    }
  }
});

test('a no-op receipt keeps the revision and a refusal is a plain typed error', async () => {
  const c = CASES[1], q = request(c);
  const noop = reply(q, { applied: false, historyEntriesAdded: 0, operation: 'raster.move', strokeId: 'native-raster:3:0' });
  assert.equal(noop.contentRevision, 7);
  const ok = SDK.createNativeApplicationAdapter('lane6', { dispatch() { return noop; } });
  assert.deepEqual(await ok.dispatch(q), noop);
  const refused = { apiVersion: 2, requestId: q.requestId, instanceId: q.instanceId, documentId: q.documentId, contentRevision: 7, ok: false, error: { code: 'unavailable', message: 'Native item command refused.', details: { reason: 'layer_locked' } } };
  const adapter = SDK.createNativeApplicationAdapter('lane6', { dispatch() { return refused; } });
  assert.equal((await adapter.dispatch(q)).error.code, 'unavailable');
});

test('descriptors: item, bounded PNG attach and separate asset removal are available with no fallback', () => {
  assert.deepEqual([item.id, item.operations, item.availability], ['native.object.item', [ITEM], { state: 'available', reason: null }]);
  assert.deepEqual([asset.id, asset.operations, asset.availability], ['native.object.asset', [ATTACH], { state: 'available', reason: null }]);
  assert.deepEqual([assetRemove.id, assetRemove.operations, assetRemove.availability], ['native.object.asset.remove', [REMOVE],
    { state: 'available', reason: null }]);
  // Common catalog shape compatibility does not admit host execution before source I/O.
  assert.match(asset.semantics, /Catalog-only attach retains compatibility validation but refuses unavailable before source I\/O/);
  assert.match(assetRemove.semantics, /Remove an unreferenced embedded image through the existing native asset transaction\/history owner/);
  for (const d of [item, asset, assetRemove]) {
    assert.deepEqual([d.schemaVersion, d.apiVersion, d.version, d.status], [2, 2, 1, 'active']);
    assert.deepEqual(d.authority, { document: 'native-engine::history::NativeObjectHistory', adapterOwnsWritableMirror: false, fallback: 'none' });
    assert.equal(d.resourceBoundary.maxRequestBytes, 4096);
    assert.ok(d.commonErrorCodes.includes('unavailable') && d.commonErrorCodes.includes('stale_revision'));
    // Descriptor examples are validated against the whole advertised union by the MCP crate tests, as for the font descriptor.
    assert.deepEqual(d.examples, [], d.id);
  }
  assert.equal(schema.examples.length, 2);
  for (const e of schema.examples) {
    assert.doesNotThrow(() => SDK.validateRequest(e.request), e.label);
    assert.doesNotThrow(() => SDK.validateResponse(e.response, e.request.requestId, e.request.operation), e.label);
    assert.ok(valid(schema.$defs.Input, e.request.payload), e.label);
    assert.ok(valid(schema.$defs.Output, e.response.result), e.label);
    assert.doesNotThrow(() => Contract.correlate(e.request, e.response), e.label);
  }
  assert.deepEqual(item.input, { $ref: '../native-object-item-v1.schema.json#/$defs/ItemEdit' });
  assert.deepEqual(asset.input, { $ref: '../native-object-item-v1.schema.json#/$defs/ImageImportAttach' });
  assert.deepEqual(assetRemove.input, { $ref: '../native-object-item-v1.schema.json#/$defs/AssetRemove' });
  assert.deepEqual([item.output.$ref, asset.output.$ref, assetRemove.output.$ref], ['../native-object-item-v1.schema.json#/$defs/ItemEditResult',
    '../native-object-item-v1.schema.json#/$defs/ImageImportResult', '../native-object-item-v1.schema.json#/$defs/AssetCommandResult']);
});

test('the v2 request union declares each operation exactly once and the receipt union stays exclusive', () => {
  const transport = read('native-transport-v2.schema.json');
  const declared = [item, asset, assetRemove].flatMap((d) => d.operations);
  const branches = transport.$defs.Request.oneOf;
  for (const [operation, ref] of [[ITEM, 'ItemEdit'], [ATTACH, 'AssetAttach'], [REMOVE, 'AssetRemove']]) {
    const found = branches.filter((b) => b.allOf[1].properties.operation.const === operation);
    assert.equal(found.length, 1, operation);
    assert.equal(found[0].allOf[0].$ref, '#/$defs/RequiredRevisionRequest', operation);
    assert.equal(found[0].allOf[1].properties.payload.$ref, 'native-object-item-v1.schema.json#/$defs/' + ref, operation);
  }
  assert.equal(new Set(declared).size, declared.length);
  const results = transport.$defs.ResponseBase.properties.result.oneOf.map((entry) => entry.$ref);
  assert.equal(results.filter((ref) => ref === 'native-object-item-v1.schema.json#/$defs/ItemEditResult').length, 1);
  assert.equal(results.filter((ref) => /AssetCommandResult/.test(ref)).length, 0, 'the asset receipt is CommandResult; its twin would make oneOf ambiguous');
  assert.equal(results.filter((ref) => ref === '#/$defs/CommandResult').length, 1);
  assert.equal(results.filter((ref) => ref === 'native-object-item-v1.schema.json#/$defs/ImageImportResult').length, 1);
  // The asset receipt really is the existing CommandResult shape, and the item receipt really is not.
  const command = transport.$defs.CommandResult;
  assert.deepEqual(Object.keys(command.properties), Object.keys(schema.$defs.AssetCommandResult.properties));
  assert.deepEqual(command.required, schema.$defs.AssetCommandResult.required);
  assert.equal(valid(command, { applied: true, historyEntriesAdded: 1, operation: 'raster.move', strokeId: 'native-raster:3:0' }), false);
  assert.equal(transport.$id.replace(/[^/]*$/, ''), schema.$id.replace(/[^/]*$/, ''), 'the relative reference resolves to the schema $id');
});

test('schema: every object is closed and the enumerations are the contract tables', () => {
  const objects = [];
  (function walk(node, at) {
    if (Array.isArray(node)) return node.forEach((v, i) => walk(v, at + '/' + i));
    if (!node || typeof node !== 'object') return;
    if (node.type === 'object') objects.push([at, node]);
    Object.keys(node).forEach((k) => walk(node[k], at + '/' + k));
  }(schema.$defs, '$defs'));
  assert.ok(objects.length >= 20);
  for (const [at, node] of objects) assert.equal(node.additionalProperties, false, at);
  const d = schema.$defs;
  assert.deepEqual(d.ItemEdit.oneOf.map((v) => v.properties.command.const), COMMANDS.map(([, p]) => p.command));
  assert.deepEqual(d.Font.oneOf[0].properties.fontKey.enum, Contract.FONTS);
  assert.deepEqual(d.AssetAttach.oneOf.map((v) => v.$ref), ['#/$defs/AssetCatalogAttach', '#/$defs/ImageImportAttach']);
  assert.deepEqual(d.AssetCatalogAttach.properties.mediaType.enum, Contract.MEDIA);
  assert.deepEqual(d.ImageImportAttach.properties.mediaType, { const: 'image/png' });
  assert.deepEqual(d.ImageImportAttach.required, ['documentSnapshotId', 'name', 'mediaType', 'source', 'placement', 'lifecycleGeneration']);
  assert.deepEqual(d.ImageImportAttach.properties.source, d.AssetCatalogAttach.properties.source);
  assert.deepEqual(d.ImageImportAttach.properties.lifecycleGeneration, { type: 'integer', minimum: 1, maximum: 9007199254740991 });
  const placements = d.ImageImportAttach.properties.placement.oneOf;
  assert.deepEqual(placements.map((v) => v.properties.kind.const), ['newLayer', 'authoredFrame']);
  assert.deepEqual(placements.map((v) => v.required), [['kind'], ['kind', 'layerUid', 'frame']]);
  assert.deepEqual(placements[1].properties.layerUid, { $ref: '#/$defs/Opaque' });
  assert.deepEqual(placements[1].properties.frame, { type: 'integer', minimum: 0, maximum: 4294967295 });
  // Retain catalog-only schema admission; the descriptor/host still refuses its execution.
  assert.ok(valid(d.AssetCatalogAttach, ATTACH_PAYLOAD()));
  assert.equal(valid(d.ImageImportAttach, ATTACH_PAYLOAD()), false);
  for (const placement of [{ kind: 'newLayer' }, { kind: 'authoredFrame', layerUid: 'legacy layer:alpha', frame: 3 }]) {
    const png = Object.assign(ATTACH_PAYLOAD(), { placement, lifecycleGeneration: 1 });
    assert.ok(valid(d.ImageImportAttach, png));
    assert.ok(valid(d.Input, png));
    assert.doesNotThrow(() => Contract.payload(ATTACH, png));
    for (const [label, bad] of flaws(png)) {
      assert.equal(valid(d.Input, bad), false, 'PNG schema refuses ' + label);
      assert.throws(() => Contract.payload(ATTACH, bad), TypeError, 'PNG contract refuses ' + label);
    }
    for (const mutate of [
      (p) => { p.mediaType = 'image/jpeg'; }, (p) => { p.mediaType = 'video/mp4'; },
      (p) => { p.source.kind = 'url'; }, (p) => { p.source.bytes = 'QUJD'; },
      (p) => { p.placement.extra = true; }, (p) => { p.placement.kind = 'reference'; },
      (p) => { p.lifecycleGeneration = 0; }, (p) => { p.lifecycleGeneration = 9007199254740992; },
    ]) {
      const bad = structuredClone(png); mutate(bad);
      assert.equal(valid(d.Input, bad), false, 'PNG schema refuses nested/type/lifecycle flaw');
      assert.throws(() => Contract.payload(ATTACH, bad), TypeError);
    }
  }
  assert.deepEqual(d.ImageImportResult.properties.applied, { const: true });
  assert.deepEqual(d.ImageImportResult.properties.historyEntriesAdded, { const: 1 });
  assert.deepEqual(d.ImageImportResult.properties.target, { $ref: '#/$defs/ImageImportTarget' });
  assert.deepEqual(d.ImageImportTarget.properties.contextId, { const: 'scene-root' });
  assert.deepEqual(d.ImageImportTarget.properties.frameScope.properties.kind, { const: 'authored' });
  assert.deepEqual(d.ImageImportTarget.properties.frameScope.properties.frame, { type: 'integer', minimum: 0, maximum: 4294967295 });
  assert.deepEqual(d.ItemEditResult.properties.operation.enum, COMMANDS.map(([name]) => name));
  assert.equal(d.CreateTarget.properties.strokeId.const, '');
  assert.equal(d.TextPatch.minProperties, 1);
  assert.equal('anchorTopLeft' in d.TextPatch.properties, false);
  assert.equal('anchorTopLeft' in d.TextBlock.properties, true);
  assert.deepEqual(d.Output.oneOf.map((v) => v.$ref), ['#/$defs/ItemEditResult', '#/$defs/AssetCommandResult', '#/$defs/ImageImportResult']);
});
