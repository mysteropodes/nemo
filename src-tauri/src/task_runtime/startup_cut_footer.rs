//! Provisional original-owner exchange; no producer seal or durable footer.
use super::super::cut::{Phase, RetentionEvidence};
use super::*;
use serde_json::Value;
use tauri::ipc::{InvokeBody, Request};
use tauri::State;
const ERROR: &str = "startup exchange refused";
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Ack {
    Drain,
    Snapshot,
}
#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub(super) enum Stage {
    #[default]
    Idle,
    WaitDrain,
    DrainAcked,
    WaitSnapshot,
    Complete,
    Failed,
}
#[derive(Clone, PartialEq)]
pub(super) struct Pins {
    pub(super) version: u64,
    pub(super) native: Value,
    pub(super) projection: Value,
    pub(super) cut: String,
    pub(super) phase: Phase,
}
struct Accepted {
    bytes: Vec<u8>,
    pins: Pins,
}
#[derive(Default)]
pub(in super::super) struct Exchange {
    pub(super) stage: Stage,
    ticket: u64,
    pub(super) busy: bool,
    drain: Option<Accepted>,
    snapshot: Option<Accepted>,
    pub(super) armed: bool,
    pub(super) exited: Arc<AtomicBool>,
}
pub(in super::super) fn closed(slot: &mut Slot) {
    slot.exchange.stage = Stage::Failed;
    slot.exchange.busy = false;
}
pub(super) fn pins(slot: &mut Slot, native: &LoadLog) -> Result<Pins, &'static str> {
    let bridge = slot.bridge.as_ref().ok_or(ERROR)?;
    let binding = slot.owner.as_mut().ok_or(ERROR)?;
    let projection = bridge.projection();
    let js = &projection["js"];
    if slot.retention_failed
        || bridge.adapter_refused
        || binding.cancel.load(Ordering::Acquire)
        || !binding.original_store()
        || !binding
            .native
            .upgrade()
            .is_some_and(|n| Arc::ptr_eq(&n, &bridge.native))
        || !binding.deadline.is_some_and(|d| Instant::now() < d)
        || native.pid != std::process::id()
        || native.task_key != binding.runtime.task_key
        || js["runId"] != native.run
        || js["taskKey"] != native.task_key
        || js["pid"] != native.pid
        || js["nativePhase"] != native.phase
        || !js["fence"].is_null()
        || binding.session.state() != owner::State::Pending
    {
        return Err(ERROR);
    }
    let cut = binding.session.pending()?;
    Ok(Pins {
        version: slot.version,
        native: native.receipt(),
        projection,
        cut: cut.id().to_owned(),
        phase: cut.phase(),
    })
}
struct Receipt {
    native: Arc<Mutex<LoadLog>>,
    store: Arc<Store>,
    pins: Pins,
    bytes: Vec<u8>,
    kind: Ack,
    ticket: u64,
    duplicate: bool,
    stage: Stage,
    deadline: Instant,
}
fn capture(
    slot: &mut Slot,
    native: &LoadLog,
    kind: Ack,
    bytes: &[u8],
) -> Result<Receipt, &'static str> {
    let pins = pins(slot, native)?;
    let exchange = &mut slot.exchange;
    if exchange.busy || pins.phase != Phase::Draining {
        return Err(ERROR);
    }
    let accepted = match kind {
        Ack::Drain => &exchange.drain,
        Ack::Snapshot => &exchange.snapshot,
    };
    let duplicate = accepted.is_some();
    if let Some(old) = accepted {
        if old.bytes != bytes || old.pins != pins {
            return Err(ERROR);
        }
    } else if !matches!(
        (kind, exchange.stage),
        (Ack::Drain, Stage::WaitDrain) | (Ack::Snapshot, Stage::WaitSnapshot)
    ) {
        return Err(ERROR);
    }
    exchange.ticket = exchange.ticket.checked_add(1).ok_or(ERROR)?;
    exchange.busy = true;
    Ok(Receipt {
        native: slot.bridge.as_ref().ok_or(ERROR)?.native.clone(),
        store: slot.owner.as_ref().ok_or(ERROR)?.store.clone(),
        pins,
        bytes: bytes.to_vec(),
        kind,
        ticket: exchange.ticket,
        duplicate,
        stage: exchange.stage,
        deadline: slot.owner.as_ref().ok_or(ERROR)?.deadline.ok_or(ERROR)?,
    })
}
fn confirm(
    slot: &mut Slot,
    native: &LoadLog,
    receipt: Receipt,
    proof: RetentionEvidence,
) -> Result<bool, &'static str> {
    if pins(slot, native)? != receipt.pins
        || !slot.exchange.busy
        || slot.exchange.ticket != receipt.ticket
        || slot.exchange.stage != receipt.stage
        || slot.owner.as_ref().ok_or(ERROR)?.deadline != Some(receipt.deadline)
        || !Arc::ptr_eq(&slot.bridge.as_ref().ok_or(ERROR)?.native, &receipt.native)
        || !Arc::ptr_eq(&slot.owner.as_ref().ok_or(ERROR)?.store, &receipt.store)
    {
        return Err(ERROR);
    }
    slot.owner
        .as_mut()
        .ok_or(ERROR)?
        .session
        .pending()?
        .validate_provisional(
            &receipt.bytes,
            native,
            &slot.bridge.as_ref().ok_or(ERROR)?.stream,
            Some(&proof),
        )?;
    let exchange = &mut slot.exchange;
    if !receipt.duplicate {
        let accepted = Some(Accepted {
            bytes: receipt.bytes,
            pins: receipt.pins,
        });
        match receipt.kind {
            Ack::Drain => {
                exchange.drain = accepted;
                exchange.stage = Stage::DrainAcked;
            }
            Ack::Snapshot => {
                exchange.snapshot = accepted;
                exchange.stage = Stage::Complete;
            }
        }
    }
    exchange.busy = false;
    Ok(true)
}
pub(super) fn refuse(state: &StartupTransport) -> Result<(), &'static str> {
    let fallback = state.1.try_lock().ok().and_then(|s| s.clone());
    let (store, snapshot) = {
        let mut slot = state.0.lock().map_err(|_| ERROR)?;
        let store = slot.owner.as_ref().map(|b| b.store.clone()).or(fallback);
        if let Some(binding) = slot.owner.as_mut() {
            binding.retire();
        }
        closed(&mut slot);
        if let Some(bridge) = slot.bridge.as_mut() {
            bridge.adapter_refused = true;
        }
        (store, slot.snapshot().map_err(|_| ERROR)?)
    };
    if let (Some(store), Some((version, projection))) = (store, snapshot) {
        if store.publish(version, &projection).is_err() {
            if let Ok(mut slot) = state.0.lock() {
                slot.retention_failed = true;
            }
            return Err(ERROR);
        }
    }
    Ok(())
}
pub(super) fn acknowledge(
    state: &StartupTransport,
    label: &str,
    url: Result<&tauri::Url, ()>,
    pid: u32,
    body: &InvokeBody,
    kind: Ack,
) -> Result<bool, &'static str> {
    acknowledge_after(state, label, url, pid, body, kind, || {})
}
pub(super) fn acknowledge_after(
    state: &StartupTransport,
    label: &str,
    url: Result<&tauri::Url, ()>,
    pid: u32,
    body: &InvokeBody,
    kind: Ack,
    after_health: impl FnOnce(),
) -> Result<bool, &'static str> {
    let result = (|| {
        let url = url.map_err(|_| ERROR)?;
        if !super::super::super::allowed_origin(label, url) || pid != std::process::id() {
            return Err(ERROR);
        }
        let InvokeBody::Raw(bytes) = body else {
            return Err(ERROR);
        };
        if bytes.is_empty() || bytes.len() > 1024 {
            return Err(ERROR);
        }
        let receipt = {
            let mut slot = state.0.lock().map_err(|_| ERROR)?;
            let native = slot.bridge.as_ref().ok_or(ERROR)?.native.clone();
            let log = native.lock().map_err(|_| ERROR)?;
            capture(&mut slot, &log, kind, bytes)?
        };
        let proof = receipt
            .store
            .current_health(receipt.pins.version, &receipt.pins.projection);
        after_health(); // Production empty; mutation/lock tests only.
        let mut slot = state.0.lock().map_err(|_| ERROR)?;
        let native = slot.bridge.as_ref().ok_or(ERROR)?.native.clone();
        let log = native.lock().map_err(|_| ERROR)?;
        confirm(&mut slot, &log, receipt, proof?)
    })();
    if result.is_err() {
        refuse(state)?;
    }
    result.map_err(|_| ERROR)
}
fn command<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, StartupTransport>,
    request: Request<'_>,
    kind: Ack,
) -> Result<bool, String> {
    let label = window.label().to_owned();
    let url = window.url().map_err(|_| ());
    let pid = std::process::id();
    acknowledge(
        &state,
        &label,
        url.as_ref().map_err(|_| ()),
        pid,
        request.body(),
        kind,
    )
    .map_err(str::to_owned)
}
#[tauri::command]
pub(crate) fn nemo_test_startup_cut_drained<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, StartupTransport>,
    request: Request<'_>,
) -> Result<bool, String> {
    command(window, state, request, Ack::Drain)
}
#[tauri::command]
pub(crate) fn nemo_test_startup_cut_snapshot<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, StartupTransport>,
    request: Request<'_>,
) -> Result<bool, String> {
    command(window, state, request, Ack::Snapshot)
}
