//! Main-thread confinement for the one optional native main-window viewport.

use crate::{
    native_application_contract::{host_error, HostResult},
    native_viewport::NativeViewport,
};
use native_engine::{
    compositor::{CompositionResult, Compositor},
    desktop_viewport::{DeferredAction, FatalAction, ViewportMapping, ViewportStatus},
    render_scene::ScheduledFrameIdentity,
    resource_leases::WorkId,
};
use std::cell::RefCell;

struct RetainedViewport {
    instance_id: String,
    viewport: NativeViewport,
}

pub(crate) struct Presentation {
    pub(crate) work_id: WorkId,
    pub(crate) view_generation: u64,
    pub(crate) status: &'static str,
}

thread_local! {
    static MAIN_VIEWPORT: RefCell<Option<RetainedViewport>> = const { RefCell::new(None) };
}

pub(crate) async fn on_main_thread<T: Send + 'static>(
    app: &tauri::AppHandle,
    operation: impl FnOnce() -> HostResult<T> + Send + 'static,
) -> HostResult<T> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    app.run_on_main_thread(move || {
        let _ = sender.send(operation());
    })
    .map_err(|_| host_error("unavailable", "native main-thread executor unavailable"))?;
    receiver
        .await
        .map_err(|_| host_error("unavailable", "native main-thread operation was dropped"))?
}

pub(crate) fn create(
    app: &tauri::AppHandle,
    instance_id: String,
    mapping: ViewportMapping,
) -> HostResult<Compositor> {
    let (viewport, compositor) = NativeViewport::from_app(app, "main", mapping)
        .map_err(|error| host_error("unavailable", error.to_string()))?;
    MAIN_VIEWPORT.with(|slot| {
        let mut slot = slot.borrow_mut();
        if slot.is_some() {
            return Err(host_error(
                "duplicate_bootstrap",
                "native viewport is already retained",
            ));
        }
        *slot = Some(RetainedViewport {
            instance_id,
            viewport,
        });
        Ok(compositor)
    })
}

pub(crate) fn remove(instance_id: &str) -> HostResult<()> {
    MAIN_VIEWPORT.with(|slot| {
        let mut slot = slot.borrow_mut();
        if slot
            .as_ref()
            .is_some_and(|value| value.instance_id == instance_id)
        {
            slot.take();
        }
        Ok(())
    })
}

pub(crate) fn require_instance_or_absent(instance_id: &str) -> HostResult<()> {
    MAIN_VIEWPORT.with(|slot| match slot.borrow().as_ref() {
        Some(value) if value.instance_id != instance_id => Err(host_error(
            "wrong_instance",
            "native viewport instance mismatch",
        )),
        _ => Ok(()),
    })
}

pub(crate) fn reconcile_replaced(
    instance_id: &str,
    work_ids: &[WorkId],
) -> HostResult<Vec<WorkId>> {
    MAIN_VIEWPORT.with(|slot| {
        let mut slot = slot.borrow_mut();
        match slot.as_mut() {
            Some(value) if value.instance_id == instance_id => {
                Ok(value.viewport.reconcile_replaced(work_ids))
            }
            Some(_) => Err(host_error(
                "wrong_instance",
                "native viewport instance mismatch",
            )),
            None => Ok(Vec::new()),
        }
    })
}

pub(crate) fn present(
    instance_id: &str,
    compositor: &Compositor,
    result: &CompositionResult,
    identity: ScheduledFrameIdentity,
) -> HostResult<Presentation> {
    with_viewport(instance_id, |viewport| {
        viewport.register(identity.clone()).map_err(|error| {
            host_error("internal", format!("register native preview: {error:?}"))
        })?;
        let receipt = viewport.present(compositor, result).map_err(|error| {
            host_error("internal", format!("present native preview: {error:?}"))
        })?;
        Ok(Presentation {
            work_id: identity.work_id(),
            view_generation: identity.view_generation().value(),
            status: status_label(receipt.status()),
        })
    })
}

pub(crate) fn resize(instance_id: &str, mapping: ViewportMapping) -> HostResult<()> {
    with_viewport(instance_id, |viewport| {
        viewport
            .resize(mapping)
            .map_err(|error| host_error("internal", format!("resize native viewport: {error:?}")))
    })
}

pub(crate) fn dispose(instance_id: &str) -> HostResult<Vec<WorkId>> {
    let mut retained = MAIN_VIEWPORT.with(|slot| {
        let mut slot = slot.borrow_mut();
        match slot.as_ref() {
            Some(value) if value.instance_id == instance_id => Ok(slot.take().unwrap()),
            Some(_) => Err(host_error(
                "wrong_instance",
                "native viewport instance mismatch",
            )),
            None => Err(host_error("unavailable", "native viewport is unavailable")),
        }
    })?;
    Ok(retained.viewport.dispose())
}

fn with_viewport<T>(
    instance_id: &str,
    operation: impl FnOnce(&mut NativeViewport) -> HostResult<T>,
) -> HostResult<T> {
    MAIN_VIEWPORT.with(|slot| {
        let mut slot = slot.borrow_mut();
        let retained = slot
            .as_mut()
            .ok_or_else(|| host_error("unavailable", "native viewport is unavailable"))?;
        if retained.instance_id != instance_id {
            return Err(host_error(
                "wrong_instance",
                "native viewport instance mismatch",
            ));
        }
        operation(&mut retained.viewport)
    })
}

fn status_label(status: &ViewportStatus) -> &'static str {
    match status {
        ViewportStatus::Presented => "presented",
        ViewportStatus::StaleDiscarded => "stale-discarded",
        ViewportStatus::Deferred(DeferredAction::Timeout) => "deferred-timeout",
        ViewportStatus::Deferred(DeferredAction::Occluded) => "deferred-occluded",
        ViewportStatus::Failed(FatalAction::ValidationFailure) => "failed-validation",
        ViewportStatus::Failed(FatalAction::DeviceLost) => "failed-device-lost",
        ViewportStatus::Failed(FatalAction::RetryLost) => "failed-retry-lost",
        ViewportStatus::Failed(FatalAction::RetryOutdated) => "failed-retry-outdated",
        ViewportStatus::Failed(FatalAction::RecreateFailed) => "failed-recreate",
        ViewportStatus::Failed(FatalAction::ReconfigureFailed) => "failed-reconfigure",
    }
}
