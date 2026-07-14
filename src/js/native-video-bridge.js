// ---- NATIVE VIDEO BRIDGE (EXPERIMENTAL, 2026-07) ----
// experimental/native-video-decode branch only. JS side of
// src-tauri/src/video_decode.rs: opens a decode session on a video FILE
// (kept closed over its original bytes — no import-time JPEG baking),
// requests single frames as raw RGBA8 over binary IPC, and feeds them
// straight into the GPU texture cache via SMEngineBridge.registerImageRaw
// (zero canvas round-trips — see that function's comment).
//
// Tauri-only by construction (native decoder lives in the Tauri process);
// every entry point no-ops with a clear error in plain-browser preview.
//
// Includes an instrumented benchmark (SMNativeVideo.bench) because the
// whole point of this branch is measuring whether the architecture holds
// its real-time budget — decode+IPC+GPU-upload per frame, sequential AND
// random access, plus JS heap growth across a sustained run.
(function () {
  function tauriOk() { return typeof window.__TAURI__ !== 'undefined'; }
  function invoke(cmd, args) { return window.__TAURI__.core.invoke(cmd, args); }

  // sessionId -> {width, height, fps, frameCount, durationSeconds}
  var sessions = {};

  async function open(path) {
    if (!tauriOk()) throw new Error('native video decode requires the Tauri app (no sidecar in browser preview)');
    var info = await invoke('open_video_session', { path: path });
    sessions[info.session_id] = info;
    return info;
  }

  // Resolves to a Uint8Array of width*height*4 RGBA bytes.
  // tauri::ipc::Response arrives as an ArrayBuffer — one structured-clone
  // copy across the IPC boundary, no JSON/base64 anywhere.
  async function frameBytes(sessionId, frameIndex) {
    var buf = await invoke('decode_video_frame', { sessionId: sessionId, frameIndex: frameIndex });
    return new Uint8Array(buf);
  }

  // Decode + upload one frame to the engine under `imageId` — after this
  // resolves, the id is usable in scene JSON exactly like any registered
  // static image (same path the rotoscopy reference video already uses
  // for its ref:video id, minus its canvas detour).
  async function registerFrame(imageId, sessionId, frameIndex) {
    var s = sessions[sessionId];
    if (!s) throw new Error('unknown session ' + sessionId);
    var px = await frameBytes(sessionId, frameIndex);
    var expected = s.width * s.height * 4;
    if (px.length !== expected) throw new Error('bad frame buffer: got ' + px.length + ', expected ' + expected);
    if (!window.SMEngineBridge || !SMEngineBridge.registerImageRaw(imageId, px, s.width, s.height)) {
      throw new Error('engine not ready (registerImageRaw unavailable)');
    }
    return s;
  }

  async function close(sessionId) {
    delete sessions[sessionId];
    if (tauriOk()) await invoke('close_video_session', { sessionId: sessionId });
  }

  // ---- benchmark harness ----
  // Usage from the dev console in the Tauri app:
  //   await SMNativeVideo.bench('/absolute/path/to/video.mp4')
  // Measures, per phase: sequential decode (playback pattern), random
  // seeks (scrub pattern), and full decode->GPU-upload round trips.
  // Reports ms percentiles + JS heap delta (performance.memory is
  // Chromium-only; on WKWebView it reports 'n/a' — Rust-side timings in
  // the tauri dev console cover the native half there).
  async function bench(path, opts) {
    opts = opts || {};
    var seqN = opts.sequentialFrames || 60;
    var rndN = opts.randomSeeks || 15;
    function heapMB() { return (performance.memory && performance.memory.usedJSHeapSize / 1048576) || null; }
    function stats(arr) {
      if (!arr.length) return null;
      var s = arr.slice().sort(function (a, b) { return a - b; });
      return {
        avg: +(arr.reduce(function (a, b) { return a + b; }, 0) / arr.length).toFixed(2),
        p50: +s[Math.floor(s.length * 0.5)].toFixed(2),
        p95: +s[Math.floor(s.length * 0.95)].toFixed(2),
        max: +s[s.length - 1].toFixed(2),
      };
    }

    var heap0 = heapMB();
    var info = await open(path);
    var fpsBudgetMs = 1000 / (info.fps || 30);
    var out = { info: info, frameBudgetMs: +fpsBudgetMs.toFixed(2) };

    // Phase 1 — sequential decode+IPC (playback pattern, no GPU upload)
    var t, seqTimes = [];
    var maxSeq = Math.min(seqN, Number(info.frame_count) - 1);
    for (var i = 0; i < maxSeq; i++) {
      t = performance.now();
      await frameBytes(info.session_id, i);
      seqTimes.push(performance.now() - t);
    }
    out.sequentialDecodeIpcMs = stats(seqTimes);

    // Phase 2 — random seeks (scrub pattern)
    var rndTimes = [];
    for (var r = 0; r < rndN; r++) {
      var target = Math.floor(Math.random() * Number(info.frame_count));
      t = performance.now();
      await frameBytes(info.session_id, target);
      rndTimes.push(performance.now() - t);
    }
    out.randomSeekMs = stats(rndTimes);

    // Phase 3 — full pipeline: decode + IPC + GPU texture upload + render
    var fullTimes = [];
    var canRender = !!(window.SMEngineBridge && SMEngineBridge.isEnabled && SMEngineBridge.isEnabled());
    for (var f = 0; f < Math.min(30, maxSeq); f++) {
      t = performance.now();
      await registerFrame('bench:video', info.session_id, f);
      if (canRender) SMEngineBridge.renderNow();
      fullTimes.push(performance.now() - t);
    }
    out.fullPipelineMs = stats(fullTimes);
    out.fullPipelineIncludesRender = canRender;

    // Real-time verdict against the source's own fps budget
    out.realtimeSequential = out.sequentialDecodeIpcMs ? out.sequentialDecodeIpcMs.p95 <= fpsBudgetMs : null;
    out.realtimeFullPipeline = out.fullPipelineMs ? out.fullPipelineMs.p95 <= fpsBudgetMs : null;

    var heap1 = heapMB();
    out.jsHeapDeltaMB = heap0 != null && heap1 != null ? +(heap1 - heap0).toFixed(1) : 'n/a (WKWebView has no performance.memory — check Rust-side eprintln timings)';

    await close(info.session_id);
    console.table ? console.table(out) : console.log(out);
    return out;
  }

  window.SMNativeVideo = {
    open: open,
    frameBytes: frameBytes,
    registerFrame: registerFrame,
    close: close,
    bench: bench,
    sessions: function () { return Object.assign({}, sessions); },
  };
})();
