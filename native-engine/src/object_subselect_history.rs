//! Native subselect uses the existing sealed vector transaction and history publication.
use crate::history::object_history::{NativeObjectHistory, ObjectHistoryEntry};
use crate::object_document::subselect::{record, Intent, Refusal};
use crate::object_document::{object_deserialize, ObjectTarget};
use crate::request_receipts::DispatchErrorCode;
use crate::transaction::CompactResult;
use serde::Deserialize;
use serde_json::json;

struct Payload {
    command: String,
    stable_target: ObjectTarget,
    intent: Intent,
}
object_deserialize!(Payload {
    command: String,
    stable_target: ObjectTarget,
    intent: Intent
});
impl NativeObjectHistory {
    pub(crate) fn dispatch_subselect_command(
        &mut self,
        bytes: &[u8],
    ) -> Result<CompactResult, (DispatchErrorCode, &'static str)> {
        let invalid = (
            DispatchErrorCode::InvalidRequest,
            "Invalid subselect intent.",
        );
        if bytes.len() > 4096 {
            return Err(invalid);
        }
        let payload: Payload = serde_json::from_slice(bytes).map_err(|_| invalid)?;
        if payload.command != "object.subselect.edit" {
            return Err(invalid);
        }
        let snapshot = self.acquire_snapshot(self.content_revision()).ok_or((
            DispatchErrorCode::Unavailable,
            "Subselect snapshot is unavailable.",
        ))?;
        let stored = snapshot
            .document()
            .objects
            .iter()
            .find(|r| r.target == payload.stable_target)
            .ok_or((DispatchErrorCode::NotFound, "Subselect target is absent."))?;
        let next = record::admit(snapshot.document(), stored)
            .and_then(|_| record::plan(stored, &payload.intent))
            .map_err(|r| match r {
                Refusal::Invalid => invalid,
                Refusal::Limit => (
                    DispatchErrorCode::Unavailable,
                    "Subselect geometry exceeds the admitted bound.",
                ),
                Refusal::Unsupported => (
                    DispatchErrorCode::Unavailable,
                    "Subselect target requires an unavailable geometry consumer.",
                ),
            })?;
        let vector = serde_json::to_vec(&json!({"command":"object.vector.replace","record":next}))
            .map_err(|_| invalid)?;
        let candidate = self
            .owner
            .prepare_vector_edit_json(&vector)
            .map_err(|c| (c, "Subselect vector preparation refused."))?;
        let before = self.content_revision();
        let after = self
            .owner
            .commit_vector_edit(candidate)
            .map_err(|c| (c, "Subselect vector publication refused."))?;
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
#[path = "../tests/object_subselect_history.rs"]
mod tests;
