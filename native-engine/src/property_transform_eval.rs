//! Pure Motion property-transform routing (D30/#2075, first provider).
//!
//! The routing layer around `motion_parent_chain`: which parents a layer
//! blends and how they are weighted, which of the two parent-chain paths
//! runs, how a sampled follow-path point lands on a layer's Motion mat, and
//! the clamp and angle arithmetic of a follow-path sample. It mirrors
//! `src/js/motion.js`: `weightedParentsOf`, the dispatch of
//! `blendedAncestorMat`, the dispatch of `parentChainMats`, the follow-path
//! block of `layerMotionAt`, and the clamp and angle steps of
//! `sampleFollowPathAt`. The numeric parent-chain primitives (blends,
//! transforms) come from `super::motion_parent_chain`, untouched.
//!
//! The caller owns layers, uids, keyframe sampling and Paper objects; this
//! module receives already-resolved values and closures and never reads a
//! document, a context or a global. A JavaScript `undefined`/empty sample
//! (`valueAtFrame(...)[0]` of nothing) is passed as NaN: every consumer here
//! reads it through `|| 0`, so NaN, 0 and -0 all become `+0`.
//!
//! Out of scope, left as follow-ups:
//! - the Paper arc-length sampler (`path.length`, `getPointAt`,
//!   `getTangentAt`) of `sampleFollowPathAt`, and its target lookup
//!   (`findLayerIndexByUid`, `getEffectiveStrokes`, the first stroke with
//!   more than one segment, the layer's Paper bounds);
//! - the blended 3D contribution that follows `blendedAncestorMat`;
//! - the caller-side sampling of `parentBlend`, `parentWeight{i}`,
//!   `pathPercent` and `pathInfluence`, and the `followPath.targetLayerUid`
//!   precondition (without it the caller passes no sample).

use crate::evaluation::motion_parent_chain::{
    apply_chain_to_segments, blend_two, blend_weighted, parent_weights, transform_segments,
    ChainEntry, MotionMat, Pose, Segment,
};
use std::f64::consts::PI;

/// One `{uid, w}` part of `weightedParentsOf`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct WeightedParent<'a> {
    pub uid: Option<&'a str>,
    pub weight: f64,
}

/// A sampled follow-path point and tangent angle (degrees) in world space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PathSample {
    pub x: f64,
    pub y: f64,
    pub angle: f64,
}

/// `weightedParentsOf`: `None` when `parentsMore` is empty. Otherwise A at
/// `1 - t`, B at `t`, then each extra with a non-empty uid at its absolute
/// percent. `extras` holds every `parentsMore` entry in stored order (`None`
/// for a null entry or one without a uid; an empty uid is skipped too);
/// `weight_samples[i]` is the sampled `parentWeight{i}` of the ORIGINAL
/// index `i` (a missing sample reads as NaN, i.e. 0).
pub(crate) fn weighted_parents_of<'a>(
    uid_a: Option<&'a str>,
    uid_b: Option<&'a str>,
    parent_blend: f64,
    extras: &[Option<&'a str>],
    weight_samples: &[f64],
) -> Option<Vec<WeightedParent<'a>>> {
    if extras.is_empty() {
        return None;
    }
    let mut uids = vec![uid_a, uid_b];
    let mut samples = Vec::new();
    for (i, e) in extras.iter().enumerate() {
        if let Some(u) = e.filter(|u| !u.is_empty()) {
            uids.push(Some(u));
            samples.push(weight_samples.get(i).copied().unwrap_or(f64::NAN));
        }
    }
    let w = parent_weights(parent_blend, &samples);
    Some(
        uids.into_iter()
            .zip(w)
            .map(|(uid, weight)| WeightedParent { uid, weight })
            .collect(),
    )
}

/// `blendedAncestorMat`: `None` unless both A and B are set (non-empty).
/// With any `parentsMore` entry the weighted blend runs, otherwise the
/// two-parent blend (an identity pose stands in for one missing parent).
/// `compose` is `composeChainTransform(uid)`, called exactly where
/// JavaScript calls it (two-parent: A then B; weighted: nonzero parts).
pub(crate) fn blended_ancestor_mat(
    uid_a: Option<&str>,
    uid_b: Option<&str>,
    parent_blend: f64,
    extras: &[Option<&str>],
    weight_samples: &[f64],
    mut compose: impl FnMut(&str) -> Option<Pose>,
) -> Option<MotionMat> {
    let (a, b) = (
        uid_a.filter(|u| !u.is_empty())?,
        uid_b.filter(|u| !u.is_empty())?,
    );
    if let Some(parts) = weighted_parents_of(Some(a), Some(b), parent_blend, extras, weight_samples)
    {
        // `blendWeighted` asks for a pose only where the normalized weight is
        // truthy, and only for the first part once the sum falls back.
        let mut sum = 0.0;
        for p in &parts {
            sum += p.weight;
        }
        let fallback = sum.is_nan() || sum <= 1e-9;
        let parts: Vec<(f64, Option<Pose>)> = parts
            .into_iter()
            .enumerate()
            .map(|(i, p)| {
                let wanted = if fallback {
                    i == 0
                } else {
                    let w = p.weight / sum;
                    w != 0.0 && !w.is_nan()
                };
                (
                    p.weight,
                    wanted.then(|| compose(p.uid.unwrap_or(""))).flatten(),
                )
            })
            .collect();
        return blend_weighted(&parts);
    }
    let pa = compose(a);
    let pb = compose(b);
    blend_two(pa, pb, parent_blend)
}

/// `parentChainMats`: with a B parent the chain is the one blended entry
/// pivoted at the origin (or empty); otherwise the legacy chain. Only the
/// taken branch runs.
pub(crate) fn parent_chain_mats(
    uid_b: Option<&str>,
    blended: impl FnOnce() -> Option<MotionMat>,
    legacy: impl FnOnce() -> Vec<ChainEntry>,
) -> Vec<ChainEntry> {
    if uid_b.is_some_and(|u| !u.is_empty()) {
        return blended()
            .map(|mat| {
                vec![ChainEntry {
                    mat,
                    pivot: [0.0, 0.0],
                }]
            })
            .unwrap_or_default();
    }
    legacy()
}

/// The follow-path block of `layerMotionAt`, given the layer's own mat
/// (`computeMotionMat`) and the already-sampled path point. `None` sample:
/// the plain mat. Otherwise a missing mat becomes the identity; Paper
/// bounds (`bounds_center`) shift the mat so the pivot lands on the path
/// point (additive, past the anchor); `align` adds the tangent angle scaled
/// by `pathInfluence` (a raw percent read through `|| 0`).
pub(crate) fn apply_follow_path(
    mat: Option<MotionMat>,
    sample: Option<PathSample>,
    bounds_center: Option<[f64; 2]>,
    align: bool,
    path_influence: f64,
) -> Option<MotionMat> {
    let Some(fp) = sample else {
        return mat;
    };
    let mut m = mat.unwrap_or(MotionMat {
        dx: 0.0,
        dy: 0.0,
        rot: 0.0,
        sx: 1.0,
        sy: 1.0,
        op: 1.0,
        ax: 0.0,
        ay: 0.0,
    });
    if let Some(c) = bounds_center {
        m.dx += fp.x - c[0] - m.ax;
        m.dy += fp.y - c[1] - m.ay;
    }
    if align {
        m.rot += fp.angle * (or0(path_influence) / 100.0);
    }
    Some(m)
}

/// The clamp of `sampleFollowPathAt`: the arc-length offset at which to read
/// the point and tangent, `per = clamp(pathPercent || 0, 0, 100) / 100`,
/// `offset = clamp(per * len, 0, len)`. `None` for a zero (or NaN) length.
pub(crate) fn follow_path_offset(path_percent: f64, len: f64) -> Option<f64> {
    if falsy(len) {
        return None;
    }
    let per = js_max(0.0, js_min(100.0, or0(path_percent))) / 100.0;
    Some(js_max(0.0, js_min(len, per * len)))
}

/// The transform step of `sampleFollowPathAt`: the point and a second point
/// 10 units along the tangent (`(10, 0)` without one) ride through the
/// target's own mat (pivot at its bounds centre plus anchor, only when both
/// the mat and the bounds exist) and then its parent chain.
pub(crate) fn follow_path_points(
    point: [f64; 2],
    tangent: Option<[f64; 2]>,
    target: Option<(MotionMat, [f64; 2])>,
    chain: &[ChainEntry],
) -> [[f64; 2]; 2] {
    let seg = |x: f64, y: f64| Segment {
        point: [x, y],
        handle_in: Some([0.0, 0.0]),
        handle_out: Some([0.0, 0.0]),
    };
    let second = match tangent {
        Some(t) => [point[0] + t[0] * 10.0, point[1] + t[1] * 10.0],
        None => [point[0] + 10.0, point[1]],
    };
    let mut pts = vec![seg(point[0], point[1]), seg(second[0], second[1])];
    if let Some((mat, centre)) = target {
        let pivot = [centre[0] + mat.ax, centre[1] + mat.ay];
        pts = transform_segments(&pts, pivot, &mat);
    }
    let pts = apply_chain_to_segments(&pts, chain);
    [pts[0].point, pts[1].point]
}

/// The angle step of `sampleFollowPathAt`: the first transformed point and
/// the angle of the pair in degrees.
pub(crate) fn follow_path_sample(
    point: [f64; 2],
    tangent: Option<[f64; 2]>,
    target: Option<(MotionMat, [f64; 2])>,
    chain: &[ChainEntry],
) -> PathSample {
    let [p, q] = follow_path_points(point, tangent, target, chain);
    PathSample {
        x: p[0],
        y: p[1],
        angle: (q[1] - p[1]).atan2(q[0] - p[0]) * 180.0 / PI,
    }
}

/// JavaScript falsiness of a number: 0, -0 and NaN.
fn falsy(x: f64) -> bool {
    x == 0.0 || x.is_nan()
}

/// JavaScript `x || 0`.
fn or0(x: f64) -> f64 {
    if falsy(x) {
        0.0
    } else {
        x
    }
}

/// `Math.max(a, b)`: NaN propagates and `+0` beats `-0`.
fn js_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a == 0.0 && b == 0.0 {
        if a.is_sign_positive() {
            a
        } else {
            b
        }
    } else if a > b {
        a
    } else {
        b
    }
}

/// `Math.min(a, b)`: NaN propagates and `-0` beats `+0`.
fn js_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a == 0.0 && b == 0.0 {
        if a.is_sign_negative() {
            a
        } else {
            b
        }
    } else if a < b {
        a
    } else {
        b
    }
}
