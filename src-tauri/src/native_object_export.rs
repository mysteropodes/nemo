//! Private original-owner binding to the existing desktop artifact port.
use crate::native_dispatch::NativeObjectHost;
use native_engine::{
    compositor::Compositor,
    export_job::{
        export_object_frames, export_object_svg_frames, CleanupStatus, ExportReleaseReconciliation,
        ExternalEffectDisposition, JobStatus, ObjectHostExportOutcome, ObjectHostExportRequest,
        StagedArtifactPort,
    },
    object_frame_packet::{prepare_object_exposure_frame, ObjectFrameSelector},
    object_render_scene::{
        prepare_object_composition, prepare_object_paint_plan, prepare_object_render_scene,
    },
    scheduler::{ObjectSchedulePins, ObjectScheduleScope, OutputSpec},
};

/// Keep the last session's identity and actual reconciliation, including debt.
/// This is an immediate replay barrier, not a historical request-ID registry.
pub(super) struct ObjectExportSession {
    request_id: String,
    pub(super) reconciliation: ExportReleaseReconciliation,
}
impl ObjectExportSession {
    fn permits_next(&self, request_id: &str) -> bool {
        self.request_id != request_id
            && self.reconciliation.cleanup_complete
            && matches!(self.reconciliation.receipts.as_slice(), [receipt]
                if receipt.status == JobStatus::Succeeded
                && receipt.external_effect_disposition == ExternalEffectDisposition::Committed
                && receipt.cleanup.status == CleanupStatus::Complete
                && receipt.cleanup.error.is_none()
                && receipt.error.is_none()
                && receipt.artifact.is_some())
    }
}

enum ObjectExportFormat<'c> {
    Png(&'c mut Compositor),
    Svg,
}

impl NativeObjectHost {
    #[cfg(test)]
    pub(crate) fn export_test_state(&mut self) -> Result<(u64, (usize, usize)), String> {
        Ok((
            self.preview.scheduler_for_export()?.lease_counters().live(),
            self.history.history_depths(),
        ))
    }

    /// A synchronous private source consumer. The later application route must
    /// lend the original desktop port and GPU under its native/revision guards.
    pub(crate) fn export_pinned_object_png<P: StagedArtifactPort>(
        &mut self,
        expected: &ObjectFrameSelector<'_>,
        request_id: String,
        target: String,
        output: OutputSpec,
        maximum_encoded_bytes: usize,
        port: &mut P,
        compositor: &mut Compositor,
    ) -> Result<ObjectHostExportOutcome, String> {
        self.export_pinned_object_png_frames(
            expected,
            &[expected.frame],
            request_id,
            target,
            output,
            maximum_encoded_bytes,
            port,
            compositor,
        )
    }
    pub(crate) fn export_pinned_object_png_frames<P: StagedArtifactPort>(
        &mut self,
        expected: &ObjectFrameSelector<'_>,
        frames: &[u32],
        request_id: String,
        target: String,
        output: OutputSpec,
        maximum_encoded_bytes: usize,
        port: &mut P,
        compositor: &mut Compositor,
    ) -> Result<ObjectHostExportOutcome, String> {
        self.export_pinned_object_frames(
            expected,
            frames,
            request_id,
            target,
            output,
            maximum_encoded_bytes,
            port,
            ObjectExportFormat::Png(compositor),
        )
    }
    pub(crate) fn export_pinned_object_svg_frames<P: StagedArtifactPort>(
        &mut self,
        expected: &ObjectFrameSelector<'_>,
        frames: &[u32],
        request_id: String,
        target: String,
        output: OutputSpec,
        maximum_encoded_bytes: usize,
        port: &mut P,
    ) -> Result<ObjectHostExportOutcome, String> {
        self.export_pinned_object_frames(
            expected,
            frames,
            request_id,
            target,
            output,
            maximum_encoded_bytes,
            port,
            ObjectExportFormat::Svg,
        )
    }
    fn export_pinned_object_frames<P: StagedArtifactPort>(
        &mut self,
        expected: &ObjectFrameSelector<'_>,
        frames: &[u32],
        request_id: String,
        target: String,
        output: OutputSpec,
        maximum_encoded_bytes: usize,
        port: &mut P,
        format: ObjectExportFormat<'_>,
    ) -> Result<ObjectHostExportOutcome, String> {
        if self.released.is_some()
            || self
                .object_export
                .as_ref()
                .is_some_and(|session| !session.permits_next(&request_id))
        {
            return Err("native object export session is unavailable".into());
        }
        if expected.instance_id != self.history.instance_id()
            || expected.document_id != self.history.document_id()
            || expected.content_revision != self.history.content_revision()
        {
            return Err("native object export owner is stale".into());
        }
        let snapshot = self
            .history
            .acquire_snapshot(expected.content_revision)
            .ok_or("native object export snapshot is unavailable")?;
        let witness_for = |frame| {
            let selector = ObjectFrameSelector {
                frame,
                ..expected.clone()
            };
            let packet = prepare_object_exposure_frame(&snapshot, &selector)
                .map_err(|error| format!("object export selector: {error:?}"))?;
            let scene = prepare_object_render_scene(&packet, &selector)
                .map_err(|error| format!("object export scene: {error:?}"))?;
            let plan = prepare_object_paint_plan(&scene, &selector)
                .map_err(|error| format!("object export paint: {error:?}"))?;
            prepare_object_composition(&plan, &selector)
                .map_err(|error| format!("object export witness: {error:?}"))
        };
        // Preserve the single-frame pre-staging selector refusal contract.
        let mut first = Some(witness_for(expected.frame)?);
        let witnesses = |frame| {
            if frame == expected.frame {
                first
                    .take()
                    .ok_or_else(|| "initial object witness already consumed".into())
            } else {
                witness_for(frame)
            }
        };
        let scheduler = self.preview.scheduler_for_export()?;
        let session_request_id = request_id.clone();
        let request = ObjectHostExportRequest {
            request_id,
            target,
            pins: ObjectSchedulePins {
                instance_id: expected.instance_id.into(),
                document_id: expected.document_id.into(),
                content_revision: expected.content_revision,
                document_snapshot_id: expected.document_snapshot_id.into(),
                context_id: expected.context_id.into(),
                frame: expected.frame,
                scope: ObjectScheduleScope::Authored,
            },
            output,
            maximum_encoded_bytes,
        };
        let outcome = match format {
            ObjectExportFormat::Png(compositor) => export_object_frames(
                &self.history,
                scheduler,
                port,
                compositor,
                request,
                frames,
                witnesses,
            ),
            ObjectExportFormat::Svg => {
                export_object_svg_frames(&self.history, scheduler, port, request, frames, witnesses)
            }
        };
        if !outcome.release.receipts.is_empty() || !outcome.release.cleanup_complete {
            self.object_export = Some(ObjectExportSession {
                request_id: session_request_id,
                reconciliation: outcome.release.clone(),
            });
        }
        Ok(outcome)
    }
}

#[cfg(test)]
#[path = "native_object_svg_export_tests.rs"]
mod svg_tests;
