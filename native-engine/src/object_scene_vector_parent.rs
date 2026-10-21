//! Actual static ordinary content ancestors at this packet's held source and evaluation frame.
//! No UI bounds, guest evaluation, new cache authority or synthetic parent pivot.
use crate::evaluation::motion_parent_chain::{
    apply_chain_to_segments, motion_mat_from_values, ChainEntry, Segment, CHAIN_NODE_LIMIT,
};
use crate::evaluation::object_image_mesh_kernels::rig_pose_eval::RigPoseEvaluation;
use crate::evaluation::object_vector_kernels::render_vector_eval::{
    common_layer_motion_target, evaluate_record_rest_with_pose, evaluate_vector_rest,
    is_vector_record, EvalSegment, EvaluatedVector,
};
use crate::object_frame_packet::{Presence, PropertyHolderTarget};
use crate::object_frame_packet::projection::layer_bounds;
use crate::object_frame_packet::ObjectFramePacket;
use crate::object_render_scene::object_scene_producer_contour::batch::legacy_record_rest;
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn chain(
    packet: &ObjectFramePacket,
    start: usize,
    poses: &BTreeMap<usize, Result<RigPoseEvaluation, &'static str>>,
) -> Result<Vec<ChainEntry>, &'static str> {
    let layer = packet
        .source_layers()
        .get(start)
        .ok_or("parent source layer is unavailable")?;
    let mut next = layer.parent_layer_uid.value().and_then(Option::as_deref);
    let mut seen = BTreeSet::from([start]);
    let mut chain = Vec::new();
    while let Some(uid) = next {
        if seen.len() > CHAIN_NODE_LIMIT {
            return Err("ordinary vector parent chain exceeds64 ancestors");
        }
        let index = packet
            .source_layers()
            .iter()
            .position(|l| l.layer_uid() == uid)
            .ok_or("ordinary vector parent target is unavailable")?;
        if !seen.insert(index) {
            return Err("ordinary vector parent chain is cyclic");
        }
        let target = PropertyHolderTarget {
            context_id: packet.context_id().to_owned(),
            layer_uid: uid.to_owned(),
            stroke_id: Presence::Missing,
        };
        let layer = &packet.source_layers()[index];
        if !common_layer_motion_target(packet.snapshot().document(), &target)
            || layer.three_d.value() == Some(&true)
        {
            return Err("ordinary vector parent has no complete native content consumer");
        }
        let values = packet
            .sampled_motion_values(&target)
            .map_err(|_| "pinned ordinary parent Motion values are unavailable")?;
        if values
            .position
            .iter()
            .chain(&values.anchor)
            .chain(&values.scale)
            .chain([&values.rotation, &values.opacity])
            .any(|v| !v.is_finite())
        {
            return Err("ordinary parent Motion must be finite");
        }
        let canvas = packet
            .snapshot()
            .project()
            .map(|p| p.project().canvas_dimensions());
        let null_base = layer
            .null_pivot_base(canvas)
            .map_err(|_| "Null parent needs nullPos or retained project canvas")?;
        if let Some(mat) = motion_mat_from_values(
            values.position,
            values.anchor,
            values.rotation,
            values.scale,
            values.opacity,
        ) {
            let base = match null_base {
                Some(base) => base,
                None => {
                    let vectors = rest(packet, index, poses)?;
                    let bounds = layer_bounds(&vectors)
                        .map_err(|_| "ordinary parent cubic bounds are unavailable")?
                        .ok_or("ordinary parent has no retained geometry pivot")?;
                    [bounds.x + bounds.width / 2., bounds.y + bounds.height / 2.]
                }
            };
            let pivot = [base[0] + mat.ax, base[1] + mat.ay];
            if !pivot.iter().all(|v| v.is_finite()) {
                return Err("ordinary parent pivot must be finite");
            }
            chain.push(ChainEntry { mat, pivot });
        }
        next = layer.parent_layer_uid.value().and_then(Option::as_deref);
    }
    Ok(chain)
}

fn rest(
    packet: &ObjectFramePacket,
    layer_index: usize,
    poses: &BTreeMap<usize, Result<RigPoseEvaluation, &'static str>>,
) -> Result<Vec<EvaluatedVector>, &'static str> {
    let layer = &packet.source_layers()[layer_index];
    let frame = packet
        .source_frame_for_layer(layer_index)
        .ok_or("ordinary parent held source is unavailable")?;
    let posed = match poses.get(&layer_index) {
        Some(Ok(pose)) => Some(pose),
        Some(Err(reason)) => return Err(*reason),
        None if layer.rig().is_some() => {
            return Err("hidden rig parent needs its actual stored pose consumer")
        }
        None => None,
    };
    let mut result = Vec::new();
    for (index, record) in packet
        .snapshot()
        .document()
        .objects()
        .iter()
        .enumerate()
        .filter(|(_, r)| {
            r.target.context_id == packet.context_id()
                && r.target.layer_uid == layer.layer_uid()
                && &r.target.frame_scope.kind == packet.scope_kind()
                && r.target.frame_scope.frame == frame
        })
    {
        let pose = posed.and_then(|p| {
            p.strokes
                .iter()
                .find(|s| s.stroke_id == record.target.stroke_id)
        });
        let vector = if is_vector_record(record) {
            match packet.records().find(|r| r.source_object_index() == index) {
                Some(source) => evaluate_record_rest_with_pose(packet, source, pose),
                None => evaluate_vector_rest(packet.snapshot(), index, packet.frame()),
            }
            .map_err(|e| e.message())?
        } else {
            legacy_record_rest(packet, index, record, pose)?
        };
        result.push(vector);
    }
    Ok(result)
}

pub(super) fn apply(
    vectors: &[EvaluatedVector],
    chain: &[ChainEntry],
) -> Result<Vec<EvaluatedVector>, &'static str> {
    if chain.is_empty() {
        return Ok(vectors.to_vec());
    }
    vectors
        .iter()
        .map(|vector| {
            let input = vector
                .segments
                .iter()
                .map(|s| Segment {
                    point: s.point,
                    handle_in: Some(s.handle_in),
                    handle_out: Some(s.handle_out),
                })
                .collect::<Vec<_>>();
            let result = EvaluatedVector {
                source_object_index: vector.source_object_index,
                closed: vector.closed,
                segments: apply_chain_to_segments(&input, chain)
                    .into_iter()
                    .map(|s| EvalSegment {
                        point: s.point,
                        handle_in: s.handle_in.unwrap_or([0., 0.]),
                        handle_out: s.handle_out.unwrap_or([0., 0.]),
                    })
                    .collect(),
            };
            // Reuse the same absolute-handle and finite cubic validation as the actual own-layer stage.
            layer_bounds(std::slice::from_ref(&result))
                .map_err(|_| "parent-transformed cubic coordinates are not finite")?;
            Ok(result)
        })
        .collect()
}
