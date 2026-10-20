//! The background readahead thread that warms a session's cache around
//! the last requested frame.

use std::sync::{Arc, Mutex};

use super::decode::decode_at;
use super::lock_or_recover;
use super::session::{
    cache_budget_for, cache_insert, would_need_respawn, VideoSession, READAHEAD_BACK,
    READAHEAD_DEPTH,
};

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
pub(super) fn spawn_readahead(arc: Arc<Mutex<VideoSession>>, center: i64) {
    {
        let mut s = lock_or_recover(&arc);
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
            let mut s = lock_or_recover(&arc);
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
            let goal_recently_moving =
                last_goal_change.elapsed() < std::time::Duration::from_millis(150);
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
            if s.cache.iter().map(|(_, b)| b.len()).sum::<usize>()
                >= cache_budget_for(s.frame_bytes)
            {
                s.readahead_active = false;
                break;
            }
            drop(s);
            std::thread::yield_now();
        }
    });
}
