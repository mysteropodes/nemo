// ---- Motion animation presets (2026-08) ----
// Feedback: "des onglets afin d'enregistrer des presets comme pour
// MotionStack" — save the ACTIVE layer's whole Motion animation (position/
// anchor/rotation/scale/opacity keyframes, per motion.js's data model:
// ld.motion/ld.motionStatic) as a named, reusable preset, organized into
// user-defined category tabs, and re-apply one to any other layer.
//
// Deliberately a GLOBAL library, NOT project-embedded data (unlike
// state.customBrushPresets, which resets per-project — see brush-editor.js).
// MotionStack itself is a cross-project preset library: the entire point is
// building up a personal stack of animation moves once and reusing them in
// every future project, so this is backed by localStorage
// ('nemo-motion-presets'/'nemo-motion-preset-categories', same
// 'nemo-<key>'+try/catch convention as i18n.js/motion.js's other persisted
// UI prefs) and never touched by New Project / project load.
(function () {
  var LS_PRESETS = 'nemo-motion-presets';
  var LS_CATEGORIES = 'nemo-motion-preset-categories';
  var LS_ACTIVE_CAT = 'nemo-motion-preset-active-cat';

  var _presets = {};
  var _categories = [{ id: 'default', name: 'Général' }];
  var _activeCatIdx = 0;

  function load() {
    try { _presets = JSON.parse(localStorage.getItem(LS_PRESETS) || '{}') || {}; } catch (e) { _presets = {}; }
    try {
      var cats = JSON.parse(localStorage.getItem(LS_CATEGORIES) || 'null');
      if (cats && cats.length) _categories = cats;
    } catch (e) {}
    try { _activeCatIdx = parseInt(localStorage.getItem(LS_ACTIVE_CAT) || '0', 10) || 0; } catch (e) {}
    if (_activeCatIdx >= _categories.length) _activeCatIdx = 0;
  }
  function persistPresets() { try { localStorage.setItem(LS_PRESETS, JSON.stringify(_presets)); } catch (e) {} }
  function persistCategories() { try { localStorage.setItem(LS_CATEGORIES, JSON.stringify(_categories)); } catch (e) {} }
  function persistActiveCat() { try { localStorage.setItem(LS_ACTIVE_CAT, String(_activeCatIdx)); } catch (e) {} }

  function activeCategory() { return _categories[_activeCatIdx] || _categories[0]; }
  function newId() { return 'mp-' + Date.now().toString(36) + '-' + Math.floor(Math.random() * 1e4); }

  function renderTabs() {
    var wrap = document.getElementById('motion-preset-tabs');
    if (!wrap) return;
    wrap.innerHTML = '';
    _categories.forEach(function (cat, idx) {
      var tab = document.createElement('div');
      tab.className = 'sym-tab' + (idx === _activeCatIdx ? ' act' : '');
      tab.textContent = cat.name;
      tab.title = cat.name;
      tab.addEventListener('click', function () { _activeCatIdx = idx; persistActiveCat(); render(); });
      tab.addEventListener('dblclick', function () { renameCategory(idx); });
      tab.addEventListener('contextmenu', function (e) {
        e.preventDefault();
        if (!window.showContextMenu) return;
        window.showContextMenu(e.clientX, e.clientY, [
          { label: 'Renommer…', action: function () { renameCategory(idx); } },
          { sep: true },
          { label: 'Supprimer l\'onglet', action: function () { deleteCategory(idx); }, disabled: _categories.length <= 1 },
        ]);
      });
      wrap.appendChild(tab);
    });
  }

  function renameCategory(idx) {
    var cat = _categories[idx];
    var name = prompt('Nom de l\'onglet', cat.name);
    if (name && name.trim()) { cat.name = name.trim(); persistCategories(); render(); }
  }
  function deleteCategory(idx) {
    if (_categories.length <= 1) return;
    var cat = _categories[idx];
    Object.keys(_presets).forEach(function (id) { if (_presets[id].categoryId === cat.id) delete _presets[id]; });
    _categories.splice(idx, 1);
    if (_activeCatIdx >= _categories.length) _activeCatIdx = _categories.length - 1;
    persistCategories(); persistPresets(); persistActiveCat();
    render();
  }

  function renderGrid() {
    var grid = document.getElementById('motion-preset-grid');
    if (!grid) return;
    grid.innerHTML = '';
    var cat = activeCategory();
    var ids = Object.keys(_presets).filter(function (id) { return _presets[id].categoryId === cat.id; });
    if (!ids.length) {
      var none = document.createElement('div');
      none.style.cssText = 'font-size:10px;color:var(--text-dim);padding:4px';
      none.textContent = 'Aucun preset dans cet onglet — sélectionnez un calque animé puis "Enregistrer le calque comme preset".';
      grid.appendChild(none);
      return;
    }
    ids.forEach(function (id) {
      var p = _presets[id];
      var item = document.createElement('button');
      item.className = 'bp-item bp-item-custom';
      var icon = document.createElement('span');
      icon.className = 'bp-item-icon';
      icon.textContent = '◆'; // same filled-diamond glyph as an animated (stopwatch-on) property row
      item.appendChild(icon);
      var span = document.createElement('span');
      span.textContent = p.label || id;
      item.appendChild(span);
      var del = document.createElement('span');
      del.className = 'bp-item-del';
      del.textContent = '×';
      del.title = SM.t('hsDeletePreset');
      del.addEventListener('click', function (e) {
        e.stopPropagation();
        delete _presets[id];
        persistPresets();
        render();
      });
      item.appendChild(del);
      item.addEventListener('click', function () { applyPreset(id); });
      grid.appendChild(item);
    });
  }

  function render() { renderTabs(); renderGrid(); }
  window.renderMotionPresetPanel = render;

  function savePreset() {
    var ld = state.layers[state.activeLayerIdx];
    if (!ld) { if (window.showToast) showToast(SM.t('hsSelectLayerFirst')); return; }
    if (!ld.motion && !ld.motionStatic) { if (window.showToast) showToast(SM.t('hsLayerNoTransform')); return; }
    var name = prompt('Nom du preset', ld.name || 'Preset');
    if (!name || !name.trim()) return;
    var id = newId();
    _presets[id] = {
      label: name.trim(),
      categoryId: activeCategory().id,
      motion: ld.motion ? JSON.parse(JSON.stringify(ld.motion)) : null,
      motionStatic: ld.motionStatic ? JSON.parse(JSON.stringify(ld.motionStatic)) : null,
    };
    persistPresets();
    render();
    if (window.showToast) showToast('Preset "' + name.trim() + '" enregistré');
  }

  function applyPreset(id) {
    var p = _presets[id];
    if (!p) return;
    var ld = state.layers[state.activeLayerIdx];
    if (!ld) { if (window.showToast) showToast(SM.t('hsSelectTargetLayerFirst')); return; }
    saveAllLayerFrames();
    pushUndoLayers(true);
    ld.motion = p.motion ? JSON.parse(JSON.stringify(p.motion)) : null;
    ld.motionStatic = p.motionStatic ? JSON.parse(JSON.stringify(p.motionStatic)) : null;
    loadFrame(state.currentFrame);
    if (window.renderOS) renderOS();
    if (window.renderArcs) renderArcs();
    updateUI();
    if (window.SMMotion) SMMotion.renderMotionPropsPanel();
    if (window.showToast) showToast('Preset "' + (p.label || id) + '" appliqué');
  }

  document.getElementById('btn-mp-save').addEventListener('click', savePreset);
  document.getElementById('btn-mp-new-tab').addEventListener('click', function () {
    var name = prompt('Nom du nouvel onglet', 'Nouveau');
    if (!name || !name.trim()) return;
    _categories.push({ id: 'cat-' + Date.now().toString(36), name: name.trim() });
    _activeCatIdx = _categories.length - 1;
    persistCategories(); persistActiveCat();
    render();
  });

  load();
  render();
})();
