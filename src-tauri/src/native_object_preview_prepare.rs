//! CPU-only preview admission. The existing host retains every mutable owner.
use crate::native_dispatch::NativeObjectHost;
use native_engine::{
    application::{
        prepare_object_motion_exposure_frame_for_control,
        prepare_object_motion_fallback_frame_for_control,
    },
    desktop_viewport::{ObjectPreviewNavigation, ObjectPreviewView},
    object_frame_packet::ObjectFrameSelector,
    object_render_scene::{ObjectBrushAdmission, ObjectPaintPlan},
    object_snapshot::ObjectSnapshot,
    scheduler::{EvaluationKey, OutputSpec},
};

// Never exported, cloned or detached from the existing preview owner.
pub(super) struct PreparedObjectPreview {
    pub(super) snapshot: ObjectSnapshot,
    pub(super) admission: ObjectBrushAdmission,
    pub(super) output: OutputSpec,
    pub(super) view: ObjectPreviewView,
    pub(super) navigation: ObjectPreviewNavigation,
}

impl PreparedObjectPreview {
    /// Only a materialized native plan supplies resource versions. CPU admission
    /// alone is not a paint plan, scheduler key or presentation proof.
    pub(super) fn materialized_key(
        &self,
        plan: &ObjectPaintPlan,
    ) -> Result<EvaluationKey, String> {
        let packet = self.admission.packet();
        EvaluationKey::new(
            self.snapshot.snapshot_id(),
            packet.context_id(),
            packet.frame(),
            self.navigation.quality(self.view),
            self.output.clone(),
            plan.scene_resource_versions()
                .map_err(|error| format!("preview scene resources: {error:?}"))?,
        )
        .map_err(|error| error.to_string())
    }
}

impl NativeObjectHost {
    pub(super) fn prepare_object_preview(
        &mut self,
        expected: &ObjectFrameSelector<'_>,
        output: OutputSpec,
        pending_proof_fallback: bool,
    ) -> Result<PreparedObjectPreview, String> {
        self.prepare_object_preview_view(
            expected,
            output,
            ObjectPreviewView::Canvas,
            pending_proof_fallback,
        )
    }

    pub(super) fn prepare_object_preview_view(
        &mut self,
        expected: &ObjectFrameSelector<'_>,
        output: OutputSpec,
        view: ObjectPreviewView,
        pending_proof_fallback: bool,
    ) -> Result<PreparedObjectPreview, String> {
        self.prepare_object_preview_navigation(
            expected,
            output,
            view,
            ObjectPreviewNavigation::default(),
            pending_proof_fallback,
        )
    }
    pub(super) fn prepare_object_preview_navigation(
        &mut self,
        expected: &ObjectFrameSelector<'_>,
        output: OutputSpec,
        view: ObjectPreviewView,
        navigation: ObjectPreviewNavigation,
        pending_proof_fallback: bool,
    ) -> Result<PreparedObjectPreview, String> {
        navigation.validate()?;
        if self.released.is_some() || self.preview.released {
            return Err("native object host was released".into());
        }
        if let Some(error) = &self.preview.blocked {
            return Err(format!("native object preview requires cleanup: {error}"));
        }
        if expected.content_revision != self.history.content_revision() {
            return Err("native object preview revision is stale".into());
        }
        let snapshot = self
            .history
            .acquire_snapshot(expected.content_revision)
            .ok_or("native object preview snapshot is unavailable")?;
        // Validate the complete retained source before disturbing deferred work.
        let prepared = prepare_object_motion_exposure_frame_for_control(
            &self.history,
            &snapshot,
            expected,
            &mut self.preview.motion_control,
        );
        let packet = match prepared {
            Ok(packet) => packet,
            Err(error) if pending_proof_fallback => {
                prepare_object_motion_fallback_frame_for_control(
                    &self.history,
                    &snapshot,
                    expected,
                    &self.preview.motion_control,
                )
                .map_err(|_| format!("preview selector: {error}"))?
            }
            Err(error) => return Err(format!("preview selector: {error}")),
        };
        let admission = ObjectBrushAdmission::prepare(&packet, expected)
            .map_err(|error| format!("preview scene admission: {error:?}"))?;
        if output.kind() != "frame"
            || output.format() != "rgba8"
            || output.color_interpretation() != "srgb"
            || output.alpha_mode() != "straight"
        {
            return Err("native object preview output is unavailable".into());
        }
        Ok(PreparedObjectPreview {
            snapshot,
            admission,
            output,
            view,
            navigation,
        })
    }
}
