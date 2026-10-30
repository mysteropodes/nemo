//! Motion preset family (G39, #2376): one native transaction replaces the transform tracks of a layer.
use super::CommandFamily;
use crate::history::NativeObjectHistory;
use crate::request_receipts::DispatchErrorCode;
use crate::transaction::CompactResult;

const OP_PRESET_APPLY: &str = "command.document.object.property.preset.apply";

pub(crate) const FAMILY: CommandFamily = CommandFamily {
    name: "preset",
    owns: |operation| operation == OP_PRESET_APPLY,
    refusal: "Motion preset request refused.",
    request_limit: 65_536,
    dispatch: |history, request| {
        let payload =
            serde_json::to_vec(&request.payload).map_err(|_| DispatchErrorCode::InvalidRequest)?;
        Ok(history.dispatch_property_preset_apply(&payload))
    },
};

impl NativeObjectHistory {
    /// The shared envelope, replay and revision checks already ran: prepare every property against the
    /// current snapshot, then commit the one sealed candidate. An exact no-op adds no history entry.
    fn dispatch_property_preset_apply(
        &mut self,
        payload: &[u8],
    ) -> Result<CompactResult, (DispatchErrorCode, &'static str)> {
        let candidate = self
            .owner
            .prepare_property_preset_apply_json(payload)
            .map_err(|code| {
                (
                    code,
                    "Motion preset payload or target is invalid or unavailable.",
                )
            })?;
        let before = self.content_revision();
        self.commit_property_edit(candidate)
            .map(|revision| CompactResult::Command {
                applied: revision != before,
                history_entries_added: u64::from(revision != before),
            })
            .map_err(|code| (code, "Motion preset could not commit."))
    }
}
