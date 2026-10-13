use crate::native_application_contract::{host_error, HostResult};
use std::sync::{Arc, Mutex};

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

#[cfg(test)]
mod cancellation_tests {
    use crate::native_application_contract::HostResult;
    use crate::native_application_viewport::callbacks::{
        complete_committed_operation, complete_scheduled_operation,
    };
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
