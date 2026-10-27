'use strict';
// L02-J public declaration of the 20 native object edit operations: contract, common SDK, capability files and schema agree.
const test = require('node:test'), assert = require('node:assert/strict'), fs = require('node:fs'), path = require('node:path');
const contract = require('../src/js/adapters/native-object-edit-contract.js');
const seam = require('../src/js/adapters/native-object-edit-seam.js');
const SDK = require('../src/js/adapters/native-application.js');

const ROOT = path.resolve(__dirname, '..');
const P = 'command.document.object.';
const sel = { active: 'layer-b', selected: ['layer-b'], anchor: 'layer-b' };
const at = { layerUid: 'layer-a', contextId: 'scene-root' };
const cell = { contextId: 'scene-root', layerUid: 'layer-a', frame: 0 };
// [suffix, literal payload]. Independent of the contract table: command strings and members are spelled out here.
const VALID = [
  ['subselect.edit', {command:'object.subselect.edit',stableTarget:{contextId:'scene-root',layerUid:'layer-a',frameScope:{kind:'authored',frame:0},strokeId:'stroke-a'},intent:{kind:'move',indices:[0],delta:[5,2]}}],
  ['layer.delete', { command: 'object.layer.delete', selection: sel }],
  ['layer.duplicate', { command: 'object.layer.duplicate', layerUid: 'layer-b' }],
  ['layer.rename', { command: 'object.layer.rename', layerUid: 'layer-b', name: 'Ink' }],
  ['layer.reorder', { command: 'object.layer.reorder', layerUids: ['layer-c'], gap: 1.5, selection: { active: 'layer-c', selected: ['layer-c'] } }],
  ['group.create', { command: 'object.group.create', ...at, groupId: 'group-1', name: 'Head', strokeIds: ['stroke-1', 'stroke-2'] }],
  ['group.ungroup', { command: 'object.group.ungroup', ...at, strokeIds: ['stroke-1'] }],
  ['group.combine', { command: 'object.group.combine', ...at, groupId: 'group-1', mode: 'exclude', strokeIds: ['stroke-1', 'stroke-2'] }],
  ['group.mode.set', { command: 'object.group.mode.set', ...at, groupId: 'group-1', mode: 'none' }],
  ['group.rename', { command: 'object.group.rename', ...at, groupId: 'group-1', name: 'Face' }],
  ['group.member.remove', { command: 'object.group.member.remove', ...at, strokeId: 'stroke-1' }],
  ['selection.delete', { command: 'object.selection.delete', ...cell, mode: 'animation2d', strokeIds: [] }],
  ['selection.duplicate', { command: 'object.selection.duplicate', ...cell, mode: 'motion', strokeIds: ['stroke-1'] }],
  ['layer.split', { command: 'object.layer.split', ...cell, labels: { shape: 'Shape', stroke: 'Stroke' } }],
  ['clipboard.paste', { command: 'object.clipboard.paste', ...cell, mode: 'motion', clip: { contextId: 'scene-root', layerUid: 'layer-a', frame: 0, records: [] } }],
  ['boolean.apply', { command: 'object.boolean.apply', mode: 'intersect', ...at, frame: 0, strokeIds: ['stroke-1', 'stroke-2'] }],
  ['boolean.erase', { command: 'object.boolean.erase', ...at, frame: 0, strokeId: 'stroke-1', footprint: [[0, 0], [4, 0], [4, 4]] }],
  ['boolean.flatten', { command: 'object.boolean.flatten', ...at, frame: 0, groupId: 'group-1' }],
  ['fill.trace', {command:'object.fill.trace',...cell,point:[20,20],color:'#336699ff',opacity:0.5,remove:false}],
  ['fill.brush', {command:'object.fill.brush',...cell,outline:[[10,10],[60,10],[60,40]].map((p)=>({point:p,handleIn:[0,0],handleOut:[0,0]})),color:'#336699ff',opacity:0.5,placement:'below',mask:false}],
  ['clipboard.copy', { command: 'object.clipboard.copy', ...cell, mode: 'animation2d', strokeIds: ['stroke-1'] }],
  ['clipboard.cut', { command: 'object.clipboard.cut', ...cell, mode: 'motion', strokeIds: [] }],
];
const envelope = (suffix, payload, extra) => Object.assign({ apiVersion: 2, requestId: 'edit-1', instanceId: 'instance-A', documentId: 'document-A',
  operation: P + suffix, expectedRevision: 7, payload }, extra);

test('the contract admits exactly the 22 declared operations, all revisioned commands', () => {
  const edit = contract;
  assert.deepEqual(edit.OPERATIONS, VALID.map(([s]) => P + s));
  assert.equal(edit.OPERATIONS.length, 22);
  for (const op of edit.OPERATIONS) { assert.equal(edit.admitted(op), true); assert.equal(edit.requiresRevision(op), true); assert.equal(seam.admitted(op), true); }
  for (const op of ['command.document.object.layer.create', 'command.document.object.layer.policy.edit', 'command.document.object.fill.unknown', 'query.document.object', P + 'group', P + 'toString', '', null, undefined]) {
    assert.equal(edit.admitted(op), false, String(op));
  }
});

test('every literal payload is accepted and every one-member mutation is refused', () => {
  for (const [suffix, payload] of VALID) {
    contract.payload(P + suffix, payload);
    for (const key of Object.keys(payload)) {
      const missing = { ...payload }; delete missing[key];
      assert.throws(() => contract.payload(P + suffix, missing), TypeError, suffix + ' without ' + key);
      assert.throws(() => contract.payload(P + suffix, { ...payload, [key]: null }), TypeError, suffix + ' null ' + key);
    }
    assert.throws(() => contract.payload(P + suffix, { ...payload, extra: 1 }), TypeError, suffix + ' extra');
    assert.throws(() => contract.payload(P + suffix, Object.keys(payload).map((k) => payload[k])), TypeError, suffix + ' positional');
    assert.throws(() => contract.payload(P + suffix, { ...payload, command: 'object.layer.policy.edit' }), TypeError, suffix + ' foreign command');
  }
  assert.throws(() => contract.payload(P + 'layer.delete', VALID.find(([s])=>s==='layer.duplicate')[1]), TypeError, 'a payload of another operation');
});

test('bounds mirror the owner: ids 128 bytes, names 256, labels 64, selections 1024, footprint 3..1024, frame u32', () => {
  const base = (suffix) => VALID.find(([s]) => s === suffix)[1];
  const ok = (suffix, patch) => contract.payload(P + suffix, { ...base(suffix), ...patch });
  const no = (suffix, patch) => assert.throws(() => contract.payload(P + suffix, { ...base(suffix), ...patch }), TypeError, suffix + JSON.stringify(patch).slice(0, 60));
  ok('group.create', { groupId: 'g'.repeat(128) }); no('group.create', { groupId: 'g'.repeat(129) });
  ok('group.create', { groupId: 'é'.repeat(64) }); no('group.create', { groupId: 'é'.repeat(65) });
  ok('group.rename', { name: 'n'.repeat(256) }); no('group.rename', { name: 'n'.repeat(257) }); no('group.rename', { name: '' }); no('group.rename', { name: 'a\u0085b' });
  ok('layer.split', { labels: { shape: 's'.repeat(64), stroke: 't' } }); no('layer.split', { labels: { shape: 's'.repeat(65), stroke: 't' } });
  ok('selection.delete', { strokeIds: Array.from({ length: 1024 }, (_, i) => 's' + i) });
  no('selection.delete', { strokeIds: Array.from({ length: 1025 }, (_, i) => 's' + i) });
  ok('boolean.apply', { frame: 4294967295 }); no('boolean.apply', { frame: 4294967296 }); no('boolean.apply', { frame: -1 }); no('boolean.apply', { frame: 0.5 }); no('boolean.apply', { frame: '0' });
  no('boolean.apply', { strokeIds: ['stroke-1'] }); no('boolean.apply', { mode: 'none' });
  no('boolean.erase', { footprint: [[0, 0], [1, 1]] }); no('boolean.erase', { footprint: [[0, 0], [1, 1], [NaN, 2]] });
  ok('layer.rename', { name: '' }); ok('layer.rename', { layerUid: 'data:opaque authored layer' }); no('layer.rename', { layerUid: '' });
  no('layer.reorder', { layerUids: ['layer-c', 'layer-c'] }); no('layer.reorder', { gap: Infinity });
  // selection.anchor: absent, null and a string are three distinct accepted states; a non-string anchor is refused.
  const withoutAnchor = { active: 'layer-b', selected: ['layer-b'] };
  for (const suffix of ['layer.delete', 'layer.reorder']) {
    const selection = suffix === 'layer.delete' ? sel : base(suffix).selection;
    ok(suffix, { selection: { ...selection, anchor: null } });
    ok(suffix, { selection: { ...selection, anchor: 'layer-b' } });
    ok(suffix, { selection: { ...withoutAnchor, active: selection.active, selected: selection.selected } });
    no(suffix, { selection: { ...selection, anchor: 5 } });
  }
  ok('group.combine', { mode: 'none' }); ok('group.combine', { mode: 'subtract' }); no('group.combine', { mode: 'merge' });
});

test('results: command receipts correlate applied and history count; a duplicate must name its minted layer', () => {
  for (const [suffix] of VALID.filter(([s]) => !['layer.duplicate', 'clipboard.copy', 'clipboard.cut'].includes(s))) {
    contract.result(P + suffix, { applied: true, historyEntriesAdded: 1 });
    contract.result(P + suffix, { applied: false, historyEntriesAdded: 0 });
    for (const bad of [{ applied: true, historyEntriesAdded: 0 }, { applied: false, historyEntriesAdded: 1 }, { applied: true, historyEntriesAdded: 1, layerUid: 'x' },
      { applied: 1, historyEntriesAdded: 1 }, { applied: true }, null, []]) assert.throws(() => contract.result(P + suffix, bad), TypeError, suffix + JSON.stringify(bad));
  }
  contract.result(P + 'layer.duplicate', { applied: true, historyEntriesAdded: 1, layerUid: 'native-layer:8:0' });
  for (const bad of [{ applied: true, historyEntriesAdded: 1 }, { applied: false, historyEntriesAdded: 0, layerUid: 'x' }, { applied: true, historyEntriesAdded: 1, layerUid: '' }]) {
    assert.throws(() => contract.result(P + 'layer.duplicate', bad), TypeError);
  }
});

test('the common SDK admits each operation, requires expectedRevision, accepts opaque data: identities, and refuses binary payloads', () => {
  for (const [suffix, payload] of VALID) {
    SDK.validateRequest(envelope(suffix, payload));
    SDK.validateRequest(envelope(suffix, payload, { cancelledBeforeDispatch: true }));
    const without = envelope(suffix, payload); delete without.expectedRevision;
    assert.throws(() => SDK.validateRequest(without), /expectedRevision presence/, suffix);
    assert.throws(() => SDK.validateRequest(envelope(suffix, payload, { expectedRevision: -1 })), TypeError, suffix);
    assert.throws(() => SDK.validateRequest(envelope(suffix, { ...payload, extra: true })), TypeError, suffix);
  }
  SDK.validateRequest(envelope('layer.rename', { command: 'object.layer.rename', layerUid: 'data:opaque authored layer', name: 'blob:not a resource' }));
  const clip = VALID[13][1].clip;
  assert.throws(() => SDK.validateRequest(envelope('clipboard.paste', { ...VALID[13][1], clip: { ...clip, records: [new Uint8Array(4)] } })), TypeError);
  assert.throws(() => SDK.validateRequest(envelope('layer.split', { ...VALID[12][1] }, { operation: P + 'layer.splits' })), /not admitted/);
});

test('the SDK correlates each receipt: identity, document replacement and the exact revision advance', async () => {
  for (const [suffix, payload] of VALID) {
    const q = envelope(suffix, payload);
    const clipboard = { contextId: 'scene-root', layerUid: 'layer-a', frame: 0, records: [] };
    const result = (applied) => suffix === 'clipboard.copy' ? { applied: false, historyEntriesAdded: 0, clipboard }
      : suffix === 'clipboard.cut' ? { applied, historyEntriesAdded: applied ? 1 : 0, clipboard: applied ? clipboard : null }
      : suffix === 'layer.duplicate' ? { applied: true, historyEntriesAdded: 1, layerUid: 'native-layer:8:0' } : { applied, historyEntriesAdded: applied ? 1 : 0 };
    const reply = (applied, mutate) => () => { const r = { apiVersion: 2, requestId: 'edit-1', instanceId: 'instance-A', documentId: 'document-A',
      contentRevision: 7 + (applied && suffix !== 'clipboard.copy' ? 1 : 0), ok: true, result: result(applied) }; if (mutate) mutate(r); return r; };
    const send = (dispatch) => SDK.createNativeApplicationAdapter('c', { dispatch }).dispatch(q);
    assert.equal((await send(reply(true))).contentRevision, suffix === 'clipboard.copy' ? 7 : 8);
    if (suffix !== 'layer.duplicate') assert.equal((await send(reply(false))).contentRevision, 7);
    for (const mutate of [(r) => { r.contentRevision = suffix === 'layer.duplicate' ? 7 : 9; }, (r) => { r.instanceId = 'foreign'; }, (r) => { r.documentId = 'foreign'; }, (r) => { r.requestId = 'foreign'; }]) {
      await assert.rejects(send(reply(true, mutate)), TypeError, suffix);
    }
    if (!['layer.duplicate', 'clipboard.copy'].includes(suffix)) await assert.rejects(send(reply(true, (r) => { r.contentRevision = 7; })), /revision mismatch/);
    const replaced = await send(() => ({ apiVersion: 2, requestId: 'edit-1', instanceId: 'instance-A', documentId: 'document-B', contentRevision: 3, ok: false, error: { code: 'wrong_document', message: 'replaced' } }));
    assert.equal(replaced.error.code, 'wrong_document');
    await assert.rejects(send(() => ({ apiVersion: 2, requestId: 'edit-1', instanceId: 'instance-A', documentId: 'document-B', contentRevision: 3, ok: false, error: { code: 'not_found', message: 'x' } })), /document mismatch/);
  }
});

test('the injected seam is the font contract composed with the edit contract and declares nothing else', () => {
  assert.equal(seam.admitted('command.document.object.asset.font.import'), true);
  assert.equal(contract.admitted('command.document.object.asset.font.import'), false);
  assert.equal(seam.admitted(P + 'group.create'), true);
  assert.equal(seam.admitted('command.document.object.layer.policy.edit'), false, 'the F06 policy command stays a plain SDK operation');
  assert.equal(seam.requiresRevision('command.document.object.asset.font.import'), true);
  assert.equal(seam.requiresRevision('query.document.object.asset.font'), false);
  assert.equal(seam.requiresRevision(P + 'group.create'), true);
  assert.equal(seam.paths('query.document.object'), null);
  assert.equal(seam.paths(P + 'group.create').has('payload.strokeIds.3'), true);
  assert.equal(seam.paths(P + 'group.create').has('result.something'), false);
  assert.equal(seam.paths('command.document.object.asset.font.import').has('payload.source.family'), true);
  assert.equal(contract.compose(null).admitted(P + 'group.create'), true, 'without a font contract only the edit operations are declared');
  assert.equal(contract.compose(null).admitted('command.document.object.asset.font.import'), false);
  // The SDK is wired to this seam only: font operations validate exactly as before.
  const fontRequest = { apiVersion: 2, requestId: 'font-1', instanceId: 'instance-A', documentId: 'document-A', expectedRevision: 7,
    operation: 'command.document.object.asset.font.import',
    payload: { command: 'object.asset.font.import', documentSnapshotId: 'native-object:document-A:7', lifecycleGeneration: 1, source: { kind: 'bundled', fontKey: 'Roboto-Regular' } } };
  SDK.validateRequest(fontRequest);
  assert.throws(() => SDK.validateRequest({ ...fontRequest, payload: { ...fontRequest.payload, source: { kind: 'bundled', fontKey: 'Comic' } } }), TypeError);
});

test('capability descriptors and the closed transport schema declare exactly the contract operations', () => {
  const dir = path.join(ROOT, 'engineering/application/capabilities-v2');
  const files = ['layer-lifecycle', 'group', 'selection-edit', 'boolean', 'fill', 'subselect'].map((k) => path.join(dir, 'native-object-' + k + '.json'));
  const schema = JSON.parse(fs.readFileSync(path.join(ROOT, 'engineering/application/native-transport-v2.schema.json'), 'utf8'));
  const declared = [];
  const ids = new Set();
  for (const file of files) {
    const d = JSON.parse(fs.readFileSync(file, 'utf8'));
    assert.equal(d.schemaVersion, 2); assert.equal(d.apiVersion, 2); assert.deepEqual(d.availability, { state: 'available', reason: null });
    assert.equal(d.authority.adapterOwnsWritableMirror, false); assert.equal(d.authority.fallback, 'none'); assert.equal(d.resourceBoundary.maxRequestBytes, d.id === 'native.object.fill' ? 65536 : 4096, d.id);
    assert.equal(ids.has(d.id), false); ids.add(d.id);
    assert.equal(d.input.oneOf.length, d.operations.length);
    for (const example of d.examples) {
      SDK.validateRequest(example.request);
      SDK.validateResponse(example.response, example.request.requestId, example.request.operation);
      assert.equal(d.operations.includes(example.request.operation), true);
      assert.equal(example.response.contentRevision, example.request.expectedRevision + (example.response.result.applied ? 1 : 0));
    }
    assert.deepEqual(d.examples.map((e) => e.request.operation), d.operations, 'one copyable example per operation, in declaration order');
    declared.push(...d.operations);
  }
  assert.deepEqual(declared.slice().sort(), contract.OPERATIONS.slice().sort());
  assert.equal(new Set(declared).size, 22);
  assert.deepEqual([...ids].sort(), ['native.object.boolean', 'native.object.fill', 'native.object.group', 'native.object.layer-lifecycle', 'native.object.selection-edit', 'native.object.subselect']);
  for (const [suffix, payload] of VALID) {
    const branches = schema.$defs.Request.oneOf.filter((b) => b.allOf[1].properties.operation.const === P + suffix);
    assert.equal(branches.length, 1, suffix);
    assert.equal(branches[0].allOf[0].$ref, '#/$defs/RequiredRevisionRequest');
    const def = schema.$defs[branches[0].allOf[1].properties.payload.$ref.replace('#/$defs/', '')];
    assert.equal(def.additionalProperties, false);
    assert.deepEqual(def.required.slice().sort(), Object.keys(payload).sort(), suffix);
    assert.equal(def.properties.command.const, payload.command);
  }
  assert.equal(schema.$defs.Request.oneOf.filter((b) => b.allOf[1].properties.operation.const === 'command.document.object.layer.policy.edit').length, 1, 'F06 branch is untouched');
});

test('copy/cut have closed clipboard receipts, copy never changes history and empty cut is null', () => {
  const clipboard = { contextId: 'scene-root', layerUid: 'layer-a', frame: 0, records: [] };
  for (const suffix of ['clipboard.copy', 'clipboard.cut']) {
    const op = P + suffix, applied = suffix === 'clipboard.cut';
    const good = { applied, historyEntriesAdded: applied ? 1 : 0, clipboard };
    contract.result(op, good);
    contract.result(op, { applied: false, historyEntriesAdded: 0, clipboard: null });
    for (const key of Object.keys(good)) {
      const missing = { ...good }; delete missing[key];
      assert.throws(() => contract.result(op, missing), TypeError, suffix + ' missing ' + key);
    }
    for (const bad of [{ ...good, extra: true }, { ...good, clipboard: [] }, { ...good, clipboard: {} },
      { ...good, clipboard: { ...clipboard, extra: true } }, { ...good, clipboard: { ...clipboard, records: [{ schemaVersion: 1 }] } },
      { ...good, applied: 1 }, { ...good, historyEntriesAdded: 2 }]) assert.throws(() => contract.result(op, bad), TypeError);
  }
  assert.throws(() => contract.result(P + 'clipboard.copy', { applied: true, historyEntriesAdded: 1, clipboard }), /copy/);
  assert.throws(() => contract.result(P + 'clipboard.cut', { applied: true, historyEntriesAdded: 1, clipboard: null }), /cut/);
  assert.throws(() => contract.result(P + 'clipboard.cut', { applied: false, historyEntriesAdded: 0, clipboard }), /cut/);
});

test('copy/cut full responses admit authored data/blob text, reject binary/shape/owner tampering and stay4096 bytes', async () => {
  const record = JSON.parse(fs.readFileSync(path.join(ROOT, 'engineering/application/examples/native-object-v1/cases.json'), 'utf8')).records[0];
  const clipboard = { contextId: 'scene-root', layerUid: 'layer-a', frame: 0, records: [{ ...record,
    target: { ...record.target, strokeId: 'data:authored-stroke' } }] };
  for (const suffix of ['clipboard.copy', 'clipboard.cut']) {
    const applied = suffix === 'clipboard.cut';
    const q = envelope(suffix, { command: 'object.' + suffix, ...cell, mode: 'motion', strokeIds: ['stroke-1'] });
    const reply = () => ({ apiVersion: 2, requestId: q.requestId, instanceId: q.instanceId, documentId: q.documentId,
      contentRevision: 7 + (applied ? 1 : 0), ok: true, result: { applied, historyEntriesAdded: applied ? 1 : 0, clipboard } });
    const send = (mutate) => SDK.createNativeApplicationAdapter('clip-test', { dispatch: () => {
      const r = JSON.parse(JSON.stringify(reply())); if (mutate) mutate(r); return r;
    } }).dispatch(q);
    assert.deepEqual((await send()).result.clipboard, clipboard);
    await send((r) => { r.result.clipboard.records[0].target.strokeId = 'blob:authored-stroke'; });
    for (const mutate of [(r) => { r.result.clipboard.contextId = 'foreign'; }, (r) => { r.result.clipboard.layerUid = 'foreign'; },
      (r) => { r.result.clipboard.frame += 1; }, (r) => { r.result.clipboard.records[0].target.frameScope = ['authored', 0]; },
      (r) => { r.result.clipboard.records[0].geometry = new Uint8Array(4); },
      (r) => { r.result.clipboard.records[0].extra = true; },
      (r) => { r.result.clipboard.records[0].target.strokeId = 'x'.repeat(4096); },
      (r) => { r.requestId = 'other'; }, (r) => { r.instanceId = 'other'; }, (r) => { r.documentId = 'other'; },
      (r) => { r.contentRevision += 1; }]) await assert.rejects(send(mutate), TypeError, suffix);
    assert.equal(contract.paths(P + suffix).has('result.clipboard.records.0.target.strokeId'), true);
    assert.equal(contract.paths(P + 'selection.delete').has('result.clipboard.records.0.target.strokeId'), false);
    assert.equal(contract.paths(P + suffix).has('result.unrelated'), false);
  }
});


test('authored effect presence survives clipboard output and paste input admission', () => {
  const original = JSON.parse(fs.readFileSync(path.join(ROOT, 'engineering/application/examples/native-object-v1/cases.json'), 'utf8')).records[0];
  for (const effects of [undefined, null, [], [{ type: 'blur', enabled: false, p1: null, p2: 0, keys: null }]]) {
    const record = JSON.parse(JSON.stringify(original));
    if (effects !== undefined) record.effects = effects;
    const clipboard = { ...cell, records: [record] };
    for (const suffix of ['clipboard.copy', 'clipboard.cut']) {
      const applied = suffix === 'clipboard.cut';
      contract.result(P + suffix, { applied, historyEntriesAdded: applied ? 1 : 0, clipboard });
    }
    SDK.validateRequest(envelope('clipboard.paste', { command: 'object.clipboard.paste', ...cell, mode: 'motion', clip: clipboard }));
    assert.deepEqual(record.effects, effects);
  }
});


test('asset-bearing clipboard packets require bounded closed provenance in both directions', () => {
  const packet = { ...cell, records: [], rasters: [{
    target: { contextId: 'scene-root', layerUid: 'layer-a', strokeId: 'ink', frameScope: { kind: 'authored', frame: 0 } },
    item: { assetId: 'image-1', x: 16, y: 16, width: 8, height: 8, isBitmapBrush: true, brushGroupId: 'brush-1' },
  }], source: { instanceId: 'instance-A', documentId: 'document-A', contentRevision: 7, strokeIds: ['anchor'] } };
  const request = (clip) => envelope('clipboard.paste', { command: 'object.clipboard.paste', ...cell, mode: 'motion', clip });
  const result = (clip) => ({ applied: false, historyEntriesAdded: 0, clipboard: clip });
  SDK.validateRequest(request(packet));
  contract.result(P + 'clipboard.copy', result(packet));
  const flaws = [
    (v) => { delete v.source; }, (v) => { delete v.rasters; },
    (v) => { v.rasters = []; }, (v) => { v.rasters[0].extra = true; },
    (v) => { v.rasters[0].target.frameScope.extra = true; },
    (v) => { v.rasters[0].item.encodedBytes = 'not admitted'; },
    (v) => { v.source.extra = true; }, (v) => { v.source.instanceId = ''; },
    (v) => { v.source.documentId = ''; }, (v) => { v.source.contentRevision = -1; },
    (v) => { v.source.contentRevision = 9007199254740992; },
    (v) => { v.source.strokeIds = []; }, (v) => { v.source.strokeIds = ['']; },
  ];
  for (const flaw of flaws) {
    const bad = JSON.parse(JSON.stringify(packet)); flaw(bad);
    assert.throws(() => SDK.validateRequest(request(bad)), TypeError);
    assert.throws(() => contract.result(P + 'clipboard.copy', result(bad)), TypeError);
  }
});
