//! Layer lifecycle operations are single transactions in the existing object
//! history (F07, L02-F): one successful delete / duplicate / rename / reorder
//! is one undo entry and one monotonic revision; a plan that changes nothing
//! leaves presence, bytes, undo and redo untouched.
use crate::commands::PreparedLayerLifecycle;
use crate::history::object_history::{NativeObjectHistory, ObjectHistoryEntry};
use crate::request_receipts::DispatchErrorCode;
use crate::transaction::CompactResult;

const OP_LAYER_DELETE: &str = "command.document.object.layer.delete";
const OP_LAYER_DUPLICATE: &str = "command.document.object.layer.duplicate";
const OP_LAYER_RENAME: &str = "command.document.object.layer.rename";
const OP_LAYER_REORDER: &str = "command.document.object.layer.reorder";
const OP_LAYER_PARENT: &str = "command.document.object.layer.parent.set";

impl NativeObjectHistory {
    pub(crate) fn prepare_layer_delete_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedLayerLifecycle, DispatchErrorCode> {
        self.owner.prepare_layer_delete_json(bytes)
    }
    pub(crate) fn prepare_layer_duplicate_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedLayerLifecycle, DispatchErrorCode> {
        self.owner.prepare_layer_duplicate_json(bytes)
    }
    pub(crate) fn prepare_layer_rename_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedLayerLifecycle, DispatchErrorCode> {
        self.owner.prepare_layer_rename_json(bytes)
    }
    pub(crate) fn prepare_layer_reorder_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedLayerLifecycle, DispatchErrorCode> {
        self.owner.prepare_layer_reorder_json(bytes)
    }
    pub(crate) fn prepare_layer_parent_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedLayerLifecycle, DispatchErrorCode> {
        self.owner.prepare_layer_parent_json(bytes)
    }
    pub(crate) fn commit_layer_lifecycle(
        &mut self,
        candidate: PreparedLayerLifecycle,
    ) -> Result<u64, DispatchErrorCode> {
        let before_revision = self.content_revision();
        let after_revision = self.owner.commit_layer_lifecycle(candidate)?;
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
    pub(crate) fn layer_lifecycle_operation(operation: &str) -> bool {
        matches!(
            operation,
            OP_LAYER_DELETE
                | OP_LAYER_DUPLICATE
                | OP_LAYER_RENAME
                | OP_LAYER_REORDER
                | OP_LAYER_PARENT
        )
    }
    /// Prepare and commit one lifecycle operation for the dispatcher, which has
    /// already checked the envelope, owner pins, cancellation, revision and
    /// request replay. A duplicate reports the identity it minted.
    pub(crate) fn dispatch_layer_lifecycle(
        &mut self,
        operation: &str,
        payload: &serde_json::Value,
    ) -> Result<CompactResult, (DispatchErrorCode, &'static str)> {
        let bytes = serde_json::to_vec(payload).map_err(|_| {
            (
                DispatchErrorCode::InvalidRequest,
                "Layer lifecycle payload is invalid.",
            )
        })?;
        let prepared = match operation {
            OP_LAYER_DELETE => self.prepare_layer_delete_json(&bytes),
            OP_LAYER_DUPLICATE => self.prepare_layer_duplicate_json(&bytes),
            OP_LAYER_RENAME => self.prepare_layer_rename_json(&bytes),
            OP_LAYER_REORDER => self.prepare_layer_reorder_json(&bytes),
            OP_LAYER_PARENT => self.prepare_layer_parent_json(&bytes),
            _ => Err(DispatchErrorCode::InvalidRequest),
        };
        let candidate = prepared.map_err(|code| {
            (
                code,
                "Layer lifecycle payload is invalid or target is absent.",
            )
        })?;
        let created = candidate.created().map(str::to_owned);
        let before = self.content_revision();
        let after = self
            .commit_layer_lifecycle(candidate)
            .map_err(|code| (code, "Layer lifecycle could not commit."))?;
        Ok(match created {
            Some(layer_uid) if after != before => CompactResult::LayerCreated { layer_uid },
            _ => CompactResult::Command {
                applied: after != before,
                history_entries_added: u64::from(after != before),
            },
        })
    }
}
