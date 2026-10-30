//! Order-neutral CPU contours from the admitted immutable object packet port.
//! Source indices are provenance; composition/display policy is not inferred.
use crate::evaluation::object_scene_style_kernels::object_scene_style_paint::SceneSolid;
use crate::object_frame_packet::{ObjectFramePacket, ObjectFrameRecord, ObjectFrameSelector};
use crate::render_geometry::ClosedCubicPath;
use std::sync::Arc;
use vello::kurbo::BezPath;

#[path = "object_component_render_scene.rs"]
pub(crate) mod component_scene;
#[cfg(all(test, feature = "history"))]
#[path = "../tests/object_component_render_scene.rs"]
mod component_tests;
#[cfg(all(test, feature = "history"))]
#[path = "../tests/object_component_exposed_override.rs"]
mod component_override_tests;

use scene_effect_kernels::object_scene_resource_set::{
    same_scene_resources, SceneResourceMismatch,
};
pub use scene_effect_kernels::object_scene_resource_set::{
    ObjectSceneResourceSet, SceneResourceMismatchKind,
};
use scene_effect_kernels::scene_effect_scene_join::SceneEffectPlan;

#[cfg(all(feature = "application", feature = "project_media_codec"))]
#[path = "object_reference_scene.rs"]
mod reference;
#[cfg(all(feature = "application", feature = "project_media_codec"))]
pub use reference::{
    object_viewport_quality, prepare_object_camera_viewport_composition,
    prepare_object_navigated_viewport_composition, prepare_object_viewport_composition,
};

#[path = "object_scheduled_scene.rs"]
mod scheduled;
pub(crate) use scheduled::ScheduledObjectComposition;
pub use scheduled::{
    prepare_scheduled_object_composition, prepare_scheduled_object_composition_from_witness,
    LiveObjectComposition, ScheduledObjectSceneError,
};

#[path = "object_brush_admission.rs"]
#[cfg(all(feature = "application", feature = "project_media_codec"))]
mod brush_admission;
#[path = "object_brush_scene_join.rs"]
#[cfg(all(feature = "application", feature = "project_media_codec"))]
pub(crate) mod brush_scene;
#[cfg(all(feature = "application", feature = "project_media_codec"))]
pub use brush_admission::ObjectBrushAdmission;
#[path = "object_brush_paint_join.rs"]
#[cfg(all(feature = "application", feature = "project_media_codec"))]
mod brush_paint;
#[path = "object_raster_scene_join.rs"]
pub(crate) mod raster_scene;
// T12 / T28.media: video placements drawn from decoded source frames.
#[path = "object_media_scene_join.rs"]
pub(crate) mod media_scene;
pub(crate) use paint::raster::{ordered_entries, PaintEntry, PaintIndex};
pub(crate) use raster_scene::declared_versions as object_raster_resource_versions;
#[path = "object_mask_inputs.rs"]
pub(crate) mod mask_inputs;
#[path = "object_paint_plan.rs"]
mod paint;
// L07-F (#2113): contour scene producer joined through the L07-C pin registry.
#[path = "object_duplicator_scene.rs"]
pub(crate) mod duplicator_scene;
// L03-P (#2252): the single rig pose call site shared by the contour and raster joins.
#[path = "object_rig_packet_pose.rs"]
pub(crate) mod rig_packet_pose;
#[path = "object_scene_producer_contour.rs"]
mod object_scene_producer_contour;
use object_scene_producer_contour::CompiledContour;
// L07-J (#2113): leased bitmap-brush ink drawn into a Vello scene (scene consumer of the brush producer).
#[path = "object_scene_producer_brush_ink.rs"]
#[allow(dead_code)]
pub(crate) mod object_scene_producer_brush_ink;
// L07-I (#2113): typed scene style -> Vello paint (fill, stroke, gradient, paint order); the compositor draws through it.
#[path = "object_scene_style_emit.rs"]
pub(crate) mod object_scene_style_emit;
// L07-L (#2113): text block scene producer joined through the L07-C pin registry.
#[path = "object_scene_producer_text.rs"]
#[allow(dead_code)]
pub(crate) mod object_scene_producer_text;
#[path = "object_text_scene_join.rs"]
pub(crate) mod text_scene;
// L08-A (#2144): lane-8 scene composition/effect family aggregator.
#[path = "scene_effect_kernels.rs"]
#[allow(dead_code)]
pub(crate) mod scene_effect_kernels;
// L06-K (#2112): lane-6 text/raster scene join family aggregator.
#[path = "text_glyph_scene_kernels.rs"]
#[allow(dead_code)]
pub(crate) mod text_glyph_scene_kernels;
// L03-S (#2109): lane-3 image-mesh scene join family aggregator (scene item and vello replay).
#[path = "render_image_mesh_kernels.rs"]
#[allow(dead_code)]
pub(crate) mod render_image_mesh_kernels;
// L01-I (#2107): drawn geometry of the editable vector family (open or closed path, clipping contour, envelope).
#[path = "render_vector_path.rs"]
pub(crate) mod render_vector_path;
#[cfg(all(test, feature = "history"))]
pub(crate) use paint::tests::paint_contents;
#[cfg(test)]
pub(crate) use paint::tests::{
    document as paint_test_document, frame_cases, join_cases, order_cases,
};
pub use paint::{
    prepare_object_paint_plan, ObjectPaintCommand, ObjectPaintPlan, ObjectPaintPlanError,
};

/// Render-scene-owned composition admission. Its private retained plan cannot
/// be constructed from unvalidated caller pins or detached identity strings.
#[derive(Debug, Clone)]
pub struct ObjectCompositionWitness {
    plan: ObjectPaintPlan,
    viewport: bool,
    viewport_quality: Option<String>,
    camera: Option<[f64; 6]>,
    resources: Option<Arc<ObjectSceneResourceSet>>,
    /// Joined layer/effect plan of this witness' packet (L08-K); `None` keeps the flat fast path.
    scene_effects: Option<Arc<SceneEffectPlan>>,
    /// Decoded video frames of the plan's video slots (T12); `None` while undecoded.
    media: Option<Arc<media_scene::MediaDraws>>,
    #[cfg(all(feature = "application", feature = "project_media_codec"))]
    rasters: Option<Arc<raster_scene::RasterResources>>,
    #[cfg(all(feature = "application", feature = "project_media_codec"))]
    reference: Option<reference::ReferenceScene>,
}

pub fn prepare_object_composition(
    plan: &ObjectPaintPlan,
    expected: &ObjectFrameSelector<'_>,
) -> Result<ObjectCompositionWitness, ObjectSceneError> {
    validate_pins(plan.packet(), expected)?;
    let camera = plan.packet().stored_camera_matrix().map_err(|_| {
        ObjectSceneError::UnsupportedGeometry {
            source_object_index: 0,
            reason: "stored camera transform unavailable",
        }
    })?;
    let witness = ObjectCompositionWitness {
        plan: plan.clone(),
        viewport: false,
        viewport_quality: None,
        camera,
        resources: None,
        scene_effects: None,
        media: None,
        #[cfg(all(feature = "application", feature = "project_media_codec"))]
        reference: None,
        #[cfg(all(feature = "application", feature = "project_media_codec"))]
        rasters: None,
    };
    #[cfg(all(feature = "application", feature = "project_media_codec"))]
    {
        return brush_paint::attach(text_scene::attach(raster_scene::attach(witness, None)?)?);
    }
    #[cfg(not(all(feature = "application", feature = "project_media_codec")))]
    {
        if !plan.raster_sources().is_empty() {
            return Err(raster_scene::unavailable(
                0,
                "native raster resources require project media support",
            ));
        }
        text_scene::attach(witness)
    }
}

impl ObjectCompositionWitness {
    /// Viewport content is never an artwork export source, even with no image.
    pub(crate) fn expected_quality(&self) -> &str {
        self.viewport_quality.as_deref().unwrap_or("final")
    }
    pub(crate) fn is_viewport(&self) -> bool {
        self.viewport
    }
    /// Artwork-only transform; unlocked Canvas presentation stays untransformed.
    pub(crate) fn camera_matrix(&self) -> Option<[f64; 6]> {
        self.camera
    }
    pub(crate) fn camera_canvas_dimensions(&self) -> Option<(u32, u32)> {
        self.camera?;
        self.plan
            .packet()
            .snapshot()
            .project()
            .map(|p| p.project().canvas_dimensions())
    }
    /// Read the same retained snapshot used for every painted record.
    pub(crate) fn canvas_background_rgba8(&self) -> [u8; 4] {
        self.plan.packet().canvas_background_rgba8()
    }
    pub fn plan(&self) -> &ObjectPaintPlan {
        &self.plan
    }
    /// Attach the immutable declared-resource set (and its retention tokens).
    /// The tokens live as long as any clone of the witness, permit or result.
    pub fn with_scene_resources(mut self, resources: Arc<ObjectSceneResourceSet>) -> Self {
        self.resources = Some(resources);
        self
    }
    pub fn scene_resources(&self) -> Option<&Arc<ObjectSceneResourceSet>> {
        self.resources.as_ref()
    }
    /// Attach the joined layer/effect plan of THIS witness' packet (L08-K). A plan joined onto another
    /// instance/document/revision/snapshot/context/scope/frame is refused and nothing is attached.
    pub(crate) fn with_scene_effects(
        mut self,
        effects: Arc<SceneEffectPlan>,
    ) -> Result<Self, ObjectSceneError> {
        if !effects.is_for_packet(self.plan.packet()) {
            return Err(ObjectSceneError::WrongSnapshot);
        }
        self.scene_effects = Some(effects);
        Ok(self)
    }
    pub(crate) fn scene_effects(&self) -> Option<&Arc<SceneEffectPlan>> {
        self.scene_effects.as_ref()
    }
    /// Set equality with an evaluation key's declaration; an absent set is empty.
    pub(crate) fn verify_scene_resources(
        &self,
        declared: &std::collections::BTreeMap<String, String>,
    ) -> Result<(), SceneResourceMismatch> {
        #[cfg(all(feature = "application", feature = "project_media_codec"))]
        if let Some(rasters) = &self.rasters {
            rasters.verify(declared)?;
        }
        text_scene::verify(self, declared)?;
        match &self.resources {
            Some(set) => set.verify_declared(declared),
            None => ObjectSceneResourceSet::empty().verify_declared(declared),
        }
    }
    pub fn instance_id(&self) -> &str {
        self.plan.packet().instance_id()
    }
    pub fn document_id(&self) -> &str {
        self.plan.packet().document_id()
    }
    pub fn content_revision(&self) -> u64 {
        self.plan.packet().content_revision()
    }
    pub fn document_snapshot_id(&self) -> &str {
        self.plan.packet().document_snapshot_id()
    }
    pub fn context_id(&self) -> &str {
        self.plan.packet().context_id()
    }
    pub fn frame(&self) -> u32 {
        self.plan.packet().frame()
    }
    /// Compare all original pins, including scope, without leaking evaluator
    /// selector/packet types into composition consumers.
    pub fn same_identity_as(&self, other: &Self) -> bool {
        self.viewport == other.viewport
            && self.viewport_quality == other.viewport_quality
            && self.camera == other.camera
            && self.instance_id() == other.instance_id()
            && self.document_id() == other.document_id()
            && self.content_revision() == other.content_revision()
            && self.document_snapshot_id() == other.document_snapshot_id()
            && self.context_id() == other.context_id()
            && self.plan.packet().scope_kind() == other.plan.packet().scope_kind()
            && self.frame() == other.frame()
            && same_scene_resources(self.resources.as_deref(), other.resources.as_deref())
            && self.scene_effects == other.scene_effects
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectSceneError {
    ReferenceUnavailable,
    UnsupportedComponent {
        source_layer_index: usize,
        reason: &'static str,
    },
    WrongInstance,
    WrongDocument,
    WrongRevision,
    WrongSnapshot,
    InvalidContext,
    WrongScope,
    WrongFrame,
    UnsupportedGeometry {
        source_object_index: usize,
        reason: &'static str,
    },
}

/// Borrow both the original scoped source record and its admitted contour.
#[derive(Debug, Clone, Copy)]
pub struct ObjectRenderEntry<'a> {
    source: ObjectFrameRecord<'a>,
    compiled: &'a CompiledContour,
}
impl<'a> ObjectRenderEntry<'a> {
    pub fn source(&self) -> ObjectFrameRecord<'a> {
        self.source
    }
    pub fn contour(&self) -> &'a ClosedCubicPath {
        &self.compiled.contour
    }
    pub fn envelope(&self) -> [f64; 4] {
        self.compiled.envelope
    }
    /// The geometry the compositor draws (L01-I): the open or closed evaluated path of a vector record, otherwise the path
    /// of the closed contour itself. Use this, not `contour()`, to paint.
    pub(crate) fn path(&self) -> &'a BezPath {
        let compiled = self.compiled;
        compiled
            .drawn
            .as_ref()
            .unwrap_or_else(|| compiled.contour.path())
    }
    pub(crate) fn layer_opacity(&self) -> f64 {
        self.compiled.layer_opacity
    }
    /// The whole-layer pose the item's paint style follows.
    pub(crate) fn paint_pose(
        &self,
    ) -> &'a crate::evaluation::object_scene_style_kernels::object_scene_paint_pose::PaintPose {
        &self.compiled.paint
    }
    /// Typed straight-RGBA paint admitted with the contour (L07-A `SceneSolid`).
    pub(crate) fn solid(&self) -> &'a SceneSolid {
        &self.compiled.solid
    }
}

/// No mutable authority, renderer resources, jobs or publication identities.
#[derive(Debug, Clone)]
pub struct ObjectRenderScene {
    packet: ObjectFramePacket,
    duplicators: Arc<duplicator_scene::DuplicatorScene>,
    contours: Arc<[CompiledContour]>,
    rasters: Arc<[raster_scene::RasterSource]>,
    texts: text_scene::TextSources,
    components: component_scene::ComponentSources,
    #[cfg(all(feature = "application", feature = "project_media_codec"))]
    brushes: Option<Arc<brush_scene::BrushSources>>,
    videos: Arc<[media_scene::VideoSlot]>,
}
impl ObjectRenderScene {
    pub(crate) fn brush_paints_empty(&self) -> bool {
        #[cfg(all(feature = "application", feature = "project_media_codec"))]
        {
            self.brushes
                .as_ref()
                .is_none_or(|sources| sources.paints.is_empty())
        }
        #[cfg(not(all(feature = "application", feature = "project_media_codec")))]
        {
            true
        }
    }
    pub fn packet(&self) -> &ObjectFramePacket {
        &self.packet
    }
    /// Index is the selected packet's provenance position, never paint order.
    pub fn entry(&self, index: usize) -> Option<ObjectRenderEntry<'_>> {
        Some(ObjectRenderEntry {
            source: self.packet.record_at(index)?,
            compiled: self.contours.get(index)?,
        })
    }
    /// Encounter order remains source provenance, never a paint-order promise.
    pub fn entries(&self) -> impl ExactSizeIterator<Item = ObjectRenderEntry<'_>> + '_ {
        self.packet
            .records()
            .zip(self.contours.iter())
            .map(|(source, compiled)| ObjectRenderEntry { source, compiled })
    }
}

/// Admit all selected contours atomically at the existing identity-space gate.
/// The selector describes this exact packet, including scope and frame.
pub fn prepare_object_render_scene(
    packet: &ObjectFramePacket,
    expected: &ObjectFrameSelector<'_>,
) -> Result<ObjectRenderScene, ObjectSceneError> {
    validate_pins(packet, expected)?;
    let contours = object_scene_producer_contour::produce(packet).into_admitted()?;
    let duplicators = Arc::new(duplicator_scene::prepare(packet)?);
    let mut rasters = raster_scene::sources(packet)?;
    let mut texts = text_scene::prepare(packet)?;
    let components = component_scene::prepare(packet, expected, &mut rasters, &mut texts)?;
    Ok(ObjectRenderScene {
        duplicators,
        packet: packet.clone(),
        contours: contours.into(),
        rasters: rasters.into(),
        texts,
        components,
        #[cfg(all(feature = "application", feature = "project_media_codec"))]
        brushes: None,
        videos: media_scene::slots(packet)?.into(),
    })
}

fn validate_pins(
    packet: &ObjectFramePacket,
    expected: &ObjectFrameSelector<'_>,
) -> Result<(), ObjectSceneError> {
    if expected.instance_id != packet.instance_id() {
        return Err(ObjectSceneError::WrongInstance);
    }
    if expected.document_id != packet.document_id() {
        return Err(ObjectSceneError::WrongDocument);
    }
    if expected.content_revision != packet.content_revision() {
        return Err(ObjectSceneError::WrongRevision);
    }
    if expected.document_snapshot_id != packet.document_snapshot_id() {
        return Err(ObjectSceneError::WrongSnapshot);
    }
    if expected.context_id != packet.context_id() {
        return Err(ObjectSceneError::InvalidContext);
    }
    if &expected.scope_kind != packet.scope_kind() {
        return Err(ObjectSceneError::WrongScope);
    }
    if expected.frame != packet.frame() {
        return Err(ObjectSceneError::WrongFrame);
    }
    Ok(())
}

#[cfg(all(
    test,
    feature = "history",
    feature = "application",
    feature = "project_media_codec"
))]
#[path = "../tests/object_raster_scene_join.rs"]
mod raster_join_tests;
#[cfg(all(
    test,
    feature = "history",
    feature = "application",
    feature = "project_media_codec"
))]
#[path = "../tests/object_component_render_image.rs"]
mod component_image_tests;
#[cfg(all(
    test,
    feature = "history",
    feature = "application",
    feature = "project_media_codec"
))]
#[path = "../tests/object_component_convert_images.rs"]
mod component_convert_image_tests;

#[path = "object_subselect_projection.rs"]
mod subselect_projection;
