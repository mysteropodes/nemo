//! #2256: a layer-lifecycle kernel inside a declared component (delete, duplicate, reorder).
use crate::commands::object_fill_owner::object_context_projection::{embed, project};
use crate::object_document::object_group_kernels::object_layer_lifecycle_plan::{
    LifecycleError, LifecyclePlan,
};
use crate::object_document::ObjectDocument;
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;

/// Run a lifecycle kernel inside `context`. A declared component is the scene of the unchanged kernel (`project`), and a changed
/// plan is written back into the real document (`embed`); the selection slots, the removed and created uids are the kernel's own.
pub(super) fn plan_in(
    origin: &ObjectSnapshot,
    context: &str,
    kernel: impl FnOnce(&ObjectDocument) -> Result<LifecyclePlan, LifecycleError>,
) -> Result<Result<LifecyclePlan, LifecycleError>, DispatchErrorCode> {
    let view = project(origin.document(), context)?;
    let mut plan = match kernel(&view) {
        Ok(plan) => plan,
        Err(error) => return Ok(Err(error)),
    };
    plan.document = if plan.changed {
        embed(origin.document(), context, &view, plan.document)?
    } else {
        origin.document().clone()
    };
    Ok(Ok(plan))
}
