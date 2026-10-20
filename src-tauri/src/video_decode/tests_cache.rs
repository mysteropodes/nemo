//! Frame cache reuse and the readahead thread in both directions.

use super::session::is_all_intra_codec;
use super::tests::*;
use super::*;

#[test]
fn revisited_frame_is_cache_accelerated_and_byte_identical() {
    let p = gen_video(
        "h264_320_biggop.mp4",
        "testsrc2=size=320x240:rate=30:duration=3",
        &[
            "-c:v",
            h264_fixture_encoder(),
            "-pix_fmt",
            "yuv420p",
            "-g",
            "60",
        ],
    );
    let (mut s, _frame_count, _) = open_session_core_t(p.to_str().unwrap()).unwrap();

    let target = 45i64;
    let t0 = std::time::Instant::now();
    let (first, seeked) = decode_frame_core(&mut s, target).unwrap();
    let cold_ms = t0.elapsed().as_secs_f64() * 1000.0;
    assert!(
        seeked,
        "target should have required a real spawn+seek to set up this test"
    );

    decode_frame_core(&mut s, 10).unwrap();
    let t1 = std::time::Instant::now();
    let (second, seeked2) = decode_frame_core(&mut s, target).unwrap();
    let warm_ms = t1.elapsed().as_secs_f64() * 1000.0;

    assert_eq!(
        first, second,
        "cached frame bytes diverged from the original decode"
    );
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
            assert!(
                std::time::Instant::now() < deadline,
                "readahead never warmed frames 1-4 (cache: {:?})",
                s.cache.iter().map(|(i, _)| *i).collect::<Vec<_>>()
            );
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
    assert!(
        is_all_intra_codec(&s.codec),
        "test file should read as all-intra, got {}",
        s.codec
    );
    let arc = std::sync::Arc::new(Mutex::new(s));
    decode_frame_core(&mut arc.lock().unwrap(), 20).unwrap();
    spawn_readahead(arc.clone(), 20);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        {
            let s = arc.lock().unwrap();
            let warmed = (17..=19).all(|i| s.cache.iter().any(|(idx, _)| *idx == i));
            if warmed {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "backward frames 17-19 never warmed (cache: {:?})",
                s.cache.iter().map(|(i, _)| *i).collect::<Vec<_>>()
            );
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
    let p = gen_video(
        "play_thrash.mp4",
        "testsrc2=size=320x240:rate=30:duration=4",
        &[
            "-c:v",
            h264_fixture_encoder(),
            "-pix_fmt",
            "yuv420p",
            "-g",
            "15",
        ],
    );
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
    let p = gen_video(
        "thrash_1080p.mp4",
        "testsrc2=size=1920x1080:rate=30:duration=3",
        &[
            "-c:v",
            h264_fixture_encoder(),
            "-pix_fmt",
            "yuv420p",
            "-g",
            "15",
        ],
    );
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
        assert!(
            !seeked,
            "backward scrub to {target} respawned — prefetch window not cached"
        );
        let truth = decode_frame_core(&mut s_ref, target).unwrap().0;
        assert_eq!(
            bytes, truth,
            "prefetched frame {target} is not byte-identical to a direct decode"
        );
    }
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
    assert_eq!(
        f46, truth_46,
        "frame 46 after a cache hit isn't the real frame 46 — decoder position corrupted"
    );
}
