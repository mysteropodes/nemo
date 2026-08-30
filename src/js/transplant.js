// ---- Transplant (2026-08) ----
// Feedback: "le panel media devienne un vrai panel de gestion de fichier
// projet comme pour AEP Transplant" — pick ANOTHER Nemo project .json file
// on disk and cherry-pick specific layers/media into the currently open
// project, without fully opening that other file (which would replace the
// whole live document — see importJSON's wholesale-reset behavior).
//
// Deliberately read-only against the picked file: it's JSON.parse'd into a
// local variable and never touched again. Nothing in state changes until
// the user hits "Importer" on the checked items.
(function () {
  function tauriOk() { return typeof window.__TAURI__ !== 'undefined'; }

  var modalEl = null;
  var _foreignData = null; // parsed JSON of the picked file, read-only
  var _foreignName = '';

  function ensureModal() {
    if (modalEl) return modalEl;
    modalEl = document.createElement('div');
    modalEl.className = 'modal-overlay';
    modalEl.style.display = 'none';
    modalEl.innerHTML =
      '<div class="modal-box wide" style="max-height:88vh;display:flex;flex-direction:column">' +
      '<div class="modal-hdr"><span id="tp-title" data-i18n="transplantTitle">Transplanter depuis un projet</span><button class="modal-x" id="tp-close">&times;</button></div>' +
      '<div class="modal-bdy" style="display:flex;flex-direction:column;gap:8px;overflow-y:auto">' +
      '<div id="tp-empty" style="font-size:11px;color:var(--text-dim)" data-i18n="transplantEmptyHint">Choisissez un fichier projet Nemo (.json) — vous pourrez ensuite cocher les calques et médias à importer, sans toucher au reste de ce projet-là.</div>' +
      '<div id="tp-open-tabs" style="display:none;flex-direction:column;gap:4px"></div>' +
      '<div id="tp-lists" style="display:none;flex-direction:column;gap:10px">' +
      '<div id="tp-layer-list" class="bp-grid asset-tree"></div>' +
      '<div id="tp-media-list" class="bp-grid asset-tree"></div>' +
      '</div>' +
      '<div id="tp-error" style="display:none;font-size:10px;color:#ff8080"></div>' +
      '</div>' +
      '<div class="pr" style="gap:6px;justify-content:flex-end;padding:10px 14px;border-top:1px solid var(--border)">' +
      '<button class="pbtn" id="tp-pick" data-i18n="transplantPickBtn">Choisir un fichier…</button>' +
      '<button class="pbtn ac" id="tp-import" style="display:none" data-i18n="transplantImportBtn">Importer la sélection</button>' +
      '</div></div>';
    document.body.appendChild(modalEl);
    modalEl.querySelector('#tp-close').addEventListener('click', close);
    modalEl.addEventListener('click', function (e) { if (e.target === modalEl) close(); });
    modalEl.querySelector('#tp-pick').addEventListener('click', pickFile);
    modalEl.querySelector('#tp-import').addEventListener('click', importChecked);
    document.getElementById('transplant-file-input').addEventListener('change', function (e) {
      var f = e.target.files && e.target.files[0];
      e.target.value = '';
      if (!f) return;
      var reader = new FileReader();
      reader.onload = function () { loadForeign(reader.result, f.name); };
      reader.readAsText(f);
    });
    return modalEl;
  }

  // Open project tabs as a pickable source (feedback #109: "voir apparaître
  // les différents projet test1, test2... afin de pouvoir les glisser comme
  // un componant dans un autre projet") — same loadForeign() the file-picker
  // path already uses, just fed a tab's in-memory JSON snapshot directly
  // instead of a file read. Rendered fresh on every open() since tabs can be
  // added/closed/renamed between transplant modal openings.
  function renderOpenTabsList() {
    var wrap = modalEl.querySelector('#tp-open-tabs');
    wrap.innerHTML = '';
    var openTabs = (window.SMProject && window.SMProject.getOpenTabs) ? window.SMProject.getOpenTabs() : [];
    if (!openTabs.length) { wrap.style.display = 'none'; return; }
    wrap.style.display = 'flex';
    var label = document.createElement('div');
    label.style.cssText = 'font-size:9px;color:var(--text-dim);text-transform:uppercase;letter-spacing:.03em;';
    label.textContent = SM && SM.t ? SM.t('transplantOpenTabsLabel') : 'Onglets de projet ouverts';
    wrap.appendChild(label);
    openTabs.forEach(function (t) {
      var row = document.createElement('button');
      row.className = 'pbtn';
      row.style.cssText = 'text-align:left;justify-content:flex-start;';
      row.textContent = t.name || 'Untitled';
      row.addEventListener('click', function () { loadForeign(t.json, t.name); });
      wrap.appendChild(row);
    });
  }
  function open() { ensureModal().style.display = 'flex'; renderOpenTabsList(); }
  function close() { if (modalEl) modalEl.style.display = 'none'; }
  function showError(msg) {
    var el = modalEl.querySelector('#tp-error');
    el.textContent = msg; el.style.display = msg ? '' : 'none';
  }

  async function pickFile() {
    showError('');
    if (!tauriOk()) { document.getElementById('transplant-file-input').click(); return; }
    try {
      var path = await window.__TAURI__.dialog.open({
        title: 'Choisir un projet Nemo', multiple: false,
        filters: [{ name: 'Nemo Project', extensions: ['json'] }],
      });
      if (!path) return;
      path = Array.isArray(path) ? path[0] : path;
      var json = await window.__TAURI__.fs.readTextFile(path);
      loadForeign(json, path.split(/[\\/]/).pop());
    } catch (e) {
      showError('Impossible de lire ce fichier: ' + e);
    }
  }

  function loadForeign(jsonText, name) {
    showError('');
    var d;
    try { d = JSON.parse(jsonText); }
    catch (e) { showError('Fichier JSON invalide.'); return; }
    if (!d || !Array.isArray(d.layers)) { showError("Ce fichier ne ressemble pas à un projet Nemo (pas de 'layers')."); return; }
    _foreignData = d;
    _foreignName = name;
    renderLists();
  }

  function renderLists() {
    var layerList = modalEl.querySelector('#tp-layer-list');
    var mediaList = modalEl.querySelector('#tp-media-list');
    layerList.innerHTML = ''; mediaList.innerHTML = '';
    modalEl.querySelector('#tp-empty').style.display = 'none';
    modalEl.querySelector('#tp-lists').style.display = 'flex';
    modalEl.querySelector('#tp-title').textContent = 'Transplanter — ' + _foreignName;
    modalEl.querySelector('#tp-import').style.display = '';

    // Real hierarchy pass (2026-08, feedback: "on est pas encore sur une
    // vrai hierarchie avec label, folder..., ou composition" — AEP
    // Transplant reference groups PRECOMPS separately from plain ASSETS).
    // A layer instantiating a symbol (ld.symbolId set) is the closest Nemo
    // equivalent of a "precomp" — grouped into its own checkable folder,
    // same automatic-by-kind pattern asset-tree.js already uses for the
    // docked Media tab (shared FOLDER_COLORS so "Composants" reads as the
    // same category in both places).
    var checkRow = function (label, dataAttr, idx) {
      var row = document.createElement('label');
      row.className = 'bp-item';
      var cb = document.createElement('input');
      cb.type = 'checkbox'; cb.dataset[dataAttr] = idx;
      var span = document.createElement('span');
      span.textContent = label;
      row.appendChild(cb); row.appendChild(span);
      return row;
    };
    var layers = _foreignData.layers || [];
    var compLayers = [], plainLayers = [];
    layers.forEach(function (ld, idx) { (ld.symbolId ? compLayers : plainLayers).push({ ld: ld, idx: idx }); });
    if (window.SMAssetTree) {
      if (compLayers.length) {
        var compBody = SMAssetTree.folderGroup(layerList, { label: SMAssetTree.componentsLabel(), color: SMAssetTree.FOLDER_COLORS.components, count: compLayers.length });
        compLayers.forEach(function (e) {
          var frameCount = (e.ld.frames || []).length;
          compBody.appendChild(checkRow((e.ld.name || ('Layer ' + (e.idx + 1))) + ' · ' + frameCount + ' frames', 'layerIdx', e.idx));
        });
      }
      if (plainLayers.length) {
        var plainBody = SMAssetTree.folderGroup(layerList, { label: SMAssetTree.layersLabel(), color: 'var(--text-dim)', count: plainLayers.length });
        plainLayers.forEach(function (e) {
          var frameCount = (e.ld.frames || []).length;
          plainBody.appendChild(checkRow((e.ld.name || ('Layer ' + (e.idx + 1))) + ' · ' + frameCount + ' frames', 'layerIdx', e.idx));
        });
      }
    }
    if (!layers.length) {
      var none = document.createElement('div');
      none.className = 'asset-folder-empty-hint';
      none.textContent = 'Aucun calque dans ce fichier.';
      layerList.appendChild(none);
    }

    var media = _foreignData.mediaLibrary || [];
    if (window.SMAssetTree) {
      ['image', 'video', 'audio'].forEach(function (kind) {
        var entries = []; media.forEach(function (m, idx) { if (m.kind === kind) entries.push({ m: m, idx: idx }); });
        if (!entries.length) return;
        var body = SMAssetTree.folderGroup(mediaList, { label: SMAssetTree.KIND_GROUP_LABEL[kind], color: SMAssetTree.FOLDER_COLORS[kind], count: entries.length });
        entries.forEach(function (e) {
          body.appendChild(checkRow((e.m.name || ('Media ' + (e.idx + 1))) + (e.m.linked ? ' (fichier lié)' : ''), 'mediaIdx', e.idx));
        });
      });
    }
    if (!media.length) {
      var none2 = document.createElement('div');
      none2.className = 'asset-folder-empty-hint';
      none2.textContent = 'Aucun média catalogué dans ce fichier.';
      mediaList.appendChild(none2);
    }
  }

  function newLayerUid() { return 'ly_' + Date.now().toString(36) + '_' + Math.floor(Math.random() * 1e6); }

  function normalizeFrameCount(frames, total) {
    frames = frames || [];
    if (frames.length === total) return frames;
    if (frames.length > total) return frames.slice(0, total);
    var out = frames.slice();
    while (out.length < total) out.push({ strokes: [], isKeyframe: false, isInterpolated: false });
    return out;
  }

  function importChecked() {
    if (!_foreignData) return;
    var layerChecks = Array.prototype.slice.call(modalEl.querySelectorAll('#tp-layer-list input[type=checkbox]:checked'));
    var mediaChecks = Array.prototype.slice.call(modalEl.querySelectorAll('#tp-media-list input[type=checkbox]:checked'));
    if (!layerChecks.length && !mediaChecks.length) { showError('Cochez au moins un calque ou un média à importer.'); return; }

    saveAllLayerFrames();
    pushUndoLayers(true);

    // uid remap table for every transplanted layer, built up-front so
    // parent/matte/timeLink references between two transplanted layers can
    // be rewritten to the fresh uids (a reference to a layer NOT in this
    // batch is dropped rather than left dangling at a foreign uid).
    var uidMap = {}; // old layerUid -> new layerUid
    var srcLayers = layerChecks.map(function (cb) { return _foreignData.layers[+cb.dataset.layerIdx]; });
    srcLayers.forEach(function (srcLd) {
      if (srcLd.layerUid) uidMap[srcLd.layerUid] = newLayerUid();
    });

    var importedCount = 0;
    srcLayers.forEach(function (srcLd) {
      var clone = JSON.parse(JSON.stringify(srcLd));
      var idx = createUserLayer(clone.name || 'Layer');
      var nl = state.layers[idx];
      // The source project's own totalFrames can differ from this one's —
      // every other layer's `frames` array is exactly state.totalFrames
      // long (renderTimeline/loadFrame index it directly by frame number,
      // no bounds check), so a shorter/longer transplanted array must be
      // padded/truncated to match, not copied as-is.
      nl.frames = normalizeFrameCount(clone.frames || nl.frames, state.totalFrames);
      nl.color = clone.color || nl.color;
      if (clone.blendMode) nl.blendMode = clone.blendMode;
      if (clone.motion) nl.motion = clone.motion;
      if (clone.motionStatic) nl.motionStatic = clone.motionStatic;
      if (clone.elementMotion) nl.elementMotion = clone.elementMotion;
      if (clone.effects) nl.effects = clone.effects;
      if (clone.rig) nl.rig = clone.rig;
      if (clone.shapeNames) nl.shapeNames = clone.shapeNames;
      if (clone.groups) nl.groups = clone.groups;
      if (clone.isTextLayer) nl.isTextLayer = clone.isTextLayer;
      if (clone.isNullLayer) { nl.isNullLayer = true; nl.nullPos = clone.nullPos; nl.nullShape = clone.nullShape; }
      // Widget (rig control) layer (2026-08-30) — carried, together with
      // its exprControls, because those two are one object: ld.widget's
      // axes point at xc_… keys that only exist as declarations inside
      // exprControls, so bringing one without the other yields a widget
      // wired to nothing and a Motion row that cannot render.
      //
      // What is deliberately NOT carried is the DRIVEN side: transplant has
      // never copied `expressions` at all (pre-existing, unrelated to this
      // feature), and it remaps every layerUid on import anyway — so an
      // expression naming the source project's uid could not resolve here
      // even if it were copied. A transplanted widget therefore arrives
      // live and re-wireable rather than silently half-connected.
      if (clone.isWidgetLayer && clone.widget && typeof clone.widget === 'object' && clone.widget.x) {
        nl.isWidgetLayer = true;
        nl.widget = clone.widget;
        if (Array.isArray(clone.exprControls) && clone.exprControls.length) {
          nl.exprControls = clone.exprControls;
          if (window.SMMotion && SMMotion.registerControlPropMeta) nl.exprControls.forEach(function (c) { SMMotion.registerControlPropMeta(c); });
        }
      }
      if (clone.threeD) nl.threeD = clone.threeD;
      if (clone.timeRemap) nl.timeRemap = clone.timeRemap;
      if (clone.motionBlur) nl.motionBlur = clone.motionBlur;
      if (clone.markers) nl.markers = clone.markers;
      if (clone.inPoint != null) nl.inPoint = clone.inPoint;
      if (clone.outPoint != null) nl.outPoint = clone.outPoint;

      // Component/Symbol linkage: bring the symbol definition along too,
      // under a freshly generated id (same collision-avoidance reasoning as
      // layerUid — the source project's sym_<ts>_<rand> id could coincide
      // with an unrelated symbol already in THIS project).
      if (clone.symbolId && _foreignData.symbols && _foreignData.symbols[clone.symbolId]) {
        if (!state.symbols) state.symbols = {};
        var newSymId = 'sym_' + Date.now() + '_' + Math.floor(Math.random() * 10000);
        state.symbols[newSymId] = JSON.parse(JSON.stringify(_foreignData.symbols[clone.symbolId]));
        nl.symbolId = newSymId;
        nl.symPlayMode = clone.symPlayMode; nl.symSpeed = clone.symSpeed;
        nl.symPlacedAt = clone.symPlacedAt; nl.symSingleFrame = clone.symSingleFrame;
        nl.symMatrix = clone.symMatrix;
      }

      nl.layerUid = uidMap[srcLd.layerUid] || newLayerUid();
      ['parentLayerUid', 'parentLayerUidB', 'matteSourceLayerUid'].forEach(function (f) {
        nl[f] = clone[f] && uidMap[clone[f]] ? uidMap[clone[f]] : null;
      });
      if (clone.timeLink && clone.timeLink.uid) {
        nl.timeLink = uidMap[clone.timeLink.uid] ? Object.assign({}, clone.timeLink, { uid: uidMap[clone.timeLink.uid] }) : null;
      }
      importedCount++;
    });

    var importedMedia = 0;
    mediaChecks.forEach(function (cb) {
      var m = JSON.parse(JSON.stringify(_foreignData.mediaLibrary[+cb.dataset.mediaIdx]));
      // layerUid pointed at a layer in the FOREIGN project — meaningless
      // here even if one of the layers above was also imported (media
      // entries aren't matched by layerUid across files, only by the
      // resolve-by-name fallback media-library.js already has). Null it so
      // the picker's own uid-first-then-name-fallback resolve degrades
      // cleanly instead of coincidentally hitting an unrelated local layer.
      m.layerUid = null;
      m.id = 'md_' + Date.now().toString(36) + '_' + Math.floor(Math.random() * 1e6);
      if (!state.mediaLibrary) state.mediaLibrary = [];
      state.mediaLibrary.push(m);
      importedMedia++;
    });

    loadFrame(state.currentFrame);
    if (window.renderOS) renderOS();
    if (window.renderArcs) renderArcs();
    updateUI();
    if (window.SMMediaLibrary) SMMediaLibrary.reload();
    if (window.showToast) showToast('Transplanté : ' + importedCount + ' calque(s), ' + importedMedia + ' média(s)');
    close();
  }

  document.getElementById('btn-transplant-open').addEventListener('click', function () {
    _foreignData = null; _foreignName = '';
    ensureModal();
    modalEl.querySelector('#tp-empty').style.display = '';
    modalEl.querySelector('#tp-lists').style.display = 'none';
    modalEl.querySelector('#tp-import').style.display = 'none';
    modalEl.querySelector('#tp-title').textContent = 'Transplanter depuis un projet';
    showError('');
    open();
  });
})();
