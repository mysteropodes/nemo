//! Actual sender adapter; bounded memory only, no persistence or acceptance.
use super::{
    js::{Fence, JsStream},
    LoadLog, Observer,
};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use tauri::{
    ipc::{InvokeBody, Request},
    Manager, Runtime, State, WebviewWindow, WindowEvent,
};

#[path = "startup_retention.rs"]
mod retention;

#[path = "startup_cut.rs"]
mod cut;

#[path = "startup_cut_owner.rs"]
mod owner;

#[path = "startup_cut_request.rs"]
mod request;
#[path = "startup_cut_runtime.rs"]
pub(super) mod runtime;

struct Bridge {
    native: Arc<Mutex<LoadLog>>,
    stream: JsStream,
    adapter_refused: bool,
}
impl Bridge {
    fn new(native: Arc<Mutex<LoadLog>>) -> Result<Self, &'static str> {
        let stream = {
            let log = native.lock().map_err(|_| "startup native unavailable")?;
            JsStream::from_native(&log).map_err(|_| "startup boot refused")?
        };
        Ok(Self {
            native,
            stream,
            adapter_refused: false,
        })
    }
    fn admit(
        &mut self,
        label: &str,
        url: &tauri::Url,
        pid: u32,
        body: &InvokeBody,
    ) -> Result<bool, &'static str> {
        let result = (|| {
            let bytes = match body {
                InvokeBody::Raw(bytes) if !bytes.is_empty() && bytes.len() <= 1024 => bytes,
                _ => return Err("startup raw body refused"),
            };
            // Order: bridge guard held by caller, then native guard through
            // reconcile/admission. Neither path calls GUI or filesystem here.
            let native = self
                .native
                .lock()
                .map_err(|_| "startup native unavailable")?;
            self.stream
                .reconcile_native(&native)
                .map_err(|_| "startup native fenced")?;
            self.stream
                .accept_json(label, url, pid, bytes)
                .map_err(|_| "startup record refused")?;
            Ok(true)
        })();
        if result.is_err() {
            self.adapter_refused = true;
        }
        result
    }
    fn projection(&self) -> Value {
        json!({"adapterRefused":self.adapter_refused, "js":self.stream.projection()})
    }
}

#[derive(Default)]
struct Slot {
    owner: Option<runtime::Binding>,
    bound: bool,
    bridge: Option<Bridge>,
    version: u64,
    retention_failed: bool,
}
impl Slot {
    fn bind(&mut self, observer: Observer) -> Result<(), &'static str> {
        if self.bound {
            return Err("startup cannot rebind");
        }
        self.bridge = observer.map(Bridge::new).transpose()?;
        self.bound = true;
        Ok(())
    }
    fn admit(
        &mut self,
        label: &str,
        url: &tauri::Url,
        pid: u32,
        body: &InvokeBody,
    ) -> Result<bool, &'static str> {
        if self.retention_failed {
            return Err("startup retention failed");
        }
        self.bridge
            .as_mut()
            .ok_or("startup disabled")?
            .admit(label, url, pid, body)
    }
    fn close(&mut self) {
        if let Some(owner) = self.owner.as_mut() {
            owner.retire();
        }
        if let Some(bridge) = self.bridge.as_mut() {
            bridge.stream.fence(Fence::Closed);
        }
    }
    fn snapshot(&mut self) -> Result<Option<(u64, Value)>, &'static str> {
        let Some(bridge) = self.bridge.as_ref() else {
            return Ok(None);
        };
        if self.retention_failed {
            return Err("startup retention failed");
        }
        let Some(next) = self.version.checked_add(1) else {
            self.retention_failed = true;
            return Err("startup retention version exhausted");
        };
        self.version = next;
        Ok(Some((self.version, bridge.projection())))
    }
}

#[derive(Default)]
pub(crate) struct StartupTransport(Arc<Mutex<Slot>>, Arc<Mutex<Option<Arc<retention::Store>>>>);

pub(in crate::task_runtime) fn initialize<R: Runtime>(
    app: &tauri::App<R>,
) -> Result<(), Box<dyn std::error::Error>> {
    if !app.manage(StartupTransport::default()) {
        return Err("startup duplicate state refused".into());
    }
    Ok(())
}
pub(in crate::task_runtime) fn bind<R: Runtime>(
    handle: &tauri::AppHandle<R>,
    observer: Observer,
    runtime: &super::super::TaskRuntime,
) -> Result<(), Box<dyn std::error::Error>> {
    let state = handle.state::<StartupTransport>();
    let initial = {
        let mut slot = state.0.lock().map_err(|_| "startup bridge unavailable")?;
        slot.bind(observer)?;
        slot.snapshot()?
    };
    if let Some((version, snapshot)) = initial {
        // Owned root validation, open, encode and IO AFTER producer guards.
        let owned = runtime
            .data_dir
            .parent()
            .ok_or("startup retention root refused")?
            .join("reports");
        let actual = std::path::PathBuf::from(std::env::var("NEMO_REPORT_DIR")?);
        let store = Arc::new(retention::Store::open(&actual, &owned, &snapshot)?);
        store.publish(version, &snapshot)?;
        *state
            .1
            .lock()
            .map_err(|_| "startup retention unavailable")? = Some(store.clone());
        runtime::install(&state, runtime, store)?;
    }
    Ok(())
}
pub(super) fn watch_destroyed<R: Runtime>(window: &WebviewWindow<R>) {
    if window.label() != "main" {
        return;
    }
    let state = window.app_handle().state::<StartupTransport>();
    let slot = state.0.clone();
    let store = state.1.clone();
    // Capture only metadata Arc; never Window/AppHandle back into its manager.
    window.on_window_event(move |event| {
        if matches!(event, WindowEvent::Destroyed) {
            if let Ok(mut slot) = slot.lock() {
                slot.close();
                let snapshot = slot.snapshot();
                drop(slot);
                if let (Ok(Some((version, snapshot))), Ok(store)) = (snapshot, store.lock()) {
                    if let Some(store) = store.as_ref() {
                        let _ = store.publish(version, &snapshot);
                    }
                }
            }
        }
    });
}

pub(in crate::task_runtime) fn identity<R: Runtime>(
    handle: &tauri::AppHandle<R>,
) -> Result<Option<Value>, Box<dyn std::error::Error>> {
    let state = handle.state::<StartupTransport>();
    let slot = state.0.lock().map_err(|_| "startup bridge unavailable")?;
    Ok(slot.bridge.as_ref().map(|bridge| {
        let pins = bridge.stream.projection();
        json!({"schema":"nemo.startup-observer/1", "runId":pins["runId"], "loadGeneration":pins["loadGeneration"]})
    }))
}

#[tauri::command]
pub(crate) fn nemo_test_startup_record<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, StartupTransport>,
    request: Request<'_>,
) -> Result<bool, String> {
    // These are native-injected sender reads, before ANY metadata lock. url()
    // may marshal to the GUI: never invoke it under bridge/native guards.
    let label = window.label().to_owned();
    let url = window.url().map_err(|_| ());
    let pid = std::process::id();
    transact(
        &state,
        &label,
        url.as_ref().map_err(|_| ()),
        pid,
        request.body(),
    )
    .map_err(str::to_owned)
}

fn transact(
    state: &StartupTransport,
    label: &str,
    url: Result<&tauri::Url, ()>,
    pid: u32,
    body: &InvokeBody,
) -> Result<bool, &'static str> {
    let store = state
        .1
        .lock()
        .map_err(|_| "startup retention unavailable")?
        .clone()
        .ok_or("startup retention disabled")?;
    let (result, snapshot) = {
        let mut slot = state.0.lock().map_err(|_| "startup bridge unavailable")?;
        let result = match url {
            Ok(url) => slot.admit(label, url, pid, body),
            Err(_) => {
                if let Some(bridge) = slot.bridge.as_mut() {
                    bridge.adapter_refused = true;
                }
                Err("startup sender unavailable")
            }
        };
        (result, slot.snapshot()?.ok_or("startup disabled")?)
    };
    store.publish(snapshot.0, &snapshot.1)?; // No bridge/native guard retained.
    result
}

#[cfg(test)]
#[path = "startup_js_host_tests.rs"]
mod tests;
