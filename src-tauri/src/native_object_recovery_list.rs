//! Listing, retention, discard and quarantine for the native recovery store.
use crate::native_object_bootstrap::recovery::store::{
    mtime_ms, plain_dir, plain_file, quarantine_slot, read_manifest, remove_slot, sync_dir,
    unique_corrupt, valid_generation_file, valid_slot_key, write_manifest, Generation, Limits,
    MANIFEST, MAX_FILE_BYTES, SNIFF, STALE_TMP_MS,
};
use serde::Serialize;
use std::{fs, fs::File, io, io::Read, path::Path};

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
