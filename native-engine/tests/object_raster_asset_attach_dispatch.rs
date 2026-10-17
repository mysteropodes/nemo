//! The attach / remove commands through the real object history owner: one
//! revision and one history entry per applied edit, exact stored bytes, silent
//! no-ops that keep redo, and typed refusals that publish nothing. Literal
//! expectations come from `fixtures/object-raster-asset-attach/oracle.txt`.
use crate::object_document::AssetKind;
use crate::request_receipts::{sha256, DispatchErrorCode};
use serde_json::json;
use std::sync::Arc;

include!("object_raster_asset_attach_support.rs");

#[test]
fn attach_publishes_one_revision_one_history_entry_and_the_exact_bytes() {
    let mut owner = open_history();
    let response = attach_png(&mut owner, "r1", "blue.png", "blue-2x1");
    assert_eq!(response.content_revision(), 1);
    assert_eq!(
        response.result().unwrap(),
        &json!({"applied":true,"historyEntriesAdded":1})
    );
    assert_eq!(owner.content_revision(), 1);
    assert_eq!(owner.history_depths(), (1, 0));
    let records = records_of(&owner);
    assert_eq!(records.len(), 1);
    let record = &records[0];
    assert_eq!(record.id().as_str(), "asset-r1");
    assert_eq!(record.name(), "blue.png");
    assert_eq!(record.kind(), AssetKind::Image);
    assert_eq!(record.source().media_type(), Some("image/png"));
    assert_eq!(record.source().locator(), None);
    let held = record
        .source()
        .encoded_allocation()
        .expect("embedded bytes");
    assert_eq!(held.as_slice(), oracle_bytes("blue-2x1").as_slice());
    assert_eq!(held.capacity(), held.len());
    let digest = hex_of(&sha256(held));
    assert_eq!(digest, oracle_row("blue-2x1")["sha256"].as_str().unwrap());
}

#[test]
fn distinct_files_append_in_attach_order_with_ids_minted_from_the_request() {
    let mut owner = open_history();
    attach_png(&mut owner, "r1", "blue.png", "blue-2x1");
    attach_png(&mut owner, "r2", "red.png", "red-2x1");
    attach_png(&mut owner, "r3", "rgba.png", "rgba-3x1");
    assert_eq!(ids_of(&owner), ["asset-r1", "asset-r2", "asset-r3"]);
    assert_eq!(names_of(&owner), ["blue.png", "red.png", "rgba.png"]);
    assert_eq!(owner.content_revision(), 3);
    assert_eq!(owner.history_depths(), (3, 0));
}

#[test]
fn an_exact_duplicate_is_a_no_op_that_keeps_redo_and_the_held_record() {
    let mut owner = open_history();
    attach_png(&mut owner, "r1", "blue.png", "blue-2x1");
    attach_png(&mut owner, "r2", "red.png", "red-2x1");
    assert_eq!(owner.undo(2), Ok(3));
    assert_eq!(owner.history_depths(), (1, 1));
    let before = saved_bytes(&owner);
    let held = records_of(&owner);
    let request = attach_request(&owner, "r3", "again.png", "image/png");
    let response = owner
        .dispatch_asset_command(&request, Some(&oracle_bytes("blue-2x1")))
        .unwrap();
    assert!(response.is_ok());
    assert_eq!(
        response.result().unwrap(),
        &json!({"applied":false,"historyEntriesAdded":0})
    );
    assert_eq!(response.content_revision(), 3);
    assert_eq!(owner.content_revision(), 3);
    assert_eq!(owner.history_depths(), (1, 1));
    assert_eq!(saved_bytes(&owner), before);
    let after = records_of(&owner);
    assert_eq!(after.len(), 1);
    assert!(Arc::ptr_eq(&after[0], &held[0]));
    assert_eq!(names_of(&owner), ["blue.png"]);
    assert_eq!(owner.redo(3), Ok(4));
    assert_eq!(names_of(&owner), ["blue.png", "red.png"]);
}

#[test]
fn remove_drops_the_asset_and_undo_redo_restore_the_very_same_records() {
    let mut owner = open_history();
    attach_png(&mut owner, "r1", "blue.png", "blue-2x1");
    attach_png(&mut owner, "r2", "red.png", "red-2x1");
    let both = records_of(&owner);
    let request = remove_request(&owner, "rm1", "asset-r1");
    let response = owner.dispatch_asset_command(&request, None).unwrap();
    assert!(response.is_ok());
    assert_eq!(
        response.result().unwrap(),
        &json!({"applied":true,"historyEntriesAdded":1})
    );
    assert_eq!(owner.content_revision(), 3);
    assert_eq!(owner.history_depths(), (3, 0));
    let left = records_of(&owner);
    assert_eq!(left.len(), 1);
    assert!(Arc::ptr_eq(&left[0], &both[1]));
    assert_eq!(owner.undo(3), Ok(4));
    let back = records_of(&owner);
    assert_eq!(back.len(), 2);
    assert!(Arc::ptr_eq(&back[0], &both[0]) && Arc::ptr_eq(&back[1], &both[1]));
    assert_eq!(owner.redo(4), Ok(5));
    let again = records_of(&owner);
    assert_eq!(again.len(), 1);
    assert!(Arc::ptr_eq(&again[0], &both[1]));
}

#[test]
fn removing_an_absent_id_is_a_silent_no_op_and_absent_stays_distinct_from_empty() {
    let mut owner = open_history();
    attach_png(&mut owner, "r1", "blue.png", "blue-2x1");
    let before = saved_bytes(&owner);
    let request = remove_request(&owner, "rm-none", "asset-nothing");
    let response = owner.dispatch_asset_command(&request, None).unwrap();
    assert!(response.is_ok());
    assert_eq!(
        response.result().unwrap(),
        &json!({"applied":false,"historyEntriesAdded":0})
    );
    assert_eq!(
        (owner.content_revision(), owner.history_depths()),
        (1, (1, 0))
    );
    assert_eq!(saved_bytes(&owner), before);
    // A project that never had a catalog stays a catalog-less (version 1) file.
    let mut fresh = open_history();
    let untouched = saved_bytes(&fresh);
    let request = remove_request(&fresh, "rm-fresh", "asset-nothing");
    let response = fresh.dispatch_asset_command(&request, None).unwrap();
    assert_eq!(
        response.result().unwrap(),
        &json!({"applied":false,"historyEntriesAdded":0})
    );
    assert_eq!(saved_bytes(&fresh), untouched);
    let top: serde_json::Value = serde_json::from_slice(&untouched).unwrap();
    assert_eq!(top["formatVersion"], 1);
    assert!(top.get("assetInventory").is_none());
}

#[test]
fn an_asset_the_viewport_reference_carries_cannot_be_removed() {
    let mut owner = open_history();
    attach_png(&mut owner, "r1", "blue.png", "blue-2x1");
    owner
        .attach_reference_png(
            owner.content_revision(),
            "asset-r1".to_string(),
            "ref.png".to_string(),
            &oracle_bytes("red-2x1"),
        )
        .unwrap();
    let revision = owner.content_revision();
    let depths = owner.history_depths();
    let request = remove_request(&owner, "rm1", "asset-r1");
    let response = owner.dispatch_asset_command(&request, None).unwrap();
    assert_eq!(
        refusal_of(&response),
        (DispatchErrorCode::BusyConflict, Some("asset_in_use".into()))
    );
    assert_eq!(
        (owner.content_revision(), owner.history_depths()),
        (revision, depths)
    );
    assert_eq!(names_of(&owner), ["blue.png"]);
}

#[test]
fn a_layer_target_is_a_typed_unavailable_refusal_and_an_explicit_null_target_is_invalid() {
    let mut owner = open_history();
    let pin = current_snapshot_id(&owner);
    let targeted = asset_request(
        &owner,
        "t1",
        "attach",
        json!({"documentSnapshotId":pin,"name":"blue.png","mediaType":"image/png",
            "target":{"layerUid":"drawing"}}),
    );
    let response = owner
        .dispatch_asset_command(&targeted, Some(&oracle_bytes("blue-2x1")))
        .unwrap();
    assert_eq!(
        refusal_of(&response),
        (
            DispatchErrorCode::Unavailable,
            Some("layer_policy_unavailable".into())
        )
    );
    assert_eq!(
        (owner.content_revision(), owner.history_depths()),
        (0, (0, 0))
    );
    assert!(records_of(&owner).is_empty());
    let null_target = asset_request(
        &owner,
        "t2",
        "attach",
        json!({"documentSnapshotId":pin,"name":"blue.png","mediaType":"image/png","target":null}),
    );
    let response = owner
        .dispatch_asset_command(&null_target, Some(&oracle_bytes("blue-2x1")))
        .unwrap();
    assert_eq!(
        refusal_of(&response),
        (DispatchErrorCode::InvalidRequest, Some("target".into()))
    );
    assert_eq!(
        (owner.content_revision(), owner.history_depths()),
        (0, (0, 0))
    );
    assert!(records_of(&owner).is_empty());
    // An ABSENT target member is the ordinary attach.
    let absent = attach_request(&owner, "t3", "blue.png", "image/png");
    let response = owner
        .dispatch_asset_command(&absent, Some(&oracle_bytes("blue-2x1")))
        .unwrap();
    assert!(response.is_ok());
    assert_eq!(names_of(&owner), ["blue.png"]);
}

#[test]
fn recognised_foreign_containers_and_unknown_bytes_are_typed_refusals_that_publish_nothing() {
    let mut owner = open_history();
    let before = saved_bytes(&owner);
    let rows = oracle_rows("foreign");
    assert_eq!(rows.len(), 5, "oracle must keep its five foreign rows");
    for row in rows {
        let name = row["name"].as_str().unwrap();
        let expect = row["expect"].as_str().unwrap();
        let request = attach_request(
            &owner,
            &format!("foreign-{name}"),
            "file.bin",
            row["media"].as_str().unwrap(),
        );
        let response = owner
            .dispatch_asset_command(&request, Some(&oracle_bytes(name)))
            .unwrap();
        let code = if expect == "decoder_unavailable" {
            DispatchErrorCode::Unavailable
        } else {
            DispatchErrorCode::InvalidRequest
        };
        assert_eq!(
            refusal_of(&response),
            (code, Some(expect.to_string())),
            "{name}"
        );
        assert_eq!(saved_bytes(&owner), before, "{name}");
        assert_eq!(
            (owner.content_revision(), owner.history_depths()),
            (0, (0, 0))
        );
    }
}

#[test]
fn corrupt_pngs_a_wrong_declared_type_and_bad_names_are_refused_with_no_publication() {
    let mut owner = open_history();
    let before = saved_bytes(&owner);
    let mut corrupt = 0;
    for row in oracle_rows("png") {
        if row["expect"] != "corrupt" {
            continue;
        }
        corrupt += 1;
        let name = row["name"].as_str().unwrap();
        let request = attach_request(&owner, &format!("bad-{name}"), "x.png", "image/png");
        let response = owner
            .dispatch_asset_command(&request, Some(&oracle_bytes(name)))
            .unwrap();
        assert_eq!(
            refusal_of(&response),
            (DispatchErrorCode::InvalidRequest, Some("corrupt".into())),
            "{name}"
        );
    }
    assert_eq!(corrupt, 3, "oracle must keep its three corrupt rows");
    let png = oracle_bytes("blue-2x1");
    let wrong_type = attach_request(&owner, "wrong-type", "blue.png", "image/jpeg");
    let response = owner
        .dispatch_asset_command(&wrong_type, Some(&png))
        .unwrap();
    assert_eq!(
        refusal_of(&response),
        (
            DispatchErrorCode::InvalidRequest,
            Some("type_mismatch".into())
        )
    );
    for (id, bad) in [
        ("blank", "   ".to_string()),
        ("long", "a".repeat(256)),
        ("nul", "a\u{0}b".to_string()),
    ] {
        let request = attach_request(&owner, &format!("name-{id}"), &bad, "image/png");
        let response = owner.dispatch_asset_command(&request, Some(&png)).unwrap();
        assert_eq!(
            refusal_of(&response),
            (DispatchErrorCode::InvalidRequest, Some("name".into())),
            "{id}"
        );
    }
    assert_eq!(saved_bytes(&owner), before);
    assert_eq!(
        (owner.content_revision(), owner.history_depths()),
        (0, (0, 0))
    );
}
