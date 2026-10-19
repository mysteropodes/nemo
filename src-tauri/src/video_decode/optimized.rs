//! The optimized-media format: the MJPEG stream indexer and the transcode
//! that writes a stream plus its `.idx` sidecar.

use std::process::Command;

use crate::media_probe::parse_probe;

use super::process::ffmpeg_path;

// ---- indexed optimized media (random-access MJPEG stream + sidecar index) ----

/// Walks a raw MJPEG stream (concatenated baseline JPEGs, the exact output
/// of `ffmpeg -f mjpeg`) and returns the byte offset of every frame's SOI
/// marker, plus a final sentinel equal to the file length. This is a REAL
/// JPEG segment parser, not a naive FFD8 byte scan: marker-segment payloads
/// (DQT/DHT tables) may legally contain any byte pair, so boundaries are
/// only trusted after structurally walking each image — length-prefixed
/// segments hopped by their declared size, then the entropy-coded scan
/// after SOS, where 0xFF is always followed by 0x00 (stuffing) or
/// 0xD0-0xD7 (restart markers) until a real marker ends the scan.
pub(super) fn index_mjpeg_stream(path: &str) -> Result<Vec<u64>, String> {
    use std::io::BufRead;
    let f = std::fs::File::open(path).map_err(|e| format!("open {path}: {e}"))?;
    let len = f.metadata().map_err(|e| e.to_string())?.len();
    let mut r = std::io::BufReader::with_capacity(1 << 20, f);
    let mut pos: u64 = 0;
    let mut offsets: Vec<u64> = Vec::new();

    // Byte-at-a-time reads through the 1MB BufReader: fill_buf/consume by
    // hand so `pos` tracking stays trivial and exact.
    fn next_byte(r: &mut std::io::BufReader<std::fs::File>, pos: &mut u64) -> Option<u8> {
        let buf = r.fill_buf().ok()?;
        if buf.is_empty() {
            return None;
        }
        let b = buf[0];
        r.consume(1);
        *pos += 1;
        Some(b)
    }
    fn skip(r: &mut std::io::BufReader<std::fs::File>, pos: &mut u64, mut n: u64) -> bool {
        while n > 0 {
            let buf = match r.fill_buf() {
                Ok(b) if !b.is_empty() => b,
                _ => return false,
            };
            let take = (buf.len() as u64).min(n) as usize;
            r.consume(take);
            *pos += take as u64;
            n -= take as u64;
        }
        true
    }

    'images: loop {
        let start = pos;
        // Expect SOI (or clean EOF between images).
        let Some(b0) = next_byte(&mut r, &mut pos) else { break };
        let Some(b1) = next_byte(&mut r, &mut pos) else { break };
        if b0 != 0xFF || b1 != 0xD8 {
            return Err(format!("not an MJPEG stream: expected SOI at byte {start}, found {b0:02x}{b1:02x}"));
        }
        offsets.push(start);
        // Walk marker segments.
        loop {
            // Marker prefix: one or more 0xFF fill bytes then the marker id.
            let m;
            loop {
                let Some(b) = next_byte(&mut r, &mut pos) else { return Err("truncated JPEG (EOF in marker)".into()) };
                if b == 0xFF {
                    loop {
                        let Some(b2) = next_byte(&mut r, &mut pos) else { return Err("truncated JPEG (EOF after 0xFF)".into()) };
                        if b2 != 0xFF {
                            m = b2;
                            break;
                        }
                    }
                    break;
                }
                // Non-FF between segments would be corruption; be strict.
                return Err(format!("corrupt JPEG segment structure at byte {}", pos - 1));
            }
            match m {
                0xD9 => continue 'images,          // EOI with no scan (degenerate but legal)
                0x01 | 0xD0..=0xD7 => continue,    // standalone markers, no payload
                0xDA => {
                    // SOS: skip its header, then scan entropy-coded data.
                    let Some(l0) = next_byte(&mut r, &mut pos) else { return Err("truncated SOS".into()) };
                    let Some(l1) = next_byte(&mut r, &mut pos) else { return Err("truncated SOS".into()) };
                    let seg_len = u16::from_be_bytes([l0, l1]) as u64;
                    if seg_len < 2 || !skip(&mut r, &mut pos, seg_len - 2) {
                        return Err("truncated SOS header".into());
                    }
                    loop {
                        let Some(b) = next_byte(&mut r, &mut pos) else { return Err("truncated JPEG (EOF in scan)".into()) };
                        if b != 0xFF {
                            continue;
                        }
                        let Some(b2) = next_byte(&mut r, &mut pos) else { return Err("truncated JPEG (EOF in scan marker)".into()) };
                        match b2 {
                            0x00 | 0xD0..=0xD7 => continue, // stuffing / restart
                            0xD9 => continue 'images,       // EOI — frame complete
                            _ => {
                                // Another marker inside the scan area means a
                                // multi-scan (progressive) image; ffmpeg's
                                // mjpeg encoder emits baseline only, but
                                // handle it by returning to segment walking.
                                // Push back is impossible with BufReader, so
                                // handle the marker inline: length-prefixed
                                // unless standalone.
                                if b2 == 0x01 || (0xD0..=0xD7).contains(&b2) {
                                    continue;
                                }
                                let Some(l0) = next_byte(&mut r, &mut pos) else { return Err("truncated segment".into()) };
                                let Some(l1) = next_byte(&mut r, &mut pos) else { return Err("truncated segment".into()) };
                                let seg_len = u16::from_be_bytes([l0, l1]) as u64;
                                if seg_len < 2 || !skip(&mut r, &mut pos, seg_len - 2) {
                                    return Err("truncated segment payload".into());
                                }
                            }
                        }
                    }
                }
                _ => {
                    // Length-prefixed segment: APPn, DQT, DHT, SOF0, COM...
                    let Some(l0) = next_byte(&mut r, &mut pos) else { return Err("truncated segment".into()) };
                    let Some(l1) = next_byte(&mut r, &mut pos) else { return Err("truncated segment".into()) };
                    let seg_len = u16::from_be_bytes([l0, l1]) as u64;
                    if seg_len < 2 || !skip(&mut r, &mut pos, seg_len - 2) {
                        return Err("truncated segment payload".into());
                    }
                }
            }
        }
    }
    if offsets.is_empty() {
        return Err("no JPEG frames found in stream".into());
    }
    offsets.push(len); // sentinel: frame i = offsets[i]..offsets[i+1]
    Ok(offsets)
}

/// Transcode `src` to an indexed optimized-media pair: `target` (raw MJPEG
/// stream, same `-q:v 3` near-visually-lossless setting the previous .mov
/// flavor used) and `target + ".idx"` (JSON: fps/width/height/count +
/// per-frame byte offsets). Tauri-free core, wrapped by the command below.
pub(super) fn create_optimized_media_core(src: &str, target: &str) -> Result<(), String> {
    // Probe the SOURCE for fps (a raw MJPEG stream carries no timing
    // metadata of its own — the sidecar is the only place it survives).
    let out = Command::new(ffmpeg_path())
        .args(["-hide_banner", "-i", src])
        .output()
        .map_err(|e| format!("probe spawn: {e}"))?;
    let stderr = String::from_utf8_lossy(&out.stderr);
    // Width/height come from the source too: the transcode never rescales,
    // and the raw MJPEG result can't be re-probed anyway (no Duration line
    // without a container — the whole reason the sidecar exists).
    let (width, height, fps, _, _) = parse_probe(&stderr).map_err(|e| format!("probe {src}: {e}"))?;

    let status = Command::new(ffmpeg_path())
        .args(["-y", "-v", "error", "-nostdin", "-i", src, "-c:v", "mjpeg", "-q:v", "3", "-an", "-f", "mjpeg", target])
        .status()
        .map_err(|e| format!("transcode spawn: {e}"))?;
    if !status.success() {
        let _ = std::fs::remove_file(target);
        return Err(format!("transcode failed (exit {status:?})"));
    }

    let offsets = index_mjpeg_stream(target).inspect_err(|_| {
        let _ = std::fs::remove_file(target);
    })?;
    let count = offsets.len() - 1;

    let idx = serde_json::json!({
        "fps": fps,
        "width": width,
        "height": height,
        "count": count,
        "offsets": offsets,
    });
    std::fs::write(format!("{target}.idx"), serde_json::to_vec(&idx).map_err(|e| e.to_string())?)
        .map_err(|e| format!("write idx: {e}"))?;
    Ok(())
}
