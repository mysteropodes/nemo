'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const source = fs.readFileSync(path.join(__dirname, '../src/js/adapters/application-mcp.js'), 'utf8');
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
