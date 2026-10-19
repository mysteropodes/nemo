//! Real managed project/history/replay/pin controls for the actual Audio producer.
use super::*;
#[path = "object_audio_track_edit_fixture.rs"]
mod fixture;
use fixture::*;
use serde_json::{json, Value};
use std::sync::Arc;

#[test]
fn public_staged_mute_changes_one_real_record_and_retains_catalog_placement_motion_and_old_pin() {
    let mut o = owner("edit-owner");
    let before = saved(&o);
    let old = o.acquire_snapshot(0).unwrap();
    let assets = old.project().unwrap().asset_inventory().unwrap();
    let record = &assets.records()[0];
    let bytes = record.source().encoded_allocation().unwrap();
    let req = request(
        &o,
        "mute",
        json!({"kind":"setMuted","audioId":"track-a","muted":true}),
    );
    let result = dispatch(&mut o, &req);
    assert_eq!(result["ok"], true);
    assert_eq!(
        result["result"],
        json!({"applied":true,"historyEntriesAdded":1})
    );
    let changed = saved(&o);
    let mut expected: Value = serde_json::from_slice(&before).unwrap();
    expected["drawing"]["audioTracks"][0]["muted"] = json!(true);
    assert_eq!(serde_json::from_slice::<Value>(&changed).unwrap(), expected);
    let after = o.acquire_snapshot(1).unwrap();
    let next = after.project().unwrap().asset_inventory().unwrap();
    assert!(Arc::ptr_eq(record, &next.records()[0]));
    assert!(Arc::ptr_eq(
        bytes,
        next.records()[0].source().encoded_allocation().unwrap()
    ));
    assert_eq!(next.placements(), assets.placements());
    assert_eq!(
        o.serialize_project_file(0, old.snapshot_id()).unwrap(),
        before
    );
    o.undo(1).unwrap();
    assert_eq!(saved(&o), before);
    o.redo(2).unwrap();
    assert_eq!(saved(&o), changed);
    assert_eq!(o.history_depths(), (1, 0));
    let reopened = NativeObjectHistory::from_project_file("reopen", &changed).unwrap();
    assert_eq!(saved(&reopened), changed);
    drop((after, o));
    assert_eq!(bytes.as_slice(), WAVE);
}
#[test]
fn rename_fractional_trim_and_explicit_unset_are_persistent_exact_field_edits() {
    let mut o = owner("authored-fields");
    let before: Value = serde_json::from_slice(&saved(&o)).unwrap();
    let mut expected = before.clone();
    for (id, payload) in [
        (
            "rename",
            json!({"kind":"rename","audioId":"track-a","name":""}),
        ),
        (
            "trim",
            json!({"kind":"setTrim","audioId":"track-a","trimStart":0.125,"trimEnd":1.875}),
        ),
        (
            "full-end",
            json!({"kind":"setTrim","audioId":"track-a","trimEnd":null}),
        ),
        (
            "unset-start",
            json!({"kind":"unset","audioId":"track-a","field":"trimStart"}),
        ),
    ] {
        let req = request(&o, id, payload);
        assert_eq!(dispatch(&mut o, &req)["ok"], true);
    }
    expected["drawing"]["audioTracks"][0]["name"] = json!("");
    expected["drawing"]["audioTracks"][0]
        .as_object_mut()
        .unwrap()
        .remove("trimStart");
    let changed = saved(&o);
    assert_eq!(serde_json::from_slice::<Value>(&changed).unwrap(), expected);
    let reopened = NativeObjectHistory::from_project_file("field-reopen", &changed).unwrap();
    assert_eq!(saved(&reopened), changed);
    assert_eq!(o.history_depths(), (4, 0));
}
#[test]
fn default_false_and_equal_field_noops_preserve_bytes_redo_and_retained_success() {
    let mut o = owner("no-op");
    let before = saved(&o);
    let noop = request(
        &o,
        "false-default",
        json!({"kind":"setMuted","audioId":"track-a","muted":false}),
    );
    let result = dispatch(&mut o, &noop);
    assert_eq!(
        result["result"],
        json!({"applied":false,"historyEntriesAdded":0})
    );
    assert_eq!(o.content_revision(), 0);
    assert_eq!(saved(&o), before);
    let rename = request(
        &o,
        "rename",
        json!({"kind":"rename","audioId":"track-a","name":"after"}),
    );
    let renamed = dispatch(&mut o, &rename);
    o.undo(1).unwrap();
    assert_eq!(saved(&o), before);
    assert_eq!(o.history_depths(), (0, 1));
    assert_eq!(dispatch(&mut o, &rename), renamed);
    assert_eq!(saved(&o), before);
    let equal = request(
        &o,
        "same-name",
        json!({"kind":"rename","audioId":"track-a","name":"original"}),
    );
    assert_eq!(
        dispatch(&mut o, &equal)["result"],
        json!({"applied":false,"historyEntriesAdded":0})
    );
    assert_eq!(o.history_depths(), (0, 1));
    assert_eq!(saved(&o), before);
    assert_eq!(dispatch(&mut o, &noop), result);
    assert_eq!(saved(&o), before);
}
#[test]
fn cancel_stale_foreign_target_and_changed_body_never_partially_publish() {
    let mut o = owner("refusal");
    let before = saved(&o);
    for (id, field, value, code) in [
        (
            "cancel",
            "cancelledBeforeDispatch",
            json!(true),
            "cancelled_before_dispatch",
        ),
        ("stale", "expectedRevision", json!(1), "stale_revision"),
        ("document", "documentId", json!("foreign"), "wrong_document"),
        ("instance", "instanceId", json!("foreign"), "wrong_instance"),
    ] {
        let mut req = request(
            &o,
            id,
            json!({"kind":"setMuted","audioId":"track-a","muted":true}),
        );
        req[field] = value;
        let response = dispatch(&mut o, &req);
        assert_eq!(response["error"]["code"], code);
        assert_eq!(dispatch(&mut o, &req), response);
        assert_eq!(saved(&o), before);
        assert_eq!(o.history_depths(), (0, 0));
    }
    let req = request(
        &o,
        "missing",
        json!({"kind":"rename","audioId":"missing","name":"n"}),
    );
    let response = dispatch(&mut o, &req);
    assert_eq!(response["error"]["code"], "not_found");
    let mut changed = req.clone();
    changed["payload"]["audioId"] = json!("track-a");
    assert_eq!(
        dispatch(&mut o, &changed)["error"]["code"],
        "invalid_request"
    );
    assert_eq!(dispatch(&mut o, &req), response);
    assert_eq!(saved(&o), before);
    let fresh = request(
        &o,
        "fresh",
        json!({"kind":"rename","audioId":"track-a","name":"n"}),
    );
    assert_eq!(dispatch(&mut o, &fresh)["ok"], true);
    assert_eq!(o.content_revision(), 1);
}
#[test]
fn sealed_audio_candidates_obey_instance_document_stale_and_drop_cancel_pins() {
    let bytes = br#"{"kind":"setMuted","audioId":"track-a","muted":true}"#;
    for same_instance in [false, true] {
        let first = owner("first");
        let mut other = owner(if same_instance { "first" } else { "other" });
        let candidate = first.prepare_audio_track_edit(bytes).unwrap();
        let before = saved(&other);
        assert_eq!(
            other.commit_audio_track_edit(candidate),
            Err(if same_instance {
                crate::request_receipts::DispatchErrorCode::WrongDocument
            } else {
                crate::request_receipts::DispatchErrorCode::WrongInstance
            })
        );
        assert_eq!(saved(&other), before);
    }
    let mut o = owner("stale-candidate");
    let before = saved(&o);
    drop(o.prepare_audio_track_edit(bytes).unwrap());
    assert_eq!(saved(&o), before);
    let stale = o.prepare_audio_track_edit(bytes).unwrap();
    let first = o.prepare_audio_track_edit(bytes).unwrap();
    assert_eq!(o.commit_audio_track_edit(first), Ok(1));
    let after = saved(&o);
    assert_eq!(
        o.commit_audio_track_edit(stale),
        Err(crate::request_receipts::DispatchErrorCode::StaleRevision)
    );
    assert_eq!(saved(&o), after);
    assert_eq!(o.history_depths(), (1, 0));
}

#[test]
fn equivalent_trim_number_after_undo_preserves_exact_bytes_revision_and_real_redo() {
    let mut o = owner("trim-spelling");
    let before = saved(&o);
    let change = request(
        &o,
        "change-end",
        json!({"kind":"setTrim","audioId":"track-a","trimEnd":1.25}),
    );
    assert_eq!(dispatch(&mut o, &change)["ok"], true);
    o.undo(1).unwrap();
    assert_eq!(saved(&o), before);
    assert_eq!(o.content_revision(), 2);
    assert_eq!(o.history_depths(), (0, 1));
    let equal = request(
        &o,
        "same-start-float",
        json!({"kind":"setTrim","audioId":"track-a","trimStart":0.0}),
    );
    let response = dispatch(&mut o, &equal);
    assert_eq!(
        response["result"],
        json!({"applied":false,"historyEntriesAdded":0})
    );
    assert_eq!(response["contentRevision"], 2);
    assert_eq!(saved(&o), before);
    assert_eq!(o.history_depths(), (0, 1));
    assert_eq!(dispatch(&mut o, &equal), response);
    o.redo(2).unwrap();
    assert_eq!(o.history_depths(), (1, 0));
    assert_eq!(
        serde_json::from_slice::<Value>(&saved(&o)).unwrap()["drawing"]["audioTracks"][0]
            ["trimEnd"],
        1.25
    );
}
#[test]
fn nearby_large_integer_trim_values_are_changed_and_undo_preserves_original_number() {
    let mut o = owner("large-trim");
    let first = request(
        &o,
        "large-a",
        json!({"kind":"setTrim","audioId":"track-a","trimStart":9007199254740993_u64}),
    );
    assert_eq!(dispatch(&mut o, &first)["ok"], true);
    let original = saved(&o);
    for (id, value) in [
        ("large-b", json!(9007199254740992_u64)),
        ("large-float", json!(9007199254740992.0)),
    ] {
        let request = request(
            &o,
            id,
            json!({"kind":"setTrim","audioId":"track-a","trimStart":value}),
        );
        let before_revision = o.content_revision();
        let response = dispatch(&mut o, &request);
        assert_eq!(
            response["result"],
            json!({"applied":true,"historyEntriesAdded":1})
        );
        assert_eq!(o.content_revision(), before_revision + 1);
        assert_eq!(
            serde_json::from_slice::<Value>(&saved(&o)).unwrap()["drawing"]["audioTracks"][0]
                ["trimStart"],
            value
        );
        o.undo(before_revision + 1).unwrap();
        assert_eq!(saved(&o), original);
    }
}
