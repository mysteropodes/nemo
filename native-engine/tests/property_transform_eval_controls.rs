//! nemo-test-harness: test-evaluation => property_transform_eval_controls_tests
//! Hand-written controls for the property-transform provider: oracle
//! coverage, mutation detection and the JavaScript coercions.

use crate::evaluation::motion_parent_chain::Pose;
use crate::evaluation::motion_parent_chain::{ChainEntry, MotionMat};
use crate::evaluation::property_transform_eval::{follow_path_offset, weighted_parents_of};
use crate::property_transform_eval_tests::{hex, list, mat, run, ORACLE};
use std::collections::HashMap;

fn bits(x: f64) -> u64 {
    if x.is_nan() {
        f64::NAN.to_bits()
    } else {
        x.to_bits()
    }
}

pub(crate) fn pose(token: &str) -> Pose {
    let v = list(token);
    Pose {
        dx: v[0],
        dy: v[1],
        rot: v[2],
        sx: v[3],
        sy: v[4],
    }
}

/// `-` is undefined and `~` the empty string.
pub(crate) fn uid(token: &str) -> Option<&str> {
    match token {
        "-" => None,
        "~" => Some(""),
        u => Some(u),
    }
}

/// A sampled value: `u` (undefined) and `z` (`[]`) read as NaN.
pub(crate) fn sample(token: &str) -> f64 {
    if token == "u" || token == "z" {
        f64::NAN
    } else {
        hex(token)
    }
}

pub(crate) fn extras(token: &str) -> Vec<Option<&str>> {
    if token == "-" {
        return Vec::new();
    }
    token
        .split(';')
        .map(|e| if e == "!" { None } else { uid(e) })
        .collect()
}

pub(crate) fn weights(token: &str) -> Vec<f64> {
    if token == "-" {
        return Vec::new();
    }
    token.split(';').map(sample).collect()
}

pub(crate) fn table(token: &str) -> HashMap<&str, Pose> {
    if token == "-" {
        return HashMap::new();
    }
    token
        .split(';')
        .map(|e| {
            let (u, p) = e.split_once('=').expect("uid=pose");
            (u, pose(p))
        })
        .collect()
}

/// A deliberate mistake the comparison must catch.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Mutation {
    None,
    /// Parent blend read as `100 - t`: A and B swapped.
    Blend,
    /// Extra weights read by position among kept extras, not original index.
    Index,
    /// `align` inverted.
    Align,
    /// Bounds centre x and y swapped.
    Axis,
    /// Path percent off by one.
    Percent,
    /// Tangent y and rotations negated.
    Sign,
    /// The B-parent dispatch ignored.
    Dispatch,
}

impl Mutation {
    pub(crate) fn blend(self, x: f64) -> f64 {
        [x, 100.0 - x][usize::from(self == Mutation::Blend)]
    }

    pub(crate) fn weights(self, ex: &[Option<&str>], w: &[f64]) -> Vec<f64> {
        if self != Mutation::Index {
            return w.to_vec();
        }
        let kept = |e: &Option<&str>| e.is_some_and(|u| !u.is_empty());
        ex.iter()
            .zip(w)
            .filter(|(e, _)| kept(e))
            .map(|(_, &w)| w)
            .collect()
    }

    pub(crate) fn uid_b(self, b: Option<&str>) -> Option<&str> {
        if self == Mutation::Dispatch {
            None
        } else {
            b
        }
    }

    pub(crate) fn rot(self, m: MotionMat) -> MotionMat {
        let mut m = m;
        if self == Mutation::Sign {
            m.rot = -m.rot;
        }
        m
    }

    pub(crate) fn chain(self, token: &str) -> Vec<ChainEntry> {
        if token == "-" {
            return Vec::new();
        }
        token
            .split(';')
            .map(|e| {
                let (m, p) = e.split_once('/').expect("mat/pivot");
                let p = list(p);
                ChainEntry {
                    mat: self.rot(mat(m).expect("chain mat")),
                    pivot: [p[0], p[1]],
                }
            })
            .collect()
    }
}

/// Every operation family appears in the oracle, with the interesting
/// shapes: each count is what the generator emits.
#[test]
fn oracle_covers_the_families_and_edges() {
    let count = |p: &str| ORACLE.lines().filter(|l| l.starts_with(p)).count();
    for (op, n) in [
        ("wp ", 110),
        ("bam ", 330),
        ("pcm ", 130),
        ("fp ", 260),
        ("fpo ", 260),
    ] {
        assert_eq!(count(op), n, "{op}");
    }
    let has = |p: &str, f: &dyn Fn(&[&str]) -> bool| {
        ORACLE
            .lines()
            .filter(|l| l.starts_with(p))
            .any(|l| f(&l.split(' ').collect::<Vec<_>>()))
    };
    let nan = "7ff8000000000000";
    let neg0 = "8000000000000000";
    assert!(
        has("wp ", &|t| t[6].split(';').any(|w| w == nan)),
        "NaN weight"
    );
    assert!(
        has("wp ", &|t| t[6].split(';').any(|w| w == neg0)),
        "-0 weight"
    );
    assert!(
        has("wp ", &|t| t[5].contains(";~;")),
        "empty uid in the middle"
    );
    assert!(has("wp ", &|t| t[5].contains("!")), "null extra entry");
    assert!(
        has("wp ", &|t| t[7].contains(":0")
            || t[7].contains(":8000000000000000")),
        "zero weight part"
    );
    assert!(
        has("bam ", &|t| t[8] == "-" && t[5] != "-"),
        "weighted None"
    );
    assert!(
        has("pcm ", &|t| t[3] != "-" && t[3] != "~" && t[9] != "-"),
        "B-parent chain"
    );
    assert!(
        has("fp ", &|t| t[5] == "-" && t[4] != "-" && t[2] == "1"),
        "missing bounds"
    );
    assert!(has("fp ", &|t| t[6] == "0" && t[4] != "-"), "align off");
    assert!(
        has("fp ", &|t| t[6] == "1" && t[4] != "-" && t[2] == "1"),
        "align on"
    );
    assert!(has("fpo ", &|t| t[9] == "-"), "zero length");
    assert!(has("fpo ", &|t| t[5] == "-" && t[9] != "-"), "no tangent");
}

/// Each deliberate mistake must make rows fail; the counts are measured on
/// this oracle.
#[test]
fn mutations_are_detected() {
    for (m, at_least) in [
        (Mutation::Blend, 172),
        (Mutation::Index, 65),
        (Mutation::Align, 86),
        (Mutation::Axis, 160),
        (Mutation::Percent, 82),
        (Mutation::Sign, 163),
        (Mutation::Dispatch, 84),
    ] {
        let failed = run(m).bad.len();
        println!("{m:?}: {failed} rows rejected");
        assert!(failed >= at_least, "{m:?} slipped through: {failed} rows");
    }
}

/// JavaScript coercion of the raw samples, outside the oracle rows.
#[test]
fn falsy_samples_read_as_zero() {
    for blend in [f64::NAN, 0.0, -0.0] {
        let p = weighted_parents_of(Some("A"), Some("B"), blend, &[Some("C")], &[f64::NAN])
            .expect("parts");
        assert_eq!(p.len(), 3);
        assert_eq!(bits(p[0].weight), bits(1.0), "A at full weight");
        assert_eq!(bits(p[1].weight), bits(0.0), "B at +0, not -0");
        assert_eq!(bits(p[2].weight), bits(0.0), "NaN extra reads +0");
    }
    assert!(weighted_parents_of(Some("A"), Some("B"), 50.0, &[], &[]).is_none());
    // Extras that all drop still take the weighted path (two parts).
    let p = weighted_parents_of(Some("A"), Some("B"), 50.0, &[Some("")], &[100.0]).expect("parts");
    assert_eq!(p.len(), 2);
    assert_eq!(follow_path_offset(f64::NAN, 100.0), Some(0.0));
    assert!(follow_path_offset(50.0, 0.0).is_none());
    assert!(follow_path_offset(50.0, f64::NAN).is_none());
}
