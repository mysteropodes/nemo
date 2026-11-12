//! The `vtxN` Motion offsets of an element holder (baseline `applyPathVertexOffsets`) for the vector evaluation join and the hit-test listing.
use crate::evaluation::object_vector_kernels::object_parametric_geometry::ParamSegment;
use crate::evaluation::object_vector_kernels::object_path_effects_kernel::PathSegment;
use crate::evaluation::object_vector_kernels::render_vector_eval::{
    from_param, from_path, is_vector_record, run_param_shape, run_param_shape_with, run_path_fx,
    stored_from_record, EvalSegment, VectorEvalError,
};
use crate::object_document::{ObjectLayer, ObjectPoint, ObjectSegment};
use crate::object_document::{ObjectRecord, Presence, PropertyHolder, PropertyHolderTarget};
use crate::object_frame_packet::{sample_property_raw, PropertySampleError};
use crate::revision::ObjectSnapshot;
use serde_json::Number;

/// Baseline `applyPathVertexOffsets`: each `vtxN` track is a `[dx, dy]` OFFSET added to anchor N's point only (handles are
/// unchanged), after path effects/duplicator placement, before a parametric rebuild and BEFORE the element and layer pose. An unkeyed vertex is `[0, 0]`; a
/// vertex driven by an enabled expression stays unavailable here (the raw sampler would silently ignore it).
fn apply_vertex_offsets(
    record: &ObjectRecord,
    mut segments: Vec<EvalSegment>,
    offsets: Vec<(usize, [f64; 2])>,
) -> Result<Vec<EvalSegment>, VectorEvalError> {
    // A ribbon's vtxN addresses its centerline, never its baked outline.
    // The joined animated centerline/width producer is still missing; do not
    // misapply a centerline index to the outline or silently draw at rest.
    if record
        .vector_ext
        .value()
        .is_some_and(|ext| ext.pressure.value().is_some())
        && offsets.iter().any(|(_, off)| *off != [0., 0.])
    {
        return Err(VectorEvalError::BrushVertexUnavailable);
    }
    for (index, [dx, dy]) in offsets {
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

/// Baseline path-local order after path effects/placement: vertex offsets, then
/// animated parametric rebuild. The rebuild uses the OFFSET bounds, replacing
/// points and handles rather than offsetting the new shape's vertex N.
pub(crate) fn vertex_then_param(
    snapshot: &ObjectSnapshot,
    record: &ObjectRecord,
    anchors: &[PathSegment],
    frame: u32,
) -> Result<Vec<EvalSegment>, VectorEvalError> {
    let offsets = vertex_offsets(snapshot, record, frame)?;
    if offsets.iter().all(|(_, offset)| *offset == [0., 0.]) {
        // Preserve the common no-vertex-motion path: no intermediate segment
        // arrays, and an idle shape never invokes the deferred anchor builder.
        return Ok(match run_param_shape(snapshot, record, anchors, frame)? {
            Some(rebuilt) => rebuilt.iter().map(from_param).collect(),
            None => anchors.iter().map(from_path).collect(),
        });
    }
    let segments = apply_vertex_offsets(record, anchors.iter().map(from_path).collect(), offsets)?;
    Ok(
        match run_param_shape_with(snapshot, record, frame, || {
            segments
                .iter()
                .map(|s| ParamSegment {
                    point: s.point,
                    handle_in: s.handle_in,
                    handle_out: s.handle_out,
                })
                .collect()
        })? {
            Some(rebuilt) => rebuilt.iter().map(from_param).collect(),
            None => segments,
        },
    )
}

/// Detached geometry in the record's local paint space, before element/layer pose.
/// Shared by the hit listing, its paged contour reader and element-pivot consumer.
pub(crate) fn painted_record(
    snapshot: &ObjectSnapshot,
    record: &ObjectRecord,
    layer: &ObjectLayer,
    frame: u32,
) -> Result<ObjectRecord, VectorEvalError> {
    if !is_vector_record(record) {
        return Ok(record.clone());
    }
    let stored = stored_from_record(record, Some(layer))?;
    let after = run_path_fx(stored.stack.as_deref(), &stored.stroke, frame)?;
    let segments = vertex_then_param(
        snapshot,
        record,
        after.segments.as_deref().unwrap_or(&[]),
        frame,
    )?;
    let point = |p: [f64; 2]| -> Result<ObjectPoint, VectorEvalError> {
        Ok(ObjectPoint {
            x: Number::from_f64(p[0]).ok_or(VectorEvalError::NonFiniteInput)?,
            y: Number::from_f64(p[1]).ok_or(VectorEvalError::NonFiniteInput)?,
        })
    };
    let mut output = record.clone();
    output.geometry.closed = after.closed;
    output.geometry.segments = segments
        .into_iter()
        .map(|s| {
            Ok(ObjectSegment {
                point: point(s.point)?,
                handle_in: point(s.handle_in)?,
                handle_out: point(s.handle_out)?,
            })
        })
        .collect::<Result<_, VectorEvalError>>()?;
    Ok(output)
}
