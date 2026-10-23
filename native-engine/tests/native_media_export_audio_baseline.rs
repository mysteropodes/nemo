//! Baseline-silence tests for the audio export disposition (G50, L09-F).
//!
//! The `V`, `T`, `K` and `A` rows of `fixtures/native-media-export-audio/oracle.txt`
//! (generator `tools/gen.js`, baseline fbe4b8a3) record that the baseline export
//! carries no audio: the ffmpeg argument vector `exportMP4ToPath` really builds
//! (captured by running the shipped function with fake ports), a text scan of
//! every export-path source, and the whole `SMAudio` API. The pinned-literal
//! tests below exercise the argv guard that keeps the native job on that shape.

use super::{verify_silent_argv, SilentArgvError, AUDIO_ARGV_OPTIONS};

const ORACLE: &str = include_str!("fixtures/native-media-export-audio/oracle.txt");

fn lines_of(kind: &str) -> Vec<Vec<&'static str>> {
    ORACLE
        .lines()
        .filter(|l| l.starts_with(kind) && l[kind.len()..].starts_with('\t'))
        .map(|l| l.split('\t').collect())
        .collect()
}

#[test]
fn baseline_ffmpeg_argv_is_silent_and_accepted_by_the_guard() {
    let rows = lines_of("V");
    assert_eq!(rows.len(), 20);
    for r in rows {
        assert_eq!(r.len(), 6, "V columns");
        let argv: Vec<String> = r[3].split('|').map(str::to_string).collect();
        assert_eq!(argv.len(), 14, "the baseline argv has 14 slots");
        assert_eq!(r[4], "1", "one -i input");
        assert_eq!(r[5], "0", "no audio option");
        assert_eq!(argv.iter().filter(|a| a.as_str() == "-i").count(), 1);
        assert_eq!(argv[3], "-i");
        assert_eq!(argv[6], "h264_videotoolbox");
        assert_eq!(verify_silent_argv(&argv), Ok(()), "{}", r[3]);
    }
}

#[test]
fn baseline_export_sources_reference_no_audio() {
    let scans = lines_of("T");
    let paths: Vec<&str> = scans.iter().map(|r| r[1]).collect();
    assert_eq!(
        paths,
        [
            "src/js/export.js",
            "src/js/application/export-job.js",
            "src/js/adapters/export-svg-frame.js",
            "src/js/adapters/export-svg-sequence.js",
            "src/js/adapters/native-opacity-export.js",
            "src/js/application/native-opacity-export-workflow.js",
        ]
    );
    for r in &scans {
        assert_eq!(r[2].len(), 64, "sha256 of the scanned text");
        assert_eq!(r[3], "0", "{} references audio", r[1]);
    }
    // The same scan finds the baseline's audio code: it is not vacuous.
    let control = lines_of("K");
    assert_eq!(control.len(), 1);
    assert_eq!(control[0][1], "src/js/audio-bridge.js");
    assert_eq!(control[0][2], "101");
}

#[test]
fn baseline_audio_api_has_no_export_entry() {
    let api = lines_of("A");
    assert_eq!(api.len(), 1);
    let keys: Vec<&str> = api[0][1].split(',').collect();
    assert_eq!(
        keys,
        [
            "scrubAt",
            "onPlayStart",
            "onPlayStop",
            "onLoop",
            "renderStrip",
            "importFile",
            "addTrackFromDataURL",
            "removeTrackByAudioId",
            "reload",
            "invalidateWaveforms"
        ]
    );
    for key in keys {
        let lower = key.to_lowercase();
        assert!(
            !lower.contains("export") && !lower.contains("offline") && !lower.contains("mix"),
            "{key}"
        );
    }
}

fn baseline_argv() -> Vec<String> {
    [
        "-y",
        "-framerate",
        "24",
        "-i",
        "/tmp/sm-export-1700000000000/frame_%04d.png",
        "-c:v",
        "h264_videotoolbox",
        "-pix_fmt",
        "yuv420p",
        "-q:v",
        "65",
        "-profile:v",
        "high",
        "/tmp/o.mp4",
    ]
    .iter()
    .map(|s| (*s).to_string())
    .collect()
}

#[test]
fn the_guard_accepts_the_literal_baseline_vector_and_pins_its_option_list() {
    assert_eq!(verify_silent_argv(&baseline_argv()), Ok(()));
    assert_eq!(
        AUDIO_ARGV_OPTIONS,
        [
            "-c:a",
            "-acodec",
            "-codec:a",
            "-b:a",
            "-ab",
            "-ar",
            "-ac",
            "-af",
            "-filter:a",
            "-map"
        ]
    );
}

#[test]
fn audio_options_and_extra_inputs_are_rejected_with_their_position() {
    for option in AUDIO_ARGV_OPTIONS {
        let mut argv = baseline_argv();
        argv.insert(5, "x".to_string());
        argv.insert(5, option.to_string());
        assert_eq!(
            verify_silent_argv(&argv),
            Err(SilentArgvError::AudioOption {
                position: 5,
                option: option.to_string()
            })
        );
    }
    let mut argv = baseline_argv();
    argv.insert(5, "voice.wav".to_string());
    argv.insert(5, "-i".to_string());
    assert_eq!(
        verify_silent_argv(&argv),
        Err(SilentArgvError::ExtraInput { position: 5 })
    );
    // The first violation by position wins.
    let mut both = baseline_argv();
    both.insert(1, "aac".to_string());
    both.insert(1, "-c:a".to_string());
    both.push("-i".to_string());
    both.push("-i".to_string());
    assert_eq!(
        verify_silent_argv(&both),
        Err(SilentArgvError::AudioOption {
            position: 1,
            option: "-c:a".to_string()
        })
    );
}

#[test]
fn guard_requires_an_input_and_does_not_scan_the_output_path() {
    let strings = |v: &[&str]| -> Vec<String> { v.iter().map(|s| (*s).to_string()).collect() };
    assert_eq!(verify_silent_argv(&[]), Err(SilentArgvError::NoInput));
    assert_eq!(
        verify_silent_argv(&strings(&["-i"])),
        Err(SilentArgvError::NoInput)
    );
    assert_eq!(
        verify_silent_argv(&strings(&["-y", "/tmp/o.mp4"])),
        Err(SilentArgvError::NoInput)
    );
    // The last argument is the output path, whatever it is named.
    let mut argv = baseline_argv();
    *argv.last_mut().unwrap() = "-ar".to_string();
    assert_eq!(verify_silent_argv(&argv), Ok(()));
    // `-an` only removes audio; it is not an audio stream option.
    let mut stripped = baseline_argv();
    stripped.insert(13, "-an".to_string());
    assert_eq!(verify_silent_argv(&stripped), Ok(()));
}
