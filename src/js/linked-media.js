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
    // A linked VIDEO's re-grant needs more than a re-render: its decode
    // session was never opened (or was dropped), and the per-layer
    // "already warned, stay quiet" latch has to be released or the retry
    // below would be swallowed. native-video-bridge.js owns that state, so
    // it clears its own — one call, before the render that follows.
    if (window.SMNativeVideo && SMNativeVideo.retryWebLinkedSessions) SMNativeVideo.retryWebLinkedSessions();
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

  // ---- VIDEO (2026-08-30, feedback #154: "le save as enregistre toujours
  // dans le json les video encoded en base 64, on avait pourtant dit
  // d'expérimenter l'url local") ----
  // Video was excluded from this whole mechanism in the 2026-08-29 wave (a
  // deliberate scope boundary, not an oversight) — which left the WEB build
  // with two bad video shapes and no good one:
  //  - images.js's importVideoFrames bakes every decoded frame to a JPEG
  //    data URL into ld.frames[].strokes[].src — hundreds of MB of base64
  //    in the project file, exactly what Cyril reported;
  //  - the fast WebCodecs path (native-video-bridge.js importAsLayer)
  //    persists NOTHING durable: ld.nativeVideo.path is just the display
  //    name for a web session, so the clip is simply gone after a reload.
  // A FileSystemFileHandle fixes BOTH at once, and it is the same handle
  // mechanism linked images already use — the functions below are the video
  // counterparts of pickWebImages/_resolveWebHandle above, deliberately
  // sharing this file's SMIdb handle store, _pendingPermission set and
  // permission banner rather than growing a second parallel copy.
  //
  // ONE shape difference, and it is the good kind: a video resolves to a
  // FILE (fed straight to a decode session), never to a data: URL. Baking a
  // video into a data URL IS the reported bug, so getHandleFile below
  // deliberately stops one step short of fileToDataUrl.
  //
  // Also note where the reference LIVES: a linked video keeps exactly ONE
  // webHandleId on ld.nativeVideo, at LAYER level — getEffectiveStrokes
  // (app.js) returns [] for a nativeVideo layer, so there are no per-frame
  // stroke dicts at all. The "a still's stroke dict is duplicated verbatim
  // into every frame" multiplication that images have to live with simply
  // has no equivalent here; only the BAKING importer writes per-frame
  // payloads, which is what this change routes around.
  var VIDEO_PICKER_TYPES = [{ description: 'Vidéos', accept: { 'video/*': ['.mp4', '.mov', '.webm', '.m4v', '.ogv', '.mkv', '.avi'] } }];

  // Opens the File System Access picker for ONE video, stashes the handle
  // in the same SMIdb store linked images use, and hands back both the id
  // to persist and the live File to open a decode session on right now.
  // Called only from a real user-gesture handler (the Vidéo… button), same
  // hard requirement as pickWebImages above.
  async function pickWebVideo() {
    if (!isWebLinkingSupported()) throw new Error('File System Access API indisponible dans ce navigateur (Safari/Firefox) — utilisez le mode Intégrés ou Chrome/Edge.');
    var handles = await window.showOpenFilePicker({ multiple: false, types: VIDEO_PICKER_TYPES });
    var handle = handles[0];
    if (!handle) return null;
    var file = await handle.getFile();
    var id = uid();
    await putHandle(id, handle);
    return { webHandleId: id, file: file, name: file.name };
  }

  // Resolve a persisted handle id back to a live File — the video
  // counterpart of _resolveWebHandle above, sharing its permission
  // handling verbatim (pending set + banner) so a re-grant click fixes
  // linked images and linked videos in the same gesture. Throws with
  // .needsPermission set so the caller can distinguish "user must click"
  // from "file genuinely gone" and report each differently.
  function getHandleFile(handleId) {
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
      });
    });
  }

  // Drop a handle the caller no longer references (a relink replacing an
  // older one) — exposed so native-video-bridge.js's own relink flow can
  // clean up through this file's store rather than reaching into SMIdb
  // with a duplicated key prefix.
  function forgetHandle(handleId) { return removeHandle(handleId); }

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

  // ---- bulk conversion (2026-08-29, follow-up to "un seul réglage" above)
  // — flipping state.mediaMode only ever governed NEW imports (this file's
  // own header comment: "never retroactively converts already-embedded
  // media"). This closes that gap with an EXPLICIT, user-triggered action
  // (never an automatic side-effect of flipping the toggle itself — a bulk
  // filesystem write firing invisibly off a checkbox click would be a bad
  // surprise) that retroactively converts every image ALREADY in the
  // project to match whichever direction the Document panel toggle
  // currently points at.
  //
  // Scope: media-library-entry driven, same as relinkImageEntry above —
  // only an entry's own SOURCE layer (layerUid/layerName) is touched, never
  // a copy dragged out of the library onto another layer afterward. Same
  // documented limitation this file's own sequence-relink flow already
  // carries (only the representative first file is offered/updated for a
  // sequence entry), not a new one introduced here.

  // Given a `d`-shaped reference ({linkedPath} or {linkedHandleId}), resolves
  // its bytes ONCE and returns a promise — the synchronous-cache-or-kick-
  // off-background-resolve dance resolveAsync does for desR's render loop
  // doesn't fit a batch conversion that needs to know the outcome (success
  // or missing/broken) before deciding what to write. Reuses the EXACT same
  // read implementations resolveAsync itself calls
  // (_resolveDesktopPath/_resolveWebHandle) and the exact same cache
  // (getCachedSrc/primeCache) — never a second, parallel bytes-reading path.
  function resolveOnce(d) {
    var cached = getCachedSrc(d);
    if (cached) return Promise.resolve(cached);
    if (!d) return Promise.reject(new Error('référence invalide'));
    var p = d.linkedPath ? _resolveDesktopPath(d.linkedPath) : (d.linkedHandleId ? _resolveWebHandle(d.linkedHandleId) : null);
    if (!p) return Promise.reject(new Error('référence invalide'));
    return p.then(function (dataUrl) { primeCache(d, dataUrl); return dataUrl; });
  }

  // Every DISTINCT linked reference found across every frame of a layer —
  // usually just one (a still, duplicated verbatim into every frame by
  // images.js's import loop, see its own comment), but a sequence layer
  // holds one per numbered file, so this can't assume a single value the
  // way relink's own _updateLayerLinkedRefs (single old→new rewrite) does.
  function _collectLinkedRefs(ld) {
    var seen = {}, out = [];
    (ld.frames || []).forEach(function (f) {
      (f.strokes || []).forEach(function (s) {
        if (!s || !s.isRaster || !s.linked) return;
        var k = s.linkedPath ? ('p:' + s.linkedPath) : (s.linkedHandleId ? ('w:' + s.linkedHandleId) : null);
        if (k && !seen[k]) { seen[k] = true; out.push({ linkedPath: s.linkedPath || null, linkedHandleId: s.linkedHandleId || null }); }
      });
    });
    return out;
  }
  // Same idea for the embedded side — a still has ONE distinct `src` shared
  // by every frame, an embedded sequence has one per frame-group.
  function _collectEmbeddedSrcs(ld) {
    var seen = {}, out = [];
    (ld.frames || []).forEach(function (f) {
      (f.strokes || []).forEach(function (s) {
        if (s && s.isRaster && !s.linked && s.src && !seen[s.src]) { seen[s.src] = true; out.push(s.src); }
      });
    });
    return out;
  }
  // In-place field mutation on every matching frame's stroke dict — never an
  // object-literal REPLACEMENT, so any other field already on the dict
  // (rotation, isBitmapBrush, brushGroupId, groupId…) survives untouched.
  function _applyToLinkedFrames(ld, ref, fn) {
    (ld.frames || []).forEach(function (f) {
      (f.strokes || []).forEach(function (s) {
        if (!s || !s.isRaster || !s.linked) return;
        if (ref.linkedPath != null ? s.linkedPath === ref.linkedPath : s.linkedHandleId === ref.linkedHandleId) fn(s);
      });
    });
  }
  function _applyToEmbeddedFrames(ld, srcValue, fn) {
    (ld.frames || []).forEach(function (f) {
      (f.strokes || []).forEach(function (s) {
        if (s && s.isRaster && !s.linked && s.src === srcValue) fn(s);
      });
    });
  }

  function extFromDataUrl(dataUrl) {
    var m = /^data:image\/([a-zA-Z0-9+.-]+);/.exec(dataUrl || '');
    var sub = m ? m[1].toLowerCase() : 'png';
    var map = { png: 'png', jpeg: 'jpg', jpg: 'jpg', gif: 'gif', webp: 'webp', bmp: 'bmp', 'x-ms-bmp': 'bmp', avif: 'avif' };
    return map[sub] || 'png';
  }
  function dataUrlToBytes(dataUrl) {
    var b64 = (dataUrl || '').split(',')[1] || '';
    var bin = atob(b64);
    var bytes = new Uint8Array(bin.length);
    for (var i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
    return bytes;
  }
  // Collision guard across the WHOLE batch (Cyril's brief: two library
  // entries sharing a name must be numbered, never silently overwrite one
  // another) — tracked in a Set shared by the entire conversion run, not
  // per-entry, since a sequence writes several files for ONE entry too.
  function _uniqueFileName(usedSet, baseName, ext) {
    var clean = String(baseName || 'media').replace(/[\\/:*?"<>|]/g, '_').trim() || 'media';
    var candidate = clean, n = 1;
    while (usedSet[(candidate + '.' + ext).toLowerCase()]) { n++; candidate = clean + ' (' + n + ')'; }
    var full = candidate + '.' + ext;
    usedSet[full.toLowerCase()] = true;
    return full;
  }

  // NOTE on the kind === 'image' filter both directions below still carry
  // (2026-08-30, feedback #154): video now participates in linked media for
  // NEW IMPORTS (see the VIDEO section above and images.js's own dispatch),
  // which is what state.mediaMode has always governed — but it stays out of
  // this BULK CONVERTER on purpose, in both directions, because neither one
  // is a reference rewrite for a video the way it is for an image:
  //  - linked → embedded would have to re-run the bake-every-frame-to-JPEG
  //    importer, i.e. deliberately reproduce the exact hundreds-of-MB
  //    base64 blowup this feedback is about;
  //  - embedded → linked has nothing to point AT — an embedded video is a
  //    pile of baked per-frame JPEGs with no original container left, so
  //    "write the file out and link it" would mean re-encoding a video,
  //    which is a whole export pipeline, not a file write.
  // Left as an explicit, documented boundary rather than a silent filter.

  // ---- Linked → Embedded — the simpler, lower-risk direction: just READS
  // bytes back (via resolveOnce above, reusing the shipped resolve cache)
  // and writes them into `src`, the exact shape the embedded import path
  // already produces. A currently-missing/broken file can't be embedded —
  // skipped and reported, never a blank placeholder baked in as `src`.
  async function convertLinkedToEmbedded() {
    var entries = (state.mediaLibrary || []).filter(function (m) { return m.kind === 'image' && m.linked; });
    if (!entries.length) { if (window.showToast) showToast('Aucun média lié à intégrer.', 'info'); return; }
    if (window.saveAllLayerFrames) saveAllLayerFrames();
    if (window.pushUndo) pushUndo(true); // ONE undo step for the whole batch
    var converted = 0, skipped = 0;
    for (var i = 0; i < entries.length; i++) {
      var m = entries[i];
      var srcLayer = _resolveEntryLayer(m);
      if (!srcLayer) { skipped++; continue; }
      var refs = _collectLinkedRefs(srcLayer);
      var okCount = 0;
      for (var j = 0; j < refs.length; j++) {
        var ref = refs[j];
        var dataUrl;
        try { dataUrl = await resolveOnce(ref); }
        catch (e) { skipped++; continue; } // missing/broken — left untouched
        _applyToLinkedFrames(srcLayer, ref, (function (du) { return function (s) { delete s.linked; delete s.linkedPath; delete s.linkedHandleId; s.src = du; }; })(dataUrl));
        converted++; okCount++;
      }
      if (okCount > 0) {
        var primaryRef = m.path != null ? { linkedPath: m.path } : { linkedHandleId: m.webHandleId };
        var primaryDataUrl = getCachedSrc(primaryRef);
        // primaryDataUrl is null only when the REPRESENTATIVE file itself
        // (as opposed to some other frame of a sequence) failed to resolve
        // — leave the catalog entry linked/broken rather than guess a thumb,
        // same "representative file only" scope this file's relink flow
        // already documents for sequences.
        if (primaryDataUrl && window.SMMediaLibrary) {
          SMMediaLibrary.updateEntry(m.id, { linked: false, path: null, webHandleId: null, thumb: primaryDataUrl, linkedBroken: false, linkedBrokenReason: null });
        }
      }
    }
    _rerender();
    if (window.SMMediaLibrary) SMMediaLibrary.reload();
    var msg = converted + (converted === 1 ? ' média intégré' : ' médias intégrés');
    if (skipped) msg += ', ' + skipped + (skipped === 1 ? ' ignoré (fichier introuvable)' : ' ignorés (fichiers introuvables)');
    if (window.showToast) showToast(msg, skipped ? 'warn' : 'success');
  }

  // ---- Embedded → Linked — the harder direction: the bytes only exist
  // INSIDE the project today, so this WRITES new files to disk (desktop) or
  // into a user-granted directory (web) before it can link to them — unlike
  // the direction above, there's no existing external file to just point
  // at. Destination is picked ONCE for the whole batch (a directory, not a
  // save-dialog per file — the entire reason to use the directory pickers
  // below instead of one native dialog per image).
  async function _pickDestinationDir() {
    if (tauriOk()) {
      var dir = await window.__TAURI__.dialog.open({ directory: true, title: 'Dossier de destination pour les médias liés' });
      return dir ? { kind: 'desktop', path: dir } : null;
    }
    if (isWebLinkingSupported() && typeof window.showDirectoryPicker === 'function') {
      // {mode:'readwrite'} requests write permission up front, at the SAME
      // user gesture as the folder pick — createWritable() below then needs
      // no separate permission prompt per file (same "one grant covers the
      // batch" idea as showOpenFilePicker's own permission model).
      try { var handle = await window.showDirectoryPicker({ mode: 'readwrite' }); return { kind: 'web', handle: handle }; }
      catch (e) { return null; } // cancelled — AbortError
    }
    return null;
  }
  async function convertEmbeddedToLinked() {
    var entries = (state.mediaLibrary || []).filter(function (m) { return m.kind === 'image' && !m.linked; });
    if (!entries.length) { if (window.showToast) showToast('Aucun média intégré à relier.', 'info'); return; }
    if (!tauriOk() && !isWebLinkingSupported()) {
      if (window.showToast) showToast('Cette action nécessite l\'app de bureau ou un navigateur compatible (Chrome/Edge) pour écrire les fichiers sur le disque.', 'warn');
      return;
    }
    var dest;
    try { dest = await _pickDestinationDir(); } catch (e) { dest = null; }
    if (!dest) return; // cancelled — nothing touched, no undo entry pushed
    if (window.showToast) showToast('Conversion des médias en liés…', 'info');
    if (window.saveAllLayerFrames) saveAllLayerFrames();
    if (window.pushUndo) pushUndo(true); // ONE undo step for the whole batch
    var usedNames = {};
    var converted = 0, skipped = 0;
    for (var i = 0; i < entries.length; i++) {
      var m = entries[i];
      var srcLayer = _resolveEntryLayer(m);
      if (!srcLayer) { skipped++; continue; }
      var srcs = _collectEmbeddedSrcs(srcLayer);
      var primaryRefForEntry = null;
      for (var k = 0; k < srcs.length; k++) {
        var dataUrl = srcs[k];
        try {
          var ext = extFromDataUrl(dataUrl);
          var base = (m.name || srcLayer.name || 'media') + (srcs.length > 1 ? '_' + (k + 1) : '');
          var fileName = _uniqueFileName(usedNames, base, ext);
          var bytes = dataUrlToBytes(dataUrl);
          var ref;
          if (dest.kind === 'desktop') {
            var fullPath = dest.path.replace(/[\\/]+$/, '') + '/' + fileName;
            await window.__TAURI__.fs.writeFile(fullPath, bytes);
            ref = { linkedPath: fullPath };
          } else {
            var fh = await dest.handle.getFileHandle(fileName, { create: true });
            var writable = await fh.createWritable();
            await writable.write(bytes);
            await writable.close();
            var newId = uid();
            await putHandle(newId, fh); // SAME SMIdb-backed handle store as pickWebImages above
            ref = { linkedHandleId: newId };
          }
          primeCache(ref, dataUrl); // instant render, no resolve round-trip
          _applyToEmbeddedFrames(srcLayer, dataUrl, (function (r) { return function (s) { delete s.src; s.linked = true; if (r.linkedPath) s.linkedPath = r.linkedPath; else s.linkedHandleId = r.linkedHandleId; }; })(ref));
          converted++;
          if (k === 0) primaryRefForEntry = ref;
        } catch (e) { skipped++; }
      }
      if (primaryRefForEntry) {
        var nat = await naturalSizeFromDataUrl(srcs[0]);
        var smallThumb = (window.SM && window.SM._makeSmallImageThumb) ? await window.SM._makeSmallImageThumb(srcs[0], nat.w, nat.h) : null;
        if (window.SMMediaLibrary) {
          var patch = { linked: true, naturalW: nat.w, naturalH: nat.h, thumb: smallThumb, sizeBytes: null, linkedBroken: false, linkedBrokenReason: null, webHandleId: null, path: null };
          if (primaryRefForEntry.linkedPath) patch.path = primaryRefForEntry.linkedPath; else patch.webHandleId = primaryRefForEntry.linkedHandleId;
          SMMediaLibrary.updateEntry(m.id, patch);
        }
      }
    }
    _rerender();
    if (window.SMMediaLibrary) SMMediaLibrary.reload();
    var msg = converted + (converted === 1 ? ' média lié' : ' médias liés');
    if (skipped) msg += ', ' + skipped + (skipped === 1 ? ' ignoré' : ' ignorés');
    if (window.showToast) showToast(msg, skipped ? 'warn' : 'success');
  }

  // Embedding direction only ever READS (always attempt-able — worst case
  // it just finds nothing linked to convert); linking direction WRITES new
  // files, which needs either Tauri or a Chromium-only web API — disabled/
  // explained rather than silently doing nothing on Safari/Firefox, same
  // graceful-degradation idiom as every other Tauri-only feature here.
  function convertActionSupported() {
    return state.mediaMode === 'linked' ? (tauriOk() || isWebLinkingSupported()) : true;
  }
  async function onConvertMediaClick() {
    var btn = document.getElementById('btn-convert-media');
    if (btn) btn.disabled = true;
    try {
      if (state.mediaMode === 'linked') await convertEmbeddedToLinked();
      else await convertLinkedToEmbedded();
    } catch (e) {
      if (window.showToast) showToast('Conversion échouée : ' + ((e && e.message) || e), 'warn');
    } finally {
      if (btn) btn.disabled = !convertActionSupported();
    }
  }
  function convertBtnLabel() {
    return (window.SM && SM.t) ? SM.t(state.mediaMode === 'linked' ? 'mediaConvertToLinked' : 'mediaConvertToEmbedded')
      : (state.mediaMode === 'linked' ? 'Convertir les médias en liés…' : 'Convertir les médias en intégrés…');
  }
  function updateConvertButtonUI() {
    var btn = document.getElementById('btn-convert-media');
    if (!btn) return;
    btn.textContent = convertBtnLabel();
    var supported = convertActionSupported();
    btn.disabled = !supported;
    btn.title = (window.SM && SM.t) ? SM.t(supported ? 'mediaConvertTip' : 'mediaConvertUnsupportedTip') : '';
  }

  // ---- project-wide setting UI (Document panel, index.html) ----
  function setMediaMode(mode) {
    state.mediaMode = (mode === 'linked') ? 'linked' : 'embedded';
    var bE = document.getElementById('btn-media-mode-embedded'), bL = document.getElementById('btn-media-mode-linked');
    if (bE) bE.classList.toggle('ac', state.mediaMode === 'embedded');
    if (bL) bL.classList.toggle('ac', state.mediaMode === 'linked');
    updateConvertButtonUI();
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
    var convertBtn = document.getElementById('btn-convert-media');
    if (convertBtn) convertBtn.addEventListener('click', onConvertMediaClick);
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
    updateConvertButtonUI();
  }
  function init() {
    initSettingUI();
    // Registered HERE, not at IIFE-evaluation time (fixed 2026-08-30 while
    // verifying feedback #156's panel move — a pre-existing silent break,
    // not a consequence of that move). index.html loads this file BEFORE
    // timeline.js, and timeline.js:349 assigns `window.SM = {...}` as a
    // fresh object LITERAL, which wipes anything attached to window.SM
    // earlier — the exact trap app.js already documents above
    // convertLayerToComponent ("silently wipes out anything attached to
    // window.SM earlier"). So the old top-level push landed on an object
    // that no longer existed by the time applyI18n() read the list, and
    // this repaint never ran: switching language left the convert button
    // showing its previous locale's label (confirmed live — SM.t returned
    // the right string while the button did not change, and a manual
    // syncUI() fixed it). init() runs on DOMContentLoaded, after every
    // script has evaluated, so the push lands on the final window.SM.
    window.SM = window.SM || {};
    window.SM.afterI18n = window.SM.afterI18n || [];
    if (window.SM.afterI18n.indexOf(syncUI) < 0) window.SM.afterI18n.push(syncUI);
    syncUI();
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', init); else init();

  window.SMLinkedMedia = {
    isWebLinkingSupported: isWebLinkingSupported,
    getCachedSrc: getCachedSrc,
    primeCache: primeCache,
    resolveAsync: resolveAsync,
    resolveOnce: resolveOnce,
    readLinkedDesktop: readLinkedDesktop,
    pickWebImages: pickWebImages,
    // Video (2026-08-30, feedback #154) — same handle store / permission
    // banner as the image side above; resolves to a FILE, never a data URL.
    pickWebVideo: pickWebVideo,
    getHandleFile: getHandleFile,
    forgetHandle: forgetHandle,
    relinkImageEntry: relinkImageEntry,
    requestPermissionForAll: requestPermissionForAll,
    syncUI: syncUI,
    // Bulk conversion (2026-08-29) — exposed mainly for tests/automation;
    // the normal entry point is the Document panel button (#btn-convert-media).
    convertLinkedToEmbedded: convertLinkedToEmbedded,
    convertEmbeddedToLinked: convertEmbeddedToLinked,
    convertActionSupported: convertActionSupported,
  };
})();
