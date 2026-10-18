//! Mixed contour/raster order through the existing pinned producer registry; no document owner.
use crate::evaluation::object_scene_style_kernels::object_scene_producer_pin::{
    join_scene, ProducerOutput, SceneOutcome, ScenePin,
};
use crate::object_frame_packet::ObjectFrameSelector;
use crate::object_render_scene::raster_scene::RasterSource;
use crate::object_render_scene::{
    ObjectPaintCommand, ObjectPaintPlan, ObjectPaintPlanError, ObjectRenderScene,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PaintIndex {
    Contour(usize),
    Raster(usize),
}
pub(crate) enum PaintEntry<'a> {
    Contour(ObjectPaintCommand<'a>),
    Raster(usize),
}
impl ObjectPaintPlan {
    pub(crate) fn raster_sources(&self) -> &[RasterSource] {
        &self.scene.rasters
    }
}
pub(crate) fn ordered_entries(plan: &ObjectPaintPlan) -> impl Iterator<Item = PaintEntry<'_>> + '_ {
    plan.mixed.iter().map(|index| match index {
        PaintIndex::Contour(index) => PaintEntry::Contour(ObjectPaintCommand {
            entry: plan.scene.entry(*index).expect("pinned contour index"),
        }),
        PaintIndex::Raster(index) => PaintEntry::Raster(*index),
    })
}
pub(super) fn prepare(
    scene: &ObjectRenderScene,
    expected: &ObjectFrameSelector<'_>,
) -> Result<(Vec<usize>, Vec<PaintIndex>), ObjectPaintPlanError> {
    let pin = ScenePin::of_packet(scene.packet());
    let expected_pin = ScenePin {
        instance_id: expected.instance_id.to_owned(),
        document_id: expected.document_id.to_owned(),
        content_revision: expected.content_revision,
        document_snapshot_id: expected.document_snapshot_id.to_owned(),
        context_id: expected.context_id.to_owned(),
        scope_kind: expected.scope_kind.clone(),
        frame: expected.frame,
    };
    let mut contours = ProducerOutput::new("contour", pin.clone());
    for (index, entry) in scene.entries().enumerate() {
        contours = contours.with_item(
            entry.source().source_object_index(),
            PaintIndex::Contour(index),
        );
    }
    let selected: std::collections::HashMap<_, _> =
        scene.packet().selected_source_indices().collect();
    let mut rasters = ProducerOutput::new("raster-image", pin);
    for (index, source) in scene.rasters.iter().enumerate() {
        if selected.get(&source.source_index) != Some(&source.layer_index) {
            return Err(ObjectPaintPlanError::InvalidOrder);
        }
        rasters = rasters.with_item(source.source_index, PaintIndex::Raster(index));
    }
    let joined = join_scene(scene.packet(), &expected_pin, vec![contours, rasters])
        .map_err(|_| ObjectPaintPlanError::InvalidOrder)?;
    if !joined.is_complete() {
        return Err(ObjectPaintPlanError::Unavailable);
    }
    let mut indices = Vec::new();
    let mut mixed = Vec::new();
    for entry in joined.entries() {
        match entry.outcome() {
            SceneOutcome::Item { payload, .. } => {
                mixed.push(*payload);
                if let PaintIndex::Contour(index) = payload {
                    indices.push(*index);
                }
            }
            SceneOutcome::Unavailable(_) => return Err(ObjectPaintPlanError::Unavailable),
        }
    }
    Ok((indices, mixed))
}
