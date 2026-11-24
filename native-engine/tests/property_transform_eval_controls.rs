//! nemo-test-harness: test-evaluation => property_transform_eval_controls_tests
//! Hand-written controls for the property-transform provider: oracle
//! coverage, mutation detection and the JavaScript coercions.

use crate::evaluation::motion_parent_chain::Pose;
use crate::evaluation::motion_parent_chain::{ChainEntry, MotionMat};
use crate::evaluation::property_transform_eval::{
    apply_follow_path, blended_ancestor_mat, follow_path_offset, follow_path_points,
    follow_path_sample, parent_chain_mats, weighted_parents_of, PathSample,
};
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

// Acceptance vectors (D30, I06). Every expectation below is hand arithmetic
// written out in the comment beside it, never a value read back from the
// kernel.

fn pz(dx: f64, dy: f64, rot: f64, sx: f64) -> Pose {
    Pose {
        dx,
        dy,
        rot,
        sx,
        sy: sx,
    }
}

fn mm(dx: f64, dy: f64, rot: f64, sx: f64, sy: f64, ax: f64, ay: f64) -> MotionMat {
    MotionMat {
        dx,
        dy,
        rot,
        sx,
        sy,
        op: 1.0,
        ax,
        ay,
    }
}

/// D30 two- and N-parent weights: A = 1 - 25/100, B = 25/100, each kept
/// extra its own percent, sampled by its ORIGINAL `parentsMore` index (the
/// null at index 1 is dropped, so D reads sample 2 = 10%, not 99%).
#[test]
fn n_parent_weights_follow_original_indices() {
    let p = weighted_parents_of(
        Some("A"),
        Some("B"),
        25.0,
        &[Some("C"), None, Some("D")],
        &[50.0, 99.0, 10.0],
    )
    .expect("parts");
    let got: Vec<(Option<&str>, f64)> = p.iter().map(|w| (w.uid, w.weight)).collect();
    let want = [(Some("A"), 0.75), (Some("B"), 0.25), (Some("C"), 0.5)];
    assert_eq!(&got[..3], &want);
    assert_eq!(got[3].0, Some("D"));
    assert!((got[3].1 - 0.1).abs() < 1e-15);
}

/// D30 N-parent blend: weights .5/.5/1 (blend 50, C at 100) normalize to
/// .25/.25/.5. dx = 100*.25 - 100*.25 + 40*.5 = 20; dy = 80*.5 = 40;
/// sx = 1*.25 + 2*.25 + 3*.5 = 2.25; rotation is the circular mean of
/// 0/90/0: (cos, sin) sums (.75, .25), atan2 = atan(1/3) = 18.43494882292201 deg.
#[test]
fn n_parent_blend_is_a_normalized_circular_mean() {
    let poses: HashMap<&str, Pose> = [
        ("A", pz(100.0, 0.0, 0.0, 1.0)),
        ("B", pz(-100.0, 0.0, 90.0, 2.0)),
        ("C", pz(40.0, 80.0, 0.0, 3.0)),
    ]
    .into_iter()
    .collect();
    let m = blended_ancestor_mat(Some("A"), Some("B"), 50.0, &[Some("C")], &[100.0], |u| {
        poses.get(u).copied()
    })
    .expect("blend");
    assert!((m.dx - 20.0).abs() < 1e-12 && (m.dy - 40.0).abs() < 1e-12);
    assert!((m.sx - 2.25).abs() < 1e-12 && (m.sy - 2.25).abs() < 1e-12);
    assert!((m.rot - 18.43494882292201).abs() < 1e-9);
}

/// D30 two-parent blend wraps the angle the short way: 350 -> 10 at 25% is
/// d = ((10 - 350 + 180) mod 360) - 180 = 20, 350 + 20 * .25 = 355 (not
/// 350 - 340 * .25 = 265). dx = 0 + 100 * .25 = 25.
#[test]
fn two_parent_blend_wraps_the_angle() {
    let m = blended_ancestor_mat(Some("A"), Some("B"), 25.0, &[], &[], |u| {
        Some(if u == "A" {
            pz(0.0, 0.0, 350.0, 1.0)
        } else {
            pz(100.0, 0.0, 10.0, 1.0)
        })
    })
    .expect("blend");
    assert_eq!((m.dx, m.rot, m.sx), (25.0, 355.0, 1.0));
}

/// D30 missing parents: without A or B (absent or empty uid) there is no
/// blend and nothing is composed; one unresolved parent is the identity
/// (A sx 3, dx 100 at 50%: dx 50, sx (3 + 1) / 2 = 2); none resolved is
/// `None`. Weights summing to <= 1e-9 (1 - .5 + .5 - 100% = 0) fall back to
/// A alone, and only A is composed.
#[test]
fn missing_parents_and_zero_weight_fallback() {
    let asked = std::cell::RefCell::new(Vec::new());
    let compose = |u: &str| {
        asked.borrow_mut().push(u.to_string());
        (u == "A").then(|| pz(100.0, 0.0, 0.0, 3.0))
    };
    assert!(blended_ancestor_mat(None, Some("B"), 50.0, &[], &[], &compose).is_none());
    assert!(blended_ancestor_mat(Some("A"), Some(""), 50.0, &[], &[], &compose).is_none());
    assert!(asked.borrow().is_empty());
    let m = blended_ancestor_mat(Some("A"), Some("B"), 50.0, &[], &[], &compose).unwrap();
    assert_eq!((m.dx, m.sx, m.sy), (50.0, 2.0, 2.0));
    let none = blended_ancestor_mat(Some("X"), Some("Y"), 50.0, &[], &[], &compose);
    assert!(none.is_none());
    asked.borrow_mut().clear();
    let f = blended_ancestor_mat(
        Some("A"),
        Some("B"),
        50.0,
        &[Some("C")],
        &[-100.0],
        &compose,
    )
    .unwrap();
    assert_eq!(
        (f.dx, f.sx, asked.take()),
        (100.0, 3.0, vec!["A".to_string()])
    );
}

/// D30 dispatch: a B parent yields one entry pivoted at the origin and never
/// runs the legacy walk; no B runs only the legacy walk.
#[test]
fn chain_dispatch_runs_only_the_taken_branch() {
    let m = mm(1.0, 2.0, 0.0, 1.0, 1.0, 0.0, 0.0);
    let legacy = vec![ChainEntry {
        mat: m,
        pivot: [7.0, 8.0],
    }];
    let blended = parent_chain_mats(Some("B"), || Some(m), || panic!("legacy ran"));
    assert_eq!(
        blended,
        vec![ChainEntry {
            mat: m,
            pivot: [0.0, 0.0]
        }]
    );
    assert!(parent_chain_mats(Some("B"), || None, || panic!("legacy ran")).is_empty());
    let l = parent_chain_mats(Some(""), || panic!("blend ran"), || legacy.clone());
    assert_eq!(l, legacy);
}

/// I06 composition and pivot order, D30 world/local and saved geometry.
/// Target mat: scale (2, 3), rot 90, translate (10, 20), anchor (5, 0);
/// bounds centre (100, 100) so the pivot is (105, 100). Point (115, 100):
/// local (10, 0) -> scaled (20, 0) -> rot 90 (x, y) -> (-y, x) = (0, 20)
/// -> + pivot (105, 120) -> + translate (115, 140). Second point (tangent
/// (0, 1) * 10) (115, 110): local (10, 10) -> (20, 30) -> (-30, 20) ->
/// (75, 120) -> (85, 140). Then the parent (scale 2 about the origin,
/// translate (100, 0)): (115, 140) -> (330, 280), (85, 140) -> (270, 280).
/// Translating before rotating would give (95, 100) instead of (115, 140).
#[test]
fn affine_composition_and_pivot_order() {
    let target = (mm(10.0, 20.0, 90.0, 2.0, 3.0, 5.0, 0.0), [100.0, 100.0]);
    let chain = vec![ChainEntry {
        mat: mm(100.0, 0.0, 0.0, 2.0, 2.0, 0.0, 0.0),
        pivot: [0.0, 0.0],
    }];
    let saved = chain.clone();
    let near = |a: [f64; 2], b: [f64; 2]| (a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9;
    let [p, q] = follow_path_points([115.0, 100.0], Some([0.0, 1.0]), Some(target), &[]);
    assert!(
        near(p, [115.0, 140.0]) && near(q, [85.0, 140.0]),
        "{p:?} {q:?}"
    );
    let [p, q] = follow_path_points([115.0, 100.0], Some([0.0, 1.0]), Some(target), &chain);
    assert!(
        near(p, [330.0, 280.0]) && near(q, [270.0, 280.0]),
        "{p:?} {q:?}"
    );
    assert_eq!(chain, saved, "the evaluated chain is never rewritten");
}

/// D30 angle wrap and align: atan2 of the tangent pair. (-1, 0) is exactly
/// 180 (atan2(+0, -10) = pi); just below the axis it is -180 + tiny, the
/// branch cut kept as is. Align adds angle * influence / 100: 90 * 50% =
/// 45 on top of rot 10. Bounds put the pivot on the path point:
/// dx = 5 + 200 - 100 - 3 = 102, dy = 0 + 50 - 40 - 0 = 10.
#[test]
fn follow_path_angle_wrap_and_align() {
    let s = follow_path_sample([3.0, 4.0], Some([-1.0, 0.0]), None, &[]);
    assert_eq!((s.x, s.y, s.angle), (3.0, 4.0, 180.0));
    let below = follow_path_sample([0.0, 0.0], Some([-1.0, -1e-12]), None, &[]);
    assert!(below.angle < -179.999 && below.angle > -180.0);
    assert_eq!(
        follow_path_sample([0.0; 2], Some([0.0, -1.0]), None, &[]).angle,
        -90.0
    );
    let fp = PathSample {
        x: 200.0,
        y: 50.0,
        angle: 90.0,
    };
    let base = mm(5.0, 0.0, 10.0, 1.0, 1.0, 3.0, 0.0);
    let m = apply_follow_path(Some(base), Some(fp), Some([100.0, 40.0]), true, 50.0).unwrap();
    assert_eq!((m.dx, m.dy, m.rot), (102.0, 10.0, 55.0));
    let unaligned = apply_follow_path(Some(base), Some(fp), None, false, 50.0).unwrap();
    assert_eq!(unaligned, base, "no bounds, no align: the mat is unchanged");
    let id = apply_follow_path(None, Some(fp), Some([100.0, 40.0]), false, 0.0).unwrap();
    assert_eq!((id.dx, id.dy, id.sx, id.op), (100.0, 10.0, 1.0, 1.0));
    assert_eq!(
        apply_follow_path(None, None, Some([1.0, 1.0]), true, 100.0),
        None
    );
}

/// D30 follow-path end: the percent clamps to [0, 100] and the offset to
/// [0, len]: 100% and 150% are the end (80), -20% the start, 37.5% of 80 = 30.
#[test]
fn follow_path_offset_clamps_to_the_ends() {
    assert_eq!(follow_path_offset(100.0, 80.0), Some(80.0));
    assert_eq!(follow_path_offset(150.0, 80.0), Some(80.0));
    assert_eq!(follow_path_offset(-20.0, 80.0), Some(0.0));
    assert_eq!(follow_path_offset(37.5, 80.0), Some(30.0));
}
