//! #2255: where the layer of a rig sits in the world, so the Rig tool works on a layer with Motion (R-rig-motion-layers).
//!
//! The baseline maps every gesture through the layer's evaluated Motion (`rig-bridge.js` `toLocalPoint` -> `layerMotionPointMap`) and
//! forward-maps the overlay the same way, whatever the layer carries. The bones, handles and posed outlines of the rig view stay in the
//! layer's own space (the space `moveSegment` writes); this module answers the ONE affine `[a, b, c, d, e, f]`
//! (`x' = a x + c y + e`, `y' = b x + d y + f`) that takes that space to the world: the layer's own Motion about its pivot (the centre of
//! its content bounds, POSED when a rig drives it, plus the anchor), then every ancestor's (`object_parent_pivot_read::ancestor_affine`).
//! `None` is the identity. The caller inverts it for gestures.
//!
//! The pivot of a rigged layer is the centre of the tight cubic bounds of its vector records at the exposed frame, the bound ones POSED
//! by the (candidate) rig: the very geometry the renderer's common layer stage pivots about (`render_vector_eval_pose`), so the
//! overlay and the picture cannot disagree. A layer of rasters only pivots about their display rectangles (a mesh pose never moves the
//! rect). A layer that mixes the two, a 3D layer, a legacy contour or an unreadable Motion is a typed reason, never a guessed map.
use crate::object_snapshot::object_rig_pose_read::VertexWrite;
use crate::revision::ObjectSnapshot;

/// `Ok(None)`: the layer and every ancestor carry no Motion at `frame`; `Err(reason)`: the map is not available.
#[cfg_attr(not(feature = "evaluation"), allow(unused_variables))]
pub(super) fn layer_matrix(
    snapshot: &ObjectSnapshot,
    layer_uid: &str,
    frame: u32,
    writes: &[VertexWrite],
) -> Result<Option<[f64; 6]>, &'static str> {
    #[cfg(feature = "evaluation")]
    {
        matrix(snapshot, layer_uid, frame, writes)
    }
    #[cfg(not(feature = "evaluation"))]
    {
        Err("sample")
    }
}

#[cfg(feature = "evaluation")]
fn matrix(
    snapshot: &ObjectSnapshot,
    layer_uid: &str,
    frame: u32,
    writes: &[VertexWrite],
) -> Result<Option<[f64; 6]>, &'static str> {
    use crate::object_snapshot::object_parent_pivot_read::ancestor_affine;
    use crate::object_snapshot::widget_presentation::kernels::layer_mat;
    const IDENTITY: [f64; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
    let layer = snapshot
        .document()
        .layers()
        .iter()
        .find(|layer| layer.layer_uid() == layer_uid)
        .ok_or("parent")?;
    if layer.three_d.value() == Some(&true) {
        return Err("three_d");
    }
    let own = layer_mat(snapshot, layer_uid, f64::from(frame))?;
    let ancestors = ancestor_affine(snapshot, "scene-root", layer_uid, frame)?;
    let Some(own) = own else {
        return Ok((ancestors != IDENTITY).then_some(ancestors));
    };
    // A pure translation turns about nothing, so a layer without content bounds still has its map.
    let turns = own.rot != 0.0 || own.sx != 1.0 || own.sy != 1.0 || own.ax != 0.0 || own.ay != 0.0;
    let base = if turns { pivot_base(snapshot, layer_uid, frame, writes)? } else { [0.0, 0.0] };
    let (px, py) = (base[0] + own.ax, base[1] + own.ay);
    let (s, c) = own.rot.to_radians().sin_cos();
    let (a, b, cc, d) = (c * own.sx, s * own.sx, -s * own.sy, c * own.sy);
    let moved = [a, b, cc, d, px - (a * px + cc * py) + own.dx, py - (b * px + d * py) + own.dy];
    let world = compose(&ancestors, &moved);
    world.iter().all(|v| v.is_finite()).then_some(world).map(Some).ok_or("sample")
}

/// `outer` after `inner`: `p -> outer(inner(p))`.
#[cfg(feature = "evaluation")]
fn compose(outer: &[f64; 6], inner: &[f64; 6]) -> [f64; 6] {
    [
        outer[0] * inner[0] + outer[2] * inner[1],
        outer[1] * inner[0] + outer[3] * inner[1],
        outer[0] * inner[2] + outer[2] * inner[3],
        outer[1] * inner[2] + outer[3] * inner[3],
        outer[0] * inner[4] + outer[2] * inner[5] + outer[4],
        outer[1] * inner[4] + outer[3] * inner[5] + outer[5],
    ]
}

/// The centre of the content bounds of the layer at its exposed frame, the bound strokes posed by `candidate` (or the stored rig).
#[cfg(feature = "evaluation")]
fn pivot_base(
    snapshot: &ObjectSnapshot,
    layer_uid: &str,
    frame: u32,
    writes: &[VertexWrite],
) -> Result<[f64; 2], &'static str> {
    use crate::evaluation::object_image_mesh_kernels::rig_pose_eval::{evaluate_for_view, RigVertexWrite};
    use crate::evaluation::object_vector_kernels::render_vector_eval::{
        evaluate_record_rest_with_pose, evaluate_vector_rest, is_vector_record,
    };
    use crate::object_frame_packet::projection::layer_bounds;
    use crate::object_frame_packet::{prepare_object_frame, FrameScopeKind, ObjectFrameSelector};
    let selector = ObjectFrameSelector {
        instance_id: snapshot.instance_id(),
        document_id: snapshot.document_id(),
        content_revision: snapshot.content_revision(),
        document_snapshot_id: snapshot.snapshot_id(),
        context_id: "scene-root",
        scope_kind: FrameScopeKind::Authored,
        frame,
    };
    let packet = prepare_object_frame(snapshot, &selector).map_err(|_| "frame")?;
    let index = packet.source_layers().iter().position(|l| l.layer_uid() == layer_uid).ok_or("parent")?;
    let held = packet.source_frame_for_layer(index).ok_or("frame")?;
    let rasters = packet.raster_records().any(|r| r.layer_index() == index);
    let posed = if packet.source_layers()[index].rig().is_some() {
        let writes: Vec<RigVertexWrite> = writes
            .iter()
            .map(|w| RigVertexWrite { bone_id: w.bone_id.clone(), vi: w.vi, point: w.point, handle_in: w.handle_in, handle_out: w.handle_out })
            .collect();
        Some(evaluate_for_view(snapshot, layer_uid, frame, &writes).map_err(|_| "pose")?.evaluation)
    } else {
        None
    };
    let mut vectors = Vec::new();
    for (at, record) in snapshot.document().objects().iter().enumerate().filter(|(_, r)| {
        r.target.context_id == "scene-root"
            && r.target.layer_uid == layer_uid
            && r.target.frame_scope.kind == FrameScopeKind::Authored
            && r.target.frame_scope.frame == held
    }) {
        if !is_vector_record(record) {
            return Err("contour");
        }
        let pose = posed.as_ref().and_then(|p| p.strokes.iter().find(|s| s.stroke_id == record.target.stroke_id));
        let vector = match packet.records().find(|r| r.source_object_index() == at) {
            Some(source) => evaluate_record_rest_with_pose(&packet, source, pose),
            None => evaluate_vector_rest(snapshot, at, packet.frame()),
        }
        .map_err(|_| "contour")?;
        vectors.push(vector);
    }
    if rasters && !vectors.is_empty() {
        return Err("mixed");
    }
    let bounds = if vectors.is_empty() {
        let r = crate::object_snapshot::object_parent_pivot_read::raster_rect(snapshot, "scene-root", layer_uid, frame).ok_or("empty")?;
        [(r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0]
    } else {
        let b = layer_bounds(&vectors).map_err(|_| "sample")?.ok_or("empty")?;
        [b.x + b.width / 2.0, b.y + b.height / 2.0]
    };
    bounds.iter().all(|v| v.is_finite()).then_some(bounds).ok_or("sample")
}
