//! Sealed boolean transactions (G19 + G18, L02-I): the boolean tool, the eraser bite
//! and Flatten of one frame of one layer through the existing revision owner.
//! Declared as a private child of `object_group_ops.rs` (itself a child of
//! `object_fill_owner.rs`, which re-exports the candidate type), because the
//! owner file has no line of headroom under the size gate; it keeps access to the
//! owner's private state as a descendant, like the group and lifecycle preparations.
//!
//! Preparation parses a bounded, strict payload, reads the owner's current
//! immutable snapshot, asks the document-level plan (`object_boolean_ops_plan.rs`)
//! for a detached candidate, runs the storage admission an opened file would
//! meet and seals the candidate to that snapshot. Commit re-checks instance,
//! document and revision exactly like `commit_vector_edit`; a plan that changes
//! nothing publishes nothing. The plan's provider output is therefore admitted
//! only together with the pins of the snapshot it was computed from: a stale
//! pin, a typed refusal or a malformed ring publishes nothing and keeps redo.
use crate::commands::ObjectFillOwner;
use crate::object_document::object_group_kernels::object_boolean_ops::plan::{
    plan_boolean, BooleanPlan, BooleanRequest, ElementMotionSource, PlanError, UnavailableMotion,
};
use crate::object_document::object_group_kernels::object_boolean_ops::stack::Scope;
use crate::object_document::object_group_kernels::object_boolean_ops::{BoolOp, OpsError};
use crate::object_document::object_group_kernels::object_fill_trace_ops::regenerate::regenerate_many;
use crate::object_document::object_group_kernels::object_group_combine_plan::{
    CombineError, FlattenError,
};
use crate::object_document::object_group_kernels::object_group_plan::GroupPlanError;
use crate::object_document::ObjectDocument;
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;

#[path = "object_fill_trace_owner.rs"]
mod fill_trace;
#[path = "object_boolean_ops_payload.rs"]
mod wire;
use wire::{
    footprint_ok, parse, strokes_ok, target_ok, text_ok, ErasePayload, FlattenPayload, OpPayload,
};

/// How a typed plan refusal reaches the request-receipt vocabulary. Exhaustive:
/// a new plan variant fails the build instead of defaulting.
pub(crate) fn refusal_code(error: &PlanError) -> DispatchErrorCode {
    match error {
        PlanError::UnknownLayer(_)
        | PlanError::UnknownStroke(_)
        | PlanError::UnknownGroup(_)
        | PlanError::MemberAbsent(_) => DispatchErrorCode::NotFound,
        PlanError::Locked(_)
        | PlanError::Curved(_)
        | PlanError::Unfilled(_)
        | PlanError::OpenTarget(_)
        | PlanError::MotionUnavailable(_)
        | PlanError::HolderOrphan(_)
        | PlanError::IdUnavailable => DispatchErrorCode::Unavailable,
        PlanError::FillUnavailable => DispatchErrorCode::Unavailable,
        PlanError::InvalidScope
        | PlanError::FewerThanTwo(_)
        | PlanError::DuplicateOperand(_)
        | PlanError::Degenerate(_)
        | PlanError::EmptyResult
        | PlanError::TooManySegments { .. } => DispatchErrorCode::InvalidRequest,
        PlanError::TableUnreadable(_)
        | PlanError::Topology(_)
        | PlanError::Stack(_)
        | PlanError::Internal(_) => DispatchErrorCode::Internal,
        PlanError::Flatten(error) => match error {
            FlattenError::UnknownGroup(_) => DispatchErrorCode::NotFound,
            FlattenError::FewerThanTwoMembers(_) | FlattenError::MissingCombineMode(_) => {
                DispatchErrorCode::InvalidRequest
            }
        },
        PlanError::Combine(error) => match error {
            CombineError::FoldUnavailable => DispatchErrorCode::Unavailable,
            CombineError::FoldFailed(_) => DispatchErrorCode::InvalidRequest,
            CombineError::FactsMismatch { .. } | CombineError::NoStyleSource(_) => {
                DispatchErrorCode::Internal
            }
        },
        PlanError::Ops(error) => match error {
            OpsError::Implausible => DispatchErrorCode::Unavailable,
            OpsError::NoOperands
            | OpsError::NonFinite { .. }
            | OpsError::OutOfRange { .. }
            | OpsError::Degenerate
            | OpsError::ModeNone => DispatchErrorCode::InvalidRequest,
        },
        PlanError::Group(error) => match error {
            GroupPlanError::UnknownLayer(_) | GroupPlanError::UnknownStroke(_) => {
                DispatchErrorCode::NotFound
            }
            GroupPlanError::Locked(_) | GroupPlanError::ContextMismatch { .. } => {
                DispatchErrorCode::Unavailable
            }
            GroupPlanError::TableUnreadable(_) => DispatchErrorCode::Internal,
            GroupPlanError::IdCollidesWithStroke(_)
            | GroupPlanError::TooDeep
            | GroupPlanError::Kernel(_) => DispatchErrorCode::InvalidRequest,
        },
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BooleanEditKind {
    Op,
    Erase,
    Flatten,
    FillTrace,
}

/// A detached candidate sealed to the snapshot it was planned from. Only this
/// module can create one or replace its origin or after-state.
#[derive(Debug, Clone)]
pub(crate) struct PreparedBooleanEdit {
    origin: ObjectSnapshot,
    plan: BooleanPlan,
    kind: BooleanEditKind,
}

// Read by the typed in-process callers (tests today, the L02-J client port
// next); the compact receipt carries only applied/historyEntriesAdded.
#[allow(dead_code)]
impl PreparedBooleanEdit {
    pub(crate) fn kind(&self) -> BooleanEditKind {
        self.kind
    }
    /// `false`: nothing will be published or recorded.
    pub(crate) fn changed(&self) -> bool {
        self.plan.changed
    }
    /// Stroke ids of the new records in z-order; the first keeps the style source's id.
    pub(crate) fn created(&self) -> &[String] {
        &self.plan.created
    }
    /// Stroke ids whose record left the frame for good.
    pub(crate) fn removed(&self) -> &[String] {
        &self.plan.removed
    }
    pub(crate) fn document(&self) -> &ObjectDocument {
        &self.plan.document
    }
}

impl PreparedBooleanEdit {
    fn seal(
        origin: ObjectSnapshot,
        kind: BooleanEditKind,
        scope: &Scope,
        request: &BooleanRequest,
        motion: &dyn ElementMotionSource,
    ) -> Result<Self, DispatchErrorCode> {
        let mut plan = plan_boolean(
            origin.document(),
            scope,
            request,
            origin.content_revision(),
            motion,
        )
        .map_err(|error| refusal_code(&error))?;
        if plan.changed {
            let touched = match request {
                BooleanRequest::Op { strokes, .. } => strokes.clone(),
                BooleanRequest::Erase { stroke, .. } => vec![stroke.clone()],
                BooleanRequest::Flatten { .. } => {
                    plan.removed.iter().chain(&plan.created).cloned().collect()
                }
            };
            plan.document =
                regenerate_many(plan.document, scope, &touched, origin.content_revision())
                    .map_err(|error| refusal_code(&error))?;
            // The edited document must pass the admission a reopened file would. The plan already
            // refused every caller-caused overflow (`TooManySegments`), so a failure here is an
            // engine invariant breach, as in the group owner.
            ObjectSnapshot::validate_drawing(&plan.document)
                .map_err(|_| DispatchErrorCode::Internal)?;
        }
        Ok(Self { origin, plan, kind })
    }
}

fn scope_of(layer_uid: String, context_id: String, frame: u32) -> Scope {
    Scope {
        context_id,
        layer_uid,
        frame,
    }
}

impl ObjectFillOwner {
    pub(crate) fn prepare_boolean_op_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedBooleanEdit, DispatchErrorCode> {
        let payload: OpPayload = parse(bytes)?;
        let op = BoolOp::from_name(&payload.mode);
        if payload.command != "object.boolean.apply"
            || !target_ok(&payload.layer_uid, &payload.context_id)
            || !strokes_ok(&payload.stroke_ids)
        {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let op = op.ok_or(DispatchErrorCode::InvalidRequest)?;
        let origin = self.current_snapshot();
        PreparedBooleanEdit::seal(
            origin,
            BooleanEditKind::Op,
            &scope_of(payload.layer_uid, payload.context_id, payload.frame),
            &BooleanRequest::Op {
                op,
                strokes: payload.stroke_ids,
            },
            &UnavailableMotion,
        )
    }

    pub(crate) fn prepare_boolean_erase_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedBooleanEdit, DispatchErrorCode> {
        let payload: ErasePayload = parse(bytes)?;
        if payload.command != "object.boolean.erase"
            || !target_ok(&payload.layer_uid, &payload.context_id)
            || !text_ok(&payload.stroke_id)
            || !footprint_ok(&payload.footprint)
        {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let origin = self.current_snapshot();
        PreparedBooleanEdit::seal(
            origin,
            BooleanEditKind::Erase,
            &scope_of(payload.layer_uid, payload.context_id, payload.frame),
            &BooleanRequest::Erase {
                stroke: payload.stroke_id,
                footprint: payload.footprint,
            },
            &UnavailableMotion,
        )
    }

    /// Flatten with the production Motion port: a member animated by element Motion is
    /// refused as unavailable until the evaluator is connected.
    pub(crate) fn prepare_boolean_flatten_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedBooleanEdit, DispatchErrorCode> {
        self.prepare_boolean_flatten_with(bytes, &UnavailableMotion)
    }

    /// Flatten with a connected element-Motion evaluator (typed in-process caller).
    pub(crate) fn prepare_boolean_flatten_with(
        &self,
        bytes: &[u8],
        motion: &dyn ElementMotionSource,
    ) -> Result<PreparedBooleanEdit, DispatchErrorCode> {
        let payload: FlattenPayload = parse(bytes)?;
        if payload.command != "object.boolean.flatten"
            || !target_ok(&payload.layer_uid, &payload.context_id)
            || !text_ok(&payload.group_id)
        {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let origin = self.current_snapshot();
        PreparedBooleanEdit::seal(
            origin,
            BooleanEditKind::Flatten,
            &scope_of(payload.layer_uid, payload.context_id, payload.frame),
            &BooleanRequest::Flatten {
                group: payload.group_id,
            },
            motion,
        )
    }

    pub(crate) fn commit_boolean_edit(
        &mut self,
        candidate: PreparedBooleanEdit,
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
