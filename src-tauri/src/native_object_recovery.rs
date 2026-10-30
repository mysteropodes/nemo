//! Private crash-recovery store for native object projects (#2360, R1).
//!
//! Writes the exact bytes of the native Save As serializer under
//! `<appData>/recovery/<slotKey>/`, never a UI-supplied path and never through a
//! JS string wrapper. A write is: create_new temp (0600) -> write_all -> sync_all
//! -> unique rename -> directory fsync -> manifest temp -> rename -> directory
//! fsync -> prune. The previous generation and manifest are untouched until the
//! manifest rename, so a kill anywhere leaves the last durable generation.
//! `store` owns publication, `list` owns listing/retention; this file is the
//! strict command surface.
use crate::application_mcp::ApplicationMcp;
use crate::native_application_contract::{
    bounded_id, host_error, require_api_instance, require_main, HostResult,
};
use crate::native_object_bootstrap::reference::RecoverySnapshot;
use nemo_mcp::contract::NativeApplicationError;
use serde::{Deserialize, Serialize};
use std::{
    io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::Manager;

#[path = "native_object_recovery_list.rs"]
pub(super) mod list;
#[path = "native_object_recovery_store.rs"]
pub(super) mod store;
pub(super) use list::{discard, list_slots, quarantine, Listing};
pub(super) use store::{publish, valid_generation_file, valid_slot_key, Entry, Limits, Manifest};
use store::{remove_slot, valid_digest, valid_text, MAX_FILE_BYTES};

const MAX_REQUEST_BYTES: usize = 4096;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Envelope {
    api_version: u32,
    request_id: String,
    instance_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WriteRequest {
    api_version: u32,
    request_id: String,
    instance_id: String,
    document_id: String,
    at_revision: u64,
    document_snapshot_id: String,
    slot_key: String,
    display_name: String,
    #[serde(default)]
    origin_path: Option<String>,
    #[serde(default)]
    supersedes: Option<String>,
    #[serde(default)]
    document_digest: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SlotRequest {
    api_version: u32,
    request_id: String,
    instance_id: String,
    slot_key: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct QuarantineRequest {
    api_version: u32,
    request_id: String,
    instance_id: String,
    slot_key: String,
    file: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RecoveryWriteReceipt {
    api_version: u32,
    request_id: String,
    instance_id: String,
    document_id: String,
    at_revision: u64,
    document_snapshot_id: String,
    slot_key: String,
    published: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<&'static str>,
    byte_count: usize,
    saved_at_ms: u64,
    cleanup_pending: bool,
    superseded_removed: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RecoveryListReceipt {
    api_version: u32,
    request_id: String,
    instance_id: String,
    #[serde(flatten)]
    listing: Listing,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RecoveryDiscardReceipt {
    api_version: u32,
    request_id: String,
    instance_id: String,
    slot_key: String,
    discarded: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RecoveryQuarantineReceipt {
    api_version: u32,
    request_id: String,
    instance_id: String,
    slot_key: String,
    quarantined: bool,
    slot_quarantined: bool,
}

fn invalid() -> NativeApplicationError {
    host_error("invalid_request", "invalid native recovery request")
}

fn parse<T: for<'de> Deserialize<'de>>(raw: &str) -> HostResult<T> {
    if raw.len() > MAX_REQUEST_BYTES || !raw.trim_start().starts_with('{') {
        return Err(invalid());
    }
    serde_json::from_str(raw).map_err(|_| invalid())
}

fn check_envelope(
    state: &ApplicationMcp,
    version: u32,
    request_id: &str,
    instance_id: &str,
) -> HostResult<()> {
    require_api_instance(version, instance_id, state.instance_id())?;
    if !bounded_id(request_id) {
        return Err(invalid());
    }
    Ok(())
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

// ---- commands ---------------------------------------------------------------

fn root_of(window: &tauri::Window) -> HostResult<PathBuf> {
    window
        .path()
        .app_data_dir()
        .map(|dir| dir.join("recovery"))
        .map_err(|_| host_error("unavailable", "native recovery directory unavailable"))
}

fn unavailable() -> NativeApplicationError {
    host_error("unavailable", "native recovery store unavailable")
}

pub(super) fn write_request(
    state: &ApplicationMcp,
    root: &Path,
    raw: &str,
    now: u64,
    mut effect: impl FnMut(&str, &Path) -> io::Result<()>,
) -> HostResult<RecoveryWriteReceipt> {
    let request: WriteRequest = parse(raw)?;
    check_envelope(
        state,
        request.api_version,
        &request.request_id,
        &request.instance_id,
    )?;
    if !bounded_id(&request.document_id)
        || !bounded_id(&request.document_snapshot_id)
        || request.at_revision > 9_007_199_254_740_991
        || !valid_slot_key(&request.slot_key)
        || request.display_name.is_empty()
        || !valid_text(&request.display_name, 128)
        || request
            .origin_path
            .as_deref()
            .is_some_and(|p| p.len() > 1024 || !Path::new(p).is_absolute() || p.contains('\0'))
        || request
            .supersedes
            .as_deref()
            .is_some_and(|s| !valid_slot_key(s) || s == request.slot_key)
        || request
            .document_digest
            .as_deref()
            .is_some_and(|d| !valid_digest(d))
    {
        return Err(invalid());
    }
    if !cfg!(unix) {
        return Err(host_error("unavailable", "native recovery unsupported"));
    }
    let receipt =
        |published: bool, reason: Option<&'static str>, byte_count, saved, pending, removed| {
            RecoveryWriteReceipt {
                api_version: request.api_version,
                request_id: request.request_id.clone(),
                instance_id: request.instance_id.clone(),
                document_id: request.document_id.clone(),
                at_revision: request.at_revision,
                document_snapshot_id: request.document_snapshot_id.clone(),
                slot_key: request.slot_key.clone(),
                published,
                reason,
                byte_count,
                saved_at_ms: saved,
                cleanup_pending: pending,
                superseded_removed: removed,
            }
        };
    let bytes = match state
        .native_project_recovery_snapshot(
            &request.document_id,
            request.at_revision,
            &request.document_snapshot_id,
        )
        .map_err(|_| host_error("unavailable", "native project snapshot refused"))?
    {
        RecoverySnapshot::Bytes(bytes) => bytes,
        RecoverySnapshot::Stale => return Ok(receipt(false, Some("stale"), 0, 0, false, false)),
        RecoverySnapshot::NotProject => {
            return Ok(receipt(false, Some("unsupported"), 0, 0, false, false))
        }
    };
    if bytes.len() > MAX_FILE_BYTES {
        return Ok(receipt(
            false,
            Some("too-large"),
            bytes.len(),
            0,
            false,
            false,
        ));
    }
    effect("snapshot", root).map_err(|_| unavailable())?;
    let entry = Entry {
        slot_key: &request.slot_key,
        display_name: &request.display_name,
        origin_path: request.origin_path.as_deref(),
        content_revision: request.at_revision,
        document_digest: request.document_digest.as_deref(),
    };
    let published = publish(root, &entry, &bytes, now, &mut effect).map_err(|_| unavailable())?;
    // Only after the new manifest is durable may the superseded slot go.
    let removed = request
        .supersedes
        .as_deref()
        .is_some_and(|slot| remove_slot(root, slot).unwrap_or(false));
    let _ = list_slots(root, now, Some(&request.slot_key), &Limits::DEFAULT);
    Ok(receipt(
        true,
        None,
        bytes.len(),
        published.saved_at_ms,
        published.cleanup_pending,
        removed,
    ))
}

pub(super) fn list_request(
    state: &ApplicationMcp,
    root: &Path,
    raw: &str,
    now: u64,
) -> HostResult<RecoveryListReceipt> {
    let request: Envelope = parse(raw)?;
    check_envelope(
        state,
        request.api_version,
        &request.request_id,
        &request.instance_id,
    )?;
    let listing = list_slots(root, now, None, &Limits::DEFAULT).map_err(|_| unavailable())?;
    Ok(RecoveryListReceipt {
        api_version: request.api_version,
        request_id: request.request_id,
        instance_id: request.instance_id,
        listing,
    })
}

pub(super) fn discard_request(
    state: &ApplicationMcp,
    root: &Path,
    raw: &str,
) -> HostResult<RecoveryDiscardReceipt> {
    let request: SlotRequest = parse(raw)?;
    check_envelope(
        state,
        request.api_version,
        &request.request_id,
        &request.instance_id,
    )?;
    if !valid_slot_key(&request.slot_key) {
        return Err(invalid());
    }
    let discarded = discard(root, &request.slot_key).map_err(|_| unavailable())?;
    Ok(RecoveryDiscardReceipt {
        api_version: request.api_version,
        request_id: request.request_id,
        instance_id: request.instance_id,
        slot_key: request.slot_key,
        discarded,
    })
}

pub(super) fn quarantine_request(
    state: &ApplicationMcp,
    root: &Path,
    raw: &str,
    now: u64,
) -> HostResult<RecoveryQuarantineReceipt> {
    let request: QuarantineRequest = parse(raw)?;
    check_envelope(
        state,
        request.api_version,
        &request.request_id,
        &request.instance_id,
    )?;
    if !valid_slot_key(&request.slot_key) || !valid_generation_file(&request.file) {
        return Err(invalid());
    }
    let (quarantined, slot_quarantined) =
        quarantine(root, &request.slot_key, &request.file, now).map_err(|_| unavailable())?;
    Ok(RecoveryQuarantineReceipt {
        api_version: request.api_version,
        request_id: request.request_id,
        instance_id: request.instance_id,
        slot_key: request.slot_key,
        quarantined,
        slot_quarantined,
    })
}

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> HostResult<T> + Send + 'static,
) -> HostResult<T> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|_| unavailable())?
}

#[tauri::command]
pub(crate) async fn nemo_native_object_recovery_write(
    window: tauri::Window,
    request_json: String,
) -> HostResult<RecoveryWriteReceipt> {
    require_main(&window)?;
    let root = root_of(&window)?;
    let app = window.app_handle().clone();
    blocking(move || {
        let state = app.state::<ApplicationMcp>();
        write_request(&state, &root, &request_json, now_ms(), |_, _| Ok(()))
    })
    .await
}

#[tauri::command]
pub(crate) async fn nemo_native_object_recovery_list(
    window: tauri::Window,
    request_json: String,
) -> HostResult<RecoveryListReceipt> {
    require_main(&window)?;
    let root = root_of(&window)?;
    let app = window.app_handle().clone();
    blocking(move || {
        let state = app.state::<ApplicationMcp>();
        list_request(&state, &root, &request_json, now_ms())
    })
    .await
}

#[tauri::command]
pub(crate) async fn nemo_native_object_recovery_discard(
    window: tauri::Window,
    request_json: String,
) -> HostResult<RecoveryDiscardReceipt> {
    require_main(&window)?;
    let root = root_of(&window)?;
    let app = window.app_handle().clone();
    blocking(move || {
        let state = app.state::<ApplicationMcp>();
        discard_request(&state, &root, &request_json)
    })
    .await
}

#[tauri::command]
pub(crate) async fn nemo_native_object_recovery_quarantine(
    window: tauri::Window,
    request_json: String,
) -> HostResult<RecoveryQuarantineReceipt> {
    require_main(&window)?;
    let root = root_of(&window)?;
    let app = window.app_handle().clone();
    blocking(move || {
        let state = app.state::<ApplicationMcp>();
        quarantine_request(&state, &root, &request_json, now_ms())
    })
    .await
}
