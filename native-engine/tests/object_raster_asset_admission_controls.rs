//! Negative / mutation controls of the admission kernel (L06-D). Each flawed
//! copy below drops exactly one rule of the real function; the named case
//! functions pass for the real one and must FAIL for the flawed copy, so a
//! change that weakens the real rule cannot go unnoticed.
use super::oracle::{embedded_record, inventory, png_like};
use super::{check_len, find_duplicate, Refusal, MAX_ENCODED_BYTES};
use crate::object_document::{AssetId, AssetInventory};
use std::sync::Arc;

type Found = Option<(AssetId, Arc<Vec<u8>>)>;
type Finder = fn(&AssetInventory, &str, &[u8], &[u8; 32], &dyn Fn(&[u8]) -> [u8; 32]) -> Found;
type LenGate = fn(usize) -> Result<(), Refusal>;

fn constant_digest(_: &[u8]) -> [u8; 32] {
    [7; 32]
}

/// Flaw 1: the digest alone decides (no byte equality).
fn digest_only(
    inventory: &AssetInventory,
    media_type: &str,
    bytes: &[u8],
    digest: &[u8; 32],
    digest_of: &dyn Fn(&[u8]) -> [u8; 32],
) -> Found {
    inventory.records().iter().find_map(|record| {
        let source = record.source();
        let allocation = source.encoded_allocation()?;
        if source.media_type() != Some(media_type)
            || allocation.len() != bytes.len()
            || digest_of(allocation.as_slice()) != *digest
        {
            return None;
        }
        Some((record.id().clone(), Arc::clone(allocation)))
    })
}

/// Flaw 2: the stored media type is ignored.
fn any_media_type(
    inventory: &AssetInventory,
    _media_type: &str,
    bytes: &[u8],
    digest: &[u8; 32],
    digest_of: &dyn Fn(&[u8]) -> [u8; 32],
) -> Found {
    inventory.records().iter().find_map(|record| {
        let allocation = record.source().encoded_allocation()?;
        if allocation.len() != bytes.len()
            || digest_of(allocation.as_slice()) != *digest
            || allocation.as_slice() != bytes
        {
            return None;
        }
        Some((record.id().clone(), Arc::clone(allocation)))
    })
}

/// Flaw 3: a duplicate is reported with a COPY of the bytes, not the held allocation.
fn copies_the_allocation(
    inventory: &AssetInventory,
    media_type: &str,
    bytes: &[u8],
    digest: &[u8; 32],
    digest_of: &dyn Fn(&[u8]) -> [u8; 32],
) -> Found {
    find_duplicate(inventory, media_type, bytes, digest, digest_of)
        .map(|(id, allocation)| (id, Arc::new(allocation.as_slice().to_vec())))
}

fn digest_collision_is_not_a_duplicate(find: Finder) -> bool {
    let held = png_like(1, 64);
    let incoming = png_like(2, 64);
    let inv = inventory(vec![embedded_record("asset-a", "a.png", "image/png", held)]);
    let digest = constant_digest(&incoming);
    find(&inv, "image/png", &incoming, &digest, &constant_digest).is_none()
}

fn another_media_type_is_not_a_duplicate(find: Finder) -> bool {
    let bytes = png_like(1, 64);
    let inv = inventory(vec![embedded_record(
        "asset-a",
        "a.bin",
        "image/x-held",
        bytes.clone(),
    )]);
    let digest = constant_digest(&bytes);
    find(&inv, "image/png", &bytes, &digest, &constant_digest).is_none()
}

fn equal_bytes_share_the_held_allocation(find: Finder) -> bool {
    let bytes = png_like(1, 64);
    let held = embedded_record("asset-a", "a.png", "image/png", bytes.clone());
    let inv = inventory(vec![held.clone()]);
    let digest = constant_digest(&bytes);
    match find(&inv, "image/png", &bytes, &digest, &constant_digest) {
        Some((id, allocation)) => {
            id.as_str() == "asset-a"
                && Arc::ptr_eq(&allocation, held.source().encoded_allocation().unwrap())
        }
        None => false,
    }
}

#[test]
fn the_real_finder_passes_all_three_named_cases() {
    let real: Finder = find_duplicate;
    assert!(digest_collision_is_not_a_duplicate(real));
    assert!(another_media_type_is_not_a_duplicate(real));
    assert!(equal_bytes_share_the_held_allocation(real));
}

#[test]
fn dropping_the_byte_equality_check_fails_the_collision_case() {
    let flawed: Finder = digest_only;
    assert!(!digest_collision_is_not_a_duplicate(flawed));
    assert!(
        equal_bytes_share_the_held_allocation(flawed),
        "flaw is local to the collision case"
    );
}

#[test]
fn ignoring_the_media_type_fails_the_media_type_case() {
    let flawed: Finder = any_media_type;
    assert!(!another_media_type_is_not_a_duplicate(flawed));
    assert!(digest_collision_is_not_a_duplicate(flawed));
}

#[test]
fn copying_the_allocation_fails_the_sharing_case() {
    let flawed: Finder = copies_the_allocation;
    assert!(!equal_bytes_share_the_held_allocation(flawed));
    assert!(digest_collision_is_not_a_duplicate(flawed));
}

/// Flaw 4: no upper bound at all.
fn no_upper_bound(len: usize) -> Result<(), Refusal> {
    if len == 0 {
        Err(Refusal::Empty)
    } else {
        Ok(())
    }
}
/// Flaw 5: off by one, the limit itself is refused.
fn limit_is_refused(len: usize) -> Result<(), Refusal> {
    if len == 0 {
        Err(Refusal::Empty)
    } else if len >= MAX_ENCODED_BYTES {
        Err(Refusal::Oversize)
    } else {
        Ok(())
    }
}
/// Flaw 6: an empty input is let through.
fn empty_is_accepted(len: usize) -> Result<(), Refusal> {
    if len > MAX_ENCODED_BYTES {
        Err(Refusal::Oversize)
    } else {
        Ok(())
    }
}

fn over_the_limit_is_oversize(gate: LenGate) -> bool {
    gate(MAX_ENCODED_BYTES + 1) == Err(Refusal::Oversize)
}
fn the_limit_itself_is_accepted(gate: LenGate) -> bool {
    gate(MAX_ENCODED_BYTES) == Ok(()) && gate(1) == Ok(())
}
fn empty_is_refused(gate: LenGate) -> bool {
    gate(0) == Err(Refusal::Empty)
}

#[test]
fn the_real_size_gate_passes_all_three_named_cases() {
    let real: LenGate = check_len;
    assert!(over_the_limit_is_oversize(real));
    assert!(the_limit_itself_is_accepted(real));
    assert!(empty_is_refused(real));
}

#[test]
fn each_size_gate_flaw_fails_its_named_case() {
    assert!(!over_the_limit_is_oversize(no_upper_bound));
    assert!(!the_limit_itself_is_accepted(limit_is_refused));
    assert!(!empty_is_refused(empty_is_accepted));
    // And only its own case: the flaws do not break the others.
    assert!(the_limit_itself_is_accepted(no_upper_bound));
    assert!(over_the_limit_is_oversize(limit_is_refused));
    assert!(over_the_limit_is_oversize(empty_is_accepted));
}
