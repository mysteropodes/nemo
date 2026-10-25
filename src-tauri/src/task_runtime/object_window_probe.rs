//! Fixed, default-off owned-window probe. No arbitrary script or invoke input.
use crate::task_runtime::TaskRuntime;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    fs::{self, File, OpenOptions},
    io::{Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};
use tauri::{Manager, Runtime, WebviewWindowBuilder};
use tauri_plugin_fs::FsExt;

const PHASES: [&str; 6] = ["ready", "blue", "red", "undo", "redo", "released"];
const EXPORT_PHASES: [&str; 5] = ["ready", "blue", "refused", "exported", "released"];
const MAX_BYTES: usize = 2048;
const FAILURE_STAGES: &[&str] = &[
    "invoke-ready",
    "runtime-invoke",
    "runtime-pins",
    "ready-checkpoint",
    "presentation-ready",
    "presentation-geometry",
    "presentation-underlay",
    "presentation-restore",
    "identity-invoke",
    "identity-pins",
    "bootstrap-invoke",
    "bootstrap-receipt",
    "status-invoke",
    "status-pins",
    "primary-query-invoke",
    "primary-query-correlation",
    "primary-query-snapshot",
    "primary-query-record",
    "sibling-query-invoke",
    "sibling-query-correlation",
    "sibling-query-snapshot",
    "sibling-query-record",
    "preview-dpr",
    "preview-invoke",
    "preview-receipt",
    "blue-checkpoint",
    "fill-invoke",
    "fill-correlation",
    "red-checkpoint",
    "undo-invoke",
    "undo-correlation",
    "undo-checkpoint",
    "redo-invoke",
    "redo-correlation",
    "redo-checkpoint",
    "cleanup-invoke",
    "cleanup-receipt",
    "released-checkpoint",
];
const FAILURE_CODES: &[&str] = &[
    "invalid_request",
    "unavailable",
    "wrong_instance",
    "wrong_document",
    "stale_revision",
    "stale_owner",
    "cancelled_before_dispatch",
    "cleanup_failed",
    "revision_unacknowledged",
];
fn failure_stage_allowed(stage: &str) -> bool {
    match stage.split_once(':') {
        Some((base, code)) => {
            FAILURE_STAGES.contains(&base)
                && base.ends_with("-invoke")
                && FAILURE_CODES.contains(&code)
        }
        None => FAILURE_STAGES.contains(&stage),
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Packet {
    run_id: String,
    phase: String,
    status: String,
    work_id: Option<String>,
    revision: Option<u64>,
    cleanup_succeeded: Option<bool>,
    failure_stage: Option<String>,
    export: Option<control::ExportEvidence>,
}
struct Session {
    run: String,
    source: String,
    task: String,
    pid: u32,
    owner: String,
    root: PathBuf,
    export_destination: Option<PathBuf>,
    sink: File,
    cursor: usize,
    waiting: bool,
    failed: bool,
    records: Vec<Value>,
    export: bool,
}
pub(crate) struct ProbeState(Mutex<Option<Session>>);

fn hex(value: &str, len: usize) -> bool {
    value.len() == len
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn gate(
    enabled: bool,
    release: bool,
    isolated: bool,
    owner: bool,
    single_main: bool,
    clean: bool,
    compiled: &str,
    expected: &str,
) -> Result<bool, &'static str> {
    if !enabled {
        return Ok(false);
    }
    if !release
        || !isolated
        || !owner
        || !single_main
        || !clean
        || !hex(compiled, 40)
        || compiled != expected
    {
        return Err(
            "object probe requires isolated clean source and one owned release main window",
        );
    }
    Ok(true)
}
fn origin(label: &str, url: &tauri::Url) -> bool {
    label == "main"
        && url.scheme() == "tauri"
        && url.host_str() == Some("localhost")
        && url.port().is_none()
        && url.username().is_empty()
        && url.password().is_none()
        && ["", "/", "/index.html"].contains(&url.path())
        && url.query().is_none()
        && url.fragment().is_none()
}
fn safe_root(path: &Path) -> Result<(), &'static str> {
    let meta = fs::symlink_metadata(path).map_err(|_| "probe report root unavailable")?;
    if !meta.is_dir()
        || meta.file_type().is_symlink()
        || fs::canonicalize(path).map_err(|_| "probe root unavailable")? != path
    {
        return Err("probe report root refused");
    }
    Ok(())
}
fn sink(root: &Path) -> Result<File, &'static str> {
    safe_root(root)?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
        .open(root.join("object-window-probe.json"))
        .map_err(|_| "fresh probe receipt refused")
}
fn scoped_export_root(report: &Path, temp: &Path) -> Result<PathBuf, &'static str> {
    safe_root(report)?;
    let meta = fs::symlink_metadata(temp).map_err(|_| "probe temp root unavailable")?;
    let canonical = fs::canonicalize(temp).map_err(|_| "probe temp root unavailable")?;
    if !report.ends_with("reports")
        || !meta.is_dir()
        || meta.file_type().is_symlink()
        || canonical != report.with_file_name("tmp")
    {
        return Err("probe temp scope binding refused");
    }
    Ok(canonical)
}
fn scoped_export_destination(
    report: &Path,
    temp: &Path,
    run: &str,
) -> Result<PathBuf, &'static str> {
    let root = scoped_export_root(report, temp)?;
    if uuid::Uuid::parse_str(run).map_or(true, |parsed| parsed.to_string() != run) {
        return Err("probe export run refused");
    }
    let destination = root.join(format!("object-export-{run}"));
    match fs::symlink_metadata(&destination) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(destination),
        _ => Err("fresh probe export destination refused"),
    }
}
fn bind_export_scope(
    destination: &Path,
    is_allowed: impl Fn(&Path) -> bool,
    allow_file: impl Fn(&Path) -> Result<(), ()>,
) -> Result<(), &'static str> {
    let parent = destination.parent().ok_or("probe export parent missing")?;
    let sibling = destination.with_file_name(format!(
        "{}-sibling",
        destination
            .file_name()
            .ok_or("probe export destination missing")?
            .to_string_lossy()
    ));
    if is_allowed(parent) || is_allowed(&sibling) {
        return Err("probe export scope already broad");
    }
    allow_file(destination).map_err(|_| "probe export scope binding refused")?;
    if !is_allowed(destination)
        || is_allowed(parent)
        || is_allowed(&sibling)
        || is_allowed(&destination.join("frame_0001.png"))
    {
        return Err("probe export scope not exact");
    }
    Ok(())
}
impl Session {
    fn persist(&mut self) -> Result<(), &'static str> {
        safe_root(&self.root)?;
        let schema = if self.export {
            "nemo.object-window-export-probe/1"
        } else {
            "nemo.object-window-probe/1"
        };
        let phases = if self.export {
            EXPORT_PHASES.len()
        } else {
            PHASES.len()
        };
        let value = json!({"schema":schema,"runId":self.run,"sourceSha":self.source,
            "taskKey":self.task,"pid":self.pid,"window":"main","checkpoint":self.cursor,
            "waiting":self.waiting,"failed":self.failed,"complete":self.cursor==phases&&!self.failed&&!self.waiting,"records":self.records});
        let bytes = serde_json::to_vec(&value).map_err(|_| "probe receipt invalid")?;
        if bytes.len() > 8192 {
            return Err("probe receipt bound exceeded");
        }
        self.sink
            .seek(SeekFrom::Start(0))
            .map_err(|_| "probe receipt unavailable")?;
        self.sink
            .set_len(0)
            .map_err(|_| "probe receipt unavailable")?;
        self.sink
            .write_all(&bytes)
            .and_then(|_| self.sink.sync_all())
            .map_err(|_| "probe receipt unavailable")
    }
    fn admit(&mut self, packet: &Packet) -> Result<bool, &'static str> {
        if self.export {
            return control::admit_export(self, packet);
        }
        if packet.run_id != self.run || self.waiting || self.cursor >= PHASES.len() {
            return Err("probe run or sequence refused");
        }
        if packet.phase == "failed" {
            if packet.status != "operation_failed"
                || packet.work_id.is_some()
                || packet.revision.is_some()
                || packet.export.is_some()
                || !packet
                    .failure_stage
                    .as_deref()
                    .is_some_and(failure_stage_allowed)
            {
                return Err("probe failure refused");
            }
            if self
                .records
                .last()
                .is_some_and(|value| value["phase"] == "failed")
            {
                return Err("probe failure already recorded");
            }
            self.failed = true;
            self.records
                .push(json!({"phase":"failed","status":"operation_failed",
                "cleanupSucceeded":packet.cleanup_succeeded,"failureStage":packet.failure_stage}));
            self.persist()?;
            return Ok(false);
        }
        if self.failed
            || packet.cleanup_succeeded.is_some()
            || packet.failure_stage.is_some()
            || packet.export.is_some()
            || (!(1..5).contains(&self.cursor)
                && (packet.work_id.is_some() || packet.revision.is_some()))
        {
            return Err("probe terminal or extra receipt data refused");
        }
        let expected = PHASES[self.cursor];
        let status = if self.cursor == 0 {
            "ready"
        } else if self.cursor == 5 {
            "succeeded"
        } else {
            "presented"
        };
        if packet.phase != expected || packet.status != status {
            return Err("probe checkpoint refused");
        }
        if (1..5).contains(&self.cursor)
            && (packet.revision != Some((self.cursor - 1) as u64)
                || packet
                    .work_id
                    .as_ref()
                    .is_none_or(|s| s.is_empty() || s.len() > 128))
        {
            return Err("probe frame receipt refused");
        }
        self.records.push(json!({"phase":packet.phase,"status":packet.status,"workId":packet.work_id,"revision":packet.revision}));
        self.waiting = true;
        self.persist()?;
        Ok(true)
    }
}

pub(crate) fn initialize<R: Runtime>(
    app: &tauri::App<R>,
    runtime: Option<&TaskRuntime>,
) -> Result<(), Box<dyn std::error::Error>> {
    let preview = std::env::var("NEMO_TEST_OBJECT_WINDOW_PROBE")
        .ok()
        .as_deref()
        == Some("1");
    let export = std::env::var("NEMO_TEST_OBJECT_EXPORT_PROBE")
        .ok()
        .as_deref()
        == Some("1");
    if preview && export {
        return Err("object probes are mutually exclusive".into());
    }
    let windows = &app.config().app.windows;
    let compiled = option_env!("NEMO_OBJECT_PROBE_BUILD_SHA").unwrap_or("");
    let expected = std::env::var("NEMO_OBJECT_PROBE_EXPECTED_SOURCE").unwrap_or_default();
    let single = windows.len() == 1
        && windows[0].label == "main"
        && windows[0].url == tauri::WebviewUrl::App(PathBuf::from("index.html"));
    let allowed = gate(
        preview || export,
        !cfg!(debug_assertions),
        runtime.is_some(),
        runtime
            .and_then(|r| r.owner_token.as_ref())
            .is_some_and(|t| !t.is_empty()),
        single,
        option_env!("NEMO_OBJECT_PROBE_BUILD_CLEAN") == Some("1"),
        compiled,
        &expected,
    )?;
    let session = if allowed {
        if ["NEMO_TEST_STARTUP_OBSERVER", "NEMO_TEST_PLAY_OBSERVER"]
            .iter()
            .any(|key| std::env::var(key).ok().as_deref() == Some("1"))
        {
            return Err("object probe refuses another scenario driver".into());
        }
        let runtime = runtime.ok_or("probe isolation missing")?;
        let root = runtime
            .data_dir
            .parent()
            .ok_or("probe root missing")?
            .join("reports");
        if std::env::var_os("NEMO_REPORT_DIR")
            .map(PathBuf::from)
            .as_ref()
            != Some(&root)
        {
            return Err("probe report binding refused".into());
        }
        if fs::symlink_metadata(&root)?.file_type().is_symlink() {
            return Err("probe report binding is a symlink".into());
        }
        // Pin the validated macOS /var alias to its canonical task root.
        let root = fs::canonicalize(root)?;
        let run = uuid::Uuid::new_v4().to_string();
        let export_destination = export
            .then(|| scoped_export_destination(&root, &std::env::temp_dir(), &run))
            .transpose()?;
        let mut session = Session {
            run,
            source: compiled.into(),
            task: runtime.task_key.clone(),
            pid: std::process::id(),
            owner: runtime.owner_token.clone().ok_or("probe owner missing")?,
            sink: sink(&root)?,
            root,
            export_destination,
            cursor: 0,
            waiting: false,
            failed: false,
            records: vec![],
            export,
        };
        session.persist()?;
        Some(session)
    } else {
        None
    };
    let export_destination = session
        .as_ref()
        .and_then(|session| session.export_destination.clone());
    if !app.manage(ProbeState(Mutex::new(session))) {
        return Err("probe state already installed".into());
    }
    if let Some(destination) = export_destination {
        let scope = app.fs_scope();
        bind_export_scope(
            &destination,
            |path| scope.is_allowed(path),
            |path| scope.allow_file(path).map_err(|_| ()),
        )?;
    }
    Ok(())
}
pub(crate) fn attach<'a, R: Runtime, M: Manager<R>>(
    builder: WebviewWindowBuilder<'a, R, M>,
    app: &tauri::AppHandle<R>,
) -> Result<WebviewWindowBuilder<'a, R, M>, Box<dyn std::error::Error>> {
    let state = app.state::<ProbeState>();
    let guard = state.0.lock().map_err(|_| "probe state unavailable")?;
    let Some(session) = guard.as_ref() else {
        return Ok(builder);
    };
    let script = if session.export {
        let destination = session
            .export_destination
            .as_ref()
            .ok_or("probe export destination missing")?;
        let config = json!({"runId":session.run,"sourceSha":session.source,"taskKey":session.task,
            "pid":session.pid,"destinationDirectory":destination});
        format!(
            "window.__NEMO_OBJECT_EXPORT_PROBE_CONFIG__ = {};\n{}",
            config,
            include_str!("../../../tests/desktop/object-export-probe-bootstrap.js")
        )
    } else {
        let config = json!({"runId":session.run,"sourceSha":session.source,"taskKey":session.task,"pid":session.pid});
        format!(
            "window.__NEMO_OBJECT_WINDOW_PROBE_CONFIG__ = {};\n{}",
            config,
            include_str!("../../../tests/desktop/object-window-probe-bootstrap.js")
        )
    };
    Ok(builder
        .initialization_script(script)
        .on_page_load(|window, payload| {
            if matches!(payload.event(), tauri::webview::PageLoadEvent::Finished)
                && window.url().is_ok_and(|url| origin(window.label(), &url))
            {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
}
#[path = "object_window_probe_control.rs"]
mod control;
#[allow(unused_imports)]
pub(crate) use control::{
    __cmd__nemo_test_object_window_probe, __tauri_command_name_nemo_test_object_window_probe,
    nemo_test_object_window_probe,
};

#[cfg(test)]
#[path = "object_window_probe_tests.rs"]
mod tests;
