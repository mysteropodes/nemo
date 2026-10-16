//! same-ledger admission and immutable retention oracles.
use super::*;
use crate::object_document::{AssetId, AssetInventory, AssetKind, AssetRecord, AssetSource};
use crate::revision::ObjectSnapshot;
use std::sync::Arc;

fn project() -> ProjectEnvelope {
    use crate::object_codec::encoded_wire_test::{limits, wire};
    crate::object_codec::decode_allocated_project(
        &serde_json::to_vec(&wire()).unwrap(),
        limits(),
        |n| Ok(vec![0; n]),
    )
    .unwrap()
}
fn inventory(project: &ProjectEnvelope, shared: bool) -> AssetInventory {
    let ProjectReferenceState::StillImage(reference) = project.reference_state() else {
        panic!()
    };
    let mut records = Vec::new();
    for id in ["first", "second"] {
        let bytes = if shared {
            Arc::clone(reference.encoded_allocation())
        } else {
            Arc::new(reference.encoded_png().to_vec())
        };
        let source = AssetSource::admitted_embedded("image/png".into(), bytes, 1024).unwrap();
        records.push(Arc::new(
            AssetRecord::new(
                AssetId::new(id.into()).unwrap(),
                id.into(),
                AssetKind::Image,
                source,
            )
            .unwrap(),
        ));
    }
    AssetInventory::new(records, Vec::new(), 2, 0).unwrap()
}

#[test]
fn reference_and_catalog_aliases_charge_one_actual_capacity_and_release_last_pin() {
    let base = project();
    let ProjectReferenceState::StillImage(reference) = base.reference_state() else {
        panic!()
    };
    let cost = reference.encoded_allocation().capacity();
    let weak = Arc::downgrade(reference.encoded_allocation());
    let retained = base.with_asset_inventory(Some(inventory(&base, true)));
    let budget = EncodedRetentionBudget::new(cost).unwrap();
    let tickets = pending_charges(&retained, &budget).unwrap();
    assert_eq!(tickets.len(), 3);
    assert_eq!(budget.used(), Ok(cost));
    for ticket in tickets {
        ticket.commit().unwrap();
    }
    let old = retained.clone();
    let changed = retained
        .with_drawing(retained.drawing().clone())
        .with_reference(ProjectReferenceState::NoMedia);
    assert!(std::ptr::eq(
        old.asset_inventory().unwrap().records()[0]
            .source()
            .encoded_allocation()
            .unwrap()
            .as_ref(),
        changed.asset_inventory().unwrap().records()[1]
            .source()
            .encoded_allocation()
            .unwrap()
            .as_ref()
    ));
    assert_eq!(changed.project(), base.project());
    drop((reference, base, retained, changed));
    assert!(weak.upgrade().is_some());
    assert_eq!(budget.used(), Ok(cost));
    drop(old);
    assert!(weak.upgrade().is_none());
    assert_eq!(budget.used(), Ok(0));
}

#[test]
fn aggregate_admission_refusal_rolls_back_all_pending_charges_before_identity() {
    let base = project();
    let ProjectReferenceState::StillImage(reference) = base.reference_state() else {
        panic!()
    };
    let cost = reference.encoded_allocation().capacity();
    let retained = base.with_asset_inventory(Some(inventory(&base, false)));
    let budget = EncodedRetentionBudget::new(cost * 2).unwrap();
    let called = std::cell::Cell::new(false);
    let result = ObjectSnapshot::project_entry("aggregate".into(), retained, &budget, |_, _| {
        called.set(true);
        unreachable!()
    });
    assert!(result.is_err());
    assert!(!called.get());
    assert_eq!(budget.used(), Ok(0));
}

#[test]
fn failed_identity_releases_pending_charge_and_existing_pin_stays_unmodified() {
    let base = project();
    let retained = base.with_asset_inventory(Some(inventory(&base, true)));
    let budget = EncodedRetentionBudget::new(1024).unwrap();
    let before = retained.clone();
    let result = ObjectSnapshot::project_entry("identity".into(), retained, &budget, |_, _| {
        Err("injected identity refusal")
    });
    assert!(result.is_err());
    assert_eq!(budget.used(), Ok(0));
    assert_eq!(before.reference_state(), base.reference_state());
    assert_eq!(before.asset_inventory().unwrap().records().len(), 2);
}

#[test]
fn nonempty_placement_cannot_enter_owner_before_its_real_consumer_exists() {
    use crate::object_document::{AssetPlacement, AssetTarget};
    let base = project();
    let records = inventory(&base, true).records().to_vec();
    let placement = AssetPlacement::new(
        "p".into(),
        AssetId::new("first".into()).unwrap(),
        AssetTarget::Drawing {
            context_id: "scene-root".into(),
            layer_uid: "L".into(),
            stroke_id: None,
        },
        0,
    )
    .unwrap();
    let inventory = AssetInventory::new(records, vec![placement], 2, 1).unwrap();
    let value = base.with_asset_inventory(Some(inventory));
    let budget = EncodedRetentionBudget::new(1024).unwrap();
    assert!(pending_charges(&value, &budget).is_err());
    assert_eq!(budget.used(), Ok(0));
}
