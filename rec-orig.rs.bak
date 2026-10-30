//! Private crash-recovery store for native object projects (#2360, R1).
//!
//! Writes the exact bytes of the native Save As serializer under
//! `<appData>/recovery/<slotKey>/`, never a UI-supplied path and never through a
//! JS string wrapper. A write is: create_new temp (0600) -> write_all -> sync_all
//! -> unique rename -> directory fsync -> manifest temp -> rename -> directory
//! fsync -> prune. The previous generation and manifest are untouched until the
//! manifest rename, so a kill anywhere leaves the last durable generation.
use crate::application_mcp::ApplicationMcp;
use crate::native_application_contract::{
    bounded_id, host_error, require_api_instance, require_main, HostResult,
};
use crate::native_object_bootstrap::reference::RecoverySnapshot;
use nemo_mcp::contract::NativeApplicationError;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    fs::File,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::Manager;

const MAX_REQUEST_BYTES: usize = 4096;
const MAX_FILE_BYTES: usize = 32 * 1024 * 1024;
const MAX_MANIFEST_BYTES: u64 = 64 * 1024;
const MAX_GENERATIONS: usize = 2;
const STALE_TMP_MS: u64 = 10 * 60 * 1000;
const MANIFEST: &str = "manifest.json";
const SNIFF: &str = "nemo.native-project-document";

/// Retention budget; tests shrink it, production always uses `DEFAULT`.
#[derive(Clone, Copy)]
pub(super) struct Limits {
    pub(super) slots: usize,
    pub(super) bytes: u64,
    pub(super) age_ms: u64,
}

impl Limits {
    pub(super) const DEFAULT: Limits = Limits {
        slots: 5,
        bytes: 512 * 1024 * 1024,
        age_ms: 14 * 24 * 60 * 60 * 1000,
    };
}

pub(super) type Effect<'a> = &'a mut dyn FnMut(&str, &Path) -> io::Result<()>;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Envelope {
    api_version: u32,
    request_id: String,
    instance_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WriteRequest {
    api_version: u32,
    request_id: String,
    instance_id: String,
    document_id: String,
    at_revision: u64,
    document_snapshot_id: String,
    slot_key: String,
    display_name: String,
    #[serde(default)]
    origin_path: Option<String>,
    #[serde(default)]
    supersedes: Option<String>,
    #[serde(default)]
    document_digest: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SlotRequest {
    api_version: u32,
    request_id: String,
    instance_id: String,
    slot_key: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct QuarantineRequest {
    api_version: u32,
    request_id: String,
    instance_id: String,
    slot_key: String,
    file: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RecoveryWriteReceipt {
    api_version: u32,
    request_id: String,
    instance_id: String,
    document_id: String,
    at_revision: u64,
    document_snapshot_id: String,
    slot_key: String,
    published: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<&'static str>,
    byte_count: usize,
    saved_at_ms: u64,
    cleanup_pending: bool,
    superseded_removed: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Generation {
    pub(super) file: String,
    pub(super) saved_at_ms: u64,
    pub(super) content_revision: u64,
    pub(super) byte_count: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) document_digest: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Manifest {
    pub(super) v: u32,
    pub(super) slot_key: String,
    pub(super) display_name: String,
    pub(super) origin_path: Option<String>,
    pub(super) app_version: String,
    /// Newest first, at most MAX_GENERATIONS.
    pub(super) generations: Vec<Generation>,
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(super) struct ListedGeneration {
    pub(super) path: String,
    pub(super) saved_at_ms: u64,
    pub(super) content_revision: u64,
    pub(super) byte_count: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) document_digest: Option<String>,
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(super) struct ListedSlot {
    pub(super) slot_key: String,
    pub(super) display_name: String,
    pub(super) origin_path: Option<String>,
    pub(super) generations: Vec<ListedGeneration>,
}

#[derive(Debug, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(super) struct Listing {
    pub(super) entries: Vec<ListedSlot>,
    pub(super) quarantined: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RecoveryListReceipt {
    api_version: u32,
    request_id: String,
    instance_id: String,
    #[serde(flatten)]
    listing: Listing,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RecoveryDiscardReceipt {
    api_version: u32,
    request_id: String,
    instance_id: String,
    slot_key: String,
    discarded: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RecoveryQuarantineReceipt {
    api_version: u32,
    request_id: String,
    instance_id: String,
    slot_key: String,
    quarantined: bool,
    slot_quarantined: bool,
}

// ---- validation -----------------------------------------------------------

pub(super) fn valid_slot_key(key: &str) -> bool {
    let bytes = key.as_bytes();
    (1..=64).contains(&bytes.len())
        && bytes[0].is_ascii_alphanumeric()
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
}

/// `rec-<unixms>-<revision>-<8 hex>.json`; never a path.
pub(super) fn valid_generation_file(name: &str) -> bool {
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

fn valid_text(value: &str, max_chars: usize) -> bool {
    value.chars().count() <= max_chars && !value.chars().any(char::is_control)
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn invalid() -> NativeApplicationError {
    host_error("invalid_request", "invalid native recovery request")
}

fn parse<T: for<'de> Deserialize<'de>>(raw: &str) -> HostResult<T> {
    if raw.len() > MAX_REQUEST_BYTES || !raw.trim_start().starts_with('{') {
        return Err(invalid());
    }
    serde_json::from_str(raw).map_err(|_| invalid())
}

fn check_envelope(
    state: &ApplicationMcp,
    version: u32,
    request_id: &str,
    instance_id: &str,
) -> HostResult<()> {
    require_api_instance(version, instance_id, state.instance_id())?;
    if !bounded_id(request_id) {
        return Err(invalid());
    }
    Ok(())
}

// ---- filesystem helpers -----------------------------------------------------

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

fn mtime_ms(meta: &fs::Metadata) -> Option<u64> {
    meta.modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| d.as_millis() as u64)
}

/// A real directory, never a symlink. Creates it (0700) when missing.
fn ensure_dir(path: &Path) -> io::Result<()> {
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

fn plain_dir(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|m| m.is_dir())
}

fn plain_file(path: &Path) -> Option<fs::Metadata> {
    fs::symlink_metadata(path).ok().filter(|m| m.is_file())
}

fn sync_dir(path: &Path) -> io::Result<()> {
    File::open(path)?.sync_all()
}

fn short_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..8].to_owned()
}

/// Temp file in `dir`, durable on return. The temp is removed on any failure.
fn write_temp(dir: &Path, label: &str, bytes: &[u8], effect: Effect) -> io::Result<PathBuf> {
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

fn read_manifest(slot_dir: &Path) -> Option<Manifest> {
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

fn write_manifest(slot_dir: &Path, manifest: &Manifest, effect: Effect) -> io::Result<()> {
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

fn unique_corrupt(parent: &Path, base: &str, now: u64) -> PathBuf {
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
fn quarantine_slot(root: &Path, slot: &str, now: u64) -> io::Result<()> {
    fs::rename(root.join(slot), unique_corrupt(root, slot, now))?;
    sync_dir(root)
}

fn remove_slot(root: &Path, slot: &str) -> io::Result<bool> {
    let dir = root.join(slot);
    if !plain_dir(&dir) {
        return Ok(false);
    }
    fs::remove_dir_all(&dir)?;
    let _ = sync_dir(root);
    Ok(true)
}

// ---- write ------------------------------------------------------------------

pub(super) struct Entry<'a> {
    pub(super) slot_key: &'a str,
    pub(super) display_name: &'a str,
    pub(super) origin_path: Option<&'a str>,
    pub(super) content_revision: u64,
    pub(super) document_digest: Option<&'a str>,
}

pub(super) struct Published {
    pub(super) saved_at_ms: u64,
    pub(super) cleanup_pending: bool,
}

/// Publishes one generation. Any error leaves the previous manifest, and with it
/// the previous generation, exactly as it was; stray files are pruned later.
pub(super) fn publish(
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
fn prune_slot(slot_dir: &Path, manifest: &Manifest, now: u64, effect: Effect) -> io::Result<()> {
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

// ---- list / retention -------------------------------------------------------

struct Candidate {
    listed: ListedSlot,
    newest: u64,
    bytes: u64,
}

fn sniff(path: &Path) -> bool {
    let mut head = [0u8; 256];
    let Ok(mut file) = File::open(path) else {
        return false;
    };
    let read = file.read(&mut head).unwrap_or(0);
    String::from_utf8_lossy(&head[..read]).contains(SNIFF)
}

fn generation_valid(slot_dir: &Path, g: &Generation) -> bool {
    let path = slot_dir.join(&g.file);
    plain_file(&path).is_some_and(|m| m.len() == g.byte_count && g.byte_count > 0)
        && g.byte_count <= MAX_FILE_BYTES as u64
        && sniff(&path)
}

fn clean_slot_tmp(slot_dir: &Path, now: u64) {
    let Ok(items) = fs::read_dir(slot_dir) else {
        return;
    };
    for item in items.flatten() {
        let name = item.file_name();
        if name.to_str().is_some_and(|n| n.ends_with(".tmp"))
            && fs::symlink_metadata(item.path())
                .ok()
                .filter(|m| m.is_file())
                .and_then(|m| mtime_ms(&m))
                .is_some_and(|t| now.saturating_sub(t) > STALE_TMP_MS)
        {
            let _ = fs::remove_file(item.path());
        }
    }
}

fn has_generation_files(slot_dir: &Path) -> bool {
    fs::read_dir(slot_dir).is_ok_and(|items| {
        items
            .flatten()
            .any(|i| i.file_name().to_str().is_some_and(valid_generation_file))
    })
}

fn inspect_slot(
    root: &Path,
    slot: &str,
    now: u64,
    limits: &Limits,
    quarantined: &mut Vec<String>,
) -> Option<Candidate> {
    let slot_dir = root.join(slot);
    clean_slot_tmp(&slot_dir, now);
    let Some(mut manifest) = read_manifest(&slot_dir) else {
        if plain_file(&slot_dir.join(MANIFEST)).is_some() || has_generation_files(&slot_dir) {
            if quarantine_slot(root, slot, now).is_ok() {
                quarantined.push(slot.to_owned());
            }
        } else {
            let _ = remove_slot(root, slot); // only temps/empty: nothing to keep
        }
        return None;
    };
    let before = manifest.generations.len();
    let mut bad = Vec::new();
    manifest.generations.retain(|g| {
        let ok = generation_valid(&slot_dir, g);
        if !ok {
            bad.push(g.file.clone());
        }
        ok
    });
    if manifest.generations.is_empty() {
        if quarantine_slot(root, slot, now).is_ok() {
            quarantined.push(slot.to_owned());
        }
        return None;
    }
    if manifest.generations.len() != before {
        for file in &bad {
            let from = slot_dir.join(file);
            if plain_file(&from).is_some() {
                let _ = fs::rename(&from, unique_corrupt(&slot_dir, file, now));
            }
        }
        let _ = write_manifest(&slot_dir, &manifest, &mut |_, _| Ok(()));
    }
    let newest = manifest.generations.iter().map(|g| g.saved_at_ms).max()?;
    if now.saturating_sub(newest) > limits.age_ms {
        let _ = remove_slot(root, slot);
        return None;
    }
    let saved_since = manifest.origin_path.as_ref().is_some_and(|origin| {
        fs::metadata(origin)
            .ok()
            .and_then(|m| mtime_ms(&m))
            .is_some_and(|t| t >= newest)
    });
    if saved_since {
        let _ = remove_slot(root, slot); // already saved after this recovery
        return None;
    }
    let bytes = manifest.generations.iter().map(|g| g.byte_count).sum();
    Some(Candidate {
        listed: ListedSlot {
            slot_key: manifest.slot_key,
            display_name: manifest.display_name,
            origin_path: manifest.origin_path,
            generations: manifest
                .generations
                .into_iter()
                .map(|g| ListedGeneration {
                    path: slot_dir.join(&g.file).to_string_lossy().into_owned(),
                    saved_at_ms: g.saved_at_ms,
                    content_revision: g.content_revision,
                    byte_count: g.byte_count,
                    document_digest: g.document_digest,
                })
                .collect(),
        },
        newest,
        bytes,
    })
}

/// Validates every slot, enforces retention and returns newest first. `protect`
/// is never evicted by the count/bytes rules (the slot just written).
pub(super) fn list_slots(
    root: &Path,
    now: u64,
    protect: Option<&str>,
    limits: &Limits,
) -> io::Result<Listing> {
    let mut listing = Listing::default();
    match fs::symlink_metadata(root) {
        Ok(meta) if meta.is_dir() => {}
        Ok(_) => return Err(io::Error::other("recovery root is not a plain directory")),
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(listing),
        Err(e) => return Err(e),
    }
    let mut found = Vec::new();
    for item in fs::read_dir(root)? {
        let item = item?;
        let name = item.file_name();
        let Some(name) = name.to_str() else { continue };
        if valid_slot_key(name) && plain_dir(&item.path()) {
            if let Some(candidate) = inspect_slot(root, name, now, limits, &mut listing.quarantined)
            {
                found.push(candidate);
            }
        }
    }
    found.sort_by(|a, b| {
        b.newest
            .cmp(&a.newest)
            .then_with(|| a.listed.slot_key.cmp(&b.listed.slot_key))
    });
    // Newest first: keep MAX_SLOTS, then shed oldest until under the byte
    // budget. The protected slot (the one just written) is never shed.
    let mut total: u64 = found.iter().map(|c| c.bytes).sum();
    let mut kept: Vec<Candidate> = Vec::new();
    for candidate in found {
        let protected = protect == Some(candidate.listed.slot_key.as_str());
        if kept.len() >= limits.slots && !protected {
            total -= candidate.bytes;
            let _ = remove_slot(root, &candidate.listed.slot_key);
        } else {
            kept.push(candidate);
        }
    }
    while total > limits.bytes {
        let Some(at) = kept
            .iter()
            .rposition(|c| protect != Some(c.listed.slot_key.as_str()))
        else {
            break;
        };
        let evicted = kept.remove(at);
        total -= evicted.bytes;
        let _ = remove_slot(root, &evicted.listed.slot_key);
    }
    listing.entries = kept.into_iter().map(|c| c.listed).collect();
    Ok(listing)
}

// ---- discard / quarantine ---------------------------------------------------

pub(super) fn discard(root: &Path, slot: &str) -> io::Result<bool> {
    if !valid_slot_key(slot) {
        return Err(io::Error::other("invalid recovery slot"));
    }
    remove_slot(root, slot)
}

/// Explicit user action. Removes one generation from service without deleting it;
/// the last generation takes the whole slot with it.
pub(super) fn quarantine(
    root: &Path,
    slot: &str,
    file: &str,
    now: u64,
) -> io::Result<(bool, bool)> {
    if !valid_slot_key(slot) || !valid_generation_file(file) {
        return Err(io::Error::other("invalid recovery quarantine request"));
    }
    let slot_dir = root.join(slot);
    if !plain_dir(&slot_dir) {
        return Ok((false, false));
    }
    let manifest = read_manifest(&slot_dir);
    let Some(mut manifest) = manifest else {
        quarantine_slot(root, slot, now)?;
        return Ok((true, true));
    };
    let Some(position) = manifest.generations.iter().position(|g| g.file == file) else {
        return Ok((false, false));
    };
    if manifest.generations.len() == 1 {
        quarantine_slot(root, slot, now)?;
        return Ok((true, true));
    }
    manifest.generations.remove(position);
    write_manifest(&slot_dir, &manifest, &mut |_, _| Ok(()))?;
    let from = slot_dir.join(file);
    if plain_file(&from).is_some() {
        fs::rename(&from, unique_corrupt(&slot_dir, file, now))?;
    }
    sync_dir(&slot_dir)?;
    Ok((true, false))
}

// ---- commands ---------------------------------------------------------------

fn root_of(window: &tauri::Window) -> HostResult<PathBuf> {
    window
        .path()
        .app_data_dir()
        .map(|dir| dir.join("recovery"))
        .map_err(|_| host_error("unavailable", "native recovery directory unavailable"))
}

fn unavailable() -> NativeApplicationError {
    host_error("unavailable", "native recovery store unavailable")
}

pub(super) fn write_request(
    state: &ApplicationMcp,
    root: &Path,
    raw: &str,
    now: u64,
    mut effect: impl FnMut(&str, &Path) -> io::Result<()>,
) -> HostResult<RecoveryWriteReceipt> {
    let request: WriteRequest = parse(raw)?;
    check_envelope(
        state,
        request.api_version,
        &request.request_id,
        &request.instance_id,
    )?;
    if !bounded_id(&request.document_id)
        || !bounded_id(&request.document_snapshot_id)
        || request.at_revision > 9_007_199_254_740_991
        || !valid_slot_key(&request.slot_key)
        || request.display_name.is_empty()
        || !valid_text(&request.display_name, 128)
        || request
            .origin_path
            .as_deref()
            .is_some_and(|p| p.len() > 1024 || !Path::new(p).is_absolute() || p.contains('\0'))
        || request
            .supersedes
            .as_deref()
            .is_some_and(|s| !valid_slot_key(s) || s == request.slot_key)
        || request
            .document_digest
            .as_deref()
            .is_some_and(|d| !valid_digest(d))
    {
        return Err(invalid());
    }
    if !cfg!(unix) {
        return Err(host_error("unavailable", "native recovery unsupported"));
    }
    let receipt =
        |published: bool, reason: Option<&'static str>, byte_count, saved, pending, removed| {
            RecoveryWriteReceipt {
                api_version: request.api_version,
                request_id: request.request_id.clone(),
                instance_id: request.instance_id.clone(),
                document_id: request.document_id.clone(),
                at_revision: request.at_revision,
                document_snapshot_id: request.document_snapshot_id.clone(),
                slot_key: request.slot_key.clone(),
                published,
                reason,
                byte_count,
                saved_at_ms: saved,
                cleanup_pending: pending,
                superseded_removed: removed,
            }
        };
    let bytes = match state
        .native_project_recovery_snapshot(
            &request.document_id,
            request.at_revision,
            &request.document_snapshot_id,
        )
        .map_err(|_| host_error("unavailable", "native project snapshot refused"))?
    {
        RecoverySnapshot::Bytes(bytes) => bytes,
        RecoverySnapshot::Stale => return Ok(receipt(false, Some("stale"), 0, 0, false, false)),
        RecoverySnapshot::NotProject => {
            return Ok(receipt(false, Some("unsupported"), 0, 0, false, false))
        }
    };
    if bytes.len() > MAX_FILE_BYTES {
        return Ok(receipt(
            false,
            Some("too-large"),
            bytes.len(),
            0,
            false,
            false,
        ));
    }
    effect("snapshot", root).map_err(|_| unavailable())?;
    let entry = Entry {
        slot_key: &request.slot_key,
        display_name: &request.display_name,
        origin_path: request.origin_path.as_deref(),
        content_revision: request.at_revision,
        document_digest: request.document_digest.as_deref(),
    };
    let published = publish(root, &entry, &bytes, now, &mut effect).map_err(|_| unavailable())?;
    // Only after the new manifest is durable may the superseded slot go.
    let removed = request
        .supersedes
        .as_deref()
        .is_some_and(|slot| remove_slot(root, slot).unwrap_or(false));
    let _ = list_slots(root, now, Some(&request.slot_key), &Limits::DEFAULT);
    Ok(receipt(
        true,
        None,
        bytes.len(),
        published.saved_at_ms,
        published.cleanup_pending,
        removed,
    ))
}

pub(super) fn list_request(
    state: &ApplicationMcp,
    root: &Path,
    raw: &str,
    now: u64,
) -> HostResult<RecoveryListReceipt> {
    let request: Envelope = parse(raw)?;
    check_envelope(
        state,
        request.api_version,
        &request.request_id,
        &request.instance_id,
    )?;
    let listing = list_slots(root, now, None, &Limits::DEFAULT).map_err(|_| unavailable())?;
    Ok(RecoveryListReceipt {
        api_version: request.api_version,
        request_id: request.request_id,
        instance_id: request.instance_id,
        listing,
    })
}

pub(super) fn discard_request(
    state: &ApplicationMcp,
    root: &Path,
    raw: &str,
) -> HostResult<RecoveryDiscardReceipt> {
    let request: SlotRequest = parse(raw)?;
    check_envelope(
        state,
        request.api_version,
        &request.request_id,
        &request.instance_id,
    )?;
    if !valid_slot_key(&request.slot_key) {
        return Err(invalid());
    }
    let discarded = discard(root, &request.slot_key).map_err(|_| unavailable())?;
    Ok(RecoveryDiscardReceipt {
        api_version: request.api_version,
        request_id: request.request_id,
        instance_id: request.instance_id,
        slot_key: request.slot_key,
        discarded,
    })
}

pub(super) fn quarantine_request(
    state: &ApplicationMcp,
    root: &Path,
    raw: &str,
    now: u64,
) -> HostResult<RecoveryQuarantineReceipt> {
    let request: QuarantineRequest = parse(raw)?;
    check_envelope(
        state,
        request.api_version,
        &request.request_id,
        &request.instance_id,
    )?;
    if !valid_slot_key(&request.slot_key) || !valid_generation_file(&request.file) {
        return Err(invalid());
    }
    let (quarantined, slot_quarantined) =
        quarantine(root, &request.slot_key, &request.file, now).map_err(|_| unavailable())?;
    Ok(RecoveryQuarantineReceipt {
        api_version: request.api_version,
        request_id: request.request_id,
        instance_id: request.instance_id,
        slot_key: request.slot_key,
        quarantined,
        slot_quarantined,
    })
}

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> HostResult<T> + Send + 'static,
) -> HostResult<T> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|_| unavailable())?
}

#[tauri::command]
pub(crate) async fn nemo_native_object_recovery_write(
    window: tauri::Window,
    request_json: String,
) -> HostResult<RecoveryWriteReceipt> {
    require_main(&window)?;
    let root = root_of(&window)?;
    let app = window.app_handle().clone();
    blocking(move || {
        let state = app.state::<ApplicationMcp>();
        write_request(&state, &root, &request_json, now_ms(), |_, _| Ok(()))
    })
    .await
}

#[tauri::command]
pub(crate) async fn nemo_native_object_recovery_list(
    window: tauri::Window,
    request_json: String,
) -> HostResult<RecoveryListReceipt> {
    require_main(&window)?;
    let root = root_of(&window)?;
    let app = window.app_handle().clone();
    blocking(move || {
        let state = app.state::<ApplicationMcp>();
        list_request(&state, &root, &request_json, now_ms())
    })
    .await
}

#[tauri::command]
pub(crate) async fn nemo_native_object_recovery_discard(
    window: tauri::Window,
    request_json: String,
) -> HostResult<RecoveryDiscardReceipt> {
    require_main(&window)?;
    let root = root_of(&window)?;
    let app = window.app_handle().clone();
    blocking(move || {
        let state = app.state::<ApplicationMcp>();
        discard_request(&state, &root, &request_json)
    })
    .await
}

#[tauri::command]
pub(crate) async fn nemo_native_object_recovery_quarantine(
    window: tauri::Window,
    request_json: String,
) -> HostResult<RecoveryQuarantineReceipt> {
    require_main(&window)?;
    let root = root_of(&window)?;
    let app = window.app_handle().clone();
    blocking(move || {
        let state = app.state::<ApplicationMcp>();
        quarantine_request(&state, &root, &request_json, now_ms())
    })
    .await
}
