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
  const request = { operation: OP, instanceId: 'i', documentId: 'd', expectedRevision: 7, payload:{command:'object.import.batch',layers:[{sourceId:'w',name:'Wire'}],records:[]} };
  const good = (revision, applied) => ({ instanceId: 'i', documentId: 'd', ok: true, contentRevision: revision, result: { applied,historyEntriesAdded:applied?1:0,layers:applied?[{sourceId:'w',layerUid:'native-layer:8:0'}]:[],objects:[],skipped:[] } });
  assert.doesNotThrow(() => contract.correlate(request, good(8, true)));
  assert.doesNotThrow(() => contract.correlate({...request,payload:{...request.payload,layers:[]}}, good(7, false)));
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

test('import receipts cannot drop, duplicate or substitute declared mappings or forge a no-op', () => {
  const q={instanceId:'i',documentId:'d',expectedRevision:7,payload:{command:'object.import.batch',layers:[{sourceId:'a',name:'A'},{sourceId:'b',name:'B'}],records:[],skipped:[{kind:'asset',sourceId:'skipped',reason:'unsupported'}]}};
  const value={applied:true,historyEntriesAdded:1,layers:[{sourceId:'a',layerUid:'native-a'},{sourceId:'b',layerUid:'native-b'}],objects:[],skipped:q.payload.skipped};
  const reply=result=>({instanceId:'i',documentId:'d',contentRevision:result.applied?8:7,ok:true,result});
  assert.doesNotThrow(()=>contract.correlate(q,reply(value)));
  for(const mutate of [r=>r.layers.pop(),r=>r.layers[1].sourceId='a',r=>r.layers[0].sourceId='foreign',r=>r.layers[1].layerUid='native-a',r=>r.skipped[0].reason='changed']){
    const r=structuredClone(value);mutate(r);contract.result(OP,r);assert.throws(()=>contract.correlate(q,reply(r)),/declared batch/);
  }
  assert.throws(()=>contract.correlate(q,reply({applied:false,historyEntriesAdded:0,layers:[],objects:[],skipped:q.payload.skipped})),/declared batch/);
  assert.throws(()=>contract.correlate({...q,payload:{...q.payload,unavailable:[{sourceId:'raster',need:'raster-asset'}]}},reply(value)),/declared batch/);
  const withRecords={...q,payload:{...q.payload,records:[{sourceId:'one',record:record('a',0)},{sourceId:'two',record:record('b',1)}]}};
  const complete={...value,objects:[{sourceId:'one',strokeId:'native-one'},{sourceId:'two',strokeId:'native-two'}]};
  assert.doesNotThrow(()=>contract.correlate(withRecords,reply(complete)));
  for(const mutate of [r=>r.objects.pop(),r=>r.objects[1].sourceId='one',r=>r.objects[0].sourceId='foreign',r=>r.objects[1].strokeId='native-one',r=>r.skipped=[]]){
    const r=structuredClone(complete);mutate(r);contract.result(OP,r);assert.throws(()=>contract.correlate(withRecords,reply(r)),/declared batch/);
  }
});

// Additional families use the same native command; fixtures describe retained lookup, not byte providers.
function itemExamples() {
  return JSON.parse(fs.readFileSync(path.join(__dirname, '../engineering/application/capabilities-v2/native-object-import.json'), 'utf8')).examples.slice(1, 4);
}
function familyPayload() {
  const examples = itemExamples();
  return { ...structuredClone(examples[0].request.payload),
    assets: examples.slice(0, 2).flatMap(e => structuredClone(e.request.payload.assets)),
    text: structuredClone(examples[1].request.payload.text) };
}
function changedFamily(change) {
  const payload = familyPayload();
  change(payload);
  return payload;
}

test('retained image, retained font and bundled Text examples use the actual shared application contract', () => {
  const application = require('../src/js/adapters/native-application.js');
  assert.equal(itemExamples().length, 3);
  for (const example of itemExamples()) {
    ok(example.request.payload);
    assert.doesNotThrow(() => application.validateRequest(example.request));
    assert.doesNotThrow(() => contract.result(OP, example.response.result));
    assert.doesNotThrow(() => contract.correlate(example.request, example.response));
  }
  ok({ command: 'object.import.batch', layers: [], records: [], assets: [], raster: [], text: [] });
  const oversized = structuredClone(itemExamples()[2].request);
  oversized.payload.text[0].entry.block.text = 'x'.repeat(4096);
  assert.throws(() => application.validateRequest(oversized), /byte|bound|4096|large/i);
});

test('asset descriptors cannot forge retained records, locators, bytes or provider provenance', () => {
  for (const key of ['bytes', 'record', 'source', 'path', 'mediaType', 'hasFontAdmission']) {
    bad(changedFamily(p => { p.assets[0][key] = 'forged'; }), /missing or unknown/, key);
  }
  for (const key of ['assets', 'raster', 'text']) bad(changedFamily(p => { p[key] = null; }));
  for (const value of ['', ' ', '\u0085', 'a\u0000b', 'é'.repeat(128), {}]) {
    bad(changedFamily(p => { p.assets[0].assetId = value; }));
  }
  bad(changedFamily(p => { p.assets[1].sourceId = p.assets[0].sourceId; }), /repeats/);
  bad(changedFamily(p => { p.skipped = [{ kind: 'asset', sourceId: p.assets[0].sourceId, reason: 'skip' }]; }), /repeats/);
  bad(changedFamily(p => { p.text[0].sourceId = p.raster[0].sourceId; }), /repeats/);
  bad(changedFamily(p => { p.skipped = [{ kind: 'element', sourceId: p.text[0].sourceId, reason: 'skip' }]; }), /repeats/);
  ok(changedFamily(p => { p.raster[0].entry.target.contextId = 'component:import'; }));
});

test('RasterItem and TextBlock preserve presence and refuse invalid native fields before dispatch', () => {
  const raster = (key, value) => changedFamily(p => { p.raster[0].entry.item[key] = value; });
  for (const [key, value] of [['width', 0], ['height', -1], ['opacity', 1.1], ['rotation', NaN], ['isBitmapBrush', null], ['brushGroupId', ''], ['meshId', 'x'.repeat(256)], ['url', 'file:///x']]) bad(raster(key, value), undefined, key);
  ok(raster('brushGroupId', ' '));
  const text = (key, value) => changedFamily(p => { p.text[0].entry.block[key] = value; });
  for (const [key, value] of [['text', ' \ufeff'], ['text', 'a\u0000b'], ['text', '\ud800'], ['size', 0], ['fixedWidth', 0], ['fixedHeight', -2], ['lineHeightMult', 0], ['bold', null], ['textCase', 'title'], ['textOverflow', false], ['color', 'red']]) bad(text(key, value), undefined, key);
  ok(text('text', '\u0085'));
  ok(text('fixedWidth', null));
  ok(text('bold', false));
  ok(text('wordSpacing', 0));
  bad(text('font', { kind: 'bundled', fontKey: 'vector:Roboto-Regular' }));
  bad(text('font', { kind: 'asset', assetId: 'font-source', bytes: 'forged' }));
  for (const key of ['raster', 'text']) {
    bad(changedFamily(p => { p[key][0].entry.extra = true; }));
    bad(changedFamily(p => { p[key][0].entry.target.strokeId = 'chosen'; }));
    bad(changedFamily(p => { p[key][0].entry.target.frameScope.kind = 'reference'; }));
  }
});

test('all item and asset mappings are required and correlated without relaxing legacy receipts', () => {
  const payload = familyPayload();
  const request = { ...itemExamples()[0].request, payload };
  const result = { applied: true, historyEntriesAdded: 1, layers: [], skipped: [],
    objects: [{ sourceId: 'raster-source', strokeId: 'native-raster' }, { sourceId: 'text-source', strokeId: 'native-text' }],
    assets: [{ sourceId: 'image-source', assetId: 'native-image' }, { sourceId: 'font-source', assetId: 'native-font' }] };
  const reply = value => ({ ...itemExamples()[0].response, result: value });
  assert.doesNotThrow(() => contract.correlate(request, reply(result)));
  for (const mutate of [r => { delete r.assets; }, r => r.assets.pop(), r => { r.assets[1].sourceId = 'image-source'; }, r => { r.assets[1].assetId = 'native-image'; }, r => { r.assets[0].sourceId = 'foreign'; }, r => r.objects.pop(), r => { r.objects[1].strokeId = 'native-raster'; }]) {
    const badResult = structuredClone(result);
    mutate(badResult);
    contract.result(OP, badResult);
    assert.throws(() => contract.correlate(request, reply(badResult)), /declared batch/);
  }
  const assetsOnly = { ...request, payload: { command: 'object.import.batch', layers: [], records: [], assets: payload.assets } };
  assert.doesNotThrow(() => contract.correlate(assetsOnly, reply({ ...result, objects: [] })));
  assert.throws(() => contract.correlate(request, reply({ applied: false, historyEntriesAdded: 0, layers: [], objects: [], assets: [], skipped: [] })), /revision mismatch|declared batch/);
  assert.throws(() => contract.result(OP, { ...result, assets: null }));
  const oversized = { ...result, assets: Array.from({ length: 70 }, (_, i) => ({ sourceId: `s${i}`, assetId: `native-asset:${i}` })) };
  assert.throws(() => contract.result(OP, oversized), /exceeds its bound/);
});

function sourceExamples() {
  return JSON.parse(fs.readFileSync(path.join(__dirname, '../engineering/application/capabilities-v2/native-object-import.json'), 'utf8')).examples.slice(4);
}

test('only actual file, bundled and Google source descriptors enter the same pinned batch', () => {
  const application = require('../src/js/adapters/native-application.js');
  assert.equal(sourceExamples().length, 3);
  for (const example of sourceExamples()) {
    assert.doesNotThrow(() => application.validateRequest(example.request));
    assert.doesNotThrow(() => contract.correlate(example.request, example.response));
    for (const key of ['documentSnapshotId', 'lifecycleGeneration']) {
      const copy = structuredClone(example.request.payload);
      delete copy[key];
      bad(copy);
    }
  }
  const base = sourceExamples()[0].request.payload;
  for (const [key, value] of [['sources', []], ['sources', null], ['documentSnapshotId', 'bad snapshot'], ['lifecycleGeneration', 0], ['lifecycleGeneration', 1.5]]) bad({ ...structuredClone(base), [key]: value });
  for (const key of ['documentSnapshotId', 'lifecycleGeneration']) bad({ command: contract.COMMAND, layers: [], records: [], [key]: base[key] });
  for (const change of [p => { p.sources[0].bytes = []; }, p => { p.sources[0].source.bytes = []; }, p => { p.sources[0].source = { kind: 'url', url: 'https://example.invalid/a.png' }; }, p => { p.sources[0].source.path = 'relative.png'; }, p => { p.sources[0].source.path = 'file:///a.png'; }, p => { p.sources[0].mediaType = 'image/jpeg'; }, p => { p.sources[0].name = ' '; }, p => { p.sources[0].name = 'x'.repeat(256); }]) {
    const copy = structuredClone(base);
    change(copy);
    bad(copy);
  }
  const google = structuredClone(sourceExamples()[2].request.payload);
  for (const [key, value] of [['weight', 0], ['weight', 1001], ['weight', 1.5], ['italic', null], ['family', ' '], ['url', 'https://example.invalid']]) {
    const copy = structuredClone(google);
    copy.sources[0].source[key] = value;
    bad(copy);
  }
  bad({ ...structuredClone(base), sources: Array.from({ length: 1025 }, (_, i) => ({ ...base.sources[0], sourceId: `source:${i}` })) });
});

test('retained assets, source assets and skips share one ID space and complete receipts', () => {
  const example = structuredClone(sourceExamples()[0]);
  const payload = example.request.payload;
  payload.assets = [{ sourceId: 'retained-source', assetId: 'retained-image' }];
  example.response.result.assets.unshift({ sourceId: 'retained-source', assetId: 'native-asset:8:1' });
  assert.doesNotThrow(() => contract.correlate(example.request, example.response));
  bad({ ...payload, assets: [{ sourceId: 'image-source', assetId: 'retained-image' }] });
  bad({ ...payload, skipped: [{ kind: 'asset', sourceId: 'image-source', reason: 'skip' }] });
  for (const change of [r => r.assets.reverse(), r => r.assets.pop(), r => { r.assets[1].sourceId = 'retained-source'; }, r => { r.assets[1].assetId = r.assets[0].assetId; }, r => { r.applied = false; r.historyEntriesAdded = 0; }]) {
    const copy = structuredClone(example.response);
    change(copy.result);
    assert.throws(() => contract.correlate(example.request, copy));
  }
  assert.doesNotThrow(() => contract.correlate(sourceExamples()[2].request, sourceExamples()[2].response));
});


test('objectOrder is an exact new-object permutation and never changes canonical receipt order', () => {
  const payload = familyPayload();
  payload.records = [{ sourceId: 'vector-source', record: record('L0', 0) }];
  const order = ['text-source', 'vector-source', 'raster-source'];
  ok(payload);
  ok({ ...payload, objectOrder: order });
  for (const value of [null, {}, 'text-source', [null], [], order.slice(1), [...order, 'extra'],
    ['text-source', 'text-source', 'raster-source'], ['image-source', ...order.slice(1)],
    ['L0', ...order.slice(1)], ['existing-origin', ...order.slice(1)]]) {
    bad({ ...payload, objectOrder: value });
  }
  ok({ command: contract.COMMAND, layers: [], records: [], objectOrder: [] });
  const example = structuredClone(itemExamples()[0]);
  example.request.payload = { ...payload, objectOrder: order };
  example.response.result = { applied: true, historyEntriesAdded: 1, layers: [], skipped: [],
    objects: ['vector-source', 'raster-source', 'text-source'].map((sourceId, i) => ({ sourceId, strokeId: `object:${i}` })),
    assets: ['image-source', 'font-source'].map((sourceId, i) => ({ sourceId, assetId: `asset:${i}` })) };
  assert.doesNotThrow(() => contract.correlate(example.request, example.response));
  const reordered = structuredClone(example.response);
  reordered.result.objects.reverse();
  assert.throws(() => contract.correlate(example.request, reordered));
  example.request.payload.objectOrder.reverse();
  assert.doesNotThrow(() => contract.correlate(example.request, example.response));
});
