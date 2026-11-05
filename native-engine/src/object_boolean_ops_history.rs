//! Boolean edits are single transactions in the existing object history (G19 + G18,
//! L02-I): one successful boolean tool application, eraser bite or Flatten is one
//! undo entry and one monotonic revision, however many records it consumes and
//! creates (and whatever group membership it detaches); an edit that changes
//! nothing leaves presence, bytes, undo and redo untouched. One Undo restores the
//! original records, their order and their groups together.
use crate::commands::PreparedBooleanEdit;
use crate::history::object_history::{NativeObjectHistory, ObjectHistoryEntry};
use crate::object_document::object_group_kernels::object_boolean_ops::plan::ElementMotionSource;
use crate::request_receipts::DispatchErrorCode;
use crate::transaction::CompactResult;

const OP_BOOLEAN_APPLY: &str = "command.document.object.boolean.apply";
const OP_BOOLEAN_ERASE: &str = "command.document.object.boolean.erase";
const OP_BOOLEAN_FLATTEN: &str = "command.document.object.boolean.flatten";
const OP_FILL_TRACE: &str = "command.document.object.fill.trace";
const OP_FILL_BRUSH: &str = "command.document.object.fill.brush";
#[cfg(test)]
#[path = "../tests/object_fill_trace_history.rs"]
mod fill_trace_tests;
#[cfg(test)]
#[path = "../tests/object_fill_brush_history.rs"]
mod fill_brush_tests;
#[cfg(test)]
#[path = "../tests/object_boolean_ops_erase_flat.rs"]
mod erase_flat_tests;

impl NativeObjectHistory {
    pub(crate) fn prepare_fill_trace_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedBooleanEdit, DispatchErrorCode> {
        self.owner.prepare_fill_trace_json(bytes)
    }
    pub(crate) fn prepare_fill_brush_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedBooleanEdit, DispatchErrorCode> {
        self.owner.prepare_fill_brush_json(bytes)
    }
    pub(crate) fn prepare_boolean_op_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedBooleanEdit, DispatchErrorCode> {
        self.owner.prepare_boolean_op_json(bytes)
    }
    pub(crate) fn prepare_boolean_erase_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedBooleanEdit, DispatchErrorCode> {
        self.owner.prepare_boolean_erase_json(bytes)
    }
    pub(crate) fn prepare_boolean_flatten_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedBooleanEdit, DispatchErrorCode> {
        self.owner.prepare_boolean_flatten_json(bytes)
    }
    /// Flatten with a connected element-Motion evaluator (typed in-process caller).
    #[allow(dead_code)]
    pub(crate) fn prepare_boolean_flatten_with(
        &self,
        bytes: &[u8],
        motion: &dyn ElementMotionSource,
    ) -> Result<PreparedBooleanEdit, DispatchErrorCode> {
        self.owner.prepare_boolean_flatten_with(bytes, motion)
    }
    pub(crate) fn commit_boolean_edit(
        &mut self,
        candidate: PreparedBooleanEdit,
    ) -> Result<u64, DispatchErrorCode> {
        let before_revision = self.content_revision();
        let after_revision = self.owner.commit_boolean_edit(candidate)?;
        if after_revision != before_revision {
            self.undo.push(ObjectHistoryEntry {
                before_revision,
                after_revision,
            });
            self.redo.clear();
        }
        Ok(after_revision)
    }
    /// An eraser bite that continues the gesture whose entry is on top of the undo stack: the revision advances, but the
    /// entry grows to end at it instead of a second one being pushed (the baseline pushes one undo per drag). Only an entry
    /// that was left by an eraser bite and ends at the current revision can be continued - an undo, a redo or any other edit in between breaks the chain.
    fn joinable(&self) -> bool {
        self.undo.last().is_some_and(|top| {
            top.after_revision == self.content_revision()
                && self.erase_tail == Some(top.after_revision)
        })
    }
    fn commit_boolean_joined(
        &mut self,
        candidate: PreparedBooleanEdit,
    ) -> Result<u64, DispatchErrorCode> {
        let before_revision = self.content_revision();
        let after_revision = self.owner.commit_boolean_edit(candidate)?;
        if after_revision != before_revision {
            if let Some(top) = self.undo.last_mut() {
                let old = top.after_revision;
                top.after_revision = after_revision;
                self.labels.rekey(old, after_revision);
            }
            self.redo.clear();
        }
        Ok(after_revision)
    }
    /// Operations the staged dispatcher routes here.
    pub(crate) fn boolean_operation(operation: &str) -> bool {
        matches!(
            operation,
            OP_BOOLEAN_APPLY | OP_BOOLEAN_ERASE | OP_BOOLEAN_FLATTEN | OP_FILL_TRACE | OP_FILL_BRUSH
        )
    }
    /// Prepare and commit one boolean edit for the dispatcher, which has already
    /// checked the envelope, owner pins, cancellation, revision and request
    /// replay. The receipt is the compact command result.
    pub(crate) fn dispatch_boolean(
        &mut self,
        operation: &str,
        payload: &serde_json::Value,
    ) -> Result<CompactResult, (DispatchErrorCode, &'static str)> {
        let bytes = serde_json::to_vec(payload).map_err(|_| {
            (
                DispatchErrorCode::InvalidRequest,
                "Boolean payload is invalid.",
            )
        })?;
        let prepared = match operation {
            OP_BOOLEAN_APPLY => self.prepare_boolean_op_json(&bytes),
            OP_BOOLEAN_ERASE => self.prepare_boolean_erase_json(&bytes),
            OP_BOOLEAN_FLATTEN => self.prepare_boolean_flatten_json(&bytes),
            OP_FILL_TRACE => self.prepare_fill_trace_json(&bytes),
            OP_FILL_BRUSH => self.prepare_fill_brush_json(&bytes),
            _ => Err(DispatchErrorCode::InvalidRequest),
        };
        let candidate =
            prepared.map_err(|code| (code, "Boolean payload is invalid or target is absent."))?;
        // `joinHistory` (eraser bites after the first of a drag): refused when there is no entry to continue, never
        // silently turned into a second undo step.
        let join = operation == OP_BOOLEAN_ERASE
            && payload.get("joinHistory").and_then(serde_json::Value::as_bool) == Some(true);
        if join && !self.joinable() {
            return Err((
                DispatchErrorCode::Unavailable,
                "There is no eraser entry to continue.",
            ));
        }
        let before = self.content_revision();
        let after = if join {
            self.commit_boolean_joined(candidate)
        } else {
            self.commit_boolean_edit(candidate)
        }
        .map_err(|code| (code, "Boolean edit could not commit."))?;
        if operation == OP_BOOLEAN_ERASE && after != before {
            self.erase_tail = Some(after);
        }
        Ok(CompactResult::Command {
            applied: after != before,
            history_entries_added: u64::from(after != before && !join),
        })
    }
}
