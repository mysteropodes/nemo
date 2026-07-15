// ---- NATIVE VIDEO DECODE (EXPERIMENTAL, 2026-07) ----
// experimental/native-video-decode branch, graduated onto claude/motion-
// mode-v1. Real-time video decoding in the native Tauri process, streaming
// decoded RGBA8 frames to the webview over binary IPC (`tauri::ipc::
// Response` — raw ArrayBuffer on the JS side, no JSON/base64 encoding of
// pixel data).
//
// ARCHITECTURE v2 (2026-07): decodes via a PIPED ffmpeg CLI subprocess
// (the same statically-linked binary already bundled as the Tauri sidecar
// for export/optimized-media — resolved at runtime by
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
use std::io::Read;
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::ipc::Response;

static NEXT_SESSION_ID: AtomicU32 = AtomicU32::new(1);

/// Resolves the bundled ffmpeg sidecar's actual path — Tauri places
/// `externalBin` entries alongside the main executable (confirmed
/// empirically: `target/release/ffmpeg` and `target/debug/ffmpeg` both
/// exist next to the `nemo`/`nemo_lib`-linked binary), stripped of the
/// `-<target-triple>` suffix `tauri.conf.json`'s `binaries/ffmpeg-*`
/// source file carries. No `tauri::AppHandle` needed — every caller here
/// is deep in per-frame session logic, far from any command's handle.
fn ffmpeg_path() -> std::path::PathBuf {
    // Test override: `cargo test`'s current_exe() is a `target/*/deps/
    // nemo_lib-<hash>` test binary, whose parent dir has no sidecar next
    // to it (only `target/release/ffmpeg` and `target/debug/ffmpeg`,
    // confirmed empirically, do) — tests point this at the bundled binary
    // directly instead. Inert/unset in the real running app.
    if let Ok(p) = std::env::var("NEMO_TEST_FFMPEG_PATH") {
        return std::path::PathBuf::from(p);
    }
    let mut p = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("ffmpeg")))
        .unwrap_or_else(|| std::path::PathBuf::from("ffmpeg"));
    if cfg!(windows) {
        p.set_extension("exe");
    }
    p
}

/// One running (or just-spawned) ffmpeg decode process for a session.
/// `next_frame` is which frame index the process's stdout will yield
/// NEXT — sequential requests (frame N+1 right after N) need no respawn,
/// matching v1's "cheap sequential fast path" exactly.
struct FfmpegProc {
    child: Child,
    stdout: std::io::BufReader<ChildStdout>,
    next_frame: i64,
}

/// Persistent JPEG→RGBA converter for INDEXED sessions (see IndexedSeq).
/// One long-lived ffmpeg child per session: JPEG bytes go in via stdin
/// (through a dedicated writer thread — writes must never block the
/// session mutex holder, since a full pipe buffer would deadlock against
/// our own stdout read), raw RGBA frames come out via stdout. Decoding
/// any frame in any order costs one pipe round-trip (~2-6ms measured),
/// with ZERO process respawns — the property that makes random scrubbing
/// on optimized media flat-cost.
struct ConvProc {
    child: Child,
    /// Sending a JPEG here wakes the writer thread; dropping the sender
    /// (session close) makes that thread exit and close the child's stdin.
    jpeg_tx: std::sync::mpsc::Sender<Vec<u8>>,
    /// Decoded RGBA frames arrive here from a dedicated reader thread —
    /// receiving THROUGH A CHANNEL (with recv_timeout) instead of a direct
    /// blocking read is what makes the whole indexed path hang-proof: the
    /// ffmpeg stdin pipeline's internal frame lag is CONTENT-DEPENDENT
    /// (measured: 2 frames at 320x240, 3 at 1080p, with hiccups), so any
    /// protocol that assumes a fixed lag can block forever on a blocking
    /// read — which is exactly what froze the app on import (live-caught
    /// 2026-07, "ça plante à l'import des fichiers": first indexed decode
    /// stuck in read_exact holding the session mutex).
    frame_rx: std::sync::mpsc::Receiver<Vec<u8>>,
}

/// Random-access frame index over a raw MJPEG stream file — the format
/// this module's create_optimized_media produces (concatenated baseline
/// JPEGs + a sidecar `.idx` JSON with per-frame byte offsets). Built
/// because live profiling showed the ONLY remaining scrub cost on
/// all-intra optimized media was the ~20ms process-spawn tax paid on
/// every non-sequential jump ("fais en sorte que le scrub soit parfait"):
/// with byte offsets known up front, any frame is one file read + one
/// round-trip through the persistent converter, no seeking of any kind.
struct IndexedSeq {
    file: std::fs::File,
    /// count+1 entries — offsets[i]..offsets[i+1] is frame i's JPEG.
    offsets: Vec<u64>,
    conv: Option<ConvProc>,
    /// Sentinel pad: a same-dimensions JPEG whose decoded pixels carry a
    /// recognizable signature (see sentinel_pad / is_sentinel_frame),
    /// generated once at session open. The ffmpeg stdin pipeline won't
    /// emit a frame until a content-dependent number of FURTHER frames
    /// enter (measured lag: 2 at 320x240, 3 at 1080p, non-constant — no
    /// flag combination removes it). Each request therefore chases the
    /// target with a few sentinel pads and the reader simply DISCARDS
    /// every sentinel it recognizes, returning the first real frame: no
    /// lag bookkeeping, self-synchronizing, and immune to the lag
    /// changing between ffmpeg builds or resolutions.
    pad: Vec<u8>,
}

/// Number of sentinel pads sent after each target JPEG. Must exceed the
/// pipeline's worst observed lag (3); timeouts in indexed_decode self-heal
/// by sending another volley if a build/resolution ever exceeds it.
const SENTINEL_PADS_PER_REQUEST: usize = 4;

/// Paints the sentinel signature into row 0 of an RGBA buffer: four
/// 8-pixel blocks of alternating saturated magenta/green. 8-wide blocks
/// straddle JPEG's chroma subsampling (a 1-pixel alternation would smear
/// into gray), and block CENTERS are what is_sentinel_frame samples, so
/// q2 compression noise (±10ish) never gets near the ±80 tolerance.
fn paint_sentinel_signature(rgba: &mut [u8], width: usize) {
    for block in 0..4usize {
        let (r, g, b) = if block % 2 == 0 { (255u8, 0u8, 255u8) } else { (0, 255, 0) };
        for i in 0..8usize {
            let x = block * 8 + i;
            if x >= width {
                return;
            }
            rgba[x * 4] = r;
            rgba[x * 4 + 1] = g;
            rgba[x * 4 + 2] = b;
            rgba[x * 4 + 3] = 255;
        }
    }
}

/// Does this decoded frame carry the sentinel signature? Samples the 4
/// centers (pixels 3-4 of each 8-pixel block) with a wide ±80 tolerance —
/// real footage reproducing four alternating fully-saturated magenta/green
/// blocks within that tolerance in its top-left 32 pixels is not a
/// realistic false-positive risk for preview media.
fn is_sentinel_frame(rgba: &[u8], width: usize) -> bool {
    if width < 32 || rgba.len() < 32 * 4 {
        return false;
    }
    for block in 0..4usize {
        let (er, eg, eb) = if block % 2 == 0 { (255i32, 0i32, 255i32) } else { (0, 255, 0) };
        for &x in &[block * 8 + 3, block * 8 + 4] {
            let p = &rgba[x * 4..x * 4 + 3];
            if (p[0] as i32 - er).abs() > 80 || (p[1] as i32 - eg).abs() > 80 || (p[2] as i32 - eb).abs() > 80 {
                return false;
            }
        }
    }
    true
}

/// Encode the sentinel pad JPEG at the given dimensions via a one-shot
/// ffmpeg run (EOF-terminated stdin — the mode with no pipeline-lag
/// surprises, verified reliable in the harness). Black frame + signature
/// row: tiny (~5-15KB at 1080p) and near-instant to decode.
fn sentinel_pad(width: u32, height: u32) -> Result<Vec<u8>, String> {
    use std::io::Write;
    let (w, h) = (width as usize, height as usize);
    let mut raw = vec![0u8; w * h * 4];
    for px in raw.chunks_exact_mut(4) {
        px[3] = 255;
    }
    paint_sentinel_signature(&mut raw, w);
    let mut child = Command::new(ffmpeg_path())
        .args([
            "-v", "error",
            "-f", "rawvideo", "-pix_fmt", "rgba", "-s", &format!("{width}x{height}"), "-i", "-",
            "-frames:v", "1", "-c:v", "mjpeg", "-q:v", "2", "-f", "mjpeg", "-",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("sentinel encode spawn: {e}"))?;
    let mut stdin = child.stdin.take().ok_or("sentinel stdin unavailable")?;
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(&raw);
        // dropping stdin closes the pipe — EOF terminates the encode
    });
    let out = child.wait_with_output().map_err(|e| format!("sentinel encode: {e}"))?;
    let _ = writer.join();
    if !out.status.success() || out.stdout.is_empty() {
        return Err("sentinel encode produced no output".into());
    }
    Ok(out.stdout)
}

pub struct VideoSession {
    path: String,
    width: u32,
    height: u32,
    fps: f64,
    frame_bytes: usize, // width*height*4, cached — the exact tightly-packed rawvideo/rgba frame size
    /// Total frames, derived from container duration × fps (ffmpeg's `-i`
    /// info dump never reliably prints a frame count — v1 only fell back
    /// to this math for WebM specifically; v2 uses it universally since
    /// there's no other source of truth available without ffprobe, which
    /// isn't bundled — see open_session_core's own comment).
    frame_count: i64,
    proc: Option<FfmpegProc>,
    /// Recently-decoded frame cache, most-recent-first — unchanged from
    /// v1 (see cache_budget_for). Byte-bounded, not frame-count-
    /// bounded: frame size varies 8x between 1080p and 4K.
    cache: std::collections::VecDeque<(i64, Vec<u8>)>,
    readahead_active: bool,
    readahead_goal: i64,
    /// Last frame index the FOREGROUND asked for — lets decode_at tell a
    /// frame-by-frame backward step (previous request minus a few) from a
    /// fast scrub fling (arbitrary large jumps), which need opposite
    /// respawn strategies (see the scrub-prefetch comment in decode_at).
    last_foreground_request: i64,
    /// Some(..) when this session is an indexed optimized-media stream
    /// (random access, persistent converter — see IndexedSeq). All the
    /// seek/respawn/readahead machinery above is bypassed entirely.
    indexed: Option<IndexedSeq>,
    /// Source codec name (lowercase, e.g. "h264", "prores") — lets the JS
    /// side decide whether background media optimization (transcode to
    /// all-intra) is worth it, and gates bidirectional readahead the same
    /// way v1 did.
    codec: String,
}

/// Per-session cache floor/ceiling. v1 used a flat 66MB, which live
/// profiling (2026-07, "le temps réel n'est pas encore là") exposed as the
/// next bottleneck once the readahead tug-of-war was fixed: at 1080p a
/// frame is ~8MB, so a flat 66MB holds ~8 frames — the readahead window
/// (READAHEAD_DEPTH ahead) exactly fills it, and warming the LAST forward
/// frame evicts the FIRST one, i.e. precisely the frame the playhead asks
/// for next. Foreground then misses the cache with the process sitting
/// ahead (negative gap), paying a full ~25ms respawn nearly every tick —
/// 522 foreground respawns in one capture, one per session per displayed
/// frame across 3 videos. The budget is now sized from the session's own
/// frame_bytes so the full window (current + DEPTH ahead + BACK behind +
/// slack) always fits: see cache_budget_for().
const FRAME_CACHE_MIN_BYTES: usize = 66 * 1024 * 1024;
const FRAME_CACHE_MAX_BYTES: usize = 320 * 1024 * 1024;
/// How far past the last requested frame the readahead thread decodes,
/// and how far BEHIND it — unchanged from v1.
const READAHEAD_DEPTH: i64 = 8;
const READAHEAD_BACK: i64 = 3;

/// Per-session cache budget: the full readahead window (1 current +
/// DEPTH forward + BACK back) plus a few frames of slack, clamped to
/// [66MB, 320MB]. 1080p (~8MB/frame): ~128MB. 4K (~33MB/frame): hits the
/// 320MB ceiling (~9 frames — window still fits, barely). The ceiling
/// bounds worst-case memory at ~1GB for 3 simultaneous 4K sessions.
fn cache_budget_for(frame_bytes: usize) -> usize {
    let window = (1 + READAHEAD_DEPTH + READAHEAD_BACK) as usize + 4;
    (frame_bytes * window).clamp(FRAME_CACHE_MIN_BYTES, FRAME_CACHE_MAX_BYTES)
}
/// Shared by decode_at (actual reuse decision) and spawn_readahead (peeking
/// whether a candidate target would cost a respawn, without paying for one).
const SMALL_FORWARD_REUSE_TOLERANCE: i64 = 12;

/// Would decoding `target` on session `s` require killing+respawning the
/// ffmpeg process, or can the running one serve it? Mirrors decode_at's own
/// reuse decision exactly (backward gaps and gaps beyond tolerance always
/// need a respawn) — used by spawn_readahead to avoid the tug-of-war where
/// a readahead-triggered respawn fights foreground playback for the same
/// session's process (see spawn_readahead's own comment for the full story).
fn would_need_respawn(s: &VideoSession, target: i64) -> bool {
    match &s.proc {
        Some(p) => !(0..=SMALL_FORWARD_REUSE_TOLERANCE).contains(&(target - p.next_frame)),
        None => true,
    }
}

/// Insert a decoded frame into the session cache (skipping if that index is
/// already present) and evict oldest entries past the session's budget.
/// Shared by foreground decode, the readahead thread, and decode_at's
/// walk-prefetch (see below) so eviction policy lives in exactly one place.
fn cache_insert(s: &mut VideoSession, idx: i64, bytes: Vec<u8>) {
    if s.cache.iter().any(|(i, _)| *i == idx) {
        return;
    }
    let budget = cache_budget_for(s.frame_bytes);
    s.cache.push_front((idx, bytes));
    let mut total: usize = s.cache.iter().map(|(_, b)| b.len()).sum();
    while total > budget && s.cache.len() > 1 {
        if let Some((_, evicted)) = s.cache.pop_back() {
            total -= evicted.len();
        }
    }
}

/// How many frames BEFORE a respawn target decode_at over-seeks by, so the
/// walk from origin to target populates the cache with the target's
/// immediate predecessors. Motivated by live scrub profiling (2026-07, "la
/// lecture est vraiment bien, le scrub moyen"): backward scrubbing steps
/// hit a full ~23ms respawn PER STEP per session (67→64→57→56→55 in one
/// capture, all seeked=true, ×3 videos), because a pipe can't rewind and
/// each backward step landed just outside whatever the cache held. Spawning
/// a few frames early converts the NEXT several backward steps into cache
/// hits — one respawn amortized over `prefetch` scrub steps instead of one
/// respawn each. Sized from the session's cache capacity so the prefetched
/// frames never blow the budget (and shrink at 4K where frames are huge):
/// a third of what fits, clamped to [2, 6].
fn scrub_prefetch_for(s: &VideoSession) -> i64 {
    let fit = (cache_budget_for(s.frame_bytes) / s.frame_bytes.max(1)) as i64;
    (fit / 3).clamp(2, 6)
}
/// Tail-of-stream backoff-retry cap — unchanged from v1's spirit (widen
/// the re-seek origin until it reaches 0), expressed here as a max
/// number of widening attempts instead of a frame-walk count, since v2
/// doesn't count ffmpeg's internal walk.
const MAX_TAIL_BACKOFF_ATTEMPTS: u32 = 8;

/// No longer used to gate readahead direction (see spawn_readahead's own
/// comment — that distinction stopped mattering in v2's cost model). Kept
/// for its test coverage (documents which codec names this module
/// recognizes as all-intra) and as a small utility if a future tuning
/// pass needs it again; JS already has its own independent equivalent
/// (`_isAllIntra` in native-video-bridge.js) for the optimized-media
/// transcode decision, so this one is genuinely unused by production
/// Rust code right now.
#[allow(dead_code)]
fn is_all_intra_codec(codec: &str) -> bool {
    ["mjpeg", "prores", "dnxhd", "rawvideo", "qtrle", "huffyuv", "ffv1", "png", "v210"]
        .iter()
        .any(|k| codec.contains(k))
}

/// Kills any running process for this session and spawns a fresh one that
/// yields rawvideo RGBA8 frames starting at `frame_index`. `-ss` (before
/// `-i`) with `-accurate_seek` (ffmpeg's default since 2.1 — not passed
/// explicitly, relying on the well-established default rather than
/// pinning a flag that could regress silently on some exotic build) seeks
/// to the nearest keyframe at/before the target and decodes-and-discards
/// forward internally, so the FIRST frame this process's stdout yields is
/// already the exact target — no per-frame walk-and-discard loop needed
/// on our side for the normal case (see decode_at for the tail-retry
/// exception, which DOES need one).
fn spawn_at(s: &mut VideoSession, frame_index: i64) -> Result<(), String> {
    if let Some(mut p) = s.proc.take() {
        let _ = p.child.kill();
        let _ = p.child.wait(); // reap — avoid leaking zombie processes across many scrub-driven respawns
    }
    let mut cmd = Command::new(ffmpeg_path());
    cmd.args(["-v", "error", "-nostdin"]);
    if frame_index > 0 {
        let seconds = frame_index as f64 / s.fps;
        cmd.args(["-ss", &format!("{seconds:.6}")]);
    }
    cmd.args(["-i", &s.path, "-f", "rawvideo", "-pix_fmt", "rgba", "-vsync", "0", "-an", "-sn", "-"]);
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| format!("spawn ffmpeg: {e}"))?;
    let stdout = child.stdout.take().ok_or("ffmpeg spawned with no stdout pipe")?;
    // Buffer sized to one full frame (or 64KB, whichever is larger) so a
    // single read_exact() below is satisfied by one or two underlying
    // syscalls even at 4K, not dozens of small reads.
    let cap = s.frame_bytes.max(64 * 1024);
    s.proc = Some(FfmpegProc {
        child,
        stdout: std::io::BufReader::with_capacity(cap, stdout),
        next_frame: frame_index,
    });
    Ok(())
}

/// Blocking read of exactly one tightly-packed RGBA8 frame from the
/// session's current process. `-f rawvideo -pix_fmt rgba` writes frames
/// with NO row padding (unlike the AVFrame plane stride v1 had to unpad
/// row-by-row) — width*height*4 bytes, done, no conversion loop at all.
fn read_one_frame(s: &mut VideoSession) -> Result<Vec<u8>, String> {
    let proc = s.proc.as_mut().ok_or("no active ffmpeg process")?;
    let mut buf = vec![0u8; s.frame_bytes];
    match proc.stdout.read_exact(&mut buf) {
        Ok(()) => {
            proc.next_frame += 1;
            Ok(buf)
        }
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
            Err("decode: stream exhausted".to_string())
        }
        Err(e) => Err(format!("read: {e}")),
    }
}

/// The actual seek+decode work, cache-lookup-free — called by
/// decode_frame_core on a cache miss AND by the readahead thread (which
/// must never take the cache-hit shortcut: its job is to advance actual
/// decode progress while filling the cache). `frame_index` must already
/// be clamped by the caller. Returns `seeked`: true if this request
/// caused a fresh ffmpeg process spawn (cold — new process start +
/// ffmpeg's own internal seek walk), false if it was served by the
/// already-running sequential stream (the cheap fast path). This is v2's
/// replacement for v1's numeric "frames walked" count — ffmpeg's own
/// `-ss`/accurate_seek walk happens inside the child process now, where
/// we can't observe or bound it directly (see the module doc comment).
fn decode_at(s: &mut VideoSession, frame_index: i64, caller: &str) -> Result<(Vec<u8>, bool), String> {
    // Tail robustness (ported from v1 — caught live 2026-07, "decode:
    // stream exhausted" aborting scrubs near the end): requesting a frame
    // at/near the tail can hit EOF because frame_count is DERIVED
    // (duration×fps), which can overshoot what the stream truly contains.
    // Widening-backoff retry: spawn a bit EARLIER than the target and
    // walk forward ourselves (unlike the normal case, where ffmpeg's own
    // seek lands exactly on target and we never walk anything) — whatever
    // frame that walk last produced when the stream ends is the honest
    // "nearest earlier" result.
    // Small-forward reuse ("un peu de régression... chaque seek respawn un
    // process complet, même pour un pas d'une frame" — caught LIVE 2026-07
    // watching the user's own real scrub in a running dev session: every
    // cache miss cost a flat ~20-26ms regardless of how close the target
    // was to the already-running process, because v2 unconditionally
    // killed+respawned on ANY `frame_index != next_frame`). A fresh spawn
    // pays ~15-90ms of pure OS/ffmpeg-init overhead (measured) — for a
    // target just a handful of frames past the running process's own
    // position, reading-and-discarding from THAT process is cheaper than
    // paying spawn tax again. Forward only: a pipe can't rewind, so this
    // never helps backward steps (still a real seek/respawn — the
    // architecture's honest trade-off vs v1's in-process av_seek_frame).
    // Tolerance picked from the measured numbers, not guessed: steady-
    // state per-frame decode is ~1.5ms (1080p) to ~6.7ms (4K); spawn
    // overhead is ~20-90ms — the break-even frame count (spawn_ms /
    // frame_ms) lands around 10-15 at both ends, so one constant works
    // without per-resolution tuning. (Constant now module-level — see
    // would_need_respawn, which mirrors this exact decision for
    // spawn_readahead's benefit.)
    let mut origin = frame_index;
    // DIAG (2026-07 — "j'ai mis 3 vidéos ça a du mal à lire en temps réel"):
    // reports the gap this decision was based on whenever a REAL respawn is
    // about to happen (proc exists but the gap exceeded tolerance, or
    // there's no process at all) — a genuinely bugged first version of this
    // logged on the CHEAP gap=0 path too (origin ends up equal to
    // frame_index in that case regardless of whether reuse "did anything",
    // since there was nothing to skip), which is why the isolated 3-session
    // Rust repro below looked alarming at a glance but was actually fine
    // (2.5% respawn rate — see three_simultaneous_sessions_playback_
    // mostly_avoids_respawns). This version distinguishes the two.
    let gap_for_diag = s.proc.as_ref().map(|p| frame_index - p.next_frame);
    if let Some(p) = &s.proc {
        let gap = frame_index - p.next_frame;
        if gap >= 0 && gap <= SMALL_FORWARD_REUSE_TOLERANCE {
            origin = p.next_frame;
        }
    }
    if origin == frame_index && (s.proc.is_none() || gap_for_diag.is_some_and(|g| !(0..=SMALL_FORWARD_REUSE_TOLERANCE).contains(&g))) {
        eprintln!("[video-decode] DIAG real-respawn: caller={caller} target={frame_index} gap={gap_for_diag:?}");
        // A real respawn is unavoidable. If this request is a small
        // BACKWARD STEP from the previous foreground request (frame-by-
        // frame arrow-key/wheel scrubbing), over-seek by a few frames so
        // the walk below caches the target's immediate predecessors —
        // the next several backward steps become cache hits instead of
        // one ~23ms respawn each (see scrub_prefetch_for). Costs a few
        // extra sequential decodes (~2ms each at 1080p) on a path already
        // paying ~20ms of spawn tax.
        //
        // NOT applied to large jumps: live capture 2026-07 ("pour le
        // scrub c'est pire") showed fast drag-scrubbing jumps 13-120
        // frames per tick — far outside any prefetch window, so the
        // prefetch never paid off but its extra decodes taxed EVERY tick.
        // Large jumps now spawn exactly at the target for minimum
        // latency; only genuine step-backs get the widened origin.
        // (No "is the process already inside the widened window" check
        // needed: any such position would be a 0..=prefetch forward gap,
        // and prefetch < SMALL_FORWARD_REUSE_TOLERANCE, so the reuse
        // branch above already claimed those.)
        let prefetch = scrub_prefetch_for(s);
        let step_back = s.last_foreground_request - frame_index;
        if caller == "foreground" && step_back > 0 && step_back <= prefetch {
            origin = (frame_index - prefetch).max(0);
        }
    }
    let mut backoff: i64 = 0;
    let mut attempts: u32 = 0;
    loop {
        let need_spawn = match &s.proc {
            Some(p) => p.next_frame != origin,
            None => true,
        };
        if need_spawn {
            spawn_at(s, origin)?;
        }
        let mut cur = origin;
        let mut last_good: Option<Vec<u8>> = None;
        loop {
            match read_one_frame(s) {
                Ok(bytes) => {
                    if cur >= frame_index {
                        // `seeked` means "did this cost a fresh process
                        // spawn" — NOT "did we walk/discard any frames".
                        // The small-forward-reuse fix specifically wants
                        // "reused the running process, no respawn" to
                        // report false even though it DID walk a few
                        // frames forward — that's the whole point of the
                        // optimization, and the earlier `|| cur > origin`
                        // clause here mislabeled exactly that case as
                        // "seeked" (caught by
                        // small_forward_gap_reuses_running_process_no_respawn:
                        // reuse worked, cheap ~2ms, but was reported as a
                        // spawn regardless).
                        return Ok((bytes, need_spawn));
                    }
                    // Cache the frames the walk passes through (both the
                    // scrub-prefetch window and small-forward-reuse skips):
                    // they're exactly the target's neighbors, i.e. what a
                    // scrub asks for next. One ~2-8ms memcpy per frame on a
                    // path already paying spawn tax.
                    cache_insert(s, cur, bytes.clone());
                    last_good = Some(bytes);
                    cur += 1;
                }
                Err(e) => {
                    if let Some(prev) = last_good {
                        eprintln!(
                            "[video-decode] stream ended before frame {frame_index} ({e}) — returning nearest earlier frame (origin {origin})"
                        );
                        return Ok((prev, true));
                    }
                    if origin == 0 || attempts >= MAX_TAIL_BACKOFF_ATTEMPTS {
                        return Err(e);
                    }
                    attempts += 1;
                    backoff = if backoff == 0 { 15 } else { backoff * 4 };
                    origin = (frame_index - backoff).max(0);
                    eprintln!(
                        "[video-decode] tail retry: re-seeking to {origin} for target {frame_index} ({e})"
                    );
                    s.proc = None; // force respawn at the new origin next outer pass
                    break;
                }
            }
        }
    }
}

// Core per-frame decode, tauri-free (see open_session_core). Returns the
// tightly-packed RGBA8 buffer (`width*height*4`) plus whether this
// request was cold (fresh process spawn) or hit the running sequential
// stream — see decode_at's doc comment for why this replaced v1's
// numeric walked-frame count.
fn decode_frame_core(s: &mut VideoSession, frame_index: i64) -> Result<(Vec<u8>, bool), String> {
    // Clamp to the stream's real range, AE-style: scrubbing to (or past)
    // the last frame HOLDS the last frame instead of erroring — unchanged
    // from v1.
    let frame_index = frame_index.clamp(0, (s.frame_count - 1).max(0));

    // Recently-decoded cache hit: skip spawn+decode entirely. Linear scan
    // (small deque) — unchanged from v1, including the next_frame-
    // corruption trap it fixed: a cache hit must NEVER touch the running
    // process's position, only v2's equivalent is `proc.next_frame`
    // rather than a bare session field, same invariant.
    if let Some(pos) = s.cache.iter().position(|(idx, _)| *idx == frame_index) {
        let (_, bytes) = s.cache.remove(pos).unwrap();
        let out = bytes.clone();
        s.cache.push_front((frame_index, bytes));
        s.last_foreground_request = frame_index;
        eprintln!("[video-decode] frame={frame_index} CACHE HIT (skipped spawn+decode)");
        return Ok((out, false));
    }

    // Indexed optimized media: pure random access via the persistent
    // converter — no seek, no respawn, no readahead machinery at all.
    if s.indexed.is_some() {
        let t0 = std::time::Instant::now();
        let rgba = indexed_decode(s, frame_index)?;
        let decode_ms = t0.elapsed().as_secs_f64() * 1000.0;
        s.last_foreground_request = frame_index;
        cache_insert(s, frame_index, rgba.clone());
        eprintln!("[video-decode] frame={frame_index} INDEXED decode={decode_ms:.1}ms");
        return Ok((rgba, false));
    }

    let t0 = std::time::Instant::now();
    // decode_at reads last_foreground_request to classify this request
    // (step-back vs fling), so it must still hold the PREVIOUS request
    // here — updated right after.
    let (rgba, seeked) = decode_at(s, frame_index, "foreground")?;
    s.last_foreground_request = frame_index;
    let decode_ms = t0.elapsed().as_secs_f64() * 1000.0;

    // Populate the cache — structure unchanged from v1, budget now sized
    // per-session from frame_bytes (see cache_budget_for / cache_insert).
    cache_insert(s, frame_index, rgba.clone());

    eprintln!("[video-decode] frame={frame_index} seeked={seeked} decode={decode_ms:.1}ms");
    Ok((rgba, seeked))
}

/// Per-session Mutex, not one global lock — unchanged from v1 (the fix
/// for "latence pour plusieurs vidéos"): outer Mutex guards only the
/// HashMap's shape, each session's actual VideoSession is behind its own
/// Mutex so session A decoding never blocks session B.
pub struct VideoSessions(pub Mutex<HashMap<u32, Arc<Mutex<VideoSession>>>>);

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

/// Parses `ffmpeg -i <path>` stderr (ffmpeg always prints full stream
/// analysis there before erroring "At least one output file must be
/// specified" — a well-established trick for probing without bundling
/// ffprobe as a second sidecar, which would reintroduce a dynamically-
/// linked-against-Homebrew binary and the exact dylib problem this
/// rewrite exists to remove: confirmed via `otool -L` that Homebrew's
/// ffprobe links ~78 /opt/homebrew paths, vs the bundled ffmpeg CLI's 23
/// pure-OS-framework links). Hand-parsed (no `regex` dependency added —
/// the format is small and stable enough, and every codec/container this
/// module's own test matrix exercises was captured and used to build
/// this parser against REAL output, not a guessed format) — see
/// `parse_probe_tests` for the captured samples this was built against.
fn parse_probe(stderr: &str) -> Result<(u32, u32, f64, f64, String), String> {
    // Duration: "  Duration: 00:00:02.00, start: 0.000000, bitrate: ..."
    let duration = stderr
        .lines()
        .find_map(|l| l.trim_start().strip_prefix("Duration: "))
        .and_then(|rest| rest.split(',').next())
        .and_then(parse_hms)
        .ok_or("could not find/parse a Duration: line in ffmpeg -i output")?;

    // Video stream: "    Stream #0:0[...](...): Video: h264 (High) (...), yuv420p(...), 320x240 [SAR ...], ..., 30 fps, ..."
    let video_line = stderr
        .lines()
        .find(|l| l.contains("Video:"))
        .ok_or("no video stream found (no 'Video:' line in ffmpeg -i output — is this a video file?)")?;

    let codec = video_line
        .split("Video: ")
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .map(|tok| tok.trim_end_matches(|c: char| c == ',' || c == '(').to_lowercase())
        .ok_or("could not parse codec name from Video: line")?;

    // Resolution: scan whitespace-delimited tokens for the first "NxN"
    // shape (SAR/DAR use "W:H" with a colon, never 'x', so this can't
    // collide with them).
    let (width, height) = video_line
        .split(|c: char| c.is_whitespace() || c == ',')
        .find_map(|tok| {
            let (w, h) = tok.split_once('x')?;
            Some((w.parse::<u32>().ok()?, h.parse::<u32>().ok()?))
        })
        .ok_or("could not find a WxH token on the Video: line")?;

    // FPS: comma-separated segment ending in " fps".
    let fps = video_line
        .split(',')
        .map(|seg| seg.trim())
        .find_map(|seg| seg.strip_suffix(" fps"))
        .and_then(|n| n.trim().parse::<f64>().ok())
        .ok_or("could not find a '<N> fps' segment on the Video: line")?;

    Ok((width, height, fps, duration, codec))
}

fn parse_hms(s: &str) -> Option<f64> {
    let s = s.trim();
    let mut it = s.split(':');
    let h: f64 = it.next()?.parse().ok()?;
    let m: f64 = it.next()?.parse().ok()?;
    let sec: f64 = it.next()?.parse().ok()?;
    Some(h * 3600.0 + m * 60.0 + sec)
}

// ---- indexed optimized media (random-access MJPEG stream + sidecar index) ----

/// Walks a raw MJPEG stream (concatenated baseline JPEGs, the exact output
/// of `ffmpeg -f mjpeg`) and returns the byte offset of every frame's SOI
/// marker, plus a final sentinel equal to the file length. This is a REAL
/// JPEG segment parser, not a naive FFD8 byte scan: marker-segment payloads
/// (DQT/DHT tables) may legally contain any byte pair, so boundaries are
/// only trusted after structurally walking each image — length-prefixed
/// segments hopped by their declared size, then the entropy-coded scan
/// after SOS, where 0xFF is always followed by 0x00 (stuffing) or
/// 0xD0-0xD7 (restart markers) until a real marker ends the scan.
fn index_mjpeg_stream(path: &str) -> Result<Vec<u64>, String> {
    use std::io::BufRead;
    let f = std::fs::File::open(path).map_err(|e| format!("open {path}: {e}"))?;
    let len = f.metadata().map_err(|e| e.to_string())?.len();
    let mut r = std::io::BufReader::with_capacity(1 << 20, f);
    let mut pos: u64 = 0;
    let mut offsets: Vec<u64> = Vec::new();

    // Byte-at-a-time reads through the 1MB BufReader: fill_buf/consume by
    // hand so `pos` tracking stays trivial and exact.
    fn next_byte(r: &mut std::io::BufReader<std::fs::File>, pos: &mut u64) -> Option<u8> {
        let buf = r.fill_buf().ok()?;
        if buf.is_empty() {
            return None;
        }
        let b = buf[0];
        r.consume(1);
        *pos += 1;
        Some(b)
    }
    fn skip(r: &mut std::io::BufReader<std::fs::File>, pos: &mut u64, mut n: u64) -> bool {
        while n > 0 {
            let buf = match r.fill_buf() {
                Ok(b) if !b.is_empty() => b,
                _ => return false,
            };
            let take = (buf.len() as u64).min(n) as usize;
            r.consume(take);
            *pos += take as u64;
            n -= take as u64;
        }
        true
    }

    'images: loop {
        let start = pos;
        // Expect SOI (or clean EOF between images).
        let Some(b0) = next_byte(&mut r, &mut pos) else { break };
        let Some(b1) = next_byte(&mut r, &mut pos) else { break };
        if b0 != 0xFF || b1 != 0xD8 {
            return Err(format!("not an MJPEG stream: expected SOI at byte {start}, found {b0:02x}{b1:02x}"));
        }
        offsets.push(start);
        // Walk marker segments.
        loop {
            // Marker prefix: one or more 0xFF fill bytes then the marker id.
            let m;
            loop {
                let Some(b) = next_byte(&mut r, &mut pos) else { return Err("truncated JPEG (EOF in marker)".into()) };
                if b == 0xFF {
                    loop {
                        let Some(b2) = next_byte(&mut r, &mut pos) else { return Err("truncated JPEG (EOF after 0xFF)".into()) };
                        if b2 != 0xFF {
                            m = b2;
                            break;
                        }
                    }
                    break;
                }
                // Non-FF between segments would be corruption; be strict.
                return Err(format!("corrupt JPEG segment structure at byte {}", pos - 1));
            }
            match m {
                0xD9 => continue 'images,          // EOI with no scan (degenerate but legal)
                0x01 | 0xD0..=0xD7 => continue,    // standalone markers, no payload
                0xDA => {
                    // SOS: skip its header, then scan entropy-coded data.
                    let Some(l0) = next_byte(&mut r, &mut pos) else { return Err("truncated SOS".into()) };
                    let Some(l1) = next_byte(&mut r, &mut pos) else { return Err("truncated SOS".into()) };
                    let seg_len = u16::from_be_bytes([l0, l1]) as u64;
                    if seg_len < 2 || !skip(&mut r, &mut pos, seg_len - 2) {
                        return Err("truncated SOS header".into());
                    }
                    loop {
                        let Some(b) = next_byte(&mut r, &mut pos) else { return Err("truncated JPEG (EOF in scan)".into()) };
                        if b != 0xFF {
                            continue;
                        }
                        let Some(b2) = next_byte(&mut r, &mut pos) else { return Err("truncated JPEG (EOF in scan marker)".into()) };
                        match b2 {
                            0x00 | 0xD0..=0xD7 => continue, // stuffing / restart
                            0xD9 => continue 'images,       // EOI — frame complete
                            _ => {
                                // Another marker inside the scan area means a
                                // multi-scan (progressive) image; ffmpeg's
                                // mjpeg encoder emits baseline only, but
                                // handle it by returning to segment walking.
                                // Push back is impossible with BufReader, so
                                // handle the marker inline: length-prefixed
                                // unless standalone.
                                if b2 == 0x01 || (0xD0..=0xD7).contains(&b2) {
                                    continue;
                                }
                                let Some(l0) = next_byte(&mut r, &mut pos) else { return Err("truncated segment".into()) };
                                let Some(l1) = next_byte(&mut r, &mut pos) else { return Err("truncated segment".into()) };
                                let seg_len = u16::from_be_bytes([l0, l1]) as u64;
                                if seg_len < 2 || !skip(&mut r, &mut pos, seg_len - 2) {
                                    return Err("truncated segment payload".into());
                                }
                            }
                        }
                    }
                }
                _ => {
                    // Length-prefixed segment: APPn, DQT, DHT, SOF0, COM...
                    let Some(l0) = next_byte(&mut r, &mut pos) else { return Err("truncated segment".into()) };
                    let Some(l1) = next_byte(&mut r, &mut pos) else { return Err("truncated segment".into()) };
                    let seg_len = u16::from_be_bytes([l0, l1]) as u64;
                    if seg_len < 2 || !skip(&mut r, &mut pos, seg_len - 2) {
                        return Err("truncated segment payload".into());
                    }
                }
            }
        }
    }
    if offsets.is_empty() {
        return Err("no JPEG frames found in stream".into());
    }
    offsets.push(len); // sentinel: frame i = offsets[i]..offsets[i+1]
    Ok(offsets)
}

/// Transcode `src` to an indexed optimized-media pair: `target` (raw MJPEG
/// stream, same `-q:v 3` near-visually-lossless setting the previous .mov
/// flavor used) and `target + ".idx"` (JSON: fps/width/height/count +
/// per-frame byte offsets). Tauri-free core, wrapped by the command below.
fn create_optimized_media_core(src: &str, target: &str) -> Result<(), String> {
    // Probe the SOURCE for fps (a raw MJPEG stream carries no timing
    // metadata of its own — the sidecar is the only place it survives).
    let out = Command::new(ffmpeg_path())
        .args(["-hide_banner", "-i", src])
        .output()
        .map_err(|e| format!("probe spawn: {e}"))?;
    let stderr = String::from_utf8_lossy(&out.stderr);
    // Width/height come from the source too: the transcode never rescales,
    // and the raw MJPEG result can't be re-probed anyway (no Duration line
    // without a container — the whole reason the sidecar exists).
    let (width, height, fps, _, _) = parse_probe(&stderr).map_err(|e| format!("probe {src}: {e}"))?;

    let status = Command::new(ffmpeg_path())
        .args(["-y", "-v", "error", "-nostdin", "-i", src, "-c:v", "mjpeg", "-q:v", "3", "-an", "-f", "mjpeg", target])
        .status()
        .map_err(|e| format!("transcode spawn: {e}"))?;
    if !status.success() {
        let _ = std::fs::remove_file(target);
        return Err(format!("transcode failed (exit {status:?})"));
    }

    let offsets = index_mjpeg_stream(target).inspect_err(|_| {
        let _ = std::fs::remove_file(target);
    })?;
    let count = offsets.len() - 1;

    let idx = serde_json::json!({
        "fps": fps,
        "width": width,
        "height": height,
        "count": count,
        "offsets": offsets,
    });
    std::fs::write(format!("{target}.idx"), serde_json::to_vec(&idx).map_err(|e| e.to_string())?)
        .map_err(|e| format!("write idx: {e}"))?;
    Ok(())
}

/// If `path` has a sidecar `.idx` (i.e. it's one of OUR optimized-media
/// streams), open it as an indexed random-access session. Returns None for
/// ordinary video files so open_session_core falls through to the normal
/// pipe-decoder path.
fn try_open_indexed(path: &str) -> Option<Result<(VideoSession, u64, f64), String>> {
    let idx_path = format!("{path}.idx");
    if !std::path::Path::new(&idx_path).exists() {
        return None;
    }
    Some((|| {
        let text = std::fs::read_to_string(&idx_path).map_err(|e| format!("read idx: {e}"))?;
        let v: serde_json::Value = serde_json::from_str(&text).map_err(|e| format!("parse idx: {e}"))?;
        let fps = v["fps"].as_f64().ok_or("idx: missing fps")?;
        let width = v["width"].as_u64().ok_or("idx: missing width")? as u32;
        let height = v["height"].as_u64().ok_or("idx: missing height")? as u32;
        let offsets: Vec<u64> = v["offsets"]
            .as_array()
            .ok_or("idx: missing offsets")?
            .iter()
            .map(|o| o.as_u64().ok_or("idx: bad offset"))
            .collect::<Result<_, _>>()?;
        if offsets.len() < 2 || fps <= 0.0 || width == 0 || height == 0 {
            return Err("idx: invalid contents".into());
        }
        // The sentinel-pad protocol needs 32 pixels of row 0 for its
        // signature (see is_sentinel_frame) — sub-32px-wide video is
        // absurd for real media, but refuse indexed mode honestly rather
        // than misdetect pads; the JS caller falls back to the original
        // file on open failure.
        if width < 32 {
            return Err(format!("indexed mode unsupported below 32px width (got {width})"));
        }
        let count = (offsets.len() - 1) as u64;
        let file = std::fs::File::open(path).map_err(|e| format!("open {path}: {e}"))?;
        // Stale-index guard: the sentinel must equal the current file size.
        let len = file.metadata().map_err(|e| e.to_string())?.len();
        if *offsets.last().unwrap() != len {
            return Err(format!("idx is stale for {path} (sentinel {} != file len {len})", offsets.last().unwrap()));
        }
        let duration = count as f64 / fps;
        // Sentinel pipeline padding — see IndexedSeq::pad. Generated once
        // per open (~20-40ms one-shot encode, amortized over the session).
        let pad = sentinel_pad(width, height)?;
        Ok((
            VideoSession {
                path: path.to_string(),
                width,
                height,
                fps,
                frame_bytes: (width as usize) * (height as usize) * 4,
                frame_count: count as i64,
                proc: None,
                cache: std::collections::VecDeque::new(),
                readahead_active: false,
                readahead_goal: 0,
                last_foreground_request: -1,
                indexed: Some(IndexedSeq { file, offsets, conv: None, pad }),
                codec: "mjpeg-indexed".into(),
            },
            count,
            duration,
        ))
    })())
}

/// Spawn the persistent JPEG→RGBA converter child for an indexed session.
/// stdin is fed by a dedicated writer thread, stdout drained by a
/// dedicated reader thread into a channel (see ConvProc for why). Flags
/// chosen empirically against the bundled ffmpeg 8.1 (2026-07 harness):
/// probesize must be tiny or find_stream_info blocks waiting for input it
/// will never get; -analyzeduration needs a NONZERO microsecond value (0
/// means "use the 5-second default", a trap); -fpsprobesize 0 stops it
/// demanding several frames to estimate a frame rate the raw stream
/// doesn't carry; -fflags nobuffer made output STOP entirely (3 in → 0
/// out) so it is deliberately absent.
fn conv_spawn(frame_bytes: usize) -> Result<ConvProc, String> {
    let mut child = Command::new(ffmpeg_path())
        .args([
            "-v", "error",
            "-probesize", "32", "-fpsprobesize", "0", "-analyzeduration", "1",
            "-f", "mjpeg", "-i", "-",
            "-f", "rawvideo", "-pix_fmt", "rgba", "-an", "-",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("converter spawn ({}): {e}", ffmpeg_path().display()))?;
    let mut stdin = child.stdin.take().ok_or("converter stdin unavailable")?;
    let stdout = child.stdout.take().ok_or("converter stdout unavailable")?;
    let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
    std::thread::spawn(move || {
        use std::io::Write;
        for buf in rx {
            if stdin.write_all(&buf).is_err() || stdin.flush().is_err() {
                break;
            }
        }
        // Sender dropped (session closed) or child died: closing stdin here
        // lets the child drain and exit on its own.
    });
    let (ftx, frx) = std::sync::mpsc::channel::<Vec<u8>>();
    std::thread::spawn(move || {
        use std::io::Read as _;
        let mut r = std::io::BufReader::new(stdout);
        loop {
            let mut frame = vec![0u8; frame_bytes];
            if r.read_exact(&mut frame).is_err() {
                break; // child exited/killed — receiver sees the channel close
            }
            if ftx.send(frame).is_err() {
                break; // session closed
            }
        }
    });
    Ok(ConvProc { child, jpeg_tx: tx, frame_rx: frx })
}

/// Decode one frame of an indexed session: read its JPEG bytes at the
/// indexed offset, round-trip through the persistent converter. Respawns
/// the converter once if it died (e.g. killed externally) before failing.
fn indexed_decode(s: &mut VideoSession, frame_index: i64) -> Result<Vec<u8>, String> {
    use std::io::{Read as _, Seek, SeekFrom};
    let frame_bytes = s.frame_bytes;
    let width = s.width as usize;
    let seq = s.indexed.as_mut().ok_or("not an indexed session")?;
    let i = frame_index as usize;
    if i + 1 >= seq.offsets.len() {
        return Err(format!("frame {frame_index} out of indexed range"));
    }
    let (a, b) = (seq.offsets[i], seq.offsets[i + 1]);
    let mut jpeg = vec![0u8; (b - a) as usize];
    seq.file.seek(SeekFrom::Start(a)).map_err(|e| e.to_string())?;
    seq.file.read_exact(&mut jpeg).map_err(|e| format!("read frame {frame_index}: {e}"))?;

    for attempt in 0..2 {
        if seq.conv.is_none() {
            seq.conv = Some(conv_spawn(frame_bytes)?);
        }
        let conv = seq.conv.as_mut().unwrap();
        // [target][pads…] — the sentinel pads flush the target through the
        // pipeline's content-dependent frame lag; the read loop discards
        // every recognized sentinel and returns the first real frame.
        // recv_timeout (never a bare blocking read) is what guarantees
        // this can't freeze the session mutex (see ConvProc::frame_rx).
        let mut payload = Vec::with_capacity(jpeg.len() + seq.pad.len() * SENTINEL_PADS_PER_REQUEST);
        payload.extend_from_slice(&jpeg);
        for _ in 0..SENTINEL_PADS_PER_REQUEST {
            payload.extend_from_slice(&seq.pad);
        }
        if conv.jpeg_tx.send(payload).is_err() {
            seq.conv = None;
            continue;
        }
        let mut extra_volleys = 0u32;
        let failure = loop {
            match conv.frame_rx.recv_timeout(std::time::Duration::from_millis(700)) {
                Ok(frame) => {
                    if is_sentinel_frame(&frame, width) {
                        continue; // leftover/current pad — discard
                    }
                    return Ok(frame);
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    if extra_volleys >= 2 {
                        break "no real frame after 2 extra pad volleys";
                    }
                    // Self-heal: the lag exceeded the volley (unseen build/
                    // resolution combination) — push more pads through.
                    extra_volleys += 1;
                    let mut more = Vec::with_capacity(seq.pad.len() * SENTINEL_PADS_PER_REQUEST);
                    for _ in 0..SENTINEL_PADS_PER_REQUEST {
                        more.extend_from_slice(&seq.pad);
                    }
                    if conv.jpeg_tx.send(more).is_err() {
                        break "converter writer died";
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    break "converter exited";
                }
            }
        };
        eprintln!("[video-decode] indexed converter reset (frame {frame_index}, attempt {attempt}): {failure}");
        let _ = conv.child.kill();
        let _ = conv.child.wait();
        seq.conv = None;
        if attempt == 1 {
            return Err(format!("indexed decode failed twice for frame {frame_index}: {failure}"));
        }
    }
    unreachable!()
}

// Core open logic, tauri-free so `cargo test` can exercise it directly
// (the #[tauri::command] wrappers below only add State registry plumbing).
fn open_session_core(path: &str) -> Result<(VideoSession, u64, f64), String> {
    // Our own indexed optimized-media streams short-circuit the probe
    // entirely (a raw MJPEG stream has no duration/fps metadata anyway —
    // the sidecar .idx is the source of truth).
    if let Some(r) = try_open_indexed(path) {
        return r;
    }
    let out = Command::new(ffmpeg_path())
        .args(["-hide_banner", "-i", path])
        .output()
        .map_err(|e| format!("probe spawn: {e}"))?;
    let stderr = String::from_utf8_lossy(&out.stderr);
    let (width, height, fps, duration_seconds, codec) = parse_probe(&stderr)
        .map_err(|e| format!("probe parse failed for {path}: {e}\n--- ffmpeg -i stderr ---\n{stderr}"))?;
    if fps <= 0.0 || width == 0 || height == 0 {
        return Err(format!("invalid stream parameters (w={width} h={height} fps={fps})"));
    }
    // frame_count is ALWAYS derived (duration×fps) — v1 only did this as a
    // WebM-specific fallback because video-rs otherwise exposed a real
    // per-stream frame count; ffmpeg's own `-i` info dump never reliably
    // prints one for any container, so v2 uses this math universally.
    let frame_count = (duration_seconds * fps).round().max(0.0) as u64;

    Ok((
        VideoSession {
            path: path.to_string(),
            width,
            height,
            fps,
            frame_bytes: (width as usize) * (height as usize) * 4,
            frame_count: frame_count as i64,
            proc: None,
            cache: std::collections::VecDeque::new(),
            readahead_active: false,
            readahead_goal: 0,
            last_foreground_request: -1,
            indexed: None,
            codec,
        },
        frame_count,
        duration_seconds,
    ))
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
    state.0.lock().unwrap().insert(id, Arc::new(Mutex::new(session)));

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
        let sessions = state.0.lock().unwrap();
        sessions
            .get(&session_id)
            .cloned()
            .ok_or_else(|| format!("no session {session_id}"))?
    };
    let (result, indexed) = {
        let mut s = session_arc.lock().unwrap();
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
    let removed = state.0.lock().unwrap().remove(&session_id);
    if let Some(arc) = removed {
        let mut s = arc.lock().unwrap();
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
    Ok(())
}

/// Warm the session cache around `center` on a dedicated thread — the mpv
/// pattern. Backward warming used to be gated to all-intra codecs (v1: a
/// backward frame on long-GOP content meant a full in-process GOP walk,
/// real CPU work competing with the foreground request for the session
/// lock). That gate no longer matches v2's cost model: ffmpeg's own
/// internal `-ss`/accurate_seek walk happens INSIDE the child process with
/// no per-intermediate-frame IPC/RGBA-conversion cost on our side, and is
/// fast regardless of GOP length — perf_seek_cost_measured shows ~16.7ms
/// average even on a GOP-15 file. Bidirectional warming now applies to
/// every codec.
fn spawn_readahead(arc: Arc<Mutex<VideoSession>>, center: i64) {
    {
        let mut s = arc.lock().unwrap();
        s.readahead_goal = center.min(s.frame_count - 1).max(0);
        if s.readahead_active {
            return;
        }
        s.readahead_active = true;
    }
    std::thread::spawn(move || {
        let mut last_goal: Option<i64> = None;
        let mut last_goal_change = std::time::Instant::now();
        // Tracks frames this thread has already spent a respawn decoding
        // for THIS goal window, even if the cache later evicted them.
        // Without this, a goal that sits still while the session's cache
        // is too small to hold the whole forward+backward window (small
        // the cache budget relative to per-frame size (fixed since — see cache_budget_for), e.g. 4K)
        // thrashes forever: decode backward target A, it evicts B, next
        // pass finds B "missing" and redecodes it, which evicts A, forever
        // — live-caught 2026-07, cycling on 4 targets at ~20-25ms/respawn
        // with the CPU cost never actually going away once playback
        // stopped. Cleared whenever the goal actually moves, since a new
        // center means a genuinely new window worth attempting.
        let mut attempted: std::collections::HashSet<i64> = std::collections::HashSet::new();
        loop {
        let mut s = arc.lock().unwrap();
        if !s.readahead_active {
            break;
        }
        let center = s.readahead_goal;
        if last_goal != Some(center) {
            last_goal = Some(center);
            last_goal_change = std::time::Instant::now();
            attempted.clear();
        }
        let back = READAHEAD_BACK;
        // Live-caught 2026-07 ("j'ai mis 3 vidéos ça a du mal à lire les 3
        // en temps réel"): readahead was fighting foreground playback for
        // the same session's ffmpeg process. First fix gated only the
        // backward-fill branch, but a second live capture showed the SAME
        // fight happening on the FORWARD side too — whenever the process
        // had drifted (e.g. from an earlier respawn) such that a forward
        // target fell outside decode_at's small-forward-reuse window, that
        // forward fill *also* forced a respawn that collided with
        // foreground's own repeated requests for a stalled frame (one
        // capture: target=192 requested 3x by foreground, each time with a
        // fresh negative gap, because readahead kept repositioning the
        // process for its own forward targets in between). The real rule
        // is simpler than "forward is fine, backward is dangerous": ANY
        // candidate that would cost a respawn is dangerous while foreground
        // is actively live (goal fresh within the last 150ms — one
        // playback tick is ~33ms, a human pause is much longer). Only a
        // target reachable without a respawn (small forward gap on the
        // process as it currently sits, or backward once things go quiet)
        // is fair game. `readahead_warms_backward_on_all_intra` still
        // passes: its forward-fill targets are all reachable without a
        // respawn (fresh session, sequential +1 gaps), so nothing here
        // defers them.
        let goal_recently_moving = last_goal_change.elapsed() < std::time::Duration::from_millis(150);
        // When the clamped budget can't hold the nominal window (4K hits
        // the 320MB ceiling), cap forward depth so warming never evicts
        // the frames the playhead is about to ask for — otherwise the
        // 1080p eviction bug (see cache_budget_for) just reappears at 4K.
        let frames_that_fit = (cache_budget_for(s.frame_bytes) / s.frame_bytes.max(1)) as i64;
        let depth = READAHEAD_DEPTH.min((frames_that_fit - 1 - READAHEAD_BACK - 1).max(1));
        let mut target: Option<i64> = None;
        for d in 1..=depth {
            let f = center + d;
            if f < s.frame_count
                && !attempted.contains(&f)
                && !s.cache.iter().any(|(i, _)| *i == f)
                && !(goal_recently_moving && would_need_respawn(&s, f))
            {
                target = Some(f);
                break;
            }
        }
        if target.is_none() && !goal_recently_moving {
            for d in 1..=back {
                let f = center - d;
                if f >= 0 && !attempted.contains(&f) && !s.cache.iter().any(|(i, _)| *i == f) {
                    target = Some(f);
                    break;
                }
            }
        }
        let Some(f) = target else {
            if goal_recently_moving {
                // Either nothing to do right now without paying a respawn,
                // or forward is fully warmed and backward is deferred — in
                // both cases don't declare the thread done (that would
                // wrongly end it before a one-shot goal crosses the quiet
                // threshold); idle briefly and recheck.
                drop(s);
                std::thread::sleep(std::time::Duration::from_millis(20));
                continue;
            }
            s.readahead_active = false;
            break;
        };
        attempted.insert(f);
        let decoded = decode_at(&mut s, f, "readahead");
        match decoded {
            Ok((bytes, _seeked)) => {
                cache_insert(&mut s, f, bytes);
            }
            Err(_) => {
                s.readahead_active = false;
                break;
            }
        }
        if s.cache.iter().map(|(_, b)| b.len()).sum::<usize>() >= cache_budget_for(s.frame_bytes) {
            s.readahead_active = false;
            break;
        }
        drop(s);
        std::thread::yield_now();
        }
    });
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
    let exists = target.exists() && std::path::Path::new(&format!("{}.idx", target.to_string_lossy())).exists();
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
mod tests {
    use super::*;

    // ---- probe parser, tested against REAL captured ffmpeg -i output
    // (2026-07, run against the bundled binary against every file this
    // module's own codec matrix generates) — not a guessed format. ----
    #[test]
    fn parse_probe_h264() {
        let stderr = "Input #0, mov,mp4,m4a,3gp,3g2,mj2, from 'x':\n  Duration: 00:00:02.00, start: 0.000000, bitrate: 338 kb/s\n  Stream #0:0[0x1](und): Video: h264 (High) (avc1 / 0x31637661), yuv420p(progressive), 320x240 [SAR 1:1 DAR 4:3], 332 kb/s, 30 fps, 30 tbr, 15360 tbn (default)\n";
        let (w, h, fps, dur, codec) = parse_probe(stderr).unwrap();
        assert_eq!((w, h), (320, 240));
        assert!((fps - 30.0).abs() < 0.001);
        assert!((dur - 2.0).abs() < 0.001);
        assert_eq!(codec, "h264");
    }
    #[test]
    fn parse_probe_hevc() {
        let stderr = "  Duration: 00:00:02.00, start: 0.000000, bitrate: 231 kb/s\n  Stream #0:0[0x1](und): Video: hevc (Main) (hvc1 / 0x31637668), yuv420p(tv, progressive), 320x240 [SAR 1:1 DAR 4:3], 214 kb/s, 30 fps, 30 tbr, 15360 tbn (default)\n";
        let (w, h, fps, _dur, codec) = parse_probe(stderr).unwrap();
        assert_eq!((w, h), (320, 240));
        assert!((fps - 30.0).abs() < 0.001);
        assert_eq!(codec, "hevc");
    }
    #[test]
    fn parse_probe_vp9_webm_no_sar_paren() {
        // WebM's Stream line has no [0x..] tag and no trailing (default) —
        // a structurally different line shape from the mp4 samples.
        let stderr = "  Duration: 00:00:02.00, start: 0.000000, bitrate: 397 kb/s\n  Stream #0:0: Video: vp9 (Profile 0), yuv420p(tv, progressive), 320x240, SAR 1:1 DAR 4:3, 30 fps, 30 tbr, 1k tbn\n";
        let (w, h, fps, _dur, codec) = parse_probe(stderr).unwrap();
        assert_eq!((w, h), (320, 240), "SAR/DAR '1:1'/'4:3' tokens must not be mistaken for the WxH token");
        assert!((fps - 30.0).abs() < 0.001);
        assert_eq!(codec, "vp9");
    }
    #[test]
    fn parse_probe_prores_hq() {
        let stderr = "  Duration: 00:00:02.00, start: 0.000000, bitrate: 8836 kb/s\n  Stream #0:0[0x1]: Video: prores (HQ) (apch / 0x68637061), yuv422p10le(tv, progressive), 320x240, 8832 kb/s, SAR 1:1 DAR 4:3, 30 fps, 30 tbr, 15360 tbn (default)\n";
        let (w, h, fps, _dur, codec) = parse_probe(stderr).unwrap();
        assert_eq!((w, h), (320, 240));
        assert!((fps - 30.0).abs() < 0.001);
        assert_eq!(codec, "prores");
    }
    #[test]
    fn parse_probe_odd_dims_and_fractional_duration() {
        let stderr = "  Duration: 00:00:01.00, start: 0.000000, bitrate: 1728 kb/s\n  Stream #0:0[0x1](und): Video: h264 (High) (avc1 / 0x31637661), yuv420p(progressive), 954x542 [SAR 1:1 DAR 477:271], 1719 kb/s, 30 fps, 30 tbr, 15360 tbn (default)\n";
        let (w, h, _fps, dur, _codec) = parse_probe(stderr).unwrap();
        assert_eq!((w, h), (954, 542), "DAR 477:271 must not be mistaken for the WxH token");
        assert!((dur - 1.0).abs() < 0.001);
    }
    #[test]
    fn parse_probe_25fps() {
        let stderr = "  Duration: 00:00:02.00, start: 0.000000, bitrate: 342 kb/s\n  Stream #0:0[0x1](und): Video: h264 (High) (avc1 / 0x31637661), yuv420p(progressive), 320x240 [SAR 1:1 DAR 4:3], 336 kb/s, 25 fps, 25 tbr, 12800 tbn (default)\n";
        let (_w, _h, fps, _dur, _codec) = parse_probe(stderr).unwrap();
        assert!((fps - 25.0).abs() < 0.001, "fps was {fps}");
    }
    #[test]
    fn parse_probe_rejects_audio_only_stderr() {
        let stderr = "  Duration: 00:00:02.00, start: 0.000000, bitrate: 128 kb/s\n  Stream #0:0: Audio: mp3, 44100 Hz, stereo, fltp, 128 kb/s\n";
        assert!(parse_probe(stderr).is_err(), "an audio-only file must not parse as having a video stream");
    }

    use std::process::Command as StdCommand;

    /// Generates a test video with the BUNDLED ffmpeg CLI binary (the
    /// exact same one this module pipes for decode AND probing — no
    /// separate "test-only" ffmpeg invocation path), cached by filename.
    fn gen_video(filename: &str, lavfi: &str, codec_args: &[&str]) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join("nemo-video-decode-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(filename);
        if !path.exists() {
            let ffmpeg = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("binaries/ffmpeg-aarch64-apple-darwin");
            let mut args: Vec<&str> = vec!["-y", "-f", "lavfi", "-i", lavfi];
            args.extend_from_slice(codec_args);
            let out = path.to_str().unwrap().to_string();
            args.push(&out);
            let status = StdCommand::new(&ffmpeg)
                .args(&args)
                .status()
                .expect("bundled ffmpeg binary must exist and run");
            assert!(status.success(), "test video generation failed: {filename}");
        }
        path
    }

    /// The production code resolves ffmpeg via current_exe() (correct at
    /// app runtime — confirmed empirically against the real dev/release
    /// build layout). `cargo test`'s current_exe() is a deps/ test binary
    /// with no sidecar next to it, so tests point decode/probe at the
    /// bundled binary directly via an env var override, read once here.
    fn test_ffmpeg_override() {
        let ffmpeg = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("binaries/ffmpeg-aarch64-apple-darwin");
        std::env::set_var("NEMO_TEST_FFMPEG_PATH", ffmpeg);
    }

    fn make_test_video() -> std::path::PathBuf {
        gen_video(
            "testsrc2_60f_30fps.mp4",
            "testsrc2=size=320x240:rate=30:duration=2",
            &["-c:v", "libx264", "-pix_fmt", "yuv420p", "-g", "15"],
        )
    }

    // ---- ffmpeg-path override plumbing for tests only, wired through a
    // OnceLock so open_session_core/spawn_at pick it up transparently ----
    fn open_session_core_t(path: &str) -> Result<(VideoSession, u64, f64), String> {
        test_ffmpeg_override();
        open_session_core(path)
    }

    #[test]
    fn open_reports_correct_stream_parameters() {
        let path = make_test_video();
        let (s, frame_count, duration) = open_session_core_t(path.to_str().unwrap()).unwrap();
        assert_eq!((s.width, s.height), (320, 240));
        assert!((s.fps - 30.0).abs() < 0.01, "fps was {}", s.fps);
        assert_eq!(frame_count, 60);
        assert!((duration - 2.0).abs() < 0.1, "duration was {duration}");
    }

    #[test]
    fn sequential_decode_yields_correctly_sized_distinct_frames() {
        let path = make_test_video();
        let (mut s, _, _) = open_session_core_t(path.to_str().unwrap()).unwrap();
        let expected_len = (s.width * s.height * 4) as usize;
        let f0 = decode_frame_core(&mut s, 0).unwrap().0;
        let f1 = decode_frame_core(&mut s, 1).unwrap().0;
        assert_eq!(f0.len(), expected_len);
        assert_eq!(f1.len(), expected_len);
        assert!(f0.iter().skip(3).step_by(4).all(|&a| a == 255));
        assert_ne!(f0, f1, "consecutive frames were byte-identical");
    }

    #[test]
    fn random_seek_is_frame_accurate() {
        let path = make_test_video();
        let (mut s, _, _) = open_session_core_t(path.to_str().unwrap()).unwrap();
        let mut sequential_f40 = Vec::new();
        for i in 0..=40 {
            sequential_f40 = decode_frame_core(&mut s, i).unwrap().0;
        }
        let seeked_f40 = decode_frame_core(&mut s, 40).unwrap().0;
        assert_eq!(
            seeked_f40, sequential_f40,
            "seeked frame 40 != sequentially-decoded frame 40 (ffmpeg's own accurate_seek is not frame-accurate?!)"
        );
    }

    #[test]
    fn decode_past_end_clamps_to_last_frame() {
        let path = make_test_video();
        let (mut s, frame_count, _) = open_session_core_t(path.to_str().unwrap()).unwrap();
        let expected_len = (s.width * s.height * 4) as usize;
        let past = decode_frame_core(&mut s, frame_count as i64 + 100).unwrap().0;
        assert_eq!(past.len(), expected_len);
        let last_a = decode_frame_core(&mut s, frame_count as i64 - 1).unwrap().0;
        let last_b = decode_frame_core(&mut s, frame_count as i64 - 1).unwrap().0;
        assert_eq!(last_a, last_b, "last-frame seek not reproducible");
        assert_eq!(past, last_a, "past-end result should BE the last frame");
    }

    #[test]
    fn webm_tail_seek_does_not_exhaust() {
        let p = gen_video(
            "vp9_tail_320.webm",
            "testsrc2=size=320x240:rate=30:duration=2",
            &["-c:v", "libvpx-vp9", "-pix_fmt", "yuv420p", "-g", "15"],
        );
        let (mut s, frame_count, _) = open_session_core_t(p.to_str().unwrap()).unwrap();
        assert!(frame_count > 0);
        let expected_len = (s.width * s.height * 4) as usize;
        for probe in [frame_count as i64 - 1, frame_count as i64, frame_count as i64 + 10] {
            let (f, _) = decode_frame_core(&mut s, probe)
                .unwrap_or_else(|e| panic!("tail probe {probe} failed: {e}"));
            assert_eq!(f.len(), expected_len, "tail probe {probe} wrong size");
        }
    }

    fn assert_decodes_and_seeks(path: &std::path::Path, expect_w: u32, expect_h: u32) {
        let (mut s, frame_count, _) = open_session_core_t(path.to_str().unwrap()).unwrap();
        assert_eq!((s.width, s.height), (expect_w, expect_h), "{path:?}");
        assert!(frame_count > 10, "{path:?} frame_count={frame_count}");
        let expected_len = (s.width * s.height * 4) as usize;
        let f0 = decode_frame_core(&mut s, 0).unwrap().0;
        assert_eq!(f0.len(), expected_len, "{path:?} frame 0 size");
        let mid = (frame_count as i64) / 2;
        let fm = decode_frame_core(&mut s, mid).unwrap().0;
        assert_eq!(fm.len(), expected_len, "{path:?} mid frame size");
        let f0_again = decode_frame_core(&mut s, 0).unwrap().0;
        assert_eq!(f0, f0_again, "{path:?} re-seek to 0 not reproducible");
    }

    #[test]
    fn codec_hevc_mp4() {
        let p = gen_video(
            "hevc_320.mp4",
            "testsrc2=size=320x240:rate=30:duration=2",
            &["-c:v", "libx265", "-pix_fmt", "yuv420p", "-g", "15", "-tag:v", "hvc1"],
        );
        assert_decodes_and_seeks(&p, 320, 240);
    }

    #[test]
    fn codec_vp9_webm() {
        let p = gen_video(
            "vp9_320.webm",
            "testsrc2=size=320x240:rate=30:duration=2",
            &["-c:v", "libvpx-vp9", "-pix_fmt", "yuv420p", "-g", "15"],
        );
        assert_decodes_and_seeks(&p, 320, 240);
    }

    #[test]
    fn codec_prores_mov() {
        let p = gen_video(
            "prores_320.mov",
            "testsrc2=size=320x240:rate=30:duration=2",
            &["-c:v", "prores_ks", "-profile:v", "3", "-pix_fmt", "yuv422p10le"],
        );
        assert_decodes_and_seeks(&p, 320, 240);
    }

    // ProRes 4444 with real alpha. v2's CAPABILITY GAIN over v1 (not just
    // a licensing workaround): piping `-pix_fmt rgba` lets ffmpeg's own
    // swscale carry real alpha straight through — v1's video-rs scaler
    // was HARDCODED to RGB24 and flattened it (documented by a test
    // asserting the flattening). This test asserts the OPPOSITE now:
    // alpha bytes must vary (not a constant 255 fill).
    #[test]
    fn codec_prores4444_alpha_survives_the_pipe() {
        let p = gen_video(
            "prores4444_alpha_320.mov",
            "testsrc2=size=320x240:rate=30:duration=1,format=rgba,colorchannelmixer=aa=0.5",
            &["-c:v", "prores_ks", "-profile:v", "4", "-pix_fmt", "yuva444p10le"],
        );
        let (mut s, _, _) = open_session_core_t(p.to_str().unwrap()).unwrap();
        let f0 = decode_frame_core(&mut s, 0).unwrap().0;
        let alphas: Vec<u8> = f0.iter().skip(3).step_by(4).copied().collect();
        assert!(
            alphas.iter().any(|&a| a != 255),
            "real alpha (colorchannelmixer aa=0.5) came through as a constant 255 fill — the rawvideo/rgba pipe should preserve it"
        );
    }

    #[test]
    fn odd_width_stride_padding() {
        // "stride padding" no longer applies to v2 at all — `-f rawvideo
        // -pix_fmt rgba` writes tightly-packed bytes with zero row
        // padding, unlike v1's AVFrame planes. Kept as a regression test
        // under its original name/intent: odd (non-16-aligned) dimensions
        // must still decode to the EXACT expected byte length and be
        // seek-reproducible — a coded-size-vs-display-size confusion
        // would still show up as a wrong buffer size here.
        let p = gen_video(
            "odd_954x542.mp4",
            "testsrc2=size=954x542:rate=30:duration=1",
            &["-c:v", "libx264", "-pix_fmt", "yuv420p", "-g", "15"],
        );
        assert_decodes_and_seeks(&p, 954, 542);
    }

    #[test]
    fn seek_accuracy_at_25fps() {
        let p = gen_video(
            "fps25_320.mp4",
            "testsrc2=size=320x240:rate=25:duration=2",
            &["-c:v", "libx264", "-pix_fmt", "yuv420p", "-g", "12"],
        );
        let (mut s, _, _) = open_session_core_t(p.to_str().unwrap()).unwrap();
        assert!((s.fps - 25.0).abs() < 0.01, "fps was {}", s.fps);
        let mut sequential_f30 = Vec::new();
        for i in 0..=30 {
            sequential_f30 = decode_frame_core(&mut s, i).unwrap().0;
        }
        let seeked_f30 = decode_frame_core(&mut s, 30).unwrap().0;
        assert_eq!(seeked_f30, sequential_f30, "25fps seek not frame-accurate");
    }

    #[test]
    fn interleaved_sessions_stay_isolated() {
        let pa = make_test_video();
        let pb = gen_video(
            "iso_b_160.mp4",
            "testsrc2=size=160x120:rate=30:duration=2",
            &["-c:v", "libx264", "-pix_fmt", "yuv420p", "-g", "15"],
        );
        let (mut a, _, _) = open_session_core_t(pa.to_str().unwrap()).unwrap();
        let (mut b, _, _) = open_session_core_t(pb.to_str().unwrap()).unwrap();
        let a0 = decode_frame_core(&mut a, 0).unwrap().0;
        let _b0 = decode_frame_core(&mut b, 0).unwrap().0;
        let a1 = decode_frame_core(&mut a, 1).unwrap().0;
        let _b1 = decode_frame_core(&mut b, 1).unwrap().0;
        let a2 = decode_frame_core(&mut a, 2).unwrap().0;

        let (mut a_ref, _, _) = open_session_core_t(pa.to_str().unwrap()).unwrap();
        assert_eq!(a0, decode_frame_core(&mut a_ref, 0).unwrap().0, "a0 diverged");
        assert_eq!(a1, decode_frame_core(&mut a_ref, 1).unwrap().0, "a1 diverged");
        assert_eq!(a2, decode_frame_core(&mut a_ref, 2).unwrap().0, "a2 diverged");
    }

    /// Steady-state per-frame decode cost — deliberately EXCLUDES frame 0
    /// (open_session_core_t only probes metadata; the actual ffmpeg decode
    /// process is spawned lazily on the FIRST decode_frame_core call).
    /// Live-measured (2026-07, this rewrite): that first spawn costs
    /// ~80-90ms at 4K — a real, one-time architectural tax the piped
    /// design pays that v1's direct-linked decoder never had (no process
    /// to start) — vs a v1-BEATING ~5ms/frame steady state once running
    /// (no more per-row stride-unpad conversion loop; that was v1's
    /// hottest path and is simply gone with `-f rawvideo -pix_fmt rgba`'s
    /// tightly-packed output). Bundling that one-time cost into a 15-frame
    /// p95 measurement (the original version of this test) was measuring
    /// the wrong thing for what the budget assertion actually protects —
    /// playback SMOOTHNESS, i.e. steady-state per-frame cost, not how long
    /// opening a fresh session takes. See perf_session_open_cost for that
    /// number, measured on its own.
    fn measure_sequential_steady_state(path: &std::path::Path, frames: i64) -> (f64, f64) {
        let (mut s, _, _) = open_session_core_t(path.to_str().unwrap()).unwrap();
        decode_frame_core(&mut s, 0).unwrap(); // warm-up: absorb the spawn cost here, not in the measured loop
        let mut times = Vec::new();
        for i in 1..=frames {
            let t = std::time::Instant::now();
            decode_frame_core(&mut s, i).unwrap();
            times.push(t.elapsed().as_secs_f64() * 1000.0);
        }
        times.sort_by(|x, y| x.partial_cmp(y).unwrap());
        let avg = times.iter().sum::<f64>() / times.len() as f64;
        let p95 = times[(times.len() as f64 * 0.95) as usize];
        (avg, p95)
    }

    #[test]
    fn perf_1080p_sequential_holds_30fps_budget() {
        let p = gen_video(
            "perf_1080p.mp4",
            "testsrc2=size=1920x1080:rate=30:duration=2",
            &["-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", "-g", "15"],
        );
        let (avg, p95) = measure_sequential_steady_state(&p, 30);
        eprintln!("[perf] 1080p sequential (steady-state): avg={avg:.1}ms p95={p95:.1}ms (budget 33.3ms)");
        assert!(p95 < 33.3, "1080p p95 {p95:.1}ms blows the 30fps budget");
    }

    #[test]
    fn perf_4k_sequential_measured() {
        let p = gen_video(
            "perf_4k.mp4",
            "testsrc2=size=3840x2160:rate=30:duration=1",
            &["-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", "-g", "15"],
        );
        let (avg, p95) = measure_sequential_steady_state(&p, 15);
        eprintln!("[perf] 4K sequential (steady-state): avg={avg:.1}ms p95={p95:.1}ms (budget 33.3ms)");
        assert!(p95 < 41.6, "4K p95 {p95:.1}ms exceeds even the relaxed bar");
    }

    /// The one-time cost this architecture pays that v1 never did: spawning
    /// the ffmpeg process for the FIRST frame of a fresh session. Measured
    /// and asserted on its OWN (not folded into the steady-state budget
    /// tests above) — a real number worth knowing (it's the "time to first
    /// frame" a user feels on import/scrub-to-a-cold-position), but a
    /// different thing than per-frame playback smoothness.
    #[test]
    fn perf_session_open_cost() {
        let p = gen_video(
            "perf_4k.mp4",
            "testsrc2=size=3840x2160:rate=30:duration=1",
            &["-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", "-g", "15"],
        );
        let (mut s, _, _) = open_session_core_t(p.to_str().unwrap()).unwrap();
        let t0 = std::time::Instant::now();
        decode_frame_core(&mut s, 0).unwrap();
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        eprintln!("[perf] 4K first-frame (process spawn + decode): {ms:.1}ms");
        // Generous regression guard (process-start variance is real) — not
        // a tuned target, just a "did this suddenly get 5x worse" catch.
        assert!(ms < 500.0, "first-frame cost {ms:.1}ms far exceeds a sane bound");
    }

    /// v2's seek cost is structurally different from v1's (process spawn
    /// + ffmpeg's own internal walk, vs v1's direct av_seek_frame + our
    /// own bounded walk) — measured here as a real assertion instead of
    /// carrying over v1's now-meaningless "walked < GOP" check (see the
    /// module doc comment on why that count isn't observable anymore).
    #[test]
    fn perf_seek_cost_measured() {
        let path = make_test_video(); // gop 15, 60 frames total
        let (mut s, frame_count, _) = open_session_core_t(path.to_str().unwrap()).unwrap();
        let mut times = Vec::new();
        for target in [7i64, 22, 44, frame_count as i64 - 2] {
            let _ = decode_frame_core(&mut s, 0).unwrap(); // force away from the target first
            let t0 = std::time::Instant::now();
            decode_frame_core(&mut s, target).unwrap();
            times.push(t0.elapsed().as_secs_f64() * 1000.0);
        }
        let avg = times.iter().sum::<f64>() / times.len() as f64;
        eprintln!("[perf] seek (process spawn + ffmpeg internal walk): times={times:?} avg={avg:.1}ms");
        // Generous bar (a cold process spawn is inherently slower than
        // v1's in-process av_seek_frame call) — this is a REGRESSION
        // guard, not a tuned target: it should catch "seeking suddenly
        // takes 2 seconds", not nitpick normal process-start variance.
        assert!(avg < 500.0, "seek avg {avg:.1}ms far exceeds a sane bound for a tiny 320x240 test file");
    }

    #[test]
    fn revisited_frame_is_cache_accelerated_and_byte_identical() {
        let p = gen_video(
            "h264_320_biggop.mp4",
            "testsrc2=size=320x240:rate=30:duration=3",
            &["-c:v", "libx264", "-pix_fmt", "yuv420p", "-g", "60"],
        );
        let (mut s, _frame_count, _) = open_session_core_t(p.to_str().unwrap()).unwrap();

        let target = 45i64;
        let t0 = std::time::Instant::now();
        let (first, seeked) = decode_frame_core(&mut s, target).unwrap();
        let cold_ms = t0.elapsed().as_secs_f64() * 1000.0;
        assert!(seeked, "target should have required a real spawn+seek to set up this test");

        decode_frame_core(&mut s, 10).unwrap();
        let t1 = std::time::Instant::now();
        let (second, seeked2) = decode_frame_core(&mut s, target).unwrap();
        let warm_ms = t1.elapsed().as_secs_f64() * 1000.0;

        assert_eq!(first, second, "cached frame bytes diverged from the original decode");
        assert!(!seeked2, "cache hit should not report a fresh spawn");
        assert!(
            warm_ms < cold_ms / 2.0,
            "cache hit ({warm_ms:.2}ms) not meaningfully faster than the cold decode ({cold_ms:.2}ms)"
        );
    }

    #[test]
    fn readahead_warms_the_cache_ahead_of_requests() {
        let path = make_test_video();
        let (s, _, _) = open_session_core_t(path.to_str().unwrap()).unwrap();
        let arc = std::sync::Arc::new(Mutex::new(s));

        decode_frame_core(&mut arc.lock().unwrap(), 0).unwrap();
        spawn_readahead(arc.clone(), 0);

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            {
                let s = arc.lock().unwrap();
                let warmed = (1..=4).all(|i| s.cache.iter().any(|(idx, _)| *idx == i));
                if warmed {
                    break;
                }
                assert!(std::time::Instant::now() < deadline, "readahead never warmed frames 1-4 (cache: {:?})", s.cache.iter().map(|(i, _)| *i).collect::<Vec<_>>());
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let (_, seeked) = decode_frame_core(&mut arc.lock().unwrap(), 3).unwrap();
        assert!(!seeked, "frame 3 should have been a readahead cache hit");
    }

    #[test]
    fn readahead_warms_backward_on_all_intra() {
        let p = gen_video(
            "mjpeg_320_backwarm.mov",
            "testsrc2=size=320x240:rate=30:duration=2",
            &["-c:v", "mjpeg", "-q:v", "3", "-pix_fmt", "yuvj420p"],
        );
        let (s, _, _) = open_session_core_t(p.to_str().unwrap()).unwrap();
        assert!(is_all_intra_codec(&s.codec), "test file should read as all-intra, got {}", s.codec);
        let arc = std::sync::Arc::new(Mutex::new(s));
        decode_frame_core(&mut arc.lock().unwrap(), 20).unwrap();
        spawn_readahead(arc.clone(), 20);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            {
                let s = arc.lock().unwrap();
                let warmed = (17..=19).all(|i| s.cache.iter().any(|(idx, _)| *idx == i));
                if warmed { break; }
                assert!(std::time::Instant::now() < deadline, "backward frames 17-19 never warmed (cache: {:?})", s.cache.iter().map(|(i, _)| *i).collect::<Vec<_>>());
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let (_, seeked) = decode_frame_core(&mut arc.lock().unwrap(), 18).unwrap();
        assert!(!seeked, "backward frame 18 should be a cache hit");
    }

    // Reproduces the LIVE "3 videos struggle" root cause found 2026-07 via
    // a real [video-decode] DIAG capture: readahead's backward-fill and
    // foreground forward decode fighting over the same session's process,
    // producing an unbounded respawn loop (15907 readahead respawns vs 580
    // foreground in the captured session). Simulates sustained forward
    // playback — each tick decodes the next sequential frame (foreground)
    // and re-triggers readahead at that same center, exactly like the JS
    // playback loop's onFrameChanged. Before the fix, the readahead thread
    // would never go idle (constantly finding a "new" backward target as
    // center advances) and every backward attempt would force a respawn
    // since the process races ahead under real playback. After the fix, a
    // process racing more than READAHEAD_BACK frames ahead of the goal
    // skips backward-fill and the readahead thread goes idle instead of
    // spinning.
    #[test]
    fn readahead_backward_fill_does_not_fight_forward_playback() {
        let p = gen_video("play_thrash.mp4", "testsrc2=size=320x240:rate=30:duration=4", &["-c:v", "libx264", "-pix_fmt", "yuv420p", "-g", "15"]);
        let (s, _, _) = open_session_core_t(p.to_str().unwrap()).unwrap();
        let arc = std::sync::Arc::new(Mutex::new(s));

        let mut backward_respawns = 0u32;
        for tick in 0..60i64 {
            let seeked = decode_frame_core(&mut arc.lock().unwrap(), tick).unwrap().1;
            if seeked {
                let s = arc.lock().unwrap();
                if let Some(p) = &s.proc {
                    if p.next_frame - 1 < tick {
                        // decoder ended up behind the just-served frame —
                        // only possible if a backward readahead respawn
                        // raced in behind foreground's back.
                        backward_respawns += 1;
                    }
                }
            }
            spawn_readahead(arc.clone(), tick);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        // Drain any in-flight readahead thread so it can't keep respawning
        // after the loop ends and pollute the next test's process list.
        {
            let mut s = arc.lock().unwrap();
            s.readahead_active = false;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert_eq!(
            backward_respawns, 0,
            "readahead's backward-fill fought forward playback and forced the decoder position backward {backward_respawns} times — expected 0 under sustained forward playback"
        );
    }

    // Reproduces a THIRD live-caught issue in the same investigation
    // (2026-07, "c'est mieux mais y a encore une marge de manoeuvre" — the
    // tug-of-war with foreground was fixed, but once playback actually
    // stopped/paused, readahead's own backward-fill was found looping
    // forever: DIAG showed it cycling on 4 targets (71/73/74/75) at
    // 20-25ms/respawn indefinitely with the goal completely static. Root
    // cause: the flat 66MB v1 budget couldn't hold the full
    // READAHEAD_DEPTH+READAHEAD_BACK (11-frame) window at high resolution
    // — decoding one backward frame evicts another, which then reads as
    // "missing" on the next pass and gets redecoded, forever. A 1920x1080
    // RGBA frame is ~7.9MB, so 11 of them (~87MB) alone exceed the budget,
    // reproducing the thrash deterministically. The `attempted` set fixes
    // this by never re-targeting a frame this thread already paid a
    // respawn for within the current goal window, even if it was since
    // evicted — so the thread reaches "nothing left to attempt" and goes
    // idle instead of spinning.
    #[test]
    fn readahead_settles_even_when_cache_cannot_hold_the_full_window() {
        let p = gen_video("thrash_1080p.mp4", "testsrc2=size=1920x1080:rate=30:duration=3", &["-c:v", "libx264", "-pix_fmt", "yuv420p", "-g", "15"]);
        let (s, _, _) = open_session_core_t(p.to_str().unwrap()).unwrap();
        let arc = std::sync::Arc::new(Mutex::new(s));

        decode_frame_core(&mut arc.lock().unwrap(), 50).unwrap();
        spawn_readahead(arc.clone(), 50);

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
        loop {
            {
                let s = arc.lock().unwrap();
                if !s.readahead_active {
                    break;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "readahead never went idle — still spinning after 8s, cache thrash likely reintroduced"
                );
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }

    // Locks in the scrub-prefetch behavior (live-caught 2026-07, "la
    // lecture est vraiment bien, le scrub moyen": every BACKWARD scrub
    // step paid a full ~23ms respawn per session because the pipe can't
    // rewind). A respawn for a small BACKWARD STEP (previous request
    // minus a few — arrow-key/wheel scrubbing) over-seeks by
    // scrub_prefetch_for() frames and caches the walked window, so the
    // following backward steps are cache hits — one respawn amortized
    // over the whole window. Large jumps deliberately do NOT prefetch
    // (second live capture, "pour le scrub c'est pire": fast drag-scrubs
    // jump 13-120 frames per tick, so prefetch never paid off there but
    // its extra decodes taxed every tick).
    #[test]
    fn backward_scrub_steps_amortize_into_cache_hits() {
        let path = make_test_video(); // 320x240 → prefetch clamps to 6
        let (mut s, _, _) = open_session_core_t(path.to_str().unwrap()).unwrap();
        let (mut s_ref, _, _) = open_session_core_t(path.to_str().unwrap()).unwrap();

        // Cold seek to 40: a fling (no previous request), spawns exactly
        // at the target — no prefetch.
        let (_, seeked) = decode_frame_core(&mut s, 40).unwrap();
        assert!(seeked, "cold seek to 40 should be a real spawn");

        // First backward STEP (39, one behind the previous request):
        // pays the one respawn, which prefetches the window behind it.
        let (_, seeked) = decode_frame_core(&mut s, 39).unwrap();
        assert!(seeked, "first backward step should be the one real spawn");

        // Following backward steps: all inside the prefetched window.
        for target in (34..=38).rev() {
            let (bytes, seeked) = decode_frame_core(&mut s, target).unwrap();
            assert!(!seeked, "backward scrub to {target} respawned — prefetch window not cached");
            let truth = decode_frame_core(&mut s_ref, target).unwrap().0;
            assert_eq!(bytes, truth, "prefetched frame {target} is not byte-identical to a direct decode");
        }
    }

    // ---- indexed optimized media (the "scrub parfait" pipeline) ----

    fn make_indexed_media() -> String {
        test_ffmpeg_override();
        let src = make_test_video(); // 320x240, 60 frames, 30fps
        let dir = std::env::temp_dir().join("nemo-video-decode-test");
        let target = dir.join("optimized_indexed.mjpeg");
        let target_s = target.to_str().unwrap().to_string();
        if !target.exists() || !std::path::Path::new(&format!("{target_s}.idx")).exists() {
            create_optimized_media_core(src.to_str().unwrap(), &target_s).unwrap();
        }
        target_s
    }

    #[test]
    fn optimized_media_transcode_produces_a_valid_index() {
        let target = make_indexed_media();
        let idx: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(format!("{target}.idx")).unwrap()).unwrap();
        assert_eq!(idx["count"].as_u64().unwrap(), 60, "expected one index entry per source frame");
        assert_eq!(idx["width"].as_u64().unwrap(), 320);
        assert_eq!(idx["height"].as_u64().unwrap(), 240);
        assert!((idx["fps"].as_f64().unwrap() - 30.0).abs() < 0.01);
        let offsets = idx["offsets"].as_array().unwrap();
        assert_eq!(offsets.len(), 61, "count+1 offsets (sentinel = file length)");
        let len = std::fs::metadata(&target).unwrap().len();
        assert_eq!(offsets.last().unwrap().as_u64().unwrap(), len);
    }

    #[test]
    fn indexed_session_random_access_never_respawns_and_is_consistent() {
        let target = make_indexed_media();
        let (mut s, frame_count, _) = open_session_core_t(&target).unwrap();
        assert!(s.indexed.is_some(), "session should have opened in indexed mode");
        assert_eq!(frame_count, 60);

        // Ground truth from a SECOND independent indexed session.
        let (mut s_ref, _, _) = open_session_core_t(&target).unwrap();

        // Brutal scrub pattern: far jumps, backward runs, repeats.
        let pattern = [30i64, 5, 59, 0, 42, 41, 40, 12, 57, 3, 30];
        let mut seen = std::collections::HashMap::new();
        for &f in &pattern {
            let (bytes, seeked) = decode_frame_core(&mut s, f).unwrap();
            assert!(!seeked, "indexed decode of {f} reported a respawn — should be impossible");
            assert_eq!(bytes.len(), 320 * 240 * 4);
            let truth = decode_frame_core(&mut s_ref, f).unwrap().0;
            assert_eq!(bytes, truth, "frame {f} differs between two indexed sessions");
            seen.insert(f, bytes);
        }
        // Distinct frames must differ (testsrc2 has a moving pattern).
        assert_ne!(seen[&5], seen[&59], "distinct frames decoded identical bytes");
    }

    #[test]
    fn indexed_random_seek_cost_is_flat_and_small() {
        let target = make_indexed_media();
        let (mut s, _, _) = open_session_core_t(&target).unwrap();
        decode_frame_core(&mut s, 0).unwrap(); // absorb converter spawn
        s.cache.clear(); // measure decode, not cache hits
        let mut times = Vec::new();
        // Worst-case scrub: alternating far jumps, never sequential.
        let targets: Vec<i64> = (0..40).map(|i| if i % 2 == 0 { (i * 7) % 60 } else { 59 - ((i * 11) % 60) }).collect();
        for f in targets {
            let t0 = std::time::Instant::now();
            decode_frame_core(&mut s, f).unwrap();
            times.push(t0.elapsed().as_secs_f64() * 1000.0);
            s.cache.clear();
        }
        times.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let avg = times.iter().sum::<f64>() / times.len() as f64;
        let p95 = times[(times.len() as f64 * 0.95) as usize];
        eprintln!("[perf] indexed random access: avg={avg:.1}ms p95={p95:.1}ms");
        // Regression guard with headroom (tiny frames decode ~1-3ms; the
        // property under test is FLAT cost with no respawn spikes).
        assert!(p95 < 20.0, "indexed random access p95 {p95:.1}ms — respawn-like spikes crept back in");
    }

    #[test]
    fn stale_index_is_rejected_not_misread() {
        let target = make_indexed_media();
        let stale = std::env::temp_dir().join("nemo-video-decode-test").join("stale_copy.mjpeg");
        let stale_s = stale.to_str().unwrap().to_string();
        std::fs::copy(&target, &stale).unwrap();
        std::fs::copy(format!("{target}.idx"), format!("{stale_s}.idx")).unwrap();
        // Truncate the stream so the sidecar's sentinel no longer matches.
        let f = std::fs::OpenOptions::new().write(true).open(&stale).unwrap();
        let len = f.metadata().unwrap().len();
        f.set_len(len - 100).unwrap();
        drop(f);
        let err = match open_session_core_t(&stale_s) {
            Err(e) => e,
            Ok(_) => panic!("stale index was accepted — sentinel check is broken"),
        };
        assert!(err.contains("stale"), "expected a stale-index error, got: {err}");
    }

    #[test]
    fn cache_hit_does_not_corrupt_decoder_position() {
        let path = make_test_video();
        let (mut s, _, _) = open_session_core_t(path.to_str().unwrap()).unwrap();
        let (mut s_ref, _, _) = open_session_core_t(path.to_str().unwrap()).unwrap();
        let truth_46 = decode_frame_core(&mut s_ref, 46).unwrap().0;

        decode_frame_core(&mut s, 45).unwrap();
        decode_frame_core(&mut s, 10).unwrap();
        let (_, seeked) = decode_frame_core(&mut s, 45).unwrap();
        assert!(!seeked, "expected a cache hit for the revisit");
        let f46 = decode_frame_core(&mut s, 46).unwrap().0;
        assert_eq!(f46, truth_46, "frame 46 after a cache hit isn't the real frame 46 — decoder position corrupted");
    }

    #[test]
    fn concurrent_sessions_decode_in_parallel_not_serialized() {
        use std::sync::Arc as StdArc;
        use std::thread;
        use std::time::Instant as StdInstant;

        let path_a = make_test_video();
        let path_b = gen_video(
            "h264_320_concurrency_b.mp4",
            "testsrc2=size=320x240:rate=30:duration=3",
            &["-c:v", "libx264", "-pix_fmt", "yuv420p", "-g", "15"],
        );

        let sessions = StdArc::new(VideoSessions::default());
        let (sa, _, _) = open_session_core_t(path_a.to_str().unwrap()).unwrap();
        let (sb, _, _) = open_session_core_t(path_b.to_str().unwrap()).unwrap();
        let (id_a, id_b) = (1u32, 2u32);
        sessions.0.lock().unwrap().insert(id_a, StdArc::new(Mutex::new(sa)));
        sessions.0.lock().unwrap().insert(id_b, StdArc::new(Mutex::new(sb)));

        fn work(sessions: StdArc<VideoSessions>, id: u32) -> u128 {
            let t0 = StdInstant::now();
            for target in [50i64, 5, 55, 2, 58, 8] {
                let arc = sessions.0.lock().unwrap().get(&id).unwrap().clone();
                let mut s = arc.lock().unwrap();
                decode_frame_core(&mut s, target).unwrap();
            }
            t0.elapsed().as_millis()
        }

        let (s1, s2) = (sessions.clone(), sessions.clone());
        let t_wall = StdInstant::now();
        let ha = thread::spawn(move || work(s1, id_a));
        let hb = thread::spawn(move || work(s2, id_b));
        let da = ha.join().unwrap();
        let db = hb.join().unwrap();
        let wall = t_wall.elapsed().as_millis();

        eprintln!("[concurrency] A={da}ms B={db}ms wall={wall}ms sum={}ms", da + db);
        assert!(
            wall < (da + db) * 8 / 10,
            "wall={wall}ms not meaningfully less than sum={}ms — sessions may still be serializing",
            da + db
        );
    }

    // Reproduces LIVE report ("j'ai mis 3 vidéos ça a du mal à lire les 3
    // en temps réel" — log from the running app showed near-EVERY frame
    // respawning even for 1-frame steps, across 3 simultaneous sessions).
    // Simulates 3 sessions "playing" together: each tick, all three are
    // asked for their next sequential frame (mirroring the JS playback
    // loop's onFrameChanged, which calls every native-video layer once
    // per tick) with spawn_readahead fired after each — the exact
    // decode_video_frame command sequence, minus the Tauri IPC layer.
    #[test]
    fn three_simultaneous_sessions_playback_mostly_avoids_respawns() {
        let pa = make_test_video(); // 320x240, 60 frames
        let pb = gen_video("play3_b.mp4", "testsrc2=size=320x240:rate=30:duration=3", &["-c:v", "libx264", "-pix_fmt", "yuv420p", "-g", "15"]);
        let pc = gen_video("play3_c.mp4", "testsrc2=size=320x240:rate=30:duration=3", &["-c:v", "libx264", "-pix_fmt", "yuv420p", "-g", "15"]);
        let (sa, _, _) = open_session_core_t(pa.to_str().unwrap()).unwrap();
        let (sb, _, _) = open_session_core_t(pb.to_str().unwrap()).unwrap();
        let (sc, _, _) = open_session_core_t(pc.to_str().unwrap()).unwrap();
        let sessions = [
            std::sync::Arc::new(Mutex::new(sa)),
            std::sync::Arc::new(Mutex::new(sb)),
            std::sync::Arc::new(Mutex::new(sc)),
        ];

        let mut respawn_count = 0u32;
        let mut total = 0u32;
        for tick in 0..40i64 {
            for arc in &sessions {
                let (seeked, cache_hit) = {
                    let mut s = arc.lock().unwrap();
                    let hit = s.cache.iter().any(|(idx, _)| *idx == tick);
                    let seeked = if hit {
                        let pos = s.cache.iter().position(|(idx, _)| *idx == tick).unwrap();
                        let entry = s.cache.remove(pos).unwrap();
                        s.cache.push_front(entry);
                        false
                    } else {
                        decode_at(&mut s, tick, "foreground").unwrap().1
                    };
                    (seeked, hit)
                };
                total += 1;
                if seeked {
                    respawn_count += 1;
                }
                if !cache_hit {
                    spawn_readahead(arc.clone(), tick);
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        eprintln!("[diag] three-session playback: {respawn_count}/{total} requests needed a fresh spawn");
        assert!(
            respawn_count <= sessions.len() as u32 * 3,
            "{respawn_count}/{total} requests respawned — expected steady-state playback to avoid this almost entirely (only early warm-up spawns)"
        );
    }

    // Locks in the small-forward-reuse fix ("un peu de régression... chaque
    // seek respawn un process complet, même pour un pas d'une frame" —
    // caught live watching the user's own real scrub). A target a few
    // frames past the running process's own position must NOT trigger a
    // fresh spawn (`seeked` false) — it should read-and-discard from the
    // already-running process instead, and land byte-identical to the same
    // frame reached by pure sequential decode.
    #[test]
    fn small_forward_gap_reuses_running_process_no_respawn() {
        let path = make_test_video(); // gop 15, 60 frames
        let (mut s, _, _) = open_session_core_t(path.to_str().unwrap()).unwrap();

        // Ground truth: frame 12 via pure sequential decode.
        let mut truth_12 = Vec::new();
        for i in 0..=12 {
            truth_12 = decode_frame_core(&mut s, i).unwrap().0;
        }

        // Fresh session: land on frame 0 (spawns), then jump to frame 12 —
        // an 12-frame forward gap, within SMALL_FORWARD_REUSE_TOLERANCE —
        // must reuse the same process, not respawn.
        let (mut s2, _, _) = open_session_core_t(path.to_str().unwrap()).unwrap();
        decode_frame_core(&mut s2, 0).unwrap();
        let (jumped_12, seeked) = decode_frame_core(&mut s2, 12).unwrap();
        assert!(!seeked, "a 12-frame forward gap should reuse the running process, not respawn");
        assert_eq!(jumped_12, truth_12, "reused-process forward walk landed on the wrong frame");

        // A gap PAST the tolerance must still (correctly) respawn.
        let (mut s3, _, _) = open_session_core_t(path.to_str().unwrap()).unwrap();
        decode_frame_core(&mut s3, 0).unwrap();
        let (_far, seeked_far) = decode_frame_core(&mut s3, 40).unwrap();
        assert!(seeked_far, "a gap far beyond the small-forward tolerance should still respawn");
    }
}
