//! Duplicator evaluation: descriptor + frame to detached copies (L04-L, I08/I09).
//!
//! The join of `applyLayerDuplicator` and `getEffectiveStrokesRendered`
//! (`src/js/app.js` at baseline `fbe4b8a3`) over the L04 kernels: the count
//! caps and the grid / radial / path base offsets (L04-B), the seeded per-clone
//! placement and the effector sums (L04-C), the placement matrix, the copy loop
//! and the temporal stagger (L04-D), the effector layers of the context
//! (`object_effector_layers`) and, here, the pieces only the join owns: the
//! entry guards, the Object-mode pool, and the order the baseline asks for
//! things in.
//!
//! The result is DERIVED data. [`Evaluation::Copies`] holds detached
//! `DuplicatorCopy` values (never an `ObjectRecord` of the document), and
//! the inputs (the host content, the descriptor, the document layers) are only
//! read: a copy cannot reach persistence, selection or a frame save by
//! construction. [`Evaluation::Unchanged`] is the baseline returning its `base`
//! argument, with the reason.
//!
//! Reading order, as in the baseline: guards; the clone count; the pivot of
//! the host content; the Object-mode pool and its filter; the eight
//! `dupOffset*` samples; the path of path mode (an unusable path returns the
//! base); then per clone the pool member or the temporally shifted content,
//! the mode offset, the placement (effector layers are sampled when the first
//! clone is placed) and the copies. Motion sampling and the content of other
//! layers come through ports; a provider that is absent is a typed
//! [`Unavailable`], never a fabricated zero.

use crate::evaluation::object_component_kernels::duplicator_placement_matrix::{
    bounds_center, Pivot,
};
use crate::evaluation::object_component_kernels::duplicator_placement_modes::{
    dup_mode, duplicator_count, grid_cols, places_no_clones, DupMode,
};
use crate::evaluation::object_component_kernels::duplicator_placement_temporal::{
    build_copies, CopyRequest,
};
use crate::evaluation::object_component_kernels::object_duplicator_evaluate_place::{
    path_info, pool_copies, pool_members, sample_deltas, Placer,
};
use crate::evaluation::object_component_kernels::object_duplicator_evaluate_spec::describe;
use crate::evaluation::object_component_kernels::object_effector_layers::EffectorScope;

#[allow(unused_imports)]
pub(crate) use crate::evaluation::object_component_kernels::object_duplicator_evaluate_types::{
    ClonePlacementTrace, CopySet, Entry, EvaluateError, Evaluation, LayerSources, Ports, Request,
    UnchangedReason,
};

#[path = "object_duplicator_snapshot.rs"]
pub(crate) mod snapshot;

/// Evaluate one duplicator layer at one frame.
pub(crate) fn evaluate_duplicator(
    req: &Request<'_>,
    ports: &Ports<'_>,
) -> Result<Evaluation, EvaluateError> {
    let record = match req.set.duplicator(req.context_id, req.host_uid) {
        Some(record) => record,
        None => return Ok(Evaluation::Unchanged(UnchangedReason::NoDuplicator)),
    };
    let view = describe(&record.duplicator);
    if let Entry::Rendered { edit_source } = req.entry {
        if edit_source {
            return Ok(Evaluation::Unchanged(UnchangedReason::EditingSource));
        }
        if req.base.is_empty() && !view.objects_mode {
            return Ok(Evaluation::Unchanged(UnchangedReason::EmptyBase));
        }
    }
    let count = duplicator_count(&view.spec);
    if places_no_clones(count) {
        return Ok(Evaluation::Unchanged(UnchangedReason::SingleClone));
    }
    let pivot =
        bounds_center(req.base, req.canvas_w, req.canvas_h).map_err(EvaluateError::Pivot)?;
    let pool = if view.objects_mode && !view.pool_uids.is_empty() {
        let members = pool_members(req, ports, view.pool_uids);
        if members.is_empty() {
            return Ok(Evaluation::Unchanged(UnchangedReason::EmptyPool));
        }
        Some(members)
    } else {
        None
    };
    let deltas = sample_deltas(ports.motion, req.host_uid)?;
    let mode = dup_mode(view.spec.mode);
    let path = if mode == DupMode::Path {
        let found = path_info(req, ports, record.duplicator.path_layer_uid())?;
        if found.is_none() {
            return Ok(Evaluation::Unchanged(UnchangedReason::BrokenPath));
        }
        found
    } else {
        None
    };
    let mut placer = Placer {
        spec: view.spec,
        seed: view.seed,
        rand: view.rand,
        deltas,
        count,
        cols: grid_cols(&view.spec),
        is_3d_layer: req.is_3d_layer,
        path,
        inline: view.inline_effectors.clone(),
        effectors: None,
        scope: EffectorScope {
            context_id: req.context_id,
            layers: req.layers,
            set: req.set,
            canvas_w: req.canvas_w,
            canvas_h: req.canvas_h,
        },
        host_uid: req.host_uid,
        motion: ports.motion,
        trace: Vec::new(),
        failure: None,
    };
    let copies = match &pool {
        Some(members) => pool_copies(req, ports, members, view.seed, &mut placer, pivot)?,
        None => {
            let can_resample = match req.entry {
                Entry::Rendered { .. } => true,
                Entry::Direct { can_resample } => can_resample,
            };
            let request = CopyRequest {
                seed: view.seed,
                count,
                frame_idx: req.frame_idx,
                canvas_w: req.canvas_w,
                canvas_h: req.canvas_h,
                temporal: view.temporal,
                span: req.span,
                can_resample,
            };
            let mut place = |k: u32, p: Pivot| placer.at(k, p);
            let mut resample = |frame: f64| ports.sources.strokes_at(req.host_uid, frame);
            let built = build_copies(req.base, &request, &mut place, &mut resample)
                .map_err(EvaluateError::Build)?;
            built.unwrap_or_default()
        }
    };
    if let Some(error) = placer.failure {
        return Err(error);
    }
    Ok(Evaluation::Copies(CopySet {
        count,
        copies,
        placements: placer.trace,
    }))
}

#[cfg(test)]
#[path = "../tests/object_duplicator_evaluate_controls.rs"]
mod controls_tests;
#[cfg(test)]
#[path = "../tests/object_duplicator_evaluate_pins.rs"]
mod pins_tests;
#[cfg(test)]
#[path = "../tests/object_duplicator_evaluate_codec.rs"]
pub(crate) mod test_codec;
#[cfg(test)]
#[path = "../tests/object_duplicator_evaluate_compare.rs"]
pub(crate) mod test_compare;
#[cfg(test)]
#[path = "../tests/object_duplicator_evaluate.rs"]
mod tests;
