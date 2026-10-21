//! Existing stored rig topology/stage gates, split before the actual common layer transform.
use crate::evaluation::object_vector_kernels::object_path_effects_kernel::has_any;
use crate::evaluation::object_vector_kernels::render_vector_eval::{
    evaluate_record, evaluate_stored_rest, load_packet_stored, motion, run_param_shape,
    EvalSegment, EvaluatedVector, VectorEvalError,
};
use crate::object_document::{FrameScopeKind, ObjectDocument, PropertyHolderTarget, VECTOR_FAMILY};
use crate::object_frame_packet::{ObjectFramePacket, ObjectFrameRecord};

/// Existing strict API retains its old layer-Position and duplicator seed admission.
pub(crate) fn evaluate_record_with_pose(
    packet: &ObjectFramePacket,
    source: ObjectFrameRecord<'_>,
    pose: Option<&crate::evaluation::object_image_mesh_kernels::rig_pose_eval::PosedStroke>,
) -> Result<EvaluatedVector, VectorEvalError> {
    if pose.is_none() {
        return evaluate_record(packet, source);
    }
    let mut evaluated = evaluate_record_rest_with_pose(packet, source, pose)?;
    evaluated.segments = motion::apply_layer_position(
        packet.snapshot(),
        source.record(),
        packet.frame(),
        evaluated.segments,
    )?;
    Ok(evaluated)
}

/// Actual pre-layer entry for detached stored rig output and ordinary stored vectors.
/// Combined rig/path-effect/rebuild ordering has no
/// original consumer oracle yet; refuse it instead of inventing that order.
pub(crate) fn evaluate_record_rest_with_pose(
    packet: &ObjectFramePacket,
    source: ObjectFrameRecord<'_>,
    pose: Option<&crate::evaluation::object_image_mesh_kernels::rig_pose_eval::PosedStroke>,
) -> Result<EvaluatedVector, VectorEvalError> {
    let Some(pose) = pose else {
        let stored = load_packet_stored(packet, source)?;
        return evaluate_stored_rest(
            packet.snapshot(),
            source.source_object_index(),
            packet.frame(),
            stored,
        );
    };
    let snapshot = packet.snapshot();
    let stored = load_packet_stored(packet, source)?;
    let anchors = stored.stroke.segments.as_deref().unwrap_or(&[]);
    if pose.stroke_id != stored.record.target.stroke_id
        || pose.closed != stored.stroke.closed
        || pose.segments.len() != anchors.len()
    {
        return Err(VectorEvalError::MissingRecord);
    }
    if stored
        .record
        .vector_ext
        .value()
        .is_some_and(|ext| ext.pressure.value().is_some())
        || stored
            .stack
            .as_deref()
            .is_some_and(|stack| has_any(Some(stack)))
        || run_param_shape(snapshot, stored.record, anchors, packet.frame())?.is_some()
    {
        return Err(VectorEvalError::RigCompositionUnavailable);
    }
    let segments = pose
        .segments
        .iter()
        .map(|segment| EvalSegment {
            point: segment.point,
            handle_in: segment.handle_in,
            handle_out: segment.handle_out,
        })
        .collect();
    let segments = motion::apply_element_motion(snapshot, stored.record, packet.frame(), segments)?;
    Ok(EvaluatedVector {
        source_object_index: source.source_object_index(),
        closed: pose.closed,
        segments,
    })
}

/// Exact root layer families consumed by the common pre-compile batch.
/// This proves a Motion consumer, not acceptance of every subsequent paint stage.
pub(crate) fn common_layer_motion_target(
    document: &ObjectDocument,
    target: &PropertyHolderTarget,
) -> bool {
    if document.format_version() != 2
        || target.context_id() != "scene-root"
        || target.stroke_id().is_some()
    {
        return false;
    }
    let Some(context) = document.composition_order.value().and_then(|order| {
        order
            .contexts
            .iter()
            .find(|c| c.context_id == target.context_id())
    }) else {
        return false;
    };
    let mut next = Some(target.layer_uid());
    let mut seen = std::collections::BTreeSet::new();
    while let Some(uid) = next {
        if seen.len() > crate::evaluation::motion_parent_chain::CHAIN_NODE_LIMIT
            || !seen.insert(uid)
            || !context.layers.iter().any(|l| l.layer_uid == uid)
        {
            return false;
        }
        let Some(layer) = document.layers().iter().find(|l| l.layer_uid() == uid) else {
            return false;
        };
        let current = PropertyHolderTarget {
            context_id: target.context_id().to_owned(),
            layer_uid: uid.to_owned(),
            stroke_id: crate::object_document::Presence::Missing,
        };
        if !common_layer_content_target(document, &current, layer) {
            return false;
        }
        // Original3D suppresses its single-parent affine; retain that existing boundary.
        if uid == target.layer_uid() && layer.three_d.value() == Some(&true) {
            return true;
        }
        if layer.three_d.value() == Some(&true) {
            return false;
        }
        next = layer.parent_layer_uid.value().and_then(Option::as_deref);
    }
    true
}

fn common_layer_content_target(
    document: &ObjectDocument,
    target: &PropertyHolderTarget,
    layer: &crate::object_document::ObjectLayer,
) -> bool {
    if layer.is_effect_layer()
        || layer.is_folder_layer()
        || layer.widget.value().is_some()
        || document.duplicator_set().is_some_and(|set| {
            set.duplicator(target.context_id(), target.layer_uid())
                .is_some_and(|d| d.duplicator.effective_clone_count() > 1.)
        })
        || has_unjoined_parent_blend(document, target)
    {
        return false;
    }
    if layer.is_null_layer() {
        return crate::evaluation::object_vector_kernels::render_vector_eval::params::null_layer_motion_target(document, target);
    }
    let mut found = false;
    for record in document.objects().iter().filter(|r| {
        r.target.context_id == target.context_id() && r.target.layer_uid == target.layer_uid()
    }) {
        found = true;
        if record.target.frame_scope.kind != FrameScopeKind::Authored
            || !matches!(
                record.family.as_str(),
                VECTOR_FAMILY | "closed-solid-fill-cubic-path"
            )
            || record.is_mask()
        {
            return false;
        }
    }
    found
        && document.item_records().is_none_or(|items| {
            !items.targets().any(|item| {
                item.context_id() == target.context_id() && item.layer_uid() == target.layer_uid()
            })
        })
}

/// No native multiple-parent descriptor exists in this storage version: strict
/// ObjectLayer ingress rejects it. Authored blend controls must also stay unjoined.
pub(crate) fn has_unjoined_parent_blend(
    document: &ObjectDocument,
    target: &PropertyHolderTarget,
) -> bool {
    document.property_holders().is_some_and(|holders| {
        holders.iter().any(|h| {
            h.target() == target
                && h.properties().iter().any(|p| {
                    p.property() == "parentBlend" || p.property().starts_with("parentWeight")
                })
        })
    })
}
