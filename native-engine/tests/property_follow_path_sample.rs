//! nemo-test-harness: test-evaluation => property_follow_path_sample_tests
//! Follow-path path sampler (D30/#2075): oracle against Paper.js itself.
//!
//! Rows come from `gen.mjs` (sha256 0aee2efd3453511ea797d227eaa5fab9184191336bc9dc0bb7d92d368a42b53f; its
//! stub-window loader `load.cjs` is 28a8aa2e9ff8496ba98807983020ab9905df1a769993ce7a6a41f32178feb93e), which runs the SHIPPED
//! `src/paper-full.min.js` (Paper.js 0.12.18, sha256
//! 13526b252be3856db63b096ca282270454a3bb4567943e137875d9ced54ce9ba) in a Node v22.22.0 vm. The same rows were
//! also reproduced by the npm `paper@0.12.18` `dist/paper-full.min.js` (sha256
//! b2a2dee63f6ab93d7ec4c50c51c894f14516cf9060b3ace0f07d6cc23584fdac), whose unminified `paper-core.js` was
//! read for the port; both builds report version 0.12.18 and the generator
//! aborts if they disagree. `sel` rows run the VERBATIM `sampleFollowPathAt`
//! (`src/js/motion.js`, blob 90503c39... of the base checkout) over real Paper with the layer lookups stubbed.
//!
//! Values are IEEE 754 bit patterns in hex; `-` is null/undefined, `!` a
//! stroke without `segments`, `z` an empty sample. Rows: `pth` (segments,
//! closed, `path.length`, then `getPointAt`/`getTangentAt` at 4 + 15 + 11 per
//! curve offsets, including 0, -0, the length, past it, NaN, infinity,
//! negative, and every curve boundary +-1e-9) and `sel` (stroke selection,
//! zero length, the offset read by `getPointAt`, and `{x, y, angle}`).
//!
//! Tolerance: Paper's arithmetic is `+ - * /` and `sqrt` only (correctly
//! rounded), so every `pth` value and the `sel` position are compared bit for
//! bit, `-0` included. Only the angle of a `sel` row goes through `atan2`
//! (libm-dependent): it may differ by TRIG_ULPS units of roundoff of 180.

use crate::evaluation::property_follow_path_sample::{
    follow_path_geometry, path_curves, path_length, point_tangent_at, SegmentIn, StrokeIn,
};
use crate::property_follow_path_sample_controls_tests::Mutation;
use std::f64::consts::PI;

pub(crate) const ORACLE: &str = include_str!("fixtures/property-follow-path-sample/oracle.txt");
const TRIG_ULPS: f64 = 4.0;
const ANGLE_FLOOR: f64 = 180.0;

pub(crate) fn bits(x: f64) -> u64 {
    if x.is_nan() {
        f64::NAN.to_bits()
    } else {
        x.to_bits()
    }
}

pub(crate) fn hex(h: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(h, 16).expect("hex bits"))
}

fn list(token: &str) -> Vec<f64> {
    token.split(',').map(hex).collect()
}

fn segments(token: &str) -> Vec<SegmentIn> {
    token
        .split(';')
        .filter(|s| !s.is_empty())
        .map(|s| {
            let v = list(s);
            SegmentIn {
                point: [v[0], v[1]],
                handle_in: Some([v[2], v[3]]),
                handle_out: Some([v[4], v[5]]),
            }
        })
        .collect()
}

/// One comparison row; a mismatch is recorded so mutated runs can be counted.
struct Check {
    label: String,
    bad: Option<String>,
}

impl Check {
    fn fail(&mut self, what: String) {
        if self.bad.is_none() {
            self.bad = Some(format!("{}: {what}", self.label));
        }
    }

    fn exact(&mut self, actual: &[f64], expected: &[f64]) {
        let same = actual.len() == expected.len()
            && actual
                .iter()
                .zip(expected)
                .all(|(&a, &e)| bits(a) == bits(e));
        if !same {
            self.fail(format!("{actual:?} != {expected:?}"));
        }
    }

    fn angle(&mut self, actual: f64, expected: f64) {
        let ulps = (actual - expected).abs() / (ANGLE_FLOOR * f64::EPSILON);
        if bits(actual) != bits(expected) && (ulps > TRIG_ULPS || ulps.is_nan()) {
            self.fail(format!("angle {actual:?} != {expected:?}"));
        }
    }
}

#[derive(Default)]
pub(crate) struct Run {
    pub rows: usize,
    pub bad: Vec<String>,
}

fn pth(c: &mut Check, t: &[&str], m: Mutation) {
    let mut segs = segments(t[2]);
    let closed = t[3] == "1" && m != Mutation::NoClosing;
    for s in &mut segs {
        s.handle_out = s.handle_out.map(|h| m.handle(h));
    }
    let curves = path_curves(&segs, closed);
    c.exact(&[path_length(&curves)], &[hex(t[4])]);
    for probe in t[5].split(';') {
        let (off, want) = probe.split_once('=').expect("off=value");
        let got = point_tangent_at(&curves, closed, m.offset(hex(off)));
        match (got, want) {
            (None, "-") => {}
            (Some(g), w) if w != "-" => {
                let tangent = g.tangent.map(|v| m.tangent(v)).unwrap_or([f64::NAN; 2]);
                c.exact(&[g.point[0], g.point[1], tangent[0], tangent[1]], &list(w));
            }
            (g, w) => c.fail(format!("offset {off}: {g:?} vs {w}")),
        }
    }
}

fn sel(c: &mut Check, t: &[&str], m: Mutation) {
    let owned: Vec<Option<(Option<Vec<SegmentIn>>, bool)>> = if t[2] == "~" {
        Vec::new()
    } else {
        t[2].split('|')
            .map(|s| match s {
                "-" => None,
                "!" => Some((None, false)),
                _ => {
                    let (g, cl) = s.split_once('@').expect("segs@closed");
                    Some((Some(segments(g)), cl == "1" && m != Mutation::NoClosing))
                }
            })
            .collect()
    };
    let strokes: Vec<Option<StrokeIn>> = owned
        .iter()
        .map(|o| {
            o.as_ref().map(|(g, closed)| StrokeIn {
                segments: g.as_deref(),
                closed: *closed,
            })
        })
        .collect();
    let off = if t[4] == "-" {
        0.0
    } else {
        m.offset(hex(t[4]))
    };
    let got = follow_path_geometry(&strokes, |_| off);
    match (got, t[6]) {
        (None, "-") => {}
        (Some(g), w) if w != "-" => {
            let want = list(w);
            c.exact(&[g.len, g.offset], &[hex(t[5]), hex(t[4])]);
            c.exact(&g.point, &want[..2]);
            let (dx, dy) = (g.second[0] - g.point[0], g.second[1] - g.point[1]);
            c.angle(dy.atan2(dx) * 180.0 / PI, want[2]);
        }
        (g, w) => c.fail(format!("{g:?} vs {w}")),
    }
}

pub(crate) fn run(m: Mutation) -> Run {
    let mut r = Run::default();
    for (n, row) in ORACLE.lines().enumerate() {
        let t: Vec<&str> = row.split(' ').collect();
        let mut c = Check {
            label: format!("line {}: {}", n + 1, &row[..row.len().min(120)]),
            bad: None,
        };
        match t[0] {
            "pth" => pth(&mut c, &t, m),
            "sel" => sel(&mut c, &t, m),
            op => panic!("unknown oracle op {op}"),
        }
        r.rows += 1;
        r.bad.extend(c.bad);
    }
    r
}

#[test]
fn generated_oracle_matches_paper() {
    let r = run(Mutation::None);
    assert!(
        r.bad.is_empty(),
        "{} rows differ:\n{}",
        r.bad.len(),
        r.bad.join("\n")
    );
    assert_eq!(r.rows, 334, "oracle row count");
}
