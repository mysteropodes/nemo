//! Staged object fill/history authority; no transport, retries or replacement port.
use crate::commands::{ObjectFillOwner, PreparedObjectFill};
use crate::object_document::ObjectDocument;
use crate::request_receipts::DispatchErrorCode;
use crate::revision::ObjectSnapshot;

#[derive(Debug, Clone)]
struct ObjectHistoryEntry {
    before_revision: u64,
    after_revision: u64,
}

/// One non-Clone owner. Construction always admits a fresh document incarnation.
#[derive(Debug)]
pub struct NativeObjectHistory {
    owner: ObjectFillOwner,
    undo: Vec<ObjectHistoryEntry>,
    redo: Vec<ObjectHistoryEntry>,
}

impl NativeObjectHistory {
    pub fn new(
        instance_id: impl Into<String>,
        document: ObjectDocument,
    ) -> Result<Self, &'static str> {
        Ok(Self {
            owner: ObjectFillOwner::new(instance_id, document)?,
            undo: vec![],
            redo: vec![],
        })
    }
    pub fn instance_id(&self) -> &str {
        self.owner.instance_id()
    }
    pub fn document_id(&self) -> &str {
        self.owner.document_id()
    }
    pub fn content_revision(&self) -> u64 {
        self.owner.content_revision()
    }
    pub fn acquire_snapshot(&self, revision: u64) -> Option<ObjectSnapshot> {
        self.owner.acquire_snapshot(revision)
    }
    pub fn history_depths(&self) -> (usize, usize) {
        (self.undo.len(), self.redo.len())
    }
    /// Strict D1 payload preparation from this owner's current immutable snapshot.
    pub fn prepare_fill_json(&self, bytes: &[u8]) -> Result<PreparedObjectFill, DispatchErrorCode> {
        self.owner.prepare_fill_json(bytes)
    }
    /// Returns the selected revision; semantic no-op returns the unchanged head.
    pub fn commit_fill(&mut self, candidate: PreparedObjectFill) -> Result<u64, DispatchErrorCode> {
        let before_revision = self.content_revision();
        let after_revision = self.owner.commit_fill(candidate)?;
        if after_revision != before_revision {
            self.undo.push(ObjectHistoryEntry {
                before_revision,
                after_revision,
            });
            self.redo.clear();
        }
        Ok(after_revision)
    }
    /// Restores full retained before-content into a new monotonic revision.
    pub fn undo(&mut self, expected_revision: u64) -> Result<u64, DispatchErrorCode> {
        self.move_history(expected_revision, true)
    }
    /// Restores full retained after-content into a new monotonic revision.
    pub fn redo(&mut self, expected_revision: u64) -> Result<u64, DispatchErrorCode> {
        self.move_history(expected_revision, false)
    }
    fn move_history(
        &mut self,
        expected_revision: u64,
        undo: bool,
    ) -> Result<u64, DispatchErrorCode> {
        if expected_revision != self.content_revision() {
            return Err(DispatchErrorCode::StaleRevision);
        }
        let source = if undo { &self.undo } else { &self.redo };
        let entry = source.last().ok_or(DispatchErrorCode::Unavailable)?.clone();
        let revision = if undo {
            entry.before_revision
        } else {
            entry.after_revision
        };
        let restored = self.owner.restore(revision)?;
        if undo {
            self.undo.pop();
            self.redo.push(entry);
        } else {
            self.redo.pop();
            self.undo.push(entry);
        }
        Ok(restored)
    }
}
