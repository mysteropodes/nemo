//! One placed component contour: the evaluated camera -> symMatrix path with its post-override style,
//! after the inner layer, held source and raw opacity checks, and the layer gates the component's other
//! paints share.
use crate::evaluation::object_scene_style_kernels::object_scene_producer_component_select::ComponentShape;
use crate::evaluation::object_scene_style_kernels::object_scene_producer_component_shape::ShapeData;
use crate::evaluation::object_scene_style_kernels::object_scene_style_item::{
    style_of_record_with_opacity, LEGACY_FAMILY,
};
use crate::evaluation::object_scene_style_kernels::object_scene_style_paint::FillResolution;
use crate::evaluation::object_vector_kernels::render_vector_stroke::stroke_reach;
use crate::object_frame_packet::{sample_property_raw, ObjectFramePacket};
use crate::object_frame_packet::{
    FrameScopeKind, ObjectLayer, Presence, PropertyHolderTarget, VECTOR_FAMILY,
};
use crate::object_render_scene::component_scene::{unavailable, ComponentPaint};
use crate::object_render_scene::object_scene_style_emit::prepare_item;
use crate::object_render_scene::render_vector_path::VectorShape;
use crate::object_render_scene::ObjectSceneError;
use crate::render_geometry::{ClosedCubicPath, CubicSegment};

pub(crate) fn layer_supported(
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

pub(crate) fn raw_opacity(
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

pub(crate) fn compile(
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
