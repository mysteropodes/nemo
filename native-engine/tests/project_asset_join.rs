//! same-ledger admission and immutable retention oracles.
use super::*;
use crate::object_document::{AssetId, AssetInventory, AssetKind, AssetRecord, AssetSource};
use crate::revision::ObjectSnapshot;
use std::sync::Arc;

#[cfg(feature = "font_asset_wire")]
#[test]
fn validated_font_aliases_share_reserved_capacity_through_last_snapshot_pin() {
    const FONT: &[u8] = include_bytes!("../../src/fonts/Roboto-Regular.ttf");
    let budget = EncodedRetentionBudget::new(FONT.len()).unwrap();
    let mut permit = budget.reserve(FONT.len()).unwrap();
    let mut bytes = permit.allocate().unwrap();
    bytes.copy_from_slice(FONT);
    let bytes = Arc::new(bytes);
    let weak = Arc::downgrade(&bytes);
    let source = crate::object_codec::admitted_font_source(
        Arc::clone(&bytes),
        crate::object_codec::FontAssetLimits::new(FONT.len(), 256).unwrap(),
    )
    .unwrap();
    assert!(source.has_font_admission());
    assert!(Arc::ptr_eq(source.encoded_allocation().unwrap(), &bytes));
    permit.bind(&bytes).unwrap();
    let records = ["font-first", "font-alias"]
        .into_iter()
        .map(|id| {
            Arc::new(
                AssetRecord::new(
                    AssetId::new(id.into()).unwrap(),
                    id.into(),
                    AssetKind::Font,
                    source.clone(),
                )
                .unwrap(),
            )
        })
        .collect();
    let inventory = AssetInventory::new(records, Vec::new(), 2, 0).unwrap();
    let retained = project()
        .with_reference(ProjectReferenceState::NoMedia)
        .with_asset_inventory(Some(inventory));
    // The sole budget is already full. Owner admission can only succeed by
    // recognizing both validated sources as aliases of this exact allocation.
    assert_eq!(budget.used(), Ok(FONT.len()));
    let snapshot = ObjectSnapshot::project_entry(
        "font-alias".into(),
        retained,
        &budget,
        ObjectSnapshot::from_value,
    )
    .unwrap();
    let old_pin = snapshot.clone();
    drop((snapshot, source, bytes, permit));
    assert!(weak.upgrade().is_some());
    assert_eq!(budget.used(), Ok(FONT.len()));
    let records = old_pin
        .project()
        .unwrap()
        .asset_inventory()
        .unwrap()
        .records();
    assert!(Arc::ptr_eq(
        records[0].source().encoded_allocation().unwrap(),
        records[1].source().encoded_allocation().unwrap()
    ));
    assert_eq!(
        records[0].source().encoded_allocation().unwrap().as_slice(),
        FONT
    );
    drop(old_pin);
    assert!(weak.upgrade().is_none());
    assert_eq!(budget.used(), Ok(0));
}

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
fn image_placement_stays_refused_until_its_raster_owner_exists() {
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
    assert_eq!(
        pending_charges(&value, &budget).err(),
        Some("asset image placement unavailable")
    );
    assert_eq!(budget.used(), Ok(0));
}

#[test]
fn video_placement_on_an_existing_layer_is_admitted_and_an_unresolved_one_charges_nothing() {
    use crate::object_document::{AssetPlacement, AssetTarget};
    let base = project();
    let mut records = inventory(&base, true).records().to_vec();
    let clip = AssetId::new("clip".into()).unwrap();
    let source = AssetSource::linked("clips/take-1.mov".into(), 4096).unwrap();
    records.push(Arc::new(
        AssetRecord::new(clip.clone(), "take-1".into(), AssetKind::Video, source).unwrap(),
    ));
    // The fixture drawing has exactly one layer, `L`, in the implicit scene root.
    let place = |layer: &str| {
        let target = AssetTarget::Drawing {
            context_id: "scene-root".into(),
            layer_uid: layer.into(),
            stroke_id: None,
        };
        AssetPlacement::new("p".into(), clip.clone(), target, -3).unwrap()
    };
    let budget = EncodedRetentionBudget::new(1024).unwrap();
    let admitted = AssetInventory::new(records.clone(), vec![place("L")], 3, 1).unwrap();
    let admitted = base.with_asset_inventory(Some(admitted));
    // Reference + the two embedded PNG records; the linked video has no bytes.
    let tickets = pending_charges(&admitted, &budget).unwrap();
    assert_eq!(tickets.len(), 3);
    drop(tickets);
    assert_eq!(budget.used(), Ok(0));
    let missing = AssetInventory::new(records, vec![place("nope")], 3, 1).unwrap();
    let missing = base.with_asset_inventory(Some(missing));
    assert_eq!(
        pending_charges(&missing, &budget).err(),
        Some("asset placement layer unresolved")
    );
    assert_eq!(budget.used(), Ok(0));
}

#[test]
fn typed_embedded_kind_and_media_refuse_before_any_ticket_or_identity() {
    let base = project();
    let ProjectReferenceState::StillImage(reference) = base.reference_state() else {
        panic!()
    };
    let original_allocation = Arc::clone(reference.encoded_allocation());
    let original_metadata = *base.project();
    // These PNG sources really passed the existing PNG codec in project().
    // Changing AssetRecord.kind does not change that valid source precondition.
    for (kind, media) in [
        (AssetKind::Font, "image/png"),
        (AssetKind::Video, "image/png"),
        (AssetKind::Audio, "image/png"),
        // Structural unsupported media controls do not claim JPEG/font decoding.
        (AssetKind::Image, "image/jpeg"),
        (AssetKind::Font, "font/ttf"),
    ] {
        let source =
            AssetSource::admitted_embedded(media.into(), Arc::clone(&original_allocation), 1024)
                .unwrap();
        let record = AssetRecord::new(
            AssetId::new("typed-source".into()).unwrap(),
            "Typed source".into(),
            kind,
            source,
        )
        .unwrap();
        let inventory = AssetInventory::new(vec![Arc::new(record)], Vec::new(), 1, 0).unwrap();
        let project = base.with_asset_inventory(Some(inventory));
        let budget = EncodedRetentionBudget::new(1).unwrap();
        // Even the reference would exceed this quota if any ticket ran first.
        let result =
            ObjectSnapshot::project_entry("typed-refusal".into(), project, &budget, |_, _| {
                panic!("unsupported source cannot issue identity")
            });
        assert_eq!(result.err(), Some("asset media codec unavailable"));
        assert_eq!(budget.used(), Ok(0));
        assert_eq!(*base.project(), original_metadata);
        assert!(base.asset_inventory().is_none());
        let ProjectReferenceState::StillImage(unchanged) = base.reference_state() else {
            panic!()
        };
        assert!(Arc::ptr_eq(
            unchanged.encoded_allocation(),
            &original_allocation
        ));
    }
    let positive = base.with_asset_inventory(Some(inventory(&base, true)));
    let budget = EncodedRetentionBudget::new(original_allocation.capacity()).unwrap();
    let tickets = pending_charges(&positive, &budget).unwrap();
    assert_eq!(tickets.len(), 3);
    assert_eq!(budget.used(), Ok(original_allocation.capacity()));
    drop(tickets);
    assert_eq!(budget.used(), Ok(0));
}
