//! Public object consumption stays in the viewport layer above composition.
use crate::compositor::{Compositor, CompositorError, ScheduledObjectCompositionResult};
use crate::desktop_viewport::{
    DesktopViewportHost, PresentationReceipt, SurfacePort, ViewportError,
};
use crate::object_render_scene::LiveObjectComposition;
use crate::render_scene::ScheduledFrameIdentity;

/// Retained pixels with no raw texture, GPU, readback or publication escape.
/// A retained frame alone grants no right to present; each consumption requires
/// a fresh live permit from the same scheduler and the original compositor.
pub struct ObjectViewportFrame {
    result: ScheduledObjectCompositionResult,
}

impl ObjectViewportFrame {
    /// Use the caller's existing GPU owner without creating a device or surface.
    pub fn compose(
        compositor: &mut Compositor,
        live: &LiveObjectComposition<'_>,
    ) -> Result<Self, CompositorError> {
        compositor
            .compose_scheduled_object(live)
            .map(|result| Self { result })
    }

    /// Read-only registration identity, never a consumption capability.
    pub fn identity(&self) -> &ScheduledFrameIdentity {
        self.result.identity()
    }

    /// Hold the same live scheduler borrow throughout acquire and settlement.
    /// The existing host rechecks work, GPU and extent before its cached receipt
    /// and retains its existing deferred/recovery/idempotent behavior.
    pub fn present<P: SurfacePort>(
        &self,
        viewport: &mut DesktopViewportHost<P>,
        compositor: &Compositor,
        live: &LiveObjectComposition<'_>,
    ) -> Result<PresentationReceipt, ViewportError> {
        viewport.present_scheduled_object(compositor, &self.result, live)
    }
}

#[cfg(all(test, feature = "history"))]
#[path = "../tests/object_viewport_consumer.rs"]
mod tests;
#[cfg(all(test, feature = "history", feature = "export_job"))]
#[path = "../tests/viewport_scene_controls.rs"]
mod viewport_scene_controls;
#[cfg(all(test, feature = "history", feature = "export_job"))]
#[path = "../tests/viewport_scene_packet_parity.rs"]
mod viewport_scene_packet_parity;
#[cfg(all(test, feature = "history", feature = "export_job"))]
#[path = "../tests/viewport_scene_stale_matrix.rs"]
mod viewport_scene_stale_matrix;
#[cfg(all(test, feature = "history", feature = "export_job"))]
#[path = "../tests/viewport_scene_support.rs"]
mod viewport_scene_support;
