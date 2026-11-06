//! Property batch family (#2520/#2525): the Motion box over several layers or shapes is one native transaction.
use super::CommandFamily;
use crate::history::NativeObjectHistory;
use crate::request_receipts::DispatchErrorCode;
use crate::transaction::CompactResult;

const OP_PROPERTY_BATCH: &str = "command.document.object.property.batch";

pub(crate) const FAMILY: CommandFamily = CommandFamily {
    name: "property_batch",
    owns: |operation| operation == OP_PROPERTY_BATCH,
    refusal: "Motion property batch request refused.",
    request_limit: 65_536,
    dispatch: |history, request| {
        let payload =
            serde_json::to_vec(&request.payload).map_err(|_| DispatchErrorCode::InvalidRequest)?;
        Ok(history.dispatch_property_batch_edit(&payload))
    },
};

impl NativeObjectHistory {
    /// The shared envelope, replay and revision checks already ran: chain every entry over one working copy of the
    /// current snapshot, then commit the one sealed candidate. A batch that changes nothing adds no history entry.
    fn dispatch_property_batch_edit(
        &mut self,
        payload: &[u8],
    ) -> Result<CompactResult, (DispatchErrorCode, &'static str)> {
        let candidate = self
            .owner
            .prepare_property_batch_edit_json(payload)
            .map_err(|code| {
                (
                    code,
                    "Motion property batch payload or target is invalid or unavailable.",
                )
            })?;
        let before = self.content_revision();
        self.commit_property_edit(candidate)
            .map(|revision| CompactResult::Command {
                applied: revision != before,
                history_entries_added: u64::from(revision != before),
            })
            .map_err(|code| (code, "Motion property batch could not commit."))
    }
}

// #2520/#2525: a batch of Motion value edits (the multi-layer / multi-element box) is one atomic native history entry.
#[cfg(test)]
#[path = "../../tests/object_property_batch_edit.rs"]
mod property_batch_edit_tests;
