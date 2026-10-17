//! Layer compositing edits are single transactions in the existing object history (G11, L08-J): one
//! successful edit is one undo entry and one monotonic revision; an edit that changes nothing leaves
//! presence, bytes, undo and redo untouched; creating an effect or folder layer is the ordinary layer-create
//! history entry. No second history, receipt or id state: this file only wires the sealed owner transaction
//! (`scene_effect_layer_edit.rs`) to `NativeObjectHistory` and to the staged command dispatcher.
use crate::commands::PreparedLayerCompositing;
use crate::history::object_history::{NativeObjectHistory, ObjectHistoryEntry};
use crate::request_receipts::DispatchErrorCode;
use crate::transaction::CompactResult;

const OP_LAYER_COMPOSITING: &str = "command.document.object.layer.compositing.edit";
const OP_LAYER_COMPOSITING_CREATE: &str = "command.document.object.layer.compositing.create";

impl NativeObjectHistory {
    pub(crate) fn prepare_layer_compositing_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedLayerCompositing, DispatchErrorCode> {
        self.owner.prepare_layer_compositing_json(bytes)
    }
    pub(crate) fn commit_layer_compositing(
        &mut self,
        candidate: PreparedLayerCompositing,
    ) -> Result<u64, DispatchErrorCode> {
        let before_revision = self.content_revision();
        let after_revision = self.owner.commit_layer_compositing(candidate)?;
        if after_revision != before_revision {
            self.undo.push(ObjectHistoryEntry {
                before_revision,
                after_revision,
            });
            self.redo.clear();
        }
        Ok(after_revision)
    }
    /// Operations the staged dispatcher routes here.
    pub(crate) fn compositing_operation(operation: &str) -> bool {
        matches!(
            operation,
            OP_LAYER_COMPOSITING | OP_LAYER_COMPOSITING_CREATE
        )
    }
    /// Prepare and commit one compositing request for the dispatcher, which has already checked the
    /// envelope, owner pins, cancellation, revision and request replay. The receipt is the compact result.
    pub(crate) fn dispatch_compositing(
        &mut self,
        operation: &str,
        payload: &serde_json::Value,
    ) -> Result<CompactResult, (DispatchErrorCode, &'static str)> {
        const REFUSED: &str = "Layer compositing payload or target is invalid or unavailable.";
        let bytes = serde_json::to_vec(payload).map_err(|_| {
            (
                DispatchErrorCode::InvalidRequest,
                "Layer compositing payload is invalid.",
            )
        })?;
        if operation == OP_LAYER_COMPOSITING_CREATE {
            let candidate = self
                .owner
                .prepare_layer_compositing_create_json(&bytes)
                .map_err(|code| (code, REFUSED))?;
            let layer_uid = candidate.layer_uid().to_owned();
            return self
                .commit_layer_create(candidate)
                .map(|_| CompactResult::LayerCreated { layer_uid })
                .map_err(|code| (code, "Layer compositing creation could not commit."));
        }
        let candidate = self
            .prepare_layer_compositing_json(&bytes)
            .map_err(|code| (code, REFUSED))?;
        let before = self.content_revision();
        let after = self
            .commit_layer_compositing(candidate)
            .map_err(|code| (code, "Layer compositing edit could not commit."))?;
        Ok(CompactResult::Command {
            applied: after != before,
            history_entries_added: u64::from(after != before),
        })
    }
}

#[cfg(all(test, feature = "history"))]
#[path = "../tests/scene_effect_layer_record_edit.rs"]
mod scene_effect_layer_record_edit;
#[cfg(all(test, feature = "history"))]
#[path = "../tests/scene_effect_layer_record_edit_rows.rs"]
mod scene_effect_layer_record_edit_rows;
