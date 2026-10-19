//! Placed-component paints from the same immutable packet used by viewport/export.
//! Geometry is already camera -> symMatrix evaluated. Source style remains the
//! post-override record: gradients and stroke widths are not transformed again.
use crate::evaluation::object_component_kernels::object_component_evaluate::evaluate_packet;
use crate::evaluation::object_component_kernels::object_component_exposed::{
    ExposedEntry, ExposedValue,
};
use crate::evaluation::object_component_kernels::object_component_exposed_apply::OverridePort;
use crate::evaluation::object_scene_style_kernels::object_scene_producer_component_join::NoPorts;
use crate::evaluation::object_scene_style_kernels::object_scene_producer_component_scene::{
    ComponentOutcome, ComponentScene,
};
use crate::evaluation::object_scene_style_kernels::object_scene_producer_component_select::{
    component_holder_consumed, ComponentShape, ComponentUnavailable,
};
use crate::evaluation::object_scene_style_kernels::object_scene_producer_component_shape::ShapeData;
use crate::evaluation::object_scene_style_kernels::object_scene_producer_pin::ScenePin;
use crate::evaluation::object_scene_style_kernels::object_scene_style_item::{
    style_of_record_with_opacity, SceneItemStyle, LEGACY_FAMILY,
};
use crate::evaluation::object_scene_style_kernels::object_scene_style_paint::FillResolution;
use crate::evaluation::object_vector_kernels::render_vector_stroke::stroke_reach;
use crate::object_frame_packet::{
    prepare_scoped_order_from_sources, sample_property_raw, ObjectFramePacket, ObjectFrameSelector,
};
use crate::object_frame_packet::{
    FrameScopeKind, ObjectLayer, ObjectTarget, Presence, PropertyHolderTarget, VECTOR_FAMILY,
};
use crate::object_render_scene::object_scene_style_emit::{prepare_item, PreparedItem};
use crate::object_render_scene::render_vector_path::VectorShape;
use crate::object_render_scene::{validate_pins, ObjectSceneError};
use crate::render_geometry::{ClosedCubicPath, CubicSegment};
use std::collections::{HashMap, HashSet};
use vello::kurbo::BezPath;

#[derive(Debug, Clone)]
pub(crate) struct ComponentSources {
    pub(crate) scene: Option<ComponentScene>,
    pub(crate) paints: Vec<ComponentPaint>,
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
}

struct UnavailableOverrides;
impl OverridePort for UnavailableOverrides {
    fn available(&mut self) -> bool {
        false
    }
    fn register(&mut self, _property: &ExposedEntry) {}
    fn read(&mut self, _layer: &str, _key: &str, _frame: u32) -> Result<Vec<ExposedValue>, String> {
        Err("component exposed-property runtime is not wired".to_owned())
    }
}

fn unavailable(index: usize, reason: &'static str) -> ObjectSceneError {
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

fn layer_supported(
    packet: &ObjectFramePacket,
    context: &str,
    layer: &ObjectLayer,
    host: usize,
) -> Result<(), ObjectSceneError> {
    if layer.blend_mode().is_some_and(|mode| mode != "normal") {
        return Err(unavailable(
            host,
            "component layer blend mode requires a layered consumer",
        ));
    }
    if layer.parent_layer_uid.value().is_some_and(Option::is_some)
        || layer.rig().is_some()
        || layer.is_effect_layer()
        || layer.is_folder_layer()
        || layer
            .effects()
            .is_some_and(|effects| effects.iter().any(|effect| effect.enabled))
        || layer.effects_from().is_some()
        || layer.matte_on()
        || !layer.mattes_more().is_empty()
    {
        return Err(unavailable(
            host,
            "component layer needs an unjoined parent/rig/effect provider",
        ));
    }
    if layer.path_fx().is_some_and(|effects| {
        effects
            .iter()
            .any(|effect| effect.enabled_flag() != Some(false))
    }) {
        return Err(unavailable(
            host,
            "component layer path effects are unavailable",
        ));
    }
    if packet
        .snapshot()
        .document()
        .duplicator_set()
        .is_some_and(|set| set.duplicator(context, layer.layer_uid()).is_some())
    {
        return Err(unavailable(
            host,
            "component duplicator consumer is unavailable",
        ));
    }
    Ok(())
}

fn raw_opacity(
    packet: &ObjectFramePacket,
    target: &PropertyHolderTarget,
    frame: f64,
    host: usize,
) -> Result<f64, ObjectSceneError> {
    let values = sample_property_raw(packet.snapshot(), target, "opacity", frame)
        .map_err(|_| unavailable(host, "component raw opacity sampler refused"))?;
    match values.as_slice() {
        [value] if value.is_finite() => Ok((value / 100.0).max(0.0)),
        _ => Err(unavailable(
            host,
            "component raw opacity must be one finite number",
        )),
    }
}

fn compile(
    packet: &ObjectFramePacket,
    shape: &ComponentShape,
    host: usize,
    host_factor: f64,
    inner_frame: f64,
    symbol: &str,
) -> Result<ComponentPaint, ObjectSceneError> {
    let source = shape.source.as_ref();
    let target = source.target();
    if target != &shape.target
        || target.context_id() != symbol
        || target.scope_kind() != &FrameScopeKind::Authored
    {
        return Err(unavailable(
            host,
            "component source and evaluated target differ",
        ));
    }
    let component = packet
        .snapshot()
        .document()
        .component_set()
        .and_then(|set| set.component(symbol))
        .ok_or_else(|| unavailable(host, "component source table is unavailable"))?;
    let layer = component
        .layers()
        .iter()
        .find(|layer| layer.layer_uid() == target.layer_uid())
        .ok_or_else(|| unavailable(host, "component source layer is unavailable"))?;
    layer_supported(packet, symbol, layer, host)?;
    let source_frame = layer.timing.value().and_then(|timing| {
        component.total_frames().checked_sub(1).and_then(|last| {
            if inner_frame >= 0.0 && inner_frame.fract() == 0.0 && inner_frame <= f64::from(last) {
                timing.exposure(inner_frame as u32).1
            } else {
                None
            }
        })
    });
    if source_frame != Some(target.frame()) {
        return Err(unavailable(
            host,
            "component held source differs from resolved inner frame",
        ));
    }
    if source.has_enabled_effects() {
        return Err(unavailable(
            host,
            "component element effects are unavailable",
        ));
    }
    let layer_target = PropertyHolderTarget {
        context_id: symbol.to_owned(),
        layer_uid: target.layer_uid().to_owned(),
        stroke_id: Presence::Missing,
    };
    let element_target = PropertyHolderTarget {
        stroke_id: Presence::Present(target.stroke_id().to_owned()),
        ..layer_target.clone()
    };
    let factor = host_factor
        * raw_opacity(packet, &layer_target, inner_frame, host)?
        * raw_opacity(packet, &element_target, inner_frame, host)?;
    let style = style_of_record_with_opacity(source, factor)
        .map_err(|error| unavailable(host, error.message()))?;
    let prepared = prepare_item(&style).map_err(|error| unavailable(host, error.message()))?;
    let ShapeData::Path { segments, rest } = shape.shape.data() else {
        return Err(unavailable(host, "component has no admitted cubic path"));
    };
    if rest.get("closed").and_then(serde_json::Value::as_bool) != Some(source.geometry.closed)
        || source.geometry.kind != "cubic-path"
        || source.geometry.coordinate_space != "document-world"
        || source.geometry.handle_space != "relative"
        || !matches!(source.family.as_str(), LEGACY_FAMILY | VECTOR_FAMILY)
    {
        return Err(unavailable(
            host,
            "component source geometry is unsupported",
        ));
    }
    let segments: Vec<_> = segments
        .iter()
        .map(|segment| CubicSegment {
            point: segment.point,
            handle_in: segment.handle_in,
            handle_out: segment.handle_out,
        })
        .collect();
    // Reuse the existing vector admission for open/closed paths and GPU bounds.
    // Its clipping contour is validation metadata only and is never retained here.
    let validated = VectorShape::new(&segments, source.geometry.closed, stroke_reach(&style))
        .map_err(|reason| unavailable(host, reason))?;
    let contour = if source.geometry.closed {
        ClosedCubicPath::new(segments).ok()
    } else {
        None
    };
    if source.family == LEGACY_FAMILY && contour.is_none() {
        return Err(unavailable(
            host,
            "legacy component fill requires a real closed contour",
        ));
    }
    let svg_rgba = if source.family == LEGACY_FAMILY {
        match &style.fill {
            FillResolution::Solid(solid) if style.stroke.is_none() => {
                let mut rgba = [0.0; 4];
                for (value, channel) in rgba.iter_mut().zip(solid.channels()) {
                    *value = channel
                        .as_f64()
                        .ok_or_else(|| unavailable(host, "component SVG paint is not finite"))?;
                }
                Some(rgba)
            }
            _ => {
                return Err(unavailable(
                    host,
                    "legacy component fill has no solid style",
                ))
            }
        }
    } else {
        None
    };
    Ok(ComponentPaint {
        host_layer_index: host,
        target: target.clone(),
        contour,
        path: validated.path,
        style,
        prepared,
        svg_rgba,
    })
}

pub(crate) fn prepare(
    packet: &ObjectFramePacket,
    expected: &ObjectFrameSelector<'_>,
) -> Result<ComponentSources, ObjectSceneError> {
    validate_pins(packet, expected)?;
    let empty = || ComponentSources {
        scene: None,
        paints: Vec::new(),
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
    let scene = evaluate_packet(packet, &pin, None, &mut NoPorts, &mut UnavailableOverrides)
        .map_err(|_| unavailable(first_host, "component production or pin join refused"))?
        .ok_or_else(|| unavailable(first_host, "component producer omitted its instance table"))?;
    let mut paints = Vec::new();
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
                for layer in order.layers() {
                    for element in layer.elements() {
                        let index = element.source_object_index();
                        let source = sources.get(&index).ok_or_else(|| {
                            unavailable(
                                entry.source_layer_index,
                                "component order names an unselected source",
                            )
                        })?;
                        let shape = *shapes.get(&key(source.1.target())).ok_or_else(|| {
                            unavailable(
                                entry.source_layer_index,
                                "component order and evaluated shapes disagree",
                            )
                        })?;
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
                    }
                }
                if seen_sources.len() != sources.len() || seen_shapes.len() != placed.shapes.len() {
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
    })
}
