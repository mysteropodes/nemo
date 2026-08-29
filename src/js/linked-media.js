// ---- LINKED MEDIA (2026-08-29) ----
// Opt-in alternative to Nemo's default "everything embedded as base64"
// project format (see the persistence audit, commit ed136dc). ONE project-
// wide setting (state.mediaMode, Cyril: "un seul réglage" — not per-image):
// 'embedded' (default, unchanged) or 'linked'. Governs NEW image imports
// only — flipping the setting never retroactively converts already-
// embedded media (see this file's own relink flow for the manual escape
// hatch if a file DOES move).
//
// Two backends, both feeding the SAME desR()/serR() choke point (app.js):
//  - Desktop (Tauri): a linked raster carries a real filesystem path
//    (d.linkedPath). Mirrors native-video-bridge.js's existing nativeVideo
//    linking pattern exactly (readFile/relink dialog), just for the per-
//    frame raster-stroke shape images use instead of a layer-level decode
//    session.
//  - Web (no Tauri): there is NO path string a web page can hold — browsers
//    deliberately never expose one (File/FileSystemFileHandle carry no
//    accessible absolute path). A linked raster instead carries an opaque
//    id (d.linkedHandleId) pointing at a FileSystemFileHandle stashed in
//    IndexedDB via the existing SMIdb key/value store (idb-store.js) — a
//    FileSystemFileHandle is documented, shipped Chromium behavior as
//    structured-clone-serializable (MDN/spec), the plumbing here (SMIdb
//    put/get, the keying scheme) was verified live against a plain
//    serializable value; a REAL handle's own round-trip could not be
//    live-driven THIS session — showOpenFilePicker() opens a native OS
//    dialog with no automation surface (no DOM `<input>` behind it for CDP
//    to target), so obtaining a genuine handle needs a real user click,
//    same platform wall documented on requestPermission() below. Only
//    Chromium exposes window.showOpenFilePicker (isWebLinkingSupported) —
//    Safari/Firefox fall back to the classic <input type=file> embedded-
//    only flow, same graceful-degradation idiom as every other Tauri-only
//    feature here.
//
// CRITICAL PERSISTENCE RULE (CLAUDE.md §1): a linked raster's stroke dict
// NEVER carries `src` — only the reference (linkedPath/linkedHandleId) plus
// ordinary geometry. That omission IS the entire weight win. serR/desR
// (app.js) are the only two places that read/write this shape; this file
// only ever supplies the RESOLVED pixels back at render time, via a cache
// keyed on the reference, never by mutating what gets persisted.
(function () {
  function tauriOk() { return typeof window.__TAURI__ !== 'undefined'; }
  function isWebLinkingSupported() { return typeof window.showOpenFilePicker === 'function'; }

  function extOf(name) { var m = /\.([^.]+)$/.exec(name || ''); return m ? m[1].toLowerCase() : ''; }
  function mimeOf(ext) { return { png: 'image/png', jpg: 'image/jpeg', jpeg: 'image/jpeg', gif: 'image/gif', webp: 'image/webp', bmp: 'image/bmp', avif: 'image/avif' }[ext] || 'image/png'; }
  function bytesToBase64(bytes) {
    var binary = '', chunk = 0x8000;
    for (var i = 0; i < bytes.length; i += chunk) binary += String.fromCharCode.apply(null, bytes.subarray(i, i + chunk));
    return btoa(binary);
  }
  function fileToDataUrl(file) {
    return new Promise(function (resolve, reject) {
      var r = new FileReader();
      r.onload = function () { resolve(r.result); };
      r.onerror = function () { reject(r.error || new Error('lecture du fichier échouée')); };
      r.readAsDataURL(file);
    });
  }
  function naturalSizeFromDataUrl(dataUrl) {
    return new Promise(function (resolve) {
      var img = new Image();
      img.onload = function () { resolve({ w: img.naturalWidth || 1, h: img.naturalHeight || 1 }); };
      img.onerror = function () { resolve({ w: 1, h: 1 }); };
      img.src = dataUrl;
    });
  }
  function uid() { return 'lm' + Date.now().toString(36) + Math.random().toString(36).slice(2, 7); }

  // ---- IndexedDB handle store (web only) — reuses the app's existing
  // generic key/value store (idb-store.js) rather than opening a second
  // database, namespaced so it can't collide with the single-slot
  // 'nemo-auto' autosave key that store already serves. ----
  function putHandle(id, handle) { return window.SMIdb ? window.SMIdb.set('linkedHandle:' + id, handle) : Promise.reject(new Error('SMIdb indisponible')); }
  function getHandle(id) { return window.SMIdb ? window.SMIdb.get('linkedHandle:' + id) : Promise.resolve(null); }
  function removeHandle(id) { return window.SMIdb ? window.SMIdb.remove('linkedHandle:' + id) : Promise.resolve(); }

  // ---- resolve cache (both backends) — keyed on the persisted reference,
  // NOT on the resolved data: URL content (that's app.js's separate
  // _imgCache's job, keyed the other way around; the two compose: this
  // cache avoids re-reading the FILE every scrub tick, that one avoids
  // re-DECODING the same bytes every scrub tick). ----
  var _resolved = {};   // key -> data: URL string, once successfully read
  var _inFlight = {};   // key -> true while a resolve is in progress
  var _failed = {};     // key -> {reason:'missing'|'permission', at:ms} — cooldown, not a permanent verdict
  var _pendingPermission = {}; // handleId -> FileSystemFileHandle, needs a user click to (re)grant
  var RETRY_COOLDOWN_MS = 4000;

  function keyFor(d) {
    if (!d) return null;
    if (d.linkedPath) return 'p:' + d.linkedPath;
    if (d.linkedHandleId) return 'w:' + d.linkedHandleId;
    return null;
  }
  function getCachedSrc(d) { var k = keyFor(d); return k ? (_resolved[k] || null) : null; }
  // Primes the cache immediately after an import (the bytes were JUST read
  // for the thumbnail/natural-size probe) — avoids a blank-placeholder
  // flash on the very first render of a brand-new linked layer.
  function primeCache(d, dataUrl) { var k = keyFor(d); if (k && dataUrl) { _resolved[k] = dataUrl; delete _failed[k]; } }

  function _resolveDesktopPath(path) {
    if (!tauriOk()) return Promise.reject(new Error('nécessite l\'app Tauri'));
    // Prefer images.js's own readAsDataUrl (exposed as SM._readImageAsDataUrl)
    // — it already handles the PRO_IMAGE_EXTS ffmpeg conversion for tiff/exr/
    // psd/dpx, which a plain fs.readFile + mime-guess below would silently
    // corrupt (raw bytes ≠ a browser-decodable format for any of those). The
    // plain fallback only fires if images.js somehow isn't loaded.
    if (window.SM && window.SM._readImageAsDataUrl) return window.SM._readImageAsDataUrl(path);
    return window.__TAURI__.fs.readFile(path).then(function (bytes) {
      return 'data:' + mimeOf(extOf(path)) + ';base64,' + bytesToBase64(bytes);
    });
  }
  function _resolveWebHandle(handleId) {
    return getHandle(handleId).then(function (handle) {
      if (!handle) { throw new Error('handle introuvable (autre navigateur/machine, ou données locales effacées)'); }
      return handle.queryPermission({ mode: 'read' }).then(function (perm) {
        if (perm !== 'granted') {
          _pendingPermission[handleId] = handle;
          _updateBanner();
          var e = new Error('autorisation requise'); e.needsPermission = true; throw e;
        }
        delete _pendingPermission[handleId];
        return handle.getFile();
      }).then(function (file) { return fileToDataUrl(file); });
    });
  }

  // Rebuilds every materialized layer's identity stamp so the next
  // loadFrame() can't short-circuit via _canReuseMaterialized (app.js
  // §5quater) — necessary here because a resolve landing doesn't change any
  // STORED stroke dict (only this file's own out-of-band cache), so the
  // identity check would otherwise see "same strokes array, nothing dirty"
  // and skip rebuilding the placeholder Raster into the real picture.
  function _forceRebuildAllLayers() {
    if (window.userLayers) userLayers.forEach(function (l) { if (l) l._matStrokes = null; });
  }
  function _rerender() {
    _forceRebuildAllLayers();
    if (window.loadFrame) loadFrame(state.currentFrame);
  }

  function _entriesForKey(d) {
    return (state.mediaLibrary || []).filter(function (m) {
      return m.linked && ((d.linkedPath && m.path === d.linkedPath) || (d.linkedHandleId && m.webHandleId === d.linkedHandleId));
    });
  }
  function _markEntryBroken(d, reason) {
    var es = _entriesForKey(d); if (!es.length) return;
    es.forEach(function (m) { m.linkedBroken = true; m.linkedBrokenReason = reason; });
    if (window.SMMediaLibrary) SMMediaLibrary.reload();
  }
  function _markEntryOk(d) {
    var es = _entriesForKey(d); if (!es.length) return;
    var changed = false;
    es.forEach(function (m) { if (m.linkedBroken) changed = true; m.linkedBroken = false; m.linkedBrokenReason = null; });
    if (changed && window.SMMediaLibrary) SMMediaLibrary.reload();
  }

  // Called from desR (app.js) whenever a linked raster's cache misses.
  // Deduped in-flight per key, and cooled down after a failure so a scrub
  // through many frames of a permanently-missing file doesn't hammer
  // fs.readFile/IndexedDB every single tick (same "warn once" spirit as
  // native-video-bridge.js's nv.isWeb reopen guard).
  function resolveAsync(d) {
    var k = keyFor(d); if (!k) return;
    if (_inFlight[k]) return;
    if (_failed[k] && (Date.now() - _failed[k].at) < RETRY_COOLDOWN_MS) return;
    _inFlight[k] = true;
    var p = d.linkedPath ? _resolveDesktopPath(d.linkedPath) : _resolveWebHandle(d.linkedHandleId);
    p.then(function (dataUrl) {
      delete _inFlight[k]; delete _failed[k];
      _resolved[k] = dataUrl;
      _markEntryOk(d);
      _rerender();
    }).catch(function (e) {
      delete _inFlight[k];
      _failed[k] = { reason: (e && e.needsPermission) ? 'permission' : 'missing', at: Date.now() };
      _markEntryBroken(d, _failed[k].reason);
    });
  }

  // ---- banner (web permission re-grant) ----
  function _updateBanner() {
    var el = document.getElementById('linked-media-banner');
    if (!el) return;
    var n = Object.keys(_pendingPermission).length;
    el.classList.toggle('show', n > 0);
    var msg = document.getElementById('linked-media-banner-msg');
    if (msg && n > 0) {
      var base = (window.SM && SM.t) ? SM.t('linkedMediaBannerMsg') : 'Certains médias liés nécessitent une autorisation d\'accès dans ce navigateur.';
      msg.textContent = base + (n > 1 ? ' (' + n + ')' : '');
    }
  }
  // Fired from the banner's own click handler — a real user gesture is
  // REQUIRED for requestPermission() to resolve to anything but 'denied'
  // (hard platform rule, not something to work around). Iterates every
  // pending handle sequentially inside this same click's task; a browser
  // that only honors the FIRST requestPermission() per gesture still lets
  // the user click again for the rest — no worse than a normal repeated
  // action, never a silent failure.
  async function requestPermissionForAll() {
    var ids = Object.keys(_pendingPermission);
    for (var i = 0; i < ids.length; i++) {
      var id = ids[i], handle = _pendingPermission[id];
      try {
        var res = await handle.requestPermission({ mode: 'read' });
        if (res === 'granted') { delete _pendingPermission[id]; delete _failed['w:' + id]; }
      } catch (e) { /* stays pending — user can retry */ }
    }
    _updateBanner();
    _rerender();
  }

  // ---- import-time helpers (images.js calls these) ----
  // Desktop: read once for the immediate thumbnail/placement, prime the
  // cache with that exact result so the layer just created renders
  // instantly (no round-trip through resolveAsync for what we already have
  // in hand) — mirrors readAsDataUrl's role in images.js's embedded path.
  function readLinkedDesktop(path) { return _resolveDesktopPath(path); }

  // Web: opens the File System Access picker (a REAL user gesture — always
  // called from a button-click handler, same requirement as any native
  // dialog), stores each picked handle in IndexedDB, and returns everything
  // images.js needs to build a linked stroke right away.
  async function pickWebImages(multiple) {
    if (!isWebLinkingSupported()) throw new Error('File System Access API indisponible dans ce navigateur (Safari/Firefox) — utilisez le mode Intégrés ou Chrome/Edge.');
    var handles = await window.showOpenFilePicker({
      multiple: !!multiple,
      types: [{ description: 'Images', accept: { 'image/*': ['.png', '.jpg', '.jpeg', '.gif', '.webp', '.bmp', '.avif'] } }],
    });
    var out = [];
    for (var i = 0; i < handles.length; i++) {
      var handle = handles[i];
      var file = await handle.getFile();
      var dataUrl = await fileToDataUrl(file);
      var id = uid();
      await putHandle(id, handle);
      var nat = await naturalSizeFromDataUrl(dataUrl);
      _resolved['w:' + id] = dataUrl; // prime — see readLinkedDesktop's own comment
      out.push({ webHandleId: id, name: file.name, dataUrl: dataUrl, naturalW: nat.w, naturalH: nat.h });
    }
    return out;
  }

  // ---- relink (moved/deleted desktop file, or a web handle from another
  // browser/machine/session) — same shape as native-video-bridge.js's
  // replaceNativeVideoSource, generalized to the per-frame raster-stroke
  // storage images use: every frame on the source layer that carries the
  // OLD reference gets rewritten to the new one (a still image duplicates
  // the same linked marker into every frame — see images.js's import loop). ----
  function _updateLayerLinkedRefs(ld, field, oldVal, newVal) {
    (ld.frames || []).forEach(function (f) {
      (f.strokes || []).forEach(function (s) {
        if (s && s.isRaster && s.linked && s[field] === oldVal) s[field] = newVal;
      });
    });
  }
  function _resolveEntryLayer(m) {
    if (m.layerUid) { var byUid = (state.layers || []).find(function (l) { return l.layerUid === m.layerUid; }); if (byUid) return byUid; }
    if (m.layerName) return (state.layers || []).find(function (l) { return l.name === m.layerName; }) || null;
    return null;
  }
  async function relinkImageEntry(m) {
    if (!m || m.kind !== 'image' || !m.linked) return;
    var srcLayer = _resolveEntryLayer(m);
    if (!srcLayer) { if (window.showToast) showToast(SM.t('toastSourceLayerNotFound')); return; }
    if (m.path != null) {
      if (!tauriOk()) { if (window.showToast) showToast(SM.t('toastRelinkRequiresTauriApp')); return; }
      var path = await window.__TAURI__.dialog.open({
        title: 'Relier / remplacer l\'image', multiple: false,
        filters: [{ name: 'Images', extensions: ['png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp', 'avif'] }],
      });
      if (!path) return;
      var dataUrl;
      try { dataUrl = await _resolveDesktopPath(path); }
      catch (e) { if (window.showToast) showToast('Ouverture impossible : ' + (e && e.message || e), 'warn'); return; }
      if (window.pushUndo) pushUndo();
      var oldPath = m.path;
      _updateLayerLinkedRefs(srcLayer, 'linkedPath', oldPath, path);
      delete _resolved['p:' + oldPath]; delete _failed['p:' + oldPath];
      _resolved['p:' + path] = dataUrl; delete _failed['p:' + path];
      m.path = path; m.name = path.split(/[\\/]/).pop().replace(/\.[^.]+$/, ''); m.linkedBroken = false; m.linkedBrokenReason = null;
      _rerender();
      if (window.SMMediaLibrary) SMMediaLibrary.reload();
      if (window.showToast) showToast('Image reliée : ' + m.name);
    } else if (m.webHandleId) {
      if (!isWebLinkingSupported()) { if (window.showToast) showToast('Cette action nécessite un navigateur compatible (Chrome/Edge).', 'warn'); return; }
      var picked;
      try { picked = await window.showOpenFilePicker({ multiple: false, types: [{ description: 'Images', accept: { 'image/*': ['.png', '.jpg', '.jpeg', '.gif', '.webp', '.bmp', '.avif'] } }] }); }
      catch (e) { return; } // cancelled
      var handle = picked[0];
      var file = await handle.getFile();
      var dataUrl2 = await fileToDataUrl(file);
      var newId = uid();
      await putHandle(newId, handle);
      if (window.pushUndo) pushUndo();
      var oldId = m.webHandleId;
      _updateLayerLinkedRefs(srcLayer, 'linkedHandleId', oldId, newId);
      delete _resolved['w:' + oldId]; delete _failed['w:' + oldId]; delete _pendingPermission[oldId];
      removeHandle(oldId).catch(function () {});
      _resolved['w:' + newId] = dataUrl2;
      m.webHandleId = newId; m.name = file.name.replace(/\.[^.]+$/, ''); m.linkedBroken = false; m.linkedBrokenReason = null;
      _rerender();
      if (window.SMMediaLibrary) SMMediaLibrary.reload();
      if (window.showToast) showToast('Image reliée : ' + m.name);
    }
  }

  // ---- project-wide setting UI (Document panel, index.html) ----
  function setMediaMode(mode) {
    state.mediaMode = (mode === 'linked') ? 'linked' : 'embedded';
    var bE = document.getElementById('btn-media-mode-embedded'), bL = document.getElementById('btn-media-mode-linked');
    if (bE) bE.classList.toggle('ac', state.mediaMode === 'embedded');
    if (bL) bL.classList.toggle('ac', state.mediaMode === 'linked');
  }
  function initSettingUI() {
    var bE = document.getElementById('btn-media-mode-embedded'), bL = document.getElementById('btn-media-mode-linked');
    if (bE) bE.addEventListener('click', function () { setMediaMode('embedded'); });
    if (bL) bL.addEventListener('click', function () {
      // Feature-detect gracefully (2026-08-29 brief): on a browser without
      // showOpenFilePicker, "Liés" is still selectable (a project saved
      // elsewhere with linked media must still be able to flip back), but
      // importing new linked images there degrades to a clear toast at
      // import time (images.js) rather than silently doing nothing — the
      // warning belongs at the moment it actually matters (import), not
      // here at every click of this toggle.
      if (!tauriOk() && !isWebLinkingSupported() && window.showToast) {
        showToast('Ce navigateur ne supporte pas les médias liés (Safari/Firefox) — les nouveaux imports resteront intégrés.', 'warn');
      }
      setMediaMode('linked');
    });
    var dismissBtn = document.getElementById('linked-media-banner-dismiss');
    if (dismissBtn) dismissBtn.addEventListener('click', function () { var el = document.getElementById('linked-media-banner'); if (el) el.classList.remove('show'); });
    var reqBtn = document.getElementById('linked-media-banner-btn');
    if (reqBtn) reqBtn.addEventListener('click', requestPermissionForAll);
  }
  // syncDocFields (timeline.js) re-stamps the plain value fields (W/H/FPS…)
  // from state on every updateUI() — this toggle isn't a plain value field
  // (two buttons, not an input), so it registers its own repaint the same
  // way i18n.js's own afterI18n list does for dynamic labels, keeping the
  // buttons honest after an importJSON()/undo/tab-switch that changes
  // state.mediaMode out from under a raw click.
  function syncUI() {
    var bE = document.getElementById('btn-media-mode-embedded'), bL = document.getElementById('btn-media-mode-linked');
    var linked = state.mediaMode === 'linked';
    if (bE) bE.classList.toggle('ac', !linked);
    if (bL) bL.classList.toggle('ac', linked);
    _updateBanner();
  }
  window.SM = window.SM || {};
  window.SM.afterI18n = window.SM.afterI18n || [];
  window.SM.afterI18n.push(syncUI);

  function init() {
    initSettingUI();
    syncUI();
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', init); else init();

  window.SMLinkedMedia = {
    isWebLinkingSupported: isWebLinkingSupported,
    getCachedSrc: getCachedSrc,
    primeCache: primeCache,
    resolveAsync: resolveAsync,
    readLinkedDesktop: readLinkedDesktop,
    pickWebImages: pickWebImages,
    relinkImageEntry: relinkImageEntry,
    requestPermissionForAll: requestPermissionForAll,
    syncUI: syncUI,
  };
})();
