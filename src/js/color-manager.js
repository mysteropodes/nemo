// ---- Selected Colors panel (2026-07) ----
// "Un nouvel onglet de gestion des couleurs en fonction de ce que l'on
// sélectionne ou si on sélectionne rien on voit toutes les couleurs dans le
// projet, et ça nous permet de les changer sur des ensembles" — a color-
// management panel, always visible (#selected-colors-sec, index.html):
//   - Selection non-empty: every distinct fill/stroke color used by the
//     selected shapes.
//   - Nothing selected: every distinct color used ANYWHERE in the project —
//     scanned from state.layers[i].frames[j].strokes[k], which already
//     stores fillColor/strokeColor as plain hex STRINGS (serP(), app.js) —
//     no need to touch live Paper.js objects or walk every frame's actual
//     geometry, just the persisted data, so this stays cheap even on a
//     project with many frames.
// Editing a row's hex/opacity batch-recolors every exact match (selection-
// scoped if there IS a selection, project-wide otherwise, mirroring the
// same "current selection redefines the scope" convention every other
// color control in this app already follows — e.g. Fill/Stroke's own
// setFillColor/setStrokeColor). The crosshair icon selects the matching
// shapes on the CURRENT layer/frame — cross-frame/cross-layer selection
// isn't something this app's selection model supports anywhere else
// either (state.selectedStrokeIndices is keyed to the active layer only),
// so this doesn't invent a new capability, just scopes honestly to what
// selection already means here.
(function () {
  var ICON_CROSSHAIR = '<svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.6"><circle cx="12" cy="12" r="7"/><path d="M12 2v4M12 18v4M2 12h4M18 12h4"/></svg>';

  // Groups by normalized hex (case-insensitive), keeping the first-seen
  // casing/alpha form as the canonical value for that group — good enough
  // since colorHex8/serP already normalize to lowercase hex on write.
  function addColor(map, hex) {
    if (!hex) return;
    var key = hex.toUpperCase();
    if (!map[key]) map[key] = { hex: hex, count: 0 };
    map[key].count++;
  }

  function computeUsedColors() {
    var map = {};
    if (window.selectedPaths && selectedPaths.length) {
      selectedPaths.forEach(function (p) {
        if (p.fillColor) addColor(map, colorHex8(p.fillColor));
        if (p.strokeColor) addColor(map, colorHex8(p.strokeColor));
      });
    } else {
      // Context colors (2026-07, "si on sélectionne rien on voit toutes les
      // couleurs dans le projet" — replaced by the active layer/canvas/
      // entered-component context instead of a project-wide scan). Scoped to
      // the ACTIVE layer's CURRENT frame only, not every layer×frame in
      // state.layers — already transparently scoped to an entered
      // component's own layers too, since enterSymbol() (app.js) swaps
      // state.layers/userLayers wholesale for the duration, so this reads
      // the symbol's own content without any extra branching here.
      var ld = state.layers && state.layers[state.activeLayerIdx];
      var fr = ld && ld.frames && ld.frames[state.currentFrame];
      if (fr) {
        (fr.strokes || []).forEach(function (sd) {
          if (sd.fillColor) addColor(map, sd.fillColor);
          // serP() (app.js) writes strokeColor:'#ffffff' as a legacy
          // fallback even when the shape has NO real stroke — hasRealStroke
          // is the authoritative flag (CLAUDE.md §1's own documented
          // gotcha). Without this check, every plain filled-only shape
          // added a phantom white "stroke color" to the list.
          var hasStroke = sd.hasRealStroke !== undefined ? sd.hasRealStroke : !!sd.strokeColor;
          if (hasStroke && sd.strokeColor) addColor(map, sd.strokeColor);
        });
      }
      // Canvas background — a real, visible "color in context" even on a
      // completely empty frame, which the old project-wide scan never
      // surfaced at all (it only ever looked at stroke/fill data).
      if (window.state && state.canvasBg) addColor(map, state.canvasBg);
    }
    return Object.keys(map).map(function (k) { return map[k]; }).sort(function (a, b) { return b.count - a.count; });
  }

  // Batch recolor — same scope rule as the scan above: within the current
  // selection if one exists, project-wide (persisted data + every live
  // on-canvas layer) otherwise.
  function recolorColor(oldHex, newHex) {
    pushUndo();
    var oldNorm = oldHex.toUpperCase();
    if (window.selectedPaths && selectedPaths.length) {
      selectedPaths.forEach(function (p) {
        if (p.fillColor && colorHex8(p.fillColor).toUpperCase() === oldNorm) p.fillColor = newHex;
        if (p.strokeColor && colorHex8(p.strokeColor).toUpperCase() === oldNorm) p.strokeColor = newHex;
      });
      saveActiveLayerFrame();
    } else {
      // Scoped to the active layer only now, matching computeUsedColors'
      // own narrowed scope above (2026-07) — recoloring a row the panel
      // labels as "this layer's colors" silently touching every OTHER
      // layer in the whole project would contradict what's actually shown.
      var ld = state.layers[state.activeLayerIdx];
      (ld.frames || []).forEach(function (fr) {
        (fr.strokes || []).forEach(function (sd) {
          if (sd.fillColor && sd.fillColor.toUpperCase() === oldNorm) sd.fillColor = newHex;
          // Same hasRealStroke guard as computeUsedColors above — without
          // it, recoloring away from the fallback white would WRITE a
          // brand new visible strokeColor onto shapes that never had one
          // (hasRealStroke would still read false, but strokeColor would
          // no longer be the recognizable '#ffffff' sentinel either),
          // corrupting persisted data on next load.
          var hasStroke = sd.hasRealStroke !== undefined ? sd.hasRealStroke : !!sd.strokeColor;
          if (hasStroke && sd.strokeColor && sd.strokeColor.toUpperCase() === oldNorm) sd.strokeColor = newHex;
        });
      });
      // Persisted data alone wouldn't show up until the frame reloads —
      // also patch the currently-live active canvas layer so the change is
      // visible immediately, same reasoning as Propager la couleur's own
      // "live objects need their own pass, persisted data isn't enough".
      var liveLayer = (window.userLayers || [])[state.activeLayerIdx];
      if (liveLayer) {
        liveLayer.children.forEach(function (item) {
          if (item.fillColor && colorHex8(item.fillColor).toUpperCase() === oldNorm) item.fillColor = newHex;
          if (item.strokeColor && colorHex8(item.strokeColor).toUpperCase() === oldNorm) item.strokeColor = newHex;
        });
      }
      // Canvas background isn't a Paper.js item color — matched/updated
      // separately since computeUsedColors now surfaces it as its own row.
      // SM.setCanvasBg (not a direct state.canvasBg write) so drawStage()/
      // syncDocFields() actually repaint the canvas and the Document
      // panel's own BG swatch instead of going stale until some unrelated
      // update happens to touch them.
      if (window.state && state.canvasBg && state.canvasBg.toUpperCase() === oldNorm && window.SM && SM.setCanvasBg) SM.setCanvasBg(newHex);
    }
    updateUI();
    if (window.SMEngineBridge) SMEngineBridge.renderNow();
  }

  // Selects every shape on the ACTIVE layer's CURRENT frame whose fill OR
  // stroke matches `hex` — same 4-step "set selection" recipe every other
  // select-these-paths feature in this codebase uses (selectGhostAll,
  // timeline.js:1637; select-bridge.js's own marquee/click-select).
  function selectPathsWithColor(hex) {
    var norm = hex.toUpperCase();
    window.SM.setTool('select');
    var layer = userLayers[state.activeLayerIdx];
    if (!layer) return;
    selectedPaths = layer.children.filter(function (c) {
      if (!(c instanceof Path) || !isSelectablePathChild(c)) return false;
      var fm = c.fillColor && colorHex8(c.fillColor).toUpperCase() === norm;
      var sm = c.strokeColor && colorHex8(c.strokeColor).toUpperCase() === norm;
      return fm || sm;
    });
    state.selectedStrokeIndices = selectedPaths.map(getSI).filter(function (i) { return i >= 0; });
    if (window.renderArcs) renderArcs();
    updateUI();
    if (!selectedPaths.length && window.showToast) showToast('Aucune forme avec cette couleur sur ce calque/cette frame');
  }

  function renderSelectedColorsPanel() {
    var body = document.getElementById('selected-colors-body');
    var hdr = document.getElementById('selected-colors-hdr');
    if (!body) return;
    var hasSel = !!(window.selectedPaths && selectedPaths.length);
    if (hdr) hdr.textContent = SM.t(hasSel ? 'hdrSelectedColors' : 'hdrLayerColors');
    var list = computeUsedColors();
    body.innerHTML = '';
    if (!list.length) {
      var empty = document.createElement('div');
      empty.className = 'pr'; empty.style.cssText = 'font-size:9px;color:var(--text-dim)';
      empty.textContent = SM.t('noColorSelected');
      body.appendChild(empty);
      return;
    }
    // No silent cap on real project sizes worth flagging — most projects
    // land well under 100 distinct colors; this is a diagnostics/recolor
    // tool, not a performance-critical hot path, so no truncation here.
    list.forEach(function (entry) {
      var row = document.createElement('div'); row.className = 'pr color-row';
      var sw = document.createElement('div'); sw.className = 'cw-mini'; sw.style.background = entry.hex; sw.style.flexShrink = '0';
      var hexInput = document.createElement('input');
      hexInput.type = 'text'; hexInput.className = 'pi color-hex-input'; hexInput.spellcheck = false; hexInput.maxLength = 9;
      hexInput.value = hexDisplayValue(entry.hex);
      var opacityInput = document.createElement('input');
      opacityInput.type = 'number'; opacityInput.className = 'pi scrub color-opacity-input'; opacityInput.min = 0; opacityInput.max = 100; opacityInput.dataset.step = '1';
      opacityInput.value = alphaPctFromHex(entry.hex);
      var pct = document.createElement('span'); pct.className = 'color-pct'; pct.textContent = '%';
      var selBtn = document.createElement('div'); selBtn.className = 'lico color-select-btn'; selBtn.title = 'Sélectionner les formes de cette couleur (calque/frame actifs)';
      selBtn.innerHTML = ICON_CROSSHAIR;

      hexInput.addEventListener('change', function () {
        var parsed = window.parseHexInput ? parseHexInput(this.value) : null;
        if (!parsed) { this.value = hexDisplayValue(entry.hex); return; }
        recolorColor(entry.hex, parsed);
      });
      hexInput.addEventListener('keydown', function (e) { if (e.key === 'Enter') this.blur(); });
      opacityInput.addEventListener('input', function () {
        var p = Math.max(0, Math.min(100, parseInt(this.value) || 0));
        var rgb = entry.hex.replace('#', '').slice(0, 6);
        var a = Math.round(p / 100 * 255).toString(16).padStart(2, '0');
        recolorColor(entry.hex, '#' + rgb + (p < 100 ? a : ''));
      });
      selBtn.addEventListener('click', function () { selectPathsWithColor(entry.hex); });

      row.appendChild(sw); row.appendChild(hexInput); row.appendChild(opacityInput); row.appendChild(pct); row.appendChild(selBtn);
      body.appendChild(row);
    });
  }

  // Cheap early-return so this doesn't cost anything while the section is
  // collapsed (the common case) — only the header's own click handler
  // below forces a render right when it actually expands.
  function updateSelectedColorsPanelIfVisible() {
    var body = document.getElementById('selected-colors-body');
    if (!body || body.classList.contains('hid')) return;
    renderSelectedColorsPanel();
  }
  window.updateSelectedColorsPanel = updateSelectedColorsPanelIfVisible;

  // Auto-expand on selection (2026-07, "auto-expand on selection") — a
  // fresh 0→non-empty transition of selectedPaths force-opens the section
  // exactly once, reusing the same .closed/.hid toggle updatePropsContext
  // (timeline.js) already uses to force-open OTHER sections on a context
  // change. Doesn't re-collapse it when the selection empties again (a
  // user who explicitly collapsed it back shouldn't have it keep popping
  // open every single click) — tracked via _hadSel so this only fires on
  // the actual empty→non-empty edge, not every updateUI() tick.
  var _hadSel = false;
  function autoExpandOnSelection() {
    var hasSelNow = !!(window.selectedPaths && selectedPaths.length);
    if (hasSelNow && !_hadSel) {
      var sec = document.getElementById('selected-colors-sec');
      if (sec) {
        var h = sec.querySelector('.phdr'), b = sec.querySelector('.pbdy');
        if (h && b) { h.classList.remove('closed'); b.classList.remove('hid'); }
      }
    }
    _hadSel = hasSelNow;
  }

  function init() {
    var sec = document.getElementById('selected-colors-sec');
    if (!sec) return;
    var hdr = sec.querySelector('.phdr');
    // ui.js's own generic .phdr delegated listener toggles the collapsed
    // class first (it's bound earlier, at DOMContentLoaded, so it always
    // runs before this one) — this one just renders once expansion lands,
    // since the generic toggle has no idea this section needs a render.
    if (hdr) hdr.addEventListener('click', function () { renderSelectedColorsPanel(); });
    // Hooks into the app's one central "everything may have changed"
    // choke point (selection, edits, frame navigation all funnel through
    // updateUI already) rather than threading a call into every individual
    // selection/edit code path — same reasoning as this file's own cheap
    // visibility gate: near-zero cost when the section is collapsed.
    var origUpdateUI = window.updateUI;
    if (typeof origUpdateUI === 'function') {
      window.updateUI = function () {
        origUpdateUI.apply(this, arguments);
        autoExpandOnSelection();
        // Must run AFTER autoExpandOnSelection (which may just have
        // removed .hid this same tick) so a fresh selection's colors
        // render immediately instead of waiting for the NEXT updateUI().
        updateSelectedColorsPanelIfVisible();
      };
    }
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', init);
  else init();
})();
