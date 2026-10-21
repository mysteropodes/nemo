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
  var ACTION_LOG_MAX = 150; // mirrors state.actionLog's own cap in app.js
  var CLICK_LOG_MAX = 200;

  function tauriOk() { return typeof window.__TAURI__ !== 'undefined'; }
  function projectKey() { return (window.SMProject && window.SMProject.getProjectKey()) || 'untitled-autosave'; }

  var local = NemoFeedbackLocal.create({
    available: tauriOk,
    tauri: function () { return window.__TAURI__; },
    project: function () { return window.SMProject; },
    profile: function () { return state.userProfile; },
    projectKey: projectKey,
    storage: function () { return localStorage; },
    hostname: function () { return location.hostname; },
    fetch: function (url, options) { return fetch(url, options); },
    warn: function () { console.warn.apply(console, arguments); },
  });
  var remote = NemoFeedbackRemote.create({
    available: tauriOk,
    // Worker only uses the desktop HTTP plugin; public/triage GitHub keeps plain fetch.
    workerFetch: function () {
      var t = window.__TAURI__;
      return (t && t.http && t.http.fetch) ? t.http.fetch : fetch;
    },
    fetch: function (url, options) { return fetch(url, options); },
    storage: function () { return localStorage; },
    title: function () { return document.title; },
    userAgent: function () { return navigator.userAgent; },
    onLine: function () { return navigator.onLine; },
    width: function () { return window.innerWidth; },
    height: function () { return window.innerHeight; },
    screenWidth: function () { return screen.width; },
    screenHeight: function () { return screen.height; },
    warn: function () { console.warn.apply(console, arguments); },
  });

  // ---- Action log (session-only trail feeding actionTrail below) ----
  // Called once per pushUndoLayers() (app.js/tweens.js's single choke point
  // before every meaningful mutation) — consecutive same-tool/same-frame
  // calls collapse into one entry with a running count, so a long freehand
  // drag or a burst of clicks doesn't flood the trail with duplicates.
  //
  // Optional `explicitTool` (2026-07, "récupère des données exploitable
  // pour résoudre les soucis"): StoryBoard (storyboard.js) and audio
  // tracks (audio-bridge.js) mutate state that ISN'T part of
  // layersSnapshotNow()'s undo snapshot (state.storyboard/state.audioTracks
  // aren't captured there), so they can't route through pushUndoLayers —
  // there'd be nothing real for undo to restore. Rather than leave those
  // two subsystems invisible to the log entirely, their own call sites log
  // directly with a descriptive string ("storyboard:addInstance",
  // "audio:deleteTrack", ...) instead of falling back to state.tool, which
  // would otherwise record whatever DRAWING tool happened to be selected —
  // misleading, not just uninformative, for an action that has nothing to
  // do with it. Every pre-existing call site (via pushUndoLayers) keeps
  // passing nothing and gets state.tool exactly as before.
  function logAction(explicitTool) {
    if (!state.actionLog) state.actionLog = [];
    var ld = state.layers[state.activeLayerIdx];
    var last = state.actionLog[state.actionLog.length - 1];
    var tool = explicitTool || state.tool, frame = state.currentFrame, layerName = ld ? ld.name : null;
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
      author: state.userProfile ? { id: state.userProfile.id, name: state.userProfile.name, color: state.userProfile.color, role: state.userProfile.role, email: state.userProfile.email || '' } : null,
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
      // Kept locally as data URLs for review even without Tauri/GitHub —
      // the GitHub copies (publishToGitHubIssue below) are separate,
      // uploaded files, not these same strings re-embedded. Array, not a
      // single field, since feedback #208 ("possibilité de... mettre
      // plusieurs image pour un seul et même feedback") — an OLDER stored
      // entry may still carry the old singular screenshotDataUrl instead;
      // every reader of this field (uploadScreenshotsIfAny below,
      // timeline.js's popover re-open) checks both shapes.
      screenshotDataUrls: opts.screenshotDataUrls || (opts.screenshotDataUrl ? [opts.screenshotDataUrl] : []),
    };
    await local.writeLocal(entry);
    // Best-effort: also publish to the shared team-sync folder (if
    // configured) so the project owner can pull it — reuses the EXACT
    // same root/<profileId>/ convention publishToShared() already writes
    // full project snapshots into, just a "feedback" subfolder instead of
    // the project-snapshot files, so the two never collide.
    if (tauriOk() && window.SMProject && window.SMProject.getSyncFolder && window.SMProject.getSyncFolder()) {
      try {
        await local.publishShared(entry);
      } catch (e) { console.warn('[feedback] publish to shared folder failed', e); }
    }
    // Best-effort: beta testers have no shared Sync folder (that's a local/
    // network-drive mechanism for Cyril's own machines) — this is THEIR
    // transport instead: one GitHub Issue per feedback entry, in
    // mysteropodes/nemo, posted through the nemo-feedback Worker (see
    // workerPost above and worker-feedback/README.md) — desktop and web
    // both go through it, so neither needs its own copy of the write token.
    // 2026-08: this used to be Tauri-only and silently no-op on web — a
    // tester's feedback looked "saved" but never reached anyone, see
    // worker-feedback/README for the fix.
    try {
      await remote.publishToGitHubIssue(entry);
    } catch (e) { console.warn('[feedback] GitHub publish failed', e); }
    return entry;
  }

  // ---- Pull from teammates (mirrors checkSharedUpdates/pullAndMerge) ----
  // Copies a remote-checked entry into the LOCAL store as 'pending' — never
  // 'approved' directly, no matter what the remote entry's own status says
  // (a teammate's local auto-approval means nothing on YOUR machine; only
  // you approving it here does).
  async function importIncoming(entry) {
    var copy = JSON.parse(JSON.stringify(entry));
    copy.status = 'pending';
    copy.origin = entry.author ? entry.author.id : 'remote';
    await local.writeLocal(copy);
    return copy;
  }
  async function pullAllIncoming() {
    var incoming = await local.checkIncomingFeedback();
    var imported = [];
    for (var i = 0; i < incoming.length; i++) imported.push(await importIncoming(incoming[i]));
    return imported;
  }

  async function approveFeedback(id) {
    var all = await local.readAllLocal();
    var entry = all.find(function (e) { return e.id === id; });
    if (!entry) return null;
    entry.status = 'approved';
    await local.writeLocal(entry);
    return entry;
  }
  async function resolveFeedback(id, resolutionText) {
    var all = await local.readAllLocal();
    var entry = all.find(function (e) { return e.id === id; });
    if (!entry) return null;
    entry.status = 'resolved';
    entry.resolution = resolutionText || '';
    entry.resolvedAt = Date.now();
    await local.writeLocal(entry);
    return entry;
  }
  async function deleteFeedback(id) { await local.deleteLocal(id); }

  window.SMFeedback = {
    logAction: logAction,
    recentClickTrail: recentClickTrail,
    actionLogMark: actionLogMark,
    clickLogMark: clickLogMark,
    actionTrailSince: actionTrailSince,
    clickTrailSince: clickTrailSince,
    submitFeedback: submitFeedback,
    readAllLocal: local.readAllLocal,
    checkIncomingFeedback: local.checkIncomingFeedback,
    importIncoming: importIncoming,
    pullAllIncoming: pullAllIncoming,
    approveFeedback: approveFeedback,
    resolveFeedback: resolveFeedback,
    deleteFeedback: deleteFeedback,
    githubTriageToken: remote.githubTriageToken,
    setGithubTriageToken: remote.setGithubTriageToken,
    fetchGithubIssues: remote.fetchGithubIssues,
    approveGithubIssue: remote.approveGithubIssue,
    resolveGithubIssue: remote.resolveGithubIssue,
    editGithubIssueBody: remote.editGithubIssueBody,
    deleteGithubIssue: remote.deleteGithubIssue,
  };
})();
