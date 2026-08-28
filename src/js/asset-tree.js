// ---- Asset tree folder groups (2026-08) ----
// Feedback: "on est pas encore sur une vrai hierarchie avec label,
// folder..., ou composition" (AEP Transplant reference: colored folder
// icons, chevron collapse, nested rows) — shared by media-library.js (the
// docked Media tab) and transplant.js (the cross-project import picker) so
// both read as ONE consistent tree widget instead of two different list
// stylings. Deliberately just the FOLDER HEADER + collapse mechanics —
// each caller still builds its own row markup exactly as before (a media
// row's rich metadata vs. a plain checkbox row have nothing in common, so
// forcing a single row-renderer would be the wrong abstraction) and appends
// into the body element this returns.
//
// Folders are AUTOMATIC (by kind: Composants/Images/Vidéos/Audio), not
// user-managed — see the scope decision this session: no create/rename/
// drag-into-folder, just fixed groups with a distinct icon color each.
(function () {
  var FOLDER_COLORS = {
    components: 'var(--purple)',
    image: 'var(--accent)',
    video: 'var(--orange)',
    audio: 'var(--green)',
  };
  function t(key, fallback) { return (window.SM && SM.t) ? SM.t(key) : fallback; }
  // Resolved lazily (not a plain object literal) since SM.t() depends on
  // the currently-selected language, which can change after this file
  // loads (i18n.js's own SM.t() convention — no baked-in strings, see
  // CLAUDE.md's "still en français" incident on the Text Animator panel).
  var KIND_GROUP_LABEL = {
    get image() { return t('assetGroupImages', 'Images'); },
    get video() { return t('assetGroupVideos', 'Vidéos'); },
    get audio() { return t('assetGroupAudio', 'Audio'); },
  };
  window.SMAssetTree = {
    FOLDER_COLORS: FOLDER_COLORS,
    KIND_GROUP_LABEL: KIND_GROUP_LABEL,
    componentsLabel: function () { return t('assetGroupComponents', 'Composants'); },
    layersLabel: function () { return t('assetGroupLayers', 'Calques'); },
    // container: element to append the folder into.
    // opts: { key (unique per container, for collapse-state persistence),
    //         label, color (one of FOLDER_COLORS values or a CSS color),
    //         count, defaultCollapsed }
    // Returns the body element — append child rows into it.
    folderGroup: function (container, opts) {
      var folder = document.createElement('div'); folder.className = 'asset-folder';
      var hdr = document.createElement('div'); hdr.className = 'asset-folder-hdr';
      var chev = document.createElement('span'); chev.className = 'asset-folder-chevron'; chev.textContent = '▾';
      var icon = document.createElement('span'); icon.className = 'asset-folder-icon'; icon.style.setProperty('--folder-color', opts.color || 'var(--text-dim)');
      var label = document.createElement('span'); label.className = 'asset-folder-label'; label.textContent = opts.label;
      var count = document.createElement('span'); count.className = 'asset-folder-count'; count.textContent = opts.count != null ? String(opts.count) : '';
      hdr.appendChild(chev); hdr.appendChild(icon); hdr.appendChild(label); hdr.appendChild(count);
      var body = document.createElement('div'); body.className = 'asset-folder-body';
      if (opts.defaultCollapsed) { hdr.classList.add('collapsed'); body.classList.add('collapsed'); }
      hdr.addEventListener('click', function () {
        hdr.classList.toggle('collapsed');
        body.classList.toggle('collapsed');
      });
      folder.appendChild(hdr); folder.appendChild(body);
      container.appendChild(folder);
      return body;
    },
  };
})();
