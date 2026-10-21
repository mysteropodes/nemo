'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const C = require('../src/js/adapters/native-workspace-contract.js');
const workspace = () => ({ layerPanelWidth: null, toolsPanelWidth: '', propsPanelWidth: '0300px',
  propsPanelCollapsed: false, timelineHeight: '90vh', panelOrder: ['effects', 'effects', '', '雪\0'] });
const query = () => ({ apiVersion: 2, requestId: 'read', instanceId: 'i', documentId: 'd', operation: C.READ,
  payload: { atRevision: 7, documentSnapshotId: 's7', contextId: 'scene-root', selector: { kind: 'record' } } });
function reply(result) { return { apiVersion: 2, requestId: 'read', instanceId: 'i', documentId: 'd', contentRevision: 7, ok: true,
  result: { instanceId: 'i', documentId: 'd', atRevision: 7, documentSnapshotId: 's7', contextId: 'scene-root', result } }; }
test('capture closes the original seven fields and preserves raw spelling, null, omission and order', () => {
  for (const motionColumns of [undefined, null, '', '3d', '[]\0雪']) {
    const value = workspace(); if (motionColumns !== undefined) value.motionColumns = motionColumns;
    const action = C.action({ kind: 'capture', workspace: value });
    assert.deepEqual(action.workspace, value); assert(Object.isFrozen(action.workspace.panelOrder));
    C.payload(C.EDIT, { command: 'object.project.workspace.edit', action });
  }
  for (const key of Object.keys(workspace())) {
    const value = workspace(); delete value[key]; assert.throws(() => C.action({ kind: 'capture', workspace: value }));
  }
  for (const value of [null, [], { ...workspace(), propsPanelCollapsed: null }, { ...workspace(), panelOrder: [1] },
    { ...workspace(), preferences: {} }, { ...workspace(), canvasW: 4 }]) assert.throws(() => C.action({ kind: 'capture', workspace: value }));
  assert.throws(() => C.action({ kind: 'save', workspace: workspace() }));
  const accessor = workspace(); Object.defineProperty(accessor, 'timelineHeight', { enumerable: true, get() { throw Error('accessor read'); } });
  assert.throws(() => C.workspace(accessor, true), /Invalid native workspace/);
});
test('raw read distinguishes unavailable envelope, absent record, partial record and explicit null fields', () => {
  const q = query(); C.payload(q.operation, q.payload);
  for (const result of [{ workspacePresent: false }, { workspacePresent: true, workspace: {} },
    { workspacePresent: true, workspace: { panelOrder: null, motionColumns: '', layerPanelWidth: null, propsPanelCollapsed: false } }]) {
    const r = reply(result); C.correlate(q, r); assert.deepEqual(r.result.result, result);
  }
  for (const result of [{ workspacePresent: true }, { workspacePresent: false, workspace: {} },
    { workspacePresent: true, workspace: null }, { workspacePresent: true, workspace: { extra: true } }]) assert.throws(() => C.correlate(q, reply(result)));
  const stale = reply({ workspacePresent: false }); stale.result.documentSnapshotId = 'stale'; assert.throws(() => C.correlate(q, stale));
  const uncertain = { ...reply({ workspacePresent: false }), ok: false, error: { code: 'unavailable' } }; delete uncertain.result;
  C.correlate(q, uncertain); assert.equal(uncertain.ok, false);
});
test('semantic duplicate comparison preserves missing versus supplied and arrays without normalization', () => {
  const value = workspace(), reordered = Object.fromEntries(Object.entries(value).reverse());
  assert(C.equal(value, reordered)); assert(!C.equal(value, { ...value, motionColumns: null }));
  assert(!C.equal(value, { ...value, panelOrder: [...value.panelOrder].reverse() }));
  const q = { ...query(), operation: C.EDIT, expectedRevision: 7, payload: { command: 'object.project.workspace.edit', action: { kind: 'capture', workspace: value } } };
  for (const applied of [false, true]) {
    const r = { ...reply(null), contentRevision: applied ? 8 : 7, result: { applied, historyEntriesAdded: Number(applied) } };
    C.correlate(q, r); r.contentRevision++; assert.throws(() => C.correlate(q, r));
  }
});
