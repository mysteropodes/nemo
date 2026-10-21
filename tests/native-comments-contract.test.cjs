'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const C = require('../src/js/adapters/native-comments-contract.js');
const draft = () => ({ id: '', x: -0.25, y: 0.5, frame: -1.5, text: 'note', createdAt: -2.25, resolved: false });
function query(selector) { return { apiVersion: 2, requestId: 'read', instanceId: 'i', documentId: 'd', operation: C.READ,
  payload: { atRevision: 7, documentSnapshotId: 's7', contextId: 'scene-root', selector } }; }
function reply(q, result) { return { apiVersion: 2, requestId: q.requestId, instanceId: 'i', documentId: 'd', contentRevision: 7, ok: true,
  result: { instanceId: 'i', documentId: 'd', atRevision: 7, documentSnapshotId: 's7', contextId: 'scene-root', result } }; }
test('original append fields and blank guard differ from verbatim existing Save', () => {
  const raw = { ...draft(), authorId: '\0raw', authorName: '', authorColor: 'unknown' };
  assert.deepEqual(C.action({ kind: 'append', comment: raw }).comment, raw);
  for (const key of ['id', 'x', 'y', 'frame', 'text', 'createdAt', 'resolved']) {
    const value = draft(); delete value[key]; assert.throws(() => C.action({ kind: 'append', comment: value }));
  }
  for (const text of ['', ' \n', '\uFEFF\u00A0']) assert.throws(() => C.action({ kind: 'append', comment: { ...draft(), text } }));
  C.action({ kind: 'append', comment: { ...draft(), text: '\u0085' } });
  for (const text of ['', ' \n', '\uFEFF']) assert.equal(C.action({ kind: 'update', ordinal: 2, text, resolved: true }).text, text);
  for (const key of Object.keys(raw)) assert.throws(() => C.comment({ [key]: null }, false));
  for (const value of [Infinity, -Infinity, NaN]) assert.throws(() => C.comment({ x: value }, false));
  for (const value of [{ kind: 'delete', id: 'same' }, { kind: 'delete', ordinal: 0 }, { kind: 'resolve', ordinal: 1 },
    { kind: 'append', comment: { ...raw, screenshotDataUrls: [] } }]) assert.throws(() => C.action(value));
  assert.throws(() => C.comment({ text: '\uD800' }, false));
});
test('pinned whole raw record pages preserve duplicate partial identities and exact progress', () => {
  const q = query({ kind: 'commentsPage', cursor: 0, limit: 16 });
  const result = { kind: 'projectCommentsPage', commentsPresent: true, commentCount: 3, cursor: 0,
    items: [{ ordinal: 1, comment: {} }, { ordinal: 2, comment: { id: 'same', text: '' } }], nextCursor: 2 };
  C.payload(C.READ, q.payload); C.correlate(q, reply(q, result));
  for (const patch of [{ nextCursor: 1 }, { items: [], nextCursor: 0 }, { commentsPresent: false }, { cursor: 1 }])
    assert.throws(() => C.correlate(q, reply(q, { ...result, ...patch })));
  const record = query({ kind: 'record', ordinal: 2 });
  C.correlate(record, reply(record, { kind: 'projectCommentRecord', ordinal: 2, comment: { id: 'same', resolved: false } }));
  assert.throws(() => C.correlate(record, reply(record, { kind: 'projectCommentRecord', ordinal: 1, comment: {} })));
  const settings = query({ kind: 'settings' }); C.correlate(settings, reply(settings, { commentsPresent: false, commentCount: 0 }));
  assert.throws(() => C.correlate(settings, reply(settings, { commentsPresent: false, commentCount: 1 })));
  const response = reply(record, { kind: 'projectCommentRecord', ordinal: 2, comment: {} });
  for (const [key, value] of [['requestId', 'other'], ['instanceId', 'other'], ['contentRevision', 8]])
    assert.throws(() => C.correlate(record, { ...response, [key]: value }));
  assert.throws(() => C.action({ kind: 'update', ordinal: 1, get text() { throw Error('must not read getter'); }, resolved: false }));
});
test('actual SDK admits the closed comment lane and rejects request bytes before transport', async () => {
  const SDK = require('../src/js/adapters/native-application.js'); let calls = 0;
  const adapter = SDK.createNativeApplicationAdapter('comments-contract', { dispatch: async q => {
    calls++; return reply(q, { commentsPresent: false, commentCount: 0 });
  } });
  const q = query({ kind: 'settings' }); assert.equal((await adapter.dispatch(q)).ok, true); assert.equal(calls, 1);
  await assert.rejects(async () => adapter.dispatch({ ...q, operation: C.EDIT, expectedRevision: 7,
    payload: { command: 'object.project.comments.edit', action: { kind: 'append', comment: { ...draft(), text: 'x'.repeat(4096) } } } }));
  assert.equal(calls, 1);
});
