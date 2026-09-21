'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const source = fs.readFileSync(path.join(__dirname, '../src/js/adapters/application-mcp.js'), 'utf8');
const { createNativeTauriTransport } = require('../src/js/adapters/application-mcp.js');
const flush = () => new Promise(resolve => setImmediate(resolve));

function revisionFixture(intercept) {
  const listeners = new Map(), calls = [];
  const binding = { instanceId: 'native-1', documentId: 'document-1', lifecycleGeneration: 1,
    contentRevision: 0, subscriptionId: 'subscriber-1' };
  const transport = createNativeTauriTransport(async (command, args) => {
    calls.push([command, args]);
    const intercepted = intercept && intercept(command, args);
    if (intercepted !== undefined) return intercepted;
    if (command === 'nemo_native_status') return { apiVersion: 2, available: true, ...binding };
    if (command === 'nemo_native_revision_sync') {
      if (args.request.action === 'binding' || args.request.action === 'subscribe') return { ...binding };
      return null;
    }
    return { apiVersion: 2, requestId: args.request.requestId, ...binding, ok: true, result: {} };
  });
  const listen = async (name, callback) => {
    listeners.set(name, callback);
    return () => listeners.delete(name);
  };
  const event = { instanceId: 'native-1', documentId: 'document-1', lifecycleGeneration: 1,
    fromRevision: 0, toRevision: 1, requestId: 'external-1' };
  return { transport, listen, listeners, calls, binding, event };
}

test('native revision acknowledgment follows exact consumer synchronization, with no implicit subscription', async () => {
  const f = revisionFixture();
  await f.transport.connect();
  assert.equal(f.listeners.size, 0);
  let finish;
  const synchronized = new Promise(resolve => { finish = resolve; });
  await f.transport.subscribeRevisions(f.listen, event => synchronized.then(() => ({ ...event })));
  const pending = f.listeners.get('nemo-native-revision')({ payload: f.event });
  await flush();
  assert.equal(f.calls.some(([, args]) => args?.request?.action === 'acknowledge'), false);
  for (const operation of ['query.document.serialize', 'job.export.png.status']) {
    assert.equal((await f.transport.dispatch({ apiVersion: 2, requestId: operation, instanceId: 'native-1', documentId: 'document-1', operation })).ok, true);
  }
  await assert.rejects(f.transport.dispatch({ apiVersion: 2, requestId: 'write', instanceId: 'native-1', documentId: 'document-1', operation: 'history.undo' }), /pending/);
  finish();
  await pending;
  const acknowledgment = f.calls.find(([, args]) => args?.request?.action === 'acknowledge');
  assert.deepEqual(acknowledgment[1].request, { action: 'acknowledge', subscriptionId: 'subscriber-1', event: f.event });
  assert.equal(f.transport.status().contentRevision, 1);
  await f.transport.disconnect();
  assert.equal(f.listeners.size, 0);
});

test('malformed, stale, duplicate and out-of-order revision events never acknowledge', async () => {
  for (const change of [event => ({ ...event, extra: true }), event => ({ ...event, lifecycleGeneration: 2 }),
    event => ({ ...event, instanceId: 'other' }), event => ({ ...event, fromRevision: 1 }),
    event => ({ ...event, toRevision: 0 })]) {
    const f = revisionFixture();
    await f.transport.connect();
    await f.transport.subscribeRevisions(f.listen, async event => event);
    await f.listeners.get('nemo-native-revision')({ payload: change(f.event) });
    assert.equal(f.transport.status(), null);
    assert.equal(f.calls.some(([, args]) => args?.request?.action === 'acknowledge'), false);
    assert.equal(f.listeners.size, 0);
  }
  const f = revisionFixture();
  await f.transport.connect();
  await f.transport.subscribeRevisions(f.listen, async event => event);
  const listener = f.listeners.get('nemo-native-revision');
  await listener({ payload: f.event });
  await listener({ payload: f.event });
  assert.equal(f.calls.filter(([, args]) => args?.request?.action === 'acknowledge').length, 1);
  assert.equal(f.transport.status(), null);
});

test('consumer failure, disconnect and host release cannot acknowledge a later lifecycle', async () => {
  for (const mode of ['reject', 'wrong-revision', 'disconnect', 'release', 'duplicate']) {
    const f = revisionFixture();
    let finish;
    const pause = new Promise(resolve => { finish = resolve; });
    await f.transport.connect();
    await f.transport.subscribeRevisions(f.listen, async event => {
      if (mode === 'reject') throw new Error('consumer failed');
      if (mode === 'wrong-revision') return { ...event, toRevision: 2 };
      await pause; return event;
    });
    const pending = f.listeners.get('nemo-native-revision')({ payload: f.event });
    if (mode === 'disconnect') await f.transport.disconnect();
    if (mode === 'release') await f.listeners.get('nemo-native-revision-disconnected')({ payload: { subscriptionId: 'subscriber-1' } });
    if (mode === 'duplicate') await f.listeners.get('nemo-native-revision')({ payload: f.event });
    finish(); await pending;
    assert.equal(f.calls.some(([, args]) => args?.request?.action === 'acknowledge'), false);
    assert.equal(f.transport.status(), null);
  }
});

test('a read observing the committed head does not impersonate synchronized consumer state', async () => {
  const f = revisionFixture();
  await f.transport.connect();
  await f.transport.subscribeRevisions(f.listen, async event => event);
  f.binding.contentRevision = 1;
  const query = { apiVersion: 2, requestId: 'read-ahead', instanceId: 'native-1', documentId: 'document-1', operation: 'query.document.revision' };
  await f.transport.dispatch(query);
  assert.equal(f.transport.status().contentRevision, 1);
  await f.listeners.get('nemo-native-revision')({ payload: f.event });
  assert.equal(f.calls.filter(([, args]) => args?.request?.action === 'acknowledge').length, 1);
  f.binding.contentRevision = 0;
  await f.transport.dispatch({ ...query, requestId: 'retained-read' });
  assert.equal(f.transport.status().contentRevision, 1, 'retained responses never regress observed head');
});

test('disconnect during registration drains a late native registration and every listener', async () => {
  let finish;
  const f = revisionFixture((command, args) => {
    if (command === 'nemo_native_revision_sync' && args.request.action === 'subscribe') return new Promise(resolve => { finish = resolve; });
  });
  await f.transport.connect();
  const registering = f.transport.subscribeRevisions(f.listen, async event => event);
  await flush();
  await f.transport.disconnect();
  finish({ ...f.binding });
  await assert.rejects(registering, /disconnected/);
  assert.equal(f.listeners.size, 0);
  assert.equal(f.calls.filter(([, args]) => args?.request?.action === 'disconnect').length, 2);
});

test('old event callbacks and late dispatch replies cannot mutate a reconnected subscriber', async () => {
  let finish;
  const f = revisionFixture((command) => {
    if (command === 'nemo_native_dispatch') return new Promise(resolve => { finish = resolve; });
  });
  await f.transport.connect();
  await f.transport.subscribeRevisions(f.listen, async event => event);
  const oldListener = f.listeners.get('nemo-native-revision');
  const old = f.transport.dispatch({ apiVersion: 2, requestId: 'late', instanceId: 'native-1', documentId: 'document-1', operation: 'query.document.revision' });
  await f.transport.disconnect();
  f.binding.lifecycleGeneration = 2; f.binding.subscriptionId = 'subscriber-2';
  await f.transport.connect();
  await f.transport.subscribeRevisions(f.listen, async event => event);
  finish({ apiVersion: 2, requestId: 'late', instanceId: 'native-1', documentId: 'document-1', contentRevision: 99, ok: true });
  await assert.rejects(old, /disconnected/);
  await oldListener({ payload: f.event });
  assert.equal(f.transport.status().contentRevision, 0);
  assert.equal(f.listeners.size, 2);
  assert.equal(f.calls.some(([, args]) => args?.request?.action === 'acknowledge'), false);
});

test('one event buffers behind an already-dispatched UI advance whose reply arrives later', async () => {
  let finish, synchronized = false;
  const f = revisionFixture(command => {
    if (command === 'nemo_native_dispatch') return new Promise(resolve => { finish = resolve; });
  });
  await f.transport.connect();
  await f.transport.subscribeRevisions(f.listen, async event => { synchronized = true; return event; });
  const request = { apiVersion: 2, requestId: 'ui-before-event', instanceId: 'native-1', documentId: 'document-1', operation: 'command.document.apply' };
  const local = f.transport.dispatch(request);
  const event = { ...f.event, fromRevision: 1, toRevision: 2 };
  const pending = f.listeners.get('nemo-native-revision')({ payload: event });
  await flush();
  assert.equal(synchronized, false);
  await assert.rejects(f.transport.dispatch({ ...request, requestId: 'too-late' }), /pending/);
  finish({ apiVersion: 2, requestId: request.requestId, instanceId: request.instanceId, documentId: request.documentId, contentRevision: 1, ok: true });
  await local; await pending;
  assert.equal(synchronized, true);
  assert.equal(f.transport.status().contentRevision, 2);
  assert.deepEqual(f.calls.find(([, args]) => args?.request?.action === 'acknowledge')[1].request.event, event);
});

test('partial listener installation failure never registers a native subscriber', async () => {
  const f = revisionFixture();
  await f.transport.connect();
  await assert.rejects(f.transport.subscribeRevisions(async (name, callback) => {
    if (name === 'nemo-native-revision-disconnected') throw new Error('listener unavailable');
    return f.listen(name, callback);
  }, async event => event), /listener unavailable/);
  assert.equal(f.listeners.size, 0);
  assert.equal(f.transport.status(), null);
  assert.equal(f.calls.some(([, args]) => args?.request?.action === 'subscribe'), false);
});

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
