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
use std::sync::{Arc, Mutex};

#[cfg(test)]
pub(crate) use crate::native_viewport::replacement_test_support::{
    FakeOutcome, FakeViewport, SurfaceState,
};
#[cfg(test)]
pub(crate) type TestCompositionResult = CompositionResult;
#[cfg(test)]
pub(crate) type TestFrameIdentity = ScheduledFrameIdentity;
#[cfg(test)]
pub(crate) type TestViewportMapping = ViewportMapping;
#[cfg(test)]
pub(crate) type TestWorkId = WorkId;

struct RetainedViewport {
    instance_id: String,
    viewport: NativeViewport,
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

thread_local! {
    static MAIN_VIEWPORT: RefCell<Option<RetainedViewport>> = const { RefCell::new(None) };
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

pub(crate) async fn on_main_thread<T: Send + 'static>(
    app: &tauri::AppHandle,
    operation: impl FnOnce() -> HostResult<T> + Send + 'static,
) -> HostResult<T> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    app.run_on_main_thread(move || {
        complete_scheduled_operation(sender, operation);
    })
    .map_err(|_| host_error("unavailable", "native main-thread executor unavailable"))?;
    receiver
        .await
        .map_err(|_| host_error("unavailable", "native main-thread operation was dropped"))?
}

/// Release is committed once admitted, so dropping the awaiting caller cannot
/// prevent the main-thread callback from terminalizing the authority.
pub(crate) async fn on_main_thread_committed<T: Send + 'static>(
    app: &tauri::AppHandle,
    operation: impl FnOnce() -> HostResult<T> + Send + 'static,
) -> HostResult<T> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    app.run_on_main_thread(move || {
        complete_committed_operation(sender, operation);
    })
    .map_err(|_| host_error("unavailable", "native main-thread executor unavailable"))?;
    receiver
        .await
        .map_err(|_| host_error("unavailable", "native release callback was dropped"))?
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReplacementCallbackFailure {
    SchedulingUnavailable,
    CallbackDropped,
}

#[derive(Clone, Copy)]
enum ReplacementCallbackState {
    Submitting,
    Scheduled,
    Started,
    Completed,
    Discarded,
    Terminal,
}

type ReplacementFailureHook = Box<dyn FnOnce(ReplacementCallbackFailure) + Send>;

struct ReplacementCallbackOwner {
    state: ReplacementCallbackState,
    failure: Option<ReplacementFailureHook>,
}

struct ReplacementCallbackToken(Arc<Mutex<ReplacementCallbackOwner>>);

impl ReplacementCallbackOwner {
    fn transition(owner: &Arc<Mutex<Self>>, event: ReplacementCallbackState) {
        let failure = {
            let mut owner = owner
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            match (owner.state, event) {
                (ReplacementCallbackState::Submitting, ReplacementCallbackState::Scheduled) => {
                    owner.state = ReplacementCallbackState::Scheduled;
                    None
                }
                (ReplacementCallbackState::Discarded, ReplacementCallbackState::Scheduled) => {
                    owner.state = ReplacementCallbackState::Terminal;
                    owner
                        .failure
                        .take()
                        .map(|hook| (hook, ReplacementCallbackFailure::CallbackDropped))
                }
                (_, ReplacementCallbackState::Terminal) => {
                    owner.state = ReplacementCallbackState::Terminal;
                    owner
                        .failure
                        .take()
                        .map(|hook| (hook, ReplacementCallbackFailure::SchedulingUnavailable))
                }
                (
                    ReplacementCallbackState::Submitting | ReplacementCallbackState::Scheduled,
                    ReplacementCallbackState::Started,
                ) => {
                    owner.state = ReplacementCallbackState::Started;
                    None
                }
                (ReplacementCallbackState::Started, ReplacementCallbackState::Completed) => {
                    owner.state = ReplacementCallbackState::Completed;
                    owner.failure.take();
                    None
                }
                _ => None,
            }
        };
        if let Some((hook, failure)) = failure {
            hook(failure);
        }
    }
}

impl Drop for ReplacementCallbackToken {
    fn drop(&mut self) {
        let failure = {
            let mut owner = self
                .0
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            match owner.state {
                ReplacementCallbackState::Submitting => {
                    owner.state = ReplacementCallbackState::Discarded;
                    None
                }
                ReplacementCallbackState::Scheduled | ReplacementCallbackState::Started => {
                    owner.state = ReplacementCallbackState::Terminal;
                    owner.failure.take()
                }
                _ => None,
            }
        };
        if let Some(hook) = failure {
            hook(ReplacementCallbackFailure::CallbackDropped);
        }
    }
}

pub(crate) fn submit_replacement<T: Send + 'static>(
    schedule: impl FnOnce(Box<dyn FnOnce() + Send>) -> Result<(), ()>,
    operation: impl FnOnce() -> HostResult<T> + Send + 'static,
    on_failure: impl FnOnce(ReplacementCallbackFailure) + Send + 'static,
) -> Result<tokio::sync::oneshot::Receiver<HostResult<T>>, ReplacementCallbackFailure> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let owner = Arc::new(Mutex::new(ReplacementCallbackOwner {
        state: ReplacementCallbackState::Submitting,
        failure: Some(Box::new(on_failure)),
    }));
    let token = ReplacementCallbackToken(Arc::clone(&owner));
    let scheduled = schedule(Box::new(move || {
        ReplacementCallbackOwner::transition(&token.0, ReplacementCallbackState::Started);
        complete_committed_operation(sender, operation);
        ReplacementCallbackOwner::transition(&token.0, ReplacementCallbackState::Completed);
    }));
    if scheduled.is_err() {
        ReplacementCallbackOwner::transition(&owner, ReplacementCallbackState::Terminal);
        return Err(ReplacementCallbackFailure::SchedulingUnavailable);
    }
    ReplacementCallbackOwner::transition(&owner, ReplacementCallbackState::Scheduled);
    Ok(receiver)
}

/// Replacement is committed at generation reservation. Unlike ordinary preview
/// work, its callback must run even when the invoking waiter is cancelled.
pub(crate) async fn on_main_thread_replacement<T: Send + 'static>(
    app: &tauri::AppHandle,
    operation: impl FnOnce() -> HostResult<T> + Send + 'static,
    on_failure: impl FnOnce(ReplacementCallbackFailure) + Send + 'static,
) -> Result<HostResult<T>, ReplacementCallbackFailure> {
    let receiver = submit_replacement(
        |callback| app.run_on_main_thread(callback).map_err(|_| ()),
        operation,
        on_failure,
    )?;
    receiver
        .await
        .map_err(|_| ReplacementCallbackFailure::CallbackDropped)
}

fn complete_committed_operation<T>(
    sender: tokio::sync::oneshot::Sender<HostResult<T>>,
    operation: impl FnOnce() -> HostResult<T>,
) {
    let _ = sender.send(operation());
}

fn complete_scheduled_operation<T>(
    sender: tokio::sync::oneshot::Sender<HostResult<T>>,
    operation: impl FnOnce() -> HostResult<T>,
) {
    if !sender.is_closed() {
        let _ = sender.send(operation());
    }
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
    let retained = MAIN_VIEWPORT.with(|slot| {
        let mut slot = slot.borrow_mut();
        match slot.as_ref() {
            Some(value) if value.instance_id == instance_id => Ok(slot.take()),
            Some(_) => Err(host_error(
                "wrong_instance",
                "native viewport instance mismatch",
            )),
            None => Ok(None),
        }
    })?;
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
mod cancellation_tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };

    #[test]
    fn cancelled_main_thread_wait_does_not_begin_the_scheduled_operation() {
        let (sender, receiver) = tokio::sync::oneshot::channel::<HostResult<()>>();
        let began = Arc::new(AtomicBool::new(false));
        let began_in_operation = Arc::clone(&began);
        drop(receiver);

        complete_scheduled_operation(sender, move || {
            began_in_operation.store(true, Ordering::SeqCst);
            Ok(())
        });

        assert!(!began.load(Ordering::SeqCst));
    }

    #[test]
    fn committed_release_operation_runs_after_waiter_is_cancelled() {
        let (sender, receiver) = tokio::sync::oneshot::channel::<HostResult<()>>();
        let began = Arc::new(AtomicBool::new(false));
        let began_in_operation = Arc::clone(&began);
        drop(receiver);
        complete_committed_operation(sender, move || {
            began_in_operation.store(true, Ordering::SeqCst);
            Ok(())
        });
        assert!(began.load(Ordering::SeqCst));
    }

    #[test]
    fn committed_replacement_operation_runs_after_waiter_is_cancelled() {
        let (sender, receiver) = tokio::sync::oneshot::channel::<HostResult<()>>();
        let began = Arc::new(AtomicBool::new(false));
        let began_in_operation = Arc::clone(&began);
        drop(receiver);
        complete_committed_operation(sender, move || {
            began_in_operation.store(true, Ordering::SeqCst);
            Ok(())
        });
        assert!(began.load(Ordering::SeqCst));
    }
}
