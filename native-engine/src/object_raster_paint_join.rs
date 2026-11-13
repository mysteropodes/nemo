//! Mixed contour/raster order through the existing pinned producer registry; no document owner.
use crate::evaluation::object_scene_style_kernels::object_scene_producer_pin::{
    join_scene, ProducerOutput, SceneOutcome, ScenePin,
};
use crate::object_frame_packet::ObjectFrameSelector;
use crate::object_render_scene::mask_inputs::{ObjectMaskInput, ObjectMaskInputs};
use crate::object_render_scene::raster_scene::RasterSource;
use crate::object_render_scene::{
    ObjectPaintCommand, ObjectPaintPlan, ObjectPaintPlanError, ObjectRenderScene,
};
use vello::kurbo::BezPath;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PaintIndex {
    Contour(usize),
    Raster(usize),
    Text(usize),
    Component(usize),
    Video(usize),
    Duplicator(usize),
    #[cfg(all(feature = "application", feature = "project_media_codec"))]
    Brush(usize),
    /// A produced brush-texture dab: its paint and the layer of its anchor.
    Dab(usize, usize),
}
#[derive(Clone, Copy)]
pub(crate) enum PaintEntry<'a> {
    Contour(ObjectPaintCommand<'a>),
    Raster(usize),
    Text(&'a crate::object_render_scene::text_scene::TextSource),
    Component(&'a crate::object_render_scene::component_scene::ComponentPaint),
    Video(usize),
    Duplicator(&'a crate::object_render_scene::duplicator_scene::DuplicatorPaint),
    Dab(&'a crate::object_render_scene::paint::dab_paint::DabPaint),
    #[cfg(all(feature = "application", feature = "project_media_codec"))]
    Brush(usize),
}
impl ObjectPaintPlan {
    pub(crate) fn mask_inputs(&self) -> &ObjectMaskInputs {
        &self.masks
    }
    pub(crate) fn mask_path(&self, mask: &ObjectMaskInput) -> Option<&BezPath> {
        self.masks.path_in_scene(&self.scene, mask)
    }
    pub(crate) fn has_duplicators(&self) -> bool {
        !self.scene.packet().duplicator_outputs().is_empty()
    }
    #[cfg(all(feature = "application", feature = "project_media_codec"))]
    pub(crate) fn brush_sources(
        &self,
    ) -> Option<&crate::object_render_scene::brush_scene::BrushSources> {
        self.scene.brushes.as_deref()
    }
    pub(crate) fn has_components(&self) -> bool {
        !self.scene.components.paints.is_empty()
    }
    /// Whether the plan paints the copies of this source layer instead of its original records.
    pub(crate) fn is_duplicator_host(&self, source_layer_index: usize) -> bool {
        self.scene.duplicators.is_host(source_layer_index)
    }
    /// Every duplicator copy paint, in scene order (the entries' `PaintIndex::Duplicator` index).
    pub(crate) fn duplicator_paints(
        &self,
    ) -> &[crate::object_render_scene::duplicator_scene::DuplicatorPaint] {
        &self.scene.duplicators.paints
    }
    /// Every placed-component inner paint, in scene order (the entries' `PaintIndex::Component` index).
    pub(crate) fn component_paints(
        &self,
    ) -> &[crate::object_render_scene::component_scene::ComponentPaint] {
        &self.scene.components.paints
    }
    /// The item-effect key of one placed-component paint of THIS plan: the packet's object count plus the paint's index,
    /// so it never collides with a source object index. A placed component's inner record lives in the symbol's own
    /// context, which the join's packet-wide object table does not hold. `None` for a paint of another plan.
    pub(crate) fn component_item_key(
        &self,
        paint: &crate::object_render_scene::component_scene::ComponentPaint,
    ) -> Option<usize> {
        let paints = self.component_paints();
        let range = paints.as_ptr_range();
        let at = std::ptr::from_ref(paint);
        if !range.contains(&at) {
            return None;
        }
        let offset = (at as usize - range.start as usize) / std::mem::size_of_val(paint);
        Some(self.packet().source_objects().len() + offset)
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
        #[cfg(all(feature = "application", feature = "project_media_codec"))]
        if let Some(brushes) = &self.scene.brushes {
            declared.extend(brushes.resources().declared_versions());
        }
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
fn entry_of<'a>(plan: &'a ObjectPaintPlan, index: &PaintIndex) -> PaintEntry<'a> {
    match index {
        PaintIndex::Contour(index) => PaintEntry::Contour(ObjectPaintCommand {
            entry: plan.scene.entry(*index).expect("pinned contour index"),
            packet: plan.packet(),
        }),
        PaintIndex::Raster(index) => PaintEntry::Raster(*index),
        #[cfg(all(feature = "application", feature = "project_media_codec"))]
        PaintIndex::Brush(index) => PaintEntry::Brush(*index),
        PaintIndex::Text(index) => PaintEntry::Text(&plan.scene.texts.entries[*index]),
        PaintIndex::Component(index) => {
            PaintEntry::Component(&plan.scene.components.paints[*index])
        }
        PaintIndex::Video(index) => PaintEntry::Video(*index),
        PaintIndex::Duplicator(index) => {
            PaintEntry::Duplicator(&plan.scene.duplicators.paints[*index])
        }
        PaintIndex::Dab(index, _) => PaintEntry::Dab(&plan.dabs[*index]),
    }
}
pub(crate) fn ordered_entries(plan: &ObjectPaintPlan) -> impl Iterator<Item = PaintEntry<'_>> + '_ {
    plan.mixed.iter().map(|index| entry_of(plan, index))
}
/// The entries a hit test reads: the paint order with every member of a combined group still present (a combined shape
/// stays independently selectable) and without the combined islands.
pub(crate) fn listed_entries(plan: &ObjectPaintPlan) -> impl Iterator<Item = PaintEntry<'_>> + '_ {
    plan.listed
        .as_deref()
        .unwrap_or(&plan.mixed)
        .iter()
        .map(|index| entry_of(plan, index))
}
/// Every entry with the source layer it belongs to, in authored order (the layered path paints one layer at a time).
pub(crate) fn layered_entries(
    plan: &ObjectPaintPlan,
) -> impl Iterator<Item = (usize, PaintEntry<'_>)> + '_ {
    plan.mixed
        .iter()
        .map(|index| (plan.layer_of_entry(index), entry_of(plan, index)))
}
impl ObjectPaintPlan {
    /// The source layers that lost a member to a combined group. Their GPU fills (`commands()`) still list the
    /// members, so a layered compositor must paint such a layer from its entries, never from those fills.
    pub(crate) fn combine_suppressed_layers(&self) -> std::collections::BTreeSet<usize> {
        let Some(listed) = &self.listed else {
            return std::collections::BTreeSet::new();
        };
        let kept: std::collections::HashSet<usize> = self
            .mixed
            .iter()
            .filter_map(|index| match index {
                PaintIndex::Contour(entry) => Some(*entry),
                _ => None,
            })
            .collect();
        listed
            .iter()
            .filter(|index| matches!(index, PaintIndex::Contour(entry) if !kept.contains(entry)))
            .map(|index| self.layer_of_entry(index))
            .collect()
    }

    /// The source layer an entry belongs to (a placed component's image or text: its host layer).
    fn layer_of_entry(&self, index: &PaintIndex) -> usize {
        match index {
            PaintIndex::Contour(index) => self
                .scene
                .entry(*index)
                .expect("pinned contour index")
                .source()
                .source_layer_index(),
            PaintIndex::Raster(index) => self.scene.rasters[*index].layer_index,
            #[cfg(all(feature = "application", feature = "project_media_codec"))]
            PaintIndex::Brush(index) => self
                .scene
                .brushes
                .as_ref()
                .map_or(0, |sources| sources.paints[*index].layer_index),
            PaintIndex::Text(index) => self.scene.texts.entries[*index].layer_index,
            PaintIndex::Component(index) => self.scene.components.paints[*index].host_layer_index,
            PaintIndex::Video(index) => self.scene.videos[*index].layer_index,
            PaintIndex::Duplicator(index) => self.scene.duplicators.paints[*index].layer_index,
            PaintIndex::Dab(_, layer) => *layer,
        }
    }
}
/// Where each visible folder's children paint: per source layer, `(stack position, child ordinal)`. A child ranks at its
/// folder's own place, after the folder's earlier children (the baseline `paint_folder_children` order: every layer whose
/// parent is the folder, bottom-up; a folder-shaped child is not consumed and stays where it is); every other layer ranks
/// at its own place. `None` when no visible folder has a child, so the authored order stands. A hidden folder paints
/// nothing and consumes nothing, exactly as in the baseline.
pub(crate) fn folder_ranks(
    packet: &crate::object_frame_packet::ObjectFramePacket,
) -> Option<std::collections::BTreeMap<usize, (usize, usize)>> {
    let placed: Vec<usize> = packet
        .numeric_order()?
        .layers()
        .iter()
        .map(|layer| layer.source_layer_index())
        .collect();
    let source = packet.source_layers();
    let mut ranks = std::collections::BTreeMap::new();
    for (position, &index) in placed.iter().enumerate() {
        let folder = source.get(index)?;
        if !crate::object_frame_packet::solo_view::layer_visible(packet.context_id(), source, folder) || !folder.is_folder_layer() {
            continue;
        }
        let mut ordinal = 0;
        for &child in &placed {
            let candidate = source.get(child)?;
            if child != index
                && !candidate.is_folder_layer()
                && candidate.parent_layer_uid.value().and_then(Option::as_deref)
                    == Some(folder.layer_uid())
            {
                ranks.entry(child).or_insert((position, 1 + ordinal));
                ordinal += 1;
            }
        }
    }
    if ranks.is_empty() {
        return None;
    }
    for (position, &index) in placed.iter().enumerate() {
        ranks.entry(index).or_insert((position, 0));
    }
    Some(ranks)
}
/// The plan's entries re-sequenced by a per-layer rank (`rank(source layer index)`), layers of equal rank and the
/// entries of one layer keeping their authored order. A folder's flattened children paint at the folder's own place
/// in the stack: a folder-only scene (every child Normal, no mask, matte or effect) is exactly the flat sequence with
/// the children moved there, because Normal compositing is associative.
pub(crate) fn ordered_entries_ranked<'a>(
    plan: &'a ObjectPaintPlan,
    rank: &dyn Fn(usize) -> (usize, usize),
) -> Vec<PaintEntry<'a>> {
    let mut keyed: Vec<((usize, usize), usize, &PaintIndex)> = plan
        .mixed
        .iter()
        .enumerate()
        .map(|(position, index)| (rank(plan.layer_of_entry(index)), position, index))
        .collect();
    keyed.sort_by_key(|(key, position, _)| (*key, *position));
    keyed
        .into_iter()
        .map(|(_, _, index)| entry_of(plan, index))
        .collect()
}
pub(super) fn prepare(
    scene: &ObjectRenderScene,
    expected: &ObjectFrameSelector<'_>,
) -> Result<(Vec<usize>, Vec<PaintIndex>), ObjectPaintPlanError> {
    #[cfg(all(feature = "application", feature = "project_media_codec"))]
    let brushes: Vec<_> = scene
        .brushes
        .iter()
        .flat_map(|sources| {
            sources
                .paints
                .iter()
                .map(|source| (source.source_index, source.layer_index))
        })
        .collect();
    #[cfg(not(all(feature = "application", feature = "project_media_codec")))]
    let brushes = Vec::new();
    prepare_with_brush_indices(scene, expected, &brushes)
}
pub(crate) fn prepare_with_brush_indices(
    scene: &ObjectRenderScene,
    expected: &ObjectFrameSelector<'_>,
    brush_indices: &[(usize, usize)],
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
        // A placed component's image paints at its host slot below.
        if source.placed {
            continue;
        }
        if selected.get(&source.source_index) != Some(&source.layer_index) {
            return Err(ObjectPaintPlanError::InvalidOrder);
        }
        rasters = rasters.with_item(source.source_index, PaintIndex::Raster(index));
    }
    #[cfg(all(feature = "application", feature = "project_media_codec"))]
    for (index, (source, layer)) in brush_indices.iter().enumerate() {
        if selected.get(source) != Some(layer) {
            return Err(ObjectPaintPlanError::InvalidOrder);
        }
        rasters = rasters.with_item(*source, PaintIndex::Brush(index));
    }
    #[cfg(not(all(feature = "application", feature = "project_media_codec")))]
    if !brush_indices.is_empty() {
        return Err(ObjectPaintPlanError::Unavailable);
    }
    let mut texts = ProducerOutput::new("text-block", pin);
    for (index, source) in scene.texts.entries.iter().enumerate() {
        // A placed component's text paints at its host slot.
        if source.placed {
            continue;
        }
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
                    for (host, index) in &scene.components.sequence {
                        if *host == slot.entry().source_layer_index {
                            mixed.push(*index);
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
impl ObjectPaintPlan {
    /// Encoded-source versions retained by this immutable plan, in stable asset order.
    pub fn raster_resource_versions(
        &self,
    ) -> Result<Vec<(String, String)>, crate::object_render_scene::ObjectSceneError> {
        crate::object_render_scene::raster_scene::declared_versions(self)
    }
}
