//! #2242: an MCP `asset.attach` / bitmap-stroke / import-batch request names its source image file directly. No
//! file picker runs, so nothing extends the fs scope the way a picked path does, and the scoped byte port refuses it
//! as "unavailable". Exactly like `project_scope` for `project.*` requests, a named PNG inside the app's static roots
//! is admitted here (that one file, never its folder); anything else stays outside the scope and is refused.
use crate::application_mcp::project_scope::{inside, roots, Target};
use nemo_mcp::contract::NativeApplicationRequest;
use std::path::{Path, PathBuf};
use tauri::Manager;
use tauri_plugin_fs::FsExt;

const ATTACH: &str = "command.document.object.asset.attach";
const BITMAP_STROKE: &str = "command.document.object.brush.bitmap.stroke";
const IMPORT_BATCH: &str = "command.document.object.import.batch";

fn file_path(source: &serde_json::Value) -> Option<&str> {
    (source["kind"] == "file").then(|| source["path"].as_str())?
}

/// The raw file paths of the image sources a request names (fonts are read by a separate unscoped port).
pub(crate) fn named_sources(request: &NativeApplicationRequest) -> Vec<&str> {
    let payload = &request.payload;
    match request.operation.as_str() {
        ATTACH | BITMAP_STROKE => file_path(&payload["source"]).into_iter().collect(),
        IMPORT_BATCH => payload["sources"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|row| row["mediaType"] == "image/png")
            .filter_map(|row| file_path(&row["source"]))
            .collect(),
        _ => Vec::new(),
    }
}

/// The resolved regular file a raw path names, when it lies strictly inside one of `roots`.
pub(crate) fn admissible(raw: &str, roots: &[PathBuf]) -> Option<PathBuf> {
    let path = Path::new(raw);
    if raw.contains('\0') || !path.is_absolute() {
        return None;
    }
    let resolved = path.canonicalize().ok()?;
    let inside_root = inside(&Target::File(resolved.clone()), roots);
    (resolved.is_file() && inside_root).then_some(resolved)
}

/// Admits each named source file into the fs scope. A refused or missing path is left out of the scope, so the byte
/// port answers its own typed refusal; this never fails the request.
pub(crate) fn admit<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    request: &NativeApplicationRequest,
) {
    let named = named_sources(request);
    if named.is_empty() {
        return;
    }
    let roots = roots(app);
    let scope = app.fs_scope();
    for resolved in named.into_iter().filter_map(|raw| admissible(raw, &roots)) {
        let _ = scope.allow_file(resolved);
    }
}

#[cfg(test)]
#[path = "native_mcp_source_scope_tests.rs"]
mod tests;
