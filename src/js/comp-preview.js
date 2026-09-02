// ---- FENÊTRE DE PRÉVISUALISATION D'UN COMPOSANT (2026-09) ----
// Cyril : « une fenêtre de préviz sur les images dans un composant avec scrub
// directement sur l'image pour aller d'une image à une autre et des options
// pour ajouter des clés de remappage à ce composant dans cette fenêtre ».
//
// C'est le pendant en GRAND de la bande d'images du panneau : celle-ci
// montre 14 pixels par image, on n'y reconnaît pas un dessin. Ici l'image
// interne est affichée en grand, on la parcourt en GLISSANT dessus (le geste
// du scrub, comme sur un champ numérique — §10), et on pose la clé de
// remappage sans quitter la fenêtre.
//
// Aucun nouveau modèle de données : la clé posée est le `componentFrame` déjà
// lu par resolveSymbolFrameIdx (app.js), point de passage unique de tous les
// lecteurs d'un composant. La fenêtre n'est qu'un éditeur de ce champ.
(function () {
  var POS_KEY = 'nemo-comp-preview-pos';
  var win = null, imgEl = null, capEl = null, keysEl = null, frameEl = null;
  var curInternal = 0;

  function layer() {
    var ld = state.layers[state.activeLayerIdx];
    return (ld && ld.symbolId && state.symbols[ld.symbolId]) ? ld : null;
  }
  function symOf(ld) { return state.symbols[ld.symbolId]; }
  function totalOf(ld) { return Math.max(1, symOf(ld).totalFrames); }

  function savedPos() {
    try { var o = JSON.parse(localStorage.getItem(POS_KEY) || 'null'); return o && typeof o.x === 'number' ? o : null; }
    catch (e) { return null; }
  }
  function savePos(x, y) { try { localStorage.setItem(POS_KEY, JSON.stringify({ x: x, y: y })); } catch (e) {} }

  function build() {
    if (win) return win;
    win = document.createElement('div');
    win.id = 'comp-preview-win';
    win.style.cssText = 'display:none;position:fixed;z-index:320;width:300px;background:var(--panel2);' +
      'border:1px solid var(--border);border-radius:8px;box-shadow:0 8px 28px rgba(0,0,0,.45);' +
      'font-size:10px;color:var(--text);user-select:none;';
    var pos = savedPos() || { x: Math.max(8, window.innerWidth - 340), y: 90 };
    win.style.left = pos.x + 'px'; win.style.top = pos.y + 'px';

    var head = document.createElement('div');
    head.style.cssText = 'display:flex;align-items:center;gap:6px;padding:6px 8px;border-bottom:1px solid var(--border);cursor:move;';
    var title = document.createElement('span');
    title.textContent = SM.t('compPreviewTitle'); title.style.flex = '1';
    var close = document.createElement('button');
    close.className = 'pbtn'; close.textContent = '×'; close.style.cssText = 'padding:1px 7px;font-size:12px';
    close.addEventListener('click', hide);
    head.appendChild(title); head.appendChild(close);
    // Déplacement de la fenêtre par son bandeau, position mémorisée : une
    // fenêtre qui revient toujours au même endroit finit par cacher ce qu'on
    // regarde.
    head.addEventListener('pointerdown', function (e) {
      if (e.target === close) return;
      var sx = e.clientX, sy = e.clientY;
      var r = win.getBoundingClientRect(), ox = r.left, oy = r.top;
      var move = function (ev) {
        var x = Math.max(0, Math.min(window.innerWidth - 60, ox + ev.clientX - sx));
        var y = Math.max(0, Math.min(window.innerHeight - 30, oy + ev.clientY - sy));
        win.style.left = x + 'px'; win.style.top = y + 'px';
      };
      var up = function (ev) {
        document.removeEventListener('pointermove', move);
        document.removeEventListener('pointerup', up);
        var rr = win.getBoundingClientRect(); savePos(rr.left, rr.top);
      };
      document.addEventListener('pointermove', move);
      document.addEventListener('pointerup', up);
      e.preventDefault();
    });

    var body = document.createElement('div');
    body.style.cssText = 'padding:8px;display:flex;flex-direction:column;gap:6px;';

    // L'image, et le scrub DESSUS : c'est la demande centrale.
    var stage = document.createElement('div');
    stage.style.cssText = 'height:180px;border:1px solid var(--border);border-radius:5px;background:var(--bg);' +
      'display:flex;align-items:center;justify-content:center;overflow:hidden;cursor:ew-resize;';
    imgEl = document.createElement('img');
    imgEl.style.cssText = 'max-width:100%;max-height:100%;pointer-events:none;';
    stage.appendChild(imgEl);
    stage.addEventListener('pointerdown', function (e) {
      var ld = layer(); if (!ld) return;
      var total = totalOf(ld);
      var sx = e.clientX, startF = curInternal;
      // Toute la largeur de la vignette parcourt tout le composant : sur un
      // composant court chaque image reste large, sur un long on garde la
      // course complète sous la main. Shift ralentit, comme partout (§10).
      var perPx = total / Math.max(80, stage.clientWidth);
      var move = function (ev) {
        var k = ev.shiftKey ? 0.25 : 1;
        setInternal(Math.round(startF + (ev.clientX - sx) * perPx * k));
      };
      var up = function () {
        document.removeEventListener('pointermove', move);
        document.removeEventListener('pointerup', up);
      };
      document.addEventListener('pointermove', move);
      document.addEventListener('pointerup', up);
      e.preventDefault(); e.stopPropagation();
    });

    capEl = document.createElement('div');
    capEl.style.cssText = 'text-align:center;color:var(--text-dim);font-variant-numeric:tabular-nums;';

    var nav = document.createElement('div');
    nav.style.cssText = 'display:flex;gap:4px;align-items:center;';
    function navBtn(label, fn) {
      var b = document.createElement('button');
      b.className = 'pbtn'; b.textContent = label; b.style.cssText = 'padding:2px 7px;';
      b.addEventListener('click', fn);
      return b;
    }
    nav.appendChild(navBtn('◀', function () { setInternal(curInternal - 1); }));
    nav.appendChild(navBtn('▶', function () { setInternal(curInternal + 1); }));
    var spacer = document.createElement('span'); spacer.style.flex = '1';
    nav.appendChild(spacer);
    frameEl = document.createElement('span');
    frameEl.style.cssText = 'color:var(--text-dim);font-variant-numeric:tabular-nums;';
    nav.appendChild(frameEl);
    nav.appendChild(navBtn('⟨', function () { if (typeof goToFrame === 'function') goToFrame(state.currentFrame - 1); refresh(); }));
    nav.appendChild(navBtn('⟩', function () { if (typeof goToFrame === 'function') goToFrame(state.currentFrame + 1); refresh(); }));

    var acts = document.createElement('div');
    acts.style.cssText = 'display:flex;gap:4px;';
    var addBtn = document.createElement('button');
    addBtn.className = 'pbtn ac'; addBtn.style.flex = '1';
    addBtn.textContent = SM.t('compPreviewAddKey');
    addBtn.addEventListener('click', addKey);
    var delBtn = document.createElement('button');
    delBtn.className = 'pbtn'; delBtn.style.flex = '1';
    delBtn.textContent = SM.t('compPreviewDelKey');
    delBtn.addEventListener('click', removeKey);
    acts.appendChild(addBtn); acts.appendChild(delBtn);

    keysEl = document.createElement('div');
    keysEl.style.cssText = 'display:flex;flex-wrap:wrap;gap:3px;max-height:70px;overflow-y:auto;';

    body.appendChild(stage); body.appendChild(capEl); body.appendChild(nav);
    body.appendChild(acts); body.appendChild(keysEl);
    win.appendChild(head); win.appendChild(body);
    document.body.appendChild(win);
    return win;
  }

  function setInternal(f) {
    var ld = layer(); if (!ld) return;
    curInternal = Math.max(0, Math.min(totalOf(ld) - 1, Math.round(f) || 0));
    refresh();
  }

  // Poser une clé, c'est écrire componentFrame sur l'image COURANTE du projet
  // — en la promouvant en clé si elle n'en était pas une, sinon le remappage
  // n'aurait nulle part où s'inscrire et le bouton ne ferait rien
  // silencieusement.
  function addKey() {
    var ld = layer(); if (!ld) return;
    var f = ld.frames && ld.frames[state.currentFrame];
    if (!f) return;
    if (typeof saveAllLayerFrames === 'function') saveAllLayerFrames();
    if (typeof pushUndoLayers === 'function') pushUndoLayers(true);
    ld.symPlayMode = 'single';
    ld.symSingleFrame = curInternal;
    f.isKeyframe = true;
    f.isInterpolated = false;
    f.componentFrame = curInternal;
    delete f.blankOverride;
    commit();
  }
  function removeKey() {
    var ld = layer(); if (!ld) return;
    var f = ld.frames && ld.frames[state.currentFrame];
    if (!f || f.componentFrame == null) return;
    if (typeof saveAllLayerFrames === 'function') saveAllLayerFrames();
    if (typeof pushUndoLayers === 'function') pushUndoLayers(true);
    delete f.componentFrame;
    commit();
  }
  function commit() {
    if (typeof loadFrame === 'function') loadFrame(state.currentFrame);
    if (typeof updateUI === 'function') updateUI();
    if (window.SMEngineBridge) SMEngineBridge.renderNow();
    refresh();
  }

  function refresh() {
    if (!win || win.style.display === 'none') return;
    var ld = layer();
    if (!ld) { hide(); return; }
    var total = totalOf(ld);
    // 320 px : la fenêtre fait 280 de large, la vignette de StoryBoard (96)
    // y serait floue — c'est tout l'intérêt d'une préviz en grand.
    var url = (window.SMStoryboard && SMStoryboard.thumbDataUrl) ? SMStoryboard.thumbDataUrl(ld.symbolId, curInternal, false, 320) : null;
    imgEl.style.display = url ? '' : 'none';
    if (url) imgEl.src = url;
    capEl.textContent = SM.t('compPreviewInternal') + ' ' + curInternal + ' / ' + (total - 1);
    frameEl.textContent = SM.t('compPreviewProject') + ' ' + state.currentFrame;
    keysEl.innerHTML = '';
    (ld.frames || []).forEach(function (fr, i) {
      if (!fr || !fr.isKeyframe || fr.componentFrame == null) return;
      var chip = document.createElement('button');
      chip.className = 'pbtn';
      chip.style.cssText = 'padding:1px 5px;font-size:9px;' + (i === state.currentFrame ? 'background:var(--accent);' : '');
      chip.textContent = i + '→' + fr.componentFrame;
      chip.addEventListener('click', function () {
        if (typeof goToFrame === 'function') goToFrame(i);
        curInternal = fr.componentFrame;
        refresh();
      });
      keysEl.appendChild(chip);
    });
  }

  function show() {
    var ld = layer();
    if (!ld) { if (window.showToast) showToast(SM.t('compPreviewNeedsInstance')); return; }
    build();
    win.style.display = 'block';
    // On ouvre sur ce qui joue RÉELLEMENT à cet instant, pas sur l'image 0 :
    // la fenêtre sert à corriger un remappage existant aussi souvent qu'à en
    // créer un.
    curInternal = (typeof resolveSymbolFrameIdx === 'function')
      ? resolveSymbolFrameIdx(symOf(ld), ld, state.currentFrame) : 0;
    refresh();
  }
  function hide() { if (win) win.style.display = 'none'; }
  function toggle() { (win && win.style.display !== 'none') ? hide() : show(); }

  window.SMCompPreview = {
    show: show, hide: hide, toggle: toggle, refresh: refresh,
    isOpen: function () { return !!win && win.style.display !== 'none'; },
    setInternal: setInternal,
    internalFrame: function () { return curInternal; },
  };
})();
