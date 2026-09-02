// ---- PANNEAU DU SUIVI DE MOUVEMENT (2026-09) ----
// Le pilotage de tracker.js : poser un point, lancer l'analyse, appliquer le
// résultat à un calque. Le noyau (Lucas-Kanade pyramidal) est en Rust, il
// n'est pas question ici.
//
// Deux règles reprises telles quelles des chantiers précédents, parce qu'elles
// ont chacune coûté un bug :
// - le marqueur est un élément DOM frère du canevas, JAMAIS un item Paper :
//   un item vivant finit dans le document (§1) ;
// - la capture du clic écoute sur `document` en phase de CAPTURE et non sur
//   #canvas-area, où motion.js est déjà enregistré et gagnerait (§13).
(function () {
  var _picking = false;
  var _result = null;

  function activeLayer() {
    var ld = state.layers[state.activeLayerIdx];
    if (!ld || !window.SMLayerKind) return null;
    var k = SMLayerKind.of(ld);
    return (k && (k.key === 'video' || k.key === 'sequence' || k.key === 'image')) ? ld : null;
  }
  function el(id) { return document.getElementById(id); }
  function status(msg) { var s = el('track-status'); if (s) s.textContent = msg || ''; }

  // Monde -> écran, par la même voie que le badge de réassignation de tween
  // (tweens.js) : le canevas donne son rect, Paper donne la projection.
  function placeMarker(world) {
    var m = el('track-marker');
    if (!m) return;
    var ld = activeLayer();
    if (!ld || !world || !window.view) { m.style.display = 'none'; return; }
    var canvas = document.getElementById('drawing-canvas');
    if (!canvas) { m.style.display = 'none'; return; }
    var rect = canvas.getBoundingClientRect();
    var p = view.projectToView(new Point(world[0], world[1]));
    m.style.display = 'block';
    m.style.left = (rect.left + p.x) + 'px';
    m.style.top = (rect.top + p.y) + 'px';
  }
  function currentPoint() {
    var x = parseFloat(el('track-x') && el('track-x').value);
    var y = parseFloat(el('track-y') && el('track-y').value);
    return (isFinite(x) && isFinite(y)) ? [x, y] : null;
  }
  // Après une analyse, le marqueur suit la position TROUVÉE à l'image
  // courante : c'est la seule façon de voir d'un coup d'œil si le suivi a
  // décroché, sans relire une liste de nombres.
  function markerForFrame() {
    if (_result && _result.length) {
      for (var i = 0; i < _result.length; i++) {
        if (_result[i].frame === state.currentFrame && _result[i].world) return _result[i].world;
      }
    }
    return currentPoint();
  }

  function startPicking() {
    if (_picking) return;
    _picking = true;
    status(SM.t('trackPicking'));
    var onDown = function (e) {
      var canvas = document.getElementById('drawing-canvas');
      if (!canvas) return;
      var r = canvas.getBoundingClientRect();
      if (e.clientX < r.left || e.clientX > r.right || e.clientY < r.top || e.clientY > r.bottom) return;
      e.preventDefault(); e.stopPropagation();
      document.removeEventListener('pointerdown', onDown, true);
      _picking = false;
      var w = (window.SMEngineBridge && SMEngineBridge.isEnabled() && SMEngineBridge.screenToWorld)
        ? SMEngineBridge.screenToWorld(e.clientX, e.clientY)
        : (function () { var p = view.viewToProject(new Point(e.clientX - r.left, e.clientY - r.top)); return [p.x, p.y]; })();
      if (el('track-x')) el('track-x').value = Math.round(w[0]);
      if (el('track-y')) el('track-y').value = Math.round(w[1]);
      _result = null;
      if (el('track-apply-row')) el('track-apply-row').style.display = 'none';
      status('');
      placeMarker([w[0], w[1]]);
    };
    document.addEventListener('pointerdown', onDown, true);
  }

  async function run() {
    var ld = activeLayer();
    if (!ld) return;
    if (!window.SMTracker || !SMTracker.available()) { status(SM.t('trackNoWasm')); return; }
    var pt = currentPoint();
    if (!pt) return;
    var start = Math.max(0, parseInt(el('track-start').value) || 0);
    var end = Math.min(state.totalFrames - 1, parseInt(el('track-end').value) || 0);
    status('…');
    try {
      var res = await SMTracker.trackLayer(state.activeLayerIdx, {
        start: start, end: end, world: pt,
        onProgress: function (f, last) { status(f + ' / ' + last); },
      });
      _result = res;
      var lost = res.filter(function (r) { return !r.ok; })[0];
      status(lost
        ? (SM.t('trackLost') + ' ' + lost.frame)
        : (SM.t('trackDone') + ' : ' + res.length));
      fillTargets();
      el('track-apply-row').style.display = res.length > 1 ? 'flex' : 'none';
      placeMarker(markerForFrame());
    } catch (e) {
      status(String(e && e.message ? e.message : e));
    }
  }

  function fillTargets() {
    var sel = el('track-target');
    if (!sel) return;
    var keep = sel.value;
    sel.innerHTML = '';
    state.layers.forEach(function (l, i) {
      var o = document.createElement('option');
      o.value = String(i);
      o.textContent = l.name || ('Layer ' + (i + 1));
      sel.appendChild(o);
    });
    if (keep && sel.querySelector('option[value="' + keep + '"]')) sel.value = keep;
  }

  function apply() {
    if (!_result || _result.length < 2) return;
    var sel = el('track-target');
    var idx = parseInt(sel && sel.value);
    if (!isFinite(idx) || !state.layers[idx]) { status('—'); return; }
    var out = SMTracker.applyToLayer(idx, _result);
    status(SM.t('trackDone') + ' : ' + out.written);
  }

  function render() {
    var sec = el('footage-track-sec');
    if (!sec) return;
    var ld = activeLayer();
    sec.style.display = ld ? 'flex' : 'none';
    if (!ld) { var m = el('track-marker'); if (m) m.style.display = 'none'; return; }
    var sx = el('track-start'), ex = el('track-end');
    if (sx && !sx._touched) sx.value = 0;
    if (ex && !ex._touched) ex.value = Math.max(0, state.totalFrames - 1);
    // Le champ part à "0" dans le HTML, et "0" est une chaîne NON VIDE donc
    // vraie : tester la valeur laisserait le point à l'origine, hors du
    // média. D'où un drapeau explicite plutôt qu'un test de vérité.
    var xEl = el('track-x');
    if (xEl && !xEl._defaulted) {
      xEl._defaulted = true;
      xEl.value = Math.round((state.canvasW || 1920) / 2);
      el('track-y').value = Math.round((state.canvasH || 1080) / 2);
    }
    // La liste des cibles est reconstruite à CHAQUE rendu, pas seulement après
    // une analyse : sinon un calque créé entre l'analyse et l'application n'y
    // figure pas, la sélection retombe sur rien et le bouton n'écrit aucune
    // clé sans rien dire.
    fillTargets();
    placeMarker(markerForFrame());
  }

  function wire() {
    var pick = el('btn-track-pick'), runBtn = el('btn-track-run'), applyBtn = el('btn-track-apply');
    if (pick) pick.addEventListener('click', startPicking);
    if (runBtn) runBtn.addEventListener('click', run);
    if (applyBtn) applyBtn.addEventListener('click', apply);
    ['track-start', 'track-end'].forEach(function (id) {
      var e = el(id);
      if (e) e.addEventListener('input', function () { e._touched = true; });
    });
    ['track-x', 'track-y'].forEach(function (id) {
      var e = el(id);
      if (e) e.addEventListener('input', function () { _result = null; placeMarker(currentPoint()); });
    });
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', wire);
  else wire();

  window.SMTrackerPanel = { render: render, placeMarker: placeMarker, result: function () { return _result; } };
})();
