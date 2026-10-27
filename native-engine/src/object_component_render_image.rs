//! A placed component's images. Each raster item of the component context that an inner layer holds
//! at the resolved inner frame is placed as the baseline's `symbolId` branch places an `isRaster`
//! dict. Camera, then symMatrix, each go through `applyMatrixToStrokeData`: the centre is mapped
//! by the matrix, width and height are scaled by its column scales, and rotation and every other
//! tag are dropped. With neither matrix, the stored rect is kept. Opacity is the item's own,
//! times the inner layer's raw opacity, times the host's. Element Motion never reaches a raster, and
//! the codec admits exposed properties only on the component's own objects.
//! Held text blocks are `text_scene::placed`, keyed here in the same source space.
use crate::evaluation::motion_parent_chain::ImageRect;
use crate::evaluation::object_scene_style_kernels::object_scene_producer_component::{
    evaluate_component_shapes, ResolvedComponentPlacement,
};
use crate::evaluation::object_scene_style_kernels::object_scene_producer_component_shape::{
    RasterRect, ShapeData,
};
use crate::object_frame_packet::{
    FrameScopeKind, ObjectFramePacket, ObjectLayer, Presence, PropertyHolderTarget,
};
use crate::object_render_scene::component_scene::contour::{layer_supported, raw_opacity};
use crate::object_render_scene::component_scene::unavailable;
use crate::object_render_scene::raster_scene::RasterSource;
use crate::object_render_scene::text_glyph_scene_kernels::object_raster_asset_scene::RasterSceneSource;
use crate::object_render_scene::ObjectSceneError;
use std::collections::HashMap;

/// The images and text blocks the inner layers hold at this frame, keyed by their position in the
/// component's order source space (its `local` objects, then the whole raster table, then the whole
/// text table): `(inner layer, entry, is_text)`.
pub(crate) fn held(
    packet: &ObjectFramePacket,
    local: usize,
    symbol: &str,
    layers: &HashMap<&str, usize>,
    source_frames: &[Option<u32>],
    host: usize,
) -> Result<HashMap<usize, (usize, usize, bool)>, ObjectSceneError> {
    let mut images = HashMap::new();
    let Some(items) = packet.snapshot().document().item_records() else {
        return Ok(images);
    };
    let held_on = |target: &crate::object_frame_packet::ObjectTarget| {
        if target.context_id() != symbol || target.scope_kind() != &FrameScopeKind::Authored {
            return Ok(None);
        }
        let layer = *layers
            .get(target.layer_uid())
            .ok_or_else(|| unavailable(host, "component item names an unknown layer"))?;
        Ok(Some(layer).filter(|&at| source_frames[at] == Some(target.frame())))
    };
    for (entry, raster) in items.raster_entries().iter().enumerate() {
        if let Some(layer) = held_on(&raster.target)? {
            images.insert(local + entry, (layer, entry, false));
        }
    }
    let texts = local + items.raster_entries().len();
    for (entry, text) in items.text_entries().iter().enumerate() {
        if let Some(layer) = held_on(&text.target)? {
            images.insert(texts + entry, (layer, entry, true));
        }
    }
    Ok(images)
}

/// One held image as the host paints it.
pub(crate) fn raster(
    packet: &ObjectFramePacket,
    layer: &ObjectLayer,
    placement: &ResolvedComponentPlacement,
    entry: usize,
    host: usize,
    host_factor: f64,
    inner_frame: u32,
) -> Result<RasterSource, ObjectSceneError> {
    let raster = packet
        .snapshot()
        .document()
        .item_records()
        .and_then(|items| items.raster_entries().get(entry))
        .ok_or_else(|| unavailable(host, "component image is unavailable"))?;
    let (target, item) = (&raster.target, &raster.item);
    if layer.layer_uid() != target.layer_uid() {
        return Err(unavailable(host, "component source layer is unavailable"));
    }
    layer_supported(packet, target.context_id(), layer, host)?;
    if item.is_bitmap_brush() == Some(true)
        || item.brush_group_id().is_some()
        || item.is_brush_texture_copy() == Some(true)
    {
        return Err(unavailable(
            host,
            "component raster brush paint is unavailable",
        ));
    }
    let rect = if placement.chain().is_empty() {
        if item.mesh_id().is_some() {
            return Err(unavailable(host, "component image mesh is unavailable"));
        }
        let rect = item.display_rect();
        ImageRect {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: rect.height,
            rotation: rect.rotation,
        }
    } else {
        let (x, y) = item.center();
        let (width, height) = item.size();
        let stored = ShapeData::Raster(RasterRect {
            flag: serde_json::Value::Bool(true),
            src: None,
            x,
            y,
            width,
            height,
            opacity: None,
            rest: serde_json::Map::new(),
        });
        let placed = evaluate_component_shapes(std::slice::from_ref(&stored), placement)
            .map_err(|_| unavailable(host, "component image placement is not finite"))?;
        let Some(ShapeData::Raster(moved)) = placed.shapes.first().map(|shape| shape.data()) else {
            return Err(unavailable(
                host,
                "component image placement is unavailable",
            ));
        };
        ImageRect {
            x: moved.x - moved.width / 2.0,
            y: moved.y - moved.height / 2.0,
            width: moved.width,
            height: moved.height,
            rotation: 0.0,
        }
    };
    let layer_target = PropertyHolderTarget {
        context_id: target.context_id().to_owned(),
        layer_uid: target.layer_uid().to_owned(),
        stroke_id: Presence::Missing,
    };
    let opacity = item.opacity()
        * raw_opacity(packet, &layer_target, f64::from(inner_frame), host)?
        * host_factor;
    if !(opacity as f32).is_finite() {
        return Err(unavailable(
            host,
            "component image opacity is not representable",
        ));
    }
    Ok(RasterSource::placed(
        packet.source_objects().len() + entry,
        host,
        RasterSceneSource {
            asset_id: item.asset_id().as_str().to_owned(),
            rect,
            opacity,
        },
    ))
}

/// One held text block as the host paints it: its glyph paths through the placement chain (camera,
/// then symMatrix), at the item opacity times the inner layer's raw opacity times the host's.
#[allow(clippy::too_many_arguments)]
pub(crate) fn text(
    packet: &ObjectFramePacket,
    layer: &ObjectLayer,
    placement: &ResolvedComponentPlacement,
    entry: usize,
    host: usize,
    host_factor: f64,
    inner_frame: u32,
    texts: &mut crate::object_render_scene::text_scene::TextSources,
) -> Result<usize, ObjectSceneError> {
    let items = packet
        .snapshot()
        .document()
        .item_records()
        .ok_or_else(|| unavailable(host, "component Text block is unavailable"))?;
    let target = &items
        .text_entries()
        .get(entry)
        .ok_or_else(|| unavailable(host, "component Text block is unavailable"))?
        .target;
    layer_supported(packet, target.context_id(), layer, host)?;
    let layer_target = PropertyHolderTarget {
        context_id: target.context_id().to_owned(),
        layer_uid: target.layer_uid().to_owned(),
        stroke_id: Presence::Missing,
    };
    let opacity =
        raw_opacity(packet, &layer_target, f64::from(inner_frame), host)? * host_factor;
    let chain: Vec<[f64; 6]> = placement.chain().iter().map(|m| m.0).collect();
    crate::object_render_scene::text_scene::placed::placed(
        packet,
        layer,
        entry,
        packet.source_objects().len() + items.raster_entries().len() + entry,
        host,
        &chain,
        opacity,
        texts,
    )
}
