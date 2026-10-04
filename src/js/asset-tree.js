// Shared presentation helper for the Media grid and Transplant picker.
// It owns folder headers and collapse listeners; callers own body rows.
(function () {
function createAssetTree(domDocument, translate) {
  'use strict';
  var FOLDER_COLORS = {
    components: 'var(--purple)',
    compositions: 'var(--red)',
    image: 'var(--accent)',
    video: 'var(--orange)',
    audio: 'var(--green)',
  };
  function t(key, fallback) { return translate(key, fallback); }
  // Resolve on every read: the selected language can change after bootstrap.
  var KIND_GROUP_LABEL = {
    get image() { return t('assetGroupImages', 'Images'); },
    get video() { return t('assetGroupVideos', 'Vidéos'); },
    get audio() { return t('assetGroupAudio', 'Audio'); },
  };

  function folderGroup(container, opts) {
    var folder = domDocument.createElement('div'); folder.className = 'asset-folder';
    var hdr = domDocument.createElement('div'); hdr.className = 'asset-folder-hdr';
    var chev = domDocument.createElement('span'); chev.className = 'asset-folder-chevron'; chev.textContent = '▾';
    var icon = domDocument.createElement('span'); icon.className = 'asset-folder-icon'; icon.style.setProperty('--folder-color', opts.color || 'var(--text-dim)');
    var label = domDocument.createElement('span'); label.className = 'asset-folder-label'; label.textContent = opts.label;
    var count = domDocument.createElement('span'); count.className = 'asset-folder-count'; count.textContent = opts.count != null ? String(opts.count) : '';
    hdr.appendChild(chev); hdr.appendChild(icon); hdr.appendChild(label); hdr.appendChild(count);
    var body = domDocument.createElement('div'); body.className = 'asset-folder-body';
    if (opts.defaultCollapsed) { hdr.classList.add('collapsed'); body.classList.add('collapsed'); }
    hdr.addEventListener('click', function () {
      hdr.classList.toggle('collapsed');
      body.classList.toggle('collapsed');
    });
    folder.appendChild(hdr); folder.appendChild(body);
    container.appendChild(folder);
    return body;
  }

  return {
    FOLDER_COLORS: FOLDER_COLORS,
    KIND_GROUP_LABEL: KIND_GROUP_LABEL,
    componentsLabel: function () { return t('assetGroupComponents', 'Composants'); },
    compositionsLabel: function () { return t('assetGroupCompositions', 'Compositions'); },
    layersLabel: function () { return t('assetGroupLayers', 'Calques'); },
    // opts.key is accepted for caller compatibility but has never persisted
    // collapse state. Recreating a folder applies defaultCollapsed anew.
    folderGroup: folderGroup,
  };
}

if (typeof module !== 'undefined' && module.exports) module.exports = { createAssetTree: createAssetTree };
if (typeof window !== 'undefined') {
  var browserAssetTree = createAssetTree(window.document, function (key, fallback) {
    return window.SM && typeof window.SM.t === 'function' ? window.SM.t(key) : fallback;
  });
  // Retain the literal classic-script facade for callers and the static
  // inventory; all behavior delegates to the injected production factory.
  window.SMAssetTree = {
    FOLDER_COLORS: browserAssetTree.FOLDER_COLORS,
    KIND_GROUP_LABEL: browserAssetTree.KIND_GROUP_LABEL,
    componentsLabel: function () { return browserAssetTree.componentsLabel(); },
    compositionsLabel: function () { return browserAssetTree.compositionsLabel(); },
    layersLabel: function () { return browserAssetTree.layersLabel(); },
    folderGroup: function (container, opts) { return browserAssetTree.folderGroup(container, opts); },
  };
}
})();
