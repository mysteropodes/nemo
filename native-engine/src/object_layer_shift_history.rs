//! The original layer-bar action owns exactly one checkpoint in the existing history.
//! Public dispatch reuses the retained request queue and preflights the full reply before publication.
use crate::commands::PreparedLayerShift;
use crate::history::object_history::{NativeObjectHistory, ObjectHistoryEntry};
use crate::request_receipts::{DispatchErrorCode, OpacityRequest};
use crate::transaction::{CompactResult, RetainedDisposition};

const OP: &str = "command.document.object.layer.range.drag";
pub(crate) fn is_layer_shift_operation(operation: &str) -> bool {
    operation == OP
}

impl NativeObjectHistory {
    pub(crate) fn prepare_layer_range_drag_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedLayerShift, DispatchErrorCode> {
        self.owner.prepare_layer_shift_json(bytes)
    }
    pub(crate) fn commit_layer_range_drag(
        &mut self,
        candidate: PreparedLayerShift,
    ) -> Result<u64, DispatchErrorCode> {
        if candidate.command() != "object.layer.range.drag" || !candidate.checkpoint_required() {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let before_revision = self.content_revision();
        let after_revision = self.owner.commit_layer_shift(candidate)?;
        self.undo.push(ObjectHistoryEntry {
            before_revision,
            after_revision,
        });
        self.redo.clear();
        Ok(after_revision)
    }
}

impl NativeObjectHistory {
    pub(crate) fn dispatch_layer_shift(
        &mut self,
        request: &OpacityRequest,
    ) -> Result<CompactResult, (DispatchErrorCode, &'static str)> {
        if !is_layer_shift_operation(&request.operation) {
            return Err((
                DispatchErrorCode::InvalidRequest,
                "Layer range drag operation is unknown.",
            ));
        }
        let payload = serde_json::to_vec(&request.payload).map_err(|_| {
            (
                DispatchErrorCode::InvalidRequest,
                "Layer range drag payload is invalid.",
            )
        })?;
        let candidate = self
            .prepare_layer_range_drag_json(&payload)
            .map_err(|code| {
                (
                    code,
                    "Layer range drag target or gesture is invalid or unavailable.",
                )
            })?;
        let result = CompactResult::Command {
            applied: true,
            history_entries_added: 1,
        };
        let revision = self.content_revision().checked_add(1).ok_or((
            DispatchErrorCode::Unavailable,
            "Layer range drag revision is exhausted.",
        ))?;
        let prospective = RetainedDisposition::Exact {
            content_revision: revision,
            outcome: Ok(result.clone()),
        };
        // Correlation IDs are part of this complete bound. No mutation, undo or redo clearing before it fits.
        self.object_reply(request, &prospective).map_err(|_| {
            (
                DispatchErrorCode::InvalidRequest,
                "Complete layer range drag reply exceeds its bound.",
            )
        })?;
        self.commit_layer_range_drag(candidate)
            .map_err(|code| (code, "Layer range drag could not commit."))?;
        Ok(result)
    }
}
#[cfg(test)]
#[path = "../tests/object_layer_shift_history.rs"]
mod tests;
