//! Private immutable snapshot file publication, never UI clean-state authority.
use crate::application_mcp::ApplicationMcp;
use crate::native_application_contract::{
    bounded_id, host_error, require_api_instance, require_main, HostResult,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    fs,
    fs::File,
    io::Write,
    path::{Path, PathBuf},
};
use tauri_plugin_fs::FsExt;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SaveRequest {
    api_version: u32,
    request_id: String,
    instance_id: String,
    document_id: String,
    at_revision: u64,
    document_snapshot_id: String,
    destination_path: String,
    #[serde(default)]
    cancelled_before_dispatch: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SaveReceipt {
    api_version: u32,
    request_id: String,
    instance_id: String,
    document_id: String,
    at_revision: u64,
    document_snapshot_id: String,
    byte_count: usize,
    published: bool,
    cleanup_pending: bool,
}

fn owned_regular(path: &Path, file: &File) -> bool {
    let (Ok(path_meta), Ok(file_meta)) = (fs::symlink_metadata(path), file.metadata()) else {
        return false;
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        path_meta.is_file()
            && file_meta.is_file()
            && path_meta.dev() == file_meta.dev()
            && path_meta.ino() == file_meta.ino()
    }
    #[cfg(not(unix))]
    {
        let _ = (path_meta, file_meta);
        false
    }
}

fn cleanup(path: &Path, file: &File) -> bool {
    owned_regular(path, file) && fs::remove_file(path).is_ok()
}

// No exported/Clone permit and no shared scope mutation. The core always asks
// about the explicit destination/stage pair; callback invocation order is not an API.
trait PublicationAuthority {
    fn allows(&self, destination: &Path, staged: &Path) -> bool;
}

struct ExistingScope<F>(F);
impl<F: Fn(&Path) -> bool> PublicationAuthority for ExistingScope<F> {
    fn allows(&self, destination: &Path, staged: &Path) -> bool {
        (self.0)(destination) && (self.0)(staged)
    }
}

struct SelectedFilePermit<F> {
    destination: PathBuf,
    staged: PathBuf,
    scope: tauri::fs::Scope,
    allowed_destination: F,
}

impl<F: Fn(&Path) -> bool> SelectedFilePermit<F> {
    fn new<R: tauri::Runtime, M: tauri::Manager<R>>(
        manager: &M,
        destination: &Path,
        staged: &Path,
        allowed_destination: F,
    ) -> HostResult<Self> {
        let denied = || host_error("unavailable", "object selected-file permit refused");
        // The core canonicalizes the parent and generates this one UUID stage.
        // Reaffirm the exact same-directory pair before extending a fresh scope.
        if destination == staged
            || destination.parent() != staged.parent()
            || !destination.is_absolute()
            || !staged.is_absolute()
            || !allowed_destination(destination)
        {
            return Err(denied());
        }
        let scope = tauri::fs::Scope::new(manager, &tauri::utils::config::FsScope::default())
            .map_err(|_| denied())?;
        scope.allow_file(destination).map_err(|_| denied())?;
        scope.allow_file(staged).map_err(|_| denied())?;
        let permit = Self {
            destination: destination.to_path_buf(),
            staged: staged.to_path_buf(),
            scope,
            allowed_destination,
        };
        if !permit.allows(destination, staged) {
            return Err(denied());
        }
        Ok(permit)
    }
}

impl<F: Fn(&Path) -> bool> PublicationAuthority for SelectedFilePermit<F> {
    fn allows(&self, destination: &Path, staged: &Path) -> bool {
        destination == self.destination
            && staged == self.staged
            && (self.allowed_destination)(&self.destination)
            && self.scope.is_allowed(destination)
            && self.scope.is_allowed(staged)
    }
}

pub(super) fn save_request(
    state: &ApplicationMcp,
    raw: &str,
    allowed: impl Fn(&Path) -> bool,
    effect: impl FnMut(&str, &Path) -> std::io::Result<()>,
) -> HostResult<SaveReceipt> {
    save_with_authority(state, raw, |_, _| Ok(ExistingScope(allowed)), effect)
}

pub(super) fn save_selected_request<R: tauri::Runtime, M: tauri::Manager<R>>(
    manager: &M,
    state: &ApplicationMcp,
    raw: &str,
    allowed_destination: impl Fn(&Path) -> bool,
    effect: impl FnMut(&str, &Path) -> std::io::Result<()>,
) -> HostResult<SaveReceipt> {
    save_with_authority(
        state,
        raw,
        |destination, staged| {
            SelectedFilePermit::new(manager, destination, staged, allowed_destination)
        },
        effect,
    )
}

fn save_with_authority<A: PublicationAuthority>(
    state: &ApplicationMcp,
    raw: &str,
    authorize: impl FnOnce(&Path, &Path) -> HostResult<A>,
    mut effect: impl FnMut(&str, &Path) -> std::io::Result<()>,
) -> HostResult<SaveReceipt> {
    let invalid = || host_error("invalid_request", "invalid object Save As request");
    if raw.len() > 4096 || !raw.trim_start().starts_with('{') {
        return Err(invalid());
    }
    let request: SaveRequest = serde_json::from_str(raw).map_err(|_| invalid())?;
    require_api_instance(
        request.api_version,
        &request.instance_id,
        state.instance_id(),
    )?;
    let path = Path::new(&request.destination_path);
    let name = request.destination_path.rsplit('/').next().unwrap_or("");
    if !bounded_id(&request.request_id)
        || !bounded_id(&request.document_id)
        || !bounded_id(&request.document_snapshot_id)
        || request.at_revision > 9_007_199_254_740_991
        || !path.is_absolute()
        || name.is_empty()
        || name == "."
        || name == ".."
        || name.len() > 255
        || request.destination_path.contains('\0')
    {
        return Err(invalid());
    }
    if request.cancelled_before_dispatch {
        return Err(host_error(
            "cancelled_before_dispatch",
            "object Save As was cancelled",
        ));
    }
    if !cfg!(unix) {
        return Err(host_error(
            "unavailable",
            "object file publication unsupported",
        ));
    }
    let parent = path
        .parent()
        .ok_or_else(invalid)?
        .canonicalize()
        .map_err(|_| host_error("unavailable", "object destination parent unavailable"))?;
    if !parent.is_dir() {
        return Err(invalid());
    }
    let destination = parent.join(name);
    let staged = parent.join(format!("nemo-object-{}.tmp", uuid::Uuid::new_v4()));
    let authority = authorize(&destination, &staged)?;
    if !authority.allows(&destination, &staged) {
        return Err(host_error(
            "unavailable",
            "object destination outside existing filesystem scope",
        ));
    }
    let bytes = match state
        .native_project_file_snapshot(
            &request.document_id,
            request.at_revision,
            &request.document_snapshot_id,
        )
        .map_err(|_| host_error("unavailable", "native project snapshot refused"))?
    {
        Some(bytes) => bytes,
        None => bare_bytes(state, &request)?,
    };
    effect("snapshot", &destination)
        .map_err(|_| host_error("unavailable", "object publication preparation refused"))?;
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    effect("create", &staged)
        .map_err(|_| host_error("unavailable", "object staging preparation refused"))?;
    if !authority.allows(&destination, &staged) {
        return Err(host_error(
            "unavailable",
            "object publication scope revoked",
        ));
    }
    let mut file = options
        .open(&staged)
        .map_err(|_| host_error("unavailable", "object staging creation refused"))?;
    let published = (|| -> std::io::Result<()> {
        effect("write", &staged)?;
        file.write_all(&bytes)?;
        effect("sync", &staged)?;
        file.sync_all()?;
        effect("link", &staged)?;
        if !owned_regular(&staged, &file) {
            return Err(std::io::Error::other("staging identity changed"));
        }
        if !authority.allows(&destination, &staged) {
            return Err(std::io::Error::other("publication scope revoked"));
        }
        // Atomic no-replace publication. Scope/check/open/link are still separate
        // path operations: adversarial parent/staging replacement is not solved.
        fs::hard_link(&staged, &destination)
    })();
    if published.is_err() {
        let cleaned = cleanup(&staged, &file);
        let mut error = host_error("unavailable", "object new-file publication refused");
        error.details = Some(json!({"published":false,"cleanupPending":!cleaned}));
        return Err(error);
    }
    let cleaned = effect("cleanup", &staged).is_ok() && cleanup(&staged, &file);
    Ok(SaveReceipt {
        api_version: request.api_version,
        request_id: request.request_id,
        instance_id: request.instance_id,
        document_id: request.document_id,
        at_revision: request.at_revision,
        document_snapshot_id: request.document_snapshot_id,
        byte_count: bytes.len(),
        published: true,
        cleanup_pending: !cleaned,
    })
}

fn bare_bytes(state: &ApplicationMcp, request: &SaveRequest) -> HostResult<Vec<u8>> {
    let invalid = || host_error("invalid_request", "invalid object serialization response");
    let query = json!({"apiVersion":request.api_version,"requestId":request.request_id,
        "instanceId":request.instance_id,"documentId":request.document_id,
        "operation":"query.document.object.serialize","payload":{"atRevision":request.at_revision}});
    let response = state
        .serialize_object_file_snapshot(&query.to_string())
        .map_err(|_| host_error("unavailable", "object serialization unavailable"))?;
    if !response.ok {
        return Err(response
            .error
            .unwrap_or_else(|| host_error("unavailable", "object serialization refused")));
    }
    let result = response.result.as_ref().ok_or_else(invalid)?;
    if response.api_version != request.api_version
        || response.request_id != request.request_id
        || response.instance_id != request.instance_id
        || response.document_id != request.document_id
        || response.content_revision != request.at_revision
        || result.as_object().is_none_or(|r| r.len() != 3)
        || result["atRevision"].as_u64() != Some(request.at_revision)
        || result["documentSnapshotId"].as_str() != Some(&request.document_snapshot_id)
    {
        return Err(host_error(
            "invalid_request",
            "object snapshot pin does not match serialization",
        ));
    }
    let bytes = result["documentJson"]
        .as_str()
        .ok_or_else(invalid)?
        .as_bytes();
    if serde_json::to_vec(&response).map_or(true, |b| b.len() > 4096) {
        return Err(invalid());
    }
    Ok(bytes.to_vec())
}

#[tauri::command]
pub(crate) fn nemo_native_object_save_as(
    window: tauri::Window,
    state: tauri::State<ApplicationMcp>,
    request_json: String,
) -> HostResult<SaveReceipt> {
    require_main(&window)?;
    save_selected_request(
        &window,
        &state,
        &request_json,
        |path| {
            window
                .try_fs_scope()
                .is_some_and(|scope| scope.is_allowed(path))
        },
        |_, _| Ok(()),
    )
}

#[cfg(test)]
mod selected_permit_tests {
    use super::*;

    #[test]
    fn selected_file_permit_requires_the_exact_ordered_pair() {
        let app = tauri::test::mock_app();
        let parent = std::env::temp_dir().canonicalize().unwrap();
        let destination = parent.join(format!("selected-{}.nemo", uuid::Uuid::new_v4()));
        let staged = parent.join(format!("nemo-object-{}.tmp", uuid::Uuid::new_v4()));
        let other = parent.join(format!("other-{}.tmp", uuid::Uuid::new_v4()));
        let permit =
            SelectedFilePermit::new(&app, &destination, &staged, |p| p == destination).unwrap();
        assert!(permit.allows(&destination, &staged));
        assert!(!permit.allows(&staged, &destination));
        assert!(!permit.allows(&other, &staged));
        assert!(!permit.allows(&destination, &other));
        assert!(!permit.scope.is_allowed(&other));
        assert!(!permit.scope.is_allowed(&parent));
    }

    #[test]
    fn selected_file_permit_uses_a_fresh_scope_without_global_or_sibling_grants() {
        let app = tauri::test::mock_app();
        let parent = std::env::temp_dir().canonicalize().unwrap();
        let destination = parent.join(format!("selected-{}.nemo", uuid::Uuid::new_v4()));
        let staged = parent.join(format!("nemo-object-{}.tmp", uuid::Uuid::new_v4()));
        let other = parent.join(format!("nemo-object-{}.tmp", uuid::Uuid::new_v4()));
        let runtime =
            tauri::fs::Scope::new(&app, &tauri::utils::config::FsScope::default()).unwrap();
        runtime.allow_file(&destination).unwrap();
        let permit =
            SelectedFilePermit::new(&app, &destination, &staged, |p| runtime.is_allowed(p))
                .unwrap();
        assert!(permit.allows(&destination, &staged));
        assert!(!runtime.is_allowed(&staged));
        assert!(!runtime.is_allowed(&other));
        let sibling =
            SelectedFilePermit::new(&app, &destination, &other, |p| runtime.is_allowed(p)).unwrap();
        assert!(!permit.scope.is_allowed(&other));
        assert!(!sibling.scope.is_allowed(&staged));
        drop(permit);
        assert!(!runtime.is_allowed(&staged));
        runtime.forbid_file(&destination).unwrap();
        assert!(!sibling.allows(&destination, &other));
    }
}
