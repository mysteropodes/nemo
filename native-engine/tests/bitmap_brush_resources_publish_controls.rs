//! Negative and mutation controls for the brush asset publication (L07-G / G06):
//! every refusal is typed and leaves the store, its id marks and the shared
//! budget exactly as they were; the comparators that back the behavior tests are
//! shown to be discriminating; source drift guards pin the charged decode and
//! the identity rule.

use super::rows::{
    bare_drawing_snapshot, blob_rows, fresh, mismatches, png_row, request, snapshot,
    snapshot_without_inventory, used, Entry,
};
use super::BrushAssetError;
use crate::evaluation::object_scene_style_kernels::bitmap_brush_resources_store::BrushStoreError;
use crate::object_codec::{ImageAssetError, ImageAssetLimits};
use crate::object_document::AssetKind;
use crate::revision::ImageResourceError;
use std::sync::Arc;

#[test]
fn wrong_kind_linked_assets_are_refused_before_any_decode() {
    let link = Entry::Link("../Library/Mixed CASE.bin");
    let snap = snapshot(&[
        ("font", AssetKind::Font, &link),
        ("video", AssetKind::Video, &link),
        ("audio", AssetKind::Audio, &link),
    ]);
    let (mut publisher, budget) = fresh(1 << 20);
    for (asset, kind) in [
        ("font", AssetKind::Font),
        ("video", AssetKind::Video),
        ("audio", AssetKind::Audio),
    ] {
        let error = publisher
            .publish(&snap, request(asset, "b"), None)
            .unwrap_err();
        assert_eq!(error, BrushAssetError::WrongKind(kind), "{asset}");
    }
    assert_eq!((used(&budget), publisher.store().stats().count), (0, 0));
}

#[test]
fn a_linked_image_without_bytes_is_not_embedded() {
    let link = Entry::Link("../Plates/plate.png");
    let snap = snapshot(&[("plate", AssetKind::Image, &link)]);
    let (mut publisher, budget) = fresh(1 << 20);
    let error = publisher
        .publish(&snap, request("plate", "b"), None)
        .unwrap_err();
    assert_eq!(error, BrushAssetError::NotEmbedded);
    assert_eq!((used(&budget), publisher.store().stats().count), (0, 0));
}

#[test]
fn a_dimension_mismatch_against_the_planned_size_is_refused_and_charges_nothing() {
    let (square, strip) = (png_row("tiny-rgba-2x2"), png_row("rgba-9x1"));
    let (a, b) = (Entry::png(&square.png), Entry::png(&strip.png));
    let snap = snapshot(&[
        ("square", AssetKind::Image, &a),
        ("strip", AssetKind::Image, &b),
    ]);
    let (mut publisher, budget) = fresh(1 << 20);
    let mut planned = request("square", "b");
    planned.expected_size = Some((3, 3));
    assert_eq!(
        publisher.publish(&snap, planned, None).unwrap_err(),
        BrushAssetError::DimensionMismatch {
            expected: (3, 3),
            actual: (2, 2)
        }
    );
    // Swapped width and height are a different size, not a match.
    let mut transposed = request("strip", "b");
    transposed.expected_size = Some((1, 9));
    assert_eq!(
        publisher.publish(&snap, transposed, None).unwrap_err(),
        BrushAssetError::DimensionMismatch {
            expected: (1, 9),
            actual: (9, 1)
        }
    );
    assert_eq!((used(&budget), publisher.store().stats().count), (0, 0));
    // The retained-reuse path enforces the plan too and leaves the version alone.
    let first = publisher
        .publish(&snap, request("square", "b"), None)
        .unwrap();
    let mut changed = request("square", "b");
    changed.expected_size = Some((2, 3));
    assert_eq!(
        publisher.publish(&snap, changed, None).unwrap_err(),
        BrushAssetError::DimensionMismatch {
            expected: (2, 3),
            actual: (2, 2)
        }
    );
    assert!(publisher.store().is_current(&first.bitmap));
    assert_eq!((used(&budget), first.bitmap.holders()), (16, 2));
}

#[test]
fn a_second_reserve_for_one_brush_id_is_a_pending_conflict_and_charges_once() {
    let row = png_row("tiny-rgba-2x2");
    let entry = Entry::png(&row.png);
    let snap = snapshot(&[("asset", AssetKind::Image, &entry)]);
    let (mut publisher, budget) = fresh(1 << 20);
    let first = publisher
        .prepare(&snap, request("asset", "b"), None)
        .unwrap();
    assert_eq!(used(&budget), 16);
    let refused = publisher
        .prepare(&snap, request("asset", "b"), None)
        .unwrap_err();
    assert_eq!(
        refused,
        BrushAssetError::Store(BrushStoreError::PendingConflict)
    );
    assert_eq!(
        used(&budget),
        16,
        "the refused attempt released its decode copy"
    );
    // An independent id is unaffected, and dropping the first frees its id again.
    let other = publisher
        .prepare(&snap, request("asset", "c"), None)
        .unwrap();
    assert_eq!(used(&budget), 32);
    drop((first, other));
    assert_eq!(used(&budget), 0);
    assert!(publisher
        .publish(&snap, request("asset", "b"), None)
        .is_ok());
}

#[test]
fn inventory_refusals_are_typed_and_charge_nothing() {
    let row = png_row("px1");
    let entry = Entry::png(&row.png);
    let snap = snapshot(&[("asset", AssetKind::Image, &entry)]);
    let (mut publisher, budget) = fresh(1 << 20);
    let (bare, no_inventory) = (bare_drawing_snapshot(), snapshot_without_inventory());
    let cases = [
        (&bare, "asset", "b", BrushAssetError::BareDrawing),
        (&no_inventory, "asset", "b", BrushAssetError::NoInventory),
        (&snap, "absent", "b", BrushAssetError::MissingAsset),
        (
            &snap,
            "asset",
            "",
            BrushAssetError::Store(BrushStoreError::InvalidId),
        ),
    ];
    for (current, asset, brush, expected) in cases {
        let error = publisher
            .publish(current, request(asset, brush), None)
            .unwrap_err();
        assert_eq!(error, expected, "{asset}/{brush}");
    }
    assert_eq!((used(&budget), publisher.store().stats().count), (0, 0));
}

#[test]
fn oversize_assets_fail_typed_against_the_admission_limits_and_the_budget() {
    let row = png_row("rgba-16x16");
    let entry = Entry::png(&row.png);
    let snap = snapshot(&[("asset", AssetKind::Image, &entry)]);
    let (mut publisher, budget) = fresh(1 << 20);
    let cases = [
        (
            ImageAssetLimits::new(16, 1 << 20, 16 << 20, 512, 512).unwrap(),
            ImageAssetError::EncodedBudget,
        ),
        (
            ImageAssetLimits::new(65536, 64, 16 << 20, 512, 512).unwrap(),
            ImageAssetError::DecodedBudget,
        ),
        (
            ImageAssetLimits::new(65536, 1 << 20, 16 << 20, 8, 8).unwrap(),
            ImageAssetError::DimensionBudget,
        ),
    ];
    for (admission, error) in cases {
        let mut oversize = request("asset", "b");
        oversize.limits = admission;
        assert_eq!(
            publisher.publish(&snap, oversize, None).unwrap_err(),
            BrushAssetError::Decode(error)
        );
        assert_eq!((used(&budget), publisher.store().stats().count), (0, 0));
    }
    // A quota that cannot even hold the decoder workspace refuses typed too.
    let (mut tiny, tiny_budget) = fresh(64);
    assert_eq!(
        tiny.publish(&snap, request("asset", "b"), None)
            .unwrap_err(),
        BrushAssetError::Budget(ImageResourceError::Capacity)
    );
    assert_eq!((used(&tiny_budget), tiny.store().stats().count), (0, 0));
}

#[test]
fn corrupt_pngs_are_malformed_and_unsupported_shapes_unsupported_with_nothing_charged() {
    let (mut publisher, budget) = fresh(1 << 20);
    for (tag, expected) in [
        ('X', ImageAssetError::Malformed),
        ('U', ImageAssetError::Unsupported),
    ] {
        let blobs = blob_rows(tag);
        assert_eq!(blobs.len(), 5, "{tag} rows");
        for (name, png) in blobs {
            let entry = Entry::png(&png);
            let snap = snapshot(&[("asset", AssetKind::Image, &entry)]);
            let error = publisher
                .publish(&snap, request("asset", "b"), None)
                .unwrap_err();
            assert_eq!(error, BrushAssetError::Decode(expected), "{name}");
            assert_eq!(
                (
                    used(&budget),
                    publisher.store().stats().count,
                    publisher.store().contains("b")
                ),
                (0, 0, false),
                "{name}"
            );
        }
    }
}

#[test]
fn a_corrupted_oracle_pixel_and_a_premultiplied_decode_are_detected() {
    let row = png_row("tiny-rgba-2x2");
    let mut bad = row.expected.clone();
    bad[5] ^= 1;
    assert_eq!(mismatches(&row.expected, &bad), vec![5]);
    assert_eq!(
        mismatches(&row.expected[..15], &row.expected),
        vec![usize::MAX]
    );
    // What a canvas round trip would give: colour scaled by alpha. The straight
    // expectation rejects it exactly where a pixel is not opaque.
    let premultiplied: Vec<u8> = row
        .expected
        .chunks(4)
        .flat_map(|p| {
            let a = u32::from(p[3]);
            [
                ((u32::from(p[0]) * a + 127) / 255) as u8,
                ((u32::from(p[1]) * a + 127) / 255) as u8,
                ((u32::from(p[2]) * a + 127) / 255) as u8,
                p[3],
            ]
        })
        .collect();
    let differing = mismatches(&premultiplied, &row.expected);
    assert!(
        differing.contains(&5) && differing.contains(&10),
        "{differing:?}"
    );
    let entry = Entry::png(&row.png);
    let snap = snapshot(&[("asset", AssetKind::Image, &entry)]);
    let (mut publisher, _budget) = fresh(1 << 20);
    let published = publisher
        .publish(&snap, request("asset", "b"), None)
        .unwrap();
    assert_eq!(
        &published.bitmap.rgba8()[8..12],
        &[0u8, 0, 255, 0],
        "a transparent pixel keeps its colour"
    );
    assert_ne!(published.bitmap.rgba8(), premultiplied.as_slice());
}

#[test]
fn the_reuse_test_distinguishes_identity_from_equal_content() {
    let row = png_row("tiny-rgba-2x2");
    let (a, b) = (Entry::png(&row.png), Entry::png(&row.png));
    let (Entry::Png(first), Entry::Png(second)) = (&a, &b) else {
        panic!("embedded entries")
    };
    assert_eq!(first.as_slice(), second.as_slice(), "equal content");
    assert!(
        !Arc::ptr_eq(first, second),
        "distinct allocations: a content rule would reuse, identity must not"
    );
}

#[test]
fn source_drift_guards_pin_the_charged_decode_the_identity_tie_and_the_layer_rules() {
    let src = include_str!("../src/bitmap_brush_resources_publish.rs");
    assert!(
        !src.contains("super::"),
        "R5: production uses absolute crate paths only"
    );
    assert!(
        !src.contains("shared_image_resource_budget"),
        "the quota is the store's own budget, never a second one"
    );
    assert!(src.contains("self.store.budget().clone()"));
    for needle in [
        "charged_png_workspace(&budget",
        "budget.allocate_bytes(n)",
        "budget.reserve(cost)",
        "decode_png_pixels_with_buffers(",
    ] {
        assert_eq!(src.matches(needle).count(), 1, "{needle}");
    }
    assert!(!src.contains("decode_png_pixels("), "no uncharged decode");
    assert_eq!(
        src.matches("Weak::ptr_eq").count(),
        1,
        "identity, by allocation"
    );
    assert!(
        !src.contains("to_vec()"),
        "no byte copy stands in for the allocation identity"
    );
    let charge = src.find("let mut charge = None;").unwrap();
    let decoded = src
        .find("let decoded = decode_png_pixels_with_buffers(")
        .unwrap();
    assert!(
        charge < decoded,
        "the decode charge outlives the decoded pixels"
    );
    let (drop_pixels, drop_charge) = (
        src.find("drop(decoded);").unwrap(),
        src.find("drop(charge);").unwrap(),
    );
    assert!(
        drop_pixels < drop_charge,
        "pixels are destroyed before their quota"
    );
    let store = include_str!("../src/bitmap_brush_resources_store.rs");
    assert!(store.contains("pub(crate) fn budget(&self) -> &ImageResourceBudget"));
}
