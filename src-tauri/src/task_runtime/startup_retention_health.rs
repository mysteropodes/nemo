//! Point-in-time Store proof only; no owner/producer locks, writes or seal.
use super::super::cut::RetentionEvidence;
use super::*;
use std::fs::Metadata;

pub(super) struct RootIdentity {
    reports: (u64, u64),
    task: (u64, u64),
}
fn directories(reports: &Path) -> Result<(Metadata, Metadata), &'static str> {
    let task = reports.parent().ok_or("startup health root unavailable")?;
    let a = std::fs::symlink_metadata(reports).map_err(|_| "startup health root unavailable")?;
    let b = std::fs::symlink_metadata(task).map_err(|_| "startup health root unavailable")?;
    if !a.is_dir()
        || a.file_type().is_symlink()
        || !b.is_dir()
        || b.file_type().is_symlink()
        || std::fs::canonicalize(reports).ok().as_deref() != Some(reports)
    {
        return Err("startup health root refused");
    }
    Ok((a, b))
}
impl RootIdentity {
    pub(super) fn capture(reports: &Path) -> Result<Self, &'static str> {
        let (a, b) = directories(reports)?;
        let pins = Self {
            reports: (a.dev(), a.ino()),
            task: (b.dev(), b.ino()),
        };
        pins.verify(reports)?;
        Ok(pins)
    }
    pub(super) fn verify(&self, reports: &Path) -> Result<(), &'static str> {
        let (a, b) = directories(reports)?;
        if self.reports != (a.dev(), a.ino()) || self.task != (b.dev(), b.ino()) {
            return Err("startup health root identity refused");
        }
        Ok(())
    }
}

fn stamp(m: &Metadata) -> (u64, u64, u64, u32, i64, i64, i64, i64) {
    (
        m.dev(),
        m.ino(),
        m.len(),
        m.mode(),
        m.mtime(),
        m.mtime_nsec(),
        m.ctime(),
        m.ctime_nsec(),
    )
}
fn owned(store: &Store, m: &Metadata) -> bool {
    m.is_file()
        && !m.file_type().is_symlink()
        && m.dev() == store.dev
        && m.ino() == store.ino
        && m.mode() & 0o7777 == 0o600
}
fn root(store: &Store) -> Result<(), &'static str> {
    let reports = store
        .path
        .parent()
        .ok_or("startup health root unavailable")?;
    store.roots.verify(reports)
}

impl Store {
    /// Host-only native expectations. Stale caller expectations do not poison
    /// healthy disk; actual tamper/IO failures are sticky. Proof cannot survive
    /// later writes: the future coordinator must recheck at terminal capture.
    #[allow(dead_code)]
    pub(in super::super) fn current_health(
        &self,
        version: u64,
        snapshot: &Value,
    ) -> Result<RetentionEvidence, &'static str> {
        let expected =
            serde_json::to_vec(snapshot).map_err(|_| "startup health expectation refused")?;
        if version == 0
            || !shape(snapshot)
            || expected.len() > MAX_BYTES
            || snapshot["js"]["runId"] != self.run
            || snapshot["js"]["taskKey"] != self.task
            || snapshot["js"]["pid"].as_u64() != Some(self.pid)
        {
            return Err("startup health expectation refused");
        }
        let mut disk = self.disk.lock().map_err(|_| "startup health poisoned")?;
        if disk.failed {
            return Err("startup retention failed");
        }
        if version != disk.version || disk.last.as_ref() != Some(snapshot) {
            return Err("startup health stale expectation");
        }
        let result = (|| {
            root(self)?;
            let pid = u32::try_from(self.pid).map_err(|_| "startup health owner refused")?;
            if pid == 0 || disk.bytes != expected || disk.bytes.len() > MAX_BYTES {
                return Err("startup health retained bytes refused");
            }
            let named = std::fs::symlink_metadata(&self.path)
                .map_err(|_| "startup health path unavailable")?;
            let held = disk
                .file
                .metadata()
                .map_err(|_| "startup health inode unavailable")?;
            if !owned(self, &named)
                || !owned(self, &held)
                || stamp(&named) != stamp(&held)
                || held.len() != expected.len() as u64
            {
                return Err("startup health identity refused");
            }
            let mut bytes = vec![0; expected.len()];
            disk.file
                .seek(SeekFrom::Start(0))
                .and_then(|_| disk.file.read_exact(&mut bytes))
                .map_err(|_| "startup health read failed")?;
            let after = disk
                .file
                .metadata()
                .map_err(|_| "startup health inode unavailable")?;
            let named_after = std::fs::symlink_metadata(&self.path)
                .map_err(|_| "startup health path unavailable")?;
            root(self)?;
            if bytes != expected
                || !owned(self, &named_after)
                || !owned(self, &after)
                || stamp(&held) != stamp(&after)
                || stamp(&held) != stamp(&named_after)
            {
                return Err("startup health concurrent change refused");
            }
            Ok(RetentionEvidence {
                run: self.run.clone(),
                task: self.task.clone(),
                pid,
                version: disk.version,
            })
        })();
        if result.is_err() {
            disk.failed = true;
        }
        result
    }
}

#[cfg(test)]
#[path = "startup_retention_health_tests.rs"]
mod tests;
