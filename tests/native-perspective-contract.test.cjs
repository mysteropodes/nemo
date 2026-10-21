'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const C = require('../src/js/adapters/native-perspective-contract.js');
const counts = { perspectiveVPsPresent: false, perspectiveVPsNull: false, perspectiveVPCount: 0, effectiveVPCount: 2, vpSource: 'default' };
function query(selector) {
  return { apiVersion: 2, requestId: 'perspective-q', instanceId: 'i', documentId: 'd', operation: C.READ,
    payload: { atRevision: 3, documentSnapshotId: 's3', contextId: 'scene-root', selector } };
}
function response(q, result) {
  return { apiVersion: 2, requestId: q.requestId, instanceId: 'i', documentId: 'd', contentRevision: 3, ok: true,
    result: { instanceId: 'i', documentId: 'd', atRevision: 3, documentSnapshotId: 's3', contextId: 'scene-root', result } };
}
function page() {
  return { ...counts, kind: 'perspectiveVPPage', cursor: 0,
    items: [{ ordinal: 1, x: -48, y: 90, locked: false }, { ordinal: 2, x: 368, y: 90, locked: false }], nextCursor: null };
}
test('only the six original Perspective gestures are admitted, with finite coordinates and one-based ordinals', () => {
  for (const action of [{ kind: 'setEnabled', enabled: true }, { kind: 'setMode', mode: 'fisheye' }, { kind: 'reset' },
    { kind: 'lockAll', locked: false }, { kind: 'move', ordinal: 1, x: -0.25, y: 2.125 }, { kind: 'translate', dx: -8, dy: 1.5 }]) {
    assert.deepEqual(C.action(action), action); assert(Object.isFrozen(C.action(action)));
    assert.doesNotThrow(() => C.payload(C.EDIT, { command: 'object.project.perspective.edit', action }));
  }
  for (const action of [{ kind: 'setDensity', density: 24 }, { kind: 'setMode', mode: '4pt' }, { kind: 'reset', points: [] },
    { kind: 'setEnabled', enabled: 1 }, { kind: 'lockAll', locked: null }, { kind: 'move', ordinal: 0, x: 1, y: 2 },
    { kind: 'move', ordinal: 1, x: Infinity, y: 0 }, { kind: 'translate', dx: NaN, dy: 0 },
    { kind: 'reset', canvasW: 320 }]) assert.throws(() => C.action(action), TypeError);
});
test('raw presence, null, inert density and unknown mode remain distinct from native effective defaults', () => {
  const q = query({ kind: 'settings' });
  for (const body of [counts, { ...counts, perspectiveVPsPresent: true, perspectiveVPsNull: true },
    { ...counts, perspectiveVPsPresent: true, perspectiveMode: 'unknown-import', perspectiveDensity: -0.125 },
    { ...counts, perspectiveMode: '3pt', effectiveVPCount: 3 },
    { ...counts, perspectiveMode: 'fisheye', effectiveVPCount: 1 }])
    assert.doesNotThrow(() => C.correlate(q, response(q, body)));
  for (const body of [{ ...counts, perspectiveVPsNull: true }, { ...counts, vpSource: 'stored' },
    { ...counts, perspectiveVPCount: 1 }, { ...counts, effectiveVPCount: 3 },
    { ...counts, perspectiveEnabled: null }, { ...counts, perspectiveDensity: Infinity }])
    assert.throws(() => C.correlate(q, response(q, body)), TypeError);
});
test('effective pages retain raw provenance, optional locks and dense complete progressing ordinals', () => {
  const q = query({ kind: 'vpPage', cursor: 0, limit: 16 }); assert.doesNotThrow(() => C.correlate(q, response(q, page())));
  const stored = { ...page(), perspectiveVPsPresent: true, perspectiveVPCount: 2, vpSource: 'stored' };
  delete stored.items[0].locked; assert.doesNotThrow(() => C.correlate(q, response(q, stored)));
  for (const mutate of [p => { p.items[1].ordinal = 1; }, p => { p.nextCursor = 0; },
    p => { p.items = []; p.nextCursor = 0; }, p => { delete p.items[0]; }, p => { p.items[0].locked = true; },
    p => { p.items[0].x = NaN; }, p => { p.effectiveVPCount = 4; }]) {
    const value = page(); mutate(value); assert.throws(() => C.correlate(q, response(q, value)), TypeError);
  }
  let calls = 0; const value = page();
  Object.defineProperty(value.items, '0', { enumerable: true, configurable: true, get() { calls++; return {}; } });
  assert.throws(() => C.correlate(q, response(q, value))); assert.equal(calls, 0);
});
test('owner, request, revision, snapshot, selector and exact history receipt correlations are closed', () => {
  const q = query({ kind: 'settings' });
  for (const mutate of [r => { r.requestId = 'other'; }, r => { r.instanceId = 'other'; }, r => { r.documentId = 'other'; },
    r => { r.contentRevision++; }, r => { r.result.documentSnapshotId = 'other'; }, r => { r.result.contextId = 'other'; },
    r => { r.result.result = page(); }]) {
    const r = response(q, { ...counts }); mutate(r); assert.throws(() => C.correlate(q, r), TypeError);
  }
  const edit = { ...q, operation: C.EDIT, expectedRevision: 3 };
  const r = { ...response(q, {}), contentRevision: 4, result: { applied: true, historyEntriesAdded: 1 } };
  assert.doesNotThrow(() => C.correlate(edit, r)); assert.throws(() => C.correlate(edit, { ...r, contentRevision: 3 }));
});
