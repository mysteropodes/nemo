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
  // Animatable volume (feedback #218, "juste un slider pour le control du
  // volume pas de value animable") — a track object is not a state.layers
  // entry, but SMMotion.setValue/valueAtFrame/toggleAnimated never actually
  // required that: they only ever touch holder.motion/.motionStatic (same
  // non-layer-holder pattern ld.elementMotion already uses). Reading through
  // this ONE helper everywhere (the slider, playback gain, scrub blips)
  // means the legacy plain track.volume field (every project saved before
  // this feature) keeps working as the fallback for a track that has never
  // had its volume keyed or set through motionStatic.
  function currentVolume(track, frame) {
    if (window.SMMotion && SMMotion.isAnimated(track, 'volume')) return SMMotion.valueAtFrame(track, 'volume', frame === undefined ? state.currentFrame : frame)[0];
    if (track.motionStatic && track.motionStatic.volume !== undefined) return track.motionStatic.volume[0];
    return track.volume !== undefined ? track.volume : 1;
  }
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
      showToast(SM.t('toastAudioUndecodable') + track.name + ')');
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

      // In/out trim (feedback #218, "pas de in/out point pour trim") —
      // track.trimStart/trimEnd are seconds LOCAL to the decoded buffer
      // (unset trimEnd means "full duration", same "unset = default"
      // convention layer-inout.js's own ld.inPoint/outPoint already use).
      // Only meaningful once the buffer is actually decoded — before that
      // there's no duration to trim against, same gate peaksCanvasFor
      // already has for drawing real waveform data.
      if (track._buffer) {
        var pxPerSec = state.fps * FC;
        var baseLeft = track.offsetFrames * FC;
        var dimL = document.createElement('div'); dimL.className = 'audio-trim-dim';
        dimL.style.top = '2px'; dimL.style.height = STRIP_H + 'px';
        row.appendChild(dimL);
        var dimR = document.createElement('div'); dimR.className = 'audio-trim-dim';
        dimR.style.top = '2px'; dimR.style.height = STRIP_H + 'px';
        row.appendChild(dimR);
        var hL = document.createElement('div'); hL.className = 'layer-inout-handle left audio-trim-handle';
        hL.style.top = '2px'; hL.style.height = STRIP_H + 'px';
        hL.title = SM.t('hsAudioInTitle');
        row.appendChild(hL);
        var hR = document.createElement('div'); hR.className = 'layer-inout-handle right audio-trim-handle';
        hR.style.top = '2px'; hR.style.height = STRIP_H + 'px';
        hR.title = SM.t('hsAudioOutTitle');
        row.appendChild(hR);
        (function (track, dimL, dimR, hL, hR, pxPerSec, baseLeft) {
          function layoutTrim() {
            var s = track.trimStart || 0;
            var e = (track.trimEnd !== undefined && track.trimEnd !== null) ? track.trimEnd : track._buffer.duration;
            var startPx = baseLeft + s * pxPerSec, endPx = baseLeft + e * pxPerSec;
            dimL.style.display = s > 0.001 ? 'block' : 'none';
            dimL.style.left = baseLeft + 'px'; dimL.style.width = (s * pxPerSec) + 'px';
            var tailSec = track._buffer.duration - e;
            dimR.style.display = tailSec > 0.001 ? 'block' : 'none';
            dimR.style.left = endPx + 'px'; dimR.style.width = (tailSec * pxPerSec) + 'px';
            hL.style.left = startPx + 'px';
            hR.style.left = (endPx - 4) + 'px';
          }
          layoutTrim();
          function trimDrag(side, handle) {
            handle.addEventListener('pointerdown', function (e) {
              e.preventDefault(); e.stopPropagation();
              var startX = e.clientX;
              var origStart = track.trimStart || 0;
              var origEnd = (track.trimEnd !== undefined && track.trimEnd !== null) ? track.trimEnd : track._buffer.duration;
              function mv(ev) {
                var dSec = (ev.clientX - startX) / pxPerSec;
                if (side === 'left') track.trimStart = Math.max(0, Math.min(origStart + dSec, origEnd - 1 / state.fps));
                else track.trimEnd = Math.min(track._buffer.duration, Math.max(origEnd + dSec, origStart + 1 / state.fps));
                layoutTrim();
              }
              function up() {
                document.removeEventListener('pointermove', mv);
                document.removeEventListener('pointerup', up);
                logAudio('trim');
                if (state.playing) restartAt(state.currentFrame);
              }
              document.addEventListener('pointermove', mv);
              document.addEventListener('pointerup', up);
            });
          }
          trimDrag('left', hL);
          trimDrag('right', hR);
        })(track, dimL, dimR, hL, hR, pxPerSec, baseLeft);
      }
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
      row.className = 'lrow audio-lrow' + (state._audioSelId === track.audioId ? ' act' : '');
      row.dataset.audioTrack = ti;
      // Select-like-a-layer (feedback #218, "impossible de le select") —
      // .act is the exact class every real layer row already uses for its
      // own highlight (style.css), so an audio row now reads as "selected"
      // the same way. Click-to-toggle, not click-to-always-select: every
      // icon button on this row (mute/stopwatch/delete) already calls
      // e.stopPropagation() on its own click, and the volume slider on its
      // own 'click' listener too, so this only ever fires for a click on
      // the row's own body/name — never steals a click meant for one of
      // those controls.
      row.addEventListener('click', function () {
        state._audioSelId = (state._audioSelId === track.audioId) ? null : track.audioId;
        renderStrip();
      });

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

      // Volume stopwatch (feedback #218) — same 3-state convention (hollow/
      // blue-outline/solid-blue) and the same arm/remove-last-key-falls-
      // back-to-static/remove-key dance every other per-property stopwatch
      // in this app already does (motion.js's renderVertexRow is the
      // closest sibling — no shared helper exists there either, this just
      // follows the same inline shape).
      var swOn = !!(window.SMMotion && SMMotion.isAnimated(track, 'volume'));
      var swTrack = swOn && SMMotion.trackFor(track, 'volume');
      var hasKeyHere = !!(swTrack && SMMotion.keyAt(swTrack, state.currentFrame));
      var sw = document.createElement('div');
      sw.className = 'lico motion-stopwatch' + (swOn ? ' on' : '');
      sw.title = SMMotion.stopwatchTitle('audioAnimateVolume', swOn, hasKeyHere);
      sw.innerHTML = '<svg viewBox="0 0 24 24" width="12" height="12"><path d="M12 3l9 9-9 9-9-9z" fill="' + (hasKeyHere ? 'currentColor' : 'none') + '" stroke="currentColor" stroke-width="2"/></svg>';
      sw.addEventListener('click', function (e) {
        e.stopPropagation();
        if (!swOn) {
          SMMotion.toggleAnimated(track, 'volume');
        } else if (hasKeyHere) {
          if (swTrack.keys.length === 1) {
            var fv = SMMotion.valueAtFrame(track, 'volume', state.currentFrame);
            track.motion.volume = { keys: [] };
            if (!track.motionStatic) track.motionStatic = {};
            track.motionStatic.volume = fv;
          } else {
            SMMotion.removeKeyAtCurrentFrame(track, 'volume');
          }
        } else {
          SMMotion.setKeyAtCurrentFrame(track, 'volume', [currentVolume(track)]);
        }
        logAudio('volumeKey');
        renderStrip();
      });
      row.appendChild(sw);

      // level (renamed from "opacity" per user request — a volume slider,
      // not a per-frame opacity, so it doesn't share that word)
      var vol = document.createElement('input');
      vol.type = 'range'; vol.min = 0; vol.max = 100; vol.value = Math.round(currentVolume(track) * 100);
      vol.className = 'audio-vol'; vol.title = 'Niveau audio';
      vol.addEventListener('click', function (e) { e.stopPropagation(); });
      vol.addEventListener('input', function () {
        var v = vol.value / 100;
        if (window.SMMotion) SMMotion.setValue(track, 'volume', [v]);
        else track.volume = v;
        if (track._gainNode) track._gainNode.gain.value = track.muted ? 0 : v;
      });
      vol.addEventListener('change', function () { logAudio('volume'); });
      row.appendChild(vol);

      // Split at playhead (feedback #218, "pas de... coupé") — same
      // "trim.start < split < trim.end" validation splitTrackAt already
      // does, kept here too only for the icon's title (the click handler's
      // own toast covers the actual out-of-range case).
      var split = document.createElement('div');
      split.className = 'lico'; split.title = window.SM.t('audioSplitTrack');
      split.innerHTML = '<svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" stroke-width="1.8"><circle cx="6" cy="6" r="2.3"/><circle cx="6" cy="18" r="2.3"/><path d="M8 7.5 20 19M8 16.5 20 5"/></svg>';
      split.addEventListener('click', function (e) {
        e.stopPropagation();
        splitTrackAt(track, state.currentFrame);
      });
      row.appendChild(split);

      // feedback #218 ("je peux pas le supprimer") — deletion already
      // worked (splice below, unchanged), but the control was a bare "×"
      // glyph with none of the affordance every other row icon in this
      // panel has (mute just above uses a real flat SVG, matching the
      // panel's own established "flat monochrome SVGs, not emoji/glyphs"
      // convention) — easy to read as decoration on a tightly-packed row
      // rather than a clickable delete, which is very likely what actually
      // read as "can't delete it" live.
      var del = document.createElement('div');
      del.className = 'lico'; del.title = window.SM.t('audioDeleteTrack');
      del.innerHTML = '<svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" stroke-width="1.8"><path d="M9 3h6l1 2h4v2H4V5h4z"/><path d="M6 9h12l-1 12H7z"/></svg>';
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
  // Component/montage guard (2026-07-30 fix — found live: entering a
  // Component swaps state.fps to the symbol's own fps and resets
  // state.currentFrame to 0 (app.js's enterSymbol), same for
  // enterMontageView's synthetic scene, but state.audioTracks is a single
  // document-wide array with no per-context swap of its own (by design —
  // audio genuinely is comp-wide, not per-symbol). tSec below computes
  // straight off state.fps/state.currentFrame, so playing/scrubbing while
  // inside silently scheduled against the WRONG timeline: confirmed live,
  // playback from inside a Component with fps 24→12 rescheduled to a
  // completely unrelated delay/offset, and scrubAt's own tSec went
  // negative for every frame inside a component placed later than frame 0,
  // making its own no-op guard below swallow every scrub with zero
  // feedback. There's no well-defined "equivalent outer-scene time" while
  // isolated inside editing a symbol's own internal animation in the first
  // place (the same symbol can be placed at multiple different points/
  // speeds elsewhere) — suspending entirely here, rather than computing a
  // guess, is the same "guard over silent wrong behavior" posture already
  // used throughout this app's other symbol/montage-context bugs this
  // session.
  function _audioContextSuspended() { return !!(state.activeSymbolId || state.activeMontageViewId); }
  // trimStart/trimEnd are seconds LOCAL to the decoded buffer — see
  // renderStrip's own trim-handle comment for why unset trimEnd means
  // "full duration" rather than some other sentinel.
  function trimOf(track) {
    return {
      start: track.trimStart || 0,
      end: (track.trimEnd !== undefined && track.trimEnd !== null) ? track.trimEnd : (track._buffer ? track._buffer.duration : 0),
    };
  }
  function startTrack(track, fromFrame) {
    if (track.muted || !track._buffer || _audioContextSuspended()) return;
    var c = ensureCtx();
    if (!c) return;
    var trim = trimOf(track);
    var tSec = (fromFrame - (track.offsetFrames || 0)) / state.fps;
    if (tSec >= trim.end) return;
    // playFrom/playDuration are in the buffer's OWN local seconds — trim
    // only clips WHICH slice of the buffer plays, it never moves
    // offsetFrames (the anchor between buffer-local time 0 and the
    // timeline), so a trimmed-in track still starts counting from the
    // exact same timeline frame it always did.
    var playFrom = Math.max(tSec, trim.start);
    var playDuration = trim.end - playFrom;
    if (playDuration <= 0) return;
    // Timeline frame at which audio ACTUALLY becomes audible, and the
    // AudioContext time that corresponds to — same reference point either
    // way, whether playback starts right now (tSec already past trim.start)
    // or later (tSec hasn't reached trim.start yet — either because the
    // track's own untrimmed beginning is still ahead on the timeline, the
    // pre-#218 case, or because trim.start alone pushes it out). The
    // animated-volume curve below samples valueAtFrame from THIS frame
    // forward, not fromFrame, so delayed playback doesn't silently burn
    // through a chunk of its curve before any sound exists.
    var startsNow = tSec >= trim.start;
    var startAudioTime = startsNow ? c.currentTime : c.currentTime + (trim.start - tSec);
    var startFrame = startsNow ? fromFrame : fromFrame + (trim.start - tSec) * state.fps;
    var src = c.createBufferSource();
    src.buffer = track._buffer;
    var gain = c.createGain();
    // Animated volume (feedback #218) — a real gain envelope instead of a
    // flat value, sampled once per FRAME (matching how every other Motion
    // curve is quantized) across whatever fraction of the buffer will
    // actually play. Capped at 2000 samples for the same reason the canvas
    // overlays elsewhere in this app cap their own per-frame dot count —
    // no audible benefit past that density, and it's one array built per
    // playback start, not per frame.
    if (window.SMMotion && SMMotion.isAnimated(track, 'volume')) {
      var n = Math.max(2, Math.min(2000, Math.ceil(playDuration * state.fps) + 1));
      var curve = new Float32Array(n);
      for (var i = 0; i < n; i++) curve[i] = Math.max(0, SMMotion.valueAtFrame(track, 'volume', startFrame + i)[0]);
      var dur = (n - 1) / state.fps;
      if (dur > 0) gain.gain.setValueCurveAtTime(curve, startAudioTime, dur);
      else gain.gain.value = curve[0];
    } else {
      gain.gain.value = currentVolume(track, fromFrame);
    }
    src.connect(gain); gain.connect(c.destination);
    if (startsNow) src.start(0, playFrom, playDuration);
    else src.start(startAudioTime, trim.start, playDuration); // starts later on the timeline — delay it
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

  // Cut/split (feedback #218, "pas de... coupé") — the split point is a
  // TIMELINE frame, converted to buffer-local seconds through the SAME
  // offsetFrames anchor playback already uses. Splitting produces two
  // independent tracks sharing the same dataB64/decoded _buffer (a
  // reference share, not a duplicate decode — dataB64 is an immutable
  // string, so this is exactly as safe as any other plain-value share) but
  // with disjoint trim windows either side of the cut, so each half is
  // independently trimmable/deletable/mutable from here on. Volume
  // keyframes are deep-cloned onto both halves (JSON round-trip, the same
  // fast-clone convention CLAUDE.md §5bis already established for this
  // codebase) rather than dropped — a cut splitting a fade in two is far
  // less surprising than a cut silently erasing the fade from one side.
  function splitTrackAt(track, timelineFrame) {
    var trim = trimOf(track);
    var minGap = 1 / state.fps;
    var splitLocal = (timelineFrame - (track.offsetFrames || 0)) / state.fps;
    if (splitLocal <= trim.start + minGap || splitLocal >= trim.end - minGap) {
      if (window.showToast) showToast(SM.t('toastAudioSplitOutOfRange'));
      return false;
    }
    var list = tracks();
    var idx = list.indexOf(track);
    if (idx < 0) return false;
    var partB = {
      name: track.name,
      dataB64: track.dataB64,
      offsetFrames: track.offsetFrames,
      volume: track.volume,
      muted: track.muted,
      trimStart: splitLocal,
      trimEnd: trim.end,
      audioId: 'au' + Date.now().toString(36) + Math.random().toString(36).slice(2, 7),
      _buffer: track._buffer,
    };
    if (track.motion) partB.motion = JSON.parse(JSON.stringify(track.motion));
    if (track.motionStatic) partB.motionStatic = JSON.parse(JSON.stringify(track.motionStatic));
    track.trimEnd = splitLocal;
    list.splice(idx + 1, 0, partB);
    logAudio('split');
    if (state.playing) restartAt(state.currentFrame);
    renderStrip();
    return true;
  }

  // ---- import ----
  function addTrackFromDataURL(name, dataURL) {
    // audioId (2026-07-31, media-library registration): audio tracks had no
    // stable identity at all — an array index isn't one (reorder/delete
    // shifts it) — minted here so the catalog entry can find its way back
    // to the right track for deletion without guessing by name.
    var track = { name: name, dataB64: dataURL, offsetFrames: 0, volume: 1, muted: false, audioId: 'au' + Date.now().toString(36) + Math.random().toString(36).slice(2, 7) };
    tracks().push(track);
    logAudio('import');
    if (dataURL.length > 8 * 1024 * 1024) {
      showToast(SM.t('toastAudioTooLarge'));
    }
    decodeTrack(track);
    renderStrip();
    // Real asset-panel pass (2026-07-31, Cyril) — audio was a fully working,
    // separate feature (this whole file) but entirely invisible to the
    // Médias catalog. No thumb (media-library.js renders a note-icon tile
    // for kind==='audio'), no owning layer — audioId is what the panel's
    // "Supprimer la piste" menu entry reuses to find this exact track.
    if (window.SMMediaLibrary) SMMediaLibrary.addEntry(name, 'audio', null, null, { audioId: track.audioId });
  }
  // Removes a track by its stable audioId (2026-07-31) — reuses the exact
  // stop+splice this file already does elsewhere for track deletion,
  // rather than the media panel duplicating that logic.
  function removeTrackByAudioId(audioId) {
    var list = tracks();
    var i = list.findIndex(function (t) { return t.audioId === audioId; });
    if (i < 0) return false;
    stopTrack(list[i]);
    list.splice(i, 1);
    renderStrip();
    return true;
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
    if (_audioContextSuspended()) return; // see startTrack's doc comment above
    var now = performance.now();
    if (now - _lastScrubT < 40) return;
    _lastScrubT = now;
    var c = ensureCtx();
    if (!c) return;
    tracks().forEach(function (track) {
      if (track.muted || !track._buffer) return;
      var trim = trimOf(track);
      var tSec = (frame - (track.offsetFrames || 0)) / state.fps;
      if (tSec < trim.start || tSec >= trim.end) return;
      var src = c.createBufferSource();
      src.buffer = track._buffer;
      var gain = c.createGain();
      gain.gain.value = currentVolume(track, frame);
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
    removeTrackByAudioId: removeTrackByAudioId,
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
