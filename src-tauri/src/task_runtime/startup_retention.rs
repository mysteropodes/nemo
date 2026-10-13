//! One bounded owned snapshot file; IO never takes producer/GUI guards.
use serde_json::Value;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const MAX_BYTES: usize = 16 * 1024;
const FILE: &str = "startup-js.json";
struct Disk {
    file: File,
    version: u64,
    failed: bool,
    last: Option<Value>,
    bytes: Vec<u8>,
}
pub(super) struct Store {
    disk: Mutex<Disk>,
    path: PathBuf,
    dev: u64,
    ino: u64,
    run: String,
    task: String,
    pid: u64,
}
fn exact(v: &Value, fields: &[&str]) -> bool {
    v.as_object()
        .is_some_and(|o| o.len() == fields.len() && o.keys().all(|k| fields.contains(&k.as_str())))
}
fn shape(v: &Value) -> bool {
    exact(v, &["adapterRefused", "js"])
        && v["adapterRefused"].is_boolean()
        && exact(
            &v["js"],
            &[
                "schema",
                "runId",
                "taskKey",
                "pid",
                "window",
                "loadGeneration",
                "nativePhase",
                "status",
                "accepted",
                "rejected",
                "rejectionOverflow",
                "fence",
                "probeRefused",
                "records",
                "sealed",
                "visiblePaint",
            ],
        )
        && v["js"]["schema"] == "nemo.startup-js-native/1"
        && v["js"]["window"] == "main"
        && v["js"]["loadGeneration"] == 1
        && v["js"]["nativePhase"].as_u64().is_some_and(|n| n <= 3)
        && v["js"]["rejected"].as_u64().is_some_and(|n| n <= 15)
        && v["js"]["rejectionOverflow"].is_boolean()
        && v["js"]["probeRefused"].is_boolean()
        && v["js"]["status"]
            .as_str()
            .is_some_and(|s| ["provisional", "native-start-correlated", "fenced"].contains(&s))
        && (v["js"]["fence"].is_null()
            || v["js"]["fence"].as_str().is_some_and(|s| {
                ["navigation", "replacement", "closed", "native-mismatch"].contains(&s)
            }))
        && v["js"]["sealed"] == false
        && v["js"]["visiblePaint"] == "not-observed"
        && v["js"]["records"].as_array().is_some_and(|r| {
            r.len() <= 15
                && v["js"]["accepted"].as_u64() == Some(r.len() as u64)
                && r.iter().enumerate().all(|(i, e)| record(e, &v["js"], i))
        })
}
fn record(e: &Value, pins: &Value, index: usize) -> bool {
    let extras: &[&str] = match e["stage"].as_str() {
        Some("early-script" | "dom-ready" | "window-load") => &[],
        Some("ready-state-observed") => &["readyState"],
        Some("startup-control") => &["control", "visibility"],
        Some("raf-ack") => &["ordinal"],
        Some("js-error" | "unhandled-rejection") => &["category"],
        Some("probe-refused") => &["reason"],
        _ => return false,
    };
    let fields = [
        vec!["schema", "runId", "loadGeneration", "sequence", "stage"],
        extras.to_vec(),
    ]
    .concat();
    let allowed = |k: &str, values: &[&str]| {
        e.get(k).is_none() || e[k].as_str().is_some_and(|s| values.contains(&s))
    };
    exact(e, &fields)
        && e["schema"] == "nemo.startup-js/1"
        && e["runId"] == pins["runId"]
        && e["loadGeneration"] == 1
        && e["sequence"].as_u64() == Some(index as u64 + 1)
        && allowed("readyState", &["interactive", "complete"])
        && allowed("control", &["start-open"])
        && allowed("visibility", &["missing", "hidden", "visible", "unknown"])
        && allowed(
            "category",
            &[
                "SyntaxError",
                "ReferenceError",
                "TypeError",
                "RangeError",
                "unknown",
            ],
        )
        && allowed(
            "reason",
            &[
                "synthetic-event",
                "stage-order",
                "unsupported-document",
                "unsupported-port",
            ],
        )
        && (e.get("ordinal").is_none() || [Some(1), Some(2)].contains(&e["ordinal"].as_u64()))
}
impl Store {
    pub(super) fn open(actual: &Path, owned: &Path, initial: &Value) -> Result<Self, &'static str> {
        let safe_dir = |p: &Path| {
            std::fs::symlink_metadata(p).is_ok_and(|m| m.is_dir() && !m.file_type().is_symlink())
        };
        if !safe_dir(actual)
            || !safe_dir(owned)
            || !actual.parent().is_some_and(safe_dir)
            || !owned.parent().is_some_and(safe_dir)
        {
            return Err("startup retention root refused");
        }
        let root =
            std::fs::canonicalize(owned).map_err(|_| "startup retention root unavailable")?;
        if std::fs::canonicalize(actual).ok().as_ref() != Some(&root) || !shape(initial) {
            return Err("startup retention refused");
        }
        let run = initial["js"]["runId"]
            .as_str()
            .ok_or("startup retention pins refused")?
            .to_owned();
        let task = initial["js"]["taskKey"]
            .as_str()
            .ok_or("startup retention pins refused")?
            .to_owned();
        let pid = initial["js"]["pid"]
            .as_u64()
            .filter(|p| *p > 0)
            .ok_or("startup retention pins refused")?;
        if uuid::Uuid::parse_str(&run)
            .ok()
            .map(|u| u.to_string())
            .as_ref()
            != Some(&run)
            || task.len() != 64
            || !task
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err("startup retention pins refused");
        }
        let path = root.join(FILE);
        let mut options = OpenOptions::new();
        options.read(true).write(true).create_new(true).mode(0o600);
        #[cfg(target_os = "macos")]
        options.custom_flags(0x100);
        let file = options
            .open(&path)
            .map_err(|_| "startup retention file refused")?;
        let meta = file
            .metadata()
            .map_err(|_| "startup retention file refused")?;
        Ok(Self {
            disk: Mutex::new(Disk {
                file,
                version: 0,
                failed: false,
                last: None,
                bytes: Vec::new(),
            }),
            path,
            dev: meta.dev(),
            ino: meta.ino(),
            run,
            task,
            pid,
        })
    }
    pub(super) fn publish(&self, version: u64, snapshot: &Value) -> Result<(), &'static str> {
        // Encode BEFORE store IO guard, and never while producer guards exist.
        let valid = version > 0
            && shape(snapshot)
            && snapshot["js"]["runId"] == self.run
            && snapshot["js"]["taskKey"] == self.task
            && snapshot["js"]["pid"].as_u64() == Some(self.pid);
        let bytes =
            serde_json::to_vec(snapshot).map_err(|_| "startup retention encoding refused")?;
        let mut disk = self.disk.lock().map_err(|_| "startup retention poisoned")?;
        if disk.failed {
            return Err("startup retention failed");
        }
        if !valid || bytes.len() > MAX_BYTES {
            disk.failed = true;
            return Err("startup retention snapshot refused");
        }
        if let Some(last) = disk.last.as_ref() {
            if version == disk.version && snapshot != last {
                disk.failed = true;
                return Err("startup retention duplicate version refused");
            }
            let (older, newer) = if version <= disk.version {
                (snapshot, last)
            } else {
                (last, snapshot)
            };
            let a = older["js"]["records"].as_array().unwrap();
            let b = newer["js"]["records"].as_array().unwrap();
            let flags = ["adapterRefused"]
                .iter()
                .all(|k| older[k] != true || newer[k] == true)
                && ["probeRefused", "rejectionOverflow"]
                    .iter()
                    .all(|k| older["js"][k] != true || newer["js"][k] == true);
            if a.len() > b.len()
                || a != &b[..a.len()]
                || !flags
                || newer["js"]["rejected"].as_u64() < older["js"]["rejected"].as_u64()
                || newer["js"]["nativePhase"].as_u64() < older["js"]["nativePhase"].as_u64()
                || (!older["js"]["fence"].is_null() && older["js"]["fence"] != newer["js"]["fence"])
            {
                disk.failed = true;
                return Err("startup retention regression refused");
            }
        }
        let write = (|| {
            let named = std::fs::symlink_metadata(&self.path)
                .map_err(|_| "startup retention identity unavailable")?;
            let held = disk
                .file
                .metadata()
                .map_err(|_| "startup retention identity unavailable")?;
            if !named.is_file()
                || named.file_type().is_symlink()
                || named.dev() != self.dev
                || named.ino() != self.ino
                || held.dev() != self.dev
                || held.ino() != self.ino
            {
                return Err("startup retention identity refused");
            }
            if held.len() != disk.bytes.len() as u64 {
                return Err("startup retention foreign bytes refused");
            }
            let mut existing = vec![0; disk.bytes.len()];
            disk.file
                .seek(SeekFrom::Start(0))
                .and_then(|_| disk.file.read_exact(&mut existing))
                .map_err(|_| "startup retention read failed")?;
            if existing != disk.bytes {
                return Err("startup retention foreign bytes refused");
            }
            if version <= disk.version {
                return Ok(());
            } // Cover only verified retained bytes.
            disk.file
                .seek(SeekFrom::Start(0))
                .and_then(|_| disk.file.write_all(&bytes))
                .and_then(|_| disk.file.set_len(bytes.len() as u64))
                .and_then(|_| disk.file.flush())
                .map_err(|_| "startup retention write failed")?;
            disk.version = version;
            disk.last = Some(snapshot.clone());
            disk.bytes = bytes;
            Ok(())
        })();
        if write.is_err() {
            disk.failed = true;
        }
        write
    }
}

#[cfg(test)]
#[path = "startup_retention_tests.rs"]
mod tests;
