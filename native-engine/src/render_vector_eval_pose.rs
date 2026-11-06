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
    motion_target(document, target, false)
}

/// A layer with nothing of its own to place in 3D: no drawn record, no image or text item, no component instance and no
/// duplicator. That covers a drawing layer with nothing on it yet and the kinds that never draw (Null, Folder, Guide, Effect
/// and widget layers: the baseline `toggleLayer3D` takes on any of them and the document keeps rendering). Its switch takes,
/// and a later stroke or image is then a record the common batch joins.
pub(crate) fn plain_empty_layer(document: &ObjectDocument, layer_uid: &str) -> bool {
    if !document.layers().iter().any(|l| l.layer_uid() == layer_uid) {
        return false;
    }
    !(document.component_set().is_some_and(|set| set.instance("scene-root", layer_uid).is_some())
        || document.duplicator_set().is_some_and(|set| set.duplicator("scene-root", layer_uid).is_some())
        || document.objects().iter().any(|r| r.target.context_id == "scene-root" && r.target.layer_uid == layer_uid)
        || document.item_records().is_some_and(|items| {
            items.targets().any(|t| t.context_id() == "scene-root" && t.layer_uid() == layer_uid)
        }))
}

/// Whether the layer, with its 3D switch ON, still joins the common batch (#2520: the switch is refused where it would not,
/// instead of leaving the whole frame unavailable). A layer parented to it keeps the baseline chain (the 3D parent contributes
/// its 2D Motion), a gradient fill keeps its raw endpoints (the baseline projects the geometry only), and images and
/// sequences take the projected rect.
pub(crate) fn three_d_joins_batch(document: &ObjectDocument, layer_uid: &str) -> bool {
    if plain_empty_layer(document, layer_uid) {
        return true;
    }
    let target = PropertyHolderTarget {
        context_id: "scene-root".into(),
        layer_uid: layer_uid.to_owned(),
        stroke_id: crate::object_document::Presence::Missing,
    };
    motion_target(document, &target, true)
}

fn motion_target(document: &ObjectDocument, target: &PropertyHolderTarget, assume_three_d: bool) -> bool {
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
        // Original3D suppresses its single-parent affine; retain that existing boundary. The target's own 3D layer also
        // takes still images (their rect is projected); an ancestor never does.
        let own_three_d = uid == target.layer_uid() && (assume_three_d || layer.three_d.value() == Some(&true));
        if !common_layer_content_target(document, &current, layer, own_three_d) {
            return false;
        }
        if own_three_d {
            return true;
        }
        // A 3D ancestor still contributes its 2D Motion to the layers parented to it (baseline `computeMotionMat`).
        next = layer.parent_layer_uid.value().and_then(Option::as_deref);
    }
    true
}

fn common_layer_content_target(
    document: &ObjectDocument,
    target: &PropertyHolderTarget,
    layer: &crate::object_document::ObjectLayer,
    still_images: bool,
) -> bool {
    if layer.is_effect_layer()
        || layer.widget.value().is_some()
        // A 3D host's clones are projected by the duplicator scene (each with its own `dup3D` delta).
        || (!still_images && document.duplicator_set().is_some_and(|set| {
            set.duplicator(target.context_id(), target.layer_uid())
                .is_some_and(|d| d.duplicator.effective_clone_count() > 1.)
        }))
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
            // A mask joins only under the layer's own 3D switch (its contour is projected with the layer).
            || (record.is_mask() && !(still_images && record.family == VECTOR_FAMILY))
        {
            return false;
        }
    }
    let on_layer = |t: &crate::object_document::ObjectTarget| {
        t.context_id() == target.context_id() && t.layer_uid() == target.layer_uid()
    };
    let Some(items) = document.item_records() else {
        return found;
    };
    // A 3D layer places plain still images and Text (glyph vertices through the layer projector); bitmap-brush and
    // brush-texture items stay unjoined.
    let images = still_images
        && items.raster_entries().iter().filter(|e| on_layer(&e.target)).all(|e| {
            e.item.is_bitmap_brush() != Some(true)
                && e.item.brush_group_id().is_none()
                && e.item.is_brush_texture_copy() != Some(true)
        });
    let has_images = items.targets().any(|t| on_layer(t));
    if has_images {
        images && !layer_has_duplicator_item(document, target)
    } else {
        found
    }
}

fn layer_has_duplicator_item(document: &ObjectDocument, target: &PropertyHolderTarget) -> bool {
    document
        .duplicator_set()
        .is_some_and(|set| set.duplicator(target.context_id(), target.layer_uid()).is_some())
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
