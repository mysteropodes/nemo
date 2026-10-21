//! Private original-owner binding to the existing desktop artifact port.
use crate::native_dispatch::NativeObjectHost;
use native_engine::{
    application::prepare_object_motion_exposure_frame_for_control,
    compositor::Compositor,
    export_job::{
        export_object_frames, export_object_svg_frames, CleanupStatus, ExportReleaseReconciliation,
        ExternalEffectDisposition, JobStatus, ObjectHostExportOutcome, ObjectHostExportRequest,
        StagedArtifactPort,
    },
    native_media_frame_export::{begin_native_media_frames, NativeMediaRelease, NativeVideoPort},
    object_frame_packet::ObjectFrameSelector,
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
    /// T12: what the export's video frames and decode sessions released.
    pub(super) media: Option<NativeMediaRelease>,
}
impl ObjectExportSession {
    fn permits_next(&self, request_id: &str) -> bool {
        self.request_id != request_id
            && self.media.as_ref().is_none_or(|media| media.complete)
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
        // T12: every drawn video is preflighted (typed MissingLink) before
        // staging; its decoded frames are released once the witnesses are gone.
        let mut decoder = crate::native_dispatch::video::video_decoder();
        let mut video_port = NativeVideoPort::new(decoder.as_mut());
        let mut media = begin_native_media_frames(
            &mut video_port,
            &snapshot,
            expected,
            frames,
            true,
            &crate::native_dispatch::video::linked_file_readable,
            crate::native_dispatch::video::now_ms(),
        )
        .map_err(|refusal| refusal.to_string())?;
        let (scheduler, motion_control) = self.preview.export_frame_ports()?;
        let mut witness_for = |frame| {
            let selector = ObjectFrameSelector {
                frame,
                ..expected.clone()
            };
            let packet = prepare_object_motion_exposure_frame_for_control(
                &self.history,
                &snapshot,
                &selector,
                motion_control,
            )
            .map_err(|error| format!("object export selector: {error}"))?;
            let scene = prepare_object_render_scene(&packet, &selector)
                .map_err(|error| format!("object export scene: {error:?}"))?;
            let plan = prepare_object_paint_plan(&scene, &selector)
                .map_err(|error| format!("object export paint: {error:?}"))?;
            let base = prepare_object_composition(&plan, &selector)
                .map_err(|error| format!("object export witness: {error:?}"))?;
            media.witness(frame, base)
        };
        // Bound the exact original request before any guest or artifact work.
        if frames.is_empty()
            || frames.len() > native_engine::export_job::MAX_OBJECT_EXPORT_FRAMES
            || frames.first() != Some(&expected.frame)
            || frames.windows(2).any(|pair| pair[0] >= pair[1])
        {
            return Err("native object export frames are invalid".into());
        }
        // Visit every actual requested frame once before staging. A later
        // timeout cannot leave an earlier artifact published or lose its effects.
        let mut prepared = std::collections::BTreeMap::new();
        let mut refusal = None;
        for &frame in frames {
            match witness_for(frame) {
                Ok(witness) => {
                    prepared.insert(frame, witness);
                }
                Err(error) => {
                    if refusal.is_none() {
                        refusal = Some(error);
                    }
                }
            }
        }
        if let Some(error) = refusal {
            return Err(error);
        }
        let witnesses = |frame| {
            prepared
                .remove(&frame)
                .ok_or_else(|| "object export witness already consumed".into())
        };
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
        // The witnesses (and the frames they hold) are gone with the closures.
        drop(prepared);
        let media = media.finish();
        if !outcome.release.receipts.is_empty()
            || !outcome.release.cleanup_complete
            || !media.complete
        {
            self.object_export = Some(ObjectExportSession {
                request_id: session_request_id,
                reconciliation: outcome.release.clone(),
                media: (media.frames_acquired > 0 || !media.complete).then_some(media),
            });
        }
        Ok(outcome)
    }
}

#[cfg(test)]
#[path = "native_object_svg_export_tests.rs"]
mod svg_tests;
