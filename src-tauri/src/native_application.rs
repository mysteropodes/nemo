//! Dormant native opacity composition root; N20 alone may activate it.

use crate::{
    native_application_contract::*,
    native_application_ports::{
        DesktopArtifactBindings, DesktopArtifactPort, DesktopResourceResolver, SharedCompositor,
    },
    native_dispatch::{
        NativeAuthority, NativeDispatch, NativePhase, NativeState, ReleaseAdmission,
        ReleaseTombstone,
    },
};
use native_engine::{
    application::NativeApplication,
    compositor::CompositionResult,
    document::OpacityDocument,
    export_job::{JobReceipt, PendingFrame},
    render_scene::{self, ScheduledFrameIdentity},
    resource_leases::{FrameFailure, FrameFailureKind, WorkId},
    scheduler::{EvaluationKey, FrameScheduler},
};
use std::{collections::BTreeSet, path::PathBuf};

#[cfg(test)]
pub(crate) use crate::native_dispatch::run_export_pump_interleaved;

type DesktopCore =
    NativeApplication<DesktopArtifactPort, SharedCompositor, DesktopResourceResolver>;

pub(crate) struct PreparedPreview {
    pub(crate) result: CompositionResult,
    pub(crate) identity: ScheduledFrameIdentity,
}

pub(crate) struct DesktopRelease {
    pub(crate) instance_id: String,
    pub(crate) document_id: String,
    pub(crate) content_revision: u64,
    pub(crate) cancelled_transaction_id: Option<String>,
    pub(crate) cancelled_transaction: Option<serde_json::Value>,
    pub(crate) undo_depth: usize,
    pub(crate) redo_depth: usize,
    pub(crate) reconciled_exports: Vec<JobReceipt>,
    pub(crate) export_cleanup_complete: bool,
    pub(crate) cancelled_preview: Vec<WorkId>,
    pub(crate) preview_error: Option<String>,
}

pub(crate) struct DesktopNativeApplication {
    core: DesktopCore,
    preview_resources: DesktopResourceResolver,
    preview_compositor: SharedCompositor,
    preview_scheduler: FrameScheduler,
    preview_work: BTreeSet<WorkId>,
    artifact_bindings: DesktopArtifactBindings,
}

impl DesktopNativeApplication {
    pub(crate) fn new(
        instance_id: String,
        admitted: AdmittedProject,
        artifacts: DesktopArtifactPort,
        compositor: SharedCompositor,
    ) -> HostResult<Self> {
        let bindings = artifacts.bindings();
        let resources = DesktopResourceResolver::new(admitted.resources)
            .map_err(|message| host_error("invalid_request", message))?;
        let core = NativeApplication::new(
            instance_id,
            admitted.document,
            artifacts,
            compositor.clone(),
            resources.clone(),
        )
        .map_err(|message| host_error("invalid_request", message))?;
        Ok(Self {
            core,
            preview_resources: resources,
            preview_compositor: compositor,
            preview_scheduler: FrameScheduler::new(),
            preview_work: BTreeSet::new(),
            artifact_bindings: bindings,
        })
    }

    pub(crate) fn instance_id(&self) -> &str {
        self.core.instance_id()
    }

    pub(crate) fn document_id(&self) -> &str {
        self.core.document_id()
    }

    pub(crate) fn content_revision(&self) -> u64 {
        self.core.content_revision()
    }

    pub(crate) fn replace_project(
        &mut self,
        admitted: AdmittedProject,
    ) -> HostResult<(Vec<JobReceipt>, Vec<WorkId>)> {
        let resources = DesktopResourceResolver::new(admitted.resources)
            .map_err(|message| host_error("invalid_request", message))?;
        let exports = self
            .core
            .replace_document(admitted.document)
            .map_err(|message| host_error("internal", message))?;
        let document_id = self.core.document_id().to_owned();
        let preview = self
            .preview_scheduler
            .replace_document(document_id)
            .map_err(|error| host_error("internal", error.to_string()))?;
        for receipt in &preview {
            self.preview_work.remove(&receipt.work_id());
        }
        *self.core.resource_resolver_mut() = resources.clone();
        self.preview_resources = resources;
        Ok((
            exports,
            preview
                .into_iter()
                .map(|receipt| receipt.work_id())
                .collect(),
        ))
    }

    pub(crate) fn bind_output(&self, handle: String, destination: PathBuf) -> HostResult<()> {
        self.artifact_bindings
            .bind(handle, destination)
            .map_err(|message| host_error("invalid_request", message))
    }

    pub(crate) fn prepare_preview(
        &mut self,
        request: &NativePreviewRequest,
    ) -> HostResult<PreparedPreview> {
        self.require_identity(
            &request.instance_id,
            &request.document_id,
            request.content_revision,
        )?;
        if request.quality != "final"
            || !bounded_id(&request.context_id)
            || !bounded_id(&request.document_snapshot_id)
        {
            return Err(host_error("invalid_request", "invalid preview identity"));
        }
        let snapshot = self
            .core
            .acquire_snapshot(request.content_revision)
            .ok_or_else(|| host_error("not_found", "native snapshot is unavailable"))?;
        if snapshot.id() != request.document_snapshot_id {
            return Err(host_error(
                "stale_revision",
                "preview snapshot identity is stale",
            ));
        }
        let geometry = self
            .preview_resources
            .resolve_identity(
                &request.geometry_handle.resource_id,
                &request.geometry_handle.resource_version,
            )
            .map_err(|message| host_error("not_found", message))?;
        let key = EvaluationKey::new(
            snapshot.id(),
            &request.context_id,
            request.frame,
            &request.quality,
            request.output_spec.admit()?,
            [(
                geometry.resource_id().to_owned(),
                geometry.resource_version().to_owned(),
            )],
        )
        .map_err(|error| host_error("invalid_request", error.to_string()))?;
        let scheduled = self
            .preview_scheduler
            .schedule(snapshot, key)
            .map_err(|error| host_error("invalid_request", error.to_string()))?;
        let result = (|| {
            let scene = render_scene::prepare(&self.preview_scheduler, &scheduled, &geometry)
                .map_err(|error| error.to_string())?;
            let shared = self.preview_compositor.inner();
            let mut compositor = shared
                .lock()
                .map_err(|_| "compositor_lock_poisoned".to_string())?;
            compositor
                .compose(&scene)
                .map_err(|error| error.to_string())
        })();
        match result {
            Ok(result) => {
                self.preview_work.insert(scheduled.work_id());
                Ok(PreparedPreview {
                    result,
                    identity: ScheduledFrameIdentity::from_scheduled(&scheduled),
                })
            }
            Err(message) => {
                let _ = self.preview_scheduler.fail(
                    scheduled.work_id(),
                    FrameFailure::new(FrameFailureKind::Worker, &message),
                );
                Err(host_error("internal", message))
            }
        }
    }

    pub(crate) fn finish_preview(
        &mut self,
        work_id: WorkId,
        status: &'static str,
    ) -> HostResult<()> {
        if matches!(status, "deferred-timeout" | "deferred-occluded") {
            return Ok(());
        }
        let result = if matches!(status, "presented" | "stale-discarded") {
            self.preview_scheduler.succeed(work_id)
        } else {
            self.preview_scheduler
                .fail(work_id, FrameFailure::new(FrameFailureKind::Worker, status))
        };
        let result = result
            .map(|_| ())
            .map_err(|error| host_error("internal", error.to_string()));
        if result.is_ok() {
            self.preview_work.remove(&work_id);
        }
        result
    }

    pub(crate) fn cancel_preview(&mut self, work_ids: &[WorkId]) -> HostResult<()> {
        for work_id in work_ids {
            self.preview_scheduler
                .cancel(*work_id)
                .map_err(|error| host_error("internal", error.to_string()))?;
            self.preview_work.remove(work_id);
        }
        Ok(())
    }

    pub(crate) fn release_project(&mut self) -> DesktopRelease {
        let application = self.core.release_authority();
        let export_cleanup_complete = application.cleanup_complete();
        let pending: Vec<WorkId> = self.preview_work.iter().copied().collect();
        let preview_error = self
            .cancel_preview(&pending)
            .err()
            .map(|error| error.message);
        DesktopRelease {
            instance_id: application.instance_id,
            document_id: application.document_id,
            content_revision: application.content_revision,
            cancelled_transaction_id: application.cancelled_transaction_id,
            cancelled_transaction: application.cancelled_transaction,
            undo_depth: application.undo_depth,
            redo_depth: application.redo_depth,
            reconciled_exports: application.exports.receipts,
            export_cleanup_complete,
            cancelled_preview: pending,
            preview_error,
        }
    }

    pub(crate) fn require_identity(
        &self,
        instance: &str,
        document: &str,
        revision: u64,
    ) -> HostResult<()> {
        let error = if instance != self.core.instance_id() {
            Some(("wrong_instance", "native application instance mismatch"))
        } else if document != self.core.document_id() {
            Some(("wrong_document", "native document was replaced"))
        } else if revision != self.core.content_revision() {
            Some(("stale_revision", "native content revision is stale"))
        } else {
            None
        };
        error.map_or(Ok(()), |(code, message)| Err(host_error(code, message)))
    }

    pub(crate) fn preview_compositor(&self) -> SharedCompositor {
        self.preview_compositor.clone()
    }
}

impl NativeDispatch for DesktopNativeApplication {
    fn instance_id(&self) -> &str {
        self.core.instance_id()
    }
    fn document_id(&self) -> &str {
        self.core.document_id()
    }
    fn content_revision(&self) -> u64 {
        self.core.content_revision()
    }
    fn dispatch(
        &mut self,
        request: native_engine::commands::OpacityRequest,
    ) -> native_engine::commands::ResponseEnvelope {
        self.core.dispatch(request)
    }
    fn replace_document(&mut self, _: OpacityDocument) -> Result<Vec<JobReceipt>, String> {
        Err("desktop replacement requires validated projection resources".into())
    }
    fn start_next_export_frame(&mut self, job_id: &str) -> Result<Option<PendingFrame>, String> {
        self.core
            .start_next_export_frame(job_id)
            .map_err(|error| error.message)
    }
    fn finish_export_frame(&mut self, pending: PendingFrame) -> Result<JobReceipt, String> {
        self.core
            .finish_export_frame(pending)
            .map_err(|error| error.message)
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

pub(crate) fn admit_release_request(
    native: &NativeState,
    request: &NativeReleaseRequest,
) -> HostResult<ReleaseAdmission> {
    let fingerprint = release_fingerprint(request)?;
    native
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .admit_release(
            &request.request_id,
            &request.instance_id,
            &request.document_id,
            request.expected_revision,
            &fingerprint,
            request.cancelled_before_dispatch,
        )
        .map_err(|message| {
            let (code, message) = message
                .split_once(':')
                .unwrap_or(("internal", message.as_str()));
            host_error(code, message)
        })
}

pub(crate) fn complete_release<F>(
    native: &NativeState,
    generation: u64,
    request: &NativeReleaseRequest,
    viewport_release: F,
) -> HostResult<NativeReleaseReceipt>
where
    F: FnOnce() -> HostResult<(Vec<WorkId>, &'static str)>,
{
    let (mut authority, authority_poisoned) = match native.lock() {
        Ok(authority) => (authority, false),
        Err(poisoned) => (poisoned.into_inner(), true),
    };
    let phase = std::mem::replace(&mut authority.phase, NativePhase::Vacant);
    let NativePhase::Releasing {
        generation: active_generation,
        reentry_used,
        request_id: retained_request_id,
        fingerprint,
        application,
    } = phase
    else {
        authority.phase = phase;
        return Err(host_error("unavailable", "stale native release generation"));
    };
    if active_generation != generation || retained_request_id != request.request_id {
        authority.phase = NativePhase::Releasing {
            generation: active_generation,
            reentry_used,
            request_id: retained_request_id,
            fingerprint,
            application,
        };
        return Err(host_error("unavailable", "stale native release generation"));
    }
    let Some(mut application) = application else {
        return Ok(retain_failed_release(
            &mut authority,
            generation,
            reentry_used,
            retained_request_id,
            fingerprint,
            (
                request.instance_id.clone(),
                request.document_id.clone(),
                request.expected_revision,
            ),
            "native release application was unavailable during cleanup",
        ));
    };
    let retained_identity = (
        application.instance_id().to_owned(),
        application.document_id().to_owned(),
        application.content_revision(),
    );
    let released = catch_unwind_message(|| {
        application
            .as_any_mut()
            .downcast_mut::<DesktopNativeApplication>()
            .map(DesktopNativeApplication::release_project)
            .ok_or_else(|| "native desktop host was unavailable during cleanup".to_string())
    })
    .and_then(|released| released);
    let released = match released {
        Ok(released) => released,
        Err(message) => {
            drop(application);
            return Ok(retain_failed_release(
                &mut authority,
                generation,
                reentry_used,
                retained_request_id,
                fingerprint,
                retained_identity,
                &message,
            ));
        }
    };
    let viewport = catch_unwind_message(viewport_release)
        .unwrap_or_else(|message| Err(host_error("cleanup_failed", message)));
    drop(application);
    let mut cleanup_error = authority_poisoned
        .then(|| host_error("cleanup_failed", "native authority lock was poisoned"))
        .or_else(|| {
            released
                .preview_error
                .map(|message| host_error("cleanup_failed", message))
        })
        .or_else(|| viewport.as_ref().err().cloned());
    if cleanup_error.is_none() && !released.export_cleanup_complete {
        cleanup_error = Some(host_error(
            "cleanup_failed",
            "native export cleanup remained indeterminate",
        ));
    }
    let cleanup_complete = released.export_cleanup_complete && cleanup_error.is_none();
    let mut preview_ids: BTreeSet<WorkId> = released.cancelled_preview.into_iter().collect();
    if let Ok(viewport) = &viewport {
        preview_ids.extend(viewport.0.iter().copied());
    }
    let receipt = NativeReleaseReceipt {
        api_version: HOST_API_VERSION,
        request_id: request.request_id.clone(),
        instance_id: released.instance_id,
        document_id: released.document_id,
        content_revision: released.content_revision,
        lifecycle_generation: generation,
        status: if cleanup_complete {
            "succeeded"
        } else {
            "indeterminate"
        }
        .into(),
        retrieved: false,
        authority_removal_completed: true,
        cancelled_transaction_id: released.cancelled_transaction_id,
        cancelled_transaction: released.cancelled_transaction,
        undo_depth: released.undo_depth,
        redo_depth: released.redo_depth,
        reconciled_exports: released
            .reconciled_exports
            .iter()
            .map(reconcile_export)
            .collect(),
        cancelled_preview_work_ids: preview_ids.into_iter().map(work_label).collect(),
        viewport_status: viewport
            .as_ref()
            .map(|value| value.1)
            .unwrap_or("cleanup_failed")
            .into(),
        reentry_available: cleanup_complete && !reentry_used,
        error: cleanup_error,
    };
    authority.finish_release(ReleaseTombstone {
        generation,
        reentry_used,
        request_id: request.request_id.clone(),
        fingerprint,
        receipt: receipt.retained_value(),
        succeeded: cleanup_complete,
    });
    Ok(receipt)
}

fn retain_failed_release(
    authority: &mut NativeAuthority,
    generation: u64,
    reentry_used: bool,
    request_id: String,
    fingerprint: Vec<u8>,
    identity: (String, String, u64),
    message: &str,
) -> NativeReleaseReceipt {
    let receipt =
        NativeReleaseReceipt::indeterminate(request_id.clone(), identity, generation, message);
    authority.finish_release(ReleaseTombstone {
        generation,
        reentry_used,
        request_id,
        fingerprint,
        receipt: receipt.retained_value(),
        succeeded: false,
    });
    receipt
}

#[cfg(test)]
#[path = "native_application_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "native_application_release_tests.rs"]
mod release_tests;
