//! Private synchronous application route; no transport or capability activation.
use crate::application_mcp::ApplicationMcp;
use crate::native_dispatch::NativeObjectHost;
use native_engine::{
    compositor::Compositor,
    desktop_viewport::{DesktopViewportHost, PresentationReceipt, SurfacePort},
    object_frame_packet::ObjectFrameSelector,
    scheduler::OutputSpec,
};

impl ApplicationMcp {
    /// The later desktop adapter must supply its original GPU and viewport on
    /// the main thread. The native owner and revision barrier remain locked for
    /// the full borrowed presentation; no owner reference or raw pixels escape.
    pub(crate) fn present_object_preview<P: SurfacePort>(
        &self,
        lifecycle_generation: u64,
        expected: &ObjectFrameSelector<'_>,
        output: OutputSpec,
        compositor: &mut Compositor,
        viewport: &mut DesktopViewportHost<P>,
    ) -> Result<PresentationReceipt, String> {
        if expected.instance_id != self.instance_id() {
            return Err("object preview application instance is stale".into());
        }
        // Match the existing dispatcher/control lock order. Neither guard may
        // be dropped between admission and the synchronous consumer call.
        let mut authority = self
            .native
            .lock()
            .map_err(|_| "native application lock unavailable")?;
        if authority.active_generation()? != lifecycle_generation {
            return Err("object preview lifecycle generation is stale".into());
        }
        let application = authority.active_mut(lifecycle_generation)?;
        if application.instance_id() != expected.instance_id
            || application.document_id() != expected.document_id
            || application.content_revision() != expected.content_revision
            || !application.object_client_available()
        {
            return Err("object preview requires the current original object owner".into());
        }
        let mut revisions = self
            .revisions
            .0
            .lock()
            .map_err(|_| "native revision lock unavailable")?;
        revisions.reconcile(lifecycle_generation, application.document_id());
        if revisions.pending.is_some() {
            return Err("native revision synchronization is pending or indeterminate; preview was not dispatched".into());
        }
        let owner = application
            .as_any_mut()
            .downcast_mut::<NativeObjectHost>()
            .ok_or("object preview requires the original native object owner")?;
        // B2B checkpoints pending work before calling SurfacePort. Catch only
        // this consumer unwind so its debt remains releasable without poisoning
        // either shared application lock. No success receipt is synthesized.
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            owner.present_object_preview(expected, output, compositor, viewport)
        }))
        .map_err(|_| "native object preview panicked; owner cleanup is required".to_string())?
    }
}

#[cfg(test)]
#[path = "native_object_preview_route_tests.rs"]
mod tests;
