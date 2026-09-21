//! Dormant production composition root for the admitted native opacity host.
//!
//! N18A exposes strict commands but installs no startup caller. N20 alone may
//! select this host for a production document; N21 owns installed acceptance.

use crate::{
    native_application_contract::*,
    native_application_ports::{
        DesktopArtifactBindings, DesktopArtifactPort, DesktopResourceResolver, SharedCompositor,
    },
    native_dispatch::NativeDispatch,
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
use std::path::PathBuf;

type DesktopCore =
    NativeApplication<DesktopArtifactPort, SharedCompositor, DesktopResourceResolver>;

pub(crate) struct PreparedPreview {
    pub(crate) result: CompositionResult,
    pub(crate) identity: ScheduledFrameIdentity,
}

pub(crate) struct DesktopNativeApplication {
    core: DesktopCore,
    preview_resources: DesktopResourceResolver,
    preview_compositor: SharedCompositor,
    preview_scheduler: FrameScheduler,
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
            Ok(result) => Ok(PreparedPreview {
                result,
                identity: ScheduledFrameIdentity::from_scheduled(&scheduled),
            }),
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
        result
            .map(|_| ())
            .map_err(|error| host_error("internal", error.to_string()))
    }

    pub(crate) fn cancel_preview(&mut self, work_ids: &[WorkId]) -> HostResult<()> {
        for work_id in work_ids {
            self.preview_scheduler
                .cancel(*work_id)
                .map_err(|error| host_error("internal", error.to_string()))?;
        }
        Ok(())
    }

    pub(crate) fn require_identity(
        &self,
        instance: &str,
        document: &str,
        revision: u64,
    ) -> HostResult<()> {
        if instance != self.core.instance_id() {
            return Err(host_error(
                "wrong_instance",
                "native application instance mismatch",
            ));
        }
        if document != self.core.document_id() {
            return Err(host_error("wrong_document", "native document was replaced"));
        }
        if revision != self.core.content_revision() {
            return Err(host_error(
                "stale_revision",
                "native content revision is stale",
            ));
        }
        Ok(())
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

#[cfg(test)]
#[path = "native_application_tests.rs"]
mod tests;
