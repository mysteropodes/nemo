// Tutorial UI runtime only: one placement/transition owner for steps and launcher.
// The caller supplies the unchanged executable lesson catalog, translator and poll interval.
(function () {
  function create(MODULES, T, POLL_MS) {
    function loadDone() { try { return JSON.parse(localStorage.getItem('nemo-tutorial-done') || '[]'); } catch (e) { return []; } }
    function markDone(id) {
      try {
        var d = loadDone();
        if (d.indexOf(id) === -1) { d.push(id); localStorage.setItem('nemo-tutorial-done', JSON.stringify(d)); }
      } catch (e) {}
    }

    // "Seen" (feedback-welcome auto-launch) is deliberately a SEPARATE flag
    // from nemo-tutorial-done above: skipping the auto-launched module still
    // counts as seen (never auto-fires twice), whereas nemo-tutorial-done
    // only flips on actually finishing all its steps — the launcher's own
    // ✓ badge should keep reflecting real completion, not just exposure.
    var FIRST_LAUNCH_SEEN_KEY = 'nemo-tut-feedback-shown';
    function markFirstLaunchSeen() { try { localStorage.setItem(FIRST_LAUNCH_SEEN_KEY, '1'); } catch (e) {} }
    function isFirstLaunchSeen() { try { return !!localStorage.getItem(FIRST_LAUNCH_SEEN_KEY); } catch (e) { return true; } }

    // ---- Runtime state ----------------------------------------------------
    var active = null; // {module, stepIdx, pollTimer, clickHandler}
    var placement = null, transition = 0, transitioning = false;

    function $(sel) { return document.querySelector(sel); }

    function ensureDom() {
      if ($('#tut-spotlight')) return;
      var sp = document.createElement('div'); sp.id = 'tut-spotlight'; document.body.appendChild(sp);
      var tt = document.createElement('div'); tt.id = 'tut-tooltip'; document.body.appendChild(tt);
      placement = window.NemoTutorialNativePlacement.create(window, stopTutorial);
    }

    function stopStepListeners() {
      if (active && active.pollTimer) { clearInterval(active.pollTimer); active.pollTimer = null; }
      if (active && active.clickHandler) { document.removeEventListener('click', active.clickHandler, true); active.clickHandler = null; }
    }

    // Shape-tool group (timeline.js, 2026-08): rect/line/ellipse/speechbubble/
    // star share one stacked toolbar slot, only the "front" one clickable —
    // front it first so a step targeting e.g. '[data-tool="ellipse"]' both
    // spotlights the right place AND is actually clickable by the user.
    function frontShapeToolIfNeeded(target) {
      if (!target || !window.SMShapeGroup) return;
      var m = /data-tool="([a-z]+)"/.exec(target);
      if (m) window.SMShapeGroup.ensureFront(m[1]);
    }

    function positionFor(target) {
      frontShapeToolIfNeeded(target);
      var el = target ? $(target) : null;
      if (!el) return null;
      var r = el.getBoundingClientRect();
      return r;
    }

    function renderStep() {
      stopStepListeners();
      var mod = active.module, step = mod.steps[active.stepIdx];
      if (window.__TAURI__ && window.__TAURI__.core && mod.id === 'perspective' && step === mod.steps[2]) {
        var run = active, index = active.stepIdx;
        step = window.NemoTutorialNativePlacement.perspectiveStep(window, step, function () {
          return active === run && active.stepIdx === index && active.module === mod;
        });
      }
      if (step.before) { try { step.before(window); } catch (e) {} }

      var sp = $('#tut-spotlight'), tt = $('#tut-tooltip');
      function targetRect() {
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
        return rect;
      }
      targetRect();
      sp.classList.add('on');

      var isLast = active.stepIdx === mod.steps.length - 1;
      tt.innerHTML =
        '<button class="tut-close" title="' + T('Quitter le tutoriel') + '">&times;</button>' +
        '<div class="tut-progress">' + T(mod.title) + ' — ' + T('étape') + ' ' + (active.stepIdx + 1) + '/' + mod.steps.length + '</div>' +
        '<div class="tut-title">' + T(step.title) + '</div>' +
        // Illustration is raw SVG, deliberately NOT run through T() — it's
        // markup, not a translatable string, and every one of these is
        // language-agnostic (shapes/motion, no text baked in).
        (step.illustration ? '<div class="tut-illus-wrap">' + step.illustration + '</div>' : '') +
        '<div class="tut-body">' + T(step.body) + '</div>' +
        (step.type !== 'info' ? '<div class="tut-hint"><span class="tut-dot"></span>' + T(step.hint || 'À toi de jouer…') + '</div>' : '') +
        '<div class="tut-actions">' +
        '<button class="tut-skip">' + T('Passer ce module') + '</button>' +
        (step.type === 'info' ? '<button class="tut-next">' + T(mod.unavailable ? 'Fermer' : isLast ? 'Terminer' : 'Suivant') + '</button>' : '') +
        '</div>';

      tt.classList.add('on');

      tt.querySelector('.tut-close').addEventListener('click', stopTutorial);
      tt.querySelector('.tut-skip').addEventListener('click', function () { finishModule(false); });
      var nextBtn = tt.querySelector('.tut-next');
      if (nextBtn) nextBtn.addEventListener('click', function () { advance(); });

      if (step.type === 'click') {
        active.clickHandler = function (e) {
          var t = e.target.closest && e.target.closest(step.target);
          if (t && placement.ready()) advance();
        };
        document.addEventListener('click', active.clickHandler, true);
      } else if (step.type === 'state') {
        active.pollTimer = setInterval(function () {
          try { if (placement.ready() && step.check(window)) advance(); } catch (e) {}
        }, POLL_MS);
      }
      placement.place(tt, targetRect, step.pinCorner);
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
      var released = placement ? placement.close() : Promise.resolve(true);
      if (mod && completed && !mod.unavailable) {
        markDone(mod.id);
        if (window.showToast) showToast(T(SM.t('toastModuleDoneSuffix')).replace('%s', T(mod.title)));
      }
      return released;
    }

    function stopTutorial() { ++transition; transitioning = false; hideLauncher(); return finishModule(false); }

    async function startModule(id) {
      var mod = MODULES.filter(function (m) { return m.id === id; })[0];
      if (!mod) return;
      var request = ++transition; transitioning = true;
      // The lessons target real toolbar/canvas elements — those are only
      // reachable once a project is open (the start screen sits on top and
      // intercepts clicks, even though the editor DOM already exists
      // underneath). Launching a lesson straight from the start screen's own
      // row must not dead-end there, so spin up a default blank project first.
      var startScreen = document.getElementById('start-screen');
      if (!mod.unavailable && startScreen && !startScreen.classList.contains('hid') && window.SMProject && window.SMProject.newProject) {
        try { await window.SMProject.newProject({ w: 1920, h: 1080, fps: 24, name: 'Tutoriel' }); }
        catch (error) { if (request === transition) transitioning = false; throw error; }
      }
      if (request !== transition) return;
      hideLauncher(); var released = await finishModule(false);
      if (request !== transition) return;
      transitioning = false; if (released === false) return;
      ensureDom();
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
        '<div class="modal-hdr">' +
        '<button class="tut-launcher-back" id="tut-launcher-back" style="display:none" title="' + T('Retour') + '"><svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><path d="M15 6l-6 6 6 6"/></svg></button>' +
        '<span id="tut-launcher-title">' + T('Découvrir Nemo') + '</span> <button class="modal-x" id="tut-launcher-close">&times;</button></div>' +
        '<div class="tut-launcher-search-wrap">' +
        '<svg class="tut-launcher-search-ico" viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="11" cy="11" r="7"/><path d="M21 21l-4.35-4.35"/></svg>' +
        '<input type="text" id="tut-launcher-search" class="tut-launcher-search" placeholder="' + T('Rechercher un module…') + '">' +
        '</div>' +
        '<div class="modal-bdy">' +
        '<div class="tut-mod-list" id="tut-mod-list"></div>' +
        '</div>' +
        '<div class="tut-ask-wrap">' +
        '<div class="tut-ask-answer" id="tut-ask-answer" style="display:none"></div>' +
        '<div class="tut-ask-row">' +
        '<input type="text" id="tut-ask-input" class="tut-ask-input" placeholder="' + T('Une question sur Nemo ?') + '">' +
        '<button class="tut-ask-btn" id="tut-ask-btn" title="' + T('Chercher une réponse') + '"><svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M5 12h14M13 6l6 6-6 6"/></svg></button>' +
        '</div></div>' +
        '</div>';
      document.body.appendChild(modal);
      modal.querySelector('#tut-launcher-close').addEventListener('click', closeLauncher);
      modal.querySelector('#tut-launcher-back').addEventListener('click', function () { renderCategoryScreen(); });
      modal.querySelector('#tut-launcher-search').addEventListener('input', onSearchInput);
      modal.querySelector('#tut-ask-btn').addEventListener('click', runAsk);
      modal.querySelector('#tut-ask-input').addEventListener('keydown', function (e) { if (e.key === 'Enter') runAsk(); });
      modal.addEventListener('mousedown', function (e) { if (e.target === modal) closeLauncher(); });
      return modal;
    }

    // Categories render in this fixed order (not alphabetical, not
    // MODULES-array order) — roughly the order someone actually learning
    // Nemo would want: draw first, then animate, then organize/output.
    var CATEGORY_ORDER = ['Vue d\'ensemble', 'Dessiner', 'Calques et animation', 'Principes d\'animation', 'Organisation', 'Médias', 'Réglages'];

    // A real two-screen menu, not an inline accordion: the launcher opens on
    // a list of CATEGORIES only (icon + count, no modules shown yet);
    // clicking one navigates into a second screen listing just that
    // category's modules, with a back arrow (in the modal header) to return
    // to the category list. Matches how a phone Settings app drills down,
    // per explicit request — a first version that expanded categories inline
    // in the same list wasn't what was asked for.
    function groupByCategory() {
      var byCat = {};
      MODULES.forEach(function (m) {
        var cat = m.category || 'Autres';
        (byCat[cat] || (byCat[cat] = [])).push(m);
      });
      var cats = CATEGORY_ORDER.filter(function (c) { return byCat[c]; })
        .concat(Object.keys(byCat).filter(function (c) { return CATEGORY_ORDER.indexOf(c) === -1; }));
      return { byCat: byCat, cats: cats };
    }

    // Tracks which screen to return to once the search box is cleared —
    // null means the top-level category list, a category name means that
    // category's module screen. Only set by the two NORMAL render paths
    // below (never by renderSearchResults itself), so clearing an active
    // search always restores wherever the user actually was.
    var lastScreenCat = null;

    function renderCategoryScreen() {
      lastScreenCat = null;
      var si = $('#tut-launcher-search'); if (si) si.placeholder = T('Rechercher un module…');
      var ai = $('#tut-ask-input'); if (ai) ai.placeholder = T('Une question sur Nemo ?');
      var list = $('#tut-mod-list'); if (!list) return;
      $('#tut-launcher-title').textContent = T('Découvrir Nemo');
      $('#tut-launcher-back').style.display = 'none';
      $('#tut-launcher-back').title = T('Retour');
      var done = loadDone();
      var g = groupByCategory();
      list.innerHTML = '';
      var intro = document.createElement('div');
      intro.className = 'tut-launcher-intro';
      intro.textContent = T('Des mini-leçons pas à pas, directement dans l\'app, sans jamais quitter ton projet. Choisis une catégorie, puis un module ; tu peux quitter à tout moment.');
      list.appendChild(intro);
      g.cats.forEach(function (cat) {
        var mods = g.byCat[cat];
        var available = mods.filter(function (m) { return !m.unavailable; });
        var doneCount = available.filter(function (m) { return done.indexOf(m.id) !== -1; }).length;
        var btn = document.createElement('button');
        btn.className = 'tut-cat-hdr';
        btn.type = 'button';
        btn.innerHTML =
          '<span class="tut-cat-name">' + T(cat) + '</span>' +
          '<span class="tut-cat-count">' + doneCount + '/' + available.length + '</span>' +
          '<span class="tut-cat-chevron">' + chevronSvg() + '</span>';
        btn.addEventListener('click', function () { renderModuleScreen(cat); });
        list.appendChild(btn);
      });
    }

    function renderModuleScreen(cat) {
      lastScreenCat = cat;
      var list = $('#tut-mod-list'); if (!list) return;
      var g = groupByCategory();
      var mods = g.byCat[cat] || [];
      var done = loadDone();
      $('#tut-launcher-title').textContent = T(cat);
      $('#tut-launcher-back').style.display = 'flex';
      $('#tut-launcher-back').title = T('Retour');
      list.innerHTML = '';
      renderModuleRows(list, mods, done, false);
    }

    // Shared row-rendering for both a category's module list and search
    // results — search results additionally show which category each hit
    // belongs to, since it flattens across all of them.
    function renderModuleRows(list, mods, done, showCategory) {
      mods.forEach(function (m) {
        var isDone = !m.unavailable && done.indexOf(m.id) !== -1;
        var btn = document.createElement('button');
        btn.className = 'tut-mod' + (isDone ? ' done' : '');
        btn.innerHTML =
          '<span class="tut-mod-ico">' + (isDone ? '✓' : m.icon) + '</span>' +
          '<span class="tut-mod-body">' +
          '<span class="tut-mod-title">' + T(m.title) + '</span>' +
          '<span class="tut-mod-desc">' + T(m.desc) + (showCategory ? ' · ' + T(m.category) : '') + '</span>' +
          '</span>' +
          '<span class="tut-mod-time">' + T(m.time) + '</span>';
        btn.addEventListener('click', function () { startModule(m.id); });
        list.appendChild(btn);
      });
    }

    // Flattens across every category — matches against both the currently
    // displayed language's title/description AND the raw French source (so
    // a search still works before a translation exists for a given string,
    // or if the user is typing in a different language than the UI).
    function renderSearchResults(query) {
      var list = $('#tut-mod-list'); if (!list) return;
      $('#tut-launcher-title').textContent = T('Découvrir Nemo');
      $('#tut-launcher-back').style.display = 'none';
      var done = loadDone();
      var q = query.trim().toLowerCase();
      var results = MODULES.filter(function (m) {
        var hay = (T(m.title) + ' ' + T(m.desc) + ' ' + m.title + ' ' + m.desc).toLowerCase();
        return hay.indexOf(q) !== -1;
      });
      list.innerHTML = '';
      if (!results.length) {
        var empty = document.createElement('div');
        empty.className = 'tut-launcher-intro';
        empty.textContent = T('Aucun module ne correspond à ta recherche.');
        list.appendChild(empty);
        return;
      }
      renderModuleRows(list, results, done, true);
    }

    function onSearchInput(e) {
      var q = e.target.value;
      if (q.trim()) { renderSearchResults(q); }
      else if (lastScreenCat) { renderModuleScreen(lastScreenCat); }
      else { renderCategoryScreen(); }
    }

    // ---- "Une question ?" — keyword search over the tutorial's own content,
    // NOT an LLM: every module step's title/body is already a hand-written,
    // accurate explanation of one specific piece of the app, so scoring
    // keyword overlap against that corpus and surfacing the best-matching
    // step is a cheap, offline, zero-cost way to answer "how do I..." /
    // "what is..." questions without needing a network call or an API key
    // embedded in a distributed binary (see CLAUDE.md §6 on why a bundled
    // secret is a real risk — the same reasoning would apply to a bundled
    // LLM key). Explicitly scoped small: no fuzzy matching, no synonyms,
    // just token overlap — good enough for a "am I in the right module?"
    // nudge, not a real conversation.
    var QA_STOPWORDS = {
      'les': 1, 'des': 1, 'une': 1, 'un': 1, 'le': 1, 'la': 1, 'du': 1, 'de': 1, 'et': 1, 'ou': 1,
      'est': 1, 'sont': 1, 'pour': 1, 'avec': 1, 'dans': 1, 'sur': 1, 'que': 1, 'qui': 1, 'ce': 1,
      'ces': 1, 'mon': 1, 'ma': 1, 'mes': 1, 'ton': 1, 'ta': 1, 'tes': 1, 'comment': 1, 'pourquoi': 1,
      'quoi': 1, 'quel': 1, 'quelle': 1, 'faire': 1, 'fais': 1, 'peux': 1, 'veux': 1, 'nemo': 1,
      'the': 1, 'and': 1, 'for': 1, 'with': 1, 'how': 1, 'what': 1, 'does': 1, 'can': 1, 'you': 1
    };
    // Strips accents (NFD-decompose, drop the combining marks) so "opacité"
    // and a query typed as "opacite" (no accent — common when typing fast,
    // or on a non-French keyboard) match each other. MUST be applied to
    // BOTH the query tokens AND the corpus text being searched — found live
    // in testing: applying it only to the query left every accented corpus
    // word (most of them, this content is French) permanently unmatchable
    // against its own unaccented query form.
    function qaStrip(str) { return (str || '').toLowerCase().normalize('NFD').replace(/[̀-ͯ]/g, ''); }
    function qaTokenize(str) {
      return qaStrip(str).split(/[^a-z0-9]+/).filter(function (t) { return t.length > 2 && !QA_STOPWORDS[t]; });
    }
    function buildQACorpus() {
      var entries = [];
      MODULES.forEach(function (m) {
        m.steps.forEach(function (s) {
          if (!s.body) return;
          entries.push({ moduleId: m.id, moduleTitle: m.title, title: s.title || m.title, body: s.body });
        });
      });
      return entries;
    }
    function answerQuestion(query) {
      var qTokens = qaTokenize(query);
      if (!qTokens.length) return null;
      var best = null, bestScore = 0;
      buildQACorpus().forEach(function (e) {
        var titleText = qaStrip(T(e.title) + ' ' + e.title);
        var bodyText = qaStrip(T(e.body) + ' ' + e.body);
        var score = 0;
        qTokens.forEach(function (t) {
          if (titleText.indexOf(t) !== -1) score += 3;
          if (bodyText.indexOf(t) !== -1) score += 1;
        });
        if (score > bestScore) { bestScore = score; best = e; }
      });
      return bestScore > 0 ? best : null;
    }
    function runAsk() {
      var input = $('#tut-ask-input'), box = $('#tut-ask-answer');
      if (!input || !box) return;
      var q = input.value.trim();
      if (!q) { box.style.display = 'none'; return; }
      var hit = answerQuestion(q);
      if (!hit) {
        box.innerHTML = '<div class="tut-ask-empty">' + T('Aucune réponse trouvée — essaie de reformuler, ou explore les catégories ci-dessus.') + '</div>';
      } else {
        box.innerHTML =
          '<div class="tut-ask-title">' + T(hit.title) + '</div>' +
          '<div class="tut-ask-body">' + T(hit.body) + '</div>' +
          '<button class="tut-ask-open" id="tut-ask-open-btn">' + T('Ouvrir le module') + ' — ' + T(hit.moduleTitle) + '</button>';
        var openBtn = box.querySelector('#tut-ask-open-btn');
        if (openBtn) openBtn.addEventListener('click', function () { startModule(hit.moduleId); });
      }
      box.style.display = 'block';
    }

    function chevronSvg() {
      return '<svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><path d="M9 6l6 6-6 6"/></svg>';
    }

    async function openLauncher() {
      var request = ++transition; transitioning = true; hideLauncher(); var released = await finishModule(false);
      if (request !== transition) return;
      transitioning = false; if (released === false) return;
      ensureDom(); ensureLauncher();
      var si = $('#tut-launcher-search'); if (si) si.value = '';
      var ai = $('#tut-ask-input'); if (ai) ai.value = '';
      var aa = $('#tut-ask-answer'); if (aa) { aa.style.display = 'none'; aa.innerHTML = ''; }
      renderCategoryScreen();
      var modal = $('#tut-launcher'); modal.style.display = 'flex';
      return placement.place(modal.querySelector('.modal-box'), function () { return null; }, null, 'launcher');
    }
    function hideLauncher() { var m = $('#tut-launcher'); if (m) m.style.display = 'none'; }
    function closeLauncher() { return stopTutorial(); }

    // ---- First-launch auto-start (feedback-welcome, once ever) -----------
    // Polls for the start screen closing (a project becoming active) rather
    // than hooking project.js's hideStartScreen() directly — same
    // decoupled-by-polling philosophy as the rest of this file (see header
    // comment): this module never patches or wraps existing app code, it
    // only ever reads state/DOM. Fires at most once per browser profile —
    // markFirstLaunchSeen() is called the instant it launches, before the
    // user has done anything inside it, so skipping never re-triggers it.
    function maybeAutoStartWelcome() {
      if (isFirstLaunchSeen()) return;
      var startScreen = document.getElementById('start-screen');
      if (!startScreen) return;
      var timer = setInterval(function () {
        if (!startScreen.classList.contains('hid')) return;
        clearInterval(timer);
        markFirstLaunchSeen();
        startModule('feedback-welcome');
      }, 400);
    }

    // ---- Entry point wiring ----------------------------------------------
    document.addEventListener('DOMContentLoaded', function () {
      ['btn-open-tutorial', 'btn-open-tutorial-topbar'].forEach(function (id) {
        var btn = document.getElementById(id);
        if (btn) btn.addEventListener('click', openLauncher);
      });
      document.addEventListener('keydown', function (e) {
        var launcher = $('#tut-launcher');
        if (e.key === 'Escape' && (active || transitioning || launcher && launcher.style.display === 'flex')) stopTutorial();
      });
      maybeAutoStartWelcome();
    });

    return { open: openLauncher, start: startModule, stop: stopTutorial, MODULES: MODULES };
  }
  window.NemoTutorialPresentation = Object.freeze({ create: create });
})();
