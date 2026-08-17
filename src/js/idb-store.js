// Minimal async key/value store on IndexedDB, used as the large-capacity
// autosave backend for the browser (no-Tauri) mode. localStorage's ~5-10MB
// quota silently throws (or truncates via a caught exception, losing the
// autosave) once a project embeds real media (base64 images/video) — see
// project-nemo-web-public-beta memory. IndexedDB has no such practical
// ceiling. Kept deliberately tiny: get/set/remove on a single object store,
// nothing else — this is not a general storage layer, just enough to back
// the existing single-slot 'nemo-auto' autosave.
window.SMIdb = (function () {
  var DB_NAME = 'nemo-store', STORE = 'kv', VERSION = 1;
  var dbPromise = null;

  function openDb() {
    if (dbPromise) return dbPromise;
    dbPromise = new Promise(function (resolve, reject) {
      if (!window.indexedDB) { reject(new Error('indexedDB unavailable')); return; }
      var req = indexedDB.open(DB_NAME, VERSION);
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
        tx.oncomplete = function () { resolve(result && result.__req ? result.__req.result : undefined); };
        tx.onerror = function () { reject(tx.error); };
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
})();
