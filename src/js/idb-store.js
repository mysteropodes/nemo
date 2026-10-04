// P34: opaque browser autosave persistence adapter. IndexedDB remains subject
// to browser quota/eviction; this is not a document writer or recovery service.
// Each factory instance owns one cached open promise and connection for its
// lifetime. The browser facade below is the sole production instance.
function createIndexedDbStore(indexedDb) {
  'use strict';
  var DB_NAME = 'nemo-store', STORE = 'kv', VERSION = 1;
  var dbPromise = null;

  function openDb() {
    if (dbPromise) return dbPromise;
    dbPromise = new Promise(function (resolve, reject) {
      if (!indexedDb) { reject(new Error('indexedDB unavailable')); return; }
      var req = indexedDb.open(DB_NAME, VERSION);
      req.onupgradeneeded = function () {
        if (!req.result.objectStoreNames.contains(STORE)) req.result.createObjectStore(STORE);
      };
      req.onsuccess = function () { resolve(req.result); };
      req.onerror = function () { reject(req.error); };
    });
    return dbPromise;
  }

  function withStore(mode, fn) {
    return openDb().then(function (db) {
      return new Promise(function (resolve, reject) {
        var tx = db.transaction(STORE, mode);
        var store = tx.objectStore(STORE);
        var result = fn(store);
        // Request success is not transaction commit, including for reads.
        tx.oncomplete = function () { resolve(result && result.__req ? result.__req.result : undefined); };
        tx.onerror = function () { reject(tx.error); };
        // Preserve baseline: no abort-only handler, retry, close or recovery.
      });
    });
  }

  function get(key) {
    return withStore('readonly', function (store) { return { __req: store.get(key) }; });
  }
  function set(key, value) {
    return withStore('readwrite', function (store) { store.put(value, key); });
  }
  function remove(key) {
    return withStore('readwrite', function (store) { store.delete(key); });
  }

  return { get: get, set: set, remove: remove };
}
if (typeof module !== 'undefined' && module.exports) module.exports = { createIndexedDbStore: createIndexedDbStore };
if (typeof window !== 'undefined') window.SMIdb = createIndexedDbStore(window.indexedDB);
