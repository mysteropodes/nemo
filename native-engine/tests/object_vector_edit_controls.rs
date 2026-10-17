//! L01-F negative and mutation controls: every refusal publishes nothing, a forged or stale candidate cannot
//! cross owners, and hand-mutated plans prove the order helper is what keeps storage admission satisfied.
use super::support::*;
use crate::history::NativeObjectHistory;
use crate::object_codec::{decode_ordered_project, decode_storage_project, ObjectCodecErrorKind};
use crate::request_receipts::DispatchErrorCode;
use serde_json::{json, Value};

fn fresh(version: u32) -> NativeObjectHistory {
    owner_of(&document(version, seeds(), None))
}
fn with(mut value: Value, edit: impl FnOnce(&mut Value)) -> Value {
    edit(&mut value);
    value
}
fn edited(base: &Value, edit: impl FnOnce(&mut Value)) -> String {
    with(base.clone(), edit).to_string()
}
fn untouched(owner: &NativeObjectHistory, bytes_before: &[u8], revision: u64) {
    assert_eq!(bytes(owner), bytes_before);
    assert_eq!(owner.content_revision(), revision);
    assert_eq!(owner.history_depths(), (0, 0));
}

#[test]
fn control_envelope_refusals_publish_nothing() {
    let mut owner = fresh(2);
    let before = bytes(&owner);
    let base = request(&owner, "env", create(record_at("A", "L1", 0, "")));
    let cases = [
        (
            "stale",
            with(base.clone(), |r| r["expectedRevision"] = json!(7)),
            "stale_revision",
        ),
        (
            "inst",
            with(base.clone(), |r| r["instanceId"] = json!("other")),
            "wrong_instance",
        ),
        (
            "doc",
            with(base.clone(), |r| r["documentId"] = json!("other-doc")),
            "wrong_document",
        ),
        (
            "cancel",
            with(base.clone(), |r| r["cancelledBeforeDispatch"] = json!(true)),
            "cancelled_before_dispatch",
        ),
        (
            "nonrev",
            with(base.clone(), |r| {
                r.as_object_mut().unwrap().remove("expectedRevision");
            }),
            "invalid_request",
        ),
        (
            "array",
            with(base.clone(), |r| r["payload"] = json!([1])),
            "invalid_request",
        ),
    ];
    for (id, mut req, code) in cases {
        req["requestId"] = json!(id);
        let response = dispatch(&mut owner, &req);
        assert_eq!(response["ok"], false, "{id}");
        assert_eq!(response["error"]["code"], code, "{id}");
        untouched(&owner, &before, 0);
    }
}

fn prepare_error(owner: &NativeObjectHistory, text: &str) -> DispatchErrorCode {
    owner.prepare_vector_edit_json(text.as_bytes()).unwrap_err()
}

#[test]
fn control_malformed_partial_and_hostile_payloads_are_invalid_request() {
    let owner = fresh(2);
    let before = bytes(&owner);
    let good = create(record_at("A", "L1", 0, ""));
    owner
        .prepare_vector_edit_json(good.to_string().as_bytes())
        .unwrap();
    let text = good.to_string();
    let raw: Vec<(&str, String)> = vec![
        (
            "duplicate command",
            text.replacen("{\"command\"", "{\"command\":\"x\",\"command\"", 1),
        ),
        (
            "duplicate member in record",
            text.replacen(
                "\"family\"",
                "\"family\":\"editable-vector-path\",\"family\"",
                1,
            ),
        ),
        ("truncated", text[..text.len() - 3].to_string()),
        ("positional", "[\"object.vector.create\"]".to_string()),
        (
            "coordinate beyond f64",
            text.replacen("\"x\":10,", "\"x\":1e999,", 1),
        ),
        ("unknown member", edited(&good, |p| p["extra"] = json!(1))),
        (
            "unknown command",
            edited(&good, |p| p["command"] = json!("object.vector.move")),
        ),
        (
            "null command",
            edited(&good, |p| p["command"] = Value::Null),
        ),
        (
            "record missing",
            edited(&good, |p| {
                p.as_object_mut().unwrap().remove("record");
            }),
        ),
        ("record null", edited(&good, |p| p["record"] = Value::Null)),
        (
            "create with target",
            edited(&good, |p| p["stableTarget"] = target("L1", 0, "s-a")),
        ),
        (
            "chosen strokeId",
            edited(&good, |p| p["record"]["target"]["strokeId"] = json!("mine")),
        ),
        (
            "legacy family",
            edited(&good, |p| {
                p["record"]["family"] = json!("closed-solid-fill-cubic-path")
            }),
        ),
        (
            "schema 2",
            edited(&good, |p| p["record"]["schemaVersion"] = json!(2)),
        ),
        (
            "fill absent",
            edited(&good, |p| {
                p["record"].as_object_mut().unwrap().remove("fill");
            }),
        ),
        (
            "vector body absent",
            edited(&good, |p| {
                p["record"].as_object_mut().unwrap().remove("vector");
            }),
        ),
        (
            "flag without stroke",
            edited(&good, |p| {
                p["record"]["vector"]["hasRealStroke"] = json!(false)
            }),
        ),
        (
            "no anchors",
            edited(&good, |p| p["record"]["geometry"]["segments"] = json!([])),
        ),
        (
            "one anchor",
            edited(&good, |p| {
                p["record"]["geometry"]["segments"]
                    .as_array_mut()
                    .unwrap()
                    .truncate(1)
            }),
        ),
        (
            "257 anchors",
            edited(&good, |p| {
                let one = p["record"]["geometry"]["segments"][0].clone();
                p["record"]["geometry"]["segments"] = Value::Array(vec![one; 257]);
            }),
        ),
        (
            "negative width",
            edited(&good, |p| p["record"]["vector"]["strokeWidth"] = json!(-1)),
        ),
        (
            "opacity above one",
            edited(&good, |p| p["record"]["vector"]["opacity"] = json!(1.5)),
        ),
        (
            "fill channel above one",
            edited(&good, |p| {
                p["record"]["fill"] = json!({"kind":"solid","r":2,"g":0,"b":0,"a":1})
            }),
        ),
        (
            "oversized",
            format!(
                "{{\"command\":\"object.vector.create\",\"pad\":\"{}\"}}",
                "x".repeat(65_536)
            ),
        ),
    ];
    for (what, text) in raw {
        assert_eq!(
            prepare_error(&owner, &text),
            DispatchErrorCode::InvalidRequest,
            "{what}"
        );
    }
    let del = delete(target("L1", 0, "s-a"));
    owner
        .prepare_vector_edit_json(del.to_string().as_bytes())
        .unwrap();
    for (what, text) in [
        (
            "delete with record",
            edited(&del, |p| p["record"] = record_at("A", "L1", 0, "")),
        ),
        (
            "delete without target",
            "{\"command\":\"object.vector.delete\"}".to_string(),
        ),
        (
            "replace without record",
            "{\"command\":\"object.vector.replace\"}".to_string(),
        ),
    ] {
        assert_eq!(
            prepare_error(&owner, &text),
            DispatchErrorCode::InvalidRequest,
            "{what}"
        );
    }
    untouched(&owner, &before, 0);
}

#[test]
fn control_forged_prepared_values_cannot_cross_owners() {
    let source = fresh(2);
    let candidate = source
        .prepare_vector_edit_json(create(record_at("B", "L1", 0, "")).to_string().as_bytes())
        .unwrap();
    // Same instance name, fresh document incarnation.
    let mut twin = fresh(2);
    let twin_bytes = bytes(&twin);
    assert_eq!(
        twin.commit_vector_edit(candidate.clone()).unwrap_err(),
        DispatchErrorCode::WrongDocument
    );
    untouched(&twin, &twin_bytes, 0);
    let mut foreign = NativeObjectHistory::new(
        "foreign",
        decode_storage_project(document(2, seeds(), None).to_string().as_bytes()).unwrap(),
    )
    .unwrap();
    let foreign_bytes = bytes(&foreign);
    assert_eq!(
        foreign.commit_vector_edit(candidate).unwrap_err(),
        DispatchErrorCode::WrongInstance
    );
    untouched(&foreign, &foreign_bytes, 0);
}

#[test]
fn control_commit_after_an_intervening_edit_is_stale() {
    let mut owner = fresh(2);
    let candidate = owner
        .prepare_vector_edit_json(create(record_at("B", "L1", 0, "")).to_string().as_bytes())
        .unwrap();
    send(
        &mut owner,
        "between",
        replace(record_at("D2", "L1", 0, "s-a")),
    );
    let between = bytes(&owner);
    assert_eq!(
        owner.commit_vector_edit(candidate).unwrap_err(),
        DispatchErrorCode::StaleRevision
    );
    assert_eq!((bytes(&owner), owner.content_revision()), (between, 1));
    assert_eq!(owner.history_depths(), (1, 0));
}

#[test]
fn control_two_creates_racing_on_one_origin_publish_once() {
    let mut owner = fresh(2);
    let a = owner
        .prepare_vector_edit_json(create(record_at("A", "L1", 0, "")).to_string().as_bytes())
        .unwrap();
    let b = owner
        .prepare_vector_edit_json(create(record_at("D", "L1", 0, "")).to_string().as_bytes())
        .unwrap();
    // Both were sealed to revision 0 and picked the same ID: exactly why the second must be refused.
    assert_eq!(a.result().stroke_id, "native-stroke:1:0");
    assert_eq!(a.result().stroke_id, b.result().stroke_id);
    assert_eq!(owner.commit_vector_edit(a).unwrap(), 1);
    let once = bytes(&owner);
    assert_eq!(
        owner.commit_vector_edit(b).unwrap_err(),
        DispatchErrorCode::StaleRevision
    );
    assert_eq!(bytes(&owner), once);
    let after = state(&owner);
    assert_eq!(objects(&after).len(), 4);
    assert_eq!(
        after["objects"][3]["geometry"]["segments"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(owner.history_depths(), (1, 0));
}

// The ordered decoder reports the order admission error itself; decode_storage_project would report the
// legacy v1 error for a v2 document.
fn decode_kind(doc: &Value) -> Result<(), ObjectCodecErrorKind> {
    decode_ordered_project(doc.to_string().as_bytes())
        .map(|_| ())
        .map_err(|e| e.kind())
}

#[test]
fn control_a_plan_without_the_helper_edits_is_refused_by_storage_admission() {
    let mut owner = fresh(2);
    send(&mut owner, "c1", create(record_at("D", "L1", 0, "")));
    send(&mut owner, "c2", create(record_at("E", "L1", 0, "")));
    let made = state(&owner);
    assert_eq!(decode_kind(&made), Ok(()));
    // Mutation 1: the created record's element is missing (insert_element not applied).
    let mut missing = made.clone();
    missing["compositionOrder"]["contexts"][0]["scopes"][0]["elements"]
        .as_array_mut()
        .unwrap()
        .pop();
    assert_eq!(decode_kind(&missing), Err(ObjectCodecErrorKind::Invalid));
    // Mutation 2: a deleted record keeps its element (remove_element not applied).
    send(
        &mut owner,
        "d1",
        delete(target("L1", 0, "native-stroke:1:0")),
    );
    let removed = state(&owner);
    assert_eq!(decode_kind(&removed), Ok(()));
    let mut stale = removed.clone();
    stale["compositionOrder"]["contexts"][0]["scopes"][0]["elements"]
        .as_array_mut()
        .unwrap()
        .push(
            json!({"layerUid":"L1","strokeId":"native-stroke:1:0","naturalPosition":2,"order":{}}),
        );
    assert_eq!(decode_kind(&stale), Err(ObjectCodecErrorKind::Invalid));
    // Mutation 3: positions are not closed up after the removal (gap at 0).
    let mut gapped = removed.clone();
    for e in gapped["compositionOrder"]["contexts"][0]["scopes"][0]["elements"]
        .as_array_mut()
        .unwrap()
    {
        if e["layerUid"] == "L1" {
            let p = e["naturalPosition"].as_u64().unwrap();
            e["naturalPosition"] = json!(p + 1);
        }
    }
    assert_eq!(decode_kind(&gapped), Err(ObjectCodecErrorKind::Invalid));
    // The helper's real output after the removal: L1 holds s-a (0), s-c is on frame 5, native-stroke:2:0 moved to 1.
    let positions: Vec<(String, u64)> = removed["compositionOrder"]["contexts"][0]["scopes"][0]
        ["elements"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["layerUid"] == "L1")
        .map(|e| {
            (
                e["strokeId"].as_str().unwrap().to_string(),
                e["naturalPosition"].as_u64().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        positions,
        [("s-a".to_string(), 0), ("native-stroke:2:0".to_string(), 1)]
    );
}

#[test]
fn control_reference_vector_targets_are_unavailable_in_both_storage_versions() {
    for version in [1, 2] {
        let mut records = seeds();
        records[0]["target"]["frameScope"]["kind"] = json!("reference");
        let source = document(version, records, None);
        let mut owner = owner_of(&source); // Admission proves Reference is stored, not malformed.
        assert_eq!(state(&owner), source);
        let owed = seed_redo(&mut owner);
        let mut incoming = record_at("D2", "L1", 0, "s-a");
        incoming["target"]["frameScope"]["kind"] = json!("reference");
        let reference = incoming["target"].clone();
        let mut new = incoming.clone();
        new["target"]["strokeId"] = json!("");
        for (id, payload) in [
            ("reference-create", create(new)),
            ("reference-replace", replace(incoming)),
            ("reference-delete", delete(reference)),
        ] {
            assert_unavailable(&mut owner, id, payload);
        }
        let revision = owner.content_revision();
        assert_eq!(owner.redo(revision), Ok(revision + 1));
        assert_eq!(bytes(&owner), owed);
    }
}

#[test]
fn control_vector_replace_and_delete_cannot_convert_or_remove_a_legacy_family() {
    for version in [1, 2] {
        let mut records = seeds();
        records[0] = record_at("C", "L1", 0, "s-a"); // Closed and filled: valid legacy geometry.
        records[0]["family"] = json!("closed-solid-fill-cubic-path");
        records[0].as_object_mut().unwrap().remove("vector");
        let source = document(version, records, None);
        let mut owner = owner_of(&source);
        assert_eq!(state(&owner), source);
        let owed = seed_redo(&mut owner);
        for (id, payload) in [
            ("legacy-replace", replace(record_at("D2", "L1", 0, "s-a"))),
            ("legacy-delete", delete(target("L1", 0, "s-a"))),
        ] {
            assert_unavailable(&mut owner, id, payload);
        }
        let revision = owner.content_revision();
        assert_eq!(owner.redo(revision), Ok(revision + 1));
        assert_eq!(bytes(&owner), owed);
    }
}

#[test]
fn control_oversized_imported_id_cannot_publish_then_fail_response_admission() {
    for version in [1, 2] {
        let stroke_id = "stored-".to_owned() + &"x".repeat(5_000);
        let mut records = seeds();
        records[0]["target"]["strokeId"] = json!(stroke_id);
        let source = document(version, records, None);
        let mut owner = owner_of(&source); // The ID remains valid stored data, never shortened.
        assert_eq!(state(&owner), source);
        let owed = seed_redo(&mut owner);
        for (id, payload) in [
            (
                "long-replace",
                replace(record_at("D2", "L1", 0, &stroke_id)),
            ),
            ("long-delete", delete(target("L1", 0, &stroke_id))),
            ("long-noop", replace(record_at("A", "L1", 0, &stroke_id))),
        ] {
            let req = request(&owner, id, payload.clone());
            assert!(req.to_string().len() < 65_536);
            assert!(json!({"strokeId":stroke_id}).to_string().len() > 4_096);
            assert_dispatch_invalid_request(&mut owner, id, payload);
        }
        let revision = owner.content_revision();
        assert_eq!(owner.redo(revision), Ok(revision + 1));
        assert_eq!(bytes(&owner), owed);
    }
}
