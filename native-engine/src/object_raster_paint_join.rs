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
    Text(usize),
    Component(usize),
    Video(usize),
}
pub(crate) enum PaintEntry<'a> {
    Contour(ObjectPaintCommand<'a>),
    Raster(usize),
    Text(&'a crate::object_render_scene::text_scene::TextSource),
    Component(&'a crate::object_render_scene::component_scene::ComponentPaint),
    Video(usize),
}
impl ObjectPaintPlan {
    pub(crate) fn has_components(&self) -> bool {
        !self.scene.components.paints.is_empty()
    }
    pub(crate) fn has_text(&self) -> bool {
        !self.scene.texts.entries.is_empty()
    }
    pub(crate) fn text_resources(
        &self,
    ) -> &std::sync::Arc<crate::object_render_scene::ObjectSceneResourceSet> {
        &self.scene.texts.resources
    }
    /// Every retained native paint resource, including fonts, for the scheduler key.
    pub fn scene_resource_versions(
        &self,
    ) -> Result<Vec<(String, String)>, crate::object_render_scene::ObjectSceneError> {
        let mut declared = self.raster_resource_versions()?;
        declared.extend(self.scene.texts.resources.declared_versions());
        Ok(declared)
    }

    /// Same immutable chain provider retained by Text/raster paints, not caller data.
    pub(crate) fn native_parent_chains(
        &self,
    ) -> &std::sync::Arc<crate::object_render_scene::text_scene::parent_chain::SceneParentChains>
    {
        &self.scene.texts.parent_chains
    }

    pub(crate) fn raster_sources(&self) -> &[RasterSource] {
        &self.scene.rasters
    }
    pub(crate) fn video_slots(&self) -> &[crate::object_render_scene::media_scene::VideoSlot] {
        &self.scene.videos
    }
}
pub(crate) fn ordered_entries(plan: &ObjectPaintPlan) -> impl Iterator<Item = PaintEntry<'_>> + '_ {
    plan.mixed.iter().map(|index| match index {
        PaintIndex::Contour(index) => PaintEntry::Contour(ObjectPaintCommand {
            entry: plan.scene.entry(*index).expect("pinned contour index"),
            packet: plan.packet(),
        }),
        PaintIndex::Raster(index) => PaintEntry::Raster(*index),
        PaintIndex::Text(index) => PaintEntry::Text(&plan.scene.texts.entries[*index]),
        PaintIndex::Component(index) => {
            PaintEntry::Component(&plan.scene.components.paints[*index])
        }
        PaintIndex::Video(index) => PaintEntry::Video(*index),
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
    let mut rasters = ProducerOutput::new("raster-image", pin.clone());
    for (index, source) in scene.rasters.iter().enumerate() {
        if selected.get(&source.source_index) != Some(&source.layer_index) {
            return Err(ObjectPaintPlanError::InvalidOrder);
        }
        rasters = rasters.with_item(source.source_index, PaintIndex::Raster(index));
    }
    let mut texts = ProducerOutput::new("text-block", pin);
    for (index, source) in scene.texts.entries.iter().enumerate() {
        if selected.get(&source.source_index) != Some(&source.layer_index) {
            return Err(ObjectPaintPlanError::InvalidOrder);
        }
        texts = texts.with_item(source.source_index, PaintIndex::Text(index));
    }
    let joined = join_scene(
        scene.packet(),
        &expected_pin,
        vec![contours, rasters, texts],
    )
    .map_err(|_| ObjectPaintPlanError::InvalidOrder)?;
    if !joined.is_complete() {
        return Err(ObjectPaintPlanError::Unavailable);
    }
    let mut indices = Vec::new();
    let mut mixed = Vec::new();
    if let Some(components) = &scene.components.scene {
        use crate::evaluation::object_scene_style_kernels::object_scene_producer_component_scene::SceneSlot;
        // Preserve the SAME plan's layer-slot ordering; an instance has no
        // root object record and must never borrow a contour source index.
        let slots = components
            .merge(scene.packet(), &joined)
            .map_err(|_| ObjectPaintPlanError::InvalidOrder)?;
        for slot in slots {
            match slot {
                SceneSlot::Object(entry) => {
                    append_object_entry(entry, &mut indices, &mut mixed)?;
                }
                SceneSlot::Component(slot) => {
                    for (index, paint) in scene.components.paints.iter().enumerate() {
                        if paint.host_layer_index == slot.entry().source_layer_index {
                            mixed.push(PaintIndex::Component(index));
                        }
                    }
                }
            }
        }
    } else {
        for entry in joined.entries() {
            append_object_entry(entry, &mut indices, &mut mixed)?;
        }
    }
    Ok((indices, mixed))
}

fn append_object_entry(
    entry: &crate::evaluation::object_scene_style_kernels::object_scene_producer_pin::SceneEntry<
        PaintIndex,
    >,
    indices: &mut Vec<usize>,
    mixed: &mut Vec<PaintIndex>,
) -> Result<(), ObjectPaintPlanError> {
    match entry.outcome() {
        SceneOutcome::Item { payload, .. } => {
            mixed.push(*payload);
            if let PaintIndex::Contour(index) = payload {
                indices.push(*index);
            }
            Ok(())
        }
        SceneOutcome::Unavailable(_) => Err(ObjectPaintPlanError::Unavailable),
    }
}
