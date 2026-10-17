//! Group edits are single transactions in the existing object history (G24,
//! L02-G): one successful group / ungroup / combine / mode / rename / exit is one undo
//! entry and one monotonic revision; an edit that changes nothing leaves
//! presence, bytes, undo and redo untouched. A removed or dissolved member
//! group is part of the SAME edit (the plan dissolves groups below two
//! entries), never a second entry.
use crate::commands::PreparedGroupEdit;
use crate::history::object_history::{NativeObjectHistory, ObjectHistoryEntry};
use crate::request_receipts::DispatchErrorCode;
use crate::transaction::CompactResult;

const OP_GROUP_CREATE: &str = "command.document.object.group.create";
const OP_GROUP_UNGROUP: &str = "command.document.object.group.ungroup";
const OP_GROUP_COMBINE: &str = "command.document.object.group.combine";
const OP_GROUP_MODE: &str = "command.document.object.group.mode.set";
const OP_GROUP_RENAME: &str = "command.document.object.group.rename";
const OP_GROUP_EXIT: &str = "command.document.object.group.member.remove";

impl NativeObjectHistory {
    pub(crate) fn prepare_group_create_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedGroupEdit, DispatchErrorCode> {
        self.owner.prepare_group_create_json(bytes)
    }
    pub(crate) fn prepare_group_ungroup_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedGroupEdit, DispatchErrorCode> {
        self.owner.prepare_group_ungroup_json(bytes)
    }
    pub(crate) fn prepare_group_combine_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedGroupEdit, DispatchErrorCode> {
        self.owner.prepare_group_combine_json(bytes)
    }
    pub(crate) fn prepare_group_mode_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedGroupEdit, DispatchErrorCode> {
        self.owner.prepare_group_mode_json(bytes)
    }
    pub(crate) fn prepare_group_rename_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedGroupEdit, DispatchErrorCode> {
        self.owner.prepare_group_rename_json(bytes)
    }
    pub(crate) fn prepare_group_exit_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedGroupEdit, DispatchErrorCode> {
        self.owner.prepare_group_exit_json(bytes)
    }
    pub(crate) fn commit_group_edit(
        &mut self,
        candidate: PreparedGroupEdit,
    ) -> Result<u64, DispatchErrorCode> {
        let before_revision = self.content_revision();
        let after_revision = self.owner.commit_group_edit(candidate)?;
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
    pub(crate) fn group_operation(operation: &str) -> bool {
        matches!(
            operation,
            OP_GROUP_CREATE
                | OP_GROUP_UNGROUP
                | OP_GROUP_COMBINE
                | OP_GROUP_MODE
                | OP_GROUP_RENAME
                | OP_GROUP_EXIT
        )
    }
    /// Prepare and commit one group edit for the dispatcher, which has already
    /// checked the envelope, owner pins, cancellation, revision and request
    /// replay. The receipt is the compact command result.
    pub(crate) fn dispatch_group(
        &mut self,
        operation: &str,
        payload: &serde_json::Value,
    ) -> Result<CompactResult, (DispatchErrorCode, &'static str)> {
        let bytes = serde_json::to_vec(payload).map_err(|_| {
            (
                DispatchErrorCode::InvalidRequest,
                "Group payload is invalid.",
            )
        })?;
        let prepared = match operation {
            OP_GROUP_CREATE => self.prepare_group_create_json(&bytes),
            OP_GROUP_UNGROUP => self.prepare_group_ungroup_json(&bytes),
            OP_GROUP_COMBINE => self.prepare_group_combine_json(&bytes),
            OP_GROUP_MODE => self.prepare_group_mode_json(&bytes),
            OP_GROUP_RENAME => self.prepare_group_rename_json(&bytes),
            OP_GROUP_EXIT => self.prepare_group_exit_json(&bytes),
            _ => Err(DispatchErrorCode::InvalidRequest),
        };
        let candidate =
            prepared.map_err(|code| (code, "Group payload is invalid or target is absent."))?;
        let before = self.content_revision();
        let after = self
            .commit_group_edit(candidate)
            .map_err(|code| (code, "Group edit could not commit."))?;
        Ok(CompactResult::Command {
            applied: after != before,
            history_entries_added: u64::from(after != before),
        })
    }
}
