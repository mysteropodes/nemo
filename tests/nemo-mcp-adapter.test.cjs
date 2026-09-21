'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const source = fs.readFileSync(path.join(__dirname, '../src/js/adapters/application-mcp.js'), 'utf8');
const { createNativeTauriTransport } = require('../src/js/adapters/application-mcp.js');
const flush = () => new Promise(resolve => setImmediate(resolve));

test('browser transport does not initialize or mutate application state', () => {
  vm.runInNewContext(source, { window: { NemoApplication: { setInstanceId() { assert.fail('browser mutation'); } } } });
});

test('native transport establishes identity before ready and cancels queued writes', async () => {
  const listeners = new Map(), timers = new Map(), invoked = [], handled = [];
  let next = 0;
  const context = { window: null, console, Map, setTimeout(callback) { timers.set(++next, callback); return next; },
    clearTimeout(id) { timers.delete(id); }, addEventListener() {},
    NemoApplication: { setInstanceId(id) { invoked.push(['identity', id]); }, handle(request) {
      handled.push(request); return { ok: true, requestId: request.requestId };
    } },
    __TAURI__: { core: { async invoke(command, args) {
      invoked.push([command, args]);
      if (command === 'nemo_mcp_identity') return { instanceId: 'native-1' };
    } }, event: { async listen(name, callback) { listeners.set(name, callback); return () => listeners.delete(name); } } },
  };
  context.window = context;
  vm.runInNewContext(source, context);
  await flush();
  assert.deepEqual(invoked.map(call => call[0]), ['nemo_mcp_identity', 'identity', 'nemo_mcp_ready']);
  const receive = listeners.get('nemo-application-request'), cancel = listeners.get('nemo-application-cancel');
  receive({ payload: { connectionId: 'cancelled', request: { requestId: 'write-1' } } });
  cancel({ payload: { connectionId: 'cancelled' } });
  assert.equal(timers.size, 0);
  assert.equal(handled.length, 0);
  receive({ payload: { connectionId: 'active', request: { requestId: 'write-2' } } });
  await [...timers.values()][0]();
  assert.equal(handled.length, 1);
  assert.equal(invoked.at(-1)[0], 'nemo_mcp_reply');
  assert.equal(invoked.at(-1)[1].connectionId, 'active');
  assert.equal(invoked.at(-1)[1].response.requestId, 'write-2');
});

test('v2 Tauri transport is explicit, binds one identity and never retries a failed dispatch', async () => {
  const calls = [];
  let dispatches = 0;
  const invoke = async (command, args) => {
    calls.push([command, args]);
    if (command === 'nemo_native_status') return {
      apiVersion: 2, available: true, instanceId: 'native-1',
      documentId: 'document-1', contentRevision: 3,
    };
    dispatches += 1;
    return {
      apiVersion: 2, requestId: args.request.requestId, instanceId: 'native-1',
      documentId: 'document-1', contentRevision: 4, ok: true,
      result: { atRevision: 4, layerUid: 'layer-1', value: 40 },
    };
  };
  const transport = createNativeTauriTransport(invoke);
  assert.equal(transport.status(), null);
  assert.deepEqual(await transport.connect(), {
    instanceId: 'native-1', documentId: 'document-1', contentRevision: 3,
  });
  const request = {
    apiVersion: 2, requestId: 'query-1', instanceId: 'native-1',
    documentId: 'document-1', operation: 'query.document.opacity', payload: {},
  };
  assert.equal((await transport.dispatch(request)).ok, true);
  assert.equal(transport.status().contentRevision, 4);
  assert.equal(dispatches, 1);
  transport.disconnect();
  await assert.rejects(transport.dispatch(request), /disconnected/);
  assert.equal(dispatches, 1);
  assert.deepEqual(calls.map(([command]) => command), ['nemo_native_status', 'nemo_native_dispatch']);
});

test('v2 Tauri transport invalidates replacement and transport faults without fallback', async () => {
  let mode = 'replacement';
  let dispatches = 0;
  const transport = createNativeTauriTransport(async (command, args) => {
    if (command === 'nemo_native_status') return {
      apiVersion: 2, available: true, instanceId: 'native-1',
      documentId: 'document-1', contentRevision: 0,
    };
    dispatches += 1;
    if (mode === 'fault') throw new Error('connection lost');
    return {
      apiVersion: 2, requestId: args.request.requestId, instanceId: 'native-1',
      documentId: 'document-2', contentRevision: 0, ok: false,
      error: { code: 'wrong_document', message: 'replaced', details: { requestedDocumentId: 'document-1' } },
    };
  });
  const request = {
    apiVersion: 2, requestId: 'old-query', instanceId: 'native-1',
    documentId: 'document-1', operation: 'query.document.opacity', payload: {},
  };
  await transport.connect();
  assert.equal((await transport.dispatch(request)).error.code, 'wrong_document');
  assert.equal(transport.status(), null);
  await assert.rejects(transport.dispatch(request), /disconnected/);
  assert.equal(dispatches, 1);

  mode = 'fault';
  await transport.connect();
  await assert.rejects(transport.dispatch({ ...request, requestId: 'fault-query' }), /connection lost/);
  assert.equal(transport.status(), null);
  assert.equal(dispatches, 2);
});
