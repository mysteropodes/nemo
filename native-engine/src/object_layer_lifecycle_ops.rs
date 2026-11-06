//! Sealed layer lifecycle transactions (F07, L02-F): delete, duplicate,
//! rename and batch reorder of document layers through the existing revision
//! owner. Declared as a private child of `object_fill_owner.rs`, next to the
//! layer-create / visibility preparation it mirrors.
//!
//! Preparation parses a bounded, strict payload (object-only, duplicate and
//! unknown members refused), reads the owner's current immutable snapshot,
//! asks the L02-E plan kernel for a detached candidate, runs the same storage
//! admission layer-create runs and seals the result to that snapshot. Commit
//! re-checks instance, document and revision exactly like
//! `commit_layer_create`; a plan that changes nothing publishes nothing. No
//! history, receipt or id state lives here: the revision-derived identity
//! comes from the kernel and the one history entry from `NativeObjectHistory`.
//! The baseline never gated delete, duplicate, rename, recolour or reorder on a
//! lock or a transform parent, so the owner hands the kernel the policy that
//! permits every decision.
use crate::commands::object_fill_owner::object_context_projection::{context_of, embed, project};
use crate::commands::object_fill_owner::object_frame_bulk_move::plan_arrange_json;
use crate::commands::ObjectFillOwner;
use crate::object_document::object_group_kernels::object_layer_lifecycle_plan::{
    plan_delete, plan_duplicate, plan_rename, plan_reorder, BaselineLayerEditPolicy, LayerSelection,
    LifecycleError,
    LifecyclePlan, SelectionOutcome,
};
use crate::object_document::{object_deserialize, ObjectDocument, Presence};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;
use serde::Deserialize;
use serde_json::Number;
use std::collections::HashSet;

// #2372 U12: Parent A planning reads the pinned snapshot's Motion through the read-query facade, so it lives with the commands.
#[path = "object_layer_lifecycle_parent.rs"]
mod parent;
use parent::plan_parent;
// #2372: Link time and Follow path write the layer's own range / follow fields; the range seed reads the range evaluator.
#[path = "object_layer_lifecycle_link.rs"]
mod link;
use link::{plan_follow_path, plan_time_link, TimeLinkRequest};
// #2371 U11: the layer colour label.
#[path = "object_layer_lifecycle_color.rs"]
mod color;
// #2371 U11 / D36: Null, Effect, Guide, Folder and Effector creation, and folder membership.
#[path = "object_layer_kind_create.rs"]
mod kind_create;
#[path = "object_layer_folder_reparent.rs"]
mod folder_reparent;

const MAX_PAYLOAD_BYTES: usize = 4096;

#[derive(Debug)]
struct SelectionPayload {
    active: String,
    selected: Vec<String>,
    anchor: Option<String>,
}
object_deserialize!(SelectionPayload {
    active: String,
    selected: Vec<String>,
    anchor: Option<String>
});

#[derive(Debug)]
struct DeletePayload {
    command: String,
    selection: SelectionPayload,
    /// #2256: the declared component the edit works inside; absent is the scene root.
    context_id: Presence<String>,
}
object_deserialize!(DeletePayload {
    command: String,
    selection: SelectionPayload,
    #[serde(default)] context_id: Presence<String>
});

#[derive(Debug)]
struct DuplicatePayload {
    command: String,
    layer_uid: String,
    context_id: Presence<String>,
}
object_deserialize!(DuplicatePayload {
    command: String,
    layer_uid: String,
    #[serde(default)] context_id: Presence<String>
});

#[derive(Debug)]
struct RenamePayload {
    command: String,
    layer_uid: String,
    name: String,
}
object_deserialize!(RenamePayload {
    command: String,
    layer_uid: String,
    name: String
});

#[derive(Debug)]
struct ReorderPayload {
    command: String,
    layer_uids: Vec<String>,
    gap: Number,
    selection: SelectionPayload,
    context_id: Presence<String>,
}
object_deserialize!(ReorderPayload {
    command: String,
    layer_uids: Vec<String>,
    gap: Number,
    selection: SelectionPayload,
    #[serde(default)] context_id: Presence<String>
});

#[derive(Debug)]
struct ParentPayload {
    command: String,
    context_id: String,
    layer_uid: String,
    parent_layer_uid: Option<String>,
    frame: u32,
}
object_deserialize!(ParentPayload {
    command: String,
    context_id: String,
    layer_uid: String,
    parent_layer_uid: Option<String>,
    frame: u32
});

#[derive(Debug)]
struct TimeLinkPayload {
    command: String,
    context_id: String,
    layer_uid: String,
    source_layer_uid: Option<String>,
    mode: Option<String>,
    src_anchor: Option<String>,
    frame: u32,
}
object_deserialize!(TimeLinkPayload {
    command: String,
    context_id: String,
    layer_uid: String,
    source_layer_uid: Option<String>,
    mode: Option<String>,
    src_anchor: Option<String>,
    frame: u32
});

#[derive(Debug)]
struct FollowPathPayload {
    command: String,
    context_id: String,
    layer_uid: String,
    target_layer_uid: Option<String>,
    align: Option<bool>,
}
object_deserialize!(FollowPathPayload {
    command: String,
    context_id: String,
    layer_uid: String,
    target_layer_uid: Option<String>,
    align: Option<bool>
});

impl SelectionPayload {
    /// Identities only; an empty string is never a layer identity.
    fn into_selection(self) -> Result<LayerSelection, DispatchErrorCode> {
        let named = !self.active.is_empty()
            && self.selected.iter().all(|uid| !uid.is_empty())
            && self.anchor.as_deref() != Some("");
        if !named {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        Ok(LayerSelection {
            active: self.active,
            selected: self.selected,
            anchor: self.anchor,
        })
    }
}

fn parse<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, DispatchErrorCode> {
    if bytes.len() > MAX_PAYLOAD_BYTES {
        return Err(DispatchErrorCode::InvalidRequest);
    }
    serde_json::from_slice(bytes).map_err(|_| DispatchErrorCode::InvalidRequest)
}

/// How a typed kernel refusal reaches the request-receipt vocabulary.
fn refusal_code(error: &LifecycleError) -> DispatchErrorCode {
    match error {
        LifecycleError::UnknownLayer(_) => DispatchErrorCode::NotFound,
        LifecycleError::LastLayer
        | LifecycleError::ParentSelf(_)
        | LifecycleError::ParentCycle { .. }
        | LifecycleError::ParentRefused(_)
        | LifecycleError::LinkSelf(_)
        | LifecycleError::LinkCycle { .. }
        | LifecycleError::LinkRefused(_)
        | LifecycleError::NonFiniteGap
        | LifecycleError::InvalidColor(_)
        | LifecycleError::SourceUnnamed(_) => DispatchErrorCode::InvalidRequest,
        // Admission guarantees every context lists every layer.
        LifecycleError::OrderPlanIncomplete { .. } => DispatchErrorCode::Internal,
        LifecycleError::PoseUnavailable(_)
        | LifecycleError::LinkUnavailable(_)
        | LifecycleError::RevisionOutOfRange
        | LifecycleError::IdUnavailable
        | LifecycleError::TooManyLayers
        | LifecycleError::Policy { .. } => DispatchErrorCode::Unavailable,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LayerLifecycleKind {
    Delete,
    Duplicate,
    Rename,
    Color,
    Reorder,
    Parent,
    TimeLink,
    FollowPath,
    FolderReparent,
    RangeArrange,
}

/// A detached candidate sealed to the snapshot it was planned from. Only this
/// module can create one or replace its origin or after-state.
#[derive(Debug, Clone)]
pub(crate) struct PreparedLayerLifecycle {
    origin: ObjectSnapshot,
    plan: LifecyclePlan,
    kind: LayerLifecycleKind,
}

// Read by the typed in-process callers (tests today, the L02-J client port
// next); the compact receipt carries only applied/historyEntriesAdded/layerUid.
#[allow(dead_code)]
impl PreparedLayerLifecycle {
    pub(crate) fn kind(&self) -> LayerLifecycleKind {
        self.kind
    }
    /// `false`: nothing will be published or recorded.
    pub(crate) fn changed(&self) -> bool {
        self.plan.changed
    }
    /// The identity a duplicate minted.
    pub(crate) fn created(&self) -> Option<&str> {
        self.plan.created.as_deref()
    }
    /// Deleted layers, in document order.
    pub(crate) fn removed(&self) -> &[String] {
        &self.plan.removed
    }
    /// A selected layer a delete kept because a document needs one layer.
    pub(crate) fn spared(&self) -> Option<&str> {
        self.plan.spared.as_deref()
    }
    /// The editor selection as slots of the candidate's layer array.
    pub(crate) fn selection(&self) -> Option<&SelectionOutcome> {
        self.plan.selection.as_ref()
    }
    pub(crate) fn document(&self) -> &ObjectDocument {
        &self.plan.document
    }
}

impl PreparedLayerLifecycle {
    fn seal(
        origin: ObjectSnapshot,
        kind: LayerLifecycleKind,
        plan: Result<LifecyclePlan, LifecycleError>,
    ) -> Result<Self, DispatchErrorCode> {
        let plan = plan.map_err(|error| refusal_code(&error))?;
        if plan.changed {
            ObjectSnapshot::validate_drawing(&plan.document)
                .map_err(|_| DispatchErrorCode::InvalidRequest)?;
        }
        Ok(Self { origin, plan, kind })
    }
}

/// #2256: run a lifecycle kernel inside `context`. A declared component is the scene of the unchanged kernel (`project`), and a changed
/// plan is written back into the real document (`embed`); the selection slots, the removed and created uids are the kernel's own.
fn plan_in(
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

impl ObjectFillOwner {
    pub(crate) fn prepare_layer_delete_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedLayerLifecycle, DispatchErrorCode> {
        let payload: DeletePayload = parse(bytes)?;
        if payload.command != "object.layer.delete" {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let context = context_of(&payload.context_id)?.to_owned();
        let selection = payload.selection.into_selection()?;
        let origin = self.current_snapshot();
        let plan = plan_in(&origin, &context, |view| {
            plan_delete(view, &selection, &BaselineLayerEditPolicy)
        })?;
        PreparedLayerLifecycle::seal(origin, LayerLifecycleKind::Delete, plan)
    }
    pub(crate) fn prepare_layer_duplicate_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedLayerLifecycle, DispatchErrorCode> {
        let payload: DuplicatePayload = parse(bytes)?;
        if payload.command != "object.layer.duplicate" || payload.layer_uid.is_empty() {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let context = context_of(&payload.context_id)?.to_owned();
        let origin = self.current_snapshot();
        // The kernel bounds the embedded revision (2^53 - 1) like layer-create.
        let next = origin
            .content_revision()
            .checked_add(1)
            .ok_or(DispatchErrorCode::Unavailable)?;
        let plan = plan_in(&origin, &context, |view| {
            plan_duplicate(view, &payload.layer_uid, next, &BaselineLayerEditPolicy)
        })?;
        PreparedLayerLifecycle::seal(origin, LayerLifecycleKind::Duplicate, plan)
    }
    pub(crate) fn prepare_layer_rename_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedLayerLifecycle, DispatchErrorCode> {
        let payload: RenamePayload = parse(bytes)?;
        if payload.command != "object.layer.rename" || payload.layer_uid.is_empty() {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let origin = self.current_snapshot();
        let plan = plan_rename(
            origin.document(),
            &payload.layer_uid,
            &payload.name,
            &BaselineLayerEditPolicy,
        );
        PreparedLayerLifecycle::seal(origin, LayerLifecycleKind::Rename, plan)
    }
    pub(crate) fn prepare_layer_reorder_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedLayerLifecycle, DispatchErrorCode> {
        let payload: ReorderPayload = parse(bytes)?;
        if payload.command != "object.layer.reorder" || payload.layer_uids.is_empty() {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let gap = payload
            .gap
            .as_f64()
            .filter(|gap| gap.is_finite())
            .ok_or(DispatchErrorCode::InvalidRequest)?;
        let context = context_of(&payload.context_id)?.to_owned();
        let selection = payload.selection.into_selection()?;
        let origin = self.current_snapshot();
        // Targets are identities; a repeated one is a malformed set, not a
        // silently merged one.
        let mut seen: HashSet<&str> = HashSet::new();
        for uid in &payload.layer_uids {
            if uid.is_empty() || !seen.insert(uid.as_str()) {
                return Err(DispatchErrorCode::InvalidRequest);
            }
        }
        // The slots are those of the context's own layer stack.
        let stack = project(origin.document(), &context)?;
        let mut moving: Vec<i64> = Vec::with_capacity(payload.layer_uids.len());
        for uid in &payload.layer_uids {
            let slot = stack
                .layers
                .iter()
                .position(|layer| layer.layer_uid == *uid)
                .ok_or(DispatchErrorCode::NotFound)?;
            moving.push(i64::try_from(slot).map_err(|_| DispatchErrorCode::InvalidRequest)?);
        }
        let plan = plan_in(&origin, &context, |view| {
            plan_reorder(view, &moving, gap, &selection, &BaselineLayerEditPolicy)
        })?;
        PreparedLayerLifecycle::seal(origin, LayerLifecycleKind::Reorder, plan)
    }
    /// #2372 U12: Parent A set or clear (`parentLayerUid: null`) that keeps the child's world pose at `frame`.
    pub(crate) fn prepare_layer_parent_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedLayerLifecycle, DispatchErrorCode> {
        let payload: ParentPayload = parse(bytes)?;
        if payload.command != "object.layer.parent.set"
            || payload.context_id.is_empty()
            || payload.layer_uid.is_empty()
            || payload.parent_layer_uid.as_deref() == Some("")
        {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let origin = self.current_snapshot();
        let plan = plan_parent(
            &origin,
            &payload.context_id,
            &payload.layer_uid,
            payload.parent_layer_uid.as_deref(),
            payload.frame,
        );
        PreparedLayerLifecycle::seal(origin, LayerLifecycleKind::Parent, plan)
    }
    /// #2372: Link time to a layer (with the seeded offsets) or unlink it (baking the range), `sourceLayerUid: null`.
    pub(crate) fn prepare_layer_time_link_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedLayerLifecycle, DispatchErrorCode> {
        let payload: TimeLinkPayload = parse(bytes)?;
        let empty = |v: &Option<String>| v.as_deref() == Some("");
        if payload.command != "object.layer.timeLink.set"
            || payload.context_id.is_empty()
            || payload.layer_uid.is_empty()
            || [&payload.source_layer_uid, &payload.mode, &payload.src_anchor]
                .into_iter()
                .any(empty)
            || (payload.source_layer_uid.is_none()
                && (payload.mode.is_some() || payload.src_anchor.is_some()))
        {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let origin = self.current_snapshot();
        let plan = plan_time_link(
            origin.document(),
            &TimeLinkRequest {
                context_id: &payload.context_id,
                layer_uid: &payload.layer_uid,
                source: payload.source_layer_uid.as_deref(),
                mode: payload.mode.as_deref(),
                src_anchor: payload.src_anchor.as_deref(),
                frame: payload.frame,
            },
        );
        PreparedLayerLifecycle::seal(origin, LayerLifecycleKind::TimeLink, plan)
    }
    /// #2372: Follow path target set or clear (`targetLayerUid: null`); an optional `align` authors the orient toggle.
    pub(crate) fn prepare_layer_follow_path_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedLayerLifecycle, DispatchErrorCode> {
        let payload: FollowPathPayload = parse(bytes)?;
        if payload.command != "object.layer.followPath.set"
            || payload.context_id.is_empty()
            || payload.layer_uid.is_empty()
            || payload.target_layer_uid.as_deref() == Some("")
        {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let origin = self.current_snapshot();
        let plan = plan_follow_path(
            origin.document(),
            &payload.context_id,
            &payload.layer_uid,
            payload.target_layer_uid.as_deref(),
            payload.align,
        );
        PreparedLayerLifecycle::seal(origin, LayerLifecycleKind::FollowPath, plan)
    }
    /// #2371 / #2372: the in/out bar context menu (reset, align, distribute, flip, stagger) as one candidate document.
    pub(crate) fn prepare_layer_range_arrange_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedLayerLifecycle, DispatchErrorCode> {
        let origin = self.current_snapshot();
        let arranged = plan_arrange_json(&origin, bytes)?;
        let changed = arranged.document != *origin.document();
        if changed {
            ObjectSnapshot::validate_drawing(&arranged.document)
                .map_err(|_| DispatchErrorCode::Unavailable)?;
        }
        let plan = LifecyclePlan {
            document: arranged.document,
            changed,
            selection: None,
            removed: Vec::new(),
            touched: arranged.touched,
            spared: None,
            created: None,
        };
        Ok(PreparedLayerLifecycle {
            origin,
            plan,
            kind: LayerLifecycleKind::RangeArrange,
        })
    }
    pub(crate) fn commit_layer_lifecycle(
        &mut self,
        candidate: PreparedLayerLifecycle,
    ) -> Result<u64, DispatchErrorCode> {
        let current = self.current_snapshot();
        if candidate.origin.instance_id() != current.instance_id() {
            return Err(DispatchErrorCode::WrongInstance);
        }
        if candidate.origin.document_id() != current.document_id() {
            return Err(DispatchErrorCode::WrongDocument);
        }
        if candidate.origin.snapshot_id() != current.snapshot_id()
            || candidate.origin.content_revision() != current.content_revision()
        {
            return Err(DispatchErrorCode::StaleRevision);
        }
        if !candidate.plan.changed {
            return Ok(self.content_revision());
        }
        self.revisions
            .publish(candidate.plan.document)
            .map_err(|_| DispatchErrorCode::Unavailable)
    }
}

#[cfg(test)]
#[path = "../tests/object_layer_lifecycle_refusals.rs"]
mod object_layer_lifecycle_refusals;
