//! #2255: the candidate rig of a live pose drag (Déplacer), for the presentation read only.
//!
//! The baseline poses the rig live while the pointer moves (`rig-bridge.js` onMove: write `poseSeg.point` / the handle vector,
//! or `rigDragIKEnd`, then `applyRigDeform`) and keeps the pose in `ld.rig` until Commit or Reset. Natively the gesture is ONE
//! `object.rig.edit` on release (`moveSegment` or `dragIk`); while it is held, the view of the rig can take the gesture as a
//! `preview` and answer every part (the bones, their vertices, the bound vertices, the posed outlines) from a CANDIDATE rig, with
//! no revision, no history entry and nothing kept. The candidate is the stored rig with the same vertex writes the edit would
//! make (the same handle mirror, the same IK solve through `object_rig_edit_read::ik_drag`); numbers are plain `f64`, since the
//! candidate is evaluated and discarded, never stored.
use crate::object_codec::{object_deserialize, Presence};
use crate::object_document::object_rig_record::{KeyedTable, RigPair, RigRecord, RigSegment};
use crate::object_snapshot::object_rig_edit_read::{ik_drag, kernel_bones, KernelRef};
use serde::Deserialize;
use serde_json::Number;

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

fn pair(value: [f64; 2]) -> Option<RigPair> {
    Some([Number::from_f64(value[0])?, Number::from_f64(value[1])?])
}

fn with_segment(rig: &RigRecord, bone_id: &str, vi: usize, write: impl FnOnce(&mut RigSegment) -> Option<()>) -> Option<RigRecord> {
    let mut entries = rig.bones.entries().to_vec();
    let slot = entries.iter_mut().find(|(id, _)| id == bone_id)?;
    write(slot.1.segments.get_mut(vi)?)?;
    let mut next = rig.clone();
    next.bones = KeyedTable::new(entries);
    Some(next)
}

/// The rig as the gesture would leave it; `None` when the gesture names no bone or vertex, or cannot be solved (the caller
/// answers it as an invalid preview, never as a guessed pose).
pub(super) fn candidate(rig: &RigRecord, preview: &Preview) -> Option<RigRecord> {
    match (preview.segment.value(), preview.ik.value()) {
        (Some(m), None) => {
            let broken = preview.segment.value().and_then(|s| s.break_tangent.value().copied()).unwrap_or(false);
            with_segment(rig, &m.bone_id, m.vi, |segment| {
                let anchor = [segment.point[0].as_f64()?, segment.point[1].as_f64()?];
                match m.part.as_str() {
                    "point" => segment.point = pair(m.to)?,
                    "handleIn" | "handleOut" => {
                        let vector = [m.to[0] - anchor[0], m.to[1] - anchor[1]];
                        let mirror = pair([-vector[0], -vector[1]])?;
                        let (mine, other) = if m.part == "handleIn" {
                            (&mut segment.handle_in, &mut segment.handle_out)
                        } else {
                            (&mut segment.handle_out, &mut segment.handle_in)
                        };
                        *mine = Presence::Present(Some(pair(vector)?));
                        if !broken {
                            *other = Presence::Present(Some(mirror));
                        }
                    }
                    _ => return None,
                }
                Some(())
            })
        }
        (None, Some(k)) => {
            let end = KernelRef { bone_id: k.bone_id.clone(), vi: u32::try_from(k.vi).ok()? };
            let drag = ik_drag(&kernel_bones(rig), &end, k.to).ok()?;
            let mut next = rig.clone();
            for (target, point) in &drag.writes {
                next = with_segment(&next, &target.bone_id, target.vi as usize, |s| {
                    s.point = pair(*point)?;
                    Some(())
                })?;
            }
            Some(next)
        }
        _ => None,
    }
}
