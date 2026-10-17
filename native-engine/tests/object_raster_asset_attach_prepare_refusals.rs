//! Preparation refusals and removal preparation: every refusal is decided before
//! any byte is reserved and leaves the private ledgers at zero; the charged PNG
//! check maps each decoder class to its own value; removal distinguishes an absent
//! id (silent no-op), a non-image record, and an asset that is still in use.
use super::{charged_png_check_in, AssetEdit};
use crate::object_codec::raster_asset_admission::PngCheck;
use crate::object_document::{AssetId, AssetInventory, AssetKind, AssetRecord, AssetSource};
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

struct Row {
    label: String,
    request_id: String,
    name: String,
    media: String,
    bytes: Vec<u8>,
    code: DispatchErrorCode,
    reason: String,
}

fn row(
    label: &str,
    request_id: &str,
    name: &str,
    media: &str,
    bytes: Vec<u8>,
    code: DispatchErrorCode,
    reason: &str,
) -> Row {
    Row {
        label: label.to_string(),
        request_id: request_id.to_string(),
        name: name.to_string(),
        media: media.to_string(),
        bytes,
        code,
        reason: reason.to_string(),
    }
}

fn refusal_table() -> Vec<Row> {
    use DispatchErrorCode::{InvalidRequest, Unavailable};
    let blue = oracle_bytes("blue-2x1");
    let mut big = vec![0u8; 16 * 1024 * 1024 + 1];
    big[..8].copy_from_slice(&blue[..8]);
    let mut rows = vec![
        row(
            "empty",
            "r1",
            "a.png",
            "image/png",
            Vec::new(),
            InvalidRequest,
            "empty",
        ),
        row(
            "oversize",
            "r1",
            "a.png",
            "image/png",
            big,
            InvalidRequest,
            "oversize",
        ),
        row(
            "declared type",
            "r1",
            "a.png",
            "image/gif",
            blue.clone(),
            InvalidRequest,
            "type_mismatch",
        ),
        row(
            "blank name",
            "r1",
            "   ",
            "image/png",
            blue.clone(),
            InvalidRequest,
            "name",
        ),
        row(
            "long name",
            "r1",
            &"n".repeat(256),
            "image/png",
            blue.clone(),
            InvalidRequest,
            "name",
        ),
        row(
            "nul in name",
            "r1",
            "a\u{0}b",
            "image/png",
            blue.clone(),
            InvalidRequest,
            "name",
        ),
        row(
            "blank request id",
            "",
            "a.png",
            "image/png",
            blue,
            InvalidRequest,
            "request_id",
        ),
    ];
    for source in oracle_rows("png") {
        if source["expect"] == "corrupt" {
            let name = source["name"].as_str().unwrap();
            rows.push(row(
                name,
                "r1",
                "a.png",
                "image/png",
                oracle_bytes(name),
                InvalidRequest,
                "corrupt",
            ));
        }
    }
    for source in oracle_rows("foreign") {
        let name = source["name"].as_str().unwrap();
        let expect = source["expect"].as_str().unwrap();
        let code = if expect == "decoder_unavailable" {
            Unavailable
        } else {
            InvalidRequest
        };
        rows.push(row(
            name,
            "r1",
            "a.bin",
            source["media"].as_str().unwrap(),
            oracle_bytes(name),
            code,
            expect,
        ));
    }
    rows
}

#[test]
fn every_refusal_happens_before_any_reservation_and_leaves_every_ledger_at_zero() {
    let snapshot = open_history().acquire_snapshot(0).unwrap();
    let encoded = ObjectSnapshot::encoded_test_budget(1 << 20);
    let cpu = ImageResourceBudget::isolated(8 << 20).unwrap();
    let reservations = Cell::new(0usize);
    let rows = refusal_table();
    assert_eq!(
        rows.len(),
        15,
        "seven fixed rows, three corrupt pngs, five foreign blobs"
    );
    for row in rows {
        let refusal = snapshot
            .prepare_asset_attachment_using(
                &row.request_id,
                &row.name,
                &row.media,
                &row.bytes,
                &cpu,
                |n| {
                    reservations.set(reservations.get() + 1);
                    encoded.reserve(n).map_err(|_| "reserve")
                },
                |p| p.allocate().map_err(|_| "allocate"),
                |p, a| p.bind(a).map_err(|_| "bind"),
            )
            .unwrap_err();
        assert_eq!(
            (refusal.code(), refusal.reason()),
            (row.code, row.reason.as_str()),
            "{}",
            row.label
        );
        assert_eq!(reservations.get(), 0, "{}", row.label);
        assert_eq!(encoded.used(), Ok(0), "{}", row.label);
        assert_eq!(cpu.used(), Ok(0), "{}", row.label);
    }
}

#[test]
fn an_existing_id_with_other_bytes_and_a_full_catalog_are_typed_refusals() {
    let cpu = ImageResourceBudget::isolated(8 << 20).unwrap();
    let encoded = ObjectSnapshot::encoded_test_budget(1 << 20);
    let held = snapshot_with_assets(&[("r1", "a.png", "blue-2x1")]);
    let red = oracle_bytes("red-2x1");
    let refusal =
        prepare_private!(held, encoded, &cpu, "r1", "b.png", "image/png", &red).unwrap_err();
    assert_eq!(
        (refusal.code(), refusal.reason()),
        (DispatchErrorCode::BusyConflict, "id_collision")
    );
    let base = open_history().acquire_snapshot(0).unwrap();
    let records: Vec<_> = (0..1024)
        .map(|i| {
            Arc::new(
                AssetRecord::new(
                    AssetId::new(format!("linked-{i}")).unwrap(),
                    format!("l{i}.png"),
                    AssetKind::Image,
                    AssetSource::linked(format!("file:///l{i}.png"), 4096).unwrap(),
                )
                .unwrap(),
            )
        })
        .collect();
    let inventory = AssetInventory::new(records, Vec::new(), 1024, 0).unwrap();
    let project = base
        .project()
        .unwrap()
        .with_asset_inventory(Some(inventory));
    let full = ObjectSnapshot::from_admitted_project("desktop".to_string(), project).unwrap();
    let refusal =
        prepare_private!(full, encoded, &cpu, "r2", "c.png", "image/png", &red).unwrap_err();
    assert_eq!(
        (refusal.code(), refusal.reason()),
        (DispatchErrorCode::Unavailable, "catalog_full")
    );
    assert_eq!(encoded.used(), Ok(0));
    assert_eq!(cpu.used(), Ok(0));
}

#[test]
fn the_charged_png_check_maps_valid_corrupt_and_resource_classes() {
    let cpu = ImageResourceBudget::isolated(8 << 20).unwrap();
    for source in oracle_rows("png") {
        let name = source["name"].as_str().unwrap();
        let checked = charged_png_check_in(&oracle_bytes(name), &cpu);
        match source["expect"].as_str().unwrap() {
            "new" => assert_eq!(
                checked,
                PngCheck::Valid {
                    width: source["width"].as_u64().unwrap() as u32,
                    height: source["height"].as_u64().unwrap() as u32
                },
                "{name}"
            ),
            "corrupt" => assert_eq!(checked, PngCheck::Corrupt, "{name}"),
            other => panic!("unexpected oracle class {other}"),
        }
        assert_eq!(cpu.used(), Ok(0), "{name}");
    }
    let starved = ImageResourceBudget::isolated(1).unwrap();
    assert_eq!(
        charged_png_check_in(&oracle_bytes("blue-2x1"), &starved),
        PngCheck::Unavailable
    );
    assert_eq!(starved.used(), Ok(0));
}

#[test]
fn removal_prepares_remove_or_unchanged_and_refuses_non_images_and_assets_in_use() {
    let held = snapshot_with_assets(&[("r1", "a.png", "blue-2x1"), ("r2", "b.png", "red-2x1")]);
    let prepared = held.prepare_asset_removal("asset-r1").unwrap();
    assert!(prepared.changed());
    let AssetEdit::Remove(id) = prepared.into_edit() else {
        panic!("a held image prepares to Remove");
    };
    assert_eq!(id.as_str(), "asset-r1");
    let absent = held.prepare_asset_removal("asset-nothing").unwrap();
    assert!(!absent.changed());
    let catalog_less = open_history().acquire_snapshot(0).unwrap();
    assert!(!catalog_less
        .prepare_asset_removal("asset-r1")
        .unwrap()
        .changed());
    let refusal = held.prepare_asset_removal("").unwrap_err();
    assert_eq!(
        (refusal.code(), refusal.reason()),
        (DispatchErrorCode::InvalidRequest, "asset_id")
    );
    // A linked font record is not an image asset.
    let font = Arc::new(
        AssetRecord::new(
            AssetId::new("font".to_string()).unwrap(),
            "f.ttf".to_string(),
            AssetKind::Font,
            AssetSource::linked("file:///f.ttf".to_string(), 4096).unwrap(),
        )
        .unwrap(),
    );
    let inventory = AssetInventory::new(vec![font], Vec::new(), 1024, 0).unwrap();
    let project = catalog_less
        .project()
        .unwrap()
        .with_asset_inventory(Some(inventory));
    let fonts = ObjectSnapshot::from_admitted_project("desktop".to_string(), project).unwrap();
    let refusal = fonts.prepare_asset_removal("font").unwrap_err();
    assert_eq!(
        (refusal.code(), refusal.reason()),
        (DispatchErrorCode::InvalidRequest, "asset_kind")
    );
}

#[test]
fn an_asset_the_reference_carries_is_refused_as_in_use_at_preparation() {
    let mut owner = open_history();
    attach_png(&mut owner, "r1", "a.png", "blue-2x1");
    owner
        .attach_reference_png(
            owner.content_revision(),
            "asset-r1".to_string(),
            "ref.png".to_string(),
            &oracle_bytes("red-2x1"),
        )
        .unwrap();
    let snapshot = owner.acquire_snapshot(owner.content_revision()).unwrap();
    let refusal = snapshot.prepare_asset_removal("asset-r1").unwrap_err();
    assert_eq!(
        (refusal.code(), refusal.reason()),
        (DispatchErrorCode::BusyConflict, "asset_in_use")
    );
}
