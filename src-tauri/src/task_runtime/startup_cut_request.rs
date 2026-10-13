//! Private controlled-file input; no human/writer-PID or finality attestation.
use std::fs::{File, Metadata, OpenOptions};
use std::io::{Read, Seek, SeekFrom};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;

const FILE: &str = "startup-cut-request.json";
const ERROR: &str = "startup request file refused";
fn open(path: &Path) -> Result<File, &'static str> {
    #[cfg(target_os = "macos")]
    {
        // Public Darwin flags: no symlink in ANY component (including final),
        // nonblocking open prevents a raced FIFO from waiting for a writer.
        OpenOptions::new()
            .read(true)
            .custom_flags(0x20000004)
            .open(path)
            .map_err(|_| ERROR)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
        Err(ERROR)
    } // This private physical operator is Mac-only.
}
struct Directory {
    path: PathBuf,
    file: File,
    id: (u64, u64, u32, u32),
}
fn directory_id(m: &Metadata) -> (u64, u64, u32, u32) {
    (m.dev(), m.ino(), m.uid(), m.mode())
}
impl Directory {
    fn new(path: PathBuf) -> Result<Self, &'static str> {
        let file = open(&path)?;
        let m = file.metadata().map_err(|_| ERROR)?;
        if !m.is_dir() {
            return Err(ERROR);
        }
        let d = Self {
            path,
            file,
            id: directory_id(&m),
        };
        d.verify()?;
        Ok(d)
    }
    fn verify(&self) -> Result<(), &'static str> {
        let named = std::fs::symlink_metadata(&self.path).map_err(|_| ERROR)?;
        let held = self.file.metadata().map_err(|_| ERROR)?;
        if !named.is_dir()
            || named.file_type().is_symlink()
            || !held.is_dir()
            || directory_id(&named) != self.id
            || directory_id(&held) != self.id
            || std::fs::canonicalize(&self.path).ok().as_deref() != Some(&self.path)
        {
            return Err(ERROR);
        }
        Ok(())
    }
}
pub(super) struct Root {
    reports: Directory,
    task: Directory,
    uid: u32,
}
impl Root {
    pub(super) fn new(reports: &Path) -> Result<Arc<Self>, &'static str> {
        let reports = std::fs::canonicalize(reports).map_err(|_| ERROR)?;
        let task = reports.parent().ok_or(ERROR)?.to_owned();
        // This file was created by the original Store, not by request bytes.
        let native =
            std::fs::symlink_metadata(reports.join("startup-js.json")).map_err(|_| ERROR)?;
        if !native.is_file()
            || native.file_type().is_symlink()
            || native.mode() & 0o7777 != 0o600
            || native.nlink() != 1
        {
            return Err(ERROR);
        }
        let root = Arc::new(Self {
            reports: Directory::new(reports)?,
            task: Directory::new(task)?,
            uid: native.uid(),
        });
        root.verify()?;
        Ok(root)
    }
    fn verify(&self) -> Result<(), &'static str> {
        self.reports.verify()?;
        self.task.verify()?;
        if self.reports.id.2 != self.uid || self.task.id.2 != self.uid {
            return Err(ERROR);
        }
        Ok(())
    }
    pub(super) fn read(self: &Arc<Self>) -> Result<Option<Input>, &'static str> {
        self.verify()?;
        let path = self.reports.path.join(FILE);
        let named = match std::fs::symlink_metadata(&path) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(ERROR),
        };
        if !valid(&named, self.uid) {
            return Err(ERROR);
        }
        let mut file = open(&path)?;
        let held = file.metadata().map_err(|_| ERROR)?;
        if !valid(&held, self.uid) || stamp(&held) != stamp(&named) {
            return Err(ERROR);
        }
        let mut bytes = vec![0; held.len() as usize];
        file.read_exact(&mut bytes).map_err(|_| ERROR)?;
        if file.read(&mut [0]).map_err(|_| ERROR)? != 0 {
            return Err(ERROR);
        }
        let mut input = Input {
            root: self.clone(),
            path,
            file,
            stamp: stamp(&held),
            bytes,
        };
        input.reconfirm()?;
        Ok(Some(input))
    }
}
type Stamp = (u64, u64, u64, u32, u32, u64, i64, i64, i64, i64);
fn stamp(m: &Metadata) -> Stamp {
    (
        m.dev(),
        m.ino(),
        m.len(),
        m.mode(),
        m.uid(),
        m.nlink(),
        m.mtime(),
        m.mtime_nsec(),
        m.ctime(),
        m.ctime_nsec(),
    )
}
fn valid(m: &Metadata, uid: u32) -> bool {
    m.is_file()
        && !m.file_type().is_symlink()
        && m.uid() == uid
        && m.mode() & 0o7777 == 0o600
        && m.nlink() == 1
        && (1..=1024).contains(&m.len())
}
pub(super) struct Input {
    root: Arc<Root>,
    path: PathBuf,
    file: File,
    stamp: Stamp,
    bytes: Vec<u8>,
}
impl Input {
    pub(super) fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    fn stable(&self) -> Result<(), &'static str> {
        self.root.verify()?;
        let named = std::fs::symlink_metadata(&self.path).map_err(|_| ERROR)?;
        let held = self.file.metadata().map_err(|_| ERROR)?;
        if !valid(&named, self.root.uid)
            || stamp(&named) != self.stamp
            || stamp(&held) != self.stamp
        {
            return Err(ERROR);
        }
        Ok(())
    }
    pub(super) fn reconfirm(&mut self) -> Result<(), &'static str> {
        self.stable()?;
        let mut bytes = vec![0; self.bytes.len()];
        self.file
            .seek(SeekFrom::Start(0))
            .and_then(|_| self.file.read_exact(&mut bytes))
            .map_err(|_| ERROR)?;
        if bytes != self.bytes || self.file.read(&mut [0]).map_err(|_| ERROR)? != 0 {
            return Err(ERROR);
        }
        self.stable()
    }
}

#[cfg(all(test, target_os = "macos"))]
#[path = "startup_cut_request_tests.rs"]
mod tests;
