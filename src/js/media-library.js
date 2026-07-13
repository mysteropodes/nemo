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

  function addEntry(name, kind, thumb, layerName) {
    if (!state.mediaLibrary) state.mediaLibrary = [];
    state.mediaLibrary.push({ id: uid(), name: name, kind: kind, thumb: thumb, layerName: layerName || null, importedAt: Date.now() });
    render();
  }

  function jumpToLayer(m) {
    if (!m.layerName) return;
    var idx = state.layers.findIndex(function (l) { return l.name === m.layerName; });
    if (idx < 0) { if (window.showToast) showToast('Calque source introuvable (supprimé ou renommé)'); return; }
    if (window.SM) SM.setActiveLayer(idx);
    if (window.showToast) showToast('Calque « ' + m.layerName + ' » sélectionné');
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
      if (window.showToast) showToast('Image insérée depuis la bibliothèque');
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
  function render() {
    var grid = document.getElementById('media-grid'); if (!grid) return;
    grid.innerHTML = '';
    (state.mediaLibrary || []).forEach(function (m) {
      var row = document.createElement('div'); row.className = 'media-row'; row.title = m.name;
      var thumb = document.createElement('div'); thumb.className = 'media-row-thumb';
      var img = document.createElement('img'); img.src = m.thumb; img.draggable = m.kind === 'image';
      thumb.appendChild(img);
      if (m.kind === 'video') { var pb = document.createElement('div'); pb.className = 'media-row-playicon'; pb.textContent = '▶'; thumb.appendChild(pb); }
      row.appendChild(thumb);

      var main = document.createElement('div'); main.className = 'media-row-main';
      var nameEl = document.createElement('div'); nameEl.className = 'media-row-name'; nameEl.textContent = m.name;
      main.appendChild(nameEl);

      var meta = document.createElement('div'); meta.className = 'media-row-meta';
      var kindBadge = document.createElement('span'); kindBadge.className = 'media-row-badge ' + (m.kind === 'video' ? 'kind-video' : 'kind-image'); kindBadge.textContent = m.kind === 'video' ? 'Vidéo' : 'Image';
      meta.appendChild(kindBadge);

      // "Owner" column equivalent: the source layer, colored dot + name —
      // or a muted "Orphelin" pill if that layer no longer exists (deleted/
      // renamed since import; jumpToLayer already toasted this on click,
      // this makes it visible without having to click first).
      var srcLayer = m.layerName ? state.layers.find(function (l) { return l.name === m.layerName; }) : null;
      if (m.layerName) {
        var owner = document.createElement('span'); owner.className = 'media-row-owner' + (srcLayer ? '' : ' orphan');
        if (srcLayer) { var dot = document.createElement('span'); dot.className = 'media-row-owner-dot'; dot.style.background = srcLayer.color || 'var(--text-dim)'; owner.appendChild(dot); }
        var ownerLbl = document.createElement('span'); ownerLbl.textContent = srcLayer ? m.layerName : 'Orphelin';
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
        if (m.layerName) items.push({ label: 'Sélectionner le calque', action: function () { jumpToLayer(m); } });
        if (m.kind === 'image') items.push({ label: 'Insérer une copie sur le calque actif', action: function () { insertImageOnCanvas(m); } });
        items.push({ label: 'Retirer de la bibliothèque', action: function () { removeEntry(m.id); } });
        window.showContextMenu(e.clientX, e.clientY, items);
      });
      if (m.kind === 'image') {
        img.addEventListener('dragstart', function (e) {
          e.dataTransfer.setData('application/x-nemo-media-id', m.id);
          e.dataTransfer.effectAllowed = 'copy';
        });
      }
      grid.appendChild(row);
    });
  }

  // OS drag-and-drop onto the panel's own drop zone (#media-drop) — separate
  // from drop-import.js's canvas/timeline drop target, which is scoped to
  // the rotoscopy reference (SMReference.importFiles) and deliberately left
  // untouched here to avoid changing that existing workflow. Dropping onto
  // THIS zone routes through images.js's real import pipeline instead
  // (images.js:importImageFiles/importVideoFile), same as the toolbar
  // buttons, so entries land as normal layers AND register a library entry.
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
      if (!images.length && !videos.length && window.showToast) showToast('Format non reconnu — dépose une image ou une vidéo');
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

  function init() {
    render();
    initDropZone();
    initCanvasDropTarget();
    var bImg = document.getElementById('btn-media-import-img'); if (bImg) bImg.addEventListener('click', function () { if (window.SM) SM.importImages(); });
    var bVid = document.getElementById('btn-media-import-video'); if (bVid) bVid.addEventListener('click', function () { if (window.SM) SM.importVideo(); });
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', init); else init();

  window.SMMediaLibrary = { addEntry: addEntry, reload: render };
})();
