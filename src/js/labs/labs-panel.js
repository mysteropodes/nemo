// ---- LABS PANEL — Réglages > Labs UI for the feature-scouting prototypes ----
// Turns "type SMLabs.enable('name') in the console" into a real, discoverable
// UI: one checkbox row per stay-on tool, one button row per one-shot action —
// matching the existing tabbed-Réglages convention (renderShortcutsList is
// the template this follows). Config-driven (LABS_UI) rather than 24
// hand-written rows: each tool differs only in which inline controls it
// needs (a number input, a select, a couple of buttons), so one row-builder
// handles all of them from a small per-tool spec.
//
// Deliberately a separate file from labs-core.js: labs-core is the
// data/registry (SMLabs.register/enable/list), this file is presentation
// only — adding a UI for a new prototype never means touching the registry,
// and disabling this file entirely still leaves every SMLabs.* console API
// fully working (matches the "Labs are prototypes, not shipped features"
// framing — this panel is a convenience on top, not a dependency).
(function () {
  // -- shared row helpers ------------------------------------------------
  function mkRow() {
    var row = document.createElement('div');
    row.style.cssText = 'padding:6px 8px;border-radius:6px;';
    return row;
  }
  function mkHeadLine() {
    var d = document.createElement('div');
    d.className = 'pr';
    d.style.cssText = 'gap:8px;';
    return d;
  }
  function mkCheckbox(checked, onChange) {
    var cb = document.createElement('input');
    cb.type = 'checkbox';
    cb.checked = !!checked;
    cb.style.accentColor = 'var(--accent)';
    cb.addEventListener('change', function () { onChange(cb.checked); });
    return cb;
  }
  function mkLabel(text, hint) {
    var lbl = document.createElement('span');
    lbl.textContent = text;
    lbl.style.cssText = 'flex:1;font-size:11px;color:var(--text);cursor:default;';
    if (hint) lbl.title = hint;
    return lbl;
  }
  function mkNum(value, min, max, step, width) {
    var inp = document.createElement('input');
    inp.type = 'number'; inp.value = value; inp.min = min; inp.max = max; inp.step = step || 1;
    inp.className = 'pi scrub';
    inp.dataset.step = step || 1;
    inp.style.cssText = 'flex:none;width:' + (width || 52) + 'px;padding:3px 5px;font-size:11px;';
    return inp;
  }
  function mkSelect(options, value) {
    var sel = document.createElement('select');
    sel.className = 'psel';
    sel.style.cssText = 'flex:none;width:auto;padding:3px 5px;font-size:11px;';
    options.forEach(function (o) {
      var opt = document.createElement('option');
      opt.value = o.value; opt.textContent = o.label;
      if (o.value === value) opt.selected = true;
      sel.appendChild(opt);
    });
    return sel;
  }
  function mkText(placeholder, width) {
    var inp = document.createElement('input');
    inp.type = 'text'; inp.placeholder = placeholder || '';
    inp.className = 'pi';
    inp.style.cssText = 'padding:3px 6px;font-size:11px;width:' + (width || 110) + 'px;flex:none;';
    return inp;
  }
  function mkBtn(text) {
    var b = document.createElement('button');
    b.className = 'pbtn'; b.textContent = text;
    b.style.cssText = 'font-size:10px;padding:4px 8px;';
    return b;
  }
  function subRow() {
    var d = document.createElement('div');
    d.className = 'pr';
    d.style.cssText = 'gap:6px;margin-top:4px;padding-left:22px;flex-wrap:wrap;';
    return d;
  }

  // -- stay-on tools: checkbox + optional inline params -------------------
  // extra(container, isOn) is called once per render; only build/show
  // controls when the tool is currently on (a param the tool ignores while
  // off would be misleading UI).
  // `label`/`hintExtra` hold i18n KEYS (not literal text) — resolved via
  // SM.t() in renderLabsPanel()/mkLabel2() below, never eagerly here: this
  // array itself is safe to build at parse time (just data), but reading it
  // is deferred to render time, well after i18n.js has loaded (feedback
  // #105: Labs stayed French under an English app language because every
  // string here — and in each labs/*.js tool file's own `describe:` — was a
  // hardcoded literal instead of going through SM.t).
  var TOOLS = [
    // 'symmetry'/'radial-symmetry' promoted out of Labs (2026-07) into a
    // real shipped feature — see src/js/symmetry-bridge.js and the
    // "Symmetry Guide" section of the right panel + toolbar button.
    { name: 'predictive-stroke', label: 'labsPanelLabelPredictiveStroke' },
    {
      name: 'vector-sculpt', label: 'labsPanelLabelVectorSculpt',
      extra: function (c) {
        var row = subRow();
        row.appendChild(mkLabel2(SM.t('labsPanelSubLabelRadius')));
        var inp = mkNum(30, 5, 200, 1, 52);
        inp.addEventListener('change', function () { window.SMLabs.setSculptRadius(parseInt(inp.value, 10)); });
        row.appendChild(inp);
        c.appendChild(row);
      },
    },
    { name: 'french-curve', label: 'labsPanelLabelFrenchCurve', hintExtra: 'labsPanelHintFrenchCurve' },
    {
      name: 'canvas-grid', label: 'labsPanelLabelCanvasGrid',
      extra: function (c) {
        var row = subRow();
        row.appendChild(mkLabel2(SM.t('labsPanelSubLabelStepPx')));
        var inp = mkNum(50, 5, 500, 5, 56);
        inp.addEventListener('change', function () { window.SMLabs.setGridStep(parseInt(inp.value, 10)); });
        row.appendChild(inp);
        c.appendChild(row);
      },
    },
    {
      name: 'view-filter', label: 'labsPanelLabelViewFilter',
      extra: function (c) {
        var row = subRow();
        var sel = mkSelect([
          { value: 'grayscale', label: SM.t('labsPanelOptGrayscale') },
          { value: 'contrast', label: SM.t('labsPanelOptContrast') },
          { value: 'dim', label: SM.t('labsPanelOptDim') },
        ], 'grayscale');
        sel.addEventListener('change', function () { window.SMLabs.setViewFilter(sel.value); });
        row.appendChild(sel);
        c.appendChild(row);
      },
    },
    {
      name: 'flip-roll', label: 'labsPanelLabelFlipRoll',
      extra: function (c) {
        var row = subRow();
        row.appendChild(mkLabel2(SM.t('labsPanelSubLabelSpeed')));
        var sp = mkNum(12, 2, 30, 1, 44); sp.title = SM.t('labsPanelTitleFps');
        sp.addEventListener('change', function () { window.SMLabs.setFlipSpeed(parseInt(sp.value, 10)); });
        row.appendChild(sp);
        row.appendChild(mkLabel2(SM.t('labsPanelSubLabelSpan')));
        var span = mkNum(2, 1, 10, 1, 44); span.title = SM.t('labsPanelTitleSpanFrames');
        span.addEventListener('change', function () { window.SMLabs.setFlipSpan(parseInt(span.value, 10)); });
        row.appendChild(span);
        c.appendChild(row);
      },
    },
    { name: 'mirror-check', label: 'labsPanelLabelMirrorCheck' },
    { name: 'lagoon-menu', label: 'labsPanelLabelLagoonMenu' },
    {
      name: 'out-of-pegs', label: 'labsPanelLabelOutOfPegs',
      extra: function (c) {
        var row = subRow();
        var pdx = mkNum(-60, -400, 400, 5, 48), pdy = mkNum(0, -400, 400, 5, 48);
        var ndx = mkNum(60, -400, 400, 5, 48), ndy = mkNum(0, -400, 400, 5, 48);
        row.appendChild(mkLabel2(SM.t('labsPanelSubLabelPrevDxDy')));
        row.appendChild(pdx); row.appendChild(pdy);
        row.appendChild(mkLabel2(SM.t('labsPanelSubLabelNextDxDy')));
        row.appendChild(ndx); row.appendChild(ndy);
        var apply = mkBtn(SM.t('labsPanelBtnApply'));
        apply.addEventListener('click', function () {
          window.SMLabs.setPegOffset('prev', parseFloat(pdx.value) || 0, parseFloat(pdy.value) || 0);
          window.SMLabs.setPegOffset('next', parseFloat(ndx.value) || 0, parseFloat(ndy.value) || 0);
        });
        var reset = mkBtn(SM.t('labsPanelBtnReset'));
        reset.addEventListener('click', function () {
          pdx.value = 0; pdy.value = 0; ndx.value = 0; ndy.value = 0;
          window.SMLabs.resetPegs();
        });
        row.appendChild(apply); row.appendChild(reset);
        c.appendChild(row);
        // Apply a sensible default spread immediately — enabling this tool
        // with 0/0 offsets is a no-op the user would otherwise have to
        // discover requires touching two number fields.
        window.SMLabs.setPegOffset('prev', -60, 0);
        window.SMLabs.setPegOffset('next', 60, 0);
      },
    },
    { name: 'timeline-markers', label: 'labsPanelLabelTimelineMarkers', hintExtra: 'labsPanelHintTimelineMarkers' },
    {
      name: 'timeline-zoom', label: 'labsPanelLabelTimelineZoom',
      extra: function (c) {
        var row = subRow();
        var reset = mkBtn(SM.t('labsPanelBtnResetZoom'));
        reset.addEventListener('click', function () { window.SMLabs.resetTimelineZoom(); });
        row.appendChild(reset);
        c.appendChild(row);
      },
    },
    { name: 'xsheet', label: 'labsPanelLabelXsheet' },
    {
      name: 'reference-3d', label: 'labsPanelLabelReference3d',
      hintExtra: 'labsPanelHintReference3d',
      extra: function (c) {
        var row = subRow();
        [[SM.t('labsPanelBtnAvatarCC0'), function () { window.SMLabs.openCC0AvatarReference(); }], [SM.t('labsPanelBtnHandsCC0'), function () { window.SMLabs.openCC0HandReference(); }], [SM.t('labsPanelBtnMannequin'), function () { window.SMLabs.openCharacterReference('neutral'); }], [SM.t('labsPanelBtnActionPose'), function () { window.SMLabs.openCharacterReference('action'); }], [SM.t('labsPanelBtnSimpleHand'), function () { window.SMLabs.openHandReference('open'); }], [SM.t('labsPanelBtnFist'), function () { window.SMLabs.openHandReference('fist'); }], [SM.t('labsPanelBtnIndex'), function () { window.SMLabs.openHandReference('point'); }]].forEach(function (d) { var b = mkBtn(d[0]); b.addEventListener('click', d[1]); row.appendChild(b); });
        c.appendChild(row);
      },
    },
    { name: 'multiframe-draw', label: 'labsPanelLabelMultiframeDraw', hintExtra: 'labsPanelHintMultiframeDraw' },
  ];

  function mkLabel2(text) {
    var s = document.createElement('span');
    s.textContent = text;
    s.style.cssText = 'font-size:10px;color:var(--text-dim);flex:none;';
    return s;
  }

  // -- one-shot actions: inline params + a button --------------------------
  // `label` holds an i18n KEY here too, same rationale as TOOLS above.
  var ACTIONS = [
    {
      name: 'move-to-layer', label: 'labsPanelActionLabelMoveToLayer',
      build: function (row) {
        var sel = document.createElement('select');
        sel.className = 'pi'; sel.style.cssText = 'flex:1;padding:3px 6px;font-size:11px;';
        (state.layers || []).forEach(function (l, i) {
          if (i === state.activeLayerIdx) return;
          var o = document.createElement('option'); o.value = i; o.textContent = l.name || (SM.t('labsPanelLayerFallbackPrefix') + (i + 1));
          sel.appendChild(o);
        });
        row.appendChild(sel);
        var btn = mkBtn(SM.t('labsPanelBtnMove'));
        btn.addEventListener('click', function () { window.SMLabs.moveSelectionToLayer(parseInt(sel.value, 10)); });
        row.appendChild(btn);
      },
    },
    {
      name: 'pingpong-cycle', label: 'labsPanelActionLabelPingpongCycle',
      build: function (row) {
        var n = mkNum(2, 1, 50, 1, 48);
        row.appendChild(n);
        var btn = mkBtn(SM.t('labsPanelBtnCreateCycle'));
        btn.addEventListener('click', function () { window.SMLabs.pingpongCycle(parseInt(n.value, 10)); });
        row.appendChild(btn);
      },
    },
    {
      name: 'retime-exposure', label: 'labsPanelActionLabelRetimeExposure',
      build: function (row) {
        // "Ones (1)"/"Twos (2)"/"Threes (3)" are the standard animation-
        // jargon terms for exposure spacing — kept untranslated in every
        // language, same convention as retime-exposure.js's own toast.
        var sel = mkSelect([{ value: '1', label: 'Ones (1)' }, { value: '2', label: 'Twos (2)' }, { value: '3', label: 'Threes (3)' }, { value: '4', label: '4' }], '2');
        row.appendChild(sel);
        var btn = mkBtn(SM.t('labsPanelBtnRetime'));
        btn.addEventListener('click', function () { window.SMLabs.retimeExposure(parseInt(sel.value, 10)); });
        row.appendChild(btn);
      },
    },
    {
      name: 'interval-assistant', label: 'labsPanelActionLabelIntervalAssistant',
      build: function (row) {
        var n = mkNum(3, 1, 8, 1, 44);
        row.appendChild(n);
        // "Ease in/out"/"Ease in"/"Ease out" are standard easing-curve jargon
        // (as in After Effects etc.) — kept untranslated; only "Linéaire"
        // (a plain French word, not jargon) needs a real translation.
        var ease = mkSelect([{ value: 'inout', label: 'Ease in/out' }, { value: 'in', label: 'Ease in' }, { value: 'out', label: 'Ease out' }, { value: 'linear', label: SM.t('labsPanelOptLinear') }], 'inout');
        row.appendChild(ease);
        var btn = mkBtn(SM.t('labsPanelBtnPlaceBreakdowns'));
        btn.addEventListener('click', function () { window.SMLabs.intervalAssistant(parseInt(n.value, 10), ease.value); });
        row.appendChild(btn);
      },
    },
    {
      name: 'pose-library', label: 'labsPanelActionLabelPoseLibrary',
      build: function (row) {
        var name = mkText(SM.t('labsPanelPlaceholderPoseName'), 100);
        row.appendChild(name);
        var save = mkBtn(SM.t('labsPanelBtnSave'));
        save.addEventListener('click', function () { if (name.value) window.SMLabs.savePose(name.value); });
        row.appendChild(save);
        var picker = document.createElement('select');
        picker.className = 'pi'; picker.style.cssText = 'flex:none;width:110px;padding:3px 5px;font-size:11px;';
        function refillPicker() {
          picker.innerHTML = '';
          Object.keys(window.SMLabs.listPoses ? window.SMLabs.listPoses() : {}).forEach(function (n) {
            var o = document.createElement('option'); o.value = n; o.textContent = n; picker.appendChild(o);
          });
        }
        refillPicker();
        row.appendChild(picker);
        var stamp = mkBtn(SM.t('labsPanelBtnStamp'));
        stamp.addEventListener('click', function () { if (picker.value) window.SMLabs.stampPose(picker.value); });
        row.appendChild(stamp);
        var del = mkBtn(SM.t('labsPanelBtnDeleteShort'));
        del.addEventListener('click', function () { if (picker.value) { window.SMLabs.deletePose(picker.value); refillPicker(); } });
        row.appendChild(del);
        save.addEventListener('click', refillPicker);
      },
    },
    {
      name: 'speed-lines', label: 'labsPanelActionLabelSpeedLines',
      build: function (row) {
        var n = mkNum(60, 3, 400, 5, 52);
        row.appendChild(n);
        var btn = mkBtn(SM.t('labsPanelBtnGenerate'));
        btn.addEventListener('click', function () { window.SMLabs.speedLines({ count: parseInt(n.value, 10) }); });
        row.appendChild(btn);
      },
    },
    {
      name: 'boil-effect', label: 'labsPanelActionLabelBoilEffect',
      build: function (row) {
        row.appendChild(mkLabel2(SM.t('labsPanelSubLabelFrames')));
        var f = mkNum(3, 1, 24, 1, 44);
        row.appendChild(f);
        row.appendChild(mkLabel2(SM.t('labsPanelSubLabelAmplitude')));
        var a = mkNum(2.5, 0.2, 20, 0.5, 44);
        row.appendChild(a);
        var btn = mkBtn(SM.t('labsPanelBtnGenerateBoil'));
        btn.addEventListener('click', function () { window.SMLabs.boil({ frames: parseInt(f.value, 10), amplitude: parseFloat(a.value) }); });
        row.appendChild(btn);
      },
    },
    {
      name: 'follow-path', label: 'labsPanelActionLabelFollowPath',
      build: function (row) {
        var pathId = null;
        var status = document.createElement('span');
        status.style.cssText = 'font-size:9px;color:var(--text-dim);flex:none;';
        status.textContent = SM.t('labsPanelNoTrajectory');
        var capture = mkBtn(SM.t('labsPanelBtnCaptureTrajectory'));
        capture.addEventListener('click', function () {
          pathId = window.SMLabs.selectedStrokeId();
          status.textContent = pathId ? SM.t('labsPanelTrajectoryCaptured') : SM.t('labsPanelSelectSingleStroke');
        });
        row.appendChild(capture); row.appendChild(status);
        // Everything below lives in the same flex-wrap row as capture/status
        // above — the container already wraps (subRow's flex-wrap:wrap), so
        // this naturally drops to its own line without a separate element.
        row.appendChild(mkLabel2(SM.t('labsPanelSubLabelFrames')));
        var f = mkNum(12, 2, 48, 1, 44);
        row.appendChild(f);
        var ease = mkSelect([{ value: 'inout', label: 'Ease in/out' }, { value: 'in', label: 'Ease in' }, { value: 'out', label: 'Ease out' }, { value: 'linear', label: SM.t('labsPanelOptLinear') }], 'inout');
        row.appendChild(ease);
        var rmLbl = document.createElement('label');
        rmLbl.style.cssText = 'display:flex;align-items:center;gap:3px;font-size:9px;color:var(--text-dim);cursor:pointer;';
        var rmCb = document.createElement('input'); rmCb.type = 'checkbox'; rmCb.checked = true; rmCb.style.accentColor = 'var(--accent)';
        rmLbl.appendChild(rmCb); rmLbl.appendChild(document.createTextNode(SM.t('labsPanelRemoveTrajectoryLabel')));
        row.appendChild(rmLbl);
        var gen = mkBtn(SM.t('labsPanelBtnGenerate'));
        gen.addEventListener('click', function () {
          if (!pathId) { if (typeof showToast === 'function') showToast(SM.t('labsToastCaptureTrajectoryFirst')); return; }
          window.SMLabs.followPath({ pathId: pathId, frames: parseInt(f.value, 10), ease: ease.value, removeTrajectory: rmCb.checked });
        });
        row.appendChild(gen);
      },
    },
    {
      name: 'reference-fill', label: 'labsPanelActionLabelReferenceFill',
      build: function (row) {
        var btn = mkBtn(SM.t('labsPanelBtnFillAtCursor'));
        btn.title = SM.t('labsPanelTitleFillAtCursor');
        btn.addEventListener('click', function () { window.SMLabs.referenceFillAtPointer(); });
        row.appendChild(btn);
      },
    },
    {
      name: 'vector-trim', label: 'labsPanelActionLabelVectorTrim',
      build: function (row) {
        var btn = mkBtn(window.SMLabs.isKnifeTrimActive() ? SM.t('labsPanelKnifeActive') : SM.t('labsPanelBtnActivateKnife'));
        btn.classList.toggle('ac', window.SMLabs.isKnifeTrimActive());
        btn.title = SM.t('labsPanelTitleKnifeHint');
        btn.addEventListener('click', function () {
          window.SMLabs.setKnifeTrimActive(!window.SMLabs.isKnifeTrimActive());
          btn.textContent = window.SMLabs.isKnifeTrimActive() ? SM.t('labsPanelKnifeActive') : SM.t('labsPanelBtnActivateKnife');
          btn.classList.toggle('ac', window.SMLabs.isKnifeTrimActive());
        });
        row.appendChild(btn);
      },
    },
  ];

  function describeOf(name) {
    if (!window.SMLabs || !window.SMLabs.list) return '';
    var found = window.SMLabs.list().find(function (p) { return p.name === name; });
    return found ? found.what : '';
  }

  function renderLabsPanel() {
    if (!window.SMLabs) return;
    var toolsEl = document.getElementById('labs-tools-list');
    var actionsEl = document.getElementById('labs-actions-list');
    if (!toolsEl || !actionsEl) return;
    toolsEl.innerHTML = '';
    actionsEl.innerHTML = '';

    TOOLS.forEach(function (spec) {
      if (!window.SMLabs.isOn) return;
      var row = mkRow();
      var head = mkHeadLine();
      var on = window.SMLabs.isOn(spec.name);
      var cb = mkCheckbox(on, function (checked) {
        window.SMLabs[checked ? 'enable' : 'disable'](spec.name);
        renderLabsPanel(); // rebuild so inline params show/hide with the toggle
        if (window.renderLabsFloatPanel) window.renderLabsFloatPanel(); // same toggle, other entry point
      });
      head.appendChild(cb);
      head.appendChild(mkLabel(SM.t(spec.label), describeOf(spec.name) || (spec.hintExtra && SM.t(spec.hintExtra))));
      row.appendChild(head);
      if (on && spec.extra) spec.extra(row);
      if (on && spec.hintExtra && !spec.extra) {
        var hint = subRow();
        var hs = document.createElement('span'); hs.style.cssText = 'font-size:9px;color:var(--text-dim);'; hs.textContent = SM.t(spec.hintExtra);
        hint.appendChild(hs); row.appendChild(hint);
      }
      toolsEl.appendChild(row);
    });

    ACTIONS.forEach(function (spec) {
      var row = mkRow();
      var lbl = document.createElement('div');
      lbl.style.cssText = 'font-size:11px;color:var(--text);margin-bottom:3px;';
      lbl.textContent = SM.t(spec.label);
      lbl.title = describeOf(spec.name);
      row.appendChild(lbl);
      var controls = subRow();
      controls.style.paddingLeft = '0';
      spec.build(controls);
      row.appendChild(controls);
      actionsEl.appendChild(row);
    });
  }

  window.renderLabsPanel = renderLabsPanel;
  // Repaint on language switch (i18n.js's documented escape hatch, same
  // pattern as timeline.js's syncPropsPanelCollapseTitle) — this panel's
  // labels/hints/describe text are all built in JS rather than living on
  // data-i18n attributes, so applyI18n()'s own DOM sweep never reaches them.
  // Deferred to 'load': this file runs BEFORE timeline.js in index.html's
  // script order, and timeline.js does `window.SM={...}` as a full object
  // REASSIGNMENT (not a `window.SM=window.SM||{}` merge) — registering here
  // at parse time would get silently discarded when that reassignment runs.
  // By 'load' every script (timeline.js, i18n.js) has already executed.
  window.addEventListener('load', function () {
    window.SM = window.SM || {};
    (window.SM.afterI18n = window.SM.afterI18n || []).push(function () { if (document.getElementById('labs-tools-list')) renderLabsPanel(); });
  });
})();
