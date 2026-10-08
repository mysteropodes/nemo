//! Private file consumer for the existing main-window object bootstrap command.
use super::{bootstrap_raw, ObjectBootstrapReceipt};
use crate::application_mcp::ApplicationMcp;
use crate::native_application_contract::{
    bounded_id, host_error, require_api_instance, HostResult,
};
use serde::Deserialize;
use std::{fs::File, io::Read, path::Path};

const MAX_FILE_REQUEST_BYTES: usize = 4096;
const MAX_FILE_BYTES: u64 = 1_048_576;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FileRequest {
    api_version: u32,
    request_id: String,
    instance_id: String,
    source_path: String,
    #[serde(default)]
    cancelled_before_dispatch: bool,
}

pub(super) fn bootstrap_request(
    state: &ApplicationMcp,
    raw: &str,
    allowed: impl Fn(&Path) -> bool,
) -> HostResult<ObjectBootstrapReceipt> {
    // Two direct strict DTO parsers preserve decoded duplicates. No Value or
    // untagged intermediate can turn malformed file input into admitted bytes.
    if raw.len() <= MAX_FILE_REQUEST_BYTES && raw.trim_start().starts_with('{') {
        if let Ok(request) = serde_json::from_str::<FileRequest>(raw) {
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
            let file = File::open(&resolved)
                .map_err(|_| host_error("unavailable", "object project file cannot be opened"))?;
            if !file.metadata().is_ok_and(|m| m.is_file()) {
                return Err(host_error(
                    "invalid_request",
                    "object project input must be a regular file",
                ));
            }
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
            let document_json = String::from_utf8(bytes)
                .map_err(|_| host_error("invalid_request", "object project file is not UTF-8"))?;
            let wrapper = serde_json::json!({"apiVersion":request.api_version,
                "requestId":request.request_id,"instanceId":request.instance_id,
                "documentJson":document_json})
            .to_string();
            // Original byte admission also bounds the entire escaped wrapper,
            // validates all records and owns the single atomic installation.
            return bootstrap_raw(state, &wrapper);
        }
    }
    bootstrap_raw(state, raw)
}

#[cfg(test)]
#[path = "native_object_reopen_tests.rs"]
mod tests;
