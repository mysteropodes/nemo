//! Sealed preparation of an asset attach on PRIVATE ledgers: the encoded bytes are
//! reserved, copied and bound once, the decoder scratch is charged to an isolated
//! image budget and returned, and an exact duplicate never reaches the reservation.
use super::{AssetEdit, AssetRefusal};
use crate::object_document::AssetKind;
use crate::request_receipts::DispatchErrorCode;
use crate::revision::{ImageResourceBudget, ObjectSnapshot};
use std::cell::Cell;
use std::sync::Arc;

/// Prepare an attachment against PRIVATE ledgers: `$encoded` is an encoded-retention
/// budget (`ObjectSnapshot::encoded_test_budget`) and `$cpu` an isolated image budget.
macro_rules! prepare_private {
    ($snapshot:expr, $encoded:expr, $cpu:expr, $id:expr, $name:expr, $media:expr, $bytes:expr) => {
        $snapshot.prepare_asset_attachment_using(
            $id,
            $name,
            $media,
            $bytes,
            $cpu,
            |n| $encoded.reserve(n).map_err(|_| "reserve"),
            |p| p.allocate().map_err(|_| "allocate"),
            |p, a| p.bind(a).map_err(|_| "bind"),
        )
    };
}

include!("object_raster_asset_attach_support.rs");

fn cpu_budget() -> ImageResourceBudget {
    ImageResourceBudget::isolated(8 << 20).expect("isolated image budget")
}

#[test]
fn a_new_png_prepares_a_bound_charged_record_and_releases_everything_when_dropped() {
    let snapshot = open_history().acquire_snapshot(0).unwrap();
    let encoded = ObjectSnapshot::encoded_test_budget(1 << 20);
    let cpu = cpu_budget();
    let png = oracle_bytes("blue-2x1");
    let prepared =
        prepare_private!(snapshot, encoded, &cpu, "r1", "blue.png", "image/png", &png).unwrap();
    assert!(prepared.changed());
    assert_eq!(prepared.origin().snapshot_id(), snapshot.snapshot_id());
    assert_eq!(prepared.origin().content_revision(), 0);
    assert_eq!(encoded.used(), Ok(png.len()));
    assert_eq!(cpu.used(), Ok(0));
    let AssetEdit::Add(record) = prepared.into_edit() else {
        panic!("a new asset prepares to Add");
    };
    assert_eq!(record.id().as_str(), "asset-r1");
    assert_eq!(record.name(), "blue.png");
    assert_eq!(record.kind(), AssetKind::Image);
    assert_eq!(record.source().media_type(), Some("image/png"));
    let held = record.source().encoded_allocation().unwrap();
    assert_eq!(held.as_slice(), png.as_slice());
    assert_eq!(held.capacity(), png.len());
    assert_eq!(encoded.used(), Ok(png.len()));
    drop(record);
    assert_eq!(encoded.used(), Ok(0));
    assert_eq!(cpu.used(), Ok(0));
}

#[test]
fn a_prepared_edit_dropped_without_a_commit_releases_the_reserved_bytes() {
    let snapshot = open_history().acquire_snapshot(0).unwrap();
    let encoded = ObjectSnapshot::encoded_test_budget(1 << 20);
    let cpu = cpu_budget();
    let png = oracle_bytes("red-2x1");
    let prepared =
        prepare_private!(snapshot, encoded, &cpu, "r1", "red.png", "image/png", &png).unwrap();
    assert_eq!(encoded.used(), Ok(png.len()));
    drop(prepared);
    assert_eq!(encoded.used(), Ok(0));
}

#[test]
fn an_exact_duplicate_prepares_unchanged_and_never_reaches_the_reservation() {
    let snapshot = snapshot_with_assets(&[("r1", "a.png", "blue-2x1")]);
    let cpu = cpu_budget();
    let calls = Cell::new(0);
    let png = oracle_bytes("blue-2x1");
    let prepared = snapshot
        .prepare_asset_attachment_using(
            "r2",
            "b.png",
            "image/png",
            &png,
            &cpu,
            |_: usize| -> Result<(), &'static str> {
                calls.set(calls.get() + 1);
                Err("never")
            },
            |_: &mut ()| -> Result<Vec<u8>, &'static str> { Err("never") },
            |_: &mut (), _: &Arc<Vec<u8>>| -> Result<(), &'static str> { Err("never") },
        )
        .unwrap();
    assert!(!prepared.changed());
    assert!(matches!(prepared.into_edit(), AssetEdit::Unchanged));
    assert_eq!(calls.get(), 0);
    assert_eq!(cpu.used(), Ok(0));
}

#[test]
fn reservation_allocation_and_binding_failures_release_every_charge() {
    let snapshot = open_history().acquire_snapshot(0).unwrap();
    let cpu = cpu_budget();
    let png = oracle_bytes("blue-2x1");
    let resource = (DispatchErrorCode::Unavailable, "resource_unavailable");
    // The ledger cannot reserve the bytes.
    let tiny = ObjectSnapshot::encoded_test_budget(8);
    let refusal =
        prepare_private!(snapshot, tiny, &cpu, "r1", "a.png", "image/png", &png).unwrap_err();
    assert_eq!((refusal.code(), refusal.reason()), resource);
    assert_eq!(tiny.used(), Ok(0));
    let encoded = ObjectSnapshot::encoded_test_budget(1 << 20);
    // The allocator fails after the reservation.
    let refusal = snapshot
        .prepare_asset_attachment_using(
            "r1",
            "a.png",
            "image/png",
            &png,
            &cpu,
            |n| encoded.reserve(n).map_err(|_| "reserve"),
            |_| Err("allocate"),
            |p, a| p.bind(a).map_err(|_| "bind"),
        )
        .unwrap_err();
    assert_eq!((refusal.code(), refusal.reason()), resource);
    assert_eq!(encoded.used(), Ok(0));
    // The allocator hands back a buffer of the wrong length.
    let wrong = png.len() + 1;
    let refusal = snapshot
        .prepare_asset_attachment_using(
            "r1",
            "a.png",
            "image/png",
            &png,
            &cpu,
            |n| encoded.reserve(n).map_err(|_| "reserve"),
            |_| Ok(vec![0u8; wrong]),
            |p, a| p.bind(a).map_err(|_| "bind"),
        )
        .unwrap_err();
    assert_eq!((refusal.code(), refusal.reason()), resource);
    assert_eq!(encoded.used(), Ok(0));
    // Binding fails after the bytes were copied and the record exists.
    let refusal = snapshot
        .prepare_asset_attachment_using(
            "r1",
            "a.png",
            "image/png",
            &png,
            &cpu,
            |n| encoded.reserve(n).map_err(|_| "reserve"),
            |p| p.allocate().map_err(|_| "allocate"),
            |_, _| Err("bind"),
        )
        .unwrap_err();
    assert_eq!((refusal.code(), refusal.reason()), resource);
    assert_eq!(encoded.used(), Ok(0));
    assert_eq!(cpu.used(), Ok(0));
}

#[test]
fn a_project_less_snapshot_prepares_nothing() {
    let snapshot = open_history().acquire_snapshot(0).unwrap();
    let bare =
        ObjectSnapshot::from_admitted("desktop".to_string(), snapshot.document().clone()).unwrap();
    let png = oracle_bytes("blue-2x1");
    let refusal: AssetRefusal = bare
        .prepare_asset_attachment("r1", "a.png", "image/png", &png)
        .unwrap_err();
    assert_eq!(
        (refusal.code(), refusal.reason()),
        (DispatchErrorCode::Unavailable, "bare_drawing")
    );
    let refusal = bare.prepare_asset_removal("asset-r1").unwrap_err();
    assert_eq!(
        (refusal.code(), refusal.reason()),
        (DispatchErrorCode::Unavailable, "bare_drawing")
    );
}
