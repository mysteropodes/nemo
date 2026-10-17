//! L06-E positive path: literal decoded pixels, pins, sharing, lifetime across
//! revisions, the release order of the carrier and the public shared-budget
//! entries. Refusals live in the sibling `refusals` module.
use super::oracle::{
    budget, embedded, expected_rgba, limits, png, request, retain_in, row, rows, shared_trio,
    snapshot, unhex,
};
use super::{retain_core, AssetPixels, AssetPixelsError};
use crate::object_codec::PngWorkspace;
use crate::object_document::AssetKind;
use std::cell::Cell;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

#[test]
fn every_oracle_png_decodes_to_its_authored_straight_rgba_and_returns_its_charge() {
    let ok_rows: Vec<_> = rows("png")
        .into_iter()
        .filter(|row| row["expect"] == "ok")
        .collect();
    assert_eq!(ok_rows.len(), 5);
    for row in ok_rows {
        let name = row["name"].as_str().unwrap();
        let bytes = unhex(row["hex"].as_str().unwrap());
        let (width, height) = (
            row["w"].as_u64().unwrap() as u32,
            row["h"].as_u64().unwrap() as u32,
        );
        let expected = expected_rgba(&row);
        let snapshot = snapshot(vec![embedded(
            "a",
            "image/png",
            AssetKind::Image,
            Arc::new(bytes.clone()),
        )]);
        let budget = budget();
        let pixels = retain_in(&budget, &snapshot, "a", &[]).unwrap();
        assert_eq!(pixels.dimensions(), (width, height), "{name}");
        assert_eq!(pixels.row_stride(), width as usize * 4, "{name}");
        assert_eq!(pixels.rgba8(), expected.as_slice(), "{name}");
        assert_eq!(pixels.encoded_png(), bytes.as_slice(), "{name}");
        // The exact RGBA allocation is the charged one: no spare capacity, no row copy.
        assert_eq!(pixels.0.pixels.capacity(), expected.len(), "{name}");
        assert_eq!(budget.used(), Ok(expected.len()), "{name}");
        drop(pixels);
        assert_eq!(budget.used(), Ok(0), "{name}");
    }
}

#[test]
fn straight_alpha_is_kept_not_premultiplied() {
    let snapshot = snapshot(vec![embedded(
        "a",
        "image/png",
        AssetKind::Image,
        Arc::new(png("rgba-3x1")),
    )]);
    let pixels = retain_in(&budget(), &snapshot, "a", &[]).unwrap();
    // alpha 128 and alpha 0 pixels keep their colour channels untouched
    assert_eq!(
        pixels.rgba8(),
        [200, 10, 20, 255, 1, 2, 3, 128, 250, 251, 252, 0]
    );
}

#[test]
fn the_handle_pins_snapshot_encoded_allocation_and_pixels_until_the_last_clone_drops() {
    let allocation = Arc::new(png("rgba-3x1"));
    let weak = Arc::downgrade(&allocation);
    let snapshot = snapshot(vec![embedded(
        "a",
        "image/png",
        AssetKind::Image,
        allocation,
    )]);
    let budget = budget();
    let pixels = retain_in(&budget, &snapshot, "a", &[]).unwrap();
    assert_eq!(pixels.snapshot().snapshot_id(), snapshot.snapshot_id());
    assert!(std::ptr::eq(
        pixels.snapshot().project().unwrap(),
        snapshot.project().unwrap()
    ));
    assert!(Arc::ptr_eq(&pixels.0.encoded, &weak.upgrade().unwrap()));
    let alias = pixels.clone();
    drop(snapshot);
    drop(pixels);
    // The alias alone keeps the encoded bytes, the snapshot and the charge alive.
    assert!(weak.upgrade().is_some());
    assert_eq!(budget.used(), Ok(12));
    assert_eq!(alias.rgba8().len(), 12);
    drop(alias);
    assert!(weak.upgrade().is_none());
    assert_eq!(budget.used(), Ok(0));
}

#[test]
fn clones_share_one_allocation_and_one_charge() {
    let snapshot = snapshot(vec![embedded(
        "a",
        "image/png",
        AssetKind::Image,
        Arc::new(png("rgba-3x1")),
    )]);
    let budget = budget();
    let pixels = retain_in(&budget, &snapshot, "a", &[]).unwrap();
    let alias = pixels.clone();
    assert!(pixels.same_allocation_as(&alias));
    assert!(std::ptr::eq(
        pixels.rgba8().as_ptr(),
        alias.rgba8().as_ptr()
    ));
    assert_eq!(budget.used(), Ok(12));
    drop(pixels);
    assert_eq!(budget.used(), Ok(12));
    drop(alias);
    assert_eq!(budget.used(), Ok(0));
}

#[test]
fn records_sharing_one_encoded_allocation_share_one_decode_and_equal_bytes_decode_separately() {
    let snapshot = snapshot(shared_trio(&png("rgba-3x1")));
    let budget = budget();
    let first = retain_in(&budget, &snapshot, "first", &[]).unwrap();
    let second = retain_in(&budget, &snapshot, "second", std::slice::from_ref(&first)).unwrap();
    assert!(first.same_allocation_as(&second));
    assert_eq!(budget.used(), Ok(12), "one decode, one charge");
    let held = [first.clone(), second.clone()];
    let third = retain_in(&budget, &snapshot, "third", &held).unwrap();
    assert!(!first.same_allocation_as(&third));
    assert_eq!(third.rgba8(), first.rgba8(), "equal bytes, equal pixels");
    assert_ne!(third.rgba8().as_ptr(), first.rgba8().as_ptr());
    assert_eq!(budget.used(), Ok(24));
    // Reuse is explicit: without held handles the shared record decodes again.
    let fresh = retain_in(&budget, &snapshot, "second", &[]).unwrap();
    assert!(!fresh.same_allocation_as(&first));
    assert_eq!(budget.used(), Ok(36));
    drop(held);
    drop(first);
    assert_eq!(
        budget.used(),
        Ok(36),
        "the alias still holds the shared charge"
    );
    drop(second);
    assert_eq!(budget.used(), Ok(24));
    drop(third);
    assert_eq!(budget.used(), Ok(12));
    drop(fresh);
    assert_eq!(budget.used(), Ok(0));
}

#[test]
fn matching_is_identity_of_the_encoded_allocation() {
    let snapshot = snapshot(shared_trio(&png("rgba-3x1")));
    let first = retain_in(&budget(), &snapshot, "first", &[]).unwrap();
    assert!(first.matches_asset(&snapshot, "first"));
    assert!(
        first.matches_asset(&snapshot, "second"),
        "same Arc, other id"
    );
    assert!(
        !first.matches_asset(&snapshot, "third"),
        "equal bytes, other Arc"
    );
    assert!(!first.matches_asset(&snapshot, "absent"));
    assert!(!first.matches_asset(&snapshot, ""));
}

#[test]
fn the_handle_outlives_newer_revisions_undo_redo_project_replacement_and_the_owner() {
    let allocation = Arc::new(png("rgb-2x2"));
    let weak = Arc::downgrade(&allocation);
    let mut owner = crate::history::NativeObjectHistory::from_admitted_project(
        "pins",
        super::oracle::project(vec![embedded(
            "a",
            "image/png",
            AssetKind::Image,
            allocation,
        )]),
    )
    .unwrap();
    let original = owner.acquire_snapshot(0).unwrap();
    let budget = budget();
    let pixels = retain_in(&budget, &original, "a", &[]).unwrap();
    let before = pixels.rgba8().to_vec();
    let prepared = owner
        .prepare_reference_edit(crate::commands::ReferenceEdit::Remove)
        .unwrap();
    assert_eq!(owner.commit_reference_edit(prepared), Ok(1));
    let newer = owner.acquire_snapshot(1).unwrap();
    assert_eq!(newer.content_revision(), 1);
    assert!(
        pixels.matches_asset(&newer, "a"),
        "the allocation is the same"
    );
    assert_eq!(owner.undo(1), Ok(2));
    assert_eq!(owner.redo(2), Ok(3));
    // The owner replaces the whole project: the old readers, and this handle, stay valid.
    let current = owner.acquire_snapshot(owner.content_revision()).unwrap();
    let replacement = owner
        .prepare_blank_project_replacement(current.snapshot_id(), 320, 180, 24.0)
        .unwrap();
    owner.commit_project_replacement(replacement).unwrap();
    let replaced = owner.acquire_snapshot(owner.content_revision()).unwrap();
    assert_ne!(replaced.document_id(), original.document_id());
    assert!(
        !pixels.matches_asset(&replaced, "a"),
        "the blank project has no such asset"
    );
    drop((newer, current, replaced, owner, original));
    assert_eq!(pixels.snapshot().content_revision(), 0);
    assert_eq!(pixels.rgba8(), before.as_slice());
    assert_eq!(before.len(), 16);
    assert!(
        weak.upgrade().is_some(),
        "the old pin keeps the bytes alive"
    );
    assert_eq!(budget.used(), Ok(16));
    drop(pixels);
    assert!(weak.upgrade().is_none());
    assert_eq!(budget.used(), Ok(0));
}

#[test]
fn a_decode_owns_and_frees_one_charged_workspace_and_retains_only_the_pixels() {
    let snapshot = snapshot(vec![embedded(
        "a",
        "image/png",
        AssetKind::Image,
        Arc::new(png("rgba-3x1")),
    )]);
    let budget = budget();
    let seen = Cell::new(None);
    let (pixels, calls) = crate::reference_image_asset::measure_allocations(
        Some(std::alloc::Layout::new::<PngWorkspace>()),
        || {
            retain_core(
                &request(&snapshot, "a", &[]),
                &budget,
                |charge| charge.allocate(),
                |retained| {
                    seen.set(Some(budget.used()));
                    Ok(AssetPixels(Arc::new(retained)))
                },
            )
            .unwrap()
        },
    );
    assert_eq!(
        calls,
        [1, 1, 0],
        "exactly one workspace, allocated and freed"
    );
    assert_eq!(
        seen.get(),
        Some(Ok(12)),
        "workspace and row buffer are already returned"
    );
    assert_eq!(budget.used(), Ok(12));
    assert_eq!(pixels.rgba8().len(), 12);
}

#[test]
fn pixel_storage_drops_before_the_charge_on_success_failure_and_unwind() {
    let snapshot = snapshot(vec![embedded(
        "a",
        "image/png",
        AssetKind::Image,
        Arc::new(png("rgba-3x1")),
    )]);
    let budget = budget();
    let dropped = Arc::new(AtomicUsize::new(0));
    for stage in 0..3 {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            retain_core(
                &request(&snapshot, "a", &[]),
                &budget,
                |charge| charge.allocate(),
                |mut retained| {
                    let (budget, dropped) = (budget.clone(), dropped.clone());
                    retained.pixels.observe_release(move || {
                        // Runs after the real Vec field is destroyed, before the charge field.
                        assert_eq!(budget.used(), Ok(12));
                        dropped.fetch_add(1, Ordering::SeqCst);
                    });
                    match stage {
                        0 => Ok(AssetPixels(Arc::new(retained))),
                        1 => Err(AssetPixelsError::Cancelled),
                        _ => panic!("carrier construction unwind"),
                    }
                },
            )
        }));
        if stage == 0 {
            let pixels = result.unwrap().unwrap();
            let alias = pixels.clone();
            drop(pixels);
            assert_eq!(dropped.load(Ordering::SeqCst), 0);
            drop(alias);
        } else if stage == 1 {
            assert_eq!(result.unwrap().unwrap_err(), AssetPixelsError::Cancelled);
        } else {
            assert!(result.is_err());
        }
        assert_eq!(dropped.load(Ordering::SeqCst), stage + 1);
        assert_eq!(budget.used(), Ok(0));
    }
}

#[test]
fn the_public_entries_decode_under_the_shared_budget_and_reuse_by_identity() {
    let snapshot = snapshot(shared_trio(&png("rgba-4x4-filters")));
    let expected = expected_rgba(&row("rgba-4x4-filters"));
    let first = AssetPixels::retain(&snapshot, "first", limits()).unwrap();
    assert_eq!(first.dimensions(), (4, 4));
    assert_eq!(first.rgba8(), expected.as_slice());
    let second =
        AssetPixels::retain_reusing(&snapshot, "second", limits(), std::slice::from_ref(&first))
            .unwrap();
    assert!(first.same_allocation_as(&second));
    let third =
        AssetPixels::retain_reusing(&snapshot, "third", limits(), std::slice::from_ref(&first))
            .unwrap();
    assert!(!first.same_allocation_as(&third));
    assert_eq!(third.rgba8(), expected.as_slice());
    assert!(first.matches_asset(&snapshot, "second"));
    // The shared domain's `used()` is deliberately not asserted: other tests charge it too.
}
