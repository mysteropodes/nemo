//! One original-Slot owner attempt. No handlers/events/seal/installed claim.
use super::super::super::TaskRuntime;
use super::{owner, request, retention::Store, Bridge, LoadLog, Observer, Slot, StartupTransport};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex, Weak,
};
use std::time::{Duration, Instant};
use tauri::{Manager, Runtime, WebviewWindow};

const ERROR: &str = "startup native owner unavailable";
const WAIT: Duration = Duration::from_secs(600);
pub(super) struct Binding {
    runtime: TaskRuntime,
    session: owner::Session,
    store: Arc<Store>,
    holder: Weak<Mutex<Option<Arc<Store>>>>,
    root: Arc<request::Root>,
    armed: bool,
    cancel: Arc<AtomicBool>,
    exited: Arc<AtomicBool>,
}
impl Binding {
    fn original_store(&self) -> bool {
        self.holder.upgrade().is_some_and(|holder| {
            holder.try_lock().is_ok_and(|value| {
                value
                    .as_ref()
                    .is_some_and(|store| Arc::ptr_eq(store, &self.store))
            })
        })
    }
    pub(super) fn retire(&mut self) {
        self.cancel.store(true, Ordering::Release);
        self.session.retire();
    }
    fn capture(
        &mut self,
        native: &LoadLog,
        bridge: &Bridge,
        version: u64,
    ) -> Result<owner::Capture, &'static str> {
        if self.cancel.load(Ordering::Acquire) || !self.original_store() {
            return Err(ERROR);
        }
        self.session.capture(&self.runtime, native, bridge, version)
    }
    fn confirm(
        &mut self,
        auth: owner::Authorization,
        native: &LoadLog,
        bridge: &Bridge,
        version: u64,
    ) -> Result<(), &'static str> {
        if self.cancel.load(Ordering::Acquire) || !self.original_store() {
            return Err(ERROR);
        }
        self.session
            .confirm(auth, &self.runtime, native, bridge, version)
            .map(|_| ())
    }
}
impl Drop for Binding {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
    }
}
pub(super) fn install(
    state: &StartupTransport,
    runtime: &TaskRuntime,
    store: Arc<Store>,
) -> Result<(), &'static str> {
    {
        let slot = state.0.lock().map_err(|_| ERROR)?;
        if !slot.bound || slot.owner.is_some() || slot.retention_failed {
            return Err(ERROR);
        }
    }
    if runtime.owner_token.is_none() {
        return Ok(());
    }
    // Root/descriptor IO before ANY producer guard.
    let reports = runtime.data_dir.parent().ok_or(ERROR)?.join("reports");
    let root = request::Root::new(&reports)?;
    if !state
        .1
        .try_lock()
        .map_err(|_| ERROR)?
        .as_ref()
        .is_some_and(|original| Arc::ptr_eq(original, &store))
    {
        return Err(ERROR);
    }
    let mut slot = state.0.lock().map_err(|_| ERROR)?;
    if !slot.bound || slot.owner.is_some() || slot.retention_failed {
        return Err(ERROR);
    }
    let bridge = slot.bridge.as_ref().ok_or(ERROR)?;
    let native = bridge.native.lock().map_err(|_| ERROR)?;
    let session = owner::Session::new(runtime, &native, bridge, store.clone())?;
    drop(native);
    slot.owner = Some(Binding {
        runtime: runtime.clone(),
        session,
        store,
        holder: Arc::downgrade(&state.1),
        root,
        armed: false,
        cancel: Arc::new(AtomicBool::new(false)),
        exited: Arc::new(AtomicBool::new(false)),
    });
    Ok(())
}
struct Work {
    slot: Weak<Mutex<Slot>>,
    root: Arc<request::Root>,
    cancel: Arc<AtomicBool>,
    exited: Arc<AtomicBool>,
}
impl Drop for Work {
    fn drop(&mut self) {
        self.exited.store(true, Ordering::Release);
    }
}
fn prepare(
    slot: &Arc<Mutex<Slot>>,
    label: &str,
    native: &Arc<Mutex<LoadLog>>,
) -> Result<Work, &'static str> {
    let mut guard = slot.lock().map_err(|_| ERROR)?;
    let original = guard
        .bridge
        .as_ref()
        .is_some_and(|b| Arc::ptr_eq(&b.native, native));
    let created = native.try_lock().is_ok_and(|log| log.phase >= 1);
    let binding = guard.owner.as_mut().ok_or(ERROR)?;
    if !original
        || !created
        || label != "main"
        || binding.armed
        || binding.cancel.load(Ordering::Acquire)
        || binding.session.state() != owner::State::Ready
    {
        binding.retire();
        return Err(ERROR);
    }
    binding.armed = true;
    Ok(Work {
        slot: Arc::downgrade(slot),
        root: binding.root.clone(),
        cancel: binding.cancel.clone(),
        exited: binding.exited.clone(),
    })
}
pub(in super::super) fn arm<R: Runtime>(observer: &Observer, window: &WebviewWindow<R>) {
    // The actual created callback supplies this window, never request bytes.
    let label = window.label();
    let Some(native) = observer.as_ref() else {
        return;
    };
    let state = window.app_handle().state::<StartupTransport>();
    if let Ok(work) = prepare(&state.0, label, native) {
        if std::thread::Builder::new()
            .name("nemo-startup-owner".into())
            .spawn(move || run(work, WAIT))
            .is_err()
        {
            retire(&state.0);
        }
    }
}
fn retire(slot: &Arc<Mutex<Slot>>) {
    if let Ok(mut guard) = slot.lock() {
        if let Some(binding) = guard.owner.as_mut() {
            binding.retire();
        }
    }
}
fn run(work: Work, timeout: Duration) {
    let Some(deadline) = Instant::now().checked_add(timeout) else {
        if let Some(slot) = work.slot.upgrade() {
            retire(&slot);
        }
        return;
    };
    loop {
        let Some(slot) = work.slot.upgrade() else {
            return;
        };
        if work.cancel.load(Ordering::Acquire) || Instant::now() >= deadline {
            retire(&slot);
            return;
        }
        match work.root.read() {
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Ok(Some(mut input)) => {
                if attempt_after(&slot, &mut input, deadline, || {}).is_err() {
                    retire(&slot);
                }
                return; // Never re-read a consumed/refused request.
            }
            Err(_) => {
                retire(&slot);
                return;
            }
        }
    }
}
fn attempt_after(
    slot: &Arc<Mutex<Slot>>,
    input: &mut request::Input,
    deadline: Instant,
    after_health: impl FnOnce(),
) -> Result<(), &'static str> {
    attempt_with_wait(slot, input, deadline, after_health, || {})
}
fn attempt_with_wait(
    slot: &Arc<Mutex<Slot>>,
    input: &mut request::Input,
    deadline: Instant,
    after_health: impl FnOnce(),
    before_native: impl FnOnce(),
) -> Result<(), &'static str> {
    if Instant::now() >= deadline {
        return Err(ERROR);
    }
    let capture = {
        let mut guard = slot.lock().map_err(|_| ERROR)?;
        let Slot {
            owner,
            bridge,
            version,
            retention_failed,
            ..
        } = &mut *guard;
        if *retention_failed {
            return Err(ERROR);
        }
        let bridge = bridge.as_ref().ok_or(ERROR)?;
        let native = bridge.native.lock().map_err(|_| ERROR)?;
        owner
            .as_mut()
            .ok_or(ERROR)?
            .capture(&native, bridge, *version)?
    };
    let auth = capture.authorize(input.bytes()); // No metadata/native guard.
    after_health();
    input.reconfirm()?; // Physical input still the same after H1, outside guards.
    let mut guard = slot.lock().map_err(|_| ERROR)?;
    let Slot {
        owner,
        bridge,
        version,
        retention_failed,
        ..
    } = &mut *guard;
    if *retention_failed {
        return Err(ERROR);
    }
    if Instant::now() >= deadline {
        return Err(ERROR);
    }
    let bridge = bridge.as_ref().ok_or(ERROR)?;
    before_native(); // Production is empty; deterministic lock-contention test seam.
    let native = bridge.native.lock().map_err(|_| ERROR)?;
    if Instant::now() >= deadline {
        return Err(ERROR);
    }
    owner
        .as_mut()
        .ok_or(ERROR)?
        .confirm(auth, &native, bridge, *version)
}

#[cfg(all(test, target_os = "macos"))]
#[path = "startup_cut_runtime_tests.rs"]
mod tests;
