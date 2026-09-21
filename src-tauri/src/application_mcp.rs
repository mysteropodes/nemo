//! Native transport port for the single application command authority in the webview.
#[cfg(test)]
use crate::native_dispatch::ReleaseTombstone;
use crate::native_dispatch::{spawn_export_pump, NativeAuthority, NativeDispatch, NativeState};
use native_engine::{
    application::{ExportResourceResolver, NativeApplication},
    commands::OpacityRequest,
    document::OpacityDocument,
    export_job::{ExportCompositor, JobReceipt, StagedArtifactPort},
    protocol::OP_JOB_EXPORT_PNG_BEGIN,
};
use nemo_mcp::{
    contract::{
        ApplicationRequest, ApplicationResponse, NativeApplicationError, NativeApplicationRequest,
        NativeApplicationResponse, NativeHostStatus, NativeStatusRequest, Operation,
        NATIVE_API_VERSION,
    },
    registry::{self, Endpoint, Registration},
    wire,
};
use serde::Serialize;
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tauri::{Emitter, Manager};
use tokio::{
    io::AsyncReadExt,
    net::{TcpListener, TcpStream},
    sync::{oneshot, Semaphore},
};

type Pending = Arc<Mutex<HashMap<String, oneshot::Sender<ApplicationResponse>>>>;

pub struct ApplicationMcp {
    instance_id: String,
    started: AtomicBool,
    pending: Pending,
    native: NativeState,
}

pub(crate) struct NativeInstallReservation {
    native: NativeState,
    generation: u64,
}

impl NativeInstallReservation {
    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }
}

impl Drop for NativeInstallReservation {
    fn drop(&mut self) {
        let Ok(mut native) = self.native.lock() else {
            return;
        };
        native.rollback_install(self.generation);
    }
}

impl Default for ApplicationMcp {
    fn default() -> Self {
        Self {
            instance_id: uuid::Uuid::new_v4().to_string(),
            started: AtomicBool::new(false),
            pending: Arc::new(Mutex::new(HashMap::new())),
            native: Arc::new(Mutex::new(NativeAuthority::default())),
        }
    }
}

impl ApplicationMcp {
    pub(crate) fn instance_id(&self) -> &str {
        &self.instance_id
    }

    pub(crate) fn reserve_native_install(&self) -> Result<NativeInstallReservation, String> {
        let mut native = self
            .native
            .lock()
            .map_err(|_| "native application lock unavailable")?;
        let generation = native.reserve_install()?;
        drop(native);
        Ok(NativeInstallReservation {
            native: Arc::clone(&self.native),
            generation,
        })
    }

    pub(crate) fn native_state(&self) -> NativeState {
        Arc::clone(&self.native)
    }

    pub(crate) fn install_dispatch(
        &self,
        generation: u64,
        application: Box<dyn NativeDispatch>,
    ) -> Result<(), String> {
        if application.instance_id() != self.instance_id {
            return Err("native application instance mismatch".into());
        }
        let mut native = self
            .native
            .lock()
            .map_err(|_| "native application lock unavailable")?;
        native.install(generation, application)
    }

    pub(crate) fn install_native<P, C, R>(
        &self,
        application: NativeApplication<P, C, R>,
    ) -> Result<(), String>
    where
        P: StagedArtifactPort + Send + 'static,
        C: ExportCompositor + Send + 'static,
        R: ExportResourceResolver + Send + 'static,
    {
        let reservation = self.reserve_native_install()?;
        self.install_dispatch(reservation.generation(), Box::new(application))
    }

    pub(crate) fn replace_native_document(
        &self,
        document: OpacityDocument,
    ) -> Result<Vec<JobReceipt>, String> {
        let mut native = self
            .native
            .lock()
            .map_err(|_| "native application lock unavailable")?;
        let generation = native.active_generation()?;
        native.active_mut(generation)?.replace_document(document)
    }

    fn native_status(&self, request: NativeStatusRequest) -> Result<NativeHostStatus, String> {
        native_status(&self.instance_id, &self.native, request)
    }

    fn dispatch_native(
        &self,
        request: NativeApplicationRequest,
    ) -> Result<NativeApplicationResponse, String> {
        dispatch_native(&self.instance_id, &self.native, request)
    }

    #[cfg(test)]
    fn retire_native_for_test(&self, receipt: serde_json::Value) {
        let mut authority = self.native.lock().unwrap();
        let generation = authority.next_generation().unwrap();
        authority.finish_release(ReleaseTombstone {
            generation,
            reentry_used: false,
            request_id: "release-shared".into(),
            fingerprint: vec![1, 2, 3],
            receipt,
            succeeded: true,
        });
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RequestEvent {
    connection_id: String,
    request: ApplicationRequest,
}

fn require_main(window: &tauri::Window) -> Result<(), String> {
    if window.label() == "main" {
        Ok(())
    } else {
        Err("application MCP requires the main window".into())
    }
}

#[tauri::command]
pub fn nemo_mcp_identity(
    window: tauri::Window,
    state: tauri::State<ApplicationMcp>,
) -> Result<serde_json::Value, String> {
    require_main(&window)?;
    Ok(serde_json::json!({"instanceId": state.instance_id, "apiVersion": 1}))
}

#[tauri::command]
pub async fn nemo_mcp_ready(
    app: tauri::AppHandle,
    window: tauri::Window,
    state: tauri::State<'_, ApplicationMcp>,
) -> Result<(), String> {
    require_main(&window)?;
    if state.started.swap(true, Ordering::SeqCst) {
        return Ok(());
    }
    let result = start(
        app,
        state.instance_id.clone(),
        state.pending.clone(),
        state.native.clone(),
    )
    .await;
    if result.is_err() {
        state.started.store(false, Ordering::SeqCst);
    }
    result
}

#[tauri::command]
pub fn nemo_mcp_reply(
    window: tauri::Window,
    state: tauri::State<ApplicationMcp>,
    connection_id: String,
    response: ApplicationResponse,
) -> Result<(), String> {
    require_main(&window)?;
    if response.instance_id != state.instance_id {
        return Err("response instance mismatch".into());
    }
    let sender = state
        .pending
        .lock()
        .map_err(|_| "pending request lock unavailable")?
        .remove(&connection_id);
    if let Some(sender) = sender {
        let _ = sender.send(response);
    }
    Ok(())
}

#[tauri::command]
pub fn nemo_native_status(
    window: tauri::Window,
    state: tauri::State<ApplicationMcp>,
) -> Result<NativeHostStatus, String> {
    require_main(&window)?;
    state.native_status(NativeStatusRequest {
        api_version: NATIVE_API_VERSION,
        request_id: uuid::Uuid::new_v4().to_string(),
        instance_id: state.instance_id.clone(),
    })
}

#[tauri::command]
pub fn nemo_native_dispatch(
    window: tauri::Window,
    state: tauri::State<ApplicationMcp>,
    request: NativeApplicationRequest,
) -> Result<NativeApplicationResponse, String> {
    require_main(&window)?;
    state.dispatch_native(request)
}

fn native_status(
    instance_id: &str,
    native: &NativeState,
    request: NativeStatusRequest,
) -> Result<NativeHostStatus, String> {
    request.validate().map_err(|error| error.to_string())?;
    let authority = native
        .lock()
        .map_err(|_| "native application lock unavailable")?;
    let Some((_, application)) = authority.active() else {
        return Ok(NativeHostStatus {
            api_version: NATIVE_API_VERSION,
            request_id: request.request_id,
            instance_id: instance_id.to_owned(),
            available: false,
            document_id: None,
            content_revision: None,
            reason: Some(authority.unavailable_reason().into()),
        });
    };
    if request.instance_id != instance_id || application.instance_id() != instance_id {
        return Ok(NativeHostStatus {
            api_version: NATIVE_API_VERSION,
            request_id: request.request_id,
            instance_id: instance_id.to_owned(),
            available: false,
            document_id: None,
            content_revision: None,
            reason: Some("native application instance mismatch".into()),
        });
    }
    Ok(NativeHostStatus {
        api_version: NATIVE_API_VERSION,
        request_id: request.request_id,
        instance_id: instance_id.to_owned(),
        available: true,
        document_id: Some(application.document_id().to_owned()),
        content_revision: Some(application.content_revision()),
        reason: None,
    })
}

fn dispatch_native(
    instance_id: &str,
    native: &NativeState,
    request: NativeApplicationRequest,
) -> Result<NativeApplicationResponse, String> {
    request.validate().map_err(|error| error.to_string())?;
    let mut authority = native
        .lock()
        .map_err(|_| "native application lock unavailable")?;
    let generation = match authority.active_generation() {
        Ok(generation) => generation,
        Err(_) => {
            return Ok(NativeApplicationResponse {
                api_version: NATIVE_API_VERSION,
                request_id: request.request_id,
                instance_id: instance_id.to_owned(),
                document_id: request.document_id,
                content_revision: 0,
                ok: false,
                result: None,
                error: Some(NativeApplicationError {
                    code: "unavailable".into(),
                    message: authority.unavailable_reason().into(),
                    details: None,
                }),
            });
        }
    };
    let application = authority.active_mut(generation)?;
    let operation = request.operation.clone();
    let native_request: OpacityRequest =
        serde_json::from_value(serde_json::to_value(&request).map_err(|error| error.to_string())?)
            .map_err(|error| error.to_string())?;
    let response: NativeApplicationResponse = serde_json::from_value(
        serde_json::to_value(application.dispatch(native_request))
            .map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    drop(authority);
    if operation == OP_JOB_EXPORT_PNG_BEGIN {
        if let Some(job_id) = response
            .result
            .as_ref()
            .filter(|_| response.ok)
            .and_then(|result| result.get("jobId"))
            .and_then(serde_json::Value::as_str)
        {
            spawn_export_pump(Arc::clone(native), generation, job_id.to_owned());
        }
    }
    Ok(response)
}

fn advertise_native(mut response: ApplicationResponse) -> ApplicationResponse {
    if response.ok {
        if let Some(result) = response
            .result
            .as_mut()
            .and_then(serde_json::Value::as_object_mut)
        {
            result.insert("nativeApiVersion".into(), NATIVE_API_VERSION.into());
        }
    }
    response
}

async fn start(
    app: tauri::AppHandle,
    instance_id: String,
    pending: Pending,
    native: NativeState,
) -> Result<(), String> {
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .map_err(|e| e.to_string())?;
    let endpoint = Endpoint {
        instance_id,
        port: listener.local_addr().map_err(|e| e.to_string())?.port(),
        secret: uuid::Uuid::new_v4().to_string(),
        build_id: format!(
            "{}:{}",
            app.package_info().version,
            nemo_mcp::BUILD_SOURCE_ID
        ),
    };
    let root = registry::registry_root().map_err(|e| e.to_string())?;
    let registration = Registration::create(&root, &endpoint).map_err(|e| e.to_string())?;
    tauri::async_runtime::spawn(async move {
        let _registration = registration;
        let capacity = Arc::new(Semaphore::new(32));
        while let Ok((stream, _)) = listener.accept().await {
            let Ok(permit) = capacity.clone().try_acquire_owned() else {
                continue;
            };
            let (app, endpoint, pending, native) = (
                app.clone(),
                endpoint.clone(),
                pending.clone(),
                native.clone(),
            );
            tauri::async_runtime::spawn(async move {
                let _permit = permit;
                let _ = serve_connection(app, stream, endpoint, pending, native).await;
            });
        }
    });
    Ok(())
}

async fn serve_connection(
    app: tauri::AppHandle,
    mut stream: TcpStream,
    endpoint: Endpoint,
    pending: Pending,
    native: NativeState,
) -> Result<(), String> {
    let message: wire::AuthenticatedWireRequest =
        tokio::time::timeout(Duration::from_secs(5), wire::read_json(&mut stream))
            .await
            .map_err(|_| "connection initialization timed out")?
            .map_err(|e| e.to_string())?;
    let secret = match &message {
        wire::AuthenticatedWireRequest::Legacy(message) => &message.secret,
        wire::AuthenticatedWireRequest::Native(message) => &message.secret,
        wire::AuthenticatedWireRequest::NativeStatus(message) => &message.secret,
    };
    if secret != &endpoint.secret {
        return Err("unauthorized connection".into());
    }
    match message {
        wire::AuthenticatedWireRequest::Legacy(message) => {
            serve_legacy(app, stream, endpoint, pending, message).await
        }
        wire::AuthenticatedWireRequest::Native(message) => {
            let response = dispatch_native(&endpoint.instance_id, &native, message.native_request)?;
            wire::write_json(&mut stream, &response)
                .await
                .map_err(|error| error.to_string())
        }
        wire::AuthenticatedWireRequest::NativeStatus(message) => {
            let response = native_status(&endpoint.instance_id, &native, message.native_status)?;
            wire::write_json(&mut stream, &response)
                .await
                .map_err(|error| error.to_string())
        }
    }
}

async fn serve_legacy(
    app: tauri::AppHandle,
    mut stream: TcpStream,
    endpoint: Endpoint,
    pending: Pending,
    message: wire::WireRequest,
) -> Result<(), String> {
    message.request.validate().map_err(|e| e.to_string())?;
    if message.request.instance_id.as_deref() != Some(endpoint.instance_id.as_str()) {
        return Err("request instance mismatch".into());
    }
    let is_capabilities = matches!(message.request.operation, Operation::Capabilities);
    let window = app
        .get_webview_window("main")
        .ok_or("application window unavailable")?;
    let connection_id = uuid::Uuid::new_v4().to_string();
    let request_id = message.request.request_id.clone();
    let (sender, receiver) = oneshot::channel();
    pending
        .lock()
        .map_err(|_| "pending request lock unavailable")?
        .insert(connection_id.clone(), sender);
    let event = RequestEvent {
        connection_id: connection_id.clone(),
        request: message.request,
    };
    if let Err(error) = window.emit("nemo-application-request", event) {
        pending
            .lock()
            .map_err(|_| "pending request lock unavailable")?
            .remove(&connection_id);
        return Err(error.to_string());
    }
    let mut byte = [0u8; 1];
    let response = tokio::select! {
        response = receiver => response.ok(),
        _ = stream.read(&mut byte) => None,
        _ = tokio::time::sleep(Duration::from_secs(30)) => None,
    };
    pending
        .lock()
        .map_err(|_| "pending request lock unavailable")?
        .remove(&connection_id);
    if let Some(mut response) = response {
        if response.request_id != request_id {
            return Err("response request mismatch".into());
        }
        if is_capabilities {
            response = advertise_native(response);
        }
        wire::write_json(&mut stream, &response)
            .await
            .map_err(|e| e.to_string())?;
    } else {
        let _ = window.emit(
            "nemo-application-cancel",
            serde_json::json!({"connectionId": connection_id}),
        );
    }
    Ok(())
}

#[cfg(test)]
#[path = "application_mcp_tests.rs"]
mod tests;
