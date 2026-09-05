//! Native task-runtime isolation (R06, https://github.com/mysteropodes/nemo/issues/902).
//!
//! `engineering/runtime-isolation.md` already lists this as the open native
//! gap: *"Merely creating a `tauri-data` directory does not make the app use
//! it. The platform owner must implement and validate the actual runtime
//! override."* This module is that override, and it covers the two halves a
//! second concurrent Nemo desktop instance actually needs:
//!
//! 1. **On-disk app state.** `history`, `autosave`, `preferences`, `cache`
//!    and `output` all hang off Tauri's `appDataDir()` today (see
//!    `src/js/project.js`'s `historyDir()` and `src/js/feedback-bridge.js`).
//!    That resolves to `~/Library/Application Support/<identifier>` — one
//!    single directory shared by every running copy of the app. Two task
//!    instances would interleave version-history snapshots and feedback
//!    entries into the same tree.
//! 2. **WebKit website data.** The bigger and less obvious half. Autosave
//!    (`nemo-auto`), the recents list, the sync-folder setting and the
//!    feedback fallback store all live in `localStorage`, which on macOS is
//!    WKWebView website data keyed by the *bundle* identifier — not by
//!    anything `appDataDir()` controls. Redirecting only the filesystem
//!    roots would leave two instances silently sharing one `localStorage`.
//!    `WebviewWindowBuilder::data_store_identifier` gives this instance its
//!    own `WKWebsiteDataStore`, which is the supported way to separate it.
//!
//! **Production default behavior is unchanged.** Isolation is off unless
//! `NEMO_TASK_DATA_ROOT` is set, and that single trigger is deliberate:
//! `scripts/nemo/lib/build-runtime.cjs` already exports `NEMO_TASK_ID` into
//! the environment of the processes it spawns, so keying off the task id
//! alone could switch a normal launch into isolation by inheritance.
//!
//! When isolation *is* requested but the request is malformed, this refuses
//! to start rather than falling back to the shared user profile: silently
//! writing a task instance's state into the real install is the exact
//! failure the feature exists to prevent.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::Manager;

/// Sole activation trigger. Absent -> the app resolves its normal paths.
pub const ENV_DATA_ROOT: &str = "NEMO_TASK_DATA_ROOT";
/// Required once `ENV_DATA_ROOT` is present. Same grammar as
/// `scripts/nemo/lib/isolation.cjs`'s `validId`, so a task id is spelled
/// identically on both sides of the launcher boundary.
pub const ENV_TASK_ID: &str = "NEMO_TASK_ID";
/// Owner secret for `nemo_task_runtime_release`. Never serialized back out.
pub const ENV_OWNER_TOKEN: &str = "NEMO_TASK_OWNER_TOKEN";
/// 16-byte WebKit website-data identity, hex, dashed or not.
pub const ENV_WEB_DATA_UUID: &str = "NEMO_TASK_WEB_DATA_UUID";
/// Opaque JSON blob the launcher declares about the checkout it started from.
pub const ENV_SOURCE_IDENTITY: &str = "NEMO_TASK_SOURCE_IDENTITY";

pub const SCHEMA: &str = "nemo.native-runtime/1";
const INSTANCE_FILE: &str = "instance.json";

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskRoots {
    pub root: PathBuf,
    /// Replaces `appDataDir()` for every JS consumer.
    pub data: PathBuf,
    pub history: PathBuf,
    pub autosave: PathBuf,
    pub preferences: PathBuf,
    pub cache: PathBuf,
    pub output: PathBuf,
    /// Where this instance *records* its WebKit identity. WebKit itself
    /// stores under an OS-managed location for that identifier; see
    /// `WebDataStore::observed_paths`.
    pub webkit: PathBuf,
}

impl TaskRoots {
    fn under(root: PathBuf) -> Self {
        let data = root.join("data");
        Self {
            history: data.join("history"),
            autosave: data.join("autosave"),
            preferences: data.join("preferences"),
            cache: root.join("cache"),
            output: root.join("output"),
            webkit: root.join("webkit"),
            data,
            root,
        }
    }

    fn all(&self) -> [&PathBuf; 8] {
        [
            &self.root,
            &self.data,
            &self.history,
            &self.autosave,
            &self.preferences,
            &self.cache,
            &self.output,
            &self.webkit,
        ]
    }

    pub fn create(&self) -> std::io::Result<()> {
        for dir in self.all() {
            std::fs::create_dir_all(dir)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IsolatedTask {
    pub task_id: String,
    pub roots: TaskRoots,
    pub owner_token: Option<String>,
    pub web_data_uuid: Option<[u8; 16]>,
    pub declared_source: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Resolved {
    /// No isolation requested: the normal install, byte-for-byte today's paths.
    Default,
    Isolated(Box<IsolatedTask>),
}

impl Resolved {
    pub fn isolated(&self) -> Option<&IsolatedTask> {
        match self {
            Resolved::Default => None,
            Resolved::Isolated(task) => Some(task),
        }
    }
}

// ---- validation ---------------------------------------------------------
// Every rejection below names the offending value class, and none of them
// normalize their input: a caller that meant `../..` gets an error, never a
// quietly cleaned-up path pointing somewhere else.

fn validate_task_id(raw: &str) -> Result<String, String> {
    let ok = (1..=120).contains(&raw.len())
        && raw
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric())
        && raw
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-');
    if ok {
        Ok(raw.to_string())
    } else {
        Err(format!(
            "{ENV_TASK_ID} must be 1-120 ASCII letters, digits, dot, underscore or hyphen, starting with a letter or digit"
        ))
    }
}

/// The whole attack surface of this feature is right here: whatever path we
/// accept is a path the app will create directories in, write project state
/// to, and later delete recursively on an owner-checked release.
fn validate_data_root(raw: &str) -> Result<PathBuf, String> {
    if raw.trim().is_empty() {
        return Err(format!("{ENV_DATA_ROOT} is empty"));
    }
    if raw.contains('\0') {
        return Err(format!("{ENV_DATA_ROOT} contains a NUL byte"));
    }
    let path = PathBuf::from(raw);
    if !path.is_absolute() {
        return Err(format!("{ENV_DATA_ROOT} must be an absolute path"));
    }
    // Scan the raw string, not `path.components()`: that iterator silently
    // drops `.` segments, so `/tmp/./x` would arrive here already normalized
    // and a "we never normalize" claim would be false. Caught by
    // `traversal_relative_and_shallow_roots_are_refused`, not by reading.
    let mut depth = 0usize;
    for segment in raw.split(std::path::is_separator) {
        match segment {
            "" => {}
            "." => return Err(format!("{ENV_DATA_ROOT} must not contain a '.' component")),
            ".." => return Err(format!("{ENV_DATA_ROOT} must not contain a '..' component")),
            _ => depth += 1,
        }
    }
    // A release deletes this tree recursively. `/`, `/Users` or `~` itself
    // must never be reachable through a typo or a half-expanded variable.
    if depth < 2 {
        return Err(format!(
            "{ENV_DATA_ROOT} must be at least two directories below the filesystem root; refusing to treat '{raw}' as a disposable task root"
        ));
    }
    Ok(path)
}

fn parse_web_data_uuid(raw: &str) -> Result<[u8; 16], String> {
    let hex: String = raw.chars().filter(|c| *c != '-').collect();
    if hex.len() != 32 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!(
            "{ENV_WEB_DATA_UUID} must be a 16-byte hex UUID, with or without dashes"
        ));
    }
    let mut out = [0u8; 16];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16)
            .map_err(|_| format!("{ENV_WEB_DATA_UUID} is not valid hex"))?;
    }
    // WebKit rejects the nil UUID, and accepting it here would hand every
    // task instance the same "identity" while looking configured.
    if out == [0u8; 16] {
        return Err(format!("{ENV_WEB_DATA_UUID} must not be the nil UUID"));
    }
    Ok(out)
}

pub fn format_uuid(uuid: &[u8; 16]) -> String {
    let hex: String = uuid.iter().map(|b| format!("{b:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

/// Resolve from an explicit environment map. Kept free of `std::env` and of
/// `cfg!` so both platform branches are reachable from unit tests.
pub fn resolve_from(env: &BTreeMap<String, String>, macos: bool) -> Result<Resolved, String> {
    let Some(raw_root) = env.get(ENV_DATA_ROOT) else {
        return Ok(Resolved::Default);
    };
    let root = validate_data_root(raw_root)?;
    let task_id = match env.get(ENV_TASK_ID) {
        Some(raw) => validate_task_id(raw)?,
        None => {
            return Err(format!(
                "{ENV_DATA_ROOT} is set but {ENV_TASK_ID} is missing; an isolated instance must be nameable"
            ))
        }
    };
    let web_data_uuid = match env.get(ENV_WEB_DATA_UUID) {
        Some(raw) => Some(parse_web_data_uuid(raw)?),
        // On macOS `localStorage` is the shared surface this feature exists
        // to split, so an unnamed website-data identity is a refusal, not a
        // degraded mode that looks isolated and is not.
        None if macos => {
            return Err(format!(
                "{ENV_WEB_DATA_UUID} is required on macOS: without it two instances share one WKWebView website data store (localStorage, IndexedDB, caches)"
            ))
        }
        None => None,
    };
    Ok(Resolved::Isolated(Box::new(IsolatedTask {
        task_id,
        roots: TaskRoots::under(root),
        owner_token: env.get(ENV_OWNER_TOKEN).filter(|t| !t.is_empty()).cloned(),
        web_data_uuid,
        declared_source: env.get(ENV_SOURCE_IDENTITY).cloned(),
    })))
}

pub fn resolve_from_process_env() -> Result<Resolved, String> {
    let env: BTreeMap<String, String> = [
        ENV_DATA_ROOT,
        ENV_TASK_ID,
        ENV_OWNER_TOKEN,
        ENV_WEB_DATA_UUID,
        ENV_SOURCE_IDENTITY,
    ]
    .into_iter()
    .filter_map(|key| std::env::var(key).ok().map(|value| (key.to_string(), value)))
    .collect();
    resolve_from(&env, cfg!(target_os = "macos"))
}

// ---- owner-checked cleanup ----------------------------------------------

fn tokens_match(expected: &str, presented: &str) -> bool {
    if expected.len() != presented.len() {
        return false;
    }
    expected
        .bytes()
        .zip(presented.bytes())
        .fold(0u8, |acc, (a, b)| acc | (a ^ b))
        == 0
}

/// Remove an isolated task's own roots. Refuses without a matching owner
/// token, and re-validates the path immediately before deleting so a record
/// mutated after startup cannot redirect the removal.
pub fn release_roots(task: &IsolatedTask, presented_token: &str) -> Result<PathBuf, String> {
    let Some(expected) = task.owner_token.as_deref() else {
        return Err(format!(
            "this instance started without {ENV_OWNER_TOKEN}; cleanup refused"
        ));
    };
    if presented_token.is_empty() || !tokens_match(expected, presented_token) {
        return Err("refused: caller is not the task owner (owner token mismatch)".to_string());
    }
    let root = task.roots.root.to_str().ok_or_else(|| {
        "task root is not valid UTF-8; refusing to remove it".to_string()
    })?;
    let revalidated = validate_data_root(root)?;
    if revalidated != task.roots.root {
        return Err("task root changed since startup; cleanup refused".to_string());
    }
    if revalidated.exists() {
        std::fs::remove_dir_all(&revalidated).map_err(|e| e.to_string())?;
    }
    Ok(revalidated)
}

// ---- serialized profile -------------------------------------------------

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebDataStore {
    pub requested: bool,
    pub uuid: Option<String>,
    /// True only once the webview was actually built with this identity.
    pub applied: bool,
    pub reason: String,
    /// Directories found on disk whose name carries this UUID. Empty is not
    /// proof of failure — it is simply "not observed from here".
    pub observed_paths: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppIdentity {
    pub version: String,
    pub identifier: String,
    pub executable: Option<String>,
    pub executable_bytes: Option<u64>,
    pub pid: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeProfile {
    pub schema: &'static str,
    pub active: bool,
    pub reason: String,
    pub task_id: Option<String>,
    pub roots: Option<TaskRoots>,
    pub web_data_store: WebDataStore,
    pub app: AppIdentity,
    /// Verbatim launcher-declared source identity. See `limitations`.
    pub declared_source: Option<serde_json::Value>,
    pub limitations: Vec<String>,
}

/// Candidate WebKit website-data locations for a non-sandboxed macOS app.
/// Only used to *observe* whether a store carrying this UUID exists.
fn observed_web_data_paths(identifier: &str, uuid: &str) -> Vec<String> {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return Vec::new();
    };
    let bases = [
        home.join("Library/WebKit").join(identifier),
        home.join("Library/Caches").join(identifier).join("WebKit"),
        home.join("Library/Containers")
            .join(identifier)
            .join("Data/Library/WebKit"),
    ];
    let mut found = Vec::new();
    for base in bases {
        collect_uuid_dirs(&base, uuid, 0, &mut found);
    }
    found
}

fn collect_uuid_dirs(dir: &Path, uuid: &str, depth: usize, out: &mut Vec<String>) {
    if depth > 4 || out.len() >= 8 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        if name.contains(uuid) {
            out.push(path.to_string_lossy().to_string());
            continue;
        }
        collect_uuid_dirs(&path, uuid, depth + 1, out);
    }
}

// ---- Tauri wiring -------------------------------------------------------

pub struct RuntimeState {
    resolved: Resolved,
    web_data_applied: bool,
}

impl RuntimeState {
    pub fn profile<R: tauri::Runtime>(&self, app: &tauri::AppHandle<R>) -> RuntimeProfile {
        let identifier = app.config().identifier.clone();
        let executable = std::env::current_exe().ok();
        let app_identity = AppIdentity {
            version: app.package_info().version.to_string(),
            identifier: identifier.clone(),
            executable_bytes: executable
                .as_ref()
                .and_then(|p| std::fs::metadata(p).ok())
                .map(|m| m.len()),
            executable: executable.map(|p| p.to_string_lossy().to_string()),
            pid: std::process::id(),
        };
        let mut limitations = vec![
            "declaredSource is echoed verbatim from the launcher: it identifies the launcher that configured this instance, not the checkout the binary was compiled from".to_string(),
        ];
        let Some(task) = self.resolved.isolated() else {
            return RuntimeProfile {
                schema: SCHEMA,
                active: false,
                reason: format!("{ENV_DATA_ROOT} not set; using the default install paths"),
                task_id: None,
                roots: None,
                web_data_store: WebDataStore {
                    requested: false,
                    uuid: None,
                    applied: false,
                    reason: "default install: the app uses its normal WKWebView website data store".to_string(),
                    observed_paths: Vec::new(),
                },
                app: app_identity,
                declared_source: None,
                limitations,
            };
        };
        let uuid = task.web_data_uuid.as_ref().map(format_uuid);
        let observed_paths = match (&uuid, cfg!(target_os = "macos")) {
            (Some(u), true) => observed_web_data_paths(&identifier, u),
            _ => Vec::new(),
        };
        if uuid.is_some() && observed_paths.is_empty() {
            limitations.push(
                "no website-data directory carrying this UUID was observed from the app process; WebKit may store it elsewhere or defer creation, so treat separation as unverified until a paired two-instance run shows it"
                    .to_string(),
            );
        }
        let web_data_store = WebDataStore {
            requested: uuid.is_some(),
            applied: self.web_data_applied,
            reason: match (uuid.is_some(), self.web_data_applied) {
                (true, true) => "webview built with this data_store_identifier (needs macOS >= 14; Tauri reports no result for it)".to_string(),
                (true, false) => "identity resolved but the isolated webview was not built with it".to_string(),
                (false, _) => "no website-data identity requested on this platform".to_string(),
            },
            uuid,
            observed_paths,
        };
        RuntimeProfile {
            schema: SCHEMA,
            active: true,
            reason: format!("isolated task runtime '{}'", task.task_id),
            task_id: Some(task.task_id.clone()),
            roots: Some(task.roots.clone()),
            web_data_store,
            app: app_identity,
            declared_source: task
                .declared_source
                .as_deref()
                .and_then(|raw| serde_json::from_str(raw).ok()),
            limitations,
        }
    }
}

/// Record what this instance is, next to the state it owns. Deliberately
/// excludes the owner token: the running process already holds it, and no
/// later reader needs it to be on disk.
fn write_instance_record<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    task: &IsolatedTask,
) -> std::io::Result<()> {
    let record = serde_json::json!({
        "schema": SCHEMA,
        "taskId": task.task_id,
        "pid": std::process::id(),
        "version": app.package_info().version.to_string(),
        "identifier": app.config().identifier,
        "executable": std::env::current_exe().ok().map(|p| p.to_string_lossy().to_string()),
        "webDataUuid": task.web_data_uuid.as_ref().map(format_uuid),
        "declaredSource": task.declared_source,
    });
    std::fs::write(
        task.roots.root.join(INSTANCE_FILE),
        serde_json::to_vec_pretty(&record).unwrap_or_default(),
    )
}

/// Called first in the app's `setup`. On the default path it does nothing at
/// all beyond registering state, which is what keeps a normal launch
/// identical to today's behavior.
pub fn install(
    app: &tauri::App,
    resolved: Resolved,
    isolated_windows: &[tauri::utils::config::WindowConfig],
) -> Result<(), Box<dyn std::error::Error>> {
    let mut web_data_applied = false;
    if let Some(task) = resolved.isolated() {
        task.roots.create()?;
        // The static fs capability scopes `$APPDATA`/`$TEMP`/`$HOME`; an
        // isolated root can sit outside all three, and without this the
        // frontend's own fs calls would be denied by the plugin rather than
        // by anything visible in the UI.
        {
            use tauri_plugin_fs::FsExt;
            app.fs_scope().allow_directory(&task.roots.data, true)?;
            app.fs_scope().allow_directory(&task.roots.cache, true)?;
            app.fs_scope().allow_directory(&task.roots.output, true)?;
        }
        write_instance_record(app.handle(), task)?;
        for config in isolated_windows {
            let mut builder =
                tauri::WebviewWindowBuilder::from_config(app.handle(), config)?;
            if let Some(uuid) = task.web_data_uuid {
                builder = builder.data_store_identifier(uuid);
                web_data_applied = true;
            }
            builder.build()?;
        }
    }
    app.manage(RuntimeState {
        resolved,
        web_data_applied,
    });
    Ok(())
}

#[tauri::command]
pub fn nemo_task_runtime(
    app: tauri::AppHandle,
    state: tauri::State<'_, RuntimeState>,
) -> RuntimeProfile {
    state.profile(&app)
}

/// Owner-only cleanup. `purge_web_data` additionally asks WebKit to drop the
/// website data store for this identity; the outcome is reported as observed,
/// never assumed, because removing a store the running webview still holds is
/// not something this code can prove from here.
#[tauri::command]
pub async fn nemo_task_runtime_release(
    app: tauri::AppHandle,
    state: tauri::State<'_, RuntimeState>,
    owner_token: String,
    purge_web_data: Option<bool>,
) -> Result<serde_json::Value, String> {
    let task = state
        .resolved
        .isolated()
        .ok_or_else(|| "this instance is not isolated; nothing to release".to_string())?
        .clone();
    let removed = release_roots(&task, &owner_token)?;
    let mut web_data = serde_json::json!({ "requested": false });
    if purge_web_data.unwrap_or(false) {
        web_data = match task.web_data_uuid {
            None => serde_json::json!({ "requested": true, "removed": false, "reason": "no website-data identity on this platform" }),
            Some(uuid) => match app.remove_data_store(uuid).await {
                Ok(()) => serde_json::json!({ "requested": true, "removed": true, "uuid": format_uuid(&uuid), "reason": "WebKit accepted remove_data_store" }),
                Err(e) => serde_json::json!({ "requested": true, "removed": false, "uuid": format_uuid(&uuid), "reason": e.to_string() }),
            },
        };
    }
    Ok(serde_json::json!({
        "schema": SCHEMA,
        "released": true,
        "taskId": task.task_id,
        "removedRoot": removed.to_string_lossy(),
        "webData": web_data,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    const UUID_A: &str = "3f2b1c0d-4e5f-6071-8293-a4b5c6d7e8f9";
    const UUID_B: &str = "00112233-4455-6677-8899-aabbccddeeff";

    fn env(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn isolated(root: &str, task: &str, uuid: &str) -> BTreeMap<String, String> {
        env(&[
            (ENV_DATA_ROOT, root),
            (ENV_TASK_ID, task),
            (ENV_WEB_DATA_UUID, uuid),
        ])
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join("nemo-task-runtime-test")
            .join(format!("{}-{}", name, std::process::id()));
        std::fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    // ---- the production default must stay the production default --------

    #[test]
    fn empty_environment_resolves_to_the_default_install() {
        assert_eq!(resolve_from(&env(&[]), true).unwrap(), Resolved::Default);
    }

    #[test]
    fn a_task_id_alone_never_switches_a_normal_launch_into_isolation() {
        // build-runtime.cjs exports NEMO_TASK_ID into every process it
        // spawns; inheriting it must not redirect a normal app launch.
        let resolved = resolve_from(&env(&[(ENV_TASK_ID, "build-a")]), true).unwrap();
        assert_eq!(resolved, Resolved::Default);
    }

    // ---- two instances actually get separate roots ----------------------

    #[test]
    fn two_task_instances_resolve_disjoint_roots() {
        let a = resolve_from(&isolated("/tmp/nemo-tasks/a", "task-a", UUID_A), true).unwrap();
        let b = resolve_from(&isolated("/tmp/nemo-tasks/b", "task-b", UUID_B), true).unwrap();
        let (a, b) = (a.isolated().unwrap(), b.isolated().unwrap());
        assert_ne!(a.task_id, b.task_id);
        assert_ne!(a.web_data_uuid, b.web_data_uuid);
        for (left, right) in a.roots.all().iter().zip(b.roots.all().iter()) {
            assert_ne!(left, right, "roots must not overlap between task instances");
            assert!(!left.starts_with(right) && !right.starts_with(left));
        }
    }

    #[test]
    fn roots_hang_off_the_declared_data_root() {
        let resolved = resolve_from(&isolated("/tmp/nemo-tasks/a", "task-a", UUID_A), true).unwrap();
        let roots = &resolved.isolated().unwrap().roots;
        assert_eq!(roots.root, PathBuf::from("/tmp/nemo-tasks/a"));
        assert_eq!(roots.data, PathBuf::from("/tmp/nemo-tasks/a/data"));
        // history stays `<appData>/history/...` so the isolated tree keeps
        // the exact layout src/js/project.js already writes.
        assert_eq!(roots.history, roots.data.join("history"));
        assert_eq!(roots.autosave, roots.data.join("autosave"));
        assert_eq!(roots.preferences, roots.data.join("preferences"));
        assert_eq!(roots.cache, roots.root.join("cache"));
        assert_eq!(roots.output, roots.root.join("output"));
        assert_eq!(roots.webkit, roots.root.join("webkit"));
    }

    // ---- malformed / hostile requests -----------------------------------

    #[test]
    fn a_data_root_without_a_task_id_is_refused() {
        let err = resolve_from(&env(&[(ENV_DATA_ROOT, "/tmp/nemo-tasks/a")]), false).unwrap_err();
        assert!(err.contains(ENV_TASK_ID), "{err}");
    }

    #[test]
    fn invalid_task_ids_are_refused_not_normalized() {
        for bad in ["", "-leading", "with space", "sl/ash", "dot..ok?", &"x".repeat(121)] {
            let result = resolve_from(&isolated("/tmp/nemo-tasks/a", bad, UUID_A), true);
            assert!(result.is_err(), "expected refusal for task id {bad:?}");
        }
        assert!(resolve_from(&isolated("/tmp/nemo-tasks/a", &"x".repeat(120), UUID_A), true).is_ok());
    }

    #[test]
    fn traversal_relative_and_shallow_roots_are_refused() {
        let cases = [
            ("relative", "nemo-tasks/a"),
            ("parent component", "/tmp/nemo-tasks/../../../etc/nemo"),
            ("bare parent", "/.."),
            ("current dir component", "/tmp/./nemo-tasks"),
            ("filesystem root", "/"),
            ("one level deep", "/tmp"),
            ("empty", ""),
            ("blank", "   "),
            ("nul byte", "/tmp/nemo\0tasks/a"),
        ];
        for (label, root) in cases {
            let result = resolve_from(&isolated(root, "task-a", UUID_A), true);
            assert!(result.is_err(), "expected refusal for {label}: {root:?}");
        }
    }

    #[test]
    fn a_refused_request_never_degrades_into_the_default_install() {
        // The dangerous failure mode is "isolation asked for, silently not
        // applied": that writes task state into the real user profile.
        let result = resolve_from(&isolated("/tmp", "task-a", UUID_A), true);
        assert!(matches!(result, Err(_)));
        assert_ne!(result.ok(), Some(Resolved::Default));
    }

    // ---- website-data identity ------------------------------------------

    #[test]
    fn macos_requires_a_website_data_identity() {
        let without = env(&[(ENV_DATA_ROOT, "/tmp/nemo-tasks/a"), (ENV_TASK_ID, "task-a")]);
        let err = resolve_from(&without, true).unwrap_err();
        assert!(err.contains(ENV_WEB_DATA_UUID), "{err}");
        // Off macOS there is no WKWebView store to split, so its absence is
        // not an error — it is simply not requested.
        let resolved = resolve_from(&without, false).unwrap();
        assert_eq!(resolved.isolated().unwrap().web_data_uuid, None);
    }

    #[test]
    fn website_data_uuids_parse_dashed_and_undashed_alike() {
        let dashed = resolve_from(&isolated("/tmp/nemo-tasks/a", "task-a", UUID_A), true).unwrap();
        let plain = UUID_A.replace('-', "");
        let undashed = resolve_from(&isolated("/tmp/nemo-tasks/a", "task-a", &plain), true).unwrap();
        let uuid = dashed.isolated().unwrap().web_data_uuid.unwrap();
        assert_eq!(uuid, undashed.isolated().unwrap().web_data_uuid.unwrap());
        assert_eq!(format_uuid(&uuid), UUID_A);
    }

    #[test]
    fn malformed_and_nil_website_data_uuids_are_refused() {
        for bad in [
            "00000000-0000-0000-0000-000000000000",
            "not-a-uuid",
            "3f2b1c0d4e5f60718293a4b5c6d7e8",
            "3f2b1c0d-4e5f-6071-8293-a4b5c6d7e8fg",
        ] {
            assert!(
                resolve_from(&isolated("/tmp/nemo-tasks/a", "task-a", bad), true).is_err(),
                "expected refusal for uuid {bad:?}"
            );
        }
    }

    // ---- owner-only cleanup ---------------------------------------------

    fn task_at(root: PathBuf, token: Option<&str>) -> IsolatedTask {
        IsolatedTask {
            task_id: "task-a".to_string(),
            roots: TaskRoots::under(root),
            owner_token: token.map(str::to_string),
            web_data_uuid: None,
            declared_source: None,
        }
    }

    #[test]
    fn the_owner_can_release_its_own_roots() {
        let root = scratch("release-owner");
        let task = task_at(root.clone(), Some("tok-correct"));
        task.roots.create().unwrap();
        std::fs::write(task.roots.history.join("1.json"), b"{}").unwrap();
        assert!(task.roots.history.exists());
        let removed = release_roots(&task, "tok-correct").unwrap();
        assert_eq!(removed, root);
        assert!(!root.exists());
    }

    #[test]
    fn a_non_owner_cannot_release_and_leaves_the_state_intact() {
        let root = scratch("release-refused");
        let task = task_at(root.clone(), Some("tok-correct"));
        task.roots.create().unwrap();
        std::fs::write(task.roots.history.join("1.json"), b"{}").unwrap();
        for wrong in ["", "tok-wrong", "tok-correct-longer", "tok-correc"] {
            let err = release_roots(&task, wrong).unwrap_err();
            assert!(err.contains("not the task owner"), "{err}");
        }
        assert!(task.roots.history.join("1.json").exists());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn an_instance_started_without_a_token_refuses_every_release() {
        let root = scratch("release-tokenless");
        let task = task_at(root.clone(), None);
        task.roots.create().unwrap();
        let err = release_roots(&task, "anything").unwrap_err();
        assert!(err.contains(ENV_OWNER_TOKEN), "{err}");
        assert!(root.exists());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn release_revalidates_the_root_before_deleting_it() {
        // A record mutated after startup must not be able to aim the
        // recursive removal at a shallow path.
        let mut task = task_at(PathBuf::from("/tmp/nemo-tasks/a"), Some("tok"));
        task.roots.root = PathBuf::from("/");
        let err = release_roots(&task, "tok").unwrap_err();
        assert!(err.contains("disposable task root"), "{err}");
    }
}
