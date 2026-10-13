//! Project-file and still-reference consumers of the one object history owner.
use crate::history::object_history::{
    NativeObjectHistory, ObjectFillOwner, ObjectHistoryEntry, StageReceipts,
};
use crate::request_receipts::DispatchErrorCode;
use crate::revision::ObjectSnapshot;

/// Immutable admitted source, bound to the original owner. No writable history
/// or replacement authority exists until this token is consumed by that owner.
#[derive(Debug)]
pub struct PreparedProjectReplacement {
    origin: ObjectSnapshot,
    next: ObjectSnapshot,
}

#[path = "object_reference_dispatch.rs"]
mod reference_dispatch;

impl NativeObjectHistory {
    pub fn prepare_project_replacement(
        &self,
        snapshot_id: &str,
        bytes: &[u8],
    ) -> Result<PreparedProjectReplacement, DispatchErrorCode> {
        let origin = self
            .acquire_snapshot(self.content_revision())
            .filter(|snapshot| snapshot.snapshot_id() == snapshot_id)
            .ok_or(DispatchErrorCode::StaleRevision)?;
        if origin.project().is_none() {
            return Err(DispatchErrorCode::Unavailable);
        }
        let next = ObjectSnapshot::admit_project_file(self.instance_id().into(), bytes)?;
        Ok(PreparedProjectReplacement { origin, next })
    }

    pub fn validate_project_replacement(
        &self,
        prepared: &PreparedProjectReplacement,
    ) -> Result<(), DispatchErrorCode> {
        let current = self
            .acquire_snapshot(self.content_revision())
            .ok_or(DispatchErrorCode::Unavailable)?;
        if prepared.origin.instance_id() != current.instance_id() {
            return Err(DispatchErrorCode::WrongInstance);
        }
        if prepared.origin.document_id() != current.document_id() {
            return Err(DispatchErrorCode::WrongDocument);
        }
        if prepared.origin.snapshot_id() != current.snapshot_id()
            || prepared.origin.content_revision() != current.content_revision()
        {
            return Err(DispatchErrorCode::StaleRevision);
        }
        Ok(())
    }

    /// Host cleanup/reservation precedes this consuming commit. The same owner
    /// receives a fresh document incarnation; A's immutable readers stay valid.
    pub fn commit_project_replacement(
        &mut self,
        prepared: PreparedProjectReplacement,
    ) -> Result<(), DispatchErrorCode> {
        self.validate_project_replacement(&prepared)?;
        self.owner.replace_admitted_snapshot(prepared.next);
        self.undo.clear();
        self.redo.clear();
        self.receipts = StageReceipts::default();
        Ok(())
    }

    pub fn from_project_file(
        instance: impl Into<String>,
        bytes: &[u8],
    ) -> Result<Self, DispatchErrorCode> {
        Ok(Self {
            owner: ObjectFillOwner::from_project_file(instance.into(), bytes)?,
            undo: Vec::new(),
            redo: Vec::new(),
            receipts: StageReceipts::default(),
        })
    }

    /// A retained revision may be saved; the exact snapshot pin is mandatory.
    pub fn serialize_project_file(
        &self,
        at_revision: u64,
        snapshot_id: &str,
    ) -> Result<Vec<u8>, DispatchErrorCode> {
        let snapshot = self
            .acquire_snapshot(at_revision)
            .filter(|s| s.snapshot_id() == snapshot_id)
            .ok_or(DispatchErrorCode::StaleRevision)?;
        snapshot.serialize_project_file()
    }

    /// Detached presentation metadata and a digest of the native file bytes.
    /// No drawing authority, media bytes or resource handles cross this port.
    pub fn project_file_info(&self) -> Result<serde_json::Value, DispatchErrorCode> {
        use crate::object_document::ProjectReferenceState;
        let snapshot = self
            .acquire_snapshot(self.content_revision())
            .ok_or(DispatchErrorCode::Unavailable)?;
        let project = snapshot.project().ok_or(DispatchErrorCode::Unavailable)?;
        let bytes = snapshot.serialize_project_file()?;
        let digest: String = crate::request_receipts::sha256(&bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        let reference = match project.reference_state() {
            ProjectReferenceState::NoMedia => serde_json::Value::Null,
            ProjectReferenceState::StillImage(image) => serde_json::json!({
                "id":image.id(), "name":image.name(), "visible":image.visible(),
                "opacity":image.opacity(), "offsetFrames":image.offset_frames()}),
        };
        let (width, height) = project.project().canvas_dimensions();
        Ok(
            serde_json::json!({"documentSnapshotId":snapshot.snapshot_id(),
            "contentRevision":snapshot.content_revision(), "documentDigest":digest,
            "byteCount":bytes.len(), "canvasW":width, "canvasH":height,
            "fps":project.project().fps(), "totalFrames":project.drawing().total_frames,
            "layers":project.drawing().layers, "reference":reference,
            "undoDepth":self.undo.len(), "redoDepth":self.redo.len()}),
        )
    }

    pub fn is_project_file(&self) -> bool {
        self.acquire_snapshot(self.content_revision())
            .is_some_and(|s| s.project().is_some())
    }

    /// Admission and publication stay under the same owner borrow. The caller's
    /// expected revision is checked before PNG decode or persistent allocation.
    pub(crate) fn attach_reference_png(
        &mut self,
        expected_revision: u64,
        id: String,
        name: String,
        png: &[u8],
    ) -> Result<u64, DispatchErrorCode> {
        if expected_revision != self.content_revision() {
            return Err(DispatchErrorCode::StaleRevision);
        }
        let snapshot: ObjectSnapshot = self
            .acquire_snapshot(expected_revision)
            .ok_or(DispatchErrorCode::Unavailable)?;
        let candidate = snapshot.prepare_reference_attachment(id, name, png)?;
        let after = self.owner.commit_reference_attachment(candidate)?;
        if after != expected_revision {
            self.undo.push(ObjectHistoryEntry {
                before_revision: expected_revision,
                after_revision: after,
            });
            self.redo.clear();
        }
        Ok(after)
    }
}

#[cfg(test)]
#[path = "../tests/project_media_workflow.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/object_project_replacement.rs"]
mod replacement_tests;
