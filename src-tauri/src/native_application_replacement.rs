//! Prepared desktop replacement; every fallible stage is visible to the host fence.

use crate::native_application::{
    host_error, AdmittedProject, DesktopNativeApplication, DesktopResourceResolver, HostResult,
    JobReceipt, OpacityDocument, WorkId,
};
use crate::native_dispatch::{ReplacementProgress, ReplacementStage};

pub(crate) struct PreparedDesktopReplacement {
    document: OpacityDocument,
    resources: DesktopResourceResolver,
    pub(crate) resource_count: usize,
}

impl DesktopNativeApplication {
    pub(crate) fn prepare_replacement(
        admitted: AdmittedProject,
    ) -> HostResult<PreparedDesktopReplacement> {
        let resource_count = admitted.resources.len();
        let resources = DesktopResourceResolver::new(admitted.resources)
            .map_err(|message| host_error("invalid_request", message))?;
        Ok(PreparedDesktopReplacement {
            document: admitted.document,
            resources,
            resource_count,
        })
    }

    pub(crate) fn replace_project_prepared(
        &mut self,
        prepared: PreparedDesktopReplacement,
        progress: &mut ReplacementProgress,
        mut checkpoint: impl FnMut(&'static str) -> HostResult<()>,
    ) -> HostResult<(Vec<JobReceipt>, Vec<WorkId>)> {
        progress.core = ReplacementStage::Running;
        let exports = self
            .core
            .replace_document(prepared.document)
            .map_err(|message| host_error("internal", message))?;
        progress.core = ReplacementStage::Complete;
        checkpoint("core")?;

        progress.preview = ReplacementStage::Running;
        let preview = self
            .preview_scheduler
            .replace_document(self.core.document_id().to_owned())
            .map_err(|error| host_error("internal", error.to_string()))?;
        for receipt in &preview {
            self.preview_work.remove(&receipt.work_id());
        }
        progress.preview = ReplacementStage::Complete;
        checkpoint("preview")?;

        progress.resources = ReplacementStage::Running;
        *self.core.resource_resolver_mut() = prepared.resources.clone();
        self.preview_resources = prepared.resources;
        progress.resources = ReplacementStage::Complete;
        checkpoint("resources")?;

        Ok((
            exports,
            preview
                .into_iter()
                .map(|receipt| receipt.work_id())
                .collect(),
        ))
    }
}
