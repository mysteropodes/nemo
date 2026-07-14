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
use std::sync::{Arc, Mutex, Once};
use std::time::Instant;

use serde::Serialize;
use tauri::ipc::Response;
use video_rs::decode::{Decoder, DecoderSplit};
use video_rs::io::Reader;
use video_rs::Error as VrError;

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
    // Split decoder (DecoderSplit + Reader via Decoder::into_parts) instead
    // of the packaged video_rs::Decoder: its Reader::seek_to_frame passes a
    // FRAME NUMBER to av_seek_frame(ctx, -1, ...) which interprets the
    // value as MICROSECONDS (AV_TIME_BASE units for stream -1) — every
    // "seek to frame N" actually landed near t=0 and walked the whole file
    // forward (measured: ahead=44 for a GOP-15 target at frame 59; scrubs
    // costing 120-980ms). Owning the Reader lets seek_precise below issue
    // the call correctly (stream index + stream-time_base timestamp +
    // AVSEEK_FLAG_BACKWARD), bounding the post-seek walk by one GOP.
    decoder: DecoderSplit,
    reader: Reader,
    stream_index: usize,
    width: u32,
    height: u32,
    fps: f64,
    /// Stream time base in seconds-per-unit (cached from the decoder) —
    /// used both for pts→frame-index mapping and seek timestamp math.
    tb: f64,
    /// Frame index the decoder will produce on the NEXT decode call —
    /// the sequential-playback fast path (frame N+1 right after N) never
    /// seeks. i64 because a fresh post-seek position is derived from pts.
    next_frame: i64,
    /// Total frames (possibly derived from container duration, see
    /// open_session_core) — used to clamp requests, AE-style: scrubbing
    /// to/past the last frame holds the last frame, never errors.
    frame_count: i64,
}

/// The packet pump video_rs::Decoder::decode_raw does internally, rebuilt
/// over the split parts (same read → decode → drain-at-EOF sequence,
/// including the decoder reset when the drain finishes).
fn decode_next_raw(s: &mut VideoSession) -> Result<video_rs::frame::RawFrame, String> {
    loop {
        match s.reader.read(s.stream_index) {
            Ok(packet) => {
                if let Some(frame) = s
                    .decoder
                    .decode_raw(packet)
                    .map_err(|e| format!("decode: {e}"))?
                {
                    return Ok(frame);
                }
            }
            Err(VrError::ReadExhausted) => match s.decoder.drain_raw() {
                Ok(Some(frame)) => return Ok(frame),
                Ok(None) | Err(VrError::ReadExhausted) => {
                    s.decoder.reset();
                    return Err("decode: stream exhausted".to_string());
                }
                Err(e) => return Err(format!("drain: {e}")),
            },
            Err(e) => return Err(format!("read: {e}")),
        }
    }
}

/// Frame-accurate-capable seek: timestamp in the STREAM's own time base,
/// on the stream's own index, with AVSEEK_FLAG_BACKWARD so the demuxer
/// lands on the keyframe AT OR BEFORE the target (never after — landing
/// after would make the forward walk skip the requested frame entirely).
fn seek_precise(s: &mut VideoSession, target_frame: i64) -> Result<(), String> {
    let ts = (target_frame as f64 / s.fps / s.tb).round() as i64;
    let ret = unsafe {
        video_rs::ffmpeg::ffi::av_seek_frame(
            s.reader.input.as_mut_ptr(),
            s.stream_index as i32,
            ts,
            video_rs::ffmpeg::ffi::AVSEEK_FLAG_BACKWARD as i32,
        )
    };
    if ret < 0 {
        return Err(format!("seek failed (av_seek_frame ret {ret})"));
    }
    s.decoder.reset(); // drop buffered pre-seek codec state
    Ok(())
}

/// Per-session Mutex, not one global lock (changed 2026-07 — reached
/// "the scaling point" the original single-Mutex comment predicted: live
/// testing with several native-video LAYERS active at once showed decode
/// requests for DIFFERENT sessions serializing behind each other, e.g.
/// two concurrent seeks to the same frame index on two different videos
/// logged as 99.6ms then 166.5ms back-to-back — the second one's decode
/// time INCLUDES waiting out the first, even though they're independent
/// ffmpeg decoder instances with no data dependency. Tauri commands
/// (these are plain sync fns, not `async fn`) already run on Tauri's own
/// threadpool, so per-session locking is enough to get real parallel
/// decode across CPU cores — no extra runtime needed.
///
/// Structure: an outer Mutex guards only the HashMap's shape (insert on
/// open, remove on close) — held just long enough to clone an Arc out,
/// never across a decode. Each session's actual VideoSession (the
/// ffmpeg decoder + its Reader) is behind its OWN Mutex, so session A
/// decoding holds only A's lock; session B's decode never waits on it.
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

    // Metadata gathered above from the packaged Decoder; now split it to
    // own the Reader (correct seeking — see the VideoSession field docs).
    let time_base = decoder.time_base();
    let tb = time_base.numerator() as f64 / time_base.denominator() as f64;
    let (split, reader, stream_index) = decoder.into_parts();

    Ok((
        VideoSession {
            decoder: split,
            reader,
            stream_index,
            width,
            height,
            fps,
            tb,
            next_frame: 0,
            frame_count: frame_count as i64,
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
    state.0.lock().unwrap().insert(id, Arc::new(Mutex::new(session)));

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
// tightly-packed RGBA8 buffer (`width*height*4`, row-major, no padding)
// plus how many frames the post-seek forward walk consumed (0 on the
// sequential fast path) — asserted against the GOP size in tests since
// bounding that walk is the whole point of seek_precise.
fn decode_frame_core(s: &mut VideoSession, frame_index: i64) -> Result<(Vec<u8>, u32), String> {
    let t0 = Instant::now();
    // Clamp to the stream's real range, AE-style: scrubbing to (or past)
    // the last frame HOLDS the last frame instead of erroring. Also
    // protects the post-seek loop below — frame_count can be derived from
    // container duration (see open_session_core), which may round one
    // frame past what the stream actually contains.
    let frame_index = frame_index.clamp(0, (s.frame_count - 1).max(0));
    let tb = s.tb;

    // Tail robustness (caught live 2026-07, "decode: stream exhausted"
    // aborting scrubs near the end): requesting a frame at/near the tail
    // can hit EOF two different ways — (a) the stream really has fewer
    // frames than metadata claimed, or (b) a re-seek to the LAST frame
    // after the reader already consumed to EOF lands so close to the end
    // that the demuxer returns exhausted before yielding anything. Both
    // are handled by the same widening-backoff retry: re-seek a bit
    // EARLIER than the target and walk forward; whatever frame the walk
    // last produced when the stream ends is the honest "nearest earlier"
    // result (for the true last frame it IS the last frame). Backoff
    // widens 15 → 60 → 240 → …, giving up only once the origin reaches 0.
    let mut seeked = false;
    if frame_index != s.next_frame {
        // Random access: the seek lands at the KEYFRAME at/before the
        // target (inter-frame codecs can't start mid-GOP), then we decode
        // forward until the frame whose pts maps to the requested index —
        // that's what frame-accurate means for real codecs, and it's why
        // backward scrubs cost more than playback.
        seek_precise(s, frame_index)?;
        seeked = true;
    }

    let mut backoff: i64 = 0;
    let mut total_ahead: u32 = 0; // across retries, for the timing log below
    let frame = 'outer: loop {
        let mut decoded_ahead: u32 = 0;
        let mut nearest_earlier: Option<video_rs::frame::RawFrame> = None;
        loop {
            let raw = match decode_next_raw(s) {
                Ok(raw) => raw,
                Err(e) => {
                    if let Some(prev) = nearest_earlier.take() {
                        eprintln!(
                            "[video-decode] stream ended before frame {frame_index} ({e}) — returning nearest earlier frame"
                        );
                        break 'outer prev;
                    }
                    // Nothing decoded on this attempt — retry from an
                    // earlier origin, unless we already started from 0.
                    let prev_origin = (frame_index - backoff).max(0);
                    if prev_origin == 0 {
                        return Err(e);
                    }
                    backoff = if backoff == 0 { 15 } else { backoff * 4 };
                    let origin = (frame_index - backoff).max(0);
                    eprintln!(
                        "[video-decode] tail retry: re-seeking to {origin} for target {frame_index} ({e})"
                    );
                    seek_precise(s, origin)?;
                    seeked = true;
                    continue 'outer;
                }
            };
            if !seeked {
                break 'outer raw; // sequential fast path: next frame IS the target
            }
            // Map the decoded frame's pts back to a frame index. rounding
            // (not floor) because pts*tb*fps for frame N is N ± float noise.
            let idx = match raw.pts() {
                Some(pts) => (pts as f64 * tb * s.fps).round() as i64,
                None => -1, // no pts (rare) — keep decoding, can't identify it
            };
            if idx >= frame_index {
                break 'outer raw;
            }
            decoded_ahead += 1;
            total_ahead += 1;
            if decoded_ahead > MAX_SEEK_DECODE_AHEAD {
                // Pathological GOP — return what we have rather than stall.
                break 'outer raw;
            }
            nearest_earlier = Some(raw);
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
        "[video-decode] frame={frame_index} seeked={seeked} ahead={total_ahead} decode={decode_ms:.1}ms convert={convert_ms:.1}ms"
    );

    Ok((rgba, total_ahead))
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
    // Clone the Arc and drop the map lock immediately — the actual decode
    // below only holds THIS session's own Mutex, so a concurrent
    // decode_video_frame call for a DIFFERENT session_id (or an
    // open/close of a third session) is never blocked by this one.
    let session_arc = {
        let sessions = state.0.lock().unwrap();
        sessions
            .get(&session_id)
            .cloned()
            .ok_or_else(|| format!("no session {session_id}"))?
    };
    let mut s = session_arc.lock().unwrap();
    decode_frame_core(&mut s, frame_index).map(|(rgba, _ahead)| Response::new(rgba))
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
        let f0 = decode_frame_core(&mut s, 0).unwrap().0;
        let f1 = decode_frame_core(&mut s, 1).unwrap().0;
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
            sequential_f40 = decode_frame_core(&mut s, i).unwrap().0;
        }
        // Now jump BACKWARD to frame 40 from position 41 — forces a real
        // keyframe seek (gop=15, so the keyframe is frame 30) + forward
        // decode. Frame accuracy = the seeked result matches the
        // sequential ground truth byte-for-byte.
        let seeked_f40 = decode_frame_core(&mut s, 40).unwrap().0;
        assert_eq!(
            seeked_f40, sequential_f40,
            "seeked frame 40 != sequentially-decoded frame 40 (seek is not frame-accurate)"
        );
    }

    // Past-end and tail-of-stream behavior — AE-style hold, not errors.
    // (Was `decode_past_end_errors_cleanly` asserting Err; changed 2026-07
    // after a live autobench run hit "decode: stream exhausted" from a
    // random seek near the tail — a user scrubbing to the end of the
    // timeline must get the last frame, never an error.)
    #[test]
    fn decode_past_end_clamps_to_last_frame() {
        let path = make_test_video();
        let (mut s, frame_count, _) = open_session_core(path.to_str().unwrap()).unwrap();
        let expected_len = (s.width * s.height * 4) as usize;
        // Way past the end — clamped to the last frame, decodes fine.
        let past = decode_frame_core(&mut s, frame_count as i64 + 100).unwrap().0;
        assert_eq!(past.len(), expected_len);
        // Exactly the last frame, twice (second run exercises the re-seek
        // path since next_frame has moved past it) — reproducible.
        let last_a = decode_frame_core(&mut s, frame_count as i64 - 1).unwrap().0;
        let last_b = decode_frame_core(&mut s, frame_count as i64 - 1).unwrap().0;
        assert_eq!(last_a, last_b, "last-frame seek not reproducible");
        assert_eq!(past, last_a, "past-end result should BE the last frame");
    }

    // Same tail robustness on a container whose frame_count is DERIVED
    // (WebM: duration×fps, see open_session_core) — the derived count can
    // overshoot the stream's real content by a frame, which is exactly
    // the "stream exhausted" trap the nearest-earlier fallback covers.
    #[test]
    fn webm_tail_seek_does_not_exhaust() {
        let p = gen_video(
            "vp9_tail_320.webm",
            "testsrc2=size=320x240:rate=30:duration=2",
            &["-c:v", "libvpx-vp9", "-pix_fmt", "yuv420p", "-g", "15"],
        );
        let (mut s, frame_count, _) = open_session_core(p.to_str().unwrap()).unwrap();
        assert!(frame_count > 0);
        let expected_len = (s.width * s.height * 4) as usize;
        for probe in [frame_count as i64 - 1, frame_count as i64, frame_count as i64 + 10] {
            let (f, _) = decode_frame_core(&mut s, probe)
                .unwrap_or_else(|e| panic!("tail probe {probe} failed: {e}"));
            assert_eq!(f.len(), expected_len, "tail probe {probe} wrong size");
        }
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
        let f0 = decode_frame_core(&mut s, 0).unwrap().0;
        assert_eq!(f0.len(), expected_len, "{path:?} frame 0 size");
        // Jump forward then back — two real seeks, both must land.
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
        let f0 = decode_frame_core(&mut s, 0).unwrap().0;
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

    // ---- seek precision: the post-seek walk must be bounded by one GOP ----
    // Locks in the seek_precise fix (video-rs's own seek_to_frame passed a
    // frame NUMBER where av_seek_frame expects AV_TIME_BASE microseconds,
    // landing every seek near t=0 — measured ahead=44 for a GOP-15 file).
    // With a correct keyframe seek, reaching any target may only require
    // walking forward from the keyframe at/before it: ahead < GOP.
    #[test]
    fn seek_walk_bounded_by_gop() {
        let path = make_test_video(); // gop 15
        let (mut s, frame_count, _) = open_session_core(path.to_str().unwrap()).unwrap();
        for target in [7i64, 22, 44, frame_count as i64 - 2] {
            // force the seek path (avoid the sequential fast path)
            let _ = decode_frame_core(&mut s, 0).unwrap();
            let (_px, ahead) = decode_frame_core(&mut s, target).unwrap();
            assert!(
                ahead < 15,
                "target {target}: walked {ahead} frames after seek (GOP is 15 — seek landed too early)"
            );
        }
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
            sequential_f30 = decode_frame_core(&mut s, i).unwrap().0;
        }
        let seeked_f30 = decode_frame_core(&mut s, 30).unwrap().0;
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
        let a0 = decode_frame_core(&mut a, 0).unwrap().0;
        let _b0 = decode_frame_core(&mut b, 0).unwrap().0;
        let a1 = decode_frame_core(&mut a, 1).unwrap().0;
        let _b1 = decode_frame_core(&mut b, 1).unwrap().0;
        let a2 = decode_frame_core(&mut a, 2).unwrap().0;

        let (mut a_ref, _, _) = open_session_core(pa.to_str().unwrap()).unwrap();
        assert_eq!(a0, decode_frame_core(&mut a_ref, 0).unwrap().0, "a0 diverged");
        assert_eq!(a1, decode_frame_core(&mut a_ref, 1).unwrap().0, "a1 diverged");
        assert_eq!(a2, decode_frame_core(&mut a_ref, 2).unwrap().0, "a2 diverged");
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
            decode_frame_core(&mut s, i).unwrap().0;
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

    // Locks in the per-session Mutex fix (2026-07 — "latence pour plusieurs
    // vidéos", live-observed as two seeks to the same frame index on
    // DIFFERENT sessions logging 99.6ms then 166.5ms back-to-back under
    // the old single global Mutex). Exercises the SAME locking pattern the
    // open_video_session/decode_video_frame commands use (outer map lock
    // held only long enough to clone an Arc, decode under the session's
    // own lock) across two real OS threads, on two independent video
    // files, each doing a chain of forced-seek (expensive) decodes.
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
        let (sa, _, _) = open_session_core(path_a.to_str().unwrap()).unwrap();
        let (sb, _, _) = open_session_core(path_b.to_str().unwrap()).unwrap();
        let (id_a, id_b) = (1u32, 2u32);
        sessions.0.lock().unwrap().insert(id_a, StdArc::new(Mutex::new(sa)));
        sessions.0.lock().unwrap().insert(id_b, StdArc::new(Mutex::new(sb)));

        fn work(sessions: StdArc<VideoSessions>, id: u32) -> u128 {
            let t0 = StdInstant::now();
            // Bounce around to force the expensive seek+forward-decode
            // path repeatedly rather than the cheap sequential fast path
            // — that's the cost worth proving runs in parallel.
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
        // True parallelism: wall time should be well under the sum of both
        // threads' own work (serialized, it would be ≈ the sum). Generous
        // slack (80% of sum) to absorb thread scheduling overhead on tiny
        // test videos without the assertion being flaky.
        assert!(
            wall < (da + db) * 8 / 10,
            "wall={wall}ms not meaningfully less than sum={}ms — sessions may still be serializing",
            da + db
        );
    }
}
