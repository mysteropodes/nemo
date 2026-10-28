//! #2510: an MCP `project.*` request names its file or folder directly. No dialog runs, so nothing extends the fs
//! scope the way a picked path does, and the native Open, Save As and export routes refuse it as outside the scope. A
//! path inside the app's own static roots (`$HOME`, `$DOCUMENT`, `$DESKTOP`, `$DOWNLOAD`, `$TEMP`) is admitted exactly
//! here: the file, or the export folder. Anything else is refused before the request reaches the app.
pub(crate) use nemo_mcp::contract::ApplicationRequest;
use nemo_mcp::contract::{ApplicationResponse, Operation};
use std::path::{Component, Path, PathBuf};
use tauri::Manager;
use tauri_plugin_fs::FsExt;

/// The exact scope entry a project request needs.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Target {
    File(PathBuf),
    Directory(PathBuf),
}

/// The path a project request names, normalized against the filesystem: an opened file must exist, and a saved file
/// or export folder is placed under its existing parent. `None` when the request names no path.
pub(crate) fn target(request: &ApplicationRequest) -> Option<Result<Target, String>> {
    let payload = &request.payload;
    let (key, directory) = match request.operation {
        Operation::ProjectOpen | Operation::ProjectSaveAs => ("path", false),
        Operation::ProjectExport => ("destination", payload["format"] != "lottie"),
        _ => return None,
    };
    let raw = payload[key].as_str()?;
    Some(
        resolve(Path::new(raw), request.operation == Operation::ProjectOpen).map(|path| {
            if directory {
                Target::Directory(path)
            } else {
                Target::File(path)
            }
        }),
    )
}

fn resolve(raw: &Path, must_exist: bool) -> Result<PathBuf, String> {
    let refused = || format!("{} is not a usable project path", raw.display());
    if !raw.is_absolute()
        || raw
            .components()
            .any(|part| matches!(part, Component::ParentDir | Component::CurDir))
    {
        return Err(refused());
    }
    if must_exist || raw.exists() {
        return raw.canonicalize().map_err(|_| refused());
    }
    let (parent, name) = (
        raw.parent().ok_or_else(refused)?,
        raw.file_name().ok_or_else(refused)?,
    );
    Ok(parent.canonicalize().map_err(|_| refused())?.join(name))
}

/// Whether the resolved path lies inside one of the app's static roots.
pub(crate) fn inside(target: &Target, roots: &[PathBuf]) -> bool {
    let path = match target {
        Target::File(path) | Target::Directory(path) => path,
    };
    roots
        .iter()
        .any(|root| path.starts_with(root) && path != root)
}

fn roots<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Vec<PathBuf> {
    let paths = app.path();
    [
        paths.home_dir(),
        paths.document_dir(),
        paths.desktop_dir(),
        paths.download_dir(),
        paths.temp_dir(),
    ]
    .into_iter()
    .filter_map(Result::ok)
    .filter_map(|root| root.canonicalize().ok())
    .collect()
}

/// Validates the request as before (an invalid request still fails the connection), then admits a project request's
/// own path into the fs scope. A path that cannot be admitted answers a typed refusal the caller writes back, so the
/// client sees why instead of a dropped connection.
pub(crate) fn validate_and_admit<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    request: &ApplicationRequest,
) -> Result<Option<ApplicationResponse>, String> {
    request.validate().map_err(|error| error.to_string())?;
    let Some(target) = target(request) else {
        return Ok(None);
    };
    let admitted = target.and_then(|target| {
        if !inside(&target, &roots(app)) {
            return Err("project path is outside the app's file roots".into());
        }
        let scope = app.fs_scope();
        match &target {
            Target::File(path) => scope.allow_file(path),
            Target::Directory(path) => scope.allow_directory(path, true),
        }
        .map_err(|error| error.to_string())
    });
    Ok(admitted.err().map(|message| refusal(request, message)))
}

/// A typed refusal for a project request whose path was not admitted, echoing only the request's own identity.
pub(crate) fn refusal(request: &ApplicationRequest, message: String) -> ApplicationResponse {
    serde_json::from_value(serde_json::json!({
        "apiVersion": request.api_version,
        "requestId": request.request_id,
        "instanceId": request.instance_id.clone().unwrap_or_default(),
        "documentId": request.document_id.clone().unwrap_or_default(),
        "revision": request.expected_revision.unwrap_or(0),
        "ok": false,
        "error": {"code": "invalid_request", "message": message},
    }))
    .expect("a refusal is a well-formed application response")
}

#[cfg(test)]
#[path = "native_mcp_project_scope_tests.rs"]
mod tests;
