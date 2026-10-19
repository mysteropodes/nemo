//! Decoding one frame: cache lookup, sequential reuse, seek respawn,
//! scrub prefetch and the tail-of-stream backoff retry.

use super::indexed::indexed_decode;
use super::process::{read_one_frame, spawn_at};
use super::session::{
    cache_insert, scrub_prefetch_for, VideoSession, MAX_TAIL_BACKOFF_ATTEMPTS,
    SMALL_FORWARD_REUSE_TOLERANCE,
};

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
pub(super) fn decode_at(
    s: &mut VideoSession,
    frame_index: i64,
    caller: &str,
) -> Result<(Vec<u8>, bool), String> {
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
    // Rust repro (tests_sessions.rs) looked alarming at a glance but was actually fine
    // (2.5% respawn rate — see three_simultaneous_sessions_playback_
    // mostly_avoids_respawns). This version distinguishes the two.
    let gap_for_diag = s.proc.as_ref().map(|p| frame_index - p.next_frame);
    if let Some(p) = &s.proc {
        let gap = frame_index - p.next_frame;
        if gap >= 0 && gap <= SMALL_FORWARD_REUSE_TOLERANCE {
            origin = p.next_frame;
        }
    }
    if origin == frame_index
        && (s.proc.is_none()
            || gap_for_diag.is_some_and(|g| !(0..=SMALL_FORWARD_REUSE_TOLERANCE).contains(&g)))
    {
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
                    // Reap before dropping — every OTHER site that replaces
                    // s.proc (spawn_at, close_video_session) kills+waits
                    // first; this one didn't, leaking a zombie ffmpeg child
                    // per tail-retry. Repeated scrubbing near a clip's end
                    // (this branch fires on every retry) accumulated
                    // defunct processes until Command::spawn started
                    // failing under process/FD-table pressure — the likely
                    // cause of "bug de crash lors de scrub" after sustained
                    // end-of-clip scrubbing.
                    if let Some(mut p) = s.proc.take() {
                        let _ = p.child.kill();
                        let _ = p.child.wait();
                    }
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
pub(super) fn decode_frame_core(
    s: &mut VideoSession,
    frame_index: i64,
) -> Result<(Vec<u8>, bool), String> {
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
