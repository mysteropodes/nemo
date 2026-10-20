//! Shared support for the native decoder tests: the checked FFmpeg
//! selection, isolated fixture generation and measurement helpers. The
//! tests themselves live in the sibling `tests_*` modules, grouped by the
//! responsibility they cover; the `tests` prefix keeps them all under the
//! `video_decode::tests` filter.

use super::*;

// The `parse_probe`/`parse_hms` unit tests moved with the parser to
// `crate::media_probe` (P28). What remains here needs a real ffmpeg
// binary; those do not.

use std::process::Command as StdCommand;

// Read the override once, before any native work. Never mutate the shared
// process environment: parallel tests and decode threads use this same path.
pub(super) fn test_ffmpeg_path() -> &'static std::path::PathBuf {
    static FFMPEG: std::sync::OnceLock<Result<std::path::PathBuf, String>> =
        std::sync::OnceLock::new();
    FFMPEG
        .get_or_init(|| {
            checked_test_ffmpeg(
                std::env::var_os("NEMO_TEST_FFMPEG_PATH"),
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("binaries/ffmpeg-aarch64-apple-darwin"),
            )
        })
        .as_ref()
        .unwrap_or_else(|error| panic!("{error}"))
}

pub(super) fn checked_test_ffmpeg(
    override_path: Option<std::ffi::OsString>,
    bundled: std::path::PathBuf,
) -> Result<std::path::PathBuf, String> {
    // An explicitly empty/missing path is an error, never a bundled fallback.
    let selected = override_path
        .map(std::path::PathBuf::from)
        .unwrap_or(bundled);
    let prerequisite = format!(
        "native video test FFmpeg prerequisite ({})",
        selected.display()
    );
    let ffmpeg = selected
        .canonicalize()
        .map_err(|error| format!("{prerequisite}: {error}"))?;
    if !ffmpeg.is_file() {
        return Err(format!("{prerequisite}: executable path is not a file"));
    }
    let output = StdCommand::new(&ffmpeg)
        .args(["-hide_banner", "-version"])
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|error| format!("{prerequisite}: cannot launch: {error}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    if !output.status.success() || !stdout.starts_with("ffmpeg version ") {
        return Err(format!(
            "{prerequisite}: -version failed ({}); stdout: {}; stderr: {}",
            output.status,
            stdout.trim(),
            String::from_utf8_lossy(&output.stderr).trim(),
        ));
    }
    eprintln!(
        "[native video tests] {}: {}",
        ffmpeg.display(),
        stdout.lines().next().unwrap()
    );
    Ok(ffmpeg)
}

pub(super) fn test_fixture_dir() -> &'static std::path::PathBuf {
    static DIR: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
    test_ffmpeg_path(); // Preflight even if a caller would otherwise reuse a fixture.
    DIR.get_or_init(|| {
        // Fresh for every process/run: old fixtures cannot hide a broken encoder
        // or come from another executable, worktree, or concurrent cargo process.
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "nemo-video-decode-test-{}-{nonce}",
            std::process::id(),
        ));
        std::fs::create_dir(&dir).expect("create isolated video test fixture directory");
        dir
    })
}

pub(super) fn h264_fixture_encoder() -> &'static str {
    static ENCODER: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    ENCODER.get_or_init(|| {
        std::env::var("NEMO_TEST_H264_ENCODER").unwrap_or_else(|_| "h264_videotoolbox".to_string())
    })
}

pub(super) fn hevc_fixture_encoder() -> &'static str {
    static ENCODER: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    ENCODER.get_or_init(|| {
        std::env::var("NEMO_TEST_HEVC_ENCODER").unwrap_or_else(|_| "hevc_videotoolbox".to_string())
    })
}

/// Generates fixtures with the same checked executable used for decode/probe,
/// cached by filename within this test process. The bundled LGPL sidecar is
/// the default; NEMO_TEST_FFMPEG_PATH may select an explicit executable path
/// before the test process starts. Hosted CI also selects software H.264/
/// HEVC encoders because its VM cannot create VideoToolbox sessions. Every
/// fixture codec, filter, and decoder assertion remains required.
pub(super) fn gen_video(filename: &str, lavfi: &str, codec_args: &[&str]) -> std::path::PathBuf {
    let ffmpeg = test_ffmpeg_path();
    let dir = test_fixture_dir();
    let path = dir.join(filename);
    // Generation is serialized and atomic. `cargo test` runs these tests
    // on every core; without the lock, two tests sharing a fixture name
    // raced (one decoded a half-written file, the other re-encoded over
    // it with `-y`), and VideoToolbox refused to open yet another
    // hardware session under that load (`err=-12903`), leaving a 0-byte
    // file that poisoned every later run. Encode to a `.part` sibling and
    // rename, so a fixture either exists complete or not at all.
    static GEN_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = GEN_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let complete = path.metadata().map(|m| m.len() > 0).unwrap_or(false);
    if !complete {
        let part = dir.join(format!("{filename}.part"));
        let _ = std::fs::remove_file(&part);
        let mut args: Vec<&str> = vec!["-y", "-f", "lavfi", "-i", lavfi];
        args.extend_from_slice(codec_args);
        let out = part.to_str().unwrap().to_string();
        // ffmpeg picks the muxer from the output extension; `.part` has
        // none, so name the format from the fixture's own extension.
        let fmt = match std::path::Path::new(filename)
            .extension()
            .and_then(|e| e.to_str())
        {
            Some("mp4") => "mp4",
            Some("mov") => "mov",
            Some("webm") => "webm",
            Some("mjpeg") => "mjpeg",
            Some(other) => other,
            None => "mp4",
        };
        args.extend_from_slice(&["-f", fmt]);
        args.push(&out);
        let output = StdCommand::new(ffmpeg)
            .args(&args)
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap_or_else(|error| {
                panic!(
                    "test fixture FFmpeg {} cannot launch: {error}",
                    ffmpeg.display()
                )
            });
        assert!(
            output.status.success(),
            "test video generation failed: {filename}; FFmpeg {}; {}; stderr: {}",
            ffmpeg.display(),
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
        std::fs::rename(&part, &path).expect("fixture rename");
    }
    path
}

pub(super) fn make_test_video() -> std::path::PathBuf {
    gen_video(
        "testsrc2_60f_30fps.mp4",
        "testsrc2=size=320x240:rate=30:duration=2",
        &[
            "-c:v",
            h264_fixture_encoder(),
            "-pix_fmt",
            "yuv420p",
            "-g",
            "15",
        ],
    )
}

// Test ffmpeg_path() routes all decode/probe calls to the checked selection.
pub(super) fn open_session_core_t(path: &str) -> Result<(VideoSession, u64, f64), String> {
    open_session_core(path)
}

pub(super) fn assert_decodes_and_seeks(path: &std::path::Path, expect_w: u32, expect_h: u32) {
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
pub(super) fn measure_sequential_steady_state(path: &std::path::Path, frames: i64) -> (f64, f64) {
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

pub(super) fn make_indexed_media() -> String {
    // All indexed tests share one complete transcode and its index. OnceLock
    // serializes initialization, just as GEN_LOCK protects source fixtures.
    static INDEXED: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    INDEXED
        .get_or_init(|| {
            let src = make_test_video(); // 320x240, 60 frames, 30fps
            let target = test_fixture_dir().join("optimized_indexed.mjpeg");
            let target_s = target.to_str().unwrap().to_string();
            create_optimized_media_core(src.to_str().unwrap(), &target_s).unwrap();
            target_s
        })
        .clone()
}
