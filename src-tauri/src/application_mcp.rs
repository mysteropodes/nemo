//! Native transport port for the single application command authority in the webview.
use nemo_mcp::{
    contract::{ApplicationRequest, ApplicationResponse},
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
}

impl Default for ApplicationMcp {
    fn default() -> Self {
        Self {
            instance_id: uuid::Uuid::new_v4().to_string(),
            started: AtomicBool::new(false),
            pending: Arc::new(Mutex::new(HashMap::new())),
        }
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
    let result = start(app, state.instance_id.clone(), state.pending.clone()).await;
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

async fn start(app: tauri::AppHandle, instance_id: String, pending: Pending) -> Result<(), String> {
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
            let (app, endpoint, pending) = (app.clone(), endpoint.clone(), pending.clone());
            tauri::async_runtime::spawn(async move {
                let _permit = permit;
                let _ = serve_connection(app, stream, endpoint, pending).await;
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
) -> Result<(), String> {
    let message: wire::WireRequest =
        tokio::time::timeout(Duration::from_secs(5), wire::read_json(&mut stream))
            .await
            .map_err(|_| "connection initialization timed out")?
            .map_err(|e| e.to_string())?;
    if message.secret != endpoint.secret {
        return Err("unauthorized connection".into());
    }
    message.request.validate().map_err(str::to_owned)?;
    if message.request.instance_id.as_deref() != Some(endpoint.instance_id.as_str()) {
        return Err("request instance mismatch".into());
    }
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
    if let Some(response) = response {
        if response.request_id != request_id {
            return Err("response request mismatch".into());
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
