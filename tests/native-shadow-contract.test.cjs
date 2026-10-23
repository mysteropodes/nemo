'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const C = require('../src/js/adapters/native-shadow-contract.js');
function query(selector) {
  return { apiVersion: 2, requestId: 'shadow-q', instanceId: 'i', documentId: 'd', operation: C.READ,
    payload: { atRevision: 3, documentSnapshotId: 's3', contextId: 'scene-root', selector } };
}
function reply(q, value) {
  return { apiVersion: 2, requestId: q.requestId, instanceId: 'i', documentId: 'd', contentRevision: 3, ok: true,
    result: { instanceId: 'i', documentId: 'd', atRevision: 3, documentSnapshotId: 's3', contextId: 'scene-root', result: value } };
}
function page() {
  return { kind: 'shadowPalettePage', shadowPalettePresent: true, shadowColorCount: 3, cursor: 0,
    items: [{ ordinal: 1, id: 'duplicate', color: '#aBc' }, { ordinal: 2, id: 'duplicate', color: '#ABC' }], nextCursor: 2 };
}
test('only original Shadow add-and-select and ordinal select are admitted; Unicode spelling is preserved', () => {
  for (const action of [{ kind: 'add', id: 'same id/é', color: '#aBc' }, { kind: 'add', id: '', color: '' }, { kind: 'select', ordinal: 2 }]) {
    assert.deepEqual(C.action(action), action); assert(Object.isFrozen(C.action(action)));
    assert.doesNotThrow(() => C.payload(C.EDIT, { command: 'object.project.shadow.edit', action }));
  }
  for (const action of [{ kind: 'remove', ordinal: 1 }, { kind: 'select', ordinal: 0 }, { kind: 'select', ordinal: 1.5 },
    { kind: 'add', id: 'x', color: 'é'.repeat(513) }, { kind: 'add', id: '\0', color: '#fff' },
    { kind: 'add', id: '\uD800', color: '#fff' }, { kind: 'add', id: 'x', color: '#fff', shadowMode: true }])
    assert.throws(() => C.action(action), TypeError);
  let calls = 0;
  const action = { kind: 'add', color: '#abc' };
  Object.defineProperty(action, 'id', { enumerable: true, get() { calls++; return 'x'; } });
  assert.throws(() => C.action(action)); assert.equal(calls, 0);
});
test('settings retain absent, explicit empty and raw active ID without default materialization', () => {
  const q = query({ kind: 'settings' });
  for (const body of [{ shadowPalettePresent: false, shadowColorCount: 0 },
    { shadowPalettePresent: true, shadowColorCount: 0, shadowActiveId: '' },
    { shadowPalettePresent: true, shadowColorCount: 2, shadowActiveId: 'missing' }])
    assert.doesNotThrow(() => C.correlate(q, reply(q, body)));
  for (const body of [{ shadowPalettePresent: false, shadowColorCount: 1 },
    { shadowPalettePresent: true, shadowColorCount: 0.5 }, { shadowPalettePresent: true, shadowColorCount: 0, shadowMode: false }])
    assert.throws(() => C.correlate(q, reply(q, body)), TypeError);
});
test('pages preserve duplicate IDs and exact colors with dense bounded progressing records', () => {
  const q = query({ kind: 'palettePage', cursor: 0, limit: 16 });
  assert.doesNotThrow(() => C.payload(C.READ, q.payload)); assert.doesNotThrow(() => C.correlate(q, reply(q, page())));
  for (const mutate of [p => { p.items[1].ordinal = 1; }, p => { p.nextCursor = 0; }, p => { p.cursor = 1; },
    p => { p.items = []; p.nextCursor = 0; }, p => { delete p.items[0]; }, p => { p.shadowPalettePresent = false; },
    p => { p.items[0].extra = true; }]) {
    const value = page(); mutate(value); assert.throws(() => C.correlate(q, reply(q, value)), TypeError);
  }
  const value = page(); let calls = 0;
  Object.defineProperty(value.items, '0', { enumerable: true, configurable: true, get() { calls++; return {}; } });
  assert.throws(() => C.correlate(q, reply(q, value))); assert.equal(calls, 0);
  for (const selector of [{ kind: 'palettePage', cursor: 0, limit: 17 }, { kind: 'palettePage', cursor: -1, limit: 1 }])
    assert.throws(() => C.payload(C.READ, query(selector).payload), TypeError);
});
test('correlation fences owner, request, revision, snapshot and exact one-entry native receipt', () => {
  const q = query({ kind: 'settings' }), body = { shadowPalettePresent: false, shadowColorCount: 0 };
  for (const mutate of [r => { r.requestId = 'other'; }, r => { r.instanceId = 'other'; }, r => { r.documentId = 'other'; },
    r => { r.contentRevision++; }, r => { r.result.documentSnapshotId = 'other'; }, r => { r.result.atRevision++; },
    r => { r.result.contextId = 'other'; }, r => { r.result.result = page(); }]) {
    const r = reply(q, { ...body }); mutate(r); assert.throws(() => C.correlate(q, r), TypeError);
  }
  const edit = { ...q, operation: C.EDIT, expectedRevision: 3 };
  const r = { ...reply(q, {}), contentRevision: 4, result: { applied: true, historyEntriesAdded: 1 } };
  assert.doesNotThrow(() => C.correlate(edit, r));
  assert.throws(() => C.correlate(edit, { ...r, contentRevision: 3 }));
  assert.throws(() => C.correlate(edit, { ...r, result: { applied: false, historyEntriesAdded: 1 } }));
});
