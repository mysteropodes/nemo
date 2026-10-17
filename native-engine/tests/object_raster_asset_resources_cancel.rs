//! L06-E cancellation before publication and the contrast with the baseline's
//! silent 1x1 fallback for an undecodable image.
use super::oracle::{
    arc_pixels, budget, limits, one_image, png, retain_in, row, rows, shared_trio, snapshot,
};
use super::{retain_core, AssetPixels, AssetPixelsError, Request};
use crate::object_codec::{ImageAssetError, ImageAssetLimits};
use crate::revision::shared_image_resource_budget;
use std::cell::Cell;

#[test]
fn cancellation_before_publication_returns_every_byte_and_charge() {
    let snapshot = snapshot(shared_trio(&png("rgba-3x1")));
    let budget = budget();
    let run = |fires_at: usize, held: &[AssetPixels], id: &str| {
        let polls = Cell::new(0usize);
        let (allocated, wrapped) = (Cell::new(false), Cell::new(false));
        let cancel = || {
            polls.set(polls.get() + 1);
            polls.get() == fires_at
        };
        let request = Request {
            snapshot: &snapshot,
            asset_id: id,
            limits: limits(),
            held,
            cancel: &cancel,
        };
        let result = retain_core(
            &request,
            &budget,
            |charge| {
                allocated.set(true);
                charge.allocate()
            },
            |retained| {
                wrapped.set(true);
                arc_pixels(retained)
            },
        );
        (result, polls.get(), allocated.get(), wrapped.get())
    };
    // Fires at the first poll: no work at all.
    let (result, polls, allocated, wrapped) = run(1, &[], "first");
    assert_eq!(result.unwrap_err(), AssetPixelsError::Cancelled);
    assert_eq!((polls, allocated, wrapped), (1, false, false));
    assert_eq!(budget.used(), Ok(0));
    // Fires after the decode, before publication: the work happened and was returned.
    let (result, polls, allocated, wrapped) = run(2, &[], "first");
    assert_eq!(result.unwrap_err(), AssetPixelsError::Cancelled);
    assert_eq!((polls, allocated, wrapped), (2, true, false));
    assert_eq!(budget.used(), Ok(0));
    // Never fires: two polls around one decode.
    let (result, polls, allocated, wrapped) = run(0, &[], "first");
    let first = result.unwrap();
    assert_eq!((polls, allocated, wrapped), (2, true, true));
    assert_eq!(budget.used(), Ok(12));
    // A reuse hit is not work: one poll, no allocator, the same allocation.
    let (result, polls, allocated, wrapped) = run(0, std::slice::from_ref(&first), "second");
    assert!(result.unwrap().same_allocation_as(&first));
    assert_eq!((polls, allocated, wrapped), (1, false, false));
    assert_eq!(budget.used(), Ok(12));
}

#[test]
fn a_corrupt_png_is_a_typed_refusal_where_the_baseline_degrades_to_one_by_one() {
    let baseline = rows("baseline-natural-size");
    assert_eq!(baseline.len(), 3);
    let size = |scenario: &str| {
        let found = baseline.iter().find(|r| r["scenario"] == scenario).unwrap();
        (found["w"].as_u64().unwrap(), found["h"].as_u64().unwrap())
    };
    assert_eq!(size("onerror"), (1, 1));
    assert_eq!(size("zero"), (1, 1));
    assert_eq!(size("loaded"), (7, 5));
    for name in [
        "rgba-3x1-idat-crc-flipped",
        "rgba-3x1-truncated",
        "rgba-0x1",
    ] {
        let result = retain_in(&budget(), &one_image(name), "a", &[]);
        assert!(
            matches!(result, Err(AssetPixelsError::Decode(_))),
            "{name} must be refused, not shown as a 1x1 placeholder"
        );
    }
    let valid = retain_in(&budget(), &one_image("rgba-5x1"), "a", &[]).unwrap();
    assert_eq!(
        valid.dimensions(),
        (5, 1),
        "a valid image reports its real size"
    );
    assert_eq!(row("rgba-5x1")["w"], 5);
}

#[test]
fn the_public_cancellable_entry_forwards_cancel_limits_and_the_shared_budget() {
    let snapshot = snapshot(shared_trio(&png("rgba-3x1")));
    let polls = Cell::new(0usize);
    let fire_at = Cell::new(0usize);
    let cancel = || {
        polls.set(polls.get() + 1);
        polls.get() == fire_at.get()
    };
    let run = |id: &str, bounds: ImageAssetLimits, held: &[AssetPixels]| {
        polls.set(0);
        AssetPixels::retain_cancellable(&snapshot, id, bounds, held, &cancel)
    };
    // A cancel that is already true: refused at the first poll, before any work.
    fire_at.set(1);
    assert_eq!(
        run("first", limits(), &[]).unwrap_err(),
        AssetPixelsError::Cancelled
    );
    assert_eq!(polls.get(), 1);
    // A cancel that fires on the second poll: the fresh decode is returned, not published.
    fire_at.set(2);
    assert_eq!(
        run("first", limits(), &[]).unwrap_err(),
        AssetPixelsError::Cancelled
    );
    assert_eq!(polls.get(), 2);
    // Never fires: two polls around one decode, and the SHARED budget carries the pixels
    // (a lower bound only: other tests charge the same domain concurrently).
    fire_at.set(0);
    let first = run("first", limits(), &[]).unwrap();
    assert_eq!(polls.get(), 2);
    assert_eq!(first.dimensions(), (3, 1));
    assert!(shared_image_resource_budget().used().unwrap() >= first.rgba8().len());
    // A reuse hit on the public path polls once and aliases the held handle.
    let alias = run("second", limits(), std::slice::from_ref(&first)).unwrap();
    assert_eq!(polls.get(), 1);
    assert!(alias.same_allocation_as(&first));
    // The limits argument reaches the decoder on the public path.
    let tight = ImageAssetLimits::new(1 << 20, 1 << 20, 4 << 20, 2, 2).unwrap();
    assert_eq!(
        run("first", tight, &[]).unwrap_err(),
        AssetPixelsError::Decode(ImageAssetError::DimensionBudget)
    );
    assert_eq!(
        AssetPixels::retain(&snapshot, "first", tight).unwrap_err(),
        AssetPixelsError::Decode(ImageAssetError::DimensionBudget)
    );
}
