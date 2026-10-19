//! Random-access decode of our own indexed optimized-media streams through
//! one persistent JPEG-to-RGBA converter child per session.

use std::process::{Child, Command, Stdio};

use super::process::ffmpeg_path;
use super::session::VideoSession;

/// Persistent JPEG→RGBA converter for INDEXED sessions (see IndexedSeq).
/// One long-lived ffmpeg child per session: JPEG bytes go in via stdin
/// (through a dedicated writer thread — writes must never block the
/// session mutex holder, since a full pipe buffer would deadlock against
/// our own stdout read), raw RGBA frames come out via stdout. Decoding
/// any frame in any order costs one pipe round-trip (~2-6ms measured),
/// with ZERO process respawns — the property that makes random scrubbing
/// on optimized media flat-cost.
pub(super) struct ConvProc {
    pub(super) child: Child,
    /// Sending a JPEG here wakes the writer thread; dropping the sender
    /// (session close) makes that thread exit and close the child's stdin.
    pub(super) jpeg_tx: std::sync::mpsc::Sender<Vec<u8>>,
    /// Decoded RGBA frames arrive here from a dedicated reader thread —
    /// receiving THROUGH A CHANNEL (with recv_timeout) instead of a direct
    /// blocking read is what makes the whole indexed path hang-proof: the
    /// ffmpeg stdin pipeline's internal frame lag is CONTENT-DEPENDENT
    /// (measured: 2 frames at 320x240, 3 at 1080p, with hiccups), so any
    /// protocol that assumes a fixed lag can block forever on a blocking
    /// read — which is exactly what froze the app on import (live-caught
    /// 2026-07, "ça plante à l'import des fichiers": first indexed decode
    /// stuck in read_exact holding the session mutex).
    pub(super) frame_rx: std::sync::mpsc::Receiver<Vec<u8>>,
}

/// Random-access frame index over a raw MJPEG stream file — the format
/// the sibling `optimized` module's create_optimized_media produces (concatenated baseline
/// JPEGs + a sidecar `.idx` JSON with per-frame byte offsets). Built
/// because live profiling showed the ONLY remaining scrub cost on
/// all-intra optimized media was the ~20ms process-spawn tax paid on
/// every non-sequential jump ("fais en sorte que le scrub soit parfait"):
/// with byte offsets known up front, any frame is one file read + one
/// round-trip through the persistent converter, no seeking of any kind.
pub(super) struct IndexedSeq {
    pub(super) file: std::fs::File,
    /// count+1 entries — offsets[i]..offsets[i+1] is frame i's JPEG.
    pub(super) offsets: Vec<u64>,
    pub(super) conv: Option<ConvProc>,
    /// Sentinel pad: a same-dimensions JPEG whose decoded pixels carry a
    /// recognizable signature (see sentinel_pad / is_sentinel_frame),
    /// generated once at session open. The ffmpeg stdin pipeline won't
    /// emit a frame until a content-dependent number of FURTHER frames
    /// enter (measured lag: 2 at 320x240, 3 at 1080p, non-constant — no
    /// flag combination removes it). Each request therefore chases the
    /// target with a few sentinel pads and the reader simply DISCARDS
    /// every sentinel it recognizes, returning the first real frame: no
    /// lag bookkeeping, self-synchronizing, and immune to the lag
    /// changing between ffmpeg builds or resolutions.
    pub(super) pad: Vec<u8>,
}

/// Number of sentinel pads sent after each target JPEG. Must exceed the
/// pipeline's worst observed lag (3); timeouts in indexed_decode self-heal
/// by sending another volley if a build/resolution ever exceeds it.
pub(super) const SENTINEL_PADS_PER_REQUEST: usize = 3;

/// Paints the sentinel signature into row 0 of an RGBA buffer: four
/// 8-pixel blocks of alternating saturated magenta/green. 8-wide blocks
/// straddle JPEG's chroma subsampling (a 1-pixel alternation would smear
/// into gray), and block CENTERS are what is_sentinel_frame samples, so
/// q2 compression noise (±10ish) never gets near the ±80 tolerance.
pub(super) fn paint_sentinel_signature(rgba: &mut [u8], width: usize) {
    for block in 0..4usize {
        let (r, g, b) = if block % 2 == 0 {
            (255u8, 0u8, 255u8)
        } else {
            (0, 255, 0)
        };
        for i in 0..8usize {
            let x = block * 8 + i;
            if x >= width {
                return;
            }
            rgba[x * 4] = r;
            rgba[x * 4 + 1] = g;
            rgba[x * 4 + 2] = b;
            rgba[x * 4 + 3] = 255;
        }
    }
}

/// Does this decoded frame carry the sentinel signature? Samples the 4
/// centers (pixels 3-4 of each 8-pixel block) with a wide ±80 tolerance —
/// real footage reproducing four alternating fully-saturated magenta/green
/// blocks within that tolerance in its top-left 32 pixels is not a
/// realistic false-positive risk for preview media.
pub(super) fn is_sentinel_frame(rgba: &[u8], width: usize) -> bool {
    if width < 32 || rgba.len() < 32 * 4 {
        return false;
    }
    for block in 0..4usize {
        let (er, eg, eb) = if block % 2 == 0 {
            (255i32, 0i32, 255i32)
        } else {
            (0, 255, 0)
        };
        for &x in &[block * 8 + 3, block * 8 + 4] {
            let p = &rgba[x * 4..x * 4 + 3];
            if (p[0] as i32 - er).abs() > 80
                || (p[1] as i32 - eg).abs() > 80
                || (p[2] as i32 - eb).abs() > 80
            {
                return false;
            }
        }
    }
    true
}

/// Encode the sentinel pad JPEG at the given dimensions via a one-shot
/// ffmpeg run (EOF-terminated stdin — the mode with no pipeline-lag
/// surprises, verified reliable in the harness). Black frame + signature
/// row: tiny (~5-15KB at 1080p) and near-instant to decode.
pub(super) fn sentinel_pad(width: u32, height: u32) -> Result<Vec<u8>, String> {
    use std::io::Write;
    let (w, h) = (width as usize, height as usize);
    let mut raw = vec![0u8; w * h * 4];
    for px in raw.chunks_exact_mut(4) {
        px[3] = 255;
    }
    paint_sentinel_signature(&mut raw, w);
    let mut child = Command::new(ffmpeg_path())
        .args([
            "-v",
            "error",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgba",
            "-s",
            &format!("{width}x{height}"),
            "-i",
            "-",
            "-frames:v",
            "1",
            "-c:v",
            "mjpeg",
            "-q:v",
            "2",
            "-f",
            "mjpeg",
            "-",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("sentinel encode spawn: {e}"))?;
    let mut stdin = child.stdin.take().ok_or("sentinel stdin unavailable")?;
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(&raw);
        // dropping stdin closes the pipe — EOF terminates the encode
    });
    let out = child
        .wait_with_output()
        .map_err(|e| format!("sentinel encode: {e}"))?;
    let _ = writer.join();
    if !out.status.success() || out.stdout.is_empty() {
        return Err("sentinel encode produced no output".into());
    }
    Ok(out.stdout)
}

/// If `path` has a sidecar `.idx` (i.e. it's one of OUR optimized-media
/// streams), open it as an indexed random-access session. Returns None for
/// ordinary video files so open_session_core falls through to the normal
/// pipe-decoder path.
pub(super) fn try_open_indexed(path: &str) -> Option<Result<(VideoSession, u64, f64), String>> {
    let idx_path = format!("{path}.idx");
    if !std::path::Path::new(&idx_path).exists() {
        return None;
    }
    Some((|| {
        let text = std::fs::read_to_string(&idx_path).map_err(|e| format!("read idx: {e}"))?;
        let v: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| format!("parse idx: {e}"))?;
        let fps = v["fps"].as_f64().ok_or("idx: missing fps")?;
        let width = v["width"].as_u64().ok_or("idx: missing width")? as u32;
        let height = v["height"].as_u64().ok_or("idx: missing height")? as u32;
        let offsets: Vec<u64> = v["offsets"]
            .as_array()
            .ok_or("idx: missing offsets")?
            .iter()
            .map(|o| o.as_u64().ok_or("idx: bad offset"))
            .collect::<Result<_, _>>()?;
        if offsets.len() < 2 || fps <= 0.0 || width == 0 || height == 0 {
            return Err("idx: invalid contents".into());
        }
        // The sentinel-pad protocol needs 32 pixels of row 0 for its
        // signature (see is_sentinel_frame) — sub-32px-wide video is
        // absurd for real media, but refuse indexed mode honestly rather
        // than misdetect pads; the JS caller falls back to the original
        // file on open failure.
        if width < 32 {
            return Err(format!(
                "indexed mode unsupported below 32px width (got {width})"
            ));
        }
        let count = (offsets.len() - 1) as u64;
        let file = std::fs::File::open(path).map_err(|e| format!("open {path}: {e}"))?;
        // Stale-index guard: the sentinel must equal the current file size.
        let len = file.metadata().map_err(|e| e.to_string())?.len();
        if *offsets.last().unwrap() != len {
            return Err(format!(
                "idx is stale for {path} (sentinel {} != file len {len})",
                offsets.last().unwrap()
            ));
        }
        let duration = count as f64 / fps;
        // Sentinel pipeline padding — see IndexedSeq::pad. Generated once
        // per open (~20-40ms one-shot encode, amortized over the session).
        let pad = sentinel_pad(width, height)?;
        Ok((
            VideoSession {
                path: path.to_string(),
                width,
                height,
                fps,
                frame_bytes: (width as usize) * (height as usize) * 4,
                frame_count: count as i64,
                proc: None,
                cache: std::collections::VecDeque::new(),
                readahead_active: false,
                readahead_goal: 0,
                last_foreground_request: -1,
                indexed: Some(IndexedSeq {
                    file,
                    offsets,
                    conv: None,
                    pad,
                }),
                codec: "mjpeg-indexed".into(),
            },
            count,
            duration,
        ))
    })())
}

/// Spawn the persistent JPEG→RGBA converter child for an indexed session.
/// stdin is fed by a dedicated writer thread, stdout drained by a
/// dedicated reader thread into a channel (see ConvProc for why). Flags
/// chosen empirically against the bundled ffmpeg 8.1 (2026-07 harness):
/// probesize must be tiny or find_stream_info blocks waiting for input it
/// will never get; -analyzeduration needs a NONZERO microsecond value (0
/// means "use the 5-second default", a trap); -fpsprobesize 0 stops it
/// demanding several frames to estimate a frame rate the raw stream
/// doesn't carry; -fflags nobuffer made output STOP entirely (3 in → 0
/// out) so it is deliberately absent.
pub(super) fn conv_spawn(frame_bytes: usize, width: usize) -> Result<ConvProc, String> {
    let mut child = Command::new(ffmpeg_path())
        .args([
            "-v",
            "error",
            "-probesize",
            "32",
            "-fpsprobesize",
            "0",
            "-analyzeduration",
            "1",
            "-f",
            "mjpeg",
            "-i",
            "-",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgba",
            "-an",
            "-",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("converter spawn ({}): {e}", ffmpeg_path().display()))?;
    let mut stdin = child.stdin.take().ok_or("converter stdin unavailable")?;
    let stdout = child.stdout.take().ok_or("converter stdout unavailable")?;
    let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
    std::thread::spawn(move || {
        use std::io::Write;
        for buf in rx {
            if stdin.write_all(&buf).is_err() || stdin.flush().is_err() {
                break;
            }
        }
        // Sender dropped (session closed) or child died: closing stdin here
        // lets the child drain and exit on its own.
    });
    let (ftx, frx) = std::sync::mpsc::channel::<Vec<u8>>();
    std::thread::spawn(move || {
        use std::io::Read as _;
        let mut r = std::io::BufReader::new(stdout);
        // One reusable scratch buffer: sentinels are recognized and
        // dropped RIGHT HERE, costing only the pipe read — no allocation,
        // no channel hop. This mattered in practice: with 4 pads per
        // request forwarded to the channel, each 1080p decode moved ~41MB
        // through alloc+memcpy and averaged 13.6ms live; filtering in the
        // reader gets the per-request traffic back down to the one real
        // frame.
        let mut scratch = vec![0u8; frame_bytes];
        loop {
            if r.read_exact(&mut scratch).is_err() {
                break; // child exited/killed — receiver sees the channel close
            }
            if is_sentinel_frame(&scratch, width) {
                continue;
            }
            if ftx.send(scratch.clone()).is_err() {
                break; // session closed
            }
        }
    });
    Ok(ConvProc {
        child,
        jpeg_tx: tx,
        frame_rx: frx,
    })
}

/// Decode one frame of an indexed session: read its JPEG bytes at the
/// indexed offset, round-trip through the persistent converter. Respawns
/// the converter once if it died (e.g. killed externally) before failing.
pub(super) fn indexed_decode(s: &mut VideoSession, frame_index: i64) -> Result<Vec<u8>, String> {
    use std::io::{Read as _, Seek, SeekFrom};
    let frame_bytes = s.frame_bytes;
    let width = s.width as usize;
    let seq = s.indexed.as_mut().ok_or("not an indexed session")?;
    let i = frame_index as usize;
    if i + 1 >= seq.offsets.len() {
        return Err(format!("frame {frame_index} out of indexed range"));
    }
    let (a, b) = (seq.offsets[i], seq.offsets[i + 1]);
    let mut jpeg = vec![0u8; (b - a) as usize];
    seq.file
        .seek(SeekFrom::Start(a))
        .map_err(|e| e.to_string())?;
    seq.file
        .read_exact(&mut jpeg)
        .map_err(|e| format!("read frame {frame_index}: {e}"))?;

    for attempt in 0..2 {
        if seq.conv.is_none() {
            seq.conv = Some(conv_spawn(frame_bytes, width)?);
        }
        let conv = seq.conv.as_mut().unwrap();
        // [target][pads…] — the sentinel pads flush the target through the
        // pipeline's content-dependent frame lag; the converter's reader
        // thread drops the sentinels at the pipe (see conv_spawn), so the
        // channel only ever yields real frames. recv_timeout (never a
        // bare blocking read) is what guarantees this can't freeze the
        // session mutex (see ConvProc::frame_rx).
        let mut payload =
            Vec::with_capacity(jpeg.len() + seq.pad.len() * SENTINEL_PADS_PER_REQUEST);
        payload.extend_from_slice(&jpeg);
        for _ in 0..SENTINEL_PADS_PER_REQUEST {
            payload.extend_from_slice(&seq.pad);
        }
        if conv.jpeg_tx.send(payload).is_err() {
            seq.conv = None;
            continue;
        }
        let mut extra_volleys = 0u32;
        let failure = loop {
            match conv
                .frame_rx
                .recv_timeout(std::time::Duration::from_millis(700))
            {
                Ok(frame) => {
                    return Ok(frame);
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    if extra_volleys >= 2 {
                        break "no real frame after 2 extra pad volleys";
                    }
                    // Self-heal: the lag exceeded the volley (unseen build/
                    // resolution combination) — push more pads through.
                    extra_volleys += 1;
                    let mut more = Vec::with_capacity(seq.pad.len() * SENTINEL_PADS_PER_REQUEST);
                    for _ in 0..SENTINEL_PADS_PER_REQUEST {
                        more.extend_from_slice(&seq.pad);
                    }
                    if conv.jpeg_tx.send(more).is_err() {
                        break "converter writer died";
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    break "converter exited";
                }
            }
        };
        eprintln!("[video-decode] indexed converter reset (frame {frame_index}, attempt {attempt}): {failure}");
        let _ = conv.child.kill();
        let _ = conv.child.wait();
        seq.conv = None;
        if attempt == 1 {
            return Err(format!(
                "indexed decode failed twice for frame {frame_index}: {failure}"
            ));
        }
    }
    unreachable!()
}
