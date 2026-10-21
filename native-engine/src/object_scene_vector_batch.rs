//! Actual selected ordinary-vector batch before the existing contour/style compiler.
//! Retained owner numeric values feed the actual whole-layer geometry and parent stages.
use crate::evaluation::object_image_mesh_kernels::rig_pose_eval::{PosedStroke, RigPoseEvaluation};
use crate::evaluation::object_vector_kernels::render_vector_eval::{
    evaluate_record_rest_with_pose, has_unjoined_parent_blend, is_vector_record,
    record_layer_has_active_path_fx, EvalSegment, EvaluatedVector,
};
use crate::object_document::{ObjectRecord, Presence, PropertyHolderTarget};
use crate::object_frame_packet::projection::{project_layer_3d, transform_layer_2d};
use crate::object_frame_packet::{MotionValues, ObjectFramePacket, ObjectFrameRecord};
use std::collections::BTreeMap;

#[path = "object_scene_vector_parent.rs"]
mod parent;

pub(super) struct PreparedVector {
    pub(super) geometry: EvaluatedVector,
    pub(super) layer_opacity: f64,
    refusal: Option<&'static str>,
}
#[derive(Default)]
pub(super) struct VectorBatches {
    vectors: BTreeMap<usize, PreparedVector>,
    errors: BTreeMap<usize, &'static str>,
}
impl VectorBatches {
    pub(super) fn check(&self, layer: usize) -> Result<(), &'static str> {
        match self.errors.get(&layer) {
            Some(reason) => Err(*reason),
            None => Ok(()),
        }
    }
    pub(super) fn get(
        &self,
        source: ObjectFrameRecord<'_>,
    ) -> Result<Option<&PreparedVector>, &'static str> {
        self.check(source.source_layer_index())?;
        let prepared = self.vectors.get(&source.source_object_index());
        match prepared.and_then(|p| p.refusal) {
            Some(reason) => Err(reason),
            None => Ok(prepared),
        }
    }
}

fn geometry_active(m: &MotionValues) -> bool {
    m.position != [0., 0.] || m.anchor != [0., 0.] || m.rotation != 0. || m.scale != [100., 100.]
}

pub(super) fn prepare(
    packet: &ObjectFramePacket,
    poses: &BTreeMap<usize, Result<RigPoseEvaluation, &'static str>>,
) -> VectorBatches {
    let mut layers: BTreeMap<usize, Vec<_>> = BTreeMap::new();
    for source in packet.records() {
        layers
            .entry(source.source_layer_index())
            .or_default()
            .push(source);
    }
    let mut result = VectorBatches::default();
    for (index, sources) in layers {
        match prepare_layer(packet, index, &sources, poses) {
            Ok(Some(vectors)) => {
                for vector in vectors {
                    result
                        .vectors
                        .insert(vector.geometry.source_object_index, vector);
                }
            }
            Ok(None) => {} // Existing duplicator/component path retains its old strict admission.
            Err(reason) => {
                result.errors.insert(index, reason);
            }
        }
    }
    result
}

fn prepare_layer(
    packet: &ObjectFramePacket,
    index: usize,
    sources: &[ObjectFrameRecord<'_>],
    poses: &BTreeMap<usize, Result<RigPoseEvaluation, &'static str>>,
) -> Result<Option<Vec<PreparedVector>>, &'static str> {
    if packet.context_id() != "scene-root" {
        return Ok(None);
    }
    let layer = packet
        .source_layers()
        .get(index)
        .ok_or("selected vector layer is unavailable")?;
    let target = PropertyHolderTarget {
        context_id: packet.context_id().to_owned(),
        layer_uid: layer.layer_uid().to_owned(),
        stroke_id: Presence::Missing,
    };
    if has_unjoined_parent_blend(packet.snapshot().document(), &target) {
        return Err("multiple-parent blend awaits its actual layer contribution consumer");
    }
    let three_d = layer.three_d.value() == Some(&true);
    // Geometry needs only retained Pose3D here. Frame capture owns the original preceding2D
    // Motion evaluation/effects, including opacity, before3D suppresses its paint factor.
    let motion = if three_d {
        None
    } else {
        Some(
            packet
                .sampled_motion_values(&target)
                .map_err(|_| "pinned ordinary vector Motion values are unavailable")?,
        )
    };
    let active = three_d || motion.as_ref().is_some_and(geometry_active);
    if packet
        .duplicator_outputs()
        .iter()
        .any(|copies| copies.layer_index == index)
    {
        if three_d
            || motion.as_ref().is_some_and(|m| {
                m.rotation != 0. || m.anchor != [0., 0.] || m.scale != [100., 100.]
            })
        {
            return Err("duplicator layer pose awaits its actual copy batch and paint consumer");
        }
        return Ok(None);
    }
    let parents = if three_d {
        Vec::new()
    } else {
        parent::chain(packet, index, poses)?
    };
    // Keep the original no-geometry-Motion route and its per-record refusal/order behavior.
    // New expanded/defaulted holder shapes still need their actual receipt-backed consumer.
    let expanded = packet
        .snapshot()
        .document()
        .property_holders()
        .is_some_and(|holders| {
            holders.iter().any(|h| {
                h.target() == &target
                    && h.properties().iter().any(|p| {
                        matches!(p.property(), "anchor" | "rotation" | "scale")
                            || (matches!(p.property(), "position" | "opacity")
                                && p.expression.value().is_some_and(|e| e.enabled))
                    })
            })
        });
    if !active && !expanded && parents.is_empty() {
        return Ok(None);
    }
    let mixed = sources.iter().any(|s| {
        (!is_vector_record(s.record()) && s.record().family != "closed-solid-fill-cubic-path")
            || s.record().is_mask()
    }) || packet.raster_records().any(|s| s.layer_index() == index)
        || packet.text_records().any(|s| s.layer_index() == index);
    if active && mixed {
        return Err(
            "layer Motion on mixed mask, Text or raster geometry awaits one complete batch",
        );
    }
    let mut vectors = Vec::new();
    let mut refusals = Vec::new();
    for source in sources.iter().copied().filter(|s| !s.record().is_mask()) {
        let record = source.record();
        // The actual paint-plan style still has only its original raw translation gradient port.
        let refusal = if record.fill_gradient().is_some()
            && (three_d
                || motion
                    .as_ref()
                    .is_some_and(|m| m.rotation != 0. || m.scale != [100., 100.])
                || packet
                    .snapshot()
                    .document()
                    .property_holders()
                    .is_some_and(|holders| {
                        holders.iter().any(|h| {
                            h.target() == &target
                                && h.properties().iter().any(|p| {
                                    p.property() == "position"
                                        && p.expression.value().is_some_and(|e| e.enabled)
                                })
                        })
                    })) {
            Some("transformed vector gradient awaits the same evaluated paint consumer")
        } else {
            None
        };
        let stroke = record.vector().is_some_and(|body| body.has_real_stroke());
        let refusal = refusal.or_else(|| {
            (stroke
                && motion
                    .as_ref()
                    .is_some_and(|m| (m.scale[0].abs() + m.scale[1].abs()) / 200. != 1.))
            .then_some("scaled vector stroke awaits its actual paint width and dash consumer")
        });
        let refusal = refusal.or_else(|| {
            if !parents.is_empty() && record.fill_gradient().is_some() {
                Some("parented vector gradient awaits its actual paint coordinates consumer")
            } else if stroke
                && parents
                    .iter()
                    .any(|p| (p.mat.sx.abs() + p.mat.sy.abs()) / 2. != 1.)
            {
                Some("scaled parent stroke awaits its actual paint width and dash consumer")
            } else {
                None
            }
        });
        refusals.push(refusal);
        let pose = match poses.get(&index) {
            Some(Ok(pose)) => pose
                .strokes
                .iter()
                .find(|s| s.stroke_id == record.target.stroke_id),
            Some(Err(reason)) => return Err(*reason),
            None => None,
        };
        vectors.push(if is_vector_record(record) {
            evaluate_record_rest_with_pose(packet, source, pose).map_err(|e| e.message())?
        } else {
            legacy_rest(packet, source, pose)?
        });
    }
    if vectors.is_empty() {
        return Ok(Some(Vec::new()));
    }
    let (vectors, layer_opacity) = if three_d {
        let pose = packet
            .sampled_pose_3d(&target)
            .map_err(|_| "pinned ordinary vector 3D values are unavailable")?
            .ok_or("stored threeD layer has no captured 3D pose")?;
        let (width, height) = packet
            .snapshot()
            .project()
            .ok_or("3D canvas metadata is unavailable")?
            .project()
            .canvas_dimensions();
        let extras = vec![None; vectors.len()];
        let vectors = project_layer_3d(
            &vectors,
            &pose,
            [f64::from(width), f64::from(height)],
            &extras,
        )
        .map_err(|_| "ordinary vector 3D batch projection refused its numeric inputs")?;
        // Original3D suppresses layer2D Motion, parent opacity and strokeScale.
        (vectors, 1.)
    } else {
        let motion = motion
            .as_ref()
            .ok_or("ordinary vector Motion receipt is unavailable")?;
        let vectors = transform_layer_2d(&vectors, motion)
            .map_err(|_| "ordinary vector Motion batch refused its numeric inputs")?;
        let vectors = parent::apply(&vectors, &parents)?;
        let opacity = parents
            .iter()
            .fold((motion.opacity / 100.).max(0.), |op, p| op * p.mat.op);
        if !opacity.is_finite() {
            return Err("parent opacity product is not finite");
        }
        (vectors, opacity)
    };
    Ok(Some(
        vectors
            .into_iter()
            .zip(refusals)
            .map(|(geometry, refusal)| PreparedVector {
                geometry,
                layer_opacity,
                refusal,
            })
            .collect(),
    ))
}

/// Original closed SolidFill source and rig-topology admission before the common layer stage.
pub(super) fn legacy_rest(
    packet: &ObjectFramePacket,
    source: ObjectFrameRecord<'_>,
    pose: Option<&PosedStroke>,
) -> Result<EvaluatedVector, &'static str> {
    legacy_record_rest(packet, source.source_object_index(), source.record(), pose)
}

fn legacy_record_rest(
    packet: &ObjectFramePacket,
    source_index: usize,
    record: &ObjectRecord,
    pose: Option<&PosedStroke>,
) -> Result<EvaluatedVector, &'static str> {
    if record_layer_has_active_path_fx(packet, record) {
        return Err("layer path effects on a legacy record await the vector join");
    }
    // Field access is confined to the original record returned by the admitted
    // evaluation port. No document lookup or second source authority exists.
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
            Ok(EvalSegment {
                point: [number(&segment.point.x)?, number(&segment.point.y)?],
                handle_in: [number(&segment.handle_in.x)?, number(&segment.handle_in.y)?],
                handle_out: [
                    number(&segment.handle_out.x)?,
                    number(&segment.handle_out.y)?,
                ],
            })
        })
        .collect::<Result<Vec<_>, &'static str>>()?;
    if pose.is_some_and(|pose| {
        pose.stroke_id != record.target.stroke_id
            || pose.closed != geometry.closed
            || pose.segments.len() != geometry.segments.len()
    }) {
        return Err("rig pose topology differs from the selected stored path");
    }
    let segments = match pose {
        Some(pose) => pose
            .segments
            .iter()
            .map(|segment| EvalSegment {
                point: segment.point,
                handle_in: segment.handle_in,
                handle_out: segment.handle_out,
            })
            .collect(),
        None => segments,
    };
    Ok(EvaluatedVector {
        source_object_index: source_index,
        closed: true,
        segments,
    })
}
