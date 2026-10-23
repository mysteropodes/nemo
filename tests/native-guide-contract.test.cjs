'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const C = require('../src/js/adapters/native-guide-contract.js');
function request(selector) {
  return { apiVersion: 2, requestId: 'guide-read', instanceId: 'i', documentId: 'd', operation: C.READ,
    payload: { atRevision: 3, documentSnapshotId: 's3', contextId: 'scene-root', selector } };
}
function reply(q, value) {
  return { apiVersion: 2, requestId: q.requestId, instanceId: q.instanceId, documentId: q.documentId,
    contentRevision: 3, ok: true, result: { instanceId: q.instanceId, documentId: q.documentId,
      atRevision: 3, documentSnapshotId: 's3', contextId: 'scene-root', result: value } };
}
const settings = { guidesPresent: true, hPresent: false, vPresent: true, hCount: 0, vCount: 0 };
function page() {
  return { kind: 'guideAxisPage', axis: 'h', guidesPresent: true, axisPresent: true,
    guideCount: 2, cursor: 0, items: [{ ordinal: 1, position: -12.25 }, { ordinal: 2, position: -12.25 }], nextCursor: null };
}

test('guide intents preserve world-coordinate fractions and duplicates without admitting bulk or unrelated fields', () => {
  for (const action of [{ kind: 'add', axis: 'h', position: -12.25 },
    { kind: 'move', axis: 'v', ordinal: 2, position: 0.125 }, { kind: 'remove', axis: 'h', ordinal: 1 }]) {
    assert.deepEqual(C.action(action), action); assert(Object.isFrozen(C.action(action)));
    assert.doesNotThrow(() => C.payload(C.EDIT, { command: 'object.project.guide.edit', action }));
  }
  for (const action of [{ kind: 'clear', axis: 'h' }, { kind: 'add', axis: 'horizontal', position: 2 },
    { kind: 'move', axis: 'h', ordinal: 0, position: 2 }, { kind: 'move', axis: 'h', ordinal: 1.5, position: 2 },
    { kind: 'add', axis: 'h', position: Infinity }, { kind: 'add', axis: 'h', position: null },
    { kind: 'add', axis: 'h', position: 2, guides: [] }]) assert.throws(() => C.action(action), TypeError);
  let calls = 0;
  assert.throws(() => C.action({ kind: { toString() { calls++; return 'add'; }, toJSON() { calls++; return 'remove'; } }, axis: 'h', position: 1 }));
  assert.equal(calls, 0);
});

test('guide read distinguishes missing axes and explicit empty axes without normalization', () => {
  const q = request({ kind: 'settings' });
  assert.doesNotThrow(() => C.payload(C.READ, q.payload)); assert.doesNotThrow(() => C.correlate(q, reply(q, settings)));
  assert.doesNotThrow(() => C.correlate(q, reply(q, { guidesPresent: false, hPresent: false, vPresent: false, hCount: 0, vCount: 0 })));
  for (const body of [{ ...settings, guidesPresent: false }, { ...settings, hCount: 1 },
    { ...settings, vCount: -1 }, { ...settings, vCount: 0.5 }, { ...settings, session: {} }])
    assert.throws(() => C.correlate(q, reply(q, body)), TypeError);
});

test('guide pages preserve complete ordered positions and require dense data-only progressing records', () => {
  const q = request({ kind: 'axisPage', axis: 'h', cursor: 0, limit: 16 });
  assert.doesNotThrow(() => C.correlate(q, reply(q, page())));
  for (const mutate of [p => { p.items[1].ordinal = 1; }, p => { p.items[0].position = NaN; },
    p => { p.nextCursor = 1; }, p => { p.items = []; p.nextCursor = 0; }, p => { p.axis = 'v'; },
    p => { p.axisPresent = false; }, p => { delete p.items[0]; }]) {
    const value = page(); mutate(value); assert.throws(() => C.correlate(q, reply(q, value)), TypeError);
  }
  const value = page(); let calls = 0;
  Object.defineProperty(value.items, '0', { enumerable: true, configurable: true, get() { calls++; return { ordinal: 1, position: 2 }; } });
  assert.throws(() => C.correlate(q, reply(q, value)), TypeError); assert.equal(calls, 0);
});

test('guide correlation refuses foreign owners, revisions, snapshots and selectors', () => {
  const q = request({ kind: 'settings' });
  for (const mutate of [r => { r.instanceId = 'other'; }, r => { r.documentId = 'other'; },
    r => { r.contentRevision++; }, r => { r.result.documentSnapshotId = 'other'; },
    r => { r.result.atRevision++; }, r => { r.result.contextId = 'other'; },
    r => { r.result.result = page(); }]) {
    const value = reply(q, { ...settings }); mutate(value); assert.throws(() => C.correlate(q, value), TypeError);
  }
  const edit = { ...q, operation: C.EDIT, expectedRevision: 3 };
  const response = { ...reply(q, {}), contentRevision: 4, result: { applied: true, historyEntriesAdded: 1 } };
  assert.doesNotThrow(() => C.correlate(edit, response));
  assert.throws(() => C.correlate(edit, { ...response, contentRevision: 3 }), TypeError);
  assert.throws(() => C.correlate(edit, { ...response, result: { applied: true, historyEntriesAdded: 0 } }), TypeError);
});
