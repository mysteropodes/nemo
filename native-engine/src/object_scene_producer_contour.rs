//! Contour scene producer joined through the pinned registry (L07-F; F16, D32, G03a).
//!
//! The admitted record families (`closed-solid-fill-cubic-path`, and with L01-H `editable-vector-path`) are produced as a
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
//! Existing factor-one paint keeps its source numbers. The packet-owned layer factor
//! folds the pinned factor into each retained paint before layer effects; source
//! records stay unchanged. `SceneSolid::rgba_f32` keeps the same `[0, 1]` admission.

use crate::evaluation::object_image_mesh_kernels::rig_pose_eval::{
    evaluate_rig_pose, PosedStroke, RigPoseEvaluation, RigPosePin,
};
use crate::evaluation::object_scene_style_kernels::object_scene_producer_pin::{
    join_scene, PinField, ProducerEntry, ProducerOutput, SceneJoinError, ScenePin,
};
use crate::evaluation::object_scene_style_kernels::object_scene_style_item::style_of_record_with_opacity;
use crate::evaluation::object_scene_style_kernels::object_scene_style_paint::SceneSolid;
use crate::evaluation::object_vector_kernels::render_vector_eval::{
    evaluate_record_with_pose, is_vector_record, record_layer_has_active_path_fx,
};
use crate::evaluation::object_vector_kernels::render_vector_stroke::{primary_solid, stroke_reach};
use crate::object_frame_packet::text::{FrameScopeKind, SCENE_CONTEXT};
use crate::object_frame_packet::{ObjectFramePacket, ObjectFrameRecord, ObjectFrameSelector};
use crate::object_render_scene::object_scene_style_emit::prepare_item;
use crate::object_render_scene::render_vector_path::VectorShape;
use crate::object_render_scene::{ObjectPaintPlanError, ObjectRenderScene, ObjectSceneError};
use crate::render_geometry::{ClosedCubicPath, CubicSegment};
use std::collections::BTreeMap;
use vello::kurbo::BezPath;

#[path = "object_scene_vector_batch.rs"]
mod batch;

/// Producer name in the pinned registry.
pub(crate) const PRODUCER: &str = "contour";

const UNREPORTED: &str = "contour producer reported the record unavailable";

/// One admitted contour with its typed solid paint, owned by the retained scene.
/// `drawn` is the geometry the compositor draws when it is not the closed contour itself: the open or closed path of an
/// `editable-vector-path` record (L01-I). For such a record `contour` is its real closed contour when `ClosedCubicPath`
/// admits one and its clipping rectangle otherwise; `solid` is the retained primary paint (fill, else stroke, else transparent).
#[derive(Debug)]
pub(crate) struct CompiledContour {
    pub(crate) contour: ClosedCubicPath,
    pub(crate) envelope: [f64; 4],
    pub(crate) solid: SceneSolid,
    pub(crate) layer_opacity: f64,
    pub(crate) drawn: Option<BezPath>,
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
    let poses = packet_poses(packet);
    let batches = batch::prepare(packet, &poses);
    for record in packet.records() {
        let index = record.source_object_index();
        let compiled = (|| {
            let pose = match poses.get(&record.source_layer_index()) {
                Some(Ok(evaluation)) => evaluation
                    .strokes
                    .iter()
                    .find(|stroke| stroke.stroke_id == record.record().target.stroke_id),
                Some(Err(reason)) => return Err(*reason),
                None => None,
            };
            compile(packet, record, pose, &batches)
        })();
        output = match compiled {
            Ok(compiled) => output.with_item(index, compiled),
            Err(reason) => {
                rejections.push((index, reason));
                output.with_unavailable(index, reason)
            }
        };
    }
    ContourProduction { output, rejections }
}

/// Evaluate each selected rigged layer ONCE against this packet's retained snapshot.
/// Stored bones/binds are the provider. Meshes cannot be admitted here: the real
/// raster producer still refuses meshed images, so no fake rect/store is supplied.
fn packet_poses(
    packet: &ObjectFramePacket,
) -> BTreeMap<usize, Result<RigPoseEvaluation, &'static str>> {
    let mut poses = BTreeMap::new();
    for record in packet.records() {
        let index = record.source_layer_index();
        let layer = &packet.source_layers()[index];
        if layer.rig().is_none() {
            continue;
        }
        poses.entry(index).or_insert_with(|| {
            let snapshot = packet.snapshot();
            // The existing provider reads the top-level layer vector, and
            // resolves only SCENE_CONTEXT/Authored targets. Never imply a
            // component-context provider by matching opaque layer IDs alone.
            if packet.context_id() != SCENE_CONTEXT
                || packet.scope_kind() != &FrameScopeKind::Authored
                || snapshot
                    .document()
                    .layers()
                    .iter()
                    .position(|source| source.layer_uid() == layer.layer_uid())
                    != Some(index)
            {
                return Err("rig pose provider scope differs from the selected packet layer");
            }
            let evaluation = evaluate_rig_pose(
                snapshot,
                &RigPosePin::of(snapshot),
                layer.layer_uid(),
                packet.frame(),
                None,
            )
            .map_err(|_| "stored rig pose evaluation refused the pinned frame")?;
            if Some(evaluation.source_frame) != packet.source_frame_for_layer(index) {
                return Err("stored rig pose and packet exposure differ");
            }
            // Per-bind mesh unavailability does not invalidate independent
            // paths. If a selected raster actually needs that mesh, the real
            // raster producer refuses the WHOLE scene before it is retained.
            // Unlinked binds retain their typed evaluator outcome and leave the
            // corresponding stored path intact, as baseline relinkRigBinds does.
            Ok(evaluation)
        });
    }
    poses
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

fn compile(
    packet: &ObjectFramePacket,
    source: ObjectFrameRecord<'_>,
    pose: Option<&PosedStroke>,
    batches: &batch::VectorBatches,
) -> Result<CompiledContour, &'static str> {
    if source.record().is_mask() {
        batches.check(source.source_layer_index())?;
        return crate::object_render_scene::mask_inputs::compile_mask(packet, source, pose);
    }
    if source.record().has_enabled_effects() {
        return Err("per-element effect evaluation awaits its native scene consumer");
    }
    batches.check(source.source_layer_index())?;
    if is_vector_record(source.record()) {
        return compile_vector(packet, source, pose, batches);
    }
    // The baseline applies a layer's path effects to every stroke of the layer (app.js:1813); the legacy family
    // does not go through the join, so an active stack must refuse rather than draw the stored geometry.
    if record_layer_has_active_path_fx(packet, source.record()) {
        return Err("layer path effects on a legacy record await the vector join");
    }
    let record = source.record();
    let prepared = batches.get(source)?;
    let evaluated = match prepared {
        Some(prepared) => prepared.geometry.clone(),
        None => batch::legacy_rest(packet, source, pose)?,
    };
    let segments = evaluated
        .segments
        .iter()
        .map(|segment| CubicSegment {
            point: segment.point,
            handle_in: segment.handle_in,
            handle_out: segment.handle_out,
        })
        .collect();
    let contour = ClosedCubicPath::new(segments)?;
    let envelope = contour.transformed_envelope([1., 0., 0., 1., 0., 0.])?;
    let layer_opacity = match prepared {
        Some(prepared) => prepared.layer_opacity,
        None => packet
            .layer_opacity(source.source_layer_index())
            .map_err(|_| "raw layer opacity is unavailable")?,
    };
    let style =
        style_of_record_with_opacity(record, layer_opacity).map_err(|error| error.message())?;
    let solid = primary_solid(&style).map_err(|error| error.message())?;
    Ok(CompiledContour {
        contour,
        envelope,
        solid,
        layer_opacity,
        drawn: None,
    })
}

/// L01-I (G14): an `editable-vector-path` record reaches the compositor through the one evaluation join (L01-H: path
/// effects, then the parametric rebuild, at the packet's pinned frame), so scene, viewport preview and export draw the same
/// geometry:
/// * its typed style (fill, stroke, paint order, opacity) is L07-I's `style_of_record`, and `prepare_item` must accept it, so
///   a style the compositor could not draw is a typed refusal here and never a missing or flat shape;
/// * the drawn path is open or closed exactly as stored (`VectorShape`), with its clipping contour and envelope;
/// * the retained solid is the fill, else the stroke paint, else transparent: an item without any paint is valid and draws
///   nothing, exactly as the baseline draws it.
/// Whole-layer Motion comes from the prepared common batch; unjoined paint transforms and camera stay explicit.
fn compile_vector(
    packet: &ObjectFramePacket,
    source: ObjectFrameRecord<'_>,
    pose: Option<&PosedStroke>,
    batches: &batch::VectorBatches,
) -> Result<CompiledContour, &'static str> {
    let prepared = batches.get(source)?;
    let layer_opacity = match prepared {
        Some(prepared) => prepared.layer_opacity,
        None => packet
            .layer_opacity(source.source_layer_index())
            .map_err(|_| "raw layer opacity is unavailable")?,
    };
    let style = style_of_record_with_opacity(source.record(), layer_opacity)
        .map_err(|error| error.message())?;
    prepare_item(&style).map_err(|error| error.message())?;
    let evaluated = match prepared {
        Some(prepared) => prepared.geometry.clone(),
        None => evaluate_record_with_pose(packet, source, pose).map_err(|error| error.message())?,
    };
    let segments = evaluated
        .segments
        .iter()
        .map(|segment| CubicSegment {
            point: segment.point,
            handle_in: segment.handle_in,
            handle_out: segment.handle_out,
        })
        .collect::<Vec<_>>();
    let shape = VectorShape::new(&segments, evaluated.closed, stroke_reach(&style))?;
    let solid = primary_solid(&style).map_err(|error| error.message())?;
    Ok(CompiledContour {
        contour: shape.contour,
        envelope: shape.envelope,
        solid,
        layer_opacity,
        drawn: Some(shape.path),
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
#[path = "../tests/render_vector_eval_scene.rs"]
mod render_vector_eval_scene;
#[cfg(test)]
#[path = "../tests/object_scene_producer_contour_rows.rs"]
mod rows;
#[cfg(test)]
#[path = "../tests/object_scene_producer_contour.rs"]
mod tests;

#[cfg(all(test, feature = "export_job"))]
#[path = "../tests/rig_pose_scene_join.rs"]
mod rig_pose_scene_join_tests;

#[cfg(all(test, feature = "export_job", feature = "object_expression_runtime"))]
#[path = "../tests/object_scene_vector_batch.rs"]
mod vector_batch_tests;
