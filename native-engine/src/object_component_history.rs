//! A component-table replacement is one transaction in the existing object
//! history (L04-I): one successful change is one undo entry and one monotonic
//! revision; an edit that changes nothing leaves presence, bytes, undo depth
//! and redo stack untouched. Undo and redo restore the full retained
//! before/after document, so the exact prior table comes back byte for byte.
use crate::commands::{ComponentTablesEdit, PreparedComponentTables};
use crate::history::object_history::{NativeObjectHistory, ObjectHistoryEntry};
use crate::request_receipts::DispatchErrorCode;

impl NativeObjectHistory {
    pub(crate) fn prepare_component_tables(
        &self,
        edit: ComponentTablesEdit,
    ) -> Result<PreparedComponentTables, DispatchErrorCode> {
        self.owner.prepare_component_tables(edit)
    }
    pub(crate) fn commit_component_tables(
        &mut self,
        candidate: PreparedComponentTables,
    ) -> Result<u64, DispatchErrorCode> {
        let before_revision = self.content_revision();
        let after_revision = self.owner.commit_component_tables(candidate)?;
        if after_revision != before_revision {
            self.undo.push(ObjectHistoryEntry {
                before_revision,
                after_revision,
            });
            self.redo.clear();
        }
        Ok(after_revision)
    }
}
