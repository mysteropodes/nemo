//! The `vtxN` Motion offsets of an element holder (baseline `applyPathVertexOffsets`) for the vector evaluation join and the hit-test listing.
use crate::evaluation::object_vector_kernels::render_vector_eval::{EvalSegment, VectorEvalError};
use crate::object_document::{ObjectRecord, Presence, PropertyHolder, PropertyHolderTarget};
use crate::object_frame_packet::{sample_property_raw, PropertySampleError};
use crate::revision::ObjectSnapshot;

/// Baseline `applyPathVertexOffsets`: each `vtxN` track is a `[dx, dy]` OFFSET added to anchor N's point only (handles are
/// unchanged), on the already rebuilt segments and BEFORE the element and layer pose. An unkeyed vertex is `[0, 0]`; a
/// vertex driven by an enabled expression stays unavailable here (the raw sampler would silently ignore it).
pub(crate) fn apply_vertex_offsets(
    snapshot: &ObjectSnapshot,
    record: &ObjectRecord,
    frame: u32,
    mut segments: Vec<EvalSegment>,
) -> Result<Vec<EvalSegment>, VectorEvalError> {
    for (index, [dx, dy]) in vertex_offsets(snapshot, record, frame)? {
        let Some(segment) = segments.get_mut(index) else {
            continue;
        };
        segment.point = [segment.point[0] + dx, segment.point[1] + dy];
        if !segment.point.iter().all(|v| v.is_finite()) {
            return Err(VectorEvalError::NonFiniteInput);
        }
    }
    Ok(segments)
}

/// The `vtxN` offsets of the record's element holder at `frame`, by vertex index (only authored vertices appear).
pub(crate) fn vertex_offsets(
    snapshot: &ObjectSnapshot,
    record: &ObjectRecord,
    frame: u32,
) -> Result<Vec<(usize, [f64; 2])>, VectorEvalError> {
    let target = PropertyHolderTarget {
        context_id: record.target.context_id.clone(),
        layer_uid: record.target.layer_uid.clone(),
        stroke_id: Presence::Present(record.target.stroke_id.clone()),
    };
    let Some(holder) = snapshot
        .document()
        .property_holders()
        .and_then(|holders| holders.iter().find(|holder| holder.target() == &target))
    else {
        return Ok(Vec::new());
    };
    let mut offsets = Vec::new();
    for property in holder.properties() {
        let Some(index) = PropertyHolder::vertex_index(property.property()) else {
            continue;
        };
        if property.expression.value().is_some_and(|code| code.enabled) {
            return Err(VectorEvalError::ExpressionUnavailable { property: "vtx" });
        }
        let sampled = sample_property_raw(snapshot, &target, property.property(), f64::from(frame))
            .map_err(|error| VectorEvalError::Sample {
                property: "vtx",
                error,
            })?;
        let (Some(dx), Some(dy)) = (sampled.first(), sampled.get(1)) else {
            return Err(VectorEvalError::Sample {
                property: "vtx",
                error: PropertySampleError::Unavailable,
            });
        };
        offsets.push((index, [*dx, *dy]));
    }
    Ok(offsets)
}

/// The record as PAINTED for the hit-test listing: a detached copy whose anchor points carry their `vtxN` offsets, with the
/// dense per-anchor offsets. `None` when the record has no vertex Motion (or one that cannot be sampled: paint refuses it).
pub(crate) fn vertex_posed(
    snapshot: &ObjectSnapshot,
    record: &ObjectRecord,
    frame: u32,
) -> Option<(ObjectRecord, Vec<[f64; 2]>)> {
    let offsets = vertex_offsets(snapshot, record, frame).ok()?;
    let mut dense = vec![[0., 0.]; record.geometry.segments.len()];
    let mut any = false;
    for (index, off) in offsets {
        if let Some(slot) = dense.get_mut(index) {
            *slot = off;
            any |= off != [0., 0.];
        }
    }
    if !any {
        return None;
    }
    let mut posed = record.clone();
    for (segment, off) in posed.geometry.segments.iter_mut().zip(&dense) {
        let x = segment.point.x.as_f64()? + off[0];
        let y = segment.point.y.as_f64()? + off[1];
        segment.point.x = serde_json::Number::from_f64(x)?;
        segment.point.y = serde_json::Number::from_f64(y)?;
    }
    Some((posed, dense))
}

