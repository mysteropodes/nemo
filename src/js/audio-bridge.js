// ---- AUDIO TRACKS (soundtrack + lip-sync reference, TVPaint-style) ----
// Data model: state.audioTracks = [{ name, dataB64, offsetFrames, volume,
// muted }] — dataB64 is a full data:audio/...;base64 URL so a project file
// stays self-contained (same tradeoff as imported rasters). Runtime-only
// fields (_buffer/_peaksCanvas/_srcNode/_gainNode) live on the same objects
// but are explicitly stripped by exportJSON's mapping — never persisted.
//
// Playback engine: Web Audio API. One shared AudioContext, created lazily on
// the first user gesture that needs it (import click / Play click) so the
// browser's autoplay policy never leaves it permanently "suspended". Frame
// sync is one-way: the existing setInterval playback loop (timeline.js
// startPlay) stays the clock for FRAMES, audio simply starts at the
// equivalent seconds offset and free-runs — for the few-seconds spans of a
// typical animation loop, drift between the two clocks is far below a frame.
// startPlay/stopPlay/loop-wrap call into onPlayStart/onPlayStop/onLoop.
(function () {
  var ctx = null;
  function ensureCtx() {
    if (!ctx) {
      var AC = window.AudioContext || window.webkitAudioContext;
      if (!AC) return null;
      ctx = new AC();
    }
    if (ctx.state === 'suspended') ctx.resume();
    return ctx;
  }

  function tracks() { return state.audioTracks || (state.audioTracks = []); }
  // Feedback action log (2026-07, "récupère des données exploitable pour
  // résoudre les soucis") — state.audioTracks isn't part of
  // layersSnapshotNow()'s undo snapshot, so these mutations can't route
  // through pushUndoLayers; logging directly is the only way an audio-track
  // edit shows up in a feedback report's action trail. See feedback-
  // bridge.js's logAction() doc comment for why an explicit string is
  // passed instead of relying on state.tool.
  function logAudio(action) { if (window.SMFeedback) SMFeedback.logAction('audio:' + action); }

  // ---- decoding + waveform peaks ----
  function dataURLToArrayBuffer(dataURL) {
    var b64 = dataURL.slice(dataURL.indexOf(',') + 1);
    var bin = atob(b64);
    var bytes = new Uint8Array(bin.length);
    for (var i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
    return bytes.buffer;
  }
  function decodeTrack(track) {
    if (track._buffer || track._decoding) return Promise.resolve(track._buffer);
    var c = ensureCtx();
    if (!c) return Promise.resolve(null);
    track._decoding = true;
    return c.decodeAudioData(dataURLToArrayBuffer(track.dataB64)).then(function (buf) {
      track._buffer = buf;
      track._decoding = false;
      track._peaksCanvas = null; // (re)draw with real data
      renderStrip();
      return buf;
    }).catch(function (e) {
      track._decoding = false;
      console.warn('[audio] decode failed', e);
      showToast('Audio: format non décodable (' + track.name + ')');
      return null;
    });
  }

  // Pre-rendered waveform canvas per track, height fixed, width = duration
  // in frames × FC. Redrawn only when missing or when fps changed (the
  // frames-per-second mapping changes the canvas width). Browsers cap
  // canvas width (~32k) — long audio at high zoom clamps and simply
  // downsamples harder, never fails.
  var STRIP_H = 26;
  function peaksCanvasFor(track) {
    var durFrames = track._buffer ? track._buffer.duration * state.fps : (state.fps * 2);
    var wantW = Math.max(8, Math.min(16000, Math.round(durFrames * FC)));
    if (track._peaksCanvas && track._peaksCanvas.width === wantW && track._peaksFps === state.fps) return track._peaksCanvas;
    var cv = document.createElement('canvas');
    cv.width = wantW; cv.height = STRIP_H;
    var g = cv.getContext('2d');
    g.fillStyle = 'rgba(94,106,210,0.25)';
    g.fillRect(0, 0, wantW, STRIP_H);
    if (track._buffer) {
      var data = track._buffer.getChannelData(0);
      var samplesPerPx = Math.max(1, Math.floor(data.length / wantW));
      g.fillStyle = 'rgba(140,150,255,0.9)';
      var mid = STRIP_H / 2;
      for (var x = 0; x < wantW; x++) {
        var s0 = x * samplesPerPx, mn = 1, mx = -1;
        // sub-sample inside the bucket (reading every sample of a long file
        // per pixel would be slow for zero visible benefit)
        var step = Math.max(1, Math.floor(samplesPerPx / 16));
        for (var s = s0; s < s0 + samplesPerPx && s < data.length; s += step) {
          var v = data[s];
          if (v < mn) mn = v;
          if (v > mx) mx = v;
        }
        var y0 = mid + mn * mid, y1 = mid + mx * mid;
        g.fillRect(x, y0, 1, Math.max(1, y1 - y0));
      }
    } else {
      g.fillStyle = 'rgba(200,200,220,0.5)';
      g.font = '9px sans-serif';
      g.fillText(track._decoding ? 'décodage…' : 'audio', 4, STRIP_H / 2 + 3);
    }
    track._peaksCanvas = cv;
    track._peaksFps = state.fps;
    return cv;
  }

  // ---- timeline strip UI ----
  function renderStrip() {
    var wrap = document.getElementById('audio-strip');
    if (!wrap) return;
    wrap.innerHTML = '';
    var list = tracks();
    wrap.style.display = list.length ? 'block' : 'none';
    list.forEach(function (track, ti) {
      var row = document.createElement('div');
      row.className = 'audio-row';
      row.style.width = (state.totalFrames * FC) + 'px';

      var cv = peaksCanvasFor(track);
      var img = document.createElement('div');
      img.className = 'audio-wave' + (track.muted ? ' muted' : '');
      img.style.left = (track.offsetFrames * FC) + 'px';
      img.style.width = cv.width + 'px';
      img.style.height = STRIP_H + 'px';
      img.appendChild(cv);
      img.title = track.name + ' — glisser pour décaler (' + (track.offsetFrames >= 0 ? '+' : '') + track.offsetFrames + ' f)';
      // drag to offset (snapped to whole frames)
      img.addEventListener('pointerdown', function (e) {
        e.preventDefault(); e.stopPropagation();
        var startX = e.clientX, startOffset = track.offsetFrames;
        function mv(ev) {
          track.offsetFrames = startOffset + Math.round((ev.clientX - startX) / FC);
          img.style.left = (track.offsetFrames * FC) + 'px';
        }
        function up() {
          document.removeEventListener('pointermove', mv);
          document.removeEventListener('pointerup', up);
          logAudio('offset');
          renderStrip();
        }
        document.addEventListener('pointermove', mv);
        document.addEventListener('pointerup', up);
      });
      row.appendChild(img);
      wrap.appendChild(row);
    });
    // extend the playhead over the audio rows so the time cursor reads
    // across them too
    var ph = document.getElementById('playhead');
    if (ph && list.length) {
      var h = parseInt(ph.style.height || '0', 10);
      ph.style.height = (h + list.length * (STRIP_H + 4)) + 'px';
    }
    renderAudioLayerRows();
  }

  // ---- layer-list rows (v15) — name + mute + level used to live in the
  // ".audio-ctl" cluster sitting on top of the waveform strip; moved here
  // per user request so an audio track reads/edits like any other layer,
  // and the waveform strip below is purely the waveform. Synthetic rows
  // only (never state.layers entries): audio never becomes a real
  // Paper.js-backed layer, so none of the layer.children consumers listed
  // in CLAUDE.md need to know these exist.
  function renderAudioLayerRows() {
    var list = document.getElementById('layer-list');
    if (!list) return;
    // Bug (2026-07, "les calques de son se dupliquent quand on clic
    // dessus"): this function only ever APPENDED rows, relying on the
    // caller to have cleared #layer-list first. That's true for the
    // normal path (renderLayerList does list.innerHTML='' before calling
    // renderStrip, which calls this) — but every row-level action here
    // (mute toggle, rename commit, delete, drag-to-offset's mouseup) calls
    // renderStrip() DIRECTLY, skipping renderLayerList entirely, so each
    // click appended a fresh full set of audio rows ON TOP of the ones
    // already sitting there. Self-clean here instead of trusting the
    // caller — idempotent regardless of which path triggered it.
    list.querySelectorAll('.audio-lrow').forEach(function (r) { r.remove(); });
    tracks().forEach(function (track, ti) {
      var row = document.createElement('div');
      row.className = 'lrow audio-lrow';
      row.dataset.audioTrack = ti;

      var spacer = document.createElement('div'); spacer.className = 'lico larrow-spacer'; row.appendChild(spacer);

      var dot = document.createElement('div'); dot.className = 'lico layer-color-dot'; dot.title = 'Piste audio';
      dot.style.setProperty('--dot-color', '#5e6ad2');
      row.appendChild(dot);

      var mute = document.createElement('div');
      mute.className = 'lico' + (track.muted ? ' off' : '');
      mute.title = window.SM.t(track.muted ? 'audioUnmute' : 'audioMute');
      // Flat monochrome SVGs, not emoji — matches every other .lico icon in
      // the layer panel (feedback: "encore des icon non flat").
      mute.innerHTML = track.muted
        ? '<svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" stroke-width="1.8"><path d="M11 5 6 9H3v6h3l5 4V5Z"/><path d="m16 9 5 6M21 9l-5 6"/></svg>'
        : '<svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" stroke-width="1.8"><path d="M11 5 6 9H3v6h3l5 4V5Z"/><path d="M15.5 8.5a5 5 0 0 1 0 7M18 6a8 8 0 0 1 0 12"/></svg>';
      mute.addEventListener('click', function (e) {
        e.stopPropagation();
        track.muted = !track.muted;
        logAudio('mute');
        if (state.playing) restartAt(state.currentFrame);
        renderStrip();
      });
      row.appendChild(mute);

      var nm = document.createElement('div');
      nm.className = 'lnm';
      nm.textContent = track.name;
      nm.title = window.SM.t('audioDblClickRename');
      row.appendChild(nm);
      row.addEventListener('dblclick', function (e) {
        if (e.target !== nm) return;
        var input = document.createElement('input'); input.type = 'text'; input.value = track.name;
        input.style.cssText = 'width:100%;background:var(--bg);border:1px solid var(--accent);color:var(--text);font-size:11px;border-radius:4px;padding:1px 4px;outline:none;';
        nm.innerHTML = ''; nm.appendChild(input); input.focus(); input.select();
        var done = false;
        function commit() { if (done) return; done = true; var v = input.value.trim(); if (v) { track.name = v; logAudio('rename'); } renderStrip(); }
        input.addEventListener('keydown', function (ev) { ev.stopPropagation(); if (ev.key === 'Enter') commit(); else if (ev.key === 'Escape') { done = true; renderStrip(); } });
        input.addEventListener('blur', commit);
        input.addEventListener('mousedown', function (e2) { e2.stopPropagation(); });
        input.addEventListener('dblclick', function (e2) { e2.stopPropagation(); });
      });

      // level (renamed from "opacity" per user request — a volume slider,
      // not a per-frame opacity, so it doesn't share that word)
      var vol = document.createElement('input');
      vol.type = 'range'; vol.min = 0; vol.max = 100; vol.value = Math.round((track.volume !== undefined ? track.volume : 1) * 100);
      vol.className = 'audio-vol'; vol.title = 'Niveau audio';
      vol.addEventListener('click', function (e) { e.stopPropagation(); });
      vol.addEventListener('input', function () {
        track.volume = vol.value / 100;
        if (track._gainNode) track._gainNode.gain.value = track.muted ? 0 : track.volume;
      });
      vol.addEventListener('change', function () { logAudio('volume'); });
      row.appendChild(vol);

      var del = document.createElement('div');
      del.className = 'lico'; del.textContent = '×'; del.title = window.SM.t('audioDeleteTrack');
      del.addEventListener('click', function (e) {
        e.stopPropagation();
        stopTrack(track);
        tracks().splice(ti, 1);
        logAudio('deleteTrack');
        renderStrip();
      });
      row.appendChild(del);

      list.appendChild(row);
    });
  }

  // ---- playback ----
  function startTrack(track, fromFrame) {
    if (track.muted || !track._buffer) return;
    var c = ensureCtx();
    if (!c) return;
    var src = c.createBufferSource();
    src.buffer = track._buffer;
    var gain = c.createGain();
    gain.gain.value = track.volume !== undefined ? track.volume : 1;
    src.connect(gain); gain.connect(c.destination);
    var tSec = (fromFrame - (track.offsetFrames || 0)) / state.fps;
    if (tSec >= track._buffer.duration) return;
    if (tSec >= 0) src.start(0, tSec);
    else src.start(c.currentTime - tSec, 0); // track starts later on the timeline — delay it
    track._srcNode = src;
    track._gainNode = gain;
  }
  function stopTrack(track) {
    if (track._srcNode) { try { track._srcNode.stop(); } catch (e) { } track._srcNode = null; }
    track._gainNode = null;
  }
  function stopAll() { tracks().forEach(stopTrack); }
  function restartAt(frame) {
    stopAll();
    tracks().forEach(function (t) { startTrack(t, frame); });
  }

  // ---- import ----
  function addTrackFromDataURL(name, dataURL) {
    var track = { name: name, dataB64: dataURL, offsetFrames: 0, volume: 1, muted: false };
    tracks().push(track);
    logAudio('import');
    if (dataURL.length > 8 * 1024 * 1024) {
      showToast('Audio volumineux : préférer mp3/ogg (l’autosave navigateur peut ne plus suivre)');
    }
    decodeTrack(track);
    renderStrip();
  }
  function importFile(file) {
    var r = new FileReader();
    r.onload = function (ev) { addTrackFromDataURL(file.name.replace(/\.[^.]+$/, ''), ev.target.result); };
    r.readAsDataURL(file);
  }

  // Audio scrubbing (v19) : jouer une tranche d'une frame quand le playhead
  // se deplace hors lecture — le reperage lip-sync de TVPaint/Callipeg.
  // Throttle (un scrub max par ~40ms) pour qu'un drag rapide ne declenche
  // pas des dizaines de sources superposees.
  var _lastScrubT = 0;
  function scrubAt(frame) {
    var now = performance.now();
    if (now - _lastScrubT < 40) return;
    _lastScrubT = now;
    var c = ensureCtx();
    if (!c) return;
    tracks().forEach(function (track) {
      if (track.muted || !track._buffer) return;
      var tSec = (frame - (track.offsetFrames || 0)) / state.fps;
      if (tSec < 0 || tSec >= track._buffer.duration) return;
      var src = c.createBufferSource();
      src.buffer = track._buffer;
      var gain = c.createGain();
      gain.gain.value = track.volume !== undefined ? track.volume : 1;
      src.connect(gain); gain.connect(c.destination);
      src.start(0, tSec, Math.max(1 / state.fps, 0.045));
    });
  }
  window.SMAudio = {
    scrubAt: scrubAt,
    onPlayStart: function (frame) { ensureCtx(); restartAt(frame); },
    onPlayStop: stopAll,
    onLoop: function (frame) { restartAt(frame); },
    renderStrip: renderStrip,
    importFile: importFile,
    addTrackFromDataURL: addTrackFromDataURL,
    // after importJSON/newProject replaced state.audioTracks wholesale:
    // decode whatever the new list holds and redraw the strip
    reload: function () {
      stopAll();
      tracks().forEach(function (t) { decodeTrack(t); });
      renderStrip();
    },
    invalidateWaveforms: function () {
      tracks().forEach(function (t) { t._peaksCanvas = null; });
      renderStrip();
    },
  };

  function init() {
    var btn = document.getElementById('btn-audio');
    var input = document.getElementById('audio-file-input');
    if (btn && input) {
      btn.addEventListener('click', function () { ensureCtx(); input.click(); });
      input.addEventListener('change', function (e) {
        for (var i = 0; i < e.target.files.length; i++) importFile(e.target.files[i]);
        e.target.value = '';
      });
    }
    renderStrip();
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', init);
  else init();
})();
