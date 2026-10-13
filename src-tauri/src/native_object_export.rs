//! Private original-owner binding to the existing desktop artifact port.
use crate::native_dispatch::NativeObjectHost;
use native_engine::{
    compositor::Compositor,
    export_job::{
        export_one_object_frame, ObjectHostExportOutcome, ObjectHostExportRequest,
        StagedArtifactPort,
    },
    object_frame_packet::{prepare_object_frame, ObjectFrameSelector},
    object_render_scene::{
        prepare_object_composition, prepare_object_paint_plan, prepare_object_render_scene,
    },
    scheduler::{ObjectSchedulePins, ObjectScheduleScope, OutputSpec},
};

impl NativeObjectHost {
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
        if self.released.is_some() || self.object_export.is_some() {
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
        let packet = prepare_object_frame(&snapshot, expected)
            .map_err(|error| format!("object export selector: {error:?}"))?;
        let scene = prepare_object_render_scene(&packet, expected)
            .map_err(|error| format!("object export scene: {error:?}"))?;
        let plan = prepare_object_paint_plan(&scene, expected)
            .map_err(|error| format!("object export paint: {error:?}"))?;
        let witness = prepare_object_composition(&plan, expected)
            .map_err(|error| format!("object export witness: {error:?}"))?;
        let scheduler = self.preview.scheduler_for_export()?;
        let outcome = export_one_object_frame(
            &self.history,
            scheduler,
            port,
            compositor,
            ObjectHostExportRequest {
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
            },
            witness,
        );
        if !outcome.release.receipts.is_empty() || !outcome.release.cleanup_complete {
            self.object_export = Some(outcome.release.clone());
        }
        Ok(outcome)
    }
}
