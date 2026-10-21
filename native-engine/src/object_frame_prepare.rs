//! Shared immutable frame construction; caller supplies only its proven Motion port.
use crate::object_frame_packet::{
    duplicators, holders_fully_consumed, sources, text, FrameContext, NumericOrderPlan,
    ObjectFrameError, ObjectFrameIndices, ObjectFramePacket, ObjectFrameRasterIndex,
    ObjectFrameSelector,
};
use crate::revision::ObjectSnapshot;
use std::collections::HashMap;

pub(super) fn prepare_frame_with_order<E, F>(
    snapshot: &ObjectSnapshot,
    selector: &ObjectFrameSelector<'_>,
    exposure: bool,
    prepare_order: F,
    motion: Option<
        &dyn crate::evaluation::object_component_kernels::object_effector_layers::MotionPort,
    >,
) -> Result<ObjectFramePacket, E>
where
    E: From<ObjectFrameError>,
    F: FnOnce(&ObjectSnapshot, &str, u32, &[Option<u32>], bool) -> Result<NumericOrderPlan, E>,
{
    let context = FrameContext::admit(snapshot, selector, exposure)?;
    let document = snapshot.document();
    let context_layers = context.layers(document);
    let exposure = exposure || context.is_component();
    let context_objects = context.objects(document);
    let v2 = document.format_version() == 2;
    // Admit evaluated keys only with the actual owner-bound Motion producer.
    let consumed = if motion.is_some() {
        crate::evaluation::object_vector_kernels::render_vector_eval::params::holders_fully_consumed_with_motion(document)
    } else {
        holders_fully_consumed(document)
    };
    if snapshot.frame_needs_camera_or_3d_join() || !consumed {
        return Err(ObjectFrameError::Unavailable.into());
    }
    let source_frames = sources::select(
        snapshot,
        selector,
        context_layers,
        exposure,
        context.is_component(),
    )?;
    let numeric_order = if v2 {
        Some(prepare_order(
            snapshot,
            selector.context_id,
            selector.frame,
            &source_frames,
            exposure,
        )?)
    } else {
        None
    };
    let layers: HashMap<_, _> = context_layers
        .iter()
        .enumerate()
        .map(|(index, layer)| (layer.layer_uid(), index))
        .collect();
    let mut records = Vec::new();
    for (source_object_index, record) in context_objects.iter().enumerate() {
        let target = record.target();
        if target.context_id() == selector.context_id && target.scope_kind() == &selector.scope_kind
        {
            let source_layer_index = *layers
                .get(target.layer_uid())
                .ok_or(ObjectFrameError::InvalidSource)?;
            if source_frames[source_layer_index] != Some(target.frame())
                || !context_layers[source_layer_index].visible()
            {
                continue;
            }
            records.push(ObjectFrameIndices {
                source_layer_index,
                source_object_index,
            });
        }
    }
    let mut raster_records = Vec::new();
    if let Some(items) = document.item_records() {
        for (entry, record) in items.raster_entries().iter().enumerate() {
            let target = &record.target;
            if target.context_id() == selector.context_id
                && target.scope_kind() == &selector.scope_kind
            {
                let layer = *layers
                    .get(target.layer_uid())
                    .ok_or(ObjectFrameError::InvalidSource)?;
                if source_frames[layer] == Some(target.frame()) && context_layers[layer].visible() {
                    raster_records.push(ObjectFrameRasterIndex { layer, entry });
                }
            }
        }
    }
    let text_records = text::select(snapshot, selector, context_layers, &source_frames)?;
    let mut packet = ObjectFramePacket {
        duplicators: Vec::new(),
        motion_receipts: Vec::new(),
        text_records,
        raster_records,
        snapshot: snapshot.clone(),
        context_id: selector.context_id.to_owned(),
        context,
        scope_kind: selector.scope_kind.clone(),
        frame: selector.frame,
        records,
        source_frames,
        numeric_order,
    };
    if let Some(motion) = motion {
        packet.motion_receipts =
            crate::object_frame_packet::motion_receipts::capture(&packet, motion)?;
    }
    packet.duplicators = duplicators::prepare(&packet, exposure, motion)?;
    Ok(packet)
}
