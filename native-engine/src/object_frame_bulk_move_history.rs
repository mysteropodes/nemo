//! Ordinary whole-cell Move uses the existing native revision/history/receipt owner.
//! Shift stays private until its caller can commit Motion, range and Arc atomically.
use crate::commands::PreparedBulkFrameEdit;
use crate::history::object_history::{NativeObjectHistory, ObjectHistoryEntry};
use crate::request_receipts::{DispatchErrorCode, OpacityRequest};
use crate::transaction::{CompactResult, RetainedDisposition};

const OP_MOVE: &str = "command.document.object.frames.move";
pub(crate) fn is_frame_bulk_move_operation(operation: &str) -> bool {
    operation == OP_MOVE
}

impl NativeObjectHistory {
    pub(crate) fn prepare_frame_move_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedBulkFrameEdit, DispatchErrorCode> {
        let candidate = self.owner.prepare_bulk_frame_edit_json(bytes)?;
        if candidate.command() != "object.frames.move" {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        Ok(candidate)
    }
    /// Eligible Move checkpoints even if ripple clamping leaves identical bytes.
    /// Original early no-ops publish nothing and preserve both history stacks.
    pub(crate) fn commit_frame_move(
        &mut self,
        candidate: PreparedBulkFrameEdit,
    ) -> Result<u64, DispatchErrorCode> {
        if candidate.command() != "object.frames.move" {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let before_revision = self.content_revision();
        let after_revision = self.owner.commit_bulk_frame_edit(candidate)?;
        if after_revision != before_revision {
            self.undo.push(ObjectHistoryEntry {
                before_revision,
                after_revision,
            });
            self.redo.clear();
        }
        Ok(after_revision)
    }
    pub(crate) fn dispatch_frame_bulk_move(
        &mut self,
        request: &OpacityRequest,
    ) -> Result<CompactResult, (DispatchErrorCode, &'static str)> {
        let invalid = |message| (DispatchErrorCode::InvalidRequest, message);
        if !is_frame_bulk_move_operation(&request.operation) {
            return Err(invalid("Frame move operation is unknown."));
        }
        let bytes = serde_json::to_vec(&request.payload)
            .map_err(|_| invalid("Frame move payload is invalid."))?;
        let candidate = self.prepare_frame_move_json(&bytes).map_err(|code| {
            (
                code,
                "Frame move payload or target is invalid or unavailable.",
            )
        })?;
        let applied = candidate.checkpoint_required() || candidate.document_changed();
        // The actual admitted targets include source/destination lock filtering, crop and ripple clamp.
        // Retain them before consuming the sealed candidate; never reconstruct from the request.
        let selection = serde_json::to_value(candidate.selection())
            .map_err(|_| invalid("Frame move result is invalid."))?;
        if applied == selection.is_null() {
            return Err((
                DispatchErrorCode::Internal,
                "Frame move result is inconsistent.",
            ));
        }
        let result = CompactResult::FrameBulkMove {
            applied,
            history_entries_added: u64::from(applied),
            selection,
        };
        let revision = self
            .content_revision()
            .checked_add(u64::from(applied))
            .ok_or((
                DispatchErrorCode::Unavailable,
                "Frame move revision is exhausted.",
            ))?;
        let prospective = RetainedDisposition::Exact {
            content_revision: revision,
            outcome: Ok(result.clone()),
        };
        // Complete correlated success must fit before any publication/history mutation. No truncation.
        self.object_reply(request, &prospective)
            .map_err(|_| invalid("Frame move result exceeds the bounded response."))?;
        self.commit_frame_move(candidate)
            .map_err(|code| (code, "Frame move could not commit."))?;
        Ok(result)
    }
}
#[cfg(test)]
#[path = "../tests/object_frame_bulk_move_history.rs"]
mod tests;
