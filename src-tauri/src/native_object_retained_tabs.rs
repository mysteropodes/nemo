//! One parked project in the original host; only self.history accepts commands.
use crate::native_dispatch::object_host::{envelope_identity, export, preview};
use crate::native_dispatch::{NativeDispatch, NativeObjectHost};
use native_engine::{
    application::{ApplicationReleaseReceipt, ObjectMotionFrameControl},
    export_job::{
        CleanupStatus, ExportReleaseReconciliation, ExternalEffectDisposition, JobStatus,
        ReconciliationStage,
    },
    history::{InactiveProject, NativeObjectHistory},
    object_snapshot::ObjectSnapshot,
};
use serde_json::{json, Value};

/// Hard cap on one project-info answer (active or inactive), checked on the serialized bytes. #2371 made every layer row
/// richer (kind, folder, blend, opacity, matte and its additional mattes, long names): measured 484 bytes for a fully
/// populated row (`rich_rows_stay_within_the_ui_bound`, native-engine), so the earlier 64 KiB cap failed at about 135
/// such layers; baseline Release to Layers makes one layer per shape, so hundreds are realistic. 4 MiB holds about
/// 8,600 such rows (4096 rich layers measure 1.9 MB) and is still one cheap IPC/JSON parse. The answer is one read, not
/// paged: open, revision sync and the layer-list presenter all correlate on the whole answer's snapshot pins. Past the
/// cap the read is refused outright, never truncated.
pub(crate) const PROJECT_INFO_LIMIT_BYTES: usize = 4 * 1024 * 1024;

pub(super) struct ParkedProject {
    history: NativeObjectHistory,
    export: Option<export::ObjectExportSession>,
    metadata: Value,
    motion_control: ObjectMotionFrameControl,
}

/// Immutable plan, consumed only by the caller holding the lifecycle reservation.
pub(crate) struct PreparedRetainedActivation {
    origin: ObjectSnapshot,
    target: Value,
    outgoing: Value,
}
impl PreparedRetainedActivation {
    /// Read-only metadata for the reserved caller to bound its receipt BEFORE cleanup.
    pub(crate) fn metadata(&self) -> (&Value, &Value) {
        (&self.target, &self.outgoing)
    }
}
pub(crate) struct RetainedActivation {
    pub(crate) active: Value,
    pub(crate) inactive: Value,
}

fn settled_export(session: &Option<export::ObjectExportSession>) -> bool {
    session.as_ref().is_none_or(|s| {
        s.reconciliation.cleanup_complete
            && s.reconciliation.receipts.iter().all(|r| {
                r.status != JobStatus::Running
                    && r.external_effect_disposition != ExternalEffectDisposition::Indeterminate
                    && matches!(
                        r.cleanup.status,
                        CleanupStatus::Complete | CleanupStatus::NotRequired
                    )
                    && r.cleanup.error.is_none()
            })
    })
}
fn bounded_metadata(value: Value) -> Result<Value, String> {
    if serde_json::to_vec(&value).map_or(true, |v| v.len() > PROJECT_INFO_LIMIT_BYTES) {
        Err("native inactive project metadata exceeds UI limit".into())
    } else {
        Ok(value)
    }
}

impl NativeObjectHost {
    pub(super) fn has_inactive_project(&self) -> bool {
        self.inactive_project.is_some() || self.parked_project.is_some()
    }
    pub(super) fn retained_project_token(&self) -> Option<&str> {
        self.inactive_project
            .as_ref()
            .map(|(token, _)| token.as_str())
            .or_else(|| self.parked_project.as_ref()?.metadata["preparationToken"].as_str())
    }
    pub(super) fn retained_project_metadata(&self) -> Result<Option<Value>, String> {
        if self.inactive_project.is_some() && self.parked_project.is_some() {
            return Err("native inactive project slot is inconsistent".into());
        }
        if let Some(parked) = &self.parked_project {
            return Ok(Some(parked.metadata.clone()));
        }
        self.inactive_project
            .as_ref()
            .map(|(token, project)| {
                let project = project
                    .as_ref()
                    .ok_or("native inactive project was consumed")?;
                let mut info = Self::inactive_metadata(project)?;
                if info["preparationToken"] != *token {
                    return Err("native inactive project identity changed".into());
                }
                if let Some(id) = self.inactive_preparation_request_for_token(token) {
                    info["preparationRequestId"] = json!(id);
                }
                Ok(info)
            })
            .transpose()
    }
    pub(crate) fn prepare_inactive_project(
        &mut self,
        instance: &str,
        document: &str,
        revision: u64,
        snapshot: &str,
        bytes: &[u8],
    ) -> Result<Value, String> {
        self.require_inactive_owner(instance, document, revision, snapshot)?;
        if self.has_inactive_project() {
            return Err("one inactive native project is already retained".into());
        }
        let project = self
            .history
            .prepare_inactive_project(snapshot, bytes)
            .map_err(|error| format!("native inactive project admission: {error:?}"))?;
        let mut info = Self::inactive_metadata(&project)?;
        if let Some(id) = self.pending_inactive_request_id() {
            info["preparationRequestId"] = json!(id);
        }
        let info = bounded_metadata(info)?;
        let token = info["preparationToken"]
            .as_str()
            .ok_or("native inactive project token unavailable")?
            .to_owned();
        self.record_inactive_prepared(&token);
        self.inactive_project = Some((token, Some(project)));
        Ok(info)
    }
    fn inactive_metadata(project: &InactiveProject) -> Result<Value, String> {
        let mut info = project
            .metadata()
            .map_err(|_| "native inactive project metadata unavailable")?;
        info["preparationToken"] = info["documentSnapshotId"].clone();
        bounded_metadata(info)
    }
    pub(crate) fn inactive_project_info(
        &self,
        instance: &str,
        document: &str,
        revision: u64,
        snapshot: &str,
        token: &str,
    ) -> Result<Value, String> {
        self.require_inactive_owner(instance, document, revision, snapshot)?;
        if self.retained_project_token() != Some(token) {
            return Err("native inactive project token is unavailable".into());
        }
        self.retained_project_metadata()?
            .ok_or("native inactive project is unavailable".into())
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
        if self.retained_project_token() != Some(token) {
            return Err("native inactive project token is unavailable".into());
        }
        self.require_parked_disposable()?;
        self.inactive_project = None;
        self.parked_project = None;
        self.record_inactive_closed(token);
        Ok(())
    }
    pub(super) fn require_parked_disposable(&self) -> Result<(), String> {
        if self
            .parked_project
            .as_ref()
            .is_some_and(|p| !settled_export(&p.export))
        {
            return Err("native parked project requires export reconciliation".into());
        }
        Ok(())
    }
    pub(crate) fn prepare_retained_activation(
        &self,
        instance: &str,
        document: &str,
        revision: u64,
        snapshot: &str,
        token: &str,
        request_id: &str,
    ) -> Result<PreparedRetainedActivation, String> {
        self.require_inactive_owner(instance, document, revision, snapshot)?;
        if !envelope_identity(request_id) || self.pending_inactive_request_id().is_some() {
            return Err("native retained activation identity is unavailable".into());
        }
        if self.retained_project_token() != Some(token) {
            return Err("native inactive project token is unavailable".into());
        }
        self.require_parked_disposable()?;
        if !settled_export(&self.object_export) {
            return Err("native active project requires export reconciliation".into());
        }
        let target = self
            .retained_project_metadata()?
            .ok_or("native inactive project is unavailable")?;
        if target["instanceId"] != instance
            || target["documentId"] == document
            || target["origin"]["documentId"] != document
        {
            return Err("native retained activation owner changed".into());
        }
        let origin = self
            .history
            .acquire_snapshot(revision)
            .filter(|s| s.snapshot_id() == snapshot && self.history.is_project_file())
            .ok_or("native retained activation origin is unavailable")?;
        let mut outgoing = self.project_file_info()?;
        outgoing["instanceId"] = json!(instance);
        outgoing["documentId"] = json!(document);
        outgoing["preparationToken"] = json!(uuid::Uuid::new_v4().to_string());
        outgoing["preparationRequestId"] = json!(request_id);
        outgoing["origin"] = json!({"documentId":target["documentId"],
            "documentSnapshotId":target["documentSnapshotId"],"contentRevision":target["contentRevision"]});
        Ok(PreparedRetainedActivation {
            origin,
            target,
            outgoing: bounded_metadata(outgoing)?,
        })
    }
    /// Partial release failure retains both histories and the cleanup checkpoint.
    /// The NativeState caller must fence its reservation; this never rolls back.
    pub(crate) fn activate_retained_project(
        &mut self,
        plan: PreparedRetainedActivation,
        viewport_release: impl FnOnce() -> Result<(), String>,
    ) -> Result<RetainedActivation, String> {
        self.require_inactive_owner(
            plan.origin.instance_id(),
            plan.origin.document_id(),
            plan.origin.content_revision(),
            plan.origin.snapshot_id(),
        )?;
        if self.retained_project_metadata()?.as_ref() != Some(&plan.target) {
            return Err("native retained activation target changed".into());
        }
        self.require_parked_disposable()?;
        if !settled_export(&self.object_export) {
            return Err("native active project requires export reconciliation".into());
        }
        let (undo_depth, redo_depth) = self.history.history_depths();
        let application = ApplicationReleaseReceipt {
            instance_id: self.instance_id().into(),
            document_id: self.document_id().into(),
            content_revision: self.content_revision(),
            cancelled_transaction_id: None,
            cancelled_transaction: None,
            undo_depth,
            redo_depth,
            transaction_stage: ReconciliationStage::Complete,
            export_stage: ReconciliationStage::Complete,
            exports: self
                .object_export
                .as_ref()
                .map(|s| s.reconciliation.clone())
                .unwrap_or(ExportReleaseReconciliation {
                    receipts: vec![],
                    cleanup_complete: true,
                }),
        };
        self.released = Some(application.clone());
        self.preview.release();
        if !self.preview.progress(application).reconciliation_complete() {
            return Err("native retained activation preview cleanup incomplete".into());
        }
        viewport_release()?;
        let old_token = plan.target["preparationToken"]
            .as_str()
            .ok_or("native retained activation token is invalid")?;
        let target_snapshot = plan.target["documentSnapshotId"]
            .as_str()
            .ok_or("native retained activation snapshot is invalid")?;
        if let Some(parked) = &mut self.parked_project {
            self.history
                .swap_project_history(&plan.origin, target_snapshot, &mut parked.history)
                .map_err(|_| "native retained activation origin changed")?;
            std::mem::swap(&mut self.object_export, &mut parked.export);
            std::mem::swap(&mut self.preview.motion_control, &mut parked.motion_control);
            parked.metadata = plan.outgoing.clone();
        } else {
            let (_, incoming) = self
                .inactive_project
                .as_mut()
                .ok_or("native retained activation target is unavailable")?;
            let outgoing = self
                .history
                .activate_inactive_project(&plan.origin, target_snapshot, incoming)
                .map_err(|_| "native retained activation origin changed")?;
            self.parked_project = Some(ParkedProject {
                history: outgoing,
                export: self.object_export.take(),
                metadata: plan.outgoing.clone(),
                motion_control: std::mem::take(&mut self.preview.motion_control),
            });
            self.inactive_project = None;
        }
        self.record_inactive_closed(old_token);
        let motion_control = std::mem::take(&mut self.preview.motion_control);
        self.preview = preview::ObjectPreview::default();
        self.preview.motion_control = motion_control;
        self.released = None;
        Ok(RetainedActivation {
            active: plan.target,
            inactive: plan.outgoing,
        })
    }
}

#[cfg(test)]
#[path = "native_object_retained_tabs_tests.rs"]
mod tests;
