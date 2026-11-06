//! Detached duplicator paints in the existing native scene. Original packet
//! provenance, selection identities, source order and resource leases stay intact.
use crate::evaluation::object_vector_kernels::render_vector_eval::{
    duplicator_geometry, is_vector_record,
};
use crate::evaluation::object_vector_kernels::render_vector_stroke::stroke_reach;
use crate::object_frame_packet::{ObjectFramePacket, ObjectTarget};
use crate::object_render_scene::object_scene_style_emit::{prepare_item, PreparedItem};
use crate::object_render_scene::render_vector_path::VectorShape;
use crate::object_render_scene::{
    ObjectPaintPlanError, ObjectRenderScene, ObjectSceneError, PaintIndex,
};
use crate::render_geometry::{ClosedCubicPath, CubicSegment};
use std::collections::BTreeMap;
use vello::kurbo::BezPath;

#[path = "object_duplicator_scene_three_d.rs"]
mod three_d;

#[derive(Debug, Clone)]
pub(crate) struct DuplicatorPaint {
    pub(crate) layer_index: usize,
    pub(crate) source_target: ObjectTarget,
    pub(crate) clone_id: String,
    pub(crate) dup_index: u32,
    pub(crate) source_index: usize,
    order: f64,
    pub(crate) path: BezPath,
    pub(crate) prepared: PreparedItem,
    pub(crate) svg_contour: Option<ClosedCubicPath>,
    pub(crate) svg_rgba: Option<[f64; 4]>,
    /// The copy as the baseline's dict holds it (`duplicators::baseline_copy`): moved geometry,
    /// rotated paints, factored opacity, the source's stroke id under the host's layer uid.
    pub(crate) record: crate::object_frame_packet::ObjectRecord,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct DuplicatorScene {
    pub(crate) paints: Vec<DuplicatorPaint>,
    layers: BTreeMap<usize, Vec<usize>>,
}

/// One clone's authored geometry in document space: a vector record through the geometry evaluator, any other record as stored.
fn copy_segments(
    packet: &ObjectFramePacket,
    record: &crate::object_frame_packet::ObjectRecord,
    layer_index: usize,
) -> Result<(Vec<CubicSegment>, bool), &'static str> {
    if is_vector_record(record) {
        let (segments, closed) = duplicator_geometry(packet, record, layer_index).map_err(|e| e.message())?;
        return Ok((
            segments
                .into_iter()
                .map(|s| CubicSegment { point: s.point, handle_in: s.handle_in, handle_out: s.handle_out })
                .collect(),
            closed,
        ));
    }
    let number = |v: &serde_json::Number| v.as_f64().filter(|v| v.is_finite()).ok_or("nonfinite duplicator point");
    let segments = record
        .geometry
        .segments
        .iter()
        .map(|s| {
            Ok(CubicSegment {
                point: [number(&s.point.x)?, number(&s.point.y)?],
                handle_in: [number(&s.handle_in.x)?, number(&s.handle_in.y)?],
                handle_out: [number(&s.handle_out.x)?, number(&s.handle_out.y)?],
            })
        })
        .collect::<Result<Vec<_>, &'static str>>()?;
    Ok((segments, record.geometry.closed))
}

pub(crate) fn prepare(packet: &ObjectFramePacket) -> Result<DuplicatorScene, ObjectSceneError> {
    let mut result = DuplicatorScene::default();
    for layer in packet.duplicator_outputs() {
        // #2520: a 3D host projects every clone around the union bounds of ALL its clones, each with its own `dup3D` delta.
        let (flat, projectors) = if packet.source_layers()[layer.layer_index].three_d.value() == Some(&true) {
            let host = packet.source_layers()[layer.layer_index].layer_uid().to_owned();
            let at = |copy_index: usize, reason| ObjectSceneError::UnsupportedGeometry {
                source_object_index: layer.source_indices[copy_index],
                reason,
            };
            let mut flat = Vec::with_capacity(layer.output.copies.len());
            for (copy_index, copy) in layer.output.copies.iter().enumerate() {
                let mut record = copy.record.clone();
                record.target.layer_uid = host.clone();
                flat.push(copy_segments(packet, &record, layer.layer_index).map_err(|r| at(copy_index, r))?);
            }
            let deltas: Vec<_> = layer
                .output
                .copies
                .iter()
                .map(|copy| {
                    copy.dup_3d.map(|d| crate::evaluation::property_projection_3d::ExtraDelta {
                        dz: Some(d.dz),
                        drx: Some(d.drx),
                        dry: Some(d.dry),
                        ..Default::default()
                    })
                })
                .collect();
            let (segments, closed): (Vec<_>, Vec<_>) = flat.iter().cloned().unzip();
            let projectors = three_d::projectors(packet, layer.layer_index, &segments, &closed, &deltas)
                .map_err(|r| at(0, r))?;
            (flat, projectors)
        } else {
            (Vec::new(), None)
        };
        let mut indices = Vec::with_capacity(layer.output.copies.len());
        for (copy_index, copy) in layer.output.copies.iter().enumerate() {
            let invalid = |reason| ObjectSceneError::UnsupportedGeometry {
                source_object_index: layer.source_indices[copy_index],
                reason,
            };
            let source = packet
                .snapshot()
                .document()
                .objects()
                .get(layer.source_indices[copy_index])
                .ok_or_else(|| invalid("duplicator original source is unavailable"))?;
            if !copy.record.is_mask()
                && (source.has_enabled_effects() || copy.record.has_enabled_effects())
            {
                return Err(invalid(
                    "per-element effect evaluation awaits its native scene consumer",
                ));
            }
            let placement = layer
                .output
                .placements
                .get(copy.dup_index as usize)
                .filter(|trace| trace.k == copy.dup_index)
                .ok_or_else(|| invalid("duplicator copy has no matching placement"))?;
            let mut record = copy.record.clone();
            // Pool/source identities stay on copy/source_target. Only this
            // temporary render view addresses the host's element/layer Motion.
            record.target.layer_uid = packet.source_layers()[layer.layer_index]
                .layer_uid()
                .to_owned();
            let style = if record.is_mask() {
                crate::evaluation::object_scene_style_kernels::object_scene_style_item::SceneItemStyle::solid_fill(
                    crate::object_render_scene::mask_inputs::white_solid().map_err(invalid)?)
            } else {
                // A 3D host ignores its 2D Motion opacity (baseline `is3D`).
                let host_opacity = if projectors.is_some() {
                    1.0
                } else {
                    packet
                        .layer_opacity(layer.layer_index)
                        .map_err(|_| invalid("duplicator host opacity is unavailable"))?
                };
                let opacity = host_opacity * placement.placement.opacity_factor;
                crate::object_frame_packet::duplicators::paint_style(
                    packet,
                    &record,
                    opacity,
                    placement.placement.hue_deg,
                )
                .map_err(invalid)?
            };
            let (segments, closed) = match flat.get(copy_index) {
                Some(found) => found.clone(),
                None => copy_segments(packet, &record, layer.layer_index).map_err(invalid)?,
            };
            let segments = match &projectors {
                Some(projectors) => three_d::project(&segments, &projectors[copy_index]),
                None => segments,
            };
            let shape =
                VectorShape::new(&segments, closed, stroke_reach(&style)).map_err(invalid)?;
            let prepared = prepare_item(&style).map_err(|e| invalid(e.message()))?;
            let svg_rgba = if closed && style.stroke.is_none() {
                style.fill.solid().and_then(|solid| {
                    let [r, g, b, a] = solid.channels();
                    let rgba = [r.as_f64()?, g.as_f64()?, b.as_f64()?, a.as_f64()?];
                    rgba.iter()
                        .all(|value| value.is_finite() && (0.0..=1.).contains(value))
                        .then_some(rgba)
                })
            } else {
                None
            };
            let order = crate::object_frame_packet::duplicators::paint_order(
                packet,
                copy.record.target(),
                layer.layer_index,
            )
            .map_err(invalid)?;
            indices.push(result.paints.len());
            result.paints.push(DuplicatorPaint {
                layer_index: layer.layer_index,
                source_target: copy.record.target().clone(),
                clone_id: copy.clone_id.clone(),
                dup_index: copy.dup_index,
                source_index: layer.source_indices[copy_index],
                order,
                path: shape.path,
                prepared,
                // VectorShape's contour may be a padded clipping rectangle.
                // SVG must use actual closed geometry, never that envelope.
                svg_contour: closed
                    .then(|| ClosedCubicPath::new(segments.clone()).ok())
                    .flatten(),
                svg_rgba,
                record: crate::object_frame_packet::duplicators::baseline_copy(
                    &record,
                    placement.placement.opacity_factor,
                    placement.placement.hue_deg,
                )
                .map_err(invalid)?,
            });
        }
        // Same stable descending element-Order sort as engine-bridge.js.
        // Equal default Order preserves the original clone-outer/source-inner loop.
        indices.sort_by(|&a, &b| result.paints[b].order.total_cmp(&result.paints[a].order));
        // A genuinely empty Object-mode result suppresses the host's content.
        result.layers.insert(layer.layer_index, indices);
    }
    Ok(result)
}

pub(crate) fn expand_order(
    scene: &ObjectRenderScene,
    indices: Vec<usize>,
    mixed: Vec<PaintIndex>,
) -> Result<(Vec<usize>, Vec<PaintIndex>), ObjectPaintPlanError> {
    if scene.duplicators.layers.is_empty() {
        return Ok((indices, mixed));
    }
    let order = scene
        .packet()
        .numeric_order()
        .ok_or(ObjectPaintPlanError::Unavailable)?;
    let layer_of = |index: &PaintIndex| -> Result<usize, ObjectPaintPlanError> {
        match *index {
            PaintIndex::Contour(i) => scene
                .entry(i)
                .map(|e| e.source().source_layer_index())
                .ok_or(ObjectPaintPlanError::InvalidOrder),
            PaintIndex::Raster(i) => scene
                .rasters
                .get(i)
                .map(|e| e.layer_index)
                .ok_or(ObjectPaintPlanError::InvalidOrder),
            PaintIndex::Text(i) => scene
                .texts
                .entries
                .get(i)
                .map(|e| e.layer_index)
                .ok_or(ObjectPaintPlanError::InvalidOrder),
            PaintIndex::Component(i) => scene
                .components
                .paints
                .get(i)
                .map(|e| e.host_layer_index)
                .ok_or(ObjectPaintPlanError::InvalidOrder),
            #[cfg(all(feature = "application", feature = "project_media_codec"))]
            PaintIndex::Brush(i) => scene
                .brushes
                .as_ref()
                .and_then(|sources| sources.paints.get(i))
                .map(|paint| paint.layer_index)
                .ok_or(ObjectPaintPlanError::InvalidOrder),
            // T12 video slots are inserted after this expansion.
            PaintIndex::Duplicator(_) | PaintIndex::Video(_) | PaintIndex::Dab(..) => {
                Err(ObjectPaintPlanError::InvalidOrder)
            }
        }
    };
    let mut buckets = BTreeMap::<usize, Vec<PaintIndex>>::new();
    for index in mixed {
        buckets.entry(layer_of(&index)?).or_default().push(index);
    }
    let mut expanded = Vec::new();
    for layer in order.layers() {
        let index = layer.source_layer_index();
        let original = buckets.remove(&index).unwrap_or_default();
        if let Some(copies) = scene.duplicators.layers.get(&index) {
            #[cfg(all(feature = "application", feature = "project_media_codec"))]
            if !copies.is_empty()
                && original
                    .iter()
                    .any(|paint| matches!(paint, PaintIndex::Brush(_)))
            {
                return Err(ObjectPaintPlanError::Unavailable);
            }
            expanded.extend(copies.iter().copied().map(PaintIndex::Duplicator));
        } else {
            expanded.extend(original);
        }
    }
    if !buckets.is_empty() {
        return Err(ObjectPaintPlanError::InvalidOrder);
    }
    let indices = indices
        .into_iter()
        .filter(|&index| {
            scene.entry(index).is_some_and(|entry| {
                !scene
                    .duplicators
                    .layers
                    .contains_key(&entry.source().source_layer_index())
            })
        })
        .collect();
    Ok((indices, expanded))
}

#[cfg(all(test, feature = "history"))]
#[path = "../tests/object_duplicator_scene.rs"]
mod tests;
