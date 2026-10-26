'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const C = require('../src/js/adapters/native-symmetry-contract.js');
const read = selector => ({ atRevision: 7, documentSnapshotId: 'snapshot-7', contextId: 'scene-root', selector });
const effective = { enabled: false, mode: 'y', axis: { x1: 1, y1: 0, x2: 1, y2: 2 }, radialCenter: { x: 1, y: 1 }, radialSectors: 6, extend: true };
const envelope = result => ({ instanceId: 'instance-A', documentId: 'document-A', atRevision: 7,
  documentSnapshotId: 'snapshot-7', contextId: 'scene-root', result });

test('the seven original gestures are closed intents and nothing else is admitted', () => {
  for (const action of [{ kind: 'setEnabled', enabled: true }, { kind: 'setMode', mode: 'radial' }, { kind: 'resetGuide' },
    { kind: 'setAxis', x1: -1.5, y1: 0, x2: 2, y2: 3 }, { kind: 'setRadialCenter', x: 1, y: 2 },
    { kind: 'setSectors', sectors: 30 }, { kind: 'setExtend', extend: false }]) {
    C.payload(C.EDIT, { command: 'object.project.symmetry.edit', action });
    assert.ok(Object.isFrozen(C.action(action)));
  }
  for (const action of [{ kind: 'setMode', mode: 'z' }, { kind: 'setEnabled', enabled: 1 }, { kind: 'setAxis', x1: 0, y1: 0, x2: 1 },
    { kind: 'setRadialCenter', x: 0, y: Infinity }, { kind: 'setSectors', sectors: NaN }, { kind: 'resetGuide', mode: 'x' },
    { kind: 'toggle' }, [], null]) {
    assert.throws(() => C.payload(C.EDIT, { command: 'object.project.symmetry.edit', action }), TypeError, JSON.stringify(action));
  }
  assert.throws(() => C.payload(C.EDIT, { command: 'object.project.shadow.edit', action: { kind: 'resetGuide' } }), TypeError);
  C.payload(C.READ, read({ kind: 'settings' }));
  for (const selector of [{ kind: 'page' }, { kind: 'settings', limit: 1 }, 'settings']) {
    assert.throws(() => C.payload(C.READ, read(selector)), TypeError);
  }
});

test('read results carry the effective settings and echo raw members with null distinct from missing', () => {
  C.result(C.READ, envelope({ effective }));
  C.result(C.READ, envelope({ effective, symmetryAxis: null, symmetryRadialCenter: { x: 0, y: 0 }, symmetryMode: '',
    symmetryRadialSectors: 0.5, symmetryEnabled: true, symmetryExtend: false }));
  for (const bad of [{ effective, symmetryAxis: [0, 0, 1, 1] }, { effective, symmetryEnabled: null }, { effective, unknown: 1 },
    { effective: { ...effective, mode: '' } }, { effective: { ...effective, axis: null } }, {}]) {
    assert.throws(() => C.result(C.READ, envelope(bad)), TypeError, JSON.stringify(bad));
  }
  C.result(C.EDIT, { applied: true, historyEntriesAdded: 1 });
  assert.throws(() => C.result(C.EDIT, { applied: true, historyEntriesAdded: 0 }), TypeError);
});

test('correlation binds the read to its pin and the edit to its expected revision', () => {
  const request = { apiVersion: 2, requestId: 'r', instanceId: 'instance-A', documentId: 'document-A', operation: C.READ, payload: read({ kind: 'settings' }) };
  const response = { apiVersion: 2, requestId: 'r', instanceId: 'instance-A', documentId: 'document-A', contentRevision: 7, ok: true, result: envelope({ effective }) };
  C.correlate(request, response);
  assert.throws(() => C.correlate(request, { ...response, contentRevision: 8 }), TypeError);
  assert.throws(() => C.correlate(request, { ...response, result: { ...envelope({ effective }), documentSnapshotId: 'other' } }), TypeError);
  const edit = { apiVersion: 2, requestId: 'e', instanceId: 'instance-A', documentId: 'document-A', expectedRevision: 7, operation: C.EDIT,
    payload: { command: 'object.project.symmetry.edit', action: { kind: 'resetGuide' } } };
  C.correlate(edit, { apiVersion: 2, requestId: 'e', instanceId: 'instance-A', documentId: 'document-A', contentRevision: 8, ok: true, result: { applied: true, historyEntriesAdded: 1 } });
  assert.throws(() => C.correlate(edit, { apiVersion: 2, requestId: 'e', instanceId: 'instance-A', documentId: 'document-A', contentRevision: 7, ok: true, result: { applied: true, historyEntriesAdded: 1 } }), TypeError);
  assert.equal(C.compose(null).admitted(C.READ), true);
  assert.equal(C.compose(null).requiresRevision(C.EDIT), true);
});
