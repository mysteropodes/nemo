//! Conversion of the stored rig record into the plain values the deform kernel takes (I12).
//!
//! The kernel (`rig_pose_deform`) and the weight model (`rig_pose_weights`) are pure and know no record
//! type; the record (`object_rig_record`) knows no kernel. This file is the one place that maps one to
//! the other, field by field, so the join cannot drift from either side:
//! - a bone becomes a [`DeformBone`]: `segments` is the live pose, `restSegments` the bind pose; a handle
//!   that is absent or `null` reads as no handle (the curve builder then uses `[0,0]`, as the baseline's
//!   `_boneSegsToPath` does for a missing array);
//! - a bind keeps its rest points, its per-point weight entries and its rotate switch; an entry with a
//!   `ptIndex` anchors on a control point, one without on the bone curve at `offset` (the discriminator
//!   is the PRESENCE of `ptIndex`, exactly as `anchorFor` routes it);
//! - `restHandles[i].i/.o` of `null` or absent is "no handle to write" (the baseline's `if(rh.i)`).
//! Numbers are read with `as_f64`; admission guarantees finite values, so a NaN here can only come from a
//! record that skipped admission and then flows through the kernel's own NaN rules.

use crate::evaluation::object_image_mesh_kernels::rig_pose_deform::RestHandle as DeformRestHandle;
use crate::evaluation::object_image_mesh_kernels::rig_pose_deform_anchor::{
    DeformBone, DeformSegment,
};
use crate::evaluation::object_image_mesh_kernels::rig_pose_weights::{
    Point2, WeightEntry, WeightParts,
};
use crate::object_document::object_rig_record::{
    RigBind, RigPair, RigRecord, RigSegment, RigWeight,
};

/// One bind's kernel inputs, owned so the join can borrow them for the call.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct OwnedBind {
    pub(crate) rest: Vec<Point2>,
    /// `None`: the bind has no `restHandles` (a rig bound before they were stored).
    pub(crate) rest_handles: Option<Vec<DeformRestHandle>>,
    pub(crate) weights: Vec<Vec<WeightEntry>>,
    pub(crate) rotate: bool,
}

fn pair(value: &RigPair) -> Point2 {
    [
        value[0].as_f64().unwrap_or(f64::NAN),
        value[1].as_f64().unwrap_or(f64::NAN),
    ]
}

/// A stored handle slot: absent and `null` both mean no handle.
fn slot(value: Option<&Option<RigPair>>) -> Option<Point2> {
    value.and_then(|inner| inner.as_ref()).map(pair)
}

fn segments(list: &[RigSegment]) -> Vec<DeformSegment> {
    list.iter()
        .map(|segment| DeformSegment {
            point: Some(pair(&segment.point)),
            handle_in: slot(segment.handle_in.value()),
            handle_out: slot(segment.handle_out.value()),
        })
        .collect()
}

/// Every bone of the rig, in record order, with its live and bind pose.
pub(crate) fn deform_bones(rig: &RigRecord) -> Vec<DeformBone> {
    rig.bones
        .entries()
        .iter()
        .map(|(id, bone)| DeformBone {
            id: id.clone(),
            closed: bone.closed,
            segments: Some(segments(&bone.segments)),
            rest_segments: Some(segments(&bone.rest_segments)),
        })
        .collect()
}

fn weight_entry(weight: &RigWeight) -> WeightEntry {
    WeightEntry::classify(WeightParts {
        bone_id: weight.bone_id.clone(),
        pt_index: weight.pt_index.value().map(|index| *index as usize),
        offset: weight.offset_value(),
        w: weight.w_value(),
    })
}

/// The kernel inputs of one stored bind.
pub(crate) fn owned_bind(bind: &RigBind) -> OwnedBind {
    OwnedBind {
        rest: bind.rest_points(),
        rest_handles: bind.rest_handles.value().map(|list| {
            list.iter()
                .map(|handle| DeformRestHandle {
                    handle_in: slot(handle.i.value()),
                    handle_out: slot(handle.o.value()),
                })
                .collect()
        }),
        weights: bind
            .weights
            .iter()
            .map(|entries| entries.iter().map(weight_entry).collect())
            .collect(),
        rotate: bind.rotate,
    }
}
