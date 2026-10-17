'use strict';
// L10-J: the closed declaration of `command.document.object.import.batch`. Every accepted and refused shape below is a literal; the
// refusals mirror the owner's own rules (id table, vector codec anchor bounds, one-member-at-a-time mutations).
const test = require('node:test');
const assert = require('node:assert/strict');
const contract = require('../src/js/adapters/native-import-contract.js');
const seam = require('../src/js/adapters/native-import-seam.js');
const font = require('../src/js/adapters/native-font-contract.js');
const { record, mixed } = require('./helpers/native-import-records.cjs');
const fs = require('node:fs');
const path = require('node:path');

const OP = 'command.document.object.import.batch';
const ok = (v) => assert.doesNotThrow(() => contract.payload(OP, v));
const bad = (v, pattern, label) => assert.throws(() => contract.payload(OP, v), pattern || TypeError, label);
const edit = (fn) => { const v = mixed(); fn(v); return v; };

test('the operation, command and bounds are the ones of the accepted L10-H transaction', () => {
  assert.equal(contract.OP, OP);
  assert.equal(contract.COMMAND, 'object.import.batch');
  assert.deepEqual([contract.MAX_REQUEST_BYTES, contract.MAX_RECEIPT_BYTES, contract.MAX_ID_BYTES, contract.MIN_ANCHORS, contract.MAX_ANCHORS], [4096, 3072, 255, 2, 256]);
  assert.deepEqual(contract.SKIP_KINDS, ['layer', 'element', 'asset']);
  assert.deepEqual(contract.PROVIDERS, ['raster-asset', 'text', 'font']);
  assert.equal(contract.admitted(OP), true);
  assert.equal(contract.requiresRevision(OP), true);
  assert.equal(contract.admitted('command.document.object.layer.create'), false);
  assert.throws(() => contract.payload('command.document.object.layer.create', mixed()), /not declared/);
});

test('the payload is closed: required members, optional skipped/unavailable, no unknown member anywhere', () => {
  ok(mixed());
  ok(edit((v) => { delete v.skipped; delete v.unavailable; }));
  for (const key of ['command', 'layers', 'records']) bad(edit((v) => { delete v[key]; }), /missing or unknown/, key);
  bad(edit((v) => { v.extra = 1; }), /missing or unknown/);
  bad(edit((v) => { v.command = 'object.fill.set'; }), /command is invalid/);
  bad(edit((v) => { v.layers = {}; }), /must be an array/);
  bad(edit((v) => { v.layers[0].color = '#fff'; }), /missing or unknown/);
  bad(edit((v) => { v.records[0].layerUid = 'x'; }), /missing or unknown/);
  bad(edit((v) => { v.skipped[0].extra = 1; }), /missing or unknown/);
  bad(edit((v) => { v.unavailable = [{ sourceId: 'p', need: 'raster-asset', extra: 1 }]; }), /missing or unknown/);
  bad(edit((v) => { v.layers[0].name = ''; }), /non-empty string/);
  bad([], /plain object/);
  bad(null, /plain object/);
});

test('source ids follow the owner id table: non-blank, at most 255 bytes, no NUL, unique per kind', () => {
  for (const id of ['', ' ', '\t\n', 'a'.repeat(256), 'é'.repeat(128), 'a\u0000b', 7, null]) bad(edit((v) => { v.layers[0].sourceId = id; }), /valid source id/, JSON.stringify(id));
  ok(edit((v) => { v.layers[0].sourceId = 'a'.repeat(255); }));
  ok(edit((v) => { v.layers[0].sourceId = 'é'.repeat(127); }));
  bad(edit((v) => { v.layers.push({ sourceId: 'w', name: 'Again' }); }), /repeats a source id/);
  bad(edit((v) => { v.records[1].sourceId = 'w:0'; }), /repeats a source id/);
  bad(edit((v) => { v.skipped[0].sourceId = 'w:0'; }), /repeats a source id/, 'a record and an element skip are both strokes');
  ok(edit((v) => { v.skipped.push({ kind: 'layer', sourceId: 'w:0', reason: 'hidden' }); }), 'a layer skip lives in the layer id space');
  bad(edit((v) => { v.skipped.push({ kind: 'layer', sourceId: 'a', reason: 'x' }, { kind: 'layer', sourceId: 'a', reason: 'y' }); }), /repeats a source id/);
  bad(edit((v) => { v.skipped.push({ kind: 'asset', sourceId: 'a', reason: 'x' }, { kind: 'asset', sourceId: 'a', reason: 'y' }); }), /repeats a source id/);
});

test('skip and unavailable rows are typed: known kinds, a reason, a known provider', () => {
  for (const kind of ['layer', 'element', 'asset']) ok(edit((v) => { v.skipped = [{ kind, sourceId: 's', reason: 'why' }]; }));
  bad(edit((v) => { v.skipped[0].kind = 'stroke'; }), /kind is not admitted/);
  bad(edit((v) => { v.skipped[0].reason = ''; }), /must say why/);
  for (const need of ['raster-asset', 'text', 'font']) ok(edit((v) => { v.unavailable = [{ sourceId: 'p', need }]; }));
  bad(edit((v) => { v.unavailable = [{ sourceId: 'p', need: 'gpu' }]; }), /known provider/);
});

test('a record is a schema-1 editable-vector-path with an empty stroke id, an authored frame and 2..256 anchors', () => {
  const r = (fn) => edit((v) => fn(v.records[0].record));
  bad(r((x) => { x.target.strokeId = 'native-stroke:1:0'; }), /owner mints it/);
  bad(r((x) => { x.target.frameScope.kind = 'reference'; }), /authored/);
  for (const frame of [-1, 1.5, 4294967296, '0']) bad(r((x) => { x.target.frameScope.frame = frame; }), /u32 frame/, String(frame));
  ok(r((x) => { x.target.frameScope.frame = 4294967295; }));
  bad(r((x) => { x.target.layerUid = ''; }), /layerUid/);
  bad(r((x) => { x.family = 'raster'; }), /not a schema-1/);
  bad(r((x) => { x.schemaVersion = 2; }), /not a schema-1/);
  bad(r((x) => { x.geometry.segments = [x.geometry.segments[0]]; }), /2\.\.256 anchors/);
  ok(r((x) => { x.geometry.segments = Array.from({ length: 256 }, (_, i) => ({ point: { x: i, y: 0 }, handleIn: { x: 0, y: 0 }, handleOut: { x: 0, y: 0 } })); }));
  bad(r((x) => { x.geometry.segments = Array.from({ length: 257 }, (_, i) => ({ point: { x: i, y: 0 }, handleIn: { x: 0, y: 0 }, handleOut: { x: 0, y: 0 } })); }), /2\.\.256 anchors/);
  bad(r((x) => { x.geometry.handleSpace = 'absolute'; }), /relative cubic path/);
  bad(r((x) => { x.geometry.coordinateSpace = 'layer-local'; }), /document-world/);
  bad(r((x) => { x.geometry.segments[0].point.x = NaN; }), /finite number/);
  bad(r((x) => { x.geometry.segments[0].handleOut.y = Infinity; }), /finite number/);
  bad(r((x) => { x.geometry.segments[0].extra = 1; }), /missing or unknown/);
  bad(r((x) => { x.paintOrder = 'strokeFirst'; }), /missing or unknown/);
});

test('paints and stroke members are closed: solid only, channels in 0..1, flags agree with the stroke', () => {
  const r = (fn) => edit((v) => fn(v.records[0].record));
  ok(r((x) => { x.fill = { kind: 'solid', r: 0, g: 0.5, b: 1, a: 0.5 }; }));
  bad(r((x) => { x.fill = { kind: 'gradient', r: 0, g: 0, b: 0, a: 1 }; }), /must be solid/);
  bad(r((x) => { x.fill = { kind: 'solid', r: 2, g: 0, b: 0, a: 1 }; }), /outside 0\.\.1/);
  bad(r((x) => { x.vector.hasRealStroke = false; }), /agree with stroke/);
  bad(r((x) => { x.vector.stroke = null; }), /agree with stroke/);
  ok(r((x) => { x.vector.stroke = null; x.vector.hasRealStroke = false; }));
  bad(r((x) => { x.vector.strokeCap = 'triangle'; }), /cap or join/);
  bad(r((x) => { x.vector.strokeJoin = 'smooth'; }), /cap or join/);
  ok(r((x) => { x.vector.miterLimit = 4; x.vector.dashArray = [4, 2]; x.vector.dashOffset = 1.5; }));
  bad(r((x) => { x.vector.miterLimit = NaN; }), /finite number/);
  bad(r((x) => { x.vector.dashArray = [4, 'a']; }), /finite number/);
  bad(r((x) => { x.vector.opacity = undefined; }), /missing or unknown|finite number/);
  bad(r((x) => { x.vector.hairline = true; }), /missing or unknown/);
});

test('the result is the compact receipt: closed members, one history entry iff applied, bounded', () => {
  const receipt = { applied: true, historyEntriesAdded: 1, layers: [{ sourceId: 'w', layerUid: 'native-layer:8:0' }],
    objects: [{ sourceId: 'w:0', strokeId: 'native-stroke:8:0' }], skipped: [{ kind: 'element', sourceId: 'svg:9', reason: 'r' }] };
  assert.doesNotThrow(() => contract.result(OP, receipt));
  assert.doesNotThrow(() => contract.result(OP, { applied: false, historyEntriesAdded: 0, layers: [], objects: [], skipped: [] }));
  const throws = (fn, p) => assert.throws(() => { const copy = JSON.parse(JSON.stringify(receipt)); fn(copy); contract.result(OP, copy); }, p || TypeError);
  throws((r) => { r.historyEntriesAdded = 0; }, /disposition/);
  throws((r) => { r.applied = false; }, /disposition/);
  throws((r) => { r.document = {}; }, /missing or unknown/);
  throws((r) => { delete r.objects; }, /missing or unknown/);
  throws((r) => { r.layers[0].layerUid = ''; }, /layerUid/);
  throws((r) => { r.objects[0].strokeId = 7; }, /strokeId/);
  throws((r) => { r.skipped[0].kind = 'stroke'; }, /skip report/);
  throws((r) => { for (let i = 0; i < 60; i++) r.objects.push({ sourceId: `s${i}`, strokeId: `native-stroke:8:${i + 1}` }); }, /exceeds its bound/);
});

test('correlation: same owner and document, revision advances by exactly the applied edit', () => {
  const request = { operation: OP, instanceId: 'i', documentId: 'd', expectedRevision: 7 };
  const good = (revision, applied) => ({ instanceId: 'i', documentId: 'd', ok: true, contentRevision: revision, result: { applied } });
  assert.doesNotThrow(() => contract.correlate(request, good(8, true)));
  assert.doesNotThrow(() => contract.correlate(request, good(7, false)));
  assert.throws(() => contract.correlate(request, good(7, true)), /revision mismatch/);
  assert.throws(() => contract.correlate(request, good(8, false)), /revision mismatch/);
  assert.throws(() => contract.correlate(request, Object.assign(good(8, true), { instanceId: 'x' })), /owner mismatch/);
  assert.throws(() => contract.correlate(request, Object.assign(good(8, true), { documentId: 'x' })), /document mismatch/);
  assert.doesNotThrow(() => contract.correlate(request, { instanceId: 'i', documentId: 'other', ok: false, error: { code: 'wrong_document' } }), 'a replaced document answers wrong_document');
  assert.throws(() => contract.correlate(request, { instanceId: 'i', documentId: 'other', ok: false, error: { code: 'unavailable' } }), /document mismatch/);
});

test('the seam serves the import operation and leaves the font contract exactly as it was', () => {
  const fontOp = 'command.document.object.asset.font.import';
  assert.equal(seam.admitted(OP), true);
  assert.equal(seam.admitted(fontOp), true);
  assert.equal(seam.admitted('command.document.object.layer.create'), false);
  assert.equal(seam.requiresRevision(OP), true);
  assert.equal(seam.requiresRevision(fontOp), true);
  assert.equal(seam.requiresRevision('query.document.object.asset.font'), false);
  assert.equal(seam.paths(fontOp).has('payload.assetId'), font.paths(fontOp).has('payload.assetId'));
  assert.equal(seam.paths('x'), null);
  assert.equal(seam.paths(OP).has('payload.layers.0.name'), true, 'opaque authored strings may start with data:');
  assert.equal(seam.paths(OP).has('result.objects.0.strokeId'), true);
  assert.doesNotThrow(() => seam.payload(OP, mixed()));
  assert.throws(() => seam.payload(OP, edit((v) => { v.extra = 1; })), /missing or unknown/);
  assert.equal(contract.compose(null).admitted(OP), true);
  assert.equal(record('L0', 0).target.layerUid, 'L0');
});

// The parity table both validators judge (tools/gen_contract_cases.mjs; nemo-mcp/src/native_import_contract_tests.rs applies the same edits).
function applyEdit(root, edit) {
  let parent = root;
  for (const step of edit.path.slice(0, -1)) parent = parent[step];
  const key = edit.path[edit.path.length - 1];
  if (edit.op === 'set') parent[key] = edit.value;
  else if (edit.op === 'remove') delete parent[key];
  else if (edit.op === 'push') parent[key].push(edit.value);
  else if (edit.op === 'repeat') parent[key] = Array.from({ length: edit.count }, () => JSON.parse(JSON.stringify(parent[key][0])));
  else throw new Error('unknown edit ' + edit.op);
}
test('the shared parity table: every named edit is judged as the table declares (the Rust validator judges the same rows)', () => {
  const table = JSON.parse(fs.readFileSync(path.join(__dirname, 'fixtures', 'native-import-adapters', 'contract-cases.json'), 'utf8'));
  assert.equal(table.rows.length, 82);
  assert.equal(table.rows.filter((r) => r.valid).length, 21);
  assert.deepEqual(table.meta, { generator: 'tools/gen_contract_cases.mjs', rows: 82, valid: 21 });
  for (const row of table.rows) {
    const payload = JSON.parse(JSON.stringify(table.base));
    row.edits.forEach((edit) => applyEdit(payload, edit));
    let accepted = true;
    try { contract.payload(OP, payload); } catch (_) { accepted = false; }
    assert.equal(accepted, row.valid, row.label);
  }
});
