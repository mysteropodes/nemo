//! Canonical-to-rendered affine from the same native rest bounds and parent chain as paint.
use super::*;
use crate::evaluation::motion_parent_chain::ChainEntry;
use crate::object_frame_packet::projection::layer_chain_entry;
use vello::kurbo::{Affine, BezPath, Shape};

fn affine(entry: &ChainEntry) -> Affine {
    let p = entry.pivot;
    let m = entry.mat;
    Affine::translate((p[0] + m.dx, p[1] + m.dy))
        * Affine::rotate(m.rot.to_radians())
        * Affine::scale_non_uniform(m.sx, m.sy)
        * Affine::translate((-p[0], -p[1]))
}
pub(crate) fn matrices(
    packet: &ObjectFramePacket,
    index: usize,
) -> Result<BTreeMap<usize, [f64; 6]>, &'static str> {
    matrices_with_preview(packet, index, None)
}
pub(crate) fn matrices_with_preview(
    packet: &ObjectFramePacket,
    index: usize,
    preview: Option<&ObjectRecord>,
) -> Result<BTreeMap<usize, [f64; 6]>, &'static str> {
    let layer = &packet.source_layers()[index];
    if layer.three_d.value() == Some(&true)
        || layer.rig().is_some()
        || packet
            .duplicator_outputs()
            .iter()
            .any(|d| d.layer_index == index)
    {
        return Err("Subselect requires an ordinary undeformed 2D Drawing layer");
    }
    let sources: Vec<_> = packet
        .records()
        .filter(|s| s.source_layer_index() == index)
        .collect();
    let poses = BTreeMap::new();
    // Reuse the paint consumer's admission, including mixed geometry, parent blends and style limitations.
    if let Some(prepared) = prepare_layer(packet, index, &sources, &poses)? {
        if prepared.iter().any(|v| v.refusal.is_some()) {
            return Err("Subselect layer paint consumer is unavailable");
        }
    }
    if packet.raster_records().any(|s| s.layer_index() == index)
        || packet.text_records().any(|s| s.layer_index() == index)
    {
        return Err("Subselect requires a complete native vector layer");
    }
    let mut rest = Vec::new();
    for source in &sources {
        crate::object_codec::subselect::record::admit(
            packet.snapshot().document(),
            source.record(),
        )
        .map_err(|_| "Subselect geometry consumer is unavailable")?;
        if source.record().is_mask() || record_layer_has_active_path_fx(packet, source.record()) {
            return Err("Subselect mask or path effect is unavailable");
        }
        rest.push(if let Some(edited) = preview.filter(|r| r.target == source.record().target) {
            let points=crate::object_codec::subselect::record::anchors(edited).map_err(|_| "Subselect preview geometry is unavailable")?;
            let segments=points.into_iter().map(|a| EvalSegment {point:a.point,handle_in:a.handle_in,handle_out:a.handle_out}).collect();
            let segments=crate::evaluation::object_vector_kernels::render_vector_eval::motion::apply_element_motion(packet.snapshot(),edited,packet.frame(),segments).map_err(|e|e.message())?;
            EvaluatedVector {source_object_index:source.source_object_index(),closed:edited.geometry.closed,segments}
        } else if is_vector_record(source.record()) {
            evaluate_record_rest_with_pose(packet, *source, None).map_err(|e| e.message())?
        } else {
            legacy_rest(packet, *source, None)?
        });
    }
    let target = PropertyHolderTarget {
        context_id: packet.context_id().into(),
        layer_uid: layer.layer_uid().into(),
        stroke_id: Presence::Missing,
    };
    let motion = packet
        .sampled_motion_values(&target)
        .map_err(|_| "Subselect Motion receipt is unavailable")?;
    let mut outer = Affine::IDENTITY;
    if let Some(entry) =
        layer_chain_entry(&rest, &motion).map_err(|_| "Subselect layer bounds are unavailable")?
    {
        outer = affine(&entry);
    }
    let parents = parent::chain(packet, index, &poses)?;
    // Admit the complete candidate through paint's own finite-coordinate checks,
    // not merely its affine coefficients (finite products can still overflow).
    let projected = transform_layer_2d(&rest, &motion)
        .map_err(|_| "Subselect preview layer projection is unavailable")?;
    parent::apply(&projected, &parents)?;
    for entry in parents {
        outer = affine(&entry) * outer;
    }
    let mut result = BTreeMap::new();
    for source in sources {
        let record = preview
            .filter(|r| r.target == source.record().target)
            .unwrap_or_else(|| source.record());
        let target = PropertyHolderTarget {
            context_id: record.target.context_id.clone(),
            layer_uid: record.target.layer_uid.clone(),
            stroke_id: Presence::Present(record.target.stroke_id.clone()),
        };
        let mut matrix = outer;
        if let Some(mat) = packet
            .snapshot()
            .sampled_motion_2d(&target, f64::from(packet.frame()))
            .map_err(|_| "Subselect element Motion is unavailable")?
        {
            // The canonical cubic bounds are the element consumer's pivot before its own Motion.
            let points = crate::object_codec::subselect::record::anchors(record)
                .map_err(|_| "Subselect geometry consumer is unavailable")?;
            let mut path = BezPath::new();
            if let Some(first) = points.first() {
                path.move_to((first.point[0], first.point[1]));
            }
            for i in 1..points.len() + usize::from(record.geometry.closed) {
                let a = &points[i - 1];
                let b = &points[i % points.len()];
                path.curve_to(
                    (a.point[0] + a.handle_out[0], a.point[1] + a.handle_out[1]),
                    (b.point[0] + b.handle_in[0], b.point[1] + b.handle_in[1]),
                    (b.point[0], b.point[1]),
                );
            }
            let c = path.bounding_box().center();
            matrix = outer
                * affine(&ChainEntry {
                    mat,
                    pivot: [c.x + mat.ax, c.y + mat.ay],
                });
        }
        let values = matrix.as_coeffs();
        let det = values[0] * values[3] - values[1] * values[2];
        if values.iter().any(|v| !v.is_finite()) || !det.is_finite() || det.abs() < 1e-12 {
            return Err("Subselect transform is singular");
        }
        result.insert(source.source_object_index(), values);
    }
    Ok(result)
}

#[cfg(test)]
#[path = "../tests/object_subselect_projection.rs"]
mod tests;
