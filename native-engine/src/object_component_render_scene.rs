//! Placed-component paints from the same immutable packet used by viewport/export.
//! Geometry is already camera -> symMatrix evaluated. Source style remains the
//! post-override record: gradients and stroke widths are not transformed again.
//! A placed component's images join the scene's raster sources and paint at the
//! host slot in the inner stack order (`sequence`).
use crate::evaluation::object_component_kernels::object_component_evaluate::evaluate_packet;
use crate::evaluation::object_component_kernels::object_component_override_port::{
    absent_source_allowed, PacketOverrides,
};
use crate::evaluation::object_scene_style_kernels::object_scene_producer_component_join::NoPorts;
use crate::evaluation::object_scene_style_kernels::object_scene_producer_component_scene::{
    ComponentOutcome, ComponentScene,
};
use crate::evaluation::object_scene_style_kernels::object_scene_producer_component_select::{
    component_holder_consumed, ComponentUnavailable,
};
use crate::evaluation::object_scene_style_kernels::object_scene_producer_pin::ScenePin;
use crate::evaluation::object_scene_style_kernels::object_scene_style_item::SceneItemStyle;
use crate::object_frame_packet::{
    prepare_scoped_order_from_sources, ObjectFramePacket, ObjectFrameSelector,
};
use crate::object_frame_packet::{FrameScopeKind, ObjectTarget};
use crate::object_render_scene::component_scene::contour::{compile, layer_supported};
use crate::object_render_scene::object_scene_style_emit::PreparedItem;
use crate::object_render_scene::raster_scene::RasterSource;
use crate::object_render_scene::{validate_pins, ObjectSceneError, PaintIndex};
use crate::render_geometry::ClosedCubicPath;
use std::collections::{HashMap, HashSet};
use vello::kurbo::BezPath;

#[path = "object_component_render_contour.rs"]
mod contour;
#[path = "object_component_render_image.rs"]
mod image;

#[derive(Debug, Clone)]
pub(crate) struct ComponentSources {
    pub(crate) scene: Option<ComponentScene>,
    pub(crate) paints: Vec<ComponentPaint>,
    /// Every placed paint in the inner stack order: its host layer and `Component` (a paint above)
    /// or `Raster` (an image appended to the scene's raster sources).
    pub(crate) sequence: Vec<(usize, PaintIndex)>,
}

#[derive(Debug, Clone)]
pub(crate) struct ComponentPaint {
    pub(crate) host_layer_index: usize,
    pub(crate) target: ObjectTarget,
    /// Only a genuine admitted closed contour, never an open path's clip rectangle.
    pub(crate) contour: Option<ClosedCubicPath>,
    pub(crate) path: BezPath,
    pub(crate) style: SceneItemStyle,
    pub(crate) prepared: PreparedItem,
    /// SVG's currently admitted legacy closed-solid subset, straight and unquantized.
    pub(crate) svg_rgba: Option<[f64; 4]>,
    /// The post-override inner record as the baseline's dict holds it (untransformed geometry).
    pub(crate) record: crate::object_frame_packet::ObjectRecord,
}

pub(crate) fn unavailable(index: usize, reason: &'static str) -> ObjectSceneError {
    ObjectSceneError::UnsupportedComponent {
        source_layer_index: index,
        reason,
    }
}

fn provider_reason(reason: &ComponentUnavailable) -> &'static str {
    match reason {
        ComponentUnavailable::TimeRemap(reason) | ComponentUnavailable::Camera(reason) => *reason,
        ComponentUnavailable::InnerFeature { feature, .. } => *feature,
        ComponentUnavailable::Geometry { reason, .. } => *reason,
        ComponentUnavailable::OverridesUnavailable | ComponentUnavailable::Overrides(_) => {
            "component exposed-property runtime is unavailable"
        }
        _ => "component evaluation requires an unavailable native provider",
    }
}

/// `rasters` is the scene's raster sources; each placed image is appended to it.
pub(crate) fn prepare(
    packet: &ObjectFramePacket,
    expected: &ObjectFrameSelector<'_>,
    rasters: &mut Vec<RasterSource>,
) -> Result<ComponentSources, ObjectSceneError> {
    validate_pins(packet, expected)?;
    let empty = || ComponentSources {
        scene: None,
        paints: Vec::new(),
        sequence: Vec::new(),
    };
    if packet.context_id() != "scene-root" {
        return Ok(empty());
    }
    let Some(table) = packet.snapshot().document().component_set() else {
        return Ok(empty());
    };
    if table.instances.is_empty() {
        return Ok(empty());
    }
    if packet.scope_kind() != &FrameScopeKind::Authored {
        return Err(ObjectSceneError::WrongScope);
    }
    let first_host = packet
        .source_layers()
        .iter()
        .position(|layer| layer.layer_uid() == table.instances[0].layer_uid())
        .ok_or(ObjectSceneError::InvalidContext)?;
    let pin = ScenePin {
        instance_id: expected.instance_id.to_owned(),
        document_id: expected.document_id.to_owned(),
        content_revision: expected.content_revision,
        document_snapshot_id: expected.document_snapshot_id.to_owned(),
        context_id: expected.context_id.to_owned(),
        scope_kind: expected.scope_kind.clone(),
        frame: expected.frame,
    };
    let scene = evaluate_packet(
        packet,
        &pin,
        None,
        &mut NoPorts,
        &mut PacketOverrides::new(packet),
    )
    .map_err(|_| unavailable(first_host, "component production or pin join refused"))?
    .ok_or_else(|| unavailable(first_host, "component producer omitted its instance table"))?;
    let mut paints = Vec::new();
    let mut sequence = Vec::new();
    for slot in scene.slots() {
        let entry = slot.entry();
        if packet
            .selected_source_indices()
            .any(|(_, layer)| layer == entry.source_layer_index)
        {
            return Err(unavailable(
                entry.source_layer_index,
                "component host contains selected root content; replacement is unavailable",
            ));
        }
        match &entry.outcome {
            ComponentOutcome::Blank => {}
            ComponentOutcome::Unavailable(reason) => {
                return Err(unavailable(
                    entry.source_layer_index,
                    provider_reason(reason),
                ));
            }
            ComponentOutcome::Shapes(placed) => {
                let host = packet
                    .source_layers()
                    .get(entry.source_layer_index)
                    .ok_or_else(|| {
                        unavailable(entry.source_layer_index, "component host layer is missing")
                    })?;
                layer_supported(packet, "scene-root", host, entry.source_layer_index)?;
                if let Some(holders) = packet.snapshot().document().property_holders() {
                    for holder in holders
                        .iter()
                        .filter(|holder| holder.target().context_id() == entry.symbol_id.as_str())
                    {
                        if component_holder_consumed(packet.snapshot().document(), holder)
                            != Some(true)
                        {
                            return Err(unavailable(
                                entry.source_layer_index,
                                "component Motion holder is not consumed",
                            ));
                        }
                    }
                }
                let host_factor = packet
                    .layer_opacity(entry.source_layer_index)
                    .map_err(|_| {
                        unavailable(
                            entry.source_layer_index,
                            "component host opacity is unavailable",
                        )
                    })?;
                let component = table.component(&entry.symbol_id).ok_or_else(|| {
                    unavailable(
                        entry.source_layer_index,
                        "component source table is missing",
                    )
                })?;
                let Some(frame) = placed.inner_frame.index(component.total_frames() as usize)
                else {
                    if !placed.shapes.is_empty() {
                        return Err(unavailable(
                            entry.source_layer_index,
                            "unresolved component frame carries shapes",
                        ));
                    }
                    continue;
                };
                let frame = frame as u32;
                let mut source_frames = Vec::with_capacity(component.layers().len());
                let mut layers = HashMap::new();
                for (index, layer) in component.layers().iter().enumerate() {
                    layers.insert(layer.layer_uid(), index);
                    source_frames.push(if layer.visible() {
                        layer
                            .timing
                            .value()
                            .ok_or_else(|| {
                                unavailable(
                                    entry.source_layer_index,
                                    "component inner timing is unavailable",
                                )
                            })?
                            .exposure(frame)
                            .1
                    } else {
                        None
                    });
                }
                let order = prepare_scoped_order_from_sources(
                    packet.snapshot(),
                    &entry.symbol_id,
                    frame,
                    &source_frames,
                )
                .map_err(|_| {
                    unavailable(
                        entry.source_layer_index,
                        "component scoped order provider refused",
                    )
                })?;
                let mut sources = HashMap::new();
                for (index, record) in component.objects().iter().enumerate() {
                    let target = record.target();
                    let layer = *layers.get(target.layer_uid()).ok_or_else(|| {
                        unavailable(
                            entry.source_layer_index,
                            "component source names an unknown layer",
                        )
                    })?;
                    if target.scope_kind() == &FrameScopeKind::Authored
                        && source_frames[layer] == Some(target.frame())
                    {
                        sources.insert(index, (layer, record));
                    }
                }
                let images = image::held(
                    packet,
                    component.objects().len(),
                    &entry.symbol_id,
                    &layers,
                    &source_frames,
                    entry.source_layer_index,
                )?;
                let key = |target: &ObjectTarget| {
                    (
                        target.layer_uid().to_owned(),
                        target.stroke_id().to_owned(),
                        target.frame(),
                    )
                };
                let mut shapes = HashMap::new();
                for (index, shape) in placed.shapes.iter().enumerate() {
                    if shapes.insert(key(&shape.target), index).is_some() {
                        return Err(unavailable(
                            entry.source_layer_index,
                            "component evaluated target is duplicated",
                        ));
                    }
                }
                let mut seen_sources = HashSet::new();
                let mut seen_shapes = HashSet::new();
                let mut seen_images = HashSet::new();
                for layer in order.layers() {
                    for element in layer.elements() {
                        let index = element.source_object_index();
                        if let Some(&(at, item)) = images.get(&index) {
                            if at != layer.source_layer_index() || !seen_images.insert(index) {
                                return Err(unavailable(
                                    entry.source_layer_index,
                                    "component order/source correlation is not unique",
                                ));
                            }
                            let placement = placed.placement.as_ref().ok_or_else(|| {
                                unavailable(
                                    entry.source_layer_index,
                                    "component placement is unavailable",
                                )
                            })?;
                            rasters.push(image::raster(
                                packet,
                                &component.layers()[at],
                                placement,
                                item,
                                entry.source_layer_index,
                                host_factor,
                                frame,
                            )?);
                            sequence.push((
                                entry.source_layer_index,
                                PaintIndex::Raster(rasters.len() - 1),
                            ));
                            continue;
                        }
                        let source = sources.get(&index).ok_or_else(|| {
                            unavailable(
                                entry.source_layer_index,
                                "component order names an unselected source",
                            )
                        })?;
                        let Some(&shape) = shapes.get(&key(source.1.target())) else {
                            // Only a source the override evaluator reported hidden may be absent.
                            absent_source_allowed(&placed.hidden, source.1.target())
                                .map_err(|reason| unavailable(entry.source_layer_index, reason))?;
                            if !seen_sources.insert(index) {
                                return Err(unavailable(
                                    entry.source_layer_index,
                                    "component order/source correlation is not unique",
                                ));
                            }
                            continue;
                        };
                        if source.0 != layer.source_layer_index()
                            || !seen_sources.insert(index)
                            || !seen_shapes.insert(shape)
                        {
                            return Err(unavailable(
                                entry.source_layer_index,
                                "component order/source correlation is not unique",
                            ));
                        }
                        paints.push(compile(
                            packet,
                            &placed.shapes[shape],
                            entry.source_layer_index,
                            host_factor,
                            f64::from(frame),
                            &entry.symbol_id,
                        )?);
                        sequence.push((
                            entry.source_layer_index,
                            PaintIndex::Component(paints.len() - 1),
                        ));
                    }
                }
                if seen_sources.len() != sources.len()
                    || seen_shapes.len() != placed.shapes.len()
                    || seen_images.len() != images.len()
                {
                    return Err(unavailable(
                        entry.source_layer_index,
                        "component order is not a complete source/shape bijection",
                    ));
                }
            }
        }
    }
    Ok(ComponentSources {
        scene: Some(scene),
        paints,
        sequence,
    })
}
