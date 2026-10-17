//! G24 (L02-G): the detached plan of one group edit over an admitted document.
//!
//! Document-layer and pure: it reads the document, runs the L02-A kernel over a
//! copy of the layer's group table and returns a candidate document. It owns no
//! history, revision, id source or receipt; the sealed owner transaction that
//! publishes the candidate is `object_group_ops.rs`. Group targets are stable
//! native identities (layer uid, context id, stroke id), never frame-local
//! presentation ids: the layer's items are its distinct authored stroke ids in
//! document order, so one stroke is one item across all of its frames.
//!
//! Native admission differs from the baseline on purpose and every difference
//! is a typed refusal that leaves the document unchanged: an unknown group or
//! stroke, a rename of a tag-only group, a mode outside the five known ones.
//! After every edit a group that fell below two entries is dissolved in the
//! SAME plan (repeatedly, in table order), where the baseline left such
//! single-entry survivors behind.
use crate::object_document::object_group_kernels::object_group_hierarchy::{
    CombineMode, GroupError, GroupLayer,
};
use crate::object_document::object_group_kernels::object_group_record::{
    layer_stroke_ids, GroupTable,
};
use crate::object_document::{ObjectDocument, Presence};

/// One user-level group edit, addressed by stroke ids and group ids.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum GroupEdit {
    /// `groupSelection`: nest the selection under a new group `gid`.
    Group {
        gid: String,
        name: String,
        strokes: Vec<String>,
    },
    /// `ungroupSelection`: dissolve one level.
    Ungroup { strokes: Vec<String> },
    /// `combineSelection`: set the mode of the group the selection IS, else
    /// create the flat group `gid`.
    Combine {
        gid: String,
        mode: CombineMode,
        strokes: Vec<String>,
    },
    /// `setGroupCombineMode`.
    SetMode { gid: String, mode: CombineMode },
    /// `renameGroup` of an existing group.
    Rename { gid: String, name: String },
    /// `removeMemberFromGroup` ("Sortir du groupe"): take ONE shape out of the
    /// group its tag names.
    ExitGroup { stroke: String },
    /// Take shapes out of EVERY group that lists them (`detachLeaf` plus the
    /// tag), so a caller that is about to delete, move or re-identify them can
    /// reconcile the table in its own transaction. Not a wire operation here.
    Detach { strokes: Vec<String> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum GroupPlanError {
    UnknownLayer(String),
    /// The layer is locked; the lock policy is Root F06's.
    Locked(String),
    /// The layer's table belongs to another context; one table per layer.
    ContextMismatch {
        table: String,
        requested: String,
    },
    UnknownStroke(String),
    /// The new group id is also the id of a stroke of the layer.
    IdCollidesWithStroke(String),
    /// The edit would nest a group more than 16 levels below its root.
    TooDeep,
    Kernel(GroupError),
    /// The stored table cannot form a kernel layer (admission was bypassed).
    TableUnreadable(String),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct GroupPlan {
    /// The candidate; a copy of the origin when nothing changed.
    pub(crate) document: ObjectDocument,
    pub(crate) changed: bool,
    /// The group an edit created.
    pub(crate) created: Option<String>,
    /// Groups of the origin that no longer exist, in table order (a dissolved
    /// level, or a group that fell below two entries).
    pub(crate) removed: Vec<String>,
}

/// Dissolve every group below two entries, first in table order, until none is
/// left. Each pass removes one entry, so the loop terminates.
fn normalize(layer: &mut GroupLayer) {
    loop {
        let small = layer
            .entries()
            .iter()
            .find(|entry| entry.order.len() < 2)
            .map(|entry| entry.gid.clone());
        match small {
            Some(gid) => {
                if layer.dissolve_group(&gid).is_err() {
                    return;
                }
            }
            None => return,
        }
    }
}

/// Ancestors above an entry the admission accepts (the member walk's depth).
const MAX_ANCESTORS: usize = 16;

fn too_deep(layer: &GroupLayer) -> bool {
    layer.entries().iter().any(|entry| {
        let mut up = layer.parent_group_of(&entry.gid);
        let mut levels = 0;
        while let Some(parent) = up {
            levels += 1;
            if levels > MAX_ANCESTORS {
                return true;
            }
            up = layer.parent_group_of(&parent);
        }
        false
    })
}

/// Stroke ids to item slots of the kernel layer, in the caller's order.
fn slots(layer: &GroupLayer, strokes: &[String]) -> Result<Vec<usize>, GroupPlanError> {
    strokes
        .iter()
        .map(|id| {
            layer
                .items()
                .iter()
                .position(|item| item.stroke_id == *id)
                .ok_or_else(|| GroupPlanError::UnknownStroke(id.clone()))
        })
        .collect()
}

pub(crate) fn plan_group_edit(
    document: &ObjectDocument,
    layer_uid: &str,
    context_id: &str,
    edit: &GroupEdit,
) -> Result<GroupPlan, GroupPlanError> {
    let index = document
        .layers
        .iter()
        .position(|layer| layer.layer_uid == layer_uid)
        .ok_or_else(|| GroupPlanError::UnknownLayer(layer_uid.to_string()))?;
    let layer = &document.layers[index];
    if layer.locked.value() == Some(&true) {
        return Err(GroupPlanError::Locked(layer_uid.to_string()));
    }
    let strokes = layer_stroke_ids(&document.objects, layer_uid, context_id);
    let table = match layer.groups.value() {
        Some(table) if table.context_id != context_id => {
            return Err(GroupPlanError::ContextMismatch {
                table: table.context_id.clone(),
                requested: context_id.to_string(),
            });
        }
        Some(table) => table.clone(),
        None => GroupTable::empty(context_id),
    };
    let before = table
        .to_layer(&strokes)
        .ok_or_else(|| GroupPlanError::TableUnreadable(layer_uid.to_string()))?;
    let mut work = before.clone();
    match edit {
        GroupEdit::Group {
            gid,
            name,
            strokes: selected,
        } => {
            let selection = slots(&work, selected)?;
            work.group_selection(&selection, gid, name)
                .map_err(GroupPlanError::Kernel)?;
        }
        GroupEdit::Ungroup { strokes: selected } => {
            let selection = slots(&work, selected)?;
            work.ungroup_selection(&selection)
                .map_err(GroupPlanError::Kernel)?;
        }
        GroupEdit::Combine {
            gid,
            mode,
            strokes: selected,
        } => {
            let selection = slots(&work, selected)?;
            work.combine_selection(&selection, *mode, gid)
                .map_err(GroupPlanError::Kernel)?;
        }
        GroupEdit::SetMode { gid, mode } => {
            work.set_combine_mode(gid, *mode)
                .map_err(GroupPlanError::Kernel)?;
        }
        GroupEdit::ExitGroup { stroke } => {
            let selection = slots(&work, std::slice::from_ref(stroke))?;
            work.remove_member(selection[0])
                .map_err(GroupPlanError::Kernel)?;
        }
        GroupEdit::Detach { strokes: selected } => {
            let selection = slots(&work, selected)?;
            for (item, stroke) in selection.iter().zip(selected) {
                let tagged = work.items()[*item]
                    .group_id
                    .as_deref()
                    .is_some_and(|gid| !gid.is_empty());
                if tagged {
                    // A tag always names the group that lists the stroke.
                    let _ = work.remove_member(*item);
                }
                work.detach_leaf(stroke);
            }
        }
        GroupEdit::Rename { gid, name } => {
            // The kernel would create a plain entry for an unknown gid (the
            // baseline's tag-only group); a native tag always has an entry.
            if work.entry(gid).is_none() {
                return Err(GroupPlanError::Kernel(GroupError::UnknownGroup(
                    gid.clone(),
                )));
            }
            work.rename_group(gid, name, &[])
                .map_err(GroupPlanError::Kernel)?;
        }
    }
    normalize(&mut work);
    let created = match edit {
        GroupEdit::Group { gid, .. } | GroupEdit::Combine { gid, .. }
            if before.entry(gid).is_none() && work.entry(gid).is_some() =>
        {
            Some(gid.clone())
        }
        _ => None,
    };
    if let Some(gid) = &created {
        if strokes.contains(gid) {
            return Err(GroupPlanError::IdCollidesWithStroke(gid.clone()));
        }
    }
    if too_deep(&work) {
        return Err(GroupPlanError::TooDeep);
    }
    let removed: Vec<String> = before
        .entries()
        .iter()
        .filter(|entry| work.entry(&entry.gid).is_none())
        .map(|entry| entry.gid.clone())
        .collect();
    let changed = work != before;
    let mut candidate = document.clone();
    if changed {
        candidate.layers[index].groups =
            Presence::Present(GroupTable::from_layer(context_id, &work));
    }
    Ok(GroupPlan {
        document: candidate,
        changed,
        created,
        removed,
    })
}
