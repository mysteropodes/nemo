//! Measured sequential, open and seek cost on real fixtures.

use super::tests::*;
use super::*;

#[test]
fn perf_1080p_sequential_holds_30fps_budget() {
    let p = gen_video(
        "perf_1080p.mp4",
        "testsrc2=size=1920x1080:rate=30:duration=2",
        &["-c:v", h264_fixture_encoder(), "-pix_fmt", "yuv420p", "-g", "15"],
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
        &["-c:v", h264_fixture_encoder(), "-pix_fmt", "yuv420p", "-g", "15"],
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
        &["-c:v", h264_fixture_encoder(), "-pix_fmt", "yuv420p", "-g", "15"],
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
