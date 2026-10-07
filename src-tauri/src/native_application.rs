//! Dormant native opacity composition root; N20 alone may activate it.

#[cfg(test)]
pub(crate) use crate::native_dispatch::{
    NativeAuthority, NativePhase, NativeState, ReleaseAdmission,
};
use crate::{
    native_application_contract::*,
    native_application_ports::{
        DesktopArtifactBindings, DesktopArtifactPort, DesktopResourceResolver, SharedCompositor,
    },
    native_dispatch::{NativeDispatch, NativeReleaseProgress},
};
use native_engine::{
    application::{
        ApplicationReleaseReceipt, NativeApplication, ReproductionStatus, REPRODUCTION_FIXTURE,
    },
    compositor::CompositionResult,
    document::OpacityDocument,
    export_job::{JobReceipt, PendingFrame, ReconciliationStage},
    render_scene::{self, ScheduledFrameIdentity},
    resource_leases::{FrameFailure, FrameFailureKind, WorkId},
    scheduler::{EvaluationKey, FrameScheduler},
};
use std::{collections::BTreeSet, path::PathBuf};

#[path = "native_application_release.rs"]
mod release;
pub(crate) use release::{admit_release_request, complete_release};

#[path = "native_application_replacement.rs"]
mod replacement;
pub(crate) use replacement::PreparedDesktopReplacement;

#[cfg(test)]
pub(crate) use crate::native_dispatch::run_export_pump_interleaved;

type DesktopCore =
    NativeApplication<DesktopArtifactPort, SharedCompositor, DesktopResourceResolver>;

pub(crate) struct PreparedPreview {
    pub(crate) result: CompositionResult,
    pub(crate) identity: ScheduledFrameIdentity,
}

struct PreviewReleaseProgress {
    cancelled: BTreeSet<WorkId>,
    unresolved: BTreeSet<WorkId>,
}

pub(crate) struct DesktopNativeApplication {
    core: DesktopCore,
    preview_resources: DesktopResourceResolver,
    preview_compositor: SharedCompositor,
    preview_scheduler: FrameScheduler,
    preview_work: BTreeSet<WorkId>,
    artifact_bindings: DesktopArtifactBindings,
    preview_release: Option<PreviewReleaseProgress>,
    #[cfg(test)]
    panic_release_after_transaction: bool,
    #[cfg(test)]
    panic_release_after_export_jobs: Option<usize>,
    #[cfg(test)]
    panic_release_after_preview_jobs: Option<usize>,
    #[cfg(test)]
    fail_release_preview_cancel_at: Option<usize>,
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
        Ok(Self::from_core(core, resources, compositor, bindings))
    }

    /// Only the explicit catalog-session host route grants synthetic provenance.
    /// Ordinary bootstrap and replacement always keep their existing constructors.
    pub(crate) fn from_reproduction_fixture(
        instance_id: String,
        artifacts: DesktopArtifactPort,
        compositor: SharedCompositor,
    ) -> HostResult<Self> {
        let bindings = artifacts.bindings();
        let resources = DesktopResourceResolver::new(Vec::new())
            .map_err(|message| host_error("internal", message))?;
        let mut core = NativeApplication::from_reproduction_fixture(
            instance_id,
            REPRODUCTION_FIXTURE,
            artifacts,
            compositor.clone(),
            resources.clone(),
        )
        .map_err(|_| host_error("unavailable", "native reproduction catalog unavailable"))?;
        core.opt_in_reproduction()
            .map_err(|_| host_error("unavailable", "native reproduction opt-in unavailable"))?;
        Ok(Self::from_core(core, resources, compositor, bindings))
    }

    fn from_core(
        core: DesktopCore,
        resources: DesktopResourceResolver,
        compositor: SharedCompositor,
        bindings: DesktopArtifactBindings,
    ) -> Self {
        Self {
            core,
            preview_resources: resources,
            preview_compositor: compositor,
            preview_scheduler: FrameScheduler::new(),
            preview_work: BTreeSet::new(),
            artifact_bindings: bindings,
            preview_release: None,
            #[cfg(test)]
            panic_release_after_transaction: false,
            #[cfg(test)]
            panic_release_after_export_jobs: None,
            #[cfg(test)]
            panic_release_after_preview_jobs: None,
            #[cfg(test)]
            fail_release_preview_cancel_at: None,
        }
    }

    pub(crate) fn reproduction_status(&self) -> ReproductionStatus {
        self.core.reproduction_status()
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

    pub(crate) fn release_project(&mut self) -> NativeReleaseProgress {
        self.core.release_transaction_stage();
        #[cfg(test)]
        if self.panic_release_after_transaction {
            panic!("injected core release panic after transaction reconciliation");
        }
        #[cfg(test)]
        let application: ApplicationReleaseReceipt = {
            let mut reconciled = 0;
            let panic_after = self.panic_release_after_export_jobs;
            self.core.release_export_stage_with_checkpoint(|| {
                reconciled += 1;
                if panic_after == Some(reconciled) {
                    panic!("injected core release panic after {reconciled} export job");
                }
            })
        };
        #[cfg(not(test))]
        let application: ApplicationReleaseReceipt = self.core.release_export_stage();
        let pending = self.preview_work.clone();
        self.preview_release = Some(PreviewReleaseProgress {
            cancelled: BTreeSet::new(),
            unresolved: pending.clone(),
        });
        let mut preview_error = None;
        for (index, work_id) in pending.into_iter().enumerate() {
            #[cfg(not(test))]
            let _ = index;
            #[cfg(test)]
            if self.fail_release_preview_cancel_at == Some(index) {
                preview_error = Some("injected preview scheduler cancellation failure".into());
                break;
            }
            if let Err(error) = self.preview_scheduler.cancel(work_id) {
                preview_error = Some(error.to_string());
                break;
            }
            self.preview_work.remove(&work_id);
            let progress = self.preview_release.as_mut().unwrap();
            progress.unresolved.remove(&work_id);
            progress.cancelled.insert(work_id);
            #[cfg(test)]
            if self.panic_release_after_preview_jobs == Some(index + 1) {
                panic!(
                    "injected core release panic after {} preview job",
                    index + 1
                );
            }
        }
        let progress = self.preview_release.as_ref().unwrap();
        NativeReleaseProgress {
            application,
            cancelled_preview: progress.cancelled.iter().copied().collect(),
            unresolved_preview: progress.unresolved.iter().copied().collect(),
            preview_stage: if preview_error.is_some() {
                ReconciliationStage::Unknown
            } else {
                ReconciliationStage::Complete
            },
            preview_error,
        }
    }

    fn release_progress(&self) -> Option<NativeReleaseProgress> {
        self.core.release_progress().map(|application| {
            let (cancelled, unresolved) = self.preview_release.as_ref().map_or_else(
                || (Vec::new(), self.preview_work.iter().copied().collect()),
                |progress| {
                    (
                        progress.cancelled.iter().copied().collect(),
                        progress.unresolved.iter().copied().collect(),
                    )
                },
            );
            NativeReleaseProgress {
                application,
                cancelled_preview: cancelled,
                unresolved_preview: unresolved,
                preview_error: None,
                preview_stage: ReconciliationStage::Unknown,
            }
        })
    }

    #[cfg(test)]
    fn inject_release_panic_after_transaction(&mut self) {
        self.panic_release_after_transaction = true;
    }

    #[cfg(test)]
    fn inject_release_panic_after_export_jobs(&mut self, jobs: usize) {
        self.panic_release_after_export_jobs = Some(jobs);
    }

    #[cfg(test)]
    fn inject_release_panic_after_preview_jobs(&mut self, jobs: usize) {
        self.panic_release_after_preview_jobs = Some(jobs);
    }

    #[cfg(test)]
    fn inject_release_preview_cancel_failure_at(&mut self, index: usize) {
        self.fail_release_preview_cancel_at = Some(index);
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
    ) -> Result<native_engine::commands::ResponseEnvelope, String> {
        Ok(self.core.dispatch(request))
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
    fn release_project(&mut self) -> Result<NativeReleaseProgress, String> {
        Ok(DesktopNativeApplication::release_project(self))
    }
    fn release_progress(&self) -> Option<NativeReleaseProgress> {
        DesktopNativeApplication::release_progress(self)
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

#[cfg(test)]
#[path = "native_application_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "native_application_release_tests.rs"]
mod release_tests;

#[cfg(test)]
#[path = "native_application_dispatch_release_tests.rs"]
mod dispatch_release_tests;

#[cfg(test)]
mod dispatch_envelope_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn opacity_success_and_failure_envelope_bytes_match_the_unchanged_core() {
        let scratch = release_tests::Scratch::new();
        let mut application = release_tests::desktop(&scratch, None);
        for (operation, payload, success) in [
            ("query.document.revision", json!({}), true),
            (
                "query.document.opacity",
                json!({"stableTarget":{"layerUid":"missing"}}),
                false,
            ),
        ] {
            let request = native_engine::commands::OpacityRequest::query(
                operation,
                application.instance_id(),
                application.document_id(),
                operation,
                payload,
            );
            let expected = application.core.dispatch(request.clone());
            assert_eq!(expected.is_ok(), success);
            let actual = NativeDispatch::dispatch(&mut application, request).unwrap();
            assert_eq!(actual.is_ok(), success);
            assert_eq!(
                serde_json::to_vec(&actual).unwrap(),
                serde_json::to_vec(&expected).unwrap()
            );
        }
    }
}
