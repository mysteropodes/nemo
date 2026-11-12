//! Labs family (U36, #2362): speed lines, boil, follow-path and reference fill, one operation and one native transaction each.
use super::{CommandFamily, FamilyOutcome, DEFAULT_REQUEST_BYTES};
use crate::commands::PreparedLabs;
use crate::history::object_history::ObjectHistoryEntry;
use crate::history::NativeObjectHistory;
use crate::transaction::CompactResult;

const OPS: [&str; 8] = [
    "command.document.object.labs.speedLines",
    "command.document.object.labs.boil",
    "command.document.object.labs.followPath",
    "command.document.object.labs.referenceFill",
    "command.document.object.labs.pingpong",
    "command.document.object.labs.retime",
    "command.document.object.labs.interval",
    "command.document.object.labs.moveToLayer",
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
    /// when the document changed (the reference fill through the boolean-edit commit, which records its own).
    fn dispatch_labs_generate(
        &mut self,
        operation: &str,
        payload: &serde_json::Value,
    ) -> FamilyOutcome {
        const REFUSED: &str = "Labs payload or target is invalid or unavailable.";
        const COMMIT: &str = "Labs operation could not commit.";
        let before = self.content_revision();
        let after = match self
            .owner
            .prepare_labs(operation, payload)
            .map_err(|code| (code, REFUSED))?
        {
            PreparedLabs::Fill(candidate) => self
                .commit_boolean_edit(candidate)
                .map_err(|code| (code, COMMIT))?,
            PreparedLabs::Generate(candidate) => {
                let after = self
                    .owner
                    .commit_labs_generate(candidate)
                    .map_err(|code| (code, COMMIT))?;
                if after != before {
                    self.undo.push(ObjectHistoryEntry {
                        before_revision: before,
                        after_revision: after,
                    });
                    self.redo.clear();
                }
                after
            }
        };
        Ok(CompactResult::Command {
            applied: after != before,
            history_entries_added: u64::from(after != before),
        })
    }
}

#[cfg(test)]
#[path = "../../tests/object_labs_reference_fill.rs"]
mod tests;
