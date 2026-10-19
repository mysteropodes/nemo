//! The raw layer Position consumer of the immutable vector evaluation join.
//!
//! Shape effects/rebuild run first, then the existing Motion segment transform.
//! This admits only root, ordinary vector-only layers. Other families, parents,
//! expressions and controls stay unavailable until their own producer consumes
//! them. Opacity remains the packet's existing paint-factor responsibility.
use super::{EvalSegment, VectorEvalError};
use crate::evaluation::motion_parent_chain::{motion_mat_from_values, transform_segments, Segment};
use crate::object_document::{
    FrameScopeKind, ObjectDocument, ObjectRecord, Presence, PropertyHolder, PropertyHolderTarget,
    VECTOR_FAMILY,
};
use crate::object_frame_packet::{sample_property_raw, PropertySampleError};
use crate::revision::ObjectSnapshot;

fn ordinary_root_layer(document: &ObjectDocument, target: &PropertyHolderTarget) -> bool {
    target.context_id() == "scene-root"
        && target.stroke_id().is_none()
        && document.layers().iter().any(|layer| {
            layer.layer_uid() == target.layer_uid()
                && !layer.is_effect_layer()
                && !layer.is_folder_layer()
                && layer.widget.value().is_none()
                && layer
                    .parent_layer_uid
                    .value()
                    .is_none_or(|parent| parent.is_none())
        })
}

fn vector_only_layer(document: &ObjectDocument, target: &PropertyHolderTarget) -> bool {
    let belongs = |record: &ObjectRecord| {
        record.target.context_id == target.context_id()
            && record.target.layer_uid == target.layer_uid()
    };
    let mut found = false;
    for record in document.objects().iter().filter(|record| belongs(record)) {
        found = true;
        if record.family != VECTOR_FAMILY
            || record.target.frame_scope.kind != FrameScopeKind::Authored
        {
            return false;
        }
    }
    // Position is not consumed by a raster/text producer merely because a
    // vector elsewhere in the same layer went through this evaluator.
    found
        && document.item_records().is_none_or(|items| {
            !items.targets().any(|item| {
                item.context_id() == target.context_id() && item.layer_uid() == target.layer_uid()
            })
        })
}

pub(super) fn layer_holder_consumed(document: &ObjectDocument, holder: &PropertyHolder) -> bool {
    let target = holder.target();
    ordinary_root_layer(document, target)
        && !holder.properties().is_empty()
        && holder.properties().iter().all(|property| {
            !property.expression.value().is_some_and(|code| code.enabled)
                && match property.property() {
                    "opacity" => true,
                    "position" => vector_only_layer(document, target),
                    _ => false,
                }
        })
}

/// No holder/Position means no transform, preserving every old float bit.
pub(super) fn apply_layer_position(
    snapshot: &ObjectSnapshot,
    record: &ObjectRecord,
    frame: u32,
    segments: Vec<EvalSegment>,
) -> Result<Vec<EvalSegment>, VectorEvalError> {
    let target = PropertyHolderTarget {
        context_id: record.target.context_id.clone(),
        layer_uid: record.target.layer_uid.clone(),
        stroke_id: Presence::Missing,
    };
    let Some(holder) = snapshot
        .document()
        .property_holders()
        .and_then(|holders| holders.iter().find(|holder| holder.target() == &target))
    else {
        return Ok(segments);
    };
    if !holder
        .properties()
        .iter()
        .any(|property| property.property() == "position")
    {
        return Ok(segments);
    }
    if holder
        .controls
        .value()
        .is_some_and(|controls| !controls.is_empty())
        || !layer_holder_consumed(snapshot.document(), holder)
    {
        return Err(VectorEvalError::LayerMotionUnavailable);
    }
    let sampled =
        sample_property_raw(snapshot, &target, "position", f64::from(frame)).map_err(|error| {
            VectorEvalError::Sample {
                property: "position",
                error,
            }
        })?;
    let [dx, dy] = sampled.as_slice() else {
        return Err(VectorEvalError::Sample {
            property: "position",
            error: PropertySampleError::Unavailable,
        });
    };
    // Translation has no pivot dependence. The existing native Motion math
    // preserves its exact scale/rotate/translate and relative-handle order.
    let Some(mat) = motion_mat_from_values([*dx, *dy], [0., 0.], 0., [100., 100.], 100.) else {
        return Ok(segments);
    };
    let source: Vec<_> = segments
        .iter()
        .map(|segment| Segment {
            point: segment.point,
            handle_in: Some(segment.handle_in),
            handle_out: Some(segment.handle_out),
        })
        .collect();
    transform_segments(&source, [0., 0.], &mat)
        .into_iter()
        .map(|segment| {
            let out = EvalSegment {
                point: segment.point,
                handle_in: segment.handle_in.unwrap_or([0., 0.]),
                handle_out: segment.handle_out.unwrap_or([0., 0.]),
            };
            if out
                .point
                .iter()
                .chain(&out.handle_in)
                .chain(&out.handle_out)
                .any(|n| !n.is_finite())
            {
                Err(VectorEvalError::NonFiniteInput)
            } else {
                Ok(out)
            }
        })
        .collect()
}
