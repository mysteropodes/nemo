//! One native undo checkpoint for a whole generate/regenerate/split/reassignment operation.
use crate::history::object_history::{NativeObjectHistory, ObjectHistoryEntry};
use crate::request_receipts::DispatchErrorCode;
use crate::transaction::CompactResult;
impl NativeObjectHistory {
    pub(crate) fn dispatch_tween_command(
        &mut self,
        operation: &str,
        bytes: &[u8],
    ) -> Result<CompactResult, (DispatchErrorCode, &'static str)> {
        let candidate = self
            .owner
            .prepare_tween_json(operation, bytes)
            .map_err(|code| {
                (
                    code,
                    "Tween payload or native producer is invalid or unavailable.",
                )
            })?;
        let before = self.content_revision();
        let after = self
            .owner
            .commit_tween(candidate)
            .map_err(|code| (code, "Tween publication refused."))?;
        if after != before {
            self.undo.push(ObjectHistoryEntry {
                before_revision: before,
                after_revision: after,
            });
            self.redo.clear();
        }
        Ok(CompactResult::Command {
            applied: after != before,
            history_entries_added: u64::from(after != before),
        })
    }
}
#[cfg(test)]
#[path = "../tests/object_tween_history.rs"]
mod tests;
