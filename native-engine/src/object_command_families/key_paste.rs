//! Motion key paste family (#2524/#2377): copied keys land at the playhead in one native transaction.
use super::CommandFamily;
use crate::history::NativeObjectHistory;
use crate::request_receipts::DispatchErrorCode;
use crate::transaction::CompactResult;

const OP_KEYS_PASTE: &str = "command.document.object.property.keys.paste";

pub(crate) const FAMILY: CommandFamily = CommandFamily {
    name: "key_paste",
    owns: |operation| operation == OP_KEYS_PASTE,
    refusal: "Motion key paste request refused.",
    request_limit: 65_536,
    dispatch: |history, request| {
        let payload =
            serde_json::to_vec(&request.payload).map_err(|_| DispatchErrorCode::InvalidRequest)?;
        Ok(history.dispatch_property_keys_paste(&payload))
    },
};

impl NativeObjectHistory {
    /// The shared envelope, replay and revision checks already ran: prepare every track against the current
    /// snapshot, then commit the one sealed candidate. A paste that changes nothing adds no history entry.
    fn dispatch_property_keys_paste(
        &mut self,
        payload: &[u8],
    ) -> Result<CompactResult, (DispatchErrorCode, &'static str)> {
        let candidate = self
            .owner
            .prepare_property_keys_paste_json(payload)
            .map_err(|code| {
                (
                    code,
                    "Motion key paste payload or target is invalid or unavailable.",
                )
            })?;
        let before = self.content_revision();
        self.commit_property_edit(candidate)
            .map(|revision| CompactResult::Command {
                applied: revision != before,
                history_entries_added: u64::from(revision != before),
            })
            .map_err(|code| (code, "Motion key paste could not commit."))
    }
}

// #2524/#2377: copied Motion keys pasted at the playhead as one atomic native history entry.
#[cfg(test)]
#[path = "../../tests/object_property_keys_paste.rs"]
mod tests;
