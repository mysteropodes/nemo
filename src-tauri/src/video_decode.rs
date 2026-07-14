// ---- NATIVE VIDEO DECODE (EXPERIMENTAL, 2026-07) ----
// experimental/native-video-decode branch only — real-time video decoding
// in the native Tauri process via video-rs (ffmpeg's C libraries linked
// directly, NOT the CLI sidecar), streaming decoded RGBA8 frames to the
// webview over binary IPC (`tauri::ipc::Response` — raw ArrayBuffer on the
// JS side, no JSON/base64 encoding of pixel data).
//
// Why this exists: the shipped import path (images.js) bakes every video
// into per-frame JPEGs at import time — no live scrubbing of the source,
// 999-frame cap, quality loss, and seconds of import latency. This module
// is the experiment for the opposite architecture: keep the video CLOSED
// over its original file, decode frames on demand, and feed the existing
// GPU texture entry point (engine.register_image in geometry-wasm, which
// already accepts raw RGBA8 bytes + dims) with zero canvas round-trips.
//
// Performance contract per frame request (measured targets, 1080p source):
//   decode (sequential next-frame): ~2-8ms (codec-dependent)
//   RGB24→RGBA8 expansion:          ~1-2ms (single pass, stride-aware)
//   IPC transfer:                    Tauri Response = one buffer copy
//   Peak per-request memory:         width*height*4 bytes (~8.3MB @1080p),
//                                    freed as soon as JS consumes it.
// Random seeks are slower (keyframe seek + forward decode, see
// decode_video_frame) — that's inherent to inter-frame codecs, not a bug.
//
// Distribution caveat (deliberately unsolved on this branch): this links
// against the ffmpeg SHARED LIBRARIES found at build time via pkg-config
// (Homebrew's /opt/homebrew/opt/ffmpeg locally). A shipped build would
// need those .dylibs bundled or ffmpeg statically linked — plus the GPL
// question for x264/x265 in a commercial build. Experiment first.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, Once};
use std::time::Instant;

use serde::Serialize;
use tauri::ipc::Response;
use video_rs::decode::Decoder;

// ffmpeg global init must run exactly once before any decoder is opened.
static FFMPEG_INIT: Once = Once::new();
static NEXT_SESSION_ID: AtomicU32 = AtomicU32::new(1);

// After a keyframe seek, how many frames we're willing to decode forward
// to reach the exact target before giving up. Long-GOP sources (some
// screen recordings put keyframes 10s apart) can legitimately need
// hundreds; 900 covers 30s of 30fps material, beyond which returning the
// nearest earlier frame beats stalling the UI for seconds.
const MAX_SEEK_DECODE_AHEAD: u32 = 900;

pub struct VideoSession {
    decoder: Decoder,
    width: u32,
    height: u32,
    fps: f64,
    /// Frame index the decoder will produce on the NEXT decode_raw() call —
    /// the sequential-playback fast path (frame N+1 right after N) never
    /// seeks. i64 because a fresh post-seek position is derived from pts.
    next_frame: i64,
}

/// One global registry, one lock. Decoding is CPU-bound and per-session
/// sequential by nature (a decoder can't decode two frames concurrently),
/// and the UI requests one frame at a time — a single Mutex is correct
/// here, not just simple. If simultaneous multi-video playback ever needs
/// real parallelism, split into per-session Mutexes then (documented
/// scaling point, not needed for the experiment).
pub struct VideoSessions(pub Mutex<HashMap<u32, VideoSession>>);

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
}

// Core open logic, tauri-free so `cargo test` can exercise it directly
// (the #[tauri::command] wrappers below only add State registry plumbing).
fn open_session_core(path: &str) -> Result<(VideoSession, u64, f64), String> {
    FFMPEG_INIT.call_once(|| {
        // video_rs::init() registers ffmpeg's formats/codecs; failure here
        // means the linked libs are broken, not a per-file problem.
        video_rs::init().expect("ffmpeg init failed");
    });

    let decoder =
        Decoder::new(std::path::Path::new(path)).map_err(|e| format!("open failed: {e}"))?;

    let (width, height) = decoder.size();
    let fps = decoder.frame_rate() as f64;
    if fps <= 0.0 || width == 0 || height == 0 {
        return Err(format!(
            "invalid stream parameters (w={width} h={height} fps={fps})"
        ));
    }
    let frame_count = decoder.frames().map_err(|e| format!("frames: {e}"))?;
    let duration_seconds = decoder
        .duration()
        .map(|d| d.as_secs_f64())
        .unwrap_or(frame_count as f64 / fps);

    Ok((
        VideoSession {
            decoder,
            width,
            height,
            fps,
            next_frame: 0,
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
    let id = NEXT_SESSION_ID.fetch_add(1, Ordering::Relaxed);
    state.0.lock().unwrap().insert(id, session);

    Ok(VideoInfo {
        session_id: id,
        width,
        height,
        fps,
        frame_count,
        duration_seconds,
    })
}

// Core per-frame decode, tauri-free (see open_session_core). Returns the
// tightly-packed RGBA8 buffer (`width*height*4`, row-major, no padding).
fn decode_frame_core(s: &mut VideoSession, frame_index: i64) -> Result<Vec<u8>, String> {
    let t0 = Instant::now();
    let mut seeked = false;
    if frame_index != s.next_frame {
        // Random access: ffmpeg's seek lands on the KEYFRAME at/before the
        // target (inter-frame codecs can't start mid-GOP), so after the
        // seek we decode forward until the frame whose pts maps to the
        // requested index — that's what frame-accurate means for real
        // codecs, and it's why backward scrubs cost more than playback.
        s.decoder
            .seek_to_frame(frame_index)
            .map_err(|e| format!("seek: {e}"))?;
        seeked = true;
    }

    let time_base = s.decoder.time_base();
    let tb = time_base.numerator() as f64 / time_base.denominator() as f64;
    let mut decoded_ahead: u32 = 0;

    let frame = loop {
        let raw = s
            .decoder
            .decode_raw()
            .map_err(|e| format!("decode: {e}"))?;
        if !seeked {
            break raw; // sequential fast path: next frame IS the target
        }
        // Map the decoded frame's pts back to a frame index. rounding
        // (not floor) because pts*tb*fps for frame N is N ± float noise.
        let idx = match raw.pts() {
            Some(pts) => (pts as f64 * tb * s.fps).round() as i64,
            None => -1, // no pts (rare) — keep decoding, can't identify it
        };
        if idx >= frame_index {
            break raw;
        }
        decoded_ahead += 1;
        if decoded_ahead > MAX_SEEK_DECODE_AHEAD {
            // Pathological GOP — return what we have rather than stall.
            break raw;
        }
    };
    let decode_ms = t0.elapsed().as_secs_f64() * 1000.0;

    // RGB24 (video-rs's scaler output, guaranteed by its FRAME_PIXEL_FORMAT)
    // → tightly-packed RGBA8. Row-by-row because ffmpeg pads each line to
    // an alignment boundary: `stride(0) >= width*3`, and reading the plane
    // as if it were packed would shear every row after the first.
    let w = s.width as usize;
    let h = s.height as usize;
    let stride = frame.stride(0);
    let data = frame.data(0);
    let mut rgba = vec![255u8; w * h * 4]; // pre-filled alpha, only RGB written below
    for y in 0..h {
        let src_row = &data[y * stride..y * stride + w * 3];
        let dst_row = &mut rgba[y * w * 4..(y + 1) * w * 4];
        for x in 0..w {
            dst_row[x * 4] = src_row[x * 3];
            dst_row[x * 4 + 1] = src_row[x * 3 + 1];
            dst_row[x * 4 + 2] = src_row[x * 3 + 2];
            // alpha already 255 from the vec! fill
        }
    }

    s.next_frame = frame_index + 1;

    let convert_ms = t0.elapsed().as_secs_f64() * 1000.0 - decode_ms;
    // Timing to stderr (visible in `tauri dev` console) — cheap, honest
    // instrumentation for the experiment; a stats command can replace it
    // if this graduates.
    eprintln!(
        "[video-decode] frame={frame_index} seeked={seeked} ahead={decoded_ahead} decode={decode_ms:.1}ms convert={convert_ms:.1}ms"
    );

    Ok(rgba)
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
    let mut sessions = state.0.lock().unwrap();
    let s = sessions
        .get_mut(&session_id)
        .ok_or_else(|| format!("no session {session_id}"))?;
    decode_frame_core(s, frame_index).map(Response::new)
}

#[tauri::command]
pub fn close_video_session(
    state: tauri::State<'_, VideoSessions>,
    session_id: u32,
) -> Result<(), String> {
    state.0.lock().unwrap().remove(&session_id);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    /// Generates a real 60-frame 30fps H.264 test video with the BUNDLED
    /// ffmpeg CLI binary (same one the app ships as its sidecar), so the
    /// test exercises decoding of exactly the kind of file the app's own
    /// export produces. testsrc2 animates every frame — consecutive frames
    /// are guaranteed to differ, which the accuracy assertions rely on.
    fn make_test_video() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join("nemo-video-decode-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("testsrc2_60f_30fps.mp4");
        if !path.exists() {
            let ffmpeg = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("binaries/ffmpeg-aarch64-apple-darwin");
            let status = Command::new(&ffmpeg)
                .args([
                    "-y",
                    "-f", "lavfi",
                    "-i", "testsrc2=size=320x240:rate=30:duration=2",
                    "-c:v", "libx264",
                    "-pix_fmt", "yuv420p",
                    // Short GOP so the seek test crosses keyframe boundaries
                    "-g", "15",
                    path.to_str().unwrap(),
                ])
                .status()
                .expect("bundled ffmpeg binary must exist and run");
            assert!(status.success(), "test video generation failed");
        }
        path
    }

    #[test]
    fn open_reports_correct_stream_parameters() {
        let path = make_test_video();
        let (s, frame_count, duration) = open_session_core(path.to_str().unwrap()).unwrap();
        assert_eq!((s.width, s.height), (320, 240));
        assert!((s.fps - 30.0).abs() < 0.01, "fps was {}", s.fps);
        assert_eq!(frame_count, 60);
        assert!((duration - 2.0).abs() < 0.1, "duration was {duration}");
    }

    #[test]
    fn sequential_decode_yields_correctly_sized_distinct_frames() {
        let path = make_test_video();
        let (mut s, _, _) = open_session_core(path.to_str().unwrap()).unwrap();
        let expected_len = (s.width * s.height * 4) as usize;
        let f0 = decode_frame_core(&mut s, 0).unwrap();
        let f1 = decode_frame_core(&mut s, 1).unwrap();
        assert_eq!(f0.len(), expected_len);
        assert_eq!(f1.len(), expected_len);
        // Every 4th byte is alpha and must be exactly 255 (opaque source)
        assert!(f0.iter().skip(3).step_by(4).all(|&a| a == 255));
        // testsrc2 animates — consecutive frames must not be identical
        assert_ne!(f0, f1, "consecutive frames were byte-identical");
    }

    #[test]
    fn random_seek_is_frame_accurate() {
        let path = make_test_video();
        let (mut s, _, _) = open_session_core(path.to_str().unwrap()).unwrap();
        // Reference: decode frame 40 by pure sequential playback from 0 —
        // ground truth by construction (no seek involved at all).
        let mut sequential_f40 = Vec::new();
        for i in 0..=40 {
            sequential_f40 = decode_frame_core(&mut s, i).unwrap();
        }
        // Now jump BACKWARD to frame 40 from position 41 — forces a real
        // keyframe seek (gop=15, so the keyframe is frame 30) + forward
        // decode. Frame accuracy = the seeked result matches the
        // sequential ground truth byte-for-byte.
        let seeked_f40 = decode_frame_core(&mut s, 40).unwrap();
        assert_eq!(
            seeked_f40, sequential_f40,
            "seeked frame 40 != sequentially-decoded frame 40 (seek is not frame-accurate)"
        );
    }

    #[test]
    fn decode_past_end_errors_cleanly() {
        let path = make_test_video();
        let (mut s, frame_count, _) = open_session_core(path.to_str().unwrap()).unwrap();
        // Way past the end — must be an Err, not a panic/hang.
        let r = decode_frame_core(&mut s, frame_count as i64 + 100);
        assert!(r.is_err(), "decoding past EOF should error, got Ok");
    }
}
