//! Oracle tests for the audio track timing kernel (G30, L09-B).
//!
//! `fixtures/object-media-timing-audio/oracle.txt` was produced by running the
//! SHIPPED `ensureCtx`, `tracks`, `currentVolume`, `_audioContextSuspended`,
//! `trimOf`, `startTrack` and `scrubAt`, cut verbatim from
//! `src/js/audio-bridge.js` at fbe4b8a3 and evaluated in a Node `vm` against a
//! recording fake `AudioContext` (generator `tools/gen.js`). The injected
//! volume sampler is one of six pure profiles, mirrored by [`Profile`].
//! Values are IEEE 754 bit patterns in hex; `-` is JS `undefined`, `u` an
//! unset `trimEnd`. Rows:
//! `P mu buf off ts te anim ms lv susp ctxok fps ct frame RESULT` (startTrack),
//! `B mu buf off ts te anim ms lv susp ctxok fps frame RESULT` (scrubAt for one
//! track). RESULT is `N:<SilentReason>`, `R:<AudioTimingRefusal>` or `Y:` plus
//! the recorded `src.start` and gain arguments. N/R labels are the generator's
//! classification (the baseline only shows that no source started), checked
//! there against the executed baseline; R rows are inputs the kernel refuses
//! on purpose. Each kernel answer is rendered in the oracle grammar and
//! compared as text, so every f64 and f32 is compared on its bit pattern.

use super::controls::Mutation;
use super::{
    flat_volume, AudioTimingRefusal, AudioTrackTiming, PlaybackEnv, ScrubOutcome, StartOutcome,
    TrackGain, TrackStart, TrackVolume, VolumeSampler,
};

const ORACLE: &str = include_str!("fixtures/object-media-timing-audio/oracle.txt");

pub(crate) fn hex(h: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(h, 16).expect("hex bits"))
}

fn h(x: f64) -> String {
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
pub(crate) struct Untouchable;

impl VolumeSampler for Untouchable {
    fn volume_at(&self, frame: f64) -> Option<f64> {
        panic!("a flat track asked the volume sampler for frame {frame}")
    }
}

fn opt(s: &str) -> Option<f64> {
    if s == "-" {
        None
    } else {
        Some(hex(s))
    }
}

struct Inputs {
    env: PlaybackEnv,
    track: AudioTrackTiming,
    anim: Option<u8>,
    ct: f64,
    frame: f64,
    expect: String,
}

fn inputs(t: &[&str], scrub: bool) -> Inputs {
    let anim = if t[6] == "-" {
        None
    } else {
        Some(t[6].parse().expect("profile digit"))
    };
    let volume = match anim {
        Some(_) => TrackVolume::Animated,
        None => TrackVolume::Flat(flat_volume(opt(t[7]), opt(t[8]))),
    };
    let at = if scrub { 12 } else { 13 };
    Inputs {
        env: PlaybackEnv {
            fps: hex(t[11]),
            context_isolated: t[9] == "1",
            audio_context: t[10] == "1",
        },
        track: AudioTrackTiming {
            muted: t[1] == "1",
            buffer_duration: opt(t[2]),
            offset_frames: hex(t[3]),
            trim_start: hex(t[4]),
            trim_end: if t[5] == "u" { None } else { Some(hex(t[5])) },
            volume,
        },
        anim,
        ct: if scrub { 0.0 } else { hex(t[12]) },
        frame: hex(t[at]),
        expect: t[at + 1].to_string(),
    }
}

fn sampler_of(anim: Option<u8>) -> Box<dyn VolumeSampler> {
    match anim {
        Some(p) => Box::new(Profile(p)),
        None => Box::new(Untouchable),
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

fn render_gain(s: &TrackStart, ct: f64) -> String {
    match &s.gain {
        TrackGain::Value(v) => format!("v:{}", h(*v)),
        TrackGain::Curve(c) => format!(
            "c:{}:{}:{}:{:016x}:{:08x}:{:08x}",
            h(s.audio_time(ct)),
            h(c.duration_s),
            c.values.len(),
            fnv(&c.values),
            c.values[0].to_bits(),
            c.values[c.values.len() - 1].to_bits()
        ),
    }
}

pub(crate) fn render_start(r: &Result<StartOutcome, AudioTimingRefusal>, ct: f64) -> String {
    match r {
        Err(e) => format!("R:{e:?}"),
        Ok(StartOutcome::Silent(why)) => format!("N:{why:?}"),
        Ok(StartOutcome::Play(s)) => format!(
            "Y:{}:{}:{}:{}",
            h(s.src_when(ct)),
            h(s.offset_s),
            h(s.duration_s),
            render_gain(s, ct)
        ),
    }
}

pub(crate) fn render_scrub(r: &Result<ScrubOutcome, AudioTimingRefusal>) -> String {
    match r {
        Err(e) => format!("R:{e:?}"),
        Ok(ScrubOutcome::Silent(why)) => format!("N:{why:?}"),
        Ok(ScrubOutcome::Blip(b)) => {
            format!("Y:{}:{}:v:{}", h(b.offset_s), h(b.duration_s), h(b.gain))
        }
    }
}

#[derive(Default)]
pub(crate) struct Run {
    pub playback: usize,
    pub scrub: usize,
    pub silent: usize,
    pub refused: usize,
    pub played: usize,
    pub curves: usize,
    pub bad: Vec<String>,
}

pub(crate) fn run(m: Mutation) -> Run {
    let mut r = Run::default();
    for line in ORACLE.lines() {
        let t: Vec<&str> = line.split(' ').collect();
        let scrub = match t[0] {
            "P" => false,
            "B" => true,
            other => panic!("unknown oracle row kind {other}"),
        };
        let i = inputs(&t, scrub);
        let sampler = sampler_of(i.anim);
        let got = if scrub {
            r.scrub += 1;
            render_scrub(&m.plan_scrub(&i.env, &i.track, i.frame, &*sampler))
        } else {
            r.playback += 1;
            render_start(&m.plan_start(&i.env, &i.track, i.frame, &*sampler), i.ct)
        };
        match i.expect.as_bytes()[0] {
            b'N' => r.silent += 1,
            b'R' => r.refused += 1,
            _ => r.played += 1,
        }
        if i.expect.contains(":c:") {
            r.curves += 1;
        }
        if got != i.expect {
            r.bad.push(format!("{}: {line} => {got}", t[0]));
        }
    }
    r
}

#[test]
fn generated_oracle_matches_baseline() {
    let r = run(Mutation::None);
    assert!(
        r.bad.is_empty(),
        "{} rows differ:\n{}",
        r.bad.len(),
        r.bad[..r.bad.len().min(5)].join("\n")
    );
    assert_eq!((r.playback, r.scrub), PLAYBACK_SCRUB_ROWS);
    assert_eq!((r.silent, r.refused, r.played), SILENT_REFUSED_PLAYED);
    assert_eq!(r.curves, CURVE_ROWS);
}

const PLAYBACK_SCRUB_ROWS: (usize, usize) = (969, 391);
const SILENT_REFUSED_PLAYED: (usize, usize, usize) = (469, 104, 787);
const CURVE_ROWS: usize = 114;

fn count_rows(kind: &str, result: &str) -> usize {
    ORACLE
        .lines()
        .filter(|l| l.starts_with(kind) && l.ends_with(&format!(" {result}")))
        .count()
}

/// Row counts per (entry point, result): every silent reason and refusal the
/// kernel can give is exercised, and the table sums to the totals above.
const ROW_COUNTS: &[(&str, &str, usize)] = &[
    ("P ", "N:Muted", 17),
    ("P ", "N:NoBuffer", 9),
    ("P ", "N:ContextIsolated", 5),
    ("P ", "N:NoAudioContext", 3),
    ("P ", "N:PastTrimEnd", 234),
    ("P ", "N:EmptyWindow", 21),
    ("P ", "R:InvalidFps", 5),
    ("P ", "R:NonFiniteFrame", 3),
    ("P ", "R:NonFiniteOffset", 2),
    ("P ", "R:NonFiniteTrimStart", 2),
    ("P ", "R:NonFiniteTrimEnd", 3),
    ("P ", "R:NonFiniteBufferDuration", 2),
    ("P ", "R:TimeOutOfRange", 6),
    ("P ", "R:NonFiniteStaticVolume", 6),
    ("P ", "R:NonFiniteVolumeSample", 45),
    ("B ", "N:Muted", 3),
    ("B ", "N:NoBuffer", 1),
    ("B ", "N:ContextIsolated", 8),
    ("B ", "N:NoAudioContext", 4),
    ("B ", "N:BeforeTrimStart", 88),
    ("B ", "N:PastTrimEnd", 76),
    ("B ", "R:InvalidFps", 4),
    ("B ", "R:NonFiniteFrame", 3),
    ("B ", "R:NonFiniteOffset", 2),
    ("B ", "R:NonFiniteTrimStart", 2),
    ("B ", "R:NonFiniteTrimEnd", 3),
    ("B ", "R:NonFiniteBufferDuration", 2),
    ("B ", "R:TimeOutOfRange", 2),
    ("B ", "R:NonFiniteStaticVolume", 6),
    ("B ", "R:NonFiniteVolumeSample", 6),
];

#[test]
fn oracle_covers_every_silent_reason_and_refusal() {
    for (kind, result, rows) in ROW_COUNTS {
        assert_eq!(count_rows(kind, result), *rows, "{kind}{result}");
    }
    let sum: usize = ROW_COUNTS.iter().map(|r| r.2).sum();
    assert_eq!(sum, SILENT_REFUSED_PLAYED.0 + SILENT_REFUSED_PLAYED.1);
}

#[test]
fn oracle_pins_the_curve_cap_boundary() {
    // 1999 and 2000 samples both occur, and nothing exceeds the cap.
    let lens: Vec<usize> = ORACLE
        .lines()
        .filter_map(|l| l.split(':').nth(7).filter(|_| l.contains(":c:")))
        .map(|n| n.parse().expect("curve length"))
        .collect();
    assert!(lens.contains(&1999) && lens.contains(&2000));
    assert!(lens.iter().all(|n| (2..=2000).contains(n)));
}
