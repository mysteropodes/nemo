// ---- FFmpeg probe text parser (R05/P28) ----
//
// Extracted verbatim from `video_decode.rs` so that turning ffmpeg's
// human-readable stream analysis into numbers is a unit that can be tested
// without a decoder, without a sidecar and without any I/O at all. Every
// function here takes a `&str` and returns values: this module deliberately
// imports nothing from `std::process`, so a test can drive it with a captured
// sample and no FFmpeg binary needs to exist on the machine running it.
//
// Owning the parsing separately also means the ffmpeg-invoking side keeps one
// responsibility (spawn, pipe, lifetime) and this side keeps the other (read a
// documented text format). The call sites in `video_decode.rs` are unchanged
// apart from the path they call through.

/// Parses `ffmpeg -i <path>` stderr (ffmpeg always prints full stream
/// analysis there before erroring "At least one output file must be
/// specified" — a well-established trick for probing without bundling
/// ffprobe as a second sidecar, which would reintroduce a dynamically-
/// linked-against-Homebrew binary and the exact dylib problem this
/// rewrite exists to remove: confirmed via `otool -L` that Homebrew's
/// ffprobe links ~78 /opt/homebrew paths, vs the bundled ffmpeg CLI's 23
/// pure-OS-framework links). Hand-parsed (no `regex` dependency added —
/// the format is small and stable enough, and every codec/container the
/// decode module's own test matrix exercises was captured and used to
/// build this parser against REAL output, not a guessed format) — see
/// this module's `tests` for the captured samples this was built against.
pub(crate) fn parse_probe(stderr: &str) -> Result<(u32, u32, f64, f64, String), String> {
    // Duration: "  Duration: 00:00:02.00, start: 0.000000, bitrate: ..."
    let duration = stderr
        .lines()
        .find_map(|l| l.trim_start().strip_prefix("Duration: "))
        .and_then(|rest| rest.split(',').next())
        .and_then(parse_hms)
        .ok_or("could not find/parse a Duration: line in ffmpeg -i output")?;

    // Video stream: "    Stream #0:0[...](...): Video: h264 (High) (...), yuv420p(...), 320x240 [SAR ...], ..., 30 fps, ..."
    let video_line = stderr
        .lines()
        .find(|l| l.contains("Video:"))
        .ok_or("no video stream found (no 'Video:' line in ffmpeg -i output — is this a video file?)")?;

    let codec = video_line
        .split("Video: ")
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .map(|tok| tok.trim_end_matches(|c: char| c == ',' || c == '(').to_lowercase())
        .ok_or("could not parse codec name from Video: line")?;

    // Resolution: scan whitespace-delimited tokens for the first "NxN"
    // shape (SAR/DAR use "W:H" with a colon, never 'x', so this can't
    // collide with them).
    let (width, height) = video_line
        .split(|c: char| c.is_whitespace() || c == ',')
        .find_map(|tok| {
            let (w, h) = tok.split_once('x')?;
            Some((w.parse::<u32>().ok()?, h.parse::<u32>().ok()?))
        })
        .ok_or("could not find a WxH token on the Video: line")?;

    // FPS: comma-separated segment ending in " fps".
    let fps = video_line
        .split(',')
        .map(|seg| seg.trim())
        .find_map(|seg| seg.strip_suffix(" fps"))
        .and_then(|n| n.trim().parse::<f64>().ok())
        .ok_or("could not find a '<N> fps' segment on the Video: line")?;

    Ok((width, height, fps, duration, codec))
}

fn parse_hms(s: &str) -> Option<f64> {
    let s = s.trim();
    let mut it = s.split(':');
    let h: f64 = it.next()?.parse().ok()?;
    let m: f64 = it.next()?.parse().ok()?;
    let sec: f64 = it.next()?.parse().ok()?;
    Some(h * 3600.0 + m * 60.0 + sec)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- probe parser, tested against REAL captured ffmpeg -i output
    // (2026-07, run against the bundled binary against every file the decode
    // module's codec matrix generates) — not a guessed format. Moved here
    // unchanged with the parser (P28); the samples are the same strings. ----
    #[test]
    fn parse_probe_h264() {
        let stderr = "Input #0, mov,mp4,m4a,3gp,3g2,mj2, from 'x':\n  Duration: 00:00:02.00, start: 0.000000, bitrate: 338 kb/s\n  Stream #0:0[0x1](und): Video: h264 (High) (avc1 / 0x31637661), yuv420p(progressive), 320x240 [SAR 1:1 DAR 4:3], 332 kb/s, 30 fps, 30 tbr, 15360 tbn (default)\n";
        let (w, h, fps, dur, codec) = parse_probe(stderr).unwrap();
        assert_eq!((w, h), (320, 240));
        assert!((fps - 30.0).abs() < 0.001);
        assert!((dur - 2.0).abs() < 0.001);
        assert_eq!(codec, "h264");
    }
    #[test]
    fn parse_probe_hevc() {
        let stderr = "  Duration: 00:00:02.00, start: 0.000000, bitrate: 231 kb/s\n  Stream #0:0[0x1](und): Video: hevc (Main) (hvc1 / 0x31637668), yuv420p(tv, progressive), 320x240 [SAR 1:1 DAR 4:3], 214 kb/s, 30 fps, 30 tbr, 15360 tbn (default)\n";
        let (w, h, fps, _dur, codec) = parse_probe(stderr).unwrap();
        assert_eq!((w, h), (320, 240));
        assert!((fps - 30.0).abs() < 0.001);
        assert_eq!(codec, "hevc");
    }
    #[test]
    fn parse_probe_vp9_webm_no_sar_paren() {
        // WebM's Stream line has no [0x..] tag and no trailing (default) —
        // a structurally different line shape from the mp4 samples.
        let stderr = "  Duration: 00:00:02.00, start: 0.000000, bitrate: 397 kb/s\n  Stream #0:0: Video: vp9 (Profile 0), yuv420p(tv, progressive), 320x240, SAR 1:1 DAR 4:3, 30 fps, 30 tbr, 1k tbn\n";
        let (w, h, fps, _dur, codec) = parse_probe(stderr).unwrap();
        assert_eq!((w, h), (320, 240), "SAR/DAR '1:1'/'4:3' tokens must not be mistaken for the WxH token");
        assert!((fps - 30.0).abs() < 0.001);
        assert_eq!(codec, "vp9");
    }
    #[test]
    fn parse_probe_prores_hq() {
        let stderr = "  Duration: 00:00:02.00, start: 0.000000, bitrate: 8836 kb/s\n  Stream #0:0[0x1]: Video: prores (HQ) (apch / 0x68637061), yuv422p10le(tv, progressive), 320x240, 8832 kb/s, SAR 1:1 DAR 4:3, 30 fps, 30 tbr, 15360 tbn (default)\n";
        let (w, h, fps, _dur, codec) = parse_probe(stderr).unwrap();
        assert_eq!((w, h), (320, 240));
        assert!((fps - 30.0).abs() < 0.001);
        assert_eq!(codec, "prores");
    }
    #[test]
    fn parse_probe_odd_dims_and_fractional_duration() {
        let stderr = "  Duration: 00:00:01.00, start: 0.000000, bitrate: 1728 kb/s\n  Stream #0:0[0x1](und): Video: h264 (High) (avc1 / 0x31637661), yuv420p(progressive), 954x542 [SAR 1:1 DAR 477:271], 1719 kb/s, 30 fps, 30 tbr, 15360 tbn (default)\n";
        let (w, h, _fps, dur, _codec) = parse_probe(stderr).unwrap();
        assert_eq!((w, h), (954, 542), "DAR 477:271 must not be mistaken for the WxH token");
        assert!((dur - 1.0).abs() < 0.001);
    }
    #[test]
    fn parse_probe_25fps() {
        let stderr = "  Duration: 00:00:02.00, start: 0.000000, bitrate: 342 kb/s\n  Stream #0:0[0x1](und): Video: h264 (High) (avc1 / 0x31637661), yuv420p(progressive), 320x240 [SAR 1:1 DAR 4:3], 336 kb/s, 25 fps, 25 tbr, 12800 tbn (default)\n";
        let (_w, _h, fps, _dur, _codec) = parse_probe(stderr).unwrap();
        assert!((fps - 25.0).abs() < 0.001, "fps was {fps}");
    }
    #[test]
    fn parse_probe_rejects_audio_only_stderr() {
        let stderr = "  Duration: 00:00:02.00, start: 0.000000, bitrate: 128 kb/s\n  Stream #0:0: Audio: mp3, 44100 Hz, stereo, fltp, 128 kb/s\n";
        assert!(parse_probe(stderr).is_err(), "an audio-only file must not parse as having a video stream");
    }

    // ---- characterization of the failure paths (P28) ----
    //
    // These pin behavior that EXISTS today but had no test: each of the five
    // `.ok_or(...)` arms, plus `parse_hms`. They assert the exact current
    // error string, so the extraction — or any later edit — cannot quietly
    // reword a diagnostic the JS side may be matching on. They document the
    // parser as it is, including where it is lenient; they are not a
    // statement that the current wording is the wording it should have.

    #[test]
    fn parse_probe_reports_a_missing_duration_line() {
        let stderr = "  Stream #0:0[0x1](und): Video: h264 (High), yuv420p, 320x240, 30 fps, 30 tbr\n";
        assert_eq!(
            parse_probe(stderr).unwrap_err(),
            "could not find/parse a Duration: line in ffmpeg -i output"
        );
    }

    #[test]
    fn parse_probe_reports_a_malformed_duration_line() {
        // "N/A" is what ffmpeg prints for a stream whose duration it cannot
        // determine; parse_hms rejects it and it surfaces as the same
        // missing-Duration diagnostic rather than a distinct one.
        let stderr = "  Duration: N/A, start: 0.000000, bitrate: N/A\n  Stream #0:0: Video: h264, yuv420p, 320x240, 30 fps\n";
        assert_eq!(
            parse_probe(stderr).unwrap_err(),
            "could not find/parse a Duration: line in ffmpeg -i output"
        );
    }

    #[test]
    fn parse_probe_reports_an_absent_video_stream() {
        let stderr = "  Duration: 00:00:02.00, start: 0.000000, bitrate: 128 kb/s\n  Stream #0:0: Audio: mp3, 44100 Hz, stereo, fltp, 128 kb/s\n";
        assert_eq!(
            parse_probe(stderr).unwrap_err(),
            "no video stream found (no 'Video:' line in ffmpeg -i output — is this a video file?)"
        );
    }

    #[test]
    fn parse_probe_reports_unparseable_dimensions() {
        // A Video: line with no WxH token at all — the codec parses, the
        // resolution scan finds nothing, and that is the arm that reports.
        let stderr = "  Duration: 00:00:02.00, start: 0.000000, bitrate: 338 kb/s\n  Stream #0:0[0x1](und): Video: h264 (High) (avc1 / 0x31637661), yuv420p(progressive), SAR 1:1 DAR 4:3, 30 fps, 30 tbr\n";
        assert_eq!(
            parse_probe(stderr).unwrap_err(),
            "could not find a WxH token on the Video: line"
        );
    }

    #[test]
    fn parse_probe_reports_an_unparseable_fps_segment() {
        let stderr = "  Duration: 00:00:02.00, start: 0.000000, bitrate: 338 kb/s\n  Stream #0:0[0x1](und): Video: h264 (High) (avc1 / 0x31637661), yuv420p(progressive), 320x240 [SAR 1:1 DAR 4:3], 332 kb/s, 30 tbr, 15360 tbn (default)\n";
        assert_eq!(
            parse_probe(stderr).unwrap_err(),
            "could not find a '<N> fps' segment on the Video: line"
        );
    }

    #[test]
    fn parse_probe_dimension_scan_is_lenient_about_a_non_numeric_x_token() {
        // Characterizes a real property of the scan: it does not stop at the
        // first token containing 'x', it keeps looking. "yuv420p(progressive)"
        // has no 'x'; a pixel format that did would be skipped the same way.
        let stderr = "  Duration: 00:00:02.00, start: 0.000000, bitrate: 338 kb/s\n  Stream #0:0[0x1](und): Video: h264 (High), xxx, 320x240 [SAR 1:1], 30 fps\n";
        let (w, h, _fps, _dur, _codec) = parse_probe(stderr).unwrap();
        assert_eq!((w, h), (320, 240));
    }

    #[test]
    fn parse_hms_accepts_hours_minutes_seconds_and_rejects_short_forms() {
        assert_eq!(parse_hms("00:00:02.00"), Some(2.0));
        assert_eq!(parse_hms("01:02:03"), Some(3723.0));
        assert_eq!(parse_hms("  00:00:01.50  "), Some(1.5), "surrounding whitespace is trimmed");
        assert_eq!(parse_hms("02.00"), None, "a bare seconds value has no h:m:s shape");
        assert_eq!(parse_hms("00:02.00"), None, "m:s is not accepted either");
        assert_eq!(parse_hms("N/A"), None);
    }
}
