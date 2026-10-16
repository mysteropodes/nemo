//! Oracle tests for the video source-frame kernel (G28, L09-A).
//!
//! `fixtures/object-media-timing-video/oracle.txt` was produced by running the
//! SHIPPED `_targetFor`, `_targetExactFor`, `_interpMode`, `_blendWeight` and
//! the frame-selection region of `_layerFrameSyncAsync`, cut verbatim from
//! `src/js/native-video-bridge.js` at fbe4b8a3 and evaluated in a Node `vm`
//! (generator `tools/gen.js`; the functions are reached through the file's own
//! `SMNativeVideo_blendTest` hook, `SMMotion.timeRemapValue` is the injected
//! port). Values are IEEE 754 bit patterns in hex; `-` is a port that answers
//! `null`/`undefined`; `u` an absent `interpMode`, `s:<text>` that string.
//! Rows: `X nv fc off tr rv frame exact rounded`, `W exact weight`,
//! `M nv tr imode fb mode`, `S fc off tr imode fb rv frame S:<f>|B:<f>:<t>:<w>`.
//!
//! Numbers compare on bits (two NaNs match). The kernel refuses where the
//! baseline yields `NaN` or silently clamps an infinity; those rows are the
//! non-finite frame and the non-finite remap value, counted below, and the
//! baseline value in the row is provenance only.

use super::controls::Mutation;
use super::{FrameSelection, InterpMode, TimeRemapPort, VideoLayerTiming, VideoTimingRefusal};

const ORACLE: &str = include_str!("fixtures/object-media-timing-video/oracle.txt");

pub(crate) fn hex(h: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(h, 16).expect("hex bits"))
}

pub(crate) fn bits(x: f64) -> u64 {
    if x.is_nan() {
        f64::NAN.to_bits()
    } else {
        x.to_bits()
    }
}

/// A time-remap port with a fixed answer.
pub(crate) struct Fixed(pub Option<f64>);

impl TimeRemapPort for Fixed {
    fn remap_value(&self, _frame: f64) -> Option<f64> {
        self.0
    }
}

fn port_of(token: &str) -> Fixed {
    Fixed(if token == "-" { None } else { Some(hex(token)) })
}

fn imode_of(token: &str) -> Option<&str> {
    if token == "u" {
        None
    } else {
        Some(token.strip_prefix("s:").expect("s: prefix"))
    }
}

fn layer_of<'a>(
    native_video: bool,
    fc: &str,
    off: &str,
    tr: &str,
    interp_mode: Option<&'a str>,
    frame_blend: bool,
) -> VideoLayerTiming<'a> {
    VideoLayerTiming {
        frame_count: hex(fc),
        offset_frames: hex(off),
        native_video,
        time_remap: tr == "1",
        interp_mode,
        frame_blend,
    }
}

/// The documented refusal for a row, from its inputs.
fn refusal_of(l: &VideoLayerTiming<'_>, frame: f64, p: &Fixed) -> Option<VideoTimingRefusal> {
    if !frame.is_finite() {
        return Some(VideoTimingRefusal::NonFiniteFrame);
    }
    match p.0 {
        Some(v) if l.native_video && l.time_remap && !v.is_finite() => {
            Some(VideoTimingRefusal::NonFiniteTimeRemap)
        }
        _ => None,
    }
}

#[derive(Default)]
pub(crate) struct Run {
    pub x: usize,
    pub w: usize,
    pub m: usize,
    pub s: usize,
    pub refused_frame: usize,
    pub refused_remap: usize,
    pub refused_weight: usize,
    pub bad: Vec<String>,
}

fn count_refusal(r: &mut Run, refusal: Option<VideoTimingRefusal>) {
    match refusal {
        Some(VideoTimingRefusal::NonFiniteFrame) => r.refused_frame += 1,
        Some(VideoTimingRefusal::NonFiniteTimeRemap) => r.refused_remap += 1,
        None => {}
    }
}

fn x_row(t: &[&str], m: Mutation, r: &mut Run) -> Option<String> {
    let base = layer_of(t[1] == "1", t[2], t[3], t[4], None, false);
    let (port, frame) = (port_of(t[5]), hex(t[6]));
    let refusal = refusal_of(&base, frame, &port);
    count_refusal(r, refusal);
    let got = m.source(&base, frame, &port);
    match (got, refusal) {
        (Err(e), Some(want)) if e == want => None,
        (Ok(g), None) if bits(g.exact) == bits(hex(t[7])) && bits(g.rounded) == bits(hex(t[8])) => {
            None
        }
        (g, want) => Some(format!("{g:?}, expected refusal {want:?}")),
    }
}

fn w_row(t: &[&str], m: Mutation, r: &mut Run) -> Option<String> {
    let exact = hex(t[1]);
    match (m.weight(exact), exact.is_finite()) {
        (Err(VideoTimingRefusal::NonFiniteFrame), false) => {
            r.refused_weight += 1;
            None
        }
        (Ok(w), true) if bits(w) == bits(hex(t[2])) => None,
        (g, _) => Some(format!("{g:?}")),
    }
}

fn m_row(t: &[&str], m: Mutation) -> Option<String> {
    let base = layer_of(
        t[1] == "1",
        "4024000000000000",
        "0",
        t[2],
        imode_of(t[3]),
        t[4] == "1",
    );
    let want = match t[5] {
        "none" => InterpMode::None,
        "blend" => InterpMode::Blend,
        "motion" => InterpMode::Motion,
        other => panic!("unknown mode {other}"),
    };
    let got = m.mode(&base);
    (got != want).then(|| format!("{got:?} != {want:?}"))
}

fn s_row(t: &[&str], m: Mutation, r: &mut Run) -> Option<String> {
    let base = layer_of(true, t[1], t[2], t[3], imode_of(t[4]), t[5] == "1");
    let (port, frame) = (port_of(t[6]), hex(t[7]));
    let refusal = refusal_of(&base, frame, &port);
    count_refusal(r, refusal);
    let got = m.select(&base, frame, &port);
    let token = match &got {
        Ok(FrameSelection::Single { frame }) => format!("S:{:016x}", bits(*frame)),
        Ok(FrameSelection::Blend(p)) => format!(
            "B:{:016x}:{:016x}:{:016x}",
            bits(p.from),
            bits(p.to),
            bits(p.weight)
        ),
        Err(_) => String::new(),
    };
    match (&got, refusal) {
        (Err(e), Some(want)) if *e == want => None,
        (Ok(_), None) if token == t[8] => None,
        (g, want) => Some(format!("{g:?} ({token}), expected refusal {want:?}")),
    }
}

pub(crate) fn run(m: Mutation) -> Run {
    let mut r = Run::default();
    for (n, row) in ORACLE.lines().enumerate() {
        let t: Vec<&str> = row.split(' ').collect();
        let miss = match t[0] {
            "X" => {
                r.x += 1;
                x_row(&t, m, &mut r)
            }
            "W" => {
                r.w += 1;
                w_row(&t, m, &mut r)
            }
            "M" => {
                r.m += 1;
                m_row(&t, m)
            }
            "S" => {
                r.s += 1;
                s_row(&t, m, &mut r)
            }
            op => panic!("unknown oracle op {op}"),
        };
        if let Some(why) = miss {
            r.bad.push(format!(
                "line {}: {} -> {why}",
                n + 1,
                &row[..row.len().min(110)]
            ));
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
        r.bad.join("\n")
    );
    assert_eq!(
        (r.x, r.w, r.m, r.s),
        (832, 148, 56, 322),
        "oracle row counts"
    );
    assert_eq!(ORACLE.lines().count(), 1358);
}

#[test]
fn oracle_refusal_rows_are_the_documented_ones() {
    // Non-finite frames and remap values are typed refusals; nothing else is.
    let r = run(Mutation::None);
    assert_eq!(
        (r.refused_frame, r.refused_remap, r.refused_weight),
        (25, 57, 3)
    );
}
