//! Actual selected ordinary-vector batch before the existing contour/style compiler.
//! Retained owner numeric values feed the actual whole-layer geometry and parent stages.
use crate::evaluation::object_image_mesh_kernels::rig_pose_eval::{PosedStroke, RigPoseEvaluation};
use crate::evaluation::object_vector_kernels::render_vector_eval::{
    common_layer_motion_target, evaluate_record_rest_with_pose, has_unjoined_parent_blend,
    is_vector_record, record_layer_has_active_path_fx, EvalSegment, EvaluatedVector,
};
use crate::evaluation::object_scene_style_kernels::object_scene_paint_pose::PaintPose;
use crate::object_frame_packet::projection::{
    apply_layer_entry, layer_chain_entry, project_layer_3d,
};
use crate::object_render_scene::text_scene::parent_chain::SceneParentChains;
use crate::object_frame_packet::{MotionValues, ObjectFramePacket, ObjectFrameRecord};
use crate::object_frame_packet::{ObjectRecord, Presence, PropertyHolderTarget};
use std::collections::BTreeMap;

#[path = "object_scene_vector_parent.rs"]
mod parent;

pub(super) struct PreparedVector {
    pub(super) geometry: EvaluatedVector,
    pub(super) layer_opacity: f64,
    pub(super) paint: PaintPose,
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
    chains: Option<&SceneParentChains>,
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
        match prepare_layer(packet, index, &sources, poses, chains) {
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
    chains: Option<&SceneParentChains>,
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
    // Vectors that share a layer with Text or raster items take the pose of that union: the same chain those items use.
    let item_consumers = packet.raster_records().any(|s| s.layer_index() == index)
        || packet.text_records().any(|s| s.layer_index() == index);
    let record_mixed = sources.iter().any(|s| {
        (!is_vector_record(s.record()) && s.record().family != "closed-solid-fill-cubic-path")
            || s.record().is_mask()
    });
    let item_chain = match chains {
        Some(chains) if item_consumers && !record_mixed && !three_d => Some(
            chains
                .chain(index)
                .map_err(|failure| failure.reason())?,
        ),
        _ => None,
    };
    let own_in_chain = motion.as_ref().is_some_and(geometry_active);
    let parents = match &item_chain {
        Some(chain) => chain[usize::from(own_in_chain).min(chain.len())..].to_vec(),
        None if three_d => Vec::new(),
        None => parent::chain(packet, index, poses)?,
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
    // Identity Position still belongs to the admitted common consumer. Falling
    // back would ask the older vector-only path to validate unrelated authored
    // keys (including a legacy fill on a later frame of the same split piece).
    let common_position = (item_chain.is_some()
        || common_layer_motion_target(packet.snapshot().document(), &target))
        && packet
            .snapshot()
            .document()
            .property_holders()
            .is_some_and(|holders| {
                holders.iter().any(|h| {
                    h.target() == &target
                        && h.properties().iter().any(|p| p.property() == "position")
                })
            });
    if !active && !expanded && parents.is_empty() && !common_position {
        return Ok(None);
    }
    let mixed = (record_mixed || item_consumers) && item_chain.is_none();
    // The original per-record producers already consume raw Position, including
    // mask curves. A union-bounds transform is unnecessary for translation; keep
    // that admitted route unless an expanded holder or parent needs this batch.
    if mixed && !three_d && !expanded && parents.is_empty() {
        return Ok(None);
    }
    if (active || !parents.is_empty()) && mixed {
        return Err(
            "layer Motion on mixed mask, Text or raster geometry awaits one complete batch",
        );
    }
    let mut vectors = Vec::new();
    let mut refusals = Vec::new();
    for source in sources.iter().copied().filter(|s| !s.record().is_mask()) {
        let record = source.record();
        // The paint style carries the layer pose (`PaintPose`): stroke width and gradient endpoints follow the same chain
        // as the path. What stays unjoined is a gradient under 3D, a gradient whose Position is an expression (the raw
        // translation port ignores it) and a dash pattern under a scaled stroke (the baseline inline route leaves the
        // dash unscaled while its retained route scales it with the pen).
        let refusal = if record.fill_gradient().is_some()
            && (three_d
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
        let dashed = record
            .vector()
            .and_then(|body| body.dash_array.value())
            .is_some_and(|dash| !dash.is_empty());
        let stroke = record.vector().is_some_and(|body| body.has_real_stroke());
        let scaled_stroke_scale = !three_d && stroke && dashed && {
            let own = motion
                .as_ref()
                .map_or(1., |m| (m.scale[0].abs() + m.scale[1].abs()) / 200.);
            own != 1.
                || parents
                    .iter()
                    .any(|p| (p.mat.sx.abs() + p.mat.sy.abs()) / 2. != 1.)
        };
        let refusal = refusal.or_else(|| {
            scaled_stroke_scale
                .then_some("scaled dashed vector stroke awaits its actual paint dash consumer")
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
    let (vectors, layer_opacity, paint) = if three_d {
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
        (vectors, 1., PaintPose::default())
    } else {
        let motion = motion
            .as_ref()
            .ok_or("ordinary vector Motion receipt is unavailable")?;
        let (entry, vectors) = match &item_chain {
            Some(chain) => {
                // The union pivot is already inside the shared chain; its first entry is this layer's own.
                let own = own_in_chain.then(|| chain[0]);
                (own, parent::apply(&vectors, chain)?)
            }
            None => {
                let entry = layer_chain_entry(&vectors, motion)
                    .map_err(|_| "ordinary vector Motion batch refused its numeric inputs")?;
                let vectors = apply_layer_entry(&vectors, entry)
                    .map_err(|_| "ordinary vector Motion batch refused its numeric inputs")?;
                (entry, parent::apply(&vectors, &parents)?)
            }
        };
        let opacity = parents
            .iter()
            .fold((motion.opacity / 100.).max(0.), |op, p| op * p.mat.op);
        if !opacity.is_finite() {
            return Err("parent opacity product is not finite");
        }
        (vectors, opacity, PaintPose::from_chain(entry, &parents))
    };
    Ok(Some(
        vectors
            .into_iter()
            .zip(refusals)
            .map(|(geometry, refusal)| PreparedVector {
                geometry,
                layer_opacity,
                paint: paint.clone(),
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

#[path = "object_scene_subselect_projection.rs"]
pub(crate) mod subselect;
