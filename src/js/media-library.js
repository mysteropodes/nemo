// ---- MEDIA LIBRARY (v1, 2026-07) ----
// A browsable catalog of every image/video imported into the project — the
// right panel's "Médias" section (#media-sec, index.html). Doesn't own its
// own decode pipeline: registered by images.js's own import functions
// (importStandalone/importSequence/importVideoFrames, both the Tauri-dialog
// and browser-fallback <input> paths) right after they successfully land
// content, so this file only ever displays/organizes, never decodes.
//
// Storage (state.mediaLibrary, app.js): {id,name,kind:'image'|'video',thumb,
// layerName}. Images keep their FULL dataURL as `thumb` (small enough, and
// doubles as the source for "drag onto canvas to insert another copy").
// Videos keep only their first decoded frame as a preview — the original
// bytes are never re-embedded a second time (the layer's own per-frame
// raster strokes already are the full decoded video), so a video entry is
// browse/jump-to-layer only, not re-importable on drag.
(function () {
  function uid() { return 'm' + Date.now().toString(36) + Math.random().toString(36).slice(2, 7); }

  // Rough byte estimate for a base64 dataURL, used for the panel's size
  // column — no need to be exact (this is a browse aid, not a disk audit).
  function dataUrlBytes(du) {
    if (!du || du.indexOf(',') < 0) return 0;
    var b64 = du.slice(du.indexOf(',') + 1);
    return Math.round(b64.length * 0.75);
  }

  // opts (2026-07-31, real asset-panel pass — Cyril: "vrai panel de gestion
  // de fichier importé"): { layerUid, linked, path } —
  // - layerUid: stable identity (app.js's createUserLayer stamp), resolved
  //   FIRST everywhere below; layerName stays only as a fallback for entries
  //   saved before this pass and for kinds (audio) with no owning layer.
  // - linked/path: nativeVideo entries only persist a filesystem path (no
  //   embedded bytes) and can go offline if the file moves — surfaced as a
  //   badge instead of a size, and is what gates the relink action.
  // status (2026-08, feedback: "afficher l'instance dans média
  // instantanément... avec wait et ready") — 'loading' | 'ready', default
  // 'ready' for every EXISTING call site (nothing else in the codebase sets
  // it, so they keep behaving exactly as before). native-video-bridge.js's
  // importAsLayer is the one caller that now adds a 'loading' entry the
  // instant a video is dropped (before its own async open()/decode even
  // starts — see that file), using the id this function now returns, then
  // calls updateEntry() below once the real thumbnail/metadata land.
  function addEntry(name, kind, thumb, layerName, opts) {
    if (!state.mediaLibrary) state.mediaLibrary = [];
    opts = opts || {};
    var entry = {
      id: uid(), name: name, kind: kind, thumb: thumb, layerName: layerName || null,
      layerUid: opts.layerUid || null, linked: !!opts.linked, path: opts.path || null,
      // audioId: audio tracks have no owning layer at all (state.audioTracks
      // is a separate array) — this is their own identity, used only by the
      // 'Supprimer la piste' menu action below to find the right track.
      audioId: opts.audioId || null,
      sizeBytes: opts.linked ? null : dataUrlBytes(thumb),
      importedAt: Date.now(),
      status: opts.status || 'ready',
    };
    state.mediaLibrary.push(entry);
    render();
    return entry.id;
  }
  // Patches an existing entry in place (fields present in `patch` overwrite,
  // everything else untouched) and re-renders — the counterpart to a
  // 'loading' addEntry() call above, or any other in-place metadata update.
  // No-ops quietly if the entry was removed in the meantime (e.g. the user
  // deleted it from the panel mid-decode).
  function updateEntry(id, patch) {
    var m = (state.mediaLibrary || []).find(function (x) { return x.id === id; });
    if (!m) return false;
    Object.keys(patch || {}).forEach(function (k) { m[k] = patch[k]; });
    if (patch && 'thumb' in patch && !('sizeBytes' in patch) && !m.linked) m.sizeBytes = dataUrlBytes(m.thumb);
    render();
    return true;
  }

  // uid-first resolve (2026-07-31) with a name fallback for pre-migration
  // entries — a rename no longer orphans the catalog link once the entry
  // carries a layerUid.
  function resolveSrcLayer(m) {
    if (m.layerUid) {
      var byUid = state.layers.find(function (l) { return l.layerUid === m.layerUid; });
      if (byUid) return byUid;
    }
    if (m.layerName) return state.layers.find(function (l) { return l.name === m.layerName; }) || null;
    return null;
  }

  function jumpToLayer(m) {
    // Audio has no owning layer at all — a click is simply inert, not an
    // error (distinct from the "was a layer, now gone" orphan case below).
    if (!m.layerName && !m.layerUid) return;
    var ld = resolveSrcLayer(m);
    if (!ld) { if (window.showToast) showToast(SM.t('toastSourceLayerNotFound')); return; }
    var idx = state.layers.indexOf(ld);
    if (window.SM) SM.setActiveLayer(idx);
    if (window.showToast) showToast('Calque « ' + ld.name + SM.t('toastSelectedSuffix'));
  }

  function removeEntry(id) {
    state.mediaLibrary = (state.mediaLibrary || []).filter(function (m) { return m.id !== id; });
    render();
  }

  // Inserts another copy of a library IMAGE onto the active layer's current
  // frame — mirrors images.js's importStandalone insertion pattern exactly
  // (same isRaster stroke shape, same keyframe-if-needed guard), just fed a
  // dataURL already in hand instead of decoding one from a file.
  function insertImageOnCanvas(m, worldPt) {
    if (m.kind !== 'image') return;
    if (window.pushUndo) pushUndo();
    if (window.saveAllLayerFrames) saveAllLayerFrames();
    var ld = state.layers[state.activeLayerIdx];
    if (!ld.frames[state.currentFrame].isKeyframe && !ld.frames[state.currentFrame].isInterpolated) {
      ld.frames[state.currentFrame].strokes = JSON.parse(JSON.stringify(getEffectiveStrokes(state.activeLayerIdx, state.currentFrame)));
      ld.frames[state.currentFrame].isKeyframe = true;
    }
    var img = new Image();
    img.onload = function () {
      var w = img.naturalWidth || 1, h = img.naturalHeight || 1;
      var s = Math.min(1, state.canvasW / w, state.canvasH / h);
      var fw = w * s, fh = h * s;
      ld.frames[state.currentFrame].strokes.push({ isRaster: true, src: m.thumb, x: worldPt ? worldPt.x : state.canvasW / 2, y: worldPt ? worldPt.y : state.canvasH / 2, width: fw, height: fh, opacity: 1 });
      if (window.loadFrame) loadFrame(state.currentFrame);
      if (window.updateUI) updateUI();
      if (window.showToast) showToast(SM.t('toastImageInsertedFromLibrary'));
    };
    img.src = m.thumb;
  }

  // Short relative date, same convention a file browser / project list would
  // use ("aujourd'hui" beats a raw timestamp for a library you browse often
  // right after importing). Falls back to a plain date past a week — no
  // point in "il y a 12 jours" precision once it's old.
  function formatImportedAt(ts) {
    if (!ts) return '';
    var diffMs = Date.now() - ts, day = 86400000;
    if (diffMs < day) return 'Aujourd’hui';
    if (diffMs < 2 * day) return 'Hier';
    if (diffMs < 7 * day) return 'Il y a ' + Math.floor(diffMs / day) + ' j';
    var d = new Date(ts);
    return d.getDate() + '/' + (d.getMonth() + 1) + '/' + String(d.getFullYear()).slice(2);
  }
  function formatBytes(n) {
    if (!n) return '';
    if (n < 1024) return n + ' o';
    if (n < 1024 * 1024) return Math.round(n / 1024) + ' Ko';
    return (n / (1024 * 1024)).toFixed(1) + ' Mo';
  }
  // Rebuilt 2026-07 as a ROW-based list ("l'organisation file des footage
  // importé dans ce genre là" — a project-list-table reference screenshot:
  // name+thumbnail, a colored status/type pill, an owner-like avatar+name)
  // instead of the original plain thumbnail grid — same underlying
  // state.mediaLibrary data and all existing interactions (click to jump,
  // context menu, image drag-out), just laid out as scannable rows with
  // more metadata visible at a glance: kind badge, source LAYER (with that
  // layer's own color as a stand-in for the reference's owner avatar, since
  // there's no per-user concept here — the layer IS the "owner" of an
  // imported asset), and import date.
  var KIND_LABEL = { video: 'Vidéo', image: 'Image', audio: 'Audio' };
  var _orphanCount = 0; // set fresh by buildRow() on every render() pass, read back after
  // Builds ONE row for a media entry — unchanged from the old flat-list
  // version, just no longer appends itself directly (the caller decides
  // which folder body to append into, see render() below).
  function buildRow(m) {
      var row = document.createElement('div'); row.className = 'media-row' + (m.status === 'loading' ? ' loading' : ''); row.title = m.name;
      var thumb = document.createElement('div'); thumb.className = 'media-row-thumb';
      if (m.status === 'loading') {
        // Decoding placeholder (2026-08, feedback: "afficher l'instance dans
        // média instantanément... avec wait et ready") — no thumb exists yet
        // (addEntry was called before the source was even opened), so this
        // is a plain spinner rather than an <img> with no src.
        thumb.classList.add('media-row-thumb-loading');
        var sp = document.createElement('div'); sp.className = 'media-row-spinner'; thumb.appendChild(sp);
      } else if (m.kind === 'audio') {
        thumb.classList.add('media-row-thumb-icon'); thumb.textContent = '♪';
      } else {
        var img = document.createElement('img'); img.src = m.thumb; img.draggable = m.kind === 'image';
        thumb.appendChild(img);
        if (m.kind === 'video') { var pb = document.createElement('div'); pb.className = 'media-row-playicon'; pb.textContent = '▶'; thumb.appendChild(pb); }
      }
      row.appendChild(thumb);

      var main = document.createElement('div'); main.className = 'media-row-main';
      var nameEl = document.createElement('div'); nameEl.className = 'media-row-name'; nameEl.textContent = m.name;
      main.appendChild(nameEl);

      if (m.status === 'loading') {
        // Indeterminate — there's no real byte-progress signal from the
        // decode pipeline to drive a determinate bar off (see
        // native-video-bridge.js's own comment on this), so an honest
        // "working on it" animation beats a fake percentage.
        var barWrap = document.createElement('div'); barWrap.className = 'media-row-progress';
        var bar = document.createElement('div'); bar.className = 'media-row-progress-bar'; barWrap.appendChild(bar);
        main.appendChild(barWrap);
        var waitEl = document.createElement('div'); waitEl.className = 'media-row-meta'; waitEl.style.color = 'var(--text-dim)';
        waitEl.textContent = SM && SM.t ? SM.t('mediaDecoding') : 'Décodage…';
        main.appendChild(waitEl);
        row.appendChild(main);
        return row; // no context menu / drag / owner lookup on a not-yet-real entry
      }

      var meta = document.createElement('div'); meta.className = 'media-row-meta';
      var kindBadge = document.createElement('span'); kindBadge.className = 'media-row-badge kind-' + m.kind; kindBadge.textContent = KIND_LABEL[m.kind] || m.kind;
      meta.appendChild(kindBadge);
      // Embedded (a size in the project file) vs linked (nativeVideo — only
      // a filesystem path persists) — a real asset panel needs this
      // distinction since it determines what "broken" even means (2026-07-31).
      if (m.linked) {
        var linkBadge = document.createElement('span'); linkBadge.className = 'media-row-badge kind-linked'; linkBadge.textContent = 'Fichier lié'; linkBadge.title = m.path || '';
        meta.appendChild(linkBadge);
      } else if (m.sizeBytes) {
        var sizeEl = document.createElement('span'); sizeEl.className = 'media-row-size'; sizeEl.textContent = formatBytes(m.sizeBytes);
        meta.appendChild(sizeEl);
      }

      // "Owner" column equivalent: the source layer, colored dot + name —
      // or a muted "Orphelin" pill if that layer no longer exists (deleted/
      // renamed since import; jumpToLayer already toasted this on click,
      // this makes it visible without having to click first).
      var srcLayer = resolveSrcLayer(m);
      if (m.layerName || m.layerUid) {
        if (!srcLayer) _orphanCount++;
        var owner = document.createElement('span'); owner.className = 'media-row-owner' + (srcLayer ? '' : ' orphan');
        if (srcLayer) { var dot = document.createElement('span'); dot.className = 'media-row-owner-dot'; dot.style.background = srcLayer.color || 'var(--text-dim)'; owner.appendChild(dot); }
        var ownerLbl = document.createElement('span'); ownerLbl.textContent = srcLayer ? srcLayer.name : 'Orphelin';
        owner.appendChild(ownerLbl);
        meta.appendChild(owner);
      }
      main.appendChild(meta);

      var dateEl = document.createElement('div'); dateEl.className = 'media-row-date'; dateEl.textContent = formatImportedAt(m.importedAt);
      main.appendChild(dateEl);
      row.appendChild(main);

      row.addEventListener('click', function () { jumpToLayer(m); });
      row.addEventListener('contextmenu', function (e) {
        e.preventDefault(); e.stopPropagation();
        if (!window.showContextMenu) return;
        var items = [];
        if (srcLayer) items.push({ label: 'Sélectionner le calque', action: function () { jumpToLayer(m); } });
        if (m.kind === 'image') items.push({ label: 'Insérer une copie sur le calque actif', action: function () { insertImageOnCanvas(m); } });
        // Relink (2026-07-31): only nativeVideo entries can go offline (a
        // moved/deleted file — the embedded kinds never can, their bytes
        // live IN the project). replaceNativeVideoSource is native-video-
        // bridge.js's own relink flow, same dialog/session-swap shape as
        // images.js's replaceFootageSource for the embedded raster kinds.
        if (m.linked && srcLayer && window.SMNativeVideo && window.SMNativeVideo.replaceNativeVideoSource) {
          items.push({ label: 'Relier / remplacer le fichier…', action: function () {
            var li = state.layers.indexOf(srcLayer);
            window.SMNativeVideo.replaceNativeVideoSource(li);
          } });
        }
        // Audio has real controls (mute/volume/offset-drag) on its own
        // timeline row already (audio-bridge.js) — deliberately NOT
        // duplicated here (same divergence risk CLAUDE.md flags for propsFor/
        // panel-grid). Only delete, reusing that file's own stop+splice.
        if (m.kind === 'audio' && m.audioId) {
          items.push({ label: 'Supprimer la piste', action: function () {
            if (window.SMAudio && SMAudio.removeTrackByAudioId(m.audioId)) removeEntry(m.id);
          } });
        } else {
          items.push({ label: 'Retirer de la bibliothèque', action: function () { removeEntry(m.id); } });
        }
        window.showContextMenu(e.clientX, e.clientY, items);
      });
      if (m.kind === 'image') {
        img.addEventListener('dragstart', function (e) {
          e.dataTransfer.setData('application/x-nemo-media-id', m.id);
          e.dataTransfer.effectAllowed = 'copy';
        });
      }
      return row;
  }

  // Real hierarchy pass (2026-08, feedback: "on est pas encore sur une vrai
  // hierarchie avec label, folder..., ou composition" — AEP Transplant
  // reference). Folders are AUTOMATIC by kind (scope decision this
  // session — no user-managed create/rename/drag-into-folder), plus a
  // "Composants" folder surfacing state.symbols (Nemo's precomp
  // equivalent) since the reference screenshot's PRECOMPS group is exactly
  // that concept. asset-tree.js owns the folder header/chevron/collapse
  // widget; this function only groups entries and builds rows into it.
  function render() {
    var grid = document.getElementById('media-grid'); if (!grid) return;
    grid.innerHTML = '';
    _orphanCount = 0;
    if (!window.SMAssetTree) return; // asset-tree.js not loaded — nothing to group into
    var symIds = Object.keys(state.symbols || {});
    if (symIds.length) {
      var compBody = SMAssetTree.folderGroup(grid, { label: SMAssetTree.componentsLabel(), color: SMAssetTree.FOLDER_COLORS.components, count: symIds.length });
      symIds.forEach(function (sid) {
        var sym = state.symbols[sid];
        var row = document.createElement('div'); row.className = 'bp-item'; row.title = sym.name;
        var icon = document.createElement('span'); icon.className = 'bp-item-icon'; icon.textContent = '▤';
        row.appendChild(icon);
        var span = document.createElement('span'); span.textContent = sym.name || 'Composant';
        row.appendChild(span);
        compBody.appendChild(row);
      });
    }
    ['image', 'video', 'audio'].forEach(function (kind) {
      var entries = (state.mediaLibrary || []).filter(function (m) { return m.kind === kind; });
      if (!entries.length) return;
      var body = SMAssetTree.folderGroup(grid, { label: SMAssetTree.KIND_GROUP_LABEL[kind], color: SMAssetTree.FOLDER_COLORS[kind], count: entries.length });
      entries.forEach(function (m) { body.appendChild(buildRow(m)); });
    });
    if (!symIds.length && !(state.mediaLibrary || []).length) {
      var empty = document.createElement('div'); empty.className = 'asset-folder-empty-hint';
      empty.textContent = SM && SM.t ? SM.t('mediaDropHint') : 'Aucun média importé.';
      grid.appendChild(empty);
    }
    // Bulk cleanup (2026-07-31) — catalog-only (never touches the
    // underlying layer, which has its own trash button already): shows/
    // hides based on whether there's anything TO clean, and labels the
    // count so it isn't a mystery button.
    var cleanupBtn = document.getElementById('btn-media-cleanup');
    if (cleanupBtn) {
      cleanupBtn.style.display = _orphanCount ? '' : 'none';
      cleanupBtn.textContent = 'Nettoyer (' + _orphanCount + ')';
    }
  }
  // Catalog-only cleanup: removes entries whose source layer no longer
  // resolves (deleted, or renamed on a pre-migration entry with no uid).
  // Never deletes the layer itself — that stays the layer panel's job.
  function cleanupOrphans() {
    var before = (state.mediaLibrary || []).length;
    state.mediaLibrary = (state.mediaLibrary || []).filter(function (m) {
      return !(m.layerName || m.layerUid) || resolveSrcLayer(m);
    });
    var removed = before - state.mediaLibrary.length;
    render();
    if (window.showToast) showToast(removed + SM.t('toastOrphanEntriesRemovedSuffix'));
  }

  // OS drag-and-drop onto the panel's own drop zone (#media-drop) — routes
  // through images.js's real import pipeline (images.js:importImageFiles/
  // importVideoFile), same as the toolbar buttons, so entries land as
  // normal layers AND register a library entry. drop-import.js's canvas/
  // timeline drop target now routes image/video files through this exact
  // same pipeline too (2026-08 fix) — only genuinely unrecognized file
  // types still fall back to the rotoscopy reference importer there.
  function initDropZone() {
    var zone = document.getElementById('media-drop'); if (!zone) return;
    zone.addEventListener('dragover', function (e) { if (e.dataTransfer && e.dataTransfer.types.indexOf('Files') >= 0) { e.preventDefault(); zone.classList.add('drop-hover'); } });
    zone.addEventListener('dragleave', function () { zone.classList.remove('drop-hover'); });
    zone.addEventListener('drop', function (e) {
      e.preventDefault(); zone.classList.remove('drop-hover');
      var files = Array.prototype.slice.call((e.dataTransfer && e.dataTransfer.files) || []);
      if (!files.length) return;
      var images = files.filter(function (f) { return f.type.indexOf('image/') === 0; });
      var videos = files.filter(function (f) { return f.type.indexOf('video/') === 0; });
      if (images.length && window.SM && window.SM.importImageFiles) window.SM.importImageFiles(images);
      videos.forEach(function (f) { if (window.SM && window.SM.importVideoFile) window.SM.importVideoFile(f); });
      if (!images.length && !videos.length && window.showToast) showToast(SM.t('toastFormatNotRecognizedDropHint'));
    });
  }

  // Canvas drop target for a library-thumbnail drag (internal DnD via the
  // custom 'application/x-nemo-media-id' MIME, distinct from drop-import.js's
  // OS-`Files`-only check — coexists on the same #canvas-area without
  // conflict since each only reacts to its own dataTransfer type).
  function initCanvasDropTarget() {
    var el = document.getElementById('canvas-area'); if (!el) return;
    el.addEventListener('dragover', function (e) {
      if (e.dataTransfer && e.dataTransfer.types.indexOf('application/x-nemo-media-id') >= 0) e.preventDefault();
    });
    el.addEventListener('drop', function (e) {
      var id = e.dataTransfer && e.dataTransfer.getData('application/x-nemo-media-id');
      if (!id) return;
      e.preventDefault();
      var m = (state.mediaLibrary || []).find(function (x) { return x.id === id; });
      if (!m) return;
      var pt = window.SMEngineBridge && SMEngineBridge.screenToWorld ? SMEngineBridge.screenToWorld(e.clientX, e.clientY) : null;
      insertImageOnCanvas(m, pt);
    });
  }

  // Compact/expand toggle (2026-07-31, Cyril: "un petit bouton qui ouvrira
  // un plus grand en hauteur") — #media-grid defaults to a bounded, scrolling
  // height (css); the chevron just flips a class, same idiom as effects-
  // panel.js's per-row .fx-row-chevron/.expanded rotate-on-toggle, not the
  // continuous #tl-resize drag-handle (that's a different interaction).
  function initExpandToggle() {
    var btn = document.getElementById('media-expand-toggle');
    var grid = document.getElementById('media-grid');
    if (!btn || !grid) return;
    btn.addEventListener('click', function () {
      grid.classList.toggle('expanded');
      btn.classList.toggle('expanded', grid.classList.contains('expanded'));
    });
  }

  function init() {
    render();
    initDropZone();
    initCanvasDropTarget();
    initExpandToggle();
    var bImg = document.getElementById('btn-media-import-img'); if (bImg) bImg.addEventListener('click', function () { if (window.SM) SM.importImages(); });
    var bVid = document.getElementById('btn-media-import-video'); if (bVid) bVid.addEventListener('click', function () { if (window.SM) SM.importVideo(); });
    var bClean = document.getElementById('btn-media-cleanup'); if (bClean) bClean.addEventListener('click', cleanupOrphans);
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', init); else init();

  window.SMMediaLibrary = { addEntry: addEntry, updateEntry: updateEntry, removeEntry: removeEntry, reload: render };
})();
