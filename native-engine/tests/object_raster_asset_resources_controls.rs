//! L06-E negative / mutation controls. Each flawed copy below drops exactly one
//! rule of the real implementation; the named case functions pass for the real
//! one and must FAIL for the flawed copy, so weakening the real rule is noticed.
use super::oracle::{budget, embedded, png, project, request, retain_in, shared_trio, snapshot};
use super::{resolve, retain_core, AssetPixels, AssetPixelsError, RetainedAsset};
use crate::object_codec::DecodedPngPixels;
use crate::object_document::AssetKind;
use crate::revision::{ImageResourceBudget, ImageResourceCharge, ObjectSnapshot};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

// ---- identity: allocation, not id string and not equal bytes --------------------------------------

type Matcher = fn(&AssetPixels, &ObjectSnapshot, &str) -> bool;

fn real_match(handle: &AssetPixels, snapshot: &ObjectSnapshot, id: &str) -> bool {
    handle.matches_asset(snapshot, id)
}
/// Flaw 1: the handle is tied to the id it happened to be decoded for.
fn decoded_for_id(_: &AssetPixels, _: &ObjectSnapshot, id: &str) -> bool {
    id == "first"
}
/// Flaw 2: equal bytes count as the same asset.
fn equal_content(handle: &AssetPixels, snapshot: &ObjectSnapshot, id: &str) -> bool {
    resolve(snapshot, id).is_ok_and(|encoded| encoded.as_slice() == handle.encoded_png())
}

fn another_id_of_the_same_allocation_matches(matcher: Matcher) -> bool {
    let snapshot = snapshot(shared_trio(&png("rgba-3x1")));
    let handle = retain_in(&budget(), &snapshot, "first", &[]).unwrap();
    matcher(&handle, &snapshot, "second")
}
fn equal_bytes_in_another_allocation_do_not_match(matcher: Matcher) -> bool {
    let snapshot = snapshot(shared_trio(&png("rgba-3x1")));
    let handle = retain_in(&budget(), &snapshot, "first", &[]).unwrap();
    !matcher(&handle, &snapshot, "third")
}

#[test]
fn the_real_matcher_passes_both_named_cases() {
    assert!(another_id_of_the_same_allocation_matches(real_match));
    assert!(equal_bytes_in_another_allocation_do_not_match(real_match));
}
#[test]
fn matching_by_the_decoded_for_id_fails_the_shared_allocation_case() {
    assert!(!another_id_of_the_same_allocation_matches(decoded_for_id));
}
#[test]
fn matching_by_content_fails_the_distinct_allocation_case() {
    assert!(!equal_bytes_in_another_allocation_do_not_match(
        equal_content
    ));
}

// ---- reuse: a held handle is shared only for the exact allocation ---------------------------------

type Reuse = fn(&ObjectSnapshot, &str, &[AssetPixels]) -> bool;

fn real_reuse(snapshot: &ObjectSnapshot, id: &str, held: &[AssetPixels]) -> bool {
    let result = retain_in(&budget(), snapshot, id, held).unwrap();
    held.iter().any(|handle| handle.same_allocation_as(&result))
}
/// Flaw: a held handle with equal bytes is aliased instead of decoding separately.
fn reuse_by_content(snapshot: &ObjectSnapshot, id: &str, held: &[AssetPixels]) -> bool {
    let encoded = resolve(snapshot, id).unwrap();
    held.iter()
        .any(|handle| handle.encoded_png() == encoded.as_slice())
}
fn equal_bytes_are_not_reused_but_the_shared_arc_is(reuse: Reuse) -> bool {
    let snapshot = snapshot(shared_trio(&png("rgba-3x1")));
    let first = retain_in(&budget(), &snapshot, "first", &[]).unwrap();
    let held = [first];
    reuse(&snapshot, "second", &held) && !reuse(&snapshot, "third", &held)
}

#[test]
fn the_real_reuse_aliases_the_shared_arc_and_decodes_equal_bytes_separately() {
    assert!(equal_bytes_are_not_reused_but_the_shared_arc_is(real_reuse));
}
#[test]
fn reusing_by_content_fails_the_equal_bytes_case() {
    assert!(!equal_bytes_are_not_reused_but_the_shared_arc_is(
        reuse_by_content
    ));
}

// ---- the carrier releases pixels BEFORE the charge ------------------------------------------------

/// Flaw: the charge field is declared first, so the quota is reusable while the
/// decoded bytes still exist.
struct ReversedCarrier {
    _charge: ImageResourceCharge,
    _pixels: DecodedPngPixels,
}
/// What `budget.used()` reads at the instant the pixel Vec is released.
fn used_at_pixel_release(reversed: bool) -> usize {
    let snapshot = snapshot(vec![embedded(
        "a",
        "image/png",
        AssetKind::Image,
        Arc::new(png("rgba-3x1")),
    )]);
    let budget = budget();
    let observed = Arc::new(AtomicUsize::new(usize::MAX));
    let (probe_budget, probe_seen) = (budget.clone(), observed.clone());
    let result = retain_core(
        &request(&snapshot, "a", &[]),
        &budget,
        |charge| charge.allocate(),
        |retained| {
            let RetainedAsset {
                mut pixels,
                _charge,
                snapshot: pin,
                encoded,
            } = retained;
            pixels.observe_release(move || {
                probe_seen.store(probe_budget.used().unwrap(), Ordering::SeqCst);
            });
            if reversed {
                drop((
                    ReversedCarrier {
                        _charge,
                        _pixels: pixels,
                    },
                    pin,
                    encoded,
                ));
            } else {
                drop(RetainedAsset {
                    pixels,
                    _charge,
                    snapshot: pin,
                    encoded,
                });
            }
            Err(AssetPixelsError::Cancelled)
        },
    );
    assert_eq!(result.unwrap_err(), AssetPixelsError::Cancelled);
    assert_eq!(budget.used(), Ok(0));
    observed.load(Ordering::SeqCst)
}

#[test]
fn the_real_carrier_still_holds_the_charge_when_the_pixels_are_released() {
    assert_eq!(used_at_pixel_release(false), 12);
}
#[test]
fn a_reversed_carrier_releases_the_charge_first_and_the_probe_sees_zero() {
    assert_eq!(used_at_pixel_release(true), 0);
}

// ---- the pin names the origin snapshot ------------------------------------------------------------

fn pins_origin(handle: &AssetPixels, origin: &ObjectSnapshot) -> bool {
    handle.snapshot().snapshot_id() == origin.snapshot_id()
        && handle.snapshot().content_revision() == origin.content_revision()
}

#[test]
fn a_handle_read_from_the_newer_revision_is_told_apart_from_one_pinned_to_the_origin() {
    let mut owner = crate::history::NativeObjectHistory::from_admitted_project(
        "pins",
        project(vec![embedded(
            "a",
            "image/png",
            AssetKind::Image,
            Arc::new(png("rgba-3x1")),
        )]),
    )
    .unwrap();
    let origin = owner.acquire_snapshot(0).unwrap();
    let budget = budget();
    let pinned = retain_in(&budget, &origin, "a", &[]).unwrap();
    let prepared = owner
        .prepare_reference_edit(crate::commands::ReferenceEdit::Remove)
        .unwrap();
    assert_eq!(owner.commit_reference_edit(prepared), Ok(1));
    let newer = owner.acquire_snapshot(1).unwrap();
    // The mutated pin: the same asset read from the newer snapshot.
    let mutated = retain_in(&budget, &newer, "a", &[]).unwrap();
    assert!(pins_origin(&pinned, &origin));
    assert!(!pins_origin(&mutated, &origin));
    assert_ne!(
        pinned.snapshot().snapshot_id(),
        mutated.snapshot().snapshot_id()
    );
    assert_eq!(mutated.snapshot().content_revision(), 1);
    assert!(
        !pinned.same_allocation_as(&mutated),
        "separate decodes never alias"
    );
}

// ---- a clone shares the charge; a leak keeps it -----------------------------------------------------

type Duplicate = fn(&AssetPixels, &ImageResourceBudget, &ObjectSnapshot) -> AssetPixels;

fn real_duplicate(
    handle: &AssetPixels,
    _: &ImageResourceBudget,
    _: &ObjectSnapshot,
) -> AssetPixels {
    handle.clone()
}
/// Flaw: the "clone" decodes again and charges a second allocation.
fn redecoding_duplicate(
    _: &AssetPixels,
    budget: &ImageResourceBudget,
    snapshot: &ObjectSnapshot,
) -> AssetPixels {
    retain_in(budget, snapshot, "a", &[]).unwrap()
}
fn used_after_duplicating(duplicate: Duplicate) -> usize {
    let snapshot = snapshot(vec![embedded(
        "a",
        "image/png",
        AssetKind::Image,
        Arc::new(png("rgba-3x1")),
    )]);
    let budget = budget();
    let handle = retain_in(&budget, &snapshot, "a", &[]).unwrap();
    let copy = duplicate(&handle, &budget, &snapshot);
    let used = budget.used().unwrap();
    drop((handle, copy));
    used
}

#[test]
fn a_real_clone_adds_no_charge() {
    assert_eq!(used_after_duplicating(real_duplicate), 12);
}
#[test]
fn a_redecoding_clone_doubles_the_charge_and_is_detected() {
    assert_eq!(used_after_duplicating(redecoding_duplicate), 24);
}
#[test]
fn a_leaked_alias_keeps_the_quota_so_the_zero_assertions_are_load_bearing() {
    let snapshot = snapshot(vec![embedded(
        "a",
        "image/png",
        AssetKind::Image,
        Arc::new(png("rgba-3x1")),
    )]);
    let budget = budget();
    let pixels = retain_in(&budget, &snapshot, "a", &[]).unwrap();
    std::mem::forget(pixels.clone());
    drop(pixels);
    assert_eq!(
        budget.used(),
        Ok(12),
        "a forgotten alias still holds the charge"
    );
}
