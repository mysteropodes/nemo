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
//! Lock and transform-parent decisions belong to Root F06, so the owner hands
//! the kernel the stand-in policy that answers "unavailable".
use crate::commands::ObjectFillOwner;
use crate::object_document::object_group_kernels::object_layer_lifecycle_plan::{
    plan_delete, plan_duplicate, plan_rename, plan_reorder, LayerSelection, LifecycleError,
    LifecyclePlan, SelectionOutcome, UnavailableLayerEditPolicy,
};
use crate::object_document::{object_deserialize, ObjectDocument};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;
use serde::Deserialize;
use serde_json::Number;
use std::collections::HashSet;

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
}
object_deserialize!(DeletePayload {
    command: String,
    selection: SelectionPayload
});

#[derive(Debug)]
struct DuplicatePayload {
    command: String,
    layer_uid: String,
}
object_deserialize!(DuplicatePayload {
    command: String,
    layer_uid: String
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
}
object_deserialize!(ReorderPayload {
    command: String,
    layer_uids: Vec<String>,
    gap: Number,
    selection: SelectionPayload
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
        | LifecycleError::NonFiniteGap
        | LifecycleError::InvalidColor(_)
        | LifecycleError::SourceUnnamed(_) => DispatchErrorCode::InvalidRequest,
        // Admission guarantees every context lists every layer.
        LifecycleError::OrderPlanIncomplete { .. } => DispatchErrorCode::Internal,
        LifecycleError::RevisionOutOfRange
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
    Reorder,
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

impl ObjectFillOwner {
    pub(crate) fn prepare_layer_delete_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedLayerLifecycle, DispatchErrorCode> {
        let payload: DeletePayload = parse(bytes)?;
        if payload.command != "object.layer.delete" {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let selection = payload.selection.into_selection()?;
        let origin = self.current_snapshot();
        let plan = plan_delete(origin.document(), &selection, &UnavailableLayerEditPolicy);
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
        let origin = self.current_snapshot();
        // The kernel bounds the embedded revision (2^53 - 1) like layer-create.
        let next = origin
            .content_revision()
            .checked_add(1)
            .ok_or(DispatchErrorCode::Unavailable)?;
        let plan = plan_duplicate(
            origin.document(),
            &payload.layer_uid,
            next,
            &UnavailableLayerEditPolicy,
        );
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
            &UnavailableLayerEditPolicy,
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
        let selection = payload.selection.into_selection()?;
        let origin = self.current_snapshot();
        // Targets are identities; a repeated one is a malformed set, not a
        // silently merged one.
        let mut seen: HashSet<&str> = HashSet::new();
        let mut moving: Vec<i64> = Vec::with_capacity(payload.layer_uids.len());
        for uid in &payload.layer_uids {
            if uid.is_empty() || !seen.insert(uid.as_str()) {
                return Err(DispatchErrorCode::InvalidRequest);
            }
            let slot = origin
                .document()
                .layers
                .iter()
                .position(|layer| layer.layer_uid == *uid)
                .ok_or(DispatchErrorCode::NotFound)?;
            moving.push(i64::try_from(slot).map_err(|_| DispatchErrorCode::InvalidRequest)?);
        }
        let plan = plan_reorder(
            origin.document(),
            &moving,
            gap,
            &selection,
            &UnavailableLayerEditPolicy,
        );
        PreparedLayerLifecycle::seal(origin, LayerLifecycleKind::Reorder, plan)
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
