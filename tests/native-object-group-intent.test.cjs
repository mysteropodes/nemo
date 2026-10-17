'use strict';
// L02-J group + boolean intents: literal wire requests, typed refusals, unavailability and selection rebuild.
const test = require('node:test'), assert = require('node:assert/strict'), fs = require('node:fs'), path = require('node:path');
const { createHarness, request } = require('./native-object-edit-harness.cjs');
const { createGroupIntent, OPERATIONS } = require('../src/js/adapters/native-object-group-intent.js');
const { portsFromSession } = require('../src/js/adapters/native-object-edit-intent-core.js');

const P = 'command.document.object.';
const target = { layerUid: 'layer-a', contextId: 'scene-root' };
const CASES = [
  ['group', { ...target, groupId: 'group-1', name: 'Head', strokeIds: ['stroke-1', 'stroke-2'] }, 'group.create', 'group-create-1',
    { command: 'object.group.create', layerUid: 'layer-a', contextId: 'scene-root', groupId: 'group-1', name: 'Head', strokeIds: ['stroke-1', 'stroke-2'] }],
  ['ungroup', { ...target, strokeIds: ['stroke-1', 'stroke-2'] }, 'group.ungroup', 'group-ungroup-1',
    { command: 'object.group.ungroup', layerUid: 'layer-a', contextId: 'scene-root', strokeIds: ['stroke-1', 'stroke-2'] }],
  ['combine', { ...target, groupId: 'group-1', mode: 'unite', strokeIds: ['stroke-1', 'stroke-2'] }, 'group.combine', 'group-combine-1',
    { command: 'object.group.combine', layerUid: 'layer-a', contextId: 'scene-root', groupId: 'group-1', mode: 'unite', strokeIds: ['stroke-1', 'stroke-2'] }],
  ['setMode', { ...target, groupId: 'group-1', mode: 'none' }, 'group.mode.set', 'group-mode-1',
    { command: 'object.group.mode.set', layerUid: 'layer-a', contextId: 'scene-root', groupId: 'group-1', mode: 'none' }],
  ['rename', { ...target, groupId: 'group-1', name: 'Face' }, 'group.rename', 'group-rename-1',
    { command: 'object.group.rename', layerUid: 'layer-a', contextId: 'scene-root', groupId: 'group-1', name: 'Face' }],
  ['removeMember', { ...target, strokeId: 'stroke-1' }, 'group.member.remove', 'group-exit-1',
    { command: 'object.group.member.remove', layerUid: 'layer-a', contextId: 'scene-root', strokeId: 'stroke-1' }],
  ['booleanApply', { mode: 'subtract', ...target, frame: 3, strokeIds: ['stroke-1', 'stroke-2'] }, 'boolean.apply', 'boolean-apply-1',
    { command: 'object.boolean.apply', mode: 'subtract', layerUid: 'layer-a', contextId: 'scene-root', frame: 3, strokeIds: ['stroke-1', 'stroke-2'] }],
  ['booleanErase', { ...target, frame: 3, strokeId: 'stroke-1', footprint: [[0, 0], [10, 0], [10.5, 10]] }, 'boolean.erase', 'boolean-erase-1',
    { command: 'object.boolean.erase', layerUid: 'layer-a', contextId: 'scene-root', frame: 3, strokeId: 'stroke-1', footprint: [[0, 0], [10, 0], [10.5, 10]] }],
  ['flatten', { ...target, frame: 3, groupId: 'group-1' }, 'boolean.flatten', 'boolean-flatten-1',
    { command: 'object.boolean.flatten', layerUid: 'layer-a', contextId: 'scene-root', frame: 3, groupId: 'group-1' }],
];
const strokes = { 'layer-a': ['stroke-1', 'stroke-2', 'stroke-3'] };

test('each intent sends exactly one closed typed request pinned to revision 7 and reports the correlated receipt', async () => {
  for (const [method, input, suffix, requestId, payload] of CASES) {
    const h = createHarness({ strokes });
    const outcome = await createGroupIntent(h.ports)[method](input);
    assert.deepEqual(h.state.sent, [request(P + suffix, payload, requestId)], method);
    assert.equal(outcome.ok, true, method);
    assert.equal(outcome.applied, true);
    assert.equal(outcome.revision, 8);
    assert.equal(outcome.operation, P + suffix);
    assert.equal(outcome.synchronized, true);
    assert.equal(outcome.pin.contentRevision, 8);
  }
  assert.deepEqual(OPERATIONS, CASES.map((c) => P + c[2]));
});

test('closed input: unknown, missing, mistyped and unbounded members never reach the transport', async () => {
  const bad = [
    ['group', { ...CASES[0][1], extra: 1 }], ['group', { layerUid: 'layer-a', contextId: 'scene-root', groupId: 'g', name: 'n' }],
    ['group', { ...CASES[0][1], strokeIds: [] }], ['group', { ...CASES[0][1], name: '' }], ['group', { ...CASES[0][1], name: 'x'.repeat(257) }],
    ['group', { ...CASES[0][1], groupId: 'g\u0007' }], ['group', { ...CASES[0][1], groupId: 'g'.repeat(129) }],
    ['group', { ...CASES[0][1], strokeIds: 'stroke-1' }], ['group', { ...CASES[0][1], layerUid: null }],
    ['combine', { ...CASES[2][1], mode: 'overlay' }], ['setMode', { ...CASES[3][1], mode: 'Unite' }],
    ['booleanApply', { ...CASES[6][1], mode: 'none' }], ['booleanApply', { ...CASES[6][1], strokeIds: ['stroke-1'] }],
    ['booleanApply', { ...CASES[6][1], frame: -1 }], ['booleanApply', { ...CASES[6][1], frame: 1.5 }], ['booleanApply', { ...CASES[6][1], frame: 4294967296 }],
    ['booleanErase', { ...CASES[7][1], footprint: [[0, 0], [1, 1]] }], ['booleanErase', { ...CASES[7][1], footprint: [[0, 0], [1, 1], [2, Infinity]] }],
    ['booleanErase', { ...CASES[7][1], footprint: [[0, 0], [1, 1], [2]] }], ['flatten', { ...CASES[8][1], groupId: '' }],
    ['removeMember', null], ['removeMember', ['stroke-1']],
  ];
  const h = createHarness({ strokes });
  const intent = createGroupIntent(h.ports);
  for (const [method, input] of bad) {
    const outcome = await intent[method](input);
    assert.equal(outcome.ok, false, JSON.stringify([method, input]));
    assert.equal(outcome.code, 'invalid_request', JSON.stringify([method, input]));
  }
  assert.equal(h.state.sent.length, 0);
  assert.equal(h.state.submits, 0);
});

test('absent native declaration, absent session and an undeclared operation are typed unavailability and send nothing', async () => {
  const none = createHarness({ strokes, declared: () => false });
  const absent = await createGroupIntent(none.ports).group(CASES[0][1]);
  assert.deepEqual({ ok: absent.ok, code: absent.code, reason: absent.reason }, { ok: false, code: 'unavailable', reason: 'native_declaration_absent' });
  const some = createHarness({ strokes, declared: (op) => op !== P + 'boolean.flatten' });
  const intent = createGroupIntent(some.ports);
  assert.equal((await intent.flatten(CASES[8][1])).reason, 'native_declaration_absent');
  assert.equal((await intent.rename(CASES[4][1])).ok, true, 'a declared sibling is unaffected');
  const noPin = createHarness({ strokes, noSession: true });
  const session = await createGroupIntent(noPin.ports).group(CASES[0][1]);
  assert.deepEqual({ code: session.code, reason: session.reason }, { code: 'unavailable', reason: 'no_presented_native_session' });
  assert.equal(none.state.sent.length + noPin.state.sent.length, 0);
  assert.equal(none.state.pins, 0, 'no pin is read before the declaration is known');
  assert.throws(() => createGroupIntent({ pin() {}, declared() {}, submit() {} }), /requires pin, declared, submit and requestId/);
});

test('a request over the 4096-byte envelope is refused locally and never split', async () => {
  const many = Array.from({ length: 120 }, (_, i) => 'stroke-' + String(i).padStart(4, '0') + '-'.repeat(40));
  const h = createHarness({ strokes: { 'layer-a': many } });
  const outcome = await createGroupIntent(h.ports).group({ ...target, groupId: 'g', name: 'n', strokeIds: many });
  assert.deepEqual({ ok: outcome.ok, code: outcome.code, limit: outcome.limit }, { ok: false, code: 'request_too_large', limit: 4096 });
  assert.equal(h.state.sent.length, 0);
});

test('owner refusals are typed, never retried, and leave the selection and pin untouched', async () => {
  for (const [code, stale] of [['stale_revision', true], ['not_found', false], ['invalid_request', false], ['unavailable', false], ['wrong_instance', false]]) {
    for (const sessionStyle of [false, true]) {
      const h = createHarness({ strokes, sessionStyle, step: () => ({ error: { code, message: 'refused' } }) });
      const intent = createGroupIntent(h.ports);
      assert.equal(intent.select(target, ['stroke-1']).ok, true);
      const outcome = await intent.combine(CASES[2][1]);
      assert.equal(outcome.ok, false);
      assert.equal(outcome.code, code);
      assert.equal(outcome.stale, stale);
      assert.equal(h.state.sent.length, 1, 'no retry');
      assert.deepEqual(intent.selection(), ['stroke-1']);
      assert.equal(h.state.revision, 7);
    }
  }
});

test('a transport rejection is indeterminate, is not retried, and does not poison the next intent', async () => {
  let calls = 0;
  const h = createHarness({ strokes, step: () => (++calls === 1 ? { drop: 'socket closed' } : { applied: true }) });
  const intent = createGroupIntent(h.ports);
  const first = await intent.rename(CASES[4][1]);
  assert.deepEqual({ ok: first.ok, code: first.code, indeterminate: first.indeterminate }, { ok: false, code: 'transport_failed', indeterminate: true });
  const second = await intent.rename(CASES[4][1]);
  assert.equal(second.ok, true);
  assert.deepEqual(h.state.sent.map((r) => r.requestId), ['group-rename-1', 'group-rename-2'], 'a new request id; the failed one is not replayed');
});

test('a malformed receipt from the host is rejected by the SDK and reported as indeterminate', async () => {
  for (const result of [{ applied: true, historyEntriesAdded: 0 }, { applied: false, historyEntriesAdded: 1 }, { applied: true, historyEntriesAdded: 1, extra: 1 }]) {
    const h = createHarness({ strokes, step: () => ({ result }) });
    const outcome = await createGroupIntent(h.ports).rename(CASES[4][1]);
    assert.equal(outcome.code, 'transport_failed');
    assert.equal(outcome.indeterminate, true);
  }
  const wrongRevision = createHarness({ strokes, step: () => ({ revision: 9 }) });
  assert.equal((await createGroupIntent(wrongRevision.ports).rename(CASES[4][1])).code, 'transport_failed');
});

test('a port reply without a result object is indeterminate and never throws', async () => {
  const reply = (request) => ({ apiVersion: 2, requestId: request.requestId, instanceId: 'instance-A', documentId: 'document-A', contentRevision: 8, ok: true });
  for (const extra of [{}, { result: null }, { result: [] }, { result: 'ok' }]) {
    const h = createHarness({ strokes, rawResponse: (request) => Object.assign(reply(request), extra) });
    const outcome = await createGroupIntent(h.ports).rename(CASES[4][1]);
    assert.deepEqual({ ok: outcome.ok, code: outcome.code, indeterminate: outcome.indeterminate }, { ok: false, code: 'transport_failed', indeterminate: true });
    assert.equal(h.state.submits, 1, 'no retry');
  }
});

test('a no-op keeps the revision and the selection and does not report a rebuild', async () => {
  const h = createHarness({ strokes, step: () => ({ applied: false }) });
  const intent = createGroupIntent(h.ports);
  intent.select(target, ['stroke-1', 'stroke-3']);
  const outcome = await intent.setMode(CASES[3][1]);
  assert.deepEqual({ ok: outcome.ok, applied: outcome.applied, revision: outcome.revision }, { ok: true, applied: false, revision: 7 });
  assert.equal('selectionRebuilt' in outcome, false);
  assert.deepEqual(intent.selection(), ['stroke-1', 'stroke-3']);
});

test('selection is rebuilt from stable native ids after the revision and stale ids are rejected before sending', async () => {
  const h = createHarness({ strokes, step: () => ({ strokes: { 'layer-a': ['stroke-1', 'stroke-9'] } }) });
  const intent = createGroupIntent(h.ports);
  assert.deepEqual(intent.select(target, ['stroke-1', 'stroke-2']).selection, ['stroke-1', 'stroke-2']);
  const stale = await intent.booleanApply({ ...CASES[6][1], strokeIds: ['stroke-1', 'stroke-7'] });
  assert.deepEqual({ code: stale.code, ids: stale.ids }, { code: 'stale_selection', ids: ['stroke-7'] });
  assert.equal(h.state.sent.length, 0);
  const outcome = await intent.booleanApply(CASES[6][1]);
  assert.equal(outcome.ok, true);
  assert.deepEqual(outcome.selection, ['stroke-1']);
  assert.deepEqual(outcome.rejected, ['stroke-2']);
  assert.deepEqual(outcome.created, ['stroke-9']);
  assert.equal(outcome.selectionRebuilt, true);
  assert.deepEqual(intent.selection(), ['stroke-1']);
  const refused = intent.select(target, ['stroke-2']);
  assert.deepEqual({ code: refused.code, ids: refused.ids }, { code: 'stale_selection', ids: ['stroke-2'] });
  assert.deepEqual(intent.selection(), ['stroke-1'], 'a refused selection does not replace the held one');
});

test('without a stroke projection the selection is cleared rather than trusted', async () => {
  const h = createHarness({ strokes, noStrokeProjection: true });
  const intent = createGroupIntent(h.ports);
  assert.equal(intent.select(target, ['stroke-1']).code, 'unavailable');
  const outcome = await intent.flatten(CASES[8][1]);
  assert.equal(outcome.ok, true);
  assert.equal(outcome.selectionRebuilt, false);
  assert.deepEqual(outcome.selection, []);
});

test('intents serialize: the second request is pinned to the revision the first one left behind', async () => {
  const h = createHarness({ strokes });
  const intent = createGroupIntent(h.ports);
  const [a, b, c] = await Promise.all([intent.rename(CASES[4][1]), intent.setMode(CASES[3][1]), intent.removeMember(CASES[5][1])]);
  assert.deepEqual([a, b, c].map((o) => o.revision), [8, 9, 10]);
  assert.deepEqual(h.state.sent.map((r) => r.expectedRevision), [7, 8, 9]);
});

test('no legacy writer is reachable: the adapters never call the Paper/WASM/bridge group or boolean paths', async () => {
  const legacy = ['foldBooleanOp', 'booleanOp', 'flattenGroup', 'applyCombinesToStrokes', 'renderCombinesFromChildren', 'erase_at_point', 'fillMergeSameColor'];
  const hits = [];
  legacy.forEach((name) => { globalThis[name] = () => { hits.push(name); }; });
  try {
    for (const [method, input] of CASES.map((c) => [c[0], c[1]])) await createGroupIntent(createHarness({ strokes }).ports)[method](input);
  } finally { legacy.forEach((name) => { delete globalThis[name]; }); }
  assert.deepEqual(hits, []);
  const dir = path.join(__dirname, '..', 'src', 'js', 'adapters');
  for (const file of ['native-object-group-intent.js', 'native-object-edit-intent-core.js', 'native-object-edit-contract.js']) {
    const source = fs.readFileSync(path.join(dir, file), 'utf8');
    assert.equal(new RegExp('\\b(?:' + legacy.join('|') + ')\\s*\\(').test(source), false, file);
    assert.equal(/(?<![\w.])(?:Paper|paper|SM|window|document|localStorage)\./.test(source), false, file + ' has no host or Paper dependency');
  }
});

test('the session mapping is typed unavailability until the session offers its pinned object mutation entry', async () => {
  const h = createHarness({ strokes });
  const state = { active: true, entry: false };
  const session = {
    isActive: () => state.active, info: () => h.ports.pin(), requestId: (label) => h.ports.requestId(label),
    get editObject() { return state.entry ? (request, pin) => h.ports.submit(request, pin) : undefined; },
  };
  const intent = createGroupIntent(portsFromSession(session, { strokeIds: h.ports.strokeIds }));
  const absent = await intent.group(CASES[0][1]);
  assert.deepEqual({ ok: absent.ok, code: absent.code, reason: absent.reason }, { ok: false, code: 'unavailable', reason: 'native_declaration_absent' });
  assert.equal(h.state.sent.length, 0);
  state.entry = true;
  const sent = await intent.group(CASES[0][1]);
  assert.equal(sent.ok, true);
  assert.equal(h.state.sent[0].requestId, 'group-create-1');
  assert.deepEqual(h.state.sent[0].payload, CASES[0][4]);
  state.active = false;
  const idle = await intent.rename(CASES[4][1]);
  assert.deepEqual({ code: idle.code, reason: idle.reason }, { code: 'unavailable', reason: 'native_declaration_absent' });
  assert.equal(h.state.sent.length, 1);
  assert.equal(portsFromSession(null).pin(), null);
  assert.equal(portsFromSession(session).strokeIds, undefined);
});

test('an un-awaited second shape intent is judged against the projection its predecessor left', async () => {
  const h = createHarness({ strokes, step: (r) => (r.operation === P + 'group.ungroup' ? { strokes: { 'layer-a': ['stroke-1'] } } : { applied: true }) });
  const intent = createGroupIntent(h.ports);
  const [first, second] = await Promise.all([intent.ungroup(CASES[1][1]), intent.combine(CASES[2][1])]);
  assert.equal(first.ok, true);
  assert.deepEqual({ code: second.code, ids: second.ids }, { code: 'stale_selection', ids: ['stroke-2'] });
  assert.equal(h.state.sent.length, 1);
});

test('the 4096-byte envelope bound is exact: 4096 is sent, 4097 is refused locally', async () => {
  // 36 unique identities of 80..120 bytes; `extra` characters of padding are spread over them (each stays <= 128 bytes).
  const ids = (extra) => Array.from({ length: 36 }, (_, i) => 's' + String(i).padStart(3, '0') + 'x'.repeat(76 + Math.max(0, Math.min(40, extra - 40 * i))));
  const size = (list) => Buffer.byteLength(JSON.stringify(request(P + 'group.create', { command: 'object.group.create', ...target, groupId: 'g', name: 'n', strokeIds: list }, 'group-create-1')));
  const base = size(ids(0)), exact = ids(4096 - base);
  assert.equal(size(exact), 4096);
  const ok = createHarness({ strokes: { 'layer-a': exact } });
  assert.equal((await createGroupIntent(ok.ports).group({ ...target, groupId: 'g', name: 'n', strokeIds: exact })).ok, true);
  assert.equal(ok.state.sent.length, 1);
  const over = ids(4097 - base);
  assert.equal(size(over), 4097);
  const refused = createHarness({ strokes: { 'layer-a': over } });
  const outcome = await createGroupIntent(refused.ports).group({ ...target, groupId: 'g', name: 'n', strokeIds: over });
  assert.deepEqual({ code: outcome.code, limit: outcome.limit }, { code: 'request_too_large', limit: 4096 });
  assert.equal(refused.state.sent.length, 0);
});

test('a reply from a port that bypasses the SDK must echo the request id, else the outcome is indeterminate', async () => {
  const reply = (requestId) => ({ apiVersion: 2, requestId, instanceId: 'instance-A', documentId: 'document-A', contentRevision: 8, ok: true, result: { applied: true, historyEntriesAdded: 1 } });
  const foreign = createHarness({ strokes, rawResponse: () => reply('someone-else-1') });
  const outcome = await createGroupIntent(foreign.ports).rename(CASES[4][1]);
  assert.deepEqual({ ok: outcome.ok, code: outcome.code, indeterminate: outcome.indeterminate }, { ok: false, code: 'transport_failed', indeterminate: true });
  const own = createHarness({ strokes, rawResponse: (req) => reply(req.requestId) });
  assert.equal((await createGroupIntent(own.ports).rename(CASES[4][1])).ok, true);
});
