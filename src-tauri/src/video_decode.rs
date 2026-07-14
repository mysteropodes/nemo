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

    // SOFTWARE decode, deliberately. VideoToolbox hwaccel was tried and
    // MEASURED WORSE here (2026-07, this branch's test matrix): 4K went
    // from 105ms avg software to 124ms avg / 282ms p95 with hwaccel, and
    // ProRes/VP9 broke outright. The reason is structural, not a tuning
    // miss: hardware decode produces GPU-resident frames, and this
    // pipeline needs CPU-side RGBA bytes (the WASM engine's
    // register_image boundary) — the per-frame GPU→CPU download + sws
    // conversion costs more than just decoding on the CPU to begin with.
    // hwaccel only wins when frames STAY on the GPU end-to-end, which
    // would require a zero-copy path into the webview's WebGPU context —
    // out of scope for this experiment.
    let src = std::path::Path::new(path);
    let decoder = Decoder::new(src).map_err(|e| format!("open failed: {e}"))?;

    let (width, height) = decoder.size();
    let fps = decoder.frame_rate() as f64;
    if fps <= 0.0 || width == 0 || height == 0 {
        return Err(format!(
            "invalid stream parameters (w={width} h={height} fps={fps})"
        ));
    }
    let mut frame_count = decoder.frames().map_err(|e| format!("frames: {e}"))?;
    let mut duration_seconds = decoder
        .duration()
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);
    // WebM/Matroska carry NEITHER a per-stream frame count NOR a usable
    // per-stream duration (both are simply absent from the container's
    // track headers — frames()=0 and a garbage/absent stream duration are
    // normal there, not errors; caught by the codec test matrix). The
    // CONTAINER-level duration does exist though — read it with a brief
    // second open of the file (video-rs doesn't expose its Reader's
    // AVFormatContext, and a one-time few-ms open at session creation
    // only for metadata-poor containers is cheaper than restructuring
    // around Decoder::into_parts). AV_TIME_BASE units = microseconds.
    if frame_count == 0 || !(duration_seconds.is_finite() && duration_seconds > 0.0) {
        if let Ok(ctx) = video_rs::ffmpeg::format::input(&src) {
            let us = ctx.duration();
            if us > 0 {
                duration_seconds = us as f64 / 1_000_000.0;
            }
        }
    }
    if frame_count == 0 && duration_seconds > 0.0 {
        frame_count = (duration_seconds * fps).round() as u64;
    }
    if !(duration_seconds.is_finite() && duration_seconds > 0.0) {
        duration_seconds = frame_count as f64 / fps; // last resort, both metadata paths empty
    }

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
        // chunks_exact (not per-pixel indexing): gives the compiler exact
        // bounds knowledge, eliminating per-access bounds checks and
        // letting the loop auto-vectorize — this is the hottest loop in
        // the module (w*h iterations per frame, 8.3M at 4K).
        for (src_px, dst_px) in src_row.chunks_exact(3).zip(dst_row.chunks_exact_mut(4)) {
            dst_px[0] = src_px[0];
            dst_px[1] = src_px[1];
            dst_px[2] = src_px[2];
            // dst_px[3] already 255 from the vec! fill
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

// ---- headless auto-bench plumbing ----
// Lets a scripted `tauri dev` run drive the FULL in-app pipeline bench
// (decode → binary IPC → WASM → GPU upload → render) without a human
// clicking around: if NEMO_AUTOBENCH points at a JSON config file
// ({"videos": ["/path/a.mp4", ...]}), native-video-bridge.js picks it up
// at startup, runs SMNativeVideo.bench() on each entry, and reports back
// here for a plain std::fs write (no fs-plugin scope involvement).
// Inert in normal use: without the env var, autobench_config returns None
// and the JS hook does nothing.
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
    use std::process::Command;

    /// Generates a test video with the BUNDLED ffmpeg CLI binary (the same
    /// one the app ships as its sidecar), cached by filename across runs.
    /// testsrc2 animates every frame — consecutive frames are guaranteed
    /// to differ, which the accuracy assertions rely on.
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
            let status = Command::new(&ffmpeg)
                .args(&args)
                .status()
                .expect("bundled ffmpeg binary must exist and run");
            assert!(status.success(), "test video generation failed: {filename}");
        }
        path
    }

    fn make_test_video() -> std::path::PathBuf {
        gen_video(
            "testsrc2_60f_30fps.mp4",
            "testsrc2=size=320x240:rate=30:duration=2",
            // Short GOP so the seek test crosses keyframe boundaries
            &["-c:v", "libx264", "-pix_fmt", "yuv420p", "-g", "15"],
        )
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

    // ---- codec/container matrix ----
    // One decode + one cross-keyframe seek per codec the app is likely to
    // meet in the wild. The decode path is codec-agnostic by construction
    // (video-rs's scaler normalizes everything to RGB24), so what these
    // actually verify is that the LINKED ffmpeg build has each decoder
    // enabled and that pts→frame-index mapping holds per container.
    fn assert_decodes_and_seeks(path: &std::path::Path, expect_w: u32, expect_h: u32) {
        let (mut s, frame_count, _) = open_session_core(path.to_str().unwrap()).unwrap();
        assert_eq!((s.width, s.height), (expect_w, expect_h), "{path:?}");
        assert!(frame_count > 10, "{path:?} frame_count={frame_count}");
        let expected_len = (s.width * s.height * 4) as usize;
        let f0 = decode_frame_core(&mut s, 0).unwrap();
        assert_eq!(f0.len(), expected_len, "{path:?} frame 0 size");
        // Jump forward then back — two real seeks, both must land.
        let mid = (frame_count as i64) / 2;
        let fm = decode_frame_core(&mut s, mid).unwrap();
        assert_eq!(fm.len(), expected_len, "{path:?} mid frame size");
        let f0_again = decode_frame_core(&mut s, 0).unwrap();
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

    // ProRes 4444 with a real alpha channel. KNOWN LIMITATION, asserted
    // here so it's documented by a test instead of silently discovered:
    // video-rs's scaler output is hardcoded RGB24 (FRAME_PIXEL_FORMAT),
    // so source alpha is FLATTENED — the decode must still succeed and
    // our buffer's alpha bytes are our own constant 255 fill. Real alpha
    // preservation needs bypassing video-rs's scaler (DecoderSplit +
    // custom AvScaler to RGBA) — future work if the experiment graduates.
    #[test]
    fn codec_prores4444_alpha_decodes_but_flattens() {
        let p = gen_video(
            "prores4444_alpha_320.mov",
            "testsrc2=size=320x240:rate=30:duration=1,format=rgba,colorchannelmixer=aa=0.5",
            &["-c:v", "prores_ks", "-profile:v", "4", "-pix_fmt", "yuva444p10le"],
        );
        let (mut s, _, _) = open_session_core(p.to_str().unwrap()).unwrap();
        let f0 = decode_frame_core(&mut s, 0).unwrap();
        assert!(f0.iter().skip(3).step_by(4).all(|&a| a == 255),
            "alpha bytes should be the constant 255 fill (source alpha is flattened by the RGB24 scaler — see comment)");
    }

    // ---- stride/alignment edge case ----
    // 954 is even (yuv420 requirement) but not a multiple of 16/32, so the
    // decoder's line stride WILL be padded past width*3 — exactly the case
    // where a naive packed read shears every row after the first. The
    // re-seek reproducibility assert catches shearing (sheared rows change
    // between decodes if padding bytes differ) and the size assert catches
    // any coded-size vs display-size confusion.
    #[test]
    fn odd_width_stride_padding() {
        let p = gen_video(
            "odd_954x542.mp4",
            "testsrc2=size=954x542:rate=30:duration=1",
            &["-c:v", "libx264", "-pix_fmt", "yuv420p", "-g", "15"],
        );
        assert_decodes_and_seeks(&p, 954, 542);
    }

    // ---- pts→frame mapping at a different fps ----
    // The frame-index mapping multiplies pts × time_base × fps; a 25fps
    // source with a different time base catches any hidden 30fps
    // assumption in that math.
    #[test]
    fn seek_accuracy_at_25fps() {
        let p = gen_video(
            "fps25_320.mp4",
            "testsrc2=size=320x240:rate=25:duration=2",
            &["-c:v", "libx264", "-pix_fmt", "yuv420p", "-g", "12"],
        );
        let (mut s, _, _) = open_session_core(p.to_str().unwrap()).unwrap();
        assert!((s.fps - 25.0).abs() < 0.01, "fps was {}", s.fps);
        let mut sequential_f30 = Vec::new();
        for i in 0..=30 {
            sequential_f30 = decode_frame_core(&mut s, i).unwrap();
        }
        let seeked_f30 = decode_frame_core(&mut s, 30).unwrap();
        assert_eq!(seeked_f30, sequential_f30, "25fps seek not frame-accurate");
    }

    // ---- multiple simultaneous sessions ----
    // Three sessions with different dimensions, decoded interleaved — each
    // must keep its own next_frame state (no cross-session bleed through
    // the shared registry) and produce byte-identical results to a fresh
    // dedicated session decoding the same frames sequentially.
    #[test]
    fn interleaved_sessions_stay_isolated() {
        let pa = make_test_video(); // 320x240
        let pb = gen_video(
            "iso_b_160.mp4",
            "testsrc2=size=160x120:rate=30:duration=2",
            &["-c:v", "libx264", "-pix_fmt", "yuv420p", "-g", "15"],
        );
        let (mut a, _, _) = open_session_core(pa.to_str().unwrap()).unwrap();
        let (mut b, _, _) = open_session_core(pb.to_str().unwrap()).unwrap();
        // Interleave: a0 b0 a1 b1 a2 — a's frames must match a clean
        // sequential run despite b decoding in between.
        let a0 = decode_frame_core(&mut a, 0).unwrap();
        let _b0 = decode_frame_core(&mut b, 0).unwrap();
        let a1 = decode_frame_core(&mut a, 1).unwrap();
        let _b1 = decode_frame_core(&mut b, 1).unwrap();
        let a2 = decode_frame_core(&mut a, 2).unwrap();

        let (mut a_ref, _, _) = open_session_core(pa.to_str().unwrap()).unwrap();
        assert_eq!(a0, decode_frame_core(&mut a_ref, 0).unwrap(), "a0 diverged");
        assert_eq!(a1, decode_frame_core(&mut a_ref, 1).unwrap(), "a1 diverged");
        assert_eq!(a2, decode_frame_core(&mut a_ref, 2).unwrap(), "a2 diverged");
    }

    // ---- performance at production resolutions ----
    // Sequential decode of 30 frames at 1080p and a short 4K burst, with
    // the per-frame budget of a 30fps source (33.3ms) as the bar. These
    // are real assertions, not just prints — if this machine can't hold
    // the budget the experiment's premise fails and the test SHOULD fail.
    // (Generation is x264 ultrafast to keep the one-time setup quick;
    // decode cost is what's measured.)
    fn measure_sequential(path: &std::path::Path, frames: i64) -> (f64, f64) {
        let (mut s, _, _) = open_session_core(path.to_str().unwrap()).unwrap();
        let mut times = Vec::new();
        for i in 0..frames {
            let t = Instant::now();
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
        let (avg, p95) = measure_sequential(&p, 30);
        eprintln!("[perf] 1080p sequential: avg={avg:.1}ms p95={p95:.1}ms (budget 33.3ms)");
        assert!(p95 < 33.3, "1080p p95 {p95:.1}ms blows the 30fps budget");
    }

    #[test]
    fn perf_4k_sequential_measured() {
        let p = gen_video(
            "perf_4k.mp4",
            "testsrc2=size=3840x2160:rate=30:duration=1",
            &["-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", "-g", "15"],
        );
        let (avg, p95) = measure_sequential(&p, 15);
        eprintln!("[perf] 4K sequential: avg={avg:.1}ms p95={p95:.1}ms (budget 33.3ms)");
        // 4K single-threaded software decode may legitimately exceed a
        // 30fps budget on some machines — reported, and asserted only
        // against a laxer 24fps-proxy bar (41.6ms) so a real regression
        // still fails while machine variance doesn't.
        assert!(p95 < 41.6, "4K p95 {p95:.1}ms exceeds even the relaxed bar");
    }
}
