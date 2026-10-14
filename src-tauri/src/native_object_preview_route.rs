//! Private synchronous application route; no transport or capability activation.
use crate::application_mcp::ApplicationMcp;
use crate::native_dispatch::NativeObjectHost;
use crate::native_object_desktop_preview::PreviewRevisionProof;
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
        self.with_object_preview_owner(lifecycle_generation, expected, |owner| {
            owner.present_object_preview(expected, output, compositor, viewport)
        })
    }

    /// Admission and the callback share the original native/revision guards.
    /// A main-thread adapter may obtain its original target only inside here.
    pub(crate) fn with_object_preview_owner<T>(
        &self,
        lifecycle_generation: u64,
        expected: &ObjectFrameSelector<'_>,
        operation: impl FnOnce(&mut NativeObjectHost) -> Result<T, String>,
    ) -> Result<T, String> {
        self.with_object_preview_owner_for_preview(lifecycle_generation, expected, None, operation)
    }

    /// Only the private desktop preview route supplies this exact pending-event
    /// proof. All other owner consumers use the strict ordinary route above.
    pub(crate) fn with_object_preview_owner_for_preview<T>(
        &self,
        lifecycle_generation: u64,
        expected: &ObjectFrameSelector<'_>,
        revision_sync: Option<&PreviewRevisionProof>,
        operation: impl FnOnce(&mut NativeObjectHost) -> Result<T, String>,
    ) -> Result<T, String> {
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
        if let Some(pending) = revisions.pending.as_ref() {
            let exact_subscriber_preview = revision_sync.is_some_and(|proof| {
                let event = &proof.event;
                !pending.failed
                    && !pending.acknowledged
                    && pending.sender.is_some()
                    && revisions.subscriber.as_ref().is_some_and(|subscriber| {
                        subscriber.subscription_id == proof.subscription_id
                            && subscriber.instance_id == expected.instance_id
                            && subscriber.document_id == expected.document_id
                            && subscriber.lifecycle_generation == lifecycle_generation
                    })
                    && pending.event.instance_id == event.instance_id
                    && pending.event.document_id == event.document_id
                    && pending.event.lifecycle_generation == event.lifecycle_generation
                    && pending.event.request_id == event.request_id
                    && pending.event.from_revision == event.from_revision
                    && pending.event.to_revision == event.to_revision
                    && pending.event.to_revision == expected.content_revision
            });
            if !exact_subscriber_preview {
                return Err("native revision synchronization is pending or indeterminate; preview was not dispatched".into());
            }
        } else if revision_sync.is_some() {
            return Err("native revision preview proof has no pending event".into());
        }
        let owner = application
            .as_any_mut()
            .downcast_mut::<NativeObjectHost>()
            .ok_or("object preview requires the original native object owner")?;
        // B2B checkpoints pending work before calling SurfacePort. Catch only
        // this consumer unwind so its debt remains releasable without poisoning
        // either shared application lock. No success receipt is synthesized.
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| operation(owner)))
            .map_err(|_| "native object preview panicked; owner cleanup is required".to_string())?
    }
    #[cfg(test)]
    pub(crate) fn preview_locks_held_for_test(&self) -> bool {
        matches!(
            self.native.try_lock(),
            Err(std::sync::TryLockError::WouldBlock)
        ) && matches!(
            self.revisions.0.try_lock(),
            Err(std::sync::TryLockError::WouldBlock)
        )
    }
    #[cfg(test)]
    pub(crate) fn external_object_preview_for_test(
        &self,
        raw: &str,
    ) -> Result<
        (
            nemo_mcp::contract::NativeApplicationResponse,
            Option<(serde_json::Value, tokio::sync::oneshot::Receiver<()>)>,
        ),
        String,
    > {
        let delivery = self.revisions.dispatch_object(
            self.instance_id(),
            &self.native,
            crate::application_mcp::object_client::admit_raw(raw)?,
            true,
        )?;
        let notification = delivery.notification.map(|(event, receiver)| {
            (
                serde_json::to_value(event).expect("revision event is serializable"),
                receiver,
            )
        });
        Ok((delivery.response, notification))
    }
    #[cfg(test)]
    pub(crate) fn settle_object_preview_for_test(
        &self,
        event: serde_json::Value,
        acknowledged: bool,
    ) -> bool {
        let event = serde_json::from_value(event).expect("actual emitted revision event");
        self.revisions.settle(&self.native, &event, acknowledged)
    }
}

#[cfg(test)]
#[path = "native_object_preview_route_tests.rs"]
mod tests;
