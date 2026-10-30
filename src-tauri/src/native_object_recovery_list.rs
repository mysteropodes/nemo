//! Listing, retention, discard and quarantine for the native recovery store.
//!
//! Quarantine moves bytes aside (`<name>.corrupt-<unixms>`) and never deletes at
//! that moment: the file or slot directory is renamed first and the manifest is
//! rewritten only after the move succeeded. Set-aside names are then part of
//! retention: `list_slots` evicts them past the age limit and, with live slots,
//! counts them in the byte budget, shedding the oldest set-aside bytes first.
//! A slot with generation files but no manifest at all (a first-ever write killed
//! between the data rename and the manifest rename) is adopted into a manifest
//! when its newest file parses, instead of being quarantined.
use crate::native_object_bootstrap::recovery::store::{
    corrupt_stamp, mtime_ms, plain_dir, plain_file, quarantine_slot, quarantine_slot_with,
    read_manifest, remove_slot, sync_dir, unique_corrupt, valid_generation_file, valid_slot_key,
    write_manifest, Generation, Limits, Manifest, Mover, CORRUPT_MARK, MANIFEST, MAX_FILE_BYTES,
    MAX_GENERATIONS, SNIFF, STALE_TMP_MS,
};
use serde::Serialize;
use std::{
    fs,
    fs::File,
    io,
    io::Read,
    path::{Path, PathBuf},
};

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ListedGeneration {
    pub(crate) path: String,
    pub(crate) saved_at_ms: u64,
    pub(crate) content_revision: u64,
    pub(crate) byte_count: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) document_digest: Option<String>,
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ListedSlot {
    pub(crate) slot_key: String,
    pub(crate) display_name: String,
    pub(crate) origin_path: Option<String>,
    pub(crate) generations: Vec<ListedGeneration>,
}

#[derive(Debug, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Listing {
    pub(crate) entries: Vec<ListedSlot>,
    pub(crate) quarantined: Vec<String>,
}

// ---- list / retention -------------------------------------------------------

/// A set-aside file or slot directory (`*.corrupt-<unixms>`), judged by the time
/// in its name and sized for the byte budget.
struct SetAside {
    path: PathBuf,
    is_dir: bool,
    at: u64,
    bytes: u64,
}

fn tree_bytes(path: &Path) -> u64 {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return 0;
    };
    if meta.is_file() {
        return meta.len();
    }
    if !meta.is_dir() {
        return 0;
    }
    fs::read_dir(path).map_or(0, |items| {
        items.flatten().map(|i| tree_bytes(&i.path())).sum()
    })
}

fn set_aside_in(dir: &Path, now: u64, out: &mut Vec<SetAside>) {
    let Ok(items) = fs::read_dir(dir) else { return };
    for item in items.flatten() {
        let name = item.file_name();
        let Some(name) = name.to_str().filter(|n| n.contains(CORRUPT_MARK)) else {
            continue;
        };
        let Ok(meta) = fs::symlink_metadata(item.path()) else {
            continue;
        };
        if !(meta.is_dir() || meta.is_file()) {
            continue; // never follow or touch a symlink
        }
        out.push(SetAside {
            path: item.path(),
            is_dir: meta.is_dir(),
            at: corrupt_stamp(name)
                .or_else(|| mtime_ms(&meta))
                .unwrap_or(now),
            bytes: tree_bytes(&item.path()),
        });
    }
}

fn evict_set_aside(item: &SetAside) {
    let _ = if item.is_dir {
        fs::remove_dir_all(&item.path)
    } else {
        fs::remove_file(&item.path)
    };
}

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

/// A slot with no manifest file at all but intact generation files: the first
/// write died between the data rename and the manifest rename. Adopts the
/// newest parsing files into a manifest; the rest are set aside, never deleted.
fn adopt_orphan(slot_dir: &Path, slot: &str, now: u64) -> Option<Manifest> {
    if fs::symlink_metadata(slot_dir.join(MANIFEST)).is_ok() {
        return None;
    }
    let mut files = Vec::new();
    for item in fs::read_dir(slot_dir).ok()?.flatten() {
        let name = item.file_name().into_string().ok()?;
        if !valid_generation_file(&name) {
            continue;
        }
        let meta = plain_file(&item.path())?;
        let parts: Vec<&str> = name.strip_suffix(".json")?.split('-').collect();
        let g = Generation {
            file: name.clone(),
            saved_at_ms: parts[1].parse().ok()?,
            content_revision: parts[2].parse().ok()?,
            byte_count: meta.len(),
            document_digest: None,
        };
        files.push((generation_valid(slot_dir, &g), g));
    }
    files.sort_by(|a, b| {
        (b.1.saved_at_ms, b.1.content_revision).cmp(&(a.1.saved_at_ms, a.1.content_revision))
    });
    let mut generations = Vec::new();
    let mut aside = Vec::new();
    for (ok, g) in files {
        if ok && generations.len() < MAX_GENERATIONS {
            generations.push(g);
        } else {
            aside.push(g.file);
        }
    }
    if generations.is_empty() {
        return None;
    }
    let manifest = Manifest {
        v: 1,
        slot_key: slot.to_owned(),
        display_name: "Recovered project".to_owned(),
        origin_path: None,
        app_version: env!("CARGO_PKG_VERSION").to_owned(),
        generations,
    };
    write_manifest(slot_dir, &manifest, &mut |_, _| Ok(())).ok()?;
    for file in aside {
        let _ = fs::rename(slot_dir.join(&file), unique_corrupt(slot_dir, &file, now));
    }
    Some(manifest)
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
    let Some(mut manifest) =
        read_manifest(&slot_dir).or_else(|| adopt_orphan(&slot_dir, slot, now))
    else {
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
pub(crate) fn list_slots(
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
    // Set-aside bytes (quarantined slots at the root, quarantined generations in
    // kept slots) share the budget: past the age limit they go, then they are the
    // first to be shed, oldest first, before any live slot.
    let mut aside = Vec::new();
    set_aside_in(root, now, &mut aside);
    for c in &kept {
        set_aside_in(&root.join(&c.listed.slot_key), now, &mut aside);
    }
    aside.sort_by_key(|a| a.at);
    aside.retain(|a| {
        let expired = now.saturating_sub(a.at) > limits.age_ms;
        if expired {
            evict_set_aside(a);
        }
        !expired
    });
    total += aside.iter().map(|a| a.bytes).sum::<u64>();
    for a in &aside {
        if total <= limits.bytes {
            break;
        }
        total -= a.bytes;
        evict_set_aside(a);
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

pub(crate) fn discard(root: &Path, slot: &str) -> io::Result<bool> {
    if !valid_slot_key(slot) {
        return Err(io::Error::other("invalid recovery slot"));
    }
    remove_slot(root, slot)
}

/// Explicit user action. Removes one generation from service without deleting it;
/// the last generation takes the whole slot with it.
pub(crate) fn quarantine(
    root: &Path,
    slot: &str,
    file: &str,
    now: u64,
) -> io::Result<(bool, bool)> {
    quarantine_with(root, slot, file, now, &mut |from, to| fs::rename(from, to))
}

/// Order matters: the bytes move first and the manifest is rewritten after. If
/// the move fails nothing else was touched (the file stays referenced). If the
/// manifest write fails after the move, the manifest still names a file that is
/// gone, which `inspect_slot` drops on the next list; the bytes are in the
/// set-aside name either way, so an unreferenced file is never left for
/// `prune_slot` to delete.
pub(crate) fn quarantine_with(
    root: &Path,
    slot: &str,
    file: &str,
    now: u64,
    mover: Mover,
) -> io::Result<(bool, bool)> {
    if !valid_slot_key(slot) || !valid_generation_file(file) {
        return Err(io::Error::other("invalid recovery quarantine request"));
    }
    let slot_dir = root.join(slot);
    if !plain_dir(&slot_dir) {
        return Ok((false, false));
    }
    let Some(mut manifest) = read_manifest(&slot_dir) else {
        quarantine_slot_with(root, slot, now, mover)?;
        return Ok((true, true));
    };
    let Some(position) = manifest.generations.iter().position(|g| g.file == file) else {
        return Ok((false, false));
    };
    if manifest.generations.len() == 1 {
        quarantine_slot_with(root, slot, now, mover)?;
        return Ok((true, true));
    }
    let from = slot_dir.join(file);
    if plain_file(&from).is_some() {
        mover(&from, &unique_corrupt(&slot_dir, file, now))?;
    }
    manifest.generations.remove(position);
    write_manifest(&slot_dir, &manifest, &mut |_, _| Ok(()))?;
    sync_dir(&slot_dir)?;
    Ok((true, false))
}
