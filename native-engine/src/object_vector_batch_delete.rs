//! #2372 Select Delete (baseline `SM.deleteSelStrokes`): what a `delete` step of `object.vector.batch` brings with it.
//! - A texture anchor takes its STORED dabs in the same revision (they follow their anchor and are never deleted alone). A dab the request
//!   names while its anchor is also named is not deleted twice: its step becomes a no-op that keeps the receipt's one id per requested step.
//! - A bitmap brush anchor takes its baked companion image and that image's catalog entry (`rebake::Rebake::Drop`).
use crate::commands::object_fill_owner::vector_edit::batch_ops::rebake::Rebake;
use crate::commands::object_fill_owner::vector_edit::batch_ops::style::dabs::{copies, step};
use crate::commands::object_fill_owner::vector_edit::batch_ops::BatchOp;
use crate::object_document::{ObjectDocument, ObjectTarget, Presence};

/// The stored dabs of the texture anchors the requested deletes name.
pub(in crate::commands::object_fill_owner::vector_edit::batch_ops) fn dabs_taken(
    base: &ObjectDocument,
    ops: &[BatchOp],
) -> Vec<ObjectTarget> {
    ops.iter()
        .filter(|op| op.op == "delete")
        .filter_map(|op| op.stable_target.value())
        .filter_map(|target| base.objects.iter().find(|r| &r.target == target))
        .flat_map(|anchor| copies(base, anchor))
        .collect()
}

/// The expanded steps of the plain delete `op` (the step itself, then one companion delete per stored dab of an anchor), or the op back when it
/// is not a plain delete.
pub(in crate::commands::object_fill_owner::vector_edit::batch_ops) fn expand(
    base: &ObjectDocument,
    taken: &[ObjectTarget],
    op: BatchOp,
) -> Result<Vec<BatchOp>, BatchOp> {
    let Presence::Present(target) = &op.stable_target else {
        return Err(op);
    };
    if op.op != "delete" || !op.matrix.missing() {
        return Err(op);
    }
    if taken.contains(target) {
        return Ok(vec![step(
            "noop",
            Presence::Missing,
            Presence::Present(target.clone()),
        )]);
    }
    let stored = base
        .objects
        .iter()
        .find(|r| &r.target == target)
        .map(|anchor| copies(base, anchor))
        .unwrap_or_default();
    let mut steps = vec![op];
    steps.extend(
        stored
            .into_iter()
            .map(|gone| step("delete", Presence::Missing, Presence::Present(gone))),
    );
    Ok(steps)
}

/// A bitmap brush anchor about to be deleted asks for its companion image to be dropped (once per brush group).
pub(in crate::commands::object_fill_owner::vector_edit::batch_ops) fn note_companion(
    document: &ObjectDocument,
    target: &ObjectTarget,
    rebakes: &mut Vec<Rebake>,
) {
    let group = document
        .objects
        .iter()
        .find(|r| &r.target == target)
        .and_then(|r| r.vector_ext.value())
        .and_then(|e| e.bitmap_brush.value());
    if let Some(fields) = group {
        let drop = Rebake::Drop(target.clone(), fields.brush_group_id.clone());
        if !rebakes.contains(&drop) {
            rebakes.push(drop);
        }
    }
}
