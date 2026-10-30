//! Durable generation store for native crash recovery: bounded names, atomic
//! publication (temp, sync, unique rename, manifest rename) and slot upkeep.
use serde::{Deserialize, Serialize};
use std::{
    fs,
    fs::File,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

pub(crate) const MAX_FILE_BYTES: usize = 32 * 1024 * 1024;
pub(crate) const MAX_MANIFEST_BYTES: u64 = 64 * 1024;
pub(crate) const MAX_GENERATIONS: usize = 2;
pub(crate) const STALE_TMP_MS: u64 = 10 * 60 * 1000;
pub(crate) const MANIFEST: &str = "manifest.json";
pub(crate) const SNIFF: &str = "nemo.native-project-document";

/// Retention budget; tests shrink it, production always uses `DEFAULT`.
#[derive(Clone, Copy)]
pub(crate) struct Limits {
    pub(crate) slots: usize,
    pub(crate) bytes: u64,
    pub(crate) age_ms: u64,
}

impl Limits {
    pub(crate) const DEFAULT: Limits = Limits {
        slots: 5,
        bytes: 512 * 1024 * 1024,
        age_ms: 14 * 24 * 60 * 60 * 1000,
    };
}

pub(crate) type Effect<'a> = &'a mut dyn FnMut(&str, &Path) -> io::Result<()>;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Generation {
    pub(crate) file: String,
    pub(crate) saved_at_ms: u64,
    pub(crate) content_revision: u64,
    pub(crate) byte_count: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) document_digest: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Manifest {
    pub(crate) v: u32,
    pub(crate) slot_key: String,
    pub(crate) display_name: String,
    pub(crate) origin_path: Option<String>,
    pub(crate) app_version: String,
    /// Newest first, at most MAX_GENERATIONS.
    pub(crate) generations: Vec<Generation>,
}

// ---- validation -----------------------------------------------------------

pub(crate) fn valid_slot_key(key: &str) -> bool {
    let bytes = key.as_bytes();
    (1..=64).contains(&bytes.len())
        && bytes[0].is_ascii_alphanumeric()
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
}

/// `rec-<unixms>-<revision>-<8 hex>.json`; never a path.
pub(crate) fn valid_generation_file(name: &str) -> bool {
    let Some(rest) = name
        .strip_prefix("rec-")
        .and_then(|r| r.strip_suffix(".json"))
    else {
        return false;
    };
    let parts: Vec<&str> = rest.split('-').collect();
    parts.len() == 3
        && parts[..2]
            .iter()
            .all(|p| (1..=20).contains(&p.len()) && p.bytes().all(|b| b.is_ascii_digit()))
        && parts[2].len() == 8
        && parts[2]
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

pub(crate) fn valid_text(value: &str, max_chars: usize) -> bool {
    value.chars().count() <= max_chars && !value.chars().any(char::is_control)
}

pub(crate) fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

pub(crate) fn mtime_ms(meta: &fs::Metadata) -> Option<u64> {
    meta.modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| d.as_millis() as u64)
}

/// A real directory, never a symlink. Creates it (0700) when missing.
pub(crate) fn ensure_dir(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() => Ok(()),
        Ok(_) => Err(io::Error::other("recovery path is not a plain directory")),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            let mut builder = fs::DirBuilder::new();
            builder.recursive(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            builder.create(path)
        }
        Err(e) => Err(e),
    }
}

pub(crate) fn plain_dir(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|m| m.is_dir())
}

pub(crate) fn plain_file(path: &Path) -> Option<fs::Metadata> {
    fs::symlink_metadata(path).ok().filter(|m| m.is_file())
}

pub(crate) fn sync_dir(path: &Path) -> io::Result<()> {
    File::open(path)?.sync_all()
}

pub(crate) fn short_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..8].to_owned()
}

/// Temp file in `dir`, durable on return. The temp is removed on any failure.
pub(crate) fn write_temp(
    dir: &Path,
    label: &str,
    bytes: &[u8],
    effect: Effect,
) -> io::Result<PathBuf> {
    let tmp = dir.join(format!(".{label}-{}.tmp", uuid::Uuid::new_v4()));
    effect(&format!("{label}-create"), &tmp)?;
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&tmp)?;
    let result = (|| {
        effect(&format!("{label}-write"), &tmp)?;
        file.write_all(bytes)?;
        effect(&format!("{label}-sync"), &tmp)?;
        file.sync_all()
    })();
    match result {
        Ok(()) => Ok(tmp),
        Err(e) => {
            let _ = fs::remove_file(&tmp);
            Err(e)
        }
    }
}

pub(crate) fn read_manifest(slot_dir: &Path) -> Option<Manifest> {
    let path = slot_dir.join(MANIFEST);
    let meta = plain_file(&path)?;
    if meta.len() > MAX_MANIFEST_BYTES {
        return None;
    }
    let mut text = String::new();
    File::open(&path).ok()?.read_to_string(&mut text).ok()?;
    let manifest: Manifest = serde_json::from_str(&text).ok()?;
    let key = slot_dir.file_name()?.to_str()?;
    (manifest.v == 1
        && manifest.slot_key == key
        && (1..=MAX_GENERATIONS).contains(&manifest.generations.len())
        && valid_text(&manifest.display_name, 128)
        && manifest.generations.iter().all(|g| {
            valid_generation_file(&g.file) && g.document_digest.as_deref().is_none_or(valid_digest)
        }))
    .then_some(manifest)
}

pub(crate) fn write_manifest(
    slot_dir: &Path,
    manifest: &Manifest,
    effect: Effect,
) -> io::Result<()> {
    let bytes = serde_json::to_vec(manifest).map_err(io::Error::other)?;
    let tmp = write_temp(slot_dir, "manifest", &bytes, effect)?;
    let renamed =
        effect("manifest-rename", &tmp).and_then(|()| fs::rename(&tmp, slot_dir.join(MANIFEST)));
    if let Err(e) = renamed {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    Ok(())
}

pub(crate) fn unique_corrupt(parent: &Path, base: &str, now: u64) -> PathBuf {
    let mut candidate = parent.join(format!("{base}.corrupt-{now}"));
    let mut n = 1;
    while fs::symlink_metadata(&candidate).is_ok() {
        candidate = parent.join(format!("{base}.corrupt-{now}-{n}"));
        n += 1;
    }
    candidate
}

/// Never deletes: the whole slot directory keeps its bytes under a name the
/// list never returns again.
pub(crate) fn quarantine_slot(root: &Path, slot: &str, now: u64) -> io::Result<()> {
    fs::rename(root.join(slot), unique_corrupt(root, slot, now))?;
    sync_dir(root)
}

pub(crate) fn remove_slot(root: &Path, slot: &str) -> io::Result<bool> {
    let dir = root.join(slot);
    if !plain_dir(&dir) {
        return Ok(false);
    }
    fs::remove_dir_all(&dir)?;
    let _ = sync_dir(root);
    Ok(true)
}

// ---- write ------------------------------------------------------------------

pub(crate) struct Entry<'a> {
    pub(crate) slot_key: &'a str,
    pub(crate) display_name: &'a str,
    pub(crate) origin_path: Option<&'a str>,
    pub(crate) content_revision: u64,
    pub(crate) document_digest: Option<&'a str>,
}

pub(crate) struct Published {
    pub(crate) saved_at_ms: u64,
    pub(crate) cleanup_pending: bool,
}

/// Publishes one generation. Any error leaves the previous manifest, and with it
/// the previous generation, exactly as it was; stray files are pruned later.
pub(crate) fn publish(
    root: &Path,
    entry: &Entry,
    bytes: &[u8],
    now: u64,
    effect: Effect,
) -> io::Result<Published> {
    if !valid_slot_key(entry.slot_key) || bytes.is_empty() || bytes.len() > MAX_FILE_BYTES {
        return Err(io::Error::other("recovery entry refused"));
    }
    ensure_dir(root)?;
    let slot_dir = root.join(entry.slot_key);
    if fs::symlink_metadata(&slot_dir).is_ok() && !plain_dir(&slot_dir) {
        return Err(io::Error::other("recovery slot is not a plain directory"));
    }
    ensure_dir(&slot_dir)?;
    // An unreadable manifest next to data is corruption: keep it, start fresh.
    let previous = match read_manifest(&slot_dir) {
        Some(manifest) => Some(manifest),
        None if plain_file(&slot_dir.join(MANIFEST)).is_some() => {
            quarantine_slot(root, entry.slot_key, now)?;
            ensure_dir(&slot_dir)?;
            None
        }
        None => None,
    };
    let tmp = write_temp(&slot_dir, "data", bytes, effect)?;
    let file = format!("rec-{now}-{}-{}.json", entry.content_revision, short_id());
    let final_path = slot_dir.join(&file);
    let renamed = effect("rename", &final_path)
        .and_then(|()| fs::rename(&tmp, &final_path))
        .and_then(|()| effect("dirsync", &slot_dir))
        .and_then(|()| sync_dir(&slot_dir));
    if let Err(e) = renamed {
        let _ = fs::remove_file(&tmp);
        let _ = fs::remove_file(&final_path);
        return Err(e);
    }
    let mut generations = vec![Generation {
        file: file.clone(),
        saved_at_ms: now,
        content_revision: entry.content_revision,
        byte_count: bytes.len() as u64,
        document_digest: entry.document_digest.map(str::to_owned),
    }];
    if let Some(previous) = &previous {
        generations.extend(previous.generations.iter().cloned());
    }
    generations.truncate(MAX_GENERATIONS);
    let manifest = Manifest {
        v: 1,
        slot_key: entry.slot_key.to_owned(),
        display_name: entry.display_name.to_owned(),
        origin_path: entry.origin_path.map(str::to_owned),
        app_version: env!("CARGO_PKG_VERSION").to_owned(),
        generations,
    };
    if let Err(e) = write_manifest(&slot_dir, &manifest, effect).and_then(|()| {
        effect("manifest-dirsync", &slot_dir)?;
        sync_dir(&slot_dir)
    }) {
        // The manifest rename may or may not have landed; only drop the new file
        // when the on-disk manifest does not reference it.
        if read_manifest(&slot_dir).is_none_or(|m| m.generations.iter().all(|g| g.file != file)) {
            let _ = fs::remove_file(&final_path);
        }
        return Err(e);
    }
    let cleanup_pending = prune_slot(&slot_dir, &manifest, now, effect).is_err();
    Ok(Published {
        saved_at_ms: now,
        cleanup_pending,
    })
}

/// Removes generation files the manifest no longer references and stale temps.
pub(crate) fn prune_slot(
    slot_dir: &Path,
    manifest: &Manifest,
    now: u64,
    effect: Effect,
) -> io::Result<()> {
    effect("prune", slot_dir)?;
    let mut first_error = None;
    for item in fs::read_dir(slot_dir)? {
        let item = item?;
        let name = item.file_name();
        let Some(name) = name.to_str() else { continue };
        let stray_generation =
            valid_generation_file(name) && manifest.generations.iter().all(|g| g.file != name);
        let stale_tmp = name.ends_with(".tmp")
            && fs::symlink_metadata(item.path())
                .ok()
                .and_then(|m| mtime_ms(&m))
                .is_some_and(|t| now.saturating_sub(t) > STALE_TMP_MS);
        if (stray_generation || stale_tmp) && plain_file(&item.path()).is_some() {
            if let Err(e) = fs::remove_file(item.path()) {
                first_error.get_or_insert(e);
            }
        }
    }
    first_error.map_or(Ok(()), Err)
}
