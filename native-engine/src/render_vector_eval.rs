//! Consumer join of the editable vector family at a pinned snapshot and frame (G22 + G13, #2107, leaf L01-H).
//!
//! `evaluate_vector` is the one function that turns a stored `editable-vector-path` record into the geometry the
//! scene, the viewport preview and the export all draw. It reproduces the baseline order at fbe4b8a3, which is NOT
//! the order one would design:
//! 1. path effects first: the layer's ordered `pathFx` stack applied to the stored anchors (app.js:1813
//!    `getEffectiveStrokesRendered`, `SMPathFx.apply(base, ld, frameIdx)`), the L01-A kernel;
//! 2. the parametric rebuild afterwards (engine-bridge.js:1652/1672, export.js:193 `applyParamShapeFor`), the L01-B
//!    kernel, and only for a shape with an animated parameter (`hasParamShapeMotionFor`). The rebuild takes the min/max
//!    rectangle of the anchors it is GIVEN, which are the post-effect anchors, and replaces them with a fresh shape:
//!    a zigzagged animated rectangle is rebuilt as a clean rounded rectangle in the zigzag-expanded box. That is the
//!    characterized quirk; it is reproduced and pinned, not fixed. `paramShape.box` is never read.
//!
//! An animated parameter is a non-empty Motion track of the stroke's element holder, sampled at the pinned frame with the
//! D11 raw sampler (`sample_property_raw`); an unkeyed parameter reads the stored `paramShape` member (`cv(key, ps...)`),
//! never the holder's static value or the catalog default. Evaluation only borrows the snapshot; the result is an owned
//! value, so a caller can neither reach nor change rest geometry. Frames 0, 10, 0 give the same frame-0 vector.
//!
//! Not delivered here (typed, never faked): a parameter track carrying an enabled expression needs the expression
//! evaluator (`ExpressionUnavailable`); raw Position of ordinary root vector layers is consumed after the rebuild.
//! Other Motion transforms, parents, camera and 3D remain unavailable until their own native consumers join.
use crate::evaluation::object_vector_kernels::object_parametric_geometry::boxed::{
    apply_param_shape_animated, has_param_shape_motion,
};
use crate::evaluation::object_vector_kernels::object_parametric_geometry::{
    ParamGeomError, ParamSegment,
};
use crate::evaluation::object_vector_kernels::object_path_effects_kernel::{
    apply as apply_path_fx, has_any, PathFxEntry, PathFxError, PathFxStroke, PathSegment,
};
use crate::object_document::{
    ObjectLayer, ObjectPoint, ObjectRecord, ObjectSegment, VECTOR_FAMILY,
};
use crate::object_frame_packet::{ObjectFramePacket, ObjectFrameRecord, PropertySampleError};
use crate::revision::ObjectSnapshot;
use serde_json::Number;

#[path = "render_vector_eval_motion.rs"]
mod motion;

#[path = "render_vector_eval_params.rs"]
pub(crate) mod params;
// Read by the packet guard (optional patch 3); unused while that patch is declined.
#[allow(unused_imports)]
pub(crate) use params::holders_fully_consumed;

/// One evaluated anchor: point and RELATIVE handles, all present.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct EvalSegment {
    pub point: [f64; 2],
    pub handle_in: [f64; 2],
    pub handle_out: [f64; 2],
}

/// The evaluated geometry of one record: what scene, preview and export draw. Owned, detached from the snapshot.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct EvaluatedVector {
    pub source_object_index: usize,
    pub closed: bool,
    pub segments: Vec<EvalSegment>,
}

/// Why a record could not be evaluated. Nothing is published for the record when one is returned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VectorEvalError {
    /// The object index, or the layer it names, is not part of the pinned snapshot.
    MissingRecord,
    NotVectorFamily,
    /// A coordinate or shape parameter is not a finite f64 (admission already refuses it).
    NonFiniteInput,
    UnknownShapeKind,
    PathFx(PathFxError),
    Rebuild(ParamGeomError),
    Sample {
        property: &'static str,
        error: PropertySampleError,
    },
    LayerMotionUnavailable,
    RigCompositionUnavailable,
    ExpressionUnavailable {
        property: &'static str,
    },
}

impl VectorEvalError {
    pub(crate) fn message(&self) -> &'static str {
        match self {
            Self::MissingRecord => "vector record or its layer is not part of the pinned snapshot",
            Self::NotVectorFamily => "record is not an editable-vector-path",
            Self::NonFiniteInput => {
                "vector geometry and shape parameters must be finite f64 values"
            }
            Self::UnknownShapeKind => "paramShape kind is not rect, ellipse or star",
            Self::PathFx(_) => "path effect exceeds the native sample budget",
            Self::Rebuild(_) => "parametric rebuild refused its input",
            Self::Sample { .. } => "animated shape parameter could not be sampled",
            Self::RigCompositionUnavailable => {
                "rig pose with path effects, animated shape rebuild or brush mirrors is unavailable"
            }
            Self::LayerMotionUnavailable => "layer Motion has no admitted native vector consumer",
            Self::ExpressionUnavailable { .. } => {
                "a shape parameter carries an expression and the expression evaluator is not joined"
            }
        }
    }
}

/// What the join reads from the immutable snapshot for one record, before any stage runs.
pub(crate) struct StoredVector<'a> {
    pub record: &'a ObjectRecord,
    /// The stored anchors as the path-effect kernel reads them (relative handles present).
    pub stroke: PathFxStroke<()>,
    /// The layer's stored ordered stack (`None` = absent), in storage order.
    pub stack: Option<Vec<Option<PathFxEntry>>>,
}

pub(crate) fn finite(number: &Number) -> Result<f64, VectorEvalError> {
    number
        .as_f64()
        .filter(|value| value.is_finite())
        .ok_or(VectorEvalError::NonFiniteInput)
}

fn pair(point: &ObjectPoint) -> Result<[f64; 2], VectorEvalError> {
    Ok([finite(&point.x)?, finite(&point.y)?])
}

fn stored_segment(segment: &ObjectSegment) -> Result<PathSegment, VectorEvalError> {
    Ok(PathSegment {
        point: pair(&segment.point)?,
        handle_in: Some(pair(&segment.handle_in)?),
        handle_out: Some(pair(&segment.handle_out)?),
    })
}

/// The layer's stored ordered stack as the path-effect kernel reads it (`None` = absent), in storage order.
fn stack_of(layer: &ObjectLayer) -> Option<Vec<Option<PathFxEntry>>> {
    layer.path_fx().map(|stored| {
        stored
            .iter()
            .map(|entry| {
                Some(PathFxEntry {
                    fx_type: entry.fx_type.clone(),
                    enabled: entry.enabled_flag(),
                    params: [entry.param(0), entry.param(1), entry.param(2)],
                })
            })
            .collect::<Vec<_>>()
    })
}

/// Whether the record is an `editable-vector-path` record. Kept here (evaluation group) so the render-scene funnel
/// reaches the family test through the already-pinned render-scene -> evaluation edge and imports no document type.
pub(crate) fn is_vector_record(record: &ObjectRecord) -> bool {
    record.family == VECTOR_FAMILY
}

/// Whether the record's layer holds a path-effect entry the kernel treats as active (`activeStack`: enabled, known
/// type). The baseline applies a layer's stack to EVERY stroke of the layer (app.js:1813), so a family that does not
/// go through the join must refuse instead of drawing its stored geometry. A record whose layer is absent reads false.
pub(crate) fn record_layer_has_active_path_fx(
    packet: &ObjectFramePacket,
    record: &ObjectRecord,
) -> bool {
    if record.target().context_id() != packet.context_id() {
        return false;
    }
    packet
        .source_layers()
        .iter()
        .find(|layer| layer.layer_uid() == record.target.layer_uid)
        .and_then(stack_of)
        .is_some_and(|stack| has_any(Some(stack.as_slice())))
}

/// Read record `object_index` and its layer's stack from the pinned snapshot.
pub(crate) fn load_stored(
    snapshot: &ObjectSnapshot,
    object_index: usize,
) -> Result<StoredVector<'_>, VectorEvalError> {
    let document = snapshot.document();
    let record = document
        .objects()
        .get(object_index)
        .ok_or(VectorEvalError::MissingRecord)?;
    stored_from_record(
        record,
        document
            .layers()
            .iter()
            .find(|layer| layer.layer_uid() == record.target.layer_uid),
    )
}

/// Build from an immutable record/layer pair; no object index is readdressed here.
fn stored_from_record<'a>(
    record: &'a ObjectRecord,
    layer: Option<&ObjectLayer>,
) -> Result<StoredVector<'a>, VectorEvalError> {
    if record.family != VECTOR_FAMILY {
        return Err(VectorEvalError::NotVectorFamily);
    }
    let layer = layer.ok_or(VectorEvalError::MissingRecord)?;
    let segments = record
        .geometry
        .segments
        .iter()
        .map(stored_segment)
        .collect::<Result<Vec<_>, _>>()?;
    let stack = stack_of(layer);
    Ok(StoredVector {
        record,
        stroke: PathFxStroke {
            is_raster: false,
            closed: record.geometry.closed,
            segments: Some(segments),
            rest: (),
        },
        stack,
    })
}

/// Packet records already borrow their selected retained context. Validate the
/// matching selected layer/frame before consuming that pair; never index root objects.
fn load_packet_stored<'a>(
    packet: &ObjectFramePacket,
    source: ObjectFrameRecord<'a>,
) -> Result<StoredVector<'a>, VectorEvalError> {
    if !packet.owns_record(source) {
        return Err(VectorEvalError::MissingRecord);
    }
    let record = source.record();
    let target = record.target();
    let layer = packet
        .source_layers()
        .get(source.source_layer_index())
        .filter(|layer| {
            layer.layer_uid() == target.layer_uid()
                && target.context_id() == packet.context_id()
                && target.scope_kind() == packet.scope_kind()
                && packet.source_frame_for_layer(source.source_layer_index())
                    == Some(target.frame())
        });
    stored_from_record(record, layer)
}

/// Stage 1, `SMPathFx.apply(strokes, ld, frameIdx)` for one stroke: the active stack in storage order, each effect
/// applied to the previous output. A stack that changes nothing returns the input unchanged.
pub(crate) fn run_path_fx(
    stack: Option<&[Option<PathFxEntry>]>,
    input: &PathFxStroke<()>,
    frame: u32,
) -> Result<PathFxStroke<()>, VectorEvalError> {
    let strokes = [input.clone()];
    let out =
        apply_path_fx(&strokes, stack, Some(f64::from(frame))).map_err(VectorEvalError::PathFx)?;
    // A named local, not a tail expression: the iterator temporary borrows `strokes`.
    let first = out
        .into_iter()
        .next()
        .map(std::borrow::Cow::into_owned)
        .expect("apply returns one stroke per input stroke");
    Ok(first)
}

/// Current-host path FX precede duplicator placement; shifted/pool source
/// drawings intentionally do not pass through this rendered-base stage.
pub(crate) fn duplicator_base(
    record: &ObjectRecord,
    layer: &ObjectLayer,
    frame: u32,
) -> Result<ObjectRecord, VectorEvalError> {
    let stack = stack_of(layer);
    if !has_any(stack.as_deref()) {
        return Ok(record.clone());
    }
    let input = PathFxStroke {
        is_raster: false,
        closed: record.geometry.closed,
        segments: Some(
            record
                .geometry
                .segments
                .iter()
                .map(stored_segment)
                .collect::<Result<Vec<_>, _>>()?,
        ),
        rest: (),
    };
    let after = run_path_fx(stack.as_deref(), &input, frame)?;
    let pair = |p: [f64; 2]| -> Result<ObjectPoint, VectorEvalError> {
        Ok(ObjectPoint {
            x: Number::from_f64(p[0]).ok_or(VectorEvalError::NonFiniteInput)?,
            y: Number::from_f64(p[1]).ok_or(VectorEvalError::NonFiniteInput)?,
        })
    };
    let mut output = record.clone();
    output.geometry.closed = after.closed;
    output.geometry.segments = after
        .segments
        .unwrap_or_default()
        .into_iter()
        .map(|segment| {
            Ok(ObjectSegment {
                point: pair(segment.point)?,
                handle_in: pair(segment.handle_in.unwrap_or([0., 0.]))?,
                handle_out: pair(segment.handle_out.unwrap_or([0., 0.]))?,
            })
        })
        .collect::<Result<Vec<_>, VectorEvalError>>()?;
    Ok(output)
}

/// Post-placement half of the ORIGINAL pathFX -> duplicator -> parametric ->
/// element Motion -> layer Position order. `record` is a detached copy whose
/// local render target names its host, never a rewritten document record.
pub(crate) fn duplicator_geometry(
    packet: &ObjectFramePacket,
    record: &ObjectRecord,
    layer_index: usize,
) -> Result<(Vec<EvalSegment>, bool), VectorEvalError> {
    let layer = packet
        .source_layers()
        .get(layer_index)
        .ok_or(VectorEvalError::MissingRecord)?;
    if record.target.context_id != packet.context_id()
        || record.target.layer_uid != layer.layer_uid()
    {
        return Err(VectorEvalError::MissingRecord);
    }
    let stored = stored_from_record(record, Some(layer))?;
    let anchors = stored.stroke.segments.as_deref().unwrap_or(&[]);
    let mut segments = match run_param_shape(packet.snapshot(), record, anchors, packet.frame())? {
        Some(rebuilt) => rebuilt.iter().map(from_param).collect(),
        None => anchors.iter().map(from_path).collect(),
    };
    let authored = packet
        .snapshot()
        .document()
        .property_holders()
        .is_some_and(|holders| {
            holders.iter().any(|holder| {
                holder.target().context_id() == record.target.context_id
                    && holder.target().layer_uid() == record.target.layer_uid
                    && holder.target().stroke_id() == Some(record.target.stroke_id.as_str())
            })
        });
    if authored {
        segments =
            motion::apply_element_motion(packet.snapshot(), record, packet.frame(), segments)?;
    }
    segments = motion::apply_duplicator_layer_position(packet, record, layer_index, segments)?;
    Ok((segments, record.geometry.closed))
}

/// Stage 2, `hasParamShapeMotionFor` + `applyParamShapeFor`: the rebuilt anchors, or `None` when the record has no
/// parametric shape or none of its own parameters is animated. `anchors` are the anchors the rebuild frames itself on.
pub(crate) fn run_param_shape(
    snapshot: &ObjectSnapshot,
    record: &ObjectRecord,
    anchors: &[PathSegment],
    frame: u32,
) -> Result<Option<Vec<ParamSegment>>, VectorEvalError> {
    let Some(shape) = params::shape_of(record)? else {
        return Ok(None);
    };
    let kind = shape.kind();
    let animated = params::animated_params(snapshot, record, kind, f64::from(frame))?;
    if !has_param_shape_motion(Some(kind), &animated) {
        return Ok(None);
    }
    let stored: Vec<ParamSegment> = anchors.iter().map(param_segment).collect();
    apply_param_shape_animated(&shape, &stored, &animated)
        .map(Some)
        .map_err(VectorEvalError::Rebuild)
}

fn param_segment(segment: &PathSegment) -> ParamSegment {
    ParamSegment {
        point: segment.point,
        handle_in: segment.handle_in.unwrap_or([0.0, 0.0]),
        handle_out: segment.handle_out.unwrap_or([0.0, 0.0]),
    }
}

pub(crate) fn from_path(segment: &PathSegment) -> EvalSegment {
    EvalSegment {
        point: segment.point,
        handle_in: segment.handle_in.unwrap_or([0.0, 0.0]),
        handle_out: segment.handle_out.unwrap_or([0.0, 0.0]),
    }
}

pub(crate) fn from_param(segment: &ParamSegment) -> EvalSegment {
    EvalSegment {
        point: segment.point,
        handle_in: segment.handle_in,
        handle_out: segment.handle_out,
    }
}

/// Exact pinned path-effect/rebuild geometry BEFORE layer pose, for native parent-pivot consumers.
/// Owned output cannot mutate the snapshot; callers still obtain true curve bounds in their geometry consumer.
pub(crate) fn evaluate_vector_rest(
    snapshot: &ObjectSnapshot,
    object_index: usize,
    frame: u32,
) -> Result<EvaluatedVector, VectorEvalError> {
    let stored = load_stored(snapshot, object_index)?;
    evaluate_stored_rest(snapshot, object_index, frame, stored)
}

/// The original pathFX -> parametric rebuild stages, shared without changing math.
fn evaluate_stored_rest(
    snapshot: &ObjectSnapshot,
    object_index: usize,
    frame: u32,
    stored: StoredVector<'_>,
) -> Result<EvaluatedVector, VectorEvalError> {
    let after_effects = run_path_fx(stored.stack.as_deref(), &stored.stroke, frame)?;
    let anchors = after_effects.segments.as_deref().unwrap_or(&[]);
    let segments = match run_param_shape(snapshot, stored.record, anchors, frame)? {
        Some(rebuilt) => rebuilt.iter().map(from_param).collect(),
        None => anchors.iter().map(from_path).collect(),
    };
    Ok(EvaluatedVector {
        source_object_index: object_index,
        closed: after_effects.closed,
        segments: motion::apply_element_motion(snapshot, stored.record, frame, segments)?,
    })
}

/// The same evaluated vector after its own raw layer Position, never a rest-bounds source.
pub(crate) fn evaluate_vector(
    snapshot: &ObjectSnapshot,
    object_index: usize,
    frame: u32,
) -> Result<EvaluatedVector, VectorEvalError> {
    let mut evaluated = evaluate_vector_rest(snapshot, object_index, frame)?;
    let record = snapshot
        .document()
        .objects()
        .get(object_index)
        .ok_or(VectorEvalError::MissingRecord)?;
    evaluated.segments = motion::apply_layer_position(snapshot, record, frame, evaluated.segments)?;
    Ok(evaluated)
}

/// The packet entry used by the scene producer: the packet's retained snapshot at the packet's evaluation frame.
/// Scene, preview and export all obtain their geometry through the packet's scene, so they obtain this value.
pub(crate) fn evaluate_record(
    packet: &ObjectFramePacket,
    record: ObjectFrameRecord<'_>,
) -> Result<EvaluatedVector, VectorEvalError> {
    let stored = load_packet_stored(packet, record)?;
    let mut evaluated = evaluate_stored_rest(
        packet.snapshot(),
        record.source_object_index(),
        packet.frame(),
        stored,
    )?;
    evaluated.segments = motion::apply_layer_position(
        packet.snapshot(),
        record.record(),
        packet.frame(),
        evaluated.segments,
    )?;
    Ok(evaluated)
}

/// Real scene producer entry for detached stored rig output. Other records use
/// the unchanged evaluator. Combined rig/path-effect/rebuild ordering has no
/// original consumer oracle yet; refuse it instead of inventing that order.
pub(crate) fn evaluate_record_with_pose(
    packet: &ObjectFramePacket,
    source: ObjectFrameRecord<'_>,
    pose: Option<&crate::evaluation::object_image_mesh_kernels::rig_pose_eval::PosedStroke>,
) -> Result<EvaluatedVector, VectorEvalError> {
    let Some(pose) = pose else {
        return evaluate_record(packet, source);
    };
    let snapshot = packet.snapshot();
    let stored = load_packet_stored(packet, source)?;
    let anchors = stored.stroke.segments.as_deref().unwrap_or(&[]);
    if pose.stroke_id != stored.record.target.stroke_id
        || pose.closed != stored.stroke.closed
        || pose.segments.len() != anchors.len()
    {
        return Err(VectorEvalError::MissingRecord);
    }
    if stored
        .record
        .vector_ext
        .value()
        .is_some_and(|ext| ext.pressure.value().is_some())
        || stored
            .stack
            .as_deref()
            .is_some_and(|stack| has_any(Some(stack)))
        || run_param_shape(snapshot, stored.record, anchors, packet.frame())?.is_some()
    {
        return Err(VectorEvalError::RigCompositionUnavailable);
    }
    let segments = pose
        .segments
        .iter()
        .map(|segment| EvalSegment {
            point: segment.point,
            handle_in: segment.handle_in,
            handle_out: segment.handle_out,
        })
        .collect();
    let segments = motion::apply_element_motion(snapshot, stored.record, packet.frame(), segments)?;
    Ok(EvaluatedVector {
        source_object_index: source.source_object_index(),
        closed: pose.closed,
        segments: motion::apply_layer_position(snapshot, stored.record, packet.frame(), segments)?,
    })
}

#[cfg(test)]
#[path = "../tests/render_vector_eval.rs"]
pub(crate) mod tests;
