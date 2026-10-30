//! Private file consumer for the existing main-window object bootstrap command.
use crate::application_mcp::ApplicationMcp;
use crate::native_application_contract::{
    bounded_id, host_error, require_api_instance, HostResult,
};
use crate::native_object_bootstrap::{bootstrap_file_bytes, bootstrap_raw, ObjectBootstrapReceipt};
use nemo_mcp::contract::NativeApplicationError;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{fs::File, io::Read, path::Path};

const MAX_FILE_REQUEST_BYTES: usize = 4096;
const MAX_FILE_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct FileRequest {
    pub(super) api_version: u32,
    pub(super) request_id: String,
    pub(super) instance_id: String,
    pub(super) source_path: String,
    #[serde(default)]
    pub(super) project_only: bool,
    #[serde(default)]
    pub(super) cancelled_before_dispatch: bool,
    #[serde(default)]
    pub(super) replace: Option<ReplacementPins>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ReplacementPins {
    pub(super) document_id: String,
    pub(super) expected_revision: u64,
    pub(super) document_snapshot_id: String,
    pub(super) lifecycle_generation: u64,
}

fn open_regular(
    resolved: &Path,
    open: impl FnOnce(&Path) -> std::io::Result<File>,
) -> HostResult<File> {
    let metadata = resolved
        .metadata()
        .map_err(|_| host_error("unavailable", "object project file is unavailable"))?;
    if !metadata.is_file() {
        return Err(host_error(
            "invalid_request",
            "object project input must be a regular file",
        ));
    }
    // Refuse existing FIFOs before open can block. Path replacement between
    // this check and open remains a race; retain the descriptor guard too.
    let file = open(resolved)
        .map_err(|_| host_error("unavailable", "object project file cannot be opened"))?;
    if !file.metadata().is_ok_and(|m| m.is_file()) {
        return Err(host_error(
            "invalid_request",
            "object project input must be a regular file",
        ));
    }
    Ok(file)
}

pub(super) fn file_request(raw: &str) -> Option<FileRequest> {
    (raw.len() <= MAX_FILE_REQUEST_BYTES && raw.trim_start().starts_with('{'))
        .then(|| serde_json::from_str(raw).ok())
        .flatten()
}

pub(super) fn read_file(
    state: &ApplicationMcp,
    request: &FileRequest,
    allowed: impl Fn(&Path) -> bool,
) -> HostResult<Vec<u8>> {
    require_api_instance(
        request.api_version,
        &request.instance_id,
        state.instance_id(),
    )?;
    if !bounded_id(&request.request_id)
        || request.source_path.contains('\0')
        || !Path::new(&request.source_path).is_absolute()
    {
        return Err(host_error("invalid_request", "invalid object file request"));
    }
    if request.cancelled_before_dispatch {
        return Err(host_error(
            "cancelled_before_dispatch",
            "object file reopen was cancelled",
        ));
    }
    let resolved = Path::new(&request.source_path)
        .canonicalize()
        .map_err(|_| host_error("unavailable", "object project file is unavailable"))?;
    if !allowed(&resolved) {
        return Err(host_error(
            "unavailable",
            "object file is outside the existing filesystem scope",
        ));
    }
    // Both scope and open consume the same resolved path, never the raw alias.
    let file = open_regular(&resolved, |path| File::open(path))?;
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| host_error("unavailable", "object project file cannot be read"))?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(host_error(
            "invalid_request",
            "object project file exceeds byte limit",
        ));
    }
    // Original file bytes go straight to the selected strict codec;
    // embedded media never crosses an escaped JSON/JavaScript wrapper.
    if request.project_only {
        #[derive(Deserialize)]
        struct Format {
            format: Option<String>,
        }
        let format: Format = serde_json::from_slice(&bytes)
            .map_err(|_| host_error("invalid_request", "invalid project file"))?;
        if format.format.as_deref() != Some("nemo.native-project-document") {
            return Err(host_error(
                "unsupported_format",
                "file is not a native project",
            ));
        }
    }
    Ok(bytes)
}

fn not_dispatched(mut error: NativeApplicationError) -> NativeApplicationError {
    error.details = Some(json!({"disposition":"notDispatched","retryExecution":false}));
    error
}

pub(super) fn bootstrap_request(
    state: &ApplicationMcp,
    raw: &str,
    allowed: impl Fn(&Path) -> bool,
) -> HostResult<ObjectBootstrapReceipt> {
    // Both routes use direct strict DTOs, never reconstructed Value input.
    if let Some(request) = file_request(raw) {
        if request.replace.is_some() {
            return Err(host_error(
                "invalid_request",
                "replacement requires its committed main-thread route",
            ));
        }
        // Nothing is installed until the file is read, so a refused read leaves the
        // slot as it was and the caller may retry instead of fencing (#2511).
        let bytes = read_file(state, &request, allowed).map_err(not_dispatched)?;
        return bootstrap_file_bytes(state, request.request_id, &bytes);
    }
    bootstrap_raw(state, raw)
}

#[cfg(test)]
#[path = "native_object_reopen_tests.rs"]
mod tests;
