//! Sole object history owner with immutable reads and reconciled preview work.
use crate::native_dispatch::{NativeDispatch, NativeReleaseProgress};
use native_engine::{
    application::ApplicationReleaseReceipt,
    commands::{OpacityRequest, ResponseEnvelope},
    document::OpacityDocument,
    export_job::{ExportReleaseReconciliation, JobReceipt, PendingFrame, ReconciliationStage},
    history::NativeObjectHistory,
    object_document::ObjectDocument,
    object_snapshot::ObjectSnapshot,
};
use std::any::Any;

#[path = "native_object_preview.rs"]
mod preview;

fn envelope_identity(value: &str) -> bool {
    let bytes = value.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 128
        && bytes[0].is_ascii_alphanumeric()
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._:/-".contains(byte))
}

/// Exactly one admitted engine owner; no cloned authority or writable host mirror.
pub(crate) struct NativeObjectHost {
    history: NativeObjectHistory,
    preview: preview::ObjectPreview,
    released: Option<ApplicationReleaseReceipt>,
}
impl NativeObjectHost {
    pub(crate) fn new(instance_id: String, document: ObjectDocument) -> Result<Self, String> {
        if !envelope_identity(&instance_id) {
            return Err("native object host instance identity is invalid".into());
        }
        let history = NativeObjectHistory::new(instance_id, document).map_err(str::to_owned)?;
        if !envelope_identity(history.instance_id()) || !envelope_identity(history.document_id()) {
            return Err("native object owner identity is invalid".into());
        }
        Ok(Self {
            history,
            preview: preview::ObjectPreview::default(),
            released: None,
        })
    }
}
impl NativeDispatch for NativeObjectHost {
    fn instance_id(&self) -> &str {
        self.history.instance_id()
    }
    fn document_id(&self) -> &str {
        self.history.document_id()
    }
    fn content_revision(&self) -> u64 {
        self.history.content_revision()
    }
    fn object_client_available(&self) -> bool {
        self.released.is_none()
    }
    fn dispatch(&mut self, request: OpacityRequest) -> Result<ResponseEnvelope, String> {
        if self.released.is_some() {
            return Err("native object host was released".into());
        }
        let before = self.history.content_revision();
        let command = self.history.try_dispatch_object_command(&request);
        if self.history.content_revision() != before {
            // The command already committed. Cleanup failure is retained by the
            // preview owner; it must not falsify this command's revision/result.
            self.preview.invalidate();
        }
        if let Some(response) =
            command.map_err(|_| "native object command correlation is invalid".to_string())?
        {
            return Ok(response);
        }
        let snapshot: ObjectSnapshot = self
            .history
            .acquire_snapshot(self.content_revision())
            .ok_or("native object snapshot is unavailable")?;
        snapshot
            .dispatch(request)
            .map_err(|_| "native object read correlation is invalid".into())
    }
    fn replace_document(&mut self, _: OpacityDocument) -> Result<Vec<JobReceipt>, String> {
        Err("native object host replacement is unavailable".into())
    }
    fn start_next_export_frame(&mut self, _: &str) -> Result<Option<PendingFrame>, String> {
        Err("native object export is unavailable".into())
    }
    fn finish_export_frame(&mut self, _: PendingFrame) -> Result<JobReceipt, String> {
        Err("native object export is unavailable".into())
    }
    fn release_project(&mut self) -> Result<NativeReleaseProgress, String> {
        let application = match &self.released {
            Some(retained) => retained.clone(),
            None => {
                let (undo_depth, redo_depth) = self.history.history_depths();
                // No transaction/export/artifact work exists here. Preview work
                // is reconciled separately below, without claiming surface disposal.
                ApplicationReleaseReceipt {
                    instance_id: self.instance_id().into(),
                    document_id: self.document_id().into(),
                    content_revision: self.content_revision(),
                    cancelled_transaction_id: None,
                    cancelled_transaction: None,
                    undo_depth,
                    redo_depth,
                    transaction_stage: ReconciliationStage::Complete,
                    export_stage: ReconciliationStage::Complete,
                    exports: ExportReleaseReconciliation {
                        receipts: Vec::new(),
                        cleanup_complete: true,
                    },
                }
            }
        };
        self.released = Some(application.clone());
        self.preview.release();
        Ok(self.preview.progress(application))
    }
    fn release_progress(&self) -> Option<NativeReleaseProgress> {
        self.released
            .clone()
            .map(|application| self.preview.progress(application))
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

#[cfg(test)]
#[path = "native_object_host_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "native_object_host_read_tests.rs"]
mod read_tests;

#[cfg(test)]
#[path = "native_object_host_command_tests.rs"]
mod command_tests;

#[cfg(test)]
#[path = "native_object_host_history_tests.rs"]
mod history_tests;
