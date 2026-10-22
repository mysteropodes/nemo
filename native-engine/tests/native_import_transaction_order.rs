//! Explicit import stack order through the real planner, owner and native consumers.
use super::support::*;
use super::{ImportBatch, ImportError};
use crate::history::NativeObjectHistory;
use crate::request_receipts::DispatchErrorCode;
use serde_json::{json, Value};

fn vectors() -> ImportBatch {
    ImportBatch {
        records: vec![item("back", "L0", 0, 0), item("front", "L0", 0, 0)],
        ..ImportBatch::default()
    }
}

fn positions(value: &Value, layer: &str) -> Vec<(String, u64)> {
    value["compositionOrder"]["contexts"][0]["scopes"][0]["elements"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["layerUid"] == layer)
        .map(|row| {
            (
                row["strokeId"].as_str().unwrap().to_owned(),
                row["naturalPosition"].as_u64().unwrap(),
            )
        })
        .collect()
}

#[test]
fn absent_order_keeps_v1_bytes_and_present_v2_changes_only_new_natural_positions() {
    let mut legacy = history(1);
    let prepared = legacy.prepare_import_batch(&vectors()).unwrap();
    legacy.commit_import_batch(prepared).unwrap();
    assert_eq!(state(&legacy)["formatVersion"], 1);
    assert!(state(&legacy).get("compositionOrder").is_none());
    for explicit in [false, true] {
        let mut owner = history(2);
        let before = state(&owner);
        let mut batch = vectors();
        if explicit {
            batch.object_order = Some(vec!["front".into(), "back".into()]);
        }
        let prepared = owner.prepare_import_batch(&batch).unwrap();
        let canonical = prepared.result().objects.clone();
        assert_eq!(
            canonical
                .iter()
                .map(|row| row.0.as_str())
                .collect::<Vec<_>>(),
            ["back", "front"]
        );
        owner.commit_import_batch(prepared).unwrap();
        let after = state(&owner);
        assert_eq!(after["layers"], before["layers"]);
        assert_eq!(after["objects"][0], before["objects"][0]);
        assert_eq!(
            positions(&after, "L0"),
            vec![
                ("s0".into(), 0),
                (canonical[0].1.clone(), if explicit { 2 } else { 1 }),
                (canonical[1].1.clone(), if explicit { 1 } else { 2 }),
            ]
        );
        assert_eq!(owner.history_depths(), (1, 0));
        let saved = bytes(&owner);
        owner.undo(1).unwrap();
        assert_eq!(state(&owner), before);
        owner.redo(2).unwrap();
        assert_eq!(bytes(&owner), saved);
    }
}

#[test]
fn object_order_rejects_partial_duplicate_foreign_and_non_object_ids_without_consuming_redo() {
    let mut owner = history(2);
    prime_redo(&mut owner);
    let before = frozen(&owner);
    for order in [
        vec![],
        vec!["back"],
        vec!["back", "back"],
        vec!["back", "front", "extra"],
        vec!["back", "L0"],
        vec!["back", "s0"],
        vec!["back", "skipped"],
    ] {
        let mut batch = vectors();
        batch.object_order = Some(order.into_iter().map(str::to_owned).collect());
        assert_eq!(
            owner.prepare_import_batch(&batch).unwrap_err(),
            ImportError::Payload
        );
        assert_eq!(frozen(&owner), before);
    }
    let mut noop = ImportBatch::default();
    noop.object_order = Some(Vec::new());
    let prepared = owner.prepare_import_batch(&noop).unwrap();
    assert!(!prepared.changed());
    owner.commit_import_batch(prepared).unwrap();
    assert_eq!(frozen(&owner), before);
}

#[cfg(not(feature = "project_media_codec"))]
#[test]
fn explicit_v1_migration_is_unavailable_without_existing_project_media_read_port() {
    let owner = history(1);
    let before = frozen(&owner);
    let mut batch = vectors();
    batch.object_order = Some(vec!["front".into(), "back".into()]);
    assert_eq!(
        owner.prepare_import_batch(&batch).unwrap_err(),
        ImportError::Dispatch(DispatchErrorCode::Unavailable)
    );
    assert_eq!(frozen(&owner), before);
}

#[cfg(feature = "project_media_codec")]
#[test]
fn empty_object_order_keeps_asset_only_import_in_its_original_format() {
    let mut owner = super::asset_tests::project_history(1);
    let before: Value = serde_json::from_slice(&super::asset_tests::project_bytes(&owner)).unwrap();
    let batch = ImportBatch {
        assets: vec![super::asset_tests::new_png(
            &owner,
            "pixels",
            "asset-only-order",
        )],
        object_order: Some(Vec::new()),
        ..ImportBatch::default()
    };
    let prepared = owner.prepare_import_batch(&batch).unwrap();
    assert!(prepared.result().objects.is_empty());
    owner.commit_import_batch(prepared).unwrap();
    let after: Value = serde_json::from_slice(&super::asset_tests::project_bytes(&owner)).unwrap();
    assert_eq!(after["drawing"], before["drawing"]);
    assert_eq!(owner.history_depths(), (1, 0));
}

#[cfg(feature = "project_media_codec")]
fn mixed(owner: &NativeObjectHistory) -> ImportBatch {
    let mut batch = super::asset_tests::png_batch(owner);
    batch.layers.clear();
    batch.records = vec![item("vector", "L0", 0, 0)];
    batch.raster = vec![super::asset_tests::raster("bitmap", "L0", 0, "pixels")];
    batch.text = vec![super::asset_tests::text(
        "caption",
        "L0",
        json!({"kind":"bundled","fontKey":"Roboto-Regular"}),
    )];
    let mut raster = serde_json::to_value(&batch.raster[0].entry).unwrap();
    raster["item"]["x"] = json!(0);
    raster["item"]["y"] = json!(0);
    raster["item"]["width"] = json!(100);
    raster["item"]["height"] = json!(100);
    batch.raster[0].entry = serde_json::from_value(raster).unwrap();
    let mut caption = serde_json::to_value(&batch.text[0].entry).unwrap();
    caption["block"]["anchorTopLeft"] = json!({"x":0,"y":0});
    batch.text[0].entry = serde_json::from_value(caption).unwrap();
    batch.object_order = Some(vec!["caption".into(), "vector".into(), "bitmap".into()]);
    batch
}

#[cfg(all(feature = "project_media_codec", feature = "evaluation"))]
fn consumer_order(owner: &NativeObjectHistory) -> Vec<usize> {
    use crate::object_document::FrameScopeKind;
    use crate::object_frame_packet::{prepare_object_frame, ObjectFrameSelector};
    let snapshot = owner.acquire_snapshot(owner.content_revision()).unwrap();
    let selector = ObjectFrameSelector {
        instance_id: snapshot.instance_id(),
        document_id: snapshot.document_id(),
        content_revision: snapshot.content_revision(),
        document_snapshot_id: snapshot.snapshot_id(),
        context_id: "scene-root",
        scope_kind: FrameScopeKind::Authored,
        frame: 0,
    };
    let packet = prepare_object_frame(&snapshot, &selector).unwrap();
    packet
        .numeric_order()
        .unwrap()
        .layers()
        .iter()
        .find(|layer| layer.source_layer_index() == 0)
        .unwrap()
        .elements()
        .iter()
        .map(|element| element.source_object_index())
        .collect()
}

#[cfg(all(feature = "project_media_codec", feature = "evaluation"))]
#[test]
fn overlapping_mixed_import_migrates_only_explicitly_and_preserves_owner_history_save_and_consumer_order(
) {
    for version in [1, 2] {
        let mut owner = super::asset_tests::project_history(version);
        let before = super::asset_tests::project_bytes(&owner);
        let before_value: Value = serde_json::from_slice(&before).unwrap();
        let old_pin = owner.acquire_snapshot(0).unwrap();
        let batch = mixed(&owner);
        let prepared = owner.prepare_import_batch(&batch).unwrap();
        assert_eq!(super::asset_tests::project_bytes(&owner), before);
        let receipt = prepared.result().clone();
        assert_eq!(
            receipt
                .objects
                .iter()
                .map(|row| row.0.as_str())
                .collect::<Vec<_>>(),
            ["vector", "bitmap", "caption"]
        );
        let stale = owner.prepare_import_batch(&batch).unwrap();
        owner.commit_import_batch(prepared).unwrap();
        assert_eq!(owner.history_depths(), (1, 0));
        assert_eq!(consumer_order(&owner), [0, 3, 1, 2]);
        let saved = super::asset_tests::project_bytes(&owner);
        let after: Value = serde_json::from_slice(&saved).unwrap();
        assert_eq!(after["project"], before_value["project"]);
        assert_eq!(
            after["drawing"]["layers"],
            before_value["drawing"]["layers"]
        );
        assert_eq!(
            after["drawing"]["objects"][0],
            before_value["drawing"]["objects"][0]
        );
        assert_eq!(after["drawing"]["formatVersion"], 2);
        assert_eq!(old_pin.document().format_version(), version);
        assert_eq!(
            owner.commit_import_batch(stale).unwrap_err(),
            DispatchErrorCode::StaleRevision
        );
        let reopened = NativeObjectHistory::from_project_file("ordered-reopen", &saved).unwrap();
        assert_eq!(consumer_order(&reopened), [0, 3, 1, 2]);
        assert_eq!(super::asset_tests::project_bytes(&reopened), saved);
        owner.undo(1).unwrap();
        assert_eq!(super::asset_tests::project_bytes(&owner), before);
        owner.redo(2).unwrap();
        assert_eq!(super::asset_tests::project_bytes(&owner), saved);
        assert_eq!(consumer_order(&owner), [0, 3, 1, 2]);
    }
}

#[cfg(all(feature = "project_media_codec", feature = "evaluation"))]
#[test]
fn v1_migration_keeps_existing_mixed_natural_stack_before_the_new_objects() {
    let mut owner = super::asset_tests::project_history(1);
    let mut old = mixed(&owner);
    old.object_order = None;
    let prepared = owner.prepare_import_batch(&old).unwrap();
    owner.commit_import_batch(prepared).unwrap();
    let before = super::asset_tests::project_bytes(&owner);
    let origin = owner.acquire_snapshot(1).unwrap();
    let record = origin
        .project()
        .unwrap()
        .asset_inventory()
        .unwrap()
        .records()[0]
        .clone();
    let mut batch = vectors();
    batch.object_order = Some(vec!["front".into(), "back".into()]);
    let prepared = owner.prepare_import_batch(&batch).unwrap();
    assert_eq!(super::asset_tests::project_bytes(&owner), before);
    owner.commit_import_batch(prepared).unwrap();
    assert_eq!(owner.history_depths(), (2, 0));
    assert_eq!(consumer_order(&owner), [0, 1, 4, 5, 3, 2]);
    let after = owner.acquire_snapshot(2).unwrap();
    assert!(std::sync::Arc::ptr_eq(
        &record,
        &after
            .project()
            .unwrap()
            .asset_inventory()
            .unwrap()
            .records()[0]
    ));
    let saved = super::asset_tests::project_bytes(&owner);
    owner.undo(2).unwrap();
    assert_eq!(super::asset_tests::project_bytes(&owner), before);
    owner.redo(3).unwrap();
    assert_eq!(super::asset_tests::project_bytes(&owner), saved);
    assert_eq!(consumer_order(&owner), [0, 1, 4, 5, 3, 2]);
}

#[cfg(feature = "project_media_codec")]
#[test]
fn exact_source_request_replay_retains_order_and_changed_order_refuses_before_any_new_source_bytes()
{
    use super::session::ImportAssetSource;
    use crate::request_receipts::OpacityRequest;
    let mut owner = super::asset_tests::project_history(1);
    let batch = mixed(&owner);
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let payload = json!({"command":"object.import.batch","layers":[],
        "records":[{"sourceId":"vector","record":batch.records[0].record}],
        "raster":[{"sourceId":"bitmap","entry":batch.raster[0].entry}],
        "text":[{"sourceId":"caption","entry":batch.text[0].entry}],
        "objectOrder":batch.object_order,"documentSnapshotId":snapshot.snapshot_id(),"lifecycleGeneration":1,
        "sources":[{"sourceId":"pixels","name":"blue.png","mediaType":"image/png",
            "source":{"kind":"file","path":"/native-source-order/blue.png"}}]});
    let request = OpacityRequest::history_stage(
        "source-order",
        owner.instance_id(),
        owner.document_id(),
        Some(0),
        OP,
        payload,
    );
    let encoded = batch.assets[0]
        .record
        .source()
        .encoded_allocation()
        .unwrap();
    let source = ImportAssetSource {
        source_id: "pixels",
        name: "blue.png",
        media_type: "image/png",
        bytes: encoded,
    };
    assert!(owner
        .import_batch_sources_status(&request)
        .unwrap()
        .is_none());
    let first = serde_json::to_value(
        owner
            .dispatch_import_batch_sources(&request, &[source])
            .unwrap(),
    )
    .unwrap();
    assert_eq!(first["ok"], true, "{first}");
    let saved = super::asset_tests::project_bytes(&owner);
    assert_eq!(owner.history_depths(), (1, 0));
    assert_eq!(
        serde_json::to_value(owner.dispatch_import_batch_sources(&request, &[]).unwrap()).unwrap(),
        first
    );
    let mut changed = request.clone();
    changed.payload["objectOrder"] = json!(["bitmap", "vector", "caption"]);
    let refusal = serde_json::to_value(
        owner
            .import_batch_sources_status(&changed)
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(refusal["error"]["code"], "invalid_request");
    assert_eq!(super::asset_tests::project_bytes(&owner), saved);
    owner.undo(1).unwrap();
    let undone = super::asset_tests::project_bytes(&owner);
    assert_eq!(
        serde_json::to_value(owner.dispatch_import_batch_sources(&request, &[]).unwrap()).unwrap(),
        first
    );
    assert_eq!(super::asset_tests::project_bytes(&owner), undone);
    assert_eq!(owner.history_depths(), (0, 1));
    owner.redo(2).unwrap();
    assert_eq!(super::asset_tests::project_bytes(&owner), saved);
}
