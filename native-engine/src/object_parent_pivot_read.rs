//! Read-queries facade for the evaluated parent pivot bases (#2372, Parent A keeping the world pose); no state, no
//! publication authority.
//!
//! The command layer may not depend on the evaluation layer (`commands` allows `read-queries`, `read-queries` allows
//! `evaluation`), so the Parent A planner reaches the evaluated pivot of a layer through the single function below,
//! exactly like the rig pose facade reaches its kernel. The pivot BASE of a layer is the point its Motion pivots
//! around before the anchor is added: the Null's own rest point, or the centre of the tight cubic bounds of every
//! stroke the layer holds at its held source frame, evaluated by the same rest kernels the renderer's parent chain
//! uses (`object_scene_vector_parent.rs`). The answer is plain numbers; a layer the renderer's common batch would
//! not consume as ordinary content (text, raster, mask, effect, 3D, rig, legacy contour, a document with an enabled
//! expression) is a typed refusal, never a guessed pivot. Without the `evaluation` feature there is no kernel and
//! every call reports that the evaluation is not part of the build.
use crate::revision::ObjectSnapshot;

/// The pivot base of each layer of `layer_uids` (same order) at `frame` of the root context.
#[cfg_attr(not(feature = "evaluation"), allow(unused_variables))]
pub(crate) fn layer_pivot_bases(
    snapshot: &ObjectSnapshot,
    context_id: &str,
    layer_uids: &[&str],
    frame: u32,
) -> Result<Vec<[f64; 2]>, &'static str> {
    #[cfg(feature = "evaluation")]
    {
        bases(snapshot, context_id, layer_uids, frame).map(|all| all.iter().map(|e| e.0).collect())
    }
    #[cfg(not(feature = "evaluation"))]
    {
        Err("the evaluated parent pivot needs the evaluation layer")
    }
}

/// The tight cubic bounds `[x0, y0, x1, y1]` of each layer (a Null is its rest point), at `frame` of the root context:
/// the rectangle whose centre `layer_pivot_bases` answers. These are the layer's own bounds before its Motion.
#[cfg_attr(not(feature = "evaluation"), allow(unused_variables))]
pub(crate) fn layer_pivot_extents(
    snapshot: &ObjectSnapshot,
    context_id: &str,
    layer_uids: &[&str],
    frame: u32,
) -> Result<Vec<[f64; 4]>, &'static str> {
    #[cfg(feature = "evaluation")]
    {
        bases(snapshot, context_id, layer_uids, frame).map(|all| all.iter().map(|e| e.1).collect())
    }
    #[cfg(not(feature = "evaluation"))]
    {
        Err("the evaluated parent pivot needs the evaluation layer")
    }
}

/// `layerWorldBoundsUnion` (`src/js/motion.js`, 1065e9d8): the union `[x0, y0, x1, y1]` of the POSED world bounds of
/// `layer_uids` at `frame`. Each layer contributes the four corners of its own rest bounds, moved by its own Motion about
/// the bounds centre plus the anchor and then by every ancestor's Motion about that ancestor's pivot, nearest first. A
/// layer with no evaluated bounds (empty, text, mask, expression-driven) adds nothing, as an empty layer does in the
/// baseline; `None` when no layer contributes.
#[cfg_attr(not(feature = "evaluation"), allow(unused_variables))]
pub(crate) fn layers_world_union(
    snapshot: &ObjectSnapshot,
    context_id: &str,
    layer_uids: &[&str],
    frame: u32,
) -> Option<[f64; 4]> {
    #[cfg(feature = "evaluation")]
    {
        world_union(snapshot, context_id, layer_uids, frame)
    }
    #[cfg(not(feature = "evaluation"))]
    {
        None
    }
}

#[cfg(feature = "evaluation")]
fn rest_rect(snapshot: &ObjectSnapshot, context_id: &str, uid: &str, frame: u32) -> Option<[f64; 4]> {
    use crate::object_frame_packet::{
        prepare_object_exposure_frame, FrameScopeKind, ObjectFrameSelector,
    };
    if let Ok(rects) = layer_pivot_extents(snapshot, context_id, &[uid], frame) {
        return rects.first().copied().filter(|r| r.iter().all(|v| v.is_finite()));
    }
    let selector = ObjectFrameSelector {
        instance_id: snapshot.instance_id(),
        document_id: snapshot.document_id(),
        content_revision: snapshot.content_revision(),
        document_snapshot_id: snapshot.snapshot_id(),
        context_id,
        scope_kind: FrameScopeKind::Authored,
        frame,
    };
    let packet = prepare_object_exposure_frame(snapshot, &selector).ok()?;
    let index = packet.source_layers().iter().position(|l| l.layer_uid() == uid)?;
    let mut union: Option<[f64; 4]> = None;
    for record in packet.raster_records().filter(|r| r.layer_index() == index) {
        let r = record.entry().item.display_rect();
        let (s, c) = r.rotation.to_radians().sin_cos();
        let (hw, hh) = (r.width / 2.0, r.height / 2.0);
        let (ex, ey) = (c.abs() * hw + s.abs() * hh, s.abs() * hw + c.abs() * hh);
        let (cx, cy) = (r.x + hw, r.y + hh);
        let rect = [cx - ex, cy - ey, cx + ex, cy + ey];
        union = Some(union.map_or(rect, |u| {
            [u[0].min(rect[0]), u[1].min(rect[1]), u[2].max(rect[2]), u[3].max(rect[3])]
        }));
    }
    union.filter(|r| r.iter().all(|v| v.is_finite()))
}

/// `p -> pivot + R(rotation) S(scale) (p - pivot) + position`, the layer Motion of `motion_parent_chain.rs`.
#[cfg(feature = "evaluation")]
fn pose_point(
    snapshot: &ObjectSnapshot,
    context_id: &str,
    uid: &str,
    frame: u32,
    base: [f64; 2],
    p: [f64; 2],
) -> Option<[f64; 2]> {
    use crate::object_frame_packet::{Presence, PropertyHolderTarget};
    let target = PropertyHolderTarget {
        context_id: context_id.to_owned(),
        layer_uid: uid.to_owned(),
        stroke_id: Presence::Missing,
    };
    let at = f64::from(frame);
    let read = |key: &str| snapshot.sample_property_raw(&target, key, at).ok();
    let (position, anchor) = (read("position")?, read("anchor")?);
    let (rotation, scale) = (read("rotation")?, read("scale")?);
    let (&[px, py], &[ax, ay], &[rot], &[sx, sy]) =
        (position.as_slice(), anchor.as_slice(), rotation.as_slice(), scale.as_slice())
    else {
        return None;
    };
    let pivot = [base[0] + ax, base[1] + ay];
    let (s, c) = rot.to_radians().sin_cos();
    let (dx, dy) = ((p[0] - pivot[0]) * sx / 100.0, (p[1] - pivot[1]) * sy / 100.0);
    let out = [pivot[0] + c * dx - s * dy + px, pivot[1] + s * dx + c * dy + py];
    out.iter().all(|v| v.is_finite()).then_some(out)
}

#[cfg(feature = "evaluation")]
fn world_union(
    snapshot: &ObjectSnapshot,
    context_id: &str,
    layer_uids: &[&str],
    frame: u32,
) -> Option<[f64; 4]> {
    let document = snapshot.document();
    let canvas = snapshot.project().map(|p| p.project().canvas_dimensions());
    let mut union: Option<[f64; 4]> = None;
    for uid in layer_uids {
        let Some(rect) = rest_rect(snapshot, context_id, uid, frame) else {
            continue;
        };
        if rect[2] - rect[0] == 0.0 && rect[3] - rect[1] == 0.0 {
            continue;
        }
        let mut corners = [
            [rect[0], rect[1]],
            [rect[2], rect[1]],
            [rect[2], rect[3]],
            [rect[0], rect[3]],
        ];
        let mut base = [(rect[0] + rect[2]) / 2.0, (rect[1] + rect[3]) / 2.0];
        let mut current = Some((*uid).to_owned());
        let mut steps = 0;
        while let Some(at) = current {
            steps += 1;
            if steps > crate::evaluation::motion_parent_chain::CHAIN_NODE_LIMIT {
                break;
            }
            if at != *uid {
                let layer = document.layers().iter().find(|l| l.layer_uid() == at)?;
                base = match layer.null_pivot_base(canvas) {
                    Ok(Some(point)) => point,
                    _ => match rest_rect(snapshot, context_id, &at, frame) {
                        Some(r) => [(r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0],
                        None => break,
                    },
                };
            }
            for corner in &mut corners {
                *corner = pose_point(snapshot, context_id, &at, frame, base, *corner)?;
            }
            current = document
                .layers()
                .iter()
                .find(|l| l.layer_uid() == at)?
                .parent_layer_uid
                .value()
                .and_then(Option::as_deref)
                .map(str::to_owned);
        }
        for corner in corners {
            union = Some(union.map_or([corner[0], corner[1], corner[0], corner[1]], |u| {
                [u[0].min(corner[0]), u[1].min(corner[1]), u[2].max(corner[0]), u[3].max(corner[1])]
            }));
        }
    }
    union
}

#[cfg(feature = "evaluation")]
fn bases(
    snapshot: &ObjectSnapshot,
    context_id: &str,
    layer_uids: &[&str],
    frame: u32,
) -> Result<Vec<([f64; 2], [f64; 4])>, &'static str> {
    use crate::evaluation::object_vector_kernels::render_vector_eval::{
        common_layer_motion_target, evaluate_record_rest_with_pose, evaluate_vector_rest,
        is_vector_record,
    };
    use crate::object_frame_packet::projection::layer_bounds;
    use crate::object_frame_packet::{
        prepare_object_exposure_frame, FrameScopeKind, ObjectFrameSelector, Presence,
        PropertyHolderTarget,
    };
    let selector = ObjectFrameSelector {
        instance_id: snapshot.instance_id(),
        document_id: snapshot.document_id(),
        content_revision: snapshot.content_revision(),
        document_snapshot_id: snapshot.snapshot_id(),
        context_id,
        scope_kind: FrameScopeKind::Authored,
        frame,
    };
    // Raw packets stay strict: an enabled expression anywhere needs the expression owner, which this read cannot be.
    let packet = prepare_object_exposure_frame(snapshot, &selector).map_err(|_| {
        "the held frame cannot be evaluated here (an expression or an unconsumed holder)"
    })?;
    let canvas = snapshot.project().map(|p| p.project().canvas_dimensions());
    let mut result = Vec::new();
    for uid in layer_uids {
        let index = packet
            .source_layers()
            .iter()
            .position(|l| l.layer_uid() == *uid)
            .ok_or("the parent chain layer is not in the edited context")?;
        let layer = &packet.source_layers()[index];
        let target = PropertyHolderTarget {
            context_id: context_id.to_owned(),
            layer_uid: (*uid).to_owned(),
            stroke_id: Presence::Missing,
        };
        if !common_layer_motion_target(snapshot.document(), &target)
            || layer.three_d.value() == Some(&true)
            || layer.rig().is_some()
        {
            return Err("the layer has no complete native pivot (text, image, mask, effect, 3D or rig content)");
        }
        if let Some(base) = layer
            .null_pivot_base(canvas)
            .map_err(|_| "a Null layer needs nullPos or the retained project canvas")?
        {
            result.push((base, [base[0], base[1], base[0], base[1]]));
            continue;
        }
        let held = packet
            .source_frame_for_layer(index)
            .ok_or("the layer shows no drawing at the edit frame")?;
        let mut vectors = Vec::new();
        for (at, record) in snapshot
            .document()
            .objects()
            .iter()
            .enumerate()
            .filter(|(_, r)| {
                r.target.context_id == context_id
                    && r.target.layer_uid == *uid
                    && &r.target.frame_scope.kind == packet.scope_kind()
                    && r.target.frame_scope.frame == held
            })
        {
            if !is_vector_record(record) {
                return Err("a legacy contour record has no evaluated pivot here");
            }
            let vector = match packet.records().find(|r| r.source_object_index() == at) {
                Some(source) => evaluate_record_rest_with_pose(&packet, source, None),
                None => evaluate_vector_rest(snapshot, at, packet.frame()),
            }
            .map_err(|e| e.message())?;
            vectors.push(vector);
        }
        let bounds = layer_bounds(&vectors)
            .map_err(|_| "the layer cubic bounds are not finite")?
            .ok_or("the layer holds no geometry at the edit frame")?;
        let base = [bounds.x + bounds.width / 2., bounds.y + bounds.height / 2.];
        if !base.iter().all(|v| v.is_finite()) {
            return Err("the layer pivot is not finite");
        }
        result.push((base, [bounds.x, bounds.y, bounds.x + bounds.width, bounds.y + bounds.height]));
    }
    Ok(result)
}
