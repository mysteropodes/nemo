//! Negative controls use the actual project, provider, planner and dispatcher.
//! A refusal or abandoned candidate must preserve the entire saved envelope and redo.
use super::asset_tests::{new_png, png_batch, project_bytes, project_history, raster, text};
use super::{
    ImportAsset, ImportBatch, ImportError, ImportRaster, ImportRecord, ImportSkip, SkipKind,
};
use crate::object_document::{AssetId, AssetKind, AssetRecord, AssetSource};
use crate::request_receipts::DispatchErrorCode;
use serde_json::json;
use std::sync::Arc;

fn frozen(owner: &crate::history::NativeObjectHistory) -> (u64, Vec<u8>, (usize, usize)) {
    (
        owner.content_revision(),
        project_bytes(owner),
        owner.history_depths(),
    )
}

fn with_redo() -> crate::history::NativeObjectHistory {
    let mut owner = project_history(2);
    super::support::prime_redo(&mut owner);
    owner
}

#[test]
fn missing_project_refuses_new_resources_and_items_but_keeps_the_original_vector_path() {
    let bare = super::support::history(1);
    let genuine = project_history(1);
    for batch in [
        ImportBatch {
            assets: vec![new_png(&genuine, "pixels", "bare")],
            ..ImportBatch::default()
        },
        ImportBatch {
            raster: vec![raster("bitmap", "L1", 0, "pixels")],
            ..ImportBatch::default()
        },
        ImportBatch {
            text: vec![text(
                "caption",
                "L1",
                json!({"kind":"bundled","fontKey":"Roboto-Regular"}),
            )],
            ..ImportBatch::default()
        },
    ] {
        assert_eq!(
            bare.prepare_import_batch(&batch).unwrap_err(),
            ImportError::ProjectRequired
        );
        assert_eq!(bare.content_revision(), 0);
        assert_eq!(bare.history_depths(), (0, 0));
    }
    assert!(bare
        .prepare_import_batch(&super::support::mixed_batch())
        .is_ok());
}

#[test]
fn dangling_skipped_wrong_kind_and_foreign_targets_refuse_without_touching_a_live_redo() {
    let owner = with_redo();
    let before = frozen(&owner);
    let mut cases = Vec::new();
    let mut missing = png_batch(&owner);
    missing.raster[0].entry = raster("bitmap", "page", 3, "missing").entry;
    cases.push(missing);
    let mut skipped = png_batch(&owner);
    skipped.assets.clear();
    skipped.skipped.push(ImportSkip {
        kind: SkipKind::Asset,
        source_id: "pixels".into(),
        reason: "unsupported".into(),
    });
    cases.push(skipped);
    let mut wrong_kind = png_batch(&owner);
    wrong_kind.text[0] = text(
        "caption",
        "page",
        json!({"kind":"asset","assetId":"pixels"}),
    );
    cases.push(wrong_kind);
    let mut bad_layer = png_batch(&owner);
    bad_layer.raster[0].entry.target.layer_uid = "absent".into();
    cases.push(bad_layer);
    let mut preset_id = png_batch(&owner);
    preset_id.text[0].entry.target.stroke_id = "chosen-by-importer".into();
    cases.push(preset_id);
    let mut foreign = png_batch(&owner);
    foreign.text[0].entry.target.context_id = "foreign".into();
    cases.push(foreign);
    for batch in cases {
        assert!(owner.prepare_import_batch(&batch).is_err());
        assert_eq!(frozen(&owner), before);
        assert_eq!(owner.history_depths(), (0, 1));
    }
}

#[test]
fn a_font_mime_label_on_generic_bytes_never_substitutes_for_native_font_admission() {
    let owner = with_redo();
    let before = frozen(&owner);
    let admitted_png = new_png(&owner, "image", "negative-font");
    let forged = AssetRecord::new(
        AssetId::new("not-a-font".into()).unwrap(),
        "Not admitted".into(),
        AssetKind::Font,
        admitted_png.record.source().clone(),
    )
    .unwrap();
    let batch = ImportBatch {
        assets: vec![ImportAsset {
            source_id: "font".into(),
            record: Arc::new(forged),
        }],
        text: vec![text(
            "caption",
            "L1",
            json!({"kind":"asset","assetId":"font"}),
        )],
        ..ImportBatch::default()
    };
    assert!(matches!(
        owner.prepare_import_batch(&batch),
        Err(ImportError::ProviderUnavailable { .. })
    ));
    assert_eq!(frozen(&owner), before);
}

#[test]
fn malformed_png_is_refused_by_the_actual_codec_without_an_encoded_copy_or_publication() {
    let owner = with_redo();
    let before = frozen(&owner);
    // Deliberately invalid internal value, never used as a positive provider.
    let bytes = Arc::new(vec![1, 2, 3]);
    let source =
        AssetSource::admitted_embedded("image/png".into(), Arc::clone(&bytes), bytes.capacity())
            .unwrap();
    let record = AssetRecord::new(
        AssetId::new("bad".into()).unwrap(),
        "bad.png".into(),
        AssetKind::Image,
        source,
    )
    .unwrap();
    let batch = ImportBatch {
        assets: vec![ImportAsset {
            source_id: "bad".into(),
            record: Arc::new(record),
        }],
        ..ImportBatch::default()
    };
    assert_eq!(
        owner.prepare_import_batch(&batch).unwrap_err(),
        ImportError::InvalidAsset { index: 0 }
    );
    assert_eq!(frozen(&owner), before);
}

#[test]
fn cancellation_noop_and_stale_or_foreign_commit_keep_complete_project_and_redo() {
    let mut owner = with_redo();
    let before = frozen(&owner);
    let first = owner.prepare_import_batch(&png_batch(&owner)).unwrap();
    let ids = first.result().clone();
    drop(first);
    assert_eq!(frozen(&owner), before);
    let second = owner.prepare_import_batch(&png_batch(&owner)).unwrap();
    assert_eq!(
        *second.result(),
        ids,
        "abandoned preparation consumes no ids"
    );
    drop(second);
    let noop = owner.prepare_import_batch(&ImportBatch::default()).unwrap();
    assert!(!noop.changed());
    owner.commit_import_batch(noop).unwrap();
    assert_eq!(frozen(&owner), before);
    let stale = owner.prepare_import_batch(&png_batch(&owner)).unwrap();
    owner.redo(owner.content_revision()).unwrap();
    let after_redo = frozen(&owner);
    assert_eq!(
        owner.commit_import_batch(stale).unwrap_err(),
        DispatchErrorCode::StaleRevision
    );
    assert_eq!(frozen(&owner), after_redo);
    let other = project_history(2);
    let foreign = other.prepare_import_batch(&png_batch(&other)).unwrap();
    assert_eq!(
        owner.commit_import_batch(foreign).unwrap_err(),
        DispatchErrorCode::WrongDocument
    );
    assert_eq!(frozen(&owner), after_redo);
}

#[test]
fn all_native_id_kinds_are_reserved_against_existing_items_and_catalog_records() {
    let mut owner = project_history(2);
    let prepared = owner.prepare_import_batch(&png_batch(&owner)).unwrap();
    let (_, receipt) = owner.commit_import_batch(prepared).unwrap();
    let ids = super::origin_ids(&owner.acquire_snapshot(1).unwrap());
    use crate::history::object_history::object_shot_kernels::object_import_remap_ids::RemapKind;
    assert!(ids.contains(RemapKind::Asset, &receipt.assets[0].1));
    assert!(ids.contains(RemapKind::Stroke, &receipt.objects[1].1));
    assert!(ids.contains(RemapKind::Stroke, &receipt.objects[2].1));
    let before = frozen(&owner);
    let duplicate = ImportBatch {
        raster: vec![raster("duplicate", "L1", 0, &receipt.assets[0].1)],
        text: vec![text(
            "duplicate",
            "L1",
            json!({"kind":"bundled","fontKey":"Roboto-Regular"}),
        )],
        ..ImportBatch::default()
    };
    assert!(matches!(
        owner.prepare_import_batch(&duplicate),
        Err(ImportError::Remap(_))
    ));
    assert_eq!(frozen(&owner), before);
}

#[test]
fn wire_new_bytes_duplicates_null_unknowns_and_payload_overflow_are_not_silently_accepted() {
    use super::wire::{parse_batch, parse_batch_at};
    let owner = project_history(1);
    let origin = owner.acquire_snapshot(0).unwrap();
    for bytes in [
        br#"{"command":"object.import.batch","layers":[],"records":[],"assets":null}"#.as_slice(),
        br#"{"command":"object.import.batch","layers":[],"records":[],"raster":[],"raster":[]}"#.as_slice(),
        br#"{"command":"object.import.batch","layers":[],"records":[],"assets":[{"sourceId":"a","assetId":"old","data":"raw"}]}"#.as_slice(),
        br#"{"command":"object.import.batch","layers":[],"records":[],"text":[{"sourceId":"a","entry":null}]}"#.as_slice(),
    ] {
        assert_eq!(parse_batch_at(&origin, bytes).unwrap_err(), ImportError::Payload);
    }
    assert_eq!(
        parse_batch_at(&origin, &vec![b' '; 4097]).unwrap_err(),
        ImportError::Payload
    );
    let reuse = br#"{"command":"object.import.batch","layers":[],"records":[],"assets":[{"sourceId":"a","assetId":"old"}]}"#;
    assert_eq!(
        parse_batch(reuse).unwrap_err(),
        ImportError::ProjectRequired
    );
    assert!(parse_batch(br#"{"command":"object.import.batch","layers":[],"records":[]}"#).is_ok());
    assert_eq!(owner.content_revision(), 0);
}

#[test]
fn origin_aware_reuse_still_obeys_dispatch_cancel_stale_and_compact_receipt_limits() {
    let mut owner = project_history(1);
    let prepared = owner
        .prepare_import_batch(&ImportBatch {
            assets: vec![new_png(&owner, "pixels", "retained")],
            ..ImportBatch::default()
        })
        .unwrap();
    let (_, receipt) = owner.commit_import_batch(prepared).unwrap();
    let asset_id = &receipt.assets[0].1;
    let payload = json!({"command":"object.import.batch","layers":[],"records":[],
        "assets":[{"sourceId":"copy","assetId":asset_id}]});
    let before = frozen(&owner);
    let mut cancelled = super::support::request(&owner, "cancel-reuse", payload.clone());
    cancelled["cancelledBeforeDispatch"] = json!(true);
    assert_eq!(
        super::support::dispatch(&mut owner, &cancelled)["error"]["code"],
        "cancelled_before_dispatch"
    );
    assert_eq!(frozen(&owner), before);
    let mut stale = super::support::request(&owner, "stale-reuse", payload);
    stale["expectedRevision"] = json!(0);
    assert_eq!(
        super::support::dispatch(&mut owner, &stale)["error"]["code"],
        "stale_revision"
    );
    assert_eq!(frozen(&owner), before);
    let assets: Vec<_> = (0..40).map(|i| json!({
        "sourceId":format!("source-asset-with-long-importer-label-number-{i:02}"),"assetId":asset_id
    })).collect();
    let request = super::support::request(
        &owner,
        "large-receipt",
        json!({
            "command":"object.import.batch","layers":[],"records":[],"assets":assets
        }),
    );
    assert!(serde_json::to_vec(&request).unwrap().len() <= 4096);
    let response = super::support::dispatch(&mut owner, &request);
    assert_eq!(response["ok"], false);
    assert_eq!(response["error"]["code"], "invalid_request");
    assert_eq!(
        frozen(&owner),
        before,
        "oversized receipt refuses before publication"
    );
}

#[test]
fn production_source_failure_after_a_real_png_preparation_keeps_history_redo_and_catalog() {
    use super::session::ImportAssetSource;
    let owner = with_redo();
    let before = frozen(&owner);
    let genuine = new_png(&owner, "fixture", "source-fixture");
    let png = genuine.record.source().encoded_allocation().unwrap();
    let batch = ImportBatch {
        raster: vec![raster("bitmap", "L1", 0, "pixels")],
        ..ImportBatch::default()
    };
    let error = owner
        .prepare_import_batch_sources(
            "later-bad",
            &batch,
            &[
                ImportAssetSource {
                    source_id: "pixels",
                    name: "blue.png",
                    media_type: "image/png",
                    bytes: png,
                },
                ImportAssetSource {
                    source_id: "broken",
                    name: "broken.png",
                    media_type: "image/png",
                    bytes: b"not a PNG",
                },
            ],
        )
        .unwrap_err();
    assert_eq!(error.code(), DispatchErrorCode::InvalidRequest);
    assert_eq!(frozen(&owner), before);
    assert!(owner
        .acquire_snapshot(owner.content_revision())
        .unwrap()
        .project()
        .unwrap()
        .asset_inventory()
        .is_none());
    let first = owner
        .prepare_import_batch_sources(
            "same-origin",
            &batch,
            &[ImportAssetSource {
                source_id: "pixels",
                name: "blue.png",
                media_type: "image/png",
                bytes: png,
            }],
        )
        .unwrap();
    let ids = first.result().clone();
    drop(first);
    let second = owner
        .prepare_import_batch_sources(
            "same-origin",
            &batch,
            &[ImportAssetSource {
                source_id: "pixels",
                name: "blue.png",
                media_type: "image/png",
                bytes: png,
            }],
        )
        .unwrap();
    assert_eq!(*second.result(), ids);
    drop(second);
    assert_eq!(
        frozen(&owner),
        before,
        "failed/cancelled sources cannot publish resources or consume ids"
    );
}

/// Requires the exact T13 bitmap fields and Root shared compound guard union.
/// This is an import-route refusal control, not bitmap import acceptance.
#[test]
fn project_item_import_cannot_bypass_the_shared_compound_refusal_or_consume_redo() {
    use crate::object_document::BitmapBrushFields;
    let mut owner = project_history(2);
    let resource = new_png(&owner, "pixels", "compound-retained-png");
    let allocation = Arc::clone(resource.record.source().encoded_allocation().unwrap());
    let retained = owner
        .prepare_import_batch(&ImportBatch {
            assets: vec![resource],
            ..ImportBatch::default()
        })
        .unwrap();
    let (_, receipt) = owner.commit_import_batch(retained).unwrap();
    let asset_id = AssetId::new(receipt.assets[0].1.clone()).unwrap();
    let visibility = owner
        .prepare_layer_visibility_json(
            br#"{"command":"object.layer.visibility.set","layerUid":"L1","visible":false}"#,
        )
        .unwrap();
    assert_eq!(owner.commit_layer_visibility(visibility), Ok(2));
    let changed = project_bytes(&owner);
    assert_eq!(owner.undo(2), Ok(3));
    assert_eq!(owner.history_depths(), (1, 1));
    let before = frozen(&owner);
    let origin = owner.acquire_snapshot(3).unwrap();
    let catalog_record = Arc::clone(
        origin
            .project()
            .unwrap()
            .asset_inventory()
            .unwrap()
            .get(&asset_id)
            .unwrap(),
    );
    assert!(origin.document().item_records().is_none());
    assert!(origin.document().objects().iter().all(|record| {
        record
            .vector_ext
            .value()
            .and_then(|ext| ext.bitmap_brush.value())
            .is_none()
    }));
    let source = BitmapBrushFields::test_drawing();
    let mut anchor = source["objects"][0].clone();
    anchor["target"]["layerUid"] = json!("L1");
    anchor["target"]["strokeId"] = json!("");
    let mut companion = source["items"]["raster"][0].clone();
    companion["target"]["layerUid"] = json!("L1");
    companion["target"]["strokeId"] = json!("");
    companion["item"]["assetId"] = json!(asset_id.as_str());
    let batch = ImportBatch {
        records: vec![ImportRecord {
            source_id: "imported-anchor".into(),
            record: serde_json::from_value(anchor).unwrap(),
        }],
        raster: vec![ImportRaster {
            source_id: "imported-companion".into(),
            entry: serde_json::from_value(companion).unwrap(),
        }],
        ..ImportBatch::default()
    };
    let prepared = owner.prepare_import_batch(&batch).unwrap();
    assert!(
        prepared.changed(),
        "valid preparation must reach publication"
    );
    assert_eq!(prepared.result().objects.len(), 2);
    assert!(
        prepared.result().assets.is_empty(),
        "the PNG is genuinely retained"
    );
    assert_eq!(frozen(&owner), before, "preparation does not publish");
    assert_eq!(
        owner.commit_import_batch(prepared),
        Err(DispatchErrorCode::Unavailable)
    );
    assert_eq!(
        frozen(&owner),
        before,
        "refusal preserves revision, full save, history and redo"
    );
    assert_eq!(origin.serialize_project_file().unwrap(), before.1);
    let current = owner.acquire_snapshot(3).unwrap();
    let current_record = current
        .project()
        .unwrap()
        .asset_inventory()
        .unwrap()
        .get(&asset_id)
        .unwrap();
    assert!(Arc::ptr_eq(current_record, &catalog_record));
    assert!(Arc::ptr_eq(
        current_record.source().encoded_allocation().unwrap(),
        &allocation
    ));
    assert_eq!(
        owner.redo(3),
        Ok(4),
        "the failed import cannot consume redo"
    );
    assert_eq!(project_bytes(&owner), changed);
}
