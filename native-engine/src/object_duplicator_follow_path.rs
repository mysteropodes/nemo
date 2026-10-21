//! Snapshot caller for the original layerMotionAt FollowPath contribution.
//! D30 owns path sampling, target-transform and additive placement arithmetic.
//! The effector's own parents are not inputs; unsupported guide parents refuse.
use crate::evaluation::motion_parent_chain::MotionMat;
use crate::evaluation::object_component_kernels::object_context_resolver::ContextSource;
use crate::evaluation::object_component_kernels::object_duplicator_evaluate::snapshot::SnapshotMotion;
use crate::evaluation::object_component_kernels::object_effector_layers::{
    MotionPort, Unavailable,
};
use crate::evaluation::property_follow_path_sample::{follow_path_geometry, SegmentIn, StrokeIn};
use crate::evaluation::property_transform_eval::{
    apply_follow_path, follow_path_offset, follow_path_sample,
};
use crate::object_document::object_group_kernels::object_boolean_ops::RingSeg;
use crate::object_document::object_group_kernels::object_fill_trace_ops::native::path;
use crate::object_document::{FrameScopeKind, ObjectRecord, Presence, PropertyHolderTarget};
use crate::object_frame_packet::{duplicator_sources::SnapshotSources, ObjectFrameSelector};
use crate::revision::ObjectSnapshot;
use kurbo::Shape;

fn segments(record: &ObjectRecord) -> Result<Vec<SegmentIn>, Unavailable> {
    let pair = |p: &crate::object_document::ObjectPoint| -> Result<[f64; 2], Unavailable> {
        Ok([
            p.x.as_f64().ok_or(Unavailable::LayerMotion)?,
            p.y.as_f64().ok_or(Unavailable::LayerMotion)?,
        ])
    };
    record
        .geometry
        .segments
        .iter()
        .map(|s| {
            Ok(SegmentIn {
                point: pair(&s.point)?,
                handle_in: Some(pair(&s.handle_in)?),
                handle_out: Some(pair(&s.handle_out)?),
            })
        })
        .collect()
}

fn centre(records: &[ObjectRecord]) -> Result<[f64; 2], Unavailable> {
    let mut bounds: Option<kurbo::Rect> = None;
    for record in records {
        // A rig/shape/style-specific pivot needs its actual geometry consumer.
        if record.vector_ext.value().is_some() || record.has_enabled_effects() {
            return Err(Unavailable::LayerMotion);
        }
        let ring = segments(record)?
            .into_iter()
            .map(|s| RingSeg {
                point: s.point,
                handle_in: s.handle_in.unwrap(),
                handle_out: s.handle_out.unwrap(),
            })
            .collect::<Vec<_>>();
        let rect = path(&ring, record.geometry.closed).bounding_box();
        bounds = Some(bounds.map_or(rect, |old| old.union(rect)));
    }
    // Every admitted layer has a Paper group in the baseline; an empty group
    // (in particular a non-content effector layer) has Rect(0,0,0,0) bounds.
    let point = bounds.unwrap_or(kurbo::Rect::ZERO).center();
    Ok([point.x, point.y])
}

pub(super) fn compose(
    snapshot: &ObjectSnapshot,
    context_id: &str,
    frame: f64,
    layer_uid: &str,
    local: Option<MotionMat>,
) -> Result<Option<MotionMat>, Unavailable> {
    if !frame.is_finite() || frame < 0. || frame > f64::from(u32::MAX) {
        return Err(Unavailable::LayerMotion);
    }
    let context = ContextSource::of_document(snapshot.document())
        .context(context_id)
        .map_err(|_| Unavailable::LayerMotion)?;
    let (_, layer) = context.layer(layer_uid).ok_or(Unavailable::LayerMotion)?;
    let Some(fp) = layer.follow_path.value().and_then(Option::as_ref) else {
        return Ok(local);
    };
    if fp.target_layer_uid().is_empty() {
        return Ok(local);
    }
    let Some((_, target_layer)) = context.layer(fp.target_layer_uid()) else {
        return Ok(local); // Original unresolved guide has no path contribution.
    };
    if context_id != "scene-root" {
        return Err(Unavailable::LayerMotion);
    }
    let selector = ObjectFrameSelector {
        instance_id: snapshot.instance_id(),
        document_id: snapshot.document_id(),
        content_revision: snapshot.content_revision(),
        document_snapshot_id: snapshot.snapshot_id(),
        context_id,
        scope_kind: FrameScopeKind::Authored,
        frame: frame as u32,
    };
    let sources =
        SnapshotSources::new(snapshot, &selector, true).map_err(|_| Unavailable::LayerMotion)?;
    let records = sources
        .read(fp.target_layer_uid(), frame)
        .map_err(|_| Unavailable::LayerMotion)?;
    let owned = records
        .iter()
        .map(segments)
        .collect::<Result<Vec<_>, _>>()?;
    let strokes = owned
        .iter()
        .zip(&records)
        .map(|(segments, record)| {
            Some(StrokeIn {
                segments: Some(segments),
                closed: record.geometry.closed,
            })
        })
        .collect::<Vec<_>>();
    let motion = SnapshotMotion::new(snapshot, context_id, frame);
    let scalar = |key| -> Result<f64, Unavailable> {
        let values = motion
            .value(layer_uid, key)
            .map_err(|_| Unavailable::LayerMotion)?
            .ok_or(Unavailable::LayerMotion)?;
        match values.as_slice() {
            [value] => Ok(value.operand()),
            _ => Err(Unavailable::LayerMotion),
        }
    };
    let mut failed = false;
    let sampled = follow_path_geometry(&strokes, |len| match scalar("pathPercent") {
        Ok(percent) => follow_path_offset(percent, len).unwrap_or(0.),
        Err(_) => {
            failed = true;
            0.
        }
    });
    if failed {
        return Err(Unavailable::LayerMotion);
    }
    let Some(sampled) = sampled else {
        return Ok(local);
    };
    // The existing compositor parent provider serves Text/raster paints, not
    // this snapshot lookup. Never substitute identity for an unconsumed guide
    // parent, rig or path-effect transform. No new parent architecture here.
    if target_layer
        .parent_layer_uid
        .value()
        .is_some_and(Option::is_some)
        || target_layer.rig().is_some()
        || target_layer.groups.value().is_some()
        || target_layer.path_fx().is_some_and(|fx| !fx.is_empty())
        || target_layer.three_d.value() == Some(&true)
    {
        return Err(Unavailable::LayerMotion);
    }
    let target = PropertyHolderTarget {
        context_id: context_id.to_owned(),
        layer_uid: fp.target_layer_uid().to_owned(),
        stroke_id: Presence::Missing,
    };
    // Deliberately PLAIN computeMotionMat: target FollowPath is never followed,
    // matching motion.js:3504 and preventing recursive FollowPath traversal.
    let target_mat = snapshot
        .sampled_motion_2d(&target, frame)
        .map_err(|_| Unavailable::LayerMotion)?;
    let target_pose = target_mat
        .map(|mat| centre(&records).map(|centre| (mat, centre)))
        .transpose()?;
    let point = follow_path_sample(sampled.point, sampled.tangent, target_pose, &[]);
    let own_records = sources
        .read(layer_uid, frame)
        .map_err(|_| Unavailable::LayerMotion)?;
    let own_centre = centre(&own_records)?;
    let influence = if fp.align() {
        scalar("pathInfluence")?
    } else {
        0.
    };
    let result = apply_follow_path(local, Some(point), Some(own_centre), fp.align(), influence);
    if result.is_some_and(|mat| !mat.dx.is_finite() || !mat.dy.is_finite()) {
        return Err(Unavailable::LayerMotion);
    }
    Ok(result)
}

#[cfg(all(test, feature = "history"))]
#[path = "../tests/object_duplicator_follow_path.rs"]
mod tests;
