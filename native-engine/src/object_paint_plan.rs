//! Pinned CPU commands join existing numeric order to retained real contours.
use crate::evaluation::object_scene_style_kernels::object_scene_style_item::{
    style_of_record_at_frame, SceneItemStyle, StyleUnavailable,
};
use crate::evaluation::object_scene_style_kernels::object_scene_style_paint::SceneSolid;
use crate::object_frame_packet::{ObjectFramePacket, ObjectFrameRecord, ObjectFrameSelector};
use crate::object_render_scene::mask_inputs::ObjectMaskInputs;
use crate::object_render_scene::object_scene_producer_contour;
use crate::object_render_scene::{
    validate_pins, ObjectRenderEntry, ObjectRenderScene, ObjectSceneError,
};
use crate::render_geometry::ClosedCubicPath;
use serde_json::Number;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use vello::kurbo::BezPath;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectPaintPlanError {
    Pins(ObjectSceneError),
    Unavailable,
    InvalidOrder,
}

/// Borrow pinned paint, without quantization or premultiplication.
#[derive(Debug, Clone, Copy)]
pub struct ObjectPaintCommand<'a> {
    entry: ObjectRenderEntry<'a>,
    packet: &'a ObjectFramePacket,
}
impl<'a> ObjectPaintCommand<'a> {
    pub fn source(&self) -> ObjectFrameRecord<'a> {
        self.entry.source()
    }
    pub fn contour(&self) -> &'a ClosedCubicPath {
        self.entry.contour()
    }
    /// The geometry to draw: the vector record's open or closed path, otherwise the closed contour's path (L01-I).
    pub(crate) fn path(&self) -> &'a BezPath {
        self.entry.path()
    }
    pub fn paint(&self) -> [&'a Number; 4] {
        // A sampled layer factor changes the retained primary paint, including SVG's legacy fill.
        if self.entry.layer_opacity() != 1.0 {
            return self.entry.solid().channels();
        }
        // The fill of the source record when it has one (the legacy family always does), otherwise the retained primary solid
        // of the entry (L01-I): an unfilled vector retains its stroke paint, an item without any paint a transparent solid.
        match self.entry.source().record().solid_fill() {
            Some(paint) => [&paint.r, &paint.g, &paint.b, &paint.a],
            None => self.entry.solid().channels(),
        }
    }
    /// The raw layer opacity factor folded into `paint()` and `solid()` (one for an untouched layer).
    pub(crate) fn layer_opacity(&self) -> f64 {
        self.entry.layer_opacity()
    }
    /// Typed straight-RGBA paint (L07-A `SceneSolid`) retained with the contour;
    /// Raw layer opacity is already folded in. At factor one `paint()` retains
    /// its original source-fill borrowing contract.
    pub(crate) fn solid(&self) -> &'a SceneSolid {
        self.entry.solid()
    }
    /// Typed item style (L07-I): the retained L07-A solid for the legacy family, otherwise the one
    /// record-to-style function (stroke, paint order, opacity). The scene admits legacy contours only
    /// today; the second arm is the join point for the vector family.
    pub(crate) fn style(&self) -> Result<SceneItemStyle, StyleUnavailable> {
        let record = self.entry.source().record();
        if record.vector().is_none() {
            return Ok(SceneItemStyle::solid_fill(self.solid().clone()));
        }
        style_of_record_at_frame(record, self.entry.layer_opacity(), self.packet)
    }
}

/// Retains the exact scene and immutable command indices, not a second owner.
#[derive(Debug, Clone)]
pub struct ObjectPaintPlan {
    scene: Arc<ObjectRenderScene>,
    masks: Arc<ObjectMaskInputs>,
    indices: Arc<[usize]>,
    mixed: Arc<[PaintIndex]>,
    dabs: Arc<[dab_paint::DabPaint]>,
}
impl ObjectPaintPlan {
    pub fn packet(&self) -> &ObjectFramePacket {
        self.scene.packet()
    }
    pub fn commands(&self) -> impl ExactSizeIterator<Item = ObjectPaintCommand<'_>> + '_ {
        // Allocation-free indexed borrows; no record/paint copies or second sort.
        self.indices.iter().map(|&index| ObjectPaintCommand {
            packet: self.packet(),
            entry: self
                .scene
                .entry(index)
                .expect("paint-plan index was validated"),
        })
    }
}

#[path = "object_raster_paint_join.rs"]
pub(crate) mod raster;
use raster::PaintIndex;
// G16 option C (#2361): the dabs seeded brush-texture anchors paint, right after each anchor.
#[path = "object_brush_dab_paint.rs"]
pub(crate) mod dab_paint;

type LayerSequence = Vec<(usize, Vec<usize>)>;

pub fn prepare_object_paint_plan(
    scene: &ObjectRenderScene,
    expected: &ObjectFrameSelector<'_>,
) -> Result<ObjectPaintPlan, ObjectPaintPlanError> {
    validate_pins(scene.packet(), expected).map_err(ObjectPaintPlanError::Pins)?;
    // The admitted F1 producer offers numeric_order ONLY for scene-root
    // Authored v2. v1/no-plan and all Reference/nonroot inputs are unavailable.
    // L07-F: the retained scene is reported as the contour producer output and joined
    // through the pinned registry (pin check once, NumericOrderPlan order).
    let (indices, mixed) = if scene.rasters.is_empty()
        && scene.texts.entries.is_empty()
        && scene.components.scene.is_none()
        && scene.brush_paints_empty()
    {
        let sequence = object_scene_producer_contour::command_sequence(scene, expected)?;
        let indices = join(scene, &sequence)?;
        let mixed = indices.iter().copied().map(PaintIndex::Contour).collect();
        (indices, mixed)
    } else {
        raster::prepare(scene, expected)?
    };
    let (mut indices, mut mixed) =
        crate::object_render_scene::duplicator_scene::expand_order(scene, indices, mixed)?;
    // Original producer coverage/order is proved ABOVE before mask extraction.
    let masks = Arc::new(ObjectMaskInputs::from_scene(scene).map_err(ObjectPaintPlanError::Pins)?);
    masks.exclude_mask_paint(scene, &mut indices, &mut mixed);
    let mixed = crate::object_render_scene::media_scene::insert_slots(scene, mixed)?;
    let (mixed, dabs) = dab_paint::expand(scene, mixed)?;
    Ok(ObjectPaintPlan {
        scene: Arc::new(scene.clone()),
        masks,
        indices: indices.into(),
        mixed: mixed.into(),
        dabs: dabs.into(),
    })
}

/// All layers and all selected original targets must occur exactly once.
fn join(
    scene: &ObjectRenderScene,
    sequence: &LayerSequence,
) -> Result<Vec<usize>, ObjectPaintPlanError> {
    let packet = scene.packet();
    let mut sources = HashMap::new();
    let mut targets = HashSet::new();
    for (index, entry) in scene.entries().enumerate() {
        let source = entry.source();
        let target = source.record().target();
        if target.context_id() != packet.context_id()
            || target.scope_kind() != packet.scope_kind()
            || Some(target.frame()) != packet.source_frame_for_layer(source.source_layer_index())
            || packet
                .source_layers()
                .get(source.source_layer_index())
                .is_none_or(|layer| layer.layer_uid() != target.layer_uid())
            || !targets.insert(target)
            || sources
                .insert(
                    source.source_object_index(),
                    (source.source_layer_index(), index),
                )
                .is_some()
        {
            return Err(ObjectPaintPlanError::InvalidOrder);
        }
    }
    if sources.len() != packet.records().len() {
        return Err(ObjectPaintPlanError::InvalidOrder);
    }
    let mut layers = HashSet::new();
    let mut seen = vec![false; sources.len()];
    let mut indices = Vec::with_capacity(sources.len());
    for (layer, elements) in sequence {
        if *layer >= packet.source_layers().len() || !layers.insert(*layer) {
            return Err(ObjectPaintPlanError::InvalidOrder);
        }
        for element in elements {
            let &(source_layer, index) = sources
                .get(element)
                .ok_or(ObjectPaintPlanError::InvalidOrder)?;
            if source_layer != *layer || seen[index] {
                return Err(ObjectPaintPlanError::InvalidOrder);
            }
            seen[index] = true;
            indices.push(index);
        }
    }
    if layers.len() != packet.source_layers().len() || seen.iter().any(|seen| !seen) {
        return Err(ObjectPaintPlanError::InvalidOrder);
    }
    Ok(indices)
}

#[cfg(test)]
#[path = "../tests/object_paint_plan_tests.rs"]
pub(crate) mod tests;
