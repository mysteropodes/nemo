//! Folder membership (#2371 U11 / D36): `reparentLayersIntoFolder` and `removeLayerFromFolder` of `src/js/timeline.js`
//! at 1065e9d8 (and the drop of the layer-row drag) as ONE sealed lifecycle edit. A folder is just a parent whose
//! children the layer list nests under it, so membership is `parentLayerUid`.
//!
//! `folderUid` names the folder to move the layers into (each keeps its world pose, through the Parent A planner; the
//! folder itself and other folders are skipped, there are no nested folders in v1; a cycle is refused as a whole, and the
//! folder is expanded to reveal the drop), or is null to take the layers whose parent is a folder out of it (a layer
//! with any other parent is left alone). The drag-out drop also lands the block at a gap: the optional `reorder`
//! reorders the same candidate, so the gesture stays one revision and one history entry.
use crate::commands::object_fill_owner::layer_lifecycle::parent::plan_parent_in;
use crate::commands::object_fill_owner::layer_lifecycle::{
    parse, LayerLifecycleKind, PreparedLayerLifecycle, SelectionPayload,
};
use crate::commands::ObjectFillOwner;
use crate::object_document::object_group_kernels::object_layer_lifecycle_plan::{
    index_of, plan_reorder, BaselineLayerEditPolicy, LifecycleError, LifecyclePlan,
};
use crate::object_document::{object_deserialize, ObjectDocument, Presence, PropertyHolder};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;
use serde::Deserialize;
use serde_json::Number;
use std::collections::HashSet;

const ROOT: &str = "scene-root";
const MAX_LAYERS: usize = 128;

#[derive(Debug)]
struct ReorderTail {
    gap: Number,
    selection: SelectionPayload,
}
object_deserialize!(ReorderTail {
    gap: Number,
    selection: SelectionPayload
});

#[derive(Debug)]
struct FolderReparentPayload {
    command: String,
    layer_uids: Vec<String>,
    folder_uid: Option<String>,
    frame: u32,
    reorder: Option<ReorderTail>,
}
object_deserialize!(FolderReparentPayload {
    command: String,
    layer_uids: Vec<String>,
    folder_uid: Option<String>,
    frame: u32,
    reorder: Option<ReorderTail>
});

fn in_folder(document: &ObjectDocument, uid: &str) -> bool {
    index_of(document, uid)
        .ok()
        .and_then(|at| {
            document.layers[at]
                .parent_layer_uid
                .value()
                .cloned()
                .flatten()
        })
        .and_then(|parent| index_of(document, &parent).ok())
        .is_some_and(|at| document.layers[at].is_folder_layer.value() == Some(&true))
}

/// Copies the layer-level Motion holder of `uid` that the Parent A plan rewrote into the accumulating document.
fn adopt_holder(into: &mut ObjectDocument, from: &ObjectDocument, uid: &str) {
    let mine =
        |h: &PropertyHolder| h.target().layer_uid() == uid && h.target().stroke_id().is_none();
    let Some(source) = from
        .property_holders()
        .and_then(|all| all.iter().find(|h| mine(h)))
    else {
        return;
    };
    let mut holders = into
        .property_holders()
        .map(<[_]>::to_vec)
        .unwrap_or_default();
    match holders.iter().position(mine) {
        Some(at) => holders[at] = source.clone(),
        None => holders.push(source.clone()),
    }
    into.property_holders = Presence::Present(holders);
}

fn plan(
    origin: &ObjectSnapshot,
    payload: FolderReparentPayload,
) -> Result<LifecyclePlan, LifecycleError> {
    let document = origin.document();
    let folder_at = match payload.folder_uid.as_deref() {
        Some(uid) => {
            let at = index_of(document, uid)?;
            if document.layers[at].is_folder_layer.value() != Some(&true) {
                return Err(LifecycleError::ParentRefused(uid.to_owned()));
            }
            Some(at)
        }
        None => None,
    };
    let mut next = document.clone();
    let mut changed = false;
    let mut touched = Vec::new();
    let mut eligible = false;
    for uid in &payload.layer_uids {
        let at = index_of(document, uid)?;
        let skip = match &payload.folder_uid {
            Some(folder) => {
                uid == folder || document.layers[at].is_folder_layer.value() == Some(&true)
            }
            None => !in_folder(document, uid),
        };
        if skip {
            continue;
        }
        eligible = true;
        let moved = plan_parent_in(
            origin,
            ROOT,
            uid,
            payload.folder_uid.as_deref(),
            payload.frame,
            true,
        )?;
        if moved.changed {
            adopt_holder(&mut next, &moved.document, uid);
            next.layers[at].parent_layer_uid = moved.document.layers[at].parent_layer_uid.clone();
            touched.push(uid.clone());
            changed = true;
        }
    }
    if let (Some(at), true) = (folder_at, eligible) {
        if next.layers[at].folder_collapsed.value() == Some(&true) {
            next.layers[at].folder_collapsed = Presence::Present(false);
            changed = true;
        }
    }
    let mut result = LifecyclePlan {
        document: next,
        changed,
        selection: None,
        removed: Vec::new(),
        touched,
        spared: None,
        created: None,
    };
    if let Some(tail) = payload.reorder {
        let gap = tail
            .gap
            .as_f64()
            .filter(|g| g.is_finite())
            .ok_or(LifecycleError::NonFiniteGap)?;
        let selection = tail
            .selection
            .into_selection()
            .map_err(|_| LifecycleError::NonFiniteGap)?;
        let moving: Vec<i64> = payload
            .layer_uids
            .iter()
            .filter_map(|uid| index_of(&result.document, uid).ok())
            .filter_map(|at| i64::try_from(at).ok())
            .collect();
        let order = plan_reorder(
            &result.document,
            &moving,
            gap,
            &selection,
            &BaselineLayerEditPolicy,
        )?;
        result.changed |= order.changed;
        result.selection = order.selection;
        if order.changed {
            result.document = order.document;
        }
    }
    if !result.changed {
        return Ok(LifecyclePlan::unchanged(document, result.selection));
    }
    Ok(result)
}

impl ObjectFillOwner {
    pub(crate) fn prepare_layer_folder_reparent_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedLayerLifecycle, DispatchErrorCode> {
        let payload: FolderReparentPayload = parse(bytes)?;
        let distinct: HashSet<&str> = payload.layer_uids.iter().map(String::as_str).collect();
        if payload.command != "object.layer.folder.reparent"
            || payload.layer_uids.is_empty()
            || payload.layer_uids.len() > MAX_LAYERS
            || distinct.len() != payload.layer_uids.len()
            || payload.layer_uids.iter().any(String::is_empty)
            || payload.folder_uid.as_deref() == Some("")
            || (payload.folder_uid.is_some() && payload.reorder.is_some())
        {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let origin = self.current_snapshot();
        let plan = plan(&origin, payload);
        PreparedLayerLifecycle::seal(origin, LayerLifecycleKind::FolderReparent, plan)
    }
}
