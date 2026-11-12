//! Sole object history owner with immutable reads and reconciled preview work.
use crate::native_dispatch::{NativeDispatch, NativeReleaseProgress};
use native_engine::{
    application::ApplicationReleaseReceipt,
    commands::{DispatchErrorCode, OpacityRequest, ResponseEnvelope},
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

#[path = "native_object_retained_tabs.rs"]
mod retained_tabs;
use retained_tabs::ParkedProject;
pub(crate) use retained_tabs::{PreparedRetainedActivation, PROJECT_INFO_LIMIT_BYTES};

#[path = "native_object_font.rs"]
mod font_assets;
#[path = "native_object_image.rs"]
mod image_import;
#[path = "native_object_brush_bitmap.rs"]
mod brush_bitmap;
#[path = "native_object_video_link.rs"]
mod video_link;
#[path = "native_object_import_batch.rs"]
mod import_batch;

#[path = "native_object_expression_state.rs"]
mod expression_state;

#[path = "native_object_motion_effects.rs"]
mod motion_effects;

#[path = "native_object_export.rs"]
mod export;
#[path = "native_object_preview.rs"]
mod preview;

/// Brackets one object preview's present for the desktop's occlusion diagnostics: before register
/// the host reports its request pins and composed work, and the caller closes the scope with the
/// receipt. The host seam itself never reaches a viewport adapter.
pub(crate) trait ObjectPreviewObserver {
    fn begin(&mut self, preview: ObservedObjectPreview<'_>);
}

impl ObjectPreviewObserver for () {
    fn begin(&mut self, _: ObservedObjectPreview<'_>) {}
}

pub(crate) struct ObservedObjectPreview<'a> {
    pub(crate) instance_id: &'a str,
    pub(crate) document_id: &'a str,
    pub(crate) content_revision: u64,
    pub(crate) frame: u32,
    pub(crate) work_id: u64,
    pub(crate) view_generation: u64,
}

fn envelope_identity(value: &str) -> bool {
    let bytes = value.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 128
        && bytes[0].is_ascii_alphanumeric()
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._:/-".contains(byte))
}

/// Encoder over an already-acquired immutable project snapshot.
pub(crate) type DetachedProjectSerializer = Box<dyn FnOnce() -> Result<Vec<u8>, String> + Send>;

/// Sole routed active history; any parked history remains private and unrouted.
pub(crate) struct NativeObjectHost {
    history: NativeObjectHistory,
    inactive_project: Option<(String, Option<InactiveProject>)>,
    parked_project: Option<ParkedProject>,
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

    /// Catalog removal delegates to the existing sealed asset/history owner.
    /// Keep preflight codes typed for the common API, including changed-body replay.
    pub(crate) fn remove_asset(
        &mut self,
        request: &OpacityRequest,
    ) -> Result<ResponseEnvelope, DispatchErrorCode> {
        if self.released.is_some() {
            return Err(DispatchErrorCode::Unavailable);
        }
        let before = self.history.content_revision();
        let response = self.history.dispatch_asset_command(request, None);
        if self.history.content_revision() != before {
            self.preview.invalidate();
        }
        response
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
    #[cfg(test)]
    pub(crate) fn prepare_blank_project_replacement(
        &self,
        snapshot: &str,
        canvas_width: u32,
        canvas_height: u32,
        fps: f64,
    ) -> Result<PreparedProjectReplacement, String> {
        self.prepare_blank_project_replacement_frames(snapshot, canvas_width, canvas_height, fps, None)
    }
    pub(crate) fn prepare_blank_project_replacement_frames(
        &self,
        snapshot: &str,
        canvas_width: u32,
        canvas_height: u32,
        fps: f64,
        total_frames: Option<u32>,
    ) -> Result<PreparedProjectReplacement, String> {
        if self.released.is_some() {
            return Err("native project owner was released".into());
        }
        self.history
            .prepare_blank_project_replacement_frames(
                snapshot,
                canvas_width,
                canvas_height,
                fps,
                total_frames,
            )
            .map_err(|error| format!("native blank project admission: {error:?}"))
    }
    pub(crate) fn commit_project_replacement(
        &mut self,
        prepared: PreparedProjectReplacement,
    ) -> Result<(), String> {
        self.require_parked_disposable()?;
        if !self
            .release_progress()
            .is_some_and(|progress| progress.reconciliation_complete())
        {
            return Err("native project replacement requires complete owner cleanup".into());
        }
        self.history
            .commit_project_replacement(prepared)
            .map_err(|_| "native project replacement origin changed".to_string())?;
        // The ghost caches hold the replaced document's snapshots alive; nothing of it can be asked for again.
        native_engine::application::release_ghost_caches();
        self.preview = preview::ObjectPreview::default();
        self.inactive_project = None;
        self.parked_project = None;
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
    /// Recovery variant of `project_file_snapshot`: the immutable snapshot is
    /// acquired here (O(1)) and the encoder runs wherever the caller invokes the
    /// returned closure, so the authority lock need not be held while encoding.
    pub(crate) fn detached_project_file_serializer(
        &self,
        revision: u64,
        snapshot: &str,
    ) -> Result<Option<DetachedProjectSerializer>, String> {
        if !self.history.is_project_file() {
            return Ok(None);
        }
        let serialize = self
            .history
            .detached_project_file_serializer(revision, snapshot)
            .map_err(|_| "native project snapshot is unavailable")?;
        Ok(Some(Box::new(move || {
            serialize().map_err(|_| "native project snapshot is unavailable".to_owned())
        })))
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
            parked_project: None,
            inactive_requests: BTreeMap::new(),
            preview: preview::ObjectPreview::default(),
            object_export: None,
            released: None,
        })
    }
    #[cfg(test)]
    pub(crate) fn new_blank_project(
        instance_id: String,
        canvas_width: u32,
        canvas_height: u32,
        fps: f64,
    ) -> Result<Self, String> {
        Self::new_blank_project_frames(instance_id, canvas_width, canvas_height, fps, None)
    }
    pub(crate) fn new_blank_project_frames(
        instance_id: String,
        canvas_width: u32,
        canvas_height: u32,
        fps: f64,
        total_frames: Option<u32>,
    ) -> Result<Self, String> {
        if !envelope_identity(&instance_id) {
            return Err("native project instance identity is invalid".into());
        }
        let history = NativeObjectHistory::new_blank_project_frames(
            instance_id,
            canvas_width,
            canvas_height,
            fps,
            total_frames,
        )
        .map_err(|error| format!("native blank project admission: {error:?}"))?;
        Ok(Self {
            history,
            inactive_project: None,
            parked_project: None,
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
            parked_project: None,
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
        if request.operation() == "query.document.object.history" {
            return self
                .history
                .dispatch_history_read(&request)
                .map_err(|_| "native history read correlation is invalid".into());
        }
        if Self::owns_brush_bitmap(&request) {
            return self.dispatch_brush_bitmap(&request);
        }
        if request.operation() == "command.document.object.asset.remove" {
            return self
                .remove_asset(&request)
                .map_err(|_| "native asset removal correlation is invalid".into());
        }
        let before = self.history.content_revision();
        let command = self.history.dispatch_object_command(&request);
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
        if request.operation() == "query.document.object"
            && serde_json::to_value(&request)
                .ok()
                .is_some_and(|wire| wire["payload"].get("view").is_some())
        {
            if serde_json::to_value(&request).ok().is_some_and(|wire| wire["payload"]["view"]["kind"] == "subselect") { return self.dispatch_subselect_view(&snapshot, request); }
            if serde_json::to_value(&request).ok().is_some_and(|wire| wire["payload"]["view"]["kind"] == "widget") { return self.dispatch_widget_view(&snapshot, request); }
            if serde_json::to_value(&request).ok().is_some_and(|wire| wire["payload"]["view"]["kind"] == "ghosts") { return self.dispatch_ghost_view(&snapshot, request); }
            if serde_json::to_value(&request).ok().is_some_and(|wire| wire["payload"]["view"]["kind"] == "rig") { return self.dispatch_rig_view(&snapshot, request); }
            return self.dispatch_image_mesh_view(&snapshot, request);
        }
        let expression_state = request.operation() == "query.document.object.property.read"
            && serde_json::to_value(&request)
                .ok()
                .is_some_and(|wire| wire["payload"]["view"]["kind"] == "expressionState");
        let response = snapshot
            .dispatch(request.clone())
            .map_err(|_| "native object read correlation is invalid".to_string())?;
        // T12: a layerPlacement read gains the decoder's source frame count.
        let response = crate::native_dispatch::video::with_source_frame_count(response, &snapshot);
        if expression_state && response.is_ok() {
            self.project_expression_state(&request, response)
        } else {
            Ok(response)
        }
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
        self.require_parked_disposable()?;
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
        native_engine::application::release_ghost_caches();
        self.inactive_project = None;
        self.parked_project = None;
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
#[path = "native_object_tween_host_tests.rs"]
mod tween_tests;

#[cfg(test)]
#[path = "native_object_export_tests.rs"]
mod export_tests;

#[cfg(test)]
#[path = "native_asset_remove_dispatch_tests.rs"]
mod asset_remove_tests;

#[cfg(test)]
pub(crate) use preview::motion_tests::{finalizer_fixture, finalizer_svg};

#[cfg(test)]
#[path = "native_object_history_read_tests.rs"]
mod history_read_tests;
