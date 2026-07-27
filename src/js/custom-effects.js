// ---- Custom WGSL effects (2026-07) ----
// Feedback: "la possibilité d'ajouter ses propres effets wgsl et leur
// paramètre correspondant qui s'afficheront" — lets an author write their
// own fragment-shader BODY (not a whole document — engine.rs's
// register_custom_effect wraps it with the standard fullscreen-triangle
// vertex shader + texture/sampler/Params bindings every built-in effect
// already uses) and define up to 4 named parameters that show up in the
// Effects stack UI exactly like a built-in effect's own params.
//
// Data model: state.customEffects = [{id, name, source, params:[{key,
// label, min, max, step, scale, unit}, ...up to 4]}, ...] — project-wide
// (like state.symbols/palettes), not per-layer: one definition can be
// applied to any number of layers' effects stacks by referencing
// "custom:<id>" as the effect type (see effects-panel.js's
// isCustomEffect/customEffectDef helpers).
(function () {
  function ensureList() { if (!state.customEffects) state.customEffects = []; return state.customEffects; }
  function customEffectDef(id) { return ensureList().find(function (c) { return c.id === id; }) || null; }
  window.customEffectDef = customEffectDef; // effects-panel.js reads this directly

  // Re-sends every saved custom effect's source to the engine — needed
  // whenever the wasm engine is a FRESH instance (new session, project
  // load, "Resume Last Session") since register_custom_effect's compiled
  // pipeline lives only in that instance's memory, not in project data.
  function registerAllCustomEffects() {
    (window.SMSHADER_EFFECTS || []).forEach(function (c) {
      if (window.SMEngineBridge) {
        try { window.SMEngineBridge.registerCustomEffect('custom:' + c.id, c.source); }
        catch (e) { console.warn('[shader-effects-library] failed to register ' + c.id, e); }
      }
    });
    ensureList().forEach(function (c) {
      if (window.SMEngineBridge) {
        try { window.SMEngineBridge.registerCustomEffect('custom:' + c.id, c.source); }
        catch (e) { console.warn('[custom-effects] failed to register ' + c.id, e); }
      }
    });
  }
  window.registerAllCustomEffects = registerAllCustomEffects;

  function newId() { return 'ce' + Date.now().toString(36) + Math.random().toString(36).slice(2, 6); }

  var DEFAULT_SOURCE = [
    '// Available: uv (vec2<f32>, 0..1), src (vec4<f32>, already sampled',
    '// at uv), texel (vec2<f32>, 1 texel in UV units), params.p1..params.p4',
    '// (this effect\'s own parameters, edited below). Must end in a',
    '// `return vec4<f32>(...)`.',
    'let amt = params.p1;',
    'return vec4<f32>(src.rgb * (1.0 - amt) + vec3<f32>(1.0, 0.0, 1.0) * amt, src.a);',
  ].join('\n');

  var modalEl = null;
  function ensureModal() {
    if (modalEl) return modalEl;
    modalEl = document.createElement('div');
    modalEl.className = 'modal-overlay';
    modalEl.style.display = 'none';
    modalEl.innerHTML =
      '<div class="modal-box wide" style="max-height:88vh;display:flex;flex-direction:column">' +
      '<div class="modal-hdr"><span>Custom WGSL Effect</span><button class="modal-x" id="ce-close">&times;</button></div>' +
      '<div class="modal-bdy" style="display:flex;flex-direction:column;gap:10px;overflow-y:auto">' +
      '<label style="font-size:11px;color:var(--text-dim)">Name<br><input type="text" id="ce-name" class="pi" style="width:100%" placeholder="My Effect"></label>' +
      '<label style="font-size:11px;color:var(--text-dim)">Fragment shader body (WGSL)<br>' +
      '<textarea id="ce-source" spellcheck="false" style="width:100%;height:160px;font-family:monospace;font-size:11px;background:var(--panel3);color:var(--text);border:1px solid var(--border2);border-radius:6px;padding:8px;resize:vertical"></textarea></label>' +
      '<div style="font-size:10px;color:var(--text-dim)">Params (up to 4) — shown in the Effects stack, mapped to params.p1..p4 in order.</div>' +
      '<div id="ce-params"></div>' +
      '<div class="pr"><button class="pbtn" id="ce-add-param">+ Add param</button></div>' +
      '<div id="ce-error" style="display:none;font-size:10px;color:#ff8080"></div>' +
      '<div class="pr" style="gap:6px;justify-content:flex-end">' +
      '<button class="pbtn" id="ce-cancel">Cancel</button>' +
      '<button class="pbtn ac" id="ce-save">Save</button>' +
      '</div>' +
      '</div></div>';
    document.body.appendChild(modalEl);
    modalEl.querySelector('#ce-close').addEventListener('click', close);
    modalEl.querySelector('#ce-cancel').addEventListener('click', close);
    modalEl.addEventListener('click', function (e) { if (e.target === modalEl) close(); });
    modalEl.querySelector('#ce-add-param').addEventListener('click', function () {
      if (_editingParams.length >= 4) return;
      _editingParams.push({ key: 'p' + (_editingParams.length + 1), label: 'Param ' + (_editingParams.length + 1), min: 0, max: 1, step: 0.01, scale: 1, unit: '' });
      renderParamRows();
    });
    modalEl.querySelector('#ce-save').addEventListener('click', save);
    return modalEl;
  }
  function close() { if (modalEl) modalEl.style.display = 'none'; }

  var _editingId = null;
  var _editingParams = [];
  function renderParamRows() {
    var host = modalEl.querySelector('#ce-params');
    host.innerHTML = '';
    _editingParams.forEach(function (p, i) {
      var row = document.createElement('div');
      row.style.cssText = 'display:flex;gap:4px;align-items:center;margin-bottom:4px';
      row.innerHTML =
        '<input type="text" class="pi" data-f="label" style="width:90px" placeholder="Label">' +
        '<input type="number" class="pi" data-f="min" style="width:56px" placeholder="min">' +
        '<input type="number" class="pi" data-f="max" style="width:56px" placeholder="max">' +
        '<input type="number" class="pi" data-f="step" style="width:56px" placeholder="step" step="any">' +
        '<input type="text" class="pi" data-f="unit" style="width:40px" placeholder="unit">' +
        '<div class="lico" data-rm style="cursor:pointer;width:18px;text-align:center">×</div>';
      row.querySelector('[data-f="label"]').value = p.label;
      row.querySelector('[data-f="min"]').value = p.min;
      row.querySelector('[data-f="max"]').value = p.max;
      row.querySelector('[data-f="step"]').value = p.step;
      row.querySelector('[data-f="unit"]').value = p.unit;
      ['label', 'min', 'max', 'step', 'unit'].forEach(function (f) {
        row.querySelector('[data-f="' + f + '"]').addEventListener('input', function () {
          p[f] = (f === 'label' || f === 'unit') ? this.value : (parseFloat(this.value) || 0);
        });
      });
      row.querySelector('[data-rm]').addEventListener('click', function () { _editingParams.splice(i, 1); renderParamRows(); });
      host.appendChild(row);
    });
  }

  function save() {
    var name = modalEl.querySelector('#ce-name').value.trim() || 'Custom Effect';
    var source = modalEl.querySelector('#ce-source').value;
    var errEl = modalEl.querySelector('#ce-error');
    if (!/return\s+vec4/.test(source)) {
      errEl.style.display = '';
      errEl.textContent = 'The shader body must end with a "return vec4<f32>(...)" statement.';
      return;
    }
    errEl.style.display = 'none';
    var list = ensureList();
    var id = _editingId || newId();
    var def = { id: id, name: name, source: source, params: _editingParams.map(function (p, i) { return Object.assign({ key: 'p' + (i + 1) }, p); }) };
    var idx = list.findIndex(function (c) { return c.id === id; });
    if (idx >= 0) list[idx] = def; else list.push(def);
    if (window.SMEngineBridge) window.SMEngineBridge.registerCustomEffect('custom:' + id, source);
    close();
    if (window.updateEffectsPanel) window.updateEffectsPanel();
    // If this is a brand-new effect (not editing an existing applied one),
    // add it to the CURRENT layer's stack immediately — matches every
    // other "+ Add Effect" menu item's own instant-apply behavior, so
    // authoring a shader doesn't require a second trip to the add-menu.
    if (!_editingId && window.addEffectToActiveLayer) window.addEffectToActiveLayer('custom:' + id);
  }

  // Opens the editor. Pass an existing def (from customEffectDef) to edit
  // it in place — re-Saving keeps the same id, so every layer already
  // referencing "custom:<id>" picks up the change immediately (a shader
  // definition is looked up by id at render time, never copied).
  function openCustomEffectEditor(existingDef) {
    ensureModal();
    _editingId = existingDef ? existingDef.id : null;
    _editingParams = existingDef ? existingDef.params.map(function (p) { return Object.assign({}, p); }) : [{ key: 'p1', label: 'Amount', min: 0, max: 1, step: 0.01, scale: 1, unit: '' }];
    modalEl.querySelector('#ce-name').value = existingDef ? existingDef.name : '';
    modalEl.querySelector('#ce-source').value = existingDef ? existingDef.source : DEFAULT_SOURCE;
    modalEl.querySelector('#ce-error').style.display = 'none';
    renderParamRows();
    modalEl.style.display = 'flex';
  }
  window.openCustomEffectEditor = openCustomEffectEditor;
})();
