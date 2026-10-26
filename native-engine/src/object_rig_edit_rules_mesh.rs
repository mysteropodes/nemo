//! Mesh binds of the rig edit transaction (L03-O, I12): the record `rigBindMesh` pushes, on a detached
//! copy of the rig. Like every rule it takes the stored rig and returns the proposed one; it reads no
//! document and calls no kernel (the weights and rest numbers arrive from `object_rig_edit_plan_mesh.rs`).
use crate::commands::object_fill_owner::widget_edit::object_rig_edit::plan_mesh::PlannedMeshBind;
use crate::commands::object_fill_owner::widget_edit::object_rig_edit::rules::control_weight;
use crate::object_document::object_rig_record::{RigBind, RigRecord};
use crate::object_document::Presence;
use crate::request_receipts::DispatchErrorCode;

/// `{meshId, rest, weights, rotate}`: a mesh bind carries no `restHandles` (a mesh has no tangents).
fn bind_from_plan(plan: &PlannedMeshBind, rotate: bool) -> Result<RigBind, DispatchErrorCode> {
    let weights = plan
        .weights
        .iter()
        .map(|list| list.iter().map(control_weight).collect::<Result<Vec<_>, _>>())
        .collect::<Result<Vec<_>, _>>()?;
    Ok(RigBind {
        stroke_id: Presence::Missing,
        mesh_id: Presence::Present(plan.mesh_id.clone()),
        rest: plan.rest.clone(),
        rest_handles: Presence::Missing,
        weights,
        rotate,
    })
}

/// Mesh binds REPLACE, never stack: each planned bind removes every bind of its mesh
/// (`rig.binds.filter(b => b.meshId !== id)`) and is appended.
pub(super) fn apply_mesh_binds(
    rig: &RigRecord,
    planned: &[PlannedMeshBind],
    rotate: bool,
) -> Result<RigRecord, DispatchErrorCode> {
    let mut next = rig.clone();
    for plan in planned {
        let bind = bind_from_plan(plan, rotate)?;
        next.binds
            .retain(|old| old.mesh_id.value().map(String::as_str) != Some(plan.mesh_id.as_str()));
        next.binds.push(bind);
    }
    Ok(next)
}
