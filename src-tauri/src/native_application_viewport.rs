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

pub(crate) use crate::native_viewport::ScheduledPreview;

#[cfg(test)]
pub(crate) use crate::native_viewport::replacement_test_support::{
    test_observations, FakeOutcome, FakeViewport, SurfaceState,
};
#[cfg(test)]
pub(crate) type TestCompositionResult = CompositionResult;
#[cfg(test)]
pub(crate) type TestFrameIdentity = ScheduledFrameIdentity;
#[cfg(test)]
pub(crate) type TestViewportMapping = ViewportMapping;
#[cfg(test)]
pub(crate) type TestWorkId = WorkId;

struct RetainedViewport<T = NativeViewport> {
    instance_id: String,
    viewport: T,
}

pub(crate) struct Presentation {
    pub(crate) work_id: WorkId,
    pub(crate) view_generation: u64,
    pub(crate) status: &'static str,
}

pub(crate) struct ViewportRelease {
    pub(crate) work_ids: Vec<WorkId>,
    pub(crate) status: &'static str,
}

#[path = "native_object_viewport.rs"]
mod object_viewport;
pub(crate) use crate::native_viewport::NativeViewportConsumer;
pub(crate) use object_viewport::with_object_target;
use object_viewport::MainViewport;
#[cfg(test)]
pub(crate) use object_viewport::TestObjectSlot;

thread_local! {
    static MAIN_VIEWPORT: RefCell<Option<RetainedViewport<MainViewport>>> = const { RefCell::new(None) };
}

#[cfg(test)]
pub(crate) trait TestViewportDriver {
    fn present(
        &mut self,
        compositor: &Compositor,
        result: &CompositionResult,
        identity: ScheduledFrameIdentity,
    ) -> HostResult<Presentation>;
    fn retire_replacement(&mut self) -> Vec<WorkId>;
    fn rebind_existing(&mut self, compositor: &Compositor) -> HostResult<()>;
}

#[cfg(test)]
struct RetainedTestViewport {
    instance_id: String,
    viewport: Box<dyn TestViewportDriver>,
}

#[cfg(test)]
thread_local! {
    static TEST_VIEWPORT: RefCell<Option<RetainedTestViewport>> = const { RefCell::new(None) };
}

#[cfg(test)]
pub(crate) fn install_test_viewport(instance_id: String, viewport: Box<dyn TestViewportDriver>) {
    TEST_VIEWPORT.with(|slot| {
        assert!(slot.borrow().is_none());
        *slot.borrow_mut() = Some(RetainedTestViewport {
            instance_id,
            viewport,
        });
    });
}

#[cfg(test)]
pub(crate) fn remove_test_viewport() -> Vec<WorkId> {
    TEST_VIEWPORT.with(|slot| {
        slot.borrow_mut()
            .take()
            .map_or_else(Vec::new, |mut value| value.viewport.retire_replacement())
    })
}

#[cfg(test)]
fn with_test_viewport<T>(
    instance_id: &str,
    operation: impl FnOnce(&mut dyn TestViewportDriver) -> HostResult<T>,
) -> Option<HostResult<T>> {
    TEST_VIEWPORT.with(|slot| {
        slot.borrow_mut().as_mut().map(|value| {
            if value.instance_id == instance_id {
                operation(value.viewport.as_mut())
            } else {
                Err(host_error(
                    "wrong_instance",
                    "native viewport instance mismatch",
                ))
            }
        })
    })
}

#[path = "native_viewport_callbacks.rs"]
mod callbacks;
pub(crate) use callbacks::{
    on_main_thread, on_main_thread_committed, on_main_thread_replacement, submit_replacement,
    ReplacementCallbackFailure,
};

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
            viewport: MainViewport::External(viewport),
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
    #[cfg(test)]
    if let Some(result) = with_test_viewport(instance_id, |_| Ok(())) {
        return result;
    }
    MAIN_VIEWPORT.with(|slot| match slot.borrow().as_ref() {
        Some(value) if value.instance_id != instance_id => Err(host_error(
            "wrong_instance",
            "native viewport instance mismatch",
        )),
        _ => Ok(()),
    })
}

/// The authority lock is held by the committed replacement callback. Release
/// A's retained surface/view before mutating its document or resources.
pub(crate) fn retire_replacement(instance_id: &str) -> HostResult<Vec<WorkId>> {
    #[cfg(test)]
    if let Some(result) =
        with_test_viewport(instance_id, |viewport| Ok(viewport.retire_replacement()))
    {
        return result;
    }
    MAIN_VIEWPORT.with(|slot| match slot.borrow_mut().as_mut() {
        Some(value) if value.instance_id == instance_id => Ok(value.viewport.retire_replacement()),
        Some(_) => Err(host_error(
            "wrong_instance",
            "native viewport instance mismatch",
        )),
        None => Ok(Vec::new()),
    })
}

/// Recreate only an empty surface on the original compositor context.
pub(crate) fn rebind_replacement(instance_id: &str, compositor: &Compositor) -> HostResult<()> {
    #[cfg(test)]
    if let Some(result) =
        with_test_viewport(instance_id, |viewport| viewport.rebind_existing(compositor))
    {
        return result;
    }
    MAIN_VIEWPORT.with(|slot| match slot.borrow_mut().as_mut() {
        Some(value) if value.instance_id == instance_id => value
            .viewport
            .rebind_existing(compositor)
            .map_err(|error| host_error("unavailable", error.to_string())),
        Some(_) => Err(host_error(
            "wrong_instance",
            "native viewport instance mismatch",
        )),
        None => Ok(()),
    })
}

pub(crate) fn present(
    instance_id: &str,
    compositor: &Compositor,
    result: &CompositionResult,
    identity: ScheduledFrameIdentity,
) -> HostResult<Presentation> {
    let diagnostic = crate::native_viewport::observe_preview(instance_id, result);
    #[cfg(test)]
    if let Some(result) = with_test_viewport(instance_id, |viewport| {
        viewport.present(compositor, result, identity.clone())
    }) {
        return result.inspect(|value| diagnostic.finish(value.status));
    }
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
    .inspect(|value| diagnostic.finish(value.status))
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

pub(crate) fn release(instance_id: &str) -> HostResult<ViewportRelease> {
    let retained = MAIN_VIEWPORT.with(|slot| object_viewport::take_slot(slot, instance_id))?;
    match retained {
        Some(mut retained) => Ok(ViewportRelease {
            work_ids: retained.viewport.dispose(),
            status: "disposed",
        }),
        None => Ok(ViewportRelease {
            work_ids: Vec::new(),
            status: "already_absent",
        }),
    }
}

pub(crate) fn with_borrowed_target<C: NativeViewportConsumer>(
    instance_id: &str,
    compositor: &mut Compositor,
    consumer: C,
) -> HostResult<C::Output> {
    with_viewport(instance_id, |viewport| {
        viewport
            .with_borrowed_target(compositor, consumer)
            .map_err(|error| host_error("unavailable", error.to_string()))
    })
}

fn with_viewport<T>(
    instance_id: &str,
    operation: impl FnOnce(&mut NativeViewport) -> HostResult<T>,
) -> HostResult<T> {
    MAIN_VIEWPORT.with(|slot| with_retained_target(slot, instance_id, |value| operation(value)))
}

fn with_retained_target<V, T>(
    slot: &RefCell<Option<RetainedViewport<V>>>,
    instance_id: &str,
    operation: impl FnOnce(&mut V) -> HostResult<T>,
) -> HostResult<T> {
    let mut slot = slot
        .try_borrow_mut()
        .map_err(|_| host_error("unavailable", "native viewport is already borrowed"))?;
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
}

/// One native object preview's occlusion-diagnostic scope, the object path's counterpart of the
/// scope `present` opens: opened from the request pins and composed work before register, closed
/// with the receipt's status. Dropped unclosed, it records that no receipt arrived.
#[derive(Default)]
pub(crate) struct ObjectPreviewDiagnostic(Option<crate::native_viewport::PreviewObservation>);

impl ObjectPreviewDiagnostic {
    pub(crate) fn open(&mut self, instance: &str, preview: &ScheduledPreview<'_>) {
        // An earlier scope closes first, so it cannot restore over this one.
        drop(self.0.take());
        self.0 = Some(crate::native_viewport::observe_preview(instance, preview));
    }

    pub(crate) fn finish(&self, status: &ViewportStatus) {
        if let Some(observation) = &self.0 {
            observation.finish(status_label(status));
        }
    }
}

pub(crate) fn status_label(status: &ViewportStatus) -> &'static str {
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

#[cfg(test)]
#[path = "native_viewport_borrow_tests.rs"]
mod borrow_tests;
