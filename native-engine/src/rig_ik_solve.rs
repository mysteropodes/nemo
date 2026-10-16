//! Two-bone IK solve and chain discovery kernel (I12), pure and I/O free.
//!
//! Mirrors the rig IK numerics of the baseline at fbe4b8a3: `rigAnchorPoint`,
//! `rigSetIK` and `rigDragIKEnd` (`src/js/app.js`) and `otherEnd` / `findIKChain`
//! plus the joint mirroring step of the Alt-drag (`src/js/rig-bridge.js`). The
//! kernel works on bone anchor POSITIONS only; it owns no rig record, bind,
//! weight, deformation (`applyRigDeform`), pose reset or commit.
//!
//! Boundaries kept on purpose:
//! - bones arrive as plain values in the baseline's `Object.keys(rig.bones)`
//!   order (`boneN` ids are never integer-like, so that is insertion order);
//!   the first coinciding bone in that order wins, so the order is an input;
//! - the solve returns NEW joint/end points. Nothing supplied is modified, and
//!   an unreachable target never moves the rest geometry;
//! - the legacy-write admission gate (`n20AllowLegacyWrite`) belongs to the
//!   JavaScript surface; the native owner is the authority and has no such gate.
//!
//! Semantics kept from the baseline:
//! - `rigAnchorPoint` needs the REST vertex to exist and then prefers the live
//!   vertex, falling back to the rest one (a pose may be shorter than the rest);
//! - `rigSetIK` stores `l1 = |joint - root|`, `l2 = |end - joint|`, `flip` and
//!   the key `endBone:endVi`; the lengths use V8's `Math.hypot`
//!   ([`hypot2`], max-scaled Kahan sum), which `f64::hypot` does not reproduce;
//! - `rigDragIKEnd` reads the ROOT from the live vertex, clamps the target
//!   distance into `[|l1-l2| + 1e-6, l1 + l2 - 1e-6]`, clamps `cosA` into
//!   `[-1, 1]` before `acos`, bends the joint to `base +/- a` (minus when
//!   `flip`) at distance `l1`, and puts the end on the base-angle ray at the
//!   CLAMPED distance, not on the raw target;
//! - the min/max are JavaScript `Math.min` / `Math.max` ([`js_min`],
//!   [`js_max`]): a NaN operand gives NaN, where `f64::min/max` would drop it;
//! - `findIKChain` accepts only an open end bone with 2+ vertices, only its
//!   first or last vertex, and takes the first other open 2+ vertex bone whose
//!   first or last vertex is within `< 2` (`Point.getDistance`) of the end
//!   bone's other end. A winning bone id that is the empty string is falsy in
//!   the baseline and does not stop the search.
//!
//! Deliberate differences, all typed:
//! - the baseline throws a `TypeError` when a chain vertex is missing, after it
//!   may already have written the joint; [`drag_ik_end`] checks the root, joint
//!   and end vertices first and returns [`IkError::SegmentMissing`] without any
//!   partial result;
//! - the baseline writes NaN into the rig when the target or a stored length is
//!   NaN, or when `0 / 0` arises (`l1 = 0` while `l2` is so large that the
//!   `1e-6` guard is absorbed); [`drag_ik_end`] returns
//!   [`IkError::NonFinitePose`] instead. [`solve_raw`] keeps the baseline
//!   arithmetic, NaN included, for characterisation. A finite degenerate pose
//!   (`l1 = 0` or `l2 = 0` with the ordinary guard, a target equal to the
//!   root, an infinite target) is the baseline's and is returned as is.

/// A point as `[x, y]`.
pub(crate) type Point2 = [f64; 2];

/// `1e-6` guard that keeps the clamped reach off the exact degenerate lengths.
const REACH_EPSILON: f64 = 1e-6;

/// `findIKChain`'s `tol`: "exact-enough bone-drawing coincidence".
const COINCIDENCE_TOLERANCE: f64 = 2.0;

/// One bone as the IK code reads it: its live and rest vertex positions.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct IkBone {
    pub(crate) id: String,
    pub(crate) closed: bool,
    pub(crate) segments: Vec<Point2>,
    pub(crate) rest_segments: Vec<Point2>,
}

/// `{boneId, vi}`: one vertex of one bone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IkRef {
    pub(crate) bone_id: String,
    pub(crate) vi: usize,
}

/// Which vertex of a chain a refusal names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IkSlot {
    Root,
    Joint,
    End,
    EndNear,
}

/// Typed refusal of an input the kernel will not guess at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IkError {
    /// `rigAnchorPoint` returned null for this reference (no bone, or no rest vertex).
    AnchorMissing(IkSlot),
    /// The live vertex the solve reads or writes does not exist.
    SegmentMissing(IkSlot),
    /// The baseline arithmetic produced a NaN or infinite joint or end.
    NonFinitePose,
}

/// The chain `rigSetIK` stores under [`chain_key`] of its end reference.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct IkChain {
    pub(crate) root: IkRef,
    pub(crate) joint: IkRef,
    pub(crate) end: IkRef,
    pub(crate) l1: f64,
    pub(crate) l2: f64,
    pub(crate) flip: bool,
}

impl IkChain {
    /// `endRef.boneId + ':' + endRef.vi`, the key `rigDragIKEnd` is called with.
    pub(crate) fn key(&self) -> String {
        chain_key(&self.end)
    }
}

/// `findIKChain`'s result: the roles of the four vertices of a two-bone chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IkChainSpec {
    pub(crate) root: IkRef,
    pub(crate) joint: IkRef,
    /// The end bone's own vertex coincident with `joint` (its near end).
    pub(crate) end_near: IkRef,
    pub(crate) end: IkRef,
}

/// The solved positions of the joint and the end effector.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct IkPose {
    pub(crate) joint: Point2,
    pub(crate) end: Point2,
}

/// One vertex position to write: an output, never applied here.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PointWrite {
    pub(crate) target: IkRef,
    pub(crate) point: Point2,
}

/// `endRef.boneId + ':' + endRef.vi`.
pub(crate) fn chain_key(end: &IkRef) -> String {
    format!("{}:{}", end.bone_id, end.vi)
}

/// V8's `Math.hypot` for two arguments (max-scaled Kahan sum), the engine the
/// oracle came from; `f64::hypot` rounds differently by up to 1 ulp. IEEE
/// arithmetic plus `sqrt` only, so it is exact on every host.
pub(crate) fn hypot2(a: f64, b: f64) -> f64 {
    let (a, b) = (a.abs(), b.abs());
    if a == f64::INFINITY || b == f64::INFINITY {
        return f64::INFINITY;
    }
    if a.is_nan() || b.is_nan() {
        return f64::NAN;
    }
    let max = a.max(b);
    if max == 0.0 {
        return 0.0;
    }
    let (mut sum, mut comp) = (0.0f64, 0.0f64);
    for v in [a, b] {
        let n = v / max;
        let summand = n * n - comp;
        let prelim = sum + summand;
        comp = (prelim - sum) - summand;
        sum = prelim;
    }
    sum.sqrt() * max
}

/// JavaScript `Math.max`: NaN if either operand is NaN, `+0` above `-0`.
pub(crate) fn js_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        return f64::NAN;
    }
    if a == 0.0 && b == 0.0 {
        return if a.is_sign_negative() { b } else { a };
    }
    if a > b {
        a
    } else {
        b
    }
}

/// JavaScript `Math.min`: NaN if either operand is NaN, `-0` below `+0`.
pub(crate) fn js_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        return f64::NAN;
    }
    if a == 0.0 && b == 0.0 {
        return if a.is_sign_negative() { a } else { b };
    }
    if a < b {
        a
    } else {
        b
    }
}

fn bone_of<'a>(bones: &'a [IkBone], id: &str) -> Option<&'a IkBone> {
    bones.iter().find(|bone| bone.id == id)
}

/// `rigAnchorPoint`: the live vertex, or the rest one, but only when the rest
/// vertex exists. `None` is the baseline's `null`.
pub(crate) fn anchor_point(bones: &[IkBone], reference: &IkRef) -> Option<Point2> {
    let bone = bone_of(bones, &reference.bone_id)?;
    let rest = bone.rest_segments.get(reference.vi)?;
    Some(*bone.segments.get(reference.vi).unwrap_or(rest))
}

/// `rigSetIK`: the chain to store, or the first of root/joint/end that has no
/// anchor (the baseline returns `false` without saying which).
pub(crate) fn set_ik(
    bones: &[IkBone],
    root: &IkRef,
    joint: &IkRef,
    end: &IkRef,
    flip: bool,
) -> Result<IkChain, IkError> {
    let r = anchor_point(bones, root).ok_or(IkError::AnchorMissing(IkSlot::Root))?;
    let j = anchor_point(bones, joint).ok_or(IkError::AnchorMissing(IkSlot::Joint))?;
    let e = anchor_point(bones, end).ok_or(IkError::AnchorMissing(IkSlot::End))?;
    Ok(IkChain {
        root: root.clone(),
        joint: joint.clone(),
        end: end.clone(),
        l1: hypot2(j[0] - r[0], j[1] - r[1]),
        l2: hypot2(e[0] - j[0], e[1] - j[1]),
        flip,
    })
}

/// The law-of-cosines arithmetic of `rigDragIKEnd`, exactly as written, so a
/// degenerate chain or a NaN target propagates NaN as it does in the baseline.
pub(crate) fn solve_raw(root: Point2, l1: f64, l2: f64, flip: bool, target: Point2) -> IkPose {
    let dx = target[0] - root[0];
    let dy = target[1] - root[1];
    let dist = hypot2(dx, dy);
    let max_d = l1 + l2;
    let min_d = (l1 - l2).abs();
    let clamped = js_max(min_d + REACH_EPSILON, js_min(max_d - REACH_EPSILON, dist));
    let base_angle = dy.atan2(dx);
    let cos_a = js_max(
        -1.0,
        js_min(
            1.0,
            (l1 * l1 + clamped * clamped - l2 * l2) / (2.0 * l1 * clamped),
        ),
    );
    let a = cos_a.acos();
    let joint_angle = base_angle + if flip { -a } else { a };
    IkPose {
        joint: [
            root[0] + l1 * joint_angle.cos(),
            root[1] + l1 * joint_angle.sin(),
        ],
        end: [
            root[0] + clamped * base_angle.cos(),
            root[1] + clamped * base_angle.sin(),
        ],
    }
}

fn live_point(bones: &[IkBone], reference: &IkRef) -> Option<Point2> {
    bone_of(bones, &reference.bone_id)?
        .segments
        .get(reference.vi)
        .copied()
}

/// `rigDragIKEnd` as a pure function: the joint and end positions for `target`.
/// The root is the chain's LIVE root vertex; the stored `l1`/`l2` are used, not
/// the current bone lengths. The root, joint and end vertices must all exist
/// (checked before anything is computed, so there is no partial result) and the
/// result must be finite.
pub(crate) fn drag_ik_end(
    bones: &[IkBone],
    chain: &IkChain,
    target: Point2,
) -> Result<IkPose, IkError> {
    let root = live_point(bones, &chain.root).ok_or(IkError::SegmentMissing(IkSlot::Root))?;
    live_point(bones, &chain.joint).ok_or(IkError::SegmentMissing(IkSlot::Joint))?;
    live_point(bones, &chain.end).ok_or(IkError::SegmentMissing(IkSlot::End))?;
    let pose = solve_raw(root, chain.l1, chain.l2, chain.flip, target);
    if pose
        .joint
        .iter()
        .chain(pose.end.iter())
        .all(|v| v.is_finite())
    {
        Ok(pose)
    } else {
        Err(IkError::NonFinitePose)
    }
}

/// The writes of one Alt-drag step (`rig-bridge.js` onMove): the joint, the end,
/// then the SOLVED joint copied onto the end bone's coincident near vertex (the
/// bones have no parent link, so the bridge keeps the elbow together by hand).
pub(crate) fn drag_writes(
    bones: &[IkBone],
    spec: &IkChainSpec,
    chain: &IkChain,
    target: Point2,
) -> Result<Vec<PointWrite>, IkError> {
    let pose = drag_ik_end(bones, chain, target)?;
    live_point(bones, &spec.end_near).ok_or(IkError::SegmentMissing(IkSlot::EndNear))?;
    Ok(vec![
        PointWrite {
            target: chain.joint.clone(),
            point: pose.joint,
        },
        PointWrite {
            target: chain.end.clone(),
            point: pose.end,
        },
        PointWrite {
            target: spec.end_near.clone(),
            point: pose.joint,
        },
    ])
}

/// `otherEnd`: the opposite end of a bone with `len` vertices.
fn other_end(len: usize, idx: usize) -> usize {
    if idx == 0 {
        len - 1
    } else {
        0
    }
}

/// `Point.getDistance`: `sqrt(dx * dx + dy * dy)`.
fn distance(a: Point2, b: Point2) -> f64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    (dx * dx + dy * dy).sqrt()
}

/// `findIKChain(ld, boneId, vi)`: the chain whose far end is vertex `vi` of
/// bone `bone_id`, or `None` (the baseline's `null`).
pub(crate) fn find_ik_chain(bones: &[IkBone], bone_id: &str, vi: usize) -> Option<IkChainSpec> {
    let end_bone = bone_of(bones, bone_id)?;
    if end_bone.closed || end_bone.segments.len() < 2 {
        return None;
    }
    let len = end_bone.segments.len();
    if vi != 0 && vi != len - 1 {
        return None;
    }
    let near_idx = other_end(len, vi);
    let near = end_bone.segments[near_idx];
    // `rootBoneId` is a string: the empty id is falsy and does not end the search.
    let mut root: Option<(&IkBone, usize)> = None;
    let is_set = |r: &Option<(&IkBone, usize)>| r.map_or(false, |(b, _)| !b.id.is_empty());
    for bone in bones {
        if bone.id == bone_id || is_set(&root) || bone.closed || bone.segments.len() < 2 {
            continue;
        }
        for i in [0, bone.segments.len() - 1] {
            if is_set(&root) {
                continue;
            }
            if distance(bone.segments[i], near) < COINCIDENCE_TOLERANCE {
                root = Some((bone, i));
            }
        }
    }
    let (root_bone, joint_idx) = root.filter(|(b, _)| !b.id.is_empty())?;
    let make = |id: &str, vi: usize| IkRef {
        bone_id: id.to_string(),
        vi,
    };
    Some(IkChainSpec {
        root: make(
            &root_bone.id,
            other_end(root_bone.segments.len(), joint_idx),
        ),
        joint: make(&root_bone.id, joint_idx),
        end_near: make(bone_id, near_idx),
        end: make(bone_id, vi),
    })
}

#[cfg(test)]
#[path = "../tests/rig_ik_solve_controls.rs"]
mod controls_tests;
#[cfg(test)]
#[path = "../tests/rig_ik_solve_find.rs"]
mod find_tests;
#[cfg(test)]
#[path = "../tests/rig_ik_solve_rows.rs"]
mod oracle_rows;
#[cfg(test)]
#[path = "../tests/rig_ik_solve.rs"]
mod tests;
