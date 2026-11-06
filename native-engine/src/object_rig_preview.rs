//! #2255: the candidate rig of a live pose drag (Déplacer), for the presentation read only.
//!
//! The baseline poses the rig live while the pointer moves (`rig-bridge.js` onMove: write `poseSeg.point` / the handle vector,
//! or `rigDragIKEnd`, then `applyRigDeform`) and keeps the pose in `ld.rig` until Commit or Reset. Natively the gesture is ONE
//! `object.rig.edit` on release (`moveSegment` or `dragIk`); while it is held, the view of the rig can take the gesture as a
//! `preview` and answer every part (the bones, their vertices, the bound vertices, the posed outlines) from a CANDIDATE rig, with
//! no revision, no history entry and nothing kept. The candidate is the stored rig with the same vertex writes the edit would
//! make (the same handle mirror, the same IK solve of `rig_ik_solve::plan_drag`, both in `rig_pose_eval_view.rs`); the gesture is
//! handed on as plain [`VertexWrite`]s, since the candidate is evaluated and discarded, never stored.
use crate::object_codec::{object_deserialize, Presence};
use crate::object_snapshot::object_rig_pose_read::VertexWrite;
use crate::revision::ObjectSnapshot;
use serde::Deserialize;

/// A held `moveSegment`: an anchor, or a handle with its symmetric mirror unless the tangent is broken (Alt).
pub(super) struct SegmentPreview {
    bone_id: String,
    vi: usize,
    part: String,
    to: [f64; 2],
    break_tangent: Presence<bool>,
}
object_deserialize!(SegmentPreview {
    bone_id: String,
    vi: usize,
    part: String,
    to: [f64; 2],
    #[serde(default)] break_tangent: Presence<bool>
});

/// A held `dragIk`: the far end of a two-bone chain dragged toward `to`.
pub(super) struct IkPreview {
    bone_id: String,
    vi: usize,
    to: [f64; 2],
}
object_deserialize!(IkPreview {
    bone_id: String,
    vi: usize,
    to: [f64; 2]
});

/// Exactly one of the two gestures.
pub(super) struct Preview {
    segment: Presence<SegmentPreview>,
    ik: Presence<IkPreview>,
}
object_deserialize!(Preview {
    #[serde(default)] segment: Presence<SegmentPreview>,
    #[serde(default)] ik: Presence<IkPreview>
});

/// The vertex writes the gesture would make on the stored rig of `layer_uid`; `None` when it names no bone or vertex, or cannot be
/// solved (the caller answers an invalid preview, never a guessed pose).
#[cfg_attr(not(feature = "evaluation"), allow(unused_variables))]
pub(super) fn writes(snapshot: &ObjectSnapshot, layer_uid: &str, preview: &Preview) -> Option<Vec<VertexWrite>> {
    #[cfg(feature = "evaluation")]
    {
        use crate::evaluation::object_image_mesh_kernels::rig_pose_eval::{ik_writes, segment_writes};
        let made = match (preview.segment.value(), preview.ik.value()) {
            (Some(m), None) => {
                let broken = m.break_tangent.value().copied().unwrap_or(false);
                segment_writes(snapshot, layer_uid, &m.bone_id, m.vi, &m.part, m.to, broken)?
            }
            (None, Some(k)) => ik_writes(snapshot, layer_uid, &k.bone_id, k.vi, k.to)?,
            _ => return None,
        };
        Some(
            made.into_iter()
                .map(|w| VertexWrite { bone_id: w.bone_id, vi: w.vi, point: w.point, handle_in: w.handle_in, handle_out: w.handle_out })
                .collect(),
        )
    }
    #[cfg(not(feature = "evaluation"))]
    {
        None
    }
}
