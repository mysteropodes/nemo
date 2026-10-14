//! Fixed, default-off P38 A/B project-open probe for an isolated test package.
//! It embeds only the two reviewed fixtures and waits for an owner receipt at
//! each visible checkpoint; it is not a general script or file-open bridge.
use crate::task_runtime::TaskRuntime;
use serde::Serialize;
use serde_json::json;
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};
use tauri::{Manager, Runtime, WebviewWindowBuilder};

const ENABLE_ENV: &str = "NEMO_TEST_P38_FIXTURE_OPEN_PROBE";
const EXPECTED_SOURCE_ENV: &str = "NEMO_P38_FIXTURE_OPEN_EXPECTED_SOURCE";
const BUILD_SHA: &str = match option_env!("NEMO_P38_FIXTURE_OPEN_BUILD_SHA") {
    Some(value) => value,
    None => "",
};
fn build_clean() -> bool {
    option_env!("NEMO_P38_FIXTURE_OPEN_BUILD_CLEAN") == Some("1")
}
const FIXTURE_A: &[u8] = include_bytes!("../../../tests/fixtures/p38/native-project/A.json");
const FIXTURE_B: &[u8] = include_bytes!("../../../tests/fixtures/p38/native-project/B.json");
const HASH_A: &str = "c5a3ec61decf27755b3a66a4ef47844660ac465196d7c3aa81856472bbaabbf1";
const HASH_B: &str = "75ab937f96de3d1865690e2b475d3968ed2eabfdfc57b8d73c7732aeffd3813b";
const MAX_PACKET: usize = 2048;
const PHASES: [&str; 3] = ["ready", "a-opened", "b-opened"];

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FixturePin {
    id: String,
    file: String,
    sha256: String,
    device: u64,
    inode: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Pins {
    schema: &'static str,
    run_id: String,
    task_key: String,
    source_sha: String,
    build_sha: String,
    pid: u32,
    window: &'static str,
    origin: &'static str,
    fixtures: [FixturePin; 2],
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Receipt<'a> {
    #[serde(flatten)]
    pins: &'a Pins,
    manifest_sha256: &'a str,
    checkpoint: usize,
    waiting: bool,
    failed: bool,
    failure_code: &'a Option<String>,
    result: &'static str,
}

struct Session {
    pins: Pins,
    pins_sha256: String,
    owner: String,
    root: PathBuf,
    root_dev: u64,
    root_ino: u64,
    files: [PathBuf; 2],
    file_ids: [(u64, u64); 2],
    cursor: usize,
    waiting: bool,
    failed: bool,
    failure_code: Option<String>,
}

pub(crate) struct ProbeState(Mutex<Option<Session>>);

fn valid_hex(value: &str, len: usize) -> bool {
    value.len() == len
        && value
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

fn verify_fixtures(session: &Session) -> Result<(), &'static str> {
    check_root(&session.root, session.root_dev, session.root_ino)?;
    for (index, expected) in [HASH_A, HASH_B].iter().enumerate() {
        let path = &session.files[index];
        let meta = fs::symlink_metadata(path).map_err(|_| "P38 fixture unavailable")?;
        if !meta.is_file()
            || meta.file_type().is_symlink()
            || meta.len() > 1_048_576
            || meta.identity() != session.file_ids[index]
        {
            return Err("P38 fixture refused");
        }
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(control::NOFOLLOW_FLAG); // O_NOFOLLOW for this Unix target.
        }
        let file = options.open(path).map_err(|_| "P38 fixture unavailable")?;
        if file
            .metadata()
            .map_err(|_| "P38 fixture unavailable")?
            .identity()
            != session.file_ids[index]
        {
            return Err("P38 fixture identity refused");
        }
        let mut bytes = Vec::new();
        file.take(1_048_577)
            .read_to_end(&mut bytes)
            .map_err(|_| "P38 fixture unavailable")?;
        let after = fs::symlink_metadata(path).map_err(|_| "P38 fixture unavailable")?;
        if !after.is_file()
            || after.file_type().is_symlink()
            || after.identity() != session.file_ids[index]
            || bytes.len() > 1_048_576
            || control::sha256_hex(&bytes) != *expected
        {
            return Err("P38 fixture hash refused");
        }
    }
    Ok(())
}

fn gate(
    enabled: bool,
    release: bool,
    isolated: bool,
    owner: bool,
    main: bool,
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
        || !main
        || !clean
        || !valid_hex(compiled, 40)
        || compiled != expected
        || control::sha256_hex(FIXTURE_A) != HASH_A
        || control::sha256_hex(FIXTURE_B) != HASH_B
    {
        return Err("P38 fixture probe requires its exact clean isolated test package");
    }
    Ok(true)
}

fn check_root(path: &Path, dev: u64, ino: u64) -> Result<(), &'static str> {
    let meta = fs::symlink_metadata(path).map_err(|_| "P38 probe root unavailable")?;
    if !meta.is_dir()
        || meta.file_type().is_symlink()
        || meta.dev() != dev
        || meta.ino() != ino
        || fs::canonicalize(path).map_err(|_| "P38 probe root unavailable")? != path
    {
        return Err("P38 probe root identity refused");
    }
    Ok(())
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

impl Session {
    fn persist(&self, result: &'static str) -> Result<(), &'static str> {
        check_root(&self.root, self.root_dev, self.root_ino)?;
        let value = serde_json::to_vec(&Receipt {
            pins: &self.pins,
            manifest_sha256: &self.pins_sha256,
            checkpoint: self.cursor,
            waiting: self.waiting,
            failed: self.failed,
            failure_code: &self.failure_code,
            result,
        })
        .map_err(|_| "P38 probe receipt unavailable")?;
        if value.len() > MAX_PACKET {
            return Err("P38 probe receipt bound exceeded");
        }
        let temp = self.root.join("manifest.next");
        let _ = fs::remove_file(&temp);
        write_new(&temp, &value).map_err(|_| "P38 probe receipt unavailable")?;
        check_root(&self.root, self.root_dev, self.root_ino)?;
        fs::rename(&temp, self.root.join("manifest.json"))
            .map_err(|_| "P38 probe receipt unavailable")
    }
}

pub(crate) fn initialize<R: Runtime>(
    app: &tauri::App<R>,
    runtime: Option<&TaskRuntime>,
) -> Result<(), Box<dyn std::error::Error>> {
    let enabled = std::env::var(ENABLE_ENV).ok().as_deref() == Some("1");
    let windows = &app.config().app.windows;
    let single_main = windows.len() == 1
        && windows[0].label == "main"
        && windows[0].url == tauri::WebviewUrl::App(PathBuf::from("index.html"));
    let expected = std::env::var(EXPECTED_SOURCE_ENV).unwrap_or_default();
    let allowed = gate(
        enabled,
        !cfg!(debug_assertions),
        runtime.is_some(),
        runtime
            .and_then(|r| r.owner_token.as_ref())
            .is_some_and(|v| !v.is_empty()),
        single_main,
        build_clean(),
        BUILD_SHA,
        &expected,
    )?;
    let session = if allowed {
        if [
            "NEMO_TEST_OBJECT_WINDOW_PROBE",
            "NEMO_TEST_OBJECT_EXPORT_PROBE",
            "NEMO_TEST_STARTUP_OBSERVER",
            "NEMO_TEST_PLAY_OBSERVER",
        ]
        .iter()
        .any(|key| std::env::var(key).ok().as_deref() == Some("1"))
        {
            return Err("P38 fixture probe refuses another scenario driver".into());
        }
        let runtime = runtime.ok_or("P38 isolated runtime missing")?;
        let owner = runtime.owner_token.clone().ok_or("P38 owner missing")?;
        let base = runtime.data_dir.parent().ok_or("P38 task root missing")?;
        let reports = base.join("reports");
        if std::env::var_os("NEMO_REPORT_DIR")
            .map(PathBuf::from)
            .as_ref()
            != Some(&reports)
        {
            return Err("P38 report root binding refused".into());
        }
        let meta = fs::symlink_metadata(&reports)?;
        if !meta.is_dir() || meta.file_type().is_symlink() || fs::canonicalize(&reports)? != reports
        {
            return Err("P38 report root refused".into());
        }
        let temp = base.join("tmp");
        let temp_meta = fs::symlink_metadata(&temp)?;
        if !temp_meta.is_dir()
            || temp_meta.file_type().is_symlink()
            || fs::canonicalize(&temp)? != temp
        {
            return Err("P38 task temp root refused".into());
        }
        let run = uuid::Uuid::new_v4().to_string();
        let root = temp.join(format!("p38-fixture-open-{run}"));
        fs::create_dir(&root)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&root, fs::Permissions::from_mode(0o700))?;
        }
        let root = fs::canonicalize(root)?;
        let root_meta = fs::symlink_metadata(&root)?;
        let files = [root.join("A.json"), root.join("B.json")];
        write_new(&files[0], FIXTURE_A)?;
        write_new(&files[1], FIXTURE_B)?;
        let file_ids = [
            fs::symlink_metadata(&files[0])?.identity(),
            fs::symlink_metadata(&files[1])?.identity(),
        ];
        let task = runtime.task_key.clone();
        let pins = Pins {
            schema: "nemo.p38-fixture-open/1",
            run_id: run,
            task_key: task,
            source_sha: BUILD_SHA.to_owned(),
            build_sha: BUILD_SHA.to_owned(),
            pid: std::process::id(),
            window: "main",
            origin: "tauri://localhost/index.html",
            fixtures: [
                FixturePin {
                    id: "A".into(),
                    file: files[0].to_string_lossy().into_owned(),
                    sha256: HASH_A.into(),
                    device: file_ids[0].0,
                    inode: file_ids[0].1,
                },
                FixturePin {
                    id: "B".into(),
                    file: files[1].to_string_lossy().into_owned(),
                    sha256: HASH_B.into(),
                    device: file_ids[1].0,
                    inode: file_ids[1].1,
                },
            ],
        };
        let pins_bytes = serde_json::to_vec(&pins)?;
        let pins_sha256 = control::sha256_hex(&pins_bytes);
        let mut session = Session {
            pins,
            pins_sha256,
            owner,
            root,
            root_dev: root_meta.dev(),
            root_ino: root_meta.ino(),
            files,
            file_ids,
            cursor: 0,
            waiting: false,
            failed: false,
            failure_code: None,
        };
        session.persist("prepared")?;
        Some(session)
    } else {
        None
    };
    if !app.manage(ProbeState(Mutex::new(session))) {
        return Err("P38 probe state already installed".into());
    }
    Ok(())
}

pub(crate) fn attach<'a, R: Runtime, M: Manager<R>>(
    builder: WebviewWindowBuilder<'a, R, M>,
    app: &tauri::AppHandle<R>,
) -> Result<WebviewWindowBuilder<'a, R, M>, Box<dyn std::error::Error>> {
    let state = app.state::<ProbeState>();
    let guard = state.0.lock().map_err(|_| "P38 probe state unavailable")?;
    let Some(session) = guard.as_ref() else {
        return Ok(builder);
    };
    let config = json!({
        "schema":"nemo.p38-fixture-open-bootstrap/1", "runId":session.pins.run_id,
        "taskKey":session.pins.task_key, "sourceSha":session.pins.source_sha,
        "buildSha":session.pins.build_sha, "pid":session.pins.pid,
        "manifestSha256":session.pins_sha256, "fixtures":{"A":session.files[0],"B":session.files[1]}
    });
    let script = format!(
        "window.__NEMO_P38_FIXTURE_OPEN__ = {};\n{}",
        config,
        include_str!("../../../tests/desktop/p38-fixture-open-bootstrap.js")
    );
    Ok(builder
        .initialization_script(script)
        .on_page_load(|window, payload| {
            if matches!(payload.event(), tauri::webview::PageLoadEvent::Finished)
                && window
                    .url()
                    .is_ok_and(|url| control::origin(window.label(), &url))
            {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
}

#[path = "p38_fixture_open_probe_control.rs"]
pub(crate) mod control;

#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

#[cfg(unix)]
trait MetadataIdentity {
    fn identity(&self) -> (u64, u64);
}

#[cfg(unix)]
impl MetadataIdentity for std::fs::Metadata {
    fn identity(&self) -> (u64, u64) {
        (self.dev(), self.ino())
    }
}

#[cfg(test)]
#[path = "p38_fixture_open_probe_tests.rs"]
mod tests;
