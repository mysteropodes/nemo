//! Original Path masks from one retained scene, never persistent layer.masks.
use crate::evaluation::object_image_mesh_kernels::rig_pose_eval::PosedStroke;
use crate::evaluation::object_scene_style_kernels::object_scene_producer_pin::ScenePin;
use crate::evaluation::object_scene_style_kernels::object_scene_style_paint::SceneSolid;
use crate::evaluation::object_vector_kernels::render_vector_eval::{
    evaluate_record_with_pose, is_vector_record,
};
use crate::object_frame_packet::{ObjectFramePacket, ObjectFrameRecord, ObjectRecord, SolidFill};
use crate::object_render_scene::object_scene_producer_contour::CompiledContour;
use crate::object_render_scene::render_vector_path::VectorShape;
use crate::object_render_scene::PaintIndex;
use crate::object_render_scene::{ObjectRenderScene, ObjectSceneError};
use crate::render_geometry::CubicSegment;
use std::collections::{BTreeMap, BTreeSet};
use vello::kurbo::BezPath;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ObjectMaskMode {
    Add,
    Subtract,
    Intersect,
}
impl ObjectMaskMode {
    fn of(record: &ObjectRecord) -> Self {
        match record.mask_mode() {
            Some("subtract") => Self::Subtract,
            Some("intersect") => Self::Intersect,
            _ => Self::Add,
        }
    }
}

/// Own geometry is extracted before the baseline's ordinary element-Order sort.
#[derive(Clone, Debug)]
pub(crate) struct ObjectMaskInput {
    source_object_index: usize,
    duplicator_paint_index: Option<usize>,
    ordinary_entry_index: Option<usize>,
    mode: ObjectMaskMode,
}
impl ObjectMaskInput {
    pub(crate) fn source_object_index(&self) -> usize {
        self.source_object_index
    }
    pub(crate) fn duplicator_paint_index(&self) -> Option<usize> {
        self.duplicator_paint_index
    }
    pub(crate) fn ordinary_entry_index(&self) -> Option<usize> {
        self.ordinary_entry_index
    }
    pub(crate) fn mode(&self) -> ObjectMaskMode {
        self.mode
    }
    pub(crate) fn rgba(&self) -> [f32; 4] {
        [1.; 4]
    }
}
#[derive(Clone, Debug)]
pub(crate) struct ObjectMaskLayer {
    source_layer_index: usize,
    masks: Vec<ObjectMaskInput>,
    feather: f64,
}
impl ObjectMaskLayer {
    pub(crate) fn source_layer_index(&self) -> usize {
        self.source_layer_index
    }
    pub(crate) fn masks(&self) -> &[ObjectMaskInput] {
        &self.masks
    }
    pub(crate) fn feather(&self) -> f64 {
        self.feather
    }
}

/// Indices borrow the authoritative scene paths; no second geometry allocation.
#[derive(Clone, Debug)]
pub(crate) struct ObjectMaskInputs {
    pin: ScenePin,
    layers: Vec<ObjectMaskLayer>,
}
struct MaskSource<'a> {
    layer: usize,
    source: usize,
    record: &'a ObjectRecord,
    clone_id: Option<&'a str>,
}
fn invalid(source_object_index: usize, reason: &'static str) -> ObjectSceneError {
    ObjectSceneError::UnsupportedGeometry {
        source_object_index,
        reason,
    }
}
fn ordinary_layer(packet: &ObjectFramePacket, index: usize) -> bool {
    packet.source_layers().get(index).is_some_and(|layer| {
        layer.visible()
            && !layer.is_folder_layer()
            && !layer.is_effect_layer()
            && layer.widget.value().is_none()
    })
}
fn sources(packet: &ObjectFramePacket) -> Result<Vec<MaskSource<'_>>, ObjectSceneError> {
    // Empty Object-mode output also suppresses its host's original seed masks.
    let replaced: BTreeSet<_> = packet
        .duplicator_outputs()
        .iter()
        .map(|layer| layer.layer_index)
        .collect();
    let mut sources = Vec::new();
    for source in packet.records() {
        let layer = source.source_layer_index();
        if ordinary_layer(packet, layer) && !replaced.contains(&layer) && source.record().is_mask()
        {
            sources.push(MaskSource {
                layer,
                source: source.source_object_index(),
                record: source.record(),
                clone_id: None,
            });
        }
    }
    for layer in packet.duplicator_outputs() {
        if !ordinary_layer(packet, layer.layer_index) {
            continue;
        }
        for (index, copy) in layer.output.copies.iter().enumerate() {
            if !copy.record.is_mask() {
                continue;
            }
            let source = *layer
                .source_indices
                .get(index)
                .ok_or_else(|| invalid(0, "mask copy has no retained source provenance"))?;
            sources.push(MaskSource {
                layer: layer.layer_index,
                source,
                record: &copy.record,
                clone_id: Some(&copy.clone_id),
            });
        }
    }
    Ok(sources)
}
fn positive_feather(record: &ObjectRecord, source: usize) -> Result<f64, ObjectSceneError> {
    let value = record.mask_feather().unwrap_or(0.);
    if !value.is_finite() || (value > 0. && !(value as f32).is_finite()) {
        return Err(invalid(
            source,
            "mask feather has no finite native mask representation",
        ));
    }
    Ok(if value > 0. { value } else { 0. })
}

/// The real instance count (including copies), plus maximum positive own feather.
/// Same selection as geometry; hidden, folder, widget and effect slots stay neutral.
pub(crate) fn mask_summary(
    packet: &ObjectFramePacket,
) -> Result<BTreeMap<usize, (usize, f64)>, ObjectSceneError> {
    let mut summary = BTreeMap::new();
    for source in sources(packet)? {
        let feather = positive_feather(source.record, source.source)?;
        let entry = summary.entry(source.layer).or_insert((0usize, 0.0f64));
        entry.0 += 1;
        entry.1 = entry.1.max(feather);
    }
    Ok(summary)
}
impl ObjectMaskInputs {
    pub(crate) fn from_scene(scene: &ObjectRenderScene) -> Result<Self, ObjectSceneError> {
        let packet = scene.packet();
        let mut layers = BTreeMap::<usize, ObjectMaskLayer>::new();
        let mut used = BTreeSet::new();
        for source in sources(packet)? {
            let (ordinary_index, paint_index) = if let Some(clone_id) = source.clone_id {
                let mut matches =
                    scene
                        .duplicators
                        .paints
                        .iter()
                        .enumerate()
                        .filter(|(_, paint)| {
                            paint.layer_index == source.layer
                                && paint.source_index == source.source
                                && paint.clone_id == clone_id
                        });
                let (index, _) = matches.next().ok_or_else(|| {
                    invalid(
                        source.source,
                        "mask copy has no admitted transformed scene path",
                    )
                })?;
                if matches.next().is_some() || !used.insert(index) {
                    return Err(invalid(
                        source.source,
                        "mask copy geometry provenance is ambiguous",
                    ));
                }
                (None, Some(index))
            } else {
                let mut matches = scene.entries().enumerate().filter(|(_, entry)| {
                    entry.source().source_layer_index() == source.layer
                        && entry.source().source_object_index() == source.source
                });
                let (index, _) = matches.next().ok_or_else(|| {
                    invalid(source.source, "mask has no admitted transformed scene path")
                })?;
                if matches.next().is_some() {
                    return Err(invalid(
                        source.source,
                        "mask scene geometry provenance is ambiguous",
                    ));
                }
                (Some(index), None)
            };
            let feather = positive_feather(source.record, source.source)?;
            let layer = layers
                .entry(source.layer)
                .or_insert_with(|| ObjectMaskLayer {
                    source_layer_index: source.layer,
                    masks: Vec::new(),
                    feather: 0.,
                });
            layer.feather = layer.feather.max(feather);
            layer.masks.push(ObjectMaskInput {
                source_object_index: source.source,
                duplicator_paint_index: paint_index,
                ordinary_entry_index: ordinary_index,
                mode: ObjectMaskMode::of(source.record),
            });
        }
        Ok(Self {
            pin: ScenePin::of_packet(packet),
            layers: layers.into_values().collect(),
        })
    }
    pub(crate) fn is_for_packet(&self, packet: &ObjectFramePacket) -> bool {
        self.pin == ScenePin::of_packet(packet)
    }
    /// Only an input borrowed from this set can resolve against its exact pinned scene.
    pub(crate) fn path_in_scene<'a>(
        &self,
        scene: &'a ObjectRenderScene,
        mask: &ObjectMaskInput,
    ) -> Option<&'a BezPath> {
        if !self.is_for_packet(scene.packet()) {
            return None;
        }
        let layer = self
            .layers
            .iter()
            .find(|layer| layer.masks.iter().any(|owned| std::ptr::eq(owned, mask)))?;
        match (mask.ordinary_entry_index, mask.duplicator_paint_index) {
            (Some(index), None) => {
                let entry = scene.entry(index)?;
                (entry.source().source_object_index() == mask.source_object_index
                    && entry.source().source_layer_index() == layer.source_layer_index)
                    .then(|| entry.path())
            }
            (None, Some(index)) => {
                let paint = scene.duplicators.paints.get(index)?;
                (paint.source_index == mask.source_object_index
                    && paint.layer_index == layer.source_layer_index)
                    .then_some(&paint.path)
            }
            _ => None,
        }
    }
    pub(crate) fn layers(&self) -> &[ObjectMaskLayer] {
        &self.layers
    }
    pub(crate) fn layer(&self, source_layer_index: usize) -> Option<&ObjectMaskLayer> {
        self.layers
            .iter()
            .find(|layer| layer.source_layer_index == source_layer_index)
    }
    pub(crate) fn is_duplicator_mask(&self, index: usize) -> bool {
        self.layers
            .iter()
            .flat_map(|layer| &layer.masks)
            .any(|mask| mask.duplicator_paint_index == Some(index))
    }
    /// Called only after the paint-plan producer proved complete source coverage.
    /// Keep the admitted mask paths in the scene while excluding them from colour paint.
    pub(crate) fn exclude_mask_paint(
        &self,
        scene: &ObjectRenderScene,
        indices: &mut Vec<usize>,
        mixed: &mut Vec<PaintIndex>,
    ) {
        let ordinary = |index| {
            !scene
                .entry(index)
                .expect("validated contour index")
                .source()
                .record()
                .is_mask()
        };
        indices.retain(|index| ordinary(*index));
        mixed.retain(|index| match index {
            PaintIndex::Contour(index) => ordinary(*index),
            PaintIndex::Duplicator(index) => !self.is_duplicator_mask(*index),
            PaintIndex::Raster(_)
            | PaintIndex::Text(_)
            | PaintIndex::Component(_)
            | PaintIndex::Video(_) => true,
        });
    }
}

/// Mask paint is independent of stored fill, stroke, gradient, opacity and item effects.
pub(crate) fn white_solid() -> Result<SceneSolid, &'static str> {
    SceneSolid::from_fill(&SolidFill {
        kind: "solid".into(),
        r: 1.into(),
        g: 1.into(),
        b: 1.into(),
        a: 1.into(),
    })
    .map_err(|error| error.message())
}
/// Called by the ordinary contour producer BEFORE its normal style admission.
/// Existing pose/pathFX/parametric/element+layer Motion geometry kernels still own geometry.
pub(crate) fn compile_mask(
    packet: &ObjectFramePacket,
    source: ObjectFrameRecord<'_>,
    pose: Option<&PosedStroke>,
) -> Result<CompiledContour, &'static str> {
    if !packet.owns_record(source) || !source.record().is_mask() {
        return Err("mask record is not an owned selected Path");
    }
    // Mask tags are admitted only on the existing editable-vector Path family.
    if !is_vector_record(source.record()) {
        return Err("mask geometry has no admitted native path representation");
    }
    let evaluated =
        evaluate_record_with_pose(packet, source, pose).map_err(|error| error.message())?;
    let segments = evaluated
        .segments
        .iter()
        .map(|s| CubicSegment {
            point: s.point,
            handle_in: s.handle_in,
            handle_out: s.handle_out,
        })
        .collect::<Vec<_>>();
    let shape = VectorShape::new(&segments, evaluated.closed, None)?;
    Ok(CompiledContour {
        contour: shape.contour,
        envelope: shape.envelope,
        solid: white_solid()?,
        layer_opacity: 1.,
        drawn: Some(shape.path),
    })
}

#[cfg(all(test, feature = "history"))]
#[path = "../tests/object_mask_inputs.rs"]
mod tests;
