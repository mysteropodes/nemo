'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const { createIndexedDbStore } = require('../src/js/idb-store.js');

// Independent event driver: requests never complete transactions themselves.
function fixture(existing = false) {
  const values = new Map();
  const opens = [], transactions = [], created = [];
  const db = {
    objectStoreNames: { contains: name => existing && name === 'kv' },
    createObjectStore(name) { created.push(name); },
    transaction(name, mode) {
      const requests = [];
      const tx = { name, mode, requests, error: null,
        objectStore(storeName) {
          assert.equal(storeName, 'kv');
          return Object.fromEntries(['get', 'put', 'delete'].map(operation => [operation, (...args) => {
            const request = { operation, args, result: undefined,
              succeed(value) { request.result = value; if (request.onsuccess) request.onsuccess({ target: request }); },
            };
            requests.push(request);
            return request;
          }]));
        },
        commit() {
          for (const req of requests) {
            if (req.operation === 'get') req.result = values.get(req.args[0]);
            if (req.operation === 'put') values.set(req.args[1], req.args[0]);
            if (req.operation === 'delete') values.delete(req.args[0]);
          }
          tx.oncomplete();
        },
      };
      transactions.push(tx);
      return tx;
    },
  };
  const indexedDb = { open(name, version) {
    const request = { name, version, result: db, error: null };
    opens.push(request);
    return request;
  } };
  return { values, opens, transactions, created, db, indexedDb,
    open() { opens[0].onupgradeneeded(); opens[0].onsuccess(); } };
}
const tick = () => new Promise(resolve => setImmediate(resolve));
test('adopted adapter registration has no unclassified or size exception', () => {
  const profile = JSON.parse(fs.readFileSync(path.join(__dirname, '../engineering/boundaries/profiles/app-js.profile.json')));
  const module = profile.modules.find(entry => entry.id === 'app.idb.store');
  assert.equal(module.layer, 'adapters');
  assert.equal(module.sizeProfile, 'Platform/engine adapter');
  assert.deepEqual(module.publicApi, ['idb-store.js']);
  assert.equal(profile.exceptions.some(entry => entry.path === 'src/js/idb-store.js'), false);
});
async function pending(promise) {
  let settled = false;
  promise.then(() => { settled = true; }, () => { settled = true; });
  await tick();
  assert.equal(settled, false, 'operation must remain pending before commit');
}

test('factory is importable without a browser; browser facade uses the same injected implementation', async () => {
  const f = fixture();
  const window = { indexedDB: f.indexedDb };
  vm.runInNewContext(fs.readFileSync(path.join(__dirname, '../src/js/idb-store.js'), 'utf8'), { window });
  assert.deepEqual(Object.keys(window.SMIdb), ['get', 'set', 'remove']);
  const result = window.SMIdb.get('absent');
  f.open(); await tick(); f.transactions[0].commit();
  assert.equal(await result, undefined);
});

test('single cached open, upgrade only creates missing kv, and concurrent callers share connection', async () => {
  for (const existing of [false, true]) {
    const f = fixture(existing), store = createIndexedDbStore(f.indexedDb);
    const a = store.get('a'), b = store.set('b', 'bytes');
    assert.deepEqual(f.opens.map(({ name, version }) => ({ name, version })), [{ name: 'nemo-store', version: 1 }]);
    f.open(); await tick();
    assert.deepEqual(f.created, existing ? [] : ['kv']);
    assert.deepEqual(f.transactions.map(tx => [tx.name, tx.mode]), [['kv', 'readonly'], ['kv', 'readwrite']]);
    f.transactions.forEach(tx => tx.commit());
    assert.equal(await a, undefined); assert.equal(await b, undefined);
    const later = store.get('b'); await tick(); f.transactions[2].commit();
    assert.equal(await later, 'bytes'); assert.equal(f.opens.length, 1);
  }
});

test('browser host lookup remains lazy and throwing getter rejection is cached', async () => {
  const error = new Error('host denied');
  let lookups = 0;
  const window = {};
  Object.defineProperty(window, 'indexedDB', { get() { lookups++; throw error; } });
  vm.runInNewContext(fs.readFileSync(path.join(__dirname, '../src/js/idb-store.js'), 'utf8'), { window });
  assert.equal(lookups, 0);
  await assert.rejects(window.SMIdb.get('x'), value => value === error);
  await assert.rejects(window.SMIdb.remove('x'), value => value === error);
  assert.equal(lookups, 1);
});

test('CommonJS host bootstrap executes the same browser open port for present and unavailable storage', async () => {
  const modulePath = require.resolve('../src/js/idb-store.js');
  const cached = require.cache[modulePath], previousWindow = global.window;
  try {
    for (const available of [true, false]) {
      const f = fixture();
      global.window = { indexedDB: available ? f.indexedDb : undefined };
      delete require.cache[modulePath]; require(modulePath);
      const result = global.window.SMIdb.get('absent');
      if (available) {
        f.open(); await tick(); f.transactions[0].commit();
        assert.equal(await result, undefined);
      } else await assert.rejects(result, /indexedDB unavailable/);
    }
  } finally {
    require.cache[modulePath] = cached;
    if (previousWindow === undefined) delete global.window;
    else global.window = previousWindow;
  }
});

test('opaque fixture bytes round-trip and remove, with every operation pending until commit', async () => {
  const f = fixture(), store = createIndexedDbStore(f.indexedDb);
  const bytes = fs.readFileSync(path.join(__dirname, 'animation/fixtures/curve-workflow.json'), 'utf8');
  const written = store.set('nemo-auto', bytes); f.open(); await tick();
  assert.deepEqual(f.transactions[0].requests[0].args, [bytes, 'nemo-auto']);
  f.transactions[0].requests[0].succeed('nemo-auto');
  await pending(written); f.transactions[0].commit(); assert.equal(await written, undefined);
  const read = store.get('nemo-auto'); await tick();
  // Simulate an early successful request independently of transaction completion.
  f.transactions[1].requests[0].succeed(bytes);
  await pending(read); f.transactions[1].commit(); assert.equal(await read, bytes);
  const removed = store.remove('nemo-auto'); await tick();
  f.transactions[2].requests[0].succeed(undefined);
  await pending(removed); f.transactions[2].commit(); assert.equal(await removed, undefined);
  const missing = store.get('nemo-auto'); await tick(); f.transactions[3].commit();
  assert.equal(await missing, undefined);
});

test('completion oracle rejects a deliberately early-resolving control', async () => {
  await assert.rejects(pending(Promise.resolve(undefined)), /operation must remain pending before commit/);
});

test('unavailable and failed opens remain cached and preserve errors', async () => {
  const unavailable = createIndexedDbStore(undefined);
  await assert.rejects(unavailable.get('x'), /indexedDB unavailable/);
  await assert.rejects(unavailable.set('x', 'y'), /indexedDB unavailable/);
  const f = fixture(), store = createIndexedDbStore(f.indexedDb), error = new Error('open failed');
  const failed = store.get('x'); f.opens[0].error = error; f.opens[0].onerror();
  await assert.rejects(failed, value => value === error);
  await assert.rejects(store.remove('x'), value => value === error);
  assert.equal(f.opens.length, 1);
});

test('synchronous open and transaction/request failures propagate', async () => {
  const error = new Error('synchronous failure');
  await assert.rejects(createIndexedDbStore({ open() { throw error; } }).get('x'), value => value === error);
  for (const failure of ['transaction', 'request']) {
    const f = fixture(), store = createIndexedDbStore(f.indexedDb);
    if (failure === 'transaction') f.db.transaction = () => { throw error; };
    else f.db.transaction = () => ({ objectStore: () => ({ put() { throw error; } }) });
    const result = store.set('x', 'y'); f.open();
    await assert.rejects(result, value => value === error);
  }
});

test('transaction error rejects with the original error without committed writes', async () => {
  const f = fixture(), store = createIndexedDbStore(f.indexedDb), error = new Error('quota');
  const result = store.set('x', 'y'); f.open(); await tick();
  const tx = f.transactions[0]; tx.error = error; tx.onerror();
  await assert.rejects(result, value => value === error);
  assert.equal(f.values.has('x'), false);
});

test('baseline abort-only and blocked open have no completion/recovery handler', async () => {
  const f = fixture(), store = createIndexedDbStore(f.indexedDb);
  const result = store.get('x');
  assert.equal(f.opens[0].onblocked, undefined); await pending(result);
  f.open(); await tick();
  assert.equal(f.transactions[0].onabort, undefined); await pending(result);
  f.transactions[0].commit(); await result;
});
