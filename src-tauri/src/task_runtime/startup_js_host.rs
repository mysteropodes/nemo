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
    bound: bool,
    bridge: Option<Bridge>,
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
        self.bridge
            .as_mut()
            .ok_or("startup disabled")?
            .admit(label, url, pid, body)
    }
    fn close(&mut self) {
        if let Some(bridge) = self.bridge.as_mut() {
            bridge.stream.fence(Fence::Closed);
        }
    }
}

#[derive(Default)]
pub(crate) struct StartupTransport(Arc<Mutex<Slot>>);

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
) -> Result<(), Box<dyn std::error::Error>> {
    let state = handle.state::<StartupTransport>();
    let mut slot = state.0.lock().map_err(|_| "startup bridge unavailable")?;
    slot.bind(observer)?;
    Ok(())
}
pub(super) fn watch_destroyed<R: Runtime>(window: &WebviewWindow<R>) {
    if window.label() != "main" {
        return;
    }
    let slot = window.app_handle().state::<StartupTransport>().0.clone();
    // Capture only metadata Arc; never Window/AppHandle back into its manager.
    window.on_window_event(move |event| {
        if matches!(event, WindowEvent::Destroyed) {
            if let Ok(mut slot) = slot.lock() {
                slot.close();
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
    let url = window
        .url()
        .map_err(|_| "startup sender unavailable".to_owned())?;
    let pid = std::process::id();
    state
        .0
        .lock()
        .map_err(|_| "startup bridge unavailable".to_owned())?
        .admit(&label, &url, pid, request.body())
        .map_err(str::to_owned)
}

#[cfg(test)]
#[path = "startup_js_host_tests.rs"]
mod tests;
