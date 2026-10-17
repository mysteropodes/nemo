//! Save / reopen with the actual asset bytes, and the baseline catalog sequences
//! replayed through the real owner. The expectations are literal rows of
//! `fixtures/object-raster-asset-attach/oracle.txt`: the order contract of the
//! baseline `addEntry` / `removeEntry` (media-library.js at fbe4b8a3, run verbatim
//! in a Node vm) plus Node-computed base64 and SHA-256 of the file bytes.
use crate::history::NativeObjectHistory;
use crate::object_document::{AssetId, AssetKind, ProjectReferenceState};
use crate::request_receipts::DispatchErrorCode;
use serde_json::{json, Value};
use std::sync::Arc;

include!("object_raster_asset_attach_support.rs");

#[test]
fn save_and_reopen_round_trips_the_actual_bytes_ids_names_and_order() {
    let mut owner = open_history();
    attach_png(&mut owner, "r1", "blue.png", "blue-2x1");
    attach_png(&mut owner, "r2", "rgb 2x2.png", "rgb-2x2");
    let saved = saved_bytes(&owner);
    let file: Value = serde_json::from_slice(&saved).unwrap();
    assert_eq!(file["formatVersion"], 2);
    assert_eq!(
        file["assetInventory"]["assetIds"],
        json!(["asset-r1", "asset-r2"])
    );
    assert_eq!(file["assetInventory"]["placements"], json!([]));
    let expected = [
        ("asset-r1", "blue.png", "blue-2x1"),
        ("asset-r2", "rgb 2x2.png", "rgb-2x2"),
    ];
    let assets = file["assets"].as_array().unwrap();
    assert_eq!(assets.len(), 2);
    for (entry, (id, name, png)) in assets.iter().zip(expected) {
        assert_eq!(entry["id"], id);
        assert_eq!(entry["name"], name);
        assert_eq!(entry["kind"], "image");
        assert_eq!(entry["source"]["kind"], "embedded");
        assert_eq!(entry["source"]["mediaType"], "image/png");
        assert_eq!(entry["source"]["encoding"], "base64");
        assert_eq!(entry["source"]["data"], oracle_row(png)["b64"]);
    }
    let reopened = NativeObjectHistory::from_project_file("desktop", &saved).unwrap();
    assert_ne!(reopened.document_id(), owner.document_id());
    assert_eq!(reopened.history_depths(), (0, 0));
    let records = records_of(&reopened);
    assert_eq!(records.len(), 2);
    for (record, (id, name, png)) in records.iter().zip(expected) {
        assert_eq!(record.id().as_str(), id);
        assert_eq!(record.name(), name);
        assert_eq!(record.kind(), AssetKind::Image);
        assert_eq!(record.source().media_type(), Some("image/png"));
        let held = record
            .source()
            .encoded_allocation()
            .expect("embedded bytes");
        assert_eq!(held.as_slice(), oracle_bytes(png).as_slice());
    }
    assert_eq!(saved_bytes(&reopened), saved);
}

#[test]
fn removing_the_last_asset_keeps_an_explicit_empty_catalog_across_save_and_reopen() {
    let mut owner = open_history();
    attach_png(&mut owner, "r1", "blue.png", "blue-2x1");
    let request = remove_request(&owner, "rm", "asset-r1");
    assert!(owner
        .dispatch_asset_command(&request, None)
        .unwrap()
        .is_ok());
    let saved = saved_bytes(&owner);
    let file: Value = serde_json::from_slice(&saved).unwrap();
    assert_eq!(file["formatVersion"], 2);
    assert_eq!(file["assetInventory"]["assetIds"], json!([]));
    assert_eq!(file["assets"], json!([]));
    let reopened = NativeObjectHistory::from_project_file("desktop", &saved).unwrap();
    let inventory = reopened
        .acquire_snapshot(0)
        .unwrap()
        .project()
        .unwrap()
        .asset_inventory()
        .cloned();
    assert!(inventory.expect("explicit catalog").records().is_empty());
    assert_eq!(saved_bytes(&reopened), saved);
}

#[test]
fn a_duplicate_attach_leaves_one_record_and_one_embedded_file_through_save_and_reopen() {
    let mut owner = open_history();
    attach_png(&mut owner, "r1", "a.png", "blue-2x1");
    let request = attach_request(&owner, "r2", "b.png", "image/png");
    let response = owner
        .dispatch_asset_command(&request, Some(&oracle_bytes("blue-2x1")))
        .unwrap();
    assert_eq!(
        response.result().unwrap(),
        &json!({"applied":false,"historyEntriesAdded":0})
    );
    let saved = saved_bytes(&owner);
    let file: Value = serde_json::from_slice(&saved).unwrap();
    assert_eq!(file["assets"].as_array().unwrap().len(), 1);
    let reopened = NativeObjectHistory::from_project_file("desktop", &saved).unwrap();
    assert_eq!(names_of(&reopened), ["a.png"]);
    assert_eq!(ids_of(&reopened), ["asset-r1"]);
}

#[test]
fn a_reference_that_names_an_attached_asset_shares_its_allocation_and_blocks_removal() {
    let mut owner = open_history();
    attach_png(&mut owner, "r1", "blue.png", "blue-2x1");
    let mut file: Value = serde_json::from_slice(&saved_bytes(&owner)).unwrap();
    file["viewportReference"] = json!({"kind":"still-image","assetId":"asset-r1",
        "name":"plate.png","visible":true,"opacity":0.5,"offsetFrames":0});
    let mut reopened =
        NativeObjectHistory::from_project_file("desktop", &serde_json::to_vec(&file).unwrap())
            .unwrap();
    let snapshot = reopened.acquire_snapshot(0).unwrap();
    let project = snapshot.project().unwrap();
    let ProjectReferenceState::StillImage(reference) = project.reference_state() else {
        panic!("the reopened project carries the reference");
    };
    let record = project
        .asset_inventory()
        .unwrap()
        .get(&AssetId::new("asset-r1".to_string()).unwrap())
        .unwrap()
        .clone();
    assert!(Arc::ptr_eq(
        record.source().encoded_allocation().unwrap(),
        reference.encoded_allocation()
    ));
    let request = remove_request(&reopened, "rm", "asset-r1");
    let response = reopened.dispatch_asset_command(&request, None).unwrap();
    assert_eq!(
        refusal_of(&response),
        (DispatchErrorCode::BusyConflict, Some("asset_in_use".into()))
    );
    assert_eq!(names_of(&reopened), ["blue.png"]);
}

fn text_list(value: &Value) -> Vec<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item.as_str().unwrap().to_string())
        .collect()
}

#[test]
fn the_baseline_catalog_sequences_replay_in_the_baseline_order_with_one_deliberate_difference() {
    let pngs = valid_names();
    assert_eq!(pngs.len(), 4, "oracle must keep its four valid rows");
    let rows = oracle_rows("sequence");
    assert_eq!(rows.len(), 8, "oracle must keep its eight sequences");
    let mut differing = Vec::new();
    for row in rows {
        let label = row["name"].as_str().unwrap().to_string();
        let mut owner = open_history();
        let mut applied_count = 0usize;
        for (index, op) in row["ops"].as_array().unwrap().iter().enumerate() {
            let id = format!("seq-{index}");
            let response = if op[0] == "add" {
                let png = &pngs[op[2].as_u64().unwrap() as usize];
                let request = attach_request(&owner, &id, op[1].as_str().unwrap(), "image/png");
                owner
                    .dispatch_asset_command(&request, Some(&oracle_bytes(png)))
                    .unwrap()
            } else {
                let name = op[1].as_str().unwrap();
                let target = records_of(&owner)
                    .iter()
                    .find(|record| record.name() == name)
                    .map(|record| record.id().as_str().to_string())
                    .unwrap_or_else(|| "asset-none".to_string());
                let request = remove_request(&owner, &id, &target);
                owner.dispatch_asset_command(&request, None).unwrap()
            };
            assert!(response.is_ok(), "{label} op {index}");
            let applied = response.result().unwrap()["applied"].as_bool().unwrap();
            assert_eq!(
                applied,
                row["applied"][index].as_bool().unwrap(),
                "{label} op {index}"
            );
            applied_count += usize::from(applied);
        }
        assert_eq!(names_of(&owner), text_list(&row["native"]), "{label}");
        assert_eq!(owner.history_depths(), (applied_count, 0), "{label}");
        assert!(records_of(&owner)
            .iter()
            .all(|record| record.kind() == AssetKind::Image));
        assert!(row["baselineKinds"]
            .as_array()
            .unwrap()
            .iter()
            .all(|kind| kind == "image"));
        if names_of(&owner) != text_list(&row["baseline"]) {
            differing.push(label);
        }
    }
    // The only row where native differs from the baseline catalog: equal bytes are
    // one asset (the baseline kept both entries).
    assert_eq!(differing, ["equal-bytes-twice"]);
}
