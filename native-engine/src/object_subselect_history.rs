//! Native subselect uses the existing sealed vector transaction and history publication.
use crate::history::object_history::{NativeObjectHistory, ObjectHistoryEntry};
use crate::object_document::subselect::{record, Intent, Refusal};
use crate::object_document::{object_deserialize, ObjectTarget, Presence};
use crate::request_receipts::DispatchErrorCode;
use crate::transaction::CompactResult;
use serde::Deserialize;
use serde_json::json;

struct Payload {
    command: String,
    stable_target: ObjectTarget,
    intent: Intent,
    at_frame: Presence<u32>,
}
object_deserialize!(Payload {
    command: String,
    stable_target: ObjectTarget,
    intent: Intent,
    #[serde(default)] at_frame: Presence<u32>
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
        let mut promote = false;
        let mut next = record::admit(snapshot.document(), stored)
            .and_then(|_| {
                if let Some(frame) = payload.at_frame.value() {
                    promote = record::at_frame(snapshot.document(), stored, *frame)?;
                }
                record::plan(stored, &payload.intent)
            })
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
        // The old controller ensures a key even for an identity click. Reuse the
        // frame owner's genuine promotion transaction, never manufacture a changed
        // coordinate to make the vector batch report a change.
        if promote && next == *stored {
            let unchanged =
                serde_json::to_vec(&json!({"command":"object.vector.replace","record":next}))
                    .map_err(|_| invalid)?;
            self.owner
                .prepare_vector_edit_json(&unchanged)
                .map_err(|c| (c, "Subselect drawing policy refused."))?;
            return self.dispatch_exposure_key("command.document.object.layer.key.promote",&json!({"command":"object.layer.key.promote","layerUids":[stored.target.layer_uid],"cells":[payload.at_frame.value().unwrap()]}));
        }
        let vector = serde_json::to_vec(&if promote {
            next.target.frame_scope.frame=*payload.at_frame.value().unwrap();
            json!({"command":"object.vector.batch","keyframe":"copy","ops":[{"op":"replace","record":next}]})
        } else {json!({"command":"object.vector.replace","record":next})})
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

#[cfg(test)]
#[path = "../tests/object_subselect_held.rs"]
mod held_tests;
