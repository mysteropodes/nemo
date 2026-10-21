//! Trusted owner-receipt finalizer. No client operation or getter can edit history.
use crate::native_dispatch::{NativeDispatch, NativeObjectHost};
use native_engine::{
    application::{finalize_object_motion_frame_effects_for_owner, ObjectMotionEffectsCommit},
    object_frame_packet::ObjectFrameSelector,
};

impl NativeObjectHost {
    pub(crate) fn pending_object_motion_frames(&self) -> Vec<u32> {
        self.preview.motion_control.pending_frames()
    }
    pub(crate) fn object_motion_requires_revision(&self) -> bool {
        self.preview.motion_control.requires_revision()
    }
    pub(crate) fn finalize_object_motion_effects(
        &mut self,
        expected: &ObjectFrameSelector<'_>,
        frames: &[u32],
    ) -> Result<Option<ObjectMotionEffectsCommit>, String> {
        if self.released.is_some() || self.preview.preparation_borrowed() {
            return Err("native motion effects require a live owner after CPU read returns".into());
        }
        if !self.preview.motion_control.has_pending() {
            return Ok(None);
        }
        let committed = finalize_object_motion_frame_effects_for_owner(
            &mut self.history,
            expected,
            frames,
            &mut self.preview.motion_control,
        )?;
        // A committed receipt is always returned even if old work cleanup fails;
        // RevisionSync must retain/notify that actual revision before reporting it.
        if committed.changed() {
            self.preview.invalidate();
        }
        Ok(Some(committed))
    }
    pub(super) fn motion_diagnostic_records(&self) -> serde_json::Value {
        self.preview.motion_control.diagnostic_records()
    }
    pub(super) fn motion_diagnostic_for(
        &self,
        snapshot: &native_engine::object_snapshot::ObjectSnapshot,
        target: &native_engine::object_document::PropertyHolderTarget,
        property: &str,
        frame: f64,
    ) -> Option<serde_json::Value> {
        self.preview
            .motion_control
            .diagnostic_for(snapshot, target, property, frame)
    }
    #[cfg(test)]
    pub(crate) fn motion_diagnostic_for_test(&self, frame: f64) -> Option<serde_json::Value> {
        let snapshot = self.history.acquire_snapshot(self.content_revision())?;
        let target: native_engine::object_document::PropertyHolderTarget =
            serde_json::from_value(serde_json::json!({
                "contextId":"scene-root", "layerUid":"L0"
            })).ok()?;
        self.motion_diagnostic_for(&snapshot, &target, "dupOffsetPos", frame)
    }
    #[cfg(test)]
    pub(crate) fn motion_state_for_test(
        &self,
    ) -> (
        native_engine::object_snapshot::ObjectSnapshot,
        (usize, usize),
        String,
    ) {
        (
            self.history
                .acquire_snapshot(self.content_revision())
                .unwrap(),
            self.history.history_depths(),
            self.preview.motion_control.diagnostics(),
        )
    }
}
