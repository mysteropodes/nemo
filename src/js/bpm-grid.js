// ---- BPM GUIDES (2026-07-25) ----
// Sander van Dijk's chapter 1.2: "a lot of animation is audio driven. Right
// now you have to listen, guess or look at the waveform to find out where
// the beat is." Type a BPM and the timeline gets beat lines you can snap
// keyframes and layer edges to, extending past the audio layer across the
// whole grid.
//
// Purely visual + an optional snap: the guides are never stored per frame,
// so they can't desync from anything. state.bpm / state.bpmOffset /
// state.bpmShow are the whole model.
(function () {
  function native() { return !!(window.__TAURI__ && window.__TAURI__.core); }
  function projection(onRefusal) {
    var c = window.NemoNativeCompositionController;
    if (!c && typeof onRefusal === 'function') try { onRefusal('controller-missing'); } catch (_) {}
    return c && c.read(onRefusal);
  }
  function settingsRefused(code) {
    try {
      var desktop = window.NemoNativeObjectProjectDesktop;
      if (desktop && typeof desktop.openDiagnostic === 'function') desktop.openDiagnostic(window, 'bpm-settings-refused', code);
    } catch (_) {}
  }
  function submit(action, origin) {
    var c = window.NemoNativeCompositionController;
    if (!c || !origin) { if (window.showToast) showToast('Native BPM settings are unavailable.'); return; }
    return c.edit(action, origin);
  }
  function cfg() {
    if (native()) {
      var p = projection(), c = p && p.composition;
      return c ? { bpm: c.effectiveBpm, offset: c.effectiveBpmOffset, on: c.effectiveBpmShow } : { bpm: 120, offset: 0, on: false };
    }
    if (state.bpm == null) state.bpm = 120;
    if (state.bpmOffset == null) state.bpmOffset = 0;   // frames before the first beat
    return { bpm: state.bpm, offset: state.bpmOffset, on: !!state.bpmShow };
  }
  // Frames per beat at the project's own fps. Non-integer on purpose: at
  // 24fps a 140bpm beat is 10.28 frames, and rounding each beat
  // independently (rather than accumulating a rounded step) keeps the grid
  // from drifting away from the music over a long timeline.
  function framesPerBeat() {
    var c = cfg();
    return (60 / Math.max(1, c.bpm)) * Math.max(1, state.fps);
  }
  function beatFrames() {
    var c = cfg(), fpb = framesPerBeat(), out = [], i = 0, f;
    if (fpb < 0.5) return out; // absurd bpm at this fps — nothing readable to draw
    while ((f = Math.round(c.offset + i * fpb)) < state.totalFrames && i < 4096) {
      if (f >= 0) out.push({ frame: f, beat: i });
      i++;
    }
    return out;
  }
  // Nearest beat to a frame — the snap used by the layer bars and keyframes
  // when the grid is on and Shift is held.
  function snapFrame(frame) {
    if (!cfg().on) return frame;
    var fpb = framesPerBeat();
    if (fpb < 0.5) return frame;
    var c = cfg();
    var i = Math.round((frame - c.offset) / fpb);
    return Math.max(0, Math.min(state.totalFrames - 1, Math.round(c.offset + i * fpb)));
  }

  function render() {
    var grid = document.getElementById('frame-grid');
    if (!grid) return;
    Array.prototype.slice.call(grid.querySelectorAll('.bpm-line')).forEach(function (el) { el.remove(); });
    if (!cfg().on) return;
    var beats = beatFrames();
    var frag = document.createDocumentFragment();
    beats.forEach(function (b) {
      var el = document.createElement('div');
      // Every 4th beat reads as a bar line — the visual anchor you actually
      // count from. Same idea as the ruler's own second markers.
      el.className = 'bpm-line' + (b.beat % 4 === 0 ? ' bpm-bar' : '');
      el.style.left = (b.frame * FC) + 'px';
      el.title = 'Temps ' + (b.beat + 1) + (b.beat % 4 === 0 ? ' (mesure ' + (b.beat / 4 + 1) + ')' : '') + ' — frame ' + (b.frame + 1);
      frag.appendChild(el);
    });
    grid.appendChild(frag);
  }

  var dismissSettings = null;
  function openNativeSettings(origin, c) {
    if (dismissSettings) return;
    var owner = window.NemoNativeCompositionController;
    if (!owner || typeof owner.edit !== 'function') { submit({}, null); return; }
    // Keep the original projection throughout text entry; confirmation must not rebase it.
    var previousFocus = document.activeElement, tempo = null, offsetStage = false;
    function element(tag, className, text) {
      var el = document.createElement(tag);
      if (className) el.className = className;
      if (text) el.textContent = text;
      return el;
    }
    var overlay = element('div', 'modal-overlay'); overlay.id = 'bpm-settings-dialog';
    // Like the existing export dialog, stay outside the native AppKit canvas column.
    overlay.style.justifyContent = 'flex-end'; overlay.style.paddingRight = '10px'; overlay.style.boxSizing = 'border-box';
    var form = element('form', 'modal-box');
    form.style.width = '260px'; form.style.maxWidth = 'calc(100vw - 20px)';
    form.setAttribute('role', 'dialog'); form.setAttribute('aria-modal', 'true');
    form.setAttribute('aria-labelledby', 'bpm-settings-label');
    var header = element('div', 'modal-hdr'); header.appendChild(element('span', '', 'BPM'));
    var x = element('button', 'modal-x', '×'); x.type = 'button'; x.id = 'bpm-settings-close';
    x.setAttribute('aria-label', 'Close'); header.appendChild(x); form.appendChild(header);
    var body = element('div', 'modal-bdy');
    var label = element('label'); label.id = 'bpm-settings-label'; label.htmlFor = 'bpm-settings-value';
    var input = element('input', 'pi'); input.id = 'bpm-settings-value'; input.type = 'text';
    input.inputMode = 'decimal'; input.style.width = '100%';
    body.appendChild(label); body.appendChild(input);
    var actions = element('div', 'pr'); actions.style.justifyContent = 'flex-end';
    var cancel = element('button', 'pbtn', 'Cancel'); cancel.type = 'button'; cancel.id = 'bpm-settings-cancel';
    var confirm = element('button', 'pbtn ac', 'OK'); confirm.type = 'submit'; confirm.id = 'bpm-settings-confirm';
    actions.appendChild(cancel); actions.appendChild(confirm); body.appendChild(actions);
    form.appendChild(body); overlay.appendChild(form);
    function close() {
      if (dismissSettings !== close) return;
      dismissSettings = null; overlay.remove();
      if (previousFocus && previousFocus.isConnected && typeof previousFocus.focus === 'function') previousFocus.focus();
    }
    function commit(action) {
      if (dismissSettings !== close) return;
      close();
      if (window.NemoNativeCompositionController !== owner) { submit({}, null); return; }
      return owner.edit(action, origin);
    }
    function stage() {
      label.textContent = offsetStage ? 'Décalage du premier temps, en frames' : 'Tempo en BPM (0 pour masquer la grille)';
      input.value = String(offsetStage ? c.offset : c.bpm); input.focus(); input.select();
    }
    form.addEventListener('submit', function (event) {
      event.preventDefault(); event.stopPropagation();
      if (dismissSettings !== close) return;
      if (offsetStage) return commit({ kind: 'setBpm', bpm: tempo, bpmOffset: Math.max(0, parseInt(input.value, 10) || 0), bpmShow: true });
      tempo = parseFloat(input.value);
      if (isNaN(tempo) || tempo <= 0) return commit({ kind: 'setBpm', bpmShow: false });
      offsetStage = true; stage();
    });
    x.addEventListener('click', close); cancel.addEventListener('click', close);
    overlay.addEventListener('click', function (event) { if (event.target === overlay) close(); });
    overlay.addEventListener('keydown', function (event) {
      event.stopPropagation();
      if (event.key === 'Escape') { event.preventDefault(); close(); }
    });
    dismissSettings = close; document.body.appendChild(overlay); stage();
  }
  window.addEventListener('pagehide', function () { if (dismissSettings) dismissSettings(); }, { once: true });

  function openSettings(origin) {
    origin = native() ? (arguments.length ? origin : projection(settingsRefused)) : null;
    var c = cfg();
    if (native() && !origin) { submit({}, null); return; }
    if (native()) return openNativeSettings(origin, { bpm: origin.composition.effectiveBpm, offset: origin.composition.effectiveBpmOffset });
    var v = prompt('Tempo en BPM (0 pour masquer la grille)', String(c.bpm));
    if (v === null) return;
    var bpm = parseFloat(v);
    if (isNaN(bpm) || bpm <= 0) { if (native()) { submit({ kind: 'setBpm', bpmShow: false }, origin); return; } state.bpmShow = false; refresh(); return; }
    var o = prompt('Décalage du premier temps, en frames', String(c.offset));
    if (o === null) return;
    if (native()) return submit({ kind: 'setBpm', bpm: bpm, bpmOffset: Math.max(0, parseInt(o, 10) || 0), bpmShow: true }, origin);
    state.bpm = bpm;
    state.bpmOffset = Math.max(0, parseInt(o, 10) || 0);
    state.bpmShow = true;
    refresh();
    if (window.showToast) showToast(bpm + ' BPM — ' + framesPerBeat().toFixed(2) + ' frames par temps');
  }
  function toggle() {
    if (native()) { var p = projection(); return submit({ kind: 'setBpm', bpmShow: !cfg().on }, p); }
    state.bpmShow = !state.bpmShow;
    if (state.bpmShow && !state.bpm) state.bpm = 120;
    refresh();
    if (window.showToast) showToast(state.bpmShow ? (SM.t('toastBpmGridShownSuffix') + state.bpm + ')') : SM.t('toastBpmGridHidden'));
  }
  function refresh() {
    render();
    var b = document.getElementById('btn-bpm');
    if (b) b.classList.toggle('active', cfg().on);
  }

  function init() {
    var b = document.getElementById('btn-bpm');
    if (b) {
      b.addEventListener('click', function () { toggle(); });
      b.addEventListener('contextmenu', function (e) { e.preventDefault(); openSettings(); });
    }
    refresh();
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', init); else init();

  window.SMBpm = {
    render: render, refresh: refresh, toggle: toggle, openSettings: openSettings,
    snapFrame: snapFrame, framesPerBeat: framesPerBeat, beatFrames: beatFrames,
  };
})();
