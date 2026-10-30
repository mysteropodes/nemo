'use strict';
// #2255: the widget presentation read of the native project reads. Exact request bodies, one complete paged list, and every way the
// answer can be refused: a stale or foreign presentation, an uncorrelated envelope, a list that changes between pages.
const test = require('node:test'), assert = require('node:assert/strict');
const Reads = require('../src/js/adapters/native-object-project-reads.js');
const Query = require('../src/js/adapters/native-object-query-contract.js');

const PIN = { instanceId: 'instance-A', documentId: 'document-A', lifecycleGeneration: 3, contentRevision: 7, documentSnapshotId: 'native-object:document-A:7' };
const axis = (label, value) => ({ key: 'xc_' + label, min: -100, max: 100, rest: 0, label, value });
const item = (uid, over) => Object.assign({ layerUid: uid, visible: true, locked: false, kind: 'joystick', x: axis('X', 0), y: axis('Y', 0), center: [960, 540],
  angle: 0, size: [160, 160], puck: [0, 0], rest: [0, 0], unavailable: null }, over || {});

function fixture(items, options) {
  options = options || {};
  const presentation = Object.assign({ owner: 'native', status: 'presented', contextId: 'scene-root', frame: 4, workId: 'native-work-9', viewGeneration: 2 }, PIN);
  const state = { info: Object.assign({}, PIN), presented: Object.assign({ available: true, presentation }, PIN), ticket: 1, active: true, opening: false, pending: 0,
    synchronizing: false, projectionReady: true };
  const sent = [];
  const ports = {
    state: () => state, settled: async () => {}, requestId: (kind) => { assert.equal(kind, 'object-geometry-read'); return 'read-' + (sent.length + 1); },
    async dispatch(request) {
      sent.push(JSON.parse(JSON.stringify(request)));
      if (options.dispatch) return options.dispatch(request, sent.length);
      const view = request.payload.view, page = items.slice(view.cursor, view.cursor + view.limit), end = view.cursor + page.length;
      const result = { kind: 'widget', atRevision: PIN.contentRevision, documentSnapshotId: PIN.documentSnapshotId, contextId: 'scene-root', frame: view.frame,
        workId: view.workId, viewGeneration: view.viewGeneration, cursor: view.cursor, total: items.length, nextCursor: end < items.length ? end : null, items: page };
      return { apiVersion: 2, requestId: request.requestId, instanceId: request.instanceId, documentId: request.documentId, contentRevision: PIN.contentRevision, ok: true, result };
    },
  };
  return { reads: Reads.create(ports), state, sent, presentation, selector: { contextId: 'scene-root', frame: 4 } };
}
const code = async (promise) => { try { await promise; } catch (error) { return error; } assert.fail('expected a refusal'); };

test('every widget layer arrives through exact, bounded pages pinned to the presented work', async () => {
  const items = [item('L1'), item('L2', { kind: 'slider', y: null, size: [200, 26] }), item('L3', { puck: null, unavailable: 'control' })];
  const f = fixture(items);
  const read = await f.reads.readWidgets(f.selector, PIN);
  const body = (cursor) => ({ atRevision: 7, documentSnapshotId: PIN.documentSnapshotId, view: { kind: 'widget', contextId: 'scene-root', frame: 4,
    workId: 'native-work-9', viewGeneration: 2, cursor, limit: 2 } });
  assert.deepEqual(f.sent.map((r) => [r.requestId, r.operation, r.payload]), [['read-1', 'query.document.object', body(0)], ['read-2', 'query.document.object', body(2)]]);
  assert.deepEqual(read.items, items);
  assert.equal(read.presentation, f.presentation);
  assert.deepEqual(read.info, PIN);
  assert.ok(Object.isFrozen(read) && Object.isFrozen(read.items[0].x), 'the read is immutable');
  assert.deepEqual((await fixture([]).reads.readWidgets(f.selector, PIN)).items, []);
});

test('the request and the answer are closed shapes in the shared query contract', () => {
  const view = { kind: 'widget', contextId: 'scene-root', frame: 4, workId: 'native-work-9', viewGeneration: 2, cursor: 0, limit: 2 };
  const payload = { atRevision: 7, documentSnapshotId: PIN.documentSnapshotId, view };
  assert.ok(Query.payload(payload));
  for (const bad of [{ limit: 5 }, { limit: 0 }, { cursor: 1025 }, { contextId: 'component:x' }, { layerUid: 'L1' }, { frame: -1 }, { viewGeneration: 0 }])
    assert.throws(() => Query.payload(Object.assign({}, payload, { view: Object.assign({}, view, bad) })), TypeError, JSON.stringify(bad));
  const page = (over) => Object.assign({ kind: 'widget', atRevision: 7, documentSnapshotId: PIN.documentSnapshotId, contextId: 'scene-root', frame: 4, workId: 'native-work-9',
    viewGeneration: 2, cursor: 0, total: 1, nextCursor: null, items: [item('L1')] }, over);
  assert.ok(Query.correlate(payload, page(), null, 'document-A'));
  for (const bad of [{ total: 2 }, { nextCursor: 1 }, { cursor: 1 }, { frame: 5 }, { workId: 'native-work-8' }, { items: [item('L1', { puck: null })] },
    { items: [item('L1', { unavailable: 'parent', puck: null })] }, { items: [item('L1', { y: null })] }, { items: [item('L1'), item('L2'), item('L3')], total: 3 }])
    assert.throws(() => Query.correlate(payload, page(bad), null, 'document-A'), TypeError, JSON.stringify(bad).slice(0, 80));
  assert.throws(() => Query.correlate(payload, page(), null, 'another-document'), TypeError);
  assert.ok(Query.paths.has('result.items.0.layerUid') && !Query.paths.has('result.items.4.layerUid'));
});

test('a selector or a pin that is not the presented one is refused before anything is sent', async () => {
  const f = fixture([item('L1')]);
  for (const selector of [{ contextId: 'component:x', frame: 4 }, { contextId: 'scene-root', frame: -1 }, { contextId: 'scene-root', frame: 4, layerUid: 'L1' }])
    assert.equal((await code(f.reads.readWidgets(selector, PIN))).code, 'invalid_request');
  assert.equal((await code(f.reads.readWidgets({ contextId: 'scene-root', frame: 5 }, PIN))).code, 'unavailable', 'another frame than the presented one');
  assert.equal((await code(f.reads.readWidgets(f.selector, Object.assign({}, PIN, { contentRevision: 8 })))).code, 'unavailable');
  f.state.presented = null;
  assert.equal((await code(f.reads.readWidgets(f.selector, PIN))).code, 'unavailable');
  assert.deepEqual(f.sent, []);
});

test('a presentation that changes during the read, or a list that changes between pages, publishes nothing', async () => {
  const moved = fixture([item('L1'), item('L2'), item('L3')], {});
  const original = moved.state.ticket;
  moved.reads = Reads.create({ state: () => moved.state, settled: async () => {}, requestId: () => 'read-x', async dispatch(request) {
    moved.state.ticket = original + 1;
    const view = request.payload.view;
    return { apiVersion: 2, requestId: request.requestId, instanceId: request.instanceId, documentId: request.documentId, contentRevision: 7, ok: true,
      result: { kind: 'widget', atRevision: 7, documentSnapshotId: PIN.documentSnapshotId, contextId: 'scene-root', frame: 4, workId: view.workId, viewGeneration: 2,
        cursor: 0, total: 1, nextCursor: null, items: [item('L1')] } };
  } });
  assert.equal((await code(moved.reads.readWidgets(moved.selector, PIN))).code, 'stale_revision');
  const grown = fixture([], { dispatch(request, n) {
    const view = request.payload.view, total = n === 1 ? 3 : 4, page = [item('L' + view.cursor), item('L' + (view.cursor + 1))].slice(0, Math.min(2, total - view.cursor));
    const end = view.cursor + page.length;
    return { apiVersion: 2, requestId: request.requestId, instanceId: request.instanceId, documentId: request.documentId, contentRevision: 7, ok: true,
      result: { kind: 'widget', atRevision: 7, documentSnapshotId: PIN.documentSnapshotId, contextId: 'scene-root', frame: 4, workId: view.workId, viewGeneration: 2,
        cursor: view.cursor, total, nextCursor: end < total ? end : null, items: page } };
  } });
  assert.equal((await code(grown.reads.readWidgets(grown.selector, PIN))).code, 'uncorrelated');
});

test('a refused, uncorrelated or failed transport answer is a typed error, never a partial list', async () => {
  const answer = (over) => (request) => Object.assign({ apiVersion: 2, requestId: request.requestId, instanceId: request.instanceId, documentId: request.documentId, contentRevision: 7 }, over);
  const refusal = fixture([], { dispatch: answer({ ok: false, error: { code: 'stale_revision', message: 'moved', details: { reason: 'x' } } }) });
  assert.equal((await code(refusal.reads.readWidgets(refusal.selector, PIN))).code, 'stale_revision');
  const foreign = fixture([], { dispatch: (request) => Object.assign(answer({ ok: true, result: {} })(request), { requestId: 'other' }) });
  assert.equal((await code(foreign.reads.readWidgets(foreign.selector, PIN))).code, 'uncorrelated');
  const shape = fixture([], { dispatch: answer({ ok: true, result: { kind: 'widget' } }) });
  assert.equal((await code(shape.reads.readWidgets(shape.selector, PIN))).code, 'uncorrelated');
  const down = fixture([], { dispatch() { throw new Error('transport'); } });
  const error = await code(down.reads.readWidgets(down.selector, PIN));
  assert.deepEqual([error.code, error.reason], ['unavailable', 'native_read_transport']);
});
