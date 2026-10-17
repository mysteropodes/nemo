//! Oracle tests for the audio export disposition (G50, L09-F).
//!
//! `fixtures/native-media-export-audio/oracle.txt` was produced by running the
//! SHIPPED `ensureCtx`, `tracks`, `currentVolume`, `_audioContextSuspended`,
//! `trimOf`, `startTrack`, `stopTrack`, `stopAll` and `restartAt`, cut
//! verbatim from `src/js/audio-bridge.js` at fbe4b8a3, in a Node `vm` against a
//! recording fake `AudioContext` whose `currentTime` is the export origin `0`
//! (generator `tools/gen.js`). `restartAt(frame)` is the baseline's only "start
//! every track from this frame" entry, so an `M` row is the audio of an export
//! that begins at `frame`. The injected volume sampler is one of six pure
//! profiles, mirrored by [`Profile`]. Values are IEEE 754 bit patterns in hex;
//! `-` is JS `undefined`, `u` an unset `trimEnd`. Rows (TAB separated):
//! `M fps susp ctxok frame tracks results` (one result per track, `N:<reason>`,
//! `R:<refusal>` or `Y:<when>:<offset>:<dur>:<gain>`), `V` (the baseline MP4
//! ffmpeg argv), `T`/`K` (audio-reference scans of the baseline export sources)
//! and `A` (the baseline `SMAudio` API). N/R labels are the generator's
//! classification, asserted there against the executed baseline.
//!
//! Each manifest entry is rendered in the oracle grammar plus its span end and
//! compared as text, so every f64 and f32 is compared on its bit pattern.

use super::{
    plan_audio_export, AudioDisposition, AudioExportMode, AudioManifest, AudioTrackInput,
    TrackAudio,
};
use crate::object_document::media_timing::object_media_timing_audio::{
    flat_volume, AudioTrackTiming, PlaybackEnv, TrackGain, TrackVolume, VolumeSampler,
};

const ORACLE: &str = include_str!("fixtures/native-media-export-audio/oracle.txt");

pub(crate) fn hex(h: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(h, 16).expect("hex bits"))
}

pub(crate) fn h(x: f64) -> String {
    format!("{:016x}", x.to_bits())
}

/// The generator's `SMMotion.valueAtFrame` stub: six pure profiles of the frame.
pub(crate) struct Profile(pub u8);

impl VolumeSampler for Profile {
    fn volume_at(&self, f: f64) -> Option<f64> {
        Some(match self.0 {
            0 => f,
            1 => f * 0.02 - 0.5,
            2 => (f % 7.0) / 7.0 - 0.2,
            3 => 0.1,
            4 if f >= 12.0 => f64::NAN,
            4 => 0.5,
            5 if f >= 12.0 => f64::INFINITY,
            5 => 0.25,
            other => panic!("unknown profile {other}"),
        })
    }
}

/// A sampler that must not be asked: flat tracks never consult the port.
struct Untouchable;

impl VolumeSampler for Untouchable {
    fn volume_at(&self, frame: f64) -> Option<f64> {
        panic!("a flat track asked the volume sampler for frame {frame}")
    }
}

/// FNV-1a over the little-endian bytes of each `f32`, as the generator hashes.
pub(crate) fn fnv(values: &[f32]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for v in values {
        for b in v.to_bits().to_le_bytes() {
            hash ^= u64::from(b);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
    }
    hash
}

fn opt(s: &str) -> Option<f64> {
    if s == "-" {
        None
    } else {
        Some(hex(s))
    }
}

/// One `M` row: a scene, the frame the export starts at and the baseline answers.
pub(crate) struct Row {
    pub(crate) env: PlaybackEnv,
    pub(crate) frame: u32,
    /// Per track: its timing and the sample profile when the volume is animated.
    pub(crate) tracks: Vec<(AudioTrackTiming, Option<u8>)>,
    pub(crate) expect: Vec<String>,
}

fn parse_track(spec: &str) -> (AudioTrackTiming, Option<u8>) {
    let t: Vec<&str> = spec.split(',').collect();
    assert_eq!(t.len(), 8, "track columns in {spec}");
    let anim: Option<u8> = if t[5] == "-" {
        None
    } else {
        Some(t[5].parse().expect("profile digit"))
    };
    let volume = match anim {
        Some(_) => TrackVolume::Animated,
        None => TrackVolume::Flat(flat_volume(opt(t[6]), opt(t[7]))),
    };
    let timing = AudioTrackTiming {
        muted: t[0] == "1",
        buffer_duration: opt(t[1]),
        offset_frames: hex(t[2]),
        trim_start: hex(t[3]),
        trim_end: if t[4] == "u" { None } else { Some(hex(t[4])) },
        volume,
    };
    (timing, anim)
}

fn parse_row(line: &str) -> Row {
    let c: Vec<&str> = line.split('\t').collect();
    assert_eq!(c.len(), 7, "M columns in {line}");
    let frame = hex(c[4]);
    assert!(
        frame >= 0.0 && frame.fract() == 0.0 && frame <= f64::from(u32::MAX),
        "an export starts at a u32 frame"
    );
    Row {
        env: PlaybackEnv {
            fps: hex(c[1]),
            context_isolated: c[2] == "1",
            audio_context: c[3] == "1",
        },
        frame: frame as u32,
        tracks: c[5].split(';').map(parse_track).collect(),
        expect: c[6].split(';').map(str::to_string).collect(),
    }
}

pub(crate) fn rows() -> Vec<Row> {
    ORACLE
        .lines()
        .filter(|l| l.starts_with("M\t"))
        .map(parse_row)
        .collect()
}

/// Plans the export of a row through the production entry point.
pub(crate) fn plan(row: &Row) -> AudioManifest {
    let mut samplers: Vec<Box<dyn VolumeSampler>> = Vec::new();
    for (_, anim) in &row.tracks {
        match anim {
            Some(p) => samplers.push(Box::new(Profile(*p))),
            None => samplers.push(Box::new(Untouchable)),
        }
    }
    let mut inputs: Vec<AudioTrackInput<'_>> = Vec::new();
    for (i, (timing, _)) in row.tracks.iter().enumerate() {
        inputs.push(AudioTrackInput {
            id: format!("t{i}"),
            timing: *timing,
            sampler: samplers[i].as_ref(),
        });
    }
    plan_audio_export(
        AudioExportMode::SilentBaseline,
        &row.env,
        row.frame,
        48,
        &inputs,
    )
    .expect("a valid export range and distinct ids")
}

/// One manifest entry in the oracle grammar, prefixed by its id and followed by
/// `@<end>` for a span.
pub(crate) fn render(entry: &TrackAudio) -> String {
    let (body, end) = match entry {
        TrackAudio::Excluded { reason, .. } => (format!("N:{reason:?}"), String::new()),
        TrackAudio::Unresolved { refusal, .. } => (format!("R:{refusal:?}"), String::new()),
        TrackAudio::Audible(span) => {
            let s = &span.start;
            let gain = match &s.gain {
                TrackGain::Value(v) => format!("v:{}", h(*v)),
                TrackGain::Curve(c) => format!(
                    "c:{}:{}:{}:{:016x}:{:08x}:{:08x}",
                    h(span.start_s),
                    h(c.duration_s),
                    c.values.len(),
                    fnv(&c.values),
                    c.values[0].to_bits(),
                    c.values[c.values.len() - 1].to_bits()
                ),
            };
            (
                format!(
                    "Y:{}:{}:{}:{gain}",
                    h(span.start_s),
                    h(s.offset_s),
                    h(s.duration_s)
                ),
                format!("@{}", h(span.end_s)),
            )
        }
    };
    format!("{}={body}{end}", entry.track_id())
}

/// The expected rendering of oracle result `token` of track `i`: a span ends at
/// `when + dur`, computed here from the oracle's own literals.
pub(crate) fn expected(i: usize, token: &str) -> String {
    let mut text = format!("t{i}={token}");
    if token.starts_with("Y:") {
        let p: Vec<&str> = token.split(':').collect();
        text.push_str(&format!("@{}", h(hex(p[1]) + hex(p[3]))));
    }
    text
}

pub(crate) fn rendered(tracks: &[TrackAudio]) -> Vec<String> {
    tracks.iter().map(render).collect()
}

pub(crate) fn wanted(row: &Row) -> Vec<String> {
    row.expect
        .iter()
        .enumerate()
        .map(|(i, token)| expected(i, token))
        .collect()
}

#[test]
fn generated_oracle_matches_baseline_manifests() {
    let all = rows();
    assert_eq!(all.len(), 182, "oracle row count");
    let mut spans = 0usize;
    for (n, row) in all.iter().enumerate() {
        let manifest = plan(row);
        assert_eq!(manifest.disposition(), AudioDisposition::SilentBaseline);
        assert_eq!(manifest.tracks().len(), row.tracks.len());
        assert_eq!(
            rendered(manifest.tracks()),
            wanted(row),
            "row {n} (export from frame {})",
            row.frame
        );
        spans += manifest.audible().len();
    }
    assert_eq!(spans, 301, "audible spans over the whole oracle");
}

#[test]
fn oracle_covers_every_manifest_outcome() {
    let mut tally: std::collections::BTreeMap<String, usize> = Default::default();
    for row in rows() {
        for token in &row.expect {
            let key = if token.starts_with("Y:") {
                token
                    .split(':')
                    .nth(4)
                    .map(|k| format!("Y:{k}"))
                    .unwrap_or_default()
            } else {
                token.clone()
            };
            *tally.entry(key).or_default() += 1;
        }
    }
    let want: [(&str, usize); 16] = [
        ("N:ContextIsolated", 16),
        ("N:EmptyWindow", 9),
        ("N:Muted", 57),
        ("N:NoAudioContext", 8),
        ("N:NoBuffer", 51),
        ("N:PastTrimEnd", 109),
        ("R:InvalidFps", 12),
        ("R:NonFiniteBufferDuration", 1),
        ("R:NonFiniteOffset", 2),
        ("R:NonFiniteStaticVolume", 2),
        ("R:NonFiniteTrimEnd", 2),
        ("R:NonFiniteTrimStart", 1),
        ("R:NonFiniteVolumeSample", 8),
        ("R:TimeOutOfRange", 2),
        ("Y:c", 89),
        ("Y:v", 212),
    ];
    let got: Vec<(String, usize)> = tally.into_iter().collect();
    let want: Vec<(String, usize)> = want.iter().map(|(k, n)| (k.to_string(), *n)).collect();
    assert_eq!(got, want);
}

#[test]
fn the_oracle_pins_a_delayed_span_literally() {
    // Export from frame 0, 24 fps, track `B` (trimStart 1.5 s, 10 s buffer): audible from
    // 1.5 s, reading the buffer from 1.5 s, for the remaining 8.5 s, flat gain 1.
    let row = rows()
        .into_iter()
        .find(|r| r.frame == 0 && r.tracks.len() == 10 && r.env.fps == 24.0 && r.env.audio_context)
        .expect("the all-tracks scene at frame 0");
    assert_eq!(
        row.expect[1],
        "Y:3ff8000000000000:3ff8000000000000:4021000000000000:v:3ff0000000000000"
    );
    let manifest = plan(&row);
    let TrackAudio::Audible(span) = &manifest.tracks()[1] else {
        panic!("track B sounds");
    };
    assert_eq!(span.start_s, 1.5);
    assert_eq!(span.end_s, 10.0);
    assert_eq!(span.start.duration_s, 8.5);
    assert!(!span.start.starts_now);
}
