'use strict';
// #2255: the rig presentation read of the native project reads. Exact request bodies for every part, one complete paged list per part,
// the weights of one vertex, the typed layer reason, and every way the answer can be refused.
const test = require('node:test'), assert = require('node:assert/strict');
const Reads = require('../src/js/adapters/native-object-project-reads.js');
const Query = require('../src/js/adapters/native-object-query-contract.js');

const PIN = { instanceId: 'instance-A', documentId: 'document-A', lifecycleGeneration: 3, contentRevision: 7, documentSnapshotId: 'native-object:document-A:7' };
const bone = (n) => ({ boneId: 'bone' + n, closed: false, radius: null, count: 2, center: [10 * n, 5] });
const vertex = (n) => ({ boneId: 'bone1', vi: n, point: [n, n], handleIn: [0, 0], handleOut: [1, 0] });
const LISTS = { bones: Array.from({ length: 9 }, (_, n) => bone(n + 1)), vertices: Array.from({ length: 17 }, (_, n) => vertex(n)), chains: [],
  binds: [{ index: 0, strokeId: 's1', meshId: null, points: 3, rotate: false, state: 'posed' }], bound: Array.from({ length: 65 }, (_, n) => [0, n, n, -n]),
  weights: [{ entry: 0, boneId: 'bone1', ptIndex: 2, offset: null, w: 0.5 }] };
const CAP = { bones: 8, vertices: 16, chains: 8, binds: 8, bound: 64, weights: 8 };

function fixture(options) {
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
      const view = request.payload.view, list = options.lists ? options.lists[view.part] : LISTS[view.part], page = list.slice(view.cursor, view.cursor + view.limit), end = view.cursor + page.length;
      const result = { kind: 'rig', atRevision: PIN.contentRevision, documentSnapshotId: PIN.documentSnapshotId, contextId: 'scene-root', frame: view.frame, workId: view.workId,
        viewGeneration: view.viewGeneration, layerUid: view.layerUid, part: view.part, bind: view.bind, vertex: view.vertex, cursor: view.cursor, total: list.length,
        nextCursor: end < list.length ? end : null, unavailable: options.unavailable || null, items: options.unavailable ? [] : page };
      if (options.unavailable) Object.assign(result, { total: 0, nextCursor: null });
      return { apiVersion: 2, requestId: request.requestId, instanceId: request.instanceId, documentId: request.documentId, contentRevision: PIN.contentRevision, ok: true, result };
    },
  };
  return { reads: Reads.create(ports), state, sent, presentation, selector: { contextId: 'scene-root', frame: 4, layerUid: 'L1' } };
}
const code = async (promise) => { try { await promise; } catch (error) { return error; } assert.fail('expected a refusal'); };
const body = (part, cursor, bind, vertex) => ({ atRevision: 7, documentSnapshotId: PIN.documentSnapshotId, view: { kind: 'rig', contextId: 'scene-root', frame: 4,
  workId: 'native-work-9', viewGeneration: 2, layerUid: 'L1', part, bind: bind || 0, vertex: vertex || 0, cursor, limit: CAP[part] } });

test('every part arrives through exact pages at its own cap, pinned to the presented work', async () => {
  const f = fixture();
  const read = await f.reads.readRig(f.selector, PIN, ['bones', 'vertices', 'chains', 'binds', 'bound']);
  assert.deepEqual(f.sent.map((r) => [r.requestId, r.operation, r.payload]), [
    ['read-1', 'query.document.object', body('bones', 0)], ['read-2', 'query.document.object', body('bones', 8)],
    ['read-3', 'query.document.object', body('vertices', 0)], ['read-4', 'query.document.object', body('vertices', 16)],
    ['read-5', 'query.document.object', body('chains', 0)], ['read-6', 'query.document.object', body('binds', 0)],
    ['read-7', 'query.document.object', body('bound', 0)], ['read-8', 'query.document.object', body('bound', 64)]]);
  assert.deepEqual(read.parts.bones, LISTS.bones);
  assert.equal(read.parts.vertices.length, 17);
  assert.equal(read.parts.bound.length, 65);
  assert.deepEqual([read.layerUid, read.unavailable, read.presentation === f.presentation], ['L1', null, true]);
  assert.deepEqual(read.info, PIN);
  assert.ok(Object.isFrozen(read) && Object.isFrozen(read.parts), 'the read is immutable');
});

test('the weights of one vertex of one bind are one read with that selector', async () => {
  const f = fixture();
  const read = await f.reads.readRigWeights(f.selector, PIN, 2, 5);
  assert.deepEqual(f.sent.map((r) => r.payload), [body('weights', 0, 2, 5)]);
  assert.deepEqual(read.parts.weights, LISTS.weights);
  for (const [bind, vertex] of [[-1, 0], [0, 1.5], [4294967296, 0]]) assert.equal((await code(f.reads.readRigWeights(f.selector, PIN, bind, vertex))).code, 'invalid_request');
});

test('a layer reason replaces every list and stops the later parts', async () => {
  const f = fixture({ unavailable: 'motion' });
  const read = await f.reads.readRig(f.selector, PIN, ['bones', 'vertices', 'binds']);
  assert.equal(read.unavailable, 'motion');
  assert.deepEqual(read.parts, { bones: [] });
  assert.equal(f.sent.length, 1, 'the next parts are not asked for once the layer is unavailable');
});

test('the request and the answer are closed shapes in the shared query contract', () => {
  const payload = body('weights', 0, 1, 2);
  assert.ok(Query.payload(payload));
  const bad = (change) => Object.assign({}, payload, { view: Object.assign({}, payload.view, change) });
  for (const change of [{ limit: 9 }, { limit: 0 }, { cursor: 100001 }, { contextId: 'component:x' }, { layerUid: '' }, { frame: -1 }, { viewGeneration: 0 }, { part: 'shapes' }, { extra: 1 }])
    assert.throws(() => Query.payload(bad(change)), TypeError, JSON.stringify(change));
  assert.throws(() => Query.payload(Object.assign({}, body('bones', 0), { view: Object.assign({}, body('bones', 0).view, { bind: 1 }) })), TypeError, 'a selector on a part that takes none');
  const page = (over) => Object.assign({ kind: 'rig', atRevision: 7, documentSnapshotId: PIN.documentSnapshotId, contextId: 'scene-root', frame: 4, workId: 'native-work-9',
    viewGeneration: 2, layerUid: 'L1', part: 'weights', bind: 1, vertex: 2, cursor: 0, total: 1, nextCursor: null, unavailable: null, items: LISTS.weights }, over);
  assert.ok(Query.correlate(payload, page(), null, 'document-A'));
  for (const change of [{ total: 2 }, { nextCursor: 1 }, { cursor: 1 }, { frame: 5 }, { layerUid: 'L2' }, { part: 'bones' }, { bind: 0 }, { workId: 'native-work-8' }, { unavailable: 'guess' },
    { unavailable: 'motion' }, { items: [{ entry: 0, boneId: 'bone1', ptIndex: 2, offset: 1, w: 0.5 }] }, { items: [{ entry: 0, boneId: 'bone1', ptIndex: null, offset: null, w: 0.5 }] },
    { items: [Object.assign({ extra: 1 }, LISTS.weights[0])] }])
    assert.throws(() => Query.correlate(payload, page(change), null, 'document-A'), TypeError, JSON.stringify(change).slice(0, 80));
  assert.throws(() => Query.correlate(payload, page(), null, 'another-document'), TypeError);
  assert.ok(Query.correlate(body('bones', 0), page({ part: 'bones', bind: 0, vertex: 0, total: 0, items: [], unavailable: 'parent' }), null, 'document-A'));
  const shapes = { bones: bone(1), vertices: vertex(0), chains: { endKey: 'bone2:1', root: { boneId: 'bone1', vi: 0 }, joint: { boneId: 'bone1', vi: 1 }, end: { boneId: 'bone2', vi: 1 }, l1: 1, l2: 2, flip: false },
    binds: LISTS.binds[0], bound: [0, 1, 2, 3] };
  for (const [part, item] of Object.entries(shapes)) assert.ok(Query.correlate(body(part, 0), page({ part, bind: 0, vertex: 0, items: [item] }), null, 'document-A'), part);
  assert.throws(() => Query.correlate(body('bones', 0), page({ part: 'bones', bind: 0, vertex: 0, items: [vertex(0)] }), null, 'document-A'), TypeError, 'an item of another part');
  assert.ok(Query.paths.has('result.items.0.boneId') && Query.paths.has('result.items.3.root.boneId') && !Query.paths.has('result.items.3.l1'));
});

test('a selector or a pin that is not the presented one is refused before anything is sent', async () => {
  const f = fixture();
  for (const selector of [{ contextId: 'component:x', frame: 4, layerUid: 'L1' }, { contextId: 'scene-root', frame: -1, layerUid: 'L1' }, { contextId: 'scene-root', frame: 4 },
    { contextId: 'scene-root', frame: 4, layerUid: '' }, { contextId: 'scene-root', frame: 4, layerUid: 'L1', extra: 1 }])
    assert.equal((await code(f.reads.readRig(selector, PIN, ['bones']))).code, 'invalid_request');
  for (const parts of [[], ['shapes'], 'bones']) assert.equal((await code(f.reads.readRig(f.selector, PIN, parts))).code, 'invalid_request');
  assert.equal((await code(f.reads.readRig(Object.assign({}, f.selector, { frame: 5 }), PIN, ['bones']))).code, 'unavailable', 'another frame than the presented one');
  assert.equal((await code(f.reads.readRig(f.selector, Object.assign({}, PIN, { contentRevision: 8 }), ['bones']))).code, 'unavailable');
  f.state.presented = null;
  assert.equal((await code(f.reads.readRig(f.selector, PIN, ['bones']))).code, 'unavailable');
  assert.deepEqual(f.sent, []);
});

test('a list that changes between pages, a presentation that changes during the read and a bad answer publish nothing', async () => {
  const grown = fixture({ dispatch(request, n) {
    const view = request.payload.view, total = n === 1 ? 10 : 11, page = LISTS.bones.slice(view.cursor, view.cursor + 8), end = view.cursor + page.length;
    return { apiVersion: 2, requestId: request.requestId, instanceId: request.instanceId, documentId: request.documentId, contentRevision: 7, ok: true,
      result: { kind: 'rig', atRevision: 7, documentSnapshotId: PIN.documentSnapshotId, contextId: 'scene-root', frame: 4, workId: view.workId, viewGeneration: 2, layerUid: 'L1',
        part: 'bones', bind: 0, vertex: 0, cursor: view.cursor, total, nextCursor: end < total ? end : null, unavailable: null, items: page } };
  } });
  assert.equal((await code(grown.reads.readRig(grown.selector, PIN, ['bones']))).code, 'uncorrelated');
  const answer = (over) => (request) => Object.assign({ apiVersion: 2, requestId: request.requestId, instanceId: request.instanceId, documentId: request.documentId, contentRevision: 7 }, over);
  const refusal = fixture({ dispatch: answer({ ok: false, error: { code: 'stale_revision', message: 'moved', details: { reason: 'x' } } }) });
  assert.equal((await code(refusal.reads.readRig(refusal.selector, PIN, ['bones']))).code, 'stale_revision');
  const shape = fixture({ dispatch: answer({ ok: true, result: { kind: 'rig' } }) });
  assert.equal((await code(shape.reads.readRig(shape.selector, PIN, ['bones']))).code, 'uncorrelated');
  const down = fixture({ dispatch() { throw new Error('transport'); } });
  const error = await code(down.reads.readRig(down.selector, PIN, ['bones']));
  assert.deepEqual([error.code, error.reason], ['unavailable', 'native_read_transport']);
  const moved = fixture({});
  moved.state.ticket = 1;
  const reads = Reads.create({ state: () => moved.state, settled: async () => {}, requestId: () => 'read-x', async dispatch(request) {
    moved.state.ticket = 2;
    const view = request.payload.view;
    return { apiVersion: 2, requestId: request.requestId, instanceId: request.instanceId, documentId: request.documentId, contentRevision: 7, ok: true,
      result: { kind: 'rig', atRevision: 7, documentSnapshotId: PIN.documentSnapshotId, contextId: 'scene-root', frame: 4, workId: view.workId, viewGeneration: 2, layerUid: 'L1',
        part: 'bones', bind: 0, vertex: 0, cursor: 0, total: 0, nextCursor: null, unavailable: null, items: [] } };
  } });
  assert.equal((await code(reads.readRig(moved.selector, PIN, ['bones']))).code, 'stale_revision');
});
