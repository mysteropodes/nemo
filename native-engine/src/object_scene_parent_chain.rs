//! Immutable native document -> sampled D30 chains and evaluated native pivot geometry.
//! No Paper bounds, document writer, cached revision or test-only chain injection.
use crate::evaluation::motion_parent_chain::{
    motion_mat_from_values, ChainEntry, MotionMat, CHAIN_NODE_LIMIT,
};
use crate::evaluation::object_vector_kernels::render_vector_eval::{
    evaluate_vector_rest, is_vector_record, record_layer_has_active_path_fx,
};
use crate::object_frame_packet::text::{ObjectLayer, Presence, PropertyHolderTarget};
use crate::object_frame_packet::{sample_property_raw, ObjectFramePacket};
use crate::object_render_scene::object_scene_producer_text::TextSceneItem;
use crate::render_geometry::{ClosedCubicPath, CubicSegment};
use std::collections::{BTreeMap, BTreeSet};
use vello::kurbo::{Rect, Shape};

#[path = "object_scene_parent_curve.rs"]
mod curve;
use curve::curve_path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChainUnavailable {
    Context,
    MissingLayer,
    MissingParent,
    Cycle,
    Depth,
    Sample,
    Expression,
    LayerContext,
    VectorGeometry,
    RasterGeometry,
    MissingGeometry,
}
impl ChainUnavailable {
    pub(crate) fn reason(self) -> &'static str {
        match self {
            Self::Context => "native parent chain context unavailable",
            Self::MissingLayer => "native parent chain layer unavailable",
            Self::MissingParent => "native parent chain target unavailable",
            Self::Cycle => "native parent chain is cyclic",
            Self::Depth => "native parent chain exceeds the D30 bound",
            Self::Sample => "native parent Motion sample unavailable",
            Self::Expression => "native parent Motion expression unavailable",
            Self::LayerContext => "native parent group, rig or effect context unavailable",
            Self::VectorGeometry => "native parent evaluated vector geometry unavailable",
            Self::RasterGeometry => "native parent raster geometry unavailable",
            Self::MissingGeometry => "native parent real geometry pivot unavailable",
        }
    }
}
#[derive(Debug, Clone, Copy)]
pub(crate) struct ChainFailure {
    pub(crate) source_index: usize,
    kind: ChainUnavailable,
}
impl ChainFailure {
    pub(crate) fn reason(self) -> &'static str {
        self.kind.reason()
    }
}
#[derive(Debug, Clone, Copy)]
struct PoseSample {
    position: [f64; 2],
    anchor: [f64; 2],
    rotation: f64,
    scale: [f64; 2],
    opacity: f64,
}
impl PoseSample {
    fn mat(self, own: bool) -> Option<MotionMat> {
        // Current Text/raster source paint has ALREADY consumed its own layer opacity.
        motion_mat_from_values(
            self.position,
            self.anchor,
            self.rotation,
            self.scale,
            if own { 100.0 } else { self.opacity },
        )
    }
}
fn safe_layer(layer: &ObjectLayer) -> bool {
    layer.widget.value().is_none()
        && layer.groups.value().is_none()
        && layer.rig.value().is_none()
        && !layer.folder_id.value().is_some_and(|v| v.is_some())
        && !layer.blend_mode.value().is_some_and(|v| v != "normal")
        && !layer.matte_mode.value().is_some_and(|v| v != "none")
        && layer.matte_source_layer_uid.value().is_none()
        && !layer.mattes_more.value().is_some_and(|v| !v.is_empty())
        && layer.is_effect_layer.value() != Some(&true)
        && !layer.effects.value().is_some_and(|v| !v.is_empty())
        && layer.effects_from.value().is_none()
}
fn sample(packet: &ObjectFramePacket, layer: usize) -> Result<PoseSample, ChainUnavailable> {
    let source = packet
        .source_layers()
        .get(layer)
        .ok_or(ChainUnavailable::MissingLayer)?;
    if !safe_layer(source) {
        return Err(ChainUnavailable::LayerContext);
    }
    let target = PropertyHolderTarget {
        context_id: packet.context_id().to_owned(),
        layer_uid: source.layer_uid().to_owned(),
        stroke_id: Presence::Missing,
    };
    let keys = ["position", "anchor", "rotation", "scale", "opacity"];
    if packet
        .snapshot()
        .document()
        .property_holders()
        .unwrap_or(&[])
        .iter()
        .filter(|h| h.target() == &target)
        .flat_map(|h| h.properties())
        .any(|r| keys.contains(&r.property()) && r.expression.value().is_some_and(|e| e.enabled))
    {
        return Err(ChainUnavailable::Expression);
    }
    let read = |key| {
        sample_property_raw(packet.snapshot(), &target, key, packet.frame() as f64)
            .map_err(|_| ChainUnavailable::Sample)
    };
    let position = read("position")?;
    let anchor = read("anchor")?;
    let rotation = read("rotation")?;
    let scale = read("scale")?;
    let opacity = read("opacity")?;
    match (
        position.as_slice(),
        anchor.as_slice(),
        rotation.as_slice(),
        scale.as_slice(),
        opacity.as_slice(),
    ) {
        ([x, y], [ax, ay], [rotation], [sx, sy], [opacity])
            if [x, y, ax, ay, rotation, sx, sy, opacity]
                .iter()
                .all(|v| v.is_finite()) =>
        {
            Ok(PoseSample {
                position: [*x, *y],
                anchor: [*ax, *ay],
                rotation: *rotation,
                scale: [*sx, *sy],
                opacity: *opacity,
            })
        }
        _ => Err(ChainUnavailable::Sample),
    }
}
/// Same retained snapshot/context/evaluation frame through construction and both consumers.
/// `bounds` is only construction-local native derived geometry, never authored state.
#[derive(Debug, Clone)]
pub(crate) struct SceneParentChains {
    packet: ObjectFramePacket,
    walks: BTreeMap<usize, Vec<usize>>,
    consumers: BTreeMap<usize, usize>,
    samples: BTreeMap<usize, PoseSample>,
    needed: BTreeSet<usize>,
    bounds: BTreeMap<usize, Rect>,
}
impl SceneParentChains {
    pub(crate) fn new(packet: &ObjectFramePacket) -> Result<Self, ChainFailure> {
        let mut result = Self {
            packet: packet.clone(),
            walks: BTreeMap::new(),
            consumers: BTreeMap::new(),
            samples: BTreeMap::new(),
            needed: BTreeSet::new(),
            bounds: BTreeMap::new(),
        };
        for (layer, source) in packet
            .text_records()
            .map(|r| (r.layer_index(), r.source_index()))
            .chain(
                packet
                    .raster_records()
                    .map(|r| (r.layer_index(), r.source_index())),
            )
        {
            result
                .consumers
                .entry(layer)
                .and_modify(|old| *old = (*old).min(source))
                .or_insert(source);
        }
        let consumers = result.consumers.clone();
        for (start, source_index) in consumers {
            let prepared = (|| -> Result<(), ChainUnavailable> {
                let mut walk = Vec::new();
                let mut seen = BTreeSet::new();
                let mut next = Some(start);
                while let Some(index) = next {
                    if !seen.insert(index) {
                        return Err(ChainUnavailable::Cycle);
                    }
                    // D30 admits at most64 ancestors, plus the consumer's own layer.
                    if walk.len() > CHAIN_NODE_LIMIT {
                        return Err(ChainUnavailable::Depth);
                    }
                    let layer = packet
                        .source_layers()
                        .get(index)
                        .ok_or(ChainUnavailable::MissingLayer)?;
                    let in_context = packet
                        .snapshot()
                        .document()
                        .composition_order
                        .value()
                        .and_then(|order| {
                            order
                                .contexts
                                .iter()
                                .find(|c| c.context_id == packet.context_id())
                        })
                        .is_some_and(|context| {
                            context
                                .layers
                                .iter()
                                .any(|l| l.layer_uid == layer.layer_uid())
                        });
                    let pose = sample(packet, index)?;
                    let parent_uid = layer.parent_layer_uid.value().and_then(Option::as_deref);
                    // Existing v1 item rendering predates compositionOrder. Its frame packet
                    // already pins the selected context/layer; identity unparented geometry
                    // needs no invented parent-order context. Every real parent/pose still
                    // requires the explicit context proof above.
                    let identity_v1 = index == start
                        && walk.is_empty()
                        && packet.snapshot().document().format_version() == 1
                        && packet
                            .snapshot()
                            .document()
                            .composition_order
                            .value()
                            .is_none()
                        && parent_uid.is_none()
                        && pose.mat(true).is_none();
                    if !in_context && !identity_v1 {
                        return Err(ChainUnavailable::Context);
                    }
                    if pose.mat(index == start).is_some() {
                        result.needed.insert(index);
                    }
                    result.samples.insert(index, pose);
                    walk.push(index);
                    next = match parent_uid {
                        None => None,
                        Some(uid) => Some(
                            packet
                                .source_layers()
                                .iter()
                                .position(|p| p.layer_uid() == uid)
                                .ok_or(ChainUnavailable::MissingParent)?,
                        ),
                    };
                }
                result.walks.insert(start, walk);
                Ok(())
            })();
            prepared.map_err(|kind| ChainFailure { source_index, kind })?;
        }
        result
            .include_vectors()
            .map_err(|(layer, kind)| result.failure(layer, kind))?;
        result
            .include_rasters()
            .map_err(|(layer, kind)| result.failure(layer, kind))?;
        Ok(result)
    }
    fn failure(&self, layer: usize, kind: ChainUnavailable) -> ChainFailure {
        let source_index = self
            .walks
            .iter()
            .filter(|(_, walk)| walk.contains(&layer))
            .filter_map(|(consumer, _)| self.consumers.get(consumer))
            .copied()
            .min()
            .unwrap_or(0);
        ChainFailure { source_index, kind }
    }
    pub(crate) fn needs_bounds(&self, layer: usize) -> bool {
        self.needed.contains(&layer)
    }
    fn matches(
        &self,
        layer: usize,
        target: &crate::object_frame_packet::text::ObjectTarget,
    ) -> bool {
        target.context_id() == self.packet.context_id()
            && target.scope_kind() == self.packet.scope_kind()
            && self.packet.source_frame_for_layer(layer) == Some(target.frame())
    }
    fn add(&mut self, layer: usize, rect: Rect) -> Result<(), ChainUnavailable> {
        if ![rect.x0, rect.y0, rect.x1, rect.y1]
            .iter()
            .all(|v| v.is_finite())
            || rect.x0 > rect.x1
            || rect.y0 > rect.y1
        {
            return Err(ChainUnavailable::MissingGeometry);
        }
        self.bounds
            .entry(layer)
            .and_modify(|old| *old = old.union(rect))
            .or_insert(rect);
        Ok(())
    }
    fn include_vectors(&mut self) -> Result<(), (usize, ChainUnavailable)> {
        // The real rest evaluator owns pathFX/paramShape ordering, before layer pose.
        // Legacy closed-cubic records use the existing strict contour admission.
        let records: Vec<_> = self
            .packet
            .snapshot()
            .document()
            .objects()
            .iter()
            .enumerate()
            .filter_map(|(source, record)| {
                self.packet
                    .source_layers()
                    .iter()
                    .position(|l| l.layer_uid() == record.target().layer_uid())
                    .filter(|layer| {
                        self.needs_bounds(*layer) && self.matches(*layer, record.target())
                    })
                    .map(|layer| (source, layer, record.clone()))
            })
            .collect();
        for (source, layer, record) in records {
            let admitted = (|| -> Result<(), ChainUnavailable> {
                let path = if is_vector_record(&record) {
                    let evaluated =
                        evaluate_vector_rest(self.packet.snapshot(), source, self.packet.frame())
                            .map_err(|_| ChainUnavailable::VectorGeometry)?;
                    curve_path(
                        &evaluated
                            .segments
                            .iter()
                            .map(|s| (s.point, s.handle_in, s.handle_out))
                            .collect::<Vec<_>>(),
                        evaluated.closed,
                    )?
                } else {
                    if record_layer_has_active_path_fx(&self.packet, &record) {
                        return Err(ChainUnavailable::VectorGeometry);
                    }
                    let pair=|p:&crate::object_frame_packet::text::ObjectPoint| -> Result<[f64;2],ChainUnavailable> {
                    Ok([p.x.as_f64().filter(|v|v.is_finite()).ok_or(ChainUnavailable::VectorGeometry)?,
                        p.y.as_f64().filter(|v|v.is_finite()).ok_or(ChainUnavailable::VectorGeometry)?])
                };
                    let segments = record
                        .geometry
                        .segments
                        .iter()
                        .map(|s| {
                            Ok(CubicSegment {
                                point: pair(&s.point)?,
                                handle_in: pair(&s.handle_in)?,
                                handle_out: pair(&s.handle_out)?,
                            })
                        })
                        .collect::<Result<Vec<_>, ChainUnavailable>>()?;
                    ClosedCubicPath::new(segments)
                        .map_err(|_| ChainUnavailable::VectorGeometry)?
                        .path()
                        .clone()
                };
                self.add(layer, path.bounding_box())?;
                Ok(())
            })();
            admitted.map_err(|kind| (layer, kind))?;
        }
        Ok(())
    }
    fn include_rasters(&mut self) -> Result<(), (usize, ChainUnavailable)> {
        let records: Vec<_> = self
            .packet
            .snapshot()
            .document()
            .item_records()
            .into_iter()
            .flat_map(|items| items.raster_entries().iter())
            .filter_map(|entry| {
                self.packet
                    .source_layers()
                    .iter()
                    .position(|l| l.layer_uid() == entry.target.layer_uid())
                    .filter(|layer| {
                        self.needs_bounds(*layer) && self.matches(*layer, &entry.target)
                    })
                    .map(|layer| (layer, entry.item.clone()))
            })
            .collect();
        for (layer, item) in records {
            let admitted = (|| -> Result<(), ChainUnavailable> {
                if item.mesh_id().is_some()
                    || item.is_bitmap_brush() == Some(true)
                    || item.brush_group_id().is_some()
                    || item.is_brush_texture_copy() == Some(true)
                {
                    return Err(ChainUnavailable::RasterGeometry);
                }
                let r = item.display_rect();
                let cx = r.x + r.width / 2.0;
                let cy = r.y + r.height / 2.0;
                let (sin, cos) = r.rotation.to_radians().sin_cos();
                let mut points = Vec::new();
                for (x, y) in [
                    (r.x, r.y),
                    (r.x + r.width, r.y),
                    (r.x + r.width, r.y + r.height),
                    (r.x, r.y + r.height),
                ] {
                    points.push((
                        cx + (x - cx) * cos - (y - cy) * sin,
                        cy + (x - cx) * sin + (y - cy) * cos,
                    ));
                }
                let mut rect = Rect::new(points[0].0, points[0].1, points[0].0, points[0].1);
                for (x, y) in points {
                    rect = rect.union(Rect::new(x, y, x, y));
                }
                self.add(layer, rect)?;
                Ok(())
            })();
            admitted.map_err(|kind| (layer, kind))?;
        }
        Ok(())
    }
    /// The caller supplies actual retained-font native contours, including hidden
    /// parent Text at its held source. Keep its font resource in TextSources too.
    pub(crate) fn include_text(
        &mut self,
        layer: usize,
        item: &TextSceneItem,
    ) -> Result<(), ChainUnavailable> {
        if self.needs_bounds(layer) {
            for (path, _) in item.painted() {
                self.add(layer, path.contour.path().bounding_box())?;
            }
        }
        Ok(())
    }
    pub(crate) fn chain(&self, layer: usize) -> Result<Vec<ChainEntry>, ChainUnavailable> {
        let mut chain = Vec::new();
        for &index in self
            .walks
            .get(&layer)
            .ok_or(ChainUnavailable::MissingLayer)?
        {
            if let Some(mat) = self.samples[&index].mat(index == layer) {
                let rect = self
                    .bounds
                    .get(&index)
                    .ok_or(ChainUnavailable::MissingGeometry)?;
                let pivot = [rect.center().x + mat.ax, rect.center().y + mat.ay];
                if !pivot.iter().all(|v| v.is_finite()) {
                    return Err(ChainUnavailable::MissingGeometry);
                }
                chain.push(ChainEntry { mat, pivot });
            }
        }
        Ok(chain)
    }
    pub(crate) fn finish(self) -> Result<Self, ChainFailure> {
        for &layer in self.walks.keys() {
            self.chain(layer).map_err(|kind| ChainFailure {
                source_index: self.consumers[&layer],
                kind,
            })?;
        }
        Ok(self)
    }
}
