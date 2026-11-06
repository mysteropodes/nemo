//! #2520: the clones of a duplicator host under the 3D switch. The baseline projects every clone VERTEX through the layer's
//! `make3DProjector` around the union bounds of all its clones, and a clone that carries a `dup3D` delta (positionZ, rotationX,
//! rotationY offsets) gets its OWN projector (`dup3DProjectorCache`, one per `dupIndex`).
use crate::evaluation::property_projection_3d::{
    make_projector, project_segments, Bounds, ExtraDelta, Projector3D, Seg,
};
use crate::object_frame_packet::{ObjectFramePacket, PropertyHolderTarget, Presence};
use crate::object_render_scene::text_scene::parent_chain::curve::curve_path;
use crate::render_geometry::CubicSegment;
use vello::kurbo::{Rect, Shape};

/// One projector per copy of the host layer, in copy order; `None` for a layer whose 3D switch is off.
pub(super) fn projectors(
    packet: &ObjectFramePacket,
    layer_index: usize,
    copies: &[Vec<CubicSegment>],
    closed: &[bool],
    deltas: &[Option<ExtraDelta>],
) -> Result<Option<Vec<Projector3D>>, &'static str> {
    let layer = &packet.source_layers()[layer_index];
    if layer.three_d.value() != Some(&true) {
        return Ok(None);
    }
    let target = PropertyHolderTarget {
        context_id: packet.context_id().to_owned(),
        layer_uid: layer.layer_uid().to_owned(),
        stroke_id: Presence::Missing,
    };
    let pose = packet
        .sampled_pose_3d(&target)
        .ok()
        .flatten()
        .ok_or("duplicator 3D pose is unavailable")?;
    let (width, height) = packet
        .snapshot()
        .project()
        .map(|p| p.project().canvas_dimensions())
        .ok_or("3D canvas metadata is unavailable")?;
    let mut union: Option<Rect> = None;
    for (segments, closed) in copies.iter().zip(closed) {
        let flat: Vec<_> = segments.iter().map(|s| (s.point, s.handle_in, s.handle_out)).collect();
        let rect = curve_path(&flat, *closed).map_err(|e| e.reason())?.bounding_box();
        union = Some(union.map_or(rect, |old| old.union(rect)));
    }
    let Some(union) = union else { return Ok(None) };
    let bounds = Bounds { x: union.x0, y: union.y0, width: union.width(), height: union.height() };
    Ok(Some(
        deltas
            .iter()
            .map(|delta| make_projector(&pose, &bounds, f64::from(width), f64::from(height), delta.as_ref()))
            .collect(),
    ))
}

/// `project3DSegments` over one clone.
pub(super) fn project(segments: &[CubicSegment], projector: &Projector3D) -> Vec<CubicSegment> {
    let flat: Vec<Seg> = segments
        .iter()
        .map(|s| Seg { point: s.point, handle_in: Some(s.handle_in), handle_out: Some(s.handle_out) })
        .collect();
    project_segments(&flat, projector)
        .into_iter()
        .map(|s| CubicSegment {
            point: s.point,
            handle_in: s.handle_in.unwrap_or([0.0, 0.0]),
            handle_out: s.handle_out.unwrap_or([0.0, 0.0]),
        })
        .collect()
}
