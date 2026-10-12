//! Same-authority inactive CPU project preparation. Never installs a second app.
use super::{NativeAuthority, NativeState};
use crate::{
    application_mcp::ApplicationMcp,
    native_application::DesktopNativeApplication,
    native_application_contract::{
        admit_project, bounded_id, host_error, require_api_instance, require_main, AdmittedProject,
        GeometryResourceInput, HostResult, HOST_API_VERSION,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    io::{self, Write},
    sync::MutexGuard,
};

const MAX_REQUEST_BYTES: usize = 4 * 1_048_576;
const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PrepareRequest {
    api_version: u32,
    request_id: String,
    instance_id: String,
    document_id: String,
    expected_revision: u64,
    lifecycle_generation: u64,
    projection: Value,
    resources: Vec<GeometryResourceInput>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CancelRequest {
    api_version: u32,
    request_id: String,
    instance_id: String,
    document_id: String,
    expected_revision: u64,
    lifecycle_generation: u64,
    preparation_token: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PreparationReceipt {
    api_version: u32,
    request_id: String,
    preparation_token: String,
    state: &'static str,
    format: &'static str,
    format_version: u32,
    layer_count: usize,
    total_frames: u32,
    geometry_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CancellationReceipt {
    api_version: u32,
    request_id: String,
    preparation_token: String,
    state: &'static str,
}

/// One immutable preparation or one bounded cancellation tombstone, never both.
/// Expired records are physically dropped on the next prepare/cancel (or owner drop).
/// An edit of A alone does not retire B; each request must still pin A's current revision.
pub(super) struct InactiveProject {
    instance_id: String,
    document_id: String,
    generation: u64,
    fingerprint: Vec<u8>,
    receipt: PreparationReceipt,
    project: Option<AdmittedProject>,
    cancellation: Option<(Vec<u8>, CancellationReceipt)>,
}

// Stop serialization at the admission ceiling rather than allocate oversized replay bytes.
#[derive(Default)]
struct Fingerprint(Vec<u8>);
impl Write for Fingerprint {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > MAX_REQUEST_BYTES.saturating_sub(self.0.len()) {
            return Err(io::Error::other("inactive request exceeds byte limit"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn fingerprint(request: &impl Serialize) -> HostResult<Vec<u8>> {
    let mut bytes = Fingerprint::default();
    serde_json::to_writer(&mut bytes, request).map_err(|_| {
        host_error(
            "invalid_request",
            "inactive request exceeds serialization limits",
        )
    })?;
    Ok(bytes.0)
}

fn lock(native: &NativeState) -> HostResult<MutexGuard<'_, NativeAuthority>> {
    native
        .lock()
        .map_err(|_| host_error("unavailable", "native authority lock unavailable"))
}

fn require_request(
    version: u32,
    requested: &str,
    actual: &str,
    request_id: &str,
    document: &str,
    revision: u64,
    generation: u64,
    window: &str,
) -> HostResult<()> {
    if window != "main" {
        return Err(host_error(
            "wrong_instance",
            "native preparation requires the main window",
        ));
    }
    require_api_instance(version, requested, actual)?;
    if !bounded_id(request_id)
        || !bounded_id(document)
        || revision > MAX_SAFE_INTEGER
        || generation == 0
        || generation > MAX_SAFE_INTEGER
    {
        return Err(host_error(
            "invalid_request",
            "invalid inactive project request identity",
        ));
    }
    Ok(())
}

fn require_owner(
    authority: &mut NativeAuthority,
    instance: &str,
    document: &str,
    revision: u64,
    generation: u64,
) -> HostResult<()> {
    let active = authority
        .active_mut(generation)
        .map_err(|message| host_error("stale_owner", message))?;
    if active.instance_id() != instance
        || active.document_id() != document
        || active.content_revision() != revision
    {
        return Err(host_error(
            "stale_owner",
            "active native project identity changed",
        ));
    }
    if !active.as_any_mut().is::<DesktopNativeApplication>() {
        return Err(host_error(
            "unavailable",
            "inactive preparation requires the opacity desktop owner",
        ));
    }
    Ok(())
}

fn retire_expired(authority: &mut NativeAuthority) {
    let expired = authority.inactive_project.as_ref().is_some_and(|slot| {
        !authority.active().is_some_and(|(generation, application)| {
            generation == slot.generation
                && application.instance_id() == slot.instance_id
                && application.document_id() == slot.document_id
        })
    });
    if expired {
        authority.inactive_project = None;
    }
}

fn replay(
    authority: &NativeAuthority,
    request: &PrepareRequest,
    fingerprint: &[u8],
) -> HostResult<Option<PreparationReceipt>> {
    let Some(slot) = &authority.inactive_project else {
        return Ok(None);
    };
    if slot.receipt.request_id == request.request_id {
        if slot.fingerprint != fingerprint {
            return Err(host_error(
                "request_conflict",
                "inactive preparation request body changed",
            ));
        }
        if slot.project.is_none() {
            return Err(host_error(
                "preparation_cancelled",
                "inactive preparation was cancelled",
            ));
        }
        return Ok(Some(slot.receipt.clone()));
    }
    if slot.project.is_some() {
        return Err(host_error(
            "preparation_busy",
            "one inactive project is already prepared",
        ));
    }
    Ok(None)
}

/// Admission occurs off-lock. The final owner check precedes ALL slot publication.
fn prepare_with(
    native: &NativeState,
    actual_instance: &str,
    window: &str,
    request: PrepareRequest,
    before_store: impl FnOnce(),
) -> HostResult<PreparationReceipt> {
    require_request(
        request.api_version,
        &request.instance_id,
        actual_instance,
        &request.request_id,
        &request.document_id,
        request.expected_revision,
        request.lifecycle_generation,
        window,
    )?;
    // Bound retained exact typed replay bytes as well as the admitted data's own limits.
    let fingerprint = fingerprint(&request)?;
    {
        let mut authority = lock(native)?;
        retire_expired(&mut authority);
        require_owner(
            &mut authority,
            &request.instance_id,
            &request.document_id,
            request.expected_revision,
            request.lifecycle_generation,
        )?;
        if let Some(receipt) = replay(&authority, &request, &fingerprint)? {
            return Ok(receipt);
        }
    }
    let admitted = admit_project(&request.projection, &request.resources)?;
    before_store();
    let mut authority = lock(native)?;
    retire_expired(&mut authority);
    require_owner(
        &mut authority,
        &request.instance_id,
        &request.document_id,
        request.expected_revision,
        request.lifecycle_generation,
    )?;
    if let Some(receipt) = replay(&authority, &request, &fingerprint)? {
        return Ok(receipt);
    }
    let receipt = PreparationReceipt {
        api_version: HOST_API_VERSION,
        request_id: request.request_id,
        preparation_token: uuid::Uuid::new_v4().to_string(),
        state: "inactive-prepared",
        format: "nemo.native-opacity-document",
        format_version: 1,
        layer_count: admitted.document.layers().len(),
        total_frames: admitted.document.total_frames(),
        geometry_count: admitted.resources.len(),
    };
    authority.inactive_project = Some(InactiveProject {
        instance_id: request.instance_id,
        document_id: request.document_id,
        generation: request.lifecycle_generation,
        fingerprint,
        receipt: receipt.clone(),
        project: Some(admitted),
        cancellation: None,
    });
    Ok(receipt)
}

fn cancel(
    native: &NativeState,
    actual_instance: &str,
    window: &str,
    request: CancelRequest,
) -> HostResult<CancellationReceipt> {
    require_request(
        request.api_version,
        &request.instance_id,
        actual_instance,
        &request.request_id,
        &request.document_id,
        request.expected_revision,
        request.lifecycle_generation,
        window,
    )?;
    if !bounded_id(&request.preparation_token) {
        return Err(host_error("invalid_request", "invalid preparation token"));
    }
    let mut authority = lock(native)?;
    retire_expired(&mut authority);
    require_owner(
        &mut authority,
        &request.instance_id,
        &request.document_id,
        request.expected_revision,
        request.lifecycle_generation,
    )?;
    let slot = authority
        .inactive_project
        .as_mut()
        .ok_or_else(|| host_error("preparation_expired", "inactive preparation is unavailable"))?;
    if slot.receipt.preparation_token != request.preparation_token {
        return Err(host_error(
            "preparation_expired",
            "inactive preparation token does not match",
        ));
    }
    let fingerprint = fingerprint(&request)?;
    if let Some((retained, receipt)) = &slot.cancellation {
        if *retained != fingerprint {
            return Err(host_error(
                "request_conflict",
                "inactive cancellation request changed",
            ));
        }
        return Ok(receipt.clone());
    }
    let receipt = CancellationReceipt {
        api_version: HOST_API_VERSION,
        request_id: request.request_id,
        preparation_token: request.preparation_token,
        state: "inactive-cancelled",
    };
    slot.project = None;
    slot.cancellation = Some((fingerprint, receipt.clone()));
    Ok(receipt)
}

#[tauri::command]
pub(crate) async fn nemo_native_prepare_inactive_project(
    window: tauri::Window,
    state: tauri::State<'_, ApplicationMcp>,
    request: PrepareRequest,
) -> HostResult<PreparationReceipt> {
    require_main(&window)?;
    prepare_with(
        &state.native_state(),
        state.instance_id(),
        window.label(),
        request,
        || {},
    )
}

#[tauri::command]
pub(crate) async fn nemo_native_cancel_inactive_project(
    window: tauri::Window,
    state: tauri::State<'_, ApplicationMcp>,
    request: CancelRequest,
) -> HostResult<CancellationReceipt> {
    require_main(&window)?;
    cancel(
        &state.native_state(),
        state.instance_id(),
        window.label(),
        request,
    )
}

#[cfg(test)]
#[path = "native_inactive_project_tests.rs"]
mod tests;
