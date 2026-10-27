//! Detached canonical-record projection of a subselect intent. Shared by preview and commit.
use crate::object_document::subselect::{edit, Anchor, Intent, Refusal};
use crate::object_document::{
    ObjectDocument, ObjectPoint, ObjectRecord, ObjectSegment, VECTOR_FAMILY,
};
use serde_json::Number;

/// The displayed frame may hold an earlier key. Admit promotion only when its
/// source and complete copied cell are representable by the native frame owner.
pub(crate) fn at_frame(
    document: &ObjectDocument,
    record: &ObjectRecord,
    frame: u32,
) -> Result<bool, Refusal> {
    use crate::object_document::{FrameScopeKind, RangeEvaluator};
    let target = &record.target;
    if frame >= document.total_frames
        || target.context_id != "scene-root"
        || target.frame_scope.kind != FrameScopeKind::Authored
    {
        return Err(Refusal::Invalid);
    }
    let layer = document
        .layers
        .iter()
        .find(|l| l.layer_uid == target.layer_uid)
        .ok_or(Refusal::Unsupported)?;
    let ranges = RangeEvaluator::for_document(document, "scene-root", frame)
        .map_err(|_| Refusal::Unsupported)?;
    if ranges
        .gates(&target.layer_uid)
        .map_err(|_| Refusal::Unsupported)?
        && !ranges
            .resolve(&target.layer_uid, true)
            .map_err(|_| Refusal::Unsupported)?
            .contains(frame)
    {
        return Err(Refusal::Unsupported);
    }
    if frame == target.frame_scope.frame {
        return Ok(false);
    }
    let timing = layer.timing.value().ok_or(Refusal::Unsupported)?;
    let (kind, source, _) = timing.exposure(frame);
    if kind != "held" || source != Some(target.frame_scope.frame) {
        return Err(Refusal::Unsupported);
    }
    let in_cell = |other: &crate::object_document::ObjectTarget| {
        other.context_id == target.context_id
            && other.layer_uid == target.layer_uid
            && other.frame_scope.kind == FrameScopeKind::Authored
            && [frame, target.frame_scope.frame].contains(&other.frame_scope.frame)
    };
    if document.items.value().is_some_and(|items| {
        items
            .raster
            .value()
            .is_some_and(|entries| entries.iter().any(|r| in_cell(&r.target)))
            || items
                .text
                .value()
                .is_some_and(|entries| entries.iter().any(|r| in_cell(&r.target)))
    }) {
        return Err(Refusal::Unsupported);
    }
    Ok(true)
}

fn pair(p: &ObjectPoint) -> Result<[f64; 2], Refusal> {
    Ok([
        p.x.as_f64().ok_or(Refusal::Invalid)?,
        p.y.as_f64().ok_or(Refusal::Invalid)?,
    ])
}
fn point(p: [f64; 2]) -> Result<ObjectPoint, Refusal> {
    Ok(ObjectPoint {
        x: Number::from_f64(p[0]).ok_or(Refusal::Invalid)?,
        y: Number::from_f64(p[1]).ok_or(Refusal::Invalid)?,
    })
}
// Keep API/MCP edits and both read paths inside the same admitted consumer scope.
pub(crate) fn admit(document: &ObjectDocument, record: &ObjectRecord) -> Result<(), Refusal> {
    if record.target.context_id != "scene-root" {
        return Err(Refusal::Unsupported);
    }
    let layer = document
        .layers
        .iter()
        .find(|l| l.layer_uid == record.target.layer_uid)
        .ok_or(Refusal::Unsupported)?;
    if layer.three_d.value() == Some(&true) || layer.rig().is_some() {
        return Err(Refusal::Unsupported);
    }
    if document.property_holders.value().is_some_and(|holders| {
        holders.iter().any(|h| {
            h.target.context_id == record.target.context_id
                && h.target.layer_uid == record.target.layer_uid
                && h.target.stroke_id.value() == Some(&record.target.stroke_id)
                && h.properties.iter().any(|p| {
                    p.property.starts_with("vtx")
                        || (record
                            .vector_ext
                            .value()
                            .is_some_and(|e| !e.param_shape.missing())
                            && matches!(
                                p.property.as_str(),
                                "cornerTL"
                                    | "cornerTR"
                                    | "cornerBR"
                                    | "cornerBL"
                                    | "arcStart"
                                    | "arcSweep"
                                    | "arcInner"
                                    | "starInner"
                                    | "starCorner"
                            )
                            && p.track.value().is_some_and(|track| !track.keys.is_empty()))
                })
        })
    }) {
        return Err(Refusal::Unsupported);
    }
    Ok(())
}
pub(crate) fn anchors(record: &ObjectRecord) -> Result<Vec<Anchor>, Refusal> {
    if record.family != VECTOR_FAMILY {
        return Err(Refusal::Unsupported);
    }
    if !(2..=256).contains(&record.geometry.segments.len()) {
        return Err(Refusal::Limit);
    }
    // These consumers require their own geometry/companion projection before a commit can be admitted.
    if record.vector_ext.value().is_some_and(|e| {
        !e.pressure.missing() || !e.bitmap_brush.missing() || !e.brush_texture.missing()
    }) {
        return Err(Refusal::Unsupported);
    }
    record
        .geometry
        .segments
        .iter()
        .map(|s| {
            Ok(Anchor {
                point: pair(&s.point)?,
                handle_in: pair(&s.handle_in)?,
                handle_out: pair(&s.handle_out)?,
            })
        })
        .collect()
}
pub(crate) fn plan(record: &ObjectRecord, intent: &Intent) -> Result<ObjectRecord, Refusal> {
    let edited = edit(&anchors(record)?, record.geometry.closed, false, intent)?;
    let mut next = record.clone();
    let mut origins: Vec<Option<usize>> = (0..record.geometry.segments.len()).map(Some).collect();
    match intent {
        Intent::Insert { curve, .. } => origins.insert(curve + 1, None),
        Intent::Delete { indices } => origins.retain(|i| !indices.contains(&i.unwrap())),
        _ => {}
    }
    // A float geometry calculation must not round untouched canonical JSON integers.
    // Also preserve absent edits and original numeric representation on surviving anchors.
    let keep = |old: &ObjectPoint, value: [f64; 2]| -> Result<ObjectPoint, Refusal> {
        let mut p = point(value)?;
        if old
            .x
            .as_f64()
            .is_some_and(|n| n.to_bits() == value[0].to_bits())
        {
            p.x = old.x.clone();
        }
        if old
            .y
            .as_f64()
            .is_some_and(|n| n.to_bits() == value[1].to_bits())
        {
            p.y = old.y.clone();
        }
        Ok(p)
    };
    next.geometry.segments = edited
        .into_iter()
        .zip(origins)
        .map(|(a, origin)| {
            if let Some(i) = origin {
                let old = &record.geometry.segments[i];
                return Ok(ObjectSegment {
                    point: keep(&old.point, a.point)?,
                    handle_in: keep(&old.handle_in, a.handle_in)?,
                    handle_out: keep(&old.handle_out, a.handle_out)?,
                });
            }
            Ok(ObjectSegment {
                point: point(a.point)?,
                handle_in: point(a.handle_in)?,
                handle_out: point(a.handle_out)?,
            })
        })
        .collect::<Result<_, _>>()?;
    Ok(next)
}

#[derive(Clone, Debug, serde::Serialize, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct Preview {
    pub(crate) stable_target: crate::object_document::ObjectTarget,
    pub(crate) intent: Intent,
}
use crate::object_document::{object_deserialize, ObjectTarget};
use serde::Deserialize;
object_deserialize!(Preview {
    stable_target: ObjectTarget,
    intent: Intent
});
