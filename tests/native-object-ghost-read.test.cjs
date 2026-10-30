'use strict';
// #2358 D23: the onion / Ghost All read of the native project reads. Exact request bodies, one complete paged assembly of the ghost
// frames, and every way the answer can be refused or dropped: a stale or foreign presentation, an uncorrelated page, a sequence that
// is out of order or incomplete.
const test = require('node:test'), assert = require('node:assert/strict');
const Reads = require('../src/js/adapters/native-object-project-reads.js');
const Query = require('../src/js/adapters/native-object-query-contract.js');

const PIN = { instanceId: 'instance-A', documentId: 'document-A', lifecycleGeneration: 3, contentRevision: 7, documentSnapshotId: 'native-object:document-A:7' };
const SELECTOR = { contextId: 'scene-root', frame: 4, layerUid: 'L1', onion: true, ghostAll: true, from: 2, to: 6, previousOpacity: 30, nextOpacity: 40, mode: 'tinted',
  previousTint: '#aabbcc', shift: { x: 4, y: -3, next: true } };
const seg = (n) => (n % 2 ? [n, 1, 0.5, 0, -0.5, 0] : [n, 1]);
// Ghost frames: [descriptor, [path segment counts]]; the fake owner pages every ghost frame in pieces of at most `piece` segments.
const GHOSTS = [{ frame: 0, kind: 'allKeys', opacity: 0.34, translation: [0, 0], paths: [5, 2] }, { frame: 3, kind: 'previous', opacity: 0.3, translation: [4, -3], paths: [3] },
  { frame: 5, kind: 'next', opacity: 0.2, translation: [0, 0], paths: [] }];

function fixture(options) {
  options = options || {};
  const presentation = Object.assign({ owner: 'native', status: 'presented', contextId: 'scene-root', frame: 4, workId: 'native-work-9', viewGeneration: 2 }, PIN);
  const state = { info: Object.assign({}, PIN), presented: Object.assign({ available: true, presentation }, PIN), ticket: 1, active: true, opening: false, pending: 0,
    synchronizing: false, projectionReady: true };
  const sent = [], piece = options.piece || 3, ghosts = options.ghosts || GHOSTS;
  const ports = {
    state: () => state, settled: async () => {}, requestId: (kind) => { assert.equal(kind, 'object-geometry-read'); return 'read-' + (sent.length + 1); },
    async dispatch(request) {
      sent.push(JSON.parse(JSON.stringify(request)));
      if (options.dispatch) return options.dispatch(request, sent.length, state);
      const view = request.payload.view, g = ghosts[view.ghost], total = g.paths.reduce((a, b) => a + b, 0), end = Math.min(total, view.cursor + piece);
      const items = []; let at = 0;
      g.paths.forEach((n, path) => {
        const lo = Math.max(at, view.cursor), hi = Math.min(at + n, end);
        if (lo < hi) items.push({ path, from: lo - at, count: n, sourceId: 's' + path, closed: false, width: 2, fill: null, stroke: '#ff4d4d4d', opacity: g.opacity,
          segments: Array.from({ length: hi - lo }, (_, k) => seg(lo - at + k)) });
        at += n;
      });
      const next = end < total ? { ghost: view.ghost, cursor: end } : view.ghost + 1 < ghosts.length ? { ghost: view.ghost + 1, cursor: 0 } : null;
      const result = { kind: 'ghosts', atRevision: 7, documentSnapshotId: PIN.documentSnapshotId, contextId: 'scene-root', frame: view.frame, workId: view.workId,
        viewGeneration: view.viewGeneration, layerUid: view.layerUid, ghost: { index: view.ghost, count: ghosts.length, frame: g.frame, kind: g.kind, opacity: g.opacity,
          translation: g.translation }, cursor: view.cursor, total, next, items };
      if (options.mutate) options.mutate(result, sent.length);
      const reply = { apiVersion: 2, requestId: request.requestId, instanceId: request.instanceId, documentId: request.documentId, contentRevision: 7, ok: true, result };
      if (options.after) options.after(sent.length, state);
      return reply;
    },
  };
  return { reads: Reads.create(ports), state, sent, presentation };
}
const code = async (promise) => { try { await promise; } catch (error) { return error; } assert.fail('expected a refusal'); };

test('every ghost frame arrives through exact pages pinned to the presented work and assembles into whole paths', async () => {
  const f = fixture();
  const read = await f.reads.readGhosts(SELECTOR, PIN);
  assert.deepEqual(f.sent[0].payload, { atRevision: 7, documentSnapshotId: PIN.documentSnapshotId, view: Object.assign({ kind: 'ghosts', workId: 'native-work-9', viewGeneration: 2,
    ghost: 0, cursor: 0 }, SELECTOR) });
  assert.deepEqual(f.sent.map((r) => [r.payload.view.ghost, r.payload.view.cursor]), [[0, 0], [0, 3], [0, 6], [1, 0], [2, 0]]);
  assert.deepEqual(read.frames.map((g) => [g.frame, g.kind, g.paths.map((p) => p.segments.length)]), [[0, 'allKeys', [5, 2]], [3, 'previous', [3]], [5, 'next', []]]);
  assert.deepEqual(read.frames[0].paths[0].segments, [0, 1, 2, 3, 4].map(seg), 'a path split across pages is whole and in order');
  assert.deepEqual(read.frames[1].translation, [4, -3]);
  assert.equal(read.layerUid, 'L1');
  assert.equal(read.presentation, f.presentation);
  assert.deepEqual(read.info, PIN);
  assert.ok(Object.isFrozen(read));
});

test('an answer for no ghost frame at all is one empty page', async () => {
  const f = fixture({ dispatch: (request) => ({ apiVersion: 2, requestId: request.requestId, instanceId: request.instanceId, documentId: request.documentId, contentRevision: 7, ok: true,
    result: { kind: 'ghosts', atRevision: 7, documentSnapshotId: PIN.documentSnapshotId, contextId: 'scene-root', frame: 4, workId: request.payload.view.workId, viewGeneration: 2,
      layerUid: 'L1', ghost: null, cursor: 0, total: 0, next: null, items: [] } }) });
  assert.deepEqual((await f.reads.readGhosts(SELECTOR, PIN)).frames, []);
  assert.equal(f.sent.length, 1);
});

test('the request and the answer are closed shapes in the shared query contract', () => {
  const view = Object.assign({ kind: 'ghosts', workId: 'native-work-9', viewGeneration: 2, ghost: 1, cursor: 0 }, SELECTOR);
  const payload = { atRevision: 7, documentSnapshotId: PIN.documentSnapshotId, view };
  assert.ok(Query.payload(payload));
  for (const bad of [{ limit: 4 }, { mode: 'outlines' }, { previousOpacity: 101 }, { previousTint: 'red' }, { shift: { x: 1, y: 2 } }, { ghost: 4096 }, { cursor: 262145 }, { contextId: 'component:x' },
    { frame: -1 }, { viewGeneration: 0 }, { onion: 1 }, { layerUid: '' }])
    assert.throws(() => Query.payload(Object.assign({}, payload, { view: Object.assign({}, view, bad) })), TypeError, JSON.stringify(bad));
  const item = (over) => Object.assign({ path: 0, from: 0, count: 2, sourceId: 's', closed: false, width: 2, fill: null, stroke: '#ff4d4d4d', opacity: 0.3, segments: [[0, 1], [2, 1, 0, 0, 1, 1]] }, over);
  const page = (over) => Object.assign({ kind: 'ghosts', atRevision: 7, documentSnapshotId: PIN.documentSnapshotId, contextId: 'scene-root', frame: 4, workId: 'native-work-9',
    viewGeneration: 2, layerUid: 'L1', ghost: { index: 1, count: 3, frame: 3, kind: 'previous', opacity: 0.3, translation: [4, -3] }, cursor: 0, total: 2, next: { ghost: 2, cursor: 0 }, items: [item()] }, over);
  assert.ok(Query.correlate(payload, page(), null, 'document-A'));
  for (const bad of [{ total: 3 }, { next: null }, { next: { ghost: 1, cursor: 2 } }, { cursor: 1 }, { frame: 5 }, { workId: 'native-work-8' }, { layerUid: 'L2' }, { items: [item({ segments: [[0, 1]] })] },
    { items: [item({ stroke: 'red' })] }, { items: [item({ opacity: 2 })] }, { items: [item({ segments: [[0, 1, 2]] })] }, { items: [item({ from: 1 })] },
    { ghost: { index: 0, count: 3, frame: 3, kind: 'previous', opacity: 0.3, translation: [4, -3] } }, { ghost: null }])
    assert.throws(() => Query.correlate(payload, page(bad), null, 'document-A'), TypeError, JSON.stringify(bad).slice(0, 80));
  assert.throws(() => Query.correlate(payload, page(), null, 'another-document'), TypeError);
  assert.ok(Query.paths.has('result.items.5.sourceId'));
});

test('a selector or a pin that is not the presented one is refused before anything is sent', async () => {
  const f = fixture();
  for (const selector of [Object.assign({}, SELECTOR, { contextId: 'component:x' }), Object.assign({}, SELECTOR, { limit: 4 }), { contextId: 'scene-root', frame: 4 }])
    assert.equal((await code(f.reads.readGhosts(selector, PIN))).code, 'invalid_request');
  assert.equal((await code(f.reads.readGhosts(Object.assign({}, SELECTOR, { frame: 5 }), PIN))).code, 'unavailable', 'another frame than the presented one');
  assert.equal((await code(f.reads.readGhosts(SELECTOR, Object.assign({}, PIN, { contentRevision: 8 })))).code, 'unavailable');
  f.state.presented = null;
  assert.equal((await code(f.reads.readGhosts(SELECTOR, PIN))).code, 'unavailable');
  assert.deepEqual(f.sent, []);
});

test('a presentation that moves mid-sequence drops the whole read, and an out-of-order or stuck page publishes nothing', async () => {
  const moved = fixture({ after: (n, state) => { state.ticket = 2; } });
  assert.equal((await code(moved.reads.readGhosts(SELECTOR, PIN))).code, 'stale_revision');
  assert.equal(moved.sent.length, 1, 'nothing more is asked once the presentation moved');
  const stuck = fixture({ mutate: (result, n) => { if (n === 1) { result.items = []; result.next = { ghost: 0, cursor: 0 }; } } });
  const error = await code(stuck.reads.readGhosts(SELECTOR, PIN));
  assert.deepEqual([error.code, error.reason], ['uncorrelated', 'ghost_page_stale']);
  const resized = fixture({ mutate: (result, n) => { if (n === 2) result.items[0].count = 6; } });
  const bad = await code(resized.reads.readGhosts(SELECTOR, PIN));
  assert.deepEqual([bad.code, bad.reason], ['uncorrelated', 'ghost_page_order']);
});

test('a refused, uncorrelated or failed transport answer is a typed error, never a partial list', async () => {
  const answer = (over) => (request) => Object.assign({ apiVersion: 2, requestId: request.requestId, instanceId: request.instanceId, documentId: request.documentId, contentRevision: 7 }, over);
  const refusal = fixture({ dispatch: answer({ ok: false, error: { code: 'stale_revision', message: 'moved', details: { reason: 'x' } } }) });
  assert.equal((await code(refusal.reads.readGhosts(SELECTOR, PIN))).code, 'stale_revision');
  const foreign = fixture({ dispatch: (request) => Object.assign(answer({ ok: true, result: {} })(request), { requestId: 'other' }) });
  assert.equal((await code(foreign.reads.readGhosts(SELECTOR, PIN))).code, 'uncorrelated');
  const shape = fixture({ dispatch: answer({ ok: true, result: { kind: 'ghosts' } }) });
  assert.equal((await code(shape.reads.readGhosts(SELECTOR, PIN))).code, 'uncorrelated');
  const down = fixture({ dispatch() { throw new Error('transport'); } });
  const error = await code(down.reads.readGhosts(SELECTOR, PIN));
  assert.deepEqual([error.code, error.reason], ['unavailable', 'native_read_transport']);
});
