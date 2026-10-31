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
        bases(snapshot, context_id, layer_uids, frame)
    }
    #[cfg(not(feature = "evaluation"))]
    {
        Err("the evaluated parent pivot needs the evaluation layer")
    }
}

#[cfg(feature = "evaluation")]
fn bases(
    snapshot: &ObjectSnapshot,
    context_id: &str,
    layer_uids: &[&str],
    frame: u32,
) -> Result<Vec<[f64; 2]>, &'static str> {
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
            result.push(base);
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
        result.push(base);
    }
    Ok(result)
}
