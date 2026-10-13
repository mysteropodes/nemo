//! One finite original-main emitter; ACKs remain provisional.
use super::*;
use footer::{pins, refuse, Stage};
use serde::Serialize;
use tauri::Emitter;
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Payload {
    run_id: String,
    load_generation: u32,
    cut_id: String,
    phase: &'static str,
}
pub(super) struct DispatchWork {
    slot: Weak<Mutex<Slot>>,
    holder: Weak<Mutex<Option<Arc<Store>>>>,
    cancel: Arc<AtomicBool>,
    deadline: Instant,
    exited: Arc<AtomicBool>,
}
impl Drop for DispatchWork {
    fn drop(&mut self) {
        self.exited.store(true, Ordering::Release);
    }
}
pub(super) fn prepare(
    state: &StartupTransport,
    label: &str,
    url: &tauri::Url,
    pid: u32,
) -> Result<DispatchWork, &'static str> {
    if !super::super::super::allowed_origin(label, url) || pid != std::process::id() {
        return Err(ERROR);
    }
    let mut slot = state.0.lock().map_err(|_| ERROR)?;
    let binding = slot.owner.as_ref().ok_or(ERROR)?;
    if !binding.armed
        || binding.cancel.load(Ordering::Acquire)
        || !binding.original_store()
        || slot.exchange.armed
        || slot.exchange.stage != Stage::Idle
    {
        return Err(ERROR);
    }
    let work = DispatchWork {
        slot: Arc::downgrade(&state.0),
        holder: binding.holder.clone(),
        cancel: binding.cancel.clone(),
        deadline: binding.deadline.ok_or(ERROR)?,
        exited: slot.exchange.exited.clone(),
    };
    slot.exchange.armed = true;
    Ok(work)
}
pub(super) fn arm<R: Runtime>(window: &WebviewWindow<R>) {
    let label = window.label();
    let url = window.url();
    let pid = std::process::id();
    let state = window.app_handle().state::<StartupTransport>();
    let work = url
        .map_err(|_| ())
        .ok()
        .and_then(|u| prepare(&state, label, &u, pid).ok());
    if let Some(work) = work {
        let window = window.clone(); // Finite worker only, never stored in metadata.
        if std::thread::Builder::new()
            .name("nemo-startup-exchange".into())
            .spawn(move || {
                run(work, |payload| {
                    window
                        .emit_to(
                            tauri::EventTarget::webview_window(window.label()),
                            "nemo-test-startup-cut",
                            payload,
                        )
                        .map_err(|_| ())
                })
            })
            .is_err()
        {
            let _ = refuse(&state);
        }
    } else {
        let _ = refuse(&state);
    }
}
pub(super) fn next(state: &StartupTransport) -> Result<Option<Payload>, &'static str> {
    let mut slot = state.0.lock().map_err(|_| ERROR)?;
    let binding = slot.owner.as_ref().ok_or(ERROR)?;
    if binding.cancel.load(Ordering::Acquire) {
        return Err(ERROR);
    }
    if matches!(
        binding.session.state(),
        owner::State::Ready | owner::State::Checking
    ) {
        return Ok(None);
    }
    let native = slot.bridge.as_ref().ok_or(ERROR)?.native.clone();
    let log = native.lock().map_err(|_| ERROR)?;
    let current = pins(&mut slot, &log)?; // Includes deadline AFTER the native lock wait.
    if slot.exchange.busy {
        return Ok(None);
    }
    let phase = match slot.exchange.stage {
        Stage::Idle if current.phase == super::super::cut::Phase::Requested => {
            slot.owner
                .as_mut()
                .ok_or(ERROR)?
                .session
                .pending()?
                .begin_drain()?;
            slot.exchange.stage = Stage::WaitDrain;
            "drain"
        }
        Stage::DrainAcked => {
            slot.exchange.stage = Stage::WaitSnapshot;
            "snapshot"
        }
        Stage::WaitDrain | Stage::WaitSnapshot | Stage::Complete => return Ok(None),
        _ => return Err(ERROR),
    };
    Ok(Some(Payload {
        run_id: log.run.clone(),
        load_generation: 1,
        cut_id: current.cut,
        phase,
    }))
}
pub(super) fn run(work: DispatchWork, mut emit: impl FnMut(Payload) -> Result<(), ()>) {
    loop {
        let (Some(slot), Some(holder)) = (work.slot.upgrade(), work.holder.upgrade()) else {
            return;
        };
        let state = StartupTransport(slot, holder);
        if work.cancel.load(Ordering::Acquire) || Instant::now() >= work.deadline {
            let _ = refuse(&state);
            return;
        }
        match next(&state) {
            Ok(Some(payload)) => {
                if work.cancel.load(Ordering::Acquire) || Instant::now() >= work.deadline {
                    let _ = refuse(&state);
                    return;
                }
                if emit(payload).is_err() {
                    let _ = refuse(&state);
                    return;
                }
            }
            Ok(None) => {
                let complete = state
                    .0
                    .lock()
                    .is_ok_and(|s| s.exchange.stage == Stage::Complete);
                if complete {
                    return;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(_) => {
                let _ = refuse(&state);
                return;
            }
        }
    }
}
