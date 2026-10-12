//! Real owner lifetime controls for the existing counted lease manager.
use super::{LeaseBundle, LeaseError, ResourceLeaseManager};
use crate::commands::NativeOpacityApplication;
use crate::history::NativeObjectHistory;
use crate::object_codec::{decode_project, encode_project};
use crate::revision::{DocumentSnapshot, ObjectSnapshot};
use serde_json::{json, Value};

fn cases() -> Value {
    serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap()
}

fn owner(instance: &str) -> NativeObjectHistory {
    let source = json!({"format":"nemo.native-object-document", "formatVersion":1,
        "totalFrames":21, "layers":[{"layerUid":"legacy layer:alpha"},{"layerUid":"0"}],
        "objects":cases()["records"]});
    NativeObjectHistory::new(
        instance,
        decode_project(&serde_json::to_vec(&source).unwrap()).unwrap(),
    )
    .unwrap()
}

fn counts(manager: &ResourceLeaseManager) -> (u64, u64, u64) {
    let counts = manager.counters();
    (counts.acquired(), counts.released(), counts.live())
}

fn resources() -> Vec<String> {
    vec!["actual-contour-a".into(), "actual-contour-b".into()]
}

#[test]
fn real_object_revision_survives_edit_undo_redo_and_owner_drop() {
    let mut owner = owner("lease-owner");
    let zero = owner.acquire_snapshot(0).unwrap();
    let original = encode_project(zero.document()).unwrap();
    let pins = (
        zero.instance_id().to_owned(),
        zero.document_id().to_owned(),
        zero.snapshot_id().to_owned(),
        zero.content_revision(),
    );
    let mut manager = ResourceLeaseManager::new();
    let lease: LeaseBundle<ObjectSnapshot> = manager.acquire_object(zero, resources()).unwrap();
    assert_eq!(counts(&manager), (3, 0, 3));
    let payload = cases()["command"]["request"]["payload"].clone();
    let prepared = owner
        .prepare_fill_json(&serde_json::to_vec(&payload).unwrap())
        .unwrap();
    assert_eq!(owner.commit_fill(prepared), Ok(1));
    assert_ne!(
        encode_project(owner.acquire_snapshot(1).unwrap().document()).unwrap(),
        original
    );
    assert_eq!(owner.undo(1), Ok(2));
    assert_eq!(owner.redo(2), Ok(3));
    drop(owner);
    let pinned: &ObjectSnapshot = lease.snapshot();
    assert_eq!(
        (
            pinned.instance_id(),
            pinned.document_id(),
            pinned.snapshot_id(),
            pinned.content_revision()
        ),
        (pins.0.as_str(), pins.1.as_str(), pins.2.as_str(), pins.3)
    );
    assert_eq!(encode_project(pinned.document()).unwrap(), original);
    let ids = [vec![lease.snapshot_lease_id()], lease.resource_lease_ids()].concat();
    manager.release(lease).unwrap();
    assert_eq!(counts(&manager), (3, 3, 0));
    for id in ids {
        assert_eq!(
            manager.release_id_for_test(id),
            Err(LeaseError::UnknownLease(id))
        );
    }
    assert_eq!(counts(&manager), (3, 3, 0));
}

#[test]
fn opacity_and_object_bundles_share_one_id_space_without_type_erasure() {
    let opacity = NativeOpacityApplication::new(
        "opacity-lease",
        crate::codec::decode_project(include_bytes!("fixtures/opacity-v2/project.json")).unwrap(),
    )
    .unwrap();
    let object = owner("object-lease");
    let mut manager = ResourceLeaseManager::new();
    let old: LeaseBundle = manager
        .acquire(opacity.acquire_snapshot(0).unwrap(), [])
        .unwrap();
    let new: LeaseBundle<ObjectSnapshot> = manager
        .acquire_object(object.acquire_snapshot(0).unwrap(), resources())
        .unwrap();
    let opacity_snapshot: &DocumentSnapshot = old.snapshot();
    let object_snapshot: &ObjectSnapshot = new.snapshot();
    assert_eq!(
        opacity_snapshot.static_opacity("r08_curve_layer"),
        Some(25.0)
    );
    assert_eq!(object_snapshot.instance_id(), "object-lease");
    assert_ne!(old.snapshot_lease_id(), new.snapshot_lease_id());
    assert_eq!(counts(&manager), (4, 0, 4));
    manager.release(new).unwrap();
    assert_eq!(counts(&manager), (4, 3, 1));
    assert_eq!(old.snapshot().static_opacity("r08_curve_layer"), Some(25.0));
    manager.release(old).unwrap();
    assert_eq!(counts(&manager), (4, 4, 0));
}

#[test]
fn injected_resource_failure_unwinds_only_the_new_object_bundle() {
    let owner = owner("unwind-owner");
    for fail_at in 0..3 {
        let mut manager = ResourceLeaseManager::new();
        let retained = manager
            .acquire_object(owner.acquire_snapshot(0).unwrap(), [])
            .unwrap();
        manager.inject_failure_after_resources(Some(fail_at));
        let result = manager.acquire_object(
            owner.acquire_snapshot(0).unwrap(),
            ["one".into(), "two".into(), "three".into()],
        );
        assert!(matches!(
            result,
            Err(LeaseError::InjectedResourceFailure { .. })
        ));
        assert_eq!(
            counts(&manager),
            (fail_at as u64 + 2, fail_at as u64 + 1, 1)
        );
        assert_eq!(retained.snapshot().instance_id(), "unwind-owner");
        manager.release(retained).unwrap();
        assert_eq!(manager.counters().live(), 0);
        assert_eq!(manager.counters().acquired(), manager.counters().released());
    }
}

#[test]
fn lease_id_exhaustion_is_atomic_or_unwinds_partial_acquisition() {
    let owner = owner("id-overflow-owner");
    let mut exhausted = ResourceLeaseManager::with_next_id(u64::MAX);
    assert!(matches!(
        exhausted.acquire_object(owner.acquire_snapshot(0).unwrap(), []),
        Err(LeaseError::IdExhausted)
    ));
    assert_eq!(counts(&exhausted), (0, 0, 0));
    assert!(exhausted.live.is_empty());
    let mut partial = ResourceLeaseManager::with_next_id(u64::MAX - 1);
    assert!(matches!(
        partial.acquire_object(owner.acquire_snapshot(0).unwrap(), resources()),
        Err(LeaseError::IdExhausted)
    ));
    assert_eq!(counts(&partial), (1, 1, 0));
    assert!(partial.live.is_empty());
}

#[test]
fn counter_exhaustion_refuses_without_leaking_object_or_resource_leases() {
    let owner = owner("counter-overflow-owner");
    for remaining in [0, 1] {
        let mut manager = ResourceLeaseManager::new();
        manager.counters.acquired = u64::MAX - remaining;
        manager.counters.released = u64::MAX - remaining;
        let result = manager.acquire_object(owner.acquire_snapshot(0).unwrap(), resources());
        assert!(matches!(result, Err(LeaseError::CounterOverflow)));
        assert_eq!(manager.counters().live(), 0);
        assert_eq!(manager.counters().acquired(), manager.counters().released());
        assert!(manager.live.is_empty());
    }
}
