//! The sealed commit: a prepared asset edit publishes only into the exact owner
//! snapshot it was built from. Real owners (`ObjectFillOwner`) over real project
//! snapshots; the pin comparison is also driven through single-flaw copies.
use super::origin_refusal;
use crate::commands::ObjectFillOwner;
use crate::object_document::ProjectEnvelope;
use crate::object_snapshot::PreparedAssetEdit;
use crate::request_receipts::DispatchErrorCode;
use crate::revision::ObjectSnapshot;
use std::sync::Arc;

include!("object_raster_asset_attach_support.rs");

fn project() -> ProjectEnvelope {
    open_history()
        .acquire_snapshot(0)
        .unwrap()
        .project()
        .unwrap()
        .clone()
}

fn fresh_owner(instance: &str) -> ObjectFillOwner {
    ObjectFillOwner::from_admitted_project(instance, project()).unwrap()
}

fn prepare(owner: &ObjectFillOwner, id: &str, name: &str, png: &str) -> PreparedAssetEdit {
    let snapshot = owner.acquire_snapshot(owner.content_revision()).unwrap();
    snapshot
        .prepare_asset_attachment(id, name, "image/png", &oracle_bytes(png))
        .unwrap()
}

fn names(owner: &ObjectFillOwner, revision: u64) -> Vec<String> {
    owner
        .acquire_snapshot(revision)
        .unwrap()
        .project()
        .unwrap()
        .asset_inventory()
        .map(|inventory| {
            inventory
                .records()
                .iter()
                .map(|record| record.name().to_string())
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn commit_publishes_the_prepared_asset_and_reports_the_new_revision() {
    let mut owner = fresh_owner("desktop");
    let prepared = prepare(&owner, "r1", "blue.png", "blue-2x1");
    assert_eq!(owner.commit_asset_edit(prepared), Ok(1));
    assert_eq!(owner.content_revision(), 1);
    assert_eq!(names(&owner, 1), ["blue.png"]);
    assert!(names(&owner, 0).is_empty());
}

#[test]
fn an_unchanged_edit_returns_the_current_revision_without_publishing() {
    let mut owner = fresh_owner("desktop");
    let first = prepare(&owner, "r1", "blue.png", "blue-2x1");
    assert_eq!(owner.commit_asset_edit(first), Ok(1));
    let duplicate = prepare(&owner, "r2", "again.png", "blue-2x1");
    assert!(!duplicate.changed());
    assert_eq!(owner.commit_asset_edit(duplicate), Ok(1));
    let snapshot = owner.acquire_snapshot(1).unwrap();
    let absent = snapshot.prepare_asset_removal("asset-nothing").unwrap();
    assert!(!absent.changed());
    assert_eq!(owner.commit_asset_edit(absent), Ok(1));
    assert_eq!(owner.content_revision(), 1);
    assert_eq!(names(&owner, 1), ["blue.png"]);
}

#[test]
fn a_remove_edit_publishes_the_shorter_catalog() {
    let mut owner = fresh_owner("desktop");
    for (id, name, png) in [("r1", "a.png", "blue-2x1"), ("r2", "b.png", "red-2x1")] {
        let prepared = prepare(&owner, id, name, png);
        owner.commit_asset_edit(prepared).unwrap();
    }
    let snapshot = owner.acquire_snapshot(2).unwrap();
    let removal = snapshot.prepare_asset_removal("asset-r1").unwrap();
    assert_eq!(owner.commit_asset_edit(removal), Ok(3));
    assert_eq!(names(&owner, 3), ["b.png"]);
    assert_eq!(names(&owner, 2), ["a.png", "b.png"]);
}

#[test]
fn a_prepared_edit_from_an_older_revision_is_stale_and_publishes_nothing() {
    let mut owner = fresh_owner("desktop");
    let first = prepare(&owner, "r1", "blue.png", "blue-2x1");
    let second = prepare(&owner, "r2", "red.png", "red-2x1");
    assert_eq!(owner.commit_asset_edit(first), Ok(1));
    assert_eq!(
        owner.commit_asset_edit(second),
        Err(DispatchErrorCode::StaleRevision)
    );
    assert_eq!(owner.content_revision(), 1);
    assert_eq!(names(&owner, 1), ["blue.png"]);
}

#[test]
fn a_prepared_edit_from_another_document_or_instance_is_refused() {
    let mut target = fresh_owner("desktop");
    let same_instance_other_document = fresh_owner("desktop");
    let other_instance = fresh_owner("elsewhere");
    let foreign = prepare(&same_instance_other_document, "r1", "blue.png", "blue-2x1");
    assert_eq!(
        target.commit_asset_edit(foreign),
        Err(DispatchErrorCode::WrongDocument)
    );
    let stranger = prepare(&other_instance, "r2", "red.png", "red-2x1");
    assert_eq!(
        target.commit_asset_edit(stranger),
        Err(DispatchErrorCode::WrongInstance)
    );
    assert_eq!(target.content_revision(), 0);
    assert!(names(&target, 0).is_empty());
}

// ---- pin comparison: the real function and single-flaw copies -----------------------------------------

type Rule = fn(&ObjectSnapshot, &ObjectSnapshot) -> Option<DispatchErrorCode>;

fn by_revision_only(a: &ObjectSnapshot, b: &ObjectSnapshot) -> Option<DispatchErrorCode> {
    (a.content_revision() != b.content_revision()).then_some(DispatchErrorCode::StaleRevision)
}

fn without_staleness(a: &ObjectSnapshot, b: &ObjectSnapshot) -> Option<DispatchErrorCode> {
    if a.instance_id() != b.instance_id() {
        return Some(DispatchErrorCode::WrongInstance);
    }
    (a.document_id() != b.document_id()).then_some(DispatchErrorCode::WrongDocument)
}

fn document_before_instance(a: &ObjectSnapshot, b: &ObjectSnapshot) -> Option<DispatchErrorCode> {
    if a.document_id() != b.document_id() {
        return Some(DispatchErrorCode::WrongDocument);
    }
    if a.instance_id() != b.instance_id() {
        return Some(DispatchErrorCode::WrongInstance);
    }
    (a.snapshot_id() != b.snapshot_id() || a.content_revision() != b.content_revision())
        .then_some(DispatchErrorCode::StaleRevision)
}

#[test]
fn the_pin_comparison_names_each_mismatch_and_each_single_flaw_copy_fails_its_case() {
    let mut a = fresh_owner("desktop");
    let at_zero = a.acquire_snapshot(0).unwrap();
    let prepared = prepare(&a, "r1", "blue.png", "blue-2x1");
    a.commit_asset_edit(prepared).unwrap();
    let at_one = a.acquire_snapshot(1).unwrap();
    let other_document = fresh_owner("desktop").acquire_snapshot(0).unwrap();
    let other_instance = fresh_owner("elsewhere").acquire_snapshot(0).unwrap();
    // (case, origin, current, expected)
    let cases = [
        ("same", &at_zero, &at_zero, None),
        (
            "stale",
            &at_zero,
            &at_one,
            Some(DispatchErrorCode::StaleRevision),
        ),
        (
            "other document",
            &other_document,
            &at_zero,
            Some(DispatchErrorCode::WrongDocument),
        ),
        (
            "other instance",
            &other_instance,
            &at_zero,
            Some(DispatchErrorCode::WrongInstance),
        ),
    ];
    for (name, origin, current, expected) in cases {
        assert_eq!(origin_refusal(origin, current), expected, "{name}");
    }
    let mutants: [(&str, Rule, &[&str]); 3] = [
        (
            "revision only",
            by_revision_only,
            &["other document", "other instance"],
        ),
        ("no staleness", without_staleness, &["stale"]),
        (
            "document before instance",
            document_before_instance,
            &["other instance"],
        ),
    ];
    for (label, rule, broken) in mutants {
        for (name, origin, current, expected) in cases {
            let answer = rule(origin, current);
            if broken.contains(&name) {
                assert_ne!(answer, expected, "{label} must fail {name}");
            } else {
                assert_eq!(answer, expected, "{label} must still pass {name}");
            }
        }
    }
    // The owner's retained snapshot is shared, not copied: pinning costs no bytes.
    assert!(Arc::ptr_eq(
        &a.acquire_snapshot(1)
            .unwrap()
            .project()
            .unwrap()
            .asset_inventory()
            .unwrap()
            .records()[0],
        &at_one
            .project()
            .unwrap()
            .asset_inventory()
            .unwrap()
            .records()[0]
    ));
}
