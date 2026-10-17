//! Behavior tests for brush bitmap publication from an admitted PNG asset (L07-G / G06):
//! exact straight RGBA8, baseline accounting, identity-based reuse, replacement,
//! cancel, stale commit and the shared-budget peak. Refusals and mutation
//! controls live in `controls`; the oracle and the snapshot builders in `rows`.

use super::rows::{
    baseline_cost, fresh, mismatches, png_row, png_rows, request, rows, snapshot, used, Entry,
    ORACLE,
};
use super::{BrushAssetError, PreparedBrushAsset};
use crate::evaluation::object_scene_style_kernels::bitmap_brush_resources_store::BrushStoreError;
use crate::object_codec::PngWorkspace;
use crate::object_document::AssetKind;
use crate::revision::ImageResourceError;

const CAPACITY: BrushAssetError =
    BrushAssetError::Store(BrushStoreError::Budget(ImageResourceError::Capacity));

#[test]
fn oracle_header_and_row_counts_are_pinned() {
    assert!(ORACLE.contains("# baseline commit fbe4b8a33e6d5a229f014154df2ba21988a9fabc"));
    assert!(ORACLE.contains(
        "image-budget.js sha256 c93eb2d0ef588abd31d7c2ca2d4d04d255351447e1c16e1ceb2465c22060966f"
    ));
    assert!(ORACLE.contains("# counts {\"P\":9,\"B\":9,\"S\":6,\"U\":5,\"X\":5}"));
    let counts = [
        rows('P').len(),
        rows('B').len(),
        rows('S').len(),
        rows('U').len(),
        rows('X').len(),
    ];
    assert_eq!(counts, [9, 9, 6, 5, 5]);
}

#[test]
fn every_oracle_png_publishes_its_exact_straight_rgba() {
    for row in png_rows() {
        let entry = Entry::png(&row.png);
        let snap = snapshot(&[("asset", AssetKind::Image, &entry)]);
        let (mut publisher, budget) = fresh(4 << 20);
        let published = publisher
            .publish(&snap, request("asset", "brush"), None)
            .unwrap();
        let bitmap = &published.bitmap;
        let cost = baseline_cost(row.name);
        assert!(!published.reused, "{}", row.name);
        assert_eq!(bitmap.dimensions(), (row.width, row.height), "{}", row.name);
        assert_eq!(
            mismatches(bitmap.rgba8(), &row.expected),
            Vec::<usize>::new(),
            "{}",
            row.name
        );
        assert_eq!((bitmap.cost(), bitmap.version()), (cost, 1), "{}", row.name);
        assert_eq!(
            used(&budget),
            cost,
            "the decode charge is released: {}",
            row.name
        );
        assert_eq!(publisher.store().total_bytes(), cost as u64, "{}", row.name);
        assert_eq!(
            bitmap.declared_resource(),
            ("bitmap-brush:brush".to_string(), "1".to_string()),
            "{}",
            row.name
        );
    }
}

#[test]
fn hand_written_pixels_survive_as_straight_alpha_literals() {
    // Independent of the oracle file: the source pixels of three corpus entries,
    // including transparent pixels that keep their colour (no premultiplication)
    // and an RGB source that gains alpha 255.
    let cases: [(&str, Vec<u8>); 3] = [
        (
            "tiny-rgba-2x2",
            vec![255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 0, 17, 34, 51, 1],
        ),
        ("px1", vec![10, 20, 30, 40]),
        (
            "rgb-3x2",
            vec![
                12, 34, 56, 255, 200, 100, 50, 255, 7, 8, 9, 255, 10, 11, 12, 255, 250, 251, 252,
                255, 1, 2, 3, 255,
            ],
        ),
    ];
    for (name, expected) in cases {
        let row = png_row(name);
        let entry = Entry::png(&row.png);
        let snap = snapshot(&[("asset", AssetKind::Image, &entry)]);
        let (mut publisher, _budget) = fresh(1 << 20);
        let published = publisher
            .publish(&snap, request("asset", "b"), None)
            .unwrap();
        assert_eq!(published.bitmap.rgba8(), expected.as_slice(), "{name}");
    }
}

#[test]
fn the_baseline_budget_sequences_equal_the_store_totals() {
    let owned: Vec<(String, Entry)> = png_rows()
        .iter()
        .map(|row| (row.name.to_string(), Entry::png(&row.png)))
        .collect();
    let entries: Vec<(&str, AssetKind, &Entry)> = owned
        .iter()
        .map(|(name, entry)| (name.as_str(), AssetKind::Image, entry))
        .collect();
    let snap = snapshot(&entries);
    for row in rows('S') {
        let (head, outcome) = row.split_once(" => ").unwrap();
        let (name, sequence) = head.split_once(' ').unwrap();
        let (total, count) = outcome.split_once(':').unwrap();
        let (mut publisher, budget) = fresh(8 << 20);
        for pair in sequence.split(',') {
            let (brush, asset) = pair.split_once('=').unwrap();
            publisher
                .publish(&snap, request(asset, brush), None)
                .unwrap();
        }
        let total: u64 = total.parse().unwrap();
        assert_eq!(publisher.store().total_bytes(), total, "{name}");
        assert_eq!(
            publisher.store().stats().count,
            count.parse::<usize>().unwrap(),
            "{name}"
        );
        assert_eq!(
            used(&budget) as u64,
            total,
            "{name}: only resident bytes stay charged"
        );
    }
}

#[test]
fn publishing_the_same_asset_twice_reuses_the_retained_allocation_and_charges_once() {
    let row = png_row("rgba-16x16");
    let entry = Entry::png(&row.png);
    let first_snapshot = snapshot(&[("asset", AssetKind::Image, &entry)]);
    let (mut publisher, budget) = fresh(1 << 20);
    let first = publisher
        .publish(&first_snapshot, request("asset", "b"), None)
        .unwrap();
    assert!(!first.reused);
    // A later snapshot (an unrelated edit) that retains the SAME allocation,
    // here even under another record id: identity is the allocation.
    let second_snapshot = snapshot(&[("alias", AssetKind::Image, &entry)]);
    let second = publisher
        .publish(&second_snapshot, request("alias", "b"), None)
        .unwrap();
    assert!(second.reused);
    assert!(second.bitmap.same_allocation_as(&first.bitmap));
    assert_eq!(second.bitmap.version(), 1);
    assert_eq!(used(&budget), 1024, "one charge");
    assert_eq!(publisher.store().stats().count, 1);
    assert_eq!(first.bitmap.holders(), 3, "two handles and the store entry");
}

#[test]
fn equal_bytes_in_another_allocation_are_a_new_version_not_a_reuse() {
    let row = png_row("tiny-rgba-2x2");
    let (a, b) = (Entry::png(&row.png), Entry::png(&row.png));
    let snap = snapshot(&[
        ("media-a", AssetKind::Image, &a),
        ("media-b", AssetKind::Image, &b),
    ]);
    let (mut publisher, budget) = fresh(1 << 20);
    let v1 = publisher
        .publish(&snap, request("media-a", "b"), None)
        .unwrap();
    let in_flight = publisher.store_mut().lease("b").unwrap();
    let v2 = publisher
        .publish(&snap, request("media-b", "b"), None)
        .unwrap();
    assert!(!v2.reused);
    assert_eq!((v1.bitmap.version(), v2.bitmap.version()), (1, 2));
    assert!(!publisher.store().is_current(&v1.bitmap));
    assert!(publisher.store().is_current(&v2.bitmap));
    assert_eq!(
        in_flight.rgba8(),
        row.expected.as_slice(),
        "the in-flight scene keeps its pixels"
    );
    assert_eq!(
        used(&budget),
        32,
        "both versions are charged while one is held"
    );
    assert!(publisher.published_from(&snap, "b", "media-a").is_none());
    let current = publisher.published_from(&snap, "b", "media-b").unwrap();
    assert!(current.same_allocation_as(&v2.bitmap));
    drop((current, v1, in_flight));
    assert_eq!(
        used(&budget),
        16,
        "the replaced version is released with its last holder"
    );
}

#[test]
fn a_bypass_replacement_in_the_store_is_never_mistaken_for_the_asset() {
    let row = png_row("tiny-rgba-2x2");
    let entry = Entry::png(&row.png);
    let snap = snapshot(&[("asset", AssetKind::Image, &entry)]);
    let (mut publisher, _budget) = fresh(1 << 20);
    let first = publisher
        .publish(&snap, request("asset", "b"), None)
        .unwrap();
    // Someone publishes different pixels under the same id behind the publisher.
    publisher
        .store_mut()
        .publish("b", 2, 2, &[7u8; 16], None)
        .unwrap();
    assert!(publisher.published_from(&snap, "b", "asset").is_none());
    let again = publisher
        .publish(&snap, request("asset", "b"), None)
        .unwrap();
    assert!(!again.reused, "the asset is decoded again");
    assert_eq!(again.bitmap.rgba8(), row.expected.as_slice());
    assert!(again.bitmap.version() > first.bitmap.version());
}

#[test]
fn a_cancelled_preparation_releases_its_charge_and_id_and_keeps_the_published_version() {
    let row = png_row("rgba-16x16");
    let (entry, other) = (Entry::png(&row.png), Entry::png(&png_row("px1").png));
    let snap = snapshot(&[
        ("asset", AssetKind::Image, &entry),
        ("asset2", AssetKind::Image, &other),
    ]);
    let (mut publisher, budget) = fresh(1 << 20);
    let prepared = publisher
        .prepare(&snap, request("asset", "b"), None)
        .unwrap();
    assert!(matches!(prepared, PreparedBrushAsset::Fresh(_)));
    assert_eq!(
        (used(&budget), publisher.store().contains("b")),
        (1024, false)
    );
    prepared.cancel();
    assert_eq!((used(&budget), publisher.store().contains("b")), (0, false));
    let first = publisher
        .publish(&snap, request("asset", "b"), None)
        .unwrap();
    assert_eq!(
        first.bitmap.version(),
        1,
        "the id mark was released by the cancel"
    );
    let replacement = publisher
        .prepare(&snap, request("asset2", "b"), None)
        .unwrap();
    assert_eq!(used(&budget), 1024 + 4);
    drop(replacement);
    assert_eq!(used(&budget), 1024);
    assert!(publisher.store().is_current(&first.bitmap));
    assert!(publisher.published_from(&snap, "b", "asset").is_some());
}

#[test]
fn a_commit_against_a_snapshot_that_no_longer_retains_the_allocation_is_stale() {
    let row = png_row("tiny-rgba-2x2");
    let (e1, e2) = (Entry::png(&row.png), Entry::png(&row.png));
    let snap_a = snapshot(&[("asset", AssetKind::Image, &e1)]);
    let replaced = snapshot(&[("asset", AssetKind::Image, &e2)]);
    let removed = snapshot(&[("other", AssetKind::Image, &e1)]);
    let unrelated_edit = snapshot(&[("asset", AssetKind::Image, &e1)]);
    let (mut publisher, budget) = fresh(1 << 20);
    for current in [&replaced, &removed] {
        let prepared = publisher
            .prepare(&snap_a, request("asset", "b"), None)
            .unwrap();
        assert_eq!(
            publisher.commit(prepared, current).unwrap_err(),
            BrushAssetError::Stale
        );
        assert_eq!((used(&budget), publisher.store().contains("b")), (0, false));
    }
    let prepared = publisher
        .prepare(&snap_a, request("asset", "b"), None)
        .unwrap();
    let bitmap = publisher.commit(prepared, &unrelated_edit).unwrap();
    assert_eq!((bitmap.version(), used(&budget)), (1, 16));
}

#[test]
fn a_matching_planned_size_publishes() {
    let row = png_row("rgba-16x16");
    let entry = Entry::png(&row.png);
    let snap = snapshot(&[("asset", AssetKind::Image, &entry)]);
    let (mut publisher, _budget) = fresh(1 << 20);
    let mut planned = request("asset", "b");
    planned.expected_size = Some((16, 16));
    let published = publisher.publish(&snap, planned, None).unwrap();
    assert_eq!(published.bitmap.dimensions(), (16, 16));
}

#[test]
fn the_decode_copy_and_the_store_copy_must_both_fit_and_a_refusal_changes_nothing() {
    let row = png_row("solid-256x256");
    let copy = row.expected.len();
    assert_eq!(copy, 262_144);
    // Premise of the sizes below: the PNG workspace is smaller than one copy,
    // so the decode peak is one copy plus a row buffer.
    assert!(std::mem::size_of::<PngWorkspace>() < copy);
    let entry = Entry::png(&row.png);
    let snap = snapshot(&[("asset", AssetKind::Image, &entry)]);
    let (mut publisher, budget) = fresh(copy + 4096);
    assert_eq!(
        publisher
            .publish(&snap, request("asset", "b"), None)
            .unwrap_err(),
        CAPACITY
    );
    assert_eq!((used(&budget), publisher.store().stats().count), (0, 0));
    let (mut publisher, budget) = fresh(2 * copy + 4096);
    let published = publisher
        .publish(&snap, request("asset", "b"), None)
        .unwrap();
    assert_eq!(
        used(&budget),
        copy,
        "exactly one charge remains after the decode copy is released"
    );
    drop(published);
    assert_eq!(used(&budget), copy, "the store entry still holds it");
}

#[test]
fn pressure_eviction_runs_through_the_store_policy_for_unpinned_brushes_only() {
    let row = png_row("solid-256x256");
    let copy = row.expected.len();
    let (old, new) = (Entry::png(&row.png), Entry::png(&row.png));
    let snap = snapshot(&[
        ("old-asset", AssetKind::Image, &old),
        ("new-asset", AssetKind::Image, &new),
    ]);
    // Unpinned resident: evicted through the retire port, then the new brush fits.
    let (mut publisher, budget) = fresh(2 * copy + 4096);
    drop(
        publisher
            .publish(&snap, request("old-asset", "old"), None)
            .unwrap(),
    );
    let mut retired: Vec<String> = Vec::new();
    {
        let mut port = |ids: &[String]| {
            retired.extend(ids.iter().cloned());
            true
        };
        publisher
            .publish(&snap, request("new-asset", "new"), Some(&mut port))
            .unwrap();
    }
    assert_eq!(retired, ["old"]);
    assert_eq!(
        (
            publisher.store().contains("old"),
            publisher.store().contains("new")
        ),
        (false, true)
    );
    assert_eq!(
        (used(&budget), publisher.store().stats().pressure_evictions),
        (copy, 1)
    );
    // A scene still holds the resident: never evicted, typed Capacity, port untouched.
    let (mut publisher, budget) = fresh(2 * copy + 4096);
    let held = publisher
        .publish(&snap, request("old-asset", "old"), None)
        .unwrap();
    let mut calls = 0;
    let mut port = |_: &[String]| {
        calls += 1;
        true
    };
    let refused = publisher
        .publish(&snap, request("new-asset", "new"), Some(&mut port))
        .unwrap_err();
    assert_eq!(refused, CAPACITY);
    assert_eq!(calls, 0);
    assert!(publisher.store().is_current(&held.bitmap));
    assert_eq!(
        (used(&budget), publisher.store().contains("new")),
        (copy, false)
    );
}
