//! Labs generator family (U36, #2362): speed lines, boil and follow-path, one operation and one native transaction each.
use super::{CommandFamily, FamilyOutcome, DEFAULT_REQUEST_BYTES};
use crate::history::object_history::ObjectHistoryEntry;
use crate::history::NativeObjectHistory;
use crate::transaction::CompactResult;

const OPS: [&str; 3] = [
    "command.document.object.labs.speedLines",
    "command.document.object.labs.boil",
    "command.document.object.labs.followPath",
];

pub(crate) const FAMILY: CommandFamily = CommandFamily {
    name: "labs",
    owns: |operation| OPS.contains(&operation),
    refusal: "Labs generation request refused.",
    request_limit: DEFAULT_REQUEST_BYTES,
    dispatch: |history, request| {
        Ok(history.dispatch_labs_generate(&request.operation, &request.payload))
    },
};

impl NativeObjectHistory {
    /// The shared envelope, replay and revision checks already ran: prepare, commit, and record one history entry
    /// when the document changed.
    fn dispatch_labs_generate(
        &mut self,
        operation: &str,
        payload: &serde_json::Value,
    ) -> FamilyOutcome {
        let candidate = self
            .owner
            .prepare_labs_generate(operation, payload)
            .map_err(|code| {
                (
                    code,
                    "Labs generation payload or target is invalid or unavailable.",
                )
            })?;
        let before = self.content_revision();
        let after = self
            .owner
            .commit_labs_generate(candidate)
            .map_err(|code| (code, "Labs generation could not commit."))?;
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
