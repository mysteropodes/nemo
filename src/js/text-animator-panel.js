// ---- TEXT ANIMATOR PANEL (2026-08-31) ----
// The right-panel face of ld.textAnimators (motion.js) / the range selector
// (text-selector.js). Cyril: "ça peut être dans layer properties quand on
// select un text".
//
// Same "section that only exists for ONE kind of selection" pattern as
// rig-widget.js's own panel and image-mesh-bridge.js's: a static #p-textanim-sec
// block in index.html, shown/hidden and populated from here, hooked into
// updatePropsContext (timeline.js) next to those two. Nothing here owns state —
// every edit writes straight onto the animator descriptor and re-renders.
//
// Every widget below is built from the panel's OWN vocabulary (.pr / .pl /
// .pi scrub / .psel / .pbtn / .dims-row) rather than a new one, so the section
// inherits the app's look for free and every numeric field is scrubbable by
// CLAUDE.md §10's single delegated handler — the `scrub` class is the ONLY
// thing that opts a field in, and a field built here later still gets it
// because the detection is delegated at the document level.
(function () {
  function el(id) { return document.getElementById(id); }
  function t(key, fallback) {
    var s = (window.SM && SM.t) ? SM.t(key) : key;
    return (s && s !== key) ? s : fallback;
  }

  // A text layer for this panel's purposes = the active layer carries vector
  // text. Read from the STORED strokes, not the live Paper items: the panel
  // re-renders from updateUI on every frame change, and loadFrame rebuilds
  // items — the same transient-null trap image-mesh-bridge.js documents
  // (§12bis piège #2) would otherwise blink the section away mid-scrub.
  function activeTextLayer() {
    if (!window.state || !state.layers) return null;
    var li = state.activeLayerIdx, ld = state.layers[li];
    if (!ld) return null;
    var strokes = (typeof getEffectiveStrokes === 'function') ? getEffectiveStrokes(li, state.currentFrame) : null;
    if (!strokes || !strokes.length) return null;
    for (var i = 0; i < strokes.length; i++) {
      if (strokes[i] && strokes[i].charIndex != null) return { li: li, ld: ld };
    }
    return null;
  }
  // A whole, not-yet-split RASTER text block — the layer/stored-frame
  // equivalent of updateTextActionsPanel's own selectedPaths-based
  // isWholeText check (timeline.js), used here only to decide whether the
  // shared section should stay visible for the split action before any
  // per-character unit exists yet. Vector text never matches this (it's
  // already split at creation, so it's always caught by activeTextLayer
  // above instead).
  function wholeTextLayerContext() {
    if (!window.state || !state.layers) return null;
    var li = state.activeLayerIdx, ld = state.layers[li];
    if (!ld) return null;
    var strokes = (typeof getEffectiveStrokes === 'function') ? getEffectiveStrokes(li, state.currentFrame) : null;
    if (!strokes || strokes.length !== 1) return null;
    var s = strokes[0];
    return (s && s.isRaster && s.isText && !s.isTextChar) ? { li: li, ld: ld } : null;
  }

  function commit(ld) {
    if (typeof saveActiveLayerFrame === 'function') saveActiveLayerFrame();
    if (typeof renderLayerList === 'function') renderLayerList();
    if (typeof renderTimeline === 'function') renderTimeline();
    if (window.SMEngineBridge) SMEngineBridge.renderNow();
    render();
  }

  function row(parent, labelText, cls) {
    var r = document.createElement('div');
    r.className = 'pr' + (cls ? ' ' + cls : '');
    var l = document.createElement('span');
    l.className = 'pl';
    l.textContent = labelText;
    r.appendChild(l);
    parent.appendChild(r);
    return r;
  }

  // A scrubbable number bound to one field of the animator descriptor.
  function numField(r, get, set, step, min, max) {
    var i = document.createElement('input');
    i.type = 'number';
    i.className = 'pi scrub';           // §10: `scrub` is what makes it draggable
    i.value = get();
    if (step != null) i.dataset.step = step;
    if (min != null) i.min = min;
    if (max != null) i.max = max;
    function apply() {
      var v = parseFloat(i.value);
      if (!isFinite(v)) { i.value = get(); return; }
      if (min != null) v = Math.max(min, v);
      if (max != null) v = Math.min(max, v);
      set(v);
    }
    i.addEventListener('change', apply);
    // Scrubbing fires `input` continuously; commit live so the canvas follows
    // the drag rather than jumping at release (same feel as every other
    // numeric field in this panel).
    i.addEventListener('input', function () {
      var v = parseFloat(i.value);
      if (!isFinite(v)) return;
      if (min != null) v = Math.max(min, v);
      if (max != null) v = Math.min(max, v);
      set(v, true);
    });
    r.appendChild(i);
    return i;
  }

  function selField(r, options, get, set) {
    var s = document.createElement('select');
    s.className = 'psel';
    for (var i = 0; i < options.length; i++) {
      var o = document.createElement('option');
      o.value = options[i].value;
      o.textContent = options[i].label;
      s.appendChild(o);
    }
    s.value = String(get());
    s.addEventListener('change', function () { set(s.value); });
    r.appendChild(s);
    return s;
  }

  // Stopwatch for one selector field. The animator descriptor IS a Motion
  // holder (it carries its own .motion/.motionStatic), which is why this can
  // reuse SMMotion's own toggle/state helpers unchanged — holders never
  // required a layer, the lever already proven for audio tracks and widgets.
  function stopwatch(r, an, prop, ld) {
    if (!window.SMMotion || !SMMotion.toggleAnimated) return null;
    var on = SMMotion.isAnimated && SMMotion.isAnimated(an, prop);
    var b = document.createElement('div');
    b.className = 'lico motion-stopwatch' + (on ? ' on' : '');
    b.title = t('motionAnimateProp', 'Enable animation for this property');
    b.innerHTML = '<svg viewBox="0 0 24 24" width="12" height="12"><path d="M12 3l9 9-9 9-9-9z" fill="' + (on ? 'currentColor' : 'none') + '" stroke="currentColor" stroke-width="2"/></svg>';
    b.addEventListener('click', function (e) {
      e.stopPropagation();
      if (typeof pushUndo === 'function') pushUndo();
      SMMotion.toggleAnimated(an, prop);
      commit(ld);
    });
    r.appendChild(b);
    return b;
  }

  function shapeOptions() {
    var S = window.SMTextSelector ? SMTextSelector.SHAPE : { SQUARE: 1, RAMP_UP: 2, RAMP_DOWN: 3, TRIANGLE: 4, ROUND: 5, SMOOTH: 6 };
    return [
      { value: S.SQUARE, label: t('textAnimShapeSquare', 'Square') },
      { value: S.RAMP_UP, label: t('textAnimShapeRampUp', 'Ramp Up') },
      { value: S.RAMP_DOWN, label: t('textAnimShapeRampDown', 'Ramp Down') },
      { value: S.TRIANGLE, label: t('textAnimShapeTriangle', 'Triangle') },
      { value: S.ROUND, label: t('textAnimShapeRound', 'Round') },
      { value: S.SMOOTH, label: t('textAnimShapeSmooth', 'Smooth') },
    ];
  }

  function buildAnimator(host, ld, an, idx) {
    var sel = an.selector, props = an.props;

    // --- header: name, enable, delete -----------------------------------
    // An ordinary .pr with the name in .pl and icon buttons on the right,
    // rather than a bespoke sub-heading: the panel has no sub-group style of
    // its own, and .pl's fixed 68px column is exactly what keeps every row
    // below aligned with the rest of the app's sections.
    var head = row(host, t('textAnimTitle', 'Animator') + ' ' + (idx + 1));
    var spacer = document.createElement('div');
    spacer.style.flex = '1';
    head.appendChild(spacer);

    var eye = document.createElement('button');
    eye.type = 'button';
    eye.className = 'pbtn icon-only-btn' + (an.enabled === false ? '' : ' active');
    eye.title = t('textAnimToggleTitle', 'Enable / disable this animator');
    eye.innerHTML = an.enabled === false
      ? '<svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M17.94 17.94A10.07 10.07 0 0 1 12 20c-7 0-11-8-11-8a18.45 18.45 0 0 1 5.06-5.94M9.9 4.24A9.12 9.12 0 0 1 12 4c7 0 11 8 11 8a18.5 18.5 0 0 1-2.16 3.19m-6.72-1.07a3 3 0 1 1-4.24-4.24"/><path d="M1 1l22 22"/></svg>'
      : '<svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z"/><circle cx="12" cy="12" r="3"/></svg>';
    eye.addEventListener('click', function () {
      if (typeof pushUndo === 'function') pushUndo();
      an.enabled = an.enabled === false;
      commit(ld);
    });
    head.appendChild(eye);

    var del = document.createElement('button');
    del.type = 'button';
    del.className = 'pbtn icon-only-btn';
    del.title = t('textAnimRemoveTitle', 'Remove this animator');
    del.innerHTML = '<svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M6 6l12 12M18 6L6 18"/></svg>';
    del.addEventListener('click', function () {
      if (typeof pushUndo === 'function') pushUndo();
      SMMotion.removeTextAnimator(state.activeLayerIdx, an.id);
      commit(ld);
    });
    head.appendChild(del);

    // --- selector --------------------------------------------------------
    var rRange = row(host, t('textAnimRange', 'Range'));
    numField(rRange, function () { return sel.start; }, function (v, live) { sel.start = v; if (!live) commit(ld); else liveRefresh(ld); }, 1, -200, 200);
    numField(rRange, function () { return sel.end; }, function (v, live) { sel.end = v; if (!live) commit(ld); else liveRefresh(ld); }, 1, -200, 200);
    stopwatch(rRange, an, 'start', ld);

    var rOff = row(host, t('textAnimOffset', 'Offset'));
    numField(rOff, function () { return sel.offset; }, function (v, live) { sel.offset = v; if (!live) commit(ld); else liveRefresh(ld); }, 1, -200, 200);
    // Offset is THE field to keyframe — two keys on it sweep the effect along
    // the whole block, which is what replaces N staggered per-glyph series.
    stopwatch(rOff, an, 'offset', ld);

    var rShape = row(host, t('textAnimShape', 'Shape'));
    selField(rShape, shapeOptions(), function () { return sel.shape; }, function (v) {
      if (typeof pushUndo === 'function') pushUndo();
      sel.shape = parseInt(v, 10); commit(ld);
    });

    var rBased = row(host, t('textAnimBasedOn', 'Based on'));
    selField(rBased, [
      { value: 'chars', label: t('textAnimUnitChars', 'Characters') },
      { value: 'words', label: t('textAnimUnitWords', 'Words') },
      { value: 'lines', label: t('textAnimUnitLines', 'Lines') },
    ], function () { return sel.basedOn || 'chars'; }, function (v) {
      if (typeof pushUndo === 'function') pushUndo();
      sel.basedOn = v; commit(ld);
    });

    var rEase = row(host, t('textAnimEase', 'Ease hi/lo'));
    numField(rEase, function () { return sel.easeHigh; }, function (v, live) { sel.easeHigh = v; if (!live) commit(ld); else liveRefresh(ld); }, 5, -100, 100);
    numField(rEase, function () { return sel.easeLow; }, function (v, live) { sel.easeLow = v; if (!live) commit(ld); else liveRefresh(ld); }, 5, -100, 100);

    var rAmt = row(host, t('textAnimAmount', 'Amount'));
    numField(rAmt, function () { return sel.amount; }, function (v, live) { sel.amount = v; if (!live) commit(ld); else liveRefresh(ld); }, 5, -100, 100);
    stopwatch(rAmt, an, 'amount', ld);

    // --- driven properties ----------------------------------------------
    var rPos = row(host, t('propPosition', 'Position'));
    numField(rPos, function () { return (props.position || [0, 0])[0]; }, function (v, live) { props.position = props.position || [0, 0]; props.position[0] = v; if (!live) commit(ld); else liveRefresh(ld); }, 1);
    numField(rPos, function () { return (props.position || [0, 0])[1]; }, function (v, live) { props.position = props.position || [0, 0]; props.position[1] = v; if (!live) commit(ld); else liveRefresh(ld); }, 1);

    var rScale = row(host, t('propScale', 'Scale'));
    numField(rScale, function () { return (props.scale || [100, 100])[0]; }, function (v, live) { props.scale = props.scale || [100, 100]; props.scale[0] = v; if (!live) commit(ld); else liveRefresh(ld); }, 1, 0, 1000);
    numField(rScale, function () { return (props.scale || [100, 100])[1]; }, function (v, live) { props.scale = props.scale || [100, 100]; props.scale[1] = v; if (!live) commit(ld); else liveRefresh(ld); }, 1, 0, 1000);

    var rRot = row(host, t('propRotation', 'Rotation'));
    numField(rRot, function () { return props.rotation || 0; }, function (v, live) { props.rotation = v; if (!live) commit(ld); else liveRefresh(ld); }, 1);

    var rOp = row(host, t('propOpacity', 'Opacity'));
    numField(rOp, function () { return props.opacity == null ? 100 : props.opacity; }, function (v, live) { props.opacity = v; if (!live) commit(ld); else liveRefresh(ld); }, 1, 0, 100);

    // --- weight ramp preview --------------------------------------------
    // Shows what the selector currently weighs, per unit. It is the one thing
    // that makes the model legible at a glance: which characters are taken,
    // and how much. Drawn from the SAME textAnimatorWeights the renderer uses,
    // so it can never drift from what the canvas does.
    var rampRow = document.createElement('div');
    rampRow.className = 'pr';
    var rampLabel = document.createElement('span');
    rampLabel.className = 'pl';
    rampLabel.textContent = t('textAnimWeights', 'Weights');
    rampRow.appendChild(rampLabel);
    var ramp = document.createElement('div');
    ramp.className = 'ta-ramp';
    var ws = (window.SMMotion && SMMotion.textAnimatorWeights) ? SMMotion.textAnimatorWeights(state.activeLayerIdx, idx, state.currentFrame) : [];
    for (var i = 0; i < ws.length && i < 60; i++) {
      var b = document.createElement('i');
      b.style.height = Math.round(Math.max(0, Math.min(1, ws[i])) * 100) + '%';
      b.title = 'w' + i + ' = ' + ws[i].toFixed(2);
      ramp.appendChild(b);
    }
    if (!ws.length) {
      var none = document.createElement('span');
      none.className = 'ta-ramp-empty';
      none.textContent = t('textAnimNoUnits', 'no characters');
      ramp.appendChild(none);
    }
    rampRow.appendChild(ramp);
    host.appendChild(rampRow);

    var sep = document.createElement('div');
    sep.className = 'ta-sep';
    host.appendChild(sep);
  }

  // Cheap path for a scrub in progress: repaint the canvas and this panel's
  // ramp without the full save/relayout commit() does on release.
  function liveRefresh(ld) {
    if (window.SMEngineBridge) SMEngineBridge.renderNow();
    var list = el('p-textanim-list');
    if (!list) return;
    var bars = list.querySelectorAll('.ta-ramp');
    var anims = (window.SMMotion && SMMotion.textAnimatorsOf) ? SMMotion.textAnimatorsOf(state.activeLayerIdx) : [];
    for (var a = 0; a < bars.length && a < anims.length; a++) {
      var ws = SMMotion.textAnimatorWeights(state.activeLayerIdx, a, state.currentFrame);
      var kids = bars[a].querySelectorAll('i');
      for (var i = 0; i < kids.length && i < ws.length; i++) {
        kids[i].style.height = Math.round(Math.max(0, Math.min(1, ws[i])) * 100) + '%';
      }
    }
  }

  // Owns the shared section's overall visibility (merged 2026-08-31 with
  // the former standalone "Text" section — see index.html's comment on
  // #p-textanim-sec). Runs LAST in the update chain (updatePropsContext
  // calls this after updateTextActionsPanel already ran, timeline.js), so
  // it's the authoritative display decision — updateTextActionsPanel only
  // ever toggles its own two rows, never the section itself, precisely so
  // the two can't stomp each other regardless of call order.
  function render() {
    var sec = el('p-textanim-sec');
    if (!sec) return;
    var ctx = activeTextLayer();
    var wholeCtx = ctx ? null : wholeTextLayerContext();
    if (!ctx && !wholeCtx) { sec.style.display = 'none'; return; }
    sec.style.display = '';
    var addRow = el('p-textanim-add-row');
    if (addRow) addRow.style.display = ctx ? '' : 'none';
    var hint = el('p-textanim-empty');
    var list = el('p-textanim-list');
    if (!ctx) {
      // Whole, not-yet-split text: nothing to list or add yet — the
      // section stays open purely so text-split-row (updateTextActionsPanel)
      // has somewhere to live, matching this file's own "add" affordance
      // pattern rather than inventing a second empty-state.
      if (list) list.innerHTML = '';
      if (hint) hint.style.display = 'none';
      return;
    }
    if (!list) return;
    list.innerHTML = '';
    var anims = (window.SMMotion && SMMotion.textAnimatorsOf) ? SMMotion.textAnimatorsOf(ctx.li) : [];
    for (var i = 0; i < anims.length; i++) buildAnimator(list, ctx.ld, anims[i], i);
    if (hint) hint.style.display = anims.length ? 'none' : '';
  }

  function init() {
    var add = el('p-textanim-add');
    if (add) {
      add.addEventListener('click', function () {
        var ctx = activeTextLayer();
        if (!ctx || !window.SMMotion || !SMMotion.addTextAnimator) return;
        if (typeof pushUndo === 'function') pushUndo();
        // Seeded with a visible effect rather than an all-neutral animator:
        // an animator that does nothing looks broken, and "rise into place"
        // is the entrance every preset library opens with.
        SMMotion.addTextAnimator(ctx.li,
          { position: [0, -60], scale: [100, 100], rotation: 0, opacity: 0 },
          { start: 0, end: 40, shape: (window.SMTextSelector ? SMTextSelector.SHAPE.SMOOTH : 6) });
        commit(ctx.ld);
      });
    }
    render();
  }

  window.renderTextAnimatorPanel = render;
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', init);
  else init();
})();
