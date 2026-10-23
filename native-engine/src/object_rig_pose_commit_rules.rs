//! Pure rules of the pose transaction (I13): the two record mutations, on detached copies.
//! None of them publishes, evaluates or reads anything but its arguments.
//!
//! - [`rest_rig`] is the statement of `rigResetPose`: for every bone,
//!   `bone.segments = JSON.parse(JSON.stringify(bone.restSegments))`. The copy is deep (the record owns
//!   its numbers, so a later pose edit cannot reach the rest pose) and keeps what the bind pose spelled:
//!   an absent handle stays absent, a `null` handle stays `null`, a number keeps its authored spelling.
//!   A pose that already equals the rest pose NUMERICALLY (`1` and `1.0` are the same JavaScript number)
//!   is left untouched, so a reset that changes nothing cannot rewrite bytes.
//! - [`replace_segments`] is what `saveActiveLayerFrame` stores after `applyRigDeform`: the posed
//!   anchors and both relative handle vectors replace the stored segments of the stroke at the exposed
//!   key. Only the segment values change (the count is the bind's, so every per-vertex companion stays
//!   aligned); a computed value is spelled like `JSON.stringify` would, and a value equal to the stored
//!   one keeps the stored spelling, so posing nothing changes no byte. A pressure ribbon is refused
//!   (`PoseRefusal::PressureRibbon`) and the detached candidate with it, so no outline is posed alone.
use crate::commands::object_fill_owner::widget_edit::object_rig_pose_commit::PoseRefusal;
use crate::object_document::object_rig_record::{KeyedTable, RigPair, RigRecord, RigSegment};
use crate::object_document::{FrameScopeKind, ObjectDocument, ObjectPoint, Presence};
use crate::object_snapshot::object_rig_pose_read::PoseStroke;
use serde_json::Number;

/// A computed value spelled like `JSON.stringify` would: integral values without a fraction, `-0` as `0`.
pub(super) fn js_number(value: f64) -> Option<Number> {
    if !value.is_finite() {
        return None;
    }
    if value == 0.0 {
        return Some(Number::from(0i64));
    }
    if value.fract() == 0.0 && value.abs() < 9_007_199_254_740_992.0 {
        return Some(Number::from(value as i64));
    }
    Number::from_f64(value)
}

/// A value equal to the stored one keeps the stored spelling.
fn spell(value: f64, stored: &Number) -> Result<Number, PoseRefusal> {
    if stored.as_f64() == Some(value) {
        return Ok(stored.clone());
    }
    js_number(value).ok_or(PoseRefusal::Internal)
}

fn same_pair(a: &RigPair, b: &RigPair) -> bool {
    a[0].as_f64() == b[0].as_f64() && a[1].as_f64() == b[1].as_f64()
}

/// Absent, `null` and a value are three different states of a handle.
fn same_handle(a: &Presence<Option<RigPair>>, b: &Presence<Option<RigPair>>) -> bool {
    match (a, b) {
        (Presence::Missing, Presence::Missing) => true,
        (Presence::Present(x), Presence::Present(y)) => match (x, y) {
            (None, None) => true,
            (Some(p), Some(q)) => same_pair(p, q),
            _ => false,
        },
        _ => false,
    }
}

/// The two poses are the same list of vertices, number for number.
pub(super) fn same_pose(a: &[RigSegment], b: &[RigSegment]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b.iter()).all(|(x, y)| {
            same_pair(&x.point, &y.point)
                && same_handle(&x.handle_in, &y.handle_in)
                && same_handle(&x.handle_out, &y.handle_out)
        })
}

/// The rig with every bone at its rest pose; `None` when every bone already is (nothing to publish).
pub(super) fn rest_rig(rig: &RigRecord) -> Option<RigRecord> {
    let mut changed = false;
    let mut entries = Vec::with_capacity(rig.bones.len());
    for (id, bone) in rig.bones.entries() {
        let mut next = bone.clone();
        if !same_pose(&bone.segments, &bone.rest_segments) {
            next.segments = bone.rest_segments.clone();
            changed = true;
        }
        entries.push((id.clone(), next));
    }
    if !changed {
        return None;
    }
    let mut next = rig.clone();
    next.bones = KeyedTable::new(entries);
    Some(next)
}

fn point(value: [f64; 2], stored: &ObjectPoint) -> Result<ObjectPoint, PoseRefusal> {
    Ok(ObjectPoint {
        x: spell(value[0], &stored.x)?,
        y: spell(value[1], &stored.y)?,
    })
}

/// Writes the posed vertices of every stroke into the authored record at `frame` of `layer_uid`.
/// The record is found with the predicate the pose evaluation read it with (one layer, one stable
/// stroke id, one authored frame), so the segments written are the segments evaluated.
pub(super) fn replace_segments(
    document: &mut ObjectDocument,
    layer_uid: &str,
    frame: u32,
    strokes: &[PoseStroke],
) -> Result<(), PoseRefusal> {
    for stroke in strokes {
        let index = document
            .objects
            .iter()
            .position(|record| {
                let target = record.target();
                target.layer_uid() == layer_uid
                    && target.stroke_id() == stroke.stroke_id.as_str()
                    && target.frame() == frame
                    && *target.scope_kind() == FrameScopeKind::Authored
            })
            .ok_or(PoseRefusal::Internal)?;
        // The baseline poses `centerSegments` with the outline (`rigCommitFrame`); only the outline is
        // written here, so a ribbon is refused instead of left with a centreline at rest.
        if document.objects[index].is_pressure_ribbon() {
            return Err(PoseRefusal::PressureRibbon);
        }
        let segments = &mut document.objects[index].geometry.segments;
        if segments.len() != stroke.vertices.len() {
            return Err(PoseRefusal::Internal);
        }
        for (segment, vertex) in segments.iter_mut().zip(stroke.vertices.iter()) {
            segment.point = point(vertex.point, &segment.point)?;
            segment.handle_in = point(vertex.handle_in, &segment.handle_in)?;
            segment.handle_out = point(vertex.handle_out, &segment.handle_out)?;
        }
    }
    Ok(())
}
