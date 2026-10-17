//! The revision-store half of the asset transaction on a PRIVATE encoded-retention
//! ledger (`used()` is exact there; the shared ledger is process wide). Real
//! owner, real snapshots from the history, real PNG bytes from the oracle.
use crate::object_document::{
    AssetId, AssetInventory, AssetKind, AssetRecord, AssetSource, ProjectReferenceState,
};
use crate::revision::object_revision::encoded_retention_budget::EncodedRetentionBudget;
use crate::revision::{ObjectRevisionOwner, ObjectSnapshot};
use std::sync::Arc;

include!("object_raster_asset_attach_support.rs");

fn private_budget(limit: usize) -> EncodedRetentionBudget {
    EncodedRetentionBudget::new(limit).expect("private budget")
}

/// Bytes allocated through the ledger's own permit: charged from the start.
fn bound_source(budget: &EncodedRetentionBudget, bytes: &[u8]) -> AssetSource {
    let mut permit = budget.reserve(bytes.len()).expect("reserve");
    let mut buffer = permit.allocate().expect("allocate");
    buffer.copy_from_slice(bytes);
    let allocation = Arc::new(buffer);
    let source =
        AssetSource::admitted_embedded("image/png".to_string(), Arc::clone(&allocation), 1 << 20)
            .expect("source");
    permit.bind(&allocation).expect("bind");
    source
}

fn record_with(id: &str, kind: AssetKind, source: AssetSource) -> Arc<AssetRecord> {
    Arc::new(
        AssetRecord::new(
            AssetId::new(id.to_string()).expect("id"),
            format!("{id}.png"),
            kind,
            source,
        )
        .expect("record"),
    )
}

fn bound_record(budget: &EncodedRetentionBudget, id: &str, bytes: &[u8]) -> Arc<AssetRecord> {
    record_with(id, AssetKind::Image, bound_source(budget, bytes))
}

fn unbound_source(media_type: &str, bytes: Vec<u8>) -> AssetSource {
    let limit = bytes.capacity() + 1;
    AssetSource::admitted_embedded(media_type.to_string(), Arc::new(bytes), limit).expect("source")
}

fn project_owner() -> ObjectRevisionOwner {
    ObjectRevisionOwner::from_admitted(open_history().acquire_snapshot(0).expect("snapshot"))
}

fn ids_at(owner: &ObjectRevisionOwner, revision: u64) -> Vec<String> {
    owner
        .acquire(revision)
        .expect("retained revision")
        .project()
        .expect("project")
        .asset_inventory()
        .map(|inventory| {
            inventory
                .records()
                .iter()
                .map(|record| record.id().as_str().to_string())
                .collect()
        })
        .unwrap_or_default()
}

fn id(text: &str) -> AssetId {
    AssetId::new(text.to_string()).expect("asset id")
}

#[test]
fn publish_charges_the_budget_once_and_advances_exactly_one_revision() {
    let budget = private_budget(1 << 20);
    let mut owner = project_owner();
    let png = oracle_bytes("blue-2x1");
    let record = bound_record(&budget, "a", &png);
    assert_eq!(budget.used(), Ok(png.len()));
    assert_eq!(
        owner.publish_admitted_asset_in(Arc::clone(&record), &budget),
        Ok(1)
    );
    assert_eq!(owner.content_revision(), 1);
    assert_eq!(budget.used(), Ok(png.len()));
    let head = owner.acquire(1).unwrap();
    let inventory = head.project().unwrap().asset_inventory().unwrap();
    assert_eq!(inventory.records().len(), 1);
    assert!(Arc::ptr_eq(&inventory.records()[0], &record));
    assert!(owner
        .acquire(0)
        .unwrap()
        .project()
        .unwrap()
        .asset_inventory()
        .is_none());
}

#[test]
fn an_allocation_that_was_never_reserved_is_charged_at_publication_and_released_with_its_pins() {
    let budget = private_budget(1 << 20);
    let mut owner = project_owner();
    let bytes = oracle_bytes("red-2x1");
    let record = record_with(
        "loose",
        AssetKind::Image,
        unbound_source("image/png", bytes.clone()),
    );
    assert_eq!(budget.used(), Ok(0));
    assert_eq!(owner.publish_admitted_asset_in(record, &budget), Ok(1));
    assert_eq!(budget.used(), Ok(bytes.len()));
    drop(owner);
    assert_eq!(budget.used(), Ok(0));
}

#[test]
fn existing_records_keep_their_arc_identity_and_order_across_revisions() {
    let budget = private_budget(1 << 20);
    let mut owner = project_owner();
    let records: Vec<_> = ["blue-2x1", "red-2x1", "rgba-3x1"]
        .iter()
        .zip(["a", "b", "c"])
        .map(|(png, name)| bound_record(&budget, name, &oracle_bytes(png)))
        .collect();
    for (index, record) in records.iter().enumerate() {
        assert_eq!(
            owner.publish_admitted_asset_in(Arc::clone(record), &budget),
            Ok(index as u64 + 1)
        );
    }
    assert_eq!(ids_at(&owner, 1), ["a"]);
    assert_eq!(ids_at(&owner, 2), ["a", "b"]);
    assert_eq!(ids_at(&owner, 3), ["a", "b", "c"]);
    let head = owner.acquire(3).unwrap();
    let held = head.project().unwrap().asset_inventory().unwrap().records();
    for (held, original) in held.iter().zip(&records) {
        assert!(Arc::ptr_eq(held, original));
    }
}

#[test]
fn every_refusal_before_advance_leaves_revision_budget_and_inventory_untouched() {
    let budget = private_budget(1 << 20);
    let mut owner = project_owner();
    let kept = bound_record(&budget, "kept", &oracle_bytes("blue-2x1"));
    owner
        .publish_admitted_asset_in(Arc::clone(&kept), &budget)
        .unwrap();
    let duplicate_id = bound_record(&budget, "kept", &oracle_bytes("red-2x1"));
    let font = record_with(
        "font",
        AssetKind::Font,
        unbound_source("image/png", oracle_bytes("red-2x1")),
    );
    let jpeg = record_with(
        "jpeg",
        AssetKind::Image,
        unbound_source("image/jpeg", oracle_bytes("red-2x1")),
    );
    let linked = record_with(
        "linked",
        AssetKind::Image,
        AssetSource::linked("file:///elsewhere.png".to_string(), 4096).unwrap(),
    );
    let too_big = record_with(
        "big",
        AssetKind::Image,
        unbound_source("image/png", vec![0u8; 16 * 1024 * 1024 + 1]),
    );
    let over_budget = record_with(
        "over",
        AssetKind::Image,
        unbound_source("image/png", vec![1u8; 2 << 20]),
    );
    // The cases stay alive for the whole loop so a refused record's own charge
    // cannot be mistaken for a leak.
    let cases = [
        ("duplicate id", duplicate_id, "asset catalog unavailable"),
        ("font kind", font, "asset admission unavailable"),
        ("media type", jpeg, "asset admission unavailable"),
        ("linked source", linked, "asset admission unavailable"),
        ("encoded bound", too_big, "asset admission unavailable"),
        (
            "ledger capacity",
            over_budget,
            "encoded retention unavailable",
        ),
    ];
    let used = budget.used().unwrap();
    for (label, record, message) in &cases {
        assert_eq!(
            owner.publish_admitted_asset_in(Arc::clone(record), &budget),
            Err(*message),
            "{label}"
        );
        assert_eq!(owner.content_revision(), 1, "{label}");
        assert_eq!(budget.used(), Ok(used), "{label}");
        assert_eq!(ids_at(&owner, 1), ["kept"], "{label}");
    }
}

#[test]
fn the_1024_asset_catalog_limit_is_enforced_and_one_allocation_is_charged_once() {
    let budget = private_budget(1 << 20);
    let mut owner = project_owner();
    let png = oracle_bytes("blue-2x1");
    let source = bound_source(&budget, &png);
    for index in 0..1024 {
        let record = record_with(&format!("n{index}"), AssetKind::Image, source.clone());
        assert_eq!(
            owner.publish_admitted_asset_in(record, &budget),
            Ok(index + 1)
        );
    }
    let extra = record_with("n1024", AssetKind::Image, source.clone());
    assert_eq!(
        owner.publish_admitted_asset_in(extra, &budget),
        Err("asset catalog unavailable")
    );
    assert_eq!(owner.content_revision(), 1024);
    assert_eq!(budget.used(), Ok(png.len()));
}

#[test]
fn removal_publishes_a_new_revision_and_the_bytes_stay_charged_until_every_pin_drops() {
    let budget = private_budget(1 << 20);
    let mut owner = project_owner();
    let (blue, red) = (oracle_bytes("blue-2x1"), oracle_bytes("red-2x1"));
    {
        let a = bound_record(&budget, "a", &blue);
        let b = bound_record(&budget, "b", &red);
        owner.publish_admitted_asset_in(a, &budget).unwrap();
        owner
            .publish_admitted_asset_in(Arc::clone(&b), &budget)
            .unwrap();
        assert_eq!(owner.publish_asset_removal(&id("a")), Ok(3));
        assert_eq!(ids_at(&owner, 3), ["b"]);
        let head = owner.acquire(3).unwrap();
        let left = head.project().unwrap().asset_inventory().unwrap().records();
        assert!(Arc::ptr_eq(&left[0], &b));
        assert_eq!(ids_at(&owner, 2), ["a", "b"]);
    }
    // Revisions 1 and 2 still pin the removed record: its charge is intact.
    assert_eq!(budget.used(), Ok(blue.len() + red.len()));
    drop(owner);
    assert_eq!(budget.used(), Ok(0));
}

fn owner_with_linked_font() -> ObjectRevisionOwner {
    let snapshot = open_history().acquire_snapshot(0).unwrap();
    let source = AssetSource::linked("file:///f.ttf".to_string(), 4096).unwrap();
    let font = record_with("font", AssetKind::Font, source);
    let inventory = AssetInventory::new(vec![font], Vec::new(), 1024, 0).unwrap();
    let project = snapshot
        .project()
        .unwrap()
        .with_asset_inventory(Some(inventory));
    let rebuilt = ObjectSnapshot::from_admitted_project("desktop".to_string(), project).unwrap();
    ObjectRevisionOwner::from_admitted(rebuilt)
}

#[test]
fn removal_refusals_leave_the_revision_untouched() {
    let budget = private_budget(1 << 20);
    let mut bare = project_owner();
    assert_eq!(bare.publish_asset_removal(&id("a")), Err("asset absent"));
    assert_eq!(bare.content_revision(), 0);
    let mut owner = project_owner();
    owner
        .publish_admitted_asset_in(
            bound_record(&budget, "a", &oracle_bytes("blue-2x1")),
            &budget,
        )
        .unwrap();
    assert_eq!(owner.publish_asset_removal(&id("zzz")), Err("asset absent"));
    assert_eq!(owner.content_revision(), 1);
    let mut fonts = owner_with_linked_font();
    assert_eq!(
        fonts.publish_asset_removal(&id("font")),
        Err("asset kind unavailable")
    );
    assert_eq!(fonts.content_revision(), 0);
}

#[test]
fn an_asset_the_reference_names_or_shares_cannot_be_removed() {
    let budget = private_budget(1 << 20);
    let mut history = open_history();
    history
        .attach_reference_png(
            0,
            "ref".to_string(),
            "ref.png".to_string(),
            &oracle_bytes("red-2x1"),
        )
        .unwrap();
    let snapshot = history.acquire_snapshot(1).unwrap();
    let ProjectReferenceState::StillImage(reference) =
        snapshot.project().unwrap().reference_state()
    else {
        panic!("the history carries the reference");
    };
    let shared = Arc::clone(reference.encoded_allocation());
    let mut owner = ObjectRevisionOwner::from_admitted(snapshot);
    // Same id as the reference, different bytes.
    owner
        .publish_admitted_asset_in(
            bound_record(&budget, "ref", &oracle_bytes("blue-2x1")),
            &budget,
        )
        .unwrap();
    assert_eq!(owner.publish_asset_removal(&id("ref")), Err("asset in use"));
    // Different id, the reference's own allocation.
    let twin = record_with(
        "twin",
        AssetKind::Image,
        AssetSource::admitted_embedded("image/png".to_string(), shared, 1 << 20).unwrap(),
    );
    owner.publish_admitted_asset_in(twin, &budget).unwrap();
    assert_eq!(
        owner.publish_asset_removal(&id("twin")),
        Err("asset in use")
    );
    assert_eq!(ids_at(&owner, 2), ["ref", "twin"]);
    assert_eq!(owner.content_revision(), 2);
}
