'use strict';

const assert = require('node:assert/strict');
const test = require('node:test');
const { createNativeViewportAdapter } = require('../src/js/adapters/native-viewport.js');

function port() {
  const sent = [];
  let disposed = 0;
  return { sent, count: () => disposed, send: (kind, value) => sent.push([kind, value]), dispose: () => { disposed += 1; } };
}
function adapter() { const transport = port(); return { transport, adapter: createNativeViewportAdapter(transport) }; }

test('is pure injected metadata transport with bounds, DPR, pointer and edge intents', () => {
  const { adapter: view, transport } = adapter();
  view.configure({ x: 100, y: 50, width: 640, height: 360 }, 1.25);
  view.register('work-1', 7); view.pointer(420, 230); view.selection(20, 60, 40, 80);
  assert.deepEqual(transport.sent, [
    ['configure', { bounds: { x: 100, y: 50, width: 640, height: 360 }, reportedDpr: 1.25, requestSequence: 0 }],
    ['register', { workId: 'work-1', viewGeneration: 7, requestSequence: 1 }],
    ['pointer', { clientX: 420, clientY: 230, requestSequence: 2 }], ['selection', { x0: 20, y0: 60, x1: 40, y1: 80, requestSequence: 3 }],
  ]);
  assert.equal(globalThis.__TAURI__, undefined);
});

test('deferred receipts do not cache and terminal receipts are retained without conflicts', () => {
  const { adapter: view } = adapter();
  view.register('work-1', 1);
  const first = view.receive({ workId: 'work-1', viewGeneration: 1, status: 'deferred-timeout' });
  const second = view.receive({ workId: 'work-1', viewGeneration: 1, status: 'deferred-occluded' });
  assert.notStrictEqual(first, second);
  const terminal = view.receive({ workId: 'work-1', viewGeneration: 1, status: 'presented' });
  assert.strictEqual(view.receive({ workId: 'work-1', viewGeneration: 1, status: 'presented' }), terminal);
  assert.throws(() => view.receive({ workId: 'work-1', viewGeneration: 1, status: 'failed-device-lost' }), /conflicting/);
});

test('generations and WorkIds are one-to-one, safe-positive and stale-safe', () => {
  const { adapter: view } = adapter();
  view.register('work-1', 1);
  const retained = view.receive({ workId: 'work-1', viewGeneration: 1, status: 'presented' });
  view.register('work-2', 2);
  assert.strictEqual(view.receive({ workId: 'work-1', viewGeneration: 1, status: 'presented' }), retained);
  view.register('work-3', 3);
  assert.throws(() => view.receive({ workId: 'work-2', viewGeneration: 2, status: 'presented' }), /older generation/);
  assert.deepEqual(view.receive({ workId: 'work-2', viewGeneration: 2, status: 'stale-discarded' }), { workId: 'work-2', viewGeneration: 2, status: 'stale-discarded' });
  assert.throws(() => view.receive({ workId: 'work-x', viewGeneration: 2, status: 'presented' }), /identity/);
  assert.throws(() => view.receive({ workId: 'work-2', viewGeneration: 2, status: 'arbitrary-data' }), /unknown/);
  assert.throws(() => view.register('conflict', 3), /same generation/);
  assert.throws(() => view.register('work-1', 4), /same WorkId/);
  assert.throws(() => view.register('zero', 0), /positive safe/);
  assert.throws(() => view.register('unsafe', Number.MAX_SAFE_INTEGER + 1), /positive safe/);
  assert.throws(() => view.receive({ workId: 'work-3', viewGeneration: 3, status: 'stale-discarded' }), /newest generation/);
  assert.throws(() => view.register('data:text/plain,not-an-id', 4), /payload|identifier/);
});

test('rejects pixels and data smuggled through neutral values or unadmitted objects', () => {
  const { adapter: view } = adapter();
  const base = { x: 0, y: 0, width: 1, height: 1 };
  assert.throws(() => view.configure({ ...base, payload: 'data:text/plain,pixels' }, 1), /payload|only/);
  assert.throws(() => view.configure({ ...base, payload: 'blob:opaque' }, 1), /payload|only/);
  assert.throws(() => view.configure({ ...base, payload: Buffer.from([1]) }, 1), /payload|only/);
  assert.throws(() => view.configure({ ...base, payload: new Uint8Array([1]) }, 1), /payload|only/);
  assert.throws(() => view.configure({ ...base, payload: { getContext() {} } }, 1), /payload|only/);
  assert.throws(() => view.receive({ workId: 'work-1', viewGeneration: 1, status: 'presented', note: {} }), /only/);
});

test('selection, DPR and post-dispose input are rejected safely', () => {
  const { adapter: view, transport } = adapter();
  assert.throws(() => view.configure({ x: 0, y: 0, width: 1, height: 1 }, 0), /reportedDpr/);
  assert.throws(() => view.selection(2, 0, 1, 1), /selection/);
  view.dispose(); view.dispose();
  assert.equal(transport.count(), 1);
  assert.throws(() => view.pointer(1, 1), /disposed/);
  assert.equal(transport.sent.length, 0);
});
