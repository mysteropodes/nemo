//! Open, sequential, seek, past-end, tail, codec/alpha and small-gap reuse.

use super::tests::*;
use super::*;

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


#[test]
fn codec_hevc_mp4() {
    let p = gen_video(
        "hevc_320.mp4",
        "testsrc2=size=320x240:rate=30:duration=2",
        &["-c:v", hevc_fixture_encoder(), "-pix_fmt", "yuv420p", "-g", "15", "-tag:v", "hvc1"],
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
        &["-c:v", h264_fixture_encoder(), "-pix_fmt", "yuv420p", "-g", "15"],
    );
    assert_decodes_and_seeks(&p, 954, 542);
}

#[test]
fn seek_accuracy_at_25fps() {
    let p = gen_video(
        "fps25_320.mp4",
        "testsrc2=size=320x240:rate=25:duration=2",
        &["-c:v", h264_fixture_encoder(), "-pix_fmt", "yuv420p", "-g", "12"],
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
        &["-c:v", h264_fixture_encoder(), "-pix_fmt", "yuv420p", "-g", "15"],
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
