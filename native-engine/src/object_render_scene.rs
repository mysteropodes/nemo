//! Order-neutral CPU contours from the admitted immutable object packet port.
//! Source indices are provenance; composition/display policy is not inferred.
use crate::object_frame_packet::{ObjectFramePacket, ObjectFrameRecord, ObjectFrameSelector};
use crate::render_geometry::{ClosedCubicPath, CubicSegment};
use std::sync::Arc;

#[cfg(all(feature = "application", feature = "project_media_codec"))]
#[path = "object_reference_scene.rs"]
mod reference;
#[cfg(all(feature = "application", feature = "project_media_codec"))]
pub use reference::prepare_object_viewport_composition;

#[path = "object_scheduled_scene.rs"]
mod scheduled;
pub(crate) use scheduled::ScheduledObjectComposition;
pub use scheduled::{
    prepare_scheduled_object_composition, prepare_scheduled_object_composition_from_witness,
    LiveObjectComposition, ScheduledObjectSceneError,
};

#[path = "object_paint_plan.rs"]
mod paint;
// L08-A (#2144): lane-8 scene composition/effect family aggregator.
#[path = "scene_effect_kernels.rs"]
#[allow(dead_code)]
pub(crate) mod scene_effect_kernels;
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
    #[cfg(all(feature = "application", feature = "project_media_codec"))]
    reference: Option<reference::ReferenceScene>,
}

pub fn prepare_object_composition(
    plan: &ObjectPaintPlan,
    expected: &ObjectFrameSelector<'_>,
) -> Result<ObjectCompositionWitness, ObjectSceneError> {
    validate_pins(plan.packet(), expected)?;
    Ok(ObjectCompositionWitness {
        plan: plan.clone(),
        viewport: false,
        #[cfg(all(feature = "application", feature = "project_media_codec"))]
        reference: None,
    })
}

impl ObjectCompositionWitness {
    /// Viewport content is never an artwork export source, even with no image.
    pub(crate) fn is_viewport(&self) -> bool {
        self.viewport
    }
    /// Read the same retained snapshot used for every painted record.
    pub(crate) fn canvas_background_rgba8(&self) -> [u8; 4] {
        self.plan.packet().canvas_background_rgba8()
    }
    pub fn plan(&self) -> &ObjectPaintPlan {
        &self.plan
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
            && self.instance_id() == other.instance_id()
            && self.document_id() == other.document_id()
            && self.content_revision() == other.content_revision()
            && self.document_snapshot_id() == other.document_snapshot_id()
            && self.context_id() == other.context_id()
            && self.plan.packet().scope_kind() == other.plan.packet().scope_kind()
            && self.frame() == other.frame()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectSceneError {
    ReferenceUnavailable,
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

#[derive(Debug)]
struct CompiledContour {
    contour: ClosedCubicPath,
    envelope: [f64; 4],
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
}

/// No mutable authority, renderer resources, jobs or publication identities.
#[derive(Debug, Clone)]
pub struct ObjectRenderScene {
    packet: ObjectFramePacket,
    contours: Arc<[CompiledContour]>,
}
impl ObjectRenderScene {
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
    let contours = packet
        .records()
        .map(compile)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ObjectRenderScene {
        packet: packet.clone(),
        contours: contours.into(),
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
    if expected.context_id != "scene-root" || expected.context_id != packet.context_id() {
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

fn compile(source: ObjectFrameRecord<'_>) -> Result<CompiledContour, ObjectSceneError> {
    let reject = |reason| ObjectSceneError::UnsupportedGeometry {
        source_object_index: source.source_object_index(),
        reason,
    };
    // Field access is confined to the original record returned by the admitted
    // evaluation port. No document lookup or second source authority exists.
    let record = source.record();
    let geometry = &record.geometry;
    if record.family != "closed-solid-fill-cubic-path"
        || geometry.kind != "cubic-path"
        || !geometry.closed
        || geometry.coordinate_space != "document-world"
        || geometry.handle_space != "relative"
    {
        return Err(reject(
            "packet geometry has no admitted contour representation",
        ));
    }
    let number = |value: &serde_json::Number| {
        value
            .as_f64()
            .filter(|n| n.is_finite())
            .ok_or_else(|| reject("packet geometry coordinates must be finite f64 values"))
    };
    let segments = geometry
        .segments
        .iter()
        .map(|segment| {
            Ok(CubicSegment {
                point: [number(&segment.point.x)?, number(&segment.point.y)?],
                handle_in: [number(&segment.handle_in.x)?, number(&segment.handle_in.y)?],
                handle_out: [
                    number(&segment.handle_out.x)?,
                    number(&segment.handle_out.y)?,
                ],
            })
        })
        .collect::<Result<Vec<_>, ObjectSceneError>>()?;
    let contour = ClosedCubicPath::new(segments).map_err(reject)?;
    let envelope = contour
        .transformed_envelope([1., 0., 0., 1., 0., 0.])
        .map_err(reject)?;
    Ok(CompiledContour { contour, envelope })
}
