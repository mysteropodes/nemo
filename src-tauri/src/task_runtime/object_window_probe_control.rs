//! Strict data-only checkpoint acknowledgment and bounded async continuation.
use crate::task_runtime::object_window_probe::origin;
use crate::task_runtime::object_window_probe::Session;
use crate::task_runtime::object_window_probe::{hex, safe_root, Packet, ProbeState, MAX_BYTES};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::Read,
    path::Path,
};
use tauri::Manager;

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ExportEvidence {
    api_version: u32,
    request_id: String,
    instance_id: String,
    document_id: String,
    content_revision: u64,
    document_snapshot_id: String,
    status: String,
    cleanup_status: String,
    external_effect_disposition: String,
    error_code: Option<String>,
    artifact_files: Vec<String>,
    published: bool,
    cleanup_complete: bool,
}

pub(super) fn admit_export(session: &mut Session, packet: &Packet) -> Result<bool, &'static str> {
    use super::EXPORT_PHASES;
    if packet.run_id != session.run || session.waiting || session.cursor >= EXPORT_PHASES.len() {
        return Err("export probe run or sequence refused");
    }
    if packet.phase == "failed" {
        if packet.status != "operation_failed"
            || packet.work_id.is_some()
            || packet.revision.is_some()
            || packet.export.is_some()
            || packet.cleanup_succeeded.is_none()
            || !matches!(
                packet.failure_stage.as_deref(),
                Some(
                    "runtime-binding"
                        | "ready"
                        | "identity"
                        | "bootstrap"
                        | "pins"
                        | "preview"
                        | "blue-checkpoint"
                        | "stale-refusal"
                        | "refusal-checkpoint"
                        | "export"
                        | "export-checkpoint"
                        | "release"
                        | "release-checkpoint"
                )
            )
            || session
                .records
                .last()
                .is_some_and(|row| row["phase"] == "failed")
        {
            return Err("export probe failure refused");
        }
        session.failed = true;
        session.records.push(
            serde_json::json!({"phase":"failed","status":"operation_failed",
            "cleanupSucceeded":packet.cleanup_succeeded,"failureStage":packet.failure_stage}),
        );
        session.persist()?;
        return Ok(false);
    }
    if session.failed
        || packet.failure_stage.is_some()
        || packet.phase != EXPORT_PHASES[session.cursor]
    {
        return Err("export probe checkpoint refused");
    }
    let phase = session.cursor;
    let valid = match phase {
        0 => {
            packet.status == "ready"
                && packet.work_id.is_none()
                && packet.revision.is_none()
                && packet.export.is_none()
                && packet.cleanup_succeeded.is_none()
        }
        1 => {
            packet.status == "presented"
                && packet.revision == Some(0)
                && packet
                    .work_id
                    .as_ref()
                    .is_some_and(|id| !id.is_empty() && id.len() <= 128)
                && packet.export.is_none()
                && packet.cleanup_succeeded.is_none()
        }
        2 => {
            packet.status == "refused"
                && packet.revision == Some(0)
                && packet.work_id.is_none()
                && packet.export.is_none()
                && packet.cleanup_succeeded.is_none()
        }
        3 => {
            packet.status == "succeeded"
                && packet.revision == Some(0)
                && packet.work_id.is_none()
                && packet.cleanup_succeeded.is_none()
                && packet.export.as_ref().is_some_and(|value| {
                    value.api_version == 2
                        && value.request_id == format!("object-export-{}", session.run)
                        && !value.instance_id.is_empty()
                        && value.instance_id.len() <= 128
                        && !value.document_id.is_empty()
                        && value.document_id.len() <= 128
                        && !value.document_snapshot_id.is_empty()
                        && value.document_snapshot_id.len() <= 128
                        && value.content_revision == 0
                        && value.status == "succeeded"
                        && value.cleanup_status == "complete"
                        && value.external_effect_disposition == "committed"
                        && value.error_code.is_none()
                        && value.artifact_files == ["frame_0001.png"]
                        && value.published
                        && value.cleanup_complete
                })
        }
        4 => {
            packet.status == "succeeded"
                && packet.revision.is_none()
                && packet.work_id.is_none()
                && packet.export.is_none()
                && packet.cleanup_succeeded == Some(true)
        }
        _ => false,
    };
    if !valid {
        return Err("export probe receipt refused");
    }
    session.records.push(
        serde_json::json!({"phase":packet.phase,"status":packet.status,
        "workId":packet.work_id,"revision":packet.revision,"export":packet.export,
        "cleanupSucceeded":packet.cleanup_succeeded}),
    );
    session.waiting = true;
    session.persist()?;
    Ok(true)
}

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
