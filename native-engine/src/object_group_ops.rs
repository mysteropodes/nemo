//! Sealed group transactions (G24, L02-G): group, ungroup, combine, combine-mode,
//! rename and exit-group of the per-layer group table through the existing revision owner.
//! Declared as a private child of `object_fill_owner.rs`, next to the layer
//! lifecycle / property / settings preparations it mirrors.
//!
//! Preparation parses a bounded, strict payload (object-only, duplicate and
//! unknown members refused), reads the owner's current immutable snapshot, asks
//! the document-layer plan (`object_group_plan.rs`, the L02-A kernel over the
//! stored table) for a detached candidate, runs the same storage admission
//! layer-create runs and seals the result to that snapshot. Commit re-checks
//! instance, document and revision exactly like `commit_layer_create`; a plan
//! that changes nothing publishes nothing. No history, receipt or id state
//! lives here: group ids come from the caller (the baseline minted them on the
//! client too) and the one history entry from `NativeObjectHistory`.
use crate::commands::ObjectFillOwner;
use crate::object_document::object_group_kernels::object_group_hierarchy::{
    CombineMode, GroupError,
};
use crate::object_document::object_group_kernels::object_group_plan::{
    plan_group_edit, GroupEdit, GroupPlan, GroupPlanError,
};
use crate::object_document::ObjectDocument;
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;

#[path = "object_group_payload.rs"]
mod wire;
use wire::{
    parse, strokes_ok, target_ok, text_ok, CombinePayload, ExitPayload, GroupPayload, ModePayload,
    RenamePayload, UngroupPayload, MAX_ID_BYTES, MAX_NAME_BYTES,
};

// L02-I (#2108): boolean tool / eraser / Flatten owner transactions. Declared here, not in
// object_fill_owner.rs (498 of 500 non-blank lines); still a descendant of that module.
#[path = "object_boolean_ops_owner.rs"]
mod boolean_edit;
pub(crate) use boolean_edit::PreparedBooleanEdit;

/// How a typed plan refusal reaches the request-receipt vocabulary. Exhaustive:
/// a new kernel variant fails the build instead of defaulting.
pub(crate) fn refusal_code(error: &GroupPlanError) -> DispatchErrorCode {
    match error {
        GroupPlanError::UnknownLayer(_) | GroupPlanError::UnknownStroke(_) => {
            DispatchErrorCode::NotFound
        }
        GroupPlanError::Locked(_) | GroupPlanError::ContextMismatch { .. } => {
            DispatchErrorCode::Unavailable
        }
        GroupPlanError::IdCollidesWithStroke(_) | GroupPlanError::TooDeep => {
            DispatchErrorCode::InvalidRequest
        }
        GroupPlanError::TableUnreadable(_) => DispatchErrorCode::Internal,
        GroupPlanError::Kernel(kernel) => match kernel {
            GroupError::UnknownGroup(_) | GroupError::UnknownItem(_) => DispatchErrorCode::NotFound,
            GroupError::EmptyGroupId
            | GroupError::DuplicateGroupId(_)
            | GroupError::EmptyStrokeId(_)
            | GroupError::DuplicateSelection(_)
            | GroupError::FewerThanTwoUnits(_)
            | GroupError::EmptySelection
            | GroupError::NoGroupInSelection
            | GroupError::NotInGroup(_) => DispatchErrorCode::InvalidRequest,
        },
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GroupEditKind {
    Group,
    Ungroup,
    Combine,
    SetMode,
    Rename,
    Exit,
}

/// A detached candidate sealed to the snapshot it was planned from. Only this
/// module can create one or replace its origin or after-state.
#[derive(Debug, Clone)]
pub(crate) struct PreparedGroupEdit {
    origin: ObjectSnapshot,
    plan: GroupPlan,
    kind: GroupEditKind,
}

// Read by the typed in-process callers (tests today, the L02-J client port
// next); the compact receipt carries only applied/historyEntriesAdded.
#[allow(dead_code)]
impl PreparedGroupEdit {
    pub(crate) fn kind(&self) -> GroupEditKind {
        self.kind
    }
    /// `false`: nothing will be published or recorded.
    pub(crate) fn changed(&self) -> bool {
        self.plan.changed
    }
    /// The group an edit created (group, or combine that made a flat group).
    pub(crate) fn created(&self) -> Option<&str> {
        self.plan.created.as_deref()
    }
    /// Origin groups that no longer exist, in table order.
    pub(crate) fn removed(&self) -> &[String] {
        &self.plan.removed
    }
    pub(crate) fn document(&self) -> &ObjectDocument {
        &self.plan.document
    }
}

impl PreparedGroupEdit {
    fn seal(
        origin: ObjectSnapshot,
        kind: GroupEditKind,
        layer_uid: &str,
        context_id: &str,
        edit: GroupEdit,
    ) -> Result<Self, DispatchErrorCode> {
        let plan = plan_group_edit(origin.document(), layer_uid, context_id, &edit)
            .map_err(|error| refusal_code(&error))?;
        if plan.changed {
            // The edited table must pass the admission a reopened file would.
            ObjectSnapshot::validate_drawing(&plan.document)
                .map_err(|_| DispatchErrorCode::Internal)?;
        }
        Ok(Self { origin, plan, kind })
    }
}

impl ObjectFillOwner {
    pub(crate) fn prepare_group_create_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedGroupEdit, DispatchErrorCode> {
        let payload: GroupPayload = parse(bytes)?;
        if payload.command != "object.group.create"
            || !target_ok(&payload.layer_uid, &payload.context_id)
            || !text_ok(&payload.group_id, MAX_ID_BYTES)
            || !text_ok(&payload.name, MAX_NAME_BYTES)
            || !strokes_ok(&payload.stroke_ids)
        {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let origin = self.current_snapshot();
        PreparedGroupEdit::seal(
            origin,
            GroupEditKind::Group,
            &payload.layer_uid,
            &payload.context_id,
            GroupEdit::Group {
                gid: payload.group_id,
                name: payload.name,
                strokes: payload.stroke_ids,
            },
        )
    }
    pub(crate) fn prepare_group_ungroup_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedGroupEdit, DispatchErrorCode> {
        let payload: UngroupPayload = parse(bytes)?;
        if payload.command != "object.group.ungroup"
            || !target_ok(&payload.layer_uid, &payload.context_id)
            || !strokes_ok(&payload.stroke_ids)
        {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let origin = self.current_snapshot();
        PreparedGroupEdit::seal(
            origin,
            GroupEditKind::Ungroup,
            &payload.layer_uid,
            &payload.context_id,
            GroupEdit::Ungroup {
                strokes: payload.stroke_ids,
            },
        )
    }
    pub(crate) fn prepare_group_combine_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedGroupEdit, DispatchErrorCode> {
        let payload: CombinePayload = parse(bytes)?;
        let mode = CombineMode::parse(&payload.mode);
        if payload.command != "object.group.combine"
            || !target_ok(&payload.layer_uid, &payload.context_id)
            || !text_ok(&payload.group_id, MAX_ID_BYTES)
            || !strokes_ok(&payload.stroke_ids)
        {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let mode = mode.ok_or(DispatchErrorCode::InvalidRequest)?;
        let origin = self.current_snapshot();
        PreparedGroupEdit::seal(
            origin,
            GroupEditKind::Combine,
            &payload.layer_uid,
            &payload.context_id,
            GroupEdit::Combine {
                gid: payload.group_id,
                mode,
                strokes: payload.stroke_ids,
            },
        )
    }
    pub(crate) fn prepare_group_mode_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedGroupEdit, DispatchErrorCode> {
        let payload: ModePayload = parse(bytes)?;
        let mode = CombineMode::parse(&payload.mode);
        if payload.command != "object.group.mode.set"
            || !target_ok(&payload.layer_uid, &payload.context_id)
            || !text_ok(&payload.group_id, MAX_ID_BYTES)
        {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let mode = mode.ok_or(DispatchErrorCode::InvalidRequest)?;
        let origin = self.current_snapshot();
        PreparedGroupEdit::seal(
            origin,
            GroupEditKind::SetMode,
            &payload.layer_uid,
            &payload.context_id,
            GroupEdit::SetMode {
                gid: payload.group_id,
                mode,
            },
        )
    }
    pub(crate) fn prepare_group_rename_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedGroupEdit, DispatchErrorCode> {
        let payload: RenamePayload = parse(bytes)?;
        if payload.command != "object.group.rename"
            || !target_ok(&payload.layer_uid, &payload.context_id)
            || !text_ok(&payload.group_id, MAX_ID_BYTES)
            || !text_ok(&payload.name, MAX_NAME_BYTES)
        {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let origin = self.current_snapshot();
        PreparedGroupEdit::seal(
            origin,
            GroupEditKind::Rename,
            &payload.layer_uid,
            &payload.context_id,
            GroupEdit::Rename {
                gid: payload.group_id,
                name: payload.name,
            },
        )
    }
    pub(crate) fn prepare_group_exit_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedGroupEdit, DispatchErrorCode> {
        let payload: ExitPayload = parse(bytes)?;
        if payload.command != "object.group.member.remove"
            || !target_ok(&payload.layer_uid, &payload.context_id)
            || !text_ok(&payload.stroke_id, MAX_ID_BYTES)
        {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let origin = self.current_snapshot();
        PreparedGroupEdit::seal(
            origin,
            GroupEditKind::Exit,
            &payload.layer_uid,
            &payload.context_id,
            GroupEdit::ExitGroup {
                stroke: payload.stroke_id,
            },
        )
    }
    pub(crate) fn commit_group_edit(
        &mut self,
        candidate: PreparedGroupEdit,
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
#[path = "../tests/object_group_ops_refusals.rs"]
mod object_group_ops_refusals;
