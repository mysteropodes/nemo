// ---- NATIVE VIDEO DECODE (EXPERIMENTAL, 2026-07) ----
// experimental/native-video-decode branch, graduated onto claude/motion-
// mode-v1. Real-time video decoding in the native Tauri process, streaming
// decoded RGBA8 frames to the webview over binary IPC (`tauri::ipc::
// Response` — raw ArrayBuffer on the JS side, no JSON/base64 encoding of
// pixel data).
//
// ARCHITECTURE v2 (2026-07): decodes via a PIPED ffmpeg CLI subprocess
// (the same binary already bundled as the Tauri sidecar for export/
// optimized-media — statically linked until the 2026-08-18 LGPL rebuild,
// see CLAUDE.md §7; now dynamically linked against Homebrew dylibs that
// scripts/bundle-ffmpeg-dylibs.py bundles at package time instead — the
// resolution logic in `process` is unaffected either way — resolved at runtime by
// std::env::current_exe().parent().join("ffmpeg"), confirmed empirically
// to be where Tauri places `externalBin` sidecars in both `tauri dev` and
// a built bundle: no `tauri::AppHandle` needed, and no async event-channel
// wrapper — `std::process::Command` gives direct synchronous access to the
// child's stdout, which raw-frame reading at potentially dozens of MB/s
// needs). REPLACES the previous v1 architecture, which linked ffmpeg's C
// libraries directly via the `video-rs`/`ffmpeg-sys-next` crates.
//
// Why the rewrite: v1's direct linking required bundling ~91 Homebrew
// dylibs (scripts/bundle-ffmpeg-dylibs.py) into every distributed build,
// and dynamically LINKING GPL-licensed libavcodec (built with libx264/
// libx265) into Nemo's own binary is a real GPL-linking concern for a
// commercial product. The bundled ffmpeg CLI binary is *itself* still GPL
// (confirmed via `ffmpeg -version`: `--enable-gpl --enable-libx264
// --enable-libx265`), but SPAWNING it as a subprocess is "mere
// aggregation," not linking — the standard, much safer pattern every
// commercial NLE that ships ffmpeg relies on. This does not eliminate the
// GPL dependency outright (a future LGPL-only decode-only ffmpeg build
// remains the real fix before a paid release — see CLAUDE.md §7), but it
// removes the linking risk AND the 91-dylib bundling step entirely:
// scripts/bundle-ffmpeg-dylibs.py is obsolete as of this rewrite.
//
// Bonus correctness fix, not just a licensing workaround: v1's scaler
// (video-rs's own, hardcoded to RGB24) FLATTENED alpha on sources that
// have it (ProRes 4444 etc.) — asserted by a test as a known limitation.
// Piping `-pix_fmt rgba` lets ffmpeg's OWN swscale produce real alpha
// bytes directly; the flatten-alpha test's expectation is INVERTED here
// (real alpha now, not always-255) — a genuine capability gain from this
// rewrite, not a side effect.
//
// What did NOT change: the per-session Mutex, the byte-bounded recently-
// decoded cache, the bidirectional readahead thread, the tail-of-stream
// backoff-retry robustness, the AE-style past-end clamp, and every public
// Tauri command's signature/behavior — all ported over from v1 nearly
// verbatim; see each function's own comment for what stayed vs changed.
// What DID change in the public per-frame contract: decode_frame_core's
// second return value used to be a numeric "frames walked during the
// post-seek forward decode" (v1 measured and bounded its OWN seek walk
// via direct av_seek_frame calls). ffmpeg's `-ss` (before `-i`, with
// `-accurate_seek` — the default) now does that walk-and-discard
// internally and doesn't expose a count; the return value is now a plain
// `seeked: bool` (was this frame served by a fresh process spawn, or the
// already-running sequential stream) — see decode_at's own comment.
//
// Performance contract per frame request (v2, measured targets, 1080p
// source): decode ~2-8ms sequential (codec-dependent, GOP-independent —
// no more per-row stride handling either: `-f rawvideo -pix_fmt rgba`
// writes tightly-packed bytes with no padding, so the RGB24→RGBA8 +
// stride-unpad loop that was v1's hottest path is GONE, not just
// simplified). A cold seek pays a full ffmpeg process spawn (~15-40ms
// process-start overhead observed) PLUS ffmpeg's own internal accurate-
// seek walk — see the perf tests for real numbers on this machine.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::ipc::Response;

// MODULE LAYOUT (T22/#2287): this file keeps the Tauri command surface,
// the single session registry (`VideoSessions`) and its lock helper. Each
// responsibility below lives in its own child module, moved without
// changing its algorithm:
// - `process`: the ffmpeg sidecar path and the piped rawvideo child.
// - `session`: per-session state, cache budget/insert and open.
// - `decode`: seek/sequential/tail-backoff decode of one frame.
// - `readahead`: the background cache-warming thread.
// - `indexed`: random-access decode of our own optimized-media streams.
// - `optimized`: the MJPEG stream indexer and optimized-media transcode.
// The `ffmpeg -i` stderr parser still lives in `crate::media_probe` (P28);
// `session::open_session_core` and `optimized::create_optimized_media_core`
// are its only callers.
mod decode;
// T12: the native export/preview decoder over the same sessions.
mod export_port;
mod indexed;
mod optimized;
mod process;
mod readahead;
mod session;

use decode::decode_frame_core;
pub(crate) use export_port::boxed_decoder;
#[cfg(test)]
use export_port::DesktopVideoDecoder;
use optimized::create_optimized_media_core;
use readahead::spawn_readahead;
use session::open_session_core;
pub use session::VideoSession;

static NEXT_SESSION_ID: AtomicU32 = AtomicU32::new(1);

/// Per-session Mutex, not one global lock — unchanged from v1 (the fix
/// for "latence pour plusieurs vidéos"): outer Mutex guards only the
/// HashMap's shape, each session's actual VideoSession is behind its own
/// Mutex so session A decoding never blocks session B.
pub struct VideoSessions(pub Mutex<HashMap<u32, Arc<Mutex<VideoSession>>>>);

/// Recovers from a poisoned lock instead of propagating the panic (audit
/// finding, 2026-07): plain `.lock().unwrap()` on every video-session
/// mutex meant a single panic anywhere inside `decode_frame_core` (which
/// does non-trivial byte-slicing on raw ffmpeg output) while holding a
/// session's lock would poison it permanently — every subsequent Tauri
/// command touching that session (or, for the outer HashMap mutex, ALL
/// sessions) would then itself panic on `.unwrap()`, turning one
/// transient decode bug into "video dead for the rest of the app's
/// lifetime". A poisoned mutex's guarded data isn't corrupted by the
/// mutex itself — only whatever partial write the panicking thread was
/// mid-way through — so recovering via `into_inner()` and continuing is
/// strictly better than a cascading permanent failure for this use case.
fn lock_or_recover<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl Default for VideoSessions {
    fn default() -> Self {
        VideoSessions(Mutex::new(HashMap::new()))
    }
}

#[derive(Serialize)]
pub struct VideoInfo {
    pub session_id: u32,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub frame_count: u64,
    pub duration_seconds: f64,
    pub codec: String,
}

#[tauri::command]
pub fn open_video_session(
    state: tauri::State<'_, VideoSessions>,
    path: String,
) -> Result<VideoInfo, String> {
    let (session, frame_count, duration_seconds) = open_session_core(&path)?;
    let (width, height, fps) = (session.width, session.height, session.fps);
    let codec = session.codec.clone();
    let id = NEXT_SESSION_ID.fetch_add(1, Ordering::Relaxed);
    lock_or_recover(&state.0).insert(id, Arc::new(Mutex::new(session)));

    Ok(VideoInfo {
        session_id: id,
        width,
        height,
        fps,
        frame_count,
        duration_seconds,
        codec,
    })
}

/// Decodes exactly one frame and returns its pixels as raw RGBA8 bytes —
/// `tauri::ipc::Response` reaches JS as an ArrayBuffer with no JSON/base64
/// step. Dimensions are fixed per session (returned once by
/// open_video_session), so the buffer is self-describing given the session.
#[tauri::command]
pub fn decode_video_frame(
    state: tauri::State<'_, VideoSessions>,
    session_id: u32,
    frame_index: i64,
) -> Result<Response, String> {
    // Clone the Arc and drop the map lock immediately — unchanged from v1.
    let session_arc = {
        let sessions = lock_or_recover(&state.0);
        sessions
            .get(&session_id)
            .cloned()
            .ok_or_else(|| format!("no session {session_id}"))?
    };
    let (result, indexed) = {
        let mut s = lock_or_recover(&session_arc);
        let r = decode_frame_core(&mut s, frame_index);
        (r, s.indexed.is_some())
    };
    // Warm the cache BEHIND this response (mpv pattern) — unchanged from
    // v1. Indexed sessions skip it entirely: every frame is a flat ~2-6ms
    // random access, so there's nothing for a warming thread to amortize.
    if result.is_ok() && !indexed {
        spawn_readahead(session_arc, frame_index);
    }
    result.map(|(rgba, _seeked)| Response::new(rgba))
}

#[tauri::command]
pub fn close_video_session(
    state: tauri::State<'_, VideoSessions>,
    session_id: u32,
) -> Result<(), String> {
    let removed = lock_or_recover(&state.0).remove(&session_id);
    if let Some(arc) = removed {
        shutdown_session(&mut lock_or_recover(&arc));
    }
    Ok(())
}

/// Stop readahead and kill+reap every child process of `s`.
fn shutdown_session(s: &mut VideoSession) {
    s.readahead_active = false; // ask the readahead thread to stop (waits at most one frame read)
    if let Some(mut p) = s.proc.take() {
        let _ = p.child.kill();
        let _ = p.child.wait(); // reap — avoid leaking a zombie ffmpeg process on every close
    }
    if let Some(seq) = s.indexed.as_mut() {
        if let Some(mut conv) = seq.conv.take() {
            // Dropping jpeg_tx (inside conv) ends the writer thread;
            // kill+reap the persistent converter like any other child.
            let _ = conv.child.kill();
            let _ = conv.child.wait();
        }
    }
}

/// Where the all-intra "optimized media" for `path` lives (Resolve's
/// pattern) — unchanged from v1.
#[tauri::command]
pub fn optimized_media_target(path: String) -> Result<(String, bool), String> {
    use std::hash::{Hash, Hasher};
    let meta = std::fs::metadata(&path).map_err(|e| format!("stat {path}: {e}"))?;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut h);
    meta.len().hash(&mut h);
    if let Ok(m) = meta.modified() {
        if let Ok(d) = m.duration_since(std::time::UNIX_EPOCH) {
            d.as_secs().hash(&mut h);
        }
    }
    let dir = std::env::temp_dir().join("nemo-optimized");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    // .mjpeg = the indexed raw-stream format (see create_optimized_media).
    // Previous .mov-flavored caches are simply ignored and re-created —
    // the optimized cache is disposable by design.
    let target = dir.join(format!("{:016x}.mjpeg", h.finish()));
    // "exists" must mean "usable": the stream without its sidecar index
    // (e.g. an interrupted transcode) is not.
    let exists = target.exists()
        && std::path::Path::new(&format!("{}.idx", target.to_string_lossy())).exists();
    Ok((target.to_string_lossy().to_string(), exists))
}

/// Transcode a long-GOP source into the indexed optimized-media format
/// (raw MJPEG stream + .idx sidecar — see create_optimized_media_core).
/// Async + spawn_blocking: transcodes take seconds to minutes and must not
/// tie up an IPC worker.
#[tauri::command]
pub async fn create_optimized_media(src: String, target: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || create_optimized_media_core(&src, &target))
        .await
        .map_err(|e| format!("optimize task panicked: {e}"))?
}

// ---- headless auto-bench plumbing (unchanged from v1) ----
#[tauri::command]
pub fn autobench_config() -> Option<serde_json::Value> {
    let path = std::env::var("NEMO_AUTOBENCH").ok()?;
    let text = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&text).ok()
}

#[tauri::command]
pub fn autobench_report(report: String) -> Result<(), String> {
    let out = std::env::var("NEMO_AUTOBENCH_OUT")
        .unwrap_or_else(|_| "/tmp/nemo-autobench-report.json".to_string());
    std::fs::write(&out, &report).map_err(|e| e.to_string())?;
    eprintln!("[autobench] report written to {out}");
    Ok(())
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_cache;
#[cfg(test)]
mod tests_decode;
#[cfg(test)]
mod tests_export_port;
#[cfg(test)]
mod tests_indexed;
// T12 / #2232 point 6: real sources through the native host (Metal + sidecar).
#[cfg(all(test, target_os = "macos"))]
mod tests_native_media_host;
#[cfg(test)]
mod tests_perf;
#[cfg(test)]
mod tests_sessions;
