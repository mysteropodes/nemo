//! Owner-held work only: GPU and surface remain borrowed from the caller.
use crate::native_dispatch::{NativeObjectHost, NativeReleaseProgress};
use native_engine::{
    application::ApplicationReleaseReceipt,
    compositor::Compositor,
    desktop_viewport::{
        DesktopViewportHost, ObjectViewportFrame, PresentationReceipt, SurfacePort, ViewportStatus,
    },
    export_job::ReconciliationStage,
    object_frame_packet::{prepare_object_exposure_frame, ObjectFrameSelector},
    object_render_scene::{
        prepare_object_paint_plan, prepare_object_render_scene,
        prepare_object_viewport_composition, prepare_scheduled_object_composition_from_witness,
        ObjectCompositionWitness, ObjectPaintPlan,
    },
    object_snapshot::ObjectSnapshot,
    resource_leases::WorkId,
    scheduler::{
        EvaluationKey, FrameScheduler, ObjectSchedulePins, ObjectScheduleScope, OutputSpec,
        ScheduledFrame,
    },
};
use std::collections::BTreeSet;

// CPU-only admission; never exported, cloned or detached from this owner.
struct PreparedObjectPreview {
    snapshot: ObjectSnapshot,
    plan: ObjectPaintPlan,
    key: EvaluationKey,
}

#[derive(Default)]
pub(super) struct ObjectPreview {
    scheduler: FrameScheduler,
    prepared: Option<PreparedObjectPreview>,
    reference_cache: Option<ObjectCompositionWitness>,
    pending: Option<ScheduledFrame>,
    blocked: Option<String>,
    released: bool,
    cancelled_on_release: BTreeSet<WorkId>,
    #[cfg(test)]
    fail_cancel: bool,
}

impl ObjectPreview {
    /// Lend only the original scheduler when no preview lease or cleanup debt exists.
    pub(super) fn scheduler_for_export(&mut self) -> Result<&mut FrameScheduler, String> {
        if self.released || self.pending.is_some() || self.blocked.is_some() {
            return Err("native object preview work must settle before export".into());
        }
        Ok(&mut self.scheduler)
    }

    fn cancel_pending(&mut self) -> Result<(), String> {
        let Some(work) = self.pending.as_ref() else {
            return Ok(());
        };
        self.blocked = Some("preview cancellation did not complete".into());
        #[cfg(test)]
        if self.fail_cancel {
            return Err("injected preview cancellation failure".into());
        }
        let id = work.work_id();
        self.scheduler
            .cancel(id)
            .map_err(|error| error.to_string())?;
        self.pending = None;
        if self.released {
            self.cancelled_on_release.insert(id);
        }
        self.blocked = None;
        Ok(())
    }
    pub(super) fn invalidate(&mut self) {
        self.prepared = None;
        if let Err(error) = self.cancel_pending() {
            self.blocked = Some(error);
        }
    }
    pub(super) fn release(&mut self) {
        // Mark before cancellation so unwind/retry retains the exact checkpoint.
        self.released = true;
        self.invalidate();
        self.reference_cache = None;
    }
    pub(super) fn progress(&self, application: ApplicationReleaseReceipt) -> NativeReleaseProgress {
        NativeReleaseProgress {
            application,
            cancelled_preview: self.cancelled_on_release.iter().copied().collect(),
            unresolved_preview: self.pending.iter().map(|work| work.work_id()).collect(),
            preview_error: self.blocked.clone(),
            preview_stage: if self.pending.is_none() && self.blocked.is_none() {
                ReconciliationStage::Complete
            } else if self.blocked.is_some() {
                ReconciliationStage::Unknown
            } else {
                ReconciliationStage::Pending
            },
        }
    }
}

impl NativeObjectHost {
    /// Internal owner operation only. A later common-authority route must hold
    /// its lifecycle/revision admission throughout this synchronous call.
    /// Only final RGBA8/sRGB/straight output with no external resources is admitted.
    pub(crate) fn present_object_preview<P: SurfacePort>(
        &mut self,
        expected: &ObjectFrameSelector<'_>,
        output: OutputSpec,
        compositor: &mut Compositor,
        viewport: &mut DesktopViewportHost<P>,
    ) -> Result<PresentationReceipt, String> {
        self.with_prepared_object_preview(expected, output, |owner| {
            owner.present_prepared_object_preview(expected, compositor, viewport)
        })
    }

    /// Selector/output/paint preparation precedes any target factory while shared
    /// guards remain held. Raster decode follows retirement of the old GPU cache.
    /// The private one-shot token cannot cross owners or callback turns.
    pub(crate) fn with_prepared_object_preview<T>(
        &mut self,
        expected: &ObjectFrameSelector<'_>,
        output: OutputSpec,
        operation: impl FnOnce(&mut Self) -> Result<T, String>,
    ) -> Result<T, String> {
        if self.preview.prepared.is_some() {
            return Err("native object preview preparation is already borrowed".into());
        }
        let prepared = self.prepare_object_preview(expected, output)?;
        self.preview.prepared = Some(prepared);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| operation(self)));
        self.preview.prepared = None;
        match result {
            Ok(result) => result,
            Err(payload) => std::panic::resume_unwind(payload),
        }
    }

    fn prepare_object_preview(
        &self,
        expected: &ObjectFrameSelector<'_>,
        output: OutputSpec,
    ) -> Result<PreparedObjectPreview, String> {
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
        let packet = prepare_object_exposure_frame(&snapshot, expected)
            .map_err(|e| format!("preview selector: {e:?}"))?;
        let scene = prepare_object_render_scene(&packet, expected)
            .map_err(|e| format!("preview scene: {e:?}"))?;
        let plan = prepare_object_paint_plan(&scene, expected)
            .map_err(|e| format!("preview paint: {e:?}"))?;
        if output.kind() != "frame"
            || output.format() != "rgba8"
            || output.color_interpretation() != "srgb"
            || output.alpha_mode() != "straight"
        {
            return Err("native object preview output is unavailable".into());
        }
        let key = EvaluationKey::new(
            snapshot.snapshot_id(),
            expected.context_id,
            expected.frame,
            "final",
            output,
            plan.raster_resource_versions()
                .map_err(|error| format!("preview raster resources: {error:?}"))?,
        )
        .map_err(|e| e.to_string())?;
        Ok(PreparedObjectPreview {
            snapshot,
            plan,
            key,
        })
    }

    pub(crate) fn present_prepared_object_preview<P: SurfacePort>(
        &mut self,
        expected: &ObjectFrameSelector<'_>,
        compositor: &mut Compositor,
        viewport: &mut DesktopViewportHost<P>,
    ) -> Result<PresentationReceipt, String> {
        let prepared = self
            .preview
            .prepared
            .take()
            .ok_or("native object preview has no current CPU preparation")?;
        if self.released.is_some() || self.preview.released || self.preview.blocked.is_some() {
            return Err("native object preview owner requires cleanup or was released".into());
        }
        let current = self
            .history
            .acquire_snapshot(self.history.content_revision())
            .ok_or("native object preview current snapshot is unavailable")?;
        let packet = prepared.plan.packet();
        if current.snapshot_id() != prepared.snapshot.snapshot_id()
            || current.instance_id() != expected.instance_id
            || current.document_id() != expected.document_id
            || current.content_revision() != expected.content_revision
            || packet.document_snapshot_id() != expected.document_snapshot_id
            || packet.context_id() != expected.context_id
            || packet.scope_kind() != &expected.scope_kind
            || packet.frame() != expected.frame
        {
            return Err("native object preview prepared pins are stale or foreign".into());
        }
        let PreparedObjectPreview {
            snapshot,
            plan,
            key,
        } = prepared;
        let composition = self.prepare_reference_preview(
            &plan,
            key.output_spec().dimensions(),
            compositor,
            |previous| {
                prepare_object_viewport_composition(&plan, expected, previous)
                    .map_err(|e| format!("preview reference: {e:?}"))
            },
        )?;
        if self
            .preview
            .pending
            .as_ref()
            .is_some_and(|work| work.key() != &key)
        {
            self.preview.cancel_pending().map_err(|error| {
                self.preview.blocked = Some(error.clone());
                error
            })?;
        }
        let work = self
            .preview
            .scheduler
            .schedule_object(
                snapshot,
                ObjectSchedulePins {
                    instance_id: expected.instance_id.into(),
                    document_id: expected.document_id.into(),
                    content_revision: expected.content_revision,
                    document_snapshot_id: expected.document_snapshot_id.into(),
                    context_id: expected.context_id.into(),
                    frame: expected.frame,
                    scope: ObjectScheduleScope::Authored,
                },
                key,
            )
            .map_err(|e| e.to_string())?;
        // Retain the actual scheduler work before compose/register/acquire/settle.
        // An Err or unwind cannot lose the work that release must reconcile.
        self.preview.pending = Some(work.clone());
        self.preview.blocked = Some("preview presentation did not complete".into());
        let result = (|| {
            let live = prepare_scheduled_object_composition_from_witness(
                &self.preview.scheduler,
                &work,
                &composition,
            )
            .map_err(|e| format!("preview admission: {e:?}"))?;
            let frame =
                ObjectViewportFrame::compose(compositor, &live).map_err(|e| e.to_string())?;
            viewport
                .register(frame.identity().clone())
                .map_err(|e| format!("preview register: {e:?}"))?;
            frame
                .present(viewport, compositor, &live)
                .map_err(|e| format!("preview present: {e:?}"))
        })();
        let receipt = match result {
            Ok(receipt) => receipt,
            Err(error) => {
                self.preview.blocked = Some(error.clone());
                return Err(error);
            }
        };
        match receipt.status() {
            ViewportStatus::Presented => {
                self.preview
                    .scheduler
                    .succeed(work.work_id())
                    .map_err(|e| e.to_string())?;
                self.preview.pending = None;
                self.preview.blocked = None;
            }
            ViewportStatus::Deferred(_) => self.preview.blocked = None,
            ViewportStatus::StaleDiscarded => self.preview.cancel_pending()?,
            ViewportStatus::Failed(reason) => {
                self.preview.blocked = Some(format!("preview failed: {reason:?}"))
            }
        }
        Ok(receipt)
    }

    fn prepare_reference_preview(
        &mut self,
        plan: &ObjectPaintPlan,
        dimensions: (u32, u32),
        compositor: &mut Compositor,
        decode: impl FnOnce(
            Option<&ObjectCompositionWitness>,
        ) -> Result<ObjectCompositionWitness, String>,
    ) -> Result<ObjectCompositionWitness, String> {
        // Both Vello and the host retain charged pixels. A failed reset leaves
        // both intact; a failed decode leaves neither stale cache and is retryable.
        compositor
            .retire_object_reference_for(plan, dimensions)
            .map_err(|error| error.to_string())?;
        if self
            .preview
            .reference_cache
            .as_ref()
            .is_some_and(|previous| !previous.reference_matches_plan(plan))
        {
            self.preview.reference_cache = None;
        }
        let composition = decode(self.preview.reference_cache.as_ref())?;
        if composition.has_scene_images() {
            self.preview.reference_cache = Some(composition.clone());
        } else {
            self.preview.reference_cache = None;
        }
        Ok(composition)
    }
}

#[cfg(test)]
#[path = "native_object_reference_preview_tests.rs"]
mod reference_tests;

#[cfg(test)]
#[path = "native_object_preview_lifecycle_tests.rs"]
mod lifecycle_tests;
#[cfg(test)]
#[path = "native_object_preview_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "native_raster_image_preview_tests.rs"]
mod raster_tests;
