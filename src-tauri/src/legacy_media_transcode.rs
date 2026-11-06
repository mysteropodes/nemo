//! One-shot media conversions for a baseline project opened as a new native document. The engine reads PNG images and WAV
//! audio only; a baseline project embeds whatever the artist imported (JPEG, WebP, GIF, MP3, M4A, OGG). The bundled ffmpeg
//! sidecar converts each payload once, from a temporary file in the user's temp directory, and the result is handed back to
//! the engine's strict admission. A missing sidecar, a failed conversion or a timeout yields `None`, which the importer
//! turns into a typed refusal naming the media; nothing here touches the document or the legacy file.
use native_engine::history::legacy_project_import::MediaTranscoder;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const LIMIT: Duration = Duration::from_secs(120);
const MAX_OUTPUT: u64 = 64 * 1024 * 1024;

pub(crate) struct FfmpegTranscoder;

fn scratch(extension: &str) -> PathBuf {
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "nemo-legacy-import-{}-{n}.{extension}",
        std::process::id()
    ))
}

/// The bytes of `path`, or `None` when it is empty or larger than `cap`. A file at the cap is read one byte past it, so a
/// longer one is refused as a failed conversion: silently cutting it off would leave a valid-looking header over missing
/// samples (a WAV whose RIFF length no longer matches the file), which the engine then rejects without naming the media.
fn read_capped(path: &std::path::Path, cap: u64) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take(cap.checked_add(1)?)
        .read_to_end(&mut out)
        .ok()?;
    (!out.is_empty() && out.len() as u64 <= cap).then_some(out)
}

/// Run ffmpeg over `input` (written to a temp file, never a shell string) and return the bytes of the output file.
fn convert(input: &[u8], in_ext: &str, out_ext: &str, args: &[&str]) -> Option<Vec<u8>> {
    let (source, target) = (scratch(in_ext), scratch(out_ext));
    let result = (|| {
        std::fs::write(&source, input).ok()?;
        let mut child = Command::new(crate::video_decode::sidecar_path())
            .args(["-hide_banner", "-loglevel", "error", "-y", "-i"])
            .arg(&source)
            .args(args)
            .arg(&target)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let started = Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait().ok()? {
                break status;
            }
            if started.elapsed() > LIMIT {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        if !status.success() {
            return None;
        }
        read_capped(&target, MAX_OUTPUT)
    })();
    let _ = std::fs::remove_file(&source);
    let _ = std::fs::remove_file(&target);
    result
}

fn extension(mime: &str) -> &str {
    match mime {
        "image/jpeg" | "image/jpg" => "jpg",
        "image/webp" => "webp",
        "image/gif" => "gif",
        "image/bmp" => "bmp",
        "audio/mpeg" | "audio/mp3" => "mp3",
        "audio/mp4" | "audio/x-m4a" | "audio/aac" => "m4a",
        "audio/ogg" => "ogg",
        "audio/flac" => "flac",
        _ => "bin",
    }
}

impl MediaTranscoder for FfmpegTranscoder {
    fn to_png(&self, mime: &str, bytes: &[u8]) -> Option<Vec<u8>> {
        convert(
            bytes,
            extension(mime),
            "png",
            &["-frames:v", "1", "-c:v", "png"],
        )
    }
    fn to_wav(&self, mime: &str, bytes: &[u8]) -> Option<Vec<u8>> {
        convert(
            bytes,
            extension(mime),
            "wav",
            &["-vn", "-acodec", "pcm_s16le"],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unreadable_media_converts_to_nothing_and_leaves_no_scratch_file() {
        let before = std::fs::read_dir(std::env::temp_dir())
            .map(|d| d.count())
            .unwrap_or(0);
        assert_eq!(FfmpegTranscoder.to_png("image/jpeg", b"not an image"), None);
        assert_eq!(FfmpegTranscoder.to_wav("audio/mpeg", b"not audio"), None);
        let after = std::fs::read_dir(std::env::temp_dir())
            .map(|d| d.count())
            .unwrap_or(0);
        assert!(after <= before + 8, "scratch files are removed");
        assert_eq!(extension("audio/x-m4a"), "m4a");
        assert_eq!(extension("image/jpeg"), "jpg");
    }

    #[test]
    fn output_larger_than_the_cap_is_a_failed_conversion_not_a_truncated_file() {
        let path = scratch("bin");
        std::fs::write(&path, [7u8; 10]).unwrap();
        assert_eq!(read_capped(&path, 10), Some(vec![7u8; 10]), "exactly at the cap is kept whole");
        assert_eq!(read_capped(&path, 9), None, "one byte over the cap is refused");
        assert_eq!(read_capped(&path, 1), None);
        std::fs::write(&path, []).unwrap();
        assert_eq!(read_capped(&path, 10), None, "an empty output is no conversion");
        let _ = std::fs::remove_file(&path);
        assert_eq!(read_capped(&path, 10), None, "a missing output is no conversion");
    }

    fn generated(args: &[&str], ext: &str) -> Option<Vec<u8>> {
        let target = scratch(ext);
        let ok = Command::new(crate::video_decode::sidecar_path())
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
            ])
            .args(args)
            .arg(&target)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        let bytes = ok.then(|| std::fs::read(&target).ok()).flatten();
        let _ = std::fs::remove_file(&target);
        bytes
    }

    #[test]
    fn the_bundled_sidecar_turns_jpeg_into_png_and_flac_into_wav() {
        let Some(jpeg) = generated(&["color=c=red:s=16x8", "-frames:v", "1"], "jpg") else {
            eprintln!("skipped: no ffmpeg sidecar for this test run");
            return;
        };
        let png = FfmpegTranscoder.to_png("image/jpeg", &jpeg).expect("png");
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(
            (
                u32::from_be_bytes(png[16..20].try_into().unwrap()),
                u32::from_be_bytes(png[20..24].try_into().unwrap())
            ),
            (16, 8)
        );
        if let Some(flac) = generated(&["sine=frequency=440:duration=0.2"], "flac") {
            let wav = FfmpegTranscoder.to_wav("audio/flac", &flac).expect("wav");
            assert_eq!((&wav[..4], &wav[8..12]), (&b"RIFF"[..], &b"WAVE"[..]));
            // The whole path: a baseline project with a JPEG raster and a FLAC track passes the strict native admission.
            use base64::Engine;
            let std = base64::engine::general_purpose::STANDARD;
            let project = serde_json::json!({"version": 13, "totalFrames": 2, "layers": [{"layerUid": "l", "frames": [
                {"isKeyframe": true, "strokes": [{"isRaster": true, "x": 8, "y": 4, "width": 16, "height": 8,
                    "src": format!("data:image/jpeg;base64,{}", std.encode(&jpeg))}]}, {"strokes": []}]}],
                "audioTracks": [{"name": "tone", "audioId": "au", "dataB64": format!("data:audio/flac;base64,{}", std.encode(&flac))}]});
            let converted = native_engine::history::legacy_project_import::convert_admitted_with(
                project.to_string().as_bytes(),
                &FfmpegTranscoder,
            )
            .unwrap_or_else(|e| panic!("{}", e.message()));
            let file: serde_json::Value = serde_json::from_slice(&converted.project_file).unwrap();
            assert_eq!(file["assets"].as_array().unwrap().len(), 2);
        }
    }

    #[test]
    fn audio_longer_than_the_cap_refuses_naming_the_track_instead_of_failing_the_whole_import() {
        // 400 s of 44.1 kHz stereo 16-bit PCM is about 70 MB, over the 64 MiB cap; FLAC keeps the payload tiny.
        let Some(flac) = generated(&["sine=frequency=440:duration=400:sample_rate=44100", "-ac", "2"], "flac") else {
            eprintln!("skipped: no ffmpeg sidecar for this test run");
            return;
        };
        assert_eq!(FfmpegTranscoder.to_wav("audio/flac", &flac), None, "a WAV over the cap is not handed back truncated");
        use base64::Engine;
        let std = base64::engine::general_purpose::STANDARD;
        let project = serde_json::json!({"version": 13, "totalFrames": 2, "layers": [{"layerUid": "l", "frames": [{"isKeyframe": true, "strokes": []}]}],
            "audioTracks": [{"name": "long", "audioId": "au", "dataB64": format!("data:audio/flac;base64,{}", std.encode(&flac))}]});
        let error = native_engine::history::legacy_project_import::convert_admitted_with(project.to_string().as_bytes(), &FfmpegTranscoder)
            .err()
            .expect("refused");
        assert_eq!(error.path(), "audioTracks[0].dataB64", "the refusal names the audio track: {}", error.message());
    }
}
