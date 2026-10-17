/* Existing Labs behavior behind explicit caller-owned ports. */
(function(root,factory){var api=factory();if(typeof module==='object'&&module.exports)module.exports=api;else root.NemoLabsFloatingUI=api;}(typeof globalThis==='object'?globalThis:this,function(){
  function create(ports){
  var document=ports.document,TOOL_CONTEXT=ports.contexts,REAL_FEATURES=ports.realFeatures,ACTIONS=ports.actions,ALWAYS=ports.always,SHORT_NAME=ports.names,ICONS=ports.icons,PARAMS=ports.params;
  var SYM_MODES=ports.symModes,PERSP_MODES=ports.perspModes,PERSP_MODE_LABEL=ports.perspLabels,SYM_MODE_LABEL=ports.symLabels;
  function paramValue(p) {
    var n = parseFloat(ports.getItem(p.key));
    return isNaN(n) ? p.def : n;
  }

  // A name is "available" (gets a button at all) if it's either a
  // registered Labs prototype, one of the real state-backed REAL_FEATURES,
  // or a plain ACTIONS entry — all three are otherwise treated identically
  // by everything below (same button, same active-state highlight).
  function buildGroup(names, registered) {
    return names.filter(function (n) { return registered.hasOwnProperty(n) || REAL_FEATURES.hasOwnProperty(n) || ACTIONS.hasOwnProperty(n); });
  }
  function renderLabsFloatPanel() {
    var panel = document.getElementById('labs-float-panel');
    var iconsWrap = document.getElementById('labs-float-icons');
    var paramsWrap = document.getElementById('labs-float-params');
    if (!panel || !iconsWrap || !paramsWrap || !ports.ready()) return;
    var registered = {};
    ports.list().forEach(function (p) { registered[p.name] = p.on; });
    // REAL_FEATURES entries override/augment the Labs `on` reading with
    // their own live state check — done here, once, rather than special-
    // casing every reader below.
    Object.keys(REAL_FEATURES).forEach(function (n) { registered[n] = REAL_FEATURES[n].isOn(); });
    Object.keys(ACTIONS).forEach(function (n) { registered[n] = ACTIONS[n].isActive(); });
    var ctx = buildGroup(TOOL_CONTEXT[ports.readField('tool')] || [], registered);
    var always = buildGroup(ALWAYS.filter(function (n) { return ctx.indexOf(n) < 0; }), registered);
    if (!ctx.length && !always.length) { panel.style.display = 'none'; return; }
    panel.style.display = 'flex';
    iconsWrap.innerHTML = '';
    var shown = ctx.concat(always);
    function addBtn(n) {
      var btn = document.createElement('button');
      btn.className = 'labs-float-btn' + (registered[n] ? ' active' : '');
      // Guarded (found live testing feedback #110's fill fix, unrelated to
      // it): this file's own init() (below) calls renderLabsFloatPanel()
      // on DOMContentLoaded — which fires BEFORE timeline.js's wholesale
      // `window.SM={...}` reassignment defines SM.t (see this file's own
      // 'load'-deferred afterI18n registration below, whose comment
      // documents the same ordering) — so the very first paint threw
      // "SM.t is not a function" on every fresh load. The afterI18n
      // requeue already repaints with real titles moments later; this
      // guard just stops that first paint from throwing uncaught.
      btn.title = ports.title(SHORT_NAME[n]) || n;
      btn.innerHTML = ICONS[n] || '';
      btn.addEventListener('click', function () {
        // ACTIONS entries manage their own re-render (BrushMenu.toggle
        // calls renderLabsFloatPanel itself once the popover's open/closed
        // state is settled) — skip the generic renderLabsFloatPanel() call
        // below for those so a just-opened popover's own button doesn't
        // get redrawn (and re-bound) out from under an in-flight click.
        if (ACTIONS[n]) { ACTIONS[n].onClick(btn); return; }
        if (REAL_FEATURES[n]) REAL_FEATURES[n].toggle();
        else ports.toggle(n);
        renderLabsFloatPanel();
        // Keep Réglages > Labs in sync if it happens to be open at the
        // same time — same checkbox state, two entry points.
        ports.refreshSettings();
      });
      iconsWrap.appendChild(btn);
    }
    ctx.forEach(addBtn);
    if (ctx.length && always.length) {
      var div = document.createElement('div');
      div.className = 'labs-float-divider';
      iconsWrap.appendChild(div);
    }
    always.forEach(addBtn);

    // Params row: one stepper pill per numeric knob, for every currently-
    // shown tool that's both ON and has a PARAMS entry — PLUS a hand-built
    // pair of pills for Symmetry (mode cycle + conditional sector count),
    // which don't fit PARAMS' "one localStorage-backed number" shape since
    // mode is a string cycle and both fields live on `state`, not
    // localStorage.
    // Every option row carries its owning tool's icon at the far left, greyed
    // (feedback 2026-07-26: "Un icon tout à gauche de la ligne de l'outil
    // grisé doit apparaître"). The PARAMS-driven rows further down already
    // built one; the hand-written Symmetry/Perspective pills never did, so
    // with several tools active at once those rows were the only ones giving
    // no clue which tool they belonged to — precisely the case the feedback
    // is about. One helper so a future hand-built pill can't forget again.
    function stepperPill(toolKey) {
      var pill = document.createElement('div');
      pill.className = 'labs-float-stepper';
      var ico = document.createElement('span');
      ico.className = 'lfs-tool-icon';
      ico.innerHTML = ICONS[toolKey] || '';
      pill.appendChild(ico);
      return pill;
    }
    paramsWrap.innerHTML = '';
    var anyParams = false;
    if (registered.symmetry) {
      anyParams = true;
      var modePill = stepperPill('symmetry');
      var modeLbl = document.createElement('span'); modeLbl.className = 'lfs-label'; modeLbl.textContent = ports.labels.lfsLabelMode();
      var modePrev = document.createElement('button'); modePrev.className = 'lfs-btn'; modePrev.textContent = '−';
      var modeVal = document.createElement('span'); modeVal.className = 'lfs-val';
      var modeNext = document.createElement('button'); modeNext.className = 'lfs-btn'; modeNext.textContent = '+';
      function cycleMode(dir) {
        var i = SYM_MODES.indexOf(ports.readField('symmetryMode'));
        var next = SYM_MODES[(i + dir + SYM_MODES.length) % SYM_MODES.length];
        ports.setSymmetryMode(next);
        var sel = document.getElementById('p-sym-mode'); if (sel) sel.value = next;
        ports.syncSymmetryPanelVisibility();
        modeVal.textContent = ports.translate(SYM_MODE_LABEL[next]);
        renderLabsFloatPanel(); // sectors pill needs to appear/disappear when entering/leaving radial
      }
      modeVal.textContent = ports.translate(SYM_MODE_LABEL[ports.readField('symmetryMode')]) || ports.readField('symmetryMode');
      modePrev.addEventListener('click', function () { cycleMode(-1); });
      modeNext.addEventListener('click', function () { cycleMode(1); });
      modePill.appendChild(modeLbl); modePill.appendChild(modePrev); modePill.appendChild(modeVal); modePill.appendChild(modeNext);
      paramsWrap.appendChild(modePill);
      if (ports.readField('symmetryMode') === 'radial') {
        var secPill = stepperPill('symmetry');
        var secLbl = document.createElement('span'); secLbl.className = 'lfs-label'; secLbl.textContent = ports.labels.lfsLabelSectors();
        var secMinus = document.createElement('button'); secMinus.className = 'lfs-btn'; secMinus.textContent = '−';
        var secVal = document.createElement('span'); secVal.className = 'lfs-val'; secVal.textContent = ports.readField('symmetryRadialSectors');
        var secPlus = document.createElement('button'); secPlus.className = 'lfs-btn'; secPlus.textContent = '+';
        function setSectors(v) {
          ports.setSectors(v);
          secVal.textContent = ports.readField('symmetryRadialSectors');
          var inp = document.getElementById('p-sym-sectors'); if (inp) inp.value = ports.readField('symmetryRadialSectors');
          ports.renderNow();
        }
        secMinus.addEventListener('click', function () { setSectors(ports.readField('symmetryRadialSectors') - 1); });
        secPlus.addEventListener('click', function () { setSectors(ports.readField('symmetryRadialSectors') + 1); });
        secPill.appendChild(secLbl); secPill.appendChild(secMinus); secPill.appendChild(secVal); secPill.appendChild(secPlus);
        paramsWrap.appendChild(secPill);
      }
      // Extend/Reset (2026-07): moved here from the now-removed right-panel
      // "Symmetry Guide" section (feedback: "les onglet guide symétrie et
      // perspective n'ont plus lieu d'être, les options doivent être gérées
      // au niveau du panneau flottant") — same wide-toggle-pill/action-pill
      // shape as Perspective's own Lock/Reset just below.
      var extPill = stepperPill('symmetry');
      var extBtn = document.createElement('button'); extBtn.className = 'lfs-btn wide' + (ports.readField('symmetryExtend') ? ' on' : '');
      extBtn.textContent = ports.labels.lfsBtnExtend(); extBtn.title = ports.labels.lfsExtendTitle();
      extBtn.addEventListener('click', function () {
        ports.toggleExtend();
        extBtn.classList.toggle('on', ports.readField('symmetryExtend'));
      });
      extPill.appendChild(extBtn);
      paramsWrap.appendChild(extPill);
      var symResetPill = stepperPill('symmetry');
      var symResetBtn = document.createElement('button'); symResetBtn.className = 'lfs-btn wide';
      symResetBtn.textContent = ports.labels.lfsBtnReset(); symResetBtn.title = ports.labels.lfsResetPositionTitle();
      symResetBtn.addEventListener('click', function () { ports.resetSymmetryGuide(); });
      symResetPill.appendChild(symResetBtn);
      paramsWrap.appendChild(symResetPill);
    }
    if (registered.perspective) {
      // Mode/Density/Lock/Reset (2026-07): moved here from the removed
      // right-panel "Perspective Guide" section (same feedback as
      // Symmetry's Extend/Reset above) — only on/off was mirrored here
      // before, so this panel had no Mode control at all yet either.
      anyParams = true;
      var pModePill = stepperPill('perspective');
      var pModeLbl = document.createElement('span'); pModeLbl.className = 'lfs-label'; pModeLbl.textContent = ports.labels.lfsLabelMode();
      var pModePrev = document.createElement('button'); pModePrev.className = 'lfs-btn'; pModePrev.textContent = '−';
      var pModeVal = document.createElement('span'); pModeVal.className = 'lfs-val';
      var pModeNext = document.createElement('button'); pModeNext.className = 'lfs-btn'; pModeNext.textContent = '+';
      function cyclePerspMode(dir) {
        var i = PERSP_MODES.indexOf(ports.readField('perspectiveMode'));
        var next = PERSP_MODES[(i + dir + PERSP_MODES.length) % PERSP_MODES.length];
        ports.setPerspectiveMode(next);
        pModeVal.textContent = ports.translate(PERSP_MODE_LABEL[next]);
      }
      pModeVal.textContent = ports.translate(PERSP_MODE_LABEL[ports.readField('perspectiveMode')]) || ports.readField('perspectiveMode');
      pModePrev.addEventListener('click', function () { cyclePerspMode(-1); });
      pModeNext.addEventListener('click', function () { cyclePerspMode(1); });
      pModePill.appendChild(pModeLbl); pModePill.appendChild(pModePrev); pModePill.appendChild(pModeVal); pModePill.appendChild(pModeNext);
      paramsWrap.appendChild(pModePill);
      // Density control removed (2026-09) — it tuned the ray count of the
      // static fan that buildPerspectiveGuideItems no longer draws, and
      // the snap-while-drawing logic dropped its own dense-fan fallback in
      // the same pass (perspective-bridge.js's axisCandidates), so nothing
      // reads state.perspectiveDensity anymore. Left in state/project I/O
      // as inert legacy data rather than touching the save format.
      var lockPill = stepperPill('perspective');
      var lockBtn = document.createElement('button'); lockBtn.className = 'lfs-btn wide';
      lockBtn.textContent = ports.labels.lfsBtnLock(); lockBtn.title = ports.labels.lfsLockTitle();
      var perspectiveLocked=ports.perspectiveLocked();
      lockBtn.classList.toggle('on', perspectiveLocked);
      lockBtn.addEventListener('click', function () {
        var locked = !lockBtn.classList.contains('on');
        ports.lockPerspective(locked);
        lockBtn.classList.toggle('on', locked);
        ports.renderNow();
      });
      lockPill.appendChild(lockBtn);
      paramsWrap.appendChild(lockPill);
      var perspResetPill = stepperPill('perspective');
      var perspResetBtn = document.createElement('button'); perspResetBtn.className = 'lfs-btn wide';
      perspResetBtn.textContent = ports.labels.lfsBtnReset(); perspResetBtn.title = ports.labels.lfsResetPositionsTitle();
      perspResetBtn.addEventListener('click', function () { ports.resetPerspectiveVPs(); });
      perspResetPill.appendChild(perspResetBtn);
      paramsWrap.appendChild(perspResetPill);
    }
    shown.forEach(function (n) {
      if (!registered[n] || !PARAMS[n]) return;
      PARAMS[n].forEach(function (p) {
        anyParams = true;
        var pill = document.createElement('div');
        pill.className = 'labs-float-stepper';
        var lbl = document.createElement('span'); lbl.className = 'lfs-label'; lbl.textContent = ports.translate(p.labelKey);
        var minus = document.createElement('button'); minus.className = 'lfs-btn'; minus.textContent = '−';
        var val = document.createElement('input');
        val.type = 'number';
        val.className = 'lfs-val scrub';
        val.min = p.min; val.max = p.max; val.step = p.step; val.dataset.step = p.step;
        var plus = document.createElement('button'); plus.className = 'lfs-btn'; plus.textContent = '+';
        function refresh() { val.value = paramValue(p); }
        refresh();
        minus.addEventListener('click', function () { p.set(Math.max(p.min, paramValue(p) - p.step)); refresh(); });
        plus.addEventListener('click', function () { p.set(Math.min(p.max, paramValue(p) + p.step)); refresh(); });
        val.addEventListener('change', function () {
          var next = Math.max(p.min, Math.min(p.max, parseFloat(val.value)));
          if (isNaN(next)) next = p.def;
          p.set(next); refresh();
        });
        var toolIcon = document.createElement('span');
        toolIcon.className = 'lfs-tool-icon';
        toolIcon.innerHTML = ICONS[n] || '';
        pill.appendChild(toolIcon); pill.appendChild(lbl); pill.appendChild(minus); pill.appendChild(val); pill.appendChild(plus);
        paramsWrap.appendChild(pill);
      });
    });
    hasParams = anyParams;
    applyParamsVisibility();
  }

  // -- option rows appear on hover, collapse on leave --------------------
  // 2026-07-27: "tous les menu sont bien comme ça mais doivent s'affiché
  // quand on hover la barre flottante et se refermer en hover off". The
  // strip sits ON the drawing area, so with three or four tools armed the
  // permanently-expanded option rows covered a good slice of the canvas
  // while you were drawing — the exact thing this panel was kept small to
  // avoid. The icon row (which says WHAT is on) always stays.
  var hasParams = false, hovering = false, dragging = false;
  function applyParamsVisibility() {
    var paramsWrap = document.getElementById('labs-float-params');
    if (!paramsWrap) return;
    // Two ways to be "still using it" with the pointer outside: mid-drag of
    // the whole panel (collapsing would resize it under the cursor), and
    // typing/scrubbing in a stepper field, which keeps focus but frees the
    // mouse to wander off.
    var busy = dragging || (document.activeElement && paramsWrap.contains(document.activeElement));
    paramsWrap.style.display = (hasParams && (hovering || busy)) ? 'flex' : 'none';
  }

  function setupHover() {
    var panel = document.getElementById('labs-float-panel');
    if (!panel) return;
    panel.addEventListener('mouseenter', function () { hovering = true; applyParamsVisibility(); });
    panel.addEventListener('mouseleave', function () { hovering = false; applyParamsVisibility(); });
    // A field can lose focus while the pointer is already elsewhere — that's
    // the moment the "busy" reprieve above expires.
    panel.addEventListener('focusout', function () { ports.setTimeout(applyParamsVisibility, 0); });
  }

  // -- drag anywhere within #canvas-area, position persisted -------------
  // Stored RELATIVE to #canvas-area's own rect (e.g. "20px from its left
  // edge"), not as raw viewport coordinates — the panel itself is
  // position:fixed (see its CSS comment for why: it's a DOM sibling of
  // #canvas-area, not a child, so it has no positioning ancestor of its
  // own), and #canvas-area's viewport position can shift (window resize,
  // side panels toggling). Storing relative-to-canvas-area keeps a saved
  // position meaningful across those changes instead of drifting off into
  // whatever now occupies those raw screen pixels.
  var POS_KEY = 'nemo-labs-float-pos';
  function loadPos() {
    try { return JSON.parse(ports.getItem(POS_KEY)) || { left: 20, top: 60 }; }
    catch (e) { return { left: 20, top: 60 }; }
  }
  function savePos(p) { try { ports.setItem(POS_KEY, JSON.stringify(p)); } catch (e) {} }

  var relPos = null; // {left, top} relative to #canvas-area's rect
  function applyFixedPos() {
    var panel = document.getElementById('labs-float-panel');
    var area = document.getElementById('canvas-area');
    if (!panel || !area || !relPos) return;
    var ar = area.getBoundingClientRect();
    panel.style.left = (ar.left + relPos.left) + 'px';
    panel.style.top = (ar.top + relPos.top) + 'px';
  }

  function setupDrag() {
    var panel = document.getElementById('labs-float-panel');
    var handle = document.getElementById('labs-float-handle');
    var area = document.getElementById('canvas-area');
    if (!panel || !handle || !area) return;
    relPos = loadPos();
    applyFixedPos();
    ports.addEventListener('resize', applyFixedPos);
    var startX = 0, startY = 0, startLeft = 0, startTop = 0;
    handle.addEventListener('pointerdown', function (e) {
      dragging = true;
      startX = e.clientX; startY = e.clientY;
      startLeft = relPos.left; startTop = relPos.top;
      handle.setPointerCapture(e.pointerId);
      e.preventDefault();
    });
    handle.addEventListener('pointermove', function (e) {
      if (!dragging) return;
      var ar = area.getBoundingClientRect();
      var nl = startLeft + (e.clientX - startX);
      var nt = startTop + (e.clientY - startY);
      // Clamped to stay fully inside the canvas area — "drop partout" means
      // anywhere in the drawing zone, not off past its edges where it'd be
      // unreachable.
      nl = Math.max(2, Math.min(ar.width - panel.offsetWidth - 2, nl));
      nt = Math.max(2, Math.min(ar.height - panel.offsetHeight - 2, nt));
      relPos = { left: nl, top: nt };
      applyFixedPos();
    });
    handle.addEventListener('pointerup', function (e) {
      if (!dragging) return;
      dragging = false;
      try { handle.releasePointerCapture(e.pointerId); } catch (er) {}
      savePos(relPos);
      applyParamsVisibility();
    });
  }

  return Object.freeze({render:renderLabsFloatPanel,setupDrag:setupDrag,setupHover:setupHover});
  }
  return Object.freeze({create:create});
}));
