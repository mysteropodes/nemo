//! Real encoded-WAV codec, local existing ledger, owner-entry and placement controls.
//! Authored UNRUN. This child deliberately does NOT claim direct coverage of
//! object_snapshot's private managed_project_wire_using orchestration.
use super::*;
use crate::commands::ObjectFillOwner;
use crate::object_codec::{prepare_project_wire, ProjectWireError, ProjectWireLimits};
use crate::object_document::media_timing::object_media_link_edit::PlacementEdit;
use crate::object_document::{AssetId, AssetInventory, AssetKind, AssetRecord};
use crate::revision::ObjectSnapshot;
use base64::Engine;
use serde_json::{json, Value};
use std::{cell::Cell, sync::Arc};

#[path = "project_audio_retention_fixture.rs"]
mod fixture;
use fixture::{allocation, decode_reserved, enter, limits, wire, WAVE};

#[test]
fn real_wire_decoder_copies_only_into_reserved_capacity_and_retains_exact_validated_source() {
    let budget = EncodedRetentionBudget::new(WAVE.len()).unwrap();
    let copies = Cell::new(0);
    let project = decode_reserved(&wire(true), &budget, &copies).unwrap();
    assert_eq!(copies.get(), 1);
    let source = project.asset_inventory().unwrap().records()[0].source();
    assert!(source.has_audio_admission());
    assert_eq!(source.media_type(), Some("audio/wav"));
    let metadata = source.audio_metadata().unwrap();
    assert_eq!(metadata.channels, 2);
    assert_eq!(metadata.sample_rate, 48000);
    assert_eq!(metadata.bits_per_sample, 16);
    assert_eq!(metadata.block_align, 4);
    assert_eq!(metadata.frame_count, 2);
    assert_eq!(metadata.duration_seconds, 2.0 / 48000.0);
    assert_eq!(allocation(&project).as_slice(), WAVE);
    let cost = allocation(&project).capacity();
    assert_eq!(cost, WAVE.len());
    assert_eq!(budget.used(), Ok(cost));
    let original = Arc::clone(allocation(&project));
    let snapshot = enter(project, &budget); // Full ledger: must recognize bound Arc.
    assert!(Arc::ptr_eq(
        allocation(snapshot.project().unwrap()),
        &original
    ));
    assert_eq!(budget.used(), Ok(cost));
    assert!(budget.reserve(1).is_err());
    drop((snapshot, original));
    assert_eq!(budget.used(), Ok(0));
}

#[test]
fn validated_source_aliases_share_one_capacity_charge_until_last_snapshot_pin_drops() {
    let budget = EncodedRetentionBudget::new(WAVE.len()).unwrap();
    let base = decode_reserved(&wire(false), &budget, &Cell::new(0)).unwrap();
    let source = base.asset_inventory().unwrap().records()[0]
        .source()
        .clone();
    let weak = Arc::downgrade(allocation(&base));
    let records = ["wave-a", "wave-alias"]
        .into_iter()
        .map(|id| {
            Arc::new(
                AssetRecord::new(
                    AssetId::new(id.into()).unwrap(),
                    id.into(),
                    AssetKind::Audio,
                    source.clone(),
                )
                .unwrap(),
            )
        })
        .collect();
    let inventory = AssetInventory::new(records, Vec::new(), 2, 0).unwrap();
    let retained = base.with_asset_inventory(Some(inventory));
    let snapshot = enter(retained, &budget);
    let old_pin = snapshot.clone();
    drop((snapshot, source, base));
    assert!(weak.upgrade().is_some());
    assert_eq!(budget.used(), Ok(WAVE.len()));
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
        records[1].source().encoded_allocation().unwrap().as_slice(),
        WAVE
    );
    drop(old_pin);
    assert!(weak.upgrade().is_none());
    assert_eq!(budget.used(), Ok(0));
}

#[test]
fn real_fractional_placement_move_and_noop_do_not_recharge_or_replace_asset_records() {
    let budget = EncodedRetentionBudget::new(WAVE.len()).unwrap();
    let project = decode_reserved(&wire(true), &budget, &Cell::new(0)).unwrap();
    let weak = Arc::downgrade(allocation(&project));
    let snapshot = enter(project, &budget);
    let mut owner = ObjectFillOwner::from_admitted_snapshot(snapshot).unwrap();
    let old = owner.acquire_snapshot(0).unwrap();
    let edit = owner
        .prepare_placement_edit(PlacementEdit::ShiftOffset {
            placement_id: "p-a".into(),
            delta_frames: serde_json::Number::from_f64(1.25).unwrap(),
        })
        .unwrap();
    assert_eq!(owner.commit_placement_edit(edit), Ok(1));
    let current = owner.acquire_snapshot(1).unwrap();
    let old_inventory = old.project().unwrap().asset_inventory().unwrap();
    let current_inventory = current.project().unwrap().asset_inventory().unwrap();
    assert_eq!(
        old_inventory.placements()[0].offset_frames().as_f64(),
        Some(-0.5)
    );
    assert_eq!(
        current_inventory.placements()[0].offset_frames().as_f64(),
        Some(0.75)
    );
    assert!(Arc::ptr_eq(
        &old_inventory.records()[0],
        &current_inventory.records()[0]
    ));
    let noop = owner
        .prepare_placement_edit(PlacementEdit::SetOffset {
            placement_id: "p-a".into(),
            offset_frames: serde_json::Number::from_f64(0.75).unwrap(),
        })
        .unwrap();
    assert_eq!(owner.commit_placement_edit(noop), Ok(1));
    assert_eq!(owner.content_revision(), 1);
    assert_eq!(budget.used(), Ok(WAVE.len()));
    assert!(budget.reserve(1).is_err());
    drop((current, owner));
    assert!(weak.upgrade().is_some());
    assert_eq!(budget.used(), Ok(WAVE.len()));
    drop(old);
    assert!(weak.upgrade().is_none());
    assert_eq!(budget.used(), Ok(0));
}

#[test]
fn semantic_zero_noop_preserves_original_integer_spelling_and_capacity() {
    let mut value = wire(true);
    value["assetInventory"]["placements"][0]["offsetFrames"] = json!(0);
    let budget = EncodedRetentionBudget::new(WAVE.len()).unwrap();
    let project = decode_reserved(&value, &budget, &Cell::new(0)).unwrap();
    let mut owner = ObjectFillOwner::from_admitted_snapshot(enter(project, &budget)).unwrap();
    let noop = owner
        .prepare_placement_edit(PlacementEdit::SetOffset {
            placement_id: "p-a".into(),
            offset_frames: serde_json::Number::from_f64(0.0).unwrap(),
        })
        .unwrap();
    assert_eq!(owner.commit_placement_edit(noop), Ok(0));
    let current = owner.acquire_snapshot(0).unwrap();
    assert_eq!(
        current
            .project()
            .unwrap()
            .asset_inventory()
            .unwrap()
            .placements()[0]
            .offset_frames()
            .as_i64(),
        Some(0)
    );
    assert_eq!(budget.used(), Ok(WAVE.len()));
    drop((current, owner));
    assert_eq!(budget.used(), Ok(0));
}

#[test]
fn identity_refusal_releases_new_source_and_never_recharges_existing_snapshot() {
    let budget = EncodedRetentionBudget::new(WAVE.len()).unwrap();
    let project = decode_reserved(&wire(false), &budget, &Cell::new(0)).unwrap();
    let existing = enter(project.clone(), &budget);
    let called = Cell::new(false);
    let result = ObjectSnapshot::project_entry("refused".into(), project, &budget, |_, _| {
        called.set(true);
        Err("injected identity refusal")
    });
    assert!(result.is_err());
    assert!(called.get());
    assert_eq!(existing.content_revision(), 0);
    assert_eq!(allocation(existing.project().unwrap()).as_slice(), WAVE);
    assert_eq!(budget.used(), Ok(WAVE.len()));
    drop(existing);
    assert_eq!(budget.used(), Ok(0));
    let fresh = decode_reserved(&wire(false), &budget, &Cell::new(0)).unwrap();
    let weak = Arc::downgrade(allocation(&fresh));
    assert!(
        ObjectSnapshot::project_entry("fresh-refused".into(), fresh, &budget, |_, _| Err(
            "injected identity refusal"
        ))
        .is_err()
    );
    assert!(weak.upgrade().is_none());
    assert_eq!(budget.used(), Ok(0));
}

#[test]
fn unresolved_audio_target_refuses_before_owner_identity_and_drops_decoded_charge() {
    let mut value = wire(true);
    value["assetInventory"]["placements"][0]["target"]["trackUid"] = json!("missing");
    let budget = EncodedRetentionBudget::new(WAVE.len()).unwrap();
    let project = decode_reserved(&value, &budget, &Cell::new(0)).unwrap();
    let weak = Arc::downgrade(allocation(&project));
    let called = Cell::new(false);
    let result = ObjectSnapshot::project_entry("bad-target".into(), project, &budget, |_, _| {
        called.set(true);
        panic!("unresolved target must stop before identity");
    });
    assert_eq!(result.err(), Some("asset audio track unresolved"));
    assert!(!called.get());
    assert!(weak.upgrade().is_none());
    assert_eq!(budget.used(), Ok(0));
}

#[test]
fn aggregate_reservation_failure_precedes_all_encoded_allocations_and_rolls_back() {
    let mut value = wire(false);
    let mut second = value["assets"][0].clone();
    second["id"] = json!("wave-b");
    value["assets"].as_array_mut().unwrap().push(second);
    value["assetInventory"]["assetIds"] = json!(["wave-a", "wave-b"]);
    let budget = EncodedRetentionBudget::new(WAVE.len()).unwrap();
    let copies = Cell::new(0);
    assert_eq!(
        decode_reserved(&value, &budget, &copies).err(),
        Some(ProjectWireError::Budget)
    );
    assert_eq!(copies.get(), 0);
    assert_eq!(budget.used(), Ok(0));
}

#[test]
fn actual_malformed_wav_or_base64_cannot_become_an_admitted_source_and_old_pin_survives() {
    let budget = EncodedRetentionBudget::new(WAVE.len() * 2).unwrap();
    let original = decode_reserved(&wire(false), &budget, &Cell::new(0)).unwrap();
    let old = enter(original, &budget);
    for bad_base64 in [false, true] {
        let mut value = wire(false);
        let data = if bad_base64 {
            let mut encoded = base64::engine::general_purpose::STANDARD.encode(WAVE);
            encoded.replace_range(0..1, "!");
            encoded
        } else {
            base64::engine::general_purpose::STANDARD.encode([0u8; 52])
        };
        value["assets"][0]["source"]["data"] = json!(data);
        let copies = Cell::new(0);
        assert_eq!(
            decode_reserved(&value, &budget, &copies).err(),
            Some(ProjectWireError::Invalid)
        );
        assert_eq!(copies.get(), 1);
        assert_eq!(budget.used(), Ok(WAVE.len()));
        assert_eq!(allocation(old.project().unwrap()).as_slice(), WAVE);
    }
    drop(old);
    assert_eq!(budget.used(), Ok(0));
}

#[test]
fn unsupported_audio_mime_refuses_at_preparation_without_copy_or_retention() {
    let mut value = wire(false);
    value["assets"][0]["source"]["mediaType"] = json!("audio/mp3");
    let budget = EncodedRetentionBudget::new(WAVE.len()).unwrap();
    let copies = Cell::new(0);
    assert_eq!(
        decode_reserved(&value, &budget, &copies).err(),
        Some(ProjectWireError::Unsupported)
    );
    assert_eq!(copies.get(), 0);
    assert_eq!(budget.used(), Ok(0));
}

#[test]
fn real_allocator_capacity_mismatch_is_refused_before_wav_copy_and_unbound_quota_releases() {
    let bytes = serde_json::to_vec(&wire(false)).unwrap();
    let plan = prepare_project_wire(&bytes, limits()).unwrap();
    let budget = EncodedRetentionBudget::new(128).unwrap();
    let mut permit = budget.reserve(128).unwrap();
    let result = plan.decode(
        |length| {
            let mut real = permit.allocate().map_err(|_| ProjectWireError::Budget)?;
            assert_eq!(length, 52);
            assert_eq!(real.capacity(), 128);
            real.truncate(length);
            Ok(real)
        },
        |_, _| panic!("no PNG source"),
    );
    assert_eq!(result.err(), Some(ProjectWireError::Budget));
    assert_eq!(budget.used(), Ok(128));
    drop(permit);
    assert_eq!(budget.used(), Ok(0));
}

#[test]
fn owner_rejects_a_validated_audio_allocation_under_a_forged_catalog_kind() {
    let budget = EncodedRetentionBudget::new(WAVE.len()).unwrap();
    let base = decode_reserved(&wire(false), &budget, &Cell::new(0)).unwrap();
    let original = base.asset_inventory().unwrap().records()[0]
        .source()
        .clone();
    let forged = Arc::new(
        AssetRecord::new(
            AssetId::new("wave-a".into()).unwrap(),
            "wrong kind".into(),
            AssetKind::Image,
            original,
        )
        .unwrap(),
    );
    let inventory = AssetInventory::new(vec![forged], Vec::new(), 1, 0).unwrap();
    let value = base.with_asset_inventory(Some(inventory));
    drop(base);
    let called = Cell::new(false);
    let result = ObjectSnapshot::project_entry("wrong-kind".into(), value, &budget, |_, _| {
        called.set(true);
        panic!("codec-kind mismatch must stop before identity");
    });
    assert_eq!(result.err(), Some("asset media codec unavailable"));
    assert!(!called.get());
    assert_eq!(budget.used(), Ok(0));
}

#[test]
fn sealed_placement_wrong_instance_or_document_refuses_without_quota_or_revision_change() {
    for same_instance in [false, true] {
        let budget = EncodedRetentionBudget::new(WAVE.len()).unwrap();
        let project = decode_reserved(&wire(true), &budget, &Cell::new(0)).unwrap();
        let first = ObjectSnapshot::project_entry(
            "first".into(),
            project.clone(),
            &budget,
            ObjectSnapshot::from_value,
        )
        .unwrap();
        let second = ObjectSnapshot::project_entry(
            if same_instance { "first" } else { "second" }.into(),
            project,
            &budget,
            ObjectSnapshot::from_value,
        )
        .unwrap();
        let owner = ObjectFillOwner::from_admitted_snapshot(first).unwrap();
        let mut other = ObjectFillOwner::from_admitted_snapshot(second).unwrap();
        let candidate = owner
            .prepare_placement_edit(PlacementEdit::ShiftOffset {
                placement_id: "p-a".into(),
                delta_frames: serde_json::Number::from(1),
            })
            .unwrap();
        let expected = if same_instance {
            crate::commands::DispatchErrorCode::WrongDocument
        } else {
            crate::commands::DispatchErrorCode::WrongInstance
        };
        assert_eq!(other.commit_placement_edit(candidate), Err(expected));
        assert_eq!(owner.content_revision(), 0);
        assert_eq!(other.content_revision(), 0);
        assert_eq!(budget.used(), Ok(WAVE.len()));
        assert_eq!(
            allocation(other.acquire_snapshot(0).unwrap().project().unwrap()).as_slice(),
            WAVE
        );
        drop((owner, other));
        assert_eq!(budget.used(), Ok(0));
    }
}

#[test]
fn dropped_preparation_and_stale_commit_do_not_recharge_the_same_source() {
    let budget = EncodedRetentionBudget::new(WAVE.len()).unwrap();
    let project = decode_reserved(&wire(true), &budget, &Cell::new(0)).unwrap();
    let mut owner = ObjectFillOwner::from_admitted_snapshot(enter(project, &budget)).unwrap();
    let shifted = || PlacementEdit::ShiftOffset {
        placement_id: "p-a".into(),
        delta_frames: serde_json::Number::from(1),
    };
    let cancelled = owner.prepare_placement_edit(shifted()).unwrap();
    drop(cancelled);
    assert_eq!(owner.content_revision(), 0);
    assert_eq!(budget.used(), Ok(WAVE.len()));
    let stale = owner.prepare_placement_edit(shifted()).unwrap();
    let accepted = owner.prepare_placement_edit(shifted()).unwrap();
    assert_eq!(owner.commit_placement_edit(accepted), Ok(1));
    assert_eq!(
        owner.commit_placement_edit(stale),
        Err(crate::commands::DispatchErrorCode::StaleRevision)
    );
    assert_eq!(owner.content_revision(), 1);
    assert_eq!(budget.used(), Ok(WAVE.len()));
    drop(owner);
    assert_eq!(budget.used(), Ok(0));
}

#[test]
fn legacy_no_catalog_refuses_dangling_audio_before_any_allocation() {
    for tracks in [
        None,
        Some(json!([])),
        Some(json!([{"assetId":"missing","audioId":"track-a"}])),
    ] {
        let dangling = tracks
            .as_ref()
            .is_some_and(|v| !v.as_array().unwrap().is_empty());
        let mut value = wire(false);
        value["formatVersion"] = json!(1);
        value["assets"] = json!([]);
        value.as_object_mut().unwrap().remove("assetInventory");
        if let Some(tracks) = tracks {
            value["drawing"]["audioTracks"] = tracks;
        }
        let prepared = prepare_project_wire(value.to_string().as_bytes(), limits());
        if dangling {
            assert_eq!(prepared.err(), Some(ProjectWireError::Invalid));
        } else {
            let prepared = prepared.unwrap();
            assert_eq!(prepared.encoded_lengths().count(), 0);
            let decoded = prepared
                .decode(
                    |_| panic!("no encoded source can allocate"),
                    |_, _| panic!("no viewport PNG can decode"),
                )
                .unwrap();
            let budget = EncodedRetentionBudget::new(1).unwrap();
            let snapshot = enter(decoded.into_project(), &budget);
            assert!(snapshot.project().unwrap().asset_inventory().is_none());
            assert_eq!(budget.used(), Ok(0));
        }
    }
}

#[test]
fn typed_no_catalog_audio_source_join_runs_before_reservation_and_identity() {
    for tracks in [
        None,
        Some(json!([])),
        Some(json!([{"assetId":"missing","audioId":"track-a"}])),
    ] {
        let dangling = tracks
            .as_ref()
            .is_some_and(|v| !v.as_array().unwrap().is_empty());
        let mut drawing = wire(false)["drawing"].clone();
        if let Some(tracks) = tracks {
            drawing["audioTracks"] = tracks;
        }
        let drawing =
            crate::object_codec::decode_storage_project(drawing.to_string().as_bytes()).unwrap();
        let project = ProjectEnvelope::no_media(
            crate::object_document::ProjectMetadata::new(320, 180, 24.).unwrap(),
            drawing,
        );
        let budget = EncodedRetentionBudget::new(1).unwrap();
        let identity_called = Cell::new(false);
        let result = ObjectSnapshot::project_entry(
            "no-catalog-audio".into(),
            project,
            &budget,
            |instance, drawing| {
                identity_called.set(true);
                ObjectSnapshot::from_value(instance, drawing)
            },
        );
        if dangling {
            assert_eq!(result.err(), Some("asset placement asset unresolved"));
            assert!(!identity_called.get());
        } else {
            let snapshot = result.unwrap();
            assert!(identity_called.get());
            assert!(snapshot.project().unwrap().asset_inventory().is_none());
        }
        assert_eq!(budget.used(), Ok(0));
    }
}
