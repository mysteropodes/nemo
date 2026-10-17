//! Read-queries facade for the rig weight, nearest-location and IK kernels (I12); no state, no
//! publication authority.
//!
//! The command layer may not depend on the evaluation layer (`commands` allows `read-queries`,
//! `read-queries` allows `evaluation`), so the rig edit transaction reaches the kernels through the
//! five functions below, exactly like `ObjectSnapshot::sample_property_raw` reaches the raw sampler:
//! - [`weigh_control`]: `rigWeighOnePointByControl` over every point of a bind pass
//!   (`weighForPoints` of `rigBindStroke`, with its `radius || 200` and softness clamp);
//! - [`weigh_offset`]: `rigWeighOnePoint`, the legacy offset model the weight popover's reset button
//!   calls (`rig-bridge.js`);
//! - [`auto_radius`]: the radius scan of `rigAutoAssignLayer`;
//! - [`offset_for`]: the popover's add-bone row (`loc ? loc.offset : 0`);
//! - [`ik_lengths`]: the two lengths `rigSetIK` stores.
//!
//! Plain numbers in, plain numbers out: the stored record types belong to the document layer and are
//! converted by the plan module of `object_rig_edit.rs` (command layer). Every input is borrowed and never
//! changed, every result is a new owned value, so the caller publishes it (or drops it) atomically.
//! Without the `evaluation` feature there is no kernel: each function reports
//! [`KernelRefusal::Unavailable`]; nothing is defaulted, guessed or approximated.

/// One bone vertex as the kernels read it: the anchor and its two handle vectors (zero when the
/// record has no handle, exactly what `_boneSegsToPath` makes of a missing one).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct KernelVertex {
    pub(crate) point: [f64; 2],
    pub(crate) handle_in: [f64; 2],
    pub(crate) handle_out: [f64; 2],
}

/// One bone: `radius` is `bone.radius` (`None`, zero and NaN are all falsy in the baseline), `live`
/// the pose (`segments`) and `rest` the bind pose (`restSegments`).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct KernelBone {
    pub(crate) id: String,
    pub(crate) radius: Option<f64>,
    pub(crate) closed: bool,
    pub(crate) live: Vec<KernelVertex>,
    pub(crate) rest: Vec<KernelVertex>,
}

/// `{boneId, vi}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct KernelRef {
    pub(crate) bone_id: String,
    pub(crate) vi: u32,
}

/// A control-point weight `{boneId, ptIndex, w}`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ControlEntry {
    pub(crate) bone_id: String,
    pub(crate) pt_index: usize,
    pub(crate) w: f64,
}

/// A legacy offset weight `{boneId, offset, w}`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct OffsetEntry {
    pub(crate) bone_id: String,
    pub(crate) offset: f64,
    pub(crate) w: f64,
}

/// Why a kernel answer is not available.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "evaluation", allow(dead_code))]
pub(crate) enum KernelRefusal {
    /// The evaluation layer is not part of this build.
    Unavailable,
    /// The nearest-location kernel found no point (the baseline would throw or store nonsense).
    Unresolved,
    /// A chain reference names no bone or no rest vertex (`rigSetIK` returns `false`).
    AnchorMissing,
}

/// `rigWeighOnePointByControl` for every point; `radius` and `softness` are the raw bind arguments
/// (`radius || 200`, `Math.max(0, Math.min(1, softness || 0))` are applied here).
#[cfg_attr(not(feature = "evaluation"), allow(unused_variables))]
pub(crate) fn weigh_control(
    bones: &[KernelBone],
    bone_ids: &[String],
    points: &[[f64; 2]],
    radius: Option<f64>,
    softness: Option<f64>,
) -> Result<Vec<Vec<ControlEntry>>, KernelRefusal> {
    #[cfg(feature = "evaluation")]
    {
        use crate::evaluation::object_image_mesh_kernels::rig_pose_weights::{
            weigh_bind_points, WeightBone,
        };
        let weight_bones: Vec<WeightBone> = bones
            .iter()
            .map(|bone| WeightBone {
                id: bone.id.clone(),
                radius: bone.radius,
                segments: Some(bone.live.iter().map(|v| Some(v.point)).collect()),
                rest_segments: Some(bone.rest.iter().map(|v| Some(v.point)).collect()),
            })
            .collect();
        let lists = weigh_bind_points(&weight_bones, bone_ids, points, radius, softness);
        Ok(lists
            .into_iter()
            .map(|list| {
                list.into_iter()
                    .map(|entry| ControlEntry {
                        bone_id: entry.bone_id,
                        pt_index: entry.pt_index,
                        w: entry.w,
                    })
                    .collect()
            })
            .collect())
    }
    #[cfg(not(feature = "evaluation"))]
    {
        Err(KernelRefusal::Unavailable)
    }
}

/// The bones as the nearest-location callers read them (`_boneSegsToPath(restSegments, closed)`).
#[cfg(feature = "evaluation")]
fn nearest_bones(
    bones: &[KernelBone],
) -> Vec<crate::evaluation::object_image_mesh_kernels::rig_pose_nearest_weigh::NearestBone> {
    use crate::evaluation::object_image_mesh_kernels::rig_pose_nearest_weigh::NearestBone;
    use crate::evaluation::property_follow_path_sample::SegmentIn;
    bones
        .iter()
        .map(|bone| NearestBone {
            id: bone.id.clone(),
            radius: bone.radius,
            closed: bone.closed,
            rest_segments: Some(
                bone.rest
                    .iter()
                    .map(|v| SegmentIn {
                        point: v.point,
                        handle_in: Some(v.handle_in),
                        handle_out: Some(v.handle_out),
                    })
                    .collect(),
            ),
        })
        .collect()
}

/// `rigWeighOnePoint(rig, boneIds, p, radius, softness)`: the offset-model weights of one vertex. The
/// popover passes `parseFloat(radius) || 200` and the raw softness, so `radius` takes the `|| 200`
/// here and `softness` is used as given.
#[cfg_attr(not(feature = "evaluation"), allow(unused_variables))]
pub(crate) fn weigh_offset(
    bones: &[KernelBone],
    bone_ids: &[String],
    point: [f64; 2],
    radius: Option<f64>,
    softness: f64,
) -> Result<Vec<OffsetEntry>, KernelRefusal> {
    #[cfg(feature = "evaluation")]
    {
        use crate::evaluation::object_image_mesh_kernels::rig_pose_nearest_weigh::weigh_one_point;
        use crate::evaluation::object_image_mesh_kernels::rig_pose_weights::bind_radius;
        let entries = weigh_one_point(
            &nearest_bones(bones),
            bone_ids,
            point,
            bind_radius(radius),
            softness,
        )
        .map_err(|_| KernelRefusal::Unresolved)?;
        entries
            .into_iter()
            .map(|entry| {
                entry
                    .offset
                    .map(|offset| OffsetEntry {
                        bone_id: entry.bone_id,
                        offset,
                        w: entry.w,
                    })
                    .ok_or(KernelRefusal::Unresolved)
            })
            .collect()
    }
    #[cfg(not(feature = "evaluation"))]
    {
        Err(KernelRefusal::Unavailable)
    }
}

/// The radius scan of `rigAutoAssignLayer`: `Math.ceil(needed * 1.05)` where `needed` starts at
/// `defaultRadius || 200` and grows to the farthest nearest-location distance of any point to any bone
/// without a radius of its own. `None` for a rig without bones (the baseline returns before the scan).
#[cfg_attr(not(feature = "evaluation"), allow(unused_variables))]
pub(crate) fn auto_radius(
    default_radius: Option<f64>,
    bones: &[KernelBone],
    points: &[[f64; 2]],
) -> Result<Option<f64>, KernelRefusal> {
    #[cfg(feature = "evaluation")]
    {
        use crate::evaluation::object_image_mesh_kernels::rig_pose_nearest_weigh::auto_assign_radius;
        auto_assign_radius(default_radius, &nearest_bones(bones), points)
            .map_err(|_| KernelRefusal::Unresolved)
    }
    #[cfg(not(feature = "evaluation"))]
    {
        Err(KernelRefusal::Unavailable)
    }
}

/// The add-bone row of the weight popover: `loc ? loc.offset : 0` for the vertex's rest point.
#[cfg_attr(not(feature = "evaluation"), allow(unused_variables))]
pub(crate) fn offset_for(bone: &KernelBone, rest_point: [f64; 2]) -> Result<f64, KernelRefusal> {
    #[cfg(feature = "evaluation")]
    {
        use crate::evaluation::object_image_mesh_kernels::rig_pose_nearest_weigh::add_bone_offset;
        let wrapped = nearest_bones(std::slice::from_ref(bone));
        let first = wrapped.first().ok_or(KernelRefusal::Unresolved)?;
        add_bone_offset(first, rest_point).map_err(|_| KernelRefusal::Unresolved)
    }
    #[cfg(not(feature = "evaluation"))]
    {
        Err(KernelRefusal::Unavailable)
    }
}

/// `rigSetIK`: `[l1, l2]`, the two lengths measured on the live pose (`rigAnchorPoint` prefers the
/// live vertex and needs the rest vertex to exist).
#[cfg_attr(not(feature = "evaluation"), allow(unused_variables))]
pub(crate) fn ik_lengths(
    bones: &[KernelBone],
    root: &KernelRef,
    joint: &KernelRef,
    end: &KernelRef,
) -> Result<[f64; 2], KernelRefusal> {
    #[cfg(feature = "evaluation")]
    {
        use crate::evaluation::object_image_mesh_kernels::rig_ik_solve::{
            set_ik, IkBone, IkError, IkRef,
        };
        let ik_bones: Vec<IkBone> = bones
            .iter()
            .map(|bone| IkBone {
                id: bone.id.clone(),
                closed: bone.closed,
                segments: bone.live.iter().map(|v| v.point).collect(),
                rest_segments: bone.rest.iter().map(|v| v.point).collect(),
            })
            .collect();
        let to_ref = |reference: &KernelRef| IkRef {
            bone_id: reference.bone_id.clone(),
            vi: reference.vi as usize,
        };
        match set_ik(
            &ik_bones,
            &to_ref(root),
            &to_ref(joint),
            &to_ref(end),
            false,
        ) {
            Ok(chain) => Ok([chain.l1, chain.l2]),
            Err(IkError::AnchorMissing(_)) => Err(KernelRefusal::AnchorMissing),
            Err(_) => Err(KernelRefusal::Unresolved),
        }
    }
    #[cfg(not(feature = "evaluation"))]
    {
        Err(KernelRefusal::Unavailable)
    }
}
