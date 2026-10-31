'use strict';
// L01-J closed public contract of the native vector operations. Literal payloads; every guard is paired with a one-member
// mutation of a payload that passes, so a guard that stops guarding fails here. The Rust table mirrors these rules
// (nemo-mcp/src/native_vector_contract_tests.rs carries the same literals).
const test = require('node:test');
const assert = require('node:assert/strict');
const C = require('../src/js/adapters/native-object-vector-contract.js');

const clone = (value) => JSON.parse(JSON.stringify(value));
const seg = (x, y) => ({ point: { x, y }, handleIn: { x: 0, y: 0 }, handleOut: { x: 0, y: 0 } });
const target = (strokeId, kind = 'authored') => ({ contextId: 'scene-root', frameScope: { kind, frame: 3 }, layerUid: 'L1', strokeId });
const record = (strokeId, kind) => ({
  schemaVersion: 1, family: 'editable-vector-path', target: target(strokeId, kind),
  geometry: { kind: 'cubic-path', closed: false, coordinateSpace: 'document-world', handleSpace: 'relative', segments: [seg(10, 20), seg(110, 70)] },
  fill: null,
  vector: { stroke: { kind: 'solid', r: 0, g: 0.5, b: 1, a: 1 }, hasRealStroke: true, strokeWidth: 4, strokeCap: 'round', strokeJoin: 'miter',
    miterLimit: 10, dashArray: [4, 2], dashOffset: 0, paintOrder: 'strokeFirst', opacity: 1 },
});
const ribbon = () => {
  const r = record('native-stroke:8:0');
  r.vector.stroke = null; r.vector.hasRealStroke = false; delete r.vector.dashArray;
  r.fill = { kind: 'solid', r: 0, g: 0, b: 0, a: 1 };
  r.vectorExt = {
    pressure: {
      centerSegments: [{ point: { x: 1, y: 2 }, handleIn: { x: 0, y: 0 }, handleOut: { x: 3, y: 0 }, width: 6.5 }],
      widthProfile: [{ t: 0, width: 2 }, { t: 0.5, width: 6 }, { t: 1, width: 2 }], strokeProfile: 'taper-both', profileBase: 12,
    },
    paramShape: { kind: 'rect', tl: 5, tr: 0, br: 0, bl: 10, box: { x1: 0, y1: 0, x2: 100, y2: 50 } }, boxAngle: 30,
  };
  return r;
};
const COMMANDS = [
  [C.VECTOR, { command: 'object.vector.create', record: record('') }],
  [C.VECTOR, { command: 'object.vector.replace', record: record('native-stroke:8:0') }],
  [C.VECTOR, { command: 'object.vector.delete', stableTarget: target('native-stroke:8:0') }],
  [C.PARAMETRIC, { command: 'object.parametric.set', stableTarget: target('native-stroke:8:0'), paramShape: { kind: 'star', pointCount: 5, innerRatio: 0.5 } }],
  [C.PARAMETRIC, { command: 'object.parametric.translate', stableTarget: target('native-stroke:8:0'), delta: [3, -4.5] }],
  [C.PARAMETRIC, { command: 'object.parametric.scale', stableTarget: target('native-stroke:8:0'), scale: [2, -1], anchor: [10, 20] }],
  [C.EFFECTS, { command: 'object.path.effects.set', layerUid: 'L1', stack: [{ type: 'zigzag', enabled: true, p1: 12, p2: 4, p3: 0 }, { type: 'roughen', p1: null }] }],
];
const accepts = (operation, payload) => { try { C.payload(operation, payload); return true; } catch (error) { assert.ok(error instanceof TypeError, String(error)); return false; } };

test('the three operations are admitted, revisioned and nothing else is', () => {
  assert.deepEqual(C.OPERATIONS.slice(), ['command.document.object.vector.edit', 'command.document.object.parametric.edit', 'command.document.object.path.effects.edit']);
  for (const op of C.OPERATIONS) { assert.equal(C.admitted(op), true); assert.equal(C.requiresRevision(op), true); assert.notEqual(C.paths(op), null); }
  for (const op of ['command.document.object.item.edit', 'query.document.object', 'command.document.object.vector.create', '', undefined, null]) {
    assert.equal(C.admitted(op), false, String(op)); assert.equal(C.paths(op), null);
  }
  assert.throws(() => C.payload('command.document.object.item.edit', {}), TypeError);
  assert.throws(() => C.result('query.document.object', {}), TypeError);
  assert.equal(C.MAX_REQUEST_BYTES, 4096, 'the public transport bound is unchanged (lead decision to raise it)');
  assert.equal(C.ENGINE_VECTOR_REQUEST_BYTES, 65536);
});

test('every literal command payload is accepted, its operation is the only one that accepts it', () => {
  for (const [operation, payload] of COMMANDS) {
    assert.equal(accepts(operation, payload), true, payload.command);
    for (const other of C.OPERATIONS.filter((op) => op !== operation)) assert.equal(accepts(other, payload), false, `${payload.command} under ${other}`);
    assert.equal(C.operationOf(payload.command), operation);
  }
  assert.equal(C.operationOf('object.vector.merge'), null);
  assert.deepEqual(C.members('object.parametric.scale'), ['stableTarget', 'scale', 'anchor']);
  assert.equal(C.members('nope'), null);
});

test('a missing, null, extra or foreign member refuses every command', () => {
  for (const [operation, payload] of COMMANDS) {
    for (const key of Object.keys(payload)) {
      const missing = clone(payload); delete missing[key];
      assert.equal(accepts(operation, missing), false, `missing ${key} of ${payload.command}`);
      const nulled = clone(payload); nulled[key] = null;
      assert.equal(accepts(operation, nulled), false, `null ${key} of ${payload.command}`);
    }
    assert.equal(accepts(operation, Object.assign(clone(payload), { extra: 1 })), false, 'extra ' + payload.command);
  }
  assert.equal(accepts(C.VECTOR, { command: 'object.vector.delete', record: record('x') }), false, 'a delete names no record');
  assert.equal(accepts(C.VECTOR, [COMMANDS[0][1]]), false);
  assert.equal(accepts(C.VECTOR, Object.assign(Object.create({ inherited: 1 }), COMMANDS[2][1])), false, 'a foreign prototype is refused');
});

const RECORD_MUTATIONS = [
  ['family', (r) => { r.family = 'closed-solid-fill-cubic-path'; }],
  ['schema version', (r) => { r.schemaVersion = 2; }],
  ['closed must be a boolean', (r) => { r.geometry.closed = 1; }],
  ['geometry kind', (r) => { r.geometry.kind = 'polyline'; }],
  ['coordinate space', (r) => { r.geometry.coordinateSpace = 'screen'; }],
  ['handle space', (r) => { r.geometry.handleSpace = 'absolute'; }],
  ['one anchor', (r) => { r.geometry.segments.length = 1; }],
  ['257 anchors', (r) => { r.geometry.segments = new Array(257).fill(seg(1, 1)); }],
  ['segment member', (r) => { r.geometry.segments[0].extra = 1; }],
  ['NaN coordinate', (r) => { r.geometry.segments[0].point.x = NaN; }],
  ['Infinity handle', (r) => { r.geometry.segments[1].handleOut.y = Infinity; }],
  ['string coordinate', (r) => { r.geometry.segments[1].point.y = '2'; }],
  ['fill must be present', (r) => { delete r.fill; }],
  ['fill channel above one', (r) => { r.fill = { kind: 'solid', r: 1.01, g: 0, b: 0, a: 1 }; }],
  ['fill kind', (r) => { r.fill = { kind: 'linear', r: 0, g: 0, b: 0, a: 1 }; }],
  ['stroke without the flag', (r) => { r.vector.hasRealStroke = false; }],
  ['flag without a stroke', (r) => { r.vector.stroke = null; }],
  ['stroke must be present', (r) => { delete r.vector.stroke; }],
  ['stroke alpha', (r) => { r.vector.stroke.a = -0.1; }],
  ['negative width', (r) => { r.vector.strokeWidth = -1; }],
  ['cap', (r) => { r.vector.strokeCap = 'arrow'; }],
  ['join', (r) => { r.vector.strokeJoin = 'curve'; }],
  ['negative miter limit', (r) => { r.vector.miterLimit = -1; }],
  ['empty dash array', (r) => { r.vector.dashArray = []; }],
  ['negative dash', (r) => { r.vector.dashArray = [4, -2]; }],
  ['zero-length dash pattern', (r) => { r.vector.dashArray = [0, 0]; }],
  ['non-finite dash offset', (r) => { r.vector.dashOffset = NaN; }],
  ['paint order', (r) => { r.vector.paintOrder = 'fill'; }],
  ['opacity above one', (r) => { r.vector.opacity = 1.5; }],
  ['opacity below zero', (r) => { r.vector.opacity = -0.5; }],
  ['unknown vector member', (r) => { r.vector.strokeGradient = {}; }],
  ['unknown record member', (r) => { r.fillGradient = {}; }],
  ['empty vectorExt', (r) => { r.vectorExt = {}; }],
];
test('each record guard refuses its one-member mutation (and the unmutated record passes)', () => {
  assert.doesNotThrow(() => C.record(record('native-stroke:8:0'), false));
  for (const [name, mutate] of RECORD_MUTATIONS) {
    const bad = record('native-stroke:8:0'); mutate(bad);
    assert.throws(() => C.record(bad, false), TypeError, name);
    // the same mutation is refused inside a create and a replace payload
    const inCreate = record(''); mutate(inCreate);
    assert.equal(accepts(C.VECTOR, { command: 'object.vector.create', record: inCreate }), false, 'create: ' + name);
  }
  const edge = record('native-stroke:8:0'); edge.geometry.segments = new Array(256).fill(seg(1, 1));
  assert.doesNotThrow(() => C.record(edge, false), '256 anchors are the maximum');
  const minimal = record('native-stroke:8:0'); minimal.vector = { stroke: null, hasRealStroke: false, strokeWidth: 0, strokeCap: 'butt', strokeJoin: 'bevel', opacity: 0 };
  assert.doesNotThrow(() => C.record(minimal, false), 'width 0, opacity 0 and every optional member absent are values, not defaults');
});

const RIBBON_MUTATIONS = [
  ['width stop t decreasing', (r) => { r.vectorExt.pressure.widthProfile[2].t = 0.25; }],
  ['width stop t above one', (r) => { r.vectorExt.pressure.widthProfile[2].t = 1.5; }],
  ['width stop t below zero', (r) => { r.vectorExt.pressure.widthProfile[0].t = -0.1; }],
  ['profile intent', (r) => { r.vectorExt.pressure.strokeProfile = 'wavy'; }],
  ['negative profile base', (r) => { r.vectorExt.pressure.profileBase = -1; }],
  ['centre anchor without width', (r) => { delete r.vectorExt.pressure.centerSegments[0].width; }],
  ['pressure scalar', (r) => { r.vectorExt.pressure.pressure = 0.5; }],
  ['rect member on a star', (r) => { r.vectorExt.paramShape = { kind: 'star', tl: 1 }; }],
  ['unknown shape kind', (r) => { r.vectorExt.paramShape = { kind: 'line' }; }],
  ['box with a missing corner', (r) => { delete r.vectorExt.paramShape.box.y2; }],
  ['non-finite box angle', (r) => { r.vectorExt.boxAngle = Infinity; }],
  ['a pressure stroke has no real stroke', (r) => { r.vector.stroke = { kind: 'solid', r: 0, g: 0, b: 0, a: 1 }; r.vector.hasRealStroke = true; }],
];
test('pressure, paramShape and boxAngle keep their own rules', () => {
  const good = ribbon();
  assert.doesNotThrow(() => C.record(good, false));
  const paramOnly = record('native-stroke:8:0'); paramOnly.vectorExt = { paramShape: { kind: 'ellipse', startAngle: 0, sweep: 359.9, innerRadius: 0 } };
  assert.doesNotThrow(() => C.record(paramOnly, false), 'a real stroke may carry a paramShape');
  const emptyRibbon = ribbon(); emptyRibbon.vectorExt = { pressure: {} };
  assert.doesNotThrow(() => C.record(emptyRibbon, false), 'a ribbon without centreline or profile is still a vector brush');
  for (const [name, mutate] of RIBBON_MUTATIONS) {
    const bad = ribbon(); mutate(bad);
    assert.throws(() => C.record(bad, false), TypeError, name);
  }
});

test('a create names no stroke id; a replace and a delete name an authored one', () => {
  assert.equal(accepts(C.VECTOR, { command: 'object.vector.create', record: record('native-stroke:8:0') }), false);
  assert.equal(accepts(C.VECTOR, { command: 'object.vector.replace', record: record('') }), false);
  assert.equal(accepts(C.VECTOR, { command: 'object.vector.replace', record: record('native-stroke:8:0', 'reference') }), false, 'reference storage is readable, never replaced');
  assert.equal(accepts(C.VECTOR, { command: 'object.vector.delete', stableTarget: target('') }), false);
  assert.doesNotThrow(() => C.record(record('native-stroke:8:0', 'reference'), false), 'the object read returns reference-scope records');
  for (const mutate of [(t) => { t.contextId = 'two words'; }, (t) => { t.layerUid = ''; }, (t) => { t.frameScope.kind = 'ghost'; }, (t) => { t.frameScope.frame = 4294967296; },
    (t) => { t.frameScope.frame = -1; }, (t) => { t.frameScope.frame = 1.5; }, (t) => { t.extra = 1; }]) {
    const t = target('native-stroke:8:0'); mutate(t);
    assert.equal(accepts(C.VECTOR, { command: 'object.vector.delete', stableTarget: t }), false);
  }
});

test('parameter and effect payloads keep their own member sets', () => {
  const PM = C.PARAMETRIC, st = target('native-stroke:8:0');
  assert.equal(accepts(PM, { command: 'object.parametric.set', stableTarget: st, paramShape: { kind: 'rect', tl: 1, box: { x1: 0, y1: 0, x2: 1, y2: 1 } } }), false, 'a set carries no box');
  assert.equal(accepts(PM, { command: 'object.parametric.set', stableTarget: st, paramShape: { kind: 'rect', startAngle: 1 } }), false, 'a member of another kind');
  assert.equal(accepts(PM, { command: 'object.parametric.set', stableTarget: st, paramShape: { kind: 'rect' } }), true, 'every member is optional: absent stays absent');
  assert.equal(accepts(PM, { command: 'object.parametric.set', stableTarget: st, paramShape: { kind: 'rect', tl: null } }), false);
  for (const delta of [[1], [1, 2, 3], ['1', 2], [1, NaN], 'ab', null]) assert.equal(accepts(PM, { command: 'object.parametric.translate', stableTarget: st, delta }), false, JSON.stringify(delta));
  assert.equal(accepts(PM, { command: 'object.parametric.scale', stableTarget: st, scale: [1, 1] }), false, 'a scale names its anchor');
  assert.equal(accepts(PM, { command: 'object.parametric.scale', stableTarget: st, scale: [1, 1], anchor: [0] }), false);
  const FX = C.EFFECTS;
  for (const stack of [[{ type: 'wave' }], [{ type: 'zigzag', enabled: null }], [{ type: 'zigzag', enabled: 1 }], [{ type: 'zigzag', p1: '12' }], [{ type: 'zigzag', p1: NaN }],
    [{ type: 'zigzag', p4: 1 }], [{}], [null], { type: 'zigzag' }, 'zigzag']) {
    assert.equal(accepts(FX, { command: 'object.path.effects.set', layerUid: 'L1', stack }), false, JSON.stringify(stack));
  }
  assert.equal(accepts(FX, { command: 'object.path.effects.set', layerUid: 'L1', stack: [] }), true, 'an empty stack is an authored empty stack');
  assert.equal(accepts(FX, { command: 'object.path.effects.set', layerUid: '', stack: [] }), false);
  assert.equal(accepts(FX, { command: 'object.path.effects.set', layerUid: 'L1', stack: [{ type: 'zigzag' }, { type: 'zigzag' }] }), true, 'duplicate types are allowed, as in the baseline');
});

const receipt = (operation, strokeId, applied) => ({ applied, historyEntriesAdded: applied ? 1 : 0, operation, strokeId });
const ok = (request, revision, result) => ({ apiVersion: 2, requestId: request.requestId, instanceId: request.instanceId, documentId: request.documentId, contentRevision: revision, ok: true, result });
const envelope = (operation, payload, revision = 7) => ({ apiVersion: 2, requestId: 'vector-7', instanceId: 'instance-A', documentId: 'document-A', operation, expectedRevision: revision, payload });

test('receipts name the operation and the stroke they touched and advance the revision by their own disposition', () => {
  const [create, replace, del, , translate, , effects] = COMMANDS.map(([operation, payload]) => envelope(operation, payload));
  assert.doesNotThrow(() => { C.result(C.VECTOR, receipt('create', 'native-stroke:8:0', true)); C.correlate(create, ok(create, 8, receipt('create', 'native-stroke:8:0', true))); });
  assert.throws(() => C.correlate(create, ok(create, 8, receipt('create', 'other:8:0', true))), TypeError, 'a created id is allocated by the owner');
  assert.throws(() => C.correlate(create, ok(create, 7, receipt('create', 'native-stroke:8:0', false))), TypeError, 'a create always applies');
  assert.throws(() => C.correlate(create, ok(create, 8, receipt('replace', 'native-stroke:8:0', true))), TypeError, 'wrong operation');
  assert.doesNotThrow(() => C.correlate(replace, ok(replace, 7, receipt('replace', 'native-stroke:8:0', false))));
  assert.throws(() => C.correlate(replace, ok(replace, 8, receipt('replace', 'native-stroke:8:0', false))), TypeError, 'a no-op does not advance');
  assert.throws(() => C.correlate(replace, ok(replace, 8, receipt('replace', 'native-stroke:9:0', true))), TypeError, 'a replace reports its target');
  assert.doesNotThrow(() => C.correlate(del, ok(del, 8, receipt('delete', 'native-stroke:8:0', true))));
  const compact = (applied) => ({ applied, historyEntriesAdded: applied ? 1 : 0 });
  assert.doesNotThrow(() => { C.result(C.PARAMETRIC, compact(true)); C.correlate(translate, ok(translate, 8, compact(true))); C.correlate(effects, ok(effects, 7, compact(false))); });
  assert.throws(() => C.correlate(effects, ok(effects, 8, compact(false))), TypeError);
  assert.throws(() => C.result(C.EFFECTS, { applied: true, historyEntriesAdded: 1, strokeId: 'x' }), TypeError, 'only the vector edit reports a stroke');
  assert.throws(() => C.result(C.VECTOR, { applied: true, historyEntriesAdded: 1 }), TypeError);
  assert.throws(() => C.result(C.VECTOR, receipt('merge', 'x', true)), TypeError);
  assert.throws(() => C.result(C.PARAMETRIC, { applied: false, historyEntriesAdded: 1 }), TypeError);
  const foreign = ok(create, 8, receipt('create', 'native-stroke:8:0', true)); foreign.instanceId = 'instance-B';
  assert.throws(() => C.correlate(create, foreign), TypeError);
  const other = ok(create, 8, receipt('create', 'native-stroke:8:0', true)); other.documentId = 'document-B';
  assert.throws(() => C.correlate(create, other), TypeError);
  const replaced = { apiVersion: 2, requestId: 'vector-7', instanceId: 'instance-A', documentId: 'document-B', contentRevision: 0, ok: false, error: { code: 'wrong_document', message: 'x' } };
  assert.doesNotThrow(() => C.correlate(create, replaced), 'a refusal may report the replacement document');
});

test('compose joins another contract of the same interface and leaves it untouched', () => {
  const seen = [];
  const base = {
    admitted: (op) => op === 'base.op', requiresRevision: (op) => op === 'base.op',
    payload: (op, value, env) => { seen.push(['payload', op, env]); }, result: (op) => { seen.push(['result', op]); },
    correlate: (q) => { seen.push(['correlate', q.operation]); }, paths: (op) => (op === 'base.op' ? 'base-paths' : null),
  };
  const joined = C.compose(base);
  assert.equal(joined.admitted('base.op'), true);
  for (const op of C.OPERATIONS) { assert.equal(joined.admitted(op), true); assert.equal(joined.requiresRevision(op), true); }
  assert.equal(joined.admitted('other.op'), false);
  joined.payload('base.op', {}, 'env'); joined.result('base.op', {}); joined.correlate({ operation: 'base.op' }, {});
  assert.equal(joined.paths('base.op'), 'base-paths');
  assert.deepEqual(seen, [['payload', 'base.op', 'env'], ['result', 'base.op'], ['correlate', 'base.op']]);
  assert.doesNotThrow(() => joined.payload(C.VECTOR, COMMANDS[2][1]));
  assert.deepEqual(seen.length, 3, 'a vector operation never reaches the base contract');
  assert.equal(C.compose(null).admitted(C.EFFECTS), true);
  assert.throws(() => Object.assign(joined, { admitted: null }), TypeError, 'the composed contract is frozen');
});

test('authored identities may start with data: or blob:, nothing else may', () => {
  const opaque = C.paths(C.VECTOR);
  for (const at of ['payload.record.target.layerUid', 'payload.record.target.strokeId', 'payload.stableTarget.layerUid', 'payload.stableTarget.strokeId', 'payload.layerUid', 'result.strokeId']) {
    assert.equal(opaque.has(at), true, at);
  }
  for (const at of ['payload.record.family', 'payload.record.geometry.kind', 'payload.stack.0.type', 'payload.record.vector.paintOrder', 'result.operation']) assert.equal(opaque.has(at), false, at);
  assert.equal(C.bytes('é'), 2);
  assert.equal(C.bytes(JSON.stringify(envelope(C.VECTOR, COMMANDS[0][1]))) < C.MAX_REQUEST_BYTES, true, 'a two-anchor create fits the public bound');
});


test('gradient record admission is closed at the same public vector and read DTO boundary', () => {
  const Stroke=require('../src/js/adapters/native-object-vector-stroke.js');
  const dict={segments:[{point:[0,0],handleIn:[0,0],handleOut:[0,0]},{point:[8,0],handleIn:[0,0],handleOut:[0,0]}],
    closed:false,hasRealStroke:false,fillColor:'#ff0000',fillGradient:{kind:'linear',from:[0,0],to:[8,0],stops:[{offset:0,color:'#ff0000'},{offset:1,color:'#0000ff'}]}};
  const mapped=Stroke.toRecord(dict,{layerUid:'L1',frame:0,strokeId:''});assert.equal(mapped.ok,true);
  const record=mapped.record;assert.doesNotThrow(()=>C.record(record,true));
  for (const change of [g=>{g.stops=[];},g=>{delete g.to;},g=>{g.stops[0].offset=null;},g=>{g.stops[0].extra=1;},
    g=>{g.from=[0,0,0];},g=>{g.stops[1].color='data:paint';}]) {
    const bad=JSON.parse(JSON.stringify(record));change(bad.vectorExt.fillGradient);assert.throws(()=>C.record(bad,true),TypeError);
  }
  const missing=JSON.parse(JSON.stringify(record));delete missing.vectorExt;assert.doesNotThrow(()=>C.record(missing,true));
  const nullGradient=JSON.parse(JSON.stringify(record));nullGradient.vectorExt.fillGradient=null;assert.throws(()=>C.record(nullGradient,true),TypeError);
});

test('trimEnds is an optional finite positive limit on the two creates and refused elsewhere', () => {
  const create = COMMANDS.find(([, p]) => p.command === 'object.vector.create');
  const replace = COMMANDS.find(([, p]) => p.command === 'object.vector.replace');
  for (const [value, ok] of [[24, true], [0.5, true], [0, false], [-1, false], [Infinity, false], ['24', false], [null, false]]) {
    assert.equal(accepts(create[0], { ...create[1], trimEnds: value }), ok, String(value));
  }
  assert.equal(accepts(replace[0], { ...replace[1], trimEnds: 24 }), false);
});

test('trimClip is a well-formed stop-at-axis clip beside trimEnds on the creates and the all-create batch (#2361)', () => {
  const create = COMMANDS.find(([, p]) => p.command === 'object.vector.create');
  const clip = { axis: [60, 0, 60, 100], mirrors: [[0, 1]] };
  const base = { ...create[1], trimEnds: 24 };
  assert.equal(accepts(create[0], { ...base, trimClip: clip }), true);
  assert.equal(accepts(create[0], { ...create[1], trimClip: clip }), false, 'needs trimEnds');
  for (const bad of [{ ...clip, axis: [60, 0, 60] }, { ...clip, axis: [60, 0, 60, '100'] }, { ...clip, axis: [60, 0, 60, Infinity] }, { ...clip, mirrors: [] }, { ...clip, mirrors: [[0]] },
    { ...clip, mirrors: [[0, -1]] }, { ...clip, mirrors: [[0, 1.5]] }, { ...clip, mirrors: [[0, 1], [1, 2]] }, { ...clip, mirrors: [[1, 1]] }, { ...clip, extend: true }, { axis: clip.axis }, null, 'clip']) {
    assert.equal(accepts(create[0], { ...base, trimClip: bad }), false, JSON.stringify(bad));
  }
  const replace = COMMANDS.find(([, p]) => p.command === 'object.vector.replace');
  assert.equal(accepts(replace[0], { ...replace[1], trimEnds: 24, trimClip: clip }), false);
  const batch = (extra) => ({ command: 'object.vector.batch', ops: [{ op: 'create', record: create[1].record }, { op: 'create', record: create[1].record }], ...extra });
  assert.equal(accepts(create[0], batch({ trimEnds: 24, trimClip: clip })), true);
  assert.equal(accepts(create[0], batch({ trimClip: clip })), false);
});

test('a batch reshape takes new geometry or a ribbon centreline (#2362 sculpt), never both or neither', () => {
  const EDIT = 'command.document.object.vector.edit';
  const anchor = (x) => ({ point: { x, y: 2 }, handleIn: { x: 0, y: 0 }, handleOut: { x: 3, y: 0 }, width: 6.5 });
  const sculpt = (op) => ({ command: 'object.vector.batch', ops: [Object.assign({ op: 'reshape', stableTarget: target('s-r') }, op)] });
  const geometry = record('s-r').geometry;
  assert.doesNotThrow(() => C.payload(EDIT, sculpt({ center: [anchor(0), anchor(9)] })));
  assert.doesNotThrow(() => C.payload(EDIT, sculpt({ geometry })));
  for (const bad of [{}, { center: [anchor(0), anchor(9)], geometry }, { center: [anchor(0)] }, { center: [anchor(0), { ...anchor(9), width: 'wide' }] },
    { center: [anchor(0), { point: { x: 1, y: 1 }, handleIn: { x: 0, y: 0 }, handleOut: { x: 0, y: 0 } }] }]) {
    assert.throws(() => C.payload(EDIT, sculpt(bad)), TypeError, JSON.stringify(Object.keys(bad)));
  }
  assert.throws(() => C.payload(EDIT, { command: 'object.vector.batch', ops: [{ op: 'clone', stableTarget: target('s-r'), frame: 3, center: [anchor(0), anchor(9)] }] }), TypeError);
});

test('a batch reshape by matrix (#2372 Select) is six finite numbers and excludes geometry and a centreline', () => {
  const EDIT = 'command.document.object.vector.edit';
  const step = (op) => ({ command: 'object.vector.batch', ops: [Object.assign({ op: 'reshape', stableTarget: target('s-r') }, op)] });
  assert.doesNotThrow(() => C.payload(EDIT, step({ matrix: [1, 0, 0, 1, 12.5, -4] })));
  assert.doesNotThrow(() => C.payload(EDIT, step({ matrix: [2, 0, 0, 2, 0, 0], frame: 3 })));
  for (const bad of [{ matrix: [1, 0, 0, 1, 0] }, { matrix: [1, 0, 0, 1, 0, 0, 0] }, { matrix: [1, 0, 0, 1, 0, NaN] }, { matrix: 'id' }, { matrix: [1, 0, 0, 1, 0, 0], geometry: record('s-r').geometry }]) {
    assert.throws(() => C.payload(EDIT, step(bad)), TypeError, JSON.stringify(bad));
  }
  assert.throws(() => C.payload(EDIT, { command: 'object.vector.batch', ops: [{ op: 'clone', stableTarget: target('s-r'), frame: 3, matrix: [1, 0, 0, 1, 0, 0] }] }), TypeError);
});

test('a batch is atomic: skipUnavailable is an optional boolean, and only an opted-in receipt may report skipped steps (#2372)', () => {
  const EDIT = 'command.document.object.vector.edit';
  const step = (id) => ({ op: 'restyle', stableTarget: target(id), style: { stroke: '#00ff00' } });
  const batch = (extra = {}) => ({ command: 'object.vector.batch', ops: [step('s-a'), step('s-b')], ...extra });
  assert.equal(accepts(EDIT, batch()), true);
  for (const good of [true, false]) assert.equal(accepts(EDIT, batch({ skipUnavailable: good })), true, String(good));
  for (const bad of [1, 0, 'true', null, [true], {}]) assert.equal(accepts(EDIT, batch({ skipUnavailable: bad })), false, JSON.stringify(bad));
  assert.equal(accepts(EDIT, { command: 'object.vector.delete', stableTarget: target('s-a'), skipUnavailable: true }), false, 'a batch member only');
  const withReceipt = (payload, extra) => {
    const request = envelope(EDIT, payload), result = { applied: true, historyEntriesAdded: 1, operation: 'batch', strokeId: 's-a', strokeIds: ['s-a', 's-b'], ...extra };
    return () => { C.result(C.VECTOR, result); C.correlate(request, ok(request, 8, result)); };
  };
  assert.doesNotThrow(withReceipt(batch({ skipUnavailable: true }), { skippedSteps: 1 }));
  assert.doesNotThrow(withReceipt(batch(), {}));
  for (const bad of [0, 2, -1, 1.5, '1']) assert.throws(withReceipt(batch({ skipUnavailable: true }), { skippedSteps: bad }), TypeError, String(bad));
  assert.throws(withReceipt(batch(), { skippedSteps: 1 }), TypeError, 'an atomic request never skips');
  assert.throws(withReceipt(batch({ skipUnavailable: false }), { skippedSteps: 1 }), TypeError, 'an explicit false never skips');
  assert.throws(() => C.result(C.VECTOR, { applied: true, historyEntriesAdded: 1, operation: 'replace', strokeId: 's-a', skippedSteps: 1 }), TypeError, 'only a batch receipt reports skipped steps');
});
