//! Selection edits and clipboard operations are single transactions in the
//! existing object history (F08/F15, L02-H): one successful delete, duplicate,
//! split, paste or cut is one undo entry and one monotonic revision; a plan that
//! changes nothing leaves presence, bytes, undo and redo untouched.
use crate::commands::{PasteDestination, PreparedSelectionEdit};
use crate::history::object_history::{NativeObjectHistory, ObjectHistoryEntry};
use crate::object_document::object_group_kernels::object_clipboard_plan::ClipPayload;
use crate::request_receipts::DispatchErrorCode;
use crate::transaction::CompactResult;

const OP_SELECTION_DELETE: &str = "command.document.object.selection.delete";
const OP_SELECTION_DUPLICATE: &str = "command.document.object.selection.duplicate";
const OP_LAYER_SPLIT: &str = "command.document.object.layer.split";
const OP_CLIPBOARD_PASTE: &str = "command.document.object.clipboard.paste";

impl NativeObjectHistory {
    pub(crate) fn prepare_selection_delete_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedSelectionEdit, DispatchErrorCode> {
        self.owner.prepare_selection_delete_json(bytes)
    }
    pub(crate) fn prepare_selection_duplicate_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedSelectionEdit, DispatchErrorCode> {
        self.owner.prepare_selection_duplicate_json(bytes)
    }
    pub(crate) fn prepare_layer_split_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedSelectionEdit, DispatchErrorCode> {
        self.owner.prepare_layer_split_json(bytes)
    }
    // Typed in-process callers only (tests today, the L02-J client port next).
    #[allow(dead_code)]
    pub(crate) fn copy_selection_json(
        &self,
        bytes: &[u8],
    ) -> Result<Option<ClipPayload>, DispatchErrorCode> {
        self.owner.copy_selection_json(bytes)
    }
    #[allow(dead_code)]
    pub(crate) fn prepare_clipboard_paste(
        &self,
        clip: &ClipPayload,
        destination: &PasteDestination,
    ) -> Result<PreparedSelectionEdit, DispatchErrorCode> {
        self.owner.prepare_clipboard_paste(clip, destination)
    }
    pub(crate) fn prepare_clipboard_paste_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedSelectionEdit, DispatchErrorCode> {
        self.owner.prepare_clipboard_paste_json(bytes)
    }
    #[allow(dead_code)]
    pub(crate) fn prepare_clipboard_cut_json(
        &self,
        bytes: &[u8],
    ) -> Result<(Option<ClipPayload>, PreparedSelectionEdit), DispatchErrorCode> {
        self.owner.prepare_clipboard_cut_json(bytes)
    }
    pub(crate) fn commit_selection_edit(
        &mut self,
        candidate: PreparedSelectionEdit,
    ) -> Result<u64, DispatchErrorCode> {
        let before_revision = self.content_revision();
        let after_revision = self.owner.commit_selection_edit(candidate)?;
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
    pub(crate) fn selection_edit_operation(operation: &str) -> bool {
        matches!(
            operation,
            OP_SELECTION_DELETE | OP_SELECTION_DUPLICATE | OP_LAYER_SPLIT | OP_CLIPBOARD_PASTE
        )
    }
    /// Prepare and commit one selection edit for the dispatcher, which has
    /// already checked the envelope, owner pins, cancellation, revision and
    /// request replay. The receipt is the compact applied / entries pair; new
    /// identities are named after the published revision and read back from the
    /// committed snapshot.
    pub(crate) fn dispatch_selection_edit(
        &mut self,
        operation: &str,
        payload: &serde_json::Value,
    ) -> Result<CompactResult, (DispatchErrorCode, &'static str)> {
        let bytes = serde_json::to_vec(payload).map_err(|_| {
            (
                DispatchErrorCode::InvalidRequest,
                "Selection edit payload is invalid.",
            )
        })?;
        let prepared = match operation {
            OP_SELECTION_DELETE => self.prepare_selection_delete_json(&bytes),
            OP_SELECTION_DUPLICATE => self.prepare_selection_duplicate_json(&bytes),
            OP_LAYER_SPLIT => self.prepare_layer_split_json(&bytes),
            OP_CLIPBOARD_PASTE => self.prepare_clipboard_paste_json(&bytes),
            _ => Err(DispatchErrorCode::InvalidRequest),
        };
        let candidate = prepared.map_err(|code| {
            (
                code,
                "Selection edit payload is invalid or target is absent.",
            )
        })?;
        let before = self.content_revision();
        let after = self
            .commit_selection_edit(candidate)
            .map_err(|code| (code, "Selection edit could not commit."))?;
        Ok(CompactResult::Command {
            applied: after != before,
            history_entries_added: u64::from(after != before),
        })
    }
}
