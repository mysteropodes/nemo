//! Genuine provider -> typed batch -> single native project publication controls.
//! Authored source only; these tests have not been executed in this packet.
use super::{ImportAsset, ImportBatch, ImportRaster, ImportText};
use crate::history::NativeObjectHistory;
use crate::object_document::{AssetId, RasterEntry};
use crate::object_snapshot::AssetEdit;
use serde_json::{json, Value};
use std::sync::Arc;
include!("object_raster_asset_attach_support.rs");

pub(super) fn project_history(version: u32) -> NativeObjectHistory {
    let mut project: Value = serde_json::from_slice(&blank_project_file()).unwrap();
    project["drawing"] = super::support::base_doc(version);
    project["project"]["bpm"] = json!(137.5);
    project["project"]["bpmOffset"] = json!(4);
    project["project"]["bpmShow"] = json!(true);
    NativeObjectHistory::from_project_file("import-assets", &serde_json::to_vec(&project).unwrap())
        .unwrap()
}

pub(super) fn new_png(owner: &NativeObjectHistory, source: &str, request: &str) -> ImportAsset {
    let origin = owner.acquire_snapshot(owner.content_revision()).unwrap();
    let prepared = origin
        .prepare_asset_attachment(request, "blue.png", "image/png", &oracle_bytes("blue-2x1"))
        .unwrap();
    assert_eq!(prepared.origin().snapshot_id(), origin.snapshot_id());
    assert!(prepared.changed());
    let AssetEdit::Add(record) = prepared.into_edit() else {
        panic!("new provider asset")
    };
    ImportAsset {
        source_id: source.into(),
        record,
    }
}

pub(super) fn raster(source: &str, layer: &str, frame: u32, asset: &str) -> ImportRaster {
    ImportRaster {
        source_id: source.into(),
        entry: serde_json::from_value(json!({
            "target": super::support::record(layer, frame, 0).target,
            "item":{"assetId":asset,"x":10,"y":20.5,"width":2,"height":1.0,
                "opacity":0.5,"rotation":0,"isBitmapBrush":false,
                "brushGroupId":"kept-group","meshId":"kept-mesh"}
        }))
        .unwrap(),
    }
}

pub(super) fn text(source: &str, layer: &str, font: Value) -> ImportText {
    ImportText {
        source_id: source.into(),
        entry: serde_json::from_value(json!({
            "target":super::support::record(layer, 0, 0).target,
            "block":{"text":"Hello\nworld","font":font,"size":17.5,"color":"#ff00aa80",
                "align":"center","fixedWidth":300,"fixedHeight":null,"bold":true,
                "italic":false,"letterSpacing":-2.5,"wordSpacing":0,"lineHeightMult":1.25,
                "textCase":"upper","anchorTopLeft":{"x":-12.75,"y":104.5}}
        }))
        .unwrap(),
    }
}

pub(super) fn png_batch(owner: &NativeObjectHistory) -> ImportBatch {
    ImportBatch {
        layers: vec![super::support::layer("page", "Imported")],
        records: vec![super::support::item("vector", "page", 0, 5)],
        assets: vec![new_png(owner, "pixels", "prepared-blue")],
        raster: vec![raster("bitmap", "page", 3, "pixels")],
        text: vec![text(
            "caption",
            "page",
            json!({"kind":"bundled","fontKey":"Roboto-Regular"}),
        )],
        ..ImportBatch::default()
    }
}

pub(super) fn project_bytes(owner: &NativeObjectHistory) -> Vec<u8> {
    saved_bytes(owner)
}

#[test]
fn actual_png_vector_and_bundled_text_publish_once_and_round_trip_with_order_and_timing() {
    for version in [1, 2] {
        let mut owner = project_history(version);
        let before = project_bytes(&owner);
        let before_value: Value = serde_json::from_slice(&before).unwrap();
        let batch = png_batch(&owner);
        let allocation = Arc::clone(
            batch.assets[0]
                .record
                .source()
                .encoded_allocation()
                .unwrap(),
        );
        let original_raster = serde_json::to_value(&batch.raster[0].entry.item).unwrap();
        let original_text = serde_json::to_value(&batch.text[0].entry.block).unwrap();
        let prepared = owner.prepare_import_batch(&batch).unwrap();
        assert_eq!(
            owner.content_revision(),
            0,
            "provider preparation publishes nothing"
        );
        assert_eq!(project_bytes(&owner), before);
        assert_eq!(owner.history_depths(), (0, 0));
        assert_eq!(prepared.result().layers.len(), 1);
        assert_eq!(prepared.result().objects.len(), 3);
        assert_eq!(prepared.result().assets.len(), 1);
        let (revision, receipt) = owner.commit_import_batch(prepared).unwrap();
        assert_eq!(revision, 1);
        assert_eq!(owner.history_depths(), (1, 0));
        let snapshot = owner.acquire_snapshot(1).unwrap();
        let inventory = snapshot.project().unwrap().asset_inventory().unwrap();
        let id = AssetId::new(receipt.assets[0].1.clone()).unwrap();
        let record = inventory.get(&id).unwrap();
        assert!(Arc::ptr_eq(
            record.source().encoded_allocation().unwrap(),
            &allocation
        ));
        let items = snapshot.document().item_records().unwrap();
        let mut expected_raster = original_raster;
        expected_raster["assetId"] = json!(id.as_str());
        assert_eq!(
            serde_json::to_value(&items.raster_entries()[0].item).unwrap(),
            expected_raster
        );
        assert_eq!(
            serde_json::to_value(&items.text_entries()[0].block).unwrap(),
            original_text
        );
        assert_eq!(
            items.raster_entries()[0].target.stroke_id(),
            receipt.objects[1].1
        );
        assert_eq!(
            items.text_entries()[0].target.stroke_id(),
            receipt.objects[2].1
        );
        assert_eq!(
            snapshot
                .document()
                .layers
                .last()
                .unwrap()
                .timing
                .value()
                .unwrap()
                .exposure(3)
                .0,
            "key"
        );
        let saved = project_bytes(&owner);
        let value: Value = serde_json::from_slice(&saved).unwrap();
        assert_eq!(value["project"]["bpm"], 137.5);
        assert_eq!(value["project"]["bpmOffset"], 4.0);
        assert_eq!(value["project"]["bpmShow"], true);
        assert_eq!(value["project"], before_value["project"]);
        let reopened = NativeObjectHistory::from_project_file("reopened", &saved).unwrap();
        assert_eq!(project_bytes(&reopened), saved);
        assert_eq!(owner.undo(1), Ok(2));
        assert_eq!(project_bytes(&owner), before);
        assert_eq!(owner.redo(2), Ok(3));
        assert_eq!(project_bytes(&owner), saved);
        let retained = owner.acquire_snapshot(3).unwrap();
        assert!(Arc::ptr_eq(
            retained
                .project()
                .unwrap()
                .asset_inventory()
                .unwrap()
                .get(&id)
                .unwrap()
                .source()
                .encoded_allocation()
                .unwrap(),
            &allocation
        ));
    }
}

#[cfg(feature = "font_asset_wire")]
#[test]
fn genuine_managed_font_and_png_are_new_resources_in_the_same_one_checkpoint_batch() {
    let mut owner = project_history(2);
    let before = project_bytes(&owner);
    let origin = owner.acquire_snapshot(0).unwrap();
    let font = origin
        .admit_font_asset(
            "provider-font".into(),
            "Roboto-Regular".into(),
            include_bytes!("../../src/fonts/Roboto-Regular.ttf"),
        )
        .unwrap();
    assert!(font.source().has_font_admission());
    let allocation = Arc::clone(font.source().encoded_allocation().unwrap());
    let mut batch = png_batch(&owner);
    batch.assets.push(ImportAsset {
        source_id: "typeface".into(),
        record: font,
    });
    batch.text[0] = text(
        "caption",
        "page",
        json!({"kind":"asset","assetId":"typeface"}),
    );
    let prepared = owner.prepare_import_batch(&batch).unwrap();
    assert_eq!(project_bytes(&owner), before);
    assert_eq!(prepared.result().assets.len(), 2);
    let (_, receipt) = owner.commit_import_batch(prepared).unwrap();
    assert_eq!(owner.content_revision(), 1);
    assert_eq!(owner.history_depths(), (1, 0));
    let snapshot = owner.acquire_snapshot(1).unwrap();
    let font_id = AssetId::new(receipt.assets[1].1.clone()).unwrap();
    let retained = snapshot
        .project()
        .unwrap()
        .asset_inventory()
        .unwrap()
        .get(&font_id)
        .unwrap();
    assert!(retained.source().has_font_admission());
    assert!(Arc::ptr_eq(
        retained.source().encoded_allocation().unwrap(),
        &allocation
    ));
    let block =
        serde_json::to_value(&snapshot.document().item_records().unwrap().text_entries()[0].block)
            .unwrap();
    assert_eq!(
        block["font"],
        json!({"kind":"asset","assetId":font_id.as_str()})
    );
    let saved = project_bytes(&owner);
    let reopened = NativeObjectHistory::from_project_file("font-reopen", &saved).unwrap();
    assert_eq!(project_bytes(&reopened), saved);
    assert_eq!(owner.undo(1), Ok(2));
    assert_eq!(project_bytes(&owner), before);
    assert_eq!(owner.redo(2), Ok(3));
    assert_eq!(project_bytes(&owner), saved);
}

#[test]
fn retained_catalog_wire_reuse_is_distinct_from_new_provider_admission_and_keeps_prefix() {
    let mut owner = project_history(2);
    attach_png(&mut owner, "old", "retained.png", "blue-2x1");
    let before_record = Arc::clone(&records_of(&owner)[0]);
    let before = project_bytes(&owner);
    let entry: RasterEntry = raster("copy", "L1", 0, "reuse").entry;
    let payload = json!({"command":"object.import.batch","layers":[],"records":[],
        "assets":[{"sourceId":"reuse","assetId":before_record.id().as_str()}],
        "raster":[{"sourceId":"copy","entry":entry}]});
    let input = super::support::request(&owner, "reuse-import", payload);
    assert!(serde_json::to_vec(&input).unwrap().len() <= 4096);
    let reply = super::support::dispatch(&mut owner, &input);
    assert_eq!(reply["ok"], true, "{reply}");
    assert_eq!(reply["result"]["assets"][0]["sourceId"], "reuse");
    assert_eq!(reply["result"]["objects"][0]["sourceId"], "copy");
    assert!(serde_json::to_vec(&reply).unwrap().len() <= 4096);
    assert_eq!(owner.content_revision(), 2);
    let after = project_bytes(&owner);
    let current = records_of(&owner);
    assert_eq!(current.len(), 2);
    assert!(Arc::ptr_eq(&current[0], &before_record));
    assert!(Arc::ptr_eq(
        current[1].source().encoded_allocation().unwrap(),
        before_record.source().encoded_allocation().unwrap()
    ));
    assert_eq!(
        super::support::dispatch(&mut owner, &input),
        reply,
        "exact replay"
    );
    assert_eq!(project_bytes(&owner), after);
    assert_eq!(owner.undo(2), Ok(3));
    assert_eq!(project_bytes(&owner), before);
    assert_eq!(super::support::dispatch(&mut owner, &input), reply);
    assert_eq!(
        project_bytes(&owner),
        before,
        "replay after undo cannot republish"
    );
}

#[test]
fn assets_only_batch_retains_real_resource_without_inventing_an_item_or_second_history() {
    let mut owner = project_history(1);
    let batch = ImportBatch {
        assets: vec![new_png(&owner, "pixels", "asset-only")],
        ..ImportBatch::default()
    };
    let prepared = owner.prepare_import_batch(&batch).unwrap();
    let (_, result) = owner.commit_import_batch(prepared).unwrap();
    assert_eq!(result.assets.len(), 1);
    assert!(result.objects.is_empty());
    assert_eq!(owner.history_depths(), (1, 0));
    assert!(owner
        .acquire_snapshot(1)
        .unwrap()
        .document()
        .item_records()
        .is_none());
}

#[test]
fn bundled_text_uses_real_record_without_materializing_an_empty_catalog() {
    let mut owner = project_history(1);
    let batch = ImportBatch {
        text: vec![text(
            "caption",
            "L1",
            json!({"kind":"bundled","fontKey":"Roboto-Regular"}),
        )],
        ..ImportBatch::default()
    };
    let prepared = owner.prepare_import_batch(&batch).unwrap();
    owner.commit_import_batch(prepared).unwrap();
    let snapshot = owner.acquire_snapshot(1).unwrap();
    assert!(snapshot.project().unwrap().asset_inventory().is_none());
    assert_eq!(
        snapshot
            .document()
            .item_records()
            .unwrap()
            .text_entries()
            .len(),
        1
    );
    assert_eq!(owner.history_depths(), (1, 0));
}

#[cfg(feature = "font_asset_wire")]
#[test]
fn production_trusted_source_preparation_imports_new_png_and_font_without_an_attach_checkpoint() {
    use super::session::ImportAssetSource;
    let mut owner = project_history(2);
    let before = project_bytes(&owner);
    let png = oracle_bytes("blue-2x1");
    let font = include_bytes!("../../src/fonts/Roboto-Regular.ttf");
    let batch = ImportBatch {
        layers: vec![super::support::layer("page", "Managed source batch")],
        records: vec![super::support::item("line", "page", 0, 9)],
        raster: vec![raster("bitmap", "page", 0, "pixels")],
        text: vec![text(
            "caption",
            "page",
            json!({"kind":"asset","assetId":"typeface"}),
        )],
        ..ImportBatch::default()
    };
    let prepared = owner
        .prepare_import_batch_sources(
            "source-batch",
            &batch,
            &[
                ImportAssetSource {
                    source_id: "pixels",
                    name: "blue.png",
                    media_type: "image/png",
                    bytes: &png,
                },
                ImportAssetSource {
                    source_id: "typeface",
                    name: "Roboto-Regular",
                    media_type: "font/ttf",
                    bytes: font,
                },
            ],
        )
        .unwrap();
    assert_eq!(project_bytes(&owner), before);
    assert_eq!(owner.content_revision(), 0);
    assert_eq!(owner.history_depths(), (0, 0));
    assert_eq!(prepared.result().assets.len(), 2);
    assert_eq!(prepared.result().objects.len(), 3);
    let (revision, receipt) = owner.commit_import_batch(prepared).unwrap();
    assert_eq!(revision, 1);
    assert_eq!(owner.history_depths(), (1, 0));
    let snapshot = owner.acquire_snapshot(1).unwrap();
    let inventory = snapshot.project().unwrap().asset_inventory().unwrap();
    assert_eq!(inventory.records().len(), 2);
    let png_id = AssetId::new(receipt.assets[0].1.clone()).unwrap();
    let font_id = AssetId::new(receipt.assets[1].1.clone()).unwrap();
    assert_eq!(
        inventory
            .get(&png_id)
            .unwrap()
            .source()
            .encoded_allocation()
            .unwrap()
            .as_slice(),
        png.as_slice()
    );
    assert!(inventory
        .get(&font_id)
        .unwrap()
        .source()
        .has_font_admission());
    assert_eq!(
        inventory
            .get(&font_id)
            .unwrap()
            .source()
            .encoded_allocation()
            .unwrap()
            .as_slice(),
        font.as_slice()
    );
    let saved = project_bytes(&owner);
    let reopened = NativeObjectHistory::from_project_file("source-reopen", &saved).unwrap();
    assert_eq!(project_bytes(&reopened), saved);
    assert_eq!(owner.undo(1), Ok(2));
    assert_eq!(project_bytes(&owner), before);
    assert_eq!(owner.redo(2), Ok(3));
    assert_eq!(project_bytes(&owner), saved);
}

#[test]
fn another_mixed_batch_preserves_existing_items_catalog_and_other_project_fields_as_prefixes() {
    let mut owner = project_history(2);
    let first = owner.prepare_import_batch(&png_batch(&owner)).unwrap();
    let (_, first_receipt) = owner.commit_import_batch(first).unwrap();
    let origin = owner.acquire_snapshot(1).unwrap();
    let old_items = origin.document().item_records().unwrap().clone();
    let old_record = Arc::clone(
        &origin
            .project()
            .unwrap()
            .asset_inventory()
            .unwrap()
            .records()[0],
    );
    let before: Value = serde_json::from_slice(&project_bytes(&owner)).unwrap();
    let batch = ImportBatch {
        records: vec![super::support::item("line-two", "L1", 0, 70)],
        raster: vec![raster("bitmap-two", "L1", 0, &first_receipt.assets[0].1)],
        text: vec![text(
            "caption-two",
            "L1",
            json!({"kind":"bundled","fontKey":"Roboto-Regular"}),
        )],
        ..ImportBatch::default()
    };
    let second = owner.prepare_import_batch(&batch).unwrap();
    assert!(second.result().assets.is_empty());
    owner.commit_import_batch(second).unwrap();
    assert_eq!(owner.history_depths(), (2, 0));
    let snapshot = owner.acquire_snapshot(2).unwrap();
    let items = snapshot.document().item_records().unwrap();
    assert!(items
        .raster_entries()
        .starts_with(old_items.raster_entries()));
    assert!(items.text_entries().starts_with(old_items.text_entries()));
    let catalog = snapshot.project().unwrap().asset_inventory().unwrap();
    assert_eq!(catalog.records().len(), 1);
    assert!(Arc::ptr_eq(&catalog.records()[0], &old_record));
    let after: Value = serde_json::from_slice(&project_bytes(&owner)).unwrap();
    assert_eq!(after["project"], before["project"]);
    assert_eq!(after["viewportReference"], before["viewportReference"]);
    assert_eq!(after["assets"], before["assets"]);
}
