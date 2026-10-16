//! Sole object history owner with immutable reads and reconciled preview work.
use crate::native_dispatch::{NativeDispatch, NativeReleaseProgress};
use native_engine::{
    application::ApplicationReleaseReceipt,
    commands::{OpacityRequest, ResponseEnvelope},
    document::OpacityDocument,
    export_job::{ExportReleaseReconciliation, JobReceipt, PendingFrame, ReconciliationStage},
    history::{InactiveProject, NativeObjectHistory, PreparedProjectReplacement},
    object_document::ObjectDocument,
    object_snapshot::ObjectSnapshot,
};
use std::any::Any;
use std::collections::BTreeMap;

#[path = "native_object_inactive_requests.rs"]
mod inactive_requests;
use inactive_requests::InactiveRequest;

#[path = "native_object_font.rs"]
mod font_assets;

#[path = "native_object_export.rs"]
mod export;
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
    inactive_project: Option<(String, InactiveProject)>,
    inactive_requests: BTreeMap<String, InactiveRequest>,
    preview: preview::ObjectPreview,
    object_export: Option<export::ObjectExportSession>,
    released: Option<ApplicationReleaseReceipt>,
}
impl NativeObjectHost {
    // These internal operations also require the authority caller to retain its
    // active lifecycle/revision admission guard for the entire host borrow.
    fn require_inactive_owner(
        &self,
        instance: &str,
        document: &str,
        revision: u64,
        snapshot: &str,
    ) -> Result<(), String> {
        if self.released.is_some() {
            return Err("native project owner was released".into());
        }
        if self.instance_id() != instance || self.document_id() != document {
            return Err("native inactive project owner changed".into());
        }
        if self.content_revision() != revision || !self.matches_project_snapshot(snapshot) {
            return Err("native inactive project snapshot is stale".into());
        }
        Ok(())
    }

    /// Only detached metadata escapes; the actual charged B remains in this host.
    pub(crate) fn prepare_inactive_project(
        &mut self,
        instance: &str,
        document: &str,
        revision: u64,
        snapshot: &str,
        bytes: &[u8],
    ) -> Result<serde_json::Value, String> {
        self.require_inactive_owner(instance, document, revision, snapshot)?;
        if self.inactive_project.is_some() {
            return Err("one inactive native project is already retained".into());
        }
        let project = self
            .history
            .prepare_inactive_project(snapshot, bytes)
            .map_err(|error| format!("native inactive project admission: {error:?}"))?;
        let mut info = Self::inactive_metadata(&project)?;
        if let Some(id) = self.pending_inactive_request_id() {
            info["preparationRequestId"] = serde_json::json!(id);
        }
        if serde_json::to_vec(&info).map_or(true, |v| v.len() > 65536) {
            return Err("native inactive project metadata exceeds UI limit".into());
        }
        let token = info["preparationToken"]
            .as_str()
            .ok_or("native inactive project token unavailable")?
            .to_owned();
        self.record_inactive_prepared(&token);
        self.inactive_project = Some((token, project));
        Ok(info)
    }

    fn inactive_metadata(project: &InactiveProject) -> Result<serde_json::Value, String> {
        let mut info = project
            .metadata()
            .map_err(|_| "native inactive project metadata unavailable")?;
        // The fresh immutable B snapshot identity is an opaque retention token,
        // never permission to activate or dispatch into B.
        info["preparationToken"] = info["documentSnapshotId"].clone();
        if serde_json::to_vec(&info).map_or(true, |bytes| bytes.len() > 65536) {
            return Err("native inactive project metadata exceeds UI limit".into());
        }
        Ok(info)
    }

    pub(crate) fn inactive_project_info(
        &self,
        instance: &str,
        document: &str,
        revision: u64,
        snapshot: &str,
        token: &str,
    ) -> Result<serde_json::Value, String> {
        self.require_inactive_owner(instance, document, revision, snapshot)?;
        let (retained, project) = self
            .inactive_project
            .as_ref()
            .filter(|(retained, _)| retained == token)
            .ok_or("native inactive project token is unavailable")?;
        let info = Self::inactive_metadata(project)?;
        if info["preparationToken"] != *retained {
            return Err("native inactive project identity changed".into());
        }
        Ok(info)
    }

    pub(crate) fn close_inactive_project(
        &mut self,
        instance: &str,
        document: &str,
        revision: u64,
        snapshot: &str,
        token: &str,
    ) -> Result<(), String> {
        self.require_inactive_owner(instance, document, revision, snapshot)?;
        if !self
            .inactive_project
            .as_ref()
            .is_some_and(|(retained, _)| retained == token)
        {
            return Err("native inactive project token is unavailable".into());
        }
        self.inactive_project = None;
        self.record_inactive_closed(token);
        Ok(())
    }

    pub(crate) fn matches_project_snapshot(&self, snapshot: &str) -> bool {
        self.released.is_none()
            && self
                .history
                .acquire_snapshot(self.content_revision())
                .is_some_and(|current| current.snapshot_id() == snapshot)
    }
    pub(crate) fn prepare_project_replacement(
        &self,
        snapshot: &str,
        bytes: &[u8],
    ) -> Result<PreparedProjectReplacement, String> {
        if self.released.is_some() {
            return Err("native project owner was released".into());
        }
        self.history
            .prepare_project_replacement(snapshot, bytes)
            .map_err(|error| format!("native project replacement admission: {error:?}"))
    }
    pub(crate) fn validate_project_replacement(
        &self,
        prepared: &PreparedProjectReplacement,
    ) -> Result<(), String> {
        if self.released.is_some() {
            return Err("native project owner was released".into());
        }
        self.history
            .validate_project_replacement(prepared)
            .map_err(|_| "native project replacement origin changed".into())
    }
    pub(crate) fn prepare_blank_project_replacement(
        &self,
        snapshot: &str,
        canvas_width: u32,
        canvas_height: u32,
        fps: f64,
    ) -> Result<PreparedProjectReplacement, String> {
        if self.released.is_some() {
            return Err("native project owner was released".into());
        }
        self.history
            .prepare_blank_project_replacement(snapshot, canvas_width, canvas_height, fps)
            .map_err(|error| format!("native blank project admission: {error:?}"))
    }
    pub(crate) fn commit_project_replacement(
        &mut self,
        prepared: PreparedProjectReplacement,
    ) -> Result<(), String> {
        if !self
            .release_progress()
            .is_some_and(|progress| progress.reconciliation_complete())
        {
            return Err("native project replacement requires complete owner cleanup".into());
        }
        self.history
            .commit_project_replacement(prepared)
            .map_err(|_| "native project replacement origin changed".to_string())?;
        self.preview = preview::ObjectPreview::default();
        self.inactive_project = None;
        self.inactive_requests.clear();
        self.object_export = None;
        self.released = None;
        Ok(())
    }

    pub(crate) fn require_project_output(&self, dimensions: (u32, u32)) -> Result<(), String> {
        let expected = self
            .history
            .project_canvas_dimensions()
            .map_err(|_| "native project dimensions unavailable")?
            .unwrap_or((320, 180)); // Preserve the bare-object compatibility contract.
        if dimensions != expected {
            return Err("object output extent differs from the owned project".into());
        }
        Ok(())
    }

    pub(crate) fn project_file_info(&self) -> Result<serde_json::Value, String> {
        self.history
            .project_file_info()
            .map_err(|_| "native project metadata unavailable".into())
    }
    pub(crate) fn project_file_snapshot(
        &self,
        revision: u64,
        snapshot: &str,
    ) -> Result<Option<Vec<u8>>, String> {
        if !self.history.is_project_file() {
            return Ok(None);
        }
        self.history
            .serialize_project_file(revision, snapshot)
            .map(Some)
            .map_err(|_| "native project snapshot is unavailable".into())
    }
    pub(crate) fn project_reference(
        &mut self,
        snapshot: &str,
        request: &OpacityRequest,
        png: Option<&[u8]>,
    ) -> Result<ResponseEnvelope, String> {
        let current = self
            .history
            .acquire_snapshot(self.content_revision())
            .ok_or("native reference snapshot unavailable")?;
        if current.snapshot_id() != snapshot {
            return Err("native reference snapshot is stale".into());
        }
        let before = self.content_revision();
        let result = self.history.dispatch_reference_command(request, png);
        if self.content_revision() != before {
            self.preview.invalidate();
        }
        result.map_err(|_| "native reference request identity or body refused".into())
    }
    pub(crate) fn from_project_file(instance_id: String, bytes: &[u8]) -> Result<Self, String> {
        if !envelope_identity(&instance_id) {
            return Err("native project instance identity is invalid".into());
        }
        let history = NativeObjectHistory::from_project_file(instance_id, bytes)
            .map_err(|_| "native project file admission failed")?;
        Ok(Self {
            history,
            inactive_project: None,
            inactive_requests: BTreeMap::new(),
            preview: preview::ObjectPreview::default(),
            object_export: None,
            released: None,
        })
    }
    pub(crate) fn new_blank_project(
        instance_id: String,
        canvas_width: u32,
        canvas_height: u32,
        fps: f64,
    ) -> Result<Self, String> {
        if !envelope_identity(&instance_id) {
            return Err("native project instance identity is invalid".into());
        }
        let history =
            NativeObjectHistory::new_blank_project(instance_id, canvas_width, canvas_height, fps)
                .map_err(|error| format!("native blank project admission: {error:?}"))?;
        Ok(Self {
            history,
            inactive_project: None,
            inactive_requests: BTreeMap::new(),
            preview: preview::ObjectPreview::default(),
            object_export: None,
            released: None,
        })
    }
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
            inactive_project: None,
            inactive_requests: BTreeMap::new(),
            preview: preview::ObjectPreview::default(),
            object_export: None,
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
                let exports = self
                    .object_export
                    .as_ref()
                    .map(|session| session.reconciliation.clone())
                    .unwrap_or(ExportReleaseReconciliation {
                        receipts: Vec::new(),
                        cleanup_complete: true,
                    });
                ApplicationReleaseReceipt {
                    instance_id: self.instance_id().into(),
                    document_id: self.document_id().into(),
                    content_revision: self.content_revision(),
                    cancelled_transaction_id: None,
                    cancelled_transaction: None,
                    undo_depth,
                    redo_depth,
                    transaction_stage: ReconciliationStage::Complete,
                    export_stage: if exports.cleanup_complete {
                        ReconciliationStage::Complete
                    } else {
                        ReconciliationStage::Unknown
                    },
                    exports,
                }
            }
        };
        self.released = Some(application.clone());
        self.inactive_project = None;
        self.inactive_requests.clear();
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

#[cfg(test)]
#[path = "native_object_export_tests.rs"]
mod export_tests;
