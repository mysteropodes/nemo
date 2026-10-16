//! Contour scene producer joined through the pinned registry (L07-F; F16, D32, G03a).
//!
//! The one admitted record family (`closed-solid-fill-cubic-path`) is produced as a
//! typed `ProducerOutput` of the L07-C registry and consumed by the paint plan:
//! * `produce` compiles every selected record of one `ObjectFramePacket` into a
//!   `CompiledContour` (admitted contour, envelope and the L07-A `SceneSolid` paint).
//!   A record that cannot be drawn becomes a typed `Unavailable` entry that keeps its
//!   `source_object_index` and reason; nothing is dropped. `into_admitted` keeps the
//!   existing atomic refusal (`ObjectSceneError::UnsupportedGeometry` for the first
//!   such record in provenance order) before any scene is retained;
//! * `command_sequence` is the paint plan's only source of order: the retained scene
//!   is reported as a producer output and joined by `join_scene`, which checks the
//!   instance/document/revision/snapshot/context/scope/frame pin once and returns the
//!   packet's `NumericOrderPlan` sequence (never producer or array order). A missing
//!   numeric plan, a record no producer claimed and a producer-reported unavailable
//!   record all stay typed errors; `ObjectPaintPlan`'s own `join` then re-validates
//!   targets and coverage exactly as before.
//!
//! Behavior is unchanged for admitted snapshots. `SceneSolid` keeps the four original
//! `serde_json::Number`s of the source fill, and the compositor converts them with the
//! same `[0, 1]` rule through `SceneSolid::rgba_f32`.

use crate::evaluation::object_scene_style_kernels::object_scene_producer_pin::{
    join_scene, PinField, ProducerEntry, ProducerOutput, SceneJoinError, ScenePin,
};
use crate::evaluation::object_scene_style_kernels::object_scene_style_paint::SceneSolid;
use crate::object_frame_packet::{ObjectFramePacket, ObjectFrameRecord, ObjectFrameSelector};
use crate::object_render_scene::{ObjectPaintPlanError, ObjectRenderScene, ObjectSceneError};
use crate::render_geometry::{ClosedCubicPath, CubicSegment};

/// Producer name in the pinned registry.
pub(crate) const PRODUCER: &str = "contour";

const UNREPORTED: &str = "contour producer reported the record unavailable";

/// One admitted contour with its typed solid paint, owned by the retained scene.
#[derive(Debug)]
pub(crate) struct CompiledContour {
    pub(crate) contour: ClosedCubicPath,
    pub(crate) envelope: [f64; 4],
    pub(crate) solid: SceneSolid,
}

/// Everything the contour producer reports for one packet. `rejections` holds the
/// static reasons of the `Unavailable` entries, in provenance order.
pub(crate) struct ContourProduction {
    output: ProducerOutput<CompiledContour>,
    rejections: Vec<(usize, &'static str)>,
}

/// Compile every selected record of the packet; one entry per record.
pub(crate) fn produce(packet: &ObjectFramePacket) -> ContourProduction {
    let mut output = ProducerOutput::new(PRODUCER, ScenePin::of_packet(packet));
    let mut rejections = Vec::new();
    for record in packet.records() {
        let index = record.source_object_index();
        output = match compile(record) {
            Ok(compiled) => output.with_item(index, compiled),
            Err(reason) => {
                rejections.push((index, reason));
                output.with_unavailable(index, reason)
            }
        };
    }
    ContourProduction { output, rejections }
}

impl ContourProduction {
    /// The retained scene's contours in packet provenance order, or the atomic
    /// refusal for the first record that is not drawable.
    pub(crate) fn into_admitted(self) -> Result<Vec<CompiledContour>, ObjectSceneError> {
        let Self { output, rejections } = self;
        let mut contours = Vec::with_capacity(output.entries.len());
        for entry in output.entries {
            match entry {
                ProducerEntry::Item { payload, .. } => contours.push(payload),
                ProducerEntry::Unavailable {
                    source_object_index,
                    ..
                } => {
                    let reason = rejections
                        .iter()
                        .find(|(index, _)| *index == source_object_index)
                        .map_or(UNREPORTED, |(_, reason)| *reason);
                    return Err(ObjectSceneError::UnsupportedGeometry {
                        source_object_index,
                        reason,
                    });
                }
            }
        }
        Ok(contours)
    }
}

fn compile(source: ObjectFrameRecord<'_>) -> Result<CompiledContour, &'static str> {
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
        return Err("packet geometry has no admitted contour representation");
    }
    let number = |value: &serde_json::Number| {
        value
            .as_f64()
            .filter(|n| n.is_finite())
            .ok_or("packet geometry coordinates must be finite f64 values")
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
        .collect::<Result<Vec<_>, &'static str>>()?;
    let contour = ClosedCubicPath::new(segments)?;
    let envelope = contour.transformed_envelope([1., 0., 0., 1., 0., 0.])?;
    let solid = SceneSolid::from_fill(&record.fill).map_err(|error| error.message())?;
    Ok(CompiledContour {
        contour,
        envelope,
        solid,
    })
}

/// Order for the paint plan: layers in `NumericOrderPlan` order, each with its
/// elements. The retained scene is the producer, the selector is the expected pin.
pub(crate) fn command_sequence(
    scene: &ObjectRenderScene,
    expected: &ObjectFrameSelector<'_>,
) -> Result<Vec<(usize, Vec<usize>)>, ObjectPaintPlanError> {
    let mut output = ProducerOutput::new(PRODUCER, ScenePin::of_packet(scene.packet()));
    for (position, entry) in scene.entries().enumerate() {
        output = output.with_item(entry.source().source_object_index(), position);
    }
    sequence_of(scene.packet(), &pin_of_selector(expected), output)
}

/// Join one producer output onto the packet and read the layer/element sequence.
/// Invisible layers keep their slot in the plan and contribute no element.
pub(crate) fn sequence_of<T>(
    packet: &ObjectFramePacket,
    expected: &ScenePin,
    output: ProducerOutput<T>,
) -> Result<Vec<(usize, Vec<usize>)>, ObjectPaintPlanError> {
    let joined = join_scene(packet, expected, vec![output]).map_err(plan_error)?;
    if !joined.is_complete() {
        return Err(ObjectPaintPlanError::Unavailable);
    }
    let order = packet
        .numeric_order()
        .ok_or(ObjectPaintPlanError::Unavailable)?;
    let mut entries = joined.entries().iter().peekable();
    let mut sequence = Vec::with_capacity(order.layers().len());
    for layer in order.layers() {
        let layer_index = layer.source_layer_index();
        let mut elements = Vec::new();
        while let Some(entry) = entries.next_if(|entry| entry.source_layer_index() == layer_index) {
            elements.push(entry.source_object_index());
        }
        sequence.push((layer_index, elements));
    }
    if entries.next().is_some() {
        return Err(ObjectPaintPlanError::InvalidOrder);
    }
    Ok(sequence)
}

fn pin_of_selector(selector: &ObjectFrameSelector<'_>) -> ScenePin {
    ScenePin {
        instance_id: selector.instance_id.to_owned(),
        document_id: selector.document_id.to_owned(),
        content_revision: selector.content_revision,
        document_snapshot_id: selector.document_snapshot_id.to_owned(),
        context_id: selector.context_id.to_owned(),
        scope_kind: selector.scope_kind.clone(),
        frame: selector.frame,
    }
}

fn plan_error(error: SceneJoinError) -> ObjectPaintPlanError {
    match error {
        SceneJoinError::WrongPin { field, .. } => ObjectPaintPlanError::Pins(match field {
            PinField::Instance => ObjectSceneError::WrongInstance,
            PinField::Document => ObjectSceneError::WrongDocument,
            PinField::Revision => ObjectSceneError::WrongRevision,
            PinField::Snapshot => ObjectSceneError::WrongSnapshot,
            PinField::Context => ObjectSceneError::InvalidContext,
            PinField::Scope => ObjectSceneError::WrongScope,
            PinField::Frame => ObjectSceneError::WrongFrame,
        }),
        SceneJoinError::NoNumericOrder => ObjectPaintPlanError::Unavailable,
        SceneJoinError::EmptyProducerName
        | SceneJoinError::DuplicateProducer { .. }
        | SceneJoinError::UnknownSource { .. }
        | SceneJoinError::DuplicateClaim { .. }
        | SceneJoinError::InvalidResource { .. }
        | SceneJoinError::ResourceConflict { .. }
        | SceneJoinError::InvalidOrder => ObjectPaintPlanError::InvalidOrder,
    }
}

#[cfg(test)]
#[path = "../tests/object_scene_producer_contour_controls.rs"]
mod controls;
#[cfg(test)]
#[path = "../tests/object_scene_producer_contour_rows.rs"]
mod rows;
#[cfg(test)]
#[path = "../tests/object_scene_producer_contour.rs"]
mod tests;
