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
  function cfg() {
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
    if (!state.bpmShow) return frame;
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
    if (!state.bpmShow) return;
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

  function openSettings() {
    var c = cfg();
    var v = prompt('Tempo en BPM (0 pour masquer la grille)', String(c.bpm));
    if (v === null) return;
    var bpm = parseFloat(v);
    if (isNaN(bpm) || bpm <= 0) { state.bpmShow = false; refresh(); return; }
    var o = prompt('Décalage du premier temps, en frames', String(c.offset));
    if (o === null) return;
    state.bpm = bpm;
    state.bpmOffset = Math.max(0, parseInt(o, 10) || 0);
    state.bpmShow = true;
    refresh();
    if (window.showToast) showToast(bpm + ' BPM — ' + framesPerBeat().toFixed(2) + ' frames par temps');
  }
  function toggle() {
    state.bpmShow = !state.bpmShow;
    if (state.bpmShow && !state.bpm) state.bpm = 120;
    refresh();
    if (window.showToast) showToast(state.bpmShow ? (SM.t('toastBpmGridShownSuffix') + state.bpm + ')') : SM.t('toastBpmGridHidden'));
  }
  function refresh() {
    render();
    var b = document.getElementById('btn-bpm');
    if (b) b.classList.toggle('active', !!state.bpmShow);
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
