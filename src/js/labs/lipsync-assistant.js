// ---- LABS PROTOTYPE — Amplitude lip-sync assistant (Moho/Toon Boom) ----
// Reads the decoded audio buffer's RMS amplitude per frame (SMAudio
// already keeps AudioBuffers around for waveform drawing — no new decode
// path) and stamps a 3-pose mouth kit (from pose-library) onto every
// frame accordingly: silence → closed, low → mid, loud → open. NOT
// phoneme recognition — amplitude only, explicitly the scoped-down
// version flagged as the one non-prototyped candidate that could still
// ship as a Labs prototype (see feature-scouting.md #9).
//
// Setup (once per character):
//   1. Draw/select the 3 mouth shapes, one at a time, on a frame
//   2. SMLabs.savePose('bouche-fermee')   (pose-library.js)
//      SMLabs.savePose('bouche-mi')
//      SMLabs.savePose('bouche-ouverte')
// Then, with an audio track imported:
//   SMLabs.lipsyncFromAudio(trackIndex, startFrame, endFrame, {
//     closed: 'bouche-fermee', mid: 'bouche-mi', open: 'bouche-ouverte'
//   })
//
// Requires pose-library (uses SMLabs.stampPose under the hood) — this
// file only computes the amplitude curve and the pose choice per frame,
// it never touches strokes directly.
(function () {
  // RMS over the frame's audio span, normalized 0..1 against the whole
  // clip's own 95th-percentile RMS (so a quiet recording still spans the
  // full closed→open range instead of reading as "always closed").
  function frameRMS(data, sr, tSec, frameDur) {
    var i0 = Math.max(0, Math.floor(tSec * sr));
    var i1 = Math.min(data.length, Math.floor((tSec + frameDur) * sr));
    if (i1 <= i0) return 0;
    var sum = 0;
    for (var i = i0; i < i1; i++) sum += data[i] * data[i];
    return Math.sqrt(sum / (i1 - i0));
  }

  window.SMLabs.lipsyncAmplitudeCurve = function (trackIndex, startFrame, endFrame) {
    var track = (state.audioTracks || [])[trackIndex];
    if (!track || !track._buffer) { console.warn('[labs] piste audio introuvable ou non décodée (index ' + trackIndex + ')'); return null; }
    var buf = track._buffer, data = buf.getChannelData(0), sr = buf.sampleRate;
    var frameDur = 1 / state.fps;
    var offsetSec = (track.offsetFrames || 0) * frameDur;
    var vals = [];
    for (var f = startFrame; f <= endFrame; f++) {
      var tSec = f * frameDur - offsetSec;
      vals.push(tSec < 0 || tSec > buf.duration ? 0 : frameRMS(data, sr, tSec, frameDur));
    }
    return vals;
  };

  window.SMLabs.lipsyncFromAudio = function (trackIndex, startFrame, endFrame, poses, opts) {
    if (!window.SMLabs.isOn('lipsync-assistant')) { console.warn('[labs] enable(\'lipsync-assistant\') d\'abord'); return 0; }
    if (!window.SMLabs.stampPose) { console.warn('[labs] active aussi pose-library'); return 0; }
    poses = poses || {};
    var closedName = poses.closed, midName = poses.mid, openName = poses.open;
    if (!closedName || !midName || !openName) { console.warn('[labs] fournir {closed, mid, open}'); return 0; }
    var vals = window.SMLabs.lipsyncAmplitudeCurve(trackIndex, startFrame, endFrame);
    if (!vals) return 0;
    var max = Math.max.apply(null, vals) || 1e-6;
    var loThr = opts && opts.loThr !== undefined ? opts.loThr : 0.15;
    var hiThr = opts && opts.hiThr !== undefined ? opts.hiThr : 0.5;
    var n = 0;
    var startCf = state.currentFrame;
    for (var i = 0; i < vals.length; i++) {
      var norm = vals[i] / max;
      var pose = norm < loThr ? closedName : (norm < hiThr ? midName : openName);
      goToFrame(startFrame + i);
      window.SMLabs.stampPose(pose);
      n++;
    }
    goToFrame(startCf);
    if (typeof showToast === 'function') showToast('Lip-sync amplitude : ' + n + ' frame(s) posée(s)');
    return n;
  };

  window.SMLabs.register('lipsync-assistant', {
    flag: 'nemo-labs-lipsync',
    describe: 'Lip-sync par amplitude (Moho/TB, amplitude seule — pas de phonèmes) : lipsyncFromAudio(trackIdx,f0,f1,{closed,mid,open}) tamponne un kit de bouches (pose-library) selon le volume par frame',
  });
})();
