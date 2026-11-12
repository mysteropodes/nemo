'use strict';
// The rows' requests against the published contracts, and row selection. A builder that drifts from the transport schema, a
// row that names an operation the contract does not have, or a lane without the rows it promises, fails here, before an app is
// ever started.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const P = require('../installed-evidence/payloads.cjs');
const { allRows, selectRows, LANES } = require('../installed-evidence/runner.cjs');
const { ASSIGNED, NOT_COVERED, laneCoverage } = require('../installed-evidence/coverage.cjs');
const { buildReceipt } = require('../installed-evidence/receipt.cjs');
const ROOT = path.resolve(__dirname, '../../..');
const load = file => JSON.parse(fs.readFileSync(path.join(ROOT, 'engineering/application', file), 'utf8'));
const transport = load('native-transport-v2.schema.json'), items = load('native-object-item-v1.schema.json'), brush = load('native-bitmap-brush-resources-v1.schema.json'), metadata = load('native-property-metadata-v1.schema.json');

// A validator for the keyword subset these schemas use, including the unions the shared one ignores (oneOf, allOf, not).
function errors(schema, value, defs, at = '$') {
  if (schema === true) return [];
  if (schema === false) return [at + ': rejected'];
  if (schema.$ref) { const target = defs[schema.$ref.replace('#/$defs/', '')]; return target === undefined ? [at + ': unresolved ' + schema.$ref] : errors(target, value, defs, at); }
  const out = [];
  if ('const' in schema && JSON.stringify(schema.const) !== JSON.stringify(value)) return [at + ': const ' + JSON.stringify(schema.const)];
  if (schema.enum && !schema.enum.some(e => JSON.stringify(e) === JSON.stringify(value))) return [at + ': enum ' + JSON.stringify(value)];
  if (schema.type) {
    const types = [].concat(schema.type), actual = value === null ? 'null' : Array.isArray(value) ? 'array' : Number.isInteger(value) ? 'integer' : typeof value;
    if (!types.includes(actual) && !(actual === 'integer' && types.includes('number'))) return [at + ': type ' + types + ' got ' + actual];
  }
  if (typeof value === 'string') {
    if (schema.pattern && !new RegExp(schema.pattern).test(value)) out.push(at + ': pattern ' + schema.pattern);
    if (schema.minLength != null && value.length < schema.minLength) out.push(at + ': minLength');
    if (schema.maxLength != null && value.length > schema.maxLength) out.push(at + ': maxLength');
  }
  if (typeof value === 'number') {
    if (schema.minimum != null && value < schema.minimum) out.push(at + ': minimum');
    if (schema.maximum != null && value > schema.maximum) out.push(at + ': maximum');
    if (schema.exclusiveMinimum != null && value <= schema.exclusiveMinimum) out.push(at + ': exclusiveMinimum');
  }
  if (Array.isArray(value)) {
    if (schema.minItems != null && value.length < schema.minItems) out.push(at + ': minItems');
    if (schema.maxItems != null && value.length > schema.maxItems) out.push(at + ': maxItems');
    if (schema.items) value.forEach((v, i) => out.push(...errors(schema.items, v, defs, at + '[' + i + ']')));
  } else if (value && typeof value === 'object') {
    for (const key of schema.required || []) if (!(key in value)) out.push(at + ': missing ' + key);
    for (const key of Object.keys(value)) {
      if (schema.properties && schema.properties[key] !== undefined) out.push(...errors(schema.properties[key], value[key], defs, at + '.' + key));
      else if (schema.additionalProperties === false) out.push(at + ': unexpected ' + key);
    }
  }
  if (schema.allOf) for (const s of schema.allOf) out.push(...errors(s, value, defs, at));
  if (schema.not && errors(schema.not, value, defs, at).length === 0) out.push(at + ': matches a forbidden shape');
  for (const key of ['oneOf', 'anyOf']) if (schema[key] && !schema[key].some(s => errors(s, value, defs, at).length === 0)) out.push(at + ': matches none of ' + key);
  return out;
}

// Which schema owns each operation's payload.
const owner = new Map();
for (const variant of transport.$defs.Request.oneOf) {
  const text = JSON.stringify(variant), op = text.match(/"operation":\{"const":"([^"]+)"\}/), ref = text.match(/"payload":\{"\$ref":"#\/\$defs\/(\w+)"/);
  if (op) owner.set(op[1], ref ? { schema: transport, name: ref[1] } : null);
}
const SPECIAL = { [P.OBJ + 'item.edit']: { schema: items, name: 'ItemEdit' }, [P.OBJ + 'asset.attach']: { schema: items, name: 'ImageImportAttach' }, [P.OBJ + 'asset.remove']: { schema: items, name: 'AssetRemove' },
  [P.OBJ + 'brush.bitmap.publish']: { schema: brush, name: 'Publish' }, 'query.document.object.brush.bitmap': { schema: brush, name: 'Query' },
  'query.document.object.property.metadata': { schema: metadata, name: 'Selector' } };

const T = P.target('L1', 'native-stroke:1:0'), rec = P.vectorRecord({ layerUid: 'L1', segments: P.rectSegments(0, 0, 10, 10), fill: [1, 0, 0, 1], stroke: [0, 0, 1, 1], width: 4 });
const clip = { contextId: 'scene-root', layerUid: 'L1', frame: 0, records: [] };
const samples = [
  P.vectorCreate(rec), P.vectorCreate(rec, { placement: 'behind' }), P.vectorReplace(P.vectorRecord({ layerUid: 'L1', strokeId: 'native-stroke:1:0', segments: P.rectSegments(0, 0, 9, 9), fill: [0, 1, 0, 0.5] })), P.vectorDelete(T),
  P.vectorCreate(P.vectorRecord({ layerUid: 'L1', closed: false, segments: [P.seg(1, 1), P.seg(5, 5)], stroke: [0, 0, 0, 1], cap: 'butt', ext: { paramShape: { kind: 'rect', tl: 0, tr: 0, br: 0, bl: 0, box: { x1: 0, y1: 0, x2: 10, y2: 10 } } } })),
  P.vectorCreate(P.vectorRecord({ layerUid: 'L1', segments: P.rectSegments(0, 0, 4, 4), fill: [1, 0, 0, 1], ext: { fillGradient: { kind: 'linear', from: [0, 0], to: [4, 0], stops: [{ offset: 0, color: '#ff0000' }, { offset: 1, color: '#0000ff' }] } } })),
  P.parametricSet(T, { kind: 'rect', tl: 20, tr: 20, br: 20, bl: 20 }), P.parametricTranslate(T, [1, 2]), P.parametricScale(T, [2, 0.5], [0, 0]),
  P.pathEffects('L1', [{ type: 'zigzag', enabled: true, p1: 12, p2: 6, p3: 0 }, { type: 'roughen', enabled: false, p1: 8, p2: 6 }]), P.pathEffects('L1', []), P.layerPolicy('L1', 'locked', true),
  P.undo(), P.redo(), P.undo(2), P.groupCreate('L1', 'grp_a', 'G', ['s1', 's2']), P.groupUngroup('L1', ['s1']), P.groupCombine('L1', 'grp_a', 'subtract', ['s1', 's2']), P.groupMode('L1', 'grp_a', 'none'),
  P.groupRename('L1', 'grp_a', 'n'), P.groupMemberRemove('L1', 's1'), P.booleanApply('L1', 'subtract', ['s1', 's2']), P.booleanErase('L1', 's1', [[0, 0], [5, 0], [5, 5]]), P.booleanFlatten('L1', 'grp_a'),
  P.layerCreate('N'), P.layerDuplicate('L1'), P.layerRename('L1', 'n'), P.layerReorder(['L2'], 0, 'L2'), P.layerDelete('L2'), P.layerVisibility('L1', false), P.layerSplit('L1'),
  P.selectionDelete('L1', ['s1']), P.selectionDuplicate('L1', ['s1'], [0, 200]), P.selectionDuplicate('L1', ['s1']), P.clipboardCopy('L1', ['s1']), P.clipboardCut('L1', ['s1']), P.clipboardPaste('L1', clip),
  P.assetAttach({ pin: 'snap-1', name: 'n', path: '/tmp/x.png', lifecycleGeneration: 1 }), P.assetRemove('snap-1', 'asset-1'),
  P.rasterMove(T, 1, 2), P.rasterResize(T, 3, 4), P.rasterRotate(T, 90), P.rasterOpacity(T, 0.5), P.textCreate('L1', P.textBlock('HELLO', { size: 72 })), P.textEdit(T, { size: 36, letterSpacing: 10 }), P.textDelete(T),
  P.textCreate('L1', P.textBlock('X', { font: { kind: 'asset', assetId: 'a' } })), P.textTargets(0, 'p', 'L1'), P.textInspect(0, 'p', T), P.textBody(0, 'p', T),
  P.brushPublish('snap-1', 'asset-1', 'b', 8, 8), P.brushInspect(0, 'snap-1', 'b'),
  P.brushStroke({ pin: 'snap-1', lifecycleGeneration: 1, layerUid: 'L1', groupId: 'bmb_1', path: '/tmp/b.png', companion: { x: 1, y: 2, width: 58, height: 18, opacity: 1 }, segments: [P.seg(1, 1), P.seg(9, 9)] }),
  P.compositingBlend('L1', 'multiply'), P.objectRead(0, T), P.propertyOpacity('L1', [50]), P.propertyMetadata(0, 'snap-1', 'L1'),
  P.meshAttach(T, { outline: [[0, 0], [1, 0], [1, 1]], verts: [[0, 0], [1, 0], [1, 1]], tris: [0, 1, 2], offsets: [[0, 0], [0, 0], [0, 0]], cols: 1, rows: 1 }), P.meshSetOffset('im_1', 2, 0.3, 0.25),
  P.historyRead(0, 'snap-1'), P.propertyRead(0, 'snap-1', 'L1', 'opacity'),
  P.vectorCreate(P.vectorRecord({ layerUid: 'L1', closed: false, segments: [P.seg(1, 1), P.seg(5, 5), P.seg(9, 1)], stroke: [0, 0, 1, 1], join: 'miter', cap: 'butt', style: { miterLimit: 3, dashArray: [12, 8], dashOffset: 0, paintOrder: 'strokeFirst' } })),
  P.vectorCreate(P.vectorRecord({ layerUid: 'L1', segments: P.rectSegments(0, 0, 4, 4), fill: [0, 0, 0, 1], ext: { pressure: { centerSegments: [{ point: { x: 0, y: 0 }, handleIn: { x: 0, y: 0 }, handleOut: { x: 0, y: 0 }, width: 6 }],
    widthProfile: [{ t: 0, width: 2 }, { t: 1, width: 12 }], strokeProfile: 'taper-out', profileBase: 3 } } })),
];

test('every request builder emits a payload the published schema accepts, for an operation the contract defines', () => {
  for (const { operation, payload } of samples) {
    const home = SPECIAL[operation] || owner.get(operation);
    assert.ok(home, 'operation is not in the contract: ' + operation);
    const found = errors(home.schema.$defs[home.name], payload, home.schema.$defs);
    assert.deepEqual(found, [], operation + ' ' + JSON.stringify(payload).slice(0, 160) + ' -> ' + found.slice(0, 3));
  }
});

test('the validator rejects what the contract rejects (a drifted builder would be caught)', () => {
  const home = owner.get(P.OBJ + 'vector.edit'), defs = home.schema.$defs;
  const bad = P.vectorCreate(rec).payload;
  assert.ok(errors(defs[home.name], { ...bad, extra: 1 }, defs).length > 0, 'unknown member');
  assert.ok(errors(defs[home.name], { ...bad, command: 'object.vector.nope' }, defs).length > 0, 'wrong command');
  assert.ok(errors(defs[home.name], { command: 'object.vector.create' }, defs).length > 0, 'missing record');
  const layer = owner.get(P.OBJ + 'layer.reorder');
  assert.ok(errors(defs[layer.name], { command: 'object.layer.reorder', layerUids: [], gap: 0, selection: { active: 'a', selected: [] } }, defs).length > 0, 'empty layerUids');
});

test('lane rows: the four lanes, unique ids, each row names tasks and issue, and every row sends only contract operations', () => {
  const rows = allRows();
  assert.deepEqual(LANES, ['2107', '2108', '2112', '2113']);
  assert.equal(new Set(rows.map(r => r.id)).size, rows.length);
  for (const lane of LANES) assert.ok(rows.filter(r => r.lane === lane).length >= 3, lane + ' has too few rows');
  assert.ok(rows.some(r => r.id === '2113-image-mesh-sculpt') && rows.some(r => r.id === '2112-asset-remove-lifetime') && rows.some(r => r.id === '2107-stroke-style-pressure-fields'));
  for (const row of rows) {
    assert.ok(row.id.startsWith(row.lane + '-') && typeof row.title === 'string' && row.title.length > 10 && Array.isArray(row.tasks) && row.tasks.length > 0 && typeof row.run === 'function' && row.issue === Number(row.lane), row.id);
  }
  // Rows build every request with the payload builders; a literal operation string in a lane file would bypass the schema test above.
  for (const lane of LANES) {
    const text = fs.readFileSync(path.join(__dirname, '../installed-evidence/lanes/lane-' + lane + '.cjs'), 'utf8');
    assert.equal(/['"](command|query|job)\.document\./.test(text), false, 'lane ' + lane + ' contains a literal operation name');
    assert.equal(/\bsetTimeout\b|\bsleep\b/.test(text), false, 'lane ' + lane + ' sleeps');
  }
  for (const [op, home] of Object.entries(SPECIAL)) assert.ok(home.schema.$defs[home.name], op);
});

test('lane and row selection: default is everything, a lane or row subset is exact, anything unknown refuses', () => {
  const catalog = allRows();
  assert.equal(selectRows({}, catalog).rows.length, catalog.length);
  const one = selectRows({ lanes: ['2107'] }, catalog);
  assert.deepEqual([...new Set(one.rows.map(r => r.lane))], ['2107']);
  assert.equal(one.omitted.length, catalog.length - one.rows.length);
  const two = selectRows({ lanes: ['2108', '2113'] }, catalog);
  assert.deepEqual(two.lanes, ['2108', '2113']);
  const row = selectRows({ rows: ['2112-text-layout-edit-reopen'] }, catalog);
  assert.deepEqual(row.rows.map(r => r.id), ['2112-text-layout-edit-reopen']);
  assert.deepEqual(selectRows({ lanes: ['2107'], rows: ['2107-path-effects-stack'] }, catalog).rows.map(r => r.id), ['2107-path-effects-stack']);
  for (const bad of [{ lanes: ['9999'] }, { lanes: [] }, { lanes: ['2107', '2107'] }, { rows: ['nope'] }, { rows: [] }, { lanes: 'all' }, { lanes: ['2107'], rows: ['2108-layer-lifecycle'] }]) {
    assert.throws(() => selectRows(bad, catalog), Error, JSON.stringify(bad));
  }
});

test('the mesh read requests are the ones the published transport schema declares', () => {
  const base = { contextId: 'scene-root', layerUid: 'L1', frame: 0, workId: 'native-work-1', viewGeneration: 1 };
  const checks = [[transport.$defs.ObjectPresentationQueryPayload, P.presentationRead(3).payload],
    ...[{ kind: 'selection' }, { kind: 'offsets', meshId: 'im_1', cursor: 0, limit: 16 }, { kind: 'verts', meshId: 'im_1', cursor: 16, limit: 1 }].map(page => [transport.$defs.ObjectImageMeshQueryPayload, P.meshQuery(3, 'native-object:d:3', { ...base, page }).payload])];
  for (const [schema, payload] of checks) assert.deepEqual(errors(schema, payload, transport.$defs), [], JSON.stringify(payload));
  assert.ok(errors(transport.$defs.ObjectImageMeshQueryPayload, P.meshQuery(3, 'p', { ...base, page: { kind: 'offsets', meshId: 'im_1', cursor: 0, limit: 99 } }).payload, transport.$defs).length > 0, 'a drifted page size is rejected');
});

test('coverage: every id a lane assigns is covered by a row or listed as not covered, every task a row names is assigned, and the receipt carries the table', () => {
  const rows = allRows();
  for (const lane of LANES) {
    const c = laneCoverage(lane, rows), mine = rows.filter(r => r.lane === lane);
    for (const row of mine) for (const task of row.tasks) assert.ok(ASSIGNED[lane].includes(task), row.id + ' names ' + task + ' which lane ' + lane + ' does not assign');
    for (const id of ASSIGNED[lane]) assert.ok(c.coveredByRows[id].length > 0 || NOT_COVERED[lane].some(e => e.id === id), lane + ' assigns ' + id + ' with no row and no not-covered entry');
    for (const entry of NOT_COVERED[lane]) assert.ok(typeof entry.what === 'string' && entry.what.length > 15 && typeof entry.reason === 'string' && entry.reason.length > 15, lane);
    assert.equal(c.laneComplete, false, 'a lane with listed gaps is never reported complete');
  }
  assert.deepEqual(laneCoverage('2112', rows).uncoveredIds, ['G38b']);
  const receipt = buildReceipt({ rows: rows.map(r => ({ ...r, verdict: 'pass', operations: [], assertions: [] })), selection: { lanes: LANES, rows: rows.map(r => r.id), omitted: [], catalogRows: {} }, cleanup: { ok: true } });
  assert.deepEqual(Object.keys(receipt.coverage).sort(), LANES);
  assert.ok(receipt.coverage['2113'].notCovered.length > 0 && receipt.residualHandsOn.some(line => /coverage\.notCovered/.test(line)));
});
