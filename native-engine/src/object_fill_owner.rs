//! Private bridge from codec admission and sealed fill origins to revision storage.
use crate::commands::{prepare_object_fill_json, PreparedObjectFill};
use crate::object_document::ObjectDocument;
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;
use crate::revision::ObjectRevisionOwner;

#[derive(Debug)]
pub(crate) struct ObjectFillOwner {
    revisions: ObjectRevisionOwner,
}

impl ObjectFillOwner {
    pub(crate) fn new(
        instance_id: impl Into<String>,
        document: ObjectDocument,
    ) -> Result<Self, &'static str> {
        let snapshot = ObjectSnapshot::new(instance_id, document)
            .map_err(|_| "object document admission failed")?;
        Ok(Self {
            revisions: ObjectRevisionOwner::from_admitted(snapshot),
        })
    }
    pub(crate) fn instance_id(&self) -> &str {
        self.revisions.instance_id()
    }
    pub(crate) fn document_id(&self) -> &str {
        self.revisions.document_id()
    }
    pub(crate) fn content_revision(&self) -> u64 {
        self.revisions.content_revision()
    }
    pub(crate) fn acquire_snapshot(&self, revision: u64) -> Option<ObjectSnapshot> {
        self.revisions.acquire(revision)
    }
    pub(crate) fn prepare_fill_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedObjectFill, DispatchErrorCode> {
        prepare_object_fill_json(&self.current_snapshot(), bytes)
    }
    pub(crate) fn commit_fill(
        &mut self,
        candidate: PreparedObjectFill,
    ) -> Result<u64, DispatchErrorCode> {
        let current = self.current_snapshot();
        if candidate.base_instance_id() != current.instance_id() {
            return Err(DispatchErrorCode::WrongInstance);
        }
        if candidate.base_document_id() != current.document_id() {
            return Err(DispatchErrorCode::WrongDocument);
        }
        if candidate.base_snapshot_id() != current.snapshot_id()
            || candidate.base_content_revision() != current.content_revision()
        {
            return Err(DispatchErrorCode::StaleRevision);
        }
        if !candidate.changed() {
            return Ok(self.content_revision());
        }
        self.revisions
            .publish(candidate.document().clone())
            .map_err(|_| DispatchErrorCode::Unavailable)
    }
    pub(crate) fn restore(&mut self, revision: u64) -> Result<u64, DispatchErrorCode> {
        self.revisions
            .restore(revision)
            .map_err(|_| DispatchErrorCode::Unavailable)
    }
    fn current_snapshot(&self) -> ObjectSnapshot {
        self.acquire_snapshot(self.content_revision())
            .expect("current object revision has a retained immutable snapshot")
    }
}
