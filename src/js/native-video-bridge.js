// ---- NATIVE VIDEO BRIDGE (EXPERIMENTAL, 2026-07) ----
// experimental/native-video-decode branch only. Opens a decode session on
// a video FILE (kept closed over its original bytes — no import-time JPEG
// baking), requests single frames as raw RGBA8, and feeds them straight
// into the GPU texture cache via SMEngineBridge.registerImageRaw (zero
// canvas round-trips — see that function's comment).
//
// TWO independent backends, dispatched on by open()/frameBytes()/close()
// below (see the "WebCodecs backend" comment right after the Tauri
// helpers): under Tauri, src-tauri/src/video_decode.rs over binary IPC
// (open_video_session/decode_video_frame/close_video_session) — unchanged
// from before. In a plain browser (no Tauri), MP4Box.js + WebCodecs
// VideoDecoder instead — added 2026-07 ("comment fait piximov/pikimov par
// exemple" — the browser-only improvement, the Tauri path was explicitly
// left alone). Every consumer past this dispatch layer (importAsLayer,
// displayRect, the JS-side cache/prefetch, _optimizeLayerMedia) works
// unmodified against either backend.
//
// Includes an instrumented benchmark (SMNativeVideo.bench) because the
// whole point of this branch is measuring whether the architecture holds
// its real-time budget — decode+IPC+GPU-upload per frame, sequential AND
// random access, plus JS heap growth across a sustained run.
(function () {
  function tauriOk() { return typeof window.__TAURI__ !== 'undefined'; }
  function invoke(cmd, args) { return window.__TAURI__.core.invoke(cmd, args); }

  // ---- WebCodecs backend (2026-07, browser-only — no Tauri) ----
  // "seulement pour la version navigateur, on utilise ce que l'on a là pour
  // tauri": the Tauri path above (ffmpeg subprocess, open_video_session/
  // decode_video_frame/close_video_session) is UNTOUCHED. This is a second,
  // independent backend for when window.__TAURI__ doesn't exist, so
  // open()/frameBytes()/close() below dispatch to whichever one applies —
  // every consumer further down this file (importAsLayer, displayRect,
  // the JS-side cache/prefetch in _layerFrameSync, _optimizeLayerMedia)
  // works unmodified against either backend, since both resolve to the
  // exact same shapes (open -> {session_id,width,height,fps,frame_count,
  // duration,codec}, frameBytes -> Uint8Array of width*height*4 RGBA).
  //
  // Demuxing (MP4Box.js, vendored src/js/mp4box.all.min.js, GPAC/BSD —
  // WebCodecs itself only decodes elementary streams, it has no container
  // parser) extracts every sample + the avcC/hvcC codec description up
  // front, whole-file-in-memory (matches how the app already reads a File
  // — no network-streaming case to support here; fine for a typical
  // imported clip, not built for hours-long footage). Per-frame decode
  // then mirrors the Rust side's own optimization: a sequential request
  // (frameIndex === last+1) continues feeding the SAME still-configured
  // VideoDecoder instead of a cold reset-to-keyframe, exactly like
  // decode_at/spawn_at reusing a running ffmpeg process for a forward walk.
  //
  // Known gap vs the Tauri backend: no background "optimize to all-intra"
  // step exists here (that's a whole VideoEncoder re-mux, out of scope for
  // this pass) — every seek pays the source's own GOP-sized decode cost,
  // same as Tauri's non-optimized fallback path. _optimizeLayerMedia below
  // no-ops for a web session accordingly (nv.isWeb guard).
  function webCodecsAvailable() { return typeof window.VideoDecoder !== 'undefined' && typeof window.MP4Box !== 'undefined'; }
  var webSessions = {}; // 'web-N' -> internal decode state (see openWeb)
  var _webSessionCounter = 0;

  // The avcC/hvcC/vpcC/av1C box's own payload, minus its [size][fourcc]
  // header — exactly what VideoDecoderConfig.description wants. MP4Box
  // parses it into a live box object with its own .write(stream) — there's
  // no public "just give me the raw bytes" accessor, so re-serializing via
  // its own DataStream and slicing off the 8-byte box header is the
  // standard extraction path (same one every WebCodecs+MP4Box demo uses).
  function _extractCodecDescription(mp4boxfile, trackId) {
    var trak = mp4boxfile.getTrackById(trackId);
    var entry = trak && trak.mdia && trak.mdia.minf.stbl.stsd.entries[0];
    var box = entry && (entry.avcC || entry.hvcC || entry.vpcC || entry.av1C);
    if (!box) return undefined;
    var DS = window.MP4Box.DataStream || window.DataStream;
    var stream = new DS(undefined, 0, DS.BIG_ENDIAN);
    box.write(stream);
    return new Uint8Array(stream.buffer, 8);
  }

  // Demux the whole file: resolves {track, samples, description}. `track`
  // is MP4Box's own onReady videoTracks[0] entry (width/height/codec/
  // timescale/duration); `samples` is every demuxed sample (data/cts/dts/
  // is_sync/duration), sorted into presentation order — frame N in this
  // app's sense is samples[N] in THAT order, not decode order.
  function _demux(file) {
    return new Promise(function (resolve, reject) {
      var mp4boxfile = window.MP4Box.createFile();
      var samples = [];
      var track = null, description;
      mp4boxfile.onError = function (e) { reject(new Error('mp4box: ' + e)); };
      mp4boxfile.onReady = function (info) {
        track = info.videoTracks && info.videoTracks[0];
        if (!track) { reject(new Error('aucune piste vidéo trouvée dans ce fichier')); return; }
        description = _extractCodecDescription(mp4boxfile, track.id);
        mp4boxfile.setExtractionOptions(track.id, null, { nbSamples: Infinity });
        mp4boxfile.onSamples = function (id, ref, s) { samples = samples.concat(s); };
        mp4boxfile.start();
      };
      file.arrayBuffer().then(function (buf) {
        buf.fileStart = 0;
        mp4boxfile.appendBuffer(buf);
        mp4boxfile.flush();
        // onReady/onSamples run synchronously inside flush() for a whole-
        // file (non-streamed) buffer — resolving on the next microtask
        // lets any already-queued onSamples calls land first.
        setTimeout(function () {
          if (!track) { reject(new Error('mp4box: infos de piste jamais reçues')); return; }
          samples.sort(function (a, b) { return a.cts - b.cts; });
          resolve({ track: track, samples: samples, description: description });
        }, 0);
      }).catch(reject);
    });
  }

  async function openWeb(file) {
    if (!webCodecsAvailable()) throw new Error('décodage vidéo navigateur indisponible (WebCodecs ou mp4box.js manquant)');
    var d = await _demux(file);
    var track = d.track;
    var timescale = track.timescale || (track.movie_timescale || 1);
    var durationSeconds = (track.duration && timescale) ? track.duration / timescale : 0;
    var frameCount = d.samples.length;
    var fps = (frameCount && durationSeconds) ? frameCount / durationSeconds : 24;
    var sessionId = 'web-' + (++_webSessionCounter);
    webSessions[sessionId] = {
      samples: d.samples,
      description: d.description,
      codec: track.codec,
      width: track.video.width,
      height: track.video.height,
      fps: fps,
      frameCount: frameCount,
      decoder: null,
      lastFrameIndex: -1,
      pending: [],
    };
    // Probe decode of frame 0 BEFORE reporting success — a browser can
    // pass webCodecsAvailable() and even isConfigSupported() yet have a
    // decoder that produces neither output nor errors (seen live in a
    // sandboxed Electron preview with the media-decode service disabled).
    // Failing HERE makes importAsLayer throw before any layer exists, so
    // images.js's caller falls back cleanly to the classic JPEG import
    // instead of leaving a layer that will never render anything. Costs
    // one frame decode (~ms) in a working browser; the decoded frame
    // stays warm for the immediately-following thumbnail/first-sync.
    try {
      await _decodeWebFrame(webSessions[sessionId], 0);
      webSessions[sessionId].lastFrameIndex = -1; // let the first real request re-decode from a clean baseline
    } catch (e) {
      delete webSessions[sessionId];
      throw e;
    }
    return {
      session_id: sessionId,
      width: track.video.width,
      height: track.video.height,
      fps: fps,
      frame_count: frameCount,
      duration: durationSeconds,
      codec: track.codec,
    };
  }

  // A VideoFrame's internal pixel layout (I420/NV12/etc.) isn't guaranteed
  // convertible to RGBA via copyTo() on every browser — drawImage() DOES
  // accept a VideoFrame directly per spec (CanvasImageSource) regardless
  // of its internal format, so drawing to a plain 2D canvas + getImageData
  // is the portable choice here, at the cost of one GPU->CPU readback per
  // frame (same cost class the Tauri path already pays over its IPC
  // boundary, just a different transport).
  function _videoFrameToRgba(frame) {
    var w = frame.displayWidth || frame.codedWidth, h = frame.displayHeight || frame.codedHeight;
    var canvas = document.createElement('canvas');
    canvas.width = w; canvas.height = h;
    var ctx = canvas.getContext('2d');
    ctx.drawImage(frame, 0, 0, w, h);
    return new Uint8Array(ctx.getImageData(0, 0, w, h).data.buffer);
  }

  // A fresh VideoDecoder for a cold seek/reset — pending outputs collect
  // in ws.pending; ws.waiter (set per in-flight _decodeWebFrame call) is
  // how that call knows enough of them have arrived without ever calling
  // flush() (see _decodeWebFrame's own comment for why flush() specifically
  // can't be used here).
  function _makeWebDecoder(ws) {
    ws.pending = [];
    ws.waiter = null;
    ws.decoder = new VideoDecoder({
      output: function (frame) {
        ws.pending.push(frame);
        if (ws.waiter && ws.pending.length >= ws.waiter.count) { var w = ws.waiter; ws.waiter = null; w.resolve(); }
      },
      error: function (e) {
        console.error('[web-video] decoder error', e);
        if (ws.waiter) { var w = ws.waiter; ws.waiter = null; w.reject(e); }
      },
    });
    ws.decoder.configure({ codec: ws.codec, codedWidth: ws.width, codedHeight: ws.height, description: ws.description });
  }

  async function _decodeWebFrame(ws, frameIndex) {
    if (frameIndex < 0 || frameIndex >= ws.samples.length) throw new Error('frame index hors limites');
    var startIdx = frameIndex;
    if (ws.decoder && ws.decoder.state === 'configured' && frameIndex === ws.lastFrameIndex + 1) {
      // Sequential continuation — feed just the new sample onto the SAME
      // still-open decoder instead of a cold reset-to-keyframe (mirrors
      // decode_at/spawn_at reusing a running ffmpeg process on the Rust
      // side, same reasoning: a fresh restart-from-keyframe for every
      // single forward step would pay full GOP-decode cost every time).
      // ws.pending was already drained+closed at the end of the PREVIOUS
      // call below, so it's empty here — no explicit reset needed.
    } else {
      if (ws.decoder && ws.decoder.state !== 'closed') { try { ws.decoder.close(); } catch (e) { /* already closing */ } }
      while (startIdx > 0 && !ws.samples[startIdx].is_sync) startIdx--;
      _makeWebDecoder(ws);
    }
    var need = frameIndex - startIdx + 1;
    for (var i = startIdx; i <= frameIndex; i++) {
      var s = ws.samples[i];
      ws.decoder.decode(new EncodedVideoChunk({ type: s.is_sync ? 'key' : 'delta', timestamp: s.cts, duration: s.duration, data: s.data }));
    }
    // Deliberately NOT decoder.flush() — flush() drains AND resets the
    // decode context in at least Chrome's implementation, so the VERY
    // NEXT decode() call after one would need to start with a keyframe
    // again ("A key frame is required after configure() or flush()"),
    // silently defeating the sequential-continuation path above every
    // single time (found live testing this exact bridge). Waiting on the
    // output callback directly needs no flush and keeps the decoder
    // reusable across calls.
    //
    // Bounded wait, never infinite — same philosophy as the Rust side's
    // recv_timeout(700ms) on its converter channel: a decoder that
    // produces neither output nor an error (seen live in a sandboxed
    // Electron preview where the media-decode service is disabled) would
    // otherwise hang this await forever, freezing whatever awaited it
    // (importAsLayer's thumbnail, every frame sync). On timeout the
    // decoder is torn down so the NEXT attempt starts from a clean
    // cold-seek state instead of feeding a wedged instance.
    await new Promise(function (resolve, reject) {
      if (ws.pending.length >= need) { resolve(); return; }
      var timer = setTimeout(function () {
        if (ws.waiter && ws.waiter.resolve === wrappedResolve) ws.waiter = null;
        if (ws.decoder && ws.decoder.state !== 'closed') { try { ws.decoder.close(); } catch (e2) { /* already closing */ } }
        ws.decoder = null;
        ws.lastFrameIndex = -1;
        reject(new Error('décodeur WebCodecs muet (aucune image en 4s) — décodage vidéo probablement indisponible dans ce navigateur'));
      }, 4000);
      function wrappedResolve() { clearTimeout(timer); resolve(); }
      function wrappedReject(e) { clearTimeout(timer); reject(e); }
      ws.waiter = { count: need, resolve: wrappedResolve, reject: wrappedReject };
    });
    ws.lastFrameIndex = frameIndex;
    var frames = ws.pending;
    if (!frames.length) throw new Error('le décodeur n\'a produit aucune image');
    var target = frames[frames.length - 1];
    var rgba = _videoFrameToRgba(target);
    frames.forEach(function (f) { if (!f.closed) { try { f.close(); } catch (e) { /* already closed */ } } });
    ws.pending = [];
    return rgba;
  }

  function _sourceName(source) { return (source instanceof Blob) ? (source.name || 'video') : source.split('/').pop(); }

  // sessionId -> {width, height, fps, frameCount, durationSeconds} (Tauri sessions only — web sessions live in webSessions)
  var sessions = {};

  async function open(source) {
    if (source instanceof Blob) {
      var infoWeb = await openWeb(source);
      sessions[infoWeb.session_id] = infoWeb;
      return infoWeb;
    }
    if (!tauriOk()) throw new Error('native video decode requires the Tauri app (no sidecar in browser preview)');
    var info = await invoke('open_video_session', { path: source });
    sessions[info.session_id] = info;
    return info;
  }

  // Resolves to a Uint8Array of width*height*4 RGBA bytes — same shape for
  // both backends. tauri::ipc::Response arrives as an ArrayBuffer (one
  // structured-clone copy across the IPC boundary, no JSON/base64); the
  // web backend produces the same shape via _decodeWebFrame/getImageData.
  async function frameBytes(sessionId, frameIndex) {
    if (String(sessionId).indexOf('web-') === 0) {
      var ws = webSessions[sessionId];
      if (!ws) throw new Error('unknown web session ' + sessionId);
      return await _decodeWebFrame(ws, frameIndex);
    }
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
    if (String(sessionId).indexOf('web-') === 0) {
      var ws = webSessions[sessionId];
      if (ws && ws.decoder && ws.decoder.state !== 'closed') { try { ws.decoder.close(); } catch (e) { /* already closing */ } }
      delete webSessions[sessionId];
      return;
    }
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
    var out = { path: path, info: info, frameBudgetMs: +fpsBudgetMs.toFixed(2) };

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

    // Phase 3 — full pipeline: decode + IPC + GPU texture upload + render.
    // Uses renderImageOnly (not renderNow) to match the production path
    // (_layerFrameSync/_refSync below) — it reuses the cached scene JSON
    // instead of rebuilding it, which is the whole point of that fix; a
    // bench that measured the old renderNow() cost here would no longer
    // reflect what playback/scrub actually pays.
    var fullTimes = [];
    var canRender = !!(window.SMEngineBridge && SMEngineBridge.isEnabled && SMEngineBridge.isEnabled());
    for (var f = 0; f < Math.min(30, maxSeq); f++) {
      t = performance.now();
      await registerFrame('bench:video', info.session_id, f);
      if (canRender) SMEngineBridge.renderImageOnly();
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

  // ---- live rotoscopy-reference integration (type:'native') ----
  // Wires the native decoder into the app's EXISTING per-frame choke point
  // (loadFrame -> SMReference.onFrameChanged -> syncToFrame, which calls
  // _refSync below for type:'native' refMedia). Busy/pending coalescing:
  // scrubbing fires loadFrame far faster than a decode completes — only
  // the LATEST requested frame matters, intermediate targets are simply
  // replaced (same principle as reference-bridge's own startSeek pending).
  var _refBusy = false, _refPending = null;
  async function _refSync(r, frame) {
    var target = Math.max(0, (frame - (r.offsetFrames || 0)));
    if (_refBusy) { _refPending = { r: r, frame: frame }; return; }
    _refBusy = true;
    try {
      // (Re)open the session from the persisted path (r.src) if missing —
      // covers both first attach and a project reloaded from disk.
      if (!r._sessionId) {
        var info = await open(r.src);
        r._sessionId = info.session_id;
        r._frameCount = Number(info.frame_count);
        r._dims = { w: info.width, h: info.height };
      }
      var clamped = Math.min(r._frameCount - 1, target);
      var s = sessions[r._sessionId];
      var px = await frameBytes(r._sessionId, clamped);
      if (window.SMEngineBridge) SMEngineBridge.registerImageRaw('ref:native', px, s.width, s.height);
      window._sceneVersion++;
      // renderImageOnly, not renderNow: the scene JSON's reference item
      // still just points at 'ref:native' by id — only the bytes behind
      // that id changed, so reuse the cached JSON instead of re-walking
      // every Paper.js item just to produce byte-identical text.
      if (window.SMEngineBridge && SMEngineBridge.isEnabled()) SMEngineBridge.renderImageOnly();
    } catch (e) {
      console.error('[native-video] refSync failed:', e);
    } finally {
      _refBusy = false;
      if (_refPending) { var p = _refPending; _refPending = null; _refSync(p.r, p.frame); }
    }
  }

  // ---- native video LAYERS (ld.nativeVideo) ----
  // The graduation of the experiment: a video imported via the Vidéo…
  // button becomes a LAYER backed by a live decode session (instant
  // import — no JPEG baking), its picture emitted by buildSceneJson as
  // one image item under 'nv:<layerIdx>' (engine-bridge.js). This hook
  // runs from loadFrame's choke point (app.js) and keeps each such
  // layer's texture in sync with the playhead, with the same
  // latest-target-wins coalescing as _refSync — per layer.
  // Per-layer sync state. Three performance layers on top of the naive
  // decode-on-demand (added after real-app testing: "scrub et lecture qui
  // rament"):
  //   1. lastShown short-circuit — loadFrame runs on EVERY app action
  //      (stroke commits, undo, tool ops…), not just frame changes;
  //      without this, drawing one stroke on ANOTHER layer triggered a
  //      full decode round-trip of the unchanged video frame.
  //   2. one-frame prefetch cache — after serving frame N, frame N+1 is
  //      decoded in the background; during playback the next loadFrame
  //      hits the cache SYNCHRONOUSLY (no await), so the texture updates
  //      inside loadFrame's own turn and the engine's normal render shows
  //      it — zero decode latency in the critical path, ONE render per
  //      frame instead of render-then-rerender-when-decode-lands.
  //   3. latest-target-wins coalescing on the async (cache-miss) path —
  //      scrubbing faster than decode only ever decodes the newest target.
  var _layerSync = {}; // layerIdx -> {busy, pending, lastShown, jsCache:Map, jsBytes, prefetchQueue, prefetching}
  // JS-SIDE frame window (the AE "green bar" idea, scaled down): frames
  // kept in THIS process's memory, not just Rust's. The Rust cache made
  // decode free on revisits, but every request still paid the full IPC
  // round trip (invoke → locks → 8.3MB response copy across the WKWebView
  // boundary → new Uint8Array) — the remaining "mini latences de scrub".
  // A hit here registers + renders synchronously inside loadFrame's own
  // turn: zero IPC, zero await. Byte-bounded like the Rust cache (frame
  // size varies 8x between 1080p and 4K), Map insertion order = LRU.
  var JS_CACHE_BUDGET = 64 * 1024 * 1024; // ~7 frames @1080p per layer
  function onFrameChanged(frame) {
    // No blanket tauriOk() gate anymore (2026-07, "la vidéo ne lit pas
    // dans le canvas") — this early-return predated the WebCodecs backend
    // and silently disabled ALL frame syncing in a plain browser, so a
    // web-imported layer rendered nothing: the scene JSON's image item
    // only appears once registerImageRaw has uploaded a frame under
    // 'nv:<li>' (see engine-bridge.js's registeredImageIds check), which
    // only ever happens through this sync path. The per-layer loop below
    // is backend-agnostic (frameBytes dispatches per session); a Tauri-only
    // layer in a browser (loaded project) just fails its lazy re-open with
    // a console error, same as any other unreachable source.
    for (var i = 0; i < state.layers.length; i++) {
      if (state.layers[i].nativeVideo) _layerFrameSync(i, frame);
    }
  }
  function _syncState(li) {
    return _layerSync[li] || (_layerSync[li] = { busy: false, pending: null, lastShown: -1, jsCache: new Map(), jsBytes: 0, prefetchQueue: [], prefetching: false });
  }
  function _jsCachePut(st, frame, px) {
    if (st.jsCache.has(frame)) { st.jsBytes -= st.jsCache.get(frame).length; st.jsCache.delete(frame); }
    st.jsCache.set(frame, px);
    st.jsBytes += px.length;
    while (st.jsBytes > JS_CACHE_BUDGET && st.jsCache.size > 1) {
      var oldest = st.jsCache.keys().next().value;
      st.jsBytes -= st.jsCache.get(oldest).length;
      st.jsCache.delete(oldest);
    }
  }
  function _jsCacheGet(st, frame) {
    var px = st.jsCache.get(frame);
    if (px) { st.jsCache.delete(frame); st.jsCache.set(frame, px); } // refresh LRU recency
    return px || null;
  }
  // Rolling perf counters — SMNativeVideo.stats() from the console gives
  // real numbers (hit rate, IPC ms, register+render ms) so the NEXT
  // optimization round is measurement-driven instead of guessed.
  var _stats = { serves: 0, jsHits: 0, ipcMs: [], renderMs: [] };
  function _pushStat(arr, v) { arr.push(v); if (arr.length > 240) arr.shift(); }
  function _pctl(arr, p) { if (!arr.length) return null; var a = arr.slice().sort(function (x, y) { return x - y; }); return +(a[Math.min(a.length - 1, Math.floor(a.length * p))]).toFixed(1); }
  function stats() {
    return {
      serves: _stats.serves,
      jsCacheHitRate: _stats.serves ? +(_stats.jsHits / _stats.serves).toFixed(2) : null,
      ipcMs: { p50: _pctl(_stats.ipcMs, 0.5), p95: _pctl(_stats.ipcMs, 0.95) },
      registerRenderMs: { p50: _pctl(_stats.renderMs, 0.5), p95: _pctl(_stats.renderMs, 0.95) },
    };
  }
  function _targetFor(nv, frame) {
    return Math.max(0, Math.min(nv.frameCount - 1, frame - (nv.offsetFrames || 0)));
  }
  // renderImageOnly reuses the cached scene JSON verbatim — safe ONLY when
  // this layer's image rect (x/y/width/height in buildSceneJson) can't have
  // changed since that JSON was built. A Motion-keyed layer's rect moves
  // every frame (position/scale animation on the video — see
  // engine-bridge.js's nvMat handling), so reusing stale JSON there would
  // freeze the video at its LAST rendered position/scale while only its
  // pixels kept updating. Falls back to a full renderNow() for those.
  function _canFastRender(li) {
    var ld = state.layers[li];
    return !(ld && ld.motion && Object.keys(ld.motion).length);
  }
  // Pull frames around `center` into the JS window, nearest-first, both
  // directions when the source is all-intra (optimized media / ProRes —
  // backward frames are one cheap independent decode there; on long-GOP
  // originals backward pulls would pay a keyframe walk each, so forward
  // only). Single in-flight chain per layer; a new center simply replaces
  // the queue (latest wins, same principle as everywhere else here).
  function _prefetch(li, center) {
    var st = _syncState(li);
    var ld = state.layers[li];
    if (!ld || !ld.nativeVideo || !ld._nvSessionId) return;
    var nv = ld.nativeVideo;
    var bidir = !!nv.optimizedPath || _isAllIntra(nv.codec);
    var wanted = bidir ? [center + 1, center - 1, center + 2, center - 2, center + 3, center + 4] : [center + 1, center + 2, center + 3];
    st.prefetchQueue = wanted.filter(function (f) { return f >= 0 && f < nv.frameCount && !st.jsCache.has(f); });
    if (st.prefetching) return; // running chain picks up the new queue
    st.prefetching = true;
    (async function () {
      try {
        while (st.prefetchQueue.length) {
          var f = st.prefetchQueue.shift();
          var sess = state.layers[li] && state.layers[li]._nvSessionId;
          if (!sess) break;
          var px = await frameBytes(sess, f);
          _jsCachePut(st, f, px);
        }
      } catch (e) { /* prefetch is best-effort */ }
      st.prefetching = false;
    })();
  }
  function _layerFrameSync(li, frame) {
    var ld = state.layers[li];
    if (!ld || !ld.nativeVideo) return;
    var nv = ld.nativeVideo;
    var st = _syncState(li);
    if (ld._nvSessionId && nv.frameCount) {
      var target = _targetFor(nv, frame);
      if (target === st.lastShown) return; // frame unchanged — loadFrame ran for an unrelated reason
      // SYNC fast path: the prefetched frame is exactly the one needed —
      // upload inside this very loadFrame turn, no decode wait. Must call
      // renderImageOnly() explicitly (NOT rely on tick()'s own dirty-check
      // loop): the scene JSON's image item only carries the id string
      // 'nv:<li>', never the frame's actual bytes, so it's byte-identical
      // before and after this update — tick()'s string-diff would see
      // "no change" and skip rendering entirely, leaving the new frame's
      // pixels sitting unrendered in the engine's image cache (found live
      // testing: video appeared to freeze/skip on this exact path).
      var cached = _jsCacheGet(st, target);
      if (cached) {
        var tR = performance.now();
        if (window.SMEngineBridge) SMEngineBridge.registerImageRaw('nv:' + li, cached, nv.width, nv.height);
        st.lastShown = target;
        if (window.SMEngineBridge && SMEngineBridge.isEnabled()) {
          if (_canFastRender(li)) SMEngineBridge.renderImageOnly(); else SMEngineBridge.renderNow();
        }
        _stats.serves++; _stats.jsHits++;
        _pushStat(_stats.renderMs, performance.now() - tR);
        _prefetch(li, target);
        return;
      }
    }
    _layerFrameSyncAsync(li, frame);
  }
  async function _layerFrameSyncAsync(li, frame) {
    var st = _syncState(li);
    if (st.busy) { st.pending = frame; return; }
    st.busy = true;
    try {
      var ld = state.layers[li];
      if (!ld || !ld.nativeVideo) return;
      var nv = ld.nativeVideo;
      // Lazy (re)open from the persisted path — first frame after an
      // import OR after a project load lands here without a session.
      // Prefer the optimized (all-intra) copy when one was produced; if
      // the OS purged it from the temp cache, fall back to the original
      // and let _optimizeLayerMedia regenerate it in the background.
      if (!ld._nvSessionId) {
        // A web (File/Blob) session can't lazily re-open: nv.path is just
        // the display name (no durable file handle survives a reload —
        // see importAsLayer's own comment), and falling through to open()
        // would throw the misleading "requires the Tauri app" error on
        // EVERY frame change. Fail once, visibly, then stay quiet.
        if (nv.isWeb) {
          if (!st.webReopenWarned) {
            st.webReopenWarned = true;
            if (window.showToast) showToast('Vidéo "' + nv.path + '" : source non rechargeable après un rechargement de page — réimporte le fichier.', 'warn');
          }
          return;
        }
        var info = null;
        // Pre-indexed-era (.mov) optimized copies must NOT be reopened —
        // they'd silently keep the session on the old seek/respawn decode
        // path (live-caught 2026-07: zero INDEXED decodes in the log
        // because every layer's persisted optimizedPath predated the
        // indexed format). Dropping it here routes through the original +
        // background re-optimize into the indexed .mjpeg flavor.
        if (nv.optimizedPath && !/\.mjpeg$/.test(nv.optimizedPath)) nv.optimizedPath = null;
        if (nv.optimizedPath) {
          try { info = await open(nv.optimizedPath); }
          catch (e) { nv.optimizedPath = null; }
        }
        if (!info) {
          info = await open(nv.path);
          nv.codec = info.codec || nv.codec || '';
          _optimizeLayerMedia(li); // no-op if already all-intra or in flight
        }
        ld._nvSessionId = info.session_id; // runtime-only (not in exportJSON's layer whitelist)
        nv.frameCount = Number(info.frame_count);
        nv.width = info.width; nv.height = info.height; nv.fps = info.fps;
      }
      var target = _targetFor(nv, frame);
      if (target === st.lastShown) return;
      var tI = performance.now();
      var px = await frameBytes(ld._nvSessionId, target);
      _pushStat(_stats.ipcMs, performance.now() - tI);
      _jsCachePut(st, target, px);
      var tR = performance.now();
      if (window.SMEngineBridge) SMEngineBridge.registerImageRaw('nv:' + li, px, nv.width, nv.height);
      st.lastShown = target;
      window._sceneVersion++;
      if (window.SMEngineBridge && SMEngineBridge.isEnabled()) {
        if (_canFastRender(li)) SMEngineBridge.renderImageOnly(); else SMEngineBridge.renderNow();
      }
      _stats.serves++;
      _pushStat(_stats.renderMs, performance.now() - tR);
      _prefetch(li, target);
    } catch (e) {
      console.error('[native-video] layer ' + li + ' sync failed:', e);
    } finally {
      st.busy = false;
      if (st.pending != null) { var p = st.pending; st.pending = null; _layerFrameSync(li, p); }
    }
  }

  // ---- optimized media (the DaVinci Resolve pattern) ----
  // Long-GOP sources (H.264/HEVC/VP9…) make every seek cost a keyframe
  // jump + up-to-a-GOP of forward decode (70-200ms measured on real
  // footage). The pros don't out-engineer that — they transcode AWAY from
  // it: a one-time background conversion to all-intra MJPEG, where every
  // frame is independently decodable, makes seek cost = one frame decode
  // (~3-5ms) STRUCTURALLY. The original file stays untouched (and stays
  // the layer's persisted source of truth); the optimized copy lives in a
  // temp cache keyed by (path, size, mtime) and is re-created on demand
  // if the OS purges it. Codecs that are already all-intra skip this.
  var ALL_INTRA = { prores: 1, mjpeg: 1, dnxhd: 1, rawvideo: 1, qtrle: 1, huffyuv: 1, ffv1: 1, png: 1, v210: 1 };
  var _optimizing = {}; // source path -> true while a transcode runs
  function _isAllIntra(codec) {
    if (!codec) return false;
    for (var k in ALL_INTRA) if (codec.indexOf(k) !== -1) return true;
    return false;
  }
  async function _optimizeLayerMedia(li) {
    var ld = state.layers[li];
    if (!ld || !ld.nativeVideo) return;
    var nv = ld.nativeVideo;
    // No browser-side equivalent yet (would need a VideoEncoder re-mux
    // pass, out of scope for this pass) — a web session keeps paying its
    // source's own GOP-sized seek cost, same as Tauri's non-optimized
    // fallback. See native-video-bridge.js's own header comment.
    if (nv.isWeb) return;
    // A pre-indexed-era optimizedPath (.mov flavor) is treated as absent:
    // it decodes through the old seek/respawn path and misses the whole
    // point of optimization now (indexed random access). Re-optimize once
    // into the .mjpeg format; the stale .mov cache file is simply ignored.
    if (nv.optimizedPath && !/\.mjpeg$/.test(nv.optimizedPath)) nv.optimizedPath = null;
    if (nv.optimizedPath || _isAllIntra(nv.codec) || _optimizing[nv.path]) return;
    _optimizing[nv.path] = true;
    try {
      var res = await invoke('optimized_media_target', { path: nv.path });
      var target = res[0], exists = res[1];
      if (!exists) {
        // Indexed optimized media (raw MJPEG stream + .idx offsets sidecar,
        // built Rust-side): all-intra AND random-access — the decoder keeps
        // one persistent converter process per session, so ANY frame in ANY
        // order costs ~2-6ms with zero process respawns (the property that
        // makes scrubbing flat-cost). -q:v 3 near-visually-lossless; audio
        // still comes from the original source.
        await invoke('create_optimized_media', { src: nv.path, target: target });
      }
      // Swap the live session to the optimized copy. Guard against the
      // layer having been deleted/replaced during the transcode.
      ld = state.layers[li];
      if (!ld || !ld.nativeVideo || ld.nativeVideo.path !== nv.path) return;
      var info = await open(target);
      var oldSession = ld._nvSessionId;
      ld._nvSessionId = info.session_id;
      nv.optimizedPath = target; // persisted (nativeVideo is whitelisted wholesale)
      var st = _syncState(li);
      st.jsCache.clear(); st.jsBytes = 0; // cached bytes came from the old decode — refresh
      st.prefetchQueue = [];
      st.lastShown = -1;
      if (oldSession) close(oldSession).catch(function () {});
      if (window.showToast) showToast('Média optimisé : ' + (nv.path.split('/').pop()) + ' — scrub instantané');
      if (window.loadFrame) loadFrame(state.currentFrame);
    } catch (e) {
      // Was silent (console.warn only) — a failure here means this video
      // stays on the slow non-optimized decode path FOREVER (nothing else
      // ever retries it), which reads to the user as "import/scrub isn't
      // instant anymore" with zero visible explanation ("j'ai plus l'import
      // instantané", 2026-07, no error toast seen). Surfacing it doesn't
      // fix the underlying transcode failure, but at least makes it
      // diagnosable without opening devtools.
      console.warn('[native-video] optimization skipped for ' + nv.path + ':', e && e.message || e);
      if (window.showToast) showToast('Optimisation vidéo échouée pour ' + (nv.path.split('/').pop()) + ' (scrub restera plus lent) — ' + (e && e.message || e), 'warn');
    } finally {
      delete _optimizing[nv.path];
    }
  }

  // Current on-canvas rect of a video layer, in WORLD coordinates — the
  // exact math buildSceneJson (engine-bridge.js) uses to place the image
  // item: fit-to-canvas centered, then the layer's Motion transform
  // (static or keyframed) via transformImageRect. select-bridge uses this
  // for footage hit-testing and drag gestures; keeping the formula in ONE
  // reusable place here means the gesture and the picture can't drift.
  function displayRect(li) {
    var ld = state.layers[li];
    if (!ld || !ld.nativeVideo) return null;
    var nv = ld.nativeVideo;
    if (!nv.width || !nv.height) return null;
    var s = Math.min(state.canvasW / nv.width, state.canvasH / nv.height);
    var w = nv.width * s, h = nv.height * s;
    var rect = { x: (state.canvasW - w) / 2, y: (state.canvasH - h) / 2, width: w, height: h };
    var mm = window.SMMotion ? SMMotion.layerMotionAt(li, state.currentFrame) : null;
    if (mm) {
      var pivot = { x: rect.x + rect.width / 2 + mm.ax, y: rect.y + rect.height / 2 + mm.ay };
      rect = SMMotion.transformImageRect(rect, pivot, mm);
    }
    return rect;
  }

  // Transform-box geometry for a selected video layer (2026-07, "une vidéo
  // ou image est un objet comme les autres") — ONE shared computation for
  // both the drawn gizmo (engine-bridge buildTransformBoxItems' nv branch)
  // and the gesture hit-testing (select-bridge), so they can never
  // silently disagree (same discipline as motionHandlePositions/
  // computeHandles). Returns null when the layer isn't a video. Corners
  // are the DISPLAY rect's, spun around its center by the rect's own
  // rotation (displayRect composes the layer's Motion rotation into
  // rect.rotation since transformImageRect carries it); ring radius
  // matches the path-selection ring's own small-and-screen-constant
  // formula exactly.
  function transformBox(li) {
    var rect = displayRect(li);
    if (!rect) return null;
    var cx = rect.x + rect.width / 2, cy = rect.y + rect.height / 2;
    var rot = rect.rotation || 0;
    var a = rot * Math.PI / 180, cos = Math.cos(a), sin = Math.sin(a);
    function spin(x, y) { var dx = x - cx, dy = y - cy; return { x: cx + dx * cos - dy * sin, y: cy + dx * sin + dy * cos }; }
    var corners = {
      nw: spin(rect.x, rect.y), ne: spin(rect.x + rect.width, rect.y),
      se: spin(rect.x + rect.width, rect.y + rect.height), sw: spin(rect.x, rect.y + rect.height),
    };
    var zs = 1 / Math.max(0.0001, view.zoom);
    var ringRadius = Math.min(36 * zs, Math.max(rect.width, rect.height) * 0.3);
    return { rect: rect, center: { x: cx, y: cy }, rotation: rot, corners: corners, ringCenter: { x: cx, y: cy }, ringRadius: ringRadius };
  }

  // Instant import: opens a session and creates a nativeVideo layer —
  // called by images.js's Vidéo… button, either with a filesystem path
  // string (Tauri) or a File/Blob from the browser's file picker (no
  // Tauri — see open()'s own dispatch). Returns the layer index. A small
  // canvas-drawn thumbnail of frame 0 is produced for the media library
  // (the ONLY canvas use here in the Tauri path — the render path itself
  // never touches one; the web backend's own decode already round-trips
  // through a canvas internally, see _videoFrameToRgba).
  async function importAsLayer(source) {
    var info = await open(source);
    if (window.saveAllLayerFrames) saveAllLayerFrames();
    if (window.pushUndoLayers) pushUndoLayers();
    var name = _sourceName(source).replace(/\.[^.]+$/, '');
    var idx = createUserLayer(name);
    var ld = state.layers[idx];
    var isWeb = source instanceof Blob;
    ld.nativeVideo = {
      // A File/Blob can't survive JSON.stringify (project save) — stored
      // only as the display name for a web session, never as something a
      // reload could re-open (browsers have no durable file handle for
      // this without the separate File System Access API). Re-importing
      // after a reload is expected/required for a web session; the Tauri
      // path's real filesystem path keeps working across reloads as before.
      path: isWeb ? name : source,
      isWeb: isWeb,
      fps: info.fps,
      frameCount: Number(info.frame_count),
      width: info.width,
      height: info.height,
      offsetFrames: 0,
      codec: info.codec || '',
    };
    ld._nvSessionId = info.session_id;
    // Fire-and-forget: transcode long-GOP sources to all-intra in the
    // background and swap the session when ready — import stays instant.
    _optimizeLayerMedia(idx);
    if (Number(info.frame_count) > state.totalFrames && window.SM && SM.setTotalFrames) SM.setTotalFrames(Number(info.frame_count));
    // thumbnail for the Médias panel
    try {
      var px = await frameBytes(info.session_id, 0);
      var tc = document.createElement('canvas');
      var tw = 96, th = Math.max(1, Math.round(96 * info.height / info.width));
      tc.width = info.width; tc.height = info.height;
      tc.getContext('2d').putImageData(new ImageData(new Uint8ClampedArray(px), info.width, info.height), 0, 0);
      var sc = document.createElement('canvas'); sc.width = tw; sc.height = th;
      sc.getContext('2d').drawImage(tc, 0, 0, tw, th);
      if (window.SMMediaLibrary) SMMediaLibrary.addEntry(name, 'video', sc.toDataURL('image/jpeg', 0.7), ld.name);
    } catch (e) { /* thumbnail is cosmetic — import already succeeded */ }
    if (window.activateUL) activateUL(idx);
    if (window.loadFrame) loadFrame(state.currentFrame);
    if (window.updateUI) updateUI();
    if (window.showToast) showToast('Vidéo native : ' + name + ' (' + info.width + '×' + info.height + ', ' + Number(info.frame_count) + ' images) — import instantané');
    return idx;
  }

  // USER-FACING TEST ENTRY (experimental): from the dev console —
  //   await SMNativeVideo.attachToPlayhead('/chemin/video.mov')
  // then scrub the timeline: the video follows the playhead as the
  // rotoscopy reference, decoded natively frame by frame. Detach with
  //   await SMNativeVideo.detachFromPlayhead()
  async function attachToPlayhead(path) {
    var info = await open(path);
    state.refMedia = {
      type: 'native',
      name: path.split('/').pop() + ' (natif)',
      src: path, // file path in the whitelisted src field — persists via exportJSON unchanged
      opacity: 1,
      visible: true,
      offsetFrames: 0,
      _sessionId: info.session_id,
      _frameCount: Number(info.frame_count),
      _dims: { w: info.width, h: info.height },
    };
    if (window.SMReference) SMReference.reload();
    return info;
  }
  async function detachFromPlayhead() {
    var r = state.refMedia;
    if (r && r.type === 'native') {
      if (r._sessionId) await close(r._sessionId);
      state.refMedia = null;
      if (window.SMReference) SMReference.reload();
    }
  }

  window.SMNativeVideo = {
    open: open,
    frameBytes: frameBytes,
    registerFrame: registerFrame,
    close: close,
    bench: bench,
    attachToPlayhead: attachToPlayhead,
    detachFromPlayhead: detachFromPlayhead,
    importAsLayer: importAsLayer,
    onFrameChanged: onFrameChanged,
    displayRect: displayRect,
    transformBox: transformBox,
    stats: stats,
    _refSync: _refSync,
    sessions: function () { return Object.assign({}, sessions); },
  };

  // ---- scripted auto-bench (see autobench_config in video_decode.rs) ----
  // Runs the full pipeline bench without human interaction when the app
  // was launched with NEMO_AUTOBENCH pointing at a config file. Inert
  // otherwise (autobench_config returns null). The 8s delay lets the
  // WebGPU engine + wasm init settle so phase 3 measures the real
  // render path, not a race with startup.
  if (tauriOk()) {
    setTimeout(async function () {
      var cfg = null;
      try { cfg = await invoke('autobench_config'); } catch (e) { return; }
      if (!cfg || !cfg.videos || !cfg.videos.length) return;
      console.log('[autobench] starting,', cfg.videos.length, 'videos');
      var report = {
        startedAt: new Date().toISOString(),
        engineEnabled: !!(window.SMEngineBridge && SMEngineBridge.isEnabled && SMEngineBridge.isEnabled()),
        runs: [],
      };
      for (var i = 0; i < cfg.videos.length; i++) {
        try {
          report.runs.push(await bench(cfg.videos[i], cfg.opts || {}));
        } catch (e) {
          report.runs.push({ path: cfg.videos[i], error: String((e && e.message) || e) });
        }
      }
      try {
        await invoke('autobench_report', { report: JSON.stringify(report, null, 2) });
        console.log('[autobench] done');
      } catch (e) { console.error('[autobench] report write failed', e); }
    }, 8000);
  }
})();
