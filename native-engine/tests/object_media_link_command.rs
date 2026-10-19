//! Actual staged dispatcher, receipt, asset and history controls; no public-host admission.
use super::*;
use crate::request_receipts::OpacityRequest;
use serde_json::{json, Value};
use std::sync::Arc;

fn placement(id: &str, asset: &str, layer: &str, offset: i64) -> Value {
    json!({"id":id,"assetId":asset,"target":{"kind":"drawing",
        "contextId":"scene-root","layerUid":layer},"offsetFrames":offset})
}
fn owner() -> NativeObjectHistory {
    let png = crate::object_codec::encoded_wire_test::PNG;
    let wire = json!({"format":"nemo.native-project-document","formatVersion":2,
        "project":{"canvasW":320,"canvasH":180,"fps":24},
        "drawing":{"format":"nemo.native-object-document","formatVersion":1,
            "totalFrames":120,"layers":[{"layerUid":"L"},{"layerUid":"M"}],"objects":[]},
        "viewportReference":null,
        "assets":[
            {"id":"v1","name":"take","kind":"video","source":{"kind":"linked","locator":"clip.mov"}},
            {"id":"a1","name":"score","kind":"audio","source":{"kind":"linked","locator":"score.wav"}},
            {"id":"i1","name":"plate","kind":"image","source":{"kind":"embedded",
                "mediaType":"image/png","encoding":"base64","data":png}}],
        "assetInventory":{"assetIds":["v1","a1","i1"],
            "placements":[placement("p1","v1","L",5)]}});
    NativeObjectHistory::from_project_file("placement-commands", wire.to_string().as_bytes())
        .unwrap()
}
fn saved(o: &NativeObjectHistory) -> Vec<u8> {
    let pin = o.acquire_snapshot(o.content_revision()).unwrap();
    o.serialize_project_file(o.content_revision(), pin.snapshot_id())
        .unwrap()
}
fn table(o: &NativeObjectHistory) -> Value {
    serde_json::from_slice::<Value>(&saved(o)).unwrap()["assetInventory"]["placements"].clone()
}
fn offset(o: &NativeObjectHistory) -> i64 {
    table(o)[0]["offsetFrames"].as_i64().unwrap()
}
fn request(o: &NativeObjectHistory, id: &str, payload: Value) -> Value {
    json!({"apiVersion":2,"requestId":id,"instanceId":o.instance_id(),
        "documentId":o.document_id(),"operation":OP_PLACEMENT_EDIT,
        "expectedRevision":o.content_revision(),"payload":payload})
}
fn set(value: i64) -> Value {
    json!({"kind":"setOffset","placementId":"p1","offsetFrames":value})
}
fn shift(value: i64) -> Value {
    json!({"kind":"shiftOffset","placementId":"p1","deltaFrames":value})
}
fn dispatch(o: &mut NativeObjectHistory, request: &Value) -> Value {
    let typed: OpacityRequest = serde_json::from_value(request.clone()).unwrap();
    serde_json::to_value(o.try_dispatch_object_command(&typed).unwrap().unwrap()).unwrap()
}
fn state(o: &NativeObjectHistory) -> (u64, (usize, usize), Vec<u8>) {
    (o.content_revision(), o.history_depths(), saved(o))
}

#[test]
fn relative_request_executes_once_and_replays_before_stale_even_after_undo() {
    let mut o = owner();
    let req = request(&o, "shift-once", shift(-7));
    let first = dispatch(&mut o, &req);
    assert_eq!(first["ok"], true);
    assert_eq!(first["contentRevision"], 1);
    assert_eq!(
        first["result"],
        json!({"applied":true,"historyEntriesAdded":1})
    );
    assert_eq!(offset(&o), -2);
    let changed = state(&o);
    assert_eq!(dispatch(&mut o, &req), first);
    assert_eq!(state(&o), changed);
    o.undo(1).unwrap();
    let undone = state(&o);
    assert_eq!(offset(&o), 5);
    assert_eq!(dispatch(&mut o, &req), first);
    assert_eq!(state(&o), undone);
    assert_eq!(o.history_depths(), (0, 1));
    o.redo(2).unwrap();
    assert_eq!(offset(&o), -2);
    assert_eq!(o.history_depths(), (1, 0));
}

#[test]
fn changed_body_request_id_cannot_replace_or_repeat_a_terminal_receipt() {
    let mut o = owner();
    let req = request(&o, "same-id", shift(3));
    let first = dispatch(&mut o, &req);
    let before = state(&o);
    for payload in [
        shift(4),
        set(9),
        json!({"kind":"remove","placementId":"p1"}),
    ] {
        let mut changed = req.clone();
        changed["payload"] = payload;
        changed["expectedRevision"] = json!(1);
        assert_eq!(
            dispatch(&mut o, &changed)["error"]["code"],
            "invalid_request"
        );
        assert_eq!(state(&o), before);
        assert_eq!(dispatch(&mut o, &req), first);
    }
}

#[test]
fn no_op_receipts_preserve_revision_bytes_and_redo_even_when_replayed_later() {
    let mut o = owner();
    let edit = request(&o, "change", set(-8));
    assert_eq!(dispatch(&mut o, &edit)["ok"], true);
    o.undo(1).unwrap();
    for (id, payload) in [("same-offset", set(5)), ("zero-shift", shift(0))] {
        let before = state(&o);
        let req = request(&o, id, payload);
        let first = dispatch(&mut o, &req);
        assert_eq!(
            first["result"],
            json!({"applied":false,"historyEntriesAdded":0})
        );
        assert_eq!(first["contentRevision"], before.0);
        assert_eq!(state(&o), before);
        assert_eq!(dispatch(&mut o, &req), first);
        o.redo(o.content_revision()).unwrap();
        let later = state(&o);
        assert_eq!(dispatch(&mut o, &req), first);
        assert_eq!(state(&o), later);
        o.undo(o.content_revision()).unwrap();
        // Revision is monotonic. The next no-op uses its actual current revision.
    }
}

#[test]
fn place_set_shift_remove_save_reopen_and_history_use_the_actual_catalog() {
    let mut o = owner();
    for (id, payload) in [
        (
            "place",
            json!({"kind":"place","placement":placement("p2","v1","M",0)}),
        ),
        (
            "set",
            json!({"kind":"setOffset","placementId":"p2","offsetFrames":-9}),
        ),
        (
            "shift",
            json!({"kind":"shiftOffset","placementId":"p2","deltaFrames":2}),
        ),
        ("remove", json!({"kind":"remove","placementId":"p1"})),
    ] {
        let req = request(&o, id, payload);
        let result = dispatch(&mut o, &req);
        assert_eq!(result["ok"], true, "{result}");
        assert_eq!(
            result["result"],
            json!({"applied":true,"historyEntriesAdded":1})
        );
    }
    assert_eq!(table(&o), json!([placement("p2", "v1", "M", -7)]));
    assert_eq!(o.content_revision(), 4);
    assert_eq!(o.history_depths(), (4, 0));
    let bytes = saved(&o);
    let reopened = NativeObjectHistory::from_project_file("reopened", &bytes).unwrap();
    assert_eq!(saved(&reopened), bytes);
    for _ in 0..4 {
        o.undo(o.content_revision()).unwrap();
    }
    assert_eq!(table(&o), json!([placement("p1", "v1", "L", 5)]));
    for _ in 0..4 {
        o.redo(o.content_revision()).unwrap();
    }
    assert_eq!(saved(&o), bytes);
}

#[test]
fn old_snapshot_keeps_real_encoded_bytes_and_inventory_across_edit_and_owner_drop() {
    let mut o = owner();
    let pin = o.acquire_snapshot(0).unwrap();
    let old_bytes = saved(&o);
    let old_inventory = pin.project().unwrap().asset_inventory().unwrap();
    let old_image = old_inventory
        .records()
        .iter()
        .find(|r| r.id().as_str() == "i1")
        .unwrap();
    let encoded = old_image.source().encoded_allocation().unwrap();
    assert_eq!(&encoded[..8], b"\x89PNG\r\n\x1a\n");
    let old_encoded = encoded.as_slice().to_vec();
    let req = request(&o, "retain", shift(4));
    assert_eq!(dispatch(&mut o, &req)["ok"], true);
    let next = o.acquire_snapshot(1).unwrap();
    let next_inventory = next.project().unwrap().asset_inventory().unwrap();
    for (old, new) in old_inventory.records().iter().zip(next_inventory.records()) {
        assert!(Arc::ptr_eq(old, new));
    }
    let new_image = next_inventory
        .records()
        .iter()
        .find(|r| r.id().as_str() == "i1")
        .unwrap();
    assert!(Arc::ptr_eq(
        encoded,
        new_image.source().encoded_allocation().unwrap()
    ));
    assert_eq!(old_inventory.placements()[0].offset_frames(), 5);
    assert_eq!(next_inventory.placements()[0].offset_frames(), 9);
    assert_eq!(
        o.serialize_project_file(0, pin.snapshot_id()).unwrap(),
        old_bytes
    );
    drop((next, o));
    assert_eq!(encoded.as_slice(), old_encoded);
    assert_eq!(
        pin.project()
            .unwrap()
            .asset_inventory()
            .unwrap()
            .placements()[0]
            .offset_frames(),
        5
    );
}

#[test]
fn cancelled_and_stale_requests_are_retained_without_publication_or_retry() {
    for (field, value, expected) in [
        (
            "cancelledBeforeDispatch",
            json!(true),
            "cancelled_before_dispatch",
        ),
        ("expectedRevision", json!(1), "stale_revision"),
    ] {
        let mut o = owner();
        let mut req = request(&o, "refused", shift(3));
        req[field] = value;
        let before = state(&o);
        let first = dispatch(&mut o, &req);
        assert_eq!(first["error"]["code"], expected);
        assert_eq!(dispatch(&mut o, &req), first);
        assert_eq!(state(&o), before);
        let advance = request(&o, "advance", shift(2));
        assert_eq!(dispatch(&mut o, &advance)["ok"], true);
        let after = state(&o);
        assert_eq!(dispatch(&mut o, &req), first);
        assert_eq!(state(&o), after);
        let corrected = request(&o, "refused", shift(3));
        assert_eq!(
            dispatch(&mut o, &corrected)["error"]["code"],
            "invalid_request"
        );
        assert_eq!(state(&o), after);
    }
}

#[test]
fn wrong_owner_refusal_precedes_receipts_and_cannot_poison_the_valid_owner() {
    for (field, code) in [
        ("instanceId", "wrong_instance"),
        ("documentId", "wrong_document"),
    ] {
        let mut o = owner();
        let req = request(&o, "owner-check", shift(1));
        let mut wrong = req.clone();
        wrong[field] = json!("another-owner");
        let before = state(&o);
        assert_eq!(dispatch(&mut o, &wrong)["error"]["code"], code);
        assert_eq!(state(&o), before);
        assert_eq!(dispatch(&mut o, &req)["ok"], true);
        let after = state(&o);
        assert_eq!(dispatch(&mut o, &wrong)["error"]["code"], code);
        assert_eq!(state(&o), after);
    }
}

#[test]
fn malformed_payloads_and_unresolved_targets_leave_no_partial_state() {
    let invalid = [
        json!({"kind":"setOffset","placementId":"p1","offsetFrames":null}),
        json!({"kind":"setOffset","placementId":"p1","offsetFrames":1.5}),
        json!({"kind":"shiftOffset","placementId":"p1","deltaFrames":9007199254740991_i64}),
        json!({"kind":"remove","placementId":"p1","extra":true}),
        json!({"kind":"unknown"}),
        json!({"kind":"place","placement":placement("p2","v1","missing",0)}),
        json!({"kind":"place","placement":placement("p2","missing","M",0)}),
        json!({"kind":"place","placement":placement("p2","v1","L",0)}),
    ];
    for (index, payload) in invalid.into_iter().enumerate() {
        let mut o = owner();
        let req = request(&o, &format!("bad-{index}"), payload);
        let before = state(&o);
        let first = dispatch(&mut o, &req);
        assert_eq!(first["ok"], false, "{first}");
        assert_eq!(dispatch(&mut o, &req), first);
        assert_eq!(state(&o), before);
    }
}

#[test]
fn audio_track_still_refuses_through_the_real_owner_and_retains_the_refusal() {
    let mut o = owner();
    let req = request(
        &o,
        "audio",
        json!({"kind":"place","placement":{
        "id":"audio-p","assetId":"a1","target":{"kind":"audioTrack","trackUid":"track-a"},"offsetFrames":0}}),
    );
    let before = state(&o);
    let first = dispatch(&mut o, &req);
    assert_eq!(first["error"]["code"], "unavailable");
    assert_eq!(dispatch(&mut o, &req), first);
    assert_eq!(state(&o), before);
}

#[test]
fn raw_payload_parser_refuses_duplicates_unknown_fields_and_null_optional_target_id() {
    for bytes in [
        r#"{"kind":"remove","kind":"remove","placementId":"p1"}"#,
        r#"{"kind":"setOffset","placementId":"p1","placementId":"p2","offsetFrames":0}"#,
        r#"{"kind":"place","placement":{"id":"p","assetId":"v1","target":{"kind":"drawing","contextId":"scene-root","layerUid":"M","strokeId":null},"offsetFrames":0}}"#,
        r#"{"kind":"place","placement":{"id":"p","assetId":"v1","target":{"kind":"drawing","contextId":"scene-root","layerUid":"M","extra":0},"offsetFrames":0}}"#,
    ] {
        assert!(matches!(
            parse_edit(bytes.as_bytes()),
            Err(DispatchErrorCode::InvalidRequest)
        ));
    }
}

#[test]
fn common_envelope_size_and_missing_revision_guards_remain_authoritative() {
    for mode in ["oversized", "missing-revision"] {
        let mut o = owner();
        let mut req = request(&o, "bounded", shift(1));
        if mode == "oversized" {
            req["payload"]["padding"] = json!("x".repeat(4096));
        } else {
            req.as_object_mut().unwrap().remove("expectedRevision");
        }
        let before = state(&o);
        assert_eq!(dispatch(&mut o, &req)["error"]["code"], "invalid_request");
        assert_eq!(state(&o), before);
    }
}
