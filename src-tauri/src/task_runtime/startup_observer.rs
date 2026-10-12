//! P38S1: default-off owned native load metadata, never JS/paint acceptance.
use super::TaskRuntime;
use serde_json::{json, Value};
use std::fs::{File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tauri::{webview::PageLoadEvent, Manager, Runtime, WebviewWindowBuilder};

const FILE: &str = "startup-observer.json";
const LIMIT: u32 = 16;
const MAX_BYTES: usize = 8192;
pub(super) type Observer = Option<Arc<Mutex<LoadLog>>>;

fn gate(
    enabled: bool,
    release: bool,
    owned: bool,
    windows: &[tauri::utils::config::WindowConfig],
) -> Result<bool, &'static str> {
    if !enabled || !release {
        return Ok(false);
    }
    if !owned
        || windows.len() != 1
        || windows[0].label != "main"
        || windows[0].url != tauri::WebviewUrl::App(PathBuf::from("index.html"))
    {
        return Err("startup observer requires an owned packaged main window");
    }
    Ok(true)
}

fn allowed_origin(label: &str, url: &tauri::Url) -> bool {
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

fn open_sink(actual: &Path, expected: &Path) -> Result<File, &'static str> {
    let meta = std::fs::symlink_metadata(actual).map_err(|_| "startup report root unavailable")?;
    let owned = std::fs::canonicalize(expected).map_err(|_| "startup owned root unavailable")?;
    let resolved = std::fs::canonicalize(actual).map_err(|_| "startup report root unavailable")?;
    let parent_safe = |path: &Path| {
        path.parent()
            .and_then(|p| std::fs::symlink_metadata(p).ok())
            .is_some_and(|m| m.is_dir() && !m.file_type().is_symlink())
    };
    if resolved != owned
        || !meta.is_dir()
        || meta.file_type().is_symlink()
        || !parent_safe(actual)
        || !parent_safe(expected)
        || !std::fs::symlink_metadata(expected)
            .is_ok_and(|m| m.is_dir() && !m.file_type().is_symlink())
    {
        return Err("startup report root refused");
    }
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
        #[cfg(target_os = "macos")]
        options.custom_flags(0x100); // O_NOFOLLOW, Darwin.
    }
    options
        .open(owned.join(FILE))
        .map_err(|_| "startup receipt refused")
}

pub(super) fn prepare(
    runtime: &TaskRuntime,
    windows: &[tauri::utils::config::WindowConfig],
) -> Result<Observer, Box<dyn std::error::Error>> {
    if !gate(
        std::env::var("NEMO_TEST_STARTUP_OBSERVER").ok().as_deref() == Some("1"),
        !cfg!(debug_assertions),
        runtime.owner_token.is_some(),
        windows,
    )? {
        return Ok(None);
    }
    let expected = runtime
        .data_dir
        .parent()
        .ok_or("startup root refused")?
        .join("reports");
    let actual = PathBuf::from(std::env::var("NEMO_REPORT_DIR")?);
    let file = open_sink(&actual, &expected)?;
    let mut log = LoadLog::new(
        uuid::Uuid::new_v4().to_string(),
        runtime.task_key.clone(),
        std::process::id(),
        file,
    );
    log.persist()?;
    Ok(Some(Arc::new(Mutex::new(log))))
}

pub(super) fn attach<'a, R: Runtime, M: Manager<R>>(
    builder: WebviewWindowBuilder<'a, R, M>,
    observer: Observer,
) -> WebviewWindowBuilder<'a, R, M> {
    let Some(observer) = observer else {
        return builder;
    };
    builder.on_page_load(move |window, payload| {
        if let Ok(mut log) = observer.lock() {
            let stage = match payload.event() {
                PageLoadEvent::Started => "native-page-started",
                PageLoadEvent::Finished => "native-page-finished",
            };
            let run = log.run.clone();
            // This callback carries the real native window even if it arrives
            // during build(), before the caller confirms creation.
            log.confirm_creation(window.label(), payload.url(), std::process::id(), &run);
            log.observe(
                stage,
                window.label(),
                payload.url(),
                std::process::id(),
                &run,
            );
            if log.persist().is_err() {
                log.refused = true;
            }
        }
    })
}

pub(super) fn created<R: Runtime>(observer: &Observer, window: &tauri::WebviewWindow<R>) {
    if let Some(observer) = observer {
        if let Ok(mut log) = observer.lock() {
            let run = log.run.clone();
            match window.url() {
                Ok(origin) => {
                    log.confirm_creation(window.label(), &origin, std::process::id(), &run)
                }
                Err(_) => {
                    log.rejected += 1;
                    log.refused = true;
                }
            }
            if log.persist().is_err() {
                log.refused = true;
            }
        }
    }
}

pub(super) struct LoadLog {
    run: String,
    task_key: String,
    pid: u32,
    started: Instant,
    file: File,
    stages: Vec<Value>,
    phase: usize,
    attempts: u32,
    rejected: u32,
    dropped: u32,
    refused: bool,
}

impl LoadLog {
    fn new(run: String, task_key: String, pid: u32, file: File) -> Self {
        Self {
            run,
            task_key,
            pid,
            started: Instant::now(),
            file,
            stages: Vec::new(),
            phase: 0,
            attempts: 0,
            rejected: 0,
            dropped: 0,
            refused: false,
        }
    }
    fn confirm_creation(&mut self, label: &str, url: &tauri::Url, pid: u32, run: &str) {
        if self.phase == 0 {
            self.observe("window-created", label, url, pid, run);
        } else if pid != self.pid || run != self.run || !allowed_origin(label, url) {
            self.rejected = self.rejected.saturating_add(1);
            self.refused = true;
        }
    }
    fn observe(&mut self, stage: &str, label: &str, url: &tauri::Url, pid: u32, run: &str) {
        if self.attempts >= LIMIT {
            self.dropped = self.dropped.saturating_add(1);
            self.refused = true;
            return;
        }
        self.attempts += 1;
        let next = [
            "window-created",
            "native-page-started",
            "native-page-finished",
        ];
        if self.refused
            || pid != self.pid
            || run != self.run
            || !allowed_origin(label, url)
            || next.get(self.phase).copied() != Some(stage)
        {
            self.rejected += 1;
            self.refused = true;
            return;
        }
        self.phase += 1;
        self.stages
            .push(json!({"sequence":self.phase, "stage":stage,
            "elapsedMs":self.started.elapsed().as_millis().min(600_000) as u64}));
    }
    fn receipt(&self) -> Value {
        let status = if self.refused {
            "refused"
        } else if self.phase == 3 {
            "native-load-observed"
        } else {
            "partial"
        };
        json!({"schema":"nemo.startup-observer/1", "runId":self.run, "taskKey":self.task_key,
            "pid":self.pid, "window":"main", "status":status, "stages":self.stages,
            "accepted":self.phase, "rejected":self.rejected, "dropped":self.dropped,
            "sealed":false, "jsReadiness":"not-observed", "visiblePaint":"not-observed"})
    }
    fn persist(&mut self) -> Result<(), &'static str> {
        let bytes =
            serde_json::to_vec(&self.receipt()).map_err(|_| "startup receipt encoding failed")?;
        if bytes.len() > MAX_BYTES {
            self.refused = true;
            return Err("startup receipt budget exceeded");
        }
        self.file
            .seek(SeekFrom::Start(0))
            .and_then(|_| self.file.write_all(&bytes))
            .and_then(|_| self.file.set_len(bytes.len() as u64))
            .and_then(|_| self.file.flush())
            .map_err(|_| "startup receipt write failed")
    }
}

#[cfg(test)]
#[path = "startup_observer_tests.rs"]
mod tests;
