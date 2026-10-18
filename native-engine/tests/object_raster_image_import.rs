//! Real native owner controls; authored here but UNRUN until an explicit compiler-slot grant.
use crate::history::NativeObjectHistory;
use crate::request_receipts::{DispatchErrorCode, OpacityRequest};
use serde_json::{json, Value};
include!("object_raster_asset_attach_support.rs");

fn request(owner: &NativeObjectHistory, id: &str, placement: Value) -> OpacityRequest {
    asset_request(
        owner,
        id,
        "attach",
        json!({"documentSnapshotId":current_snapshot_id(owner),
        "name":"still.png","lifecycleGeneration":1,"mediaType":"image/png","source":{"kind":"file","path":"/admitted/still.png"},
        "placement":placement}),
    )
}
fn result(response: &crate::request_receipts::ResponseEnvelope) -> Value {
    serde_json::to_value(response).unwrap()["result"].clone()
}
fn pair_counts(owner: &NativeObjectHistory) -> (usize, usize, usize) {
    let snapshot = owner.acquire_snapshot(owner.content_revision()).unwrap();
    (
        records_of(owner).len(),
        snapshot.document().layers().len(),
        snapshot
            .document()
            .item_records()
            .map_or(0, |i| i.raster_entries().len()),
    )
}

#[test]
fn a_new_still_layer_and_resource_publish_once_undo_redo_and_save_reload_together() {
    let mut owner = open_history();
    let png = oracle_bytes("blue-2x1");
    let input = request(&owner, "image-one", json!({"kind":"newLayer"}));
    let reply = owner.dispatch_image_import(&input, &png).unwrap();
    assert!(reply.is_ok());
    assert_eq!(owner.content_revision(), 1);
    assert_eq!(owner.history_depths(), (1, 0));
    assert_eq!(pair_counts(&owner), (1, 2, 1));
    let receipt = result(&reply);
    assert_eq!(receipt["assetId"], "asset-image-one");
    assert_eq!(receipt["naturalWidth"], 2);
    assert_eq!(receipt["naturalHeight"], 1);
    assert_eq!(receipt["displayWidth"], 2.0);
    assert_eq!(receipt["displayHeight"], 1.0);
    let snapshot = owner.acquire_snapshot(1).unwrap();
    let layer = &snapshot.document().layers()[1];
    assert_eq!(layer.name(), Some("still.png"));
    assert_eq!(layer.timing.value().unwrap().exposure(20).1, Some(0));
    let saved = saved_bytes(&owner);
    let reopened = NativeObjectHistory::from_project_file("desktop", &saved).unwrap();
    assert_eq!(pair_counts(&reopened), (1, 2, 1));
    assert_eq!(saved_bytes(&reopened), saved);
    assert_eq!(owner.undo(1), Ok(2));
    assert_eq!(pair_counts(&owner), (0, 1, 0));
    assert_eq!(owner.redo(2), Ok(3));
    assert_eq!(pair_counts(&owner), (1, 2, 1));
}

#[test]
fn duplicate_content_still_places_once_with_the_canonical_existing_asset_id() {
    let mut owner = open_history();
    let png = oracle_bytes("blue-2x1");
    let first = request(
        &owner,
        "first",
        json!({"kind":"authoredFrame","layerUid":"drawing","frame":0}),
    );
    let reply = owner.dispatch_image_import(&first, &png).unwrap();
    assert!(reply.is_ok());
    let second = request(
        &owner,
        "second",
        json!({"kind":"authoredFrame","layerUid":"drawing","frame":0}),
    );
    let duplicate = owner.dispatch_image_import(&second, &png).unwrap();
    assert!(duplicate.is_ok());
    assert_eq!(result(&reply)["assetId"], result(&duplicate)["assetId"]);
    assert_ne!(result(&reply)["target"], result(&duplicate)["target"]);
    assert_eq!(pair_counts(&owner), (1, 1, 2));
    assert_eq!(owner.history_depths(), (2, 0));
}

#[test]
fn exact_retry_replays_before_source_resolution_even_after_undo_but_changed_source_or_placement_refuses(
) {
    let mut owner = open_history();
    let png = oracle_bytes("blue-2x1");
    let input = request(&owner, "retry", json!({"kind":"newLayer"}));
    let first = owner.dispatch_image_import(&input, &png).unwrap();
    assert!(first.is_ok());
    assert_eq!(owner.undo(1), Ok(2));
    assert_eq!(
        owner.image_import_status(&input).unwrap(),
        Some(first.clone())
    );
    assert_eq!(
        owner
            .dispatch_image_import(&input, b"now missing/corrupt source")
            .unwrap(),
        first
    );
    assert_eq!(pair_counts(&owner), (0, 1, 0));
    assert_eq!(owner.history_depths(), (0, 1));
    let mut changed = serde_json::to_value(&input).unwrap();
    changed["payload"]["source"]["path"] = json!("/another.png");
    let changed: OpacityRequest = serde_json::from_value(changed).unwrap();
    let changed_reply = owner.image_import_status(&changed).unwrap().unwrap();
    assert_eq!(
        changed_reply.error().unwrap().code(),
        DispatchErrorCode::InvalidRequest
    );
    assert_eq!(owner.image_import_status(&input).unwrap(), Some(first));
    assert_eq!(owner.history_depths(), (0, 1));
}

#[test]
fn corrupt_decode_missing_target_and_precommit_cancel_leave_no_resource_layer_or_history() {
    for mode in ["corrupt", "missing", "cancel"] {
        let mut owner = open_history();
        let bytes = oracle_bytes(if mode == "corrupt" {
            "signature-then-zeros"
        } else {
            "blue-2x1"
        });
        let placement = if mode == "missing" {
            json!({"kind":"authoredFrame","layerUid":"missing","frame":0})
        } else {
            json!({"kind":"newLayer"})
        };
        let input = request(&owner, mode, placement);
        let reply = owner
            .dispatch_image_import_cancellable(&input, &bytes, &|| mode == "cancel")
            .unwrap();
        assert!(!reply.is_ok(), "{mode}");
        assert_eq!(owner.content_revision(), 0);
        assert_eq!(owner.history_depths(), (0, 0));
        assert_eq!(pair_counts(&owner), (0, 1, 0));
    }
}

#[test]
fn stale_prepared_candidate_cannot_commit_after_another_owner_edit() {
    use crate::commands::ImageImport;
    let mut owner = crate::commands::ObjectFillOwner::from_project_file(
        "desktop".into(),
        &blank_project_file(),
    )
    .unwrap();
    let before = owner.current_snapshot();
    let payload=ImageImport::parse(&serde_json::to_vec(&json!({"documentSnapshotId":before.snapshot_id(),"name":"still.png","lifecycleGeneration":1,"mediaType":"image/png",
        "source":{"kind":"file","path":"/admitted/still.png"},"placement":{"kind":"newLayer"}})).unwrap()).unwrap();
    let prepared = owner
        .prepare_image_import("stale-image", &payload, &oracle_bytes("blue-2x1"))
        .unwrap();
    let other = owner
        .prepare_layer_create_json(br#"{"command":"object.layer.create","name":"Other"}"#)
        .unwrap();
    owner.commit_layer_create(other).unwrap();
    assert_eq!(
        owner.commit_image_import(prepared),
        Err(DispatchErrorCode::StaleRevision)
    );
    assert_eq!(
        owner
            .current_snapshot()
            .project()
            .unwrap()
            .asset_inventory()
            .map_or(0, |i| i.records().len()),
        0
    );
}

#[test]
fn placement_wire_rejects_duplicate_unknown_and_untrusted_source_members() {
    use crate::commands::ImageImport;
    let valid = json!({"documentSnapshotId":"snapshot","name":"still.png","lifecycleGeneration":1,"mediaType":"image/png",
        "source":{"kind":"file","path":"/admitted/still.png"},"placement":{"kind":"newLayer"}});
    assert!(ImageImport::parse(&serde_json::to_vec(&valid).unwrap()).is_ok());
    let mut unknown = valid.clone();
    unknown["source"]["bytes"] = json!("page supplied bytes");
    assert!(ImageImport::parse(&serde_json::to_vec(&unknown).unwrap()).is_err());
    let raw = serde_json::to_string(&valid).unwrap().replace(
        "\"mediaType\":\"image/png\"",
        "\"mediaType\":\"image/png\",\"mediaType\":\"image/png\"",
    );
    assert!(ImageImport::parse(raw.as_bytes()).is_err());
    let mut unknown = valid;
    unknown["placement"]["invented"] = json!(true);
    assert!(ImageImport::parse(&serde_json::to_vec(&unknown).unwrap()).is_err());
}

#[test]
fn held_authored_cell_and_early_cancellation_refuse_without_decoding_or_publication() {
    let mut owner =
        NativeObjectHistory::new_blank_project("held-image".into(), 320, 180, 24.).unwrap();
    let before = owner.acquire_snapshot(0).unwrap();
    let uid = before.document().layers()[0].layer_uid().to_owned();
    let held = request(
        &owner,
        "held",
        json!({"kind":"authoredFrame","layerUid":uid,"frame":1}),
    );
    let refused = owner.dispatch_image_import(&held, b"not decoded").unwrap();
    assert!(!refused.is_ok());
    assert_eq!(owner.content_revision(), 0);
    assert_eq!(owner.history_depths(), (0, 0));
    let cancelled = request(&owner, "early-cancel", json!({"kind":"newLayer"}));
    let refused = owner
        .dispatch_image_import_cancellable(&cancelled, b"not decoded", &|| true)
        .unwrap();
    let refused: Value = serde_json::to_value(refused).unwrap();
    assert_eq!(refused["error"]["code"], "cancelled_before_dispatch");
    assert_eq!(owner.content_revision(), 0);
    assert_eq!(pair_counts(&owner), (0, 1, 0));
}

#[test]
fn wire_refuses_positional_outer_payload_and_nested_arrays_on_original_bytes() {
    use crate::commands::ImageImport;
    let source = json!({"kind":"file","path":"/admitted/still.png"});
    let placement = json!({"kind":"newLayer"});
    let positional = json!(["snapshot", 1, "still.png", "image/png", source, placement]);
    assert!(ImageImport::parse(&serde_json::to_vec(&positional).unwrap()).is_err());
    for bytes in [b"null".as_slice(), b"true", b"42", b"\"text\""] {
        assert!(ImageImport::parse(bytes).is_err());
    }
    let valid = json!({"documentSnapshotId":"snapshot","lifecycleGeneration":1,"name":"still.png","mediaType":"image/png",
        "source":{"kind":"file","path":"/admitted/still.png"},"placement":{"kind":"newLayer"}});
    for field in ["source", "placement"] {
        let mut bad = valid.clone();
        bad[field] = json!(["file", "/admitted/still.png"]);
        assert!(ImageImport::parse(&serde_json::to_vec(&bad).unwrap()).is_err());
    }
}

#[test]
fn wrong_owner_typed_terminal_preflight_does_not_poison_a_genuine_request_id() {
    let mut owner = open_history();
    let genuine = request(&owner, "shared-id", json!({"kind":"newLayer"}));
    for (field, code) in [
        ("instanceId", DispatchErrorCode::WrongInstance),
        ("documentId", DispatchErrorCode::WrongDocument),
    ] {
        let mut altered = serde_json::to_value(&genuine).unwrap();
        altered[field] = json!("foreign");
        let altered: OpacityRequest = serde_json::from_value(altered).unwrap();
        let reply = owner.image_import_status(&altered).unwrap().unwrap();
        assert_eq!(reply.error().unwrap().code(), code);
        let reply = serde_json::to_value(reply).unwrap();
        assert_eq!(reply["requestId"], genuine.request_id);
        assert_eq!(reply["instanceId"], owner.instance_id());
        assert_eq!(reply["documentId"], owner.document_id());
        assert_eq!(pair_counts(&owner), (0, 1, 0));
        assert_eq!(owner.history_depths(), (0, 0));
    }
    assert!(owner.image_import_status(&genuine).unwrap().is_none());
    let success = owner
        .dispatch_image_import(&genuine, &oracle_bytes("blue-2x1"))
        .unwrap();
    assert!(success.is_ok());
    let mut altered = serde_json::to_value(&genuine).unwrap();
    altered["payload"]["placement"] =
        json!({"kind":"authoredFrame","layerUid":"drawing","frame":0});
    let altered: OpacityRequest = serde_json::from_value(altered).unwrap();
    let refusal = owner.image_import_status(&altered).unwrap().unwrap();
    assert_eq!(
        refusal.error().unwrap().code(),
        DispatchErrorCode::InvalidRequest
    );
    assert_eq!(owner.image_import_status(&genuine).unwrap(), Some(success));
    assert_eq!(pair_counts(&owner), (1, 2, 1));
    assert_eq!(owner.history_depths(), (1, 0));
}

#[test]
fn malformed_request_ids_never_expand_into_unbounded_typed_error_envelopes() {
    let mut owner = open_history();
    let before = saved_bytes(&owner);
    for id in [
        "x".repeat(4096),
        "x".repeat(129),
        String::new(),
        "bad\0id".into(),
        "bad id".into(),
    ] {
        let input = request(&owner, &id, json!({"kind":"newLayer"}));
        assert_eq!(
            owner.image_import_status(&input),
            Err(DispatchErrorCode::InvalidRequest)
        );
        assert_eq!(saved_bytes(&owner), before);
        assert_eq!(owner.history_depths(), (0, 0));
    }
    let genuine = request(&owner, "bounded-id", json!({"kind":"newLayer"}));
    for field in ["instanceId", "documentId"] {
        let mut malformed = serde_json::to_value(&genuine).unwrap();
        malformed[field] = json!("x".repeat(4096));
        let malformed: OpacityRequest = serde_json::from_value(malformed).unwrap();
        match owner.image_import_status(&malformed) {
            Err(DispatchErrorCode::InvalidRequest) => {}
            Ok(Some(reply)) => {
                assert_eq!(
                    reply.error().unwrap().code(),
                    DispatchErrorCode::InvalidRequest
                );
                assert!(serde_json::to_vec(&reply).unwrap().len() <= 4096);
            }
            other => panic!("malformed envelope must never grant source resolution: {other:?}"),
        }
        assert_eq!(saved_bytes(&owner), before);
    }
    assert!(owner.image_import_status(&genuine).unwrap().is_none());
}

#[test]
fn maximum_valid_request_id_is_preserved_in_a_bounded_wrong_owner_response_without_retention() {
    let mut owner = open_history();
    let id = "x".repeat(128);
    let genuine = request(&owner, &id, json!({"kind":"newLayer"}));
    let mut foreign = serde_json::to_value(&genuine).unwrap();
    foreign["instanceId"] = json!("foreign");
    let foreign: OpacityRequest = serde_json::from_value(foreign).unwrap();
    let reply = owner.image_import_status(&foreign).unwrap().unwrap();
    assert_eq!(
        reply.error().unwrap().code(),
        DispatchErrorCode::WrongInstance
    );
    let encoded = serde_json::to_vec(&reply).unwrap();
    assert!(encoded.len() <= 4096);
    assert_eq!(serde_json::to_value(reply).unwrap()["requestId"], id);
    assert!(owner.image_import_status(&genuine).unwrap().is_none());
    assert_eq!(owner.history_depths(), (0, 0));
}
