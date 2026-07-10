// ---- DEBUG / TEAM FEEDBACK LOG ----
// A lightweight, always-available way to leave a note tied to real repro
// context (what you actually just did, not just a screenshot), stored
// ENTIRELY OUTSIDE the project's own .json — never touches exportJSON()/
// importJSON(), so switching/closing/sharing a project never gains or
// loses feedback entries, and the project file itself never grows because
// of debug chatter.
//
// Two tiers, matching the existing team-sync trust model exactly (a pulled
// remote change always lands as a "correction" needing Accept/Reject —
// see mergeRemoteSnapshot in app.js — feedback reuses that same idea):
//   - Your OWN entries (submitted from this machine, this profile) write
//     straight to the local feedback folder with status 'approved'.
//   - Entries PULLED from someone else's shared-sync folder land as
//     'pending' until you explicitly approve them (see approveFeedback).
// Only 'approved' entries are meant to be read/acted on downstream.
(function () {
  var LOCAL_MAX = 500; // local entries pruned oldest-first past this count
  var ACTION_LOG_MAX = 150; // mirrors state.actionLog's own cap in app.js

  function tauriOk() { return typeof window.__TAURI__ !== 'undefined'; }
  function projectKey() { return (window.SMProject && window.SMProject.getProjectKey()) || 'untitled-autosave'; }

  // ---- Action log (session-only trail feeding actionTrail below) ----
  // Called once per pushUndoLayers() (app.js/tweens.js's single choke point
  // before every meaningful mutation) — consecutive same-tool/same-frame
  // calls collapse into one entry with a running count, so a long freehand
  // drag or a burst of clicks doesn't flood the trail with duplicates.
  function logAction() {
    if (!state.actionLog) state.actionLog = [];
    var ld = state.layers[state.activeLayerIdx];
    var last = state.actionLog[state.actionLog.length - 1];
    var tool = state.tool, frame = state.currentFrame, layerName = ld ? ld.name : null;
    if (last && last.tool === tool && last.frame === frame && last.layer === layerName) {
      last.count = (last.count || 1) + 1;
      last.t = Date.now();
      return;
    }
    state.actionLog.push({ t: Date.now(), tool: tool, frame: frame, layer: layerName, count: 1 });
    if (state.actionLog.length > ACTION_LOG_MAX) state.actionLog.shift();
  }
  function recentActionTrail(n) {
    var log = state.actionLog || [];
    return log.slice(Math.max(0, log.length - (n || 20))).map(function (e) {
      return { t: e.t, tool: e.tool, frame: e.frame, layer: e.layer, count: e.count };
    });
  }

  // ---- Local storage (outside the project — Tauri app-data dir, or a
  // localStorage fallback so this is still testable in the browser preview) ----
  function localDirKey() { return 'feedback-' + projectKey(); }
  async function localDir() {
    var base = await window.__TAURI__.path.appDataDir();
    return base.replace(/[\\/]+$/, '') + '/feedback/' + projectKey();
  }
  function lsKey() { return 'sm-feedback-' + projectKey(); }
  function lsReadAll() { try { return JSON.parse(localStorage.getItem(lsKey()) || '[]'); } catch (e) { return []; } }
  function lsWriteAll(list) { try { localStorage.setItem(lsKey(), JSON.stringify(list)); } catch (e) {} }

  async function readAllLocal() {
    if (!tauriOk()) return lsReadAll();
    try {
      var dir = await localDir();
      var entries = await window.__TAURI__.fs.readDir(dir);
      var files = entries.filter(function (e) { return /\.json$/.test(e.name); });
      var out = [];
      for (var i = 0; i < files.length; i++) {
        try { out.push(JSON.parse(await window.__TAURI__.fs.readTextFile(dir + '/' + files[i].name))); } catch (e) {}
      }
      return out.sort(function (a, b) { return b.createdAt - a.createdAt; });
    } catch (e) { return []; }
  }
  async function writeLocal(entry) {
    if (!tauriOk()) {
      var list = lsReadAll();
      var idx = list.findIndex(function (e) { return e.id === entry.id; });
      if (idx >= 0) list[idx] = entry; else list.unshift(entry);
      if (list.length > LOCAL_MAX) list = list.slice(0, LOCAL_MAX);
      lsWriteAll(list);
      return;
    }
    var dir = await localDir();
    await window.__TAURI__.fs.mkdir(dir, { recursive: true });
    await window.__TAURI__.fs.writeTextFile(dir + '/' + entry.id + '.json', JSON.stringify(entry));
    // Prune oldest past LOCAL_MAX, same convention as history/sync pruning
    // elsewhere in this app (pushVersionSnapshot, publishToShared).
    var entries = await window.__TAURI__.fs.readDir(dir);
    var files = entries.filter(function (e) { return /\.json$/.test(e.name); });
    if (files.length > LOCAL_MAX) {
      var withTimes = [];
      for (var i = 0; i < files.length; i++) {
        try { var d = JSON.parse(await window.__TAURI__.fs.readTextFile(dir + '/' + files[i].name)); withTimes.push({ name: files[i].name, createdAt: d.createdAt || 0 }); } catch (e) {}
      }
      withTimes.sort(function (a, b) { return a.createdAt - b.createdAt; });
      var toRemove = withTimes.slice(0, withTimes.length - LOCAL_MAX);
      for (var j = 0; j < toRemove.length; j++) { try { await window.__TAURI__.fs.remove(dir + '/' + toRemove[j].name); } catch (e) {} }
    }
  }
  async function deleteLocal(id) {
    if (!tauriOk()) { lsWriteAll(lsReadAll().filter(function (e) { return e.id !== id; })); return; }
    try { await window.__TAURI__.fs.remove((await localDir()) + '/' + id + '.json'); } catch (e) {}
  }

  function genId() { return 'fb_' + Date.now().toString(36) + '_' + Math.floor(Math.random() * 1e6); }

  // ---- Submit (always YOUR OWN — auto-approved, per this module's own
  // doc comment: only a PULLED entry from someone else needs approval) ----
  async function submitFeedback(opts) {
    opts = opts || {};
    var entry = {
      id: genId(),
      projectKey: projectKey(),
      frame: state.currentFrame,
      pos: opts.pos ? [opts.pos.x, opts.pos.y] : null,
      author: state.userProfile ? { id: state.userProfile.id, name: state.userProfile.name, color: state.userProfile.color, role: state.userProfile.role } : null,
      tags: opts.tags || [],
      blocking: !!opts.blocking,
      note: opts.note || '',
      actionTrail: recentActionTrail(20),
      status: 'approved', // your own machine, your own profile — trusted by definition
      resolution: null,
      createdAt: Date.now(),
      resolvedAt: null,
      origin: 'local',
    };
    await writeLocal(entry);
    // Best-effort: also publish to the shared team-sync folder (if
    // configured) so the project owner can pull it — reuses the EXACT
    // same root/<profileId>/ convention publishToShared() already writes
    // full project snapshots into, just a "feedback" subfolder instead of
    // the project-snapshot files, so the two never collide.
    if (tauriOk() && window.SMProject && window.SMProject.getSyncFolder && window.SMProject.getSyncFolder()) {
      try {
        var root = window.SMProject.getSyncFolder();
        var dir = window.SMProject.profileDir(root, state.userProfile.id) + '/feedback';
        await window.__TAURI__.fs.mkdir(dir, { recursive: true });
        await window.__TAURI__.fs.writeTextFile(dir + '/_profile.json', JSON.stringify(state.userProfile));
        await window.__TAURI__.fs.writeTextFile(dir + '/' + entry.id + '.json', JSON.stringify(entry));
      } catch (e) { console.warn('[feedback] publish to shared folder failed', e); }
    }
    return entry;
  }

  // ---- Pull from teammates (mirrors checkSharedUpdates/pullAndMerge) ----
  async function checkIncomingFeedback() {
    if (!tauriOk() || !window.SMProject || !window.SMProject.getSyncFolder) return [];
    var root = window.SMProject.getSyncFolder();
    if (!root) return [];
    var localIds = (await readAllLocal()).map(function (e) { return e.id; });
    var out = [];
    try {
      var entries = await window.__TAURI__.fs.readDir(root);
      for (var i = 0; i < entries.length; i++) {
        var e = entries[i];
        if (e.name === state.userProfile.id) continue; // your own — already local
        var fbDir = root.replace(/[\\/]+$/, '') + '/' + e.name + '/feedback';
        try {
          var sub = await window.__TAURI__.fs.readDir(fbDir);
          var files = sub.filter(function (x) { return /^fb_.*\.json$/.test(x.name); });
          for (var j = 0; j < files.length; j++) {
            var id = files[j].name.replace(/\.json$/, '');
            if (localIds.indexOf(id) >= 0) continue; // already imported
            try {
              var data = JSON.parse(await window.__TAURI__.fs.readTextFile(fbDir + '/' + files[j].name));
              if (data.projectKey !== projectKey()) continue; // feedback for a DIFFERENT project sharing the same folder
              out.push(data);
            } catch (e2) {}
          }
        } catch (e3) {}
      }
    } catch (e4) { console.warn('[feedback] check incoming failed', e4); }
    return out;
  }
  // Copies a remote-checked entry into the LOCAL store as 'pending' — never
  // 'approved' directly, no matter what the remote entry's own status says
  // (a teammate's local auto-approval means nothing on YOUR machine; only
  // you approving it here does).
  async function importIncoming(entry) {
    var copy = JSON.parse(JSON.stringify(entry));
    copy.status = 'pending';
    copy.origin = entry.author ? entry.author.id : 'remote';
    await writeLocal(copy);
    return copy;
  }
  async function pullAllIncoming() {
    var incoming = await checkIncomingFeedback();
    var imported = [];
    for (var i = 0; i < incoming.length; i++) imported.push(await importIncoming(incoming[i]));
    return imported;
  }

  async function approveFeedback(id) {
    var all = await readAllLocal();
    var entry = all.find(function (e) { return e.id === id; });
    if (!entry) return null;
    entry.status = 'approved';
    await writeLocal(entry);
    return entry;
  }
  async function resolveFeedback(id, resolutionText) {
    var all = await readAllLocal();
    var entry = all.find(function (e) { return e.id === id; });
    if (!entry) return null;
    entry.status = 'resolved';
    entry.resolution = resolutionText || '';
    entry.resolvedAt = Date.now();
    await writeLocal(entry);
    return entry;
  }
  async function deleteFeedback(id) { await deleteLocal(id); }

  window.SMFeedback = {
    logAction: logAction,
    submitFeedback: submitFeedback,
    readAllLocal: readAllLocal,
    checkIncomingFeedback: checkIncomingFeedback,
    importIncoming: importIncoming,
    pullAllIncoming: pullAllIncoming,
    approveFeedback: approveFeedback,
    resolveFeedback: resolveFeedback,
    deleteFeedback: deleteFeedback,
  };
})();
