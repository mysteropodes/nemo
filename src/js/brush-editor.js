// Procedural brush editor — a Sketchbook-style "Nib" panel: live sliders
// for every buildBrushDabs() parameter (tools.js) with a real-time preview
// (same dab-stamping code the actual brush uses, see brush-preset-picker.js's
// own comment on why the preview must run the real engine, not a lookalike).
// Editing a BUILT-IN preset always forks into a new custom preset — built-ins
// are read-only reference points, not editable in place, so they can't be
// silently changed out from under a project that already uses them by name.
(function () {
  var popover = null, closeHandlers = null;
  var demoPath = null;
  var FIELDS = [
    { key: 'nibSize', label: 'Taille du nib', min: 0.2, max: 4, step: 0.05 },
    { key: 'roundness', label: 'Rondeur', min: 0.1, max: 1, step: 0.05 },
    { key: 'spacing', label: 'Espacement', min: 0.05, max: 1.5, step: 0.05 },
    { key: 'spaceJitter', label: 'Var. espacement', min: 0, max: 1, step: 0.05 },
    { key: 'rotationJitter', label: 'Var. rotation (°)', min: 0, max: 180, step: 5 },
    { key: 'sizeJitter', label: 'Var. taille', min: 0, max: 1, step: 0.05 },
    { key: 'opacity', label: 'Opacité', min: 0.05, max: 1, step: 0.05 },
    { key: 'opacityJitter', label: 'Var. opacité', min: 0, max: 1, step: 0.05 },
    { key: 'scatter', label: 'Dispersion', min: 0, max: 1, step: 0.05 },
    { key: 'dashGap', label: 'Trous (bord cassé)', min: 0, max: 0.6, step: 0.02 },
    { key: 'edgeNoise', label: 'Bord irrégulier', min: 0, max: 0.4, step: 0.02 },
    { key: 'polySides', label: 'Côtés (polygone)', min: 3, max: 10, step: 1 },
    { key: 'bristleCount', label: 'Nb. de poils (bristle)', min: 2, max: 12, step: 1 },
    // Scribble-fill (tipShape:'scribble') — a woven patch of short,
    // independently-angled hatching marks per stamp position instead of one
    // blob, the graphite/charcoal "scribbled shading" look. Only meaningful
    // for that tip shape, same as polySides/bristleCount above are only
    // meaningful for theirs — shown always, like those, for consistency.
    { key: 'scribbleCount', label: 'Nb. de traits (scribble)', min: 2, max: 20, step: 1 },
    { key: 'scribbleLen', label: 'Longueur trait (scribble)', min: 0.4, max: 3, step: 0.1 },
    { key: 'scribbleLenJitter', label: 'Var. longueur (scribble)', min: 0, max: 1, step: 0.05 },
    { key: 'scribbleWidth', label: 'Épaisseur trait (scribble)', min: 0.02, max: 0.4, step: 0.01 },
    { key: 'scribbleSpread', label: 'Étalement patch (scribble)', min: 0.1, max: 2, step: 0.05 },
    { key: 'scribbleAngleSpread', label: 'Var. angle ° (scribble)', min: 0, max: 180, step: 5 },
  ];
  var TIP_SHAPES = [
    { value: 'ellipse', label: 'Ellipse (rond déformé)' },
    { value: 'rect', label: 'Plat / biseau (marqueur)' },
    { value: 'polygon', label: 'Polygone (chip anguleux)' },
    { value: 'splatter', label: 'Éclaboussure' },
    { value: 'bristle', label: 'Poils (dry-brush)' },
    { value: 'scribble', label: 'Gribouillis (graphite/fusain)' },
    { value: 'custom', label: 'Personnalisé (dessiné)…' },
  ];
  var DEFAULT_PARAMS = { nibSize: 1, roundness: 0.9, spacing: 0.4, spaceJitter: 0.2, rotationMode: 'tangent', rotationJitter: 20, sizeJitter: 0.2, opacity: 0.6, opacityJitter: 0.2, scatter: 0.15, dashGap: 0, tipShape: 'ellipse', edgeNoise: 0, polySides: 5, bristleCount: 5, tipCorner: 0.15, scribbleCount: 8, scribbleLen: 1.4, scribbleLenJitter: 0.4, scribbleWidth: 0.12, scribbleSpread: 0.6, scribbleAngleSpread: 70 };

  function closePopover() {
    if (!popover) return;
    popover.remove();
    popover = null;
    if (closeHandlers) { closeHandlers(); closeHandlers = null; }
  }

  function getDemoPath(w, h) {
    if (demoPath) return demoPath;
    demoPath = new Path({ insert: false });
    var n = 40;
    for (var i = 0; i <= n; i++) {
      var t = i / n;
      demoPath.add(new Point(10 + t * (w - 20), h / 2 + Math.sin(t * Math.PI * 2.6) * (h * 0.28)));
    }
    demoPath.smooth();
    return demoPath;
  }
  function traceOnCanvas2D(ctx, path) {
    var segs = path.segments;
    if (!segs.length) return;
    ctx.beginPath();
    ctx.moveTo(segs[0].point.x, segs[0].point.y);
    var n = segs.length;
    for (var i = 1; i <= n; i++) {
      var prev = segs[i - 1], cur = segs[i % n];
      var c1 = prev.point.add(prev.handleOut), c2 = cur.point.add(cur.handleIn);
      ctx.bezierCurveTo(c1.x, c1.y, c2.x, c2.y, cur.point.x, cur.point.y);
      if (i === n && !path.closed) break;
    }
    if (path.closed) ctx.closePath();
  }

  function open(startFromKey, onApply) {
    closePopover();
    var starting = startFromKey ? window.resolveBrushPreset(startFromKey) : null;
    var editingCustomId = (startFromKey && state.customBrushPresets && state.customBrushPresets[startFromKey]) ? startFromKey : null;
    var params = Object.assign({}, DEFAULT_PARAMS, starting || {});
    var nameSeed = editingCustomId ? (state.customBrushPresets[editingCustomId].label || 'Mon brush') : (startFromKey && window.BrushPresetPicker ? window.BrushPresetPicker.labelFor(startFromKey) + ' copie' : 'Mon brush');

    var el = document.createElement('div');
    el.className = 'ctx-menu bp-editor-pop';
    var slidersHtml = FIELDS.map(function (f) {
      return '<div class="bpe-row"><span class="bpe-lbl">' + f.label + '</span>' +
        '<input type="range" class="bpe-slider" data-key="' + f.key + '" min="' + f.min + '" max="' + f.max + '" step="' + f.step + '">' +
        '<span class="bpe-val" data-valfor="' + f.key + '"></span></div>';
    }).join('');
    var tipShapeOptionsHtml = TIP_SHAPES.map(function (s) { return '<option value="' + s.value + '">' + s.label + '</option>'; }).join('');
    el.innerHTML =
      '<div class="bpe-preview-wrap"><canvas id="bpe-preview" width="320" height="60"></canvas></div>' +
      '<div class="bpe-row"><span class="bpe-lbl">Forme de pointe</span><select class="bpe-tipshape" id="bpe-tipshape">' + tipShapeOptionsHtml + '</select></div>' +
      '<div class="bpe-row bpe-capture-row" id="bpe-capture-row" style="display:none">' +
      '<span class="bpe-lbl"></span><button class="pbtn" id="bpe-capture-btn">Capturer la sélection</button>' +
      '<span class="bpe-val" id="bpe-capture-status"></span></div>' +
      '<div class="bpe-row"><span class="bpe-lbl">Rotation</span><select class="bpe-rotmode" id="bpe-rotmode">' +
      '<option value="tangent">Suit le tracé</option><option value="random">Aléatoire</option><option value="fixed">Fixe</option>' +
      '</select></div>' +
      slidersHtml +
      '<div class="bpe-row"><span class="bpe-lbl">Nom</span><input type="text" id="bpe-name" class="bpe-name-input"></div>' +
      '<div class="bpe-actions"><button class="pbtn" id="bpe-cancel">Annuler</button><button class="pbtn ac" id="bpe-save">Enregistrer et utiliser</button></div>';
    document.body.appendChild(el);
    popover = el;

    var canvas = el.querySelector('#bpe-preview');
    var nameInput = el.querySelector('#bpe-name');
    nameInput.value = nameSeed;
    var rotSel = el.querySelector('#bpe-rotmode');
    rotSel.value = params.rotationMode || 'tangent';
    var tipSel = el.querySelector('#bpe-tipshape');
    tipSel.value = params.tipShape || 'ellipse';
    var captureRow = el.querySelector('#bpe-capture-row');
    var captureStatus = el.querySelector('#bpe-capture-status');

    // "dessiner sa texture de brush" — capture whatever's currently
    // selected on the canvas (drawn with Pen/Draw beforehand, same
    // pattern as pose-library's savePose) as the dab's own tip geometry,
    // normalized to a unit box (captureBrushStamp, tools.js) so
    // buildDabShape's 'custom' branch can rescale it like any other shape.
    // Geometry only — a dab is always solid-filled with the ink color, so
    // the captured path's own fill/stroke/color never matters.
    function refreshCaptureStatus() {
      if (params.customStamp && params.customStamp.segments && params.customStamp.segments.length) {
        captureStatus.textContent = 'Forme capturée (' + params.customStamp.segments.length + ' points)';
      } else {
        captureStatus.textContent = 'Aucune forme — ellipse utilisée en attendant';
      }
    }
    function syncCaptureRowVisibility() {
      captureRow.style.display = params.tipShape === 'custom' ? 'flex' : 'none';
      if (params.tipShape === 'custom') refreshCaptureStatus();
    }
    el.querySelector('#bpe-capture-btn').addEventListener('click', function () {
      var sel = (typeof selectedPaths !== 'undefined' && selectedPaths.length === 1) ? selectedPaths[0] : null;
      var res = window.captureBrushStamp(sel);
      if (!res.ok) { showToast(res.reason); return; }
      params.customStamp = res.stamp;
      refreshCaptureStatus();
      renderPreview();
      showToast(SM.t('toastShapeCapturedSuffix') + res.stamp.pointCount + ' points)');
    });
    syncCaptureRowVisibility();

    function renderPreview() {
      var ctx = canvas.getContext('2d'), w = canvas.width, h = canvas.height;
      ctx.clearRect(0, 0, w, h);
      var demo = getDemoPath(w, h);
      var baseWidth = Math.max(2, h * 0.14);
      var dabs = window.buildBrushDabs(demo, params, baseWidth);
      var textColor = getComputedStyle(document.documentElement).getPropertyValue('--text').trim() || '#eee';
      dabs.forEach(function (dab) {
        ctx.globalAlpha = dab.data.dabOpacity;
        ctx.fillStyle = textColor;
        traceOnCanvas2D(ctx, dab);
        ctx.fill();
        dab.remove();
      });
      ctx.globalAlpha = 1;
    }

    FIELDS.forEach(function (f) {
      var slider = el.querySelector('.bpe-slider[data-key="' + f.key + '"]');
      var valEl = el.querySelector('.bpe-val[data-valfor="' + f.key + '"]');
      slider.value = params[f.key] !== undefined ? params[f.key] : DEFAULT_PARAMS[f.key];
      valEl.textContent = slider.value;
      slider.addEventListener('input', function () {
        params[f.key] = parseFloat(slider.value);
        valEl.textContent = slider.value;
        renderPreview();
      });
    });
    rotSel.addEventListener('change', function () { params.rotationMode = rotSel.value; renderPreview(); });
    tipSel.addEventListener('change', function () { params.tipShape = tipSel.value; syncCaptureRowVisibility(); renderPreview(); });

    renderPreview();

    el.querySelector('#bpe-cancel').addEventListener('click', closePopover);
    el.querySelector('#bpe-save').addEventListener('click', function () {
      var id = editingCustomId || ('custom-' + Date.now().toString(36) + '-' + Math.floor(Math.random() * 1e4));
      if (!state.customBrushPresets) state.customBrushPresets = {};
      params.label = nameInput.value.trim() || 'Mon brush';
      state.customBrushPresets[id] = params;
      closePopover();
      if (onApply) onApply(id);
      showToast('Brush "' + params.label + SM.t('toastSavedSuffix'));
    });

    // Anchored center-screen (this panel is bigger/more of a destination
    // than the quick swatch popover, which anchors to its trigger button) —
    // simpler and avoids the panel running off-screen at any trigger
    // position given its size.
    el.style.visibility = 'hidden'; el.style.display = 'block';
    var ew = el.offsetWidth, eh = el.offsetHeight;
    el.style.left = Math.max(4, (window.innerWidth - ew) / 2) + 'px';
    el.style.top = Math.max(4, (window.innerHeight - eh) / 2) + 'px';
    el.style.visibility = '';

    function onKey(e) { if (e.key === 'Escape') closePopover(); }
    document.addEventListener('keydown', onKey);
    closeHandlers = function () { document.removeEventListener('keydown', onKey); };
  }

  window.BrushEditor = { open: open };
})();
