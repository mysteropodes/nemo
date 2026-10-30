'use strict';
// #2376 G26: the history read and the multi-step undo/redo in the JS contract and in the project reads. Exact request bodies, the
// closed answer shape, and every way the answer can be refused or dropped: a page that does not answer its request, a pin the project left.
const test = require('node:test'), assert = require('node:assert/strict');
const Contract = require('../src/js/adapters/native-history-read-contract.js');
const Application = require('../src/js/adapters/native-application.js');
const Reads = require('../src/js/adapters/native-object-project-reads.js');

const SNAP = 'native-object:document-A:7';
const PIN = { instanceId: 'instance-A', documentId: 'document-A', lifecycleGeneration: 3, contentRevision: 7, documentSnapshotId: SNAP };
const entries = (from, n) => Array.from({ length: n }, (_, k) => ({ index: from + k, label: k % 2 ? 'Rename Layer' : 'Property: Opacity' }));
const answer = (request, over) => Object.assign({ apiVersion: 2, requestId: request.requestId, instanceId: request.instanceId, documentId: request.documentId, contentRevision: 7, ok: true,
  result: { atRevision: 7, documentSnapshotId: SNAP, undoDepth: 3, redoDepth: 2, offset: 0, entries: entries(0, 5) } }, over || {});
const code = async (promise) => { try { await promise; } catch (error) { return error; } assert.fail('expected a refusal'); };

function fixture(options) {
  options = options || {};
  const state = { info: Object.assign({}, PIN), ticket: 1, active: true, opening: false, pending: 0, synchronizing: false, projectionReady: true, presented: null };
  const sent = [];
  const ports = { state: () => state, settled: async () => { if (options.settle) options.settle(state); }, requestId: (kind) => { assert.equal(kind, 'history-read'); return 'read-' + (sent.length + 1); },
    async dispatch(request) { sent.push(JSON.parse(JSON.stringify(request))); return options.dispatch ? options.dispatch(request, state) : answer(request); } };
  return { reads: Reads.create(ports), state, sent };
}

test('the read is one pinned request with no expectedRevision and returns the page frozen', async () => {
  const f = fixture();
  const page = await f.reads.readHistory(PIN);
  assert.deepEqual(f.sent, [{ apiVersion: 2, requestId: 'read-1', instanceId: 'instance-A', documentId: 'document-A', operation: 'query.document.object.history',
    payload: { atRevision: 7, documentSnapshotId: SNAP } }]);
  assert.equal(page.undoDepth, 3);
  assert.equal(page.redoDepth, 2);
  assert.equal(page.entries.length, 5);
  assert.deepEqual(page.info, PIN);
  assert.ok(Object.isFrozen(page) && Object.isFrozen(page.entries));
  await f.reads.readHistory(PIN, 12);
  assert.deepEqual(f.sent[1].payload, { atRevision: 7, documentSnapshotId: SNAP, offset: 12 });
});

test('a read is refused when the pin is invalid, moved before it was asked, or moved while it was in flight', async () => {
  const f = fixture();
  assert.equal((await code(f.reads.readHistory({ instanceId: 'x' }))).code, 'invalid_request');
  assert.equal((await code(f.reads.readHistory(PIN, -1))).code, 'invalid_request');
  assert.equal(f.sent.length, 0);
  const before = fixture({ settle: (state) => { state.info.contentRevision = 8; } });
  assert.equal((await code(before.reads.readHistory(PIN))).code, 'stale_revision');
  assert.equal(before.sent.length, 0, 'a moved project is not asked');
  const during = fixture({ dispatch: (request, state) => { state.info.contentRevision = 8; return answer(request); } });
  assert.equal((await code(during.reads.readHistory(PIN))).code, 'stale_revision', 'a late answer is dropped');
  const busy = fixture();
  busy.state.pending = 1;
  assert.equal((await code(busy.reads.readHistory(PIN))).code, 'stale_revision');
});

test('an uncorrelated, refused or wrong-revision answer is never returned', async () => {
  const cases = [
    (request) => answer(request, { requestId: 'other' }),
    (request) => answer(request, { documentId: 'document-B' }),
    (request) => Object.assign(answer(request), { result: { atRevision: 8, documentSnapshotId: SNAP, undoDepth: 0, redoDepth: 0, offset: 0, entries: [] } }),
    (request) => Object.assign(answer(request), { result: { atRevision: 7 } }),
  ];
  for (const dispatch of cases) assert.equal((await code(fixture({ dispatch }).reads.readHistory(PIN))).code, 'uncorrelated');
  const refused = fixture({ dispatch: (request) => ({ apiVersion: 2, requestId: request.requestId, instanceId: request.instanceId, documentId: request.documentId, contentRevision: 7, ok: false,
    error: { code: 'stale_revision', message: 'Current native history read refused.' } }) });
  assert.equal((await code(refused.reads.readHistory(PIN))).code, 'stale_revision');
});

test('the contract admits only the closed request and a page that answers it', () => {
  const ok = { atRevision: 7, documentSnapshotId: SNAP };
  assert.doesNotThrow(() => Contract.payload(Contract.OP, ok));
  assert.doesNotThrow(() => Contract.payload(Contract.OP, Object.assign({ offset: 4, limit: 32 }, ok)));
  for (const bad of [{ limit: 0 }, { limit: 33 }, { limit: 1.5 }, { offset: -1 }, { offset: '1' }, { extra: 1 }, { atRevision: -1 }, { documentSnapshotId: '' }])
    assert.throws(() => Contract.payload(Contract.OP, Object.assign({}, ok, bad)), TypeError, JSON.stringify(bad));
  const page = (over) => Object.assign({ atRevision: 7, documentSnapshotId: SNAP, undoDepth: 3, redoDepth: 2, offset: 0, entries: entries(0, 5) }, over);
  assert.doesNotThrow(() => Contract.result(Contract.OP, page()));
  for (const bad of [{ entries: entries(1, 5) }, { entries: entries(0, 6) }, { entries: [{ index: 0, label: '' }] }, { entries: [{ index: 0, label: 'x'.repeat(33) }] },
    { entries: [{ index: 0, label: 'a', extra: 1 }] }, { entries: entries(0, 33), undoDepth: 40 }, { offset: -1 }, { extra: 1 }, { undoDepth: 0.5 }])
    assert.throws(() => Contract.result(Contract.OP, page(bad)), TypeError, JSON.stringify(bad));
  assert.throws(() => Contract.result(Contract.OP, page({ offset: 1 }), { offset: 0 }), TypeError, 'the answer must be the requested offset');
  assert.throws(() => Contract.result(Contract.OP, page(), { limit: 2 }), TypeError, 'more entries than the requested limit');
});

test('the application adapter admits the read without an expected revision and the undo/redo step count only as a positive integer', () => {
  const base = { apiVersion: 2, requestId: 'r1', instanceId: 'instance-A', documentId: 'document-A' };
  const read = Object.assign({ operation: 'query.document.object.history', payload: { atRevision: 7, documentSnapshotId: SNAP } }, base);
  assert.doesNotThrow(() => Application.validateRequest(read));
  assert.throws(() => Application.validateRequest(Object.assign({ expectedRevision: 7 }, read)), TypeError);
  for (const op of ['undo', 'redo']) {
    const jump = (payload) => Object.assign({ operation: 'command.document.object.' + op, expectedRevision: 7, payload }, base);
    assert.doesNotThrow(() => Application.validateRequest(jump({ command: 'object.' + op })));
    assert.doesNotThrow(() => Application.validateRequest(jump({ command: 'object.' + op, steps: 5 })));
    for (const steps of [0, -1, 1.5, '2', null]) assert.throws(() => Application.validateRequest(jump({ command: 'object.' + op, steps })), TypeError, String(steps));
    assert.throws(() => Application.validateRequest(jump({ command: 'object.' + op, count: 2 })), TypeError);
  }
  const response = (over) => Object.assign({ apiVersion: 2, requestId: 'r1', instanceId: 'instance-A', documentId: 'document-A', contentRevision: 7, ok: true,
    result: { atRevision: 7, documentSnapshotId: SNAP, undoDepth: 1, redoDepth: 0, offset: 0, entries: [{ index: 0, label: 'Draw' }] } }, over);
  assert.doesNotThrow(() => Application.validateResponse(response(), 'r1', 'query.document.object.history'));
  assert.throws(() => Application.validateResponse(response({ result: { atRevision: 7, documentSnapshotId: SNAP, undoDepth: 1, redoDepth: 0 } }), 'r1', 'query.document.object.history'), TypeError);
});
