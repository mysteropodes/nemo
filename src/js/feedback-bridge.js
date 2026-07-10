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
  var CLICK_LOG_MAX = 200;

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

  // ---- Raw click log (session-only, module-local — deliberately NOT on
  // `state`, since this is UI-interaction noise, not project or even
  // action-log-grade semantic data) ----
  // logAction() above only fires for actual content mutations (pushUndo's
  // choke point); this fires on EVERY pointerdown across the whole app —
  // UI chrome buttons, toggles, canvas — so a feedback report comes with
  // exactly which element was clicked, not just "user did something with
  // the draw tool". Captured on `document` with capture:true so it sees
  // the click before any tool bridge's stopImmediatePropagation() can
  // swallow it (see draw-bridge.js/select-bridge.js's own capture-phase
  // interception pattern — this listener runs even earlier, at the root).
  var _clickLog = [];
  function describeElement(el) {
    if (!el || el.nodeType !== 1) return null;
    var parts = [], cur = el, depth = 0;
    while (cur && cur.nodeType === 1 && depth < 4) {
      var seg = cur.tagName.toLowerCase();
      if (cur.id) seg += '#' + cur.id;
      else if (cur.className && typeof cur.className === 'string' && cur.className.trim()) {
        seg += '.' + cur.className.trim().split(/\s+/).slice(0, 2).join('.');
      }
      parts.unshift(seg);
      if (cur.id) break; // an id anchors the path uniquely enough — no need to climb further
      cur = cur.parentElement;
      depth++;
    }
    var data = null;
    if (el.dataset && Object.keys(el.dataset).length) {
      data = {};
      Object.keys(el.dataset).forEach(function (k) { data[k] = el.dataset[k]; });
    }
    return {
      tag: el.tagName.toLowerCase(),
      id: el.id || null,
      cls: (el.className && typeof el.className === 'string') ? el.className : null,
      text: (el.textContent || '').trim().slice(0, 40) || null,
      title: el.title || null,
      data: data,
      path: parts.join(' > '),
    };
  }
  function logClick(e) {
    var desc = describeElement(e.target);
    if (!desc) return;
    var last = _clickLog[_clickLog.length - 1];
    if (last && last.el && last.el.path === desc.path && last.el.id === desc.id) {
      last.count = (last.count || 1) + 1;
      last.t = Date.now();
      return;
    }
    _clickLog.push({ t: Date.now(), x: e.clientX, y: e.clientY, tool: (window.state ? state.tool : null), el: desc, count: 1 });
    if (_clickLog.length > CLICK_LOG_MAX) _clickLog.shift();
  }
  document.addEventListener('pointerdown', logClick, true);
  function recentClickTrail(n) {
    return _clickLog.slice(Math.max(0, _clickLog.length - (n || 20)));
  }
  // ---- Precise start/stop recording (timeline.js's comment-popover Record
  // button) — a "last N" trail is a guess at what's relevant; bracketing an
  // explicit start/stop around the actual repro gesture captures exactly
  // that and nothing else. Both logs are simple append-only arrays, so a
  // "since" slice is just the array length at record-start.
  function actionLogMark() { return (state.actionLog || []).length; }
  function clickLogMark() { return _clickLog.length; }
  function actionTrailSince(idx) { return (state.actionLog || []).slice(idx || 0); }
  function clickTrailSince(idx) { return _clickLog.slice(idx || 0); }

  // ---- Storage tiers, tried in order: Tauri fs (real app, real OS folder)
  // -> local dev server (scripts/dev_server.py's /__feedback/* endpoints —
  // a real file too, just reachable over fetch() instead of Tauri's fs
  // plugin, for the ordinary `python3 -m http.server`-replacement dev
  // preview) -> localStorage (last resort — a live browser tab only, not
  // reachable outside an active eval session, but keeps this working even
  // if the plain http.server is what's actually running). ----
  async function localDir() {
    var base = await window.__TAURI__.path.appDataDir();
    return base.replace(/[\\/]+$/, '') + '/feedback/' + projectKey();
  }
  function lsKey() { return 'sm-feedback-' + projectKey(); }
  function lsReadAll() { try { return JSON.parse(localStorage.getItem(lsKey()) || '[]'); } catch (e) { return []; } }
  function lsWriteAll(list) { try { localStorage.setItem(lsKey(), JSON.stringify(list)); } catch (e) {} }

  var _devServerChecked = null;
  async function devServerAvailable() {
    if (tauriOk()) return false; // Tauri always wins when present, dev server is browser-preview-only
    if (_devServerChecked !== null) return _devServerChecked;
    try {
      var r = await fetch('/__feedback/ping');
      _devServerChecked = r.ok;
    } catch (e) { _devServerChecked = false; }
    return _devServerChecked;
  }

  async function readAllLocal() {
    if (tauriOk()) {
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
    if (await devServerAvailable()) {
      try {
        var res = await fetch('/__feedback/list?projectKey=' + encodeURIComponent(projectKey()));
        if (res.ok) return await res.json();
      } catch (e) { console.warn('[feedback] dev-server list failed, falling back to localStorage', e); }
    }
    return lsReadAll();
  }
  async function writeLocal(entry) {
    if (tauriOk()) {
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
      return;
    }
    if (await devServerAvailable()) {
      try {
        var res = await fetch('/__feedback/save', { method: 'POST', body: JSON.stringify(entry) });
        if (res.ok) return;
      } catch (e) { console.warn('[feedback] dev-server save failed, falling back to localStorage', e); }
    }
    var list = lsReadAll();
    var idx = list.findIndex(function (e) { return e.id === entry.id; });
    if (idx >= 0) list[idx] = entry; else list.unshift(entry);
    if (list.length > LOCAL_MAX) list = list.slice(0, LOCAL_MAX);
    lsWriteAll(list);
  }
  async function deleteLocal(id) {
    if (tauriOk()) {
      try { await window.__TAURI__.fs.remove((await localDir()) + '/' + id + '.json'); } catch (e) {}
      return;
    }
    if (await devServerAvailable()) {
      try {
        var res = await fetch('/__feedback/delete', { method: 'POST', body: JSON.stringify({ id: id, projectKey: projectKey() }) });
        if (res.ok) return;
      } catch (e) { console.warn('[feedback] dev-server delete failed, falling back to localStorage', e); }
    }
    lsWriteAll(lsReadAll().filter(function (e) { return e.id !== id; }));
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
      // A precise start/stop recording (opts.actionTrail/clickTrail, set by
      // the popover's Record button — see actionLogMark/clickLogMark below)
      // always wins over the generic "last 20" guess when one was made.
      actionTrail: opts.actionTrail || recentActionTrail(20),
      clickTrail: opts.clickTrail || recentClickTrail(20),
      recorded: !!(opts.actionTrail || opts.clickTrail),
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
    // Best-effort: beta testers have no shared Sync folder (that's a local/
    // network-drive mechanism for Cyril's own machines) — this is THEIR
    // transport instead: one GitHub Issue per feedback entry, in the public
    // mysteropodes/strokemotion-feedback repo. The actual HTTP call (and the
    // write-scoped token) lives entirely in Rust (submit_feedback_issue,
    // src-tauri/src/lib.rs) so the token never appears in this file or in
    // any devtools-visible fetch() — see that command's own comment.
    if (tauriOk()) {
      try {
        await publishToGitHubIssue(entry);
      } catch (e) { console.warn('[feedback] GitHub publish failed', e); }
    }
    return entry;
  }

  function fbTagLabelPlain(tag) {
    return { bug: 'bug', perf: 'perf', idee: 'idée', polish: 'polish' }[tag] || tag;
  }
  async function publishToGitHubIssue(entry) {
    var title = (entry.blocking ? '[BLOQUANT] ' : '') + (entry.note || '(sans titre)').slice(0, 80);
    var labels = ['pending'].concat((entry.tags || []).map(fbTagLabelPlain));
    if (entry.blocking) labels.push('blocking');
    var body = [
      '**Note**', entry.note || '(vide)', '',
      '**Contexte**',
      '- Frame: ' + entry.frame,
      '- Auteur: ' + (entry.author ? entry.author.name + ' (' + entry.author.role + ')' : 'inconnu'),
      '- Projet: ' + entry.projectKey,
      '- Enregistré précisément: ' + (entry.recorded ? 'oui' : 'non (dernières actions)'),
      '',
      '**Trail d\'actions**',
      '```json', JSON.stringify(entry.actionTrail || [], null, 2), '```',
      '',
      '**Trail de clics**',
      '```json', JSON.stringify(entry.clickTrail || [], null, 2), '```',
      '',
      '<!-- sm-feedback-id: ' + entry.id + ' -->',
    ].join('\n');
    var t = window.__TAURI__;
    await t.core.invoke('submit_feedback_issue', { title: title, body: body, labels: labels });
  }

  // ---- Dev-side triage over the GitHub feedback repo (Cyril only — a
  // beta tester's shipped copy of this app has these functions too, since
  // it's all one codebase, but they're useless to them without CYRIL'S OWN
  // GitHub token, which is never embedded/shipped — see setGithubTriageToken
  // below). Reading issues needs no auth at all (public repo); only
  // labeling/closing/commenting needs Cyril's token, entered once in
  // Réglages → Feedback and kept in localStorage on his own machine only. ----
  var GH_REPO = 'mysteropodes/strokemotion-feedback';
  var GH_TOKEN_KEY = 'sm-github-triage-token';
  function githubTriageToken() { try { return localStorage.getItem(GH_TOKEN_KEY) || ''; } catch (e) { return ''; } }
  function setGithubTriageToken(token) { try { localStorage.setItem(GH_TOKEN_KEY, token || ''); } catch (e) {} }

  async function fetchGithubIssues() {
    var res = await fetch('https://api.github.com/repos/' + GH_REPO + '/issues?state=all&per_page=100', {
      headers: { 'Accept': 'application/vnd.github+json' },
    });
    if (!res.ok) throw new Error('GitHub list failed: ' + res.status);
    var issues = await res.json();
    return issues.filter(function (i) { return !i.pull_request; }).map(function (i) {
      var m = /sm-feedback-id:\s*(\S+)\s*-->/.exec(i.body || '');
      return {
        number: i.number, id: m ? m[1] : ('gh_' + i.number),
        title: i.title, body: i.body, url: i.html_url,
        labels: (i.labels || []).map(function (l) { return l.name; }),
        state: i.state, createdAt: i.created_at,
      };
    });
  }
  function ghAuthHeaders() {
    var token = githubTriageToken();
    if (!token) throw new Error('Aucun token GitHub configuré (Réglages → Feedback)');
    return { 'Authorization': 'Bearer ' + token, 'Accept': 'application/vnd.github+json', 'Content-Type': 'application/json' };
  }
  async function patchGithubIssue(number, payload) {
    var res = await fetch('https://api.github.com/repos/' + GH_REPO + '/issues/' + number, {
      method: 'PATCH', headers: ghAuthHeaders(), body: JSON.stringify(payload),
    });
    if (!res.ok) throw new Error('GitHub update failed: ' + res.status + ' ' + await res.text());
    return res.json();
  }
  async function approveGithubIssue(number, currentLabels) {
    var labels = (currentLabels || []).filter(function (l) { return l !== 'pending'; });
    return patchGithubIssue(number, { labels: labels });
  }
  async function resolveGithubIssue(number, currentLabels, resolutionText) {
    if (resolutionText) {
      await fetch('https://api.github.com/repos/' + GH_REPO + '/issues/' + number + '/comments', {
        method: 'POST', headers: ghAuthHeaders(), body: JSON.stringify({ body: resolutionText }),
      });
    }
    var labels = (currentLabels || []).filter(function (l) { return l !== 'pending'; });
    if (labels.indexOf('resolved') < 0) labels.push('resolved');
    return patchGithubIssue(number, { labels: labels, state: 'closed' });
  }
  async function editGithubIssueBody(number, newBody) {
    return patchGithubIssue(number, { body: newBody });
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
    recentClickTrail: recentClickTrail,
    actionLogMark: actionLogMark,
    clickLogMark: clickLogMark,
    actionTrailSince: actionTrailSince,
    clickTrailSince: clickTrailSince,
    submitFeedback: submitFeedback,
    readAllLocal: readAllLocal,
    checkIncomingFeedback: checkIncomingFeedback,
    importIncoming: importIncoming,
    pullAllIncoming: pullAllIncoming,
    approveFeedback: approveFeedback,
    resolveFeedback: resolveFeedback,
    deleteFeedback: deleteFeedback,
    githubTriageToken: githubTriageToken,
    setGithubTriageToken: setGithubTriageToken,
    fetchGithubIssues: fetchGithubIssues,
    approveGithubIssue: approveGithubIssue,
    resolveGithubIssue: resolveGithubIssue,
    editGithubIssueBody: editGithubIssueBody,
  };
})();
