// ---- Kitsu (zou/CGWire) pipeline integration (Phase 4) ----
// Separate workstream from the team-review system (profiles/revisions/sync):
// this is the app's first outbound network code. Endpoint paths below were
// verified against zou's own Python client (gazu — github.com/cgwire/gazu,
// files shot.py/project.py/task.py) rather than assumed, since the public
// docs at zou.cg-wire.com/api are marked outdated. Field names inside a
// shot's `data` dict (fps/deadline) are NOT guaranteed by the schema across
// studio configs, so every read here degrades gracefully with a fallback
// instead of throwing — this integration has not been exercised against a
// real or sandboxed Kitsu server and should be smoke-tested against one
// before relying on it in production.
//
// Requests go through the Tauri HTTP plugin (window.__TAURI__.http.fetch),
// NOT the page's own fetch() — a same-origin CSP (connect-src 'self' ipc: …)
// blocks the browser fetch from ever reaching an external Kitsu server, but
// plugin-http requests are proxied through the Rust backend and aren't
// subject to that page-level CSP. See src-tauri/capabilities/default.json's
// http:default scope (wildcarded — the server URL is only known at runtime,
// entered by the user, same trust model as SMProject's folder picker).
(function () {
  function kitsuOk() { return typeof window.__TAURI__ !== 'undefined' && !!window.__TAURI__.http; }
  var SESSION_KEY = 'sm-kitsu-session';

  function loadSession() {
    try { return JSON.parse(sessionStorage.getItem(SESSION_KEY) || 'null'); } catch (e) { return null; }
  }
  function saveSession(s) {
    try { if (s) sessionStorage.setItem(SESSION_KEY, JSON.stringify(s)); else sessionStorage.removeItem(SESSION_KEY); } catch (e) { }
  }
  // Session-only (sessionStorage, cleared when the app closes), not
  // persisted to localStorage like the user profile — an access token is a
  // bearer credential and shouldn't outlive the running app on disk.
  state.kitsuSession = loadSession();
  // Set only when the current project was opened FROM a Kitsu shot; drives
  // whether the "Publier vers Kitsu" button appears. NOT part of
  // exportJSON/importJSON — reconnecting to the right shot after reopening
  // a saved .json file is a nice-to-have for later, not v1 scope.
  state.kitsuShot = null;

  async function apiRequest(session, path, opts) {
    opts = opts || {};
    var url = session.serverUrl + '/api' + path;
    var headers = Object.assign({}, opts.headers || {});
    if (session.accessToken) headers['Authorization'] = 'Bearer ' + session.accessToken;
    var res = await window.__TAURI__.http.fetch(url, {
      method: opts.method || 'GET',
      headers: headers,
      body: opts.body
    });
    if (!res.ok) {
      var text = '';
      try { text = await res.text(); } catch (e) { }
      throw new Error('Kitsu (' + res.status + ') ' + (text || res.statusText || path));
    }
    return res.json();
  }

  async function login(serverUrl, email, password) {
    if (!kitsuOk()) throw new Error('Connexion Kitsu disponible uniquement dans l\'app desktop');
    var base = serverUrl.replace(/\/+$/, '');
    var form = new URLSearchParams();
    form.set('email', email);
    form.set('password', password);
    var res = await window.__TAURI__.http.fetch(base + '/api/auth/login', {
      method: 'POST',
      headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
      body: form.toString()
    });
    if (!res.ok) throw new Error('Échec de connexion (' + res.status + ') — vérifie l\'URL, l\'email et le mot de passe');
    var data = await res.json();
    if (!data.access_token) throw new Error('Réponse de connexion inattendue (pas de token)');
    var session = {
      serverUrl: base,
      accessToken: data.access_token,
      personName: (data.user && (data.user.full_name || (data.user.first_name || '') + ' ' + (data.user.last_name || ''))) || email,
      personId: data.user && data.user.id
    };
    state.kitsuSession = session;
    saveSession(session);
    return session;
  }
  function logout() { state.kitsuSession = null; saveSession(null); }

  function listProjects(session) { return apiRequest(session, '/projects/open'); }
  function listSequences(session, projectId) { return apiRequest(session, '/projects/' + projectId + '/sequences'); }
  function listShots(session, projectId) { return apiRequest(session, '/projects/' + projectId + '/shots'); }
  function listShotsForSequence(session, sequenceId) { return apiRequest(session, '/sequences/' + sequenceId + '/shots'); }
  function getShot(session, shotId) { return apiRequest(session, '/shots/' + shotId); }
  function listShotTasks(session, shotId) { return apiRequest(session, '/shots/' + shotId + '/tasks'); }
  function listTaskStatuses(session) { return apiRequest(session, '/task-status'); }
  function listShotPreviewFiles(session, shotId) {
    return apiRequest(session, '/shots/' + shotId + '/preview-files').catch(function () { return []; });
  }

  // Best-effort metadata extraction — the shot schema's `data` dict is
  // free-form per studio, so every field is tried against a few plausible
  // locations before falling back to a sane default.
  function pick(obj, paths, fallback) {
    for (var i = 0; i < paths.length; i++) {
      var v = paths[i].split('.').reduce(function (o, k) { return o && o[k] !== undefined ? o[k] : undefined; }, obj);
      if (v !== undefined && v !== null && v !== '') return v;
    }
    return fallback;
  }
  function shotMetadata(shot, project) {
    var fps = pick(shot, ['data.fps', 'data.frame_rate', 'fps'], pick(project, ['fps', 'data.fps'], 24));
    var frameCount = pick(shot, ['nb_frames', 'data.nb_frames'], null);
    var frameIn = pick(shot, ['data.frame_in'], 0);
    var frameOut = pick(shot, ['data.frame_out'], null);
    if (!frameCount && frameOut !== null) frameCount = (Number(frameOut) - Number(frameIn)) + 1;
    if (!frameCount) frameCount = Math.round(Number(fps) * 5); // same default newProject() uses
    var resolutionStr = pick(shot, ['data.resolution'], pick(project, ['resolution', 'data.resolution'], null));
    var w = 1920, h = 1080;
    if (resolutionStr && /^\d+x\d+$/i.test(resolutionStr)) {
      var parts = resolutionStr.split(/x/i);
      w = parseInt(parts[0], 10); h = parseInt(parts[1], 10);
    }
    var deadline = pick(shot, ['data.deadline', 'data.end_date'], null);
    return { fps: Number(fps) || 24, frameCount: Math.max(1, Math.round(frameCount)), w: w, h: h, deadline: deadline };
  }

  // Sets up the project from a chosen shot: canvas size/fps/frame count
  // from Kitsu, and a Rough/Clean/Color layer structure instead of the
  // default single "Layer 1" — the expected pipeline structure per the
  // original spec. Does NOT attempt full asset-casting/character-model
  // import (that needs the casting API and per-asset preview resolution,
  // both unverified) — the shot's own preview-file history is surfaced
  // read-only instead, see openShotResult.previews.
  async function openShot(session, project, sequence, shot) {
    var meta = shotMetadata(shot, project);
    window.SMProject.newProject({ w: meta.w, h: meta.h, fps: meta.fps, name: (project.name || 'Kitsu') + ' — ' + shot.name });
    state.totalFrames = meta.frameCount;
    state.waIn = 0; state.waOut = meta.frameCount - 1;
    window._waIn = 0; window._waOut = meta.frameCount - 1; window._totalF = meta.frameCount;
    state.layers[0].frames = state.layers[0].frames.slice(0, meta.frameCount);
    while (state.layers[0].frames.length < meta.frameCount) state.layers[0].frames.push({ strokes: [], isKeyframe: false, isInterpolated: false });
    state.layers[0].name = 'Rough';
    var cleanIdx = createUserLayer('Clean');
    var colorIdx = createUserLayer('Color');
    activateUL(0);
    loadFrame(0); renderOS(); renderArcs(); updateUI(); if (window.renderSymbolTabs) renderSymbolTabs();

    var tasks = [];
    try { tasks = await listShotTasks(session, shot.id); } catch (e) { }
    var previews = [];
    try { previews = await listShotPreviewFiles(session, shot.id); } catch (e) { }

    state.kitsuShot = {
      session: session, shotId: shot.id, shotName: shot.name,
      projectId: project.id, projectName: project.name,
      sequenceName: sequence ? sequence.name : null,
      taskId: tasks.length ? tasks[0].id : null,
      taskName: tasks.length ? tasks[0].task_type_name : null,
      deadline: meta.deadline
    };
    return { meta: meta, tasks: tasks, previews: previews };
  }

  function nextVersionLabel(existingCount) {
    return 'v' + String(existingCount + 1).padStart(3, '0');
  }

  // Renders the current timeline to MP4 (reusing export.js's ffmpeg
  // pipeline, no save dialog — see exportMP4Silent), uploads it as a new
  // preview on the shot's active task, and moves the task to a
  // review-like status if one can be found by name (honor-system best
  // effort: exact status short_names/workflow vary per studio, so this
  // never hard-fails the publish over a status match miss).
  async function publishToKitsu(onProgress) {
    if (!state.kitsuShot) throw new Error('Ce projet n\'a pas été ouvert depuis Kitsu');
    if (!window.SMExport || !window.SMExport.isAvailable()) throw new Error('Export MP4 disponible uniquement dans l\'app desktop');
    var ks = state.kitsuShot, session = ks.session;
    if (!ks.taskId) throw new Error('Aucune tâche trouvée sur ce shot — impossible de publier');

    var previews = [];
    try { previews = await listShotPreviewFiles(session, ks.shotId); } catch (e) { }
    var versionLabel = nextVersionLabel(previews.length);

    var tmpDirFn = window.__TAURI__.path && window.__TAURI__.path.tempDir;
    var tmpDir = tmpDirFn ? await tmpDirFn() : null;
    var outPath = (tmpDir || '') + 'sm-kitsu-' + Date.now() + '.mp4';
    if (onProgress) onProgress('render');
    var renderRes = await window.SMExport.exportMP4Silent(outPath, {});
    if (!renderRes || !renderRes.ok) throw new Error('Échec du rendu MP4');

    if (onProgress) onProgress('upload');
    var bytes = await window.__TAURI__.fs.readFile(outPath);

    var commentBody = { task_status_id: null, comment: 'Publié depuis Nemo (' + versionLabel + ')' };
    try {
      var statuses = await listTaskStatuses(session);
      var reviewStatus = statuses.find(function (s) {
        var n = (s.short_name || s.name || '').toLowerCase();
        return n.indexOf('review') >= 0 || n === 'wfa' || n.indexOf('waiting') >= 0;
      });
      if (reviewStatus) commentBody.task_status_id = reviewStatus.id;
    } catch (e) { }
    if (!commentBody.task_status_id) delete commentBody.task_status_id; // don't send a null id if we couldn't resolve one

    var comment = await apiRequest(session, '/actions/tasks/' + ks.taskId + '/comment', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(commentBody)
    });
    var previewFile = await apiRequest(session, '/actions/tasks/' + ks.taskId + '/comments/' + comment.id + '/add-preview', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ revision: previews.length + 1 })
    });

    var form = new FormData();
    form.append('file', new Blob([bytes], { type: 'video/mp4' }), versionLabel + '.mp4');
    var uploadRes = await window.__TAURI__.http.fetch(session.serverUrl + '/api/pictures/preview-files/' + previewFile.id, {
      method: 'POST',
      headers: { 'Authorization': 'Bearer ' + session.accessToken },
      body: form
    });
    if (!uploadRes.ok) throw new Error('Échec de l\'envoi du fichier (' + uploadRes.status + ')');

    if (onProgress) onProgress('done');
    return { ok: true, version: versionLabel, statusChanged: !!commentBody.task_status_id };
  }

  // ---- Cross-app file handoff (e.g. Nemo's After Effects camera .jsx,
  // or a future Rive export) ----
  // Kitsu's "Output Files" are metadata-only records that point at a path
  // on a shared pipeline filesystem (confirmed against gazu's own
  // files.py: every output-file function either builds/reads a path or
  // updates metadata — there is no upload-the-bytes endpoint at all,
  // unlike preview files). Nemo has no shared filesystem to assume, so
  // this uses "Working Files" instead — same task-scoped versioned-file
  // concept, but with a real PUT .../working-files/{id}/file upload
  // endpoint (mirrors the exact FormData pattern publishToKitsu already
  // uses for preview uploads). This is the one piece of the "reconstitute
  // the plan from another app, in the target app's own format" pipeline
  // Kitsu itself can actually do: it stores and versions the file, tagged
  // by task and software — it does NOT know how to interpret or convert
  // it. The After Effects side still has to fetch this working file and
  // run it manually (or via a future dedicated AE panel) — no official
  // Kitsu↔After Effects addon exists to automate that half.
  async function getOrCreateSoftware(session, name, shortName, ext) {
    var list = [];
    try { list = await apiRequest(session, '/softwares'); } catch (e) { }
    var found = (list || []).find(function (s) { return (s.name || '').toLowerCase() === name.toLowerCase(); });
    if (found) return found;
    return apiRequest(session, '/data/softwares', {
      method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ name: name, short_name: shortName, file_extension: ext })
    });
  }
  async function pushWorkingFile(session, taskId, opts) {
    var wf = await apiRequest(session, '/data/tasks/' + taskId + '/working-files/new', {
      method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        name: opts.name || 'main', comment: opts.comment || '',
        software_id: opts.softwareId || undefined, revision: 0
      })
    });
    var form = new FormData();
    form.append('file', new Blob([opts.bytes], { type: opts.mimeType || 'application/octet-stream' }), opts.filename);
    var uploadRes = await window.__TAURI__.http.fetch(session.serverUrl + '/api/data/working-files/' + wf.id + '/file', {
      method: 'PUT',
      headers: { 'Authorization': 'Bearer ' + session.accessToken },
      body: form
    });
    if (!uploadRes.ok) throw new Error('Échec de l\'envoi du fichier vers Kitsu (' + uploadRes.status + ')');
    return wf;
  }
  // filename/bytes/mimeType describe the file to push; softwareName tags
  // it (e.g. "After Effects") so whoever downloads it from Kitsu can tell
  // what it's for without opening it.
  async function pushWorkingFileToCurrentShot(filename, bytes, mimeType, softwareName, label) {
    if (!state.kitsuShot) throw new Error('Ce projet n\'a pas été ouvert depuis Kitsu');
    if (!state.kitsuShot.taskId) throw new Error('Aucune tâche trouvée sur ce shot — impossible de publier');
    var session = state.kitsuShot.session;
    var software = null;
    if (softwareName) {
      try { software = await getOrCreateSoftware(session, softwareName, softwareName.slice(0, 3).toLowerCase(), ''); } catch (e) { }
    }
    return pushWorkingFile(session, state.kitsuShot.taskId, {
      name: label || filename.replace(/\.[^.]+$/, ''),
      comment: 'Exporté depuis Nemo',
      softwareId: software && software.id,
      bytes: bytes, filename: filename, mimeType: mimeType
    });
  }

  window.SMKitsu = {
    isAvailable: kitsuOk,
    login: login, logout: logout,
    getSession: function () { return state.kitsuSession; },
    listProjects: listProjects, listSequences: listSequences, listShots: listShots,
    listShotsForSequence: listShotsForSequence, getShot: getShot,
    openShot: openShot, publish: publishToKitsu,
    isOpenedFromKitsu: function () { return !!state.kitsuShot; },
    getCurrentShot: function () { return state.kitsuShot; },
    pushWorkingFile: pushWorkingFileToCurrentShot
  };
})();

// ---- Kitsu UI wiring — separate IIFE, talks to window.SMKitsu only ----
(function () {
  var nav = { level: 'projects', project: null, sequence: null, items: [] };

  function el(id) { return document.getElementById(id); }
  function showModal() { el('kitsu-modal').style.display = 'flex'; }
  function hideModal() { el('kitsu-modal').style.display = 'none'; }
  function showStep(step) {
    el('kitsu-step-login').style.display = step === 'login' ? '' : 'none';
    el('kitsu-step-browse').style.display = step === 'browse' ? '' : 'none';
  }
  function loginError(msg) {
    var e = el('kitsu-login-error');
    if (!msg) { e.style.display = 'none'; return; }
    e.textContent = msg; e.style.display = '';
  }

  function breadcrumbText() {
    var parts = ['Projets'];
    if (nav.project) parts.push(nav.project.name);
    if (nav.sequence) parts.push(nav.sequence.name);
    return parts.join(' / ');
  }

  function renderRow(label, sub, onClick) {
    var row = document.createElement('div');
    row.style.cssText = 'display:flex;flex-direction:column;padding:7px 8px;border-radius:4px;cursor:pointer;font-size:11px';
    row.onmouseenter = function () { row.style.background = 'var(--panel3)'; };
    row.onmouseleave = function () { row.style.background = ''; };
    var top = document.createElement('div'); top.textContent = label;
    row.appendChild(top);
    if (sub) { var s = document.createElement('div'); s.style.cssText = 'font-size:9px;color:var(--text-dim)'; s.textContent = sub; row.appendChild(s); }
    row.addEventListener('click', onClick);
    return row;
  }

  async function loadProjects() {
    nav.level = 'projects'; nav.project = null; nav.sequence = null;
    el('kitsu-breadcrumb').textContent = breadcrumbText();
    var list = el('kitsu-list'); list.innerHTML = '<div style="font-size:10px;color:var(--text-dim)">Chargement…</div>';
    try {
      var projects = await window.SMKitsu.listProjects(window.SMKitsu.getSession());
      list.innerHTML = '';
      if (!projects.length) { list.innerHTML = '<div style="font-size:10px;color:var(--text-dim)">Aucun projet ouvert sur ce serveur.</div>'; return; }
      projects.forEach(function (p) {
        list.appendChild(renderRow(p.name, null, function () { loadSequences(p); }));
      });
    } catch (e) {
      list.innerHTML = '<div style="font-size:10px;color:#ff6b6b">' + e.message + '</div>';
    }
  }
  async function loadSequences(project) {
    nav.level = 'sequences'; nav.project = project; nav.sequence = null;
    el('kitsu-breadcrumb').textContent = breadcrumbText();
    var list = el('kitsu-list'); list.innerHTML = '<div style="font-size:10px;color:var(--text-dim)">Chargement…</div>';
    try {
      var seqs = await window.SMKitsu.listSequences(window.SMKitsu.getSession(), project.id);
      list.innerHTML = '';
      list.appendChild(renderRow('Tous les shots (sans séquence)', null, function () { loadShots(project, null); }));
      seqs.forEach(function (s) {
        list.appendChild(renderRow(s.name, null, function () { loadShots(project, s); }));
      });
    } catch (e) {
      list.innerHTML = '<div style="font-size:10px;color:#ff6b6b">' + e.message + '</div>';
    }
  }
  async function loadShots(project, sequence) {
    nav.level = 'shots'; nav.project = project; nav.sequence = sequence;
    el('kitsu-breadcrumb').textContent = breadcrumbText();
    var list = el('kitsu-list'); list.innerHTML = '<div style="font-size:10px;color:var(--text-dim)">Chargement…</div>';
    try {
      var session = window.SMKitsu.getSession();
      var shots = sequence ? await window.SMKitsu.listShotsForSequence(session, sequence.id) : await window.SMKitsu.listShots(session, project.id);
      list.innerHTML = '';
      if (!shots.length) { list.innerHTML = '<div style="font-size:10px;color:var(--text-dim)">Aucun shot.</div>'; return; }
      shots.forEach(function (sh) {
        list.appendChild(renderRow(sh.name, sh.description || null, function () { pickShot(project, sequence, sh); }));
      });
    } catch (e) {
      list.innerHTML = '<div style="font-size:10px;color:#ff6b6b">' + e.message + '</div>';
    }
  }
  async function pickShot(project, sequence, shot) {
    var list = el('kitsu-list');
    list.innerHTML = '<div style="font-size:10px;color:var(--text-dim)">Ouverture du shot…</div>';
    try {
      var res = await window.SMKitsu.openShot(window.SMKitsu.getSession(), project, sequence, shot);
      hideModal(); hideStartScreen(); ensureInitialTab();
      updateKitsuShotUI();
      var previewNote = res.previews && res.previews.length ? (' — ' + res.previews.length + ' référence(s) publiée(s) visibles dans l\'historique Kitsu') : '';
      showToast('Shot ouvert : ' + shot.name + ' (' + res.meta.w + '×' + res.meta.h + ', ' + res.meta.fps + 'fps, ' + res.meta.frameCount + ' frames)' + previewNote);
    } catch (e) {
      list.innerHTML = '<div style="font-size:10px;color:#ff6b6b">' + e.message + '</div>';
    }
  }

  function backOneLevel() {
    if (nav.level === 'shots') loadSequences(nav.project);
    else if (nav.level === 'sequences') loadProjects();
    else hideModal();
  }

  function updateKitsuShotUI() {
    var row = el('kitsu-shot-row'), pubRow = el('kitsu-publish-row');
    var ks = window.SMKitsu.getCurrentShot();
    if (!row || !pubRow) return;
    if (ks) {
      row.style.display = '';
      row.textContent = 'Kitsu: ' + ks.projectName + (ks.sequenceName ? ' / ' + ks.sequenceName : '') + ' / ' + ks.shotName + (ks.taskName ? ' (' + ks.taskName + ')' : '');
      pubRow.style.display = '';
    } else {
      row.style.display = 'none';
      pubRow.style.display = 'none';
    }
  }

  function initKitsuUI() {
    function openEntry() {
      showModal();
      if (window.SMKitsu.getSession()) { showStep('browse'); loadProjects(); }
      else showStep('login');
    }
    var startCard = el('start-kitsu');
    if (startCard) startCard.addEventListener('click', openEntry);
    var openBtn = el('btn-kitsu-open');
    if (openBtn) openBtn.addEventListener('click', openEntry);
    var closeBtn = el('kitsu-close');
    if (closeBtn) closeBtn.addEventListener('click', hideModal);
    var modal = el('kitsu-modal');
    if (modal) modal.addEventListener('click', function (e) { if (e.target === modal) hideModal(); });

    var loginBtn = el('kitsu-login-btn');
    if (loginBtn) loginBtn.addEventListener('click', function () {
      loginError(null);
      var url = el('kitsu-url').value.trim(), email = el('kitsu-email').value.trim(), pass = el('kitsu-password').value;
      if (!url || !email || !pass) { loginError(window.SM.t('kitsuLoginMissingFields')); return; }
      loginBtn.disabled = true; loginBtn.textContent = window.SM.t('kitsuConnecting');
      window.SMKitsu.login(url, email, pass).then(function () {
        loginBtn.disabled = false; loginBtn.textContent = window.SM.t('btnLogin');
        showStep('browse'); loadProjects();
      }).catch(function (e) {
        loginBtn.disabled = false; loginBtn.textContent = window.SM.t('btnLogin');
        loginError(e.message);
      });
    });
    var backBtn = el('kitsu-back-btn');
    if (backBtn) backBtn.addEventListener('click', backOneLevel);
    var logoutBtn = el('kitsu-logout-btn');
    if (logoutBtn) logoutBtn.addEventListener('click', function () { window.SMKitsu.logout(); showStep('login'); });

    var publishBtn = el('btn-kitsu-publish');
    if (publishBtn) publishBtn.addEventListener('click', function () {
      publishBtn.disabled = true; var origText = publishBtn.textContent;
      window.SMKitsu.publish(function (phase) {
        publishBtn.textContent = phase === 'render' ? window.SM.t('kitsuPublishRenderPhase') : phase === 'upload' ? window.SM.t('kitsuPublishUploadPhase') : origText;
      }).then(function (res) {
        publishBtn.disabled = false; publishBtn.textContent = origText;
        showToast(window.SM.t('kitsuPublishSuccess').replace('{v}', res.version) + (res.statusChanged ? window.SM.t('kitsuPublishStatusChanged') : ''));
      }).catch(function (e) {
        publishBtn.disabled = false; publishBtn.textContent = origText;
        showToast(window.SM.t('kitsuPublishError').replace('{e}', e.message));
      });
    });

    updateKitsuShotUI();
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', initKitsuUI); else initKitsuUI();
})();
