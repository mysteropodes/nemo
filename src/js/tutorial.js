// ---- Interactive tutorial ("Découvrir Nemo") ----
// Flash 4/5-style in-app lessons: a spotlight highlights a REAL UI element,
// a tooltip explains what to do, and the step only advances once the user
// actually did it — either a real click on the target (event delegation)
// or a real state change polled straight off the live app state (global
// `state`/`userLayers`, same objects app.js/tools.js/timeline.js mutate
// directly — no separate event bus exists in this codebase, see CLAUDE.md).
// Polling (not hooking into every internal function) keeps this module
// completely decoupled: it never patches or wraps existing app code, so it
// can't introduce the "consumer forgot about a new item type" bug family
// documented in CLAUDE.md §1 — it only ever READS state, never writes it.
(function () {
  var POLL_MS = 220;

  // ---- Module content ------------------------------------------------
  // Each step is one of:
  //   {type:'info', title, body}                         — just a "Suivant" button
  //   {type:'click', target, title, body}                 — real click on `target` (CSS selector)
  //   {type:'state', target, title, body, hint, check}    — check(win) polled until true;
  //                                                          `target` (optional) is only used
  //                                                          to aim the spotlight
  var MODULES = [
    {
      id: 'draw',
      icon: '1',
      title: 'Premier trait',
      desc: 'Le pinceau, les couleurs, dessiner une forme',
      time: '2 min',
      steps: [
        { type: 'info', title: 'Bienvenue dans Nemo', body: 'On va dessiner un premier trait ensemble. À chaque étape, fais vraiment le geste demandé — le tutoriel avance tout seul dès que c\'est fait.' },
        { type: 'click', target: '.tool-btn[data-tool="draw"]', title: 'Choisis le Pinceau', body: 'Clique sur l\'outil Pinceau dans la barre de gauche (raccourci B).' },
        {
          type: 'state', target: '#drawing-canvas', title: 'Dessine un trait', body: 'Clique-glisse sur le canevas blanc pour tracer un trait, comme au crayon.', hint: 'En attente de ton trait…',
          check: function (win) {
            var ul = win.userLayers, st = win.state;
            if (!ul || !st) return false;
            var l = ul[st.activeLayerIdx];
            return !!(l && l.children && l.children.length > (win.__tutStrokeStart || 0));
          },
          before: function (win) { win.__tutStrokeStart = (win.userLayers && win.userLayers[win.state.activeLayerIdx] && win.userLayers[win.state.activeLayerIdx].children.length) || 0; }
        },
        { type: 'click', target: '#stroke-well', title: 'Change la couleur du trait', body: 'Clique le carré de couleur du Trait pour ouvrir le sélecteur, choisis une teinte.' },
        { type: 'info', title: 'Bien joué !', body: 'Tu sais dessiner un trait et changer sa couleur. La suite du chapitre "Dessiner" du guide utilisateur détaille tous les autres outils (plume, formes, gomme…).' }
      ]
    },
    {
      id: 'layers',
      icon: '2',
      title: 'Calques et images-clés',
      desc: 'Ajouter un calque, poser une keyframe',
      time: '2 min',
      steps: [
        { type: 'info', title: 'Calques et timeline', body: 'Un calque contient une série de frames. Une "keyframe" est une frame où tu as vraiment dessiné quelque chose de nouveau — c\'est ce sur quoi l\'interpolation automatique s\'appuie.' },
        {
          type: 'click', target: '#btn-al', title: 'Ajoute un calque', body: 'Clique le bouton "+" en bas du panneau Calques, à gauche de la timeline.'
        },
        {
          type: 'state', title: 'Vérification…', body: 'Le nouveau calque doit apparaître dans la liste.', hint: 'Un instant…',
          check: function (win) { return !!(win.state && win.state.layers && win.state.layers.length >= (win.__tutLayerStart || 2)); },
          before: function (win) { win.__tutLayerStart = ((win.state && win.state.layers && win.state.layers.length) || 1) + 1; }
        },
        {
          // A brand-new layer's frame 0 is already a keyframe by definition
          // (it's the layer's own starting content) — asking for F6 while
          // still sitting on frame 0 would pass instantly without the user
          // pressing anything. Move off frame 0 first so isKeyframe is
          // genuinely false until F6 is pressed for real.
          type: 'state', title: 'Avance de quelques frames', body: 'Clique "Frame suivante" 2 ou 3 fois pour te placer plus loin dans la timeline.', hint: 'En attente…',
          check: function (win) { return !!(win.state && win.state.currentFrame >= (win.__tutFrameStart || 2)); },
          before: function (win) { win.__tutFrameStart = ((win.state && win.state.currentFrame) || 0) + 2; }
        },
        {
          type: 'state', title: 'Insère une image-clé', body: 'Appuie sur la touche F6 de ton clavier pour transformer la frame actuelle en keyframe.', hint: 'En attente de F6…',
          check: function (win) {
            var st = win.state; if (!st || !st.layers) return false;
            var ld = st.layers[st.activeLayerIdx];
            var fr = ld && ld.frames && ld.frames[st.currentFrame];
            return !!(fr && fr.isKeyframe);
          }
        },
        { type: 'info', title: 'Bien joué !', body: 'Tu as un nouveau calque et une keyframe posée. Le chapitre "Calques et timeline" du guide couvre F5/F7, le drag & drop de frames, et le clic-droit sur la timeline.' }
      ]
    },
    {
      id: 'tween',
      icon: '3',
      title: 'Interpolation automatique',
      desc: 'Deux keyframes, un tween généré tout seul',
      time: '3 min',
      steps: [
        { type: 'info', title: 'Le tween automatique', body: 'Pose deux keyframes avec un dessin différent, puis laisse Nemo générer les frames intermédiaires tout seul. C\'est le cœur de Nemo.' },
        {
          type: 'state', title: 'Avance de quelques frames', body: 'Clique "Frame suivante" plusieurs fois (ou glisse le curseur) pour te placer 5 à 10 frames plus loin.', hint: 'En attente…',
          check: function (win) { return !!(win.state && win.state.currentFrame >= (win.__tutTweenStartFrame || 5)); },
          before: function (win) { win.__tutTweenStartFrame = ((win.state && win.state.currentFrame) || 0) + 5; }
        },
        {
          type: 'state', target: '#drawing-canvas', title: 'Dessine un second trait, différent du premier', body: 'Même geste qu\'au premier module — mais dessine autre chose, pour que le tween ait quelque chose à interpoler.',
          hint: 'En attente de ton trait…',
          check: function (win) {
            var ul = win.userLayers, st = win.state;
            if (!ul || !st) return false;
            var l = ul[st.activeLayerIdx];
            return !!(l && l.children && l.children.length > 0);
          }
        },
        {
          type: 'state', title: 'Pose la seconde keyframe', body: 'Appuie sur F6 pour figer ce dessin comme keyframe.', hint: 'En attente de F6…',
          check: function (win) {
            var st = win.state; if (!st || !st.layers) return false;
            var ld = st.layers[st.activeLayerIdx];
            var fr = ld && ld.frames && ld.frames[st.currentFrame];
            return !!(fr && fr.isKeyframe);
          }
        },
        {
          type: 'state', title: 'Lance le tween', body: 'Appuie sur la touche T pour interpoler automatiquement entre les deux keyframes.', hint: 'En attente de T…',
          check: function (win) {
            var st = win.state; if (!st || !st.layers) return false;
            var ld = st.layers[st.activeLayerIdx];
            if (!ld || !ld.frames) return false;
            for (var i = 0; i < ld.frames.length; i++) if (ld.frames[i] && ld.frames[i].isInterpolated) return true;
            return false;
          }
        },
        { type: 'info', title: 'Bravo, premier tween !', body: 'Regarde la timeline : les frames entre tes deux keyframes sont maintenant interpolées. Le chapitre "Interpolation automatique" du guide couvre l\'éditeur de courbes et l\'onion skin.' }
      ]
    }
  ];

  // ---- Progress persistence ------------------------------------------
  function loadDone() { try { return JSON.parse(localStorage.getItem('nemo-tutorial-done') || '[]'); } catch (e) { return []; } }
  function markDone(id) {
    try {
      var d = loadDone();
      if (d.indexOf(id) === -1) { d.push(id); localStorage.setItem('nemo-tutorial-done', JSON.stringify(d)); }
    } catch (e) {}
  }

  // ---- Runtime state ----------------------------------------------------
  var active = null; // {module, stepIdx, pollTimer, clickHandler}

  function $(sel) { return document.querySelector(sel); }

  function ensureDom() {
    if ($('#tut-spotlight')) return;
    var sp = document.createElement('div'); sp.id = 'tut-spotlight'; document.body.appendChild(sp);
    var tt = document.createElement('div'); tt.id = 'tut-tooltip'; document.body.appendChild(tt);
  }

  function stopStepListeners() {
    if (active && active.pollTimer) { clearInterval(active.pollTimer); active.pollTimer = null; }
    if (active && active.clickHandler) { document.removeEventListener('click', active.clickHandler, true); active.clickHandler = null; }
  }

  function positionFor(target) {
    var el = target ? $(target) : null;
    if (!el) return null;
    var r = el.getBoundingClientRect();
    return r;
  }

  function renderStep() {
    stopStepListeners();
    var mod = active.module, step = mod.steps[active.stepIdx];
    if (step.before) { try { step.before(window); } catch (e) {} }

    var sp = $('#tut-spotlight'), tt = $('#tut-tooltip');
    var rect = positionFor(step.target);

    if (rect) {
      sp.classList.remove('no-target');
      sp.style.top = (rect.top - 6) + 'px';
      sp.style.left = (rect.left - 6) + 'px';
      sp.style.width = (rect.width + 12) + 'px';
      sp.style.height = (rect.height + 12) + 'px';
    } else {
      sp.classList.add('no-target');
      sp.style.top = '40%'; sp.style.left = '50%'; sp.style.width = '0px'; sp.style.height = '0px';
    }
    sp.classList.add('on');

    var isLast = active.stepIdx === mod.steps.length - 1;
    tt.innerHTML =
      '<button class="tut-close" title="Quitter le tutoriel">&times;</button>' +
      '<div class="tut-progress">' + mod.title + ' — étape ' + (active.stepIdx + 1) + '/' + mod.steps.length + '</div>' +
      '<div class="tut-title">' + step.title + '</div>' +
      '<div class="tut-body">' + step.body + '</div>' +
      (step.type !== 'info' ? '<div class="tut-hint"><span class="tut-dot"></span>' + (step.hint || 'À toi de jouer…') + '</div>' : '') +
      '<div class="tut-actions">' +
      '<button class="tut-skip">Passer ce module</button>' +
      (step.type === 'info' ? '<button class="tut-next">' + (isLast ? 'Terminer' : 'Suivant') + '</button>' : '') +
      '</div>';

    // Position the tooltip near the spotlight (or centered if none)
    tt.classList.add('on');
    requestAnimationFrame(function () {
      var tw = tt.offsetWidth, th = tt.offsetHeight;
      var top, left;
      if (rect) {
        var spaceBelow = window.innerHeight - rect.bottom;
        if (spaceBelow > th + 24) { top = rect.bottom + 16; } else { top = Math.max(12, rect.top - th - 16); }
        left = Math.min(window.innerWidth - tw - 16, Math.max(16, rect.left));
      } else {
        top = window.innerHeight / 2 - th / 2;
        left = window.innerWidth / 2 - tw / 2;
      }
      tt.style.top = top + 'px'; tt.style.left = left + 'px';
    });

    tt.querySelector('.tut-close').addEventListener('click', stopTutorial);
    tt.querySelector('.tut-skip').addEventListener('click', function () { finishModule(false); });
    var nextBtn = tt.querySelector('.tut-next');
    if (nextBtn) nextBtn.addEventListener('click', function () { advance(); });

    if (step.type === 'click') {
      active.clickHandler = function (e) {
        var t = e.target.closest && e.target.closest(step.target);
        if (t) advance();
      };
      document.addEventListener('click', active.clickHandler, true);
    } else if (step.type === 'state') {
      active.pollTimer = setInterval(function () {
        try { if (step.check(window)) advance(); } catch (e) {}
      }, POLL_MS);
    }
  }

  function advance() {
    if (!active) return;
    var mod = active.module;
    if (active.stepIdx >= mod.steps.length - 1) { finishModule(true); return; }
    active.stepIdx++;
    renderStep();
  }

  function finishModule(completed) {
    stopStepListeners();
    var mod = active && active.module;
    active = null;
    var sp = $('#tut-spotlight'), tt = $('#tut-tooltip');
    if (sp) sp.classList.remove('on');
    if (tt) tt.classList.remove('on');
    if (mod && completed) {
      markDone(mod.id);
      if (window.showToast) showToast('Module "' + mod.title + '" terminé ✓');
    }
  }

  function stopTutorial() { finishModule(false); }

  function startModule(id) {
    var mod = MODULES.filter(function (m) { return m.id === id; })[0];
    if (!mod) return;
    // The lessons target real toolbar/canvas elements — those are only
    // reachable once a project is open (the start screen sits on top and
    // intercepts clicks, even though the editor DOM already exists
    // underneath). Launching a lesson straight from the start screen's own
    // row must not dead-end there, so spin up a default blank project first.
    var startScreen = document.getElementById('start-screen');
    if (startScreen && !startScreen.classList.contains('hid') && window.SMProject && window.SMProject.newProject) {
      window.SMProject.newProject({ w: 1920, h: 1080, fps: 24, name: 'Tutoriel' });
    }
    ensureDom();
    closeLauncher();
    active = { module: mod, stepIdx: 0 };
    renderStep();
  }

  // ---- Module launcher modal -------------------------------------------
  function ensureLauncher() {
    if ($('#tut-launcher')) return $('#tut-launcher');
    var modal = document.createElement('div');
    modal.id = 'tut-launcher';
    modal.className = 'modal-overlay';
    modal.style.display = 'none';
    modal.innerHTML =
      '<div class="modal-box">' +
      '<div class="modal-hdr"><span>Découvrir Nemo</span> <button class="modal-x" id="tut-launcher-close">&times;</button></div>' +
      '<div class="modal-bdy">' +
      '<div style="font-size:11px;color:var(--text-dim);margin-bottom:12px">Des mini-leçons pas à pas, directement dans l\'app — comme les tutoriels intégrés de Flash. Choisis un module ; tu peux quitter à tout moment.</div>' +
      '<div class="tut-mod-list" id="tut-mod-list"></div>' +
      '</div></div>';
    document.body.appendChild(modal);
    modal.querySelector('#tut-launcher-close').addEventListener('click', closeLauncher);
    modal.addEventListener('mousedown', function (e) { if (e.target === modal) closeLauncher(); });
    return modal;
  }

  function renderLauncherList() {
    var list = $('#tut-mod-list'); if (!list) return;
    var done = loadDone();
    list.innerHTML = '';
    MODULES.forEach(function (m) {
      var isDone = done.indexOf(m.id) !== -1;
      var btn = document.createElement('button');
      btn.className = 'tut-mod' + (isDone ? ' done' : '');
      btn.innerHTML =
        '<span class="tut-mod-ico">' + (isDone ? '✓' : m.icon) + '</span>' +
        '<span class="tut-mod-body">' +
        '<span class="tut-mod-title">' + m.title + '</span>' +
        '<span class="tut-mod-desc">' + m.desc + '</span>' +
        '</span>' +
        '<span class="tut-mod-time">' + m.time + '</span>';
      btn.addEventListener('click', function () { startModule(m.id); });
      list.appendChild(btn);
    });
  }

  function openLauncher() {
    ensureLauncher();
    renderLauncherList();
    $('#tut-launcher').style.display = 'flex';
  }
  function closeLauncher() { var m = $('#tut-launcher'); if (m) m.style.display = 'none'; }

  // ---- Entry point wiring ----------------------------------------------
  document.addEventListener('DOMContentLoaded', function () {
    ['btn-open-tutorial', 'btn-open-tutorial-topbar'].forEach(function (id) {
      var btn = document.getElementById(id);
      if (btn) btn.addEventListener('click', openLauncher);
    });
    document.addEventListener('keydown', function (e) {
      if (e.key === 'Escape' && active) stopTutorial();
    });
  });

  window.SMTutorial = { open: openLauncher, start: startModule, stop: stopTutorial, MODULES: MODULES };
})();
