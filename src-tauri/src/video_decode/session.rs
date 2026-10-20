//! Per-session decode state, its byte-bounded frame cache and the open
//! path that probes a source (or detects an indexed optimized stream).

use std::process::Command;

use crate::media_probe::parse_probe;

use super::indexed::{try_open_indexed, IndexedSeq};
use super::process::{ffmpeg_path, FfmpegProc};

pub struct VideoSession {
    pub(super) path: String,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) fps: f64,
    pub(super) frame_bytes: usize, // width*height*4, cached — the exact tightly-packed rawvideo/rgba frame size
    /// Total frames, derived from container duration × fps (ffmpeg's `-i`
    /// info dump never reliably prints a frame count — v1 only fell back
    /// to this math for WebM specifically; v2 uses it universally since
    /// there's no other source of truth available without ffprobe, which
    /// isn't bundled — see open_session_core's own comment).
    pub(super) frame_count: i64,
    pub(super) proc: Option<FfmpegProc>,
    /// Recently-decoded frame cache, most-recent-first — unchanged from
    /// v1 (see cache_budget_for). Byte-bounded, not frame-count-
    /// bounded: frame size varies 8x between 1080p and 4K.
    pub(super) cache: std::collections::VecDeque<(i64, Vec<u8>)>,
    pub(super) readahead_active: bool,
    pub(super) readahead_goal: i64,
    /// Last frame index the FOREGROUND asked for — lets decode_at tell a
    /// frame-by-frame backward step (previous request minus a few) from a
    /// fast scrub fling (arbitrary large jumps), which need opposite
    /// respawn strategies (see the scrub-prefetch comment in decode_at).
    pub(super) last_foreground_request: i64,
    /// Some(..) when this session is an indexed optimized-media stream
    /// (random access, persistent converter — see IndexedSeq). All the
    /// seek/respawn/readahead machinery above is bypassed entirely.
    pub(super) indexed: Option<IndexedSeq>,
    /// Source codec name (lowercase, e.g. "h264", "prores") — lets the JS
    /// side decide whether background media optimization (transcode to
    /// all-intra) is worth it, and gates bidirectional readahead the same
    /// way v1 did.
    pub(super) codec: String,
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
pub(super) const FRAME_CACHE_MIN_BYTES: usize = 66 * 1024 * 1024;
pub(super) const FRAME_CACHE_MAX_BYTES: usize = 320 * 1024 * 1024;
/// How far past the last requested frame the readahead thread decodes,
/// and how far BEHIND it — unchanged from v1.
pub(super) const READAHEAD_DEPTH: i64 = 8;
pub(super) const READAHEAD_BACK: i64 = 3;

/// Per-session cache budget: the full readahead window (1 current +
/// DEPTH forward + BACK back) plus a few frames of slack, clamped to
/// [66MB, 320MB]. 1080p (~8MB/frame): ~128MB. 4K (~33MB/frame): hits the
/// 320MB ceiling (~9 frames — window still fits, barely). The ceiling
/// bounds worst-case memory at ~1GB for 3 simultaneous 4K sessions.
pub(super) fn cache_budget_for(frame_bytes: usize) -> usize {
    let window = (1 + READAHEAD_DEPTH + READAHEAD_BACK) as usize + 4;
    (frame_bytes * window).clamp(FRAME_CACHE_MIN_BYTES, FRAME_CACHE_MAX_BYTES)
}
/// Shared by decode_at (actual reuse decision) and spawn_readahead (peeking
/// whether a candidate target would cost a respawn, without paying for one).
pub(super) const SMALL_FORWARD_REUSE_TOLERANCE: i64 = 12;

/// Would decoding `target` on session `s` require killing+respawning the
/// ffmpeg process, or can the running one serve it? Mirrors decode_at's own
/// reuse decision exactly (backward gaps and gaps beyond tolerance always
/// need a respawn) — used by spawn_readahead to avoid the tug-of-war where
/// a readahead-triggered respawn fights foreground playback for the same
/// session's process (see spawn_readahead's own comment for the full story).
pub(super) fn would_need_respawn(s: &VideoSession, target: i64) -> bool {
    match &s.proc {
        Some(p) => !(0..=SMALL_FORWARD_REUSE_TOLERANCE).contains(&(target - p.next_frame)),
        None => true,
    }
}

/// Insert a decoded frame into the session cache (skipping if that index is
/// already present) and evict oldest entries past the session's budget.
/// Shared by foreground decode, the readahead thread, and decode_at's
/// walk-prefetch (see decode.rs) so eviction policy lives in exactly one place.
pub(super) fn cache_insert(s: &mut VideoSession, idx: i64, bytes: Vec<u8>) {
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
pub(super) fn scrub_prefetch_for(s: &VideoSession) -> i64 {
    let fit = (cache_budget_for(s.frame_bytes) / s.frame_bytes.max(1)) as i64;
    (fit / 3).clamp(2, 6)
}
/// Tail-of-stream backoff-retry cap — unchanged from v1's spirit (widen
/// the re-seek origin until it reaches 0), expressed here as a max
/// number of widening attempts instead of a frame-walk count, since v2
/// doesn't count ffmpeg's internal walk.
pub(super) const MAX_TAIL_BACKOFF_ATTEMPTS: u32 = 8;

/// No longer used to gate readahead direction (see spawn_readahead's own
/// comment — that distinction stopped mattering in v2's cost model). Kept
/// for its test coverage (documents which codec names this module
/// recognizes as all-intra) and as a small utility if a future tuning
/// pass needs it again; JS already has its own independent equivalent
/// (`_isAllIntra` in native-video-bridge.js) for the optimized-media
/// transcode decision, so this one is genuinely unused by production
/// Rust code right now.
#[allow(dead_code)]
pub(super) fn is_all_intra_codec(codec: &str) -> bool {
    [
        "mjpeg", "prores", "dnxhd", "rawvideo", "qtrle", "huffyuv", "ffv1", "png", "v210",
    ]
    .iter()
    .any(|k| codec.contains(k))
}

// Core open logic, tauri-free so `cargo test` can exercise it directly
// (the #[tauri::command] wrappers in the parent module only add State registry plumbing).
pub(super) fn open_session_core(path: &str) -> Result<(VideoSession, u64, f64), String> {
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
    let (width, height, fps, duration_seconds, codec) = parse_probe(&stderr).map_err(|e| {
        format!("probe parse failed for {path}: {e}\n--- ffmpeg -i stderr ---\n{stderr}")
    })?;
    if fps <= 0.0 || width == 0 || height == 0 {
        return Err(format!(
            "invalid stream parameters (w={width} h={height} fps={fps})"
        ));
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
