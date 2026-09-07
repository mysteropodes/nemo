// Task-runtime isolation for the native app (R06, https://github.com/mysteropodes/nemo/issues/902).
//
// `scripts/nemo/lib/isolation.cjs` already allocates a `tauri-data` root per
// task, and `engineering/runtime-isolation.md` says in as many words why that is
// not enough on its own: "Merely creating a `tauri-data` directory does not make
// the app use it." This module is the native half — it makes a running Nemo
// actually resolve its mutable state per task, so two task instances can run at
// once without one overwriting the other's settings, history, feedback,
// autosave or WebKit storage.
//
// Two mechanisms, because macOS keeps those two kinds of state in two different
// places:
//
//   * every `app_*_dir()` in Tauri resolves as `<platform dir>/<config
//     identifier>` (tauri 2.11.3, `src/path/desktop.rs`), so a per-task
//     identifier moves Application Support / Caches / Logs / config in one step,
//     and every consumer that goes through the path resolver — the fs plugin
//     scope, `appDataDir()` in JS, the feedback and history folders — follows
//     without knowing this module exists;
//   * WKWebView's localStorage/IndexedDB do NOT live under those directories;
//     they follow the webview's data store, so an isolated window is built with
//     an explicit `data_store_identifier` (macOS >= 14 / iOS >= 17).
//
// Isolation is OFF unless `NEMO_TAURI_DATA_DIR` is set: with that variable
// absent nothing here changes a single resolved path, and the production app
// keeps using `com.strokemotion.app` exactly as before. When it IS set,
// everything else this module needs must be present and valid or startup fails
// loudly — silently falling back to the shared production identifier would let
// an isolated run scribble on the user's real app data, which is the exact
// failure this work package exists to prevent.
//
// The task key is supplied by the launcher instead of being recomputed here on
// purpose: `isolation.cjs` already derives it (`idKey`, SHA-256 of the exact
// task id, which is what keeps case variants distinct on a case-insensitive
// filesystem). A second implementation on this side would be one more pair of
// functions that has to stay identical by hand — the maintenance trap CLAUDE.md
// §3 is about — and it would need a hashing dependency this crate does not have.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};
use tauri::{Manager, Runtime};

/// Manifest schema written into the task's `tauri-data` root.
pub const SCHEMA: &str = "nemo.native-runtime/1";
/// File name of that manifest, inside `NEMO_TAURI_DATA_DIR`.
pub const MANIFEST_FILE: &str = "native-runtime.json";

/// Presence of this variable is what turns isolation on. Nothing else in the
/// repository sets it, unlike `NEMO_TASK_ID`, which the R06 build launcher
/// already exports for every desktop build.
const ENV_DATA_DIR: &str = "NEMO_TAURI_DATA_DIR";
const ENV_TASK_ID: &str = "NEMO_TASK_ID";
const ENV_TASK_KEY: &str = "NEMO_TASK_KEY";
const ENV_OWNER_TOKEN: &str = "NEMO_TASK_OWNER_TOKEN";

/// A resolved isolated-runtime request. Construction is fallible and total:
/// either every field is valid, or the process must not start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskRuntime {
    pub task_id: String,
    pub task_key: String,
    pub data_dir: PathBuf,
    owner_token: Option<String>,
}

/// Managed state for the two commands. Present even when the app is NOT
/// isolated, so `nemo_task_runtime` can answer "isolated: false" instead of
/// failing in a way a caller could mistake for "isolation is working".
pub struct TaskRuntimeState {
    manifest: Mutex<Value>,
    owner_token: Option<String>,
    manifest_file: Option<PathBuf>,
}

// ---- validation (pure, unit-tested) ---------------------------------------

/// Mirror of the task-id rule in `scripts/nemo/lib/isolation.cjs`: 1-120 ASCII
/// letters, digits, dot, underscore or hyphen, starting with a letter or digit.
/// Rejected, never normalized — two ids that differ must stay two ids.
pub fn valid_task_id(id: &str) -> bool {
    let mut chars = id.chars();
    match chars.next() {
        Some(first) if first.is_ascii_alphanumeric() => {}
        _ => return false,
    }
    id.len() <= 120
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-')
}

/// `idKey` from `isolation.cjs`: SHA-256 of the exact task id, lowercase hex.
pub fn valid_task_key(key: &str) -> bool {
    key.len() == 64
        && key
            .chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
}

fn valid_owner_token(token: &str) -> bool {
    (16..=128).contains(&token.len())
        && token
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// `<base>.nemo-task-<16 hex>`. Stays a legal bundle-style identifier
/// (alphanumerics, dots and hyphens only) because it becomes a real directory
/// name under Application Support / Caches / Logs.
pub fn isolated_identifier(base_identifier: &str, task_key: &str) -> String {
    format!("{base_identifier}.nemo-task-{}", &task_key[..16])
}

/// First 16 bytes of the task key. Same input as the identifier, so one task id
/// always addresses one WebKit data store.
pub fn data_store_identifier(task_key: &str) -> [u8; 16] {
    let mut out = [0u8; 16];
    for (i, byte) in out.iter_mut().enumerate() {
        // Safe: `valid_task_key` proved 64 hex characters before we get here.
        *byte = u8::from_str_radix(&task_key[i * 2..i * 2 + 2], 16).unwrap_or(0);
    }
    out
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn epoch_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

impl TaskRuntime {
    /// Resolve an isolation request from an arbitrary environment lookup.
    /// Taking the lookup as a parameter is what makes every branch below
    /// testable without mutating the real process environment (and without the
    /// cross-test interference that `std::env::set_var` causes under
    /// `cargo test`'s thread-per-test model).
    ///
    /// `Ok(None)` = not isolated, run exactly as before.
    /// `Err(_)`   = isolation was requested but cannot be honored; the caller
    ///              must abort rather than fall back to the shared identifier.
    pub fn resolve<F>(get: F) -> Result<Option<Self>, String>
    where
        F: Fn(&str) -> Option<String>,
    {
        let Some(raw_dir) = get(ENV_DATA_DIR).filter(|v| !v.trim().is_empty()) else {
            return Ok(None);
        };
        let data_dir = PathBuf::from(raw_dir.trim());
        if !data_dir.is_absolute() {
            return Err(format!("{ENV_DATA_DIR} must be an absolute path"));
        }
        let task_id = get(ENV_TASK_ID)
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .ok_or_else(|| format!("{ENV_DATA_DIR} is set but {ENV_TASK_ID} is missing"))?;
        if !valid_task_id(&task_id) {
            return Err(format!(
                "{ENV_TASK_ID} must be 1-120 ASCII letters, digits, dot, underscore or hyphen, starting with a letter or digit"
            ));
        }
        let task_key = get(ENV_TASK_KEY)
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .ok_or_else(|| format!("{ENV_DATA_DIR} is set but {ENV_TASK_KEY} is missing"))?;
        if !valid_task_key(&task_key) {
            return Err(format!(
                "{ENV_TASK_KEY} must be the 64 lowercase hex characters of the task id digest produced by scripts/nemo/lib/isolation.cjs"
            ));
        }
        // Optional: without it this instance simply refuses every release
        // request. A missing token must not silently authorize one.
        let owner_token = match get(ENV_OWNER_TOKEN).map(|v| v.trim().to_string()) {
            Some(token) if token.is_empty() => None,
            Some(token) if !valid_owner_token(&token) => {
                return Err(format!("{ENV_OWNER_TOKEN} is not a valid owner token"))
            }
            Some(token) => Some(token),
            None => None,
        };
        Ok(Some(Self {
            task_id,
            task_key,
            data_dir,
            owner_token,
        }))
    }

    /// Read the real process environment.
    pub fn from_env() -> Result<Option<Self>, String> {
        Self::resolve(|key| std::env::var(key).ok())
    }

    pub fn identifier(&self, base_identifier: &str) -> String {
        isolated_identifier(base_identifier, &self.task_key)
    }

    pub fn data_store_identifier(&self) -> [u8; 16] {
        data_store_identifier(&self.task_key)
    }

    pub fn manifest_file(&self) -> PathBuf {
        self.data_dir.join(MANIFEST_FILE)
    }

    /// The only mutation of the generated context. Two edits:
    ///
    /// 1. the identifier, which is what every `app_*_dir()` is built from;
    /// 2. `create = false` on each configured window, so the window is rebuilt
    ///    in `setup` with a data store identifier instead of being created by
    ///    Tauri before we can attach one. This is the documented use of that
    ///    field (see `WindowConfig::create` in tauri-utils). Labels are left
    ///    untouched, so `capabilities/default.json`'s `"windows": ["main"]`
    ///    keeps applying to exactly the same window.
    pub fn apply_to_config(&self, config: &mut tauri::utils::config::Config) -> String {
        let identifier = self.identifier(&config.identifier);
        config.identifier = identifier.clone();
        for window in config.app.windows.iter_mut() {
            window.create = false;
        }
        identifier
    }
}

/// Resolve the app directories this instance will actually write to. Reported
/// rather than assumed: the whole claim of this module is "these paths differ
/// per task", and a manifest that echoed the intended value instead of the
/// resolved one could not prove it.
fn resolved_dirs<R: Runtime>(app: &tauri::AppHandle<R>) -> Value {
    let path = app.path();
    let one = |result: tauri::Result<PathBuf>| match result {
        Ok(dir) => json!(dir.to_string_lossy()),
        Err(err) => json!({ "error": err.to_string() }),
    };
    json!({
        "appData": one(path.app_data_dir()),
        "appLocalData": one(path.app_local_data_dir()),
        "appConfig": one(path.app_config_dir()),
        "appCache": one(path.app_cache_dir()),
        "appLog": one(path.app_log_dir()),
    })
}

fn write_manifest(file: &Path, manifest: &Value) -> std::io::Result<()> {
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = file.with_extension(format!("json.{}.tmp", std::process::id()));
    std::fs::write(
        &temporary,
        format!("{}\n", serde_json::to_string_pretty(manifest)?),
    )?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&temporary, std::fs::Permissions::from_mode(0o600))?;
    }
    std::fs::rename(&temporary, file)
}

/// Bootstrap hook, called first inside `setup` so the isolated window exists
/// before anything else resolves `"main"`.
pub fn start<R: Runtime>(
    app: &tauri::App<R>,
    runtime: Option<TaskRuntime>,
) -> Result<(), Box<dyn std::error::Error>> {
    let handle = app.handle().clone();
    let Some(runtime) = runtime else {
        app.manage(TaskRuntimeState {
            manifest: Mutex::new(json!({
                "schema": SCHEMA,
                "isolated": false,
                "identifier": handle.config().identifier,
                "pid": std::process::id(),
                "reason": "NEMO_TAURI_DATA_DIR is not set; this instance uses the shared application state",
            })),
            owner_token: None,
            manifest_file: None,
        });
        return Ok(());
    };

    // The config was already rewritten before the builder ran; read back what
    // Tauri will actually use rather than recomputing it.
    let identifier = handle.config().identifier.clone();
    let store = runtime.data_store_identifier();
    let windows = handle.config().app.windows.clone();
    let mut labels = Vec::with_capacity(windows.len());
    for window in &windows {
        tauri::WebviewWindowBuilder::from_config(&handle, window)?
            .data_store_identifier(store)
            .build()?;
        labels.push(window.label.clone());
    }

    let manifest_file = runtime.manifest_file();
    let manifest = json!({
        "schema": SCHEMA,
        "isolated": true,
        "state": "active",
        "taskId": runtime.task_id,
        "taskKey": runtime.task_key,
        "identifier": identifier,
        "dataStoreIdentifier": hex(&store),
        "dataDir": runtime.data_dir.to_string_lossy(),
        "manifestFile": manifest_file.to_string_lossy(),
        "pid": std::process::id(),
        "executable": std::env::current_exe().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default(),
        "appVersion": handle.package_info().version.to_string(),
        "windows": labels,
        "ownerTokenConfigured": runtime.owner_token.is_some(),
        "startedAtEpochMs": epoch_millis(),
        "dirs": resolved_dirs(&handle),
        // Observe only the task's known directory variables, never the full
        // environment or owner token. The harness compares these child values
        // with the launcher's intended roots instead of trusting its config.
        "processEnvironment": {
            "tempDir": std::env::temp_dir().to_string_lossy(),
            "TMPDIR": std::env::var("TMPDIR").ok(),
            "TMP": std::env::var("TMP").ok(),
            "TEMP": std::env::var("TEMP").ok(),
            "XDG_CACHE_HOME": std::env::var("XDG_CACHE_HOME").ok(),
            "NEMO_REPORT_DIR": std::env::var("NEMO_REPORT_DIR").ok(),
        },
    });
    write_manifest(&manifest_file, &manifest)?;
    // One machine-readable line for a launcher that captured stdout, matching
    // the existing browser/build launchers' "first line is JSON" convention.
    // Never includes the owner token.
    println!("{}", serde_json::to_string(&manifest)?);

    app.manage(TaskRuntimeState {
        manifest: Mutex::new(manifest),
        owner_token: runtime.owner_token.clone(),
        manifest_file: Some(manifest_file),
    });
    Ok(())
}

// ---- commands -------------------------------------------------------------

/// Source/build handshake for the running instance. Readable from the page so
/// an automated UI check can assert which task's state it is looking at before
/// it writes anything. Contains no owner token.
#[tauri::command]
pub fn nemo_task_runtime(state: tauri::State<'_, TaskRuntimeState>) -> Result<Value, String> {
    state
        .manifest
        .lock()
        .map(|manifest| manifest.clone())
        .map_err(|_| "task runtime manifest lock poisoned".to_string())
}

/// Owner-authorized release: mark the manifest released and exit.
///
/// This exists because process-group termination is not always available to the
/// owner — an instance started through macOS `open` is reparented by launchd —
/// and because "only its owner can stop it" has to be enforced by the instance
/// itself in that case. Refuses when no owner token was configured, so a
/// missing token can never be read as "no check required".
#[tauri::command]
pub fn nemo_task_runtime_release<R: Runtime>(
    app: tauri::AppHandle<R>,
    state: tauri::State<'_, TaskRuntimeState>,
    owner_token: String,
) -> Result<Value, String> {
    let Some(expected) = state.owner_token.as_ref() else {
        return Err("refused: this instance has no configured owner token".into());
    };
    if owner_token != *expected {
        return Err("refused: caller is not the task owner".into());
    }
    let manifest = {
        let mut manifest = state
            .manifest
            .lock()
            .map_err(|_| "task runtime manifest lock poisoned".to_string())?;
        manifest["state"] = json!("released");
        manifest["releasedAtEpochMs"] = json!(epoch_millis());
        manifest.clone()
    };
    if let Some(file) = state.manifest_file.as_ref() {
        write_manifest(file, &manifest).map_err(|err| err.to_string())?;
    }
    app.exit(0);
    Ok(manifest)
}

#[cfg(test)]
mod tests;
