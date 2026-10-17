//! The rig callers of `Path.getNearestLocation` in the baseline at fbe4b8a3 (I12), as pure functions over
//! plain bone values: `rigWeighOnePoint` (the legacy offset-model weigher), the radius scan of
//! `rigAutoAssignLayer` and the add-bone row of the weight popover (`rig-bridge.js`).
//!
//! Each caller builds a throwaway Paper path from `bone.restSegments` with `_boneSegsToPath` (every
//! handle defaults to `[0, 0]`, `closed` from the bone) and asks it for the nearest location; here the
//! curves come from `property_follow_path_sample::path_curves` and the location from
//! [`nearest_on_curves`]. Semantics kept from the baseline:
//! - `rigWeighOnePoint` visits `boneIds` in order, skips a bone the rig lacks and a bone with no nearest
//!   location, measures `dist` from the LOCATION'S POINT (not the stored distance), reaches
//!   `bone.radius || radius` (zero, NaN and absent bone radii fall back), takes the taper
//!   `max(0, 1 - dist / reach)` BEFORE the softness blend (which runs only for a truthy softness), and
//!   emits `{boneId, offset: location.offset, w}` only for `w > 0`. It has no `radius || 200` default and
//!   no softness clamp: the bind pass in front of it owns both (`rig_pose_weights::bind_radius`,
//!   `clamp_softness`), and the manual reset button passes the panel values straight through;
//! - the radius scan starts at `defaultRadius || 200`, skips a bone whose `radius` is truthy BEFORE
//!   building its path, takes the largest `dist` over every point and every remaining bone (path
//!   vertices first, then mesh world vertices; the maximum does not depend on the order), and returns
//!   `Math.ceil(needed * 1.05)`; a rig without bones returns before the scan (`0` strokes bound), so the
//!   kernel returns `None` there;
//! - the add-bone row stores `loc ? loc.offset : 0`.
//!
//! Deliberate differences, all typed: a bone whose `restSegments` is absent makes `_boneSegsToPath` throw
//! a TypeError, and a winning location without a point (see `rig_pose_nearest`) makes the callers throw
//! or store nonsense; both are [`BoneNearestError`] and nothing partial is returned. A segment with no
//! `.point` is not representable ([`SegmentIn`] requires one): the record layer refuses it at admission.

use crate::evaluation::object_image_mesh_kernels::rig_pose_nearest::{
    nearest_on_curves, NearestError, NearestLocation,
};
use crate::evaluation::object_image_mesh_kernels::rig_pose_nearest_curve::Values;
use crate::evaluation::object_image_mesh_kernels::rig_pose_weights::{
    bind_radius, effective_radius, influence, OffsetWeight, WeightBone,
};
use crate::evaluation::property_follow_path_sample::{path_curves, SegmentIn};

/// A bone as the offset-model callers read it: `rest_segments` is `bone.restSegments` (`None` when the
/// property is absent), `radius` is `bone.radius` (`None`, `0` and NaN are all falsy).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct NearestBone {
    pub(crate) id: String,
    pub(crate) radius: Option<f64>,
    pub(crate) closed: bool,
    pub(crate) rest_segments: Option<Vec<SegmentIn>>,
}

/// Where a baseline caller would throw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BoneNearestError {
    /// `_boneSegsToPath(undefined, ...)`: the bone has no `restSegments`.
    MissingRestSegments { bone_id: String },
    /// The winning location has no point.
    Unresolved {
        bone_id: String,
        error: NearestError,
    },
}

/// `_boneSegsToPath(bone.restSegments, bone.closed)` as curves.
fn rest_curves(bone: &NearestBone) -> Result<Vec<Values>, BoneNearestError> {
    let segs =
        bone.rest_segments
            .as_deref()
            .ok_or_else(|| BoneNearestError::MissingRestSegments {
                bone_id: bone.id.clone(),
            })?;
    Ok(path_curves(segs, bone.closed))
}

/// `bp.getNearestLocation(point)` on the rest path of one bone.
fn locate(bone: &NearestBone, p: [f64; 2]) -> Result<Option<NearestLocation>, BoneNearestError> {
    let curves = rest_curves(bone)?;
    nearest_on_curves(&curves, bone.closed, p).map_err(|error| BoneNearestError::Unresolved {
        bone_id: bone.id.clone(),
        error,
    })
}

/// `rigWeighOnePoint(rig, boneIds, p, radius, softness)`: the offset-model weights of one vertex.
/// `radius` and `softness` are used as given (an undefined radius is NaN, an undefined softness 0).
pub(crate) fn weigh_one_point(
    bones: &[NearestBone],
    bone_ids: &[String],
    p: [f64; 2],
    radius: f64,
    softness: f64,
) -> Result<Vec<OffsetWeight>, BoneNearestError> {
    let mut out = Vec::new();
    for bid in bone_ids {
        let Some(bone) = bones.iter().find(|b| &b.id == bid) else {
            continue;
        };
        let Some(loc) = locate(bone, p)? else {
            continue;
        };
        let (dx, dy) = (p[0] - loc.point[0], p[1] - loc.point[1]);
        let dist = (dx * dx + dy * dy).sqrt();
        let reach = effective_radius(
            &WeightBone {
                id: String::new(),
                radius: bone.radius,
                segments: None,
                rest_segments: None,
            },
            radius,
        );
        let infl = influence(dist, reach, softness);
        if infl > 0.0 {
            out.push(OffsetWeight {
                bone_id: bid.clone(),
                offset: Some(loc.offset),
                w: infl,
            });
        }
    }
    Ok(out)
}

/// JavaScript truthiness of `bone.radius`.
fn has_radius(radius: Option<f64>) -> bool {
    matches!(radius, Some(r) if r != 0.0 && !r.is_nan())
}

/// The radius scan of `rigAutoAssignLayer`: `Math.ceil(needed * 1.05)` where `needed` starts at
/// `defaultRadius || 200` and grows to the farthest nearest-location distance of any point to any bone
/// that has no radius of its own. `None` for a rig without bones (the function returns before the scan).
pub(crate) fn auto_assign_radius(
    default_radius: Option<f64>,
    bones: &[NearestBone],
    points: &[[f64; 2]],
) -> Result<Option<f64>, BoneNearestError> {
    if bones.is_empty() {
        return Ok(None);
    }
    let mut needed = bind_radius(default_radius);
    let mut cache: Vec<Option<Vec<Values>>> = vec![None; bones.len()];
    for p in points {
        for (k, bone) in bones.iter().enumerate() {
            if has_radius(bone.radius) {
                continue;
            }
            if cache[k].is_none() {
                cache[k] = Some(rest_curves(bone)?);
            }
            let curves = cache[k].as_deref().unwrap_or(&[]);
            let found = nearest_on_curves(curves, bone.closed, *p).map_err(|error| {
                BoneNearestError::Unresolved {
                    bone_id: bone.id.clone(),
                    error,
                }
            })?;
            let Some(loc) = found else {
                continue;
            };
            let (dx, dy) = (p[0] - loc.point[0], p[1] - loc.point[1]);
            let dist = (dx * dx + dy * dy).sqrt();
            if dist > needed {
                needed = dist;
            }
        }
    }
    Ok(Some((needed * 1.05).ceil()))
}

/// The add-bone row of the weight popover: `loc ? loc.offset : 0` for the vertex's rest point.
pub(crate) fn add_bone_offset(
    bone: &NearestBone,
    rest_pt: [f64; 2],
) -> Result<f64, BoneNearestError> {
    Ok(locate(bone, rest_pt)?.map_or(0.0, |loc| loc.offset))
}

#[cfg(test)]
#[path = "../tests/rig_pose_nearest_weigh_controls.rs"]
mod controls_tests;
#[cfg(test)]
#[path = "../tests/rig_pose_nearest_weigh.rs"]
mod tests;
