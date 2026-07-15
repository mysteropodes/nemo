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
    inp.className = 'pi';
    inp.style.cssText = 'flex:none;width:' + (width || 52) + 'px;padding:3px 5px;font-size:11px;';
    return inp;
  }
  function mkSelect(options, value) {
    var sel = document.createElement('select');
    sel.className = 'pi';
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
  var TOOLS = [
    { name: 'symmetry', label: 'Miroir vertical' },
    {
      name: 'radial-symmetry', label: 'Miroir radial / mandala',
      extra: function (c) {
        var n = parseInt(localStorage.getItem('nemo-labs-radial-sectors') || '6', 10);
        var row = subRow();
        row.appendChild(mkLabel2('Secteurs'));
        var inp = mkNum(n, 2, 16, 1, 46);
        inp.addEventListener('change', function () { window.SMLabs.setRadialSectors(parseInt(inp.value, 10)); });
        row.appendChild(inp);
        c.appendChild(row);
      },
    },
    { name: 'predictive-stroke', label: 'Trait prédictif — cercle/ligne/rect' },
    {
      name: 'vector-sculpt', label: 'Sculpt vectoriel — pousser W, lisser W+Shift',
      extra: function (c) {
        var row = subRow();
        row.appendChild(mkLabel2('Rayon'));
        var inp = mkNum(30, 5, 200, 1, 52);
        inp.addEventListener('change', function () { window.SMLabs.setSculptRadius(parseInt(inp.value, 10)); });
        row.appendChild(inp);
        c.appendChild(row);
      },
    },
    { name: 'french-curve', label: 'Gabarit courbe/ellipse aimanté — maintenir F', hintExtra: 'Gabarit par défaut : ellipse au centre du canvas. Réajustable via SMLabs.setCurveGuide(\'ellipse\'|\'line\', {…}) dans la console.' },
    {
      name: 'canvas-grid', label: 'Grille monde',
      extra: function (c) {
        var row = subRow();
        row.appendChild(mkLabel2('Pas (px)'));
        var inp = mkNum(50, 5, 500, 5, 56);
        inp.addEventListener('change', function () { window.SMLabs.setGridStep(parseInt(inp.value, 10)); });
        row.appendChild(inp);
        c.appendChild(row);
      },
    },
    {
      name: 'view-filter', label: 'Contrôle des valeurs',
      extra: function (c) {
        var row = subRow();
        var sel = mkSelect([
          { value: 'grayscale', label: 'Niveaux de gris' },
          { value: 'contrast', label: 'Contraste renforcé' },
          { value: 'dim', label: 'Assombrir' },
        ], 'grayscale');
        sel.addEventListener('change', function () { window.SMLabs.setViewFilter(sel.value); });
        row.appendChild(sel);
        c.appendChild(row);
      },
    },
    {
      name: 'flip-roll', label: 'Rouleau d\'animateur — maintenir R',
      extra: function (c) {
        var row = subRow();
        row.appendChild(mkLabel2('Vitesse'));
        var sp = mkNum(12, 2, 30, 1, 44); sp.title = 'images/seconde';
        sp.addEventListener('change', function () { window.SMLabs.setFlipSpeed(parseInt(sp.value, 10)); });
        row.appendChild(sp);
        row.appendChild(mkLabel2('Portée'));
        var span = mkNum(2, 1, 10, 1, 44); span.title = '± frames autour de la pose';
        span.addEventListener('change', function () { window.SMLabs.setFlipSpan(parseInt(span.value, 10)); });
        row.appendChild(span);
        c.appendChild(row);
      },
    },
    { name: 'mirror-check', label: 'Miroir de contrôle — maintenir M' },
    { name: 'lagoon-menu', label: 'Menu radial d\'outils — maintenir Q' },
    {
      name: 'out-of-pegs', label: 'Décalage des fantômes onion',
      extra: function (c) {
        var row = subRow();
        var pdx = mkNum(-60, -400, 400, 5, 48), pdy = mkNum(0, -400, 400, 5, 48);
        var ndx = mkNum(60, -400, 400, 5, 48), ndy = mkNum(0, -400, 400, 5, 48);
        row.appendChild(mkLabel2('Préc. dx/dy'));
        row.appendChild(pdx); row.appendChild(pdy);
        row.appendChild(mkLabel2('Suiv. dx/dy'));
        row.appendChild(ndx); row.appendChild(ndy);
        var apply = mkBtn('Appliquer');
        apply.addEventListener('click', function () {
          window.SMLabs.setPegOffset('prev', parseFloat(pdx.value) || 0, parseFloat(pdy.value) || 0);
          window.SMLabs.setPegOffset('next', parseFloat(ndx.value) || 0, parseFloat(ndy.value) || 0);
        });
        var reset = mkBtn('Réinitialiser');
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
    { name: 'timeline-markers', label: 'Marqueurs de timeline', hintExtra: 'Clic-droit sur une frame de la timeline pour poser un marqueur nommé/coloré.' },
    {
      name: 'timeline-zoom', label: 'Zoom horizontal de la timeline',
      extra: function (c) {
        var row = subRow();
        var reset = mkBtn('Réinitialiser le zoom');
        reset.addEventListener('click', function () { window.SMLabs.resetTimelineZoom(); });
        row.appendChild(reset);
        c.appendChild(row);
      },
    },
    { name: 'xsheet', label: 'Feuille d\'exposition flottante' },
    { name: 'multiframe-draw', label: 'Dessin multi-frames', hintExtra: 'Sélectionne plusieurs frames dans la timeline avant de dessiner — le trait est tamponné sur chacune.' },
  ];

  function mkLabel2(text) {
    var s = document.createElement('span');
    s.textContent = text;
    s.style.cssText = 'font-size:10px;color:var(--text-dim);flex:none;';
    return s;
  }

  // -- one-shot actions: inline params + a button --------------------------
  var ACTIONS = [
    {
      name: 'move-to-layer', label: 'Déplacer la sélection vers un calque',
      build: function (row) {
        var sel = document.createElement('select');
        sel.className = 'pi'; sel.style.cssText = 'flex:1;padding:3px 6px;font-size:11px;';
        (state.layers || []).forEach(function (l, i) {
          if (i === state.activeLayerIdx) return;
          var o = document.createElement('option'); o.value = i; o.textContent = l.name || ('Calque ' + (i + 1));
          sel.appendChild(o);
        });
        row.appendChild(sel);
        var btn = mkBtn('Déplacer');
        btn.addEventListener('click', function () { window.SMLabs.moveSelectionToLayer(parseInt(sel.value, 10)); });
        row.appendChild(btn);
      },
    },
    {
      name: 'pingpong-cycle', label: 'Cycle aller-retour sur la plage sélectionnée',
      build: function (row) {
        var n = mkNum(2, 1, 50, 1, 48);
        row.appendChild(n);
        var btn = mkBtn('Créer le cycle');
        btn.addEventListener('click', function () { window.SMLabs.pingpongCycle(parseInt(n.value, 10)); });
        row.appendChild(btn);
      },
    },
    {
      name: 'retime-exposure', label: 'Re-caler l\'exposition — ones/twos/threes',
      build: function (row) {
        var sel = mkSelect([{ value: '1', label: 'Ones (1)' }, { value: '2', label: 'Twos (2)' }, { value: '3', label: 'Threes (3)' }, { value: '4', label: '4' }], '2');
        row.appendChild(sel);
        var btn = mkBtn('Re-caler');
        btn.addEventListener('click', function () { window.SMLabs.retimeExposure(parseInt(sel.value, 10)); });
        row.appendChild(btn);
      },
    },
    {
      name: 'interval-assistant', label: 'Breakdowns éasés entre 2 poses',
      build: function (row) {
        var n = mkNum(3, 1, 8, 1, 44);
        row.appendChild(n);
        var ease = mkSelect([{ value: 'inout', label: 'Ease in/out' }, { value: 'in', label: 'Ease in' }, { value: 'out', label: 'Ease out' }, { value: 'linear', label: 'Linéaire' }], 'inout');
        row.appendChild(ease);
        var btn = mkBtn('Poser les breakdowns');
        btn.addEventListener('click', function () { window.SMLabs.intervalAssistant(parseInt(n.value, 10), ease.value); });
        row.appendChild(btn);
      },
    },
    {
      name: 'pose-library', label: 'Bibliothèque de poses / substitution',
      build: function (row) {
        var name = mkText('nom de la pose', 100);
        row.appendChild(name);
        var save = mkBtn('Enregistrer');
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
        var stamp = mkBtn('Tamponner');
        stamp.addEventListener('click', function () { if (picker.value) window.SMLabs.stampPose(picker.value); });
        row.appendChild(stamp);
        var del = mkBtn('Suppr.');
        del.addEventListener('click', function () { if (picker.value) { window.SMLabs.deletePose(picker.value); refillPicker(); } });
        row.appendChild(del);
        save.addEventListener('click', refillPicker);
      },
    },
    {
      name: 'speed-lines', label: 'Lignes de vitesse',
      build: function (row) {
        var n = mkNum(60, 3, 400, 5, 52);
        row.appendChild(n);
        var btn = mkBtn('Générer');
        btn.addEventListener('click', function () { window.SMLabs.speedLines({ count: parseInt(n.value, 10) }); });
        row.appendChild(btn);
      },
    },
    {
      name: 'boil-effect', label: 'Ligne bouillante — bruit vectoriel',
      build: function (row) {
        row.appendChild(mkLabel2('Frames'));
        var f = mkNum(3, 1, 24, 1, 44);
        row.appendChild(f);
        row.appendChild(mkLabel2('Amplitude'));
        var a = mkNum(2.5, 0.2, 20, 0.5, 44);
        row.appendChild(a);
        var btn = mkBtn('Générer le bouillonnement');
        btn.addEventListener('click', function () { window.SMLabs.boil({ frames: parseInt(f.value, 10), amplitude: parseFloat(a.value) }); });
        row.appendChild(btn);
      },
    },
    {
      name: 'follow-path', label: 'Bake le long d\'un trait-trajectoire',
      build: function (row) {
        var pathId = null;
        var status = document.createElement('span');
        status.style.cssText = 'font-size:9px;color:var(--text-dim);flex:none;';
        status.textContent = 'aucune trajectoire';
        var capture = mkBtn('Capturer trajectoire (sélection)');
        capture.addEventListener('click', function () {
          pathId = window.SMLabs.selectedStrokeId();
          status.textContent = pathId ? 'trajectoire capturée' : 'sélectionne 1 seul trait';
        });
        row.appendChild(capture); row.appendChild(status);
        // Everything below lives in the same flex-wrap row as capture/status
        // above — the container already wraps (subRow's flex-wrap:wrap), so
        // this naturally drops to its own line without a separate element.
        row.appendChild(mkLabel2('Frames'));
        var f = mkNum(12, 2, 48, 1, 44);
        row.appendChild(f);
        var ease = mkSelect([{ value: 'inout', label: 'Ease in/out' }, { value: 'in', label: 'Ease in' }, { value: 'out', label: 'Ease out' }, { value: 'linear', label: 'Linéaire' }], 'inout');
        row.appendChild(ease);
        var rmLbl = document.createElement('label');
        rmLbl.style.cssText = 'display:flex;align-items:center;gap:3px;font-size:9px;color:var(--text-dim);cursor:pointer;';
        var rmCb = document.createElement('input'); rmCb.type = 'checkbox'; rmCb.checked = true; rmCb.style.accentColor = 'var(--accent)';
        rmLbl.appendChild(rmCb); rmLbl.appendChild(document.createTextNode('supprimer la trajectoire'));
        row.appendChild(rmLbl);
        var gen = mkBtn('Générer');
        gen.addEventListener('click', function () {
          if (!pathId) { if (typeof showToast === 'function') showToast('Capture d\'abord une trajectoire'); return; }
          window.SMLabs.followPath({ pathId: pathId, frames: parseInt(f.value, 10), ease: ease.value, removeTrajectory: rmCb.checked });
        });
        row.appendChild(gen);
      },
    },
    {
      name: 'reference-fill', label: 'Fill multi-calques — ink & paint',
      build: function (row) {
        var btn = mkBtn('Remplir au curseur');
        btn.title = 'Positionne le curseur sur la zone à remplir, puis clique ce bouton';
        btn.addEventListener('click', function () { window.SMLabs.referenceFillAtPointer(); });
        row.appendChild(btn);
      },
    },
    {
      name: 'vector-trim', label: 'Gomme vectorielle aux intersections',
      build: function (row) {
        var btn = mkBtn('Couper au curseur');
        btn.title = 'Positionne le curseur sur le trait à couper, puis clique ce bouton';
        btn.addEventListener('click', function () { window.SMLabs.trimAtPointer(); });
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
      head.appendChild(mkLabel(spec.label, describeOf(spec.name) || spec.hintExtra));
      row.appendChild(head);
      if (on && spec.extra) spec.extra(row);
      if (on && spec.hintExtra && !spec.extra) {
        var hint = subRow();
        var hs = document.createElement('span'); hs.style.cssText = 'font-size:9px;color:var(--text-dim);'; hs.textContent = spec.hintExtra;
        hint.appendChild(hs); row.appendChild(hint);
      }
      toolsEl.appendChild(row);
    });

    ACTIONS.forEach(function (spec) {
      var row = mkRow();
      var lbl = document.createElement('div');
      lbl.style.cssText = 'font-size:11px;color:var(--text);margin-bottom:3px;';
      lbl.textContent = spec.label;
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
})();
