//! Access to the files of an XFL document, zipped (`.fla`) or as a folder.
//! Entries are read eagerly under hard size limits; the declared sizes in a
//! zip header are never trusted (each entry is read through a bounded `take`).

use std::collections::BTreeMap;
use std::io::{Cursor, Read};
use std::path::Path;

use crate::XflError;

pub const MAX_ENTRIES: usize = 100_000;
pub const MAX_ENTRY_BYTES: u64 = 256 * 1024 * 1024;
pub const MAX_TOTAL_BYTES: u64 = 1024 * 1024 * 1024;

const OLE_MAGIC: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];

#[derive(Debug, Default)]
pub struct Archive {
    files: BTreeMap<String, Vec<u8>>,
    /// Lower-cased name → stored name, for case-insensitive fallback lookups.
    folded: BTreeMap<String, String>,
}

fn normalize(name: &str) -> String {
    name.replace('\\', "/")
        .trim_start_matches("./")
        .trim_start_matches('/')
        .to_string()
}

impl Archive {
    pub fn from_entries<I: IntoIterator<Item = (String, Vec<u8>)>>(entries: I) -> Self {
        Archive::indexed(
            entries
                .into_iter()
                .map(|(k, v)| (normalize(&k), v))
                .collect(),
        )
    }

    fn indexed(files: BTreeMap<String, Vec<u8>>) -> Self {
        let mut folded = BTreeMap::new();
        for k in files.keys() {
            folded.entry(k.to_lowercase()).or_insert_with(|| k.clone());
        }
        Archive { files, folded }
    }

    pub fn from_zip(bytes: &[u8]) -> Result<Self, XflError> {
        if bytes.starts_with(&OLE_MAGIC) {
            return Err(XflError::LegacyBinaryFla);
        }
        let mut zip = zip::ZipArchive::new(Cursor::new(bytes))
            .map_err(|e| XflError::NotAnArchive(e.to_string()))?;
        if zip.len() > MAX_ENTRIES {
            return Err(XflError::TooLarge(format!(
                "{} entries (limit {MAX_ENTRIES})",
                zip.len()
            )));
        }
        let mut files = BTreeMap::new();
        let mut total = 0u64;
        for i in 0..zip.len() {
            let entry = zip
                .by_index(i)
                .map_err(|e| XflError::NotAnArchive(e.to_string()))?;
            if entry.is_dir() {
                continue;
            }
            let name = normalize(entry.name());
            let mut data = Vec::new();
            entry
                .take(MAX_ENTRY_BYTES + 1)
                .read_to_end(&mut data)
                .map_err(|e| XflError::NotAnArchive(format!("{name}: {e}")))?;
            if data.len() as u64 > MAX_ENTRY_BYTES {
                return Err(XflError::TooLarge(format!(
                    "{name} is larger than {MAX_ENTRY_BYTES} bytes"
                )));
            }
            total += data.len() as u64;
            if total > MAX_TOTAL_BYTES {
                return Err(XflError::TooLarge(format!(
                    "more than {MAX_TOTAL_BYTES} bytes uncompressed"
                )));
            }
            if files.insert(name.clone(), data).is_some() {
                return Err(XflError::NotAnArchive(format!("duplicate entry {name}")));
            }
        }
        Ok(Archive::indexed(files))
    }

    pub fn from_dir(dir: &Path) -> Result<Self, XflError> {
        let mut files = BTreeMap::new();
        let mut total = 0u64;
        let mut stack = vec![dir.to_path_buf()];
        while let Some(current) = stack.pop() {
            let entries = std::fs::read_dir(&current)
                .map_err(|e| XflError::Io(format!("{}: {e}", current.display())))?;
            for entry in entries {
                let entry = entry.map_err(|e| XflError::Io(e.to_string()))?;
                let kind = entry.file_type().map_err(|e| XflError::Io(e.to_string()))?;
                let path = entry.path();
                // Symlinks are not followed: an XFL folder never needs them.
                if kind.is_dir() {
                    stack.push(path);
                } else if kind.is_file() {
                    if files.len() >= MAX_ENTRIES {
                        return Err(XflError::TooLarge(format!("more than {MAX_ENTRIES} files")));
                    }
                    let len = entry
                        .metadata()
                        .map_err(|e| XflError::Io(e.to_string()))?
                        .len();
                    if len > MAX_ENTRY_BYTES {
                        return Err(XflError::TooLarge(format!(
                            "{} is larger than {MAX_ENTRY_BYTES} bytes",
                            path.display()
                        )));
                    }
                    total += len;
                    if total > MAX_TOTAL_BYTES {
                        return Err(XflError::TooLarge(format!(
                            "more than {MAX_TOTAL_BYTES} bytes"
                        )));
                    }
                    let rel = path
                        .strip_prefix(dir)
                        .map_err(|e| XflError::Io(e.to_string()))?;
                    // Bounded read: the file may have grown since metadata().
                    let mut data = Vec::new();
                    std::fs::File::open(&path)
                        .and_then(|f| f.take(MAX_ENTRY_BYTES + 1).read_to_end(&mut data))
                        .map_err(|e| XflError::Io(format!("{}: {e}", path.display())))?;
                    if data.len() as u64 > MAX_ENTRY_BYTES {
                        return Err(XflError::TooLarge(format!(
                            "{} is larger than {MAX_ENTRY_BYTES} bytes",
                            path.display()
                        )));
                    }
                    total = total - len + data.len() as u64;
                    if total > MAX_TOTAL_BYTES {
                        return Err(XflError::TooLarge(format!(
                            "more than {MAX_TOTAL_BYTES} bytes"
                        )));
                    }
                    files.insert(normalize(&rel.to_string_lossy()), data);
                }
            }
        }
        Ok(Archive::indexed(files))
    }

    /// Exact lookup, then a case-insensitive one (documents saved on macOS
    /// sometimes reference library files with a different case).
    pub fn get(&self, name: &str) -> Option<&[u8]> {
        let key = normalize(name);
        if let Some(v) = self.files.get(&key) {
            return Some(v);
        }
        let stored = self.folded.get(&key.to_lowercase())?;
        self.files.get(stored).map(Vec::as_slice)
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.files.keys().map(String::as_str)
    }
}
