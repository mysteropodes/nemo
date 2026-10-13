//! Strict data-only checkpoint acknowledgment and bounded async continuation.
use crate::task_runtime::object_window_probe::origin;
use crate::task_runtime::object_window_probe::Session;
use crate::task_runtime::object_window_probe::{hex, safe_root, Packet, ProbeState, MAX_BYTES};
use serde::Deserialize;
use std::{
    fs::{self, OpenOptions},
    io::Read,
    path::Path,
};
use tauri::Manager;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Ack {
    run_id: String,
    source_sha: String,
    checkpoint: usize,
    owner_token: String,
    screenshot_sha256: Option<String>,
}
pub(super) fn read_ack(
    root: &Path,
    run: &str,
    source: &str,
    checkpoint: usize,
    owner: &str,
) -> Result<bool, &'static str> {
    safe_root(root)?;
    let path = root.join("object-window-probe-control.json");
    let meta = match fs::symlink_metadata(&path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(_) => return Err("probe control unavailable"),
    };
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > MAX_BYTES as u64 {
        return Err("probe control refused");
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(target_os = "macos")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(0x100);
    }
    let mut bytes = vec![];
    options
        .open(path)
        .map_err(|_| "probe control unavailable")?
        .take((MAX_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| "probe control unavailable")?;
    if bytes.len() > MAX_BYTES {
        return Err("probe control bound exceeded");
    }
    let ack: Ack = serde_json::from_slice(&bytes).map_err(|_| "probe control invalid")?;
    if ack.run_id != run || ack.source_sha != source || ack.owner_token != owner {
        return Err("probe control identity refused");
    }
    if ack.checkpoint < checkpoint {
        return Ok(false);
    }
    if ack.checkpoint != checkpoint || ack.screenshot_sha256.as_ref().is_some_and(|s| !hex(s, 64)) {
        return Err("probe control checkpoint refused");
    }
    Ok(true)
}
#[tauri::command]
pub(crate) async fn nemo_test_object_window_probe(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    payload_json: String,
) -> Result<(), String> {
    let packet: Packet = if payload_json.len() <= MAX_BYTES {
        serde_json::from_str(&payload_json).map_err(|_| "probe packet invalid")?
    } else {
        return Err("probe packet too large".into());
    };
    let url = window.url().map_err(|_| "probe window unavailable")?;
    if !origin(window.label(), &url) {
        return Err("probe window refused".into());
    }
    let (root, run, source, checkpoint, owner) = {
        let state = app.try_state::<ProbeState>().ok_or("probe inactive")?;
        let mut guard = state.0.lock().map_err(|_| "probe state unavailable")?;
        let session = guard.as_mut().ok_or("probe inactive")?;
        if session.pid != std::process::id() {
            return Err("probe process refused".into());
        }
        if !session.admit(&packet)? {
            return Ok(());
        }
        (
            session.root.clone(),
            session.run.clone(),
            session.source.clone(),
            session.cursor,
            session.owner.clone(),
        )
    };
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
    let result = loop {
        match read_ack(&root, &run, &source, checkpoint, &owner) {
            Ok(true) => break Ok(()),
            Err(e) => break Err(e),
            Ok(false) => {}
        }
        if std::time::Instant::now() >= deadline {
            break Err("probe checkpoint timed out");
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    };
    let state = app.state::<ProbeState>();
    let mut guard = state.0.lock().map_err(|_| "probe state unavailable")?;
    let session = guard.as_mut().ok_or("probe inactive")?;
    complete(session, &run, checkpoint, result).map_err(str::to_owned)
}

pub(super) fn complete(
    session: &mut Session,
    run: &str,
    checkpoint: usize,
    result: Result<(), &'static str>,
) -> Result<(), &'static str> {
    if session.run != run || session.cursor != checkpoint || !session.waiting {
        return Err("probe continuation refused");
    }
    session.waiting = false;
    if result.is_ok() {
        session.cursor += 1;
    } else {
        session.failed = true;
    }
    session.persist()?;
    result
}
