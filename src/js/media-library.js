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
      // webHandleId/naturalW/naturalH (2026-08-29, linked-media.js): a
      // web-linked image's IndexedDB reference (no filesystem path exists
      // to store instead — see linked-media.js's own header comment) plus
      // its ORIGINAL pixel size, needed since `thumb` for a linked entry is
      // a small preview, not the full image (see the header comment above:
      // re-embedding the full image as a "thumbnail" would defeat the
      // entire point of linking).
      webHandleId: opts.webHandleId || null,
      naturalW: opts.naturalW || null, naturalH: opts.naturalH || null,
      // audioId: audio tracks have no owning layer at all (state.audioTracks
      // is a separate array) — this is their own identity, used only by the
      // 'Supprimer la piste' menu action below to find the right track.
      audioId: opts.audioId || null,
      sizeBytes: opts.linked ? null : dataUrlBytes(thumb),
      importedAt: Date.now(),
      status: opts.status || 'ready',
      // linkedBroken/linkedBrokenReason (2026-08-29): session-only, set by
      // linked-media.js's resolveAsync when the file/handle can't be read
      // right now — deliberately not part of exportJSON (see that file's
      // own comment on why this must never round-trip stale).
      linkedBroken: false, linkedBrokenReason: null,
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
  // Can this entry be dropped back onto the canvas? (feedback #159 — "il
  // faudrait que ça soit un élément persistent tant que l'on ne l'enlève pas
  // de ce panel"). One decider, used both to arm the drag and to explain the
  // refusal, so the thumbnail never offers a gesture the drop would drop.
  //
  // Images always can: the entry carries its own bytes (or, when linked, a
  // file reference desR resolves lazily), and deleting the raster from the
  // canvas never touched either. Confirmed live, including for an entry whose
  // source layer no longer exists.
  //
  // Video is the half that was broken: the drag was armed for images only, so
  // a video thumbnail was inert forever. What it can be rebuilt from depends
  // on the media mode:
  //  - linked: `path` (desktop) or `webHandleId` (web) still points at the
  //    original file, so a real re-import is possible — this is the case that
  //    makes the panel a genuine library.
  //  - embedded: the project deliberately keeps no copy of the source bytes
  //    (feedback #154 stopped baking base64), only a first-frame preview. The
  //    decoded frames live in the owning layer, so a re-drop is possible for
  //    as long as that layer exists, and genuinely impossible after it goes.
  function reinsertKind(m) {
    if (!m) return null;
    if (m.kind === 'image') return 'image';
    if (m.kind !== 'video') return null;
    if (m.linked && (m.path || m.webHandleId)) return 'video-linked';
    if (resolveSrcLayer(m)) return 'video-clone';
    return null;
  }
  function reinsertBlockedReason(m) {
    if (m.kind !== 'video') return null;
    return t('mediaVideoSourceGoneTip',
      'Source introuvable — la vidéo était intégrée et son calque a été supprimé. En mode Liés, le fichier d\'origine reste réutilisable.');
  }

  // Re-imports a video as its own NEW layer (a video is a layer, not a stroke
  // on one — unlike insertImageOnCanvas below), delegating to the same
  // importAsLayer the original import used rather than reimplementing decode.
  async function insertVideoAsLayer(m) {
    var kind = reinsertKind(m);
    if (kind === 'video-linked') {
      if (!window.SMNativeVideo) return;
      var source = m.path;
      if (!source && m.webHandleId && window.SMLinkedMedia && SMLinkedMedia.getHandleFile) {
        try { source = await SMLinkedMedia.getHandleFile(m.webHandleId); } catch (e) { source = null; }
      }
      if (!source) {
        if (window.showToast) showToast(t('mediaVideoRelinkNeeded', 'Fichier lié inaccessible — autorise l\'accès ou relie le fichier.'));
        return;
      }
      await SMNativeVideo.importAsLayer(source, m.webHandleId ? { webHandleId: m.webHandleId } : {});
      return;
    }
    if (kind === 'video-clone') {
      // Embedded: the decoded rasters still live in the source layer, so this
      // duplicates that layer rather than decoding anything a second time.
      var src = resolveSrcLayer(m);
      var si = state.layers.indexOf(src);
      if (si < 0) return;
      if (window.SM && SM.duplicateLayer) {
        var prev = state.activeLayerIdx;
        SM.setActiveLayer(si);
        SM.duplicateLayer();
        if (state.activeLayerIdx === si) SM.setActiveLayer(prev);
      }
      if (window.showToast) showToast(t('mediaVideoReinserted', 'Vidéo réinsérée sur un nouveau calque'));
      return;
    }
    if (window.showToast) showToast(reinsertBlockedReason(m));
  }

  function insertImageOnCanvas(m, worldPt) {
    if (m.kind !== 'image') return;
    if (window.pushUndo) pushUndo();
    if (window.saveAllLayerFrames) saveAllLayerFrames();
    var ld = state.layers[state.activeLayerIdx];
    if (!ld.frames[state.currentFrame].isKeyframe && !ld.frames[state.currentFrame].isInterpolated) {
      ld.frames[state.currentFrame].strokes = JSON.parse(JSON.stringify(getEffectiveStrokes(state.activeLayerIdx, state.currentFrame)));
      ld.frames[state.currentFrame].isKeyframe = true;
    }
    // Linked entry (2026-08-29, linked-media.js): m.thumb is only a small
    // preview (see this file's own header comment — re-embedding the FULL
    // image here would defeat the entire point of linking), so this pushes
    // the SAME linked marker instead of a `src`, resolved lazily by desR
    // exactly like the original import's strokes. naturalW/naturalH (stored
    // at import time) stand in for the onload probe below, which only
    // makes sense against a real full-res `src`.
    if (m.linked) {
      var nw = m.naturalW || 1, nh = m.naturalH || 1;
      var sc = Math.min(1, state.canvasW / nw, state.canvasH / nh);
      var stroke = { isRaster: true, linked: true, x: worldPt ? worldPt.x : state.canvasW / 2, y: worldPt ? worldPt.y : state.canvasH / 2, width: nw * sc, height: nh * sc, opacity: 1 };
      if (m.path != null) stroke.linkedPath = m.path; else if (m.webHandleId) stroke.linkedHandleId = m.webHandleId;
      ld.frames[state.currentFrame].strokes.push(stroke);
      if (window.loadFrame) loadFrame(state.currentFrame);
      if (window.updateUI) updateUI();
      if (window.showToast) showToast(SM.t('toastImageInsertedFromLibrary'));
      return;
    }
    // No preview bytes to insert (2026-08-29): an embedded entry's `thumb` IS
    // the image here, so a null one means there is genuinely nothing to place —
    // bail with a message instead of feeding `new Image()` a null (which the
    // browser coerces to the string "null", firing a spurious GET /null) and
    // then pushing a stroke carrying `src: null` into the frame, which is real
    // data corruption rather than just a bad request.
    if (!m.thumb) { if (window.showToast) showToast(t('toastMediaNoPreview', 'Aperçu indisponible pour ce média.')); return; }
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
  // Lazy getters, not a literal — this was a baked-in French object, so the
  // kind badge read "Vidéo" inside an otherwise English panel (spotted while
  // fixing feedback #158, same family as #157). Mirrors asset-tree.js's
  // KIND_GROUP_LABEL, which already resolves through SM.t for this reason.
  var KIND_LABEL = {
    get video() { return t('mediaKindVideo', 'Vidéo'); },
    get image() { return t('mediaKindImage', 'Image'); },
    get audio() { return t('mediaKindAudio', 'Audio'); },
  };
  var _orphanCount = 0; // set fresh by buildRow() on every render() pass, read back after
  // ---- Search / type filter / view mode (2026-08-29, gap-analysis pass
  // against the "Stash" AE panel Cyril pointed at as a reference) — plain
  // module state, not persisted: reopening the panel/project starts back at
  // "All / list", same convention as _viewMode below and unlike the actually-
  // persisted expand-toggle (that one survives because it's a layout
  // preference, not a transient browsing filter).
  var _searchQuery = '';
  var _activeFilter = 'all'; // 'all' | 'image' | 'video' | 'audio' | 'missing'
  // 'list' | 'compact' | 'grid' — three densities since feedback #158 asked for
  // "une liste plus petite". compact reuses buildRow (same DOM, same context
  // menu, same drag wiring) and only changes CSS: a smaller thumb and a single
  // line, so nothing downstream has to know a third mode exists.
  var VIEW_MODES = ['list', 'compact', 'grid'];
  var _viewMode = 'list';
  try { var _vmSaved = localStorage.getItem('nemo-media-view'); if (VIEW_MODES.indexOf(_vmSaved) >= 0) _viewMode = _vmSaved; } catch (e) {}
  // Applied to #media-grid AND to every per-kind folder body (rows live in the
  // folder bodies, not in the grid itself — see render()).
  function applyViewClasses(el) {
    el.classList.toggle('media-grid-view', _viewMode === 'grid');
    el.classList.toggle('media-compact-view', _viewMode === 'compact');
  }
  var FILTER_CHIPS = [
    { id: 'all', get label() { return t('mediaFilterAll', 'Tout'); } },
    { id: 'image', get label() { return t('assetGroupImages', 'Images'); } },
    { id: 'video', get label() { return t('assetGroupVideos', 'Vidéos'); } },
    { id: 'audio', get label() { return t('assetGroupAudio', 'Audio'); } },
    { id: 'missing', get label() { return t('mediaFilterMissing', 'Introuvable'); } },
  ];
  function t(key, fallback) { return (window.SM && SM.t) ? SM.t(key) : fallback; }
  // An entry counts as "missing" the same way the existing orphan-cleanup
  // button already does — its source layer no longer resolves (deleted, or
  // renamed on a pre-migration entry with no uid). This is NOT filesystem-
  // level "the linked file moved on disk" detection (Stash's own Missing
  // filter is exactly that) — Nemo has no path-exists check exposed from
  // Tauri to build that on, and almost everything here is embedded bytes
  // anyway, so "the catalog entry can no longer be resolved at all" is the
  // realistic definition of broken for this app today, not a smaller
  // stand-in for the real thing.
  // linkedBroken (2026-08-29, linked-media.js): the source LAYER still
  // resolves fine here — it's the underlying FILE/handle that's gone (moved,
  // deleted, or a web handle from a different browser/machine/session). A
  // genuinely different flavor of "missing" than the orphan-layer case
  // below, but the same filter/badge surface makes sense for both — this is
  // exactly the "broken link" state a real filesystem-level Missing filter
  // would show, which the pre-existing comment on this function noted Nemo
  // didn't have a way to build until linked media existed.
  function isMissing(m) { return (!!(m.layerName || m.layerUid) && !resolveSrcLayer(m)) || !!m.linkedBroken; }
  // Thumbnail element for one entry — an <img> when there are real preview
  // bytes, otherwise the SAME icon-placeholder treatment audio entries already
  // use ('♪'), so a missing preview reads as a deliberate state rather than a
  // broken image (2026-08-29). `m.thumb` is genuinely nullable: a linked entry
  // whose small-preview generation failed stores null (linked-media.js), and a
  // linked entry pointing at a moved/unreadable file has no bytes to preview at
  // all. Assigning null to img.src makes the browser coerce it to the STRING
  // "null" and issue a real GET /null — a 404 in the network log on every
  // render of that row, for a request nothing wanted.
  function appendThumbVisual(container, m, iconClass) {
    if (!m.thumb) { container.classList.add(iconClass); container.textContent = '▦'; return null; }
    var img = document.createElement('img');
    img.src = m.thumb;
    img.draggable = m.kind === 'image';
    container.appendChild(img);
    return img;
  }
  // Matches the SAME fields a user would recognize the entry by: its name,
  // and its source layer's name (so searching "background" finds every clip
  // that layer owns, not just files literally named that).
  function matchesSearch(m, q) {
    if (!q) return true;
    var hay = (m.name || '') + ' ' + (m.layerName || '');
    return hay.toLowerCase().indexOf(q) >= 0;
  }
  function matchesFilter(m) {
    if (_activeFilter === 'all') return true;
    if (_activeFilter === 'missing') return isMissing(m);
    return m.kind === _activeFilter;
  }
  function filteredEntries() {
    var q = _searchQuery.trim().toLowerCase();
    return (state.mediaLibrary || []).filter(function (m) { return matchesFilter(m) && matchesSearch(m, q); });
  }
  function renderFilterChips() {
    var wrap = document.getElementById('media-filter-chips');
    if (!wrap) return;
    wrap.innerHTML = '';
    FILTER_CHIPS.forEach(function (c) {
      var btn = document.createElement('button');
      btn.type = 'button';
      btn.className = 'media-filter-chip' + (_activeFilter === c.id ? ' active' : '');
      btn.textContent = c.label;
      if (c.id === 'missing') {
        var n = (state.mediaLibrary || []).filter(isMissing).length;
        if (!n) return; // no point offering a filter that would always show nothing
        btn.textContent = c.label + ' (' + n + ')';
      }
      btn.addEventListener('click', function () { _activeFilter = c.id; render(); });
      wrap.appendChild(btn);
    });
  }
  // Compact tile for grid mode — thumbnail-forward, name below, same click/
  // context-menu/drag wiring as a list row but built fresh rather than
  // reusing buildRow() outright: a list row's rich per-field layout
  // (kind+size+owner+date all on their own line) has no equivalent grid
  // arrangement that wouldn't just be a squeezed, unreadable copy of the
  // list row — Stash's own grid tiles are thumbnail+name only too.
  function buildTile(m) {
    var tile = document.createElement('div'); tile.className = 'media-tile' + (m.status === 'loading' ? ' loading' : ''); tile.title = m.name;
    var thumb = document.createElement('div'); thumb.className = 'media-tile-thumb';
    if (m.status === 'loading') {
      thumb.classList.add('media-tile-thumb-loading');
      var sp = document.createElement('div'); sp.className = 'media-row-spinner'; thumb.appendChild(sp);
    } else if (m.kind === 'audio') {
      thumb.classList.add('media-tile-thumb-icon'); thumb.textContent = '♪';
    } else {
      var img = appendThumbVisual(thumb, m, 'media-tile-thumb-icon');
      if (m.kind === 'video') { var pb = document.createElement('div'); pb.className = 'media-row-playicon'; pb.textContent = '▶'; thumb.appendChild(pb); }
      // Drag source is the img when there IS one, the tile's thumb box
      // otherwise — a linked entry whose preview failed is still perfectly
      // insertable (the linked branch of insertImageOnCanvas never reads
      // m.thumb), so losing drag there would be a real regression.
      if (reinsertKind(m)) {
        var dragEl = img || thumb;
        dragEl.draggable = true;
        dragEl.addEventListener('dragstart', function (e) {
          e.dataTransfer.setData('application/x-nemo-media-id', m.id);
          e.dataTransfer.effectAllowed = 'copy';
        });
      } else if (m.kind === 'video') {
        thumb.title = reinsertBlockedReason(m);
      }
    }
    if (isMissing(m)) { var mb = document.createElement('div'); mb.className = 'media-tile-missing-badge'; mb.title = t('mediaMissingTip', 'Calque source introuvable'); mb.textContent = '!'; thumb.appendChild(mb); }
    tile.appendChild(thumb);
    if (m.status !== 'loading') {
      var nameEl = document.createElement('div'); nameEl.className = 'media-tile-name'; nameEl.textContent = m.name;
      tile.appendChild(nameEl);
      tile.addEventListener('click', function () { jumpToLayer(m); });
      tile.addEventListener('contextmenu', function (e) {
        e.preventDefault(); e.stopPropagation();
        // Reuses buildRow's own context-menu construction by delegating to
        // a synthetic row (never inserted) — avoids a second copy of the
        // same item-actions list drifting out of sync with the list view's.
        var proxyRow = buildRow(m);
        proxyRow.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: e.clientX, clientY: e.clientY }));
      });
    }
    return tile;
  }
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
        var img = appendThumbVisual(thumb, m, 'media-row-thumb-icon');
        if (m.kind === 'video') { var pb = document.createElement('div'); pb.className = 'media-row-playicon'; pb.textContent = '▶'; thumb.appendChild(pb); }
        // Feedback #106: the row already flipped out of status:'loading' (one
        // frame decoded, thumbnail built — genuinely instant), but the video
        // may still be optimizing in the background (native-video-bridge.js's
        // _optimizeLayerMedia, separate fire-and-forget transcode) before it's
        // truly instant-scrub-ready — this small corner spinner is that
        // "still working, don't worry" signal the loading overlay used to give.
        if (m.kind === 'video' && m.optimizing) {
          var ob = document.createElement('div'); ob.className = 'media-row-optim-badge';
          ob.title = SM && SM.t ? SM.t('mediaOptimizingTip') : 'Optimisation en arrière-plan pour un scrub instantané…';
          thumb.appendChild(ob);
        }
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
        var linkBadge = document.createElement('span'); linkBadge.className = 'media-row-badge kind-linked'; linkBadge.textContent = SM.t('mediaLinkedFileBadge'); linkBadge.title = m.path || m.webHandleId || '';
        meta.appendChild(linkBadge);
        // Broken-link badge (2026-08-29, linked-media.js) — file moved/
        // deleted (desktop) or handle missing/permission not granted (web).
        // Separate badge from kind-linked above so "can go offline" (always
        // true for a linked entry) and "IS offline right now" stay visually
        // distinct — see this file's own isMissing() comment.
        if (m.linkedBroken) {
          var brokenBadge = document.createElement('span'); brokenBadge.className = 'media-row-badge kind-missing';
          brokenBadge.textContent = m.linkedBrokenReason === 'permission' ? t('mediaNeedsPermissionBadge', 'Permission requise') : t('mediaMissingFileBadge', 'Introuvable');
          meta.appendChild(brokenBadge);
        }
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
        // No _orphanCount++ here. render()'s own pass over the UNFILTERED
        // catalog is the one counting site; this one used to increment too,
        // so every orphan was counted twice (and only while it was visible,
        // in list mode — buildTile has no equivalent line, so switching to
        // grid changed the number). Found by driving it: one orphaned entry
        // reported "2".
        var owner = document.createElement('span'); owner.className = 'media-row-owner' + (srcLayer ? '' : ' orphan');
        if (srcLayer) { var dot = document.createElement('span'); dot.className = 'media-row-owner-dot'; dot.style.background = srcLayer.color || 'var(--text-dim)'; owner.appendChild(dot); }
        var ownerLbl = document.createElement('span'); ownerLbl.textContent = srcLayer ? srcLayer.name : SM.t('mediaOrphanBadge');
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
        if (srcLayer) items.push({ label: SM.t('ctxSelectLayer'), action: function () { jumpToLayer(m); } });
        if (m.kind === 'image') items.push({ label: SM.t('ctxInsertCopyOnActiveLayer'), action: function () { insertImageOnCanvas(m); } });
        // Relink (2026-07-31): only nativeVideo entries can go offline (a
        // moved/deleted file — the embedded kinds never can, their bytes
        // live IN the project). replaceNativeVideoSource is native-video-
        // bridge.js's own relink flow, same dialog/session-swap shape as
        // images.js's replaceFootageSource for the embedded raster kinds.
        if (m.linked && srcLayer && m.kind === 'video' && window.SMNativeVideo && window.SMNativeVideo.replaceNativeVideoSource) {
          items.push({ label: SM.t('ctxRelinkReplaceFile'), action: function () {
            var li = state.layers.indexOf(srcLayer);
            window.SMNativeVideo.replaceNativeVideoSource(li);
          } });
        }
        // Same relink action for a linked IMAGE (2026-08-29, linked-media.js)
        // — the desktop/web dispatch and the actual dialog/picker live there,
        // mirroring replaceNativeVideoSource's own shape for the per-frame
        // raster-stroke storage images use instead of a layer-level session.
        if (m.linked && srcLayer && m.kind === 'image' && window.SMLinkedMedia && window.SMLinkedMedia.relinkImageEntry) {
          items.push({ label: SM.t('ctxRelinkReplaceFile'), action: function () { window.SMLinkedMedia.relinkImageEntry(m); } });
        }
        // Audio has real controls (mute/volume/offset-drag) on its own
        // timeline row already (audio-bridge.js) — deliberately NOT
        // duplicated here (same divergence risk CLAUDE.md flags for propsFor/
        // panel-grid). Only delete, reusing that file's own stop+splice.
        if (m.kind === 'audio' && m.audioId) {
          items.push({ label: SM.t('ctxDeleteTrack'), action: function () {
            if (window.SMAudio && SMAudio.removeTrackByAudioId(m.audioId)) removeEntry(m.id);
          } });
        } else {
          items.push({ label: SM.t('ctxRemoveFromLibrary'), action: function () { removeEntry(m.id); } });
        }
        window.showContextMenu(e.clientX, e.clientY, items);
      });
      // Same fallback as the tile view: drag from the thumb box when there's
      // no preview img to hang the listener on.
      if (reinsertKind(m)) {
        var dragEl = img || thumb;
        dragEl.draggable = true;
        dragEl.addEventListener('dragstart', function (e) {
          e.dataTransfer.setData('application/x-nemo-media-id', m.id);
          e.dataTransfer.effectAllowed = 'copy';
        });
      } else if (m.kind === 'video') {
        thumb.title = reinsertBlockedReason(m);
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
    applyViewClasses(grid);
    _orphanCount = 0;
    renderFilterChips();
    if (!window.SMAssetTree) return; // asset-tree.js not loaded — nothing to group into
    var q = _searchQuery.trim().toLowerCase();
    // Composants (symbols) is its own axis (not a state.mediaLibrary kind) —
    // only offered under "Tout"/no type filter, same reasoning as Stash's
    // own Comps chip being mutually exclusive with Video/Images/etc there:
    // narrowing to a media KIND has no sensible reading of "also show
    // components". Search still applies, by symbol name.
    var symIds = _activeFilter === 'all' ? Object.keys(state.symbols || {}).filter(function (sid) {
      return !q || ((state.symbols[sid].name || '').toLowerCase().indexOf(q) >= 0);
    }) : [];
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
    // Compositions (feedback #140: "les différents projets ouverts dans
    // nemo (onglet de projet) devraient apparaître comme des compositions
    // dans média et preset") — every OTHER open project tab, read straight
    // from SMProject.getOpenTabs() (project.js; already built for the
    // near-identical feedback #109, consumed there by transplant.js's
    // picker modal). Same "all filter only" + name-search rule as
    // Composants just above — a project tab isn't a media KIND either.
    var openTabs = (_activeFilter === 'all' && window.SMProject && SMProject.getOpenTabs)
      ? SMProject.getOpenTabs().filter(function (t) { return !q || (t.name || '').toLowerCase().indexOf(q) >= 0; })
      : [];
    if (openTabs.length) {
      var compoBody = SMAssetTree.folderGroup(grid, { label: SMAssetTree.compositionsLabel(), color: SMAssetTree.FOLDER_COLORS.compositions, count: openTabs.length });
      openTabs.forEach(function (t) {
        var row = document.createElement('div'); row.className = 'bp-item'; row.title = t.name;
        row.draggable = true;
        var icon = document.createElement('span'); icon.className = 'bp-item-icon'; icon.textContent = '⧉';
        row.appendChild(icon);
        var span = document.createElement('span'); span.textContent = t.name || 'Untitled';
        row.appendChild(span);
        // Internal DnD, same idiom as an image row's 'application/x-nemo-
        // media-id' below — the payload is just the tab id, re-resolved
        // against a FRESH SMProject.getOpenTabs() at drop time (see
        // media-library.js's canvas/timeline drop targets and
        // instantiateForeignProjectAsComponent in app.js) rather than
        // carrying the (possibly large) JSON snapshot through
        // dataTransfer itself.
        row.addEventListener('dragstart', function (e) {
          e.dataTransfer.setData('application/x-nemo-project-tab', t.id);
          e.dataTransfer.effectAllowed = 'copy';
        });
        compoBody.appendChild(row);
      });
    }
    var visible = filteredEntries();
    var anyShown = symIds.length > 0 || openTabs.length > 0;
    ['image', 'video', 'audio'].forEach(function (kind) {
      var entries = visible.filter(function (m) { return m.kind === kind; });
      // _orphanCount must count EVERY orphan regardless of the active
      // filter/search (the cleanup button acts on the whole catalog, not
      // just what's currently visible) — computed from the unfiltered
      // list, not `entries`.
      (state.mediaLibrary || []).filter(function (m) { return m.kind === kind; }).forEach(function (m) { if (isMissing(m)) _orphanCount++; });
      if (!entries.length) return;
      anyShown = true;
      var body = SMAssetTree.folderGroup(grid, { label: SMAssetTree.KIND_GROUP_LABEL[kind], color: SMAssetTree.FOLDER_COLORS[kind], count: entries.length });
      applyViewClasses(body);
      entries.forEach(function (m) { body.appendChild(_viewMode === 'grid' ? buildTile(m) : buildRow(m)); });
    });
    if (!anyShown) {
      var empty = document.createElement('div'); empty.className = 'asset-folder-empty-hint';
      var hasAnyContent = !!((state.mediaLibrary || []).length || Object.keys(state.symbols || {}).length
        || (window.SMProject && SMProject.getOpenTabs && SMProject.getOpenTabs().length));
      // Genuinely empty -> this IS the drop zone (its dashed styling and the
      // gesture it names). Empty only because a filter/search excludes
      // everything -> a plain message: media exists, the dashed "drop here"
      // box would be telling you the wrong thing.
      if (hasAnyContent) {
        empty.textContent = t('mediaNoMatch', 'Aucun média ne correspond à ce filtre.');
      } else {
        empty.className += ' media-drop';
        empty.textContent = t('mediaDropHint', 'Glisser des images/vidéos ici, ou cliquer Importer…');
      }
      grid.appendChild(empty);
    }
    // Bulk cleanup (2026-07-31) — catalog-only (never touches the
    // underlying layer, which has its own trash button already): shows/
    // hides based on whether there's anything TO clean, and labels the
    // count so it isn't a mystery button.
    // Now a badge in the status bar (feedback #160): it states the PROBLEM
    // ("2 manquants") instead of the remedy, appears only when there is one,
    // and clicking it still runs the same cleanup. Hardcoded French label
    // fixed on the way through — it read "Nettoyer (0)" in every locale.
    var cleanupBtn = document.getElementById('btn-media-cleanup');
    if (cleanupBtn) {
      cleanupBtn.style.display = _orphanCount ? '' : 'none';
      cleanupBtn.textContent = _orphanCount + ' ' + t(_orphanCount > 1 ? 'mediaMissingCountOther' : 'mediaMissingCountOne', 'manquant');
      cleanupBtn.title = t('mediaMissingBadgeTitle', 'Entrées dont la source a disparu — cliquer pour les retirer de la bibliothèque');
    }
    // Left side of the status bar: what the panel holds.
    var countEl = document.getElementById('media-count');
    if (countEl) {
      var n = (state.mediaLibrary || []).length;
      countEl.textContent = n ? n + ' ' + t(n > 1 ? 'mediaItemsOther' : 'mediaItemsOne', 'élément') : '';
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

  // OS drag-and-drop onto the panel's own list (#media-grid) — routes
  // through images.js's real import pipeline (images.js:importImageFiles/
  // importVideoFile), same as the toolbar buttons, so entries land as
  // normal layers AND register a library entry. drop-import.js's canvas/
  // timeline drop target now routes image/video files through this exact
  // same pipeline too (2026-08 fix) — only genuinely unrecognized file
  // types still fall back to the rotoscopy reference importer there.
  // Bound to #media-grid, not to the dashed hint: the hint is rebuilt by
  // every render() (it IS the empty state now), so listeners on it would die
  // the first time the list changed. Listening on the stable container also
  // means dropping files onto a list that already has media works — which is
  // what anyone would try once the hint is gone.
  function initDropZone() {
    var zone = document.getElementById('media-grid'); if (!zone) return;
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
      if (m.kind === 'video') insertVideoAsLayer(m); else insertImageOnCanvas(m, pt);
    });
  }

  // Canvas + timeline drop targets for a "Compositions" row drag (feedback
  // #140) — internal DnD via the custom 'application/x-nemo-project-tab'
  // MIME, same coexistence idiom as initCanvasDropTarget's own
  // 'application/x-nemo-media-id' just above (each listener only reacts to
  // its own dataTransfer type, so OS-file drops via drop-import.js and the
  // media-thumbnail drop just above both keep working untouched on the same
  // elements). Wired on BOTH #canvas-area and #timeline-area — the issue's
  // own wording ("si on drop... elles apparaissent comme des composant dans
  // la timeline") points at the timeline, but the canvas is the more
  // discoverable drop target for a media-panel drag, so both accept it.
  //
  // Re-resolves the tab against a FRESH SMProject.getOpenTabs() at drop
  // time rather than trusting anything captured at dragstart — a tab can be
  // renamed, edited (changing its live-but-not-yet-snapshotted .json), or
  // closed in the moments between picking it up and releasing the drop.
  function initCompositionDropTargets() {
    function wire(id) {
      var el = document.getElementById(id); if (!el) return;
      el.addEventListener('dragover', function (e) {
        if (e.dataTransfer && e.dataTransfer.types.indexOf('application/x-nemo-project-tab') >= 0) e.preventDefault();
      });
      el.addEventListener('drop', function (e) {
        var tabId = e.dataTransfer && e.dataTransfer.getData('application/x-nemo-project-tab');
        if (!tabId) return;
        e.preventDefault();
        var openTabs = (window.SMProject && SMProject.getOpenTabs) ? SMProject.getOpenTabs() : [];
        var tab = openTabs.find(function (t) { return t.id === tabId; });
        if (!tab || !tab.json) { if (window.showToast) showToast('Cet onglet de projet n\'est plus disponible.'); return; }
        var foreignData;
        try { foreignData = JSON.parse(tab.json); }
        catch (e2) { if (window.showToast) showToast('Impossible de lire cet onglet de projet.'); return; }
        if (window.instantiateForeignProjectAsComponent) window.instantiateForeignProjectAsComponent(foreignData, tab.name || 'Untitled');
      });
    }
    wire('canvas-area');
    wire('timeline-area');
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

  // The button shows the mode you are IN, not the one you would switch to —
  // it is a 3-state cycle now, and "click me to get X" only reads
  // unambiguously on a 2-state toggle. The title names the next mode instead,
  // which is where that information belongs.
  var VIEW_ICON = {
    list:    '<rect x="3" y="4" width="18" height="4" rx="1"/><rect x="3" y="12" width="18" height="4" rx="1"/>',
    compact: '<line x1="3" y1="5" x2="21" y2="5"/><line x1="3" y1="10" x2="21" y2="10"/><line x1="3" y1="15" x2="21" y2="15"/><line x1="3" y1="20" x2="21" y2="20"/>',
    grid:    '<rect x="3" y="3" width="7" height="7" rx="1"/><rect x="14" y="3" width="7" height="7" rx="1"/><rect x="3" y="14" width="7" height="7" rx="1"/><rect x="14" y="14" width="7" height="7" rx="1"/>',
  };
  function syncViewToggle(btn) {
    btn.innerHTML = '<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">' + VIEW_ICON[_viewMode] + '</svg>';
    btn.classList.toggle('active', _viewMode !== 'list');
    var next = VIEW_MODES[(VIEW_MODES.indexOf(_viewMode) + 1) % VIEW_MODES.length];
    btn.title = t('mediaViewNext', 'Affichage') + ' \u2192 ' + t('mediaView_' + next, next);
  }
  // Settings gear (feedback #160) — closes on outside click and on Escape,
  // the two ways anyone expects a popover to go away. Bound once at init;
  // the popover's own contents are wired by linked-media.js, by id, and are
  // untouched by being moved in here.
  function initSettingsPopover() {
    var btn = document.getElementById('media-settings-btn');
    var pop = document.getElementById('media-settings-pop');
    if (!btn || !pop) return;
    function close() { pop.style.display = 'none'; btn.classList.remove('open'); }
    btn.addEventListener('click', function (e) {
      e.stopPropagation();
      var opening = pop.style.display === 'none';
      pop.style.display = opening ? 'block' : 'none';
      btn.classList.toggle('open', opening);
    });
    pop.addEventListener('click', function (e) { e.stopPropagation(); });
    document.addEventListener('click', function () { if (pop.style.display !== 'none') close(); });
    document.addEventListener('keydown', function (e) { if (e.key === 'Escape' && pop.style.display !== 'none') close(); });
  }
  function initSearchAndViewToggle() {
    var input = document.getElementById('media-search');
    var clearBtn = document.getElementById('media-search-clear');
    if (input) {
      input.addEventListener('input', function () {
        _searchQuery = input.value;
        if (clearBtn) clearBtn.classList.toggle('show', !!input.value);
        render();
      });
      input.addEventListener('keydown', function (e) { if (e.key === 'Escape') { input.value = ''; _searchQuery = ''; input.blur(); if (clearBtn) clearBtn.classList.remove('show'); render(); } });
    }
    if (clearBtn) {
      clearBtn.addEventListener('click', function () {
        _searchQuery = ''; if (input) input.value = '';
        clearBtn.classList.remove('show');
        render();
      });
    }
    var viewBtn = document.getElementById('media-view-toggle');
    if (viewBtn) {
      syncViewToggle(viewBtn);
      viewBtn.addEventListener('click', function () {
        _viewMode = VIEW_MODES[(VIEW_MODES.indexOf(_viewMode) + 1) % VIEW_MODES.length];
        try { localStorage.setItem('nemo-media-view', _viewMode); } catch (x) {}
        syncViewToggle(viewBtn);
        render();
      });
    }
  }

  function init() {
    // Rows are built once and cached in the DOM, so a language switch left
    // every badge/label in the previous locale until the next import forced a
    // render (confirmed live: SM.t already returned the new string while the
    // panel did not change). Same escape hatch, and same reason, as
    // linked-media.js's own afterI18n push — see its comment there. Pushed
    // from init(), i.e. on DOMContentLoaded, so it lands on the final
    // window.SM rather than one a later script replaces.
    window.SM = window.SM || {};
    window.SM.afterI18n = window.SM.afterI18n || [];
    if (window.SM.afterI18n.indexOf(render) < 0) window.SM.afterI18n.push(render);
    render();
    initDropZone();
    initCanvasDropTarget();
    initCompositionDropTargets();
    initExpandToggle();
    initSearchAndViewToggle();
    initSettingsPopover();
    var bImg = document.getElementById('btn-media-import-img'); if (bImg) bImg.addEventListener('click', function () { if (window.SM) SM.importImages(); });
    var bVid = document.getElementById('btn-media-import-video'); if (bVid) bVid.addEventListener('click', function () { if (window.SM) SM.importVideo(); });
    var bClean = document.getElementById('btn-media-cleanup'); if (bClean) bClean.addEventListener('click', cleanupOrphans);
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', init); else init();

  window.SMMediaLibrary = { addEntry: addEntry, updateEntry: updateEntry, removeEntry: removeEntry, reload: render };
})();
