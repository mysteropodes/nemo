/* Existing feedback responsibility behind injected platform ports; no eager IO. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoFeedbackLocal = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  function create(ports) {
    var LOCAL_MAX = 500; // original oldest-first cap
    // ---- Storage tiers, tried in order: Tauri fs (real app, real OS folder)
    // -> local dev server (scripts/dev_server.py's /__feedback/* endpoints —
    // a real file too, just reachable over fetch() instead of Tauri's fs
    // plugin, for the ordinary `python3 -m http.server`-replacement dev
    // preview) -> localStorage (last resort — a live browser tab only, not
    // reachable outside an active eval session, but keeps this working even
    // if the plain http.server is what's actually running). ----
    async function localDir() {
      var base = await ports.tauri().path.appDataDir();
      return base.replace(/[\\/]+$/, '') + '/feedback/' + ports.projectKey();
    }
    function lsKey() { return 'nemo-feedback-' + ports.projectKey(); }
    function lsReadAll() { try { return JSON.parse(ports.storage().getItem(lsKey()) || '[]'); } catch (e) { return []; } }
    // No try/catch here (unlike lsReadAll) — a write failure (most likely
    // QuotaExceededError: a screenshot-bearing entry list can genuinely blow
    // past the ~5-10MB per-origin localStorage limit, since this is the ONLY
    // durable store in a browser-preview session with no dev server reachable)
    // must propagate up through writeLocal()'s unwrapped `await` to
    // submitFeedback's caller, so the UI's existing .catch shows "Échec de
    // l'enregistrement du feedback" instead of the false-positive success
    // toast — a report entered, "saved", then silently gone with the tab.
    function lsWriteAll(list) { ports.storage().setItem(lsKey(), JSON.stringify(list)); }

    var _devServerChecked = null;
    // Only scripts/dev_server.py implements /__feedback/* — a plain
    // `python -m http.server` (every other local preview in this codebase)
    // and any REAL deployment (feedback #33: nemo-editor.mysteropodes-auth.
    // workers.dev, a Cloudflare Worker with no such route) both 404 on it.
    // fetch() only REJECTS on a network-level failure, not on a non-2xx
    // status — the try/catch below already handled the 404 fine logically
    // (r.ok=false → same fallback to localStorage), but Chrome logs any
    // non-2xx request to the console regardless of whether JS catches it,
    // which is what actually got reported ("j'ai cette erreur dans la
    // console"). Cheapest fix: never issue the probe outside an actual
    // local-dev hostname, where dev_server.py is the only thing that could
    // possibly be listening.
    function isLocalDevHost() {
      var h = ports.hostname();
      return h === 'localhost' || h === '127.0.0.1' || h === '';
    }
    async function devServerAvailable() {
      if (ports.available()) return false; // Tauri always wins when present, dev server is browser-preview-only
      if (!isLocalDevHost()) return false;
      if (_devServerChecked !== null) return _devServerChecked;
      try {
        var r = await ports.fetch('/__feedback/ping');
        _devServerChecked = r.ok;
      } catch (e) { _devServerChecked = false; }
      return _devServerChecked;
    }

    async function readAllLocal() {
      if (ports.available()) {
        try {
          var dir = await localDir();
          var entries = await ports.tauri().fs.readDir(dir);
          var files = entries.filter(function (e) { return /\.json$/.test(e.name); });
          var out = [];
          for (var i = 0; i < files.length; i++) {
            try { out.push(JSON.parse(await ports.tauri().fs.readTextFile(dir + '/' + files[i].name))); } catch (e) {}
          }
          return out.sort(function (a, b) { return b.createdAt - a.createdAt; });
        } catch (e) { return []; }
      }
      if (await devServerAvailable()) {
        try {
          var res = await ports.fetch('/__feedback/list?projectKey=' + encodeURIComponent(ports.projectKey()));
          if (res.ok) return await res.json();
        } catch (e) { ports.warn('[feedback] dev-server list failed, falling back to localStorage', e); }
      }
      return lsReadAll();
    }
    async function writeLocal(entry) {
      if (ports.available()) {
        var dir = await localDir();
        await ports.tauri().fs.mkdir(dir, { recursive: true });
        await ports.tauri().fs.writeTextFile(dir + '/' + entry.id + '.json', JSON.stringify(entry));
        // Prune oldest past LOCAL_MAX, same convention as history/sync pruning
        // elsewhere in this app (pushVersionSnapshot, publishToShared).
        var entries = await ports.tauri().fs.readDir(dir);
        var files = entries.filter(function (e) { return /\.json$/.test(e.name); });
        if (files.length > LOCAL_MAX) {
          var withTimes = [];
          for (var i = 0; i < files.length; i++) {
            try { var d = JSON.parse(await ports.tauri().fs.readTextFile(dir + '/' + files[i].name)); withTimes.push({ name: files[i].name, createdAt: d.createdAt || 0 }); } catch (e) {}
          }
          withTimes.sort(function (a, b) { return a.createdAt - b.createdAt; });
          var toRemove = withTimes.slice(0, withTimes.length - LOCAL_MAX);
          for (var j = 0; j < toRemove.length; j++) { try { await ports.tauri().fs.remove(dir + '/' + toRemove[j].name); } catch (e) {} }
        }
        return;
      }
      if (await devServerAvailable()) {
        try {
          var res = await ports.fetch('/__feedback/save', { method: 'POST', body: JSON.stringify(entry) });
          if (res.ok) return;
        } catch (e) { ports.warn('[feedback] dev-server save failed, falling back to localStorage', e); }
      }
      var list = lsReadAll();
      var idx = list.findIndex(function (e) { return e.id === entry.id; });
      if (idx >= 0) list[idx] = entry; else list.unshift(entry);
      if (list.length > LOCAL_MAX) list = list.slice(0, LOCAL_MAX);
      lsWriteAll(list);
    }
    async function deleteLocal(id) {
      if (ports.available()) {
        try { await ports.tauri().fs.remove((await localDir()) + '/' + id + '.json'); } catch (e) {}
        return;
      }
      if (await devServerAvailable()) {
        try {
          var res = await ports.fetch('/__feedback/delete', { method: 'POST', body: JSON.stringify({ id: id, projectKey: ports.projectKey() }) });
          if (res.ok) return;
        } catch (e) { ports.warn('[feedback] dev-server delete failed, falling back to localStorage', e); }
      }
      lsWriteAll(lsReadAll().filter(function (e) { return e.id !== id; }));
    }

    async function publishShared(entry) {
      var root = ports.project().getSyncFolder();
      var dir = ports.project().profileDir(root, ports.profile().id) + '/feedback';
      await ports.tauri().fs.mkdir(dir, { recursive: true });
      await ports.tauri().fs.writeTextFile(dir + '/_profile.json', JSON.stringify(ports.profile()));
      await ports.tauri().fs.writeTextFile(dir + '/' + entry.id + '.json', JSON.stringify(entry));
    }
    async function checkIncomingFeedback() {
      if (!ports.available() || !ports.project() || !ports.project().getSyncFolder) return [];
      var root = ports.project().getSyncFolder();
      if (!root) return [];
      var localIds = (await readAllLocal()).map(function (e) { return e.id; });
      var out = [];
      try {
        var entries = await ports.tauri().fs.readDir(root);
        for (var i = 0; i < entries.length; i++) {
          var e = entries[i];
          if (e.name === ports.profile().id) continue; // your own — already local
          var fbDir = root.replace(/[\\/]+$/, '') + '/' + e.name + '/feedback';
          try {
            var sub = await ports.tauri().fs.readDir(fbDir);
            var files = sub.filter(function (x) { return /^fb_.*\.json$/.test(x.name); });
            for (var j = 0; j < files.length; j++) {
              var id = files[j].name.replace(/\.json$/, '');
              if (localIds.indexOf(id) >= 0) continue; // already imported
              try {
                var data = JSON.parse(await ports.tauri().fs.readTextFile(fbDir + '/' + files[j].name));
                if (data.projectKey !== ports.projectKey()) continue; // feedback for a DIFFERENT project sharing the same folder
                out.push(data);
              } catch (e2) {}
            }
          } catch (e3) {}
        }
      } catch (e4) { ports.warn('[feedback] check incoming failed', e4); }
      return out;
    }
    return Object.freeze({
      readAllLocal: readAllLocal,
      writeLocal: writeLocal,
      deleteLocal: deleteLocal,
      publishShared: publishShared,
      checkIncomingFeedback: checkIncomingFeedback
    });
  }
  return Object.freeze({ create: create });
}));
