//! Owner-held work only: GPU and surface remain borrowed from the caller.
use crate::native_dispatch::{NativeObjectHost, NativeReleaseProgress};
use native_engine::desktop_viewport::{
    ObjectPresentedFrame, ObjectPreviewNavigation, ObjectPreviewView, ObjectViewProjection,
    ViewportDisplayFilter,
};
use native_engine::{
    application::{ApplicationReleaseReceipt, ObjectBrushRuntime, ObjectMotionFrameControl},
    compositor::Compositor,
    desktop_viewport::{
        DesktopViewportHost, ObjectViewportFrame, PresentationReceipt, SurfacePort, ViewportStatus,
    },
    export_job::ReconciliationStage,
    native_media_frame_export::{
        begin_native_media_frames, declared_scene_resources, NativeVideoPort,
    },
    object_frame_packet::ObjectFrameSelector,
    object_render_scene::{
        prepare_object_navigated_viewport_composition,
        prepare_scheduled_object_composition_from_witness, ObjectCompositionWitness,
    },
    resource_leases::WorkId,
    scheduler::{
        EvaluationKey, FrameScheduler, ObjectSchedulePins, ObjectScheduleScope, OutputSpec,
        ScheduledFrame,
    },
};
use std::collections::BTreeSet;

#[path = "native_object_preview_prepare.rs"]
mod prepare;
#[path = "native_object_reference_preview.rs"]
mod reference_preview;
use prepare::PreparedObjectPreview;
#[path = "native_object_image_mesh_read.rs"]
mod image_mesh_read;
#[path = "native_object_subselect_read.rs"]
mod subselect_read;

#[derive(Default)]
pub(super) struct ObjectPreview {
    scheduler: FrameScheduler,
    pub(super) brush_runtime: ObjectBrushRuntime,
    prepared: Option<PreparedObjectPreview>,
    reference_cache: Option<ObjectCompositionWitness>,
    presented_mesh_view: Option<PresentationReceipt>,
    // Required original C02 effects remain at this actual owner. They never
    // become a render-time history edit or disappear into an error string.
    pub(super) motion_control: ObjectMotionFrameControl,
    pending: Option<ScheduledFrame>,
    pending_filter: Option<ViewportDisplayFilter>,
    blocked: Option<String>,
    released: bool,
    cancelled_on_release: BTreeSet<WorkId>,
    #[cfg(test)]
    fail_cancel: bool,
}

impl ObjectPreview {
    pub(super) fn preparation_borrowed(&self) -> bool {
        self.prepared.is_some()
    }

    /// Lend only the original scheduler when no preview lease or cleanup debt exists.
    pub(super) fn scheduler_for_export(&mut self) -> Result<&mut FrameScheduler, String> {
        if self.released || self.pending.is_some() || self.blocked.is_some() {
            return Err("native object preview work must settle before export".into());
        }
        Ok(&mut self.scheduler)
    }

    pub(super) fn export_frame_ports(
        &mut self,
    ) -> Result<(&mut FrameScheduler, &mut ObjectMotionFrameControl), String> {
        self.scheduler_for_export()?;
        Ok((&mut self.scheduler, &mut self.motion_control))
    }

    pub(super) fn resources_for_export(
        &mut self,
    ) -> Result<
        (
            &mut FrameScheduler,
            &mut ObjectMotionFrameControl,
            &mut ObjectBrushRuntime,
        ),
        String,
    > {
        if self.prepared.is_some() {
            return Err("native preview admission is already borrowed".into());
        }
        self.scheduler_for_export()?;
        Ok((
            &mut self.scheduler,
            &mut self.motion_control,
            &mut self.brush_runtime,
        ))
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
        self.pending_filter = None;
        if self.released {
            self.cancelled_on_release.insert(id);
        }
        self.blocked = None;
        Ok(())
    }
    pub(super) fn invalidate(&mut self) {
        self.presented_mesh_view = None;
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
        self.brush_runtime.release();
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
    #[cfg(test)]
    pub(crate) fn fail_preview_cancel_for_test(&mut self, fail: bool) {
        self.preview.fail_cancel = fail;
    }
    /// Internal owner operation only. A later common-authority route must hold
    /// its lifecycle/revision admission throughout this synchronous call.
    /// Only final RGBA8/sRGB/straight output with retained native resources is admitted.
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
        self.with_prepared_object_preview_for_view(
            expected,
            output,
            ObjectPreviewView::Canvas,
            false,
            operation,
        )
    }

    /// Called only after the existing exact pending-event proof route admits it.
    /// Pure fallback never publishes diagnostics, drains receipts or edits history.
    pub(crate) fn with_prepared_object_preview_for_revision_sync<T>(
        &mut self,
        expected: &ObjectFrameSelector<'_>,
        output: OutputSpec,
        operation: impl FnOnce(&mut Self) -> Result<T, String>,
    ) -> Result<T, String> {
        self.with_prepared_object_preview_for_view(
            expected,
            output,
            ObjectPreviewView::Canvas,
            true,
            operation,
        )
    }

    pub(crate) fn with_prepared_object_preview_for_view<T>(
        &mut self,
        expected: &ObjectFrameSelector<'_>,
        output: OutputSpec,
        view: ObjectPreviewView,
        pending_proof_fallback: bool,
        operation: impl FnOnce(&mut Self) -> Result<T, String>,
    ) -> Result<T, String> {
        self.with_prepared_object_preview_for_navigation(
            expected,
            output,
            view,
            ObjectPreviewNavigation::default(),
            pending_proof_fallback,
            operation,
        )
    }
    pub(crate) fn with_prepared_object_preview_for_navigation<T>(
        &mut self,
        expected: &ObjectFrameSelector<'_>,
        output: OutputSpec,
        view: ObjectPreviewView,
        navigation: ObjectPreviewNavigation,
        pending_proof_fallback: bool,
        operation: impl FnOnce(&mut Self) -> Result<T, String>,
    ) -> Result<T, String> {
        if self.preview.prepared.is_some() {
            return Err("native object preview preparation is already borrowed".into());
        }
        let prepared = self.prepare_object_preview_navigation(
            expected,
            output,
            view,
            navigation,
            pending_proof_fallback,
        )?;
        self.preview.prepared = Some(prepared);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| operation(self)));
        self.preview.prepared = None;
        match result {
            Ok(result) => result,
            Err(payload) => std::panic::resume_unwind(payload),
        }
    }

    pub(crate) fn present_prepared_object_preview<P: SurfacePort>(
        &mut self,
        expected: &ObjectFrameSelector<'_>,
        compositor: &mut Compositor,
        viewport: &mut DesktopViewportHost<P>,
    ) -> Result<PresentationReceipt, String> {
        self.present_prepared_object_preview_view(expected, compositor, viewport)
            .map(|result| result.receipt)
    }

    pub(crate) fn present_prepared_object_preview_view<P: SurfacePort>(
        &mut self,
        expected: &ObjectFrameSelector<'_>,
        compositor: &mut Compositor,
        viewport: &mut DesktopViewportHost<P>,
    ) -> Result<ObjectPresentedFrame, String> {
        self.present_prepared_object_preview_filtered(
            expected,
            compositor,
            viewport,
            ViewportDisplayFilter::None,
        )
    }

    pub(crate) fn present_prepared_object_preview_filtered<P: SurfacePort>(
        &mut self,
        expected: &ObjectFrameSelector<'_>,
        compositor: &mut Compositor,
        viewport: &mut DesktopViewportHost<P>,
        display_filter: ViewportDisplayFilter,
    ) -> Result<ObjectPresentedFrame, String> {
        let prepared = self
            .preview
            .prepared
            .take()
            .ok_or("native object preview has no current CPU preparation")?;
        self.preview.presented_mesh_view = None;
        if self.released.is_some() || self.preview.released || self.preview.blocked.is_some() {
            return Err("native object preview owner requires cleanup or was released".into());
        }
        let current = self
            .history
            .acquire_snapshot(self.history.content_revision())
            .ok_or("native object preview current snapshot is unavailable")?;
        let packet = prepared.admission.packet();
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
        // Cancel a different source or viewport identity before brush retirement.
        // The same immutable admission and navigation may retain deferred work.
        let quality = prepared.navigation.quality(prepared.view);
        if self.preview.pending.as_ref().is_some_and(|work| {
            let key = work.key();
            key.document_snapshot_id() != prepared.snapshot.snapshot_id()
                || key.context_id() != expected.context_id
                || key.frame() != expected.frame
                || key.output_spec() != &prepared.output
                || key.quality() != quality.as_str()
                || self.preview.pending_filter != Some(display_filter)
        }) {
            self.preview.cancel_pending()?;
        }
        let plan = self
            .preview
            .brush_runtime
            .materialize(&prepared.admission, &mut |ids| {
                compositor
                    .retire_object_brush_resources(ids, prepared.output.dimensions())
                    .is_ok()
            })
            .map_err(|error| format!("preview brush resources: {error:?}"))?;
        let key = prepared.materialized_key(&plan)?;
        let PreparedObjectPreview {
            snapshot,
            view,
            navigation,
            ..
        } = prepared;
        // T12: the frame's videos (MissingLink refused before any decode). Media is
        // attached after the reference cache so the cache never holds a lease.
        let mut decoder = crate::native_dispatch::video::video_decoder();
        let mut video_port = NativeVideoPort::new(decoder.as_mut());
        let mut media = begin_native_media_frames(
            &mut video_port,
            &snapshot,
            expected,
            &[expected.frame],
            true,
            &crate::native_dispatch::video::linked_file_readable,
            crate::native_dispatch::video::now_ms(),
        )
        .map_err(|refusal| refusal.to_string())?;
        let composition = self.prepare_reference_preview(
            &plan,
            key.output_spec().dimensions(),
            compositor,
            |previous| {
                prepare_object_navigated_viewport_composition(
                    &plan,
                    expected,
                    previous,
                    view == ObjectPreviewView::StoredCamera,
                    navigation.zoom(),
                    navigation.pan(),
                )
                .map_err(|e| format!("preview reference: {e:?}"))
            },
        )?;
        let view = ObjectViewProjection::from_navigation(
            view,
            navigation,
            composition
                .viewport_document_transform()
                .ok_or("preview requires an actual viewport witness")?,
            composition.viewport_stored_camera_available(),
        )?;
        let composition = media.witness(expected.frame, composition)?;
        let key = if media.draws_video(expected.frame) {
            EvaluationKey::new(
                key.document_snapshot_id(),
                key.context_id(),
                key.frame(),
                key.quality(),
                key.output_spec().clone(),
                declared_scene_resources(&composition),
            )
            .map_err(|e| e.to_string())?
        } else {
            key
        };
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
        self.preview.pending_filter = Some(display_filter);
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
                .register_with_display_filter(frame.identity().clone(), display_filter)
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
        // The presented frame no longer needs its decoded video.
        drop(composition);
        let media_released = media.finish().complete;
        match receipt.status() {
            ViewportStatus::Presented => {
                self.preview
                    .scheduler
                    .succeed(work.work_id())
                    .map_err(|e| e.to_string())?;
                self.preview.pending = None;
                self.preview.pending_filter = None;
                self.preview.blocked = None;
            }
            ViewportStatus::Deferred(_) => self.preview.blocked = None,
            ViewportStatus::StaleDiscarded => self.preview.cancel_pending()?,
            ViewportStatus::Failed(reason) => {
                self.preview.blocked = Some(format!("preview failed: {reason:?}"))
            }
        }
        if !media_released {
            // A leaked decoded frame blocks the next preview until cleanup.
            self.preview.blocked = Some("preview video frames were not released".into());
        }
        if media_released && receipt.status() == &ViewportStatus::Presented {
            self.preview.presented_mesh_view = Some(receipt.clone());
        }
        Ok(ObjectPresentedFrame { receipt, view })
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

#[cfg(test)]
#[path = "native_object_motion_tests.rs"]
pub(crate) mod motion_tests;
